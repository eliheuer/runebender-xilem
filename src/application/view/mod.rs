// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! What the window shows: the canvases, the panels, the chrome, the
//! render tree, the design system, the recipes, and the theme.

use self::design::{INPUT_BASELINE_OFFSET, INPUT_HORIZONTAL_INSET, INPUT_INSET, TextSize};
use masonry::layout::Length;

pub(crate) mod canvas;
pub(crate) mod chrome;
pub(crate) mod design;
pub(crate) mod panels;
pub(crate) mod recipes;
pub(crate) mod render;
pub(crate) mod theme;

/// The interface font that ships with the editor.
pub(crate) const UI_FONT: &[u8] = include_bytes!("../../../assets/fonts/VirtuaGrotesk-Regular.ttf");
/// The interface font's family name.
pub(crate) const UI_FONT_FAMILY: &str = "Virtua Grotesk";

/// Property defaults shared by every application host.
pub(crate) fn default_property_set() -> masonry::core::DefaultProperties {
    let mut properties = masonry::theme::default_property_set();
    properties.insert::<masonry::widgets::TextInput, _>(masonry::properties::Padding {
        left: Length::px(INPUT_HORIZONTAL_INSET),
        right: Length::px(INPUT_HORIZONTAL_INSET),
        top: Length::px(INPUT_INSET + INPUT_BASELINE_OFFSET),
        bottom: Length::px(INPUT_INSET - INPUT_BASELINE_OFFSET),
    });
    properties
}

/// A label in the interface font.
pub(crate) fn label(text: impl Into<masonry::core::ArcStr>) -> xilem::view::Label {
    xilem::view::label(text)
        .font(UI_FONT_FAMILY)
        .text_size(TextSize::Body.px())
}

/// Editable text in the interface font.
pub(crate) fn text_input<F, State, Action: 'static>(
    contents: String,
    on_changed: F,
) -> xilem::view::TextInput<State, Action>
where
    F: Fn(&mut State, String) -> Action + Send + Sync + 'static,
    State: 'static,
{
    xilem::view::text_input(contents, on_changed)
        .font(UI_FONT_FAMILY)
        .text_size(TextSize::Body.px())
}
