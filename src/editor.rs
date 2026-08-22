// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The glyph editor island: a canvas widget that owns the edit session
//! and gesture state, and the view that hosts it.

use std::collections::HashSet;
use std::sync::Arc;

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PointerScrollEvent, PointerUpdate, PropertiesMut,
    PropertiesRef, RegisterCtx, ScrollDelta, TextEvent, Widget,
};
use masonry::imaging::Painter;
use masonry::kurbo::{BezPath, Circle, Line, Point, Rect, Size, Stroke, Axis};
use masonry::layout::{LenReq, Length};
use runebender_core::editing::viewport::ViewPort;
use runebender_core::model::entity_id::EntityId;
use runebender_core::model::workspace::Contour;
use runebender_core::path::Path;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use crate::theme::Palette;
use crate::App;

// ---------------------------------------------------------------------------
// Session: the glyph being edited, owned by the canvas island.

#[derive(Clone)]
pub struct Metrics {
    ascender: f64,
    descender: f64,
    x_height: f64,
    cap_height: f64,
}

#[derive(Clone)]
pub struct Session {
    glyph_name: String,
    paths: Vec<Path>,
    advance: f64,
    metrics: Metrics,
    selection: HashSet<EntityId>,
    viewport: ViewPort,
    fitted: bool,
}

impl Session {
    pub fn new(font: &norad::Font, name: &str) -> Option<Self> {
        let glyph = font.get_glyph(name)?;
        let paths = glyph
            .contours
            .iter()
            .map(|c| Path::from_contour(&Contour::from_norad(c)))
            .collect();
        let info = &font.font_info;
        let upm = info.units_per_em.map(|u| u.as_f64()).unwrap_or(1000.0);
        Some(Self {
            glyph_name: name.to_string(),
            paths,
            advance: glyph.width,
            metrics: Metrics {
                ascender: info.ascender.unwrap_or(upm * 0.8),
                descender: info.descender.unwrap_or(-upm * 0.2),
                x_height: info.x_height.unwrap_or(upm * 0.5),
                cap_height: info.cap_height.unwrap_or(upm * 0.7),
            },
            selection: HashSet::new(),
            viewport: ViewPort::new(),
            fitted: false,
        })
    }

    pub fn bezpath(&self) -> BezPath {
        let mut out = BezPath::new();
        for path in &self.paths {
            path.append_to_bezpath(&mut out);
        }
        out
    }

    pub fn point_count(&self) -> usize {
        self.paths.iter().map(|p| p.points().len()).sum()
    }

    pub fn glyph_name(&self) -> &str {
        &self.glyph_name
    }

    pub fn advance(&self) -> f64 {
        self.advance
    }

    #[allow(dead_code)]
    pub fn selection_len(&self) -> usize {
        self.selection.len()
    }
}

// ---------------------------------------------------------------------------
// The canvas island: a Masonry widget that owns the session and the gesture state.

const HIT_RADIUS_PX: f64 = 8.0;

/// What the editor reports upward.
#[derive(Debug)]
pub enum EditorEvent {
    /// Selection changed; carries how many points are selected.
    Selection(usize),
    /// The user asked to leave the editor (Escape).
    Exit,
}

enum Drag {
    None,
    Points { last: Point },
    Pan { last: Point },
}

pub struct EditorWidget {
    session: Session,
    palette: Arc<Palette>,
    size: Size,
    drag: Drag,
    undo: runebender_core::editing::undo::UndoState<Vec<Path>>,
}

impl EditorWidget {
    fn screen_points(&self) -> Vec<(EntityId, Point, bool, bool)> {
        let affine = self.session.viewport.affine();
        let mut out = Vec::new();
        for path in &self.session.paths {
            for p in path.points().iter() {
                let smooth = matches!(
                    p.typ,
                    runebender_core::path::point::PointType::OnCurve { smooth: true }
                );
                out.push((p.id, affine * p.point, p.is_on_curve(), smooth));
            }
        }
        out
    }

    fn hit_point(&self, at: Point) -> Option<EntityId> {
        self.screen_points()
            .into_iter()
            .filter(|(_, sp, _, _)| sp.distance(at) <= HIT_RADIUS_PX)
            .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
            .map(|(id, _, _, _)| id)
    }

    fn translate_selection(&mut self, delta_design: kurbo::Vec2) {
        for path in &mut self.session.paths {
            let points = match path {
                Path::Cubic(p) => p.points.make_mut(),
                Path::Quadratic(p) => p.points.make_mut(),
                Path::Hyper(p) => p.points.make_mut(),
            };
            for p in points.iter_mut() {
                if self.session.selection.contains(&p.id) {
                    p.point += delta_design;
                }
            }
        }
    }

    fn all_ids(&self) -> Vec<EntityId> {
        self.session
            .paths
            .iter()
            .flat_map(|p| p.points().iter().map(|pt| pt.id))
            .collect()
    }

