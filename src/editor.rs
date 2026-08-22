// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The glyph editor island: a canvas widget that owns the edit session
//! and gesture state, and the view that hosts it.

use std::sync::Arc;

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
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
use crate::session::Session;
use crate::theme::Palette;

const HIT_RADIUS_PX: f64 = 8.0;

/// What the editor reports upward.
#[derive(Debug)]
pub enum EditorEvent {
    /// The glyph changed; the app should refresh its cached preview.
    Edited,
    /// Selection changed; carries how many points are selected.
    Selection(usize),
    /// The user asked to leave the editor (Escape).
    Exit,
}

enum Drag {
    None,
    Points { start: Point },
    Pan { last: Point },
}

pub struct EditorWidget {
    session: Session,
    palette: Arc<Palette>,
    size: Size,
    drag: Drag,
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

        let m = &self.session.metrics;
        let thin = Stroke::new(1.0);
        let quiet = pal.role("metricQuiet");
        let x0 = (affine * Point::new(-10_000.0, 0.0)).x;
        let x1 = (affine * Point::new(10_000.0, 0.0)).x;
        for y in [0.0, m.x_height, m.cap_height, m.ascender, m.descender] {
            let sy = (affine * Point::new(0.0, y)).y;
            painter.stroke(Line::new((x0, sy), (x1, sy)), &thin, quiet).draw();
        }
        for x in [0.0, self.session.advance()] {
            let sx = (affine * Point::new(x, 0.0)).x;
            painter
                .stroke(Line::new((sx, -10_000.0), (sx, 10_000.0)), &thin, quiet)
                .draw();
        }

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
                                self.session.begin_point_drag();
                                self.drag = Drag::Points { start: at };
                                self.emit(ctx, false);
                            }
                            None => {
                                if !shift {
                                    self.session.selection.clear();
                                    self.emit(ctx, false);
                                }
                                self.drag = Drag::Pan { last: at };
                            }
                        }
                    }
                    _ => self.drag = Drag::Pan { last: at },
                }
                ctx.set_handled();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let at = ctx.local_position(current.position);
                match &mut self.drag {
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
                    Drag::None => {}
                }
            }
            PointerEvent::Up(_) | PointerEvent::Cancel(_) => match &self.drag {
                Drag::Points { .. } => {
                    self.session.end_point_drag();
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
        let cmd = key.modifiers.meta() || key.modifiers.ctrl();
        let shift = key.modifiers.shift();
        let step = if shift { 10.0 } else { 1.0 };
        let (edited, handled) = match &key.key {
            Key::Named(NamedKey::Escape) => {
                ctx.submit_action::<EditorEvent>(EditorEvent::Exit);
                ctx.set_handled();
                return;
            }
            Key::Named(NamedKey::ArrowLeft) => (self.session.nudge(-step, 0.0), true),
            Key::Named(NamedKey::ArrowRight) => (self.session.nudge(step, 0.0), true),
            Key::Named(NamedKey::ArrowUp) => (self.session.nudge(0.0, step), true),
            Key::Named(NamedKey::ArrowDown) => (self.session.nudge(0.0, -step), true),
            Key::Named(NamedKey::Delete) | Key::Named(NamedKey::Backspace) => {
                (self.session.delete_selected(), true)
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
// View wrapper.

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
