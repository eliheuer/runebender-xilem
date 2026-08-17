// Copyright 2026 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Editor left sidebar: mini glyph overview with search, so a
//! designer can switch glyphs without leaving the editor. Parity
//! target is runebender-web's `EditorSidebar.vue` Overview tab
//! (Shapes and Axes tabs come later).

use masonry::layout::AsUnit;
use xilem::WidgetView;
use xilem::core::one_of::Either;
use xilem::style::Style;
use xilem::view::{
    CrossAxisAlignment, FlexExt, button, flex_col, flex_row, label, sized_box, text_input,
};

use crate::components::{GridScrollAction, glyph_view, grid_scroll_handler};
use crate::data::AppState;
use crate::model::{glyph_renderer, read_workspace};
use crate::theme;

/// Total sidebar tile width, matching the grid tab's side panels.
pub const EDITOR_SIDEBAR_WIDTH: f64 = 220.0;

const COLUMNS: usize = 4;
const CELL_WIDTH: f64 = 46.0;
const CELL_PREVIEW_HEIGHT: f64 = 40.0;
const CELL_HEIGHT: f64 = 60.0;
const GAP: f64 = 4.0;

/// One mini cell: glyph preview + name, jumps the editor on click.
fn mini_cell(
    name: String,
    path: Option<kurbo::BezPath>,
    upm: f64,
    is_current: bool,
) -> impl WidgetView<AppState> + use<> {
    let preview = match path {
        Some(path) if !path.is_empty() => Either::A(
            glyph_view(path, CELL_WIDTH - 6.0, CELL_PREVIEW_HEIGHT, upm)
                .color(theme::grid::GLYPH_COLOR),
        ),
        _ => Either::B(label("")),
    };
    let name_color = if is_current {
        theme::grid::CELL_SELECTED_OUTLINE
    } else {
        theme::grid::CELL_TEXT
    };
    let jump_name = name.clone();
    let display_name = if name.len() > 7 {
        format!("{}…", &name[..6])
    } else {
        name
    };
    // Strip the Button widget's default padding so four cells fit
    // in the 220px tile.
    button(
        sized_box(
            flex_col((
                sized_box(preview).height(CELL_PREVIEW_HEIGHT.px()),
                label(display_name).text_size(9.0).color(name_color),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(2.px()),
        )
        .width(CELL_WIDTH.px())
        .height(CELL_HEIGHT.px()),
        move |state: &mut AppState| {
            state.jump_to_glyph(jump_name.clone());
        },
    )
    .padding(0.px())
}

/// The sidebar tile: search input on top, scrollable mini glyph
/// grid below. Windowed like the main grid: only visible rows get
/// bezpaths built.
pub fn editor_sidebar(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let Some(workspace_arc) = state.active_workspace() else {
        return Either::B(label("No font"));
    };
    let workspace = read_workspace(&workspace_arc);
    let upm = workspace.units_per_em.unwrap_or(1000.0);
    let current_glyph = state
        .editor_session
        .as_ref()
        .map(|s| s.glyph.name.clone())
        .unwrap_or_default();

    // Filter by case-insensitive substring of the glyph name.
    let query = state.sidebar_search.trim().to_lowercase();
    let all_names = workspace.glyph_names();
    let filtered: Vec<&str> = all_names
        .iter()
        .filter(|n| query.is_empty() || n.to_lowercase().contains(&query))
        .map(|s| s.as_str())
        .collect();

    // Window to the visible rows. Chrome: top bar (~90) + search
    // input (~40) + tile padding.
    let visible_rows = ((state.window_height - 230.0).max(CELL_HEIGHT)
        / (CELL_HEIGHT + GAP))
        .floor() as usize;
    let total_rows = filtered.len().div_ceil(COLUMNS);
    let max_scroll = total_rows.saturating_sub(visible_rows);
    let scroll_row = state.sidebar_scroll_row.min(max_scroll);
    let start = scroll_row * COLUMNS;
    let end = ((scroll_row + visible_rows) * COLUMNS).min(filtered.len());
    let visible = if start < filtered.len() {
        &filtered[start..end]
    } else {
        &[]
    };

    let rows: Vec<_> = visible
        .chunks(COLUMNS)
        .map(|chunk| {
            let cells: Vec<_> = chunk
                .iter()
                .map(|name| {
                    let path = workspace.get_glyph(name).map(|glyph| {
                        glyph_renderer::glyph_to_bezpath_with_components(glyph, &workspace)
                    });
                    mini_cell((*name).to_string(), path, upm, **name == current_glyph)
                })
                .collect();
            flex_row(cells).gap(GAP.px())
        })
        .collect();

    drop(workspace);

    let grid = grid_scroll_handler(
        flex_col(rows).gap(GAP.px()),
        move |state: &mut AppState, action| {
            if let GridScrollAction::Scroll(delta) = action {
                let row = state.sidebar_scroll_row as i64 + delta as i64;
                state.sidebar_scroll_row = row.clamp(0, max_scroll as i64) as usize;
            }
        },
    );

    Either::A(
        sized_box(
            flex_col((
                text_input(
                    state.sidebar_search.clone(),
                    |state: &mut AppState, new_value: String| {
                        if state.sidebar_search != new_value {
                            state.sidebar_search = new_value;
                            state.sidebar_scroll_row = 0;
                        }
                    },
                )
                .placeholder("Search"),
                grid.flex(1.0),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(6.px()),
        )
        .width(EDITOR_SIDEBAR_WIDTH.px())
        .padding(6.0.px())
        .background_color(theme::panel::BACKGROUND)
        .corner_radius(8.0.px()),
    )
}
