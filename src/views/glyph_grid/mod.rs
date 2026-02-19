// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Glyph grid view — the font overview tab showing all glyphs in a grid.
//!
//! Builds a responsive grid of glyph cells, a category filter sidebar, and
//! info/anatomy panels. Glyph cells (in the `glyph_cell` sub-module) render
//! a preview of each glyph's outline and handle click/double-click for
//! selection and opening the editor. The grid reflows based on window width
//! and supports arrow-key navigation and multi-select.

mod glyph_cell;

use std::collections::HashSet;

use kurbo::BezPath;
use masonry::properties::types::AsUnit;
use xilem::WidgetView;
use xilem::core::one_of::Either;
use xilem::style::Style;
use xilem::view::{CrossAxisAlignment, FlexExt, flex_col, flex_row, label, sized_box, zstack};

use glyph_cell::{GlyphCellAction, glyph_cell_view};

use crate::components::{
    CATEGORY_PANEL_WIDTH, GLYPH_INFO_PANEL_WIDTH, GlyphCategory, SystemToolbarButton,
    category_panel, create_master_infos, glyph_anatomy_panel, glyph_info_panel,
    grid_scroll_handler, mark_color_panel, master_toolbar_view, size_tracker, system_toolbar_view,
};
use crate::data::AppState;
use crate::model::glyph_renderer;
use crate::model::read_workspace;
use crate::model::workspace;
use crate::theme;

// ============================================================
// Bento Layout Constants
// ============================================================

/// Uniform gap between all tiles — panels, grid cells, outer padding
const BENTO_GAP: f64 = 6.0;

// ============================================================
// Glyph Grid Tab View
// ============================================================

/// Tab 0: Glyph grid view with bento tile layout
pub fn glyph_grid_tab(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    zstack((
        // Invisible: size tracker (measures window dimensions)
        size_tracker(|state: &mut AppState, width, height| {
            // Grid width = window - panels - outer padding - inner gaps
            state.window_width =
                width - CATEGORY_PANEL_WIDTH - GLYPH_INFO_PANEL_WIDTH - BENTO_GAP * 4.0;
            state.window_height = height;
        }),
        // Bento tile layout
        flex_col((
            // Row 1: File info stretches, toolbars fixed on right
            flex_row((
                file_info_panel(state).flex(1.0),
                master_toolbar_panel(state),
                system_toolbar_view(|state: &mut AppState, button| match button {
                    SystemToolbarButton::Save => {
                        state.save_workspace();
                    }
                }),
            ))
            .gap(BENTO_GAP.px()),
            // Row 2: Three-column content (fills remaining height)
            flex_row((
                flex_col((
                    category_panel(state.glyph_category_filter, |state: &mut AppState, cat| {
                        state.glyph_category_filter = cat;
                        state.grid_scroll_row = 0;
                    })
                    .flex(1.0),
                    mark_color_panel(
                        current_mark_color_index(state),
                        |state: &mut AppState, color_index| {
                            state.set_glyph_mark_color(color_index);
                        },
                    ),
                ))
                .gap(BENTO_GAP.px()),
                // Grid wrapped in scroll handler container
                // (captures scroll wheel, arrow keys, Cmd+S)
                grid_scroll_handler(
                    glyph_grid_view(state),
                    |state: &mut AppState, delta| {
                        let count = state.cached_filtered_count;
                        state.scroll_grid(delta, count);
                    },
                    |state: &mut AppState, direction| {
                        state.navigate_grid_selection(direction);
                    },
                    |state: &mut AppState| {
                        state.save_workspace();
                    },
                )
                .flex(1.0),
                sized_box(
                    flex_col((
                        glyph_info_panel(state),
                        glyph_anatomy_panel(state).flex(1.0),
                    ))
                    .gap(BENTO_GAP.px())
                    .cross_axis_alignment(CrossAxisAlignment::Fill),
                )
                .width(GLYPH_INFO_PANEL_WIDTH.px())
                .expand_height(),
            ))
            .gap(BENTO_GAP.px())
            .cross_axis_alignment(CrossAxisAlignment::Fill)
            .flex(1.0),
        ))
        .gap(BENTO_GAP.px())
        .padding(BENTO_GAP * 2.0)
        .background_color(theme::app::BACKGROUND),
    ))
}

