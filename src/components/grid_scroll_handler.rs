// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Grid scroll handler — replaces xilem's `portal()` for the
//! glyph grid.
//!
//! # Why not portal?
//!
//! Xilem provides `portal()` as its built-in scroll container,
//! but it didn't work well for the glyph grid for three reasons:
//!
//! 1. **Performance.** Portal clips painting to a viewport, but
//!    xilem's reactive model still rebuilds the *entire* view
//!    tree on every state change. With portal wrapping all glyph
//!    cells, every rebuild would construct views for hundreds of
//!    off-screen glyphs and compute their bezpaths — even though
//!    only a few rows are visible.
//!
//! 2. **Virtual rendering.** By handling scroll state ourselves
//!    (via `AppState.grid_scroll_row`), the view layer in
//!    `src/views/glyph_grid/mod.rs` can do virtual rendering —
//!    it only builds xilem views for the rows currently visible
//!    on screen. This is the key performance win.
//!
//! 3. **Keyboard integration.** The grid needs arrow-key
//!    navigation and Cmd shortcuts (save, copy, paste) routed
//!    through a single focused widget. Portal handles scroll
//!    wheel and scroll bars, but doesn't accept keyboard focus
//!    and has no keyboard event handling (its `on_text_event`
//!    is empty).
//!
//! # How it works
//!
//! [`GridScrollWidget`] is a transparent container that wraps the
//! grid's flex column. It accepts focus and captures:
//! - **Scroll wheel** → [`GridScrollAction::Scroll`]
//! - **Arrow keys** → [`GridScrollAction::Navigate`]
//! - **Cmd+S/C/V** → Save, Copy, Paste actions
//!
//! The xilem [`GridScrollHandlerView`] wrapper routes these
//! actions to a single `on_action` callback that updates
//! `AppState`. The state change triggers a reactive rebuild, and
//! the grid view layer re-slices to show the new visible rows.
//!
//! The widget itself does no painting — its child (the grid flex
//! column built by `glyph_grid_view()`) handles all rendering.
//! Child glyph cell actions (click, double-click) pass through
//! the view wrapper's `message()` method to their own handlers.

use std::marker::PhantomData;

use kurbo::{Point, Size};
use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx,
    NewWidget, PaintCtx, PointerEvent, PropertiesMut,
    PropertiesRef, RegisterCtx, ScrollDelta, TextEvent, Update,
    UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::vello::Scene;
use xilem::core::{
    MessageContext, MessageResult, Mut, View, ViewMarker,
};
use xilem::{Pod, ViewCtx, WidgetView};

/// Pixels per scroll "row" for trackpad pixel-based deltas.
/// Line-based scroll (mouse wheel) already arrives in rows.
const PIXELS_PER_SCROLL_ROW: f64 = 40.0;

// ============================================================
// Actions
// ============================================================

/// Arrow-key navigation direction in the glyph grid.
#[derive(Clone, Copy, Debug)]
pub enum NavDirection {
    Left,
    Right,
    Up,
    Down,
}

/// Actions emitted by [`GridScrollWidget`] and handled by the
/// view layer's `on_action` callback.
#[derive(Clone, Copy, Debug)]
pub enum GridScrollAction {
    /// Scroll by `delta` rows (positive = down, negative = up).
    Scroll(i32),
    /// Arrow-key navigation in the given direction.
    Navigate(NavDirection),
    /// Save the current workspace (Cmd+S).
    Save,
    /// Copy selected glyph outlines (Cmd+C).
    Copy,
    /// Paste clipboard outlines (Cmd+V).
    Paste,
    /// Open the selected glyph in the editor (Enter).
    OpenSelected,
}

// ============================================================
// Widget
// ============================================================

/// A transparent container that wraps the glyph grid, accepts
/// keyboard focus, and emits [`GridScrollAction`]s.
///
/// Child widgets (glyph cells) handle their own painting and
/// pointer events. This widget only intercepts scroll, arrow
/// keys, and Cmd shortcuts.
pub struct GridScrollWidget {
    child: WidgetPod<dyn Widget>,
}

