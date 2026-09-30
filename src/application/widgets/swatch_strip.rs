// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A width-fitted strip of square swatch buttons with one uniform gutter.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, PaintCtx, PropertiesRef, RegisterCtx, Widget,
    WidgetMut, WidgetPod,
};
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

pub(crate) struct SwatchStrip<W: Widget> {
    child: WidgetPod<W>,
    count: usize,
}
impl<W: Widget> SwatchStrip<W> {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}
impl<W: Widget> Widget for SwatchStrip<W> {
    type Action = ();
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        request: LenReq,
        cross: Option<Length>,
    ) -> Length {
        if self.count == 0 {
            return match request {
                LenReq::FitContent(space) => space,
                LenReq::MinContent => Length::ZERO,
                LenReq::MaxContent => {
                    crate::application::view::design::ControlSize::Swatch.length()
                }
            };
        }
        if axis == Axis::Vertical {
            let width = cross.map_or(crate::application::view::design::DOCK_WIDTH, Length::get);
            Length::px(strip_height(width, self.count))
        } else {
            ctx.redirect_measurement(&mut self.child, axis, cross)
        }
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.derive_baselines(&self.child);
    }
    fn paint(
        &mut self,
        _: &mut PaintCtx<'_>,
        _: &PropertiesRef<'_>,
        _: &mut masonry::imaging::Painter<'_>,
    ) {
    }
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

fn strip_height(width: f64, count: usize) -> f64 {
    let gap = crate::application::view::design::MARK_SWATCH_GAP;
    ((width - gap * (count + 1) as f64) / count.max(1) as f64).max(1.0) + 2.0 * gap
}

pub(crate) struct SwatchStripView<V>(V, usize);
pub(crate) fn swatch_strip<V>(input: V, count: usize) -> SwatchStripView<V> {
    SwatchStripView(input, count)
}
/// A swatch drawing whose intrinsic minimum does not force Canvas's 100px fallback.
pub(crate) fn swatch_face<V>(face: V) -> SwatchStripView<V> {
    SwatchStripView(face, 0)
}

impl<V> ViewMarker for SwatchStripView<V> {}
impl<V> View<crate::application::workspace::Workspace, (), ViewCtx> for SwatchStripView<V>
where
    V: WidgetView<crate::application::workspace::Workspace, Widget: Sized>,
{
    type Element = Pod<SwatchStrip<V::Widget>>;
    type ViewState = V::ViewState;
    fn build(
        &self,
        ctx: &mut ViewCtx,
        state: &mut crate::application::workspace::Workspace,
    ) -> (Self::Element, Self::ViewState) {
        let (child, state) = self.0.build(ctx, state);
        (
            ctx.create_pod(SwatchStrip {
                child: child.new_widget.to_pod(),
                count: self.1,
            }),
            state,
        )
    }
    fn rebuild(
        &self,
        prev: &Self,
        state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut crate::application::workspace::Workspace,
    ) {
        let mut child = SwatchStrip::child_mut(&mut element);
        self.0
            .rebuild(&prev.0, state, ctx, child.reborrow_mut(), app);
        drop(child);
        if self.1 != prev.1 {
            element.widget.count = self.1;
            element.ctx.request_layout();
        }
    }
    fn teardown(
        &self,
        state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        self.0
            .teardown(state, ctx, SwatchStrip::child_mut(&mut element));
    }
    fn message(
        &self,
        state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut crate::application::workspace::Workspace,
    ) -> MessageResult<()> {
        self.0
            .message(state, message, SwatchStrip::child_mut(&mut element), app)
    }
}
