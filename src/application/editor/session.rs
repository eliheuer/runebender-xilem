// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Edit sessions: one glyph, its selection, viewport, and undo stack,
//! and the tabs that hold them: opening a glyph, parking and resuming,
//! switching masters, and the axis location.
//!
//! The session works on a `norad::Glyph` directly so every operation in
//! `runebender::outline::glyph_ops` and `point_ops` applies without conversion.
//! The editor island owns the session; the app receives copies of the glyph.

use crate::application::editor::tools::metaballs;
use crate::application::font_model::FontModel;
use crate::application::platform::host;
use crate::application::view::canvas::grid::cells_of;
use crate::application::view::panels::sections::metric_bufs;
use crate::application::workspace::{Mode, Tab, TextContext, Tool, Workspace};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use masonry::kurbo::{self as kurbo, BezPath, Point, Rect};
use runebender::outline::glyph_ops::{self, PointId};
use runebender::outline::glyph_paths;
use runebender::outline::glyph_paths::round_units;
use runebender::outline::point_ops;
use runebender::ui::editing::edit_types::EditType;
use runebender::ui::editing::viewport::ViewPort;

/// Boolean operation kinds, mapped to `linesweeper::BinaryOp` internally.
#[derive(Clone, Copy)]
pub(crate) enum BoolOp {
    Union,
    Subtract,
    Intersect,
    Exclude,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Metrics {
    pub upm: f64,
    pub ascender: f64,
    pub descender: f64,
    pub x_height: f64,
    pub cap_height: f64,
}

impl Metrics {
    pub(crate) fn of(font: &norad::Font) -> Self {
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

    fn of_canonical(info: &runebender::document::model::font_info::CanonicalFontInfo) -> Self {
        let metrics = info.metrics.resolved();
        Self {
            upm: metrics.units_per_em,
            ascender: metrics.ascender,
            descender: metrics.descender,
            x_height: metrics.x_height,
            cap_height: metrics.cap_height,
        }
    }
}

/// One point, as the editor sees it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PointView {
    pub id: PointId,
    pub point: Point,
    pub on_curve: bool,
    pub smooth: bool,
    pub start: bool,
}

#[derive(Clone)]
pub(crate) struct Session {
    pub glyph_name: String,
    pub glyph: norad::Glyph,
    pub metaballs: metaballs::MetaballSelection,
    pub metaball_preview: BezPath,
    /// Components, resolved against the font at session creation.
    pub components: BezPath,
    /// Resolved path for each top-level component, for selection and feedback.
    component_paths: Vec<BezPath>,
    /// Contours the components resolve to, precomputed for decompose.
    component_contours: Vec<Vec<norad::Contour>>,
    pub metrics: Metrics,
    pub selection: HashSet<PointId>,
    pub viewport: ViewPort,
    pub fitted: bool,
    /// Undo steps taken since the app last pulled this session: the
    /// pile itself is the master's, in core. The app drains these on
    /// every sync.
    pub(crate) pending: Vec<HistoryOp>,
    drag_originals: HashMap<PointId, (f64, f64)>,
    in_drag: bool,
    /// The contour the pen is currently extending, if any.
    pub active_contour: Option<usize>,
    /// In-progress pen points (on- and off-curve), materialized into
    /// `active_contour` on each change.
    pen: Vec<PenPt>,
    /// The currently selected anchor, if any.
    pub selected_anchor: Option<usize>,
    /// The selected top-level component, if any.
    pub selected_component: Option<usize>,
    /// Last flip or rotation, re-applied by Duplicate + Repeat.
    last_transform: Option<kurbo::Affine>,
}

/// An undo step the session took, for the master's pile.
#[derive(Clone)]
pub(crate) enum HistoryOp {
    /// The glyph as it was before an edit.
    Record(Box<norad::Glyph>),
    /// The last step turned out empty; drop it.
    DiscardLast,
}

/// One point in the pen's in-progress buffer.
#[derive(Clone, Copy)]
struct PenPt {
    point: Point,
    off: bool,
    smooth: bool,
}

impl Session {
    /// Build an inactive session with metrics from the canonical active source.
    pub(crate) fn inactive_from_model(font: &FontModel) -> Self {
        let mut session = Self::inactive(font.font());
        session.metrics = Metrics::of_canonical(font.font_info());
        session
    }

    /// Build an editor session with metrics from the canonical active source.
    pub(crate) fn new_from_model(font: &FontModel, name: &str) -> Option<Self> {
        let mut session = Self::new(font.font(), name)?;
        session.metrics = Metrics::of_canonical(font.font_info());
        Some(session)
    }

    /// Makes the inactive session held while the overview has no glyph to open.
    ///
    /// The editor only reads this session in [`Mode::Editor`]. Keeping an
    /// inert session here avoids making every editor-facing view optional when
    /// a valid UFO has no glyphs yet; opening the first glyph replaces it.
    pub(crate) fn inactive(font: &norad::Font) -> Self {
        Self {
            glyph_name: String::new(),
            glyph: norad::Glyph::new(".notdef"),
            metaball_preview: BezPath::new(),
            components: BezPath::new(),
            component_paths: Vec::new(),
            component_contours: Vec::new(),
            metaballs: metaballs::MetaballSelection::default(),
            metrics: Metrics::of(font),
            selection: HashSet::new(),
            viewport: ViewPort::new(),
            fitted: false,
            pending: Vec::new(),
            drag_originals: HashMap::new(),
            in_drag: false,
            active_contour: None,
            pen: Vec::new(),
            selected_anchor: None,
            selected_component: None,
            last_transform: None,
        }
    }

    pub(crate) fn new(font: &norad::Font, name: &str) -> Option<Self> {
        let glyph = font.get_glyph(name)?.clone();
        let components = glyph_paths::components_to_bezpath(&glyph, font);
        let component_paths = resolved_component_paths(font, &glyph);
        let component_contours = resolved_component_contour_sets(font, &glyph);
        Some(Self {
            glyph_name: name.to_string(),
            metaball_preview: runebender::outline::metaballs::glyph_preview(&glyph)
                .unwrap_or_default(),
            glyph,
            components,
            component_paths,
            component_contours,
            metaballs: metaballs::MetaballSelection::default(),
            metrics: Metrics::of(font),
            selection: HashSet::new(),
            viewport: ViewPort::new(),
            fitted: false,
            pending: Vec::new(),
            drag_originals: HashMap::new(),
            in_drag: false,
            active_contour: None,
            pen: Vec::new(),
            selected_anchor: None,
            selected_component: None,
            last_transform: None,
        })
    }

    pub(crate) fn advance(&self) -> f64 {
        self.glyph.width
    }