impl GridScrollWidget {
    /// Wrap a child widget in a new scroll handler container.
    pub fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            child: child.erased().to_pod(),
        }
    }

    /// Borrow the child widget mutably (used by the view layer
    /// to forward rebuild/teardown/message calls).
    pub fn child_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
    ) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for GridScrollWidget {
    type Action = GridScrollAction;

    fn accepts_focus(&self) -> bool {
        true
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let PointerEvent::Down(_) = event {
            ctx.request_focus();
        }

        if let PointerEvent::Scroll(scroll) = event {
            let rows = scroll_delta_to_rows(&scroll.delta);
            if rows != 0 {
                ctx.request_focus();
                ctx.submit_action::<GridScrollAction>(
                    GridScrollAction::Scroll(rows),
                );
                ctx.set_handled();
            }
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let TextEvent::Keyboard(key_event) = event else {
            return;
        };
        if key_event.state != KeyState::Down {
            return;
        }

        let cmd = key_event.modifiers.meta()
            || key_event.modifiers.ctrl();

        if let Some(action) = key_to_action(&key_event.key, cmd)
        {
            ctx.submit_action::<GridScrollAction>(action);
            ctx.set_handled();
        }
    }

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

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _scene: &mut Scene,
    ) {
        // Transparent — child paints itself automatically.
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

// ============================================================
// Keyboard & Scroll Helpers
// ============================================================

/// Map a key press to a grid action, if any.
///
/// Cmd+S/C/V map to Save/Copy/Paste. Arrow keys (without Cmd)
/// map to Navigate in the corresponding direction.
fn key_to_action(
    key: &Key,
    cmd: bool,
) -> Option<GridScrollAction> {
    if cmd {
        return match key {
            Key::Character(c) if c == "s" => {
                Some(GridScrollAction::Save)
            }
            Key::Character(c) if c == "c" => {
                Some(GridScrollAction::Copy)
            }
            Key::Character(c) if c == "v" => {
                Some(GridScrollAction::Paste)
            }
            _ => None,
        };
    }

    match key {
        Key::Named(NamedKey::ArrowUp) => {
            Some(GridScrollAction::Navigate(NavDirection::Up))
        }
        Key::Named(NamedKey::ArrowDown) => {
            Some(GridScrollAction::Navigate(NavDirection::Down))
        }
        Key::Named(NamedKey::ArrowLeft) => {
            Some(GridScrollAction::Navigate(NavDirection::Left))
        }
        Key::Named(NamedKey::ArrowRight) => {
            Some(GridScrollAction::Navigate(
                NavDirection::Right,
            ))
        }
        Key::Named(NamedKey::Enter) => {
            Some(GridScrollAction::OpenSelected)
        }
        _ => None,
    }
}

/// Convert a scroll delta (line-based or pixel-based) into a
/// row count. Positive = scroll down, negative = scroll up.
fn scroll_delta_to_rows(delta: &ScrollDelta) -> i32 {
    match delta {
        ScrollDelta::LineDelta(_, y) => -(*y as i32),
        ScrollDelta::PixelDelta(pos) => {
            -(pos.y / PIXELS_PER_SCROLL_ROW) as i32
        }
        _ => 0,
    }
}

// ============================================================
// Xilem View Wrapper
// ============================================================

/// Wrap a child view in a [`GridScrollWidget`] that captures
/// scroll, arrow-key, and Cmd shortcut events.
///
/// All captured events are delivered as [`GridScrollAction`]s
/// to the `on_action` callback. Pointer and paint events pass
/// through to child widgets unchanged.
pub fn grid_scroll_handler<State, Action, V>(
    inner: V,
    on_action: impl Fn(&mut State, GridScrollAction)
        + Send
        + Sync
        + 'static,
) -> GridScrollHandlerView<V, State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    GridScrollHandlerView {
        inner,
        on_action: Box::new(on_action),
        phantom: PhantomData,
    }
}

/// Boxed callback for [`GridScrollHandlerView`].
type ActionCallback<S> =
    Box<dyn Fn(&mut S, GridScrollAction) + Send + Sync>;

/// The [`View`] created by [`grid_scroll_handler`].
///
/// Pairs a child view with a [`GridScrollWidget`] container
/// and routes widget actions to the `on_action` callback.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct GridScrollHandlerView<V, State, Action = ()> {
    inner: V,
    on_action: ActionCallback<State>,
    // Tells the compiler this struct is generic over State and
    // Action without storing them. The fn() -> wrapper avoids
    // implying ownership (which would affect Send/Sync).
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<V, State, Action> ViewMarker
    for GridScrollHandlerView<V, State, Action>
{
}

// The View impl below is xilem boilerplate that connects
// GridScrollWidget to the reactive view tree. The four methods
// map to the widget lifecycle:
//
//   build    — create the widget for the first time
//   rebuild  — update the widget when state changes
//   teardown — clean up when the view is removed
//   message  — handle actions from the widget

impl<V, State: 'static, Action: 'static + Default>
    View<State, Action, ViewCtx>
    for GridScrollHandlerView<V, State, Action>
where
    V: WidgetView<State, Action>,
{
    type Element = Pod<GridScrollWidget>;
    type ViewState = V::ViewState;

    fn build(
        &self,
        ctx: &mut ViewCtx,
        app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let (child, child_state) =
            self.inner.build(ctx, app_state);
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
        let mut child =
            GridScrollWidget::child_mut(&mut element);
        self.inner.rebuild(
            &prev.inner,
            view_state,
            ctx,
            child.downcast(),
            app_state,
        );
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut child =
            GridScrollWidget::child_mut(&mut element);
        self.inner
            .teardown(view_state, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageContext,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        // Check for our own scroll/keyboard actions first.
        if let Some(action) =
            message.take_message::<GridScrollAction>()
        {
            (self.on_action)(app_state, *action);
            return MessageResult::Action(Action::default());
        }

        // Not ours — delegate to child (e.g. glyph cell clicks).
        let mut child =
            GridScrollWidget::child_mut(&mut element);
        self.inner.message(
            view_state,
            message,
            child.downcast(),
            app_state,
        )
    }
}
