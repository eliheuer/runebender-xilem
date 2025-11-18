// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Glyph grid view - displays all glyphs in a scrollable grid

use std::sync::Arc;

use masonry::properties::types::AsUnit;
use xilem::core::one_of::Either;
use xilem::style::Style;
use xilem::view::{
    button, flex_col, flex_row, label, portal, sized_box,
};
use xilem::WidgetView;

use crate::components::glyph_view;
use crate::data::AppState;
use crate::glyph_renderer;
use crate::theme;
use crate::workspace;

// ===== Glyph Grid Tab View =====

/// Tab 0: Glyph grid view with header
pub fn glyph_grid_tab(
    state: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
    flex_col((glyph_grid_view(state),))
        .background_color(theme::app::BACKGROUND)
}

// ===== Glyph Grid View =====

/// Glyph grid showing all glyphs
fn glyph_grid_view(
    state: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
    let glyph_names = state.glyph_names();

    // Get UPM from workspace for uniform scaling
    let upm = get_upm_from_state(state);

    // Pre-compute glyph data
    let glyph_data = build_glyph_data(state, &glyph_names);

    const COLUMNS: usize = 8;
    let selected_glyph = state.selected_glyph.clone();

    // Build rows of glyph cells
    let rows_of_cells = build_glyph_rows(
        &glyph_data,
        COLUMNS,
        &selected_glyph,
        upm,
    );

    flex_col((
        sized_box(label("")).height(6.px()),
        flex_row((
            sized_box(label("")).width(6.px()),
            portal(flex_col(rows_of_cells).gap(6.px())),
            sized_box(label("")).width(6.px()),
        )),
    ))
}

// ===== Grid Building Helpers =====

/// Get UPM (units per em) from workspace state
fn get_upm_from_state(state: &AppState) -> f64 {
    state
        .workspace
        .as_ref()
        .and_then(|w| w.units_per_em)
        .unwrap_or(1000.0)
}

/// Type alias for glyph data tuple
type GlyphData = (
    String,
    Option<Arc<workspace::Glyph>>,
    Vec<char>,
    usize,
);

/// Build glyph data vector from workspace
fn build_glyph_data(
    state: &AppState,
    glyph_names: &[String],
) -> Vec<GlyphData> {
    if let Some(workspace) = &state.workspace {
        glyph_names
            .iter()
            .map(|name| build_single_glyph_data(workspace, name))
            .collect()
    } else {
        glyph_names
            .iter()
            .map(|name| (name.clone(), None, Vec::new(), 0))
            .collect()
    }
}

/// Build data for a single glyph
fn build_single_glyph_data(
    workspace: &workspace::Workspace,
    name: &str,
) -> GlyphData {
    if let Some(glyph) = workspace.get_glyph(name) {
        let count = glyph.contours.len();
        let codepoints = glyph.codepoints.clone();
        (
            name.to_string(),
            Some(Arc::new(glyph.clone())),
            codepoints,
            count,
        )
    } else {
        (name.to_string(), None, Vec::new(), 0)
    }
}

/// Build rows of glyph cells from glyph data
fn build_glyph_rows(
    glyph_data: &[GlyphData],
    columns: usize,
    selected_glyph: &Option<String>,
    upm: f64,
) -> Vec<impl WidgetView<AppState> + use<>> {
    glyph_data
        .chunks(columns)
        .map(|chunk| {
            let row_items: Vec<_> = chunk
                .iter()
                .map(|(name, glyph_opt, codepoints, contour_count)| {
                    let is_selected =
                        selected_glyph.as_ref() == Some(name);
                    glyph_cell(
                        name.clone(),
                        glyph_opt.clone(),
                        codepoints.clone(),
                        is_selected,
                        upm,
                        *contour_count,
                    )
                })
                .collect();
            flex_row(row_items).gap(6.px())
        })
        .collect()
}

// ===== Glyph Cell View =====

/// Individual glyph cell in the grid
fn glyph_cell(
    glyph_name: String,
    glyph_opt: Option<Arc<workspace::Glyph>>,
    codepoints: Vec<char>,
    is_selected: bool,
    upm: f64,
    contour_count: usize,
) -> impl WidgetView<AppState> + use<> {
    let name_clone = glyph_name.clone();
    let display_name = format_display_name(&glyph_name);
    let unicode_display = format_unicode_display(&codepoints, contour_count);
    let glyph_view_widget = build_glyph_view_widget(glyph_opt, upm);
    let (bg_color, border_color) = get_cell_colors(is_selected);

    sized_box(
        button(
            flex_col((
                glyph_view_widget,
                build_cell_labels(display_name, unicode_display),
            )),
            move |state: &mut AppState| {
                state.select_glyph(name_clone.clone());
                state.open_editor(name_clone.clone());
            },
        )
        .background_color(bg_color)
        .border_color(border_color),
    )
    .width(120.px())
    .height(120.px())
}

// ===== Cell Building Helpers =====

/// Format display name with truncation if too long
fn format_display_name(glyph_name: &str) -> String {
    if glyph_name.len() > 12 {
        format!("{}...", &glyph_name[..9])
    } else {
        glyph_name.to_string()
    }
}

/// Format Unicode codepoint display string
fn format_unicode_display(codepoints: &[char], contour_count: usize) -> String {
    if let Some(first_char) = codepoints.first() {
        format!("U+{:04X} {}", *first_char as u32, contour_count)
    } else {
        format!("{}", contour_count)
    }
}

/// Build the glyph view widget (either glyph preview or placeholder)
fn build_glyph_view_widget(
    glyph_opt: Option<Arc<workspace::Glyph>>,
    upm: f64,
) -> Either<
    impl WidgetView<AppState> + use<>,
    impl WidgetView<AppState> + use<>,
> {
    if let Some(glyph) = glyph_opt {
        let path = glyph_renderer::glyph_to_bezpath(&glyph);
        Either::A(
            sized_box(
                flex_col((
                    sized_box(label("")).height(4.px()),
                    glyph_view(path, 60.0, 60.0, upm)
                        .baseline_offset(0.06),
                )),
            )
            .height(78.px()),
        )
    } else {
        Either::B(
            sized_box(
                flex_col((
                    sized_box(label("")).height(4.px()),
                    label("?").text_size(40.0),
                )),
            )
            .height(78.px()),
        )
    }
}

/// Build the cell labels (name and Unicode)
fn build_cell_labels(
    display_name: String,
    unicode_display: String,
) -> impl WidgetView<AppState> + use<> {
    // Glyph name label (truncated if too long)
    let name_label = label(display_name)
        .text_size(14.0)
        .color(theme::text::PRIMARY);

    // Unicode codepoint and contour count label
    let unicode_label = label(unicode_display)
        .text_size(14.0)
        .color(theme::text::PRIMARY);

    // Container for both labels with vertical spacing
    sized_box(
        flex_col((
            name_label,
            unicode_label,
            sized_box(label("")).height(12.px()), // Bottom margin
        ))
        .gap(2.px()),
    )
    .height(36.px()) // Increased to accommodate larger bottom margin
}

/// Get cell colors based on selection state
fn get_cell_colors(
    is_selected: bool,
) -> (
    masonry::vello::peniko::Color,
    masonry::vello::peniko::Color,
) {
    if is_selected {
        (
            theme::grid::CELL_SELECTED_BACKGROUND,
            theme::grid::CELL_SELECTED_OUTLINE,
        )
    } else {
        (theme::grid::CELL_BACKGROUND, theme::grid::CELL_OUTLINE)
    }
}
