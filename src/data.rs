// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Application state and data structures

use crate::edit_session::EditSession;
use crate::workspace::Workspace;
use std::path::PathBuf;
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
    /// The loaded font workspace, if any
    /// Wrapped in Arc<RwLock<>> to allow shared mutable access with EditSession
    pub workspace: Option<Arc<RwLock<Workspace>>>,

    /// Error message to display, if any
    pub error_message: Option<String>,

    /// Currently selected glyph name (for showing in grid)
    pub selected_glyph: Option<String>,

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
}

#[allow(dead_code)]
impl AppState {
    /// Create a new empty application state
    pub fn new() -> Self {
        Self {
            workspace: None,
            welcome_session: None,
            error_message: None,
            selected_glyph: None,
            editor_session: None,
            active_tab: Tab::GlyphGrid,
            running: true,
            main_window_id: WindowId::next(),
        }
    }

    /// Open a file dialog to select a UFO directory
    pub fn open_font_dialog(&mut self) {
        self.error_message = None;

        // Use file picker with .ufo extension filter
        // On macOS, .ufo directories are treated as packages/bundles,
        // so pick_folder() won't allow selecting them
        let path = rfd::FileDialog::new()
            .set_title("Open UFO Font")
            .add_filter("UFO Font", &["ufo"])
            .pick_file();

        if let Some(path) = path {
            self.load_ufo(path);
        }
    }

    /// Load a UFO from a path
    pub fn load_ufo(&mut self, path: PathBuf) {
        match Workspace::load(&path) {
            Ok(workspace) => {
                tracing::info!(
                    "Loaded font: {} ({} glyphs)",
                    workspace.display_name(),
                    workspace.glyph_count()
                );
                self.workspace = Some(Arc::new(RwLock::new(workspace)));
                self.error_message = None;
            }
            Err(e) => {
                let error = format!("Failed to load UFO: {}", e);
                tracing::error!("{}", error);
                self.error_message = Some(error);
            }
        }
    }

    /// Create a new empty font
    pub fn create_new_font(&mut self) {
        // TODO: Implement new font creation
        self.error_message = Some(
            "New font creation not yet implemented".to_string(),
        );
    }

    /// Get the current font display name
    pub fn font_display_name(&self) -> Option<String> {
        self.workspace.as_ref().map(|w| w.read().unwrap().display_name())
    }

    /// Get the number of glyphs in the current font
    pub fn glyph_count(&self) -> Option<usize> {
        self.workspace.as_ref().map(|w| w.read().unwrap().glyph_count())
    }

    /// Select a glyph by name
    pub fn select_glyph(&mut self, name: String) {
        self.selected_glyph = Some(name);
    }

    /// Get all glyph names
    pub fn glyph_names(&self) -> Vec<String> {
        self.workspace
            .as_ref()
            .map(|w| w.read().unwrap().glyph_names())
            .unwrap_or_default()
    }

    /// Get the selected glyph's advance width
    pub fn selected_glyph_advance(&self) -> Option<f64> {
        let workspace = self.workspace.as_ref()?;
        let glyph_name = self.selected_glyph.as_ref()?;
        workspace.read().unwrap().get_glyph(glyph_name).map(|g| g.width)
    }

    /// Get the selected glyph's unicode value
    pub fn selected_glyph_unicode(&self) -> Option<String> {
        let workspace_arc = self.workspace.as_ref()?;
        let glyph_name = self.selected_glyph.as_ref()?;
        let workspace = workspace_arc.read().unwrap();
        let glyph = workspace.get_glyph(glyph_name)?;

        if glyph.codepoints.is_empty() {
            return None;
        }

        glyph.codepoints
            .first()
            .map(|c| format!("U+{:04X}", *c as u32))
    }

    /// Create an edit session for a glyph
    pub fn create_edit_session(
        &self,
        glyph_name: &str,
    ) -> Option<EditSession> {
        let workspace_arc = self.workspace.as_ref()?;
        let workspace = workspace_arc.read().unwrap();
        let glyph = workspace.get_glyph(glyph_name)?;

        // Create session with text buffer for text editing support
        let mut session = EditSession::new_with_text_buffer(
            glyph_name.to_string(),
            workspace.path.clone(),
            glyph.clone(),
            workspace.units_per_em.unwrap_or(1000.0),
            workspace.ascender.unwrap_or(800.0),
            workspace.descender.unwrap_or(-200.0),
            workspace.x_height,
            workspace.cap_height,
        );

        // Set workspace reference for text mode character mapping (Phase 5)
        session.workspace = Some(Arc::clone(workspace_arc));

        Some(session)
    }

