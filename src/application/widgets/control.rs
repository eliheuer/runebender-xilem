// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The one pressable container: every button, toggle, row, chip, and tab.
//!
//! A control holds any content and takes its whole look from a
//! [`ControlStyle`]: the face for rest, hover, pressed, selected, and
//! disabled, and a focus ring. It sets its own background, border, and
//! corner radius from that style as its state changes, so callers set
//! only size and padding. Masonry's stock button cannot do this, because
//! a locally set face overrides its hover and pressed styles.
//!
//! A click lands on release inside the control, from the keyboard with
//! Space or Return, or from assistive technology. A pointer press does
//! not take keyboard focus, so the focus ring shows only for keyboard use.

use std::marker::PhantomData;

use masonry::accesskit::{Action, Node, Role};
use masonry::core::keyboard::{Key, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::properties::{Background, BorderColor, BorderWidth, CornerRadius};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::{Pod, ViewCtx, WidgetView};

use crate::application::view::design::ControlStyle;

/// A control was clicked.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ControlPress;

/// The Masonry widget behind [`control`].
pub(crate) struct ControlWidget {
    child: WidgetPod<dyn Widget>,
    style: ControlStyle,
    hovered: bool,
    pressed: bool,
}

impl ControlWidget {
    pub(crate) fn new(style: ControlStyle, child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            child: child.erased().to_pod(),
            style,
            hovered: false,
            pressed: false,
        }
    }

    /// The control's content.
    pub(crate) fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    /// Change the style, repainting the face for the current state.
    pub(crate) fn set_style(this: &mut WidgetMut<'_, Self>, style: ControlStyle) {
        if this.widget.style == style {
            return;
        }
        this.widget.style = style;
        let face = this
            .widget
            .style
            .face(this.widget.hovered, this.widget.pressed);
        this.insert_prop(Background::Color(face.fill));
        this.insert_prop(BorderColor {
            color: face.outline,
        });
        this.insert_prop(BorderWidth {
            width: Length::px(style.stroke()),
        });
        this.insert_prop(CornerRadius {
            radius: Length::px(style.radius()),
        });
        this.ctx.set_disabled(style.is_disabled());
        this.ctx.request_render();
    }

    fn apply_face(&self, props: &mut PropertiesMut<'_>) {
        let face = self.style.face(self.hovered, self.pressed);
        props.insert(Background::Color(face.fill));
        props.insert(BorderColor {
            color: face.outline,
        });
        props.insert(BorderWidth {
            width: Length::px(self.style.stroke()),
        });
        props.insert(CornerRadius {
            radius: Length::px(self.style.radius()),
        });
    }
}

impl Widget for ControlWidget {
    type Action = ControlPress;

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
                    ctx.submit_action::<ControlPress>(ControlPress);
                }
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if let TextEvent::Keyboard(event) = event
            && event.state.is_up()
            && (matches!(&event.key, Key::Character(c) if c == " ")
                || event.key == Key::Named(NamedKey::Enter))
        {
            ctx.submit_action::<ControlPress>(ControlPress);
            ctx.set_handled();
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == Action::Click {
            ctx.submit_action::<ControlPress>(ControlPress);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded => self.apply_face(props),
            Update::HoveredChanged(_) | Update::ActiveChanged(_) => {
                self.hovered = ctx.is_hovered();
                self.pressed = ctx.is_active();
                self.apply_face(props);
                ctx.request_render();
            }
            Update::FocusChanged(_) => ctx.request_render(),
            _ => {}
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);
        ctx.compute_length(
            &mut self.child,
            len_req.into(),
            context_size,
            axis,
            cross_length,
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let child_size = ctx.compute_size(&mut self.child, SizeDef::fit(size), size.into());
        ctx.run_layout(&mut self.child, child_size);
        let child_origin = ((size - child_size).to_vec2() * 0.5).to_point();
        ctx.place_child(&mut self.child, child_origin);
        ctx.derive_baselines(&self.child);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        if !ctx.is_focus_target() {
            return;
        }
        // The ring sits just inside the outline, so it never changes the
        // control's size or overlaps a neighbor.
        let stroke = self.style.stroke().max(1.0);
        let inset = self.style.stroke() + stroke / 2.0;
        let ring = ctx
            .border_box()
            .inset(-inset)
            .to_rounded_rect((self.style.radius() - inset).max(0.0));
        painter
            .stroke(ring, &Stroke::new(stroke), self.style.focus_ring())
            .draw();
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
        node.add_action(Action::Click);
        if self.style.is_selectable() {
            node.set_toggled(if self.style.is_selected() {
                masonry::accesskit::Toggled::True
            } else {
                masonry::accesskit::Toggled::False
            });
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }

    fn accepts_focus(&self) -> bool {
        true
    }
}

const CONTENT: ViewId = ViewId::new(0xc0_7701);

/// A pressable control around `content`, drawn in `style`.
///
/// Every button in the application is one of these. Choose its look by
/// [`ControlKind`](crate::application::view::design::ControlKind) through
/// [`ControlStyle`]; set only its size and padding at the call site.
pub(crate) fn control<State, V, F>(
    style: ControlStyle,
    content: V,
    on_click: F,
) -> ControlView<State, V, F>
where
    State: 'static,
    V: WidgetView<State>,
    F: Fn(&mut State) + Send + Sync + 'static,
{
    ControlView {
        style,
        content,
        on_click,
        phantom: PhantomData,
    }
}

/// The view behind [`control`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub(crate) struct ControlView<State, V, F> {
    style: ControlStyle,
    content: V,
    on_click: F,
    phantom: PhantomData<fn(State)>,
}

