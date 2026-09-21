// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Stock Masonry sliders with pointer-gesture boundaries for one-step editor undo.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, PropertySet, RegisterCtx,
    TextEvent, Widget, WidgetMut, WidgetPod,
};
use masonry::kurbo::{Axis, Circle, Point, Size, Stroke};
use masonry::layout::{LenReq, Length};
use masonry::properties::{BorderColor, ThumbColor, ThumbRadius, TrackColor};
use masonry::widgets::{Slider, SliderMoved};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

use crate::application::view::design::SLIDER_THUMB_RADIUS;
use crate::application::view::theme::Palette;
use crate::application::workspace::Workspace;

#[derive(Debug, PartialEq)]
pub(crate) struct GestureEnded {
    cancelled: bool,
}

pub(crate) struct GestureSliderWidget {
    child: WidgetPod<Slider>,
    pointer_down: bool,
    value_from_pointer: bool,
    accessibility_name: Option<String>,
    min: f64,
    max: f64,
    value: f64,
    thumb_outline: Color,
}

impl GestureSliderWidget {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, Slider> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    fn finish_gesture(&mut self) {
        self.value_from_pointer = false;
    }
}

impl Widget for GestureSliderWidget {
    type Action = GestureEnded;

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
    }

    fn paint(
        &mut self,
        _: &mut PaintCtx<'_>,
        _: &PropertiesRef<'_>,
        _: &mut masonry::imaging::Painter<'_>,
    ) {
    }

    fn post_paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _: &PropertiesRef<'_>,
        painter: &mut masonry::imaging::Painter<'_>,
    ) {
        let progress =
            ((self.value - self.min) / (self.max - self.min).max(f64::EPSILON)).clamp(0.0, 1.0);
        let width = ctx.content_box().width();
        let thumb_x = SLIDER_THUMB_RADIUS + progress * (width - SLIDER_THUMB_RADIUS * 2.0).max(0.0);
        let thumb = Circle::new(
            (thumb_x, ctx.content_box().height() / 2.0),
            SLIDER_THUMB_RADIUS - 1.0,
        );
        let outline = if ctx.is_disabled() {
            self.thumb_outline.with_alpha(0.4)
        } else {
            self.thumb_outline
        };
        painter.stroke(thumb, &Stroke::new(2.0), outline).draw();
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if ctx.is_disabled() {
            return;
        }
        // The stock slider retains capture, focus, values, painting and accessibility.
        // Its Down action is queued before this bubbled event; the view reads these
        // flags only after dispatch completes, so the first value joins the gesture.
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary) | None,
                ..
            }) => {
                self.pointer_down = true;
                self.value_from_pointer = true;
            }
            PointerEvent::Move(_) if self.pointer_down => {
                self.value_from_pointer = true;
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary) | None,
                ..
            }) if self.pointer_down => {
                self.pointer_down = false;
                ctx.submit_action::<GestureEnded>(GestureEnded { cancelled: false });
            }
            PointerEvent::Cancel(_) if self.pointer_down => {
                self.pointer_down = false;
                ctx.submit_action::<GestureEnded>(GestureEnded { cancelled: true });
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        _: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if matches!(event, TextEvent::Keyboard(_)) {
            self.value_from_pointer = false;
        }
    }

    fn on_access_event(
        &mut self,
        _: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        _: &AccessEvent,
    ) {
        self.value_from_pointer = false;
    }

    fn accessibility_role(&self) -> Role {
        Role::Group
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        if let Some(name) = &self.accessibility_name {
            node.set_label(name.clone());
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

pub(crate) struct GestureSliderView<F, E> {
    min: f64,
    max: f64,
    value: f64,
    step: Option<f64>,
    disabled: bool,
    accessibility_name: Option<String>,
    track: Color,
    thumb: Color,
    thumb_outline: Color,
    on_change: F,
    on_end: E,
}

/// Make a neutral slider whose pointer values share one editor gesture.
/// `on_change` receives true for pointer values, false for keyboard/accessibility.
/// `on_end` receives true on pointer cancellation and false on release.
pub(crate) fn gesture_slider<F, E>(
    pal: &Palette,
    min: f64,
    max: f64,
    value: f64,
    on_change: F,
    on_end: E,
) -> GestureSliderView<F, E>
where
    F: Fn(&mut Workspace, f64, bool) + Send + Sync + 'static,
    E: Fn(&mut Workspace, bool) + Send + Sync + 'static,
{
    GestureSliderView {
        min,
        max,
        value,
        step: None,
        disabled: false,
        accessibility_name: None,
        track: pal.slider_track(),
        thumb: pal.button,
        thumb_outline: pal.handle_line,
        on_change,
        on_end,
    }
}

impl<F, E> GestureSliderView<F, E> {
    /// Label the accessibility group containing the stock slider.
    pub(crate) fn accessibility_name(mut self, name: impl Into<String>) -> Self {
        self.accessibility_name = Some(name.into());
        self
    }

    pub(crate) fn step(mut self, step: f64) -> Self {
        self.step = (step.is_finite() && step > 0.0).then_some(step);
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl<F, E> ViewMarker for GestureSliderView<F, E> {}

impl<F, E> View<Workspace, (), ViewCtx> for GestureSliderView<F, E>
where
    F: Fn(&mut Workspace, f64, bool) + Send + Sync + 'static,
    E: Fn(&mut Workspace, bool) + Send + Sync + 'static,
{
    type Element = Pod<GestureSliderWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut Workspace) -> (Self::Element, Self::ViewState) {
        let mut slider = Slider::new(self.min, self.max, self.value);
        if let Some(step) = self.step {
            slider = slider.with_step(step);
        }
        let mut props = PropertySet::new();
        props.insert(TrackColor {
            active: self.track,
            inactive: self.track,
        });
        props.insert(ThumbColor(self.thumb));
        props.insert(ThumbRadius(Length::px(SLIDER_THUMB_RADIUS)));
        props.insert(BorderColor {
            color: Color::TRANSPARENT,
        });
        let child = Pod::new_with_props(slider, props);
        ctx.record_action_source(child.new_widget.id());
        let mut pod = ctx.with_action_widget(|ctx| {
            ctx.create_pod(GestureSliderWidget {
                child: child.new_widget.to_pod(),
                pointer_down: false,
                value_from_pointer: false,
                accessibility_name: self.accessibility_name.clone(),
                min: self.min,
                max: self.max,
                value: self.value,
                thumb_outline: self.thumb_outline,
            })
        });
        pod.new_widget.options.disabled = self.disabled;
        (pod, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        _: &mut Self::ViewState,
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut Workspace,
    ) {
        if self.disabled != prev.disabled {
            element.ctx.set_disabled(self.disabled);
        }
        if self.accessibility_name != prev.accessibility_name {
            element.widget.accessibility_name = self.accessibility_name.clone();
            element.ctx.request_accessibility_update();
        }
        if self.min != prev.min
            || self.max != prev.max
            || self.value != prev.value
            || self.thumb_outline != prev.thumb_outline
        {
            element.widget.min = self.min;
            element.widget.max = self.max;
            element.widget.value = self.value;
            element.widget.thumb_outline = self.thumb_outline;
            element.ctx.request_post_paint();
        }
        let mut child = GestureSliderWidget::child_mut(&mut element);
        if self.min != prev.min || self.max != prev.max {
            Slider::set_range(&mut child, self.min, self.max);
        }
        if self.step != prev.step {
            Slider::set_step(&mut child, self.step);
        }
        if self.value != prev.value {
            Slider::set_value(&mut child, self.value);
        }
        if self.track != prev.track {
            child.insert_prop(TrackColor {
                active: self.track,
                inactive: self.track,
            });
        }
        if self.thumb != prev.thumb {
            child.insert_prop(ThumbColor(self.thumb));
        }
    }

    fn teardown(
        &self,
        _: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.teardown_action_source(GestureSliderWidget::child_mut(&mut element));
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        _: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        app: &mut Workspace,
    ) -> MessageResult<()> {
        if message.take_first().is_some() {
            return MessageResult::Stale;
        }
        if let Some(value) = message.take_message::<SliderMoved>() {
            (self.on_change)(app, value.value, element.widget.value_from_pointer);
            MessageResult::Action(())
        } else if let Some(end) = message.take_message::<GestureEnded>() {
            // Clear only after all earlier SliderMoved actions have been delivered.
            element.widget.finish_gesture();
            (self.on_end)(app, end.cancelled);
            MessageResult::Action(())
        } else {
            MessageResult::Stale
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::accesskit::{Action, ActionRequest, TreeId};
    use masonry::core::keyboard::{Key, NamedKey};
    use masonry_testing::{PRIMARY_MOUSE, TestHarness};

    fn harness() -> TestHarness<GestureSliderWidget> {
        TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            GestureSliderWidget {
                child: Slider::new(0.0, 100.0, 25.0)
                    .with_step(5.0)
                    .prepare()
                    .to_pod(),
                pointer_down: false,
                value_from_pointer: false,
                accessibility_name: Some("Radius".into()),
                min: 0.0,
                max: 100.0,
                value: 25.0,
                thumb_outline: Color::BLACK,
            }
            .prepare(),
            (200, 32),
        )
    }

    #[test]
    fn pointer_values_begin_with_down_and_end_once_on_release() {
        let mut harness = harness();
        harness.mouse_move(Point::new(100.0, 16.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        assert_eq!(harness.pop_action::<SliderMoved>().unwrap().0.value, 50.0);
        assert!(harness.edit_root_widget(|root| root.widget.value_from_pointer));
        harness.mouse_move(Point::new(150.0, 16.0));
        harness.mouse_button_release(Some(PointerButton::Primary));
        assert_eq!(harness.pop_action::<SliderMoved>().unwrap().0.value, 75.0);
        assert!(harness.edit_root_widget(|root| root.widget.value_from_pointer));
        assert_eq!(
            harness.pop_action::<GestureEnded>().unwrap().0,
            GestureEnded { cancelled: false }
        );
        harness.edit_root_widget(|root| root.widget.finish_gesture());
        assert!(!harness.edit_root_widget(|root| root.widget.value_from_pointer));
        assert!(harness.pop_action_erased().is_none());
    }

    #[test]
    fn pointer_cancel_reports_cancellation_without_an_extra_release() {
        let mut harness = harness();
        harness.mouse_move(Point::new(100.0, 16.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        assert!(harness.pop_action::<SliderMoved>().is_some());
        harness.process_pointer_event(PointerEvent::Cancel(PRIMARY_MOUSE));
        assert_eq!(
            harness.pop_action::<GestureEnded>().unwrap().0,
            GestureEnded { cancelled: true }
        );
        harness.edit_root_widget(|root| root.widget.finish_gesture());
        harness.mouse_button_release(Some(PointerButton::Primary));
        assert!(harness.pop_action_erased().is_none());
    }

    #[test]
    fn stock_keyboard_and_accessibility_values_are_discrete_edits() {
        let mut harness = harness();
        let slider = harness.edit_root_widget(|root| root.widget.child.id());
        harness.focus_on(Some(slider));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowRight)));
        assert_eq!(harness.pop_action::<SliderMoved>().unwrap().0.value, 30.0);
        assert!(!harness.edit_root_widget(|root| root.widget.value_from_pointer));
        harness.process_access_event(ActionRequest {
            action: Action::SetValue,
            target_tree: TreeId::ROOT,
            target_node: slider.into(),
            data: Some(masonry::accesskit::ActionData::NumericValue(80.0)),
        });
        assert_eq!(harness.pop_action::<SliderMoved>().unwrap().0.value, 80.0);
        assert!(!harness.edit_root_widget(|root| root.widget.value_from_pointer));
        assert!(harness.pop_action_erased().is_none());
    }
}
