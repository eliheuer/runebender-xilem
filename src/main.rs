// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A font editor built on the Linebender ecosystem.

// The browser build reuses this desktop crate root, so some platform-only actions
// are compiled but unused on WASM.
#![cfg_attr(
    target_arch = "wasm32",
    allow(
        dead_code,
        reason = "the browser reuses desktop modules with platform-only actions"
    )
)]

mod application;

// Keep the application vocabulary available at the crate root. This lets views
// say `crate::workspace` and `crate::view` while the files themselves live under
// the single `application/` boundary described in ARCHITECTURE.md.
#[cfg(target_arch = "wasm32")]
pub(crate) use application::browser;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use application::cli;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use application::launch;
pub(crate) use application::{
    actions, editor as edit, font_model as model, platform, view, widgets, workspace,
};

// Transitional internal prelude for application modules that still import
// `crate::*`. Keep this list explicit so new dependencies are visible in review;
// new modules should import their dependencies directly.
#[cfg(not(target_arch = "wasm32"))]
use std::process::ExitCode;
use std::{path::Path as FsPath, sync::Arc};

use edit::session::Session;
use edit::{chat, local_ai, metaballs, nodes, session, text_tool};
#[cfg(not(target_arch = "wasm32"))]
use launch::run;
use masonry::layout::{Dim, Length};
use masonry::properties::{Dimensions, types::CrossAxisAlignment};
use model::FontModel;
#[cfg(not(target_arch = "wasm32"))]
use platform::screenshot;
use platform::{dialogs, export, host};
use runebender::GlyphCategory;
use view::canvas::editor::editor;
use view::canvas::grid::{Cell, CellMetrics, GridEvent, cells_of, grid};
use view::chrome::{direction_chips, header_tools, marks_bar, status, titlebar};
use view::design::{
    ButtonShape, ControlSize, INPUT_BASELINE_OFFSET, INPUT_HORIZONTAL_INSET, INPUT_INSET, Radius,
    Region, Space, Stroke, TextSize, column as xcolumn, row as xrow,
};
use view::panels::chat::chat_panel;
use view::panels::editor::{editor_pane, overview};
use view::panels::editor_info::{
    compare_section, dimensions_section, features_section, groups_section, kerning_section,
    related_section,
};
use view::panels::info::info_panel;
use view::panels::local_ai::local_ai_panel;
use view::panels::nodes::nodes_pane;
use view::panels::preview::{glyph_preview, preview_strip};
use view::panels::sections::{
    axes_section, background_section, coordinates_section, curves_section, font_advanced_section,
    font_info_section, layers_section, mark_section, masters_section, measure_section, metric_bufs,
    path_operations_section, shaping_section, transformations_section,
};
use view::panels::tabs::{Rail, editor_nav, sidebar, tab_chip, tab_strip};
use view::recipes::button;
use view::render::{bottom_keyline, px32, top_keyline};
use view::theme::Palette;
use view::{canvas, design, recipes};
use widgets::drag_region::drag_region;
use widgets::icon_button::icon_button;
use widgets::scroll_viewport::portal;
use widgets::{icon_button, input_typography, menu_shell, preview_blur, shortcuts};
#[cfg(not(target_arch = "wasm32"))]
use winit::{dpi::LogicalSize, error::EventLoopError};
use workspace::{
    AppState, DirtyDecision, FontDataSnapshot, MetadataEdit, Mode, OverviewEditBatch, Sel, Sort,
    Tab, TextContext, Tool, Workspace,
};
use xilem::view::{FlexExt as _, FlexSpacer, canvas, flex_col, flex_row, sized_box};
#[cfg(not(target_arch = "wasm32"))]
use xilem::{EventLoop, EventLoopBuilder, Xilem};
use xilem::{WidgetView, style::Style};

/// The interface font that ships with the editor: Virtua Grotesk, the
/// same family as the demo font. Registered at launch.
pub(crate) const UI_FONT: &[u8] = include_bytes!("../assets/fonts/VirtuaGrotesk-Regular.ttf");
/// Its family name, as the font's name table spells it.
pub(crate) const UI_FONT_FAMILY: &str = "Virtua Grotesk";

/// Native and headless rendering share the same property defaults.
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

/// A label in the interface font. Every label goes through here so
/// the family is set in one place.
pub(crate) fn label(text: impl Into<masonry::core::ArcStr>) -> xilem::view::Label {
    xilem::view::label(text)
        .font(UI_FONT_FAMILY)
        .text_size(TextSize::Body.px())
}

/// Editable interface text uses the same family and size as labels.
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

#[cfg(not(target_arch = "wasm32"))]
fn main() -> ExitCode {
    match cli::run() {
        cli::Startup::Exit(code) => code,
        cli::Startup::Editor(font) => match run(EventLoop::with_user_event(), font.as_deref()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        },
    }
}
