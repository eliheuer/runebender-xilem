// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An edit session: one glyph, its selection, viewport, and undo stack.
//!
//! The session works on a `norad::Glyph` directly so every operation in
//! `runebender_core::glyph_ops` and `point_ops` applies without conversion.
//! The editor island owns the session; the app receives copies of the glyph.

use std::collections::{HashMap, HashSet};

use masonry::kurbo::{self as kurbo, BezPath, Point, Rect, Shape};
use runebender_core::editing::edit_types::EditType;
use runebender_core::editing::undo::UndoState;
use runebender_core::editing::viewport::ViewPort;
use runebender_core::glyph_ops::{self, GlyphSnapshot, PointId};
use runebender_core::glyph_paths;
use runebender_core::point_ops;

/// Boolean operation kinds, mapped to `linesweeper::BinaryOp` internally.
#[derive(Clone, Copy)]
pub enum BoolOp {
    Union,
    Subtract,
    Intersect,
    Exclude,
}

#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub upm: f64,
    pub ascender: f64,
    pub descender: f64,
    pub x_height: f64,
    pub cap_height: f64,
}

impl Metrics {
    pub fn of(font: &norad::Font) -> Self {
        let info = &font.font_info;
        let upm = info.units_per_em.map(|u| u.as_f64()).unwrap_or(1000.0);
        Self {
            upm,
            ascender: info.ascender.unwrap_or(upm * 0.8),
            descender: info.descender.unwrap_or(-upm * 0.2),
            x_height: info.x_height.unwrap_or(upm * 0.5),
            cap_height: info.cap_height.unwrap_or(upm * 0.7),
        }
    }
}

/// One point, as the editor sees it.
#[derive(Clone, Copy, Debug)]
pub struct PointView {
    pub id: PointId,
    pub point: Point,
    pub on_curve: bool,
    pub smooth: bool,
    pub start: bool,
}

#[derive(Clone)]
pub struct Session {
    pub glyph_name: String,
    pub glyph: norad::Glyph,
    /// Components, resolved against the font at session creation.
    pub components: BezPath,
    /// Contours the components resolve to, precomputed for decompose.
    component_contours: Vec<norad::Contour>,
    pub metrics: Metrics,
    pub selection: HashSet<PointId>,
    pub viewport: ViewPort,
    pub fitted: bool,
    undo: UndoState<GlyphSnapshot>,
    drag_originals: HashMap<PointId, (f64, f64)>,
    in_drag: bool,
    /// The contour the pen is currently extending, if any.
    pub active_contour: Option<usize>,
    /// In-progress pen points (on- and off-curve), materialized into
    /// `active_contour` on each change.
    pen: Vec<PenPt>,
    /// The currently selected anchor, if any.
    pub selected_anchor: Option<usize>,
}

/// One point in the pen's in-progress buffer.
#[derive(Clone, Copy)]
struct PenPt {
    point: Point,
    off: bool,
    smooth: bool,
}

impl Session {
    pub fn new(font: &norad::Font, name: &str) -> Option<Self> {
        let glyph = font.get_glyph(name)?.clone();
        let components = glyph_paths::components_to_bezpath(&glyph, font);
        let component_contours = resolve_components(font, &glyph);
        Some(Self {
            glyph_name: name.to_string(),
            glyph,
            components,
            component_contours,
            metrics: Metrics::of(font),
            selection: HashSet::new(),
            viewport: ViewPort::new(),
            fitted: false,
            undo: UndoState::new(),
            drag_originals: HashMap::new(),
            in_drag: false,
            active_contour: None,
            pen: Vec::new(),
            selected_anchor: None,
        })
    }

    pub fn advance(&self) -> f64 {
        self.glyph.width
    }

    pub fn set_unicode(&mut self, u: &str) -> bool {
        self.record(EditType::Normal);
        glyph_ops::set_glyph_unicode(&mut self.glyph, u)
    }

    /// Shift all points and anchors horizontally (left-sidebearing drag).
    pub fn shift_glyph(&mut self, dx: f64) {
        self.record(EditType::Drag);
        for contour in &mut self.glyph.contours {
            for p in &mut contour.points {
                p.x += dx;
            }
        }
        for a in &mut self.glyph.anchors {
            a.x += dx;
        }
        self.glyph.width = (self.glyph.width + dx).max(0.0);
    }

    pub fn set_advance(&mut self, w: f64) {
        self.record(EditType::Normal);
        self.glyph.width = w.max(0.0);
    }

