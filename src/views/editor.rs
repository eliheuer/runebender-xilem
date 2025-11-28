// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Editor view - main glyph editing interface

use std::sync::Arc;

use kurbo::BezPath;
use masonry::properties::types::{AsUnit, UnitPoint};
use xilem::core::one_of::Either;
use xilem::style::Style;
use xilem::view::{
    ChildAlignment, ZStackExt, flex_col, flex_row, label, sized_box, text_input,
    transformed, zstack,
};
use xilem::WidgetView;

use crate::components::workspace_toolbar::WorkspaceToolbarButton;
use crate::components::{
    coordinate_panel, create_master_infos, edit_mode_toolbar_view, editor_view, glyph_view,
    master_toolbar_view, shapes_toolbar_view, text_direction_toolbar_view, workspace_toolbar_view,
};
use crate::data::AppState;
use crate::shaping::TextDirection;
use crate::theme;
use crate::theme::size::{UI_PANEL_GAP, UI_PANEL_MARGIN};
use crate::tools::{ToolBox, ToolId};
use crate::tools::shapes::ShapeType;

// ===== Editor Tab View =====

/// Tab 1: Editor view with toolbar floating over canvas
pub fn editor_tab(
    state: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
    let Some(session) = &state.editor_session else {
        // No session - show empty view (shouldn't happen)
        return Either::B(flex_col((label("No editor session"),)));
    };

    let current_tool = session.current_tool.id();
    let glyph_name = session.active_sort_name.clone().unwrap_or_else(|| "".to_string());
    let session_arc = Arc::new(session.clone());

    // Get current shape type if shapes tool is selected
    let current_shape = if let ToolBox::Shapes(shapes_tool) = &session.current_tool {
        shapes_tool.shape_type()
    } else {
        ShapeType::Rectangle // Default
    };

    // Get current text direction
    let current_text_direction = session.text_direction;

    // Determine which sub-toolbar to show
    let show_shapes_toolbar = current_tool == ToolId::Shapes;
    let show_text_direction_toolbar = current_tool == ToolId::Text;

    // Use zstack to layer UI elements over the canvas
    Either::A(zstack((
        // Background: the editor canvas (full screen)
        editor_view(
            session_arc.clone(),
            |state: &mut AppState, updated_session, save_requested| {
                state.update_editor_session(updated_session);
                if save_requested {
                    state.save_workspace();
                }
            },
        ),
        // Foreground: floating toolbars (edit mode + optional sub-toolbar) positioned in top-left
        transformed(
            flex_col((
                edit_mode_toolbar_view(
                    current_tool,
                    |state: &mut AppState, tool_id| {
                        state.set_editor_tool(tool_id);
                    },
                ),
                if show_shapes_toolbar {
                    Either::A(shapes_toolbar_view(
                        current_shape,
                        |state: &mut AppState, shape_type| {
                            state.set_shape_type(shape_type);
                        },
                    ))
                } else if show_text_direction_toolbar {
                    Either::B(Either::A(text_direction_toolbar_view(
                        current_text_direction,
                        |state: &mut AppState, direction| {
                            state.set_text_direction(direction);
                        },
                    )))
                } else {
                    Either::B(Either::B(label("")))
                },
            ))
            .cross_axis_alignment(xilem::view::CrossAxisAlignment::Start)
        )
        .translate((UI_PANEL_MARGIN, UI_PANEL_MARGIN))
        .alignment(ChildAlignment::SelfAligned(UnitPoint::TOP_LEFT)),
        // Bottom-left: glyph preview panel
        transformed(glyph_preview_pane(session_arc.clone(), glyph_name.clone()))
            .translate((UI_PANEL_MARGIN, -UI_PANEL_MARGIN))
            .alignment(ChildAlignment::SelfAligned(UnitPoint::BOTTOM_LEFT)),
        // Bottom-center-top: text buffer preview panel (above active glyph, standard margin)
        transformed(text_buffer_preview_pane_centered(session_arc.clone()))
            .translate((0.0, -(UI_PANEL_MARGIN + 140.0 + UI_PANEL_MARGIN)))
            .alignment(ChildAlignment::SelfAligned(UnitPoint::BOTTOM)),
        // Bottom-center-bottom: active glyph panel
        transformed(active_glyph_panel_centered(state))
            .translate((0.0, -UI_PANEL_MARGIN))
            .alignment(ChildAlignment::SelfAligned(UnitPoint::BOTTOM)),
        // Bottom-right: coordinate panel (locked to corner like workspace toolbar)
        transformed(coordinate_panel_from_session(&session_arc))
            .translate((-UI_PANEL_MARGIN, -UI_PANEL_MARGIN))
            .alignment(ChildAlignment::SelfAligned(UnitPoint::BOTTOM_RIGHT)),
        // Top-right: Master toolbar (if designspace) + Workspace toolbar (horizontal)
        transformed(
            flex_row((
                // Master toolbar (only shown when designspace is loaded)
                master_toolbar_panel(state),
                // Workspace toolbar for navigation
                workspace_toolbar_view(
                    |state: &mut AppState, button| {
                        match button {
                            WorkspaceToolbarButton::GlyphGrid => {
                                state.close_editor();
                            }
                        }
                    },
                ),
            ))
            .gap(UI_PANEL_GAP.px())
        )
        .translate((-UI_PANEL_MARGIN, UI_PANEL_MARGIN))
        .alignment(ChildAlignment::SelfAligned(UnitPoint::TOP_RIGHT)),
    )))
}

