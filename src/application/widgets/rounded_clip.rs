// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Clip a whole widget subtree to a rounded panel without painting over the backdrop.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, FromDynWidget, LayoutCtx, MeasureCtx, PaintCtx, PropertiesRef,
    RegisterCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

pub(crate) struct RoundedClipWidget<W: Widget + FromDynWidget + ?Sized> {
    child: WidgetPod<W>,
    radius: f64,
}

impl<W: Widget + FromDynWidget + ?Sized> RoundedClipWidget<W> {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl<W: Widget + FromDynWidget + ?Sized> Widget for RoundedClipWidget<W> {
    type Action = ();

    fn accepts_pointer_interaction(&self) -> bool {
        false
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        cross: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.child, axis, cross)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.derive_baselines(&self.child);
        ctx.set_clip_path(size.to_rect());
    }

    fn pre_paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _: &PropertiesRef<'_>,
        painter: &mut masonry::imaging::Painter<'_>,
    ) {
        // Masonry appends pre-paint before the children and post-paint afterward.
        // Keep the clip open across that subtree instead of covering corners with a flat color.
        let size = ctx.content_box().size();
        let radius = self.radius.clamp(0.0, size.width.min(size.height) / 2.0);
        painter.push_fill_clip(size.to_rect().to_rounded_rect(radius));
    }

    fn post_paint(
        &mut self,
        _: &mut PaintCtx<'_>,
        _: &PropertiesRef<'_>,
        painter: &mut masonry::imaging::Painter<'_>,
    ) {
        painter.pop_clip();
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

pub(crate) struct RoundedClip<V> {
    content: V,
    radius: f64,
}

pub(crate) fn rounded_clip<V>(content: V, radius: f64) -> RoundedClip<V> {
    RoundedClip { content, radius }
}

impl<V> ViewMarker for RoundedClip<V> {}

impl<State: 'static, V: WidgetView<State>> View<State, (), ViewCtx> for RoundedClip<V> {
    type Element = Pod<RoundedClipWidget<V::Widget>>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, state) = self.content.build(ctx, app);
        (
            ctx.create_pod(RoundedClipWidget {
                child: child.new_widget.to_pod(),
                radius: self.radius,
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
        app: &mut State,
    ) {
        if self.radius != prev.radius {
            element.widget.radius = self.radius;
            element.ctx.request_layout();
        }
        self.content.rebuild(
            &prev.content,
            state,
            ctx,
            RoundedClipWidget::child_mut(&mut element),
            app,
        );
    }

    fn teardown(
        &self,
        state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        self.content
            .teardown(state, ctx, RoundedClipWidget::child_mut(&mut element));
    }

    fn message(
        &self,
        state: &mut Self::ViewState,
        ctx: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut State,
    ) -> MessageResult<()> {
        self.content
            .message(state, ctx, RoundedClipWidget::child_mut(&mut element), app)
    }
}
