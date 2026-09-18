// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Selectable, word-wrapping interface prose in Runebender's bundled font.
//!
//! Xilem's `prose` view provides selection and wrapping but does not expose a
//! font-family setter yet. This small view keeps those native text behaviors
//! while applying the same family and type scale as every other label.

use std::marker::PhantomData;

use masonry::core::{ArcStr, NewWidget, PropertySet, StyleProperty};
use masonry::properties::ContentColor;
use masonry::widgets::{self, TextArea};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

/// Selectable prose that wraps words to the width offered by its parent.
pub(crate) fn selectable_text<State, Action>(
    content: impl Into<ArcStr>,
) -> SelectableText<State, Action> {
    SelectableText {
        content: content.into(),
        color: None,
        size: crate::application::view::design::TextSize::Body.px(),
        phantom: PhantomData,
    }
}

/// The view created by [`selectable_text`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub(crate) struct SelectableText<State, Action> {
    content: ArcStr,
    color: Option<Color>,
    size: f32,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<State, Action> SelectableText<State, Action> {
    /// Set the text color.
    pub(crate) fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Set the text size in logical pixels.
    pub(crate) fn text_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl<State, Action> ViewMarker for SelectableText<State, Action> {}

impl<State: 'static, Action: 'static> View<State, Action, ViewCtx>
    for SelectableText<State, Action>
{
    type Element = Pod<widgets::Prose>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        let text_area = TextArea::new_immutable(&self.content)
            .with_style(StyleProperty::FontFamily(
                crate::application::view::UI_FONT_FAMILY.into(),
            ))
            .with_style(StyleProperty::FontSize(self.size))
            .with_word_wrap(true);
        let mut props = PropertySet::new();
        if let Some(color) = self.color {
            props.insert(ContentColor { color });
        }
        let text_area = NewWidget::new(text_area).with_props(props);
        (
            ctx.create_pod(widgets::Prose::from_text_area(text_area).with_clip(true)),
            (),
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        let mut text = widgets::Prose::text_mut(&mut element);
        if self.color != prev.color {
            if let Some(color) = self.color {
                text.insert_prop(ContentColor { color });
            } else {
                text.remove_prop::<ContentColor>();
            }
        }
        if self.content != prev.content {
            TextArea::reset_text(&mut text, &self.content);
        }
        if self.size != prev.size {
            TextArea::insert_style(&mut text, StyleProperty::FontSize(self.size));
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        _: &mut Self::ViewState,
        _message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        _state: &mut State,
    ) -> MessageResult<Action> {
        MessageResult::Stale
    }
}