// ===== Helper Views =====

/// Master toolbar panel - only shown when a designspace is loaded
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
                        // Reload the current glyph from the new master
                        if let Some(ref session) = state.editor_session {
                            if let Some(glyph_name) = &session.active_sort_name {
                                // Re-create editor session with new master's glyph
                                state.open_editor(glyph_name.clone());
                            }
                        }
                    }
                },
            ));
        }
    }

    // No designspace or single master - return empty view
    Either::B(sized_box(label("")).width(0.px()).height(0.px()))
}

/// Helper to create coordinate panel from session data
fn coordinate_panel_from_session(
    session: &Arc<crate::edit_session::EditSession>,
) -> impl WidgetView<AppState> + use<> {
    tracing::debug!(
        "[coordinate_panel_from_session] Building view with \
         quadrant={:?}",
        session.coord_selection.quadrant
    );
    coordinate_panel(
        Arc::clone(session),
        |state: &mut AppState, updated_session| {
            tracing::debug!(
                "[coordinate_panel callback] Session updated, \
                 new quadrant={:?}",
                updated_session.coord_selection.quadrant
            );
            state.editor_session = Some(updated_session);
        },
    )
}

/// Glyph preview pane showing the rendered glyph
fn glyph_preview_pane(
    session: Arc<crate::edit_session::EditSession>,
    _glyph_name: String,
) -> impl WidgetView<AppState> + use<> {
    const PANEL_HEIGHT: f64 = 140.0;
    const PANEL_WIDTH: f64 = 240.0; // Match coordinate panel width
    const GLYPH_SIZE: f64 = 100.0; // Fit within 140px height with padding

    // Get the glyph outline path from the session
    let glyph_path = build_glyph_path(&session);
    let upm = session.ascender - session.descender;

    // Centered glyph preview with upward offset
    let glyph_preview = if !glyph_path.is_empty() {
        Either::A(
            glyph_view(glyph_path, GLYPH_SIZE, GLYPH_SIZE, upm)
                .color(theme::panel::GLYPH_PREVIEW)
        )
    } else {
        Either::B(label(""))
    };

    sized_box(
        flex_col((
            // Add spacing at top to push content up
            sized_box(label("")).height(0.px()),
            glyph_preview,
            // Add more spacing at bottom to shift visual center up
            sized_box(label("")).height(15.px()),
        ))
        .main_axis_alignment(xilem::view::MainAxisAlignment::Center)
        .cross_axis_alignment(xilem::view::CrossAxisAlignment::Center)
    )
    .width(PANEL_WIDTH.px())
    .height(PANEL_HEIGHT.px())
    .background_color(theme::panel::BACKGROUND)
    .border_color(theme::panel::OUTLINE)
    .border_width(1.5)
    .corner_radius(8.0)
}

