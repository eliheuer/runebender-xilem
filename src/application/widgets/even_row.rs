// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A row of equal slots with whole-pixel widths and exact gaps.
//!
//! A flex row gives equal children fractional widths. Masonry then rounds each
//! child's position and width separately, so neighboring gaps drift by a pixel.
//! This row rounds the slot widths itself and gives any leftover pixels to the
//! first slots, so every gap and outer inset keeps its exact width.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, PaintCtx, PropertiesRef, RegisterCtx, Widget,
    WidgetPod,
};
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenDef, LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::{Pod, ViewCtx, WidgetView};

/// Whole-pixel slot widths for `count` slots sharing `width` after the insets and gaps.
fn slot_widths(width: f64, count: usize, inset: f64, gap: f64) -> Vec<f64> {
    if count == 0 {
        return Vec::new();
    }
    let slots = count as f64;
    let available = (width - 2.0 * inset - gap * (slots - 1.0)).max(0.0).floor();
    let base = (available / slots).floor();
    let mut leftover = available - base * slots;
    (0..count)
        .map(|_| {
            if leftover >= 1.0 {
                leftover -= 1.0;
                base + 1.0
            } else {
                base
            }
        })
        .collect()
}

pub(crate) struct EvenRowWidget<W: Widget> {
    children: Vec<WidgetPod<W>>,
    inset: f64,
    gap: f64,
}

impl<W: Widget> Widget for EvenRowWidget<W> {
    type Action = ();

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for child in &mut self.children {
            ctx.register_child(child);
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        request: LenReq,
        cross: Option<Length>,
    ) -> Length {
        if let LenReq::FitContent(space) = request {
            return space;
        }
        let count = self.children.len() as f64;
        let insets = 2.0 * self.inset + self.gap * (count - 1.0).max(0.0);
        if axis == Axis::Horizontal {
            return Length::px(insets);
        }
        let width = cross.map(|width| (width.get() - insets).max(0.0) / count.max(1.0));
        let tallest = self
            .children
            .iter_mut()
            .map(|child| {
                ctx.compute_length(
                    child,
                    request.into(),
                    LayoutSize::maybe(Axis::Horizontal, width.map(Length::px)),
                    Axis::Vertical,
                    width.map(Length::px),
                )
                .get()
            })
            .fold(0.0, f64::max);
        Length::px(self.inset + tallest)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        let widths = slot_widths(size.width, self.children.len(), self.inset, self.gap);
        let height = (size.height - self.inset).max(0.0);
        let mut x = self.inset;
        for (child, width) in self.children.iter_mut().zip(widths) {
            let child_height = ctx
                .compute_length(
                    child,
                    LenDef::Fixed(Length::px(height)),
                    LayoutSize::new(Length::px(width), Length::px(height)),
                    Axis::Vertical,
                    Some(Length::px(width)),
                )
                .get();
            ctx.run_layout(child, Size::new(width, child_height));
            ctx.place_child(child, Point::new(x, self.inset));
            x += width + self.gap;
        }
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
        self.children.iter().map(WidgetPod::id).collect()
    }
}

/// Lay out `children` as equal slots with `inset` above and beside them and `gap` between.
pub(crate) fn even_row<V>(children: Vec<V>, inset: f64, gap: f64) -> EvenRow<V> {
    EvenRow {
        children,
        inset,
        gap,
    }
}

pub(crate) struct EvenRow<V> {
    children: Vec<V>,
    inset: f64,
    gap: f64,
}

/// Child view states, plus a generation that retires messages from replaced children.
pub(crate) struct EvenRowState<S> {
    children: Vec<S>,
    generation: u32,
}

fn child_id(generation: u32, index: usize) -> ViewId {
    ViewId::new((u64::from(generation) << 32) | index as u64)
}

impl<V> EvenRow<V> {
    fn build_children<State: 'static>(
        &self,
        ctx: &mut ViewCtx,
        app: &mut State,
        generation: u32,
    ) -> (Vec<WidgetPod<V::Widget>>, Vec<V::ViewState>)
    where
        V: WidgetView<State, Widget: Sized>,
    {
        self.children
            .iter()
            .enumerate()
            .map(|(index, child)| {
                let (pod, state) =
                    ctx.with_id(child_id(generation, index), |ctx| child.build(ctx, app));
                (pod.new_widget.to_pod(), state)
            })
            .unzip()
    }
}

