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
    coordinate_panel, edit_mode_toolbar_view, editor_view, glyph_view,
    shapes_toolbar_view, workspace_toolbar_view,
};
use crate::data::AppState;
use crate::theme;
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

    const MARGIN: f64 = 16.0; // Fixed 16px margin for all panels
    const TOOLBAR_HEIGHT: f64 = 64.0; // TOOLBAR_ITEM_SIZE (48) + TOOLBAR_PADDING * 2 (8 * 2)

    // Get current shape type if shapes tool is selected
    let current_shape = if let ToolBox::Shapes(shapes_tool) = &session.current_tool {
        shapes_tool.shape_type()
    } else {
        ShapeType::Rectangle // Default
    };

    // Determine if we should show the shapes sub-toolbar
    let show_shapes_toolbar = current_tool == ToolId::Shapes;

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
        // Foreground: floating toolbars (edit mode + optional shapes sub-toolbar) positioned in top-left
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
                } else {
                    Either::B(label(""))
                },
            ))
            .cross_axis_alignment(xilem::view::CrossAxisAlignment::Start)
        )
        .translate((MARGIN, MARGIN))
        .alignment(ChildAlignment::SelfAligned(UnitPoint::TOP_LEFT)),
        // Bottom-left: glyph preview panel
        transformed(glyph_preview_pane(session_arc.clone(), glyph_name.clone()))
            .translate((MARGIN, -MARGIN))
            .alignment(ChildAlignment::SelfAligned(UnitPoint::BOTTOM_LEFT)),
        // Bottom-center-top: text buffer preview panel (above active glyph, standard margin)
        transformed(text_buffer_preview_pane_centered(session_arc.clone()))
            .translate((0.0, -(MARGIN + 100.0 + MARGIN)))
            .alignment(ChildAlignment::SelfAligned(UnitPoint::BOTTOM)),
        // Bottom-center-bottom: active glyph panel
        transformed(active_glyph_panel_centered(session_arc.clone(), glyph_name.clone()))
            .translate((0.0, -MARGIN))
            .alignment(ChildAlignment::SelfAligned(UnitPoint::BOTTOM)),
        // Bottom-right: coordinate panel (locked to corner like workspace toolbar)
        transformed(coordinate_panel_from_session(&session_arc))
            .translate((-MARGIN, -MARGIN))
            .alignment(ChildAlignment::SelfAligned(UnitPoint::BOTTOM_RIGHT)),
        // Top-right: Workspace toolbar for navigation
        transformed(workspace_toolbar_view(
            |state: &mut AppState, button| {
                match button {
                    WorkspaceToolbarButton::GlyphGrid => {
                        state.close_editor();
                    }
                }
            },
        ))
        .translate((-MARGIN, MARGIN))
        .alignment(ChildAlignment::SelfAligned(UnitPoint::TOP_RIGHT)),
    )))
}

