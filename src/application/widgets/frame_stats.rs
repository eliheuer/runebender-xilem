// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Report the window's frame cadence on stderr while `RUNEBENDER_FRAME_STATS` is set.
//!
//! The wrapper asks for an animation frame on every frame, so the window renders and
//! presents continuously, as it does during a drag, and once a second it prints how many
//! frames the display delivered and how long the longest gap between two of them was. The
//! numbers come from the same animation clock the widgets use, so they include everything
//! the GPU and the compositor add: this is how a native backdrop or a wallpaper blur is
//! measured, which no headless benchmark can see.

use std::time::Instant;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, FromDynWidget, LayoutCtx, MeasureCtx, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

/// The environment variable that turns the report on.
pub(crate) const ENV: &str = "RUNEBENDER_FRAME_STATS";

pub(crate) struct FrameStatsWidget<W: Widget + FromDynWidget + ?Sized> {
    child: WidgetPod<W>,
    /// Nanoseconds between consecutive frames since the last report.
    intervals: Vec<u64>,
    reported: Instant,
}

impl<W: Widget + FromDynWidget + ?Sized> FrameStatsWidget<W> {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    fn report(&mut self) {
        let frames = self.intervals.len();
        if frames == 0 {
            return;
        }
        let total: u64 = self.intervals.iter().sum();
        let worst = self.intervals.iter().copied().max().unwrap_or_default();
        let mean = total / frames as u64;
        // A gap half again as long as the mean is a frame the display showed twice.
        let late = self
            .intervals
            .iter()
            .filter(|interval| **interval > mean + mean / 2)
            .count();
        let ms = |nanos: u64| nanos as f64 / 1e6;
        eprintln!(
            "frames {frames:>4}/s   mean {:>6.2} ms   worst {:>6.2} ms   late {late}",
            ms(mean),
            ms(worst),
        );
        self.intervals.clear();
    }
}

impl<W: Widget + FromDynWidget + ?Sized> Widget for FrameStatsWidget<W> {
    type Action = ();

    fn accepts_pointer_interaction(&self) -> bool {
        false
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::WidgetAdded) {
            ctx.request_anim_frame();
        }
    }

    fn on_anim_frame(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, interval: u64) {
        if interval > 0 {
            self.intervals.push(interval);
        }
        if self.reported.elapsed().as_secs() >= 1 {
            self.report();
            self.reported = Instant::now();
        }
        ctx.request_anim_frame();
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        request: LenReq,
        cross: Option<Length>,
    ) -> Length {
        ctx.compute_length(
            &mut self.child,
            request.into(),
            LayoutSize::maybe(axis.cross(), cross),
            axis,
            cross,
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.derive_baselines(&self.child);
    }

    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

/// Wrap `content` so the window renders every frame and reports its cadence on stderr.
pub(crate) fn frame_stats<V>(content: V) -> FrameStats<V> {
    FrameStats { content }
}

pub(crate) struct FrameStats<V> {
    content: V,
}

impl<V> ViewMarker for FrameStats<V> {}
impl<State: 'static, V: WidgetView<State>> View<State, (), ViewCtx> for FrameStats<V> {
    type Element = Pod<FrameStatsWidget<V::Widget>>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, state) = self.content.build(ctx, app);
        (
            ctx.create_pod(FrameStatsWidget {
                child: child.new_widget.to_pod(),
                intervals: Vec::with_capacity(256),
                reported: Instant::now(),
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
        self.content.rebuild(
            &prev.content,
            state,
            ctx,
            FrameStatsWidget::child_mut(&mut element),
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
            .teardown(state, ctx, FrameStatsWidget::child_mut(&mut element));
    }

    fn message(
        &self,
        state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut State,
    ) -> MessageResult<()> {
        self.content.message(
            state,
            message,
            FrameStatsWidget::child_mut(&mut element),
            app,
        )
    }
}
