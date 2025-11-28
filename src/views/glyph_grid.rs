// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Glyph grid view - displays all glyphs in a scrollable grid

use kurbo::BezPath;
use masonry::properties::types::{AsUnit, UnitPoint};
use xilem::core::one_of::Either;
use xilem::style::Style;
use xilem::view::{
    button, flex_col, flex_row, label, portal, sized_box, transformed, zstack,
    ChildAlignment, ZStackExt,
};
use xilem::WidgetView;

use crate::components::{
    create_master_infos, glyph_view, keyboard_shortcuts, master_toolbar_view,
};
use crate::data::AppState;
use crate::glyph_renderer;
use crate::theme;
use crate::theme::size::{UI_PANEL_GAP, UI_PANEL_MARGIN, TOOLBAR_ITEM_SIZE, TOOLBAR_PADDING};
use crate::workspace;

// ===== Glyph Grid Tab View =====

/// Height of a toolbar (button size + padding on both sides)
const TOOLBAR_HEIGHT: f64 = TOOLBAR_ITEM_SIZE + TOOLBAR_PADDING * 2.0;

/// Tab 0: Glyph grid view with header and floating toolbar
pub fn glyph_grid_tab(
    state: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
    zstack((
        // Keyboard shortcut handler (invisible, handles Cmd+S)
        keyboard_shortcuts(|state: &mut AppState| {
            state.save_workspace();
        }),
        // Background: the glyph grid with top margin for toolbar
        flex_col((
            // Top margin to make room for floating toolbar
            sized_box(label("")).height((TOOLBAR_HEIGHT + UI_PANEL_MARGIN).px()),
            glyph_grid_view(state),
        ))
        .background_color(theme::app::BACKGROUND),
        // Top-left: File info panel
        transformed(file_info_panel(state))
            .translate((UI_PANEL_MARGIN, UI_PANEL_MARGIN))
            .alignment(ChildAlignment::SelfAligned(UnitPoint::TOP_LEFT)),
        // Top-right: Master toolbar (if designspace)
        transformed(
            flex_row((
                // Master toolbar (only shown when designspace is loaded)
                master_toolbar_panel(state),
            ))
            .gap(UI_PANEL_GAP.px())
        )
        .translate((-UI_PANEL_MARGIN, UI_PANEL_MARGIN))
        .alignment(ChildAlignment::SelfAligned(UnitPoint::TOP_RIGHT)),
    ))
}

/// File info panel showing the loaded file path and last save time
fn file_info_panel(
    state: &AppState,
) -> impl WidgetView<AppState> + use<> {
    // Get file path (shortened to last 3 components)
    let path_display = state
        .loaded_file_path()
        .map(|p| shorten_path(&p, 3))
        .unwrap_or_else(|| "No file loaded".to_string());

    // Get last saved info
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
        .cross_axis_alignment(xilem::view::CrossAxisAlignment::Start),
    )
    .padding(12.0)
    .background_color(theme::panel::BACKGROUND)
    .border_color(theme::panel::OUTLINE)
    .border_width(1.5)
    .corner_radius(theme::size::PANEL_RADIUS)
}

/// Master toolbar panel for glyph grid - only shown when designspace is loaded
fn master_toolbar_panel(
    state: &AppState,
) -> impl WidgetView<AppState> + use<> {
    // Only show master toolbar when we have a designspace with multiple masters
    if let Some(ref designspace) = state.designspace {
        if designspace.masters.len() > 1 {
            let master_infos = create_master_infos(&designspace.masters);
            let active_master = designspace.active_master;

            return Either::A(master_toolbar_view(
                master_infos,
                active_master,
                |state: &mut AppState, index| {
                    // Switch to the selected master
                    if let Some(ref mut ds) = state.designspace {
                        ds.switch_master(index);
                    }
                },
            ));
        }
    }

    // No designspace or single master - return empty view
    Either::B(sized_box(label("")).width(0.px()).height(0.px()))
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

/// Build glyph data vector from workspace
fn build_glyph_data(
    state: &AppState,
    glyph_names: &[String],
) -> Vec<GlyphData> {
    if let Some(workspace_arc) = state.active_workspace() {
        let workspace = workspace_arc.read().unwrap();
        glyph_names
            .iter()
            .map(|name| build_single_glyph_data(&workspace, name))
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
        // Build path including components
        let path = glyph_renderer::glyph_to_bezpath_with_components(glyph, workspace);
        (
            name.to_string(),
            Some(path),
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
                .map(|(name, path_opt, codepoints, contour_count)| {
                    let is_selected =
                        selected_glyph.as_ref() == Some(name);
                    glyph_cell(
                        name.clone(),
                        path_opt.clone(),
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
    path_opt: Option<BezPath>,
    codepoints: Vec<char>,
    is_selected: bool,
    upm: f64,
    contour_count: usize,
) -> impl WidgetView<AppState> + use<> {
    let name_clone = glyph_name.clone();
    let display_name = format_display_name(&glyph_name);
    let unicode_display = format_unicode_display(&codepoints, contour_count);
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

// ===== Path Helpers =====

/// Shorten a path to show only the last N components with ".." prefix
fn shorten_path(path: &std::path::Path, components: usize) -> String {
    let parts: Vec<_> = path.components().collect();
    if parts.len() <= components {
        return path.display().to_string();
    }

    // Take the last N components
    let start = parts.len() - components;
    let shortened: std::path::PathBuf = parts[start..].iter().collect();
    format!("../{}", shortened.display())
}