    pub fn outline_arc(&self) -> std::sync::Arc<BezPath> {
        std::sync::Arc::new(self.outline())
    }

    pub fn components_arc(&self) -> std::sync::Arc<BezPath> {
        std::sync::Arc::new(self.components.clone())
    }

    pub fn outline(&self) -> BezPath {
        glyph_paths::contours_to_bezpath(&self.glyph)
    }

    pub fn ink(&self) -> Option<Rect> {
        let mut path = self.outline();
        path.extend(self.components.iter());
        if path.elements().is_empty() {
            None
        } else {
            Some(path.bounding_box())
        }
    }

    pub fn points(&self) -> Vec<PointView> {
        let mut out = Vec::new();
        for (ci, contour) in self.glyph.contours.iter().enumerate() {
            for (pi, p) in contour.points.iter().enumerate() {
                let on_curve = !matches!(p.typ, norad::PointType::OffCurve);
                out.push(PointView {
                    id: (ci, pi),
                    point: Point::new(p.x, p.y),
                    on_curve,
                    smooth: on_curve && p.smooth,
                    start: pi == 0,
                });
            }
        }
        out
    }

    pub fn point_count(&self) -> usize {
        self.glyph.contours.iter().map(|c| c.points.len()).sum()
    }

    pub fn can_undo(&self) -> bool {
        self.undo.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.undo.can_redo()
    }

    // ---- edits ----

    /// Record the state before an edit.
    pub fn record(&mut self, edit: EditType) {
        let snapshot = glyph_ops::snapshot(&self.glyph);
        match edit {
            EditType::Drag => {
                if !self.in_drag {
                    self.undo.add_undo_group(snapshot);
                    self.in_drag = true;
                }
            }
            EditType::DragUp => self.in_drag = false,
            _ => self.undo.add_undo_group(snapshot),
        }
    }

    pub fn undo(&mut self) -> bool {
        let current = glyph_ops::snapshot(&self.glyph);
        match self.undo.undo(current) {
            Some(prev) => {
                glyph_ops::restore(&mut self.glyph, prev);
                self.prune_selection();
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        let current = glyph_ops::snapshot(&self.glyph);
        match self.undo.redo(current) {
            Some(next) => {
                glyph_ops::restore(&mut self.glyph, next);
                self.prune_selection();
                true
            }
            None => false,
        }
    }

    fn prune_selection(&mut self) {
        let glyph = &self.glyph;
        self.selection.retain(|(c, p)| {
            glyph
                .contours
                .get(*c)
                .is_some_and(|contour| *p < contour.points.len())
        });
    }

    pub fn begin_point_drag(&mut self) {
        self.record(EditType::Drag);
        self.drag_originals = self
            .points()
            .into_iter()
            .filter(|p| self.selection.contains(&p.id))
            .map(|p| (p.id, (p.point.x, p.point.y)))
            .collect();
    }

    /// Move the selection to `total` design units from where the drag began.
    pub fn drag_points_to(&mut self, total: (f64, f64)) -> bool {
        point_ops::translate_points(
            &mut self.glyph,
            &self.selection,
            &self.drag_originals,
            total,
            false,
        )
    }

    pub fn end_point_drag(&mut self) {
        self.record(EditType::DragUp);
        self.drag_originals.clear();
    }

    pub fn nudge(&mut self, dx: f64, dy: f64) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        self.record(EditType::Normal);
        let originals: HashMap<PointId, (f64, f64)> = self
            .points()
            .into_iter()
            .filter(|p| self.selection.contains(&p.id))
            .map(|p| (p.id, (p.point.x, p.point.y)))
            .collect();
        point_ops::translate_points(&mut self.glyph, &self.selection, &originals, (dx, dy), false)
    }

    pub fn delete_selected(&mut self) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        self.record(EditType::Normal);
        let changed = glyph_ops::delete_points(&mut self.glyph, &self.selection);
        self.selection.clear();
        changed
    }

    /// The last committed on-curve point in the pen buffer, in design space.
    pub fn pen_last_point(&self) -> Option<Point> {
        self.pen.iter().rev().find(|p| !p.off).map(|p| p.point)
    }

    /// The first point of the pen buffer, in design space.
    pub fn pen_first_point(&self) -> Option<Point> {
        self.pen.first().map(|p| p.point)
    }

    pub fn pen_is_active(&self) -> bool {
        !self.pen.is_empty()
    }

