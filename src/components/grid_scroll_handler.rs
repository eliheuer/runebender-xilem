// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Grid scroll handler — handles scroll wheel, arrow keys, and Cmd+S
//! for the virtual glyph grid.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx,
    PaintCtx, PointerEvent, PropertiesMut, PropertiesRef,
    RegisterCtx, TextEvent, Update, UpdateCtx, Widget,
};
use masonry::vello::Scene;
use kurbo::Size;
use std::marker::PhantomData;
use xilem::core::{
    MessageContext, MessageResult, Mut, View, ViewMarker,
};
use xilem::{Pod, ViewCtx};

// ============================================================
// Action
// ============================================================

/// Actions emitted by the grid scroll handler widget
#[derive(Clone, Copy, Debug)]
pub enum GridScrollAction {
    /// Scroll by `delta` rows (positive = down)
    Scroll(i32),
    /// Save requested (Cmd+S)
    Save,
}

// ============================================================
// Widget
// ============================================================

/// Invisible widget that captures scroll wheel, arrow keys, and
/// Cmd+S for the glyph grid.
pub struct GridScrollWidget {
    size: Size,
}

impl GridScrollWidget {
    pub fn new() -> Self {
        Self { size: Size::ZERO }
    }
}

impl Widget for GridScrollWidget {
    type Action = GridScrollAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

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
        self.size = bc.max();
        self.size
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _scene: &mut Scene,
    ) {
        // Invisible
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
        match event {
            PointerEvent::Down(_) | PointerEvent::Move(_) => {
                ctx.request_focus();
            }
            PointerEvent::Scroll(scroll_event) => {
                // Convert scroll delta to row count.
                // LineDelta: each tick is one row.
                // PixelDelta: divide by threshold.
                let rows = match &scroll_event.delta {
                    masonry::core::ScrollDelta::LineDelta(_, y) => {
                        -(*y as i32)
                    }
                    masonry::core::ScrollDelta::PixelDelta(pos) => {
                        -(pos.y / 40.0) as i32
                    }
                    _ => 0,
                };
                if rows != 0 {
                    ctx.submit_action::<GridScrollAction>(
                        GridScrollAction::Scroll(rows),
                    );
                    ctx.set_handled();
                }
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
        use masonry::core::keyboard::{Key, KeyState, NamedKey};

        if let TextEvent::Keyboard(key_event) = event {
            if key_event.state != KeyState::Down {
                return;
            }

            let cmd = key_event.modifiers.meta()
                || key_event.modifiers.ctrl();

            // Cmd+S → save
            if cmd
                && matches!(
                    &key_event.key,
                    Key::Character(c) if c == "s"
                )
            {
                ctx.submit_action::<GridScrollAction>(
                    GridScrollAction::Save,
                );
                ctx.set_handled();
                return;
            }

            // Arrow keys → scroll (no modifier)
            if !cmd {
                match &key_event.key {
                    Key::Named(NamedKey::ArrowDown) => {
                        ctx.submit_action::<GridScrollAction>(
                            GridScrollAction::Scroll(1),
                        );
                        ctx.set_handled();
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        ctx.submit_action::<GridScrollAction>(
                            GridScrollAction::Scroll(-1),
                        );
                        ctx.set_handled();
                    }
                    _ => {}
                }
            }
        }
    }
}

// ============================================================
// Xilem View wrapper
// ============================================================

/// Create a grid scroll handler view.
///
/// `on_scroll` receives the row delta (positive = down).
/// `on_save` is called when Cmd+S is pressed.
pub fn grid_scroll_handler<State, Action>(
    on_scroll: impl Fn(&mut State, i32) + Send + Sync + 'static,
    on_save: impl Fn(&mut State) + Send + Sync + 'static,
) -> GridScrollHandlerView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    GridScrollHandlerView {
        on_scroll: Box::new(on_scroll),
        on_save: Box::new(on_save),
        phantom: PhantomData,
    }
}

type ScrollCb<S> = Box<dyn Fn(&mut S, i32) + Send + Sync>;
type SaveCb<S> = Box<dyn Fn(&mut S) + Send + Sync>;

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct GridScrollHandlerView<State, Action = ()> {
    on_scroll: ScrollCb<State>,
    on_save: SaveCb<State>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<State, Action> ViewMarker
    for GridScrollHandlerView<State, Action>
{
}

impl<State: 'static, Action: 'static + Default>
    View<State, Action, ViewCtx>
    for GridScrollHandlerView<State, Action>
{
    type Element = Pod<GridScrollWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let widget = GridScrollWidget::new();
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
        match message.take_message::<GridScrollAction>() {
            Some(action) => match *action {
                GridScrollAction::Scroll(delta) => {
                    (self.on_scroll)(app_state, delta);
                    MessageResult::Action(Action::default())
                }
                GridScrollAction::Save => {
                    (self.on_save)(app_state);
                    MessageResult::Action(Action::default())
                }
            },
            None => MessageResult::Stale,
        }
    }
}
