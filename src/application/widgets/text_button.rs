// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A push button named in words, with rest, hover and pressed faces.
//!
//! Masonry's button takes its hover and pressed styles from a property
//! stack that any locally set face overrides, so a themed face loses its
//! states. This widget paints all three from the palette instead. A click
//! lands on release inside the button, as on a native button.

use masonry::accesskit::{Action, Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx,
    Widget,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size, Stroke};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

use crate::application::view::design::{ControlSize, TextSize};
use crate::application::view::theme::Palette;
use crate::application::widgets::text_label::{self, Anchor};

/// Space between the text and each side edge.
const SIDE_INSET: f64 = 8.0;

#[derive(Debug)]
pub(crate) struct TextButtonClicked;

/// The colors of each state, taken from the palette.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Faces {
    rest: Color,
    hover: Color,
    outline: Color,
    ink: Color,
    radius: f64,
}

impl Faces {
    fn of(pal: &Palette) -> Self {
        Self {
            rest: pal.button,
            hover: pal.control,
            outline: pal.outline,
            ink: pal.editor_control_ink(),
            radius: pal.control_radius,
        }
    }
}

pub(crate) struct TextButtonWidget {
    text: &'static str,
    faces: Faces,
    height: f64,
}

impl TextButtonWidget {
    fn text_px() -> f32 {
        TextSize::Body.px()
    }
}

impl Widget for TextButtonWidget {
    type Action = TextButtonClicked;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross: Option<Length>,
    ) -> Length {
        match axis {
            Axis::Horizontal => Length::px(
                (text_label::width(self.text, Self::text_px()) + SIDE_INSET * 2.0).ceil(),
            ),
            Axis::Vertical => Length::px(self.height),
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let rect = ctx.border_box();
        let faces = self.faces;
        // Pressed inverts the face and the text, as a selection does.
        let (face, ink) = if ctx.is_active() && ctx.is_hovered() {
            (faces.ink, faces.rest)
        } else if ctx.is_hovered() {
            (faces.hover, faces.ink)
        } else {
            (faces.rest, faces.ink)
        };
        let shape = rect.inset(-0.5).to_rounded_rect(faces.radius);
        painter.fill(shape, face).draw();
        painter
            .stroke(shape, &Stroke::new(1.0), faces.outline)
            .draw();
        text_label::draw(
            painter,
            Point::new(rect.center().x, rect.center().y),
            self.text,
            Self::text_px(),
            ink,
            Anchor::Middle,
        );
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
                ..
            }) => {
                ctx.capture_pointer();
                ctx.set_handled();
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) => {
                if ctx.is_active() && ctx.is_hovered() {
                    ctx.submit_action::<TextButtonClicked>(TextButtonClicked);
                }
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::HoveredChanged(_) | Update::ActiveChanged(_)) {
            ctx.request_render();
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == Action::Click {
            ctx.submit_action::<TextButtonClicked>(TextButtonClicked);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.text);
        node.add_action(Action::Click);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

pub(crate) struct TextButtonView<F> {
    text: &'static str,
    faces: Faces,
    height: f64,
    on_click: F,
}

/// A push button showing `text`, at the icon control height.
pub(crate) fn text_button<State: 'static, F: Fn(&mut State) + 'static>(
    pal: &Palette,
    text: &'static str,
    on_click: F,
) -> TextButtonView<F> {
    TextButtonView {
        text,
        faces: Faces::of(pal),
        height: ControlSize::Icon.px(),
        on_click,
    }
}

impl<F> ViewMarker for TextButtonView<F> {}
impl<State: 'static, F: Fn(&mut State) + 'static> View<State, (), ViewCtx> for TextButtonView<F> {
    type Element = Pod<TextButtonWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = TextButtonWidget {
            text: self.text,
            faces: self.faces,
            height: self.height,
        };
        (ctx.with_action_widget(|ctx| ctx.create_pod(widget)), ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut el: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if self.text != prev.text || self.height != prev.height {
            el.widget.text = self.text;
            el.widget.height = self.height;
            el.ctx.request_layout();
        }
        if self.faces != prev.faces {
            el.widget.faces = self.faces;
            el.ctx.request_render();
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _el: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<()> {
        match message.take_message::<TextButtonClicked>() {
            Some(_) => {
                (self.on_click)(state);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::core::NewWidget;
    use masonry_testing::TestHarness;

    fn harness() -> (TestHarness<TextButtonWidget>, masonry::core::WidgetId) {
        let widget = NewWidget::new(TextButtonWidget {
            text: "Center Graph",
            faces: Faces {
                rest: Color::WHITE,
                hover: Color::from_rgb8(0xee, 0xee, 0xee),
                outline: Color::BLACK,
                ink: Color::BLACK,
                radius: 4.0,
            },
            height: ControlSize::Icon.px(),
        });
        let id = widget.id();
        let harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            widget,
            (200, 40),
        );
        (harness, id)
    }

    #[test]
    fn a_click_lands_on_release_inside() {
        let (mut harness, id) = harness();
        harness.mouse_click_on(id, Some(PointerButton::Primary));
        assert!(harness.pop_action::<TextButtonClicked>().is_some());
    }

    #[test]
    fn a_press_dragged_off_the_button_does_not_click() {
        let (mut harness, id) = harness();
        harness.mouse_move_to(id);
        harness.mouse_button_press(Some(PointerButton::Primary));
        assert!(harness.pop_action::<TextButtonClicked>().is_none());
        // The harness window is the button, so leave the window.
        harness.mouse_move((500.0, 500.0));
        harness.mouse_button_release(Some(PointerButton::Primary));
        assert!(harness.pop_action::<TextButtonClicked>().is_none());
    }

    #[test]
    fn rest_hover_and_pressed_paint_three_faces() {
        let (mut harness, id) = harness();
        let rest = harness.render();
        harness.mouse_move_to(id);
        let hover = harness.render();
        harness.mouse_button_press(Some(PointerButton::Primary));
        let pressed = harness.render();
        assert_ne!(rest, hover);
        assert_ne!(hover, pressed);
        assert_ne!(rest, pressed);
    }
}
