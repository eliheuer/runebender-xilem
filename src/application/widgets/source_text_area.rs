// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A multiline source editor which keeps unsupported text history keys local.
//!
//! The pinned Masonry text area treats modified `z` and `y` keys as ordinary text.
//! This leaf owns the upstream text area value directly, filters those two shortcuts first, and
//! delegates all other editing, IME, selection, accessibility, layout, and painting behavior.

use std::any::TypeId;
use std::marker::PhantomData;

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, Modifiers};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, CursorIcon, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
    PointerEvent, PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, StyleProperty, TextEvent,
    Update, UpdateCtx, Widget, WidgetMut,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use masonry::parley::{FontFamily, FontFamilyName, GenericFamily};
use masonry::properties::ContentColor;
use masonry::widgets::{InsertNewline, TextAction, TextArea};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

use crate::application::view::design::TextSize;

type Callback<State, Action> = Box<dyn Fn(&mut State, String) -> Action + Send + Sync + 'static>;

/// Build a multiline, monospace editor for code or structured parameters.
pub(crate) fn source_text_area<F, State, Action>(
    contents: String,
    on_changed: F,
) -> SourceTextAreaView<State, Action>
where
    F: Fn(&mut State, String) -> Action + Send + Sync + 'static,
    State: 'static,
{
    SourceTextAreaView {
        contents,
        on_changed: Box::new(on_changed),
        text_color: None,
        phantom: PhantomData,
    }
}

/// Reactive view for [`SourceTextArea`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub(crate) struct SourceTextAreaView<State: 'static, Action> {
    contents: String,
    on_changed: Callback<State, Action>,
    text_color: Option<Color>,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<State: 'static, Action: 'static> SourceTextAreaView<State, Action> {
    /// Set the editable text color.
    pub(crate) fn text_color(mut self, color: Color) -> Self {
        self.text_color = Some(color);
        self
    }
}

impl<State: 'static, Action> ViewMarker for SourceTextAreaView<State, Action> {}

