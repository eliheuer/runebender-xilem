// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The nodes canvas: the open `.nodes.json` as boxes and wires, drawn
//! by Vello.
//!
//! The layout is core's (`runebender_core::ui::nodes`): where every
//! box, port and wire sits, in canvas units, and what is under a
//! point. This widget adds the paint calls and the mouse, the same
//! way the glyph editor does: core's `ViewPort` for pan and zoom, one
//! drag enum, and a `Painter`. Vello rasterizes the paths with edge
//! coverage, so a ring, a wire and a keyline are anti-aliased here
//! where the GPUI build's lyon triangles are not.

use std::collections::BTreeMap;
use std::sync::Arc;

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayerType, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PointerScrollEvent, PointerUpdate,
    PropertiesMut, PropertiesRef, RegisterCtx, ScrollDelta, TextEvent, Widget, WidgetId,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, BezPath, Line, Point, Rect, Shape as _, Size, Stroke};
use masonry::layout::{LenReq, Length};
use runebender_core::document::nodes::{Kind, NodeGraph, Registry};
use runebender_core::document::nodes_run::Status;
use runebender_core::ui::editing::viewport::ViewPort;
use runebender_core::ui::nodes::{self as nl, Hit, NodeBox};
use xilem::Color;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use crate::Workspace;
use crate::edit::nodes::RowState;
use crate::view::theme::Palette;
use crate::widgets::context_menu::{ContextMenu, MenuAction, MenuRow, MenuTarget};
use crate::widgets::text_label::{self, Anchor};

/// What the canvas tells the app.
#[derive(Debug, Clone)]
pub(crate) enum NodesEvent {
    /// The graph was edited: a node moved, a wire made or taken off, a
    /// node deleted.
    Changed(NodeGraph),
    /// The selection moved.
    Selected(Option<u32>),
    /// Something to say in the bar.
    Note(String),
}

/// What a drag on the canvas is doing.
#[derive(Debug, Clone)]
enum Drag {
    /// Moving a node: where the gesture began and the node's position
    /// then, in canvas units.
    Move {
        id: u32,
        start: Point,
        origin: [f32; 2],
    },
    /// Panning: the last pointer position in local pixels.
    Pan { last: Point },
    /// Pulling a wire from an output to wherever the pointer is, in
    /// canvas units.
    Wire {
        from: u32,
        output: String,
        kind: Kind,
        to: Point,
    },
}

/// A rectangle as a path, so the affine applies to it.
fn rect_path(r: Rect) -> BezPath {
    r.to_path(0.1)
}

/// The widget.
pub(crate) struct NodesWidget {
    graph: NodeGraph,
    registry: Arc<Registry>,
    palette: Arc<Palette>,
    rows: Arc<BTreeMap<u32, RowState>>,
    boxes: Vec<NodeBox>,
    viewport: ViewPort,
    fitted: bool,
    size: Size,
    selected: Option<u32>,
    drag: Option<Drag>,
    /// The right-click menu's layer, while it is up.
    menu: Option<WidgetId>,
}

impl NodesWidget {
    /// A choice from the right-click menu: a node of that type lands
    /// where the menu was opened, snapped to the grid.
    pub(crate) fn apply_menu_choice(
        this: &mut masonry::core::WidgetMut<'_, Self>,
        action: MenuAction,
        at: Point,
    ) {
        this.widget.menu = None;
        if let MenuAction::AddNode(type_name) = action {
            let id = this.widget.graph.add(
                &type_name,
                [crate::px32(nl::snap(at.x)), crate::px32(nl::snap(at.y))],
            );
            this.widget.selected = Some(id);
            this.widget.relayout();
            let graph = this.widget.graph.clone();
            this.ctx
                .submit_action::<NodesEvent>(NodesEvent::Changed(graph));
            this.ctx
                .submit_action::<NodesEvent>(NodesEvent::Selected(Some(id)));
        }
        this.ctx.request_render();
    }

    /// The menu closed without a choice.
    pub(crate) fn forget_menu(this: &mut masonry::core::WidgetMut<'_, Self>) {
        this.widget.menu = None;
    }

