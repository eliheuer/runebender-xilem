// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The nodes canvas: the open `.nodes.json` as boxes and wires, drawn
//! by Vello.
//!
//! The layout comes from the font engine (`runebender::ui::nodes`): where every
//! box, port and wire sits, in canvas units, and what is under a
//! point. This widget adds the paint calls and the mouse, the same
//! way the glyph editor does: the engine's `ViewPort` for pan and zoom, one
//! drag enum, and a `Painter`. Vello rasterizes the paths with edge
//! coverage so rings, wires, and keylines are anti-aliased.

use std::collections::BTreeMap;
use std::sync::Arc;

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayerType, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PointerScrollEvent, PointerUpdate,
    PropertiesMut, PropertiesRef, RegisterCtx, ScrollDelta, TextEvent, Widget, WidgetId, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, BezPath, Line, Point, Rect, Shape as _, Size, Stroke, Vec2};
use masonry::layout::{LenReq, Length};
use masonry::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use masonry::widgets::{Image as MasonryImage, Portal, TextAction};
use runebender::document::nodes::{Kind, NodeGraph, Registry};
use runebender::document::nodes_run::Status;
use runebender::ui::editing::viewport::ViewPort;
use runebender::ui::nodes::{self as nl, Hit, NodeBox, NodeContentMap, NodeRegion};
use xilem::Color;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::{Pod, ViewCtx};

use crate::application::editor::tools::nodes::RowState;
use crate::application::view::theme::Palette;
use crate::application::widgets::context_menu::{ContextMenu, MenuAction, MenuRow, MenuTarget};
use crate::application::widgets::source_text_area::SourceTextArea;
use crate::application::widgets::text_label::{self, Anchor};
use crate::application::workspace::Workspace;