impl<State: 'static, Action: 'static> View<State, Action, ViewCtx>
    for SourceTextAreaView<State, Action>
{
    type Element = Pod<SourceTextArea>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        let mut props = masonry::core::PropertySet::new();
        if let Some(color) = self.text_color {
            props.insert(ContentColor { color });
        }
        let pod = Pod::new_with_props(
            SourceTextArea::new(&self.contents).with_text_size(TextSize::Body.px()),
            props,
        );
        ctx.record_action_source(pod.new_widget.id());
        (pod, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        _: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if self.text_color != prev.text_color {
            if let Some(color) = self.text_color {
                element.insert_prop(ContentColor { color });
            } else {
                element.remove_prop::<ContentColor>();
            }
        }
        SourceTextArea::replace_external_content(&mut element, &self.contents);
    }

    fn teardown(
        &self,
        _: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        _: &mut Self::ViewState,
        message: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<Action> {
        debug_assert!(message.remaining_path().is_empty());
        match message.take_message::<TextAction>() {
            Some(action) => match *action {
                TextAction::Changed(text) => MessageResult::Action((self.on_changed)(state, text)),
                TextAction::Entered(_) | TextAction::Cancelled => MessageResult::Stale,
            },
            None => MessageResult::Stale,
        }
    }
}

/// An upstream text area with source-editor shortcut filtering at the focused leaf.
pub(crate) struct SourceTextArea {
    inner: TextArea<true>,
    text_size: f32,
}

impl SourceTextArea {
    /// Create a no-wrap multiline editor using the standard body size.
    pub(crate) fn new(contents: &str) -> Self {
        let text_size = TextSize::Body.px();
        Self {
            inner: Self::text_area(contents, text_size),
            text_size,
        }
    }

    /// Choose a text size before installing the widget in a view tree.
    pub(crate) fn with_text_size(mut self, text_size: f32) -> Self {
        let contents = self.inner.text().into_iter().collect::<String>();
        self.inner = Self::text_area(&contents, text_size);
        self.text_size = text_size;
        self
    }

    fn text_area(contents: &str, text_size: f32) -> TextArea<true> {
        TextArea::new_editable(contents)
            .with_insert_newline(InsertNewline::OnEnter)
            .with_word_wrap(false)
            .with_style(StyleProperty::FontSize(text_size))
            .with_style(StyleProperty::FontFamily(FontFamily::Single(
                FontFamilyName::Generic(GenericFamily::Monospace),
            )))
    }

    /// Replace a genuinely external value while leaving matching reactive rebuilds untouched.
    ///
    /// Matching text retains the upstream editor, including its cursor, selection, and styles.
    /// A different external value intentionally resets those transient editing details.
    pub(crate) fn replace_external_content(this: &mut WidgetMut<'_, Self>, contents: &str) {
        if this.widget.inner.text() != contents {
            this.widget.inner = Self::text_area(contents, this.widget.text_size);
            this.ctx.request_layout();
            this.ctx.request_render();
        }
    }

    /// Change the rendered text size while retaining the editor's text and selection.
    pub(crate) fn set_text_size(this: &mut WidgetMut<'_, Self>, text_size: f32) {
        if (this.widget.text_size - text_size).abs() > f32::EPSILON {
            let inner = std::mem::replace(&mut this.widget.inner, Self::text_area("", text_size));
            this.widget.inner = inner.with_style(StyleProperty::FontSize(text_size));
            this.widget.text_size = text_size;
            this.ctx.request_layout();
            this.ctx.request_render();
        }
    }
}

impl Widget for SourceTextArea {
    type Action = TextAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        self.inner.on_pointer_event(ctx, props, event);
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if let TextEvent::Keyboard(key) = event
            && key.state == KeyState::Down
            && blocks_text_history_key(&key.key, key.modifiers)
        {
            ctx.set_handled();
            return;
        }
        self.inner.on_text_event(ctx, props, event);
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        self.inner.on_access_event(ctx, props, event);
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        self.inner.on_anim_frame(ctx, props, interval);
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        self.inner.register_children(ctx);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, props: &mut PropertiesMut<'_>, event: &Update) {
        self.inner.update(ctx, props, event);
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        self.inner.property_changed(ctx, property_type);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross: Option<Length>,
    ) -> Length {
        self.inner.measure(ctx, props, axis, len_req, cross)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, props: &PropertiesRef<'_>, size: Size) {
        self.inner.layout(ctx, props, size);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        self.inner.paint(ctx, props, painter);
    }

    fn accessibility_role(&self) -> Role {
        self.inner.accessibility_role()
    }

    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        self.inner.accessibility(ctx, props, node);
    }

    fn children_ids(&self) -> ChildrenIds {
        self.inner.children_ids()
    }

    fn accepts_focus(&self) -> bool {
        self.inner.accepts_focus()
    }

    fn accepts_text_input(&self) -> bool {
        self.inner.accepts_text_input()
    }

    fn get_cursor(&self, ctx: &QueryCtx<'_>, point: Point) -> CursorIcon {
        self.inner.get_cursor(ctx, point)
    }

    fn get_debug_text(&self) -> Option<String> {
        self.inner.get_debug_text()
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
    fn focused_source_area_blocks_history_keys_and_accepts_typing() {
        let area = SourceTextArea::new("a").prepare();
        let area_id = area.id();
        let host = ShortcutHost::new(area)
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
        for character in ["z", "Y"] {
            harness.process_text_event(key(Key::Character(character.into()), modifiers));
            assert!(harness.pop_action_erased().is_none());
        }

        harness.process_text_event(key(Key::Character("b".into()), Modifiers::empty()));
        let changed = harness.pop_action::<TextAction>().map(|(action, _)| action);
        assert!(matches!(changed, Some(TextAction::Changed(text)) if text.contains('b')));
        assert!(harness.pop_action::<AppAction>().is_none());
    }
}
