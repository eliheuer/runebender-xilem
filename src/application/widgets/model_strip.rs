// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The proof strip's model view: a trained font's drawing of the text, with a node at every
//! caret index. Dragging a node pulls that letter and the rest of its word.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx,
    Widget,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Axis, Circle, Point, Rect, Shape as _, Size, Stroke, Vec2};
use masonry::layout::{LenReq, Length};
use std::sync::Arc;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

use crate::application::platform::model::ModelRender;

/// What the strip reports upward. Distances are in font units, y up.
#[derive(Debug)]
pub(crate) enum ModelStripEvent {
    DragStart(usize),
    Drag { node: usize, delta: Vec2 },
    DragEnd,
}

/// The colors the strip is drawn with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StripInks {
    pub ink: Color,
    pub background: Color,
    pub node: Color,
    pub node_outline: Color,
}

const NODE_RADIUS: f64 = 4.0;
const HIT_RADIUS: f64 = 10.0;

pub(crate) struct ModelStripWidget {
    render: Option<Arc<ModelRender>>,
    inks: StripInks,
    size: Size,
    hovered: Option<usize>,
    /// A drag in progress: the node, where it began, and the transform then, which stays
    /// put so the stretching word does not move under the pointer.
    drag: Option<(usize, Point, Affine)>,
}

impl ModelStripWidget {
    /// Font units to the strip, fitting the drawing and its nodes.
    fn transform(&self) -> Affine {
        let Some(render) = &self.render else {
            return Affine::IDENTITY;
        };
        let bounds = render
            .paths
            .iter()
            .filter(|path| !path.elements().is_empty())
            .map(|path| path.bounding_box())
            .chain(
                render
                    .nodes
                    .iter()
                    .map(|node| Rect::from_center_size(*node, (1.0, 1.0))),
            )
            .reduce(|bounds, next| bounds.union(next))
            .unwrap_or(Rect::ZERO);
        crate::application::view::panels::preview::proof_transform(bounds, 0.0, self.size)
    }

    fn node_at(&self, at: Point) -> Option<usize> {
        let t = self.transform();
        let render = self.render.as_ref()?;
        render
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (index, (t * *node).distance(at)))
            .filter(|(_, distance)| *distance <= HIT_RADIUS)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    }
}

impl Widget for ModelStripWidget {
    type Action = ModelStripEvent;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        len_req: LenReq,
        _cross: Option<Length>,
    ) -> Length {
        match len_req {
            LenReq::FitContent(space) => space,
            _ => Length::px(100.0),
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.size = size;
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        painter.fill_rect(self.size.to_rect(), self.inks.background);
        let Some(render) = &self.render else {
            return;
        };
        let t = self.drag.map_or_else(|| self.transform(), |(_, _, t)| t);
        for path in &render.paths {
            painter.fill(&(t * path.clone()), self.inks.ink).draw();
        }
        for (index, node) in render.nodes.iter().enumerate() {
            let lit = self.hovered == Some(index) || self.drag.is_some_and(|(n, ..)| n == index);
            let radius = if lit { NODE_RADIUS + 2.0 } else { NODE_RADIUS };
            let dot = Circle::new(t * *node, radius);
            painter.fill(dot, self.inks.node).draw();
            painter
                .stroke(dot, &Stroke::new(1.0), self.inks.node_outline)
                .draw();
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
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let at = ctx.local_position(state.position);
                if let Some(node) = self.node_at(at) {
                    self.drag = Some((node, at, self.transform()));
                    ctx.capture_pointer();
                    ctx.submit_action::<ModelStripEvent>(ModelStripEvent::DragStart(node));
                    ctx.set_handled();
                    ctx.request_render();
                }
            }
            PointerEvent::Move(update) => {
                let at = ctx.local_position(update.current.position);
                if let Some((node, from, t)) = self.drag {
                    // Strip pixels back to font units: the transform's scale, with y up.
                    let scale = t.as_coeffs()[0].max(1e-9);
                    let delta = (at - from) / scale;
                    ctx.submit_action::<ModelStripEvent>(ModelStripEvent::Drag {
                        node,
                        delta: Vec2::new(delta.x, -delta.y),
                    });
                    ctx.set_handled();
                } else {
                    let hovered = self.node_at(at);
                    if hovered != self.hovered {
                        self.hovered = hovered;
                        ctx.request_render();
                    }
                }
            }
            PointerEvent::Up(_) | PointerEvent::Cancel(_) if self.drag.is_some() => {
                self.drag = None;
                ctx.submit_action::<ModelStripEvent>(ModelStripEvent::DragEnd);
                ctx.set_handled();
                ctx.request_render();
            }
            PointerEvent::Leave(_) if self.hovered.is_some() => {
                self.hovered = None;
                ctx.request_render();
            }
            _ => {}
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::HoveredChanged(false)) && self.hovered.is_some() {
            self.hovered = None;
            ctx.request_render();
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

pub(crate) struct ModelStrip<F> {
    render: Option<Arc<ModelRender>>,
    inks: StripInks,
    on_event: F,
}

pub(crate) fn model_strip<F, Workspace: 'static>(
    render: Option<Arc<ModelRender>>,
    inks: StripInks,
    on_event: F,
) -> ModelStrip<F>
where
    F: Fn(&mut Workspace, ModelStripEvent) + 'static,
{
    ModelStrip {
        render,
        inks,
        on_event,
    }
}

impl<F> ViewMarker for ModelStrip<F> {}

impl<F, Workspace: 'static> View<Workspace, (), ViewCtx> for ModelStrip<F>
where
    F: Fn(&mut Workspace, ModelStripEvent) + 'static,
{
    type Element = Pod<ModelStripWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut Workspace) -> (Self::Element, Self::ViewState) {
        let widget = ModelStripWidget {
            render: self.render.clone(),
            inks: self.inks,
            size: Size::ZERO,
            hovered: None,
            drag: None,
        };
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
        let same = match (&self.render, &prev.render) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same {
            element.widget.render = self.render.clone();
            element.ctx.request_render();
        }
        if self.inks != prev.inks {
            element.widget.inks = self.inks;
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
        match message.take_message::<ModelStripEvent>() {
            Some(event) => {
                (self.on_event)(app, *event);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}