impl<V> ViewMarker for EvenRow<V> {}
impl<State: 'static, V> View<State, (), ViewCtx> for EvenRow<V>
where
    V: WidgetView<State, Widget: Sized>,
{
    type Element = Pod<EvenRowWidget<V::Widget>>;
    type ViewState = EvenRowState<V::ViewState>;

    fn build(&self, ctx: &mut ViewCtx, app: &mut State) -> (Self::Element, Self::ViewState) {
        let (children, states) = self.build_children(ctx, app, 0);
        (
            ctx.create_pod(EvenRowWidget {
                children,
                inset: self.inset,
                gap: self.gap,
            }),
            EvenRowState {
                children: states,
                generation: 0,
            },
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
        if self.inset != prev.inset || self.gap != prev.gap {
            element.widget.inset = self.inset;
            element.widget.gap = self.gap;
            element.ctx.request_layout();
        }
        if self.children.len() != prev.children.len() {
            // A changed tab set is rare; replace every child under a new generation.
            self.teardown(state, ctx, element.reborrow_mut());
            state.generation = state.generation.wrapping_add(1);
            let (children, states) = self.build_children(ctx, app, state.generation);
            element.widget.children = children;
            element.ctx.children_changed();
            state.children = states;
            return;
        }
        for (index, ((child, prev_child), child_state)) in self
            .children
            .iter()
            .zip(&prev.children)
            .zip(&mut state.children)
            .enumerate()
        {
            let mut pod = element.ctx.get_mut(&mut element.widget.children[index]);
            ctx.with_id(child_id(state.generation, index), |ctx| {
                child.rebuild(prev_child, child_state, ctx, pod.reborrow_mut(), app);
            });
        }
    }

    fn teardown(
        &self,
        state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let children = std::mem::take(&mut element.widget.children);
        for (index, ((child, child_state), mut pod)) in self
            .children
            .iter()
            .zip(&mut state.children)
            .zip(children)
            .enumerate()
        {
            ctx.with_id(child_id(state.generation, index), |ctx| {
                child.teardown(child_state, ctx, element.ctx.get_mut(&mut pod));
            });
            element.ctx.remove_child(pod);
        }
        state.children.clear();
    }

    fn message(
        &self,
        state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut State,
    ) -> MessageResult<()> {
        let Some(first) = message.take_first() else {
            return MessageResult::Stale;
        };
        let raw = first.routing_id();
        if raw >> 32 != u64::from(state.generation) {
            return MessageResult::Stale;
        }
        let Ok(index) = usize::try_from(raw & u64::from(u32::MAX)) else {
            return MessageResult::Stale;
        };
        let (Some(child), Some(child_state), Some(pod)) = (
            self.children.get(index),
            state.children.get_mut(index),
            element.widget.children.get_mut(index),
        ) else {
            return MessageResult::Stale;
        };
        let pod = element.ctx.get_mut(pod);
        child.message(child_state, message, pod, app)
    }
}

#[cfg(test)]
mod tests {
    use super::{EvenRowWidget, child_id, even_row, slot_widths};
    use crate::application::widgets::icon_button::{IconClicked, IconWidget, icon_button};
    use masonry_testing::TestHarness;
    use std::sync::Arc;
    use xilem::core::{
        DynMessage, Environment, MessageCtx, MessageResult, ProxyError, RawProxy, SendMessage,
        View, ViewId,
    };
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

    /// Deliver a click message along `id`, as the Xilem driver does for a widget action.
    fn send<V>(
        harness: &mut TestHarness<EvenRowWidget<IconWidget>>,
        view: &V,
        state: &mut V::ViewState,
        clicked: &mut Option<usize>,
        id: ViewId,
    ) -> MessageResult<()>
    where
        V: View<Option<usize>, (), ViewCtx, Element = xilem::Pod<EvenRowWidget<IconWidget>>>,
    {
        let mut message =
            MessageCtx::new(Environment::new(), vec![id], DynMessage::new(IconClicked));
        harness.edit_root_widget(|root| view.message(state, &mut message, root, clicked))
    }

    #[test]
    fn clicks_reach_the_tab_they_target_after_the_tab_set_changes() {
        let tab = |index: usize| {
            icon_button(
                "text",
                false,
                Color::BLACK,
                Color::BLACK,
                Color::TRANSPARENT,
                Color::TRANSPARENT,
                move |clicked: &mut Option<usize>| *clicked = Some(index),
            )
        };
        let mut ctx = ViewCtx::new(
            Arc::new(NoProxy),
            Arc::new(
                tokio::runtime::Builder::new_current_thread()
                    .build()
                    .unwrap(),
            ),
        );
        let mut clicked = None;
        let three = even_row((0..3).map(tab).collect(), 6.0, 6.0);
        let (pod, mut state) = three.build(&mut ctx, &mut clicked);
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (200, 40),
        );
        assert!(matches!(
            send(
                &mut harness,
                &three,
                &mut state,
                &mut clicked,
                child_id(0, 2)
            ),
            MessageResult::Action(())
        ));
        assert_eq!(clicked, Some(2));

        let four = even_row((10..14).map(tab).collect(), 6.0, 6.0);
        harness.edit_root_widget(|root| {
            four.rebuild(&three, &mut state, &mut ctx, root, &mut clicked);
        });
        // A click from the replaced tab set must not reach a new tab at the same index.
        assert!(matches!(
            send(
                &mut harness,
                &four,
                &mut state,
                &mut clicked,
                child_id(0, 2)
            ),
            MessageResult::Stale
        ));
        assert!(matches!(
            send(
                &mut harness,
                &four,
                &mut state,
                &mut clicked,
                child_id(1, 3)
            ),
            MessageResult::Action(())
        ));
        assert_eq!(clicked, Some(13));
    }

    #[test]
    fn neighbors_keep_exact_gaps_when_the_width_does_not_divide_evenly() {
        use masonry::layout::{Dim, Length};
        use masonry::properties::Dimensions;
        use xilem::style::Style as _;
        use xilem::view::{label, sized_box};

        let slot = || {
            sized_box(label("")).dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(25.0))))
        };
        let view = even_row((0..5).map(|_| slot()).collect(), 6.0, 6.0);
        let mut ctx = ViewCtx::new(
            Arc::new(NoProxy),
            Arc::new(
                tokio::runtime::Builder::new_current_thread()
                    .build()
                    .unwrap(),
            ),
        );
        let (pod, _) = View::<(), (), ViewCtx>::build(&view, &mut ctx, &mut ());
        // 244 - 12 inset - 24 gaps leaves 208, which five slots cannot share evenly.
        let harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (244, 38),
        );
        let rects: Vec<_> = harness
            .root_widget()
            .children()
            .iter()
            .map(|child| {
                let ctx = child.ctx();
                ctx.window_transform().transform_rect_bbox(ctx.border_box())
            })
            .collect();
        assert_eq!(rects.first().map(|r| (r.x0, r.y0)), Some((6.0, 6.0)));
        assert_eq!(rects.last().map(|r| r.x1), Some(238.0));
        for pair in rects.windows(2) {
            assert_eq!(pair[1].x0 - pair[0].x1, 6.0, "{rects:?}");
        }
    }

    #[test]
    fn slots_use_whole_pixels_and_fill_the_row_exactly() {
        for width in [244.0, 243.0, 300.5, 410.0] {
            for count in 1..=6 {
                let widths = slot_widths(width, count, 6.0, 6.0);
                assert!(widths.iter().all(|w| w.fract() == 0.0), "{width} {count}");
                let spread = widths.iter().copied().fold(f64::MIN, f64::max)
                    - widths.iter().copied().fold(f64::MAX, f64::min);
                assert!(spread <= 1.0, "{width} {count}: {widths:?}");
                let used: f64 = widths.iter().sum::<f64>() + 6.0 * (count as f64 + 1.0);
                assert!(width - used < 1.0 && used <= width, "{width} {count}");
            }
        }
    }
}
