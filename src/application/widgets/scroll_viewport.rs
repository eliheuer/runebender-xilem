// Copyright 2024 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Scroll without visible scrollbar overlays, retaining Masonry Portal behavior.
//!
//! The pinned Portal has no scrollbar-visibility policy. A zero-size `ScrollBar`
//! still paints an inverted cursor rectangle. Until upstream exposes a hidden
//! policy, move only its overlay widgets beyond Portal's clip after layout.
//! The content, viewport, wheel, focus-pan and accessibility-scroll handlers
//! remain the upstream implementation. This adapter can be deleted when that
//! policy is available; it does not fork Portal or change font/editor state.

use std::marker::PhantomData;

use masonry::widgets;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

/// A view which puts `child` into a scrollable region.
///
/// This corresponds to the Masonry [`Portal`](masonry::widgets::Portal) widget.
pub(crate) fn portal<Child, State, Action>(child: Child) -> Portal<Child, State, Action>
where
    State: 'static,
    Child: WidgetView<State, Action>,
{
    Portal {
        child,
        constrain_horizontal: false,
        constrain_vertical: false,
        must_fill: false,
        phantom: PhantomData,
    }
}

/// The [`View`] created by [`portal`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub(crate) struct Portal<V, State, Action> {
    child: V,
    constrain_horizontal: bool,
    constrain_vertical: bool,
    must_fill: bool,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, State, Action> Portal<V, State, Action> {
    /// Set horizontal constraining of the child.
    ///
    /// - When it is `false` (the default), the child does not receive any upper
    ///   bound on its width. The child can be as wide as it wants,
    ///   and the viewport gets moved around to see all of it.
    /// - When it is `true`, the [`Portal`]'s width will be passed down as an upper bound
    ///   on the width of the child. There will be no horizontal scrollbar and
    ///   the mouse wheel can't be used to horizontally scroll either.
    pub(crate) fn constrain_horizontal(mut self, constrain_horizontal: bool) -> Self {
        self.constrain_horizontal = constrain_horizontal;
        self
    }
}

impl<V, State, Action> ViewMarker for Portal<V, State, Action> {}
impl<Child, State, Action> View<State, Action, ViewCtx> for Portal<Child, State, Action>
where
    Child: WidgetView<State, Action>,
    State: 'static,
    Action: 'static,
{
    type Element = Pod<ScrollViewport<Child::Widget>>;
    type ViewState = Child::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        // The Portal `View` doesn't get any messages directly (yet - scroll events?), so doesn't need to
        // use ctx.with_id.
        let (child, child_state) = self.child.build(ctx, app_state);
        let widget_pod = ctx.create_pod(ScrollViewport::new(
            widgets::Portal::new(child.new_widget)
                .constrain_horizontal(self.constrain_horizontal)
                .constrain_vertical(self.constrain_vertical)
                .content_must_fill(self.must_fill),
        ));
        (widget_pod, child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        let mut element = ScrollViewport::portal_mut(&mut element);
        if self.constrain_horizontal != prev.constrain_horizontal {
            widgets::Portal::set_constrain_horizontal(&mut element, self.constrain_horizontal);
        }
        if self.constrain_vertical != prev.constrain_vertical {
            widgets::Portal::set_constrain_vertical(&mut element, self.constrain_vertical);
        }
        if self.must_fill != prev.must_fill {
            widgets::Portal::set_content_must_fill(&mut element, self.must_fill);
        }

        let child_element = widgets::Portal::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child_element, app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut element = ScrollViewport::portal_mut(&mut element);
        let child_element = widgets::Portal::child_mut(&mut element);
        self.child.teardown(view_state, ctx, child_element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let mut element = ScrollViewport::portal_mut(&mut element);
        let child_element = widgets::Portal::child_mut(&mut element);
        self.child
            .message(view_state, message, child_element, app_state)
    }
}

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, PaintCtx, PropertiesRef, RegisterCtx, Widget,
    WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Axis, Point, Size};