    fn relayout(&mut self) {
        self.boxes = nl::layout(&self.graph, &self.registry);
    }

    fn to_canvas(&self, local: Point) -> Point {
        nl::to_canvas(&self.viewport, local)
    }

    fn emit_changed(&self, ctx: &mut EventCtx<'_>) {
        ctx.submit_action::<NodesEvent>(NodesEvent::Changed(self.graph.clone()));
    }

    /// The grid mark colour a port kind carries, in this palette.
    fn kind_color(&self, kind: Kind) -> Color {
        nl::kind_mark(kind)
            .and_then(|m| self.palette.mark(m))
            .unwrap_or(self.palette.text_muted)
    }
}

impl Widget for NodesWidget {
    type Action = NodesEvent;

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
        self.size = size;
        if !self.fitted {
            // First use: canvas units at one pixel each, a margin in.
            self.viewport.zoom = 1.0;
            self.viewport.offset = kurbo::Vec2::new(24.0, 24.0);
            self.fitted = true;
        }
        ctx.set_clip_path(size.to_rect());
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let pal = self.palette.clone();
        let tf = nl::canvas_affine(&self.viewport);
        let zoom = self.viewport.zoom;
        painter.fill_rect(self.size.to_rect(), pal.app);

        // The grid: a ring at every pitch, quiet, behind everything.
        if nl::GRID * zoom >= 8.0 {
            let rings = nl::grid_rings(nl::visible_canvas(&self.viewport, self.size.to_rect()));
            painter
                .stroke(
                    &(tf * rings),
                    &Stroke::new(1.0),
                    // Mixed most of the way into the ground, as the
                    // GPUI build draws it, so the grid stays behind.
                    mix(pal.app, pal.outline, 0.4),
                )
                .draw();
        }