/// What the canvas tells the app.
#[derive(Debug, Clone)]
pub(crate) enum NodesEvent {
    /// The graph was edited: a node moved, a wire made or taken off, a
    /// node deleted.
    Changed(NodeGraph),
    /// The selection moved.
    Selected(Option<u32>),
    /// Inline Python code changed through the focused child editor.
    EditCode { node: u32, code: String },
    /// A completed header drag changed only presentation layout.
    MoveNode { node: u32, pos: [f32; 2] },
    /// An embedded-content node was resized without changing its semantic input.
    Resize { node: u32, size: [f32; 2] },
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
    /// Resizing embedded content without changing graph semantics.
    Resize {
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
    /// A child widget or inert node body owns the press.
    Idle,
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
    content: Arc<NodeContentMap>,
    code_editors: BTreeMap<u32, WidgetPod<Portal<SourceTextArea>>>,
    code_area_ids: BTreeMap<u32, WidgetId>,
    preview_images: BTreeMap<u32, WidgetPod<SpecimenPreview>>,
    boxes: Vec<NodeBox>,
    viewport: ViewPort,
    fitted: bool,
    size: Size,
    selected: Option<u32>,
    drag: Option<Drag>,
    /// The right-click menu's layer, while it is up.
    menu: Option<WidgetId>,
    code_font_size: f32,
}

fn code_editor(text: &str) -> (WidgetPod<Portal<SourceTextArea>>, WidgetId) {
    let source = NewWidget::new(
        SourceTextArea::new(text)
            .with_text_size(crate::application::view::design::TextSize::Caption.px()),
    );
    let area_id = source.id();
    let portal = Portal::new(source).content_must_fill(true);
    (NewWidget::new(portal).to_pod(), area_id)
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    Some((
        u32::from_be_bytes(bytes[16..20].try_into().ok()?),
        u32::from_be_bytes(bytes[20..24].try_into().ok()?),
    ))
}

fn decode_png(png: &nl::ImmutablePng) -> Option<ImageData> {
    const MAX_PNG_BYTES: usize = 5 * 1024 * 1024;
    const MAX_PNG_PIXELS: u64 = 16 * 1024 * 1024;
    if png.bytes.len() > MAX_PNG_BYTES {
        return None;
    }
    let (width, height) = png_dimensions(&png.bytes)?;
    if (width, height) != (png.width, png.height) {
        return None;
    }
    let pixels = u64::from(width).checked_mul(u64::from(height))?;
    if pixels == 0 || pixels > MAX_PNG_PIXELS {
        return None;
    }
    let rgba = image::load_from_memory_with_format(&png.bytes, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    if rgba.width() != width || rgba.height() != height {
        return None;
    }
    Some(ImageData {
        data: Blob::new(Arc::new(rgba.into_raw())),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    })
}

struct SpecimenPreview {
    image: WidgetPod<MasonryImage>,
    zoom: f64,
    pan: Vec2,
    drag: Option<Point>,
}

impl SpecimenPreview {
    fn new(image: ImageData) -> Self {
        Self {
            image: NewWidget::new(MasonryImage::new(image).with_alt_text("Specimen proof image"))
                .to_pod(),
            zoom: 1.0,
            pan: Vec2::ZERO,
            drag: None,
        }
    }
}

impl Widget for SpecimenPreview {
    type Action = ();

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.image);
    }

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
            _ => Length::px(120.0),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let child = Size::new(size.width * self.zoom, size.height * self.zoom);
        ctx.run_layout(&mut self.image, child);
        ctx.place_child(
            &mut self.image,
            Point::new(
                (size.width - child.width) / 2.0 + self.pan.x,
                (size.height - child.height) / 2.0 + self.pan.y,
            ),
        );
        ctx.set_clip_path(size.to_rect());
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                if state.count >= 2 {
                    self.zoom = 1.0;
                    self.pan = Vec2::ZERO;
                    self.drag = None;
                    ctx.request_layout();
                } else {
                    self.drag = Some(ctx.local_position(state.position));
                    ctx.capture_pointer();
                }
                ctx.set_handled();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let at = ctx.local_position(current.position);
                if let Some(last) = self.drag.replace(at) {
                    self.pan += at - last;
                    ctx.request_layout();
                    ctx.set_handled();
                }
            }
            PointerEvent::Up(_) => {
                self.drag = None;
                ctx.set_handled();
            }
            PointerEvent::Scroll(PointerScrollEvent { delta, .. }) => {
                let dy = match delta {
                    ScrollDelta::PixelDelta(delta) => delta.y,
                    ScrollDelta::LineDelta(_, y) => f64::from(*y) * 20.0,
                    _ => 0.0,
                };
                self.zoom = (self.zoom * (dy * 0.0015).exp()).clamp(0.25, 8.0);
                ctx.request_layout();
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
    }

    fn accessibility_role(&self) -> Role {
        Role::Image
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.image.id()])
    }
}

type ContentChildren = (
    BTreeMap<u32, WidgetPod<Portal<SourceTextArea>>>,
    BTreeMap<u32, WidgetId>,
    BTreeMap<u32, WidgetPod<SpecimenPreview>>,
);

fn content_children(content: &NodeContentMap) -> ContentChildren {
    let mut editors = BTreeMap::new();
    let mut area_ids = BTreeMap::new();
    let mut images = BTreeMap::new();
    for (&node, content) in &content.by_node {
        match content {
            nl::NodeContent::Script(script) => {
                let (editor, area_id) = code_editor(&script.text);
                editors.insert(node, editor);
                area_ids.insert(node, area_id);
            }
            nl::NodeContent::Image(image) => {
                let visible = image.image.as_ref().or(image.previous_image.as_ref());
                if let Some(decoded) = visible.and_then(decode_png) {
                    images.insert(node, NewWidget::new(SpecimenPreview::new(decoded)).to_pod());
                }
            }
        }
    }
    (editors, area_ids, images)
}

type ProjectedImage = (u32, Option<(String, u32, u32)>);

