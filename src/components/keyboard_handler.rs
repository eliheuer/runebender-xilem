// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Keyboard handler widget - handles global keyboard shortcuts like Cmd+S

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx, PaintCtx,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, Widget,
};
use masonry::vello::Scene;
use kurbo::Size;
use std::marker::PhantomData;
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

/// Action emitted when save is requested
#[derive(Clone, Copy, Debug)]
pub struct SaveRequested;

/// A simple widget that can receive keyboard events and handle shortcuts
pub struct KeyboardShortcutsWidget {
    size: Size,
}

impl KeyboardShortcutsWidget {
    pub fn new() -> Self {
        Self {
            size: Size::ZERO,
        }
    }
}

impl Widget for KeyboardShortcutsWidget {
    type Action = SaveRequested;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {
        // No children
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
    }

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        // Take full size to receive pointer events in empty areas
        self.size = bc.max();
        self.size
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _scene: &mut Scene,
    ) {
        // Invisible - no painting
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
        ChildrenIds::new()
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        // Request focus on pointer activity so we can receive keyboard events
        match event {
            PointerEvent::Down(_) | PointerEvent::Move(_) => {
                ctx.request_focus();
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
        use masonry::core::keyboard::{Key, KeyState};

        if let TextEvent::Keyboard(key_event) = event {
            if key_event.state != KeyState::Down {
                return;
            }

            // Check for Cmd+S (save)
            let cmd = key_event.modifiers.meta() || key_event.modifiers.ctrl();
            if cmd && matches!(&key_event.key, Key::Character(c) if c == "s") {
                tracing::info!("Glyph grid: Cmd+S pressed - save requested");
                ctx.submit_action::<SaveRequested>(SaveRequested);
                ctx.set_handled();
            }
        }
    }
}

// --- Xilem View Wrapper ---

/// Public API to create a keyboard shortcuts view
pub fn keyboard_shortcuts<State, Action>(
    on_save: impl Fn(&mut State) + Send + Sync + 'static,
) -> KeyboardShortcutsView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    KeyboardShortcutsView {
        on_save: Box::new(on_save),
        phantom: PhantomData,
    }
}

type SaveCallback<State> = Box<dyn Fn(&mut State) + Send + Sync>;

/// The Xilem View for KeyboardShortcutsWidget
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct KeyboardShortcutsView<State, Action = ()> {
    on_save: SaveCallback<State>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<State, Action> ViewMarker for KeyboardShortcutsView<State, Action> {}

impl<State: 'static, Action: 'static + Default> View<State, Action, ViewCtx>
    for KeyboardShortcutsView<State, Action>
{
    type Element = Pod<KeyboardShortcutsWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let widget = KeyboardShortcutsWidget::new();
        let pod = ctx.create_pod(widget);
        ctx.record_action(pod.new_widget.id());
        (pod, ())
    }

    fn rebuild(
        &self,
        _prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        // Nothing to rebuild
    }

    fn teardown(
        &self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
    ) {
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        message: &mut MessageContext,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<SaveRequested>() {
            Some(_) => {
                (self.on_save)(app_state);
                MessageResult::Action(Action::default())
            }
            None => MessageResult::Stale,
        }
    }
}