// ============================================================
// Toolbar Panels
// ============================================================

/// File info panel showing the loaded file path and last save time
fn file_info_panel(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let path_display = state
        .loaded_file_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "No file loaded".to_string());

    let (save_display, save_color) = match state.last_saved_display() {
        Some(s) => (format!("Saved {}", s), theme::grid::CELL_SELECTED_OUTLINE),
        None => (
            "Not saved".to_string(),
            theme::mark::COLORS[2], // Yellow
        ),
    };

    sized_box(
        flex_col((
            label(path_display)
                .text_size(16.0)
                .color(theme::grid::CELL_TEXT),
            label(save_display).text_size(16.0).color(save_color),
        ))
        .gap(2.px())
        .cross_axis_alignment(CrossAxisAlignment::Start),
    )
    .expand_width()
    .padding(12.0)
    .background_color(theme::panel::BACKGROUND)
    .border_color(theme::panel::OUTLINE)
    .border_width(1.5)
    .corner_radius(theme::size::PANEL_RADIUS)
}

/// Master toolbar panel — only shown when designspace has multiple masters
fn master_toolbar_panel(state: &AppState) -> impl WidgetView<AppState> + use<> {
    if let Some(ref designspace) = state.designspace
        && designspace.masters.len() > 1
    {
        let master_infos = create_master_infos(&designspace.masters);
        let active_master = designspace.active_master;

        return Either::A(master_toolbar_view(
            master_infos,
            active_master,
            |state: &mut AppState, index| {
                if let Some(ref mut ds) = state.designspace {
                    ds.switch_master(index);
                }
            },
        ));
    }

    Either::B(sized_box(label("")).width(0.px()).height(0.px()))
}

// ============================================================
// Mark Color Helpers
// ============================================================

/// Look up the selected glyph's mark color palette index
fn current_mark_color_index(state: &AppState) -> Option<usize> {
    let glyph_name = state.selected_glyph.as_ref()?;
    let workspace_arc = state.active_workspace()?;
    let workspace = read_workspace(&workspace_arc);
    let glyph = workspace.get_glyph(glyph_name)?;
    glyph
        .mark_color
        .as_ref()
        .and_then(|s| rgba_string_to_palette_index(s))
}

// ============================================================
// Glyph Grid View
// ============================================================

/// Glyph grid showing only rows that fit in the visible area.
/// Scrolling is handled by `grid_scroll_handler` which adjusts
/// `state.grid_scroll_row`.
fn glyph_grid_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let columns = state.grid_columns();
    let visible = state.visible_grid_rows();
    let upm = get_upm_from_state(state);
    // window_width doesn't include the 2 flex_row gaps
    // between category | grid | info columns
    let grid_width = state.window_width - 2.0 * BENTO_GAP;
    let selected_glyphs = state.selected_glyphs.clone();

    // Build only the visible slice of glyph data —
    // filter first, then slice, then build bezpaths.
    let (visible_data, filtered_count) =
        build_visible_glyph_data(state, columns, visible, state.grid_scroll_row);
    // Cache the filtered count so scroll callbacks don't
    // have to re-iterate all glyphs.
    state.cached_filtered_count = filtered_count;

    let rows_of_cells = build_glyph_rows(&visible_data, columns, &selected_glyphs, upm, grid_width);

    // Each row flexes to fill available height evenly
    let flexy_rows: Vec<_> = rows_of_cells.into_iter().map(|row| row.flex(1.0)).collect();

    flex_col(flexy_rows)
        .gap(BENTO_GAP.px())
        .cross_axis_alignment(CrossAxisAlignment::Fill)
}

// ============================================================
// Grid Building Helpers
// ============================================================

/// Get UPM (units per em) from workspace state
fn get_upm_from_state(state: &AppState) -> f64 {
    state
        .active_workspace()
        .and_then(|w| read_workspace(&w).units_per_em)
        .unwrap_or(1000.0)
}

