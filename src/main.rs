// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Runebender, a font editor and headless font-tool executable built with
//! Xilem and the workspace's `runebender-core` library.

// The browser shares editor code whose desktop-only actions are intentionally dormant.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

mod actions;
#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(not(target_arch = "wasm32"))]
mod cli;
mod edit;
#[cfg(not(target_arch = "wasm32"))]
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
#[cfg(not(target_arch = "wasm32"))]
use winit::dpi::LogicalSize;
#[cfg(not(target_arch = "wasm32"))]
use winit::error::EventLoopError;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::{FlexExt as _, FlexSpacer, canvas, flex_col, flex_row, sized_box};
#[cfg(not(target_arch = "wasm32"))]
use xilem::{EventLoop, EventLoopBuilder, Xilem};

use edit::session::Session;
use edit::*;
#[cfg(not(target_arch = "wasm32"))]
use launch::*;
use model::FontModel;
use platform::*;
use runebender_core::analysis::category::GlyphCategory;
use view::canvas::editor::editor;
use view::canvas::grid::{Cell, CellMetrics, GridEvent, cells_of, grid};
use view::chrome::*;
use view::design::{ButtonShape, ControlSize, Radius, Region, Space, Stroke, TextSize};
use view::panels::{
    chat::*, editor::*, editor_info::*, info::*, local_ai::*, nodes::*, preview::*, sections::*,
    tabs::*,
};
use view::recipes::button;
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

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    match cli::run() {
        cli::Startup::Exit(code) => code,
        cli::Startup::Editor(font) => match run(EventLoop::with_user_event(), font.as_deref()) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                std::process::ExitCode::FAILURE
            }
        },
    }
}
