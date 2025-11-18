// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Editor view - main glyph editing interface

use std::sync::Arc;

use kurbo::BezPath;
use masonry::properties::types::{AsUnit, UnitPoint};
use xilem::core::one_of::Either;
use xilem::style::Style;
use xilem::view::{
    ChildAlignment, ZStackExt, flex_col, label, sized_box, transformed,
    zstack,
};
use xilem::WidgetView;

use crate::components::workspace_toolbar::WorkspaceToolbarButton;
use crate::components::{
    coordinate_panel, edit_mode_toolbar_view, editor_view, glyph_view,
    workspace_toolbar_view,
};
use crate::data::AppState;
use crate::theme;

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
    let glyph_name = session.glyph_name.clone();
    let session_arc = Arc::new(session.clone());

    const MARGIN: f64 = 16.0; // Fixed 16px margin for all panels

    // Use zstack to layer UI elements over the canvas
    Either::A(zstack((
        // Background: the editor canvas (full screen)
        editor_view(
            session_arc.clone(),
            |state: &mut AppState, updated_session| {
                state.update_editor_session(updated_session);
            },
        ),
        // Foreground: floating edit mode toolbar positioned in top-left
        // with fixed margin
        transformed(edit_mode_toolbar_view(
            current_tool,
            |state: &mut AppState, tool_id| {
                state.set_editor_tool(tool_id);
            },
        ))
        .translate((MARGIN, MARGIN))
        .alignment(ChildAlignment::SelfAligned(UnitPoint::TOP_LEFT)),
        // Bottom-left: glyph preview pane with fixed margin
        transformed(glyph_preview_pane(
            session_arc.clone(),
            glyph_name.clone(),
        ))
        .translate((MARGIN, -MARGIN))
        .alignment(ChildAlignment::SelfAligned(UnitPoint::BOTTOM_LEFT)),
        // Bottom-right: coordinate panel with fixed margin
        transformed(coordinate_panel_from_session(&session_arc))
            .translate((-MARGIN, -MARGIN))
            .alignment(
                ChildAlignment::SelfAligned(UnitPoint::BOTTOM_RIGHT),
            ),
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
fn glyph_preview_pane(
    session: Arc<crate::edit_session::EditSession>,
    glyph_name: String,
) -> impl WidgetView<AppState> + use<> {
    // Get the glyph outline path from the session
    let glyph_path = build_glyph_path(&session);

    // Make the preview larger to fill more space
    let preview_size = 150.0;
    let upm = session.ascender - session.descender;

    // Format Unicode codepoint (use first codepoint if available)
    let unicode_display = format_unicode_display(&session);

    sized_box(flex_col((
        // Add 4px spacer above glyph preview
        sized_box(label("")).height(4.px()),
        // Glyph preview - use theme color with custom baseline offset
        build_glyph_preview(&glyph_path, preview_size, upm),
        // Glyph name and unicode labels - use primary UI text color
        build_glyph_labels(glyph_name, unicode_display),
    )))
    .width(160.px())
    .height(180.px())
    .background_color(theme::panel::BACKGROUND)
    .border_color(theme::panel::OUTLINE)
    .border_width(1.5)
    .corner_radius(8.0)
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
