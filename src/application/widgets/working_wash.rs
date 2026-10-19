// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A card that shows work in progress: the conversation card's keyline
//! around a soft two-tone wash that drifts slowly across it, so the panel
//! shows something is happening without a spinner or a bright color.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, FromDynWidget, LayoutCtx, MeasureCtx, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size, Stroke};
use masonry::layout::{LenReq, Length};
use masonry::peniko::{Color, Extend, Gradient};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

/// Seconds for the wash to travel one full cycle across the card.
const CYCLE_SECONDS: f64 = 2.4;

/// How the card looks; the motion is the widget's own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WashStyle {
    /// The two tones the wash moves between.
    pub(crate) tones: [Color; 2],
    pub(crate) outline: Color,
    pub(crate) stroke: f64,
    pub(crate) radius: f64,
}

pub(crate) struct WorkingWashWidget<W: Widget + FromDynWidget + ?Sized> {
    child: WidgetPod<W>,
    style: WashStyle,
    /// Where the wash is in its cycle, 0 to 1.
    phase: f64,
}

impl<W: Widget + FromDynWidget + ?Sized> WorkingWashWidget<W> {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl<W: Widget + FromDynWidget + ?Sized> Widget for WorkingWashWidget<W> {
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
        // Nanoseconds as f64 lose no precision that matters for a slow drift.
        #[expect(
            clippy::cast_precision_loss,
            reason = "frame intervals are far below f64's exact integer range"
        )]
        let seconds = interval as f64 / 1e9;
        self.phase = (self.phase + seconds / CYCLE_SECONDS).fract();
        ctx.request_paint_only();
        ctx.request_anim_frame();
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
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let size = ctx.border_box().size();
        let WashStyle {
            tones: [a, b],
            outline,
            stroke,
            radius,
        } = self.style;
        // The same concentric geometry as every keyline: the stroke's
        // centerline sits half a stroke inside the box with half a stroke
        // less radius, and the wash fills to that centerline beneath it.
        let half = stroke / 2.0;
        let shape = size
            .to_rect()
            .inset(-half)
            .to_rounded_rect((radius - half).max(0.0));
        // One wave, a tone out and back, as wide as the card, sliding by
        // the phase; the gradient repeats so the wave enters as it leaves.
        let span = size.width.max(1.0);
        let start = -span * self.phase;
        let wash = Gradient::new_linear((start, 0.0), (start + span, 0.0))
            .with_extend(Extend::Repeat)
            .with_stops([(0.0, a), (0.5, b), (1.0, a)]);
        painter.fill(shape, &wash).draw();
        painter.stroke(shape, &Stroke::new(stroke), outline).draw();
    }

    fn accessibility_role(&self) -> Role {
        Role::Status
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

pub(crate) struct WorkingWash<V> {
    content: V,
    style: WashStyle,
}

/// `content` on a card whose wash drifts while it is shown.
pub(crate) fn working_wash<V>(content: V, style: WashStyle) -> WorkingWash<V> {
    WorkingWash { content, style }
}

impl<V> ViewMarker for WorkingWash<V> {}

impl<State: 'static, V: WidgetView<State>> View<State, (), ViewCtx> for WorkingWash<V> {
    type Element = Pod<WorkingWashWidget<V::Widget>>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, state) = self.content.build(ctx, app);
        (
            ctx.create_pod(WorkingWashWidget {
                child: child.new_widget.to_pod(),
                style: self.style,
                phase: 0.0,
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
        if self.style != prev.style {
            element.widget.style = self.style;
            element.ctx.request_paint_only();
        }
        self.content.rebuild(
            &prev.content,
            state,
            ctx,
            WorkingWashWidget::child_mut(&mut element),
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
            .teardown(state, ctx, WorkingWashWidget::child_mut(&mut element));
    }

    fn message(
        &self,
        state: &mut Self::ViewState,
        ctx: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut State,
    ) -> MessageResult<()> {
        self.content
            .message(state, ctx, WorkingWashWidget::child_mut(&mut element), app)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry_testing::TestHarness;
    use xilem::style::Style as _;
    use xilem::view::{label, sized_box};

    fn style() -> WashStyle {
        WashStyle {
            tones: [
                Color::from_rgb8(0xb8, 0xc8, 0x98),
                Color::from_rgb8(0x98, 0xc8, 0xc0),
            ],
            outline: Color::from_rgb8(0x30, 0x30, 0x30),
            stroke: 1.0,
            radius: 4.0,
        }
    }

    #[test]
    fn the_wash_moves_between_frames() {
        let view = working_wash(
            sized_box(label("Thinking")).padding(crate::application::design::Space::Md),
            style(),
        );
        let mut ctx = crate::application::view::render::panel_resize_tests_support::context();
        let (pod, _) = View::<(), (), ViewCtx>::build(&view, &mut ctx, &mut ());
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (200, 40),
        );
        let first = harness.render();
        harness.animate_ms(600);
        let later = harness.render();
        assert_ne!(first, later, "the wash drifts while the card is shown");
    }
}