/// Active glyph panel showing editable metrics (Glyphs app style)
/// Only shown when a glyph is active
fn active_glyph_panel_centered(
    state: &AppState,
) -> impl WidgetView<AppState> + use<> {
    const PANEL_HEIGHT: f64 = 140.0;
    const PANEL_WIDTH: f64 = 488.0; // Match text buffer preview width

    // Get session and glyph name
    let session = match &state.editor_session {
        Some(s) => s,
        None => return Either::B(sized_box(label("")).width(0.px()).height(0.px())),
    };

    let glyph_name = match &session.active_sort_name {
        Some(n) => n.clone(),
        None => return Either::B(sized_box(label("")).width(0.px()).height(0.px())),
    };

    // Only show if we have an active glyph
    if glyph_name.is_empty() {
        return Either::B(sized_box(label("")).width(0.px()).height(0.px()));
    }

    // Get current values
    let width = session.glyph.width;
    let lsb = session.glyph.left_side_bearing();
    let rsb = session.glyph.right_side_bearing();
    let left_group = session.glyph.left_group.as_ref().map(|s| s.as_str()).unwrap_or("");
    let right_group = session.glyph.right_group.as_ref().map(|s| s.as_str()).unwrap_or("");

    // Get kerning values
    let left_kern = state.get_left_kern();
    let right_kern = state.get_right_kern();

    // Format Unicode
    let unicode_display = if let Some(first_char) = session.glyph.codepoints.first() {
        format!("{:04X}", *first_char as u32)
    } else {
        String::from("")
    };

    // Row 1 (Top): Name and Unicode (both editable)
    // Widths: 346 (3 quarters) + 8 gap + 110 (1 quarter) = 464px (aligns with row 2)
    let top_row = flex_row((
        sized_box(
            text_input(
                glyph_name.clone(),
                |_state: &mut AppState, _new_value| {
                    // TODO: implement glyph name editing
                }
            )
            .text_alignment(parley::Alignment::Center)
        ).width(346.px()),
        sized_box(
            text_input(
                unicode_display,
                |_state: &mut AppState, _new_value| {
                    // TODO: implement unicode editing
                }
            )
            .text_alignment(parley::Alignment::Center)
        ).width(110.px()),
    ))
    .gap(8.px())
    .main_axis_alignment(xilem::view::MainAxisAlignment::Start);

    // Row 2 (Middle): Left kern, LSB, RSB, Right kern (all editable)
    // Widths: 4 × 110 + 3 × 8 gaps = 464px
    // NOTE: text_alignment is set before placeholder, but due to an upstream issue in Xilem 0.4.0,
    // placeholder text does not respect text_alignment and remains left-aligned.
    // See context/text-input-placeholder-alignment.md for details and upstream PR tracking.
    let middle_row = flex_row((
        sized_box(
            text_input(
                left_kern.map(|v| format!("{:.0}", v)).unwrap_or_default(),
                |state: &mut AppState, new_value| {
                    state.update_left_kern(new_value);
                }
            )
            .text_alignment(parley::Alignment::Center) // Placeholder won't center until upstream fix
            .placeholder("Kern")
        ).width(110.px()),
        sized_box(
            text_input(
                format!("{:.0}", lsb),
                |_state: &mut AppState, _new_value| {
                    // TODO: implement LSB editing
                }
            )
            .text_alignment(parley::Alignment::Center)
        ).width(110.px()),
        sized_box(
            text_input(
                format!("{:.0}", rsb),
                |_state: &mut AppState, _new_value| {
                    // TODO: implement RSB editing
                }
            )
            .text_alignment(parley::Alignment::Center)
        ).width(110.px()),
        sized_box(
            text_input(
                right_kern.map(|v| format!("{:.0}", v)).unwrap_or_default(),
                |state: &mut AppState, new_value| {
                    state.update_right_kern(new_value);
                }
            )
            .text_alignment(parley::Alignment::Center) // Placeholder won't center until upstream fix
            .placeholder("Kern")
        ).width(110.px()),
    ))
    .gap(8.px())
    .main_axis_alignment(xilem::view::MainAxisAlignment::Start);

    // Row 3 (Bottom): Left kern group, Width, Right kern group (all editable)
    // Widths: 149 + 8 + 150 + 8 + 149 = 464px
    // NOTE: Placeholder alignment issue same as row 2 - see context/text-input-placeholder-alignment.md
    let bottom_row = flex_row((
        sized_box(
            text_input(
                left_group.to_string(),
                |state: &mut AppState, new_value| {
                    state.update_left_group(new_value);
                }
            )
            .text_alignment(parley::Alignment::Center) // Placeholder won't center until upstream fix
            .placeholder("Group")
        ).width(149.px()),
        sized_box(
            text_input(
                format!("{:.0}", width),
                |state: &mut AppState, new_value| {
                    state.update_glyph_width(new_value);
                }
            )
            .text_alignment(parley::Alignment::Center)
        ).width(150.px()),
        sized_box(
            text_input(
                right_group.to_string(),
                |state: &mut AppState, new_value| {
                    state.update_right_group(new_value);
                }
            )
            .text_alignment(parley::Alignment::Center) // Placeholder won't center until upstream fix
            .placeholder("Group")
        ).width(149.px()),
    ))
    .gap(8.px())
    .main_axis_alignment(xilem::view::MainAxisAlignment::Start);

    // Combine all three rows with consistent 8px vertical gap
    let content = flex_col((top_row, middle_row, bottom_row))
        .gap(8.px())
        .main_axis_alignment(xilem::view::MainAxisAlignment::Center);

    Either::A(
        sized_box(content)
            .width(PANEL_WIDTH.px())
            .height(PANEL_HEIGHT.px())
            .background_color(theme::panel::BACKGROUND)
            .border_color(theme::panel::OUTLINE)
            .border_width(1.5)
            .corner_radius(8.0)
            .padding(12.0)
    )
}