        // Wires: a keyline one stroke wider on each side, then the
        // colour of what they carry.
        let wire_w = (1.5 * zoom).max(1.0);
        let keyline = pal.text_muted;
        let pending_kind = match &self.drag {
            Some(Drag::Wire { kind, .. }) => Some(*kind),
            _ => None,
        };
        let draw_wire = |painter: &mut Painter<'_>, a: Point, b: Point, ink: Color| {
            let path = tf * nl::wire_path(a, b);
            painter
                .stroke(&path, &Stroke::new(wire_w + 2.0), keyline)
                .draw();
            painter.stroke(&path, &Stroke::new(wire_w), ink).draw();
        };
        for (a, o, b, i) in nl::wires(&self.graph, &self.boxes) {
            let port = &self.boxes[a].outputs[o];
            draw_wire(
                painter,
                port.at,
                self.boxes[b].inputs[i].at,
                self.kind_color(port.kind),
            );
        }
        if let Some(Drag::Wire {
            from, output, to, ..
        }) = &self.drag
            && let Some(port) = self
                .boxes
                .iter()
                .find(|b| b.id == *from)
                .and_then(|b| b.outputs.iter().find(|p| p.name == *output))
        {
            let ink = pending_kind.map_or(pal.text, |k| self.kind_color(k));
            draw_wire(painter, port.at, *to, ink);
        }

        let text_px = crate::px32((13.0 * zoom).clamp(6.0, 40.0));
        for nb in &self.boxes {
            let selected = self.selected == Some(nb.id);
            let mark = nb.mark().and_then(|m| pal.mark(m));
            let outline = if selected { pal.text } else { keyline };
            // Body, header band in the mark colour (inverted when
            // selected), the rule between them, the keyline.
            painter.fill(&(tf * rect_path(nb.rect)), pal.field).draw();
            let header_bg = if selected {
                pal.text
            } else {
                mark.unwrap_or(pal.panel)
            };
            painter
                .fill(&(tf * rect_path(nb.header())), header_bg)
                .draw();
            let rule = Line::new(
                tf * Point::new(nb.rect.x0, nb.header().y1),
                tf * Point::new(nb.rect.x1, nb.header().y1),
            );
            painter.stroke(rule, &Stroke::new(1.0), outline).draw();
            painter
                .stroke(
                    &(tf * rect_path(nb.rect)),
                    &Stroke::new(if selected { 2.0 } else { 1.0 }),
                    outline,
                )
                .draw();
            // Title left, the run mark right.
            let title_ink = if selected { pal.app } else { pal.text };
            let pad = nl::PAD * zoom;
            let header_mid = tf * Point::new(nb.rect.x0, nb.rect.y0 + nl::HEADER_H / 2.0);
            text_label::draw(
                painter,
                Point::new(header_mid.x + pad, header_mid.y),
                &nb.title,
                text_px,
                title_ink,
                Anchor::Start,
            );
            let mark_text = match self.rows.get(&nb.id) {
                Some(RowState::Running(_)) => "\u{2026}",
                Some(RowState::Done(Status::Ran, _)) => "\u{2713}",
                Some(RowState::Done(Status::Skipped, _)) => "=",
                Some(RowState::Done(Status::Failed, _)) => "\u{2717}",
                Some(RowState::Done(Status::Blocked, _)) => "\u{2013}",
                _ => "",
            };
            let header_right = tf * Point::new(nb.rect.x1, nb.rect.y0 + nl::HEADER_H / 2.0);
            text_label::draw(
                painter,
                Point::new(header_right.x - pad, header_mid.y),
                mark_text,
                text_px,
                title_ink,
                Anchor::End,
            );
            // Rows: an input's name at the left, an output's at the right.
            let port_r = nl::PORT_R * zoom;
            for port in &nb.inputs {
                let label = match &port.value {
                    Some(v) => format!("{} {v}", port.name),
                    None => port.name.clone(),
                };
                let at = tf * Point::new(nb.rect.x0, nb.row_top(port.row) + nl::ROW_H / 2.0);
                text_label::draw(
                    painter,
                    Point::new(at.x + pad + port_r, at.y),
                    &label,
                    text_px,
                    if port.linked || port.value.is_some() {
                        pal.text
                    } else {
                        pal.text_muted
                    },
                    Anchor::Start,
                );
            }
            for port in &nb.outputs {
                let at = tf * Point::new(nb.rect.x1, nb.row_top(port.row) + nl::ROW_H / 2.0);
                text_label::draw(
                    painter,
                    Point::new(at.x - pad - port_r, at.y),
                    &port.name,
                    text_px,
                    pal.text_muted,
                    Anchor::End,
                );
            }
            // Ports: a filled dot when wired, a ring when not. While a
            // wire is out, the inputs that take it grow a second ring
            // and the rest fade.
            for (port, is_input) in nb
                .inputs
                .iter()
                .map(|p| (p, true))
                .chain(nb.outputs.iter().map(|p| (p, false)))
            {
                let (takes, fades) = match pending_kind {
                    Some(k) if is_input => (port.kind == k, port.kind != k),
                    Some(_) => (false, true),
                    None => (false, false),
                };
                let ink = if fades { pal.text_muted } else { pal.text };
                let dot = tf * nl::circle(port.at, nl::PORT_R);
                painter
                    .fill(&dot, if port.linked { ink } else { pal.field })
                    .draw();
                painter.stroke(&dot, &Stroke::new(1.0), ink).draw();
                if takes {
                    let ring = tf * nl::circle(port.at, nl::PORT_R * 2.0);
                    painter.stroke(&ring, &Stroke::new(1.0), pal.text).draw();
                }
            }
            // A result line under the box, when the node has one.
            if let Some(RowState::Done(_, Some(note))) = self.rows.get(&nb.id) {
                let at = tf * Point::new(nb.rect.x0, nb.rect.y1 + nl::PAD);
                text_label::draw(
                    painter,
                    at,
                    note,
                    text_px * 0.9,
                    pal.text_muted,
                    Anchor::Start,
                );
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
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Secondary),
                state,
                ..
            }) => {
                // The node types to add, as a layer rooted in window
                // space, the way the editor's menu is.
                if self.menu.is_none() {
                    let local = ctx.local_position(state.position);
                    let at = self.to_canvas(local);
                    let rows: Vec<MenuRow> = self
                        .registry
                        .types
                        .iter()
                        .filter(|t| t.implemented)
                        .map(|t| MenuRow {
                            label: std::borrow::Cow::Owned(t.title.clone()),
                            action: MenuAction::AddNode(t.name.clone()),
                        })
                        .collect();
                    let menu = ContextMenu::new(
                        ctx.widget_id(),
                        MenuTarget::Nodes,
                        rows,
                        self.palette.clone(),
                        at,
                    );
                    let menu = NewWidget::new(menu);
                    self.menu = Some(menu.id());
                    ctx.create_layer(LayerType::Other, menu, ctx.to_window(local));
                }
                ctx.set_handled();
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                ctx.request_focus();
                ctx.capture_pointer();
                let local = ctx.local_position(state.position);
                let at = self.to_canvas(local);
                let drag = match nl::hit(&self.boxes, at) {
                    Hit::Node(id) => {
                        self.selected = Some(id);
                        ctx.submit_action::<NodesEvent>(NodesEvent::Selected(Some(id)));
                        let origin = self.graph.node(id).map(|n| n.pos).unwrap_or_default();
                        Drag::Move {
                            id,
                            start: at,
                            origin,
                        }
                    }
                    Hit::Output(from, output, kind) => Drag::Wire {
                        from,
                        output,
                        kind,
                        to: at,
                    },
                    Hit::Input(to, input, _) => {
                        // Picking up a wired input takes the wire off
                        // it, to drop somewhere else or nowhere.
                        match self.graph.link_into(to, &input).cloned() {
                            Some(link) => {
                                let kind = self
                                    .graph
                                    .node(link.from())
                                    .and_then(|n| self.registry.get(&n.type_name))
                                    .and_then(|t| t.output(link.output()).map(|p| p.kind))
                                    .unwrap_or(Kind::Text);
                                self.graph.links.retain(|l| l != &link);
                                self.relayout();
                                self.emit_changed(ctx);
                                Drag::Wire {
                                    from: link.from(),
                                    output: link.output().to_string(),
                                    kind,
                                    to: at,
                                }
                            }
                            None => Drag::Pan { last: local },
                        }
                    }
                    Hit::Empty => {
                        self.selected = None;
                        ctx.submit_action::<NodesEvent>(NodesEvent::Selected(None));
                        Drag::Pan { last: local }
                    }
                };
                self.drag = Some(drag);
                ctx.request_render();
                ctx.set_handled();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let local = ctx.local_position(current.position);
                let at = self.to_canvas(local);
                match &mut self.drag {
                    Some(Drag::Move { id, start, origin }) => {
                        let (id, start, origin) = (*id, *start, *origin);
                        if let Some(node) = self.graph.node_mut(id) {
                            node.pos = [
                                crate::px32(nl::snap(f64::from(origin[0]) + (at.x - start.x))),
                                crate::px32(nl::snap(f64::from(origin[1]) + (at.y - start.y))),
                            ];
                        }
                        self.relayout();
                        ctx.request_render();
                    }
                    Some(Drag::Pan { last }) => {
                        let d = local - *last;
                        *last = local;
                        self.viewport.pan(d.x, d.y);
                        ctx.request_render();
                    }
                    Some(Drag::Wire { to, .. }) => {
                        *to = at;
                        ctx.request_render();
                    }
                    None => {}
                }
            }
            PointerEvent::Up(PointerButtonEvent { state, .. }) => {
                let local = ctx.local_position(state.position);
                let at = self.to_canvas(local);
                match self.drag.take() {
                    Some(Drag::Wire {
                        from, output, kind, ..
                    }) => {
                        if let Hit::Input(to, input, want) = nl::hit(&self.boxes, at) {
                            if to != from && want == kind {
                                self.graph.connect(from, &output, to, &input);
                                self.relayout();
                            } else if want != kind {
                                ctx.submit_action::<NodesEvent>(NodesEvent::Note(format!(
                                    "{kind} does not go into {want}"
                                )));
                            }
                        }
                        self.emit_changed(ctx);
                    }
                    Some(Drag::Move { .. }) => self.emit_changed(ctx),
                    _ => {}
                }
                ctx.request_render();
                ctx.set_handled();
            }
            PointerEvent::Scroll(PointerScrollEvent { delta, state, .. }) => {
                let at = ctx.local_position(state.position);
                let dy = match delta {
                    ScrollDelta::PixelDelta(p) => p.y,
                    ScrollDelta::LineDelta(_, y) => f64::from(*y) * 20.0,
                    _ => 0.0,
                };
                let factor = (dy * 0.0015).exp();
                self.viewport.zoom_about(at, factor, 0.25, 4.0);
                ctx.request_render();
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down {
            return;
        }
        if matches!(
            key.key,
            Key::Named(NamedKey::Backspace) | Key::Named(NamedKey::Delete)
        ) && let Some(id) = self.selected.take()
        {
            self.graph.remove(id);
            self.relayout();
            ctx.submit_action::<NodesEvent>(NodesEvent::Selected(None));
            self.emit_changed(ctx);
            ctx.request_render();
            ctx.set_handled();
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Canvas
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

/// The view: the file, the registry, the run marks, and the selection
/// the app remembers.
pub(crate) struct NodesView<F> {
    graph: Arc<NodeGraph>,
    registry: Arc<Registry>,
    palette: Arc<Palette>,
    rows: Arc<BTreeMap<u32, RowState>>,
    selected: Option<u32>,
    on_event: F,
}

pub(crate) fn nodes_canvas<F: Fn(&mut Workspace, NodesEvent) + 'static>(
    graph: Arc<NodeGraph>,
    registry: Arc<Registry>,
    palette: Arc<Palette>,
    rows: Arc<BTreeMap<u32, RowState>>,
    selected: Option<u32>,
    on_event: F,
) -> NodesView<F> {
    NodesView {
        graph,
        registry,
        palette,
        rows,
        selected,
        on_event,
    }
}

impl<F> ViewMarker for NodesView<F> {}
impl<F: Fn(&mut Workspace, NodesEvent) + 'static> View<Workspace, (), ViewCtx> for NodesView<F> {
    type Element = Pod<NodesWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut Workspace) -> (Self::Element, Self::ViewState) {
        let mut widget = NodesWidget {
            graph: (*self.graph).clone(),
            registry: self.registry.clone(),
            palette: self.palette.clone(),
            rows: self.rows.clone(),
            boxes: Vec::new(),
            viewport: ViewPort::new(),
            fitted: false,
            size: Size::ZERO,
            selected: self.selected,
            drag: None,
            menu: None,
        };
        widget.relayout();
        (ctx.with_action_widget(|ctx| ctx.create_pod(widget)), ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut Workspace,
    ) {
        let mut dirty = false;
        if !Arc::ptr_eq(&self.graph, &prev.graph) && *self.graph != element.widget.graph {
            element.widget.graph = (*self.graph).clone();
            element.widget.relayout();
            dirty = true;
        }
        if !Arc::ptr_eq(&self.registry, &prev.registry) {
            element.widget.registry = self.registry.clone();
            element.widget.relayout();
            dirty = true;
        }
        if !Arc::ptr_eq(&self.rows, &prev.rows) {
            element.widget.rows = self.rows.clone();
            dirty = true;
        }
        if !Arc::ptr_eq(&self.palette, &prev.palette) {
            element.widget.palette = self.palette.clone();
            dirty = true;
        }
        if self.selected != prev.selected {
            element.widget.selected = self.selected;
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
        _element: Mut<'_, Self::Element>,
        app: &mut Workspace,
    ) -> MessageResult<()> {
        match message.take_message::<NodesEvent>() {
            Some(event) => {
                (self.on_event)(app, *event);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}

/// `a` moved `t` of the way toward `b`, opaque.
fn mix(a: Color, b: Color, t: f32) -> Color {
    let (a, b) = (a.components, b.components);
    Color::new([
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        1.0,
    ])
}
