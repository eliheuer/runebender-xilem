// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The proof strip's model view: a trained font's drawing of the text, with the strand and a
//! node at every caret index, drawn and handled as `post-opentype/docs/VIEWER.md` says. A
//! click moves the caret, a drag selects, and dragging the active node pulls that letter and
//! the rest of its word.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx,
    Widget,
};
use masonry::imaging::Painter;
use masonry::kurbo::{
    Affine, Arc as Arc2, Axis, BezPath, Cap, Circle, Join, Point, Rect, Shape as _, Size, Stroke,
    Vec2,
};
use masonry::layout::{LenReq, Length};
use std::sync::Arc;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

use crate::application::platform::model::ModelRender;

/// What the strip reports upward. Distances are in font units, y up.
#[derive(Debug)]
pub(crate) enum ModelStripEvent {
    /// The caret moved to a caret index; `extend` keeps the anchor, making a selection.
    Caret {
        index: usize,
        extend: bool,
    },
    DragStart(usize),
    Drag {
        node: usize,
        delta: Vec2,
    },
    DragEnd,
}

/// The colors the strip is drawn with: the names of `docs/VIEWER.md` section 3.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StripInks {
    pub ground: Color,
    pub ink: Color,
    pub strand: Color,
    pub active: Color,
    pub ring: Color,
    pub cloud: Color,
}

/// Sizes from the web demo, in strip pixels.
const NODE_RADIUS: f64 = 5.5;
const ACTIVE_RADIUS: f64 = 10.0;
const RING_RADIUS: f64 = 15.0;
/// The active node and its ring are one target, with a little slack.
const ACTIVE_HIT: f64 = 24.0;
/// One turn of the ring.
const RING_PERIOD: f64 = 1.6;

/// What the pointer is doing.
#[derive(Clone, Copy)]
enum Gesture {
    /// Dragging the active node: the node, where the drag began, and the transform then,
    /// which stays put so the stretching word does not move under the pointer.
    Node(usize, Point, Affine),
    /// Selecting from the caret index where the press landed.
    Select,
}

pub(crate) struct ModelStripWidget {
    render: Option<Arc<ModelRender>>,
    inks: StripInks,
    size: Size,
    gesture: Option<Gesture>,
    /// Seconds since the strip appeared, for the ring's turn.
    clock: f64,
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

