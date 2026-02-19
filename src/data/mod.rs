// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Application state and data structures

mod editor;
mod file_io;
mod grid;
mod kerning;

use crate::components::GlyphCategory;
use crate::editing::EditSession;
use crate::model::workspace::Workspace;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use xilem::WindowId;

/// Which tab is currently active
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Tab {
    /// Glyph grid view (font overview)
    GlyphGrid = 0,
    /// Editor view for a specific glyph
    Editor = 1,
}

/// Main application state
pub struct AppState {
    /// The loaded font workspace, if any (for single UFO files)
    /// Wrapped in Arc<RwLock<>> to allow shared mutable access with EditSession
    pub workspace: Option<Arc<RwLock<Workspace>>>,

    /// Designspace project, if loaded (for variable font masters)
    pub designspace: Option<crate::model::designspace::DesignspaceProject>,

    /// Error message to display, if any
    pub error_message: Option<String>,

    /// Currently selected glyph name (for showing in grid)
    pub selected_glyph: Option<String>,

    /// All currently selected glyph names (for multi-select)
    pub selected_glyphs: HashSet<String>,

    /// Current editor session (when Editor tab is active)
    pub editor_session: Option<EditSession>,

    /// Demo welcome session (used when no workspace is loaded)
    pub welcome_session: Option<EditSession>,

    /// Which tab is currently active
    pub active_tab: Tab,

    /// Whether the app should keep running
    pub running: bool,

    /// Main window ID (stable across rebuilds to prevent window
    /// recreation)
    pub main_window_id: WindowId,

    /// When the file was last saved (formatted time string for UI)
    pub last_saved: Option<String>,

    /// Current window width for responsive layout
    pub window_width: f64,

    /// Category filter for glyph grid
    pub glyph_category_filter: GlyphCategory,

    /// First visible row index in the virtual glyph grid
    pub grid_scroll_row: usize,

    /// Current window height (tracked by size_tracker)
    pub window_height: f64,

    /// Cached count of glyphs matching current category filter.
    /// Updated by `glyph_grid_view` on each rebuild so the scroll
    /// callback can use it without re-iterating all glyphs.
    pub cached_filtered_count: usize,
}

#[allow(dead_code)]
impl AppState {
    /// Create a new empty application state
    pub fn new() -> Self {
        Self {
            workspace: None,
            designspace: None,
            welcome_session: None,
            error_message: None,
            selected_glyph: None,
            selected_glyphs: HashSet::new(),
            editor_session: None,
            active_tab: Tab::GlyphGrid,
            running: true,
            main_window_id: WindowId::next(),
            last_saved: None,
            window_width: 1030.0, // Default window width
            glyph_category_filter: GlyphCategory::All,
            grid_scroll_row: 0,
            window_height: 800.0,
            cached_filtered_count: 0,
        }
    }

    /// Get the active workspace (from designspace or direct UFO)
    pub fn active_workspace(&self) -> Option<Arc<RwLock<Workspace>>> {
        if let Some(ds) = &self.designspace {
            Some(ds.active_workspace())
        } else {
            self.workspace.clone()
        }
    }

    /// Check if any font is loaded (either UFO or designspace)
    pub fn has_font_loaded(&self) -> bool {
        self.workspace.is_some() || self.designspace.is_some()
    }
}

/// Implement the Xilem AppState trait
impl xilem::AppState for AppState {
    fn keep_running(&self) -> bool {
        self.running
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