    /// Open or focus an editor for a glyph
    pub fn open_editor(&mut self, glyph_name: String) {
        if let Some(session) = self.create_edit_session(&glyph_name) {
            self.editor_session = Some(session);
            self.active_tab = Tab::Editor;
        }
    }

    /// Close the editor and return to glyph grid
    ///
    /// This syncs any final changes to the workspace before closing.
    pub fn close_editor(&mut self) {
        self.sync_editor_to_workspace();
        self.editor_session = None;
        self.active_tab = Tab::GlyphGrid;
    }

    /// Sync the current editor session to the workspace
    fn sync_editor_to_workspace(&mut self) {
        let session = match &self.editor_session {
            Some(s) => s,
            None => return,
        };

        let workspace_arc = match &self.workspace {
            Some(w) => w,
            None => return,
        };

        let updated_glyph = session.to_glyph();

        // Save to the active sort's glyph (if there is one)
        if let Some(active_name) = &session.active_sort_name {
            // Debug logging only for glyph "a"
            if active_name == "a" {
                println!(
                    "[close_editor] Synced glyph 'a' with {} contours to \
                     workspace",
                    updated_glyph.contours.len()
                );
            }

            workspace_arc.write().unwrap().update_glyph(active_name, updated_glyph);
        }
    }

    /// Set the tool for the current editor session
    pub fn set_editor_tool(
        &mut self,
        tool_id: crate::tools::ToolId,
    ) {
        let session = match &mut self.editor_session {
            Some(s) => s,
            None => return,
        };

        // Phase 4: When switching to text tool, enter text mode
        if tool_id == crate::tools::ToolId::Text {
            if session.has_text_buffer() {
                session.enter_text_mode();
                tracing::info!("Entered text editing mode");
            } else {
                tracing::warn!("Text tool selected but no text buffer available");
            }
        } else {
            // Exit text mode when switching to other tools
            if session.text_mode_active {
                session.exit_text_mode();
                tracing::info!("Exited text editing mode");
            }
        }

        session.current_tool = crate::tools::ToolBox::for_id(tool_id);
    }

    /// Set the shape type for the shapes tool
    pub fn set_shape_type(
        &mut self,
        shape_type: crate::tools::shapes::ShapeType,
    ) {
        let session = match &mut self.editor_session {
            Some(s) => s,
            None => return,
        };

        // Update the shape type if the current tool is the shapes tool
        if let crate::tools::ToolBox::Shapes(shapes_tool) = &mut session.current_tool {
            shapes_tool.set_shape_type(shape_type);
        }
    }

    /// Update the current editor session with new state
    ///
    /// This also syncs the edited glyph back to the workspace so
    /// changes persist when switching views.
    pub fn update_editor_session(&mut self, session: EditSession) {
        self.sync_session_to_workspace(&session);
        self.editor_session = Some(session);
    }

    /// Sync a session's changes to the workspace
    fn sync_session_to_workspace(&mut self, session: &EditSession) {
        let workspace_arc = match &self.workspace {
            Some(w) => w,
            None => return,
        };

        // Save to the active sort's glyph (if there is one)
        if let Some(active_name) = &session.active_sort_name {
            let updated_glyph = session.to_glyph();
            workspace_arc.write().unwrap().update_glyph(active_name, updated_glyph);
        }
    }

    /// Save the current workspace to disk
    pub fn save_workspace(&mut self) {
        let workspace_arc = match &self.workspace {
            Some(w) => w,
            None => {
                self.error_message = Some("No workspace to save".to_string());
                return;
            }
        };

        let workspace = workspace_arc.read().unwrap();
        match workspace.save() {
            Ok(()) => {
                tracing::info!("Saved: {}", workspace.path.display());
                self.error_message = None;
            }
            Err(e) => {
                let error = format!("Failed to save: {}", e);
                tracing::error!("{}", error);
                self.error_message = Some(error);
            }
        }
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