    /// Write the pen buffer into `active_contour`, creating it if needed.
    fn pen_sync(&mut self) {
        let ci = match self.active_contour {
            Some(c) if c < self.glyph.contours.len() => c,
            _ => {
                self.glyph.contours.push(norad::Contour::new(Vec::new(), None));
                let c = self.glyph.contours.len() - 1;
                self.active_contour = Some(c);
                c
            }
        };
        let mut points = Vec::with_capacity(self.pen.len());
        let mut prev_off = false;
        for (i, pt) in self.pen.iter().enumerate() {
            let typ = if i == 0 {
                norad::PointType::Move
            } else if pt.off {
                norad::PointType::OffCurve
            } else if prev_off {
                norad::PointType::Curve
            } else {
                norad::PointType::Line
            };
            points.push(norad::ContourPoint::new(pt.point.x, pt.point.y, typ, pt.smooth, None, None));
            prev_off = pt.off;
        }
        self.glyph.contours[ci].points = points;
    }

    /// Place a corner on-curve point (a plain click).
    pub fn pen_corner(&mut self, x: f64, y: f64) {
        if self.pen.is_empty() {
            self.record(EditType::Normal);
        }
        self.pen.push(PenPt { point: Point::new(x, y), off: false, smooth: false });
        self.pen_sync();
    }

    /// Begin a smooth point with symmetric handles at `origin`; the outgoing
    /// handle starts at `to`.
    pub fn pen_smooth_begin(&mut self, origin: Point, to: Point) {
        if self.pen.is_empty() {
            self.record(EditType::Normal);
        }
        self.pen.push(PenPt { point: origin, off: true, smooth: false });
        self.pen.push(PenPt { point: origin, off: false, smooth: true });
        self.pen.push(PenPt { point: to, off: true, smooth: false });
        self.pen_sync();
    }

    /// Update the handles of the smooth point currently being dragged.
    pub fn pen_smooth_drag(&mut self, origin: Point, to: Point) {
        let n = self.pen.len();
        if n < 3 {
            return;
        }
        self.pen[n - 1].point = to;
        self.pen[n - 3].point = Point::new(2.0 * origin.x - to.x, 2.0 * origin.y - to.y);
        self.pen_sync();
    }

    /// Close the active contour.
    pub fn pen_close(&mut self) {
        if let Some(c) = self.active_contour.take() {
            if let Some(contour) = self.glyph.contours.get_mut(c) {
                if contour.points.first().map(|p| p.typ) == Some(norad::PointType::Move)
                    && contour.points.len() > 1
                {
                    let first = contour.points.remove(0);
                    let typ = if contour
                        .points
                        .last()
                        .map(|p| p.typ == norad::PointType::OffCurve)
                        .unwrap_or(false)
                    {
                        norad::PointType::Curve
                    } else {
                        norad::PointType::Line
                    };
                    contour.points.push(norad::ContourPoint::new(
                        first.x, first.y, typ, first.smooth, None, None,
                    ));
                }
            }
        }
        self.pen.clear();
    }

    /// End the current pen path without closing (Escape / tool switch).
    pub fn pen_cancel(&mut self) {
        self.active_contour = None;
        self.pen.clear();
    }

    // ---- hyperbezier pen: on-curve points only, curve solved by the spline ----

    /// Add a hyperbezier on-curve point (smooth), starting a contour if idle.
    pub fn hyper_add(&mut self, x: f64, y: f64, corner: bool) {
        if self.active_contour.is_none() {
            self.record(EditType::Normal);
            let c = glyph_ops::start_hyper_contour(&mut self.glyph, x, y);
            self.active_contour = Some(c);
            if corner {
                // First point corner-ness is applied on the Move via append below.
            }
        } else if let Some(c) = self.active_contour {
            glyph_ops::append_hyper_point(&mut self.glyph, c, x, y, corner);
        }
    }

    pub fn hyper_close(&mut self) {
        if let Some(c) = self.active_contour.take() {
            self.record(EditType::Normal);
            glyph_ops::close_hyper_contour(&mut self.glyph, c);
        }
    }

    pub fn first_contour_point(&self) -> Option<Point> {
        let c = self.active_contour?;
        let p = self.glyph.contours.get(c)?.points.first()?;
        Some(Point::new(p.x, p.y))
    }

    pub fn last_contour_point(&self) -> Option<Point> {
        let c = self.active_contour?;
        let p = self.glyph.contours.get(c)?.points.last()?;
        Some(Point::new(p.x, p.y))
    }