// ===== Helper Views =====

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
/// Horizontal layout: glyph on left, labels on right (matching coordinate panel style)
fn glyph_preview_pane(
    session: Arc<crate::edit_session::EditSession>,
    glyph_name: String,
) -> impl WidgetView<AppState> + use<> {
    const PANEL_HEIGHT: f64 = 100.0;
    const PANEL_WIDTH: f64 = 240.0; // Match coordinate panel width
    const GLYPH_SIZE: f64 = 80.0; // Fit within 100px height with padding

    // Get the glyph outline path from the session
    let glyph_path = build_glyph_path(&session);
    let upm = session.ascender - session.descender;

    // Format Unicode codepoint (use first codepoint if available)
    let unicode_display = format_unicode_display(&session);

    // Glyph preview on the left
    let glyph_preview = if !glyph_path.is_empty() {
        Either::A(
            sized_box(
                glyph_view(glyph_path, GLYPH_SIZE, GLYPH_SIZE, upm)
                    .color(theme::panel::GLYPH_PREVIEW)
                    .baseline_offset(0.15)
            )
            .width(100.px())
        )
    } else {
        Either::B(sized_box(label("")).width(100.px()))
    };

    // Labels on the right
    let labels = flex_col((
        label(glyph_name)
            .text_size(16.0)
            .color(theme::text::PRIMARY),
        label(unicode_display)
            .text_size(14.0)
            .color(theme::text::PRIMARY),
    ))
    .gap(4.px())
    .cross_axis_alignment(xilem::view::CrossAxisAlignment::Start);

    sized_box(
        flex_row((glyph_preview, labels))
            .gap(8.px())
            .main_axis_alignment(xilem::view::MainAxisAlignment::Start)
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
    session: Arc<crate::edit_session::EditSession>,
    glyph_name: String,
) -> impl WidgetView<AppState> + use<> {
    const PANEL_HEIGHT: f64 = 100.0;
    const PANEL_WIDTH: f64 = 488.0; // Match text buffer preview width

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

    // Format Unicode
    let unicode_display = if let Some(first_char) = session.glyph.codepoints.first() {
        format!("{:04X}", *first_char as u32)
    } else {
        String::from("")
    };

    // Row 1 (Top): Name and Unicode (both editable)
    // Widths: 340 + 8 gap + 116 = 464px
    let top_row = flex_row((
        sized_box(
            text_input(
                glyph_name.clone(),
                |_state: &mut AppState, _new_value| {
                    // TODO: implement glyph name editing
                }
            )
        ).width(340.px()),
        sized_box(
            text_input(
                unicode_display,
                |_state: &mut AppState, _new_value| {
                    // TODO: implement unicode editing
                }
            )
        ).width(116.px()),
    ))
    .gap(8.px())
    .main_axis_alignment(xilem::view::MainAxisAlignment::Start);

    // Row 2 (Middle): Left kern, LSB, RSB, Right kern (all editable)
    // Widths: 4 × 110 + 3 × 8 gaps = 464px
    let middle_row = flex_row((
        sized_box(
            text_input(
                "—".to_string(), // Left kern placeholder
                |_state: &mut AppState, _new_value| {
                    // TODO: implement left kern editing
                }
            )
        ).width(110.px()),
        sized_box(
            text_input(
                format!("{:.0}", lsb),
                |_state: &mut AppState, _new_value| {
                    // TODO: implement LSB editing
                }
            )
        ).width(110.px()),
        sized_box(
            text_input(
                format!("{:.0}", rsb),
                |_state: &mut AppState, _new_value| {
                    // TODO: implement RSB editing
                }
            )
        ).width(110.px()),
        sized_box(
            text_input(
                "—".to_string(), // Right kern placeholder
                |_state: &mut AppState, _new_value| {
                    // TODO: implement right kern editing
                }
            )
        ).width(110.px()),
    ))
    .gap(8.px())
    .main_axis_alignment(xilem::view::MainAxisAlignment::Start);

    // Row 3 (Bottom): Left kern group, Width, Right kern group (all editable)
    // Widths: 149 + 8 + 150 + 8 + 149 = 464px
    let bottom_row = flex_row((
        sized_box(
            text_input(
                left_group.to_string(),
                |state: &mut AppState, new_value| {
                    state.update_left_group(new_value);
                }
            )
            .placeholder("Group")
        ).width(149.px()),
        sized_box(
            text_input(
                format!("{:.0}", width),
                |state: &mut AppState, new_value| {
                    state.update_glyph_width(new_value);
                }
            )
        ).width(150.px()),
        sized_box(
            text_input(
                right_group.to_string(),
                |state: &mut AppState, new_value| {
                    state.update_right_group(new_value);
                }
            )
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
    const PANEL_HEIGHT: f64 = 100.0;
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

    // Build a combined BezPath from all sorts in the buffer (like preview mode)
    let mut combined_path = BezPath::new();
    let mut x_offset = 0.0;

    for sort in buffer.iter() {
        match &sort.kind {
            crate::sort::SortKind::Glyph { name, advance_width, .. } => {
                let mut glyph_path = BezPath::new();

                if sort.is_active {
                    // For active sort: use session.paths (live editing state)
                    // This updates in real-time as the user moves points
                    for path in session.paths.iter() {
                        glyph_path.extend(path.to_bezpath());
                    }
                } else {
                    // For inactive sorts: load from workspace (saved state)
                    if let Some(glyph) = workspace.read().unwrap().glyphs.get(name) {
                        for contour in &glyph.contours {
                            let path = crate::path::Path::from_contour(contour);
                            glyph_path.extend(path.to_bezpath());
                        }
                    }
                }

                // Translate the glyph to its position in the text buffer
                let translated_path = kurbo::Affine::translate((x_offset, 0.0)) * glyph_path;
                combined_path.extend(translated_path);

                x_offset += advance_width;
            }
            crate::sort::SortKind::LineBreak => {
                // For now, ignore line breaks in preview (Phase 1 is single line)
            }
        }
    }

    let preview_size = 60.0; // Smaller than glyph preview
    let upm = session.ascender - session.descender;

    // Render the combined path as a glyph view, aligned to bottom
    Either::A(
        sized_box(
            flex_col((
                glyph_view(combined_path, preview_size, preview_size, upm)
                    .color(theme::panel::GLYPH_PREVIEW)
                    .baseline_offset(0.0), // Bottom alignment
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