    /// The caret index whose node is nearest `at`.
    fn index_at(&self, at: Point, t: Affine) -> Option<usize> {
        let render = self.render.as_ref()?;
        render
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (index, (t * *node).distance(at)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    }

    /// The caret: the second of the selection.
    fn caret(&self) -> Option<usize> {
        let render = self.render.as_ref()?;
        Some(
            render
                .request
                .selection
                .1
                .min(render.nodes.len().saturating_sub(1)),
        )
    }
}

/// A disc with a 1 px ground rim, filled or, for a gap, hollow.
fn node(painter: &mut Painter<'_>, at: Point, radius: f64, hollow: bool, inks: &StripInks) {
    painter
        .fill(Circle::new(at, radius + 1.0), inks.ground)
        .draw();
    if hollow {
        painter
            .stroke(
                Circle::new(at, radius - 1.1),
                &Stroke::new(2.2),
                inks.strand,
            )
            .draw();
    } else {
        painter.fill(Circle::new(at, radius), inks.strand).draw();
    }
}

/// The strand between parameters `u0` and `u1`, with a ground rim.
fn strand(
    painter: &mut Painter<'_>,
    render: &ModelRender,
    t: Affine,
    (u0, u1): (f64, f64),
    width: f64,
    color: Color,
    ground: Color,
) {
    let (lo, hi) = (u0.min(u1), u0.max(u1));
    let mut path = BezPath::new();
    for (u, point) in &render.strand {
        if *u < lo - 1e-9 || *u > hi + 1e-9 {
            continue;
        }
        if path.elements().is_empty() {
            path.move_to(t * *point);
        } else {
            path.line_to(t * *point);
        }
    }
    let round = |w: f64| Stroke::new(w).with_caps(Cap::Round).with_join(Join::Round);
    painter.stroke(&path, &round(width + 2.0), ground).draw();
    painter.stroke(&path, &round(width), color).draw();
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
        let inks = self.inks;
        painter.fill_rect(self.size.to_rect(), inks.ground);
        let Some(render) = self.render.clone() else {
            return;
        };
        let t = match self.gesture {
            Some(Gesture::Node(_, _, t)) => t,
            _ => self.transform(),
        };
        // The selection cloud sits behind the ink.
        if !render.outline_is_hint {
            for path in &render.outline {
                let path = t * path.clone();
                painter.fill(&path, inks.cloud).draw();
                painter.stroke(&path, &Stroke::new(3.0), inks.ground).draw();
                painter.stroke(&path, &Stroke::new(1.5), inks.active).draw();
            }
        }
        for path in &render.paths {
            painter.fill(&(t * path.clone()), inks.ink).draw();
        }
        let Some(caret) = self.caret() else {
            return;
        };
        if render.outline_is_hint {
            for path in &render.outline {
                let path = t * path.clone();
                painter.stroke(&path, &Stroke::new(4.5), inks.ground).draw();
                painter.stroke(&path, &Stroke::new(2.5), inks.active).draw();
            }
        }
        // The whole strand and every node.
        let end = render.strand.last().map_or(0.0, |(u, _)| *u);
        strand(
            painter,
            &render,
            t,
            (0.0, end),
            2.0,
            inks.strand,
            inks.ground,
        );
        for (index, at) in render.nodes.iter().enumerate() {
            let hollow = render.gaps.get(index).copied().unwrap_or(false);
            node(painter, t * *at, NODE_RADIUS, hollow, &inks);
        }
        // Three neighbors each way, larger nearer the caret; the segment flowing into the
        // caret from the hinted letter is the ring's color.
        let plain = render.request.selection.0 == render.request.selection.1;
        let count = render.nodes.len();
        for back in [true, false] {
            for step in 1..=3_usize {
                let at = |n: usize| {
                    if back {
                        caret.checked_sub(n)
                    } else {
                        Some(caret + n).filter(|i| *i < count)
                    }
                };
                let (Some(i0), Some(i1)) = (at(step - 1), at(step)) else {
                    break;
                };
                let incoming = back && step == 1 && plain;
                strand(
                    painter,
                    &render,
                    t,
                    (render.node_t[i0], render.node_t[i1]),
                    2.5,
                    if incoming { inks.ring } else { inks.strand },
                    inks.ground,
                );
                let hollow = render.gaps.get(i1).copied().unwrap_or(false);
                let radius = 9.5 - 1.5 * f64::from(u8::try_from(step).unwrap_or(3));
                node(painter, t * render.nodes[i1], radius, hollow, &inks);
            }
        }
        // The active node, and its turning half-ring.
        let at = t * render.nodes[caret];
        painter
            .fill(Circle::new(at, ACTIVE_RADIUS + 1.0), inks.ground)
            .draw();
        painter
            .fill(Circle::new(at, ACTIVE_RADIUS), inks.active)
            .draw();
        let theta = (self.clock % RING_PERIOD) / RING_PERIOD * std::f64::consts::TAU;
        let ring = Arc2::new(
            at,
            (RING_RADIUS, RING_RADIUS),
            theta,
            std::f64::consts::PI,
            0.0,
        );
        let ring: BezPath = ring.path_elements(0.1).collect();
        painter.stroke(&ring, &Stroke::new(5.5), inks.ground).draw();
        painter.stroke(&ring, &Stroke::new(3.5), inks.ring).draw();
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
                let t = self.transform();
                let Some(render) = self.render.clone() else {
                    return;
                };
                // Grabbing the active node pulls it; anywhere else moves the caret.
                if let Some(caret) = self.caret()
                    && (t * render.nodes[caret]).distance(at) < ACTIVE_HIT
                {
                    self.gesture = Some(Gesture::Node(caret, at, t));
                    ctx.submit_action::<ModelStripEvent>(ModelStripEvent::DragStart(caret));
                } else if let Some(index) = self.index_at(at, t) {
                    self.gesture = Some(Gesture::Select);
                    ctx.submit_action::<ModelStripEvent>(ModelStripEvent::Caret {
                        index,
                        extend: false,
                    });
                }
                ctx.capture_pointer();
                ctx.set_handled();
                ctx.request_render();
            }
            PointerEvent::Move(update) => {
                let at = ctx.local_position(update.current.position);
                match self.gesture {
                    Some(Gesture::Node(node, from, t)) => {
                        // Strip pixels back to font units: the transform's scale, with y up.
                        let scale = t.as_coeffs()[0].max(1e-9);
                        let delta = (at - from) / scale;
                        ctx.submit_action::<ModelStripEvent>(ModelStripEvent::Drag {
                            node,
                            delta: Vec2::new(delta.x, -delta.y),
                        });
                        ctx.set_handled();
                    }
                    Some(Gesture::Select) => {
                        if let Some(index) = self.index_at(at, self.transform()) {
                            ctx.submit_action::<ModelStripEvent>(ModelStripEvent::Caret {
                                index,
                                extend: true,
                            });
                        }
                        ctx.set_handled();
                    }
                    None => {}
                }
            }
            PointerEvent::Up(_) | PointerEvent::Cancel(_) => {
                if matches!(self.gesture.take(), Some(Gesture::Node(..))) {
                    ctx.submit_action::<ModelStripEvent>(ModelStripEvent::DragEnd);
                }
                ctx.request_render();
            }
            _ => {}
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::WidgetAdded) {
            ctx.request_anim_frame();
        }
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        // The ring turns while the strip shows, as in the web demo.
        self.clock += interval as f64 * 1e-9;
        ctx.request_paint_only();
        ctx.request_anim_frame();
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
            gesture: None,
            clock: 0.0,
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
