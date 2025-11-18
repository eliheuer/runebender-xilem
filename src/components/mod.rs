// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! UI components for the Runebender Xilem font editor

pub mod coordinate_panel;
pub mod edit_mode_toolbar;
pub mod editor_canvas;
pub mod glyph_preview_widget;
pub mod toolbars;
pub mod workspace_toolbar;

// Re-export commonly used widget views and types
pub use coordinate_panel::{CoordinateSelection, coordinate_panel};
pub use edit_mode_toolbar::edit_mode_toolbar_view;
pub use editor_canvas::editor_view;
pub use glyph_preview_widget::glyph_view;
pub use workspace_toolbar::workspace_toolbar_view;

