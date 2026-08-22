// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An edit session: one glyph, its selection, viewport, and undo stack.
//!
//! The session works on a `norad::Glyph` directly so every operation in
//! `runebender_core::glyph_ops` and `point_ops` applies without conversion.
//! The editor island owns the session; the app receives copies of the glyph.

use std::collections::{HashMap, HashSet};

use masonry::kurbo::{BezPath, Point, Rect, Shape};
use runebender_core::editing::edit_types::EditType;
use runebender_core::editing::undo::UndoState;
use runebender_core::editing::viewport::ViewPort;
use runebender_core::glyph_ops::{self, GlyphSnapshot, PointId};
use runebender_core::glyph_paths;
use runebender_core::point_ops;

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
    pub metrics: Metrics,
    pub selection: HashSet<PointId>,
    pub viewport: ViewPort,
    pub fitted: bool,
    undo: UndoState<GlyphSnapshot>,
    drag_originals: HashMap<PointId, (f64, f64)>,
    in_drag: bool,
    /// The contour the pen is currently extending, if any.
    pub active_contour: Option<usize>,
}

impl Session {
    pub fn new(font: &norad::Font, name: &str) -> Option<Self> {
        let glyph = font.get_glyph(name)?.clone();
        let components = glyph_paths::components_to_bezpath(&glyph, font);
        Some(Self {
            glyph_name: name.to_string(),
            glyph,
            components,
            metrics: Metrics::of(font),
            selection: HashSet::new(),
            viewport: ViewPort::new(),
            fitted: false,
            undo: UndoState::new(),
            drag_originals: HashMap::new(),
            in_drag: false,
            active_contour: None,
        })
    }

    pub fn advance(&self) -> f64 {
        self.glyph.width
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

    /// The first point of the active pen contour, in design space.
    pub fn pen_first_point(&self) -> Option<Point> {
        let c = self.active_contour?;
        let p = self.glyph.contours.get(c)?.points.first()?;
        Some(Point::new(p.x, p.y))
    }

    /// The last point of the active pen contour, in design space.
    pub fn pen_last_point(&self) -> Option<Point> {
        let c = self.active_contour?;
        let p = self.glyph.contours.get(c)?.points.last()?;
        Some(Point::new(p.x, p.y))
    }

    /// Add a corner point at (x, y), starting a contour if the pen is idle.
    pub fn pen_line_to(&mut self, x: f64, y: f64) {
        self.record(EditType::Normal);
        match self.active_contour {
            Some(c) => glyph_ops::append_segment(&mut self.glyph, c, None, x, y, false),
            None => {
                let c = glyph_ops::start_contour(&mut self.glyph, x, y);
                self.active_contour = Some(c);
            }
        }
    }

    /// Close the active contour.
    pub fn pen_close(&mut self) {
        if let Some(c) = self.active_contour.take() {
            self.record(EditType::Normal);
            glyph_ops::close_contour(&mut self.glyph, c, None);
        }
    }

    /// End the current pen path without closing (Escape / tool switch).
    pub fn pen_cancel(&mut self) {
        self.active_contour = None;
    }

    pub fn select_all(&mut self) {
        self.selection = self.points().into_iter().map(|p| p.id).collect();
    }
}
