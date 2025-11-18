// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Welcome screen view for Runebender Xilem
//!
//! Displays an interactive welcome screen with a demo "R" glyph that users
//! can interact with. The welcome screen appears when no font is loaded.

use std::sync::Arc;

use masonry::properties::types::{AsUnit, UnitPoint};
use xilem::style::Style;
use xilem::view::{
    ChildAlignment, CrossAxisAlignment, MainAxisAlignment, ZStackExt,
    button, flex_col, label, sized_box, transformed, zstack,
};
use xilem::WidgetView;

use crate::components::editor_view;
use crate::data::AppState;
use crate::edit_session::EditSession;
use crate::workspace::{Contour, ContourPoint, Glyph, PointType};

// ===== Welcome View =====

/// Welcome screen shown when no font is loaded
pub fn welcome(
    state: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
    let error_text = format_error_text(&state.error_message);

    // Create or reuse the demo edit session with the hardcoded R glyph
    if state.welcome_session.is_none() {
        state.welcome_session = Some(create_demo_session());
    }

    let session = state.welcome_session.as_ref().unwrap();
    let session_arc = Arc::new(session.clone());

    const MARGIN: f64 = 16.0;

    // Layer welcome UI over interactive editor
    zstack((
        // Background: Interactive editor with demo R glyph
        editor_view(session_arc, |state: &mut AppState, updated_session| {
            // Save changes back to the welcome session so they persist
            state.welcome_session = Some(updated_session);
        }),
        // Foreground: Welcome UI in upper left (constrained size so it
        // doesn't block editor)
        transformed(build_welcome_ui(error_text))
            .translate((MARGIN, MARGIN))
            .alignment(ChildAlignment::SelfAligned(UnitPoint::TOP_LEFT)),
    ))
}

// ===== Welcome UI Helpers =====

/// Format error message text for display
fn format_error_text(error_message: &Option<String>) -> String {
    error_message
        .as_ref()
        .map(|msg| format!("Error: {}", msg))
        .unwrap_or_default()
}

/// Build the welcome UI panel
fn build_welcome_ui(
    error_text: String,
) -> impl WidgetView<AppState> + use<> {
    sized_box(
        flex_col((
            label("Runebender Xilem")
                .text_size(48.0)
                .color(crate::theme::text::PRIMARY),
            label(error_text)
                .text_size(12.0)
                .color(crate::theme::text::PRIMARY),
            sized_box(label("")).height(8.px()),
            build_open_button(),
            build_new_font_button(),
        ))
        .main_axis_alignment(MainAxisAlignment::Start)
        .cross_axis_alignment(CrossAxisAlignment::Start),
    )
    .width(220.px()) // Constrained width
    .height(200.px()) // Constrained height
}

/// Build the "Open UFO..." button
fn build_open_button() -> impl WidgetView<AppState> + use<> {
    sized_box(button(
        label("Open UFO...").color(crate::theme::text::PRIMARY),
        |state: &mut AppState| {
            state.open_font_dialog();
        },
    ))
    .width(200.px())
}

/// Build the "New Font" button
fn build_new_font_button() -> impl WidgetView<AppState> + use<> {
    sized_box(button(
        label("New Font").color(crate::theme::text::PRIMARY),
        |state: &mut AppState| {
            // TODO: Implement proper new font creation
            // For now, open the bundled sample font
            state.load_ufo(std::path::PathBuf::from("assets/untitled.ufo"));
        },
    ))
    .width(200.px())
}

// ===== Demo Session Creation =====