    pub fn hyper_is_active(&self) -> bool {
        self.active_contour.is_some() && self.pen.is_empty()
    }

    /// Add a closed rectangle contour.
    pub fn add_rect(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        let (lx, rx) = (x0.min(x1), x0.max(x1));
        let (by, ty) = (y0.min(y1), y0.max(y1));
        if (rx - lx).abs() < 1.0 || (ty - by).abs() < 1.0 {
            return;
        }
        self.record(EditType::Normal);
        let corner = |x, y| norad::ContourPoint::new(x, y, norad::PointType::Line, false, None, None);
        let points = vec![corner(lx, by), corner(rx, by), corner(rx, ty), corner(lx, ty)];
        self.glyph.contours.push(norad::Contour::new(points, None));
    }

    /// Add a closed ellipse contour (four cubic segments).
    pub fn add_ellipse(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (rx, ry) = ((x1 - x0).abs() / 2.0, (y1 - y0).abs() / 2.0);
        if rx < 1.0 || ry < 1.0 {
            return;
        }
        self.record(EditType::Normal);
        const K: f64 = 0.552_284_749_831;
        let on = |x, y| norad::ContourPoint::new(x, y, norad::PointType::Curve, true, None, None);
        let off = |x, y| norad::ContourPoint::new(x, y, norad::PointType::OffCurve, false, None, None);
        // Start at East, go counter-clockwise through N, W, S.
        let points = vec![
            on(cx + rx, cy),
            off(cx + rx, cy + ry * K),
            off(cx + rx * K, cy + ry),
            on(cx, cy + ry),
            off(cx - rx * K, cy + ry),
            off(cx - rx, cy + ry * K),
            on(cx - rx, cy),
            off(cx - rx, cy - ry * K),
            off(cx - rx * K, cy - ry),
            on(cx, cy - ry),
            off(cx + rx * K, cy - ry),
            off(cx + rx, cy - ry * K),
        ];
        self.glyph.contours.push(norad::Contour::new(points, None));
    }

    /// Apply an affine to the selection (or the whole glyph if none),
    /// centered on the target bounding box.
    pub fn transform(&mut self, affine: kurbo::Affine) -> bool {
        self.record(EditType::Normal);
        glyph_ops::transform_selection(&mut self.glyph, &self.selection, affine)
    }

    pub fn flip_horizontal(&mut self) -> bool {
        self.transform(kurbo::Affine::new([-1.0, 0.0, 0.0, 1.0, 0.0, 0.0]))
    }

