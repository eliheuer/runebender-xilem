// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Grid scroll handler — container widget that wraps the glyph
//! grid and handles scroll wheel, arrow keys, and Cmd+S.
//!
//! Events bubble up from child glyph cell widgets to this
//! container, which holds keyboard focus for arrow key scrolling.

use kurbo::{Point, Size};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx, NewWidget, PaintCtx, PointerEvent,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetMut,
    WidgetPod,
};
use masonry::vello::Scene;
use std::marker::PhantomData;
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

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

/// Container widget that wraps the glyph grid and captures
/// scroll wheel, arrow keys, and Cmd+S. Events bubble up from
/// child widgets (glyph cells) to this container.
pub struct GridScrollWidget {
    child: WidgetPod<dyn Widget>,
}

impl GridScrollWidget {
    pub fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            child: child.erased().to_pod(),
        }
    }

    /// Get mutable access to the child widget
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for GridScrollWidget {
    type Action = GridScrollAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
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
        ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        let size = ctx.run_layout(&mut self.child, bc);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        size
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _scene: &mut Scene) {
        // Transparent — child painting is automatic
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

    fn accepts_focus(&self) -> bool {
        true
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            // Grab focus when user clicks in the grid area
            // (bubbles from cells that don't set_handled on Down)
            PointerEvent::Down(_) => {
                ctx.request_focus();
            }
            PointerEvent::Scroll(scroll_event) => {
                ctx.request_focus();
                // Convert scroll delta to row count.
                let rows = match &scroll_event.delta {
                    masonry::core::ScrollDelta::LineDelta(_, y) => -(*y as i32),
                    masonry::core::ScrollDelta::PixelDelta(pos) => -(pos.y / 40.0) as i32,
                    _ => 0,
                };
                if rows != 0 {
                    ctx.submit_action::<GridScrollAction>(GridScrollAction::Scroll(rows));
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

            let cmd = key_event.modifiers.meta() || key_event.modifiers.ctrl();

            // Cmd+S → save
            if cmd
                && matches!(
                    &key_event.key,
                    Key::Character(c) if c == "s"
                )
            {
                ctx.submit_action::<GridScrollAction>(GridScrollAction::Save);
                ctx.set_handled();
                return;
            }

            // Arrow keys → scroll (no modifier)
            if !cmd {
                match &key_event.key {
                    Key::Named(NamedKey::ArrowDown) => {
                        ctx.submit_action::<GridScrollAction>(GridScrollAction::Scroll(1));
                        ctx.set_handled();
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        ctx.submit_action::<GridScrollAction>(GridScrollAction::Scroll(-1));
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

/// Create a grid scroll handler that wraps a child view.
///
/// The container captures scroll events, arrow keys, and Cmd+S
/// while delegating pointer/click events to child widgets.
///
/// `on_scroll` receives the row delta (positive = down).
/// `on_save` is called when Cmd+S is pressed.
pub fn grid_scroll_handler<State, Action, V>(
    inner: V,
    on_scroll: impl Fn(&mut State, i32) + Send + Sync + 'static,
    on_save: impl Fn(&mut State) + Send + Sync + 'static,
) -> GridScrollHandlerView<V, State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    GridScrollHandlerView {
        inner,
        on_scroll: Box::new(on_scroll),
        on_save: Box::new(on_save),
        phantom: PhantomData,
    }
}

type ScrollCb<S> = Box<dyn Fn(&mut S, i32) + Send + Sync>;
type SaveCb<S> = Box<dyn Fn(&mut S) + Send + Sync>;

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct GridScrollHandlerView<V, State, Action = ()> {
    inner: V,
    on_scroll: ScrollCb<State>,
    on_save: SaveCb<State>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<V, State, Action> ViewMarker for GridScrollHandlerView<V, State, Action> {}

impl<V, State: 'static, Action: 'static + Default> View<State, Action, ViewCtx>
    for GridScrollHandlerView<V, State, Action>
where
    V: WidgetView<State, Action>,
{
    type Element = Pod<GridScrollWidget>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.inner.build(ctx, app_state);
        let widget = GridScrollWidget::new(child.new_widget);
        let pod = ctx.create_pod(widget);
        ctx.record_action(pod.new_widget.id());
        (pod, child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        let mut child = GridScrollWidget::child_mut(&mut element);
        self.inner
            .rebuild(&prev.inner, view_state, ctx, child.downcast(), app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut child = GridScrollWidget::child_mut(&mut element);
        self.inner.teardown(view_state, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageContext,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        // Handle container's own actions (scroll, save)
        if let Some(action) = message.take_message::<GridScrollAction>() {
            match *action {
                GridScrollAction::Scroll(delta) => {
                    (self.on_scroll)(app_state, delta);
                    return MessageResult::Action(Action::default());
                }
                GridScrollAction::Save => {
                    (self.on_save)(app_state);
                    return MessageResult::Action(Action::default());
                }
            }
        }

        // Delegate to child for cell actions
        let mut child = GridScrollWidget::child_mut(&mut element);
        self.inner
            .message(view_state, message, child.downcast(), app_state)
    }
}
