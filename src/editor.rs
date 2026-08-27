// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The glyph editor island: a canvas widget that owns the edit session
//! and gesture state, and the view that hosts it.

use std::sync::Arc;

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayerType, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, WidgetId,
    PointerButtonEvent, PointerEvent, PointerScrollEvent, PointerUpdate, PropertiesMut,
    PropertiesRef, RegisterCtx, ScrollDelta, TextEvent, Widget,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Circle, Line, Point, Rect, Size, Stroke};
use masonry::layout::{LenReq, Length};
use runebender_core::glyph_ops::PointId;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use crate::App;
use crate::context_menu::{ContextMenu, MenuAction, MenuRow};
use crate::session::Session;
use crate::text_label::{self, Anchor};
use crate::theme::Palette;
use crate::Tool;

const HIT_RADIUS_PX: f64 = 8.0;

/// Context-menu items: (label, op). Op returns whether the glyph changed.
/// The right-click menu's rows. Shared with the layer that draws them.
const MENU_ITEMS: &[MenuRow] = &[
    MenuRow { label: "Add Anchor", action: MenuAction::AddAnchor },
    MenuRow { label: "Set Start Point", action: MenuAction::Op(|s| s.set_start()) },
    MenuRow { label: "Round Corners", action: MenuAction::Op(|s| s.round_corners()) },
    MenuRow { label: "Reverse Contours", action: MenuAction::Op(|s| s.reverse()) },
    MenuRow { label: "Remove Overlap", action: MenuAction::Op(|s| s.remove_overlap()) },
    MenuRow { label: "Flip Horizontal", action: MenuAction::Op(|s| s.flip_horizontal()) },
    MenuRow { label: "Flip Vertical", action: MenuAction::Op(|s| s.flip_vertical()) },
    MenuRow { label: "Rotate 90", action: MenuAction::Op(|s| s.rotate_90()) },
    MenuRow { label: "Duplicate", action: MenuAction::Op(|s| s.duplicate()) },
    MenuRow { label: "Harmonize", action: MenuAction::Op(|s| s.harmonize()) },
    MenuRow { label: "Balance", action: MenuAction::Op(|s| s.balance()) },
    MenuRow { label: "Optimize", action: MenuAction::Op(|s| s.optimize()) },
    MenuRow { label: "Decompose", action: MenuAction::Op(|s| s.decompose()) },
];

impl EditorWidget {
    /// Apply a context-menu choice. Called from the menu layer, which is
    /// not a child of this widget and so cannot reach it with an action.
    pub(crate) fn apply_menu_choice(
        this: &mut masonry::core::WidgetMut<'_, Self>,
        action: MenuAction,
        at: Point,
    ) {
        let changed = match action {
            MenuAction::AddAnchor => {
                this.widget.session.add_anchor(at.x.round(), at.y.round());
                true
            }
            MenuAction::Op(op) => op(&mut this.widget.session),
        };
        this.widget.menu = None;
        if changed {
            this.ctx.submit_action::<EditorEvent>(EditorEvent::Edited);
        }
        this.ctx.request_render();
    }

    /// The menu closed without a choice.
    pub(crate) fn forget_menu(this: &mut masonry::core::WidgetMut<'_, Self>) {
        this.widget.menu = None;
    }
}

/// What the editor reports upward.
#[derive(Debug)]
pub enum EditorEvent {
    /// The glyph changed; the app should refresh its cached preview.
    Edited,
    /// The user pressed the save shortcut while the editor had focus.
    Save,
    /// Selection changed; carries how many points are selected.
    Selection(usize),
    /// The user asked to leave the editor (Escape).
    Exit,
}

enum Drag {
    None,
    Points { start: Point },
    Pan { last: Point },
    /// Pen mouse-down at `origin` (design space); becomes handle-drag past a threshold.
    Pen { origin: Point, dragging: bool },
    /// Rubber-band selection in screen space.
    Marquee { start: Point, current: Point, additive: bool },
    /// Drawing a shape; endpoints in design space.
    Shape { start: Point, current: Point },
    /// Dragging an anchor by index.
    Anchor { idx: usize },
    /// Dragging the advance (right sidebearing) line.
    AdvanceLine,
    /// Dragging the left sidebearing line; carries the last cursor x (screen).
    LeftLine { last_x: f64 },
}