fn projected_images(content: &NodeContentMap) -> Vec<ProjectedImage> {
    content
        .by_node
        .iter()
        .filter_map(|(&node, content)| match content {
            nl::NodeContent::Image(content) => Some((
                node,
                content
                    .image
                    .as_ref()
                    .or(content.previous_image.as_ref())
                    .map(|image| (image.output_hash.clone(), image.width, image.height)),
            )),
            nl::NodeContent::Script(_) => None,
        })
        .collect()
}

fn editor_node_from_path(path: &[ViewId]) -> Option<u32> {
    path.first()
        .and_then(|id| u32::try_from(id.routing_id()).ok())
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
                [
                    crate::application::view::render::px32(nl::snap(at.x)),
                    crate::application::view::render::px32(nl::snap(at.y)),
                ],
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
        self.boxes = nl::layout_with_content(&self.graph, &self.registry, &self.content);
    }

    fn to_canvas(&self, local: Point) -> Point {
        nl::to_canvas(&self.viewport, local)
    }

    fn editor_target(&self, target: WidgetId) -> bool {
        self.code_area_ids.values().any(|&id| id == target)
            || self
                .code_editors
                .values()
                .any(|editor| editor.id() == target)
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

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for editor in self.code_editors.values_mut() {
            ctx.register_child(editor);
        }
        for image in self.preview_images.values_mut() {
            ctx.register_child(image);
        }
    }

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
            // Fit the initial graph without rewriting its authored positions.
            const FIT_MARGIN: f64 = 24.0;
            let bounds = self
                .boxes
                .iter()
                .map(|node| node.rect)
                .reduce(|a, b| a.union(b));
            if let Some(bounds) = bounds {
                let zoom = ((size.width - FIT_MARGIN * 2.0).max(1.0) / bounds.width().max(1.0))
                    .min((size.height - FIT_MARGIN * 2.0).max(1.0) / bounds.height().max(1.0))
                    .clamp(0.05, 1.0);
                self.viewport.zoom = zoom;
                self.viewport.offset = Vec2::new(
                    size.width / 2.0 - bounds.center().x * zoom,
                    size.height / 2.0 - bounds.center().y * zoom,
                );
            } else {
                self.viewport.zoom = 1.0;
                self.viewport.offset = Vec2::new(FIT_MARGIN, FIT_MARGIN);
            }
            self.fitted = true;
        }
        let zoom = crate::application::view::render::px32(self.viewport.zoom);
        let code_font_size =
            (crate::application::view::design::TextSize::Caption.px() * zoom).clamp(6.0, 40.0);
        if (self.code_font_size - code_font_size).abs() > f32::EPSILON {
            self.code_font_size = code_font_size;
            for editor in self.code_editors.values_mut() {
                ctx.mutate_child_later(editor, move |mut portal| {
                    let mut source = Portal::child_mut(&mut portal);
                    SourceTextArea::set_text_size(&mut source, code_font_size);
                });
            }
        }
        let transform = nl::canvas_affine(&self.viewport);
        for node in &self.boxes {
            let Some(content) = node.content_rect() else {
                continue;
            };
            let screen = Rect::from_points(
                transform * Point::new(content.x0, content.y0),
                transform * Point::new(content.x1, content.y1),
            );
            let child_size = Size::new(screen.width().max(1.0), screen.height().max(1.0));
            if let Some(editor) = self.code_editors.get_mut(&node.id) {
                ctx.run_layout(editor, child_size);
                ctx.place_child(editor, screen.origin());
            }
            if let Some(image) = self.preview_images.get_mut(&node.id) {
                ctx.run_layout(image, child_size);
                ctx.place_child(image, screen.origin());
            }
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

        // The GPUI canvas uses a quiet field of solid alignment dots.
        // Painting the shared circles rather than stroking them keeps the
        // pattern dense and legible at normal zoom without competing with
        // node ports or wires.
        if nl::GRID * zoom >= 8.0 {
            let rings = nl::grid_rings(nl::visible_canvas(&self.viewport, self.size.to_rect()));
            painter
                .fill(
                    &(tf * rings),
                    // Mixed most of the way into the ground so the grid stays
                    // behind.
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

        let text_px = crate::application::view::render::px32((13.0 * zoom).clamp(6.0, 40.0));
        for nb in &self.boxes {
            let selected = self.selected == Some(nb.id);
            let mark = nb.mark().and_then(|m| pal.mark(m));
            let outline = if selected { pal.outline } else { keyline };
            // Body, header band in the mark colour (inverted when
            // selected), the rule between them, the keyline.
            let offset = if selected { 5.0 } else { 4.0 } / zoom.max(0.01);
            let shadow = nb.rect + Vec2::new(-offset, offset);
            painter
                .fill(&(tf * rect_path(shadow)), pal.cell_shadow())
                .draw();
            painter.fill(&(tf * rect_path(nb.rect)), pal.field).draw();
            let header_bg = if selected {
                pal.selected_bg()
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
                .stroke(&(tf * rect_path(nb.rect)), &Stroke::new(1.0), outline)
                .draw();
            // Title left, the run mark right.
            let title_ink = if selected {
                pal.selected_ink()
            } else if mark.is_some() {
                pal.mark_ink.unwrap_or(pal.text)
            } else {
                pal.text
            };
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
                let label = input_port_label(port);
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
            if let Some(content) = &nb.content {
                let (state, identity) = match content {
                    nl::NodeContent::Script(content) => {
                        (&content.state, Some(content.content_hash.as_str()))
                    }
                    nl::NodeContent::Image(content) => (
                        &content.state,
                        content
                            .image
                            .as_ref()
                            .or(content.previous_image.as_ref())
                            .map(|image| image.output_hash.as_str()),
                    ),
                };
                let short_identity = identity
                    .filter(|identity| !identity.is_empty())
                    .map(|identity| identity.chars().take(24).collect::<String>());
                let status: String = match state {
                    nl::ContentState::Idle => "Not run".into(),
                    nl::ContentState::Running => "Running\u{2026}".into(),
                    nl::ContentState::Current => short_identity.map_or_else(
                        || "Current".into(),
                        |identity| format!("Current · {identity}"),
                    ),
                    nl::ContentState::Stale => short_identity
                        .map_or_else(|| "Stale".into(), |identity| format!("Stale · {identity}")),
                    nl::ContentState::Error(message) => message.clone(),
                };
                let at =
                    tf * Point::new(nb.rect.x0 + nl::PAD, nb.rect.y1 - nl::RESIZE_HANDLE / 2.0);
                text_label::draw(
                    painter,
                    at,
                    &status,
                    text_px * 0.8,
                    if matches!(state, nl::ContentState::Error(_)) {
                        pal.role("error")
                    } else {
                        pal.text_muted
                    },
                    Anchor::Start,
                );
                if let Some(handle) = nb.resize_rect() {
                    let a = tf * Point::new(handle.x0 + nl::PAD / 2.0, handle.y1 - nl::PAD / 2.0);
                    let b = tf * Point::new(handle.x1, handle.y0);
                    painter
                        .stroke(Line::new(a, b), &Stroke::new(1.0), pal.text_muted)
                        .draw();
                }
            }
        }
        // Foreground wires and ports stay visible over card edges, as in GPUI.
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

        for nb in &self.boxes {
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
                let fill = if port.linked {
                    self.kind_color(port.kind)
                } else {
                    pal.field
                };
                painter.fill(&dot, fill).draw();
                painter.stroke(&dot, &Stroke::new(1.0), ink).draw();
                if takes {
                    let ring = tf * nl::circle(port.at, nl::PORT_R * 2.0);
                    painter.stroke(&ring, &Stroke::new(1.0), pal.text).draw();
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
                if !self.editor_target(ctx.target()) {
                    ctx.request_focus();
                    ctx.capture_pointer();
                }
                let local = ctx.local_position(state.position);
                let at = self.to_canvas(local);
                let drag = match nl::hit(&self.boxes, at) {
                    Hit::Node(id) => {
                        self.selected = Some(id);
                        ctx.submit_action::<NodesEvent>(NodesEvent::Selected(Some(id)));
                        match nl::node_region_hit(&self.boxes, at).map(|hit| hit.region) {
                            Some(NodeRegion::Header) => {
                                let origin = self.graph.node(id).map(|n| n.pos).unwrap_or_default();
                                Drag::Move {
                                    id,
                                    start: at,
                                    origin,
                                }
                            }
                            Some(NodeRegion::Resize) => {
                                let origin = match self.content.get(id) {
                                    Some(nl::NodeContent::Script(content)) => content.size,
                                    Some(nl::NodeContent::Image(content)) => content.size,
                                    None => [
                                        crate::application::view::render::px32(nl::LIVE_W),
                                        crate::application::view::render::px32(nl::IMAGE_H),
                                    ],
                                };
                                Drag::Resize {
                                    id,
                                    start: at,
                                    origin,
                                }
                            }
                            _ => Drag::Idle,
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
                                crate::application::view::render::px32(nl::snap(
                                    f64::from(origin[0]) + (at.x - start.x),
                                )),
                                crate::application::view::render::px32(nl::snap(
                                    f64::from(origin[1]) + (at.y - start.y),
                                )),
                            ];
                        }
                        self.relayout();
                        ctx.request_layout();
                        ctx.request_render();
                    }
                    Some(Drag::Resize { id, start, origin }) => {
                        let size = [
                            crate::application::view::render::px32(
                                (f64::from(origin[0]) + at.x - start.x).clamp(nl::NODE_W, 1024.0),
                            ),
                            crate::application::view::render::px32(
                                (f64::from(origin[1]) + at.y - start.y)
                                    .clamp(nl::ROW_H * 3.0, 768.0),
                            ),
                        ];
                        if let Some(content) = Arc::make_mut(&mut self.content).by_node.get_mut(id)
                        {
                            match content {
                                nl::NodeContent::Script(content) => content.size = size,
                                nl::NodeContent::Image(content) => content.size = size,
                            }
                        }
                        self.relayout();
                        ctx.request_layout();
                        ctx.request_render();
                    }
                    Some(Drag::Pan { last }) => {
                        let d = local - *last;
                        *last = local;
                        self.viewport.pan(d.x, d.y);
                        ctx.request_layout();
                        ctx.request_render();
                    }
                    Some(Drag::Wire { to, .. }) => {
                        *to = at;
                        ctx.request_render();
                    }
                    Some(Drag::Idle) => {}
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
                    Some(Drag::Move { id, .. }) => {
                        if let Some(node) = self.graph.node(id) {
                            ctx.submit_action::<NodesEvent>(NodesEvent::MoveNode {
                                node: id,
                                pos: node.pos,
                            });
                        }
                    }
                    Some(Drag::Resize { id, .. }) => {
                        let size = match self.content.get(id) {
                            Some(nl::NodeContent::Script(content)) => content.size,
                            Some(nl::NodeContent::Image(content)) => content.size,
                            None => return,
                        };
                        ctx.submit_action::<NodesEvent>(NodesEvent::Resize { node: id, size });
                    }
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
                ctx.request_layout();
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
        if self.editor_target(ctx.target()) {
            if (key.modifiers.meta() || key.modifiers.ctrl())
                && matches!(&key.key, Key::Character(c) if c.eq_ignore_ascii_case("z") || c.eq_ignore_ascii_case("y"))
            {
                // TextArea has no local history. Consume these shortcuts here so
                // typing in a node cannot undo an unrelated font edit.
                ctx.set_handled();
            }
            // Do not let an unhandled Backspace/Delete reach canvas node removal.
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
        ChildrenIds::from_iter(
            self.code_editors
                .values()
                .map(WidgetPod::id)
                .chain(self.preview_images.values().map(WidgetPod::id)),
        )
    }
}

/// Keep large source and recipe payloads in their real child/editor surfaces.
/// Painting the same JSON beside the socket makes it run through adjacent
/// nodes and competes with the graph topology.
fn input_port_label(port: &nl::PortBox) -> String {
    let Some(value) = &port.value else {
        return port.name.clone();
    };
    let hide_value =
        matches!(port.name.as_str(), "code" | "recipe" | "edits") || value.chars().count() > 48;
    if hide_value {
        format!("{} …", port.name)
    } else {
        format!("{} {value}", port.name)
    }
}

/// The view: the file, the registry, the run marks, and the selection
/// the app remembers.
pub(crate) struct NodesView<F> {
    fit_request: u64,
    graph: Arc<NodeGraph>,
    registry: Arc<Registry>,
    palette: Arc<Palette>,
    rows: Arc<BTreeMap<u32, RowState>>,
    content: Arc<NodeContentMap>,
    selected: Option<u32>,
    on_event: F,
}

pub(crate) fn nodes_canvas<F: Fn(&mut Workspace, NodesEvent) + 'static>(
    graph: Arc<NodeGraph>,
    registry: Arc<Registry>,
    palette: Arc<Palette>,
    rows: Arc<BTreeMap<u32, RowState>>,
    content: Arc<NodeContentMap>,
    selected: Option<u32>,
    fit_request: u64,
    on_event: F,
) -> NodesView<F> {
    NodesView {
        fit_request,
        graph,
        registry,
        palette,
        rows,
        content,
        selected,
        on_event,
    }
}

impl<F> ViewMarker for NodesView<F> {}
impl<F: Fn(&mut Workspace, NodesEvent) + 'static> View<Workspace, (), ViewCtx> for NodesView<F> {
    type Element = Pod<NodesWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut Workspace) -> (Self::Element, Self::ViewState) {
        let (code_editors, code_area_ids, preview_images) = content_children(&self.content);
        for (&node, &area_id) in &code_area_ids {
            ctx.with_id(ViewId::new(u64::from(node)), |ctx| {
                ctx.record_action_source(area_id);
            });
        }
        let mut widget = NodesWidget {
            graph: (*self.graph).clone(),
            registry: self.registry.clone(),
            palette: self.palette.clone(),
            rows: self.rows.clone(),
            content: self.content.clone(),
            code_editors,
            code_area_ids,
            preview_images,
            boxes: Vec::new(),
            viewport: ViewPort::new(),
            fitted: false,
            size: Size::ZERO,
            selected: self.selected,
            drag: None,
            menu: None,
            code_font_size: 0.0,
        };
        widget.relayout();
        (ctx.with_action_widget(|ctx| ctx.create_pod(widget)), ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut Workspace,
    ) {
        let mut dirty = false;
        if self.fit_request != prev.fit_request {
            element.widget.fitted = false;
            element.ctx.request_layout();
            dirty = true;
        }
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
        if self.content != prev.content {
            element.widget.content = self.content.clone();
            let wanted_editors: Vec<u32> = self
                .content
                .by_node
                .iter()
                .filter_map(|(&node, content)| {
                    matches!(content, nl::NodeContent::Script(_)).then_some(node)
                })
                .collect();
            let current_editors: Vec<u32> = element.widget.code_editors.keys().copied().collect();
            if wanted_editors != current_editors {
                for (_, editor) in std::mem::take(&mut element.widget.code_editors) {
                    element.ctx.remove_child(editor);
                }
                let (editors, area_ids, _) = content_children(&self.content);
                element.widget.code_editors = editors;
                element.widget.code_area_ids = area_ids;
                for (&node, &area_id) in &element.widget.code_area_ids {
                    ctx.with_id(ViewId::new(u64::from(node)), |ctx| {
                        ctx.record_action_source(area_id);
                    });
                }
                element.ctx.children_changed();
            } else {
                for (&node, content) in &self.content.by_node {
                    let nl::NodeContent::Script(script) = content else {
                        continue;
                    };
                    let editor = element
                        .widget
                        .code_editors
                        .get_mut(&node)
                        .expect("script node has an editor child");
                    let mut editor = element.ctx.get_mut(editor);
                    let mut source = Portal::child_mut(&mut editor);
                    SourceTextArea::replace_external_content(&mut source, &script.text);
                }
            }
            if projected_images(&self.content) != projected_images(&prev.content) {
                for (_, image) in std::mem::take(&mut element.widget.preview_images) {
                    element.ctx.remove_child(image);
                }
                element.widget.preview_images = content_children(&self.content).2;
                element.ctx.children_changed();
            }
            element.widget.relayout();
            element.ctx.request_layout();
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
        let editor_node = editor_node_from_path(message.remaining_path());
        match message.take_message::<NodesEvent>() {
            Some(event) => {
                (self.on_event)(app, *event);
                MessageResult::Action(())
            }
            None => match message.take_message::<TextAction>() {
                Some(action) => match *action {
                    TextAction::Changed(code) | TextAction::Entered(code) => {
                        if let Some(node) = editor_node {
                            (self.on_event)(app, NodesEvent::EditCode { node, code });
                            MessageResult::Action(())
                        } else {
                            MessageResult::Stale
                        }
                    }
                    TextAction::Cancelled => MessageResult::Nop,
                },
                None => MessageResult::Stale,
            },
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgba, RgbaImage};
    use masonry::core::keyboard::{Code, Key, KeyState, KeyboardEvent, Modifiers};
    use masonry::core::{Ime, TextEvent};
    use masonry_testing::TestHarness;
    use runebender::document::nodes_live;
    use runebender::document::variable::SourceId;
    use runebender::ui::nodes::{
        ContentState, ImageContent, ImmutablePng, NodeContent, ScriptContent,
    };
    use std::io::Cursor;

    fn png() -> Arc<[u8]> {
        let mut raster = RgbaImage::new(2, 2);
        raster.put_pixel(0, 0, Rgba([0, 0, 0, 255]));
        raster.put_pixel(1, 0, Rgba([255, 255, 255, 255]));
        raster.put_pixel(0, 1, Rgba([255, 255, 255, 255]));
        raster.put_pixel(1, 1, Rgba([0, 0, 0, 255]));
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(raster)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("fixture PNG encodes");
        Arc::from(bytes.into_inner())
    }

    fn fixture_with_scripts(extra_script: bool) -> (NodesWidget, BTreeMap<u32, WidgetId>) {
        let mut graph = nodes_live::comparison_starter(SourceId(0));
        if extra_script {
            graph.add("live.python", [320.0, 220.0]);
        }
        let registry = Arc::new(Registry::core());
        let mut content = NodeContentMap::default();
        for node in &graph.nodes {
            match node.type_name.as_str() {
                "live.python" => {
                    content.by_node.insert(
                        node.id,
                        NodeContent::Script(ScriptContent {
                            text: "print('A')\n".into(),
                            content_hash: "script".into(),
                            state: ContentState::Current,
                            size: [240.0, 112.0],
                        }),
                    );
                }
                "live.proof" => {
                    content.by_node.insert(
                        node.id,
                        NodeContent::Image(ImageContent {
                            image: Some(ImmutablePng {
                                bytes: png(),
                                width: 2,
                                height: 2,
                                output_hash: format!("proof-{}", node.id),
                            }),
                            previous_image: None,
                            state: ContentState::Current,
                            size: [240.0, 144.0],
                        }),
                    );
                }
                _ => {}
            }
        }
        let content = Arc::new(content);
        let (code_editors, code_area_ids, preview_images) = content_children(&content);
        let editor_ids = code_area_ids.clone();
        let mut widget = NodesWidget {
            graph,
            registry,
            palette: Arc::new(Palette::load("gray")),
            rows: Arc::new(BTreeMap::new()),
            content,
            code_editors,
            code_area_ids,
            preview_images,
            boxes: Vec::new(),
            viewport: ViewPort::new(),
            fitted: false,
            size: Size::ZERO,
            selected: None,
            drag: None,
            menu: None,
            code_font_size: 0.0,
        };
        widget.relayout();
        (widget, editor_ids)
    }

    fn fixture() -> (NodesWidget, WidgetId) {
        let (widget, editor_ids) = fixture_with_scripts(false);
        let editor_id = editor_ids
            .values()
            .next()
            .expect("Python editor child")
            .to_owned();
        (widget, editor_id)
    }

    #[test]
    fn real_code_and_png_children_render_and_route_text_input() {
        let (widget, editor_id) = fixture();
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            NewWidget::new(widget),
            (1100, 720),
        );
        let rendered = harness.render();
        assert!(rendered.iter().any(|byte| *byte != 0));

        harness.mouse_click_on(editor_id, Some(PointerButton::Primary));
        assert!(matches!(
            harness.pop_action::<NodesEvent>(),
            Some((NodesEvent::Selected(Some(3)), _))
        ));
        harness.process_text_event(TextEvent::Ime(Ime::Commit("#".into())));
        assert!(matches!(
            harness.pop_action::<TextAction>(),
            Some((TextAction::Changed(text), _)) if text.contains('#')
        ));
        harness.process_text_event(TextEvent::ClipboardPaste("\npass".into()));
        assert!(matches!(
            harness.pop_action::<TextAction>(),
            Some((TextAction::Changed(text), _)) if text.contains("pass")
        ));
    }

    #[test]
    fn second_editor_action_keeps_its_real_child_origin() {
        let (widget, editor_ids) = fixture_with_scripts(true);
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            NewWidget::new(widget),
            (1100, 720),
        );
        let (&second_node, &second_editor) = editor_ids.iter().next_back().unwrap();
        let (&first_node, &first_editor) = editor_ids.iter().next().unwrap();
        assert_ne!(first_node, second_node);
        assert_ne!(first_editor, second_editor);

        harness.mouse_click_on(second_editor, Some(PointerButton::Primary));
        assert!(matches!(
            harness.pop_action::<NodesEvent>(),
            Some((NodesEvent::Selected(Some(node)), _)) if node == second_node
        ));
        harness.process_text_event(TextEvent::Ime(Ime::Commit("# second".into())));
        let Some((TextAction::Changed(text), source)) = harness.pop_action::<TextAction>() else {
            panic!("second editor did not emit TextAction");
        };
        assert!(text.contains("# second"));
        assert_eq!(source, second_editor);
        assert_ne!(source, first_editor);
        assert_eq!(
            editor_node_from_path(&[ViewId::new(u64::from(second_node))]),
            Some(second_node)
        );

        let mut modifiers = Modifiers::empty();
        modifiers.set(
            if cfg!(target_os = "macos") {
                Modifiers::META
            } else {
                Modifiers::CONTROL
            },
            true,
        );
        harness.process_text_event(TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: Key::Character("z".into()),
            code: Code::Unidentified,
            modifiers,
            ..KeyboardEvent::default()
        }));
        assert!(harness.pop_action::<TextAction>().is_none());
    }

    #[test]
    fn png_dimensions_are_read_from_the_encoded_header() {
        let mut bytes = vec![
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0,
            0, 0, 2, 0, 0, 0, 3,
        ];
        assert_eq!(png_dimensions(&bytes), Some((2, 3)));
        bytes[12] = b't';
        assert_eq!(png_dimensions(&bytes), None);
    }
}
