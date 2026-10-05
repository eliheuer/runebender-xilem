// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Paint the window ground in a wrapper's padding, around the rounded content it wraps.
//!
//! The ground (the window color, or its tint over the native macOS blur) belongs only in the
//! gutters between floating panels. Painting it once beneath the whole window would also tint
//! the area under every panel, so a frosted side panel would sit on the tint instead of on the
//! blur. Each wrapper here paints only its own padding and the corners outside its child's
//! rounded shape; nested wrappers together cover every gutter and nothing under a panel.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, FromDynWidget, LayoutCtx, MeasureCtx, PaintCtx, PropertiesRef,
    RegisterCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Insets, Point, Rect, Shape as _, Size};
use masonry::layout::{LayoutSize, LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx, WidgetView};

pub(crate) struct GroundWidget<W: Widget + FromDynWidget + ?Sized> {
    child: WidgetPod<W>,
    color: Color,
    insets: Insets,
    panel_radius: Option<f64>,
    child_rect: Rect,
}

impl<W: Widget + FromDynWidget + ?Sized> GroundWidget<W> {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl<W: Widget + FromDynWidget + ?Sized> Widget for GroundWidget<W> {
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
        request: LenReq,
        cross: Option<Length>,
    ) -> Length {
        let (along, across) = match axis {
            Axis::Horizontal => (
                self.insets.x0 + self.insets.x1,
                self.insets.y0 + self.insets.y1,
            ),
            Axis::Vertical => (
                self.insets.y0 + self.insets.y1,
                self.insets.x0 + self.insets.x1,
            ),
        };
        let cross = cross.map(|cross| cross.saturating_sub(Length::px(across)));
        let child = ctx.compute_length(
            &mut self.child,
            request.reduce(Length::px(along)).into(),
            LayoutSize::maybe(axis.cross(), cross),
            axis,
            cross,
        );
        child.saturating_add(Length::px(along))
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        let insets = self.insets;
        let child = Size::new(
            (size.width - insets.x0 - insets.x1).max(0.0),
            (size.height - insets.y0 - insets.y1).max(0.0),
        );
        ctx.run_layout(&mut self.child, child);
        let origin = Point::new(insets.x0, insets.y0);
        ctx.place_child(&mut self.child, origin);
        ctx.derive_baselines(&self.child);
        // Masonry aligns a placed child by rounding its origin and end point separately.
        // Cutting out exactly that box makes neighboring grounds meet without overlapping.
        self.child_rect = Rect::from_points(origin.round(), (origin + child.to_vec2()).round());
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        if self.color.components[3] == 0.0 {
            return;
        }
        let rect = self.child_rect;
        let hole = match self.panel_radius {
            // A panel's rounded shape stays open, tucked half a pixel inside its edge so the
            // panel's own outline covers the seam instead of an antialiased gap.
            Some(radius) => {
                let half = 0.5_f64.min(rect.width() / 2.0).min(rect.height() / 2.0);
                rect.inset(-half)
                    .to_rounded_rect((radius - half).max(0.0))
                    .to_path(0.1)
            }
            // Another ground fills its whole box, so this one stops exactly at its edge.
            None => rect.to_path(0.1),
        };
        let mut ground = ctx.border_box().to_path(0.1);
        ground.extend(hole.reverse_subpaths().elements().iter().copied());
        painter.fill(&ground, self.color).draw();
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

/// Wrap `content` in `insets` of padding and paint `color` there, never beneath it.
///
/// With `panel_radius`, the content is an outlined panel with that corner radius, and the
/// ground also fills the corners outside its rounded shape. Without it, the content fills its
/// whole box, such as another ground.
pub(crate) fn ground<V>(
    content: V,
    color: Color,
    insets: Insets,
    panel_radius: Option<f64>,
) -> Ground<V> {
    Ground {
        content,
        color,
        insets,
        panel_radius,
    }
}

pub(crate) struct Ground<V> {
    content: V,
    color: Color,
    insets: Insets,
    panel_radius: Option<f64>,
}

impl<V> ViewMarker for Ground<V> {}
impl<State: 'static, V: WidgetView<State>> View<State, (), ViewCtx> for Ground<V> {
    type Element = Pod<GroundWidget<V::Widget>>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, state) = self.content.build(ctx, app);
        (
            ctx.create_pod(GroundWidget {
                child: child.new_widget.to_pod(),
                color: self.color,
                insets: self.insets,
                panel_radius: self.panel_radius,
                child_rect: Rect::ZERO,
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
        if self.insets != prev.insets {
            element.widget.insets = self.insets;
            element.ctx.request_layout();
        }
        if self.color != prev.color || self.panel_radius != prev.panel_radius {
            element.widget.color = self.color;
            element.widget.panel_radius = self.panel_radius;
            element.ctx.request_paint_only();
        }
        self.content.rebuild(
            &prev.content,
            state,
            ctx,
            GroundWidget::child_mut(&mut element),
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
            .teardown(state, ctx, GroundWidget::child_mut(&mut element));
    }

    fn message(
        &self,
        state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut State,
    ) -> MessageResult<()> {
        self.content
            .message(state, message, GroundWidget::child_mut(&mut element), app)
    }
}

#[cfg(test)]
mod tests {
    use super::ground;
    use masonry::kurbo::Insets;
    use masonry::layout::Dim;
    use masonry::properties::Dimensions;
    use masonry_testing::{TestHarness, TestHarnessParams};
    use std::sync::Arc;
    use xilem::core::{ProxyError, RawProxy, SendMessage, View, ViewId};
    use xilem::style::Style as _;
    use xilem::view::{label, sized_box};
    use xilem::{Color, ViewCtx};

    #[derive(Debug)]
    struct NoProxy;
    impl RawProxy for NoProxy {
        fn send_message(&self, _: Arc<[ViewId]>, _: SendMessage) -> Result<(), ProxyError> {
            Ok(())
        }
        fn dyn_debug(&self) -> &dyn std::fmt::Debug {
            self
        }
    }

    #[test]
    fn ground_fills_the_padding_and_corners_but_never_beneath_the_content() {
        let view = ground(
            sized_box(label("")).dims(Dimensions::new(Dim::Stretch, Dim::Stretch)),
            Color::from_rgb8(255, 0, 0),
            Insets::new(10.0, 10.0, 10.0, 10.0),
            Some(12.0),
        );
        let mut ctx = ViewCtx::new(
            Arc::new(NoProxy),
            Arc::new(
                tokio::runtime::Builder::new_current_thread()
                    .build()
                    .unwrap(),
            ),
        );
        let (pod, _) = View::<(), (), ViewCtx>::build(&view, &mut ctx, &mut ());
        let mut harness = TestHarness::create_with(
            crate::application::view::default_property_set(),
            pod.new_widget,
            TestHarnessParams::default()
                .with_size((100, 100))
                .with_background(Color::from_rgb8(0, 0, 255)),
        );
        let image = harness.render();
        let red = [255, 0, 0, 255];
        assert_eq!(image.get_pixel(5, 50).0, red, "the padding is ground");
        assert_eq!(
            image.get_pixel(11, 11).0,
            red,
            "outside the rounded corner is ground"
        );
        let center = image.get_pixel(50, 50).0;
        assert_eq!(
            &center[..3],
            &[0, 0, 255],
            "nothing is painted beneath the content"
        );
    }
}