pub struct EditorWidget {
    session: Session,
    palette: Arc<Palette>,
    tool: Tool,
    ghosts: Arc<Vec<masonry::kurbo::BezPath>>,
    /// Read-only interpolated instance overlay at the current axis location.
    interp: Option<Arc<masonry::kurbo::BezPath>>,
    /// Background layer and reference glyph, drawn under everything.
    underlay: Underlay,
    size: Size,
    drag: Drag,
    /// Last cursor position in design space, for the pen preview segment.
    hover: Option<Point>,
    /// The open context-menu layer, if there is one.
    menu: Option<WidgetId>,
    view: ViewOptions,
}

impl EditorWidget {
    fn screen_points(&self) -> Vec<(PointId, Point, bool, bool, bool)> {
        let affine = self.session.viewport.affine();
        self.session
            .points()
            .into_iter()
            .map(|p| (p.id, affine * p.point, p.on_curve, p.smooth, p.start))
            .collect()
    }

    fn hit_point(&self, at: Point) -> Option<PointId> {
        self.screen_points()
            .into_iter()
            .filter(|(_, sp, _, _, _)| sp.distance(at) <= HIT_RADIUS_PX)
            .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
            .map(|(id, _, _, _, _)| id)
    }

    fn fit(&mut self) {
        let m = self.session.metrics;
        self.session.viewport.fit_to_canvas(
            self.size.width,
            self.size.height,
            self.session.advance(),
            m.ascender,
            m.descender,
            0.62,
        );
        self.session.fitted = true;
    }

    fn emit(&self, ctx: &mut EventCtx<'_>, edited: bool) {
        if edited {
            ctx.submit_action::<EditorEvent>(EditorEvent::Edited);
        }
        ctx.submit_action::<EditorEvent>(EditorEvent::Selection(self.session.selection.len()));
        ctx.request_render();
    }
}

