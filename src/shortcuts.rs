// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A window-level shortcut host.
//!
//! Masonry routes key events to the focused widget, then bubbles them up
//! the ancestor chain until one calls `set_handled`. This widget wraps the
//! whole app, so it receives any key the focused widget did not consume,
//! matches it against a keymap, and submits an app-level action. That is
//! how Cmd+S and tool shortcuts work regardless of what has focus.
//!
//! xix note: this is a stand-in for the framework's window-level action +
//! keymap layer (DESIGN.md D5). The real version lives in the fork and also
//! drives a native menu bar (muda) from the same action list.

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, FromDynWidget, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Widget, WidgetMut, WidgetPod,
};
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

use crate::{App, Tool};

/// App-level actions a shortcut (or, later, a menu item) can fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    Save,
    Overview,
    Tool(Tool),
    FlipHorizontal,
    FlipVertical,
    Rotate90,
    RemoveOverlap,
    Decompose,
}

/// Resolve a key press the focused widget did not consume to an app action.
fn keymap(key: &Key, cmd: bool) -> Option<AppAction> {
    match key {
        Key::Character(c) if cmd && c.eq_ignore_ascii_case("s") => Some(AppAction::Save),
        Key::Named(NamedKey::Escape) => Some(AppAction::Overview),
        Key::Character(c) if !cmd => match c.as_str() {
            "v" => Some(AppAction::Tool(Tool::Select)),
            "p" => Some(AppAction::Tool(Tool::Pen)),
            "b" => Some(AppAction::Tool(Tool::HyperPen)),
            "u" => Some(AppAction::Tool(Tool::Rect)),
            "o" => Some(AppAction::Tool(Tool::Ellipse)),
            "e" => Some(AppAction::Tool(Tool::Knife)),
            "m" => Some(AppAction::Tool(Tool::Measure)),
            _ => None,
        },
        _ => None,
    }
}

pub struct ShortcutHost {
    inner: WidgetPod<dyn Widget>,
}

impl ShortcutHost {
    pub fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            inner: child.erased().to_pod(),
        }
    }

    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.inner)
    }
}

impl Widget for ShortcutHost {
    type Action = AppAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.inner);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.inner, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.inner, size);
        ctx.place_child(&mut self.inner, Point::ORIGIN);
        ctx.derive_baselines(&self.inner);
    }

    fn on_text_event(&mut self, ctx: &mut EventCtx<'_>, _props: &mut PropertiesMut<'_>, event: &TextEvent) {
        let TextEvent::Keyboard(key) = event else { return };
        if key.state != KeyState::Down {
            return;
        }
        let cmd = key.modifiers.meta() || key.modifiers.ctrl();
        if let Some(action) = keymap(&key.key, cmd) {
            ctx.submit_action::<AppAction>(action);
            ctx.set_handled();
        }
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _painter: &mut masonry::imaging::Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(&mut self, _ctx: &mut AccessCtx<'_>, _props: &PropertiesRef<'_>, _node: &mut Node) {}

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.inner.id()])
    }
}

// ---------------------------------------------------------------------------
// View wrapper (single reactive child, following `sized_box`).

pub struct ShortcutHostView<V> {
    inner: V,
}

pub fn shortcut_host<V: WidgetView<App>>(inner: V) -> ShortcutHostView<V> {
    ShortcutHostView { inner }
}

impl<V> ViewMarker for ShortcutHostView<V> {}
impl<V> View<App, (), ViewCtx> for ShortcutHostView<V>
where
    V: WidgetView<App>,
{
    type Element = Pod<ShortcutHost>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app: &mut App) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.inner.build(ctx, app);
        let widget = ShortcutHost::new(child.new_widget);
        let pod = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (pod, child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut App,
    ) {
        let mut child = ShortcutHost::child_mut(&mut element);
        self.inner
            .rebuild(&prev.inner, view_state, ctx, child.downcast(), app);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut child = ShortcutHost::child_mut(&mut element);
        self.inner.teardown(view_state, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut App,
    ) -> MessageResult<()> {
        if message.remaining_path().is_empty() {
            return match message.take_message::<AppAction>() {
                Some(action) => {
                    app.dispatch(*action);
                    MessageResult::Action(())
                }
                None => MessageResult::Stale,
            };
        }
        let mut child = ShortcutHost::child_mut(&mut element);
        self.inner
            .message(view_state, message, child.downcast(), app)
    }
}

// Keep FromDynWidget in scope for downcast().
#[allow(unused_imports)]
use FromDynWidget as _;

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::core::keyboard::{Code, Key, KeyState, KeyboardEvent, Modifiers, NamedKey};
    use masonry::core::{NewWidget, TextEvent};
    use masonry::properties::Dimensions;
    use masonry::theme::default_property_set;
    use masonry::widgets::{Button, Label};
    use masonry_testing::TestHarness;

    fn key(k: Key, cmd: bool) -> TextEvent {
        let mut modifiers = Modifiers::empty();
        modifiers.set(Modifiers::META, cmd);
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: k,
            code: Code::Unidentified,
            modifiers,
            ..KeyboardEvent::default()
        })
    }

    fn harness() -> (TestHarness<ShortcutHost>, masonry::core::WidgetId) {
        // A focusable child (Button) so key events route and bubble to the host.
        let button = Button::new(Label::new("x").prepare());
        let button = NewWidget::new(button);
        let button_id = button.id();
        let host = ShortcutHost::new(button)
            .prepare()
            .with_props(Dimensions::MAX);
        let harness = TestHarness::create_with_size(default_property_set(), host, (100, 40));
        (harness, button_id)
    }

    #[test]
    fn cmd_s_dispatches_save_even_though_button_is_focused() {
        let (mut harness, button_id) = harness();
        harness.focus_on(Some(button_id));
        harness.process_text_event(key(Key::Character("s".into()), true));
        let action = harness.pop_action::<AppAction>();
        assert_eq!(action.map(|(a, _)| a), Some(AppAction::Save));
    }

    #[test]
    fn tool_letter_dispatches_tool() {
        let (mut harness, button_id) = harness();
        harness.focus_on(Some(button_id));
        harness.process_text_event(key(Key::Character("p".into()), false));
        let action = harness.pop_action::<AppAction>();
        assert_eq!(action.map(|(a, _)| a), Some(AppAction::Tool(Tool::Pen)));
    }

    #[test]
    fn escape_dispatches_overview() {
        let (mut harness, button_id) = harness();
        harness.focus_on(Some(button_id));
        harness.process_text_event(key(Key::Named(NamedKey::Escape), false));
        let action = harness.pop_action::<AppAction>();
        assert_eq!(action.map(|(a, _)| a), Some(AppAction::Overview));
    }
}
