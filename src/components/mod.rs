// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! UI components for the Runebender Xilem font editor

pub mod category_panel;
pub mod coordinate_panel;
pub mod edit_mode_toolbar;
pub mod editor_canvas;
pub mod glyph_anatomy_panel;
pub mod glyph_info_panel;
pub mod glyph_preview_widget;
pub mod grid_scroll_handler;
pub mod mark_color_panel;
pub mod master_toolbar;
pub mod shapes_toolbar;
pub mod size_tracker;
pub mod system_toolbar;
pub mod text_direction_toolbar;
pub mod toolbars;
pub mod workspace_toolbar;

// Re-export commonly used widget views and types
pub use category_panel::{CATEGORY_PANEL_WIDTH, GlyphCategory, category_panel};
pub use coordinate_panel::{CoordinateSelection, coordinate_panel};
pub use edit_mode_toolbar::edit_mode_toolbar_view;
pub use editor_canvas::editor_view;
pub use glyph_anatomy_panel::glyph_anatomy_panel;
pub use glyph_info_panel::{GLYPH_INFO_PANEL_WIDTH, glyph_info_panel};
pub use glyph_preview_widget::{glyph_view, multi_glyph_view};
pub use grid_scroll_handler::grid_scroll_handler;
pub use mark_color_panel::mark_color_panel;
pub use master_toolbar::{create_master_infos, master_toolbar_view};
pub use shapes_toolbar::shapes_toolbar_view;
pub use size_tracker::size_tracker;
pub use system_toolbar::{SystemToolbarButton, system_toolbar_view};
pub use text_direction_toolbar::text_direction_toolbar_view;
pub use workspace_toolbar::workspace_toolbar_view;