/// Type alias for glyph data tuple
/// (name, path with components, codepoints, contour count,
///  mark color palette index, column span)
type GlyphData = (
    String,
    Option<BezPath>,
    Vec<char>,
    usize,
    Option<usize>,
    usize,
);

/// Build glyph data for only the visible rows.
///
/// Filters by category (cheap — only checks codepoints), slices
/// to the visible window, THEN builds bezpaths (expensive) for
/// only those glyphs.
fn build_visible_glyph_data(
    state: &AppState,
    columns: usize,
    visible_rows: usize,
    scroll_row: usize,
) -> (Vec<GlyphData>, usize) {
    let workspace_arc = match state.active_workspace() {
        Some(w) => w,
        None => return (Vec::new(), 0),
    };
    let workspace = read_workspace(&workspace_arc);
    let category_filter = state.glyph_category_filter;

    // Step 1: Collect filtered glyph names (cheap — no bezpath)
    let all_names = workspace.glyph_names();
    let filtered_names: Vec<&str> = all_names
        .iter()
        .filter(|name| {
            if let Some(glyph) = workspace.get_glyph(name) {
                matches_category(&glyph.codepoints, category_filter)
            } else {
                false
            }
        })
        .map(|s| s.as_str())
        .collect();

    // Step 2: Slice to only the visible window
    let start = scroll_row * columns;
    let end = ((scroll_row + visible_rows) * columns).min(filtered_names.len());
    let total_filtered = filtered_names.len();
    if start > total_filtered {
        return (Vec::new(), total_filtered);
    }
    let visible_names = &filtered_names[start..end];

    // Step 3: Build full glyph data (with bezpaths) for
    // only the visible glyphs
    let upm = workspace.units_per_em.unwrap_or(1000.0);
    let data = visible_names
        .iter()
        .map(|name| build_single_glyph_data(&workspace, name, upm))
        .collect();
    (data, total_filtered)
}

/// Check if a glyph matches the category filter
fn matches_category(codepoints: &[char], category: GlyphCategory) -> bool {
    if category == GlyphCategory::All {
        return true;
    }
    if codepoints.is_empty() {
        return category == GlyphCategory::Other;
    }
    let glyph_category = GlyphCategory::from_codepoint(codepoints[0]);
    glyph_category == category
}

/// Build data for a single glyph
fn build_single_glyph_data(workspace: &workspace::Workspace, name: &str, upm: f64) -> GlyphData {
    if let Some(glyph) = workspace.get_glyph(name) {
        let count = glyph.contours.len();
        let codepoints = glyph.codepoints.clone();
        let path = glyph_renderer::glyph_to_bezpath_with_components(glyph, workspace);
        let mark_index = glyph
            .mark_color
            .as_ref()
            .and_then(|s| rgba_string_to_palette_index(s));
        let span = compute_col_span(name, glyph.width, upm);
        (
            name.to_string(),
            Some(path),
            codepoints,
            count,
            mark_index,
            span,
        )
    } else {
        (name.to_string(), None, Vec::new(), 0, None, 1)
    }
}

/// Compute how many columns a glyph cell should span based
/// on name length and advance width relative to UPM.
fn compute_col_span(name: &str, advance_width: f64, upm: f64) -> usize {
    // Span based on name length (chars that fit in one cell)
    let name_span = if name.len() <= 14 {
        1
    } else if name.len() <= 26 {
        2
    } else {
        3
    };

    // Span based on glyph advance width
    let width_span = if upm > 0.0 {
        let ratio = advance_width / upm;
        if ratio <= 1.5 {
            1
        } else if ratio <= 2.8 {
            2
        } else if ratio <= 4.0 {
            3
        } else {
            4
        }
    } else {
        1
    };

    name_span.max(width_span).min(4)
}

/// Convert an RGBA string to a palette index by matching
/// against the known palette strings
fn rgba_string_to_palette_index(rgba: &str) -> Option<usize> {
    theme::mark::RGBA_STRINGS.iter().position(|&s| s == rgba)
}

