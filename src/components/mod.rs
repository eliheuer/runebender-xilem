// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! UI components for the Runebender Xilem font editor

pub mod coordinate_panel;
pub mod edit_mode_toolbar;
pub mod editor_canvas;
pub mod glyph_preview_widget;
pub mod keyboard_handler;
pub mod master_toolbar;
pub mod shapes_toolbar;
pub mod system_toolbar;
pub mod text_direction_toolbar;
pub mod toolbars;
pub mod workspace_toolbar;

// Re-export commonly used widget views and types
pub use coordinate_panel::{CoordinateSelection, coordinate_panel};
pub use edit_mode_toolbar::edit_mode_toolbar_view;
pub use editor_canvas::editor_view;
pub use glyph_preview_widget::glyph_view;
pub use keyboard_handler::keyboard_shortcuts;
pub use master_toolbar::{create_master_infos, master_toolbar_view, MasterInfo};
pub use shapes_toolbar::shapes_toolbar_view;
pub use system_toolbar::{system_toolbar_view, SystemToolbarButton};
pub use text_direction_toolbar::text_direction_toolbar_view;
pub use workspace_toolbar::workspace_toolbar_view;

