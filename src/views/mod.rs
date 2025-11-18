// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! View modules for the Runebender Xilem font editor
//!
//! This module contains the top-level view functions that compose the
//! application UI. Each view represents a different screen or tab in the
//! application:
//!
//! - `editor`: The main glyph editing interface with canvas and toolbars
//! - `glyph_grid`: The grid view showing all glyphs in the font
//! - `welcome`: The welcome screen shown when no font is loaded

pub mod editor;
pub mod glyph_grid;
pub mod welcome;

pub use editor::editor_tab;
pub use glyph_grid::glyph_grid_tab;
pub use welcome::welcome;