/// Compute the pixel width for a cell spanning `span` columns.
///
/// A span-1 cell is one grid unit. A span-N cell covers N grid
/// units plus the (N−1) gaps between them — bento-box style.
fn cell_pixel_width(span: usize, cell_unit: f64) -> f64 {
    let s = span as f64;
    s * cell_unit + (s - 1.0).max(0.0) * BENTO_GAP
}

/// Pack glyph data into rows of (index, span) pairs.
///
/// Each row's total span equals exactly `columns` — if items
/// don't fill the row, the last item is expanded to absorb the
/// remaining columns (bento-box: no gaps on the right).
fn pack_rows(glyph_data: &[GlyphData], columns: usize) -> Vec<Vec<(usize, usize)>> {
    let mut rows: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut row: Vec<(usize, usize)> = Vec::new();
    let mut row_span = 0;

    for (i, (.., col_span)) in glyph_data.iter().enumerate() {
        let span = (*col_span).min(columns);

        if row_span + span > columns && !row.is_empty() {
            // Expand last item to fill remaining columns
            if let Some(last) = row.last_mut() {
                last.1 += columns - row_span;
            }
            rows.push(std::mem::take(&mut row));
            row_span = 0;
        }

        row.push((i, span));
        row_span += span;
    }

    // Flush last row — expand last item to fill
    if !row.is_empty() {
        if let Some(last) = row.last_mut() {
            last.1 += columns - row_span;
        }
        rows.push(row);
    }

    rows
}

/// Build rows of glyph cells with span-aware packing.
///
/// Uses exact pixel widths so that spanning cells align
/// perfectly to the grid — a span-2 cell equals two single
/// cells plus the gap between them. Rows are always filled
/// to the full column count.
fn build_glyph_rows(
    glyph_data: &[GlyphData],
    columns: usize,
    selected_glyphs: &HashSet<String>,
    upm: f64,
    grid_width: f64,
) -> Vec<impl WidgetView<AppState> + use<>> {
    // Single-column cell width (grid unit)
    let cols = columns as f64;
    let cell_unit = (grid_width - (cols - 1.0) * BENTO_GAP) / cols;

    let packed = pack_rows(glyph_data, columns);
    let mut rows = Vec::new();

    for row_slots in &packed {
        let mut items: Vec<_> = Vec::new();
        let mut used = 0;

        for &(idx, span) in row_slots {
            let (name, path_opt, codepoints, _, mark_color, _) = &glyph_data[idx];
            let is_selected = selected_glyphs.contains(name);
            let w = cell_pixel_width(span, cell_unit);
            items.push(Either::A(
                sized_box(glyph_cell(
                    name.clone(),
                    path_opt.clone(),
                    codepoints.clone(),
                    is_selected,
                    upm,
                    *mark_color,
                ))
                .width(w.px())
                .expand_height(),
            ));
            used += span;
        }

        // Pad the last (partial) row with an invisible spacer
        if used < columns {
            let w = cell_pixel_width(columns - used, cell_unit);
            items.push(Either::B(
                sized_box(label("")).width(w.px()).expand_height(),
            ));
        }

        rows.push(
            flex_row(items)
                .gap(BENTO_GAP.px())
                .cross_axis_alignment(CrossAxisAlignment::Fill),
        );
    }

    rows
}

// ============================================================
// Glyph Cell
// ============================================================

/// Individual glyph cell in the grid — uses custom widget for
/// single-click (select), double-click (open editor),
/// and shift-click (toggle multi-select).
fn glyph_cell(
    glyph_name: String,
    path_opt: Option<BezPath>,
    codepoints: Vec<char>,
    is_selected: bool,
    upm: f64,
    mark_color: Option<usize>,
) -> impl WidgetView<AppState> + use<> {
    glyph_cell_view(
        glyph_name,
        path_opt,
        codepoints,
        is_selected,
        upm,
        mark_color,
        |state: &mut AppState, action| match action {
            GlyphCellAction::Select(name) => {
                state.select_glyph(name);
            }
            GlyphCellAction::ShiftSelect(name) => {
                state.toggle_glyph_selection(name);
            }
            GlyphCellAction::Open(name) => {
                state.select_glyph(name.clone());
                state.open_editor(name);
            }
        },
    )
}