    pub fn flip_vertical(&mut self) -> bool {
        self.transform(kurbo::Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, 0.0]))
    }

    pub fn rotate_90(&mut self) -> bool {
        self.transform(kurbo::Affine::new([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]))
    }

    pub fn reverse(&mut self) -> bool {
        self.record(EditType::Normal);
        glyph_ops::reverse_contours(&mut self.glyph, &self.selection)
    }

    pub fn decompose(&mut self) -> bool {
        if self.glyph.components.is_empty() || self.component_contours.is_empty() {
            return false;
        }
        self.record(EditType::Normal);
        self.glyph.contours.extend(self.component_contours.drain(..));
        self.glyph.components.clear();
        self.components = kurbo::BezPath::new();
        true
    }

    pub fn boolean(&mut self, op: BoolOp) -> bool {
        let op = match op {
            BoolOp::Union => linesweeper::BinaryOp::Union,
            BoolOp::Subtract => linesweeper::BinaryOp::Difference,
            BoolOp::Intersect => linesweeper::BinaryOp::Intersection,
            BoolOp::Exclude => linesweeper::BinaryOp::Xor,
        };
        if let Some(contours) = glyph_ops::boolean_contours(&self.glyph, op) {
            self.record(EditType::Normal);
            self.glyph.contours = contours;
            self.selection.clear();
            true
        } else {
            false
        }
    }

    pub fn remove_overlap(&mut self) -> bool {
        if let Some(contours) = glyph_ops::remove_overlap(&self.glyph) {
            self.record(EditType::Normal);
            self.glyph.contours = contours;
            self.selection.clear();
            true
        } else {
            false
        }
    }

    /// Points where a knife line from p0 to p1 crosses the outline.
    pub fn knife_hits(&self, p0: Point, p1: Point) -> Vec<Point> {
        runebender_core::knife::knife_hit_points(&self.glyph, p0, p1)
    }

    /// Cut the outline along the line p0..p1.
    pub fn knife_cut(&mut self, p0: Point, p1: Point) -> bool {
        self.record(EditType::Normal);
        let changed = runebender_core::knife::knife_cut_glyph(&mut self.glyph, p0, p1);
        if !changed {
            // Nothing cut; drop the empty undo group we just pushed.
            let _ = self.undo.undo(glyph_ops::snapshot(&self.glyph));
        }
        changed
    }

    /// The glyph's contours as core `Path`s (for measurement/analysis).
    pub fn paths(&self) -> Vec<runebender_core::path::Path> {
        self.glyph
            .contours
            .iter()
            .map(|c| {
                runebender_core::path::Path::from_contour(
                    &runebender_core::model::workspace::Contour::from_norad(c),
                )
            })
            .collect()
    }

    pub fn measurements(&self) -> Vec<runebender_core::measure::Measurement> {
        runebender_core::measure::glyph_measurements(&self.paths())
    }

    pub fn side_bearings(&self) -> Option<runebender_core::measure::SideBearings> {
        runebender_core::measure::side_bearings(&self.paths(), self.advance())
    }

    /// Bounding box of the selected points in design space, if any.
    pub fn selection_bounds(&self) -> Option<Rect> {
        let mut min = (f64::INFINITY, f64::INFINITY);
        let mut max = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (ci, contour) in self.glyph.contours.iter().enumerate() {
            for (pi, p) in contour.points.iter().enumerate() {
                if self.selection.contains(&(ci, pi)) {
                    min = (min.0.min(p.x), min.1.min(p.y));
                    max = (max.0.max(p.x), max.1.max(p.y));
                }
            }
        }
        if min.0.is_finite() {
            Some(Rect::new(min.0, min.1, max.0, max.1))
        } else {
            None
        }
    }

    /// Make the first selected on-curve point the start of its contour.
    pub fn set_start(&mut self) -> bool {
        let Some(&(ci, pi)) = self.selection.iter().min() else {
            return false;
        };
        self.record(EditType::Normal);
        let ok = glyph_ops::set_contour_start(&mut self.glyph, ci, pi);
        if ok {
            self.selection.clear();
        }
        ok
    }

    /// Round the selected corner points (fillet).
    pub fn round_corners(&mut self) -> bool {
        self.record(EditType::Normal);
        match glyph_ops::round_selected_corners(&mut self.glyph, &self.selection) {
            Some(next) => {
                self.selection = next;
                true
            }
            None => false,
        }
    }

    pub fn harmonize(&mut self) -> bool {
        self.record(EditType::Normal);
        glyph_ops::curve_op(&mut self.glyph, &self.selection, glyph_ops::CurveOp::Harmonize)
    }

    pub fn balance(&mut self) -> bool {
        self.record(EditType::Normal);
        glyph_ops::curve_op(&mut self.glyph, &self.selection, glyph_ops::CurveOp::Balance)
    }

    pub fn optimize(&mut self) -> bool {
        self.record(EditType::Normal);
        glyph_ops::curve_op(&mut self.glyph, &self.selection, glyph_ops::CurveOp::Optimize(0.12))
    }

    pub fn duplicate(&mut self) -> bool {
        self.record(EditType::Normal);
        match glyph_ops::duplicate_selection(&mut self.glyph, &self.selection) {
            Some(next) => {
                self.selection = next;
                true
            }
            None => false,
        }
    }

    /// Index of the anchor near `p` (design space), if within `tol`.
    pub fn anchor_at(&self, p: Point, tol: f64) -> Option<usize> {
        self.glyph
            .anchors
            .iter()
            .enumerate()
            .filter(|(_, a)| Point::new(a.x, a.y).distance(p) <= tol)
            .min_by(|a, b| {
                Point::new(a.1.x, a.1.y)
                    .distance(p)
                    .total_cmp(&Point::new(b.1.x, b.1.y).distance(p))
            })
            .map(|(i, _)| i)
    }

    pub fn add_anchor(&mut self, x: f64, y: f64) {
        self.record(EditType::Normal);
        let n = self.glyph.anchors.len();
        let name = norad::Name::new(&format!("anchor.{n}")).ok();
        self.glyph.anchors.push(norad::Anchor::new(x, y, name, None, None));
        self.selected_anchor = Some(n);
    }

    pub fn move_anchor(&mut self, idx: usize, x: f64, y: f64) {
        if let Some(a) = self.glyph.anchors.get_mut(idx) {
            a.x = x;
            a.y = y;
        }
    }

    pub fn delete_selected_anchor(&mut self) -> bool {
        let Some(idx) = self.selected_anchor.take() else {
            return false;
        };
        if idx < self.glyph.anchors.len() {
            self.record(EditType::Normal);
            self.glyph.anchors.remove(idx);
            true
        } else {
            false
        }
    }

    pub fn anchor_point(&self, idx: usize) -> Option<Point> {
        self.glyph.anchors.get(idx).map(|a| Point::new(a.x, a.y))
    }

    pub fn curvature_comb(&self) -> Vec<Vec<(Point, Point)>> {
        let cubics = runebender_core::curve::cubics_from_norad(&self.glyph);
        runebender_core::curve::curvature_comb(&cubics, 1.0, 4000.0, false, 12)
            .into_iter()
            .map(|strip| strip.into_iter().map(|s| (s.on, s.outer)).collect())
            .collect()
    }

    pub fn set_mark(&mut self, label: Option<&str>) {
        self.record(EditType::Normal);
        runebender_core::theme_oklch::set_glyph_mark(&mut self.glyph, label);
    }

    /// The contours to copy: the ones holding a selected point, or every
    /// contour when nothing is selected. This is the web editor's rule,
    /// and the GPUI build's.
    pub fn contours_for_copy(&self) -> Vec<norad::Contour> {
        if self.selection.is_empty() {
            return self.glyph.contours.clone();
        }
        self.glyph
            .contours
            .iter()
            .enumerate()
            .filter(|(index, _)| self.selection.iter().any(|(c, _)| c == index))
            .map(|(_, contour)| contour.clone())
            .collect()
    }

    /// Append contours to the glyph, and select the points they brought.
    pub fn paste_contours(&mut self, contours: &[norad::Contour]) -> bool {
        if contours.is_empty() {
            return false;
        }
        self.record(EditType::Normal);
        let first_new = self.glyph.contours.len();
        self.glyph.contours.extend(contours.iter().cloned());
        self.selection.clear();
        for (offset, contour) in contours.iter().enumerate() {
            for point in 0..contour.points.len() {
                self.selection.insert((first_new + offset, point));
            }
        }
        true
    }

    pub fn select_all(&mut self) {
        self.selection = self.points().into_iter().map(|p| p.id).collect();
    }
}