// ===== Preview Pane Helpers =====

/// Build the glyph path from session paths
fn build_glyph_path(
    session: &crate::edit_session::EditSession,
) -> BezPath {
    let mut glyph_path = BezPath::new();
    for path in session.paths.iter() {
        glyph_path.extend(path.to_bezpath());
    }
    glyph_path
}

/// Format Unicode codepoint display string
fn format_unicode_display(
    session: &crate::edit_session::EditSession,
) -> String {
    if let Some(first_char) = session.glyph.codepoints.first() {
        format!("U+{:04X}", *first_char as u32)
    } else {
        String::new()
    }
}

/// Build the glyph preview view (either glyph or empty label)
fn build_glyph_preview(
    glyph_path: &BezPath,
    preview_size: f64,
    upm: f64,
) -> Either<
    impl WidgetView<AppState> + use<>,
    impl WidgetView<AppState> + use<>,
> {
    if !glyph_path.is_empty() {
        Either::A(
            glyph_view(
                glyph_path.clone(),
                preview_size,
                preview_size,
                upm,
            )
            .color(theme::panel::GLYPH_PREVIEW)
            .baseline_offset(0.15),
        )
    } else {
        Either::B(label(""))
    }
}

/// Build the glyph name and Unicode labels
fn build_glyph_labels(
    glyph_name: String,
    unicode_display: String,
) -> impl WidgetView<AppState> + use<> {
    sized_box(
        flex_col((
            label(glyph_name)
                .text_size(18.0)
                .color(theme::text::PRIMARY),
            label(unicode_display)
                .text_size(18.0)
                .color(theme::text::PRIMARY),
            sized_box(label("")).height(4.px()),
        ))
        .gap(2.px()),
    )
    .height(32.px())
}

