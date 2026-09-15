// Copyright 2026 the Runebender Authors
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
use masonry::core::keyboard::KeyState;
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, FromDynWidget, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Widget, WidgetMut, WidgetPod,
};
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

use crate::{AppState, Tool};

/// Workspace-level actions a shortcut or a menu item can fire.
// Some variants are only constructed by the menu table, which the
// native menu bar reads, and that exists on macOS.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "the menu table is read by the native menu bar, which is macOS only"
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppAction {
    Quit,
    OpenFont,
    SaveAs,
    RevertToSaved,
    ExportFont,
    Save,
    Undo,
    Redo,
    Overview,
    Tool(Tool),
    /// Hold Space to pan in a chrome-free filled preview.
    BeginSpacePan,
    /// Restore the selected tool and editing chrome when Space is released.
    EndSpacePan,
    FlipHorizontal,
    FlipVertical,
    Rotate90,
    RotateRight,
    Rotate180,
    RemoveOverlap,
    BooleanUnion,
    BooleanSubtract,
    BooleanIntersect,
    BooleanExclude,
    Decompose,
    Duplicate,
    DuplicateRepeat,
    ReverseContours,
    SetStartPoint,
    TidyPaths,
    AddExtremes,
    RoundCoordinates,
    CorrectPathDirection,
    HyperToCubic,
    QuadsToCubics,
    CubicsToQuads,
    RoundCorners,
    Harmonize,
    Balance,
    Optimize,
    FilterOffset,
    FilterExtrude,
    FilterRoughen,
    FilterSlant,
    Copy,
    Paste,
    CopySelectedGlyphs,
    SelectAll,
    DeselectAll,
    InvertSelection,
    NewFont,
    CycleTheme,
    Theme(&'static str),
    ZoomToFit,
    ShowAllMasters,
    NextMaster,
    PreviousMaster,
    NextSampleString,
    PreviousSampleString,
    GridDots,
    GridLines,
    MeasureColorize,
    MeasureHandles,
    MeasureSegments,
    MeasureSizes,
    MeasureSpans,
    MeasureSideBearings,
    MeasurePopcount,
    MeasureAllOn,
    MeasureAllOff,
    /// The Nodes menu: the canvas, a new file, save it, run it.
    NodesTab,
    NodesNew,
    NodesOpen,
    NodesSave,
    NodesRun,
    /// Add every glyph the selected coverage filter is missing.
    GenerateMissing,
    NewGlyph,
    DuplicateGlyph,
    RemoveGlyph,
    UpdateMetrics,
    Reinterpolate,
    CheckJoining,
    ComposeFromAnchors,
    BakeMasks,
    ExportGlyphSvg,
    TraceImage,
    BoldenWithModel,
    PlaceImage,
    ImportSvg,
    RemoveImage,
    /// The grid's order, as the GPUI build's View menu has it.
    SortByName,
    SortByUnicode,
}

pub(crate) struct ShortcutHost {
    inner: WidgetPod<dyn Widget>,
}

impl ShortcutHost {
    pub(crate) fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            inner: child.erased().to_pod(),
        }
    }

    #[cfg_attr(
        not(target_os = "macos"),
        allow(dead_code, reason = "the reactive wrapper is used by the macOS root")
    )]
    pub(crate) fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
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

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        let unmodified_space = matches!(&key.key, masonry::core::keyboard::Key::Character(value) if value == " ")
            && !key.modifiers.meta()
            && !key.modifiers.ctrl()
            && !key.modifiers.alt();
        if unmodified_space {
            let action = match key.state {
                KeyState::Down => AppAction::BeginSpacePan,
                KeyState::Up => AppAction::EndSpacePan,
            };
            ctx.submit_action::<AppAction>(action);
            ctx.set_handled();
            return;
        }
        if key.state != KeyState::Down {
            return;
        }
        if let Some(action) = crate::actions::action_for_key(&key.key, key.modifiers) {
            ctx.submit_action::<AppAction>(action);
            ctx.set_handled();
        }
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut masonry::imaging::Painter<'_>,
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
        ChildrenIds::from_slice(&[self.inner.id()])
    }
}

// ---------------------------------------------------------------------------
// View wrapper (single reactive child, following `sized_box`).

#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "the in-window menu is the non-macOS root wrapper")
)]
pub(crate) struct ShortcutHostView<V> {
    inner: V,
}

#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "the in-window menu is the non-macOS root wrapper")
)]
pub(crate) fn shortcut_host<V: WidgetView<AppState>>(inner: V) -> ShortcutHostView<V> {
    ShortcutHostView { inner }
}

impl<V> ViewMarker for ShortcutHostView<V> {}
impl<V> View<AppState, (), ViewCtx> for ShortcutHostView<V>
where
    V: WidgetView<AppState>,
{
    type Element = Pod<ShortcutHost>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app: &mut AppState) -> (Self::Element, Self::ViewState) {
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
        app: &mut AppState,
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
        app: &mut AppState,
    ) -> MessageResult<()> {
        if message.remaining_path().is_empty() {
            return match message.take_message::<AppAction>() {
                Some(action) => {
                    if crate::actions::action_enabled(*action, app) {
                        app.dispatch(*action);
                    }
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
#[expect(unused_imports, reason = "the trait is used only on some platforms")]
use FromDynWidget as _;

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::core::keyboard::{Code, Key, KeyState, KeyboardEvent, Modifiers, NamedKey};
    use masonry::core::{NewWidget, TextEvent};
    use masonry::properties::Dimensions;
    use masonry::theme::default_property_set;
    use masonry::widgets::{Button, Label, TextArea};
    use masonry_testing::TestHarness;

    fn key_with_state(k: Key, cmd: bool, state: KeyState) -> TextEvent {
        let mut modifiers = Modifiers::empty();
        modifiers.set(Modifiers::META, cmd);
        TextEvent::Keyboard(KeyboardEvent {
            state,
            key: k,
            code: Code::Unidentified,
            modifiers,
            ..KeyboardEvent::default()
        })
    }

    fn key(k: Key, cmd: bool) -> TextEvent {
        key_with_state(k, cmd, KeyState::Down)
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

    #[test]
    fn space_press_and_release_dispatch_temporary_pan_actions() {
        let (mut harness, _) = harness();
        harness.focus_on(Some(harness.root_id()));
        harness.process_text_event(key_with_state(
            Key::Character(" ".into()),
            false,
            KeyState::Down,
        ));
        let down = harness.pop_action::<AppAction>();
        assert_eq!(
            down.map(|(action, _)| action),
            Some(AppAction::BeginSpacePan)
        );

        harness.process_text_event(key_with_state(
            Key::Character(" ".into()),
            false,
            KeyState::Up,
        ));
        let up = harness.pop_action::<AppAction>();
        assert_eq!(up.map(|(action, _)| action), Some(AppAction::EndSpacePan));
    }

    #[test]
    fn focused_text_area_consumes_editing_shortcuts_before_the_host() {
        let area = NewWidget::new(TextArea::new_editable("hello"));
        let area_id = area.id();
        let host = ShortcutHost::new(area)
            .prepare()
            .with_props(Dimensions::MAX);
        let mut harness = TestHarness::create_with_size(default_property_set(), host, (160, 40));
        harness.focus_on(Some(area_id));

        harness.process_text_event(key(Key::Character("a".into()), true));

        while let Some((action, _)) = harness.pop_action_erased() {
            assert!(action.downcast_ref::<AppAction>().is_none());
        }
    }
}