/// Create a demo edit session with the hardcoded R glyph
fn create_demo_session() -> EditSession {
    let glyph = create_r_glyph();

    let mut session = EditSession::new(
        "R".to_string(),
        std::path::PathBuf::from("demo.ufo"), // Dummy path for demo
        glyph,
        1024.0,      // UPM (units per em) - from VirtuaGrotesk
        832.0,       // ascender - from VirtuaGrotesk
        -256.0,      // descender - from VirtuaGrotesk
        Some(576.0), // x_height - from VirtuaGrotesk
        Some(768.0), // cap_height - from VirtuaGrotesk
    );

    // Center the glyph perfectly in the window
    session.viewport.offset = kurbo::Vec2::new(500.0, 700.0);

    // Adjust zoom level for nice preview size
    session.viewport.zoom = 0.7;

    session.viewport_initialized = true;

    session
}

// ===== R Glyph Data =====

/// Create a hardcoded "R" glyph from VirtuaGrotesk-Regular
///
/// This is used as the background for the welcome screen. The glyph data
/// is stored as workspace::ContourPoint structures, which can be converted
/// to BezPath for rendering.
#[allow(clippy::vec_init_then_push)]
fn create_r_glyph() -> Glyph {
    let mut contours = Vec::new();

    // First contour (inner counter of R)
    contours.push(build_inner_contour());

    // Second contour (outer outline of R)
    contours.push(build_outer_contour());

    Glyph {
        name: "R".to_string(),
        width: 668.0,
        height: None,
        codepoints: vec!['R'],
        contours,
    }
}

/// Build the inner contour (counter) of the R glyph
fn build_inner_contour() -> Contour {
    Contour {
        points: vec![
            contour_point(192.0, 416.0, PointType::Line),
            contour_point(184.0, 424.0, PointType::Line),
            contour_point(184.0, 664.0, PointType::Line),
            contour_point(192.0, 672.0, PointType::Line),
            contour_point(368.0, 672.0, PointType::Line),
            contour_point(440.0, 672.0, PointType::OffCurve),
            contour_point(496.0, 616.0, PointType::OffCurve),
            contour_point(496.0, 544.0, PointType::Curve),
            contour_point(496.0, 472.0, PointType::OffCurve),
            contour_point(440.0, 416.0, PointType::OffCurve),
            contour_point(368.0, 416.0, PointType::Curve),
        ],
    }
}

/// Build the outer contour (outline) of the R glyph
fn build_outer_contour() -> Contour {
    Contour {
        points: vec![
            contour_point(96.0, 0.0, PointType::Line),
            contour_point(168.0, 0.0, PointType::Line),
            contour_point(184.0, 16.0, PointType::Line),
            contour_point(184.0, 320.0, PointType::Line),
            contour_point(192.0, 328.0, PointType::Line),
            contour_point(360.0, 328.0, PointType::Line),
            contour_point(456.0, 328.0, PointType::OffCurve),
            contour_point(496.0, 288.0, PointType::OffCurve),
            contour_point(496.0, 192.0, PointType::Curve),
            contour_point(496.0, 16.0, PointType::Line),
            contour_point(512.0, 0.0, PointType::Line),
            contour_point(584.0, 0.0, PointType::Line),
            contour_point(600.0, 16.0, PointType::Line),
            contour_point(600.0, 208.0, PointType::Line),
            contour_point(600.0, 304.0, PointType::OffCurve),
            contour_point(544.0, 360.0, PointType::OffCurve),
            contour_point(472.0, 368.0, PointType::Curve),
            contour_point(472.0, 376.0, PointType::Line),
            contour_point(528.0, 392.0, PointType::OffCurve),
            contour_point(604.0, 448.0, PointType::OffCurve),
            contour_point(604.0, 544.0, PointType::Curve),
            contour_point(604.0, 672.0, PointType::OffCurve),
            contour_point(504.0, 768.0, PointType::OffCurve),
            contour_point(376.0, 768.0, PointType::Curve),
            contour_point(96.0, 768.0, PointType::Line),
            contour_point(80.0, 752.0, PointType::Line),
            contour_point(80.0, 16.0, PointType::Line),
        ],
    }
}

/// Helper to create a ContourPoint
fn contour_point(x: f64, y: f64, point_type: PointType) -> ContourPoint {
    ContourPoint {
        x,
        y,
        point_type,
    }
}