impl Widget for EditorWidget {
    type Action = EditorEvent;

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_text_input(&self) -> bool {
        true
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        match len_req {
            LenReq::FitContent(space) => space,
            _ => Length::px(200.0),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        if self.size != size {
            self.size = size;
            if !self.session.fitted {
                self.fit();
            }
        }
        ctx.set_clip_path(size.to_rect());
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let pal = self.palette.clone();
        let affine = self.session.viewport.affine();
        painter.fill_rect(self.size.to_rect(), pal.canvas);

        // Underlay, drawn first so everything else sits on top of it. The
        // reference glyph is a quiet fill (it is a shape to match), the
        // background layer is a quiet outline (it is a trace to follow).
        if let Some(reference) = &self.underlay.reference {
            painter
                .fill(&(affine * (**reference).clone()), pal.text_muted.with_alpha(0.18))
                .draw();
        }
        if let Some(background) = &self.underlay.background {
            painter
                .stroke(
                    &(affine * (**background).clone()),
                    &Stroke::new(1.0),
                    pal.text_muted.with_alpha(0.5),
                )
                .draw();
        }

        // Interpolation ghosts: the other masters' outlines, faint.
        for ghost in self.ghosts.iter() {
            painter
                .stroke(&(affine * ghost.clone()), &Stroke::new(1.0), pal.role("reference").with_alpha(0.55))
                .draw();
        }

        let m = &self.session.metrics;
        let thin = Stroke::new(1.0);
        let quiet = pal.role("metricQuiet");
        let x0 = (affine * Point::new(-10_000.0, 0.0)).x;
        let x1 = (affine * Point::new(10_000.0, 0.0)).x;
        for y in [0.0, m.x_height, m.cap_height, m.ascender, m.descender] {
            let sy = (affine * Point::new(0.0, y)).y;
            painter.stroke(Line::new((x0, sy), (x1, sy)), &thin, quiet).draw();
        }
        // Em box: the advance width by the ascender..descender height, framing
        // the glyph like gpui/Glyphs rather than infinite sidebearing lines.
        let em_box = Rect::from_points(
            affine * Point::new(0.0, m.descender),
            affine * Point::new(self.session.advance(), m.ascender),
        );
        painter.stroke(em_box, &Stroke::new(1.0), quiet).draw();

        // Editing affordances only render on a master. Off a master the view
        // shows the read-only interpolated instance instead (web/Glyphs
        // behavior): swap the outline, don't ghost it behind an editable one.
        if self.interp.is_none() {
        if !self.session.components.elements().is_empty() {
            painter
                .fill(&(affine * self.session.components.clone()), pal.role("component").with_alpha(0.5))
                .draw();
        }
        let outline = affine * self.session.outline();
        painter.fill(&outline, pal.role("pathStroke").with_alpha(0.08)).draw();
        painter.stroke(&outline, &Stroke::new(1.0), pal.role("pathStroke")).draw();

        let handle = Stroke::new(1.0);
        for contour in &self.session.glyph.contours {
            let n = contour.points.len();
            for i in 0..n {
                if !matches!(contour.points[i].typ, norad::PointType::OffCurve) {
                    continue;
                }
                let off = affine * Point::new(contour.points[i].x, contour.points[i].y);
                for j in [(i + n - 1) % n, (i + 1) % n] {
                    if !matches!(contour.points[j].typ, norad::PointType::OffCurve) {
                        let on = affine * Point::new(contour.points[j].x, contour.points[j].y);
                        painter
                            .stroke(Line::new(off, on), &handle, pal.role("pointOffcurve").with_alpha(0.7))
                            .draw();
                    }
                }
            }
        }

        for (id, sp, on_curve, smooth, start) in self.screen_points() {
            let selected = self.session.selection.contains(&id);
            let fill = if selected {
                pal.role("pointSelected")
            } else if start {
                pal.role("startNode")
            } else if !on_curve {
                pal.role("pointOffcurve")
            } else if smooth {
                pal.role("pointSmooth")
            } else {
                pal.role("pointCorner")
            };
            if on_curve && !smooth {
                let r = 4.5;
                painter.fill(Rect::new(sp.x - r, sp.y - r, sp.x + r, sp.y + r), fill).draw();
            } else {
                let r = if on_curve { 4.5 } else { 3.0 };
                painter.fill(Circle::new(sp, r), fill).draw();
            }
        }

        // Anchors: a small diamond at each, in the accent color.
        let anchor_color = pal.role("accent");
        for (ai, anchor) in self.session.glyph.anchors.iter().enumerate() {
            let p = affine * Point::new(anchor.x, anchor.y);
            let selected = self.session.selected_anchor == Some(ai);
            let anchor_color = if selected { pal.role("pointSelected") } else { anchor_color };
            let r = if selected { 6.5 } else { 5.0 };
            let diamond = masonry::kurbo::BezPath::from_vec(vec![
                masonry::kurbo::PathEl::MoveTo(Point::new(p.x, p.y - r)),
                masonry::kurbo::PathEl::LineTo(Point::new(p.x + r, p.y)),
                masonry::kurbo::PathEl::LineTo(Point::new(p.x, p.y + r)),
                masonry::kurbo::PathEl::LineTo(Point::new(p.x - r, p.y)),
                masonry::kurbo::PathEl::ClosePath,
            ]);
            painter.stroke(&diamond, &Stroke::new(1.5), anchor_color).draw();
            painter.fill(Circle::new(p, 1.5), anchor_color).draw();
        }
        } else if let Some(interp) = &self.interp {
            // Read-only interpolated instance in warm amber, filled and
            // stroked, standing in for the editable outline.
            let path = affine * (**interp).clone();
            painter.fill(&path, pal.role("warning").with_alpha(0.14)).draw();
            painter
                .stroke(&path, &Stroke::new(1.75), pal.role("warning").with_alpha(0.95))
                .draw();
        }

        // Curvature comb: a strip pushed out along the normal of every
        // curved segment, so the shape of the curvature is visible.
        if self.view.comb && self.interp.is_none() {
            let comb = pal.role("accent").with_alpha(0.7);
            for strip in self.session.curvature_comb() {
                let mut previous: Option<Point> = None;
                for (on, outer) in strip {
                    let on = affine * on;
                    let outer = affine * outer;
                    painter
                        .stroke(Line::new(on, outer), &Stroke::new(1.0), comb.with_alpha(0.35))
                        .draw();
                    if let Some(previous) = previous {
                        painter
                            .stroke(Line::new(previous, outer), &Stroke::new(1.0), comb)
                            .draw();
                    }
                    previous = Some(outer);
                }
            }
        }

        // Continuity: one dot per on-curve node, colored by how smooth
        // the join actually is. A kink is a node marked smooth whose
        // tangents do not line up, which is the defect worth seeing.
        if self.view.continuity && self.interp.is_none() {
            use runebender_core::curve::GLevel;
            for node in self.session.continuity() {
                let color = match node.level {
                    GLevel::Kink => pal.role("error"),
                    GLevel::Corner => pal.role("metricQuiet"),
                    GLevel::G1 => pal.role("warning"),
                    GLevel::G1Line => pal.role("pointOffcurve"),
                    GLevel::G2 | GLevel::G3 => pal.role("success"),
                };
                let at = affine * node.at;
                painter.fill(Circle::new(at, 4.5), color).draw();
            }
        }

        // Colorize: tint the outline and its handles by segment length,
        // the web editor's mode for spotting odd measurements.
        if self.view.colorize && self.interp.is_none() {
            for stroke in self.session.colored_strokes() {
                let color = pal.popcount(stroke.popcount);
                let width = if stroke.wide { 2.0 } else { 1.0 };
                painter
                    .stroke(&(affine * stroke.path), &Stroke::new(width), color)
                    .draw();
            }
        }

        // Measure overlay: segment and handle lengths, and side bearings.
        if self.view.measures() && self.interp.is_none() {
            let zoom = self.session.viewport.zoom;
            for m in self.session.measurements() {
                use runebender_core::measure::MeasureKind;
                let wanted = match m.kind {
                    MeasureKind::Handle => self.view.handles,
                    MeasureKind::Segment => self.view.segments,
                    _ => self.view.segments,
                };
                if !wanted {
                    continue;
                }
                let a = affine * m.a;
                let b = affine * m.b;
                let color = match m.kind {
                    MeasureKind::Handle => pal.role("pointOffcurve"),
                    MeasureKind::Segment => pal.role("accent"),
                    _ => pal.role("selection"),
                };
                painter.stroke(Line::new(a, b), &Stroke::new(1.0), color).draw();
                let mid = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
                let text = self.view.label(m.length);
                text_label::draw(painter, mid, &text, 11.0, color, Anchor::Middle);
            }
            if let Some(sb) = self.session.side_bearings().filter(|_| self.view.bearings) {
                let quiet = pal.role("metricQuiet");
                let y = (affine * Point::new(0.0, sb.y_left.min(sb.y_right) - 40.0 / zoom.max(0.001))).y;
                let l = affine * Point::new(0.0, 0.0);
                let ink_l = affine * Point::new(sb.min_x, 0.0);
                let ink_r = affine * Point::new(sb.max_x, 0.0);
                let adv = affine * Point::new(sb.advance, 0.0);
                painter.stroke(Line::new((l.x, y), (ink_l.x, y)), &Stroke::new(1.0), quiet).draw();
                painter.stroke(Line::new((ink_r.x, y), (adv.x, y)), &Stroke::new(1.0), quiet).draw();
                let (lsb, rsb) = (self.view.label(sb.lsb), self.view.label(sb.rsb));
                text_label::draw(painter, Point::new((l.x + ink_l.x) / 2.0, y - 8.0), &lsb, 11.0, quiet, Anchor::Middle);
                text_label::draw(painter, Point::new((ink_r.x + adv.x) / 2.0, y - 8.0), &rsb, 11.0, quiet, Anchor::Middle);
            }
        }

        // Marquee rectangle.
        if let Drag::Marquee { start, current, .. } = &self.drag {
            let rect = Rect::from_points(*start, *current);
            painter.fill(rect, pal.role("selection").with_alpha(0.15)).draw();
            painter
                .stroke(rect, &Stroke::new(1.0), pal.role("selection").with_alpha(0.8))
                .draw();
        }

        // Shape preview.
        if let Drag::Shape { start, current } = &self.drag {
            let p0 = affine * *start;
            let p1 = affine * *current;
            let accent = pal.role("accent");
            match self.tool {
                Tool::Knife => {
                    let danger = pal.role("danger");
                    painter.stroke(Line::new(p0, p1), &Stroke::new(1.0), danger).draw();
                    for hit in self.session.knife_hits(*start, *current) {
                        let sp = affine * hit;
                        painter.fill(Circle::new(sp, 3.5), danger).draw();
                    }
                }
                Tool::Ellipse => {
                    let c = ((p0.x + p1.x) / 2.0, (p0.y + p1.y) / 2.0);
                    let rr = ((p1.x - p0.x).abs() / 2.0, (p1.y - p0.y).abs() / 2.0);
                    let e = masonry::kurbo::Ellipse::new(c, rr, 0.0);
                    painter.stroke(&e, &Stroke::new(1.0), accent).draw();
                }
                _ => {
                    painter
                        .stroke(Rect::from_points(p0, p1), &Stroke::new(1.0), accent)
                        .draw();
                }
            }
        }
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        // Off a master the view is a read-only instance: swallow edit clicks.
        if self.interp.is_some() {
            if let PointerEvent::Down(PointerButtonEvent { button: Some(PointerButton::Primary | PointerButton::Secondary), .. }) = event {
                ctx.set_handled();
                return;
            }
        }
        match event {
            PointerEvent::Down(PointerButtonEvent { button, state, .. }) => {
                ctx.request_focus();
                let at = ctx.local_position(state.position);
                if *button == Some(PointerButton::Secondary) {
                    // A layer, not a rectangle painted into this canvas:
                    // it is rooted in window space, so it can hang past
                    // the editor's edge like a menu should.
                    if self.menu.is_none() {
                        let design = self.session.viewport.screen_to_design(at);
                        let menu = ContextMenu::new(
                            ctx.widget_id(),
                            MENU_ITEMS,
                            self.palette.clone(),
                            design,
                        );
                        let menu = NewWidget::new(menu);
                        self.menu = Some(menu.id());
                        ctx.create_layer(LayerType::Other, menu, ctx.to_window(at));
                    }
                    ctx.set_handled();
                    return;
                }
                ctx.capture_pointer();
                match button {
                    Some(PointerButton::Primary) if self.tool == Tool::HyperPen => {
                        let affine = self.session.viewport.affine();
                        let near_first = self
                            .session
                            .first_contour_point()
                            .map(|p| (affine * p).distance(at) <= HIT_RADIUS_PX)
                            .unwrap_or(false);
                        let corner = state.modifiers.alt();
                        if near_first && self.session.hyper_is_active() {
                            self.session.hyper_close();
                        } else {
                            let d = self.session.viewport.screen_to_design(at);
                            self.session.hyper_add(d.x, d.y, corner);
                        }
                        self.emit(ctx, true);
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if matches!(self.tool, Tool::Rect | Tool::Ellipse | Tool::Knife) => {
                        ctx.request_focus();
                        ctx.capture_pointer();
                        let d = self.session.viewport.screen_to_design(at);
                        self.drag = Drag::Shape { start: d, current: d };
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Pen => {
                        let affine = self.session.viewport.affine();
                        let near_first = self
                            .session
                            .pen_first_point()
                            .map(|p| (affine * p).distance(at) <= HIT_RADIUS_PX)
                            .unwrap_or(false);
                        if near_first && self.session.pen_is_active() {
                            self.session.pen_close();
                            self.drag = Drag::None;
                            self.emit(ctx, true);
                        } else {
                            let origin = self.session.viewport.screen_to_design(at);
                            self.drag = Drag::Pen { origin, dragging: false };
                        }
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) => {
                        let shift = state.modifiers.shift();
                        // Sidebearing lines (only when not near a point).
                        let affine = self.session.viewport.affine();
                        let adv_x = (affine * Point::new(self.session.advance(), 0.0)).x;
                        let lsb_x = (affine * Point::new(0.0, 0.0)).x;
                        if self.hit_point(at).is_none() {
                            if (at.x - adv_x).abs() <= 4.0 {
                                self.drag = Drag::AdvanceLine;
                                ctx.set_handled();
                                return;
                            }
                            if (at.x - lsb_x).abs() <= 4.0 {
                                self.drag = Drag::LeftLine { last_x: at.x };
                                ctx.set_handled();
                                return;
                            }
                        }
                        // Anchor hit takes priority over points.
                        if let Some(ai) = self.session.anchor_at(self.session.viewport.screen_to_design(at), HIT_RADIUS_PX / self.session.viewport.zoom) {
                            self.session.selected_anchor = Some(ai);
                            self.session.selection.clear();
                            self.drag = Drag::Anchor { idx: ai };
                            self.emit(ctx, false);
                            ctx.set_handled();
                            return;
                        }
                        self.session.selected_anchor = None;
                        match self.hit_point(at) {
                            Some(id) => {
                                if shift {
                                    if !self.session.selection.remove(&id) {
                                        self.session.selection.insert(id);
                                    }
                                } else if !self.session.selection.contains(&id) {
                                    self.session.selection.clear();
                                    self.session.selection.insert(id);
                                }
                                self.session.begin_point_drag();
                                self.drag = Drag::Points { start: at };
                                self.emit(ctx, false);
                            }
                            None => {
                                if !shift {
                                    self.session.selection.clear();
                                    self.emit(ctx, false);
                                }
                                self.drag = Drag::Marquee { start: at, current: at, additive: shift };
                            }
                        }
                    }
                    _ => self.drag = Drag::Pan { last: at },
                }
                ctx.set_handled();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let at = ctx.local_position(current.position);
                if matches!(self.tool, Tool::Pen | Tool::HyperPen) {
                    self.hover = Some(self.session.viewport.screen_to_design(at));
                    if self.session.active_contour.is_some() {
                        ctx.request_render();
                    }
                }
                match &mut self.drag {
                    Drag::AdvanceLine => {
                        let d = self.session.viewport.screen_to_design(at);
                        self.session.set_advance(d.x.round());
                        ctx.request_render();
                    }
                    Drag::LeftLine { last_x } => {
                        let dx_screen = at.x - *last_x;
                        *last_x = at.x;
                        let dx = (dx_screen / self.session.viewport.zoom).round();
                        if dx != 0.0 {
                            self.session.shift_glyph(dx);
                            ctx.request_render();
                        }
                    }
                    Drag::Anchor { idx } => {
                        let idx = *idx;
                        let d = self.session.viewport.screen_to_design(at);
                        self.session.move_anchor(idx, d.x.round(), d.y.round());
                        ctx.request_render();
                    }
                    Drag::Pen { origin, dragging } => {
                        let origin = *origin;
                        let to = self.session.viewport.screen_to_design(at);
                        let moved_px = (self.session.viewport.affine() * origin).distance(at);
                        if *dragging {
                            self.session.pen_smooth_drag(origin, to);
                            ctx.request_render();
                        } else if moved_px > 4.0 {
                            if let Drag::Pen { dragging, .. } = &mut self.drag {
                                *dragging = true;
                            }
                            self.session.pen_smooth_begin(origin, to);
                            ctx.request_render();
                        }
                    }
                    Drag::Points { start } => {
                        let zoom = self.session.viewport.zoom;
                        let total = ((at.x - start.x) / zoom, -(at.y - start.y) / zoom);
                        if self.session.drag_points_to(total) {
                            ctx.request_render();
                        }
                    }
                    Drag::Pan { last } => {
                        let d = at - *last;
                        *last = at;
                        self.session.viewport.pan(d.x, d.y);
                        ctx.request_render();
                    }
                    Drag::Marquee { current, .. } => {
                        *current = at;
                        ctx.request_render();
                    }
                    Drag::Shape { current, .. } => {
                        *current = self.session.viewport.screen_to_design(at);
                        ctx.request_render();
                    }
                    Drag::None => {}
                }
            }
            PointerEvent::Up(_) | PointerEvent::Cancel(_) => match &self.drag {
                Drag::Points { .. } => {
                    self.session.end_point_drag();
                    self.drag = Drag::None;
                    self.emit(ctx, true);
                }
                Drag::Pen { origin, dragging } => {
                    if !dragging {
                        self.session.pen_corner(origin.x, origin.y);
                    }
                    self.drag = Drag::None;
                    self.emit(ctx, true);
                }
                Drag::Marquee { start, current, additive } => {
                    let rect = Rect::from_points(*start, *current);
                    let additive = *additive;
                    if !additive {
                        self.session.selection.clear();
                    }
                    for (id, sp, _, _, _) in self.screen_points() {
                        if rect.contains(sp) {
                            self.session.selection.insert(id);
                        }
                    }
                    self.drag = Drag::None;
                    self.emit(ctx, false);
                }
                Drag::Anchor { .. } | Drag::AdvanceLine | Drag::LeftLine { .. } => {
                    self.drag = Drag::None;
                    self.emit(ctx, true);
                }
                Drag::Shape { start, current } => {
                    let (s0, c0) = (*start, *current);
                    match self.tool {
                        Tool::Rect => self.session.add_rect(s0.x, s0.y, c0.x, c0.y),
                        Tool::Ellipse => self.session.add_ellipse(s0.x, s0.y, c0.x, c0.y),
                        Tool::Knife => {
                            self.session.knife_cut(s0, c0);
                        }
                        _ => {}
                    }
                    self.drag = Drag::None;
                    self.emit(ctx, true);
                }
                Drag::Pan { .. } => self.drag = Drag::None,
                Drag::None => {}
            },
            PointerEvent::Scroll(PointerScrollEvent { delta, state, .. }) => {
                let at = ctx.local_position(state.position);
                let dy = match delta {
                    ScrollDelta::PixelDelta(p) => p.y,
                    ScrollDelta::LineDelta(_, y) => f64::from(*y) * 20.0,
                    _ => 0.0,
                };
                let factor = (dy / 300.0).exp();
                self.session.viewport.zoom_about(at, factor, 0.02, 64.0);
                ctx.request_render();
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn on_text_event(&mut self, ctx: &mut EventCtx<'_>, _props: &mut PropertiesMut<'_>, event: &TextEvent) {
        let TextEvent::Keyboard(key) = event else { return };
        if key.state != KeyState::Down {
            return;
        }
        // Read-only instance: don't handle edit keys, but let Escape bubble to
        // the shortcut host (so it still leaves the editor).
        if self.interp.is_some() {
            return;
        }
        let cmd = key.modifiers.meta() || key.modifiers.ctrl();
        let shift = key.modifiers.shift();
        let step = if shift { 10.0 } else { 1.0 };
        let (edited, handled) = match &key.key {
            Key::Named(NamedKey::Escape) => {
                if self.session.pen_is_active() || self.session.active_contour.is_some() {
                    self.session.pen_cancel();
                    self.emit(ctx, false);
                    ctx.set_handled();
                }
                // Otherwise do not handle: the shortcut host turns Escape into
                // "back to overview".
                return;
            }
            Key::Named(NamedKey::ArrowLeft) => (self.session.nudge(-step, 0.0), true),
            Key::Named(NamedKey::ArrowRight) => (self.session.nudge(step, 0.0), true),
            Key::Named(NamedKey::ArrowUp) => (self.session.nudge(0.0, step), true),
            Key::Named(NamedKey::ArrowDown) => (self.session.nudge(0.0, -step), true),
            Key::Named(NamedKey::Delete) | Key::Named(NamedKey::Backspace) => {
                if self.session.selected_anchor.is_some() {
                    (self.session.delete_selected_anchor(), true)
                } else {
                    (self.session.delete_selected(), true)
                }
            }
            Key::Character(c) if cmd && c.eq_ignore_ascii_case("a") => {
                self.session.select_all();
                (false, true)
            }
            Key::Character(c) if cmd && !shift && c.eq_ignore_ascii_case("z") => {
                (self.session.undo(), true)
            }
            Key::Character(c) if cmd && (c == "y" || (shift && c.eq_ignore_ascii_case("z"))) => {
                (self.session.redo(), true)
            }
            _ => return,
        };
        if handled {
            self.emit(ctx, edited);
            ctx.set_handled();
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Canvas
    }

    fn accessibility(&mut self, _ctx: &mut AccessCtx<'_>, _props: &PropertiesRef<'_>, node: &mut Node) {
        node.set_description(format!("Glyph editor: {}", self.session.glyph_name));
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}



// ---------------------------------------------------------------------------
/// Cheap equality for the interpolation overlay: same Arc, or both absent.
fn interp_eq(a: &Option<Arc<masonry::kurbo::BezPath>>, b: &Option<Arc<masonry::kurbo::BezPath>>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => Arc::ptr_eq(x, y),
        (None, None) => true,
        _ => false,
    }
}

// View wrapper.

pub struct EditorView<F> {
    session: Arc<Session>,
    palette: Arc<Palette>,
    tool: Tool,
    view: ViewOptions,
    ghosts: Arc<Vec<masonry::kurbo::BezPath>>,
    interp: Option<Arc<masonry::kurbo::BezPath>>,
    underlay: Underlay,
    on_event: F,
}

pub fn editor<F: Fn(&mut App, EditorEvent) + 'static>(
    session: Arc<Session>,
    palette: Arc<Palette>,
    tool: Tool,
    view: ViewOptions,
    ghosts: Arc<Vec<masonry::kurbo::BezPath>>,
    interp: Option<Arc<masonry::kurbo::BezPath>>,
    underlay: Underlay,
    on_event: F,
) -> EditorView<F> {
    EditorView {
        session,
        palette,
        tool,
        view,
        ghosts,
        interp,
        underlay,
        on_event,
    }
}

/// The analysis overlays: what the editor draws on top of the outline
/// besides the points.
///
/// These match the GPUI build's Measure and Curves options, and all of
/// them read from `runebender-core`, so the three editors agree about
/// what a kink or a stem is.
#[derive(Clone, Copy, Default, PartialEq)]
pub struct ViewOptions {
    /// The curvature comb.
    pub comb: bool,
    /// A dot per on-curve node, colored by continuity level.
    pub continuity: bool,
    /// Tint the outline and handles by segment length.
    pub colorize: bool,
    /// Label handle lengths.
    pub handles: bool,
    /// Label straight segment lengths.
    pub segments: bool,
    /// Draw and label the side bearings.
    pub bearings: bool,
    /// Spell lengths as sums of powers of two: 96 = 64+32.
    pub popcount: bool,
}

impl ViewOptions {
    /// What the Measure tool turns on when it is picked.
    pub fn measuring() -> Self {
        Self {
            handles: true,
            segments: true,
            bearings: true,
            popcount: true,
            ..Self::default()
        }
    }

    /// Whether anything in the measure group is on.
    pub fn measures(self) -> bool {
        self.colorize || self.handles || self.segments || self.bearings
    }

    /// A length, spelled the way the options ask for.
    fn label(self, value: i64) -> String {
        if self.popcount {
            runebender_core::measure::label(value)
        } else {
            value.to_string()
        }
    }
}

/// What is drawn under the outline: the UFO background layer, and a
/// reference glyph. Both are read-only and quiet on purpose; they are
/// there to trace against, not to compete with the drawing.
#[derive(Clone, Default, PartialEq)]
pub struct Underlay {
    /// The glyph's contours in the UFO's background layer.
    pub background: Option<Arc<masonry::kurbo::BezPath>>,
    /// Another glyph, shown behind this one.
    pub reference: Option<Arc<masonry::kurbo::BezPath>>,
}

impl<F> ViewMarker for EditorView<F> {}
impl<F: Fn(&mut App, EditorEvent) + 'static> View<App, (), ViewCtx> for EditorView<F> {
    type Element = Pod<EditorWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut App) -> (Self::Element, Self::ViewState) {
        let widget = EditorWidget {
            session: (*self.session).clone(),
            palette: self.palette.clone(),
            tool: self.tool,
            ghosts: self.ghosts.clone(),
            interp: self.interp.clone(),
            underlay: self.underlay.clone(),
            size: Size::ZERO,
            drag: Drag::None,
            hover: None,
            menu: None,
            view: self.view,
        };
        (ctx.with_action_widget(|ctx| ctx.create_pod(widget)), ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut App,
    ) {
        let mut dirty = false;
        if !Arc::ptr_eq(&self.session, &prev.session) {
            let viewport = element.widget.session.viewport.clone();
            let fitted = element.widget.session.fitted;
            element.widget.session = (*self.session).clone();
            element.widget.session.viewport = viewport;
            element.widget.session.fitted = fitted;
            dirty = true;
        }
        if self.tool != prev.tool {
            element.widget.tool = self.tool;
            if self.tool != Tool::Pen {
                element.widget.session.pen_cancel();
            }
            dirty = true;
        }
        if self.view != prev.view {
            element.widget.view = self.view;
            dirty = true;
        }
        if !Arc::ptr_eq(&self.ghosts, &prev.ghosts) {
            element.widget.ghosts = self.ghosts.clone();
            dirty = true;
        }
        if !interp_eq(&self.interp, &prev.interp) {
            element.widget.interp = self.interp.clone();
            dirty = true;
        }
        if self.underlay != prev.underlay {
            element.widget.underlay = self.underlay.clone();
            dirty = true;
        }
        if dirty {
            element.ctx.request_render();
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        app: &mut App,
    ) -> MessageResult<()> {
        match message.take_message::<EditorEvent>() {
            Some(event) => {
                // The island is the live source of truth while editing. Pull its
                // session back into the app before the callback runs, so save and
                // the grid preview see the edits (the widget edits its own clone).
                app.sync_session_from(&element.widget.session);
                (self.on_event)(app, *event);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry_testing::TestHarness;
    use masonry::theme::default_property_set;

    fn session() -> Session {
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("A");
        glyph.width = 500.0;
        let mut contour = norad::Contour::default();
        for (x, y) in [(0.0, 0.0), (400.0, 0.0), (400.0, 700.0), (0.0, 700.0)] {
            contour.points.push(norad::ContourPoint::new(
                x,
                y,
                norad::PointType::Line,
                false,
                None,
                None,
            ));
        }
        glyph.contours.push(contour);
        font.default_layer_mut().insert_glyph(glyph);
        Session::new(&font, "A").expect("the glyph is there")
    }

    fn widget() -> EditorWidget {
        EditorWidget {
            session: session(),
            palette: Arc::new(crate::theme::Palette::load("dark")),
            tool: Tool::Select,
            ghosts: Arc::new(Vec::new()),
            interp: None,
            underlay: Underlay::default(),
            size: Size::ZERO,
            drag: Drag::None,
            hover: None,
            menu: None,
            view: ViewOptions::default(),
        }
    }

    /// A right click opens the menu as a layer, and the editor remembers
    /// which layer so a second right click does not stack another one.
    #[test]
    fn right_click_opens_one_menu_layer() {
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget().prepare(), (600, 400));
        harness.mouse_move(Point::new(300.0, 200.0));
        harness.mouse_button_press(Some(PointerButton::Secondary));
        let first = harness.edit_root_widget(|root| root.widget.menu);
        assert!(first.is_some(), "the menu layer was created");

        harness.mouse_button_press(Some(PointerButton::Secondary));
        let second = harness.edit_root_widget(|root| root.widget.menu);
        assert_eq!(first, second, "a second right click did not stack a layer");
    }

    /// A left click still edits rather than being eaten by menu handling.
    #[test]
    fn left_click_is_not_swallowed() {
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget().prepare(), (600, 400));
        harness.mouse_move(Point::new(300.0, 200.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        let menu = harness.edit_root_widget(|root| root.widget.menu);
        assert!(menu.is_none(), "a left click does not open the menu");
    }
}
