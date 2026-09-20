// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Keep unsupported text-history shortcuts out of document history.
//!
//! Masonry's text area does not yet implement local Undo and Redo.
//! Those unhandled keys otherwise bubble to Runebender's window shortcut host and mutate the font.

use std::marker::PhantomData;

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, Modifiers};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, FromDynWidget, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

/// Wrap editable text which must not send unsupported local history keys to document history.
pub(crate) fn guard_text_undo<Child, State, Action>(
    child: Child,
) -> TextUndoGuardView<Child, State, Action>
where
    State: 'static,
    Child: WidgetView<State, Action>,
{
    TextUndoGuardView {
        child,
        phantom: PhantomData,
    }
}

/// The view created by [`guard_text_undo`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub(crate) struct TextUndoGuardView<Child, State, Action> {
    child: Child,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<Child, State, Action> ViewMarker for TextUndoGuardView<Child, State, Action> {}

impl<Child, State, Action> View<State, Action, ViewCtx> for TextUndoGuardView<Child, State, Action>
where
    Child: WidgetView<State, Action>,
    State: 'static,
    Action: 'static,
{
    type Element = Pod<TextUndoGuard<Child::Widget>>;
    type ViewState = Child::ViewState;

    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.child.build(ctx, state);
        (
            ctx.create_pod(TextUndoGuard::new(child.new_widget)),
            child_state,
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) {
        let child = TextUndoGuard::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child, state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let child = TextUndoGuard::child_mut(&mut element);
        self.child.teardown(view_state, ctx, child);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<Action> {
        let child = TextUndoGuard::child_mut(&mut element);
        self.child.message(view_state, message, child, state)
    }
}

pub(crate) struct TextUndoGuard<W: Widget + FromDynWidget + ?Sized> {
    child: WidgetPod<W>,
}

impl<W: Widget + FromDynWidget + ?Sized> TextUndoGuard<W> {
    fn new(child: NewWidget<W>) -> Self {
        Self {
            child: child.to_pod(),
        }
    }

    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl<W: Widget + FromDynWidget + ?Sized> Widget for TextUndoGuard<W> {
    type Action = ();

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if let TextEvent::Keyboard(key) = event
            && key.state == KeyState::Down
            && blocks_text_history_key(&key.key, key.modifiers)
        {
            ctx.set_handled();
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.child, axis, cross)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.derive_baselines(&self.child);
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

fn blocks_text_history_key(key: &Key, modifiers: Modifiers) -> bool {
    let action_mod = if cfg!(target_os = "macos") {
        modifiers.meta()
    } else {
        modifiers.ctrl()
    };
    action_mod
        && matches!(
            key,
            Key::Character(character)
                if character.as_str().eq_ignore_ascii_case("z")
                    || character.as_str().eq_ignore_ascii_case("y")
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::widgets::shortcuts::{AppAction, ShortcutHost};
    use masonry::core::keyboard::{Code, KeyboardEvent};
    use masonry::properties::Dimensions;
    use masonry::theme::default_property_set;
    use masonry::widgets::{TextAction, TextInput};
    use masonry_testing::TestHarness;

    fn key(key: Key, modifiers: Modifiers) -> TextEvent {
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key,
            code: Code::Unidentified,
            modifiers,
            ..KeyboardEvent::default()
        })
    }

    #[test]
    fn focused_text_input_blocks_font_undo_but_still_accepts_typing() {
        let input = TextInput::new("a");
        let area_id = input.area_pod().id();
        let guard = TextUndoGuard::new(input.prepare()).prepare();
        let host = ShortcutHost::new(guard)
            .prepare()
            .with_props(Dimensions::MAX);
        let mut harness = TestHarness::create_with_size(default_property_set(), host, (160, 40));
        harness.focus_on(Some(area_id));

        let mut modifiers = Modifiers::empty();
        modifiers.set(
            if cfg!(target_os = "macos") {
                Modifiers::META
            } else {
                Modifiers::CONTROL
            },
            true,
        );
        harness.process_text_event(key(Key::Character("z".into()), modifiers));
        assert!(harness.pop_action::<AppAction>().is_none());

        harness.process_text_event(key(Key::Character("b".into()), Modifiers::empty()));
        let changed = harness.pop_action::<TextAction>().map(|(action, _)| action);
        assert!(matches!(changed, Some(TextAction::Changed(text)) if text.contains('b')));
    }
}
