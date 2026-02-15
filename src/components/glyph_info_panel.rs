// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Glyph info panel for displaying details about the selected glyph
//!
//! Shows glyph name, metrics (LSB, width, RSB), kerning groups, unicode, etc.

use masonry::properties::types::AsUnit;
use xilem::WidgetView;
use xilem::core::one_of::Either;
use xilem::style::Style;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, label, sized_box};

use crate::data::AppState;
use crate::theme;

/// Width of the glyph info panel
pub const GLYPH_INFO_PANEL_WIDTH: f64 = 220.0;

/// Glyph info panel view for the right sidebar
pub fn glyph_info_panel(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let content = if let Some(ref glyph_name) = state.selected_glyph {
        // Get glyph data if available
        let glyph_data = state.active_workspace().and_then(|w| {
            let workspace = w.read().unwrap();
            workspace.get_glyph(glyph_name).map(|g| {
                (
                    g.name.clone(),
                    g.width,
                    g.codepoints.clone(),
                    g.left_group.clone(),
                    g.right_group.clone(),
                    g.contours.len(),
                )
            })
        });

        if let Some((name, width, codepoints, left_group, right_group, contour_count)) = glyph_data
        {
            Either::A(glyph_info_content(
                name,
                width,
                codepoints,
                left_group,
                right_group,
                contour_count,
            ))
        } else {
            Either::B(no_selection_content())
        }
    } else {
        Either::B(no_selection_content())
    };

    sized_box(content)
        .width(GLYPH_INFO_PANEL_WIDTH.px())
        .expand_height()
        .background_color(theme::panel::BACKGROUND)
        .border_color(theme::panel::OUTLINE)
        .border_width(1.5)
        .corner_radius(theme::size::PANEL_RADIUS)
}

/// Content when a glyph is selected
fn glyph_info_content(
    name: String,
    width: f64,
    codepoints: Vec<char>,
    left_group: Option<String>,
    right_group: Option<String>,
    contour_count: usize,
) -> impl WidgetView<AppState> + use<> {
    // Format unicode codepoints
    let unicode_display = if codepoints.is_empty() {
        "No Selection".to_string()
    } else {
        codepoints
            .iter()
            .map(|c| format!("{:04X}", *c as u32))
            .collect::<Vec<_>>()
            .join(", ")
    };

    // Format kerning groups
    let left_group_display = left_group
        .as_ref()
        .map(|s| s.replace("public.kern1.", ""))
        .unwrap_or_else(|| "(empty)".to_string());
    let right_group_display = right_group
        .as_ref()
        .map(|s| s.replace("public.kern2.", ""))
        .unwrap_or_else(|| "(empty)".to_string());

    flex_col((
        // Glyph Name header
        info_row_header("Glyph Name"),
        info_row_value(&name),
        sized_box(label("")).height(8.px()),
        // Metrics section
        info_row_header("Width"),
        info_row_value(&format!("{:.0}", width)),
        sized_box(label("")).height(8.px()),
        // Kerning Groups section
        info_row_header("Kerning Groups"),
        info_row_label_value("Left", &left_group_display),
        info_row_label_value("Right", &right_group_display),
        sized_box(label("")).height(8.px()),
        // Unicode section
        info_row_header("Unicode"),
        info_row_value(&unicode_display),
        sized_box(label("")).height(8.px()),
        // Stats section
        info_row_header("Contours"),
        info_row_value(&format!("{}", contour_count)),
    ))
    .gap(2.px())
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .padding(12.0)
}

/// Content when no glyph is selected
fn no_selection_content() -> impl WidgetView<AppState> + use<> {
    flex_col((
        info_row_header("Glyph Name"),
        info_row_value("No Selection"),
        sized_box(label("")).height(8.px()),
        info_row_header("Unicode"),
        info_row_value("No Selection"),
    ))
    .gap(2.px())
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .padding(12.0)
}

/// Header row for a section
fn info_row_header(text: &str) -> impl WidgetView<AppState> + use<> {
    label(text.to_string())
        .text_size(16.0)
        .color(theme::grid::CELL_SELECTED_OUTLINE)
}

/// Value row
fn info_row_value(value: &str) -> impl WidgetView<AppState> + use<> {
    label(value.to_string())
        .text_size(16.0)
        .color(theme::grid::CELL_TEXT)
}

/// Row with label and value side by side
fn info_row_label_value(label_text: &str, value_text: &str) -> impl WidgetView<AppState> + use<> {
    flex_row((
        sized_box(
            label(label_text.to_string())
                .text_size(16.0)
                .color(theme::grid::CELL_TEXT),
        )
        .width(50.px()),
        label(value_text.to_string())
            .text_size(16.0)
            .color(theme::grid::CELL_TEXT),
    ))
    .gap(8.px())
}