    fn push_undo(&mut self) {
        self.undo.add_undo_group(self.session.paths.clone());
    }

    fn delete_selected(&mut self) -> bool {
        if self.session.selection.is_empty() {
            return false;
        }
        let sel = self.session.selection.clone();
        for path in &mut self.session.paths {
            let pts = match path {
                Path::Cubic(p) => &mut p.points,
                Path::Quadratic(p) => &mut p.points,
                Path::Hyper(p) => &mut p.points,
            };
            let kept: Vec<_> = pts.iter().filter(|p| !sel.contains(&p.id)).cloned().collect();
            *pts.make_mut() = kept;
        }
        self.session.paths.retain(|p| !p.points().is_empty());
        self.session.selection.clear();
        true
    }

    fn fit(&mut self) {
        let m = &self.session.metrics;
        self.session.viewport.fit_to_canvas(
            self.size.width,
            self.size.height,
            self.session.advance,
            m.ascender,
            m.descender,
            0.62,
        );
        self.session.fitted = true;
    }
}

impl Widget for EditorWidget {
    type Action = EditorEvent;

    fn accepts_focus(&self) -> bool {
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
        let zoom = self.session.viewport.zoom;
        painter.fill_rect(self.size.to_rect(), pal.canvas);

        // Metrics: baseline, x-height, cap height, ascender, descender, sidebearings.
        let m = &self.session.metrics;
        let thin = Stroke::new(1.0);
        let quiet = pal.role("metricQuiet");
        let x0 = (affine * Point::new(-10_000.0, 0.0)).x;
        let x1 = (affine * Point::new(10_000.0, 0.0)).x;
        for y in [0.0, m.x_height, m.cap_height, m.ascender, m.descender] {
            let sy = (affine * Point::new(0.0, y)).y;
            painter
                .stroke(Line::new((x0, sy), (x1, sy)), &thin, quiet)
                .draw();
        }
        for x in [0.0, self.session.advance] {
            let sx = (affine * Point::new(x, 0.0)).x;
            painter
                .stroke(Line::new((sx, -10_000.0), (sx, 10_000.0)), &thin, quiet)
                .draw();
        }

        // Outline.
        let outline = affine * self.session.bezpath();
        painter
            .fill(&outline, pal.role("pathStroke").with_alpha(0.08))
            .draw();
        painter
            .stroke(&outline, &Stroke::new(1.0), pal.role("pathStroke"))
            .draw();

        // Handles: off-curve points connect to their neighboring on-curve points.
        let handle = Stroke::new(1.0);
        for path in &self.session.paths {
            let pts = path.points().as_slice();
            let n = pts.len();
            if n == 0 {
                continue;
            }
            for i in 0..n {
                let p = &pts[i];
                if !p.is_off_curve() {
                    continue;
                }
                for j in [(i + n - 1) % n, (i + 1) % n] {
                    if pts[j].is_on_curve() {
                        painter
                            .stroke(
                                Line::new(affine * p.point, affine * pts[j].point),
                                &handle,
                                pal.role("pointOffcurve").with_alpha(0.7),
                            )
                            .draw();
                    }
                }
            }
        }

        // Points. Sizes are screen-space; they do not scale with zoom.
        let _ = zoom;
        for (id, sp, on_curve, smooth) in self.screen_points() {
            let selected = self.session.selection.contains(&id);
            let fill = if selected {
                pal.role("pointSelected")
            } else if !on_curve {
                pal.role("pointOffcurve")
            } else if smooth {
                pal.role("pointSmooth")
            } else {
                pal.role("pointCorner")
            };
            if on_curve && !smooth {
                let r = 4.5;
                painter
                    .fill(Rect::new(sp.x - r, sp.y - r, sp.x + r, sp.y + r), fill)
                    .draw();
            } else {
                let r = if on_curve { 4.5 } else { 3.0 };
                painter.fill(Circle::new(sp, r), fill).draw();
            }
        }
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent { button, state, .. }) => {
                ctx.request_focus();
                ctx.capture_pointer();
                let at = ctx.local_position(state.position);
                match button {
                    Some(PointerButton::Primary) => {
                        let shift = state.modifiers.shift();
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
                                self.push_undo();
                                self.drag = Drag::Points { last: at };
                            }
                            None => {
                                if !shift {
                                    self.session.selection.clear();
                                }
                                self.drag = Drag::Pan { last: at };
                            }
                        }
                    }
                    _ => self.drag = Drag::Pan { last: at },
                }
                ctx.request_render();
                ctx.set_handled();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let at = ctx.local_position(current.position);
                match &mut self.drag {
                    Drag::Points { last } => {
                        let zoom = self.session.viewport.zoom;
                        let d = at - *last;
                        *last = at;
                        // Screen y grows down; design y grows up.
                        self.translate_selection(kurbo::Vec2::new(d.x / zoom, -d.y / zoom));
                        ctx.request_render();
                    }
                    Drag::Pan { last } => {
                        let d = at - *last;
                        *last = at;
                        self.session.viewport.pan(d.x, d.y);
                        ctx.request_render();
                    }
                    Drag::None => {}
                }
            }
            PointerEvent::Up(_) | PointerEvent::Cancel(_) => {
                if !matches!(self.drag, Drag::None) {
                    self.drag = Drag::None;
                    ctx.submit_action::<EditorEvent>(EditorEvent::Selection(
                        self.session.selection.len(),
                    ));
                    ctx.request_render();
                }
            }
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

    fn accepts_text_input(&self) -> bool {
        true
    }

    fn on_text_event(&mut self, ctx: &mut EventCtx<'_>, _props: &mut PropertiesMut<'_>, event: &TextEvent) {
        let TextEvent::Keyboard(key) = event else { return };
        if key.state != KeyState::Down {
            return;
        }
        let cmd = key.modifiers.meta() || key.modifiers.ctrl();
        let shift = key.modifiers.shift();
        let step = if shift { 10.0 } else { 1.0 };
        let mut changed = false;
        let mut selection_changed = false;
        match &key.key {
            Key::Named(NamedKey::Escape) => {
                ctx.submit_action::<EditorEvent>(EditorEvent::Exit);
                ctx.set_handled();
                return;
            }
            Key::Named(NamedKey::ArrowLeft) => { self.push_undo(); self.translate_selection(kurbo::Vec2::new(-step, 0.0)); changed = true; }
            Key::Named(NamedKey::ArrowRight) => { self.push_undo(); self.translate_selection(kurbo::Vec2::new(step, 0.0)); changed = true; }
            Key::Named(NamedKey::ArrowUp) => { self.push_undo(); self.translate_selection(kurbo::Vec2::new(0.0, step)); changed = true; }
            Key::Named(NamedKey::ArrowDown) => { self.push_undo(); self.translate_selection(kurbo::Vec2::new(0.0, -step)); changed = true; }
            Key::Named(NamedKey::Delete) | Key::Named(NamedKey::Backspace) => {
                self.push_undo();
                changed = self.delete_selected();
                selection_changed = true;
            }
            Key::Character(c) if cmd && c.eq_ignore_ascii_case("a") => {
                self.session.selection = self.all_ids().into_iter().collect();
                selection_changed = true;
                changed = true;
            }
            Key::Character(c) if cmd && !shift && c.eq_ignore_ascii_case("z") => {
                if let Some(prev) = self.undo.undo(self.session.paths.clone()) {
                    self.session.paths = prev;
                    self.session.selection.clear();
                    changed = true; selection_changed = true;
                }
            }
            Key::Character(c) if cmd && (c == "y" || (shift && c.eq_ignore_ascii_case("z"))) => {
                if let Some(next) = self.undo.redo(self.session.paths.clone()) {
                    self.session.paths = next;
                    self.session.selection.clear();
                    changed = true; selection_changed = true;
                }
            }
            _ => return,
        }
        if changed {
            ctx.request_render();
            if selection_changed {
                ctx.submit_action::<EditorEvent>(EditorEvent::Selection(self.session.selection.len()));
            }
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
// The view that hosts the island. The app hands it a session; it reports events back.

pub struct EditorView<F> {
    session: Arc<Session>,
    palette: Arc<Palette>,
    on_event: F,
}

pub fn editor<F: Fn(&mut App, EditorEvent) + 'static>(
    session: Arc<Session>,
    palette: Arc<Palette>,
    on_event: F,
) -> EditorView<F> {
    EditorView {
        session,
        palette,
        on_event,
    }
}

impl<F> ViewMarker for EditorView<F> {}
impl<F: Fn(&mut App, EditorEvent) + 'static> View<App, (), ViewCtx> for EditorView<F> {
    type Element = Pod<EditorWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut App) -> (Self::Element, Self::ViewState) {
        let widget = EditorWidget {
            session: (*self.session).clone(),
            palette: self.palette.clone(),
            size: Size::ZERO,
            drag: Drag::None,
            undo: runebender_core::editing::undo::UndoState::new(),
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
        if !Arc::ptr_eq(&self.session, &prev.session) {
            // A new glyph. The island keeps its viewport; the session is replaced.
            let viewport = element.widget.session.viewport.clone();
            let fitted = element.widget.session.fitted;
            element.widget.session = (*self.session).clone();
            element.widget.session.viewport = viewport;
            element.widget.session.fitted = fitted;
            element.ctx.request_render();
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app: &mut App,
    ) -> MessageResult<()> {
        match message.take_message::<EditorEvent>() {
            Some(event) => {
                (self.on_event)(app, *event);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}

