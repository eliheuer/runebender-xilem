// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Glyph grid view - displays all glyphs in a scrollable grid

use kurbo::BezPath;
use masonry::properties::types::AsUnit;
use xilem::core::one_of::Either;
use xilem::style::Style;
use xilem::view::{
    button, flex_col, flex_row, label, portal, sized_box, zstack,
    CrossAxisAlignment, FlexExt,
};
use xilem::WidgetView;

use crate::components::{
    category_panel, create_master_infos, glyph_info_panel, glyph_view,
    keyboard_shortcuts, master_toolbar_view, size_tracker,
    system_toolbar_view, GlyphCategory, SystemToolbarButton,
    CATEGORY_PANEL_WIDTH, GLYPH_INFO_PANEL_WIDTH,
};
use crate::data::AppState;
use crate::glyph_renderer;
use crate::theme;
use crate::workspace;

// ===== Bento Layout Constants =====

/// Uniform gap between all tiles — panels, grid cells, outer padding
const BENTO_GAP: f64 = 6.0;

// ===== Glyph Grid Tab View =====

/// Tab 0: Glyph grid view with bento tile layout
pub fn glyph_grid_tab(
    state: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
    zstack((
        // Invisible: size tracker (measures window width for responsive
        // grid columns)
        size_tracker(|state: &mut AppState, width| {
            // Grid width = window - panels - outer padding - inner gaps
            state.window_width = width
                - CATEGORY_PANEL_WIDTH
                - GLYPH_INFO_PANEL_WIDTH
                - BENTO_GAP * 4.0;
        }),
        // Invisible: keyboard shortcut handler (Cmd+S)
        keyboard_shortcuts(|state: &mut AppState| {
            state.save_workspace();
        }),
        // Bento tile layout
        flex_col((
            // Row 1: File info stretches, toolbars fixed on right
            flex_row((
                file_info_panel(state).flex(1.0),
                master_toolbar_panel(state),
                system_toolbar_view(
                    |state: &mut AppState, button| match button {
                        SystemToolbarButton::Save => {
                            state.save_workspace();
                        }
                    },
                ),
            ))
            .gap(BENTO_GAP.px()),
            // Row 2: Three-column content (fills remaining height)
            flex_row((
                category_panel(state),
                glyph_grid_view(state).flex(1.0),
                glyph_info_panel(state),
            ))
            .gap(BENTO_GAP.px())
            .cross_axis_alignment(CrossAxisAlignment::Fill)
            .flex(1.0),
        ))
        .gap(BENTO_GAP.px())
        .padding(BENTO_GAP)
        .background_color(theme::app::BACKGROUND),
    ))
}

// ===== Toolbar Panels =====