/// Resolve a glyph's components into concrete contours (for decompose),
/// computed once at session creation while the font is available.
fn resolve_components(font: &norad::Font, glyph: &norad::Glyph) -> Vec<norad::Contour> {
    let mut work = glyph.clone();
    let before = work.contours.len();
    while !work.components.is_empty() {
        if !glyph_ops::decompose_single_component(font, &mut work, 0) {
            break;
        }
    }
    work.contours.split_off(before)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A glyph with two square contours, so copy has something to choose
    /// between.
    fn two_squares() -> Session {
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("test");
        for offset in [0.0, 200.0] {
            let mut contour = norad::Contour::default();
            for (x, y) in [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)] {
                contour.points.push(norad::ContourPoint::new(
                    x + offset,
                    y,
                    norad::PointType::Line,
                    false,
                    None,
                    None,
                ));
            }
            glyph.contours.push(contour);
        }
        font.default_layer_mut().insert_glyph(glyph);
        Session::new(&font, "test").expect("glyph is there")
    }

    #[test]
    fn copy_with_no_selection_takes_every_contour() {
        let session = two_squares();
        assert_eq!(session.contours_for_copy().len(), 2);
    }

    #[test]
    fn copy_takes_the_contours_holding_a_selected_point() {
        let mut session = two_squares();
        session.selection.insert((1, 0));
        let copied = session.contours_for_copy();
        assert_eq!(copied.len(), 1);
        // The second square starts at x = 200.
        assert_eq!(copied[0].points[0].x, 200.0);
    }

    #[test]
    fn paste_appends_and_selects_what_it_pasted() {
        let mut session = two_squares();
        let copied = session.contours_for_copy();
        assert!(session.paste_contours(&copied));
        assert_eq!(session.glyph.contours.len(), 4);
        // Every point of the two new contours, and nothing else.
        assert_eq!(session.selection.len(), 8);
        assert!(session.selection.iter().all(|(c, _)| *c >= 2));
    }

    #[test]
    fn pasting_nothing_changes_nothing() {
        let mut session = two_squares();
        assert!(!session.paste_contours(&[]));
        assert_eq!(session.glyph.contours.len(), 2);
    }
}
