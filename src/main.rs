// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Runebender on xix. A font editor: glyph grid, glyph editor, sidebar.
//! See `docs/XILEM-GAPS.md` for what this build costs against the
//! same editor on GPUI.

mod actions;
mod edit;
mod launch;
mod model;
mod platform;
mod view;
mod widgets;
mod workspace;

use std::path::Path as FsPath;
use std::sync::Arc;

use crate::view::design::{column as xcolumn, row as xrow};
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use masonry::properties::types::CrossAxisAlignment;
/// Native and headless rendering share the same property defaults.
fn default_property_set() -> masonry::core::DefaultProperties {
    let mut properties = masonry::theme::default_property_set();
    use crate::view::design::{INPUT_BASELINE_OFFSET, INPUT_HORIZONTAL_INSET, INPUT_INSET};
    properties.insert::<masonry::widgets::TextInput, _>(masonry::properties::Padding {
        left: Length::px(INPUT_HORIZONTAL_INSET),
        right: Length::px(INPUT_HORIZONTAL_INSET),
        top: Length::px(INPUT_INSET + INPUT_BASELINE_OFFSET),
        bottom: Length::px(INPUT_INSET - INPUT_BASELINE_OFFSET),
    });
    properties
}
use crate::widgets::scroll_viewport::portal;
use winit::dpi::LogicalSize;
use winit::error::EventLoopError;
use xilem::style::Style;
use xilem::view::{
    FlexExt as _, FlexSpacer, button, canvas, flex_col, flex_row, sized_box, text_button,
};
use xilem::{EventLoop, EventLoopBuilder, WidgetView, Xilem};

use edit::session::Session;
use edit::*;
use launch::*;
use model::FontModel;
use platform::*;
use runebender_core::analysis::category::GlyphCategory;
use view::canvas::editor::editor;
use view::canvas::grid::{Cell, CellMetrics, GridEvent, cells_of, grid};
use view::chrome::*;
use view::design::{ControlSize, Radius, Region, Space, Stroke, TextSize};
use view::panels::{
    chat::*, editor::*, editor_info::*, info::*, local_ai::*, nodes::*, preview::*, sections::*,
    tabs::*,
};
use view::render::*;
use view::theme::Palette;
use view::*;
use widgets::drag_region::drag_region;
use widgets::icon_button::icon_button;
use widgets::*;
use workspace::*;

/// The interface font that ships with the editor: Virtua Grotesk, the
/// same family as the demo font. Registered at launch.
pub(crate) const UI_FONT: &[u8] = include_bytes!("../assets/fonts/VirtuaGrotesk-Regular.ttf");
/// Its family name, as the font's name table spells it.
pub(crate) const UI_FONT_FAMILY: &str = "Virtua Grotesk";

/// A label in the interface font. Every label goes through here so
/// the family is set in one place.
pub(crate) fn label(text: impl Into<masonry::core::ArcStr>) -> xilem::view::Label {
    xilem::view::label(text)
        .font(UI_FONT_FAMILY)
        .text_size(TextSize::Body.px())
}

/// Editable interface text uses the same family and size as labels.
fn text_input<F, State, Action: 'static>(
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

fn main() -> Result<(), EventLoopError> {
    run(EventLoop::with_user_event())
}