use masonry::layout::{LenReq, Length};

pub(crate) struct ScrollViewport<W: Widget + masonry::core::FromDynWidget + ?Sized> {
    inner: WidgetPod<widgets::Portal<W>>,
}

impl<W: Widget + masonry::core::FromDynWidget + ?Sized> ScrollViewport<W> {
    pub(crate) fn new(portal: widgets::Portal<W>) -> Self {
        Self {
            inner: WidgetPod::new(portal),
        }
    }

    pub(crate) fn portal_mut<'a>(
        this: &'a mut WidgetMut<'_, Self>,
    ) -> WidgetMut<'a, widgets::Portal<W>> {
        this.ctx.get_mut(&mut this.widget.inner)
    }
}

impl<W: Widget + masonry::core::FromDynWidget + ?Sized> Widget for ScrollViewport<W> {
    type Action = ();

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.inner);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        cross: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.inner, axis, cross)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.inner, size);
        ctx.place_child(&mut self.inner, Point::ORIGIN);
        ctx.derive_baselines(&self.inner);
        // Even tiny viewports must clear the cursor's minimum length and stroke.
        let clearance = masonry::theme::SCROLLBAR_MIN_SIZE
            + masonry::theme::SCROLLBAR_WIDTH
            + masonry::theme::SCROLLBAR_PAD * 2.0
            + masonry::theme::SCROLLBAR_EDGE_WIDTH;
        let outside = Affine::translate((size.width + clearance, size.height + clearance));
        ctx.mutate_child_later(&mut self.inner, move |mut portal| {
            let mut horizontal = widgets::Portal::horizontal_scrollbar_mut(&mut portal);
            horizontal.set_transform(outside);
            horizontal.ctx.set_disabled(true);
            drop(horizontal);
            let mut vertical = widgets::Portal::vertical_scrollbar_mut(&mut portal);
            vertical.set_transform(outside);
            vertical.ctx.set_disabled(true);
        });
    }

    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.inner.id()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::core::WindowEvent;
    use masonry::dpi::PhysicalSize;
    use masonry::kurbo::Vec2;
    use masonry::layout::AsUnit;
    use masonry::properties::Background;
    use masonry::widgets::SizedBox;
    use masonry_testing::TestHarness;

    #[test]
    fn active_scroll_and_resize_never_paint_bars() {
        let fill = masonry::peniko::Color::from_rgb8(40, 90, 130);
        let content = SizedBox::empty()
            .size(1000.px(), 1000.px())
            .prepare()
            .with_props(Background::Color(fill));
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            ScrollViewport::new(widgets::Portal::new(content)).prepare(),
            (200, 200),
        );
        for size in [200, 80, 180] {
            harness.process_window_event(WindowEvent::Resize(PhysicalSize::new(size, size)));
            // Exercise both overlay edges, where hover would otherwise expose them.
            for point in [
                (20.0, 20.0),
                (f64::from(size) - 1.0, 20.0),
                (20.0, f64::from(size) - 1.0),
            ] {
                harness.mouse_move(point);
                harness.mouse_wheel(Vec2::new(-10.0, -10.0));
                let rendered = harness.render();
                let expected = *rendered.get_pixel(size / 2, size / 2);
                assert!(
                    rendered
                        .enumerate_pixels()
                        .filter(|(x, y, _)| *x < size && *y < size)
                        .all(|(_, _, pixel)| *pixel == expected),
                    "scrollbar or gutter pixels appeared during active scrolling"
                );
            }
        }
        let position = harness.edit_root_widget(|mut viewport| {
            ScrollViewport::portal_mut(&mut viewport)
                .widget
                .get_viewport_pos()
        });
        assert!(
            position.x > 0.0 && position.y > 0.0,
            "the hidden overlays must not prevent two-axis scrolling"
        );
    }
}