/// Text buffer preview pane showing rendered glyphs from the font (mini preview mode)
/// Centered version with fixed width for displaying a line of text
fn text_buffer_preview_pane_centered(
    session: Arc<crate::edit_session::EditSession>,
) -> impl WidgetView<AppState> + use<> {
    // Panel dimensions to match other bottom panels
    const PANEL_HEIGHT: f64 = 140.0;
    // Width calculation for centered panel: window width - side panels - margins - gaps
    // At 1200px window: (1200 - 240*2 - 16*4) = 640px leaves 16px gaps
    const PANEL_WIDTH: f64 = 640.0; // Extended width for text buffer preview (now on top)

    // Only show if text buffer exists
    if session.text_buffer.is_none() {
        return Either::B(sized_box(label("")).height(PANEL_HEIGHT.px()));
    }

    // Get workspace reference to load glyphs
    let workspace = match &session.workspace {
        Some(ws) => ws,
        None => return Either::B(sized_box(label("")).width(0.px()).height(0.px())),
    };

    let buffer = session.text_buffer.as_ref().unwrap();

    // Check text direction for RTL support
    let is_rtl = session.text_direction.is_rtl();

    // For RTL: calculate total width first so we can start from the right
    let total_width = if is_rtl {
        buffer.iter().filter_map(|sort| {
            if let crate::sort::SortKind::Glyph { advance_width, .. } = &sort.kind {
                Some(*advance_width)
            } else {
                None
            }
        }).sum()
    } else {
        0.0
    };

    // Build a combined BezPath from all sorts in the buffer (like preview mode)
    let mut combined_path = BezPath::new();
    let mut x_offset = if is_rtl { total_width } else { 0.0 };

    // Track previous glyph for kerning lookup
    let mut prev_glyph_name: Option<String> = None;
    let mut prev_glyph_group: Option<String> = None;

    for sort in buffer.iter() {
        match &sort.kind {
            crate::sort::SortKind::Glyph { name, advance_width, .. } => {
                // For RTL: move x left BEFORE drawing this glyph
                if is_rtl {
                    x_offset -= advance_width;
                }

                // Apply kerning if we have a previous glyph
                if let Some(prev_name) = &prev_glyph_name {
                    let workspace_guard = workspace.read().unwrap();

                    // Get current glyph's left kerning group
                    let curr_group = workspace_guard.get_glyph(name)
                        .and_then(|g| g.left_group.as_ref().map(|s| s.as_str()));

                    // Look up kerning value
                    let kern_value = crate::kerning::lookup_kerning(
                        &workspace_guard.kerning,
                        &workspace_guard.groups,
                        prev_name,
                        prev_glyph_group.as_deref(),
                        name,
                        curr_group,
                    );

                    if is_rtl {
                        x_offset -= kern_value;
                    } else {
                        x_offset += kern_value;
                    }
                }

                let mut glyph_path = BezPath::new();

                if sort.is_active {
                    // For active sort: use session.paths (live editing state)
                    // This updates in real-time as the user moves points
                    for path in session.paths.iter() {
                        glyph_path.extend(path.to_bezpath());
                    }
                    // Also include components from the session glyph
                    // We need to render components separately since session.paths only has editable contours
                    let workspace_guard = workspace.read().unwrap();
                    for component in &session.glyph.components {
                        append_component_path(&mut glyph_path, component, &workspace_guard, kurbo::Affine::IDENTITY);
                    }
                } else {
                    // For inactive sorts: load from workspace (saved state)
                    // Use glyph_to_bezpath_with_components to include components
                    let workspace_guard = workspace.read().unwrap();
                    if let Some(glyph) = workspace_guard.glyphs.get(name) {
                        glyph_path = crate::glyph_renderer::glyph_to_bezpath_with_components(
                            glyph,
                            &workspace_guard,
                        );
                    }
                }

                // Translate the glyph to its position in the text buffer
                let translated_path = kurbo::Affine::translate((x_offset, 0.0)) * glyph_path;
                combined_path.extend(translated_path);

                // For LTR: advance x forward AFTER drawing
                if !is_rtl {
                    x_offset += advance_width;
                }

                // Update previous glyph info for next iteration
                prev_glyph_name = Some(name.clone());
                prev_glyph_group = workspace.read().unwrap().get_glyph(name)
                    .and_then(|g| g.right_group.clone());
            }
            crate::sort::SortKind::LineBreak => {
                // For now, ignore line breaks in preview (Phase 1 is single line)
                // Reset kerning tracking (no kerning across lines)
                prev_glyph_name = None;
                prev_glyph_group = None;
            }
        }
    }

    let preview_size = 100.0; // Match glyph preview size
    let upm = session.ascender - session.descender;

    // Render the combined path as a glyph view
    // baseline_offset controls vertical position (0.0 = bottom, 1.0 = top)
    // Use 0.25 to leave room for Arabic descenders which are deeper than Latin
    Either::A(
        sized_box(
            flex_col((
                glyph_view(combined_path, preview_size, preview_size, upm)
                    .color(theme::panel::GLYPH_PREVIEW)
                    .baseline_offset(0.15), // Leave room for Arabic descenders
            ))
            .main_axis_alignment(xilem::view::MainAxisAlignment::End)
        )
        .width(PANEL_WIDTH.px())
        .height(PANEL_HEIGHT.px())
        // Background container temporarily disabled for wider text display
        // .background_color(theme::panel::BACKGROUND)
        // .border_color(theme::panel::OUTLINE)
        // .border_width(1.5)
        // .corner_radius(8.0),
    )
}

/// Helper function to append component paths to a BezPath
///
/// Recursively resolves component references and applies transforms.
fn append_component_path(
    path: &mut BezPath,
    component: &crate::workspace::Component,
    workspace: &crate::workspace::Workspace,
    parent_transform: kurbo::Affine,
) {
    // Look up the base glyph
    let base_glyph = match workspace.glyphs.get(&component.base) {
        Some(g) => g,
        None => return,
    };

    // Combine transforms
    let combined_transform = parent_transform * component.transform;

    // Add contours from base glyph
    for contour in &base_glyph.contours {
        let contour_path = crate::path::Path::from_contour(contour);
        let transformed = combined_transform * contour_path.to_bezpath();
        path.extend(transformed);
    }

    // Recursively add nested components
    for nested_component in &base_glyph.components {
        append_component_path(path, nested_component, workspace, combined_transform);
    }
}