impl<State, V, F> ViewMarker for ControlView<State, V, F> {}
impl<State, V, F> View<State, (), ViewCtx> for ControlView<State, V, F>
where
    State: 'static,
    V: WidgetView<State>,
    F: Fn(&mut State) + Send + Sync + 'static,
{
    type Element = Pod<ControlWidget>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = ctx.with_id(CONTENT, |ctx| {
            View::<State, (), _>::build(&self.content, ctx, state)
        });
        let pod = ctx.with_action_widget(|ctx| {
            let mut pod = ctx.create_pod(ControlWidget::new(self.style, child.new_widget));
            pod.new_widget.options.disabled = self.style.is_disabled();
            pod
        });
        (pod, child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) {
        ControlWidget::set_style(&mut element, self.style);
        ctx.with_id(CONTENT, |ctx| {
            View::<State, (), _>::rebuild(
                &self.content,
                &prev.content,
                view_state,
                ctx,
                ControlWidget::child_mut(&mut element).downcast(),
                state,
            );
        });
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(CONTENT, |ctx| {
            View::<State, (), _>::teardown(
                &self.content,
                view_state,
                ctx,
                ControlWidget::child_mut(&mut element).downcast(),
            );
        });
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<()> {
        match message.take_first() {
            Some(CONTENT) => self.content.message(
                view_state,
                message,
                ControlWidget::child_mut(&mut element).downcast(),
                state,
            ),
            None => match message.take_message::<ControlPress>() {
                Some(_) => {
                    (self.on_click)(state);
                    MessageResult::Action(())
                }
                None => MessageResult::Stale,
            },
            _ => MessageResult::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::view::design::ControlKind;
    use crate::application::view::theme::Palette;
    use masonry::widgets::Label;
    use masonry_testing::TestHarness;

    fn harness(kind: ControlKind) -> (TestHarness<ControlWidget>, masonry::core::WidgetId) {
        let pal = Palette::load("gray");
        let widget = NewWidget::new(ControlWidget::new(
            ControlStyle::new(&pal, kind),
            Label::new("Center Graph").prepare(),
        ));
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
        let (mut harness, id) = harness(ControlKind::Secondary);
        harness.mouse_move_to(id);
        harness.mouse_button_press(Some(PointerButton::Primary));
        assert!(harness.pop_action::<ControlPress>().is_none());
        harness.mouse_button_release(Some(PointerButton::Primary));
        assert!(harness.pop_action::<ControlPress>().is_some());
    }

    #[test]
    fn a_press_dragged_off_does_not_click() {
        let (mut harness, id) = harness(ControlKind::Secondary);
        harness.mouse_move_to(id);
        harness.mouse_button_press(Some(PointerButton::Primary));
        // The harness window is the control, so leave the window.
        harness.mouse_move((500.0, 500.0));
        harness.mouse_button_release(Some(PointerButton::Primary));
        assert!(harness.pop_action::<ControlPress>().is_none());
    }

    #[test]
    fn every_kind_paints_rest_hover_and_pressed_differently() {
        for kind in [
            ControlKind::Primary,
            ControlKind::Secondary,
            ControlKind::Quiet,
            ControlKind::Toggle,
            ControlKind::Row,
            ControlKind::Disclosure,
            ControlKind::Bare,
        ] {
            let (mut harness, id) = harness(kind);
            let rest = harness.render();
            harness.mouse_move_to(id);
            let hover = harness.render();
            harness.mouse_button_press(Some(PointerButton::Primary));
            let pressed = harness.render();
            assert_ne!(rest, hover, "{kind:?} hover");
            assert_ne!(hover, pressed, "{kind:?} pressed");
        }
    }

    #[test]
    fn a_pointer_press_does_not_take_keyboard_focus() {
        let (mut harness, id) = harness(ControlKind::Secondary);
        harness.mouse_click_on(id, Some(PointerButton::Primary));
        assert_ne!(harness.focused_widget_id(), Some(id));
    }

    #[test]
    fn keyboard_focus_draws_the_ring_and_space_clicks() {
        let (mut harness, id) = harness(ControlKind::Secondary);
        let unfocused = harness.render();
        harness.focus_on(Some(id));
        assert_ne!(unfocused, harness.render(), "focus ring");
        harness.process_text_event(TextEvent::Keyboard(
            masonry::core::keyboard::KeyboardEvent {
                state: masonry::core::keyboard::KeyState::Up,
                key: Key::Character(" ".into()),
                ..Default::default()
            },
        ));
        assert!(harness.pop_action::<ControlPress>().is_some());
    }
}