    /// Whether a gesture currently owns the session's undo transaction.
    #[cfg_attr(
        not(unix),
        allow(dead_code, reason = "the live document mailbox is Unix-only")
    )]
    pub(crate) fn gesture_in_progress(&self) -> bool {
        self.in_drag
    }

    /// Shift all points and anchors horizontally (left-sidebearing drag).
    pub(crate) fn shift_glyph(&mut self, dx: f64) {
        if !dx.is_finite() || dx == 0.0 {
            return;
        }
        self.record(EditType::Drag);
        for contour in &mut self.glyph.contours {
            for p in &mut contour.points {
                p.x += dx;
            }
        }
        for a in &mut self.glyph.anchors {
            a.x += dx;
        }
    }

    pub(crate) fn set_advance(&mut self, w: f64) {
        let width = w.max(0.0);
        if !w.is_finite() || self.glyph.width == width {
            return;
        }
        self.record(EditType::Normal);
        self.glyph.width = width;
    }

    /// Set advance during a pointer gesture, grouped into one undo step.
    pub(crate) fn drag_advance(&mut self, w: f64) {
        let width = w.max(0.0);
        if !w.is_finite() || self.glyph.width == width {
            return;
        }
        self.record(EditType::Drag);
        self.glyph.width = width;
    }

    /// Close an anchor, advance, or sidebearing pointer transaction.
    pub(crate) fn end_metric_drag(&mut self) {
        self.record(EditType::DragUp);
    }

    pub(crate) fn outline_arc(&self) -> Arc<BezPath> {
        Arc::new(self.outline())
    }

    pub(crate) fn components_arc(&self) -> Arc<BezPath> {
        Arc::new(self.components.clone())
    }

    pub(crate) fn selected_component_path(&self) -> Option<&BezPath> {
        self.component_paths.get(self.selected_component?)
    }

    pub(crate) fn component_at(&self, point: Point) -> Option<usize> {
        use kurbo::Shape as _;
        self.component_paths
            .iter()
            .enumerate()
            .rev()
            .find(|(_, path)| path.contains(point))
            .map(|(index, _)| index)
    }

    pub(crate) fn select_component(&mut self, index: usize) -> bool {
        if index >= self.glyph.components.len() {
            return false;
        }
        self.selection.clear();
        self.selected_anchor = None;
        self.selected_component = Some(index);
        true
    }

    pub(crate) fn selected_component_aligned(&self) -> Option<bool> {
        self.glyph
            .components
            .get(self.selected_component?)
            .map(|component| {
                !runebender::document::composites::component_alignment_disabled(component)
            })
    }

    fn rebuild_combined_components(&mut self) {
        self.components = self
            .component_paths
            .iter()
            .fold(BezPath::new(), |mut all, path| {
                all.extend(path.iter());
                all
            });
    }

    #[cfg(test)]
    fn rebuild_component_caches(&mut self, font: &norad::Font) {
        self.component_paths = resolved_component_paths(font, &self.glyph);
        self.component_contours = resolved_component_contour_sets(font, &self.glyph);
        self.rebuild_combined_components();
    }

    #[cfg(test)]
    pub(crate) fn add_component(&mut self, font: &norad::Font, base: &str) -> bool {
        let mut changed = self.glyph.clone();
        if !runebender::outline::component_ops::add_component(font, &mut changed, base) {
            return false;
        }
        self.record(EditType::Normal);
        self.glyph = changed;
        self.selected_component = Some(self.glyph.components.len() - 1);
        self.selection.clear();
        self.selected_anchor = None;
        self.rebuild_component_caches(font);
        true
    }

    #[cfg(test)]
    pub(crate) fn toggle_component_alignment(&mut self, font: &norad::Font) -> bool {
        let Some(index) = self.selected_component else {
            return false;
        };
        let mut changed = self.glyph.clone();
        let Some(component) = changed.components.get_mut(index) else {
            return false;
        };
        let aligned = !runebender::document::composites::component_alignment_disabled(component);
        runebender::document::composites::set_component_alignment_disabled(component, aligned);
        if !aligned {
            runebender::document::composites::realign_glyph(font, &mut changed, true);
        }
        self.record(EditType::Normal);
        self.glyph = changed;
        self.rebuild_component_caches(font);
        true
    }

    pub(crate) fn drag_component_by(&mut self, dx: f64, dy: f64) -> bool {
        if self.selected_component.is_none()
            || self.selected_component_aligned() == Some(true)
            || !dx.is_finite()
            || !dy.is_finite()
            || (dx == 0.0 && dy == 0.0)
        {
            return false;
        }
        self.record(EditType::Drag);
        self.translate_selected_component(dx, dy)
    }

    fn translate_selected_component(&mut self, dx: f64, dy: f64) -> bool {
        let Some(index) = self.selected_component else {
            return false;
        };
        let changed =
            runebender::outline::component_ops::translate_component(&mut self.glyph, index, dx, dy);
        if changed {
            if let Some(path) = self.component_paths.get_mut(index) {
                *path = kurbo::Affine::translate((dx, dy)) * path.clone();
            }
            if let Some(contours) = self.component_contours.get_mut(index) {
                for contour in contours {
                    for point in &mut contour.points {
                        point.x += dx;
                        point.y += dy;
                    }
                }
            }
            self.rebuild_combined_components();
        }
        changed
    }

    pub(crate) fn end_component_drag(&mut self) {
        self.record(EditType::DragUp);
    }

    pub(crate) fn outline(&self) -> BezPath {
        let mut path = glyph_paths::contours_to_bezpath(&self.glyph);
        path.extend(self.metaball_preview.clone());
        path
    }

    pub(crate) fn points(&self) -> Vec<PointView> {
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

    pub(crate) fn point_count(&self) -> usize {
        self.glyph.contours.iter().map(|c| c.points.len()).sum()
    }

    // ---- edits ----

    /// Record the state before an edit.
    pub(crate) fn record(&mut self, edit: EditType) {
        match edit {
            EditType::Drag => {
                if !self.in_drag {
                    self.pending
                        .push(HistoryOp::Record(Box::new(self.glyph.clone())));
                    self.in_drag = true;
                }
            }
            EditType::DragUp => self.in_drag = false,
            _ => self
                .pending
                .push(HistoryOp::Record(Box::new(self.glyph.clone()))),
        }
    }

    /// Take the glyph as the master now has it, after an undo or a
    /// redo there, keeping the selection where it still fits.
    pub(crate) fn reload_glyph(&mut self, font: &norad::Font, glyph: norad::Glyph) {
        self.components = glyph_paths::components_to_bezpath(&glyph, font);
        self.component_paths = resolved_component_paths(font, &glyph);
        self.component_contours = resolved_component_contour_sets(font, &glyph);
        self.selected_component = self
            .selected_component
            .filter(|index| *index < glyph.components.len());
        self.glyph = glyph;
        self.refresh_metaball_preview();
        self.pen.clear();
        self.active_contour = None;
        self.in_drag = false;
        self.selected_anchor = self
            .selected_anchor
            .filter(|index| *index < self.glyph.anchors.len());
        self.prune_selection();
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

    pub(crate) fn begin_point_drag(&mut self) {
        self.record(EditType::Drag);
        self.drag_originals = point_ops::drag_origins(&self.glyph, &self.selection, false);
    }

    /// Move the selection to `total` design units from where the drag began.
    pub(crate) fn drag_points_to(&mut self, total: (f64, f64)) -> bool {
        point_ops::translate_points(
            &mut self.glyph,
            &self.selection,
            &self.drag_originals,
            total,
            false,
        )
    }

    pub(crate) fn end_point_drag(&mut self) {
        self.record(EditType::DragUp);
        self.drag_originals.clear();
    }

    pub(crate) fn nudge(&mut self, dx: f64, dy: f64) -> bool {
        if self.selected_component.is_some() {
            if !dx.is_finite() || !dy.is_finite() || (dx == 0.0 && dy == 0.0) {
                return false;
            }
            if self.selected_component_aligned() == Some(true) {
                return false;
            }
            self.record(EditType::Normal);
            return self.translate_selected_component(dx, dy);
        }
        if self.selection.is_empty() {
            return false;
        }
        self.record(EditType::Normal);
        point_ops::translate_points(
            &mut self.glyph,
            &self.selection,
            &HashMap::new(),
            (dx, dy),
            false,
        )
    }

    pub(crate) fn delete_selected(&mut self) -> bool {
        if let Some(index) = self.selected_component.take() {
            self.record(EditType::Normal);
            if runebender::outline::component_ops::delete_component(&mut self.glyph, index) {
                self.component_paths.remove(index);
                self.component_contours.remove(index);
                self.rebuild_combined_components();
                return true;
            }
            return false;
        }
        if self.selection.is_empty() {
            return false;
        }
        self.record(EditType::Normal);
        let changed = glyph_ops::delete_points(&mut self.glyph, &self.selection);
        self.selection.clear();
        changed
    }

    /// The first point of the pen buffer, in design space.
    pub(crate) fn pen_first_point(&self) -> Option<Point> {
        self.pen.first().map(|p| p.point)
    }

    pub(crate) fn pen_is_active(&self) -> bool {
        !self.pen.is_empty()
    }

    /// Write the pen buffer into `active_contour`, creating it if needed.
    fn pen_sync(&mut self) {
        let ci = match self.active_contour {
            Some(c) if c < self.glyph.contours.len() => c,
            _ => {
                self.glyph
                    .contours
                    .push(norad::Contour::new(Vec::new(), None));
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
            points.push(norad::ContourPoint::new(
                pt.point.x, pt.point.y, typ, pt.smooth, None, None,
            ));
            prev_off = pt.off;
        }
        self.glyph.contours[ci].points = points;
    }

    /// Place a corner on-curve point (a plain click).
    pub(crate) fn pen_corner(&mut self, x: f64, y: f64) {
        if self.pen.is_empty() {
            self.record(EditType::Normal);
        }
        self.pen.push(PenPt {
            point: Point::new(x, y),
            off: false,
            smooth: false,
        });
        self.pen_sync();
    }

    /// Begin a smooth point with symmetric handles at `origin`; the outgoing
    /// handle starts at `to`.
    pub(crate) fn pen_smooth_begin(&mut self, origin: Point, to: Point) {
        if self.pen.is_empty() {
            self.record(EditType::Normal);
        }
        self.pen.push(PenPt {
            point: origin,
            off: true,
            smooth: false,
        });
        self.pen.push(PenPt {
            point: origin,
            off: false,
            smooth: true,
        });
        self.pen.push(PenPt {
            point: to,
            off: true,
            smooth: false,
        });
        self.pen_sync();
    }

    /// Update the handles of the smooth point currently being dragged.
    pub(crate) fn pen_smooth_drag(&mut self, origin: Point, to: Point) {
        let n = self.pen.len();
        if n < 3 {
            return;
        }
        self.pen[n - 1].point = to;
        self.pen[n - 3].point = Point::new(2.0 * origin.x - to.x, 2.0 * origin.y - to.y);
        self.pen_sync();
    }

    /// Close the active contour.
    pub(crate) fn pen_close(&mut self) {
        if let Some(c) = self.active_contour.take()
            && let Some(contour) = self.glyph.contours.get_mut(c)
            && contour.points.first().map(|p| p.typ) == Some(norad::PointType::Move)
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
                first.x,
                first.y,
                typ,
                first.smooth,
                None,
                None,
            ));
        }
        self.pen.clear();
    }

    /// End the current pen path without closing (Escape / tool switch).
    pub(crate) fn pen_cancel(&mut self) {
        self.active_contour = None;
        self.pen.clear();
    }

    // ---- hyperbezier pen: on-curve points only, curve solved by the spline ----

    /// Add a hyperbezier on-curve point (smooth), starting a contour if idle.
    pub(crate) fn hyper_add(&mut self, x: f64, y: f64, corner: bool) {
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

    pub(crate) fn hyper_close(&mut self) {
        if let Some(c) = self.active_contour.take() {
            self.record(EditType::Normal);
            glyph_ops::close_hyper_contour(&mut self.glyph, c);
        }
    }

    pub(crate) fn first_contour_point(&self) -> Option<Point> {
        let c = self.active_contour?;
        let p = self.glyph.contours.get(c)?.points.first()?;
        Some(Point::new(p.x, p.y))
    }

    pub(crate) fn hyper_is_active(&self) -> bool {
        self.active_contour.is_some() && self.pen.is_empty()
    }

    /// Add a closed rectangle contour.
    pub(crate) fn add_rect(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        let (lx, rx) = (x0.min(x1), x0.max(x1));
        let (by, ty) = (y0.min(y1), y0.max(y1));
        if (rx - lx).abs() < 1.0 || (ty - by).abs() < 1.0 {
            return;
        }
        self.record(EditType::Normal);
        let corner =
            |x, y| norad::ContourPoint::new(x, y, norad::PointType::Line, false, None, None);
        let points = vec![
            corner(lx, by),
            corner(rx, by),
            corner(rx, ty),
            corner(lx, ty),
        ];
        self.glyph.contours.push(norad::Contour::new(points, None));
    }

    /// Add a closed ellipse contour (four cubic segments).
    pub(crate) fn add_ellipse(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (rx, ry) = ((x1 - x0).abs() / 2.0, (y1 - y0).abs() / 2.0);
        if rx < 1.0 || ry < 1.0 {
            return;
        }
        self.record(EditType::Normal);
        const K: f64 = 0.552_284_749_831;
        let on = |x, y| norad::ContourPoint::new(x, y, norad::PointType::Curve, true, None, None);
        let off =
            |x, y| norad::ContourPoint::new(x, y, norad::PointType::OffCurve, false, None, None);
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
    pub(crate) fn transform(&mut self, affine: kurbo::Affine) -> bool {
        self.record(EditType::Normal);
        let changed = glyph_ops::transform_selection(&mut self.glyph, &self.selection, affine);
        if changed {
            self.last_transform = Some(affine);
        }
        changed
    }

    pub(crate) fn flip_horizontal(&mut self) -> bool {
        self.transform(kurbo::Affine::new([-1.0, 0.0, 0.0, 1.0, 0.0, 0.0]))
    }

    pub(crate) fn flip_vertical(&mut self) -> bool {
        self.transform(kurbo::Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, 0.0]))
    }

    /// Rotate counterclockwise in the font's upward-Y coordinate system.
    pub(crate) fn rotate_90(&mut self) -> bool {
        self.transform(kurbo::Affine::new([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]))
    }

    /// Rotate clockwise around the selected geometry's center.
    pub(crate) fn rotate_90_clockwise(&mut self) -> bool {
        self.transform(kurbo::Affine::new([0.0, -1.0, 1.0, 0.0, 0.0, 0.0]))
    }

    /// Apply a whole-glyph outline effect under one undo record.
    fn effect(&mut self, operation: impl FnOnce(&mut norad::Glyph) -> bool) -> bool {
        let mut changed = self.glyph.clone();
        if !operation(&mut changed) {
            return false;
        }
        self.record(EditType::Normal);
        self.glyph = changed;
        self.selection.clear();
        true
    }

    /// Expand selected contours (or all contours) under one undo record.
    pub(crate) fn expand_stroke(&mut self, width: f64) -> bool {
        if !width.is_finite() || width <= 0.0 || self.selected_component.is_some() {
            return false;
        }
        let contours = self.selection.iter().map(|(contour, _)| *contour).collect();
        self.effect(|glyph| {
            runebender::outline::effects::expand_stroke_contours(glyph, &contours, width)
        })
    }

    pub(crate) fn offset(&mut self, delta: f64) -> bool {
        self.effect(|glyph| runebender::outline::effects::offset_glyph_contours(glyph, delta))
    }

    pub(crate) fn extrude(&mut self, offset: f64, angle: f64, keep_front: bool) -> bool {
        self.effect(|glyph| {
            runebender::outline::effects::extrude_glyph_contours(glyph, offset, angle, keep_front)
        })
    }

    pub(crate) fn roughen(
        &mut self,
        segment: f64,
        horizontal: f64,
        vertical: f64,
        seed: u64,
    ) -> bool {
        let selected = self.selection.iter().map(|(contour, _)| *contour).collect();
        self.effect(|glyph| {
            runebender::outline::effects::roughen_glyph_contours(
                glyph, &selected, segment, horizontal, vertical, seed,
            )
        })
    }

    pub(crate) fn reverse(&mut self) -> bool {
        self.record(EditType::Normal);
        glyph_ops::reverse_contours(&mut self.glyph, &self.selection)
    }

    pub(crate) fn decompose(&mut self) -> bool {
        if self.glyph.components.is_empty() || self.component_contours.is_empty() {
            return false;
        }
        self.record(EditType::Normal);
        for contours in &mut self.component_contours {
            self.glyph.contours.append(contours);
        }
        self.glyph.components.clear();
        self.components = BezPath::new();
        self.component_paths.clear();
        self.selected_component = None;
        true
    }

    pub(crate) fn boolean(&mut self, op: BoolOp) -> bool {
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

    pub(crate) fn remove_overlap(&mut self) -> bool {
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
    pub(crate) fn knife_hits(&self, p0: Point, p1: Point) -> Vec<Point> {
        runebender::outline::knife::knife_hit_points(&self.glyph, p0, p1)
    }

    /// Cut the outline along the line p0..p1.
    pub(crate) fn knife_cut(&mut self, p0: Point, p1: Point) -> bool {
        self.record(EditType::Normal);
        let changed = runebender::outline::knife::knife_cut_glyph(&mut self.glyph, p0, p1);
        if !changed {
            // Nothing cut; drop the empty undo group we just pushed.
            if matches!(self.pending.last(), Some(HistoryOp::Record(_))) {
                self.pending.pop();
            } else {
                self.pending.push(HistoryOp::DiscardLast);
            }
        }
        changed
    }

    /// The glyph's contours as core `Path`s (for measurement/analysis).
    pub(crate) fn paths(&self) -> Vec<runebender::outline::path::Path> {
        self.glyph
            .contours
            .iter()
            .map(|c| {
                runebender::outline::path::Path::from_contour(
                    &runebender::outline::path::hyper_model::Contour::from_norad(c),
                )
            })
            .collect()
    }

    pub(crate) fn measurements(&self) -> Vec<runebender::analysis::measure::Measurement> {
        runebender::analysis::measure::glyph_measurements(&self.paths())
    }

    pub(crate) fn side_bearings(&self) -> Option<runebender::analysis::measure::SideBearings> {
        runebender::analysis::measure::side_bearings(&self.paths(), self.advance())
    }

    /// Bounding box of the selected points in design space, if any.
    pub(crate) fn selection_bounds(&self) -> Option<Rect> {
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
    pub(crate) fn set_start(&mut self) -> bool {
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
    pub(crate) fn round_corners(&mut self) -> bool {
        self.record(EditType::Normal);
        match glyph_ops::round_selected_corners(&mut self.glyph, &self.selection) {
            Some(next) => {
                self.selection = next;
                true
            }
            None => false,
        }
    }

    pub(crate) fn harmonize(&mut self) -> bool {
        self.record(EditType::Normal);
        glyph_ops::curve_op(
            &mut self.glyph,
            &self.selection,
            glyph_ops::CurveOp::Harmonize,
        )
    }

    pub(crate) fn balance(&mut self) -> bool {
        self.record(EditType::Normal);
        glyph_ops::curve_op(
            &mut self.glyph,
            &self.selection,
            glyph_ops::CurveOp::Balance,
        )
    }

    pub(crate) fn optimize(&mut self) -> bool {
        self.record(EditType::Normal);
        glyph_ops::curve_op(
            &mut self.glyph,
            &self.selection,
            glyph_ops::CurveOp::Optimize(0.12),
        )
    }

    pub(crate) fn duplicate(&mut self) -> bool {
        if let Some(index) = self.selected_component {
            let Some(path) = self.component_paths.get(index).cloned() else {
                return false;
            };
            self.record(EditType::Normal);
            let Some(next) =
                runebender::outline::component_ops::duplicate_component(&mut self.glyph, index)
            else {
                return false;
            };
            self.component_paths
                .push(kurbo::Affine::translate((20.0, 20.0)) * path);
            let mut contours = self
                .component_contours
                .get(index)
                .cloned()
                .unwrap_or_default();
            for contour in &mut contours {
                for point in &mut contour.points {
                    point.x += 20.0;
                    point.y += 20.0;
                }
            }
            self.component_contours.push(contours);
            self.selected_component = Some(next);
            self.rebuild_combined_components();
            return true;
        }
        self.record(EditType::Normal);
        match glyph_ops::duplicate_selection(&mut self.glyph, &self.selection) {
            Some(next) => {
                self.selection = next;
                true
            }
            None => false,
        }
    }

    pub(crate) fn duplicate_repeat(&mut self) -> bool {
        let transform = self.last_transform;
        if !self.duplicate() {
            return false;
        }
        if let Some(transform) = transform {
            let _ = glyph_ops::transform_selection(&mut self.glyph, &self.selection, transform);
        }
        true
    }

    pub(crate) fn tidy_paths(&mut self) -> bool {
        self.record(EditType::Normal);
        runebender::outline::cleanup::tidy_contours(&mut self.glyph) > 0
    }

    pub(crate) fn add_extremes(&mut self) -> bool {
        self.record(EditType::Normal);
        runebender::outline::cleanup::add_extreme_points(&mut self.glyph, &self.selection)
    }

    pub(crate) fn round_coordinates(&mut self) -> bool {
        self.record(EditType::Normal);
        runebender::outline::cleanup::round_glyph_coordinates(&mut self.glyph) > 0
    }

    pub(crate) fn correct_path_direction(&mut self) -> bool {
        self.record(EditType::Normal);
        runebender::outline::cleanup::correct_path_directions(&mut self.glyph) > 0
    }

    pub(crate) fn hyper_to_cubic(&mut self) -> bool {
        self.record(EditType::Normal);
        let changed = glyph_ops::convert_hyper_to_cubic(&mut self.glyph, &self.selection);
        if changed {
            self.selection.clear();
        }
        changed
    }

    pub(crate) fn quads_to_cubics(&mut self) -> bool {
        self.record(EditType::Normal);
        runebender::outline::convert::quads_to_cubics(&mut self.glyph)
    }

    pub(crate) fn cubics_to_quads(&mut self) -> bool {
        self.record(EditType::Normal);
        runebender::outline::convert::cubics_to_quads(&mut self.glyph, 1.0)
    }

    /// Index of the anchor near `p` (design space), if within `tol`.
    pub(crate) fn anchor_at(&self, p: Point, tol: f64) -> Option<usize> {
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

    pub(crate) fn add_anchor(&mut self, x: f64, y: f64) {
        self.record(EditType::Normal);
        let n = self.glyph.anchors.len();
        let name = norad::Name::new(&format!("anchor.{n}")).ok();
        self.glyph
            .anchors
            .push(norad::Anchor::new(x, y, name, None, None));
        self.selected_anchor = Some(n);
    }

    pub(crate) fn move_anchor(&mut self, idx: usize, x: f64, y: f64) {
        let Some(anchor) = self.glyph.anchors.get(idx) else {
            return;
        };
        if !x.is_finite() || !y.is_finite() || (anchor.x == x && anchor.y == y) {
            return;
        }
        self.record(EditType::Drag);
        let anchor = &mut self.glyph.anchors[idx];
        anchor.x = x;
        anchor.y = y;
    }

    pub(crate) fn delete_selected_anchor(&mut self) -> bool {
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

    /// Continuity of every on-curve node: corner, kink, G1, G2, G3.
    pub(crate) fn continuity(&self) -> Vec<runebender::analysis::curve::NodeContinuity> {
        let cubics = runebender::analysis::curve::cubics_from_norad(&self.glyph);
        runebender::analysis::curve::node_continuity(&cubics)
    }

    /// The outline split into strokes colored by segment length, the web
    /// editor's colorize mode.
    pub(crate) fn colored_strokes(&self) -> Vec<runebender::analysis::measure::ColoredStroke> {
        runebender::analysis::measure::colored_strokes(&self.paths())
    }

    pub(crate) fn curvature_comb(&self) -> Vec<Vec<runebender::analysis::curve::CombSample>> {
        let cubics = runebender::analysis::curve::cubics_from_norad(&self.glyph);
        let maxk = runebender::analysis::curve::max_curvature(&cubics);
        if maxk <= 1e-12 {
            return Vec::new();
        }
        runebender::analysis::curve::curvature_comb(&cubics, 1.0, 74.0 / maxk, false, 16)
    }

    pub(crate) fn set_mark(&mut self, label: Option<&str>) {
        self.record(EditType::Normal);
        runebender::ui::theme::set_glyph_mark(&mut self.glyph, label);
    }

    /// The contours to copy: the ones holding a selected point, or every
    /// contour when nothing is selected. This is shared with the web editor.
    pub(crate) fn contours_for_copy(&self) -> Vec<norad::Contour> {
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

    /// Replace every contour, keeping the advance. Used by the swap with
    /// the background layer, which is an edit like any other.
    pub(crate) fn set_contours(&mut self, contours: Vec<norad::Contour>) -> bool {
        self.record(EditType::Normal);
        self.glyph.contours = contours;
        self.selection.clear();
        true
    }

    /// Append contours to the glyph, and select the points they brought.
    pub(crate) fn paste_contours(&mut self, contours: &[norad::Contour]) -> bool {
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

    pub(crate) fn select_all(&mut self) {
        self.selected_component = None;
        self.selection = self.points().into_iter().map(|p| p.id).collect();
    }
}

fn resolved_component_paths(font: &norad::Font, glyph: &norad::Glyph) -> Vec<BezPath> {
    glyph
        .components
        .iter()
        .map(|component| {
            font.get_glyph(&component.base)
                .map_or_else(BezPath::new, |base| {
                    glyph_paths::component_affine(&component.transform)
                        * glyph_paths::glyph_to_bezpath(base, font)
                })
        })
        .collect()
}

/// Resolve each top-level component separately so selection edits and
/// decomposition keep the same cached geometry.
fn resolved_component_contour_sets(
    font: &norad::Font,
    glyph: &norad::Glyph,
) -> Vec<Vec<norad::Contour>> {
    glyph
        .components
        .iter()
        .map(|component| {
            let mut wrapper = norad::Glyph::new("component-wrapper");
            wrapper.components.push(component.clone());
            runebender::outline::component_ops::resolved_component_contours(font, &wrapper)
        })
        .collect()
}

impl Workspace {
    /// The OKLCH themes in menu order.
    pub(crate) const THEMES: [&'static str; 3] = ["dark", "gray", "light"];

    fn text_context(&self) -> TextContext {
        TextContext {
            editor_text: self.initial_text.clone(),
            has_text_session: self.has_text_session,
            preview_text: self.preview_text.clone(),
            direction: self.text_dir,
            features_disabled: self.text_features_disabled.clone(),
            script: self.text_script.clone(),
            language: self.text_language.clone(),
        }
    }

    pub(crate) fn restore_text_context(&mut self, context: TextContext) {
        self.initial_text = context.editor_text;
        self.has_text_session = context.has_text_session;
        self.preview_text = context.preview_text;
        self.text_dir = context.direction;
        self.text_features_disabled = context.features_disabled;
        self.text_script = context.script;
        self.text_language = context.language;
    }

    /// Stable identity of the active tab's widget-owned text buffer.
    pub(crate) fn text_context_id(&self) -> (u64, u64) {
        (
            self.document_id,
            self.tabs
                .get(self.active_tab)
                .map_or(u64::MAX, |tab| tab.text_context_id),
        )
    }

    /// Keep the active tab's plain text copy in step with the editor widget.
    pub(crate) fn set_editor_text(&mut self, text: String) {
        self.begin_text_session();
        self.initial_text = text.clone();
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.text_context.editor_text = text;
        }
    }

    /// Open the active tab's text composition without coupling it to a tool.
    pub(crate) fn begin_text_session(&mut self) {
        self.has_text_session = true;
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.text_context.has_text_session = true;
        }
    }

    /// Pick a tool while preserving any text composition already on the tab.
    pub(crate) fn select_tool(&mut self, tool: Tool) {
        self.tool_before_space_pan = None;
        self.tool = tool;
        if tool == Tool::Metaball && self.session.metaballs.selected.is_empty() {
            Arc::make_mut(&mut self.session).select_all_metaballs();
        }
        if tool == Tool::Text {
            self.begin_text_session();
        }
    }

    /// Temporarily use the Hand tool while Space is held.
    pub(crate) fn begin_space_pan(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) || self.tool_before_space_pan.is_some() {
            return;
        }
        self.tool_before_space_pan = Some(self.tool);
        self.tool = Tool::Hand;
    }

    /// Restore the tool that was active before a Space-held pan.
    pub(crate) fn end_space_pan(&mut self) {
        if let Some(tool) = self.tool_before_space_pan.take() {
            self.tool = tool;
        }
    }

    fn persistent_tool(&self) -> Tool {
        self.tool_before_space_pan.unwrap_or(self.tool)
    }

    /// Write the live session back into its tab, so switching away from
    /// it does not lose the edit, the selection, or the undo stack.
    pub(crate) fn park(&mut self) {
        let text_context = self.text_context();
        let persistent_tool = self.persistent_tool();
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.session = self.session.clone();
            tab.tool = persistent_tool;
            tab.text_context = text_context;
        }
    }

    /// Leave the editor while keeping any parked session aligned to the active master.
    pub(crate) fn settle_on_overview(&mut self) {
        self.end_space_pan();
        self.mode = Mode::Overview;
        self.selected = None;
        if self.tabs.is_empty() {
            self.active_tab = 0;
            self.session = Arc::new(Session::inactive_from_model(&self.font));
        } else {
            self.active_tab = self.active_tab.min(self.tabs.len() - 1);
            let tab = &self.tabs[self.active_tab];
            let (session, tool, text_context) =
                (tab.session.clone(), tab.tool, tab.text_context.clone());
            self.session = session;
            self.tool = tool;
            self.restore_text_context(text_context);
        }
        self.selected_points = 0;
    }

    /// Make a tab the live one.
    pub(crate) fn activate_tab(&mut self, index: usize) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let (session, tool, text_context) =
            (tab.session.clone(), tab.tool, tab.text_context.clone());
        self.park();
        self.active_tab = index;
        self.session = session;
        // An overview batch can change this glyph while its tab is parked.
        if let Some(glyph) = self
            .font
            .font()
            .get_glyph(&self.session.glyph_name)
            .cloned()
        {
            Arc::make_mut(&mut self.session).reload_glyph(self.font.font(), glyph);
        }
        self.tool_before_space_pan = None;
        self.tool = tool;
        self.restore_text_context(text_context);
        let name = self.session.glyph_name.clone();
        if let Some(glyph) = self.font.index_of(&name) {
            self.selected = Some(glyph);
            self.mode = Mode::Editor(glyph);
        }
        self.selected_points = 0;
        self.refresh_metric_bufs();
        self.refresh_coord_bufs();
        self.name_buf = name;
        self.unicode_buf = self
            .selected
            .and_then(|i| self.font.glyphs.get(i))
            .and_then(|g| g.codepoint)
            .map(|c| format!("{:04X}", c as u32))
            .unwrap_or_default();
    }

    /// A second tab on the glyph that is open, with its own session.
    pub(crate) fn new_tab(&mut self) {
        let Some(session) = Session::new_from_model(&self.font, &self.session.glyph_name) else {
            return;
        };
        let persistent_tool = self.persistent_tool();
        self.park();
        self.tabs.push(Tab {
            text_context_id: host::NEXT_TEXT_CONTEXT_ID
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            session: Arc::new(session),
            tool: persistent_tool,
            text_context: self.text_context(),
        });
        self.activate_tab(self.tabs.len() - 1);
    }

    /// Close a tab. Closing the last one leaves the editor.
    pub(crate) fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            self.back_to_overview();
            return;
        }
        self.park();
        self.tabs.remove(index);
        let next = if self.active_tab > index {
            self.active_tab - 1
        } else {
            self.active_tab.min(self.tabs.len() - 1)
        };
        self.active_tab = usize::MAX; // parking again would write to the wrong tab
        self.activate_tab(next);
    }

    pub(crate) fn open_glyph(&mut self, index: usize) {
        // A glyph that already has a tab gets that tab, rather than a
        // second one on the same glyph.
        if let Some(entry) = self.font.glyphs.get(index) {
            let name = entry.name.clone();
            if let Some(existing) = self
                .tabs
                .iter()
                .position(|tab| tab.session.glyph_name == name)
            {
                self.activate_tab(existing);
                return;
            }
        }
        self.replace_active_tab_glyph(index);
    }

    /// Edit one sort from the current text buffer without changing tabs or
    /// replacing that tab's text context.
    ///
    /// A grid open follows an existing glyph tab, but a sort activation is
    /// local to the word being edited. GPUI changes only the active edit
    /// session, and Web reloads only that sort's glyph metrics; both keep the
    /// surrounding text buffer intact.
    pub(crate) fn edit_text_sort_glyph(&mut self, index: usize, tool: Tool) {
        if self.replace_active_tab_glyph(index) {
            self.select_tool(tool);
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.tool = tool;
            }
        }
    }

    /// Replace the glyph session in the active tab while retaining the tab's
    /// stable identity and parked text/preview state.
    fn replace_active_tab_glyph(&mut self, index: usize) -> bool {
        if let Some(entry) = self.font.glyphs.get(index)
            && let Some(session) = Session::new_from_model(&self.font, &entry.name)
        {
            self.advance_buf = format!("{}", round_units(session.advance()));
            let (l, r) = metric_bufs(&session);
            self.lsb_buf = l;
            self.rsb_buf = r;
            self.name_buf = entry.name.clone();
            self.unicode_buf = entry
                .codepoint
                .map(|c| format!("{:04X}", c as u32))
                .unwrap_or_default();
            self.session = Arc::new(session);
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.session = self.session.clone();
            } else {
                self.tabs.push(Tab {
                    text_context_id: host::NEXT_TEXT_CONTEXT_ID
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                    session: self.session.clone(),
                    tool: self.persistent_tool(),
                    text_context: self.text_context(),
                });
                self.active_tab = self.tabs.len() - 1;
            }
            self.selected = Some(index);
            self.selected_points = 0;
            self.mode = Mode::Editor(index);
            return true;
        }
        false
    }

    /// After an edit, pull the glyph back out of the session and refresh
    /// the model + grid cache so the overview preview matches.
    /// Replace the app's session with the island's live one (called on every
    /// editor event so save/preview see interactive edits).
    pub(crate) fn sync_session_from(&mut self, session: &mut Session) {
        let name = session.glyph_name.clone();
        if session
            .pending
            .iter()
            .any(|operation| matches!(operation, HistoryOp::Record(_)))
        {
            self.metadata_redo.clear();
        }
        let mut master = self.font.master_mut();
        for op in session.pending.drain(..) {
            match op {
                HistoryOp::Record(glyph) => {
                    master.history.record(&name, &glyph);
                }
                HistoryOp::DiscardLast => {
                    master.history.discard_last(&name);
                }
            }
        }
        drop(master);
        self.session = Arc::new(session.clone());
        // Keep the panel's advance field in step after canvas edits
        // (sidebearing/advance drags). This path is never hit by typing in
        // the field, so it does not clobber input.
        self.refresh_metric_bufs();
        self.selected_points = self.session.selection.len();
    }

    /// Rebase the open transitional session after a canonical layer commit or replay.
    pub(crate) fn reload_canonical_layer(
        &mut self,
        address: &runebender::document::variable::GlyphLayerAddress,
    ) -> bool {
        if self.font.project.source_id(self.font.active()) != Some(address.layer.source)
            || self.session.glyph_name != address.glyph
        {
            return false;
        }
        let Some(glyph) = self
            .font
            .project
            .glyph_layer(&address.glyph, &address.layer)
        else {
            return false;
        };
        let mut session = (*self.session).clone();
        session.pending.clear();
        session.reload_glyph(self.font.font(), glyph);
        self.session = Arc::new(session);
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.session = self.session.clone();
        }
        if let Some(index) = self.font.index_of(&address.glyph) {
            self.font.refresh_entry(index);
        }
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        self.refresh_metric_bufs();
        self.refresh_coord_bufs();
        self.selected_points = self.session.selection.len();
        self.modified = true;
        true
    }

    /// Undo or redo the open glyph on the master's pile, then reload
    /// the session from the master.
    pub(crate) fn undo_open_glyph(&mut self, redo: bool) {
        let Mode::Editor(index) = self.mode else {
            return;
        };
        if self.metadata_history_step(redo) {
            return;
        }
        let mut master = self.font.master_mut();
        let done = if redo {
            master.redo(index)
        } else {
            master.undo(index)
        };
        if !done {
            self.note = if redo {
                "Nothing to redo"
            } else {
                "Nothing to undo"
            }
            .into();
            return;
        }
        drop(master);
        self.font.refresh_entry(index);
        let Some(glyph) = self
            .font
            .font()
            .get_glyph(&self.session.glyph_name)
            .cloned()
        else {
            return;
        };
        let mut session = (*self.session).clone();
        session.reload_glyph(self.font.font(), glyph);
        self.session = Arc::new(session);
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.session = self.session.clone();
        }
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        self.refresh_metric_bufs();
        self.refresh_coord_bufs();
        self.selected_points = self.session.selection.len();
        self.modified = true;
        self.note.clear();
    }

    /// Undo in the active editing context. Overview actions may change a
    /// multi-selection, so their per-glyph engine snapshots travel as one batch.
    pub(crate) fn undo_active_edit(&mut self, redo: bool) {
        if matches!(self.mode, Mode::Editor(_)) {
            self.undo_open_glyph(redo);
            return;
        }
        if !matches!(self.mode, Mode::Overview) {
            return;
        }
        if self.metadata_history_step(redo) {
            return;
        }
        let batch = if redo {
            self.overview_redo.pop()
        } else {
            self.overview_undo.pop()
        };
        let Some(batch) = batch else {
            self.note = if redo {
                "Nothing to redo"
            } else {
                "Nothing to undo"
            }
            .into();
            return;
        };
        let Some(mut master) = self.font.project.edit_source(batch.source) else {
            self.note = "Restore the removed source before undoing its glyph edits".into();
            if redo {
                self.overview_redo.push(batch);
            } else {
                self.overview_undo.push(batch);
            }
            return;
        };
        for glyph in &batch.glyphs {
            if let Some(&index) = master.name_map.get(glyph) {
                if redo {
                    master.redo(index);
                } else {
                    master.undo(index);
                }
            }
        }
        drop(master);
        if Some(batch.source) == self.font.project.source_id(self.font.active()) {
            self.font.rebuild_cache();
        }
        if redo {
            self.overview_undo.push(batch);
        } else {
            self.overview_redo.push(batch);
        }
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        self.modified = true;
    }

    pub(crate) fn set_master(&mut self, index: usize) {
        if index == self.font.active() || index >= self.font.master_count() {
            return;
        }
        if self.features_edited {
            self.note = "Apply or Revert feature edits before switching masters".into();
            return;
        }
        self.park();
        self.font.set_active(index);
        self.refresh_source_views();
    }

    pub(crate) fn refresh_source_views(&mut self) {
        let active_name =
            matches!(self.mode, Mode::Editor(_)).then(|| self.session.glyph_name.clone());
        let index = self.font.active();
        self.source_name_buf = self.font.master_names()[index].to_string();
        self.features_buf = self.font.feature_text().to_owned();
        self.features_status = None;
        if self.show_all_masters {
            self.reference_layers = (0..self.font.master_count())
                .filter(|master| *master != index)
                .collect();
        }
        self.axis_values = self.font.master_axis_values(index);
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        // A parked session contains one master's glyph data. Rebuild every tab
        // by glyph name so activating another tab cannot write the old master
        // into the new one. Viewports and text contexts remain tab-local.
        let font = &self.font;
        self.tabs.retain_mut(|tab| {
            let name = tab.session.glyph_name.clone();
            let Some(mut session) = Session::new_from_model(font, &name) else {
                return false;
            };
            session.viewport = tab.session.viewport.clone();
            session.fitted = tab.session.fitted;
            tab.session = Arc::new(session);
            true
        });
        if let Some(name) = active_name {
            let tab = self
                .tabs
                .iter()
                .position(|tab| tab.session.glyph_name == name);
            let glyph = self.font.index_of(&name);
            if let (Some(tab), Some(glyph)) = (tab, glyph) {
                self.active_tab = tab;
                self.session = self.tabs[tab].session.clone();
                self.selected = Some(glyph);
                self.mode = Mode::Editor(glyph);
                self.name_buf = name;
                self.unicode_buf = self
                    .session
                    .glyph
                    .codepoints
                    .iter()
                    .next()
                    .map(|codepoint| format!("{:04X}", codepoint as u32))
                    .unwrap_or_default();
                self.refresh_metric_bufs();
                self.refresh_coord_bufs();
                self.selected_points = 0;
            } else {
                self.settle_on_overview();
            }
        }
    }

    /// The current axis location as a name->value map (user units).
    pub(crate) fn axis_location(&self) -> HashMap<String, f64> {
        self.font
            .axes
            .iter()
            .zip(&self.axis_values)
            .map(|(a, v)| (a.name.clone(), *v))
            .collect()
    }

    /// True when the sliders sit exactly on the active master's location.
    pub(crate) fn on_active_master(&self) -> bool {
        match self.font.project.master_locations.get(self.font.active()) {
            Some(m) => self.font.axes.iter().enumerate().all(|(i, a)| {
                // Axis values are user coordinates; the engine stores normalized locations.
                let cur =
                    a.user_to_normalized(self.axis_values.get(i).copied().unwrap_or(a.default));
                let mst = m.get(&a.name).copied().unwrap_or(0.0);
                (cur - mst).abs() < 1e-6
            }),
            None => true,
        }
    }

    /// The interpolated instance outline at the current axis location, shown as
    /// a read-only overlay. `None` on a master (the editable outline is enough)
    /// or when the glyph is not interpolatable.
    pub(crate) fn interp_preview(&self) -> Option<Arc<BezPath>> {
        if self.on_active_master() {
            return None;
        }
        self.font
            .interpolate_outline(&self.session.glyph_name, &self.axis_location())
            .map(Arc::new)
    }

    /// Status shown under the axis sliders when the location is between masters.
    pub(crate) fn interpolation_status(&self) -> Option<String> {
        if !self.on_active_master()
            && let Some(detail) = self.font.project.compat_detail(&self.session.glyph_name)
        {
            return Some(format!("Cannot interpolate: {detail}"));
        }
        if !self.font.project.axes.is_empty() {
            match self.font.preview_font() {
                Err(error) => return Some(format!("Variable preview unavailable: {error}")),
                Ok(None) => return Some("Compiling variable preview…".into()),
                Ok(Some(_)) => (),
            }
        }
        (!self.on_active_master()).then(|| "interpolated".into())
    }

    pub(crate) fn set_axis(&mut self, index: usize, value: f64) {
        if let Some(v) = self.axis_values.get_mut(index) {
            *v = value;
        }
        self.font.project.location = self
            .font
            .axes
            .iter()
            .zip(&self.axis_values)
            .map(|(axis, value)| (axis.name.clone(), axis.user_to_normalized(*value)))
            .collect();
    }

    pub(crate) fn refresh_open_glyph(&mut self) {
        if let Mode::Editor(index) = self.mode {
            // Inspector fields edit a cloned `Session` directly rather than
            // travelling through the canvas rebuild hook. Move their pending
            // history into the engine before replacing the live glyph, just as
            // `sync_session_from` does for pointer and keyboard edits.
            let mut session = (*self.session).clone();
            let name = session.glyph_name.clone();
            if session
                .pending
                .iter()
                .any(|operation| matches!(operation, HistoryOp::Record(_)))
            {
                self.metadata_redo.clear();
            }
            let mut master = self.font.master_mut();
            for op in session.pending.drain(..) {
                match op {
                    HistoryOp::Record(glyph) => {
                        master.history.record(&name, &glyph);
                    }
                    HistoryOp::DiscardLast => {
                        master.history.discard_last(&name);
                    }
                }
            }
            drop(master);
            let glyph = session.glyph.clone();
            self.session = Arc::new(session);
            self.font.replace_glyph(index, glyph);
            if let Some(aligned) = self.font.font().get_glyph(&name).cloned() {
                let mut session = (*self.session).clone();
                session.reload_glyph(self.font.font(), aligned);
                self.session = Arc::new(session);
            }
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            self.modified = true;
            self.note.clear();
        }
    }

    pub(crate) fn back_to_overview(&mut self) {
        self.end_space_pan();
        self.mode = Mode::Overview;
    }
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
    fn edits_land_on_the_masters_pile_and_undo_from_it() {
        use runebender::document::project::Master;
        let mut session = two_squares();
        let mut master = Master::from_font(
            {
                let mut font = norad::Font::new();
                font.default_layer_mut().insert_glyph(session.glyph.clone());
                font
            },
            std::path::PathBuf::new(),
        );
        let index = master.name_map["test"];
        // Two point edits, two steps waiting for the app to pull them.
        let x = |s: &Session| s.glyph.contours[0].points[0].x;
        session.record(EditType::Normal);
        session.glyph.contours[0].points[0].x = 50.0;
        session.record(EditType::Normal);
        session.glyph.contours[0].points[0].x = 75.0;
        assert_eq!(session.pending.len(), 2);
        // What the app does on sync: drain onto the master's pile and
        // write the session's glyph through.
        for op in session.pending.drain(..) {
            if let HistoryOp::Record(glyph) = op {
                master.history.record("test", &glyph);
            }
        }
        let edited = session.glyph.clone();
        master.edit_glyph(index, |g| *g = edited);
        assert_eq!(master.undo_depth(index), 2);
        assert!(master.undo(index));
        let back = master.font.get_glyph("test").expect("still there").clone();
        session.reload_glyph(&master.font, back);
        assert_eq!(x(&session), 50.0);
        assert!(master.undo(index));
        session.reload_glyph(
            &master.font,
            master.font.get_glyph("test").expect("still there").clone(),
        );
        assert_eq!(x(&session), 0.0);
        assert!(master.redo(index));
        session.reload_glyph(
            &master.font,
            master.font.get_glyph("test").expect("still there").clone(),
        );
        assert_eq!(x(&session), 50.0);
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
    fn component_selection_editing_and_cache_round_trip() {
        let mut font = norad::Font::new();
        let mut base = norad::Glyph::new("base");
        base.contours.push(norad::Contour::new(
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
                .into_iter()
                .map(|(x, y)| {
                    norad::ContourPoint::new(x, y, norad::PointType::Line, false, None, None)
                })
                .collect(),
            None,
        ));
        font.default_layer_mut().insert_glyph(base);
        let mut composite = norad::Glyph::new("composite");
        composite.components.push(norad::Component::new(
            norad::Name::new("base").unwrap(),
            norad::AffineTransform {
                x_offset: 30.0,
                y_offset: 40.0,
                ..Default::default()
            },
            None,
        ));
        font.default_layer_mut().insert_glyph(composite);

        let mut session = Session::new(&font, "composite").unwrap();
        assert_eq!(session.component_at(Point::new(50.0, 60.0)), Some(0));
        assert!(session.select_component(0));
        assert_eq!(session.selected_component_aligned(), Some(true));
        assert!(!session.drag_component_by(10.0, 0.0));
        assert!(session.pending.is_empty());
        assert!(session.toggle_component_alignment(&font));
        assert_eq!(session.selected_component_aligned(), Some(false));
        session.pending.clear();
        assert!(session.drag_component_by(10.0, 0.0));
        assert!(session.drag_component_by(5.0, 5.0));
        assert_eq!(
            session.pending.len(),
            1,
            "one pointer gesture, one undo step"
        );
        session.end_component_drag();
        assert!(!session.gesture_in_progress());
        assert_eq!(session.glyph.components[0].transform.x_offset, 45.0);
        assert_eq!(session.glyph.components[0].transform.y_offset, 45.0);
        assert!(session.component_at(Point::new(50.0, 50.0)).is_some());

        assert!(session.duplicate());
        assert_eq!(session.glyph.components.len(), 2);
        assert_eq!(session.selected_component, Some(1));
        assert!(session.delete_selected());
        assert_eq!(session.glyph.components.len(), 1);
        assert_eq!(session.selected_component, None);

        let before_drag = match &session.pending[0] {
            HistoryOp::Record(glyph) => (**glyph).clone(),
            HistoryOp::DiscardLast => panic!("drag must record its original glyph"),
        };
        session.reload_glyph(&font, before_drag);
        assert_eq!(session.glyph.components[0].transform.x_offset, 30.0);
        assert_eq!(session.component_at(Point::new(50.0, 60.0)), Some(0));
        assert!(session.select_component(0));
        assert!(session.decompose());
        assert_eq!(session.glyph.contours.len(), 1);
        assert!(session.glyph.components.is_empty());
    }

    #[test]
    fn adding_a_component_validates_before_recording() {
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("base"));
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("target"));
        let mut session = Session::new(&font, "target").unwrap();

        assert!(!session.add_component(&font, "missing"));
        assert!(!session.add_component(&font, "target"));
        assert!(session.pending.is_empty());
        assert!(session.add_component(&font, "base"));
        assert_eq!(session.pending.len(), 1);
        assert_eq!(session.selected_component, Some(0));
    }

    #[test]
    fn locking_a_component_snaps_it_to_matching_anchors() {
        let mut font = norad::Font::new();
        let mut carrier = norad::Glyph::new("carrier");
        carrier.anchors.push(norad::Anchor::new(
            300.0,
            500.0,
            norad::Name::new("top").ok(),
            None,
            None,
        ));
        font.default_layer_mut().insert_glyph(carrier);
        let mut mark = norad::Glyph::new("mark");
        mark.anchors.push(norad::Anchor::new(
            20.0,
            30.0,
            norad::Name::new("_top").ok(),
            None,
            None,
        ));
        font.default_layer_mut().insert_glyph(mark);
        let mut composite = norad::Glyph::new("composite");
        composite.components.push(norad::Component::new(
            norad::Name::new("carrier").unwrap(),
            norad::AffineTransform::default(),
            None,
        ));
        let mut loose = norad::Component::new(
            norad::Name::new("mark").unwrap(),
            norad::AffineTransform::default(),
            None,
        );
        runebender::document::composites::set_component_alignment_disabled(&mut loose, true);
        composite.components.push(loose);
        font.default_layer_mut().insert_glyph(composite);

        let mut session = Session::new(&font, "composite").unwrap();
        assert!(session.select_component(1));
        assert_eq!(session.selected_component_aligned(), Some(false));
        assert!(session.toggle_component_alignment(&font));
        assert_eq!(session.selected_component_aligned(), Some(true));
        assert_eq!(session.glyph.components[1].transform.x_offset, 280.0);
        assert_eq!(session.glyph.components[1].transform.y_offset, 470.0);
        assert_eq!(session.pending.len(), 1);
    }

    #[test]
    fn pasting_nothing_changes_nothing() {
        let mut session = two_squares();
        assert!(!session.paste_contours(&[]));
        assert_eq!(session.glyph.contours.len(), 2);
    }

    #[test]
    fn parameterized_filters_record_only_real_edits() {
        let mut unchanged = two_squares();
        assert!(!unchanged.offset(0.0));
        assert!(unchanged.pending.is_empty());

        let mut offset = two_squares();
        assert!(offset.offset(10.0));
        assert_eq!(offset.pending.len(), 1);

        let mut extrude = two_squares();
        assert!(extrude.extrude(20.0, 30.0, false));
        assert_eq!(extrude.pending.len(), 1);

        let mut rough = two_squares();
        assert!(rough.roughen(10.0, 4.0, 4.0, 7));
        assert_eq!(rough.pending.len(), 1);
    }

    #[test]
    fn stroke_expansion_targets_selected_contours_and_records_the_original() {
        let mut session = two_squares();
        let original = session.glyph.contours.clone();
        for width in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(!session.expand_stroke(width));
        }
        assert!(session.pending.is_empty());
        session.selection.insert((0, 0));
        assert!(session.expand_stroke(20.0));
        assert_eq!(session.glyph.contours.last(), original.last());
        assert_ne!(session.glyph.contours, original);
        assert!(session.selection.is_empty());
        assert_eq!(session.pending.len(), 1);
        let HistoryOp::Record(before) = &session.pending[0] else {
            panic!("undo snapshot");
        };
        assert_eq!(before.contours, original);
        let mut all = two_squares();
        assert!(all.expand_stroke(20.0));
        assert!(all.glyph.contours.len() > original.len());
    }

    #[test]
    fn cleanup_command_records_an_edit_and_changes_the_glyph() {
        let mut session = two_squares();
        session.glyph.contours[0].points[0].x = 0.4;

        assert!(session.round_coordinates());
        assert_eq!(session.glyph.contours[0].points[0].x, 0.0);
        assert!(matches!(session.pending.last(), Some(HistoryOp::Record(_))));
    }

    #[test]
    fn clockwise_and_counterclockwise_rotations_are_opposites_on_the_selection() {
        let mut clockwise = two_squares();
        let original = clockwise.glyph.contours.clone();
        clockwise.selection.extend((0..4).map(|point| (0, point)));
        let mut counterclockwise = clockwise.clone();
        assert!(clockwise.rotate_90_clockwise());
        assert!(counterclockwise.rotate_90());
        let cw = &clockwise.glyph.contours[0].points[0];
        let ccw = &counterclockwise.glyph.contours[0].points[0];
        assert_eq!((cw.x, cw.y), (0.0, 100.0));
        assert_eq!((ccw.x, ccw.y), (100.0, 0.0));
        assert_eq!(clockwise.glyph.contours[1], original[1]);
        assert_eq!(counterclockwise.glyph.contours[1], original[1]);
        assert_eq!(clockwise.pending.len(), 1);
        assert!(clockwise.rotate_90());
        assert_eq!(clockwise.glyph.contours, original);
        assert_eq!(clockwise.pending.len(), 2);
    }

    #[test]
    fn duplicate_repeat_reapplies_the_last_transform_to_the_clone() {
        let mut session = two_squares();
        session.selection.extend((0..4).map(|point| (0, point)));
        assert!(session.rotate_90());
        assert!(session.duplicate_repeat());
        assert_eq!(session.glyph.contours.len(), 3);
        assert!(session.selection.iter().all(|(contour, _)| *contour == 2));
    }

    #[test]
    fn anchor_and_metric_drags_record_one_closed_transaction() {
        let mut session = two_squares();
        session.glyph.width = 500.0;
        session.glyph.anchors.push(norad::Anchor::new(
            100.0,
            200.0,
            Some(norad::Name::new("top").expect("anchor name")),
            None,
            None,
        ));

        session.move_anchor(0, 120.0, 220.0);
        session.move_anchor(0, 140.0, 240.0);
        assert_eq!(session.pending.len(), 1);
        assert!(session.gesture_in_progress());
        let HistoryOp::Record(before) = &session.pending[0] else {
            panic!("anchor drag records its starting glyph");
        };
        assert_eq!((before.anchors[0].x, before.anchors[0].y), (100.0, 200.0));
        session.end_metric_drag();
        assert!(!session.gesture_in_progress());

        session.pending.clear();
        session.drag_advance(520.0);
        session.drag_advance(540.0);
        assert_eq!(session.pending.len(), 1);
        session.end_metric_drag();
        assert!(!session.gesture_in_progress());
        let HistoryOp::Record(before) = &session.pending[0] else {
            panic!("advance drag records its starting glyph");
        };
        assert_eq!(before.width, 500.0);
        assert_eq!(session.advance(), 540.0);

        session.pending.clear();
        session.move_anchor(0, f64::NAN, 10.0);
        session.drag_advance(f64::INFINITY);
        assert!(session.pending.is_empty());
    }

    #[test]
    fn decompose_undo_rebuilds_nested_transformed_component_preview() {
        use masonry::kurbo::Shape as _;
        use runebender::document::project::Master;

        let mut font = norad::Font::new();
        let mut base = norad::Glyph::new("base");
        let mut contour = norad::Contour::default();
        for (x, y) in [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)] {
            contour.points.push(norad::ContourPoint::new(
                x,
                y,
                norad::PointType::Line,
                false,
                None,
                None,
            ));
        }
        base.contours.push(contour);
        font.default_layer_mut().insert_glyph(base);
        let mut nested = norad::Glyph::new("nested");
        nested.components.push(norad::Component::new(
            norad::Name::new("base").expect("base name"),
            norad::AffineTransform {
                x_scale: 2.0,
                y_scale: 2.0,
                x_offset: 40.0,
                y_offset: 60.0,
                ..norad::AffineTransform::default()
            },
            None,
        ));
        font.default_layer_mut().insert_glyph(nested);
        let mut composite = norad::Glyph::new("composite");
        composite.components.push(norad::Component::new(
            norad::Name::new("nested").expect("nested name"),
            norad::AffineTransform {
                x_offset: 20.0,
                y_offset: 30.0,
                ..norad::AffineTransform::default()
            },
            None,
        ));
        font.default_layer_mut().insert_glyph(composite);
        let mut master = Master::from_font(font, std::path::PathBuf::new());
        let index = master.name_map["composite"];
        let mut session = Session::new(&master.font, "composite").expect("composite exists");
        let before_bounds = session.components.bounding_box();

        assert!(session.decompose());
        for op in session.pending.drain(..) {
            if let HistoryOp::Record(glyph) = op {
                master.history.record("composite", &glyph);
            }
        }
        master.edit_glyph(index, |glyph| *glyph = session.glyph.clone());
        assert!(session.components.elements().is_empty());
        assert!(master.undo(index));
        session.reload_glyph(
            &master.font,
            master
                .font
                .get_glyph("composite")
                .expect("component restored")
                .clone(),
        );
        assert_eq!(session.glyph.components.len(), 1);
        assert_eq!(session.components.bounding_box(), before_bounds);
    }
}