/// File info panel showing the loaded file path and last save time
fn file_info_panel(
    state: &AppState,
) -> impl WidgetView<AppState> + use<> {
    let path_display = state
        .loaded_file_path()
        .map(|p| shorten_path(&p, 3))
        .unwrap_or_else(|| "No file loaded".to_string());

    let save_display = state
        .last_saved_display()
        .map(|s| format!("Saved {}", s))
        .unwrap_or_else(|| "Not saved".to_string());

    sized_box(
        flex_col((
            label(path_display)
                .text_size(14.0)
                .color(theme::text::PRIMARY),
            label(save_display)
                .text_size(14.0)
                .color(theme::text::SECONDARY),
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
fn master_toolbar_panel(
    state: &AppState,
) -> impl WidgetView<AppState> + use<> {
    if let Some(ref designspace) = state.designspace
        && designspace.masters.len() > 1
    {
        let master_infos =
            create_master_infos(&designspace.masters);
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

// ===== Glyph Grid View =====

/// Glyph grid showing all glyphs in a scrollable portal
fn glyph_grid_view(
    state: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
    let glyph_names = state.glyph_names();
    let upm = get_upm_from_state(state);
    let glyph_data = build_glyph_data(state, &glyph_names);
    let columns = state.grid_columns();
    let selected_glyph = state.selected_glyph.clone();

    let rows_of_cells = build_glyph_rows(
        &glyph_data,
        columns,
        &selected_glyph,
        upm,
    );

    portal(
        flex_col((
            flex_col(rows_of_cells).gap(BENTO_GAP.px()),
            // Bottom spacer so last row is visible when scrolled
            sized_box(label("")).height(100.px()),
        ))
        .padding(BENTO_GAP / 2.0),
    )
}

// ===== Grid Building Helpers =====

/// Get UPM (units per em) from workspace state
fn get_upm_from_state(state: &AppState) -> f64 {
    state
        .active_workspace()
        .and_then(|w| w.read().unwrap().units_per_em)
        .unwrap_or(1000.0)
}

/// Type alias for glyph data tuple
/// (name, path with components, codepoints, contour count)
type GlyphData = (
    String,
    Option<BezPath>,
    Vec<char>,
    usize,
);

/// Build glyph data vector from workspace, filtered by category
fn build_glyph_data(
    state: &AppState,
    glyph_names: &[String],
) -> Vec<GlyphData> {
    let category_filter = state.glyph_category_filter;

    if let Some(workspace_arc) = state.active_workspace() {
        let workspace = workspace_arc.read().unwrap();
        glyph_names
            .iter()
            .filter_map(|name| {
                let data =
                    build_single_glyph_data(&workspace, name);
                if matches_category(&data.2, category_filter) {
                    Some(data)
                } else {
                    None
                }
            })
            .collect()
    } else {
        glyph_names
            .iter()
            .map(|name| (name.clone(), None, Vec::new(), 0))
            .collect()
    }
}

/// Check if a glyph matches the category filter
fn matches_category(
    codepoints: &[char],
    category: GlyphCategory,
) -> bool {
    if category == GlyphCategory::All {
        return true;
    }
    if codepoints.is_empty() {
        return category == GlyphCategory::Other;
    }
    let glyph_category =
        GlyphCategory::from_codepoint(codepoints[0]);
    glyph_category == category
}

/// Build data for a single glyph
fn build_single_glyph_data(
    workspace: &workspace::Workspace,
    name: &str,
) -> GlyphData {
    if let Some(glyph) = workspace.get_glyph(name) {
        let count = glyph.contours.len();
        let codepoints = glyph.codepoints.clone();
        let path = glyph_renderer::glyph_to_bezpath_with_components(
            glyph, workspace,
        );
        (name.to_string(), Some(path), codepoints, count)
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
                .map(|(name, path_opt, codepoints, _)| {
                    let is_selected =
                        selected_glyph.as_ref() == Some(name);
                    glyph_cell(
                        name.clone(),
                        path_opt.clone(),
                        codepoints.clone(),
                        is_selected,
                        upm,
                    )
                    .flex(1.0)
                })
                .collect();
            flex_row(row_items).gap(BENTO_GAP.px())
        })
        .collect()
}

// ===== Glyph Cell View =====

/// Individual glyph cell in the grid
fn glyph_cell(
    glyph_name: String,
    path_opt: Option<BezPath>,
    codepoints: Vec<char>,
    is_selected: bool,
    upm: f64,
) -> impl WidgetView<AppState> + use<> {
    let name_clone = glyph_name.clone();
    let display_name = format_display_name(&glyph_name);
    let unicode_display = format_unicode_display(&codepoints);
    let glyph_view_widget = build_glyph_view_widget(path_opt, upm);
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
    .height(100.px())
    .expand_width()
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
fn format_unicode_display(codepoints: &[char]) -> String {
    if let Some(first_char) = codepoints.first() {
        format!("U+{:04X}", *first_char as u32)
    } else {
        String::new()
    }
}

/// Build the glyph view widget (either glyph preview or placeholder)
fn build_glyph_view_widget(
    path_opt: Option<BezPath>,
    upm: f64,
) -> Either<
    impl WidgetView<AppState> + use<>,
    impl WidgetView<AppState> + use<>,
> {
    if let Some(path) = path_opt {
        Either::A(
            sized_box(
                flex_col((
                    sized_box(label("")).height(2.px()),
                    glyph_view(path, 50.0, 50.0, upm)
                        .baseline_offset(0.06),
                )),
            )
            .height(62.px()),
        )
    } else {
        Either::B(
            sized_box(
                flex_col((
                    sized_box(label("")).height(2.px()),
                    label("?").text_size(32.0),
                )),
            )
            .height(62.px()),
        )
    }
}

/// Build the cell labels (name and Unicode)
fn build_cell_labels(
    display_name: String,
    unicode_display: String,
) -> impl WidgetView<AppState> + use<> {
    let name_label = label(display_name)
        .text_size(12.0)
        .color(theme::text::PRIMARY);

    let unicode_label = label(unicode_display)
        .text_size(12.0)
        .color(theme::text::SECONDARY);

    sized_box(
        flex_col((name_label, unicode_label)).gap(2.px()),
    )
    .height(32.px())
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

// ===== Path Helpers =====

/// Shorten a path to show only the last N components with ".." prefix
fn shorten_path(
    path: &std::path::Path,
    components: usize,
) -> String {
    let parts: Vec<_> = path.components().collect();
    if parts.len() <= components {
        return path.display().to_string();
    }

    let start = parts.len() - components;
    let shortened: std::path::PathBuf =
        parts[start..].iter().collect();
    format!("../{}", shortened.display())
}
