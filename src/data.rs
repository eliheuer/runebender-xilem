// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Application state and data structures

use crate::components::GlyphCategory;
use crate::designspace::{is_designspace_file, DesignspaceProject};
use crate::edit_session::EditSession;
use crate::workspace::Workspace;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use chrono::Local;
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
    pub designspace: Option<DesignspaceProject>,

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

    /// When the file was last saved (formatted time string for UI)
    pub last_saved: Option<String>,

    /// Current window width for responsive layout
    pub window_width: f64,

    /// Category filter for glyph grid
    pub glyph_category_filter: GlyphCategory,
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
            editor_session: None,
            active_tab: Tab::GlyphGrid,
            running: true,
            main_window_id: WindowId::next(),
            last_saved: None,
            window_width: 1030.0, // Default window width
            glyph_category_filter: GlyphCategory::All,
        }
    }

    /// Open a file dialog to select a UFO or designspace file
    pub fn open_font_dialog(&mut self) {
        self.error_message = None;

        // Use file picker with .ufo and .designspace extension filters
        // On macOS, .ufo directories are treated as packages/bundles,
        // so pick_folder() won't allow selecting them
        let path = rfd::FileDialog::new()
            .set_title("Open Font")
            .add_filter("Font Sources", &["ufo", "designspace"])
            .add_filter("UFO Font", &["ufo"])
            .add_filter("Designspace", &["designspace"])
            .pick_file();

        if let Some(path) = path {
            self.load_font(path);
        }
    }

    /// Load a font from a path (detects UFO vs designspace)
    pub fn load_font(&mut self, path: PathBuf) {
        if is_designspace_file(&path) {
            self.load_designspace(path);
        } else {
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
                self.designspace = None; // Clear any loaded designspace
                self.error_message = None;
            }
            Err(e) => {
                let error = format!("Failed to load UFO: {}", e);
                tracing::error!("{}", error);
                self.error_message = Some(error);
            }
        }
    }

    /// Load a designspace project from a path
    pub fn load_designspace(&mut self, path: PathBuf) {
        match DesignspaceProject::load(&path) {
            Ok(project) => {
                tracing::info!(
                    "Loaded designspace: {} ({} masters, {} glyphs)",
                    project.display_name(),
                    project.masters.len(),
                    project.glyph_count()
                );
                self.designspace = Some(project);
                self.workspace = None; // Clear any loaded single UFO
                self.error_message = None;
            }
            Err(e) => {
                let error = format!("Failed to load designspace: {}", e);
                tracing::error!("{}", error);
                self.error_message = Some(error);
            }
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

    /// Get the path of the loaded file (designspace or UFO)
    pub fn loaded_file_path(&self) -> Option<PathBuf> {
        if let Some(ds) = &self.designspace {
            Some(ds.path.clone())
        } else { self.workspace.as_ref().map(|ws| ws.read().unwrap().path.clone()) }
    }

    /// Get the last saved time string
    pub fn last_saved_display(&self) -> Option<String> {
        self.last_saved.clone()
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
        // For designspace, include master info
        if let Some(ds) = &self.designspace {
            let master = ds.active_master();
            Some(format!("{} - {}", ds.display_name(), master.style_name))
        } else {
            self.workspace.as_ref().map(|w| w.read().unwrap().display_name())
        }
    }

    /// Get the number of glyphs in the current font
    pub fn glyph_count(&self) -> Option<usize> {
        self.active_workspace()
            .map(|w| w.read().unwrap().glyph_count())
    }

    /// Select a glyph by name
    pub fn select_glyph(&mut self, name: String) {
        self.selected_glyph = Some(name);
    }

    /// Get all glyph names
    pub fn glyph_names(&self) -> Vec<String> {
        self.active_workspace()
            .map(|w| w.read().unwrap().glyph_names())
            .unwrap_or_default()
    }

    /// Calculate number of grid columns based on window width
    /// Ensures cells are never narrower than their height (120px)
    pub fn grid_columns(&self) -> usize {
        const CELL_HEIGHT: f64 = 120.0;
        const GAP: f64 = 6.0;
        const MARGIN: f64 = 6.0 * 2.0; // Left and right margins

        // Available width for cells (minus margins)
        let available = self.window_width - MARGIN;

        // Calculate how many cells can fit at minimum width (= height)
        // Each cell needs CELL_HEIGHT + GAP (except the last one)
        // n * CELL_HEIGHT + (n-1) * GAP <= available
        // n * (CELL_HEIGHT + GAP) - GAP <= available
        // n <= (available + GAP) / (CELL_HEIGHT + GAP)
        let max_cols = ((available + GAP) / (CELL_HEIGHT + GAP)).floor() as usize;

        // Clamp between 1 and 8 columns
        max_cols.clamp(1, 8)
    }

    /// Get the selected glyph's advance width
    pub fn selected_glyph_advance(&self) -> Option<f64> {
        let workspace = self.active_workspace()?;
        let glyph_name = self.selected_glyph.as_ref()?;
        workspace.read().unwrap().get_glyph(glyph_name).map(|g| g.width)
    }

    /// Get the selected glyph's unicode value
    pub fn selected_glyph_unicode(&self) -> Option<String> {
        let workspace_arc = self.active_workspace()?;
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
        let workspace_arc = self.active_workspace()?;
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
        session.workspace = Some(Arc::clone(&workspace_arc));

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

        let workspace_arc = match self.active_workspace() {
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

    /// Switch the editor to a different master while preserving the text buffer
    ///
    /// This syncs current edits to the old master, switches to the new master,
    /// and reloads the active glyph's paths from the new master's workspace.
    /// The text buffer (list of sorts being edited) is preserved.
    pub fn switch_editor_master(&mut self, new_master_index: usize) {
        // First sync any edits to the current master
        self.sync_editor_to_workspace();

        // Switch to the new master in the designspace
        if let Some(ref mut ds) = self.designspace {
            if !ds.switch_master(new_master_index) {
                return; // Invalid index
            }
        } else {
            return; // No designspace
        }

        // Get the new workspace
        let workspace_arc = match self.active_workspace() {
            Some(w) => w,
            None => return,
        };

        // Update the editor session to use the new master's data
        if let Some(ref mut session) = self.editor_session {
            // Update workspace reference
            session.workspace = Some(Arc::clone(&workspace_arc));

            // Reload the active sort's glyph from the new master
            if let Some(glyph_name) = session.active_sort_name.clone() {
                let workspace = workspace_arc.read().unwrap();

                if let Some(glyph) = workspace.get_glyph(&glyph_name) {
                    // Update the glyph data
                    session.glyph = Arc::new(glyph.clone());

                    // Convert contours to editable paths
                    let paths: Vec<crate::path::Path> = glyph
                        .contours
                        .iter()
                        .map(crate::path::Path::from_contour)
                        .collect();
                    session.paths = Arc::new(paths);

                    // Update font metrics from new workspace
                    session.units_per_em = workspace.units_per_em.unwrap_or(1000.0);
                    session.ascender = workspace.ascender.unwrap_or(800.0);
                    session.descender = workspace.descender.unwrap_or(-200.0);
                    session.x_height = workspace.x_height;
                    session.cap_height = workspace.cap_height;

                    // Clear selection since points have new IDs
                    session.selection = crate::selection::Selection::new();
                    session.selected_component = None;

                    tracing::info!(
                        "Switched editor to master {}, reloaded glyph '{}'",
                        new_master_index,
                        glyph_name
                    );
                }
            }
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

    /// Set the text direction for RTL/LTR text editing
    pub fn set_text_direction(
        &mut self,
        direction: crate::shaping::TextDirection,
    ) {
        let session = match &mut self.editor_session {
            Some(s) => s,
            None => return,
        };

        session.text_direction = direction;
        tracing::info!("Text direction set to {:?}", direction);
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
        let workspace_arc = match self.active_workspace() {
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
        // Handle designspace saving
        if let Some(ref mut designspace) = self.designspace {
            // Mark current master as modified before saving
            designspace.mark_active_modified();

            match designspace.save() {
                Ok(()) => {
                    tracing::info!("Saved designspace: {}", designspace.path.display());
                    self.error_message = None;
                    self.last_saved = Some(Local::now().format("%I:%M %p").to_string());
                }
                Err(e) => {
                    let error = format!("Failed to save designspace: {}", e);
                    tracing::error!("{}", error);
                    self.error_message = Some(error);
                }
            }
            return;
        }

        // Handle single UFO saving
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
                self.last_saved = Some(Local::now().format("%I:%M %p").to_string());
            }
            Err(e) => {
                let error = format!("Failed to save: {}", e);
                tracing::error!("{}", error);
                self.error_message = Some(error);
            }
        }
    }

    /// Update the glyph's advance width
    pub fn update_glyph_width(&mut self, new_width: String) {
        // Parse the width value
        let Ok(width) = new_width.parse::<f64>() else { return };

        // Get workspace arc first (before borrowing session mutably)
        let workspace_arc = self.active_workspace();

        let session = match &mut self.editor_session {
            Some(s) => s,
            None => return,
        };

        // Update the glyph in the session
        let glyph = Arc::make_mut(&mut session.glyph);
        glyph.width = width;

        // Sync to workspace (inline to avoid borrow issues)
        if let Some(workspace_arc) = workspace_arc
            && let Some(active_name) = &session.active_sort_name {
                let updated_glyph = session.to_glyph();
                workspace_arc.write().unwrap().update_glyph(active_name, updated_glyph);
            }
    }

    /// Update the glyph's left kerning group
    pub fn update_left_group(&mut self, new_group: String) {
        // Get workspace arc first (before borrowing session mutably)
        let workspace_arc = self.active_workspace();

        let session = match &mut self.editor_session {
            Some(s) => s,
            None => return,
        };

        // Update the glyph in the session
        let glyph = Arc::make_mut(&mut session.glyph);
        glyph.left_group = if new_group.is_empty() || new_group == "-" {
            None
        } else {
            Some(new_group)
        };

        // Sync to workspace (inline to avoid borrow issues)
        if let Some(workspace_arc) = workspace_arc
            && let Some(active_name) = &session.active_sort_name {
                let updated_glyph = session.to_glyph();
                workspace_arc.write().unwrap().update_glyph(active_name, updated_glyph);
            }
    }

    /// Update the glyph's right kerning group
    pub fn update_right_group(&mut self, new_group: String) {
        // Get workspace arc first (before borrowing session mutably)
        let workspace_arc = self.active_workspace();

        let session = match &mut self.editor_session {
            Some(s) => s,
            None => return,
        };

        // Update the glyph in the session
        let glyph = Arc::make_mut(&mut session.glyph);
        glyph.right_group = if new_group.is_empty() || new_group == "-" {
            None
        } else {
            Some(new_group)
        };

        // Sync to workspace (inline to avoid borrow issues)
        if let Some(workspace_arc) = workspace_arc
            && let Some(active_name) = &session.active_sort_name {
                let updated_glyph = session.to_glyph();
                workspace_arc.write().unwrap().update_glyph(active_name, updated_glyph);
            }
    }

    /// Get the left kern value (kerning from previous glyph to current glyph)
    /// Returns None if there's no previous glyph or no kerning defined
    pub fn get_left_kern(&self) -> Option<f64> {
        let session = self.editor_session.as_ref()?;
        let workspace_arc = self.active_workspace()?;
        let buffer = session.text_buffer.as_ref()?;
        let active_index = session.active_sort_index?;

        // Can't have left kerning if we're the first glyph
        if active_index == 0 {
            return None;
        }

        // Get previous glyph
        let prev_sort = buffer.get(active_index - 1)?;
        let prev_name = match &prev_sort.kind {
            crate::sort::SortKind::Glyph { name, .. } => name,
            crate::sort::SortKind::LineBreak => return None,
        };

        // Get current glyph name
        let curr_name = session.active_sort_name.as_ref()?;

        // Look up kerning
        let workspace = workspace_arc.read().unwrap();
        let prev_glyph = workspace.get_glyph(prev_name)?;
        let curr_glyph = workspace.get_glyph(curr_name)?;

        let kern_value = crate::kerning::lookup_kerning(
            &workspace.kerning,
            &workspace.groups,
            prev_name,
            prev_glyph.right_group.as_deref(),
            curr_name,
            curr_glyph.left_group.as_deref(),
        );

        if kern_value == 0.0 {
            None
        } else {
            Some(kern_value)
        }
    }

    /// Get the right kern value (kerning from current glyph to next glyph)
    /// Returns None if there's no next glyph or no kerning defined
    pub fn get_right_kern(&self) -> Option<f64> {
        let session = self.editor_session.as_ref()?;
        let workspace_arc = self.active_workspace()?;
        let buffer = session.text_buffer.as_ref()?;
        let active_index = session.active_sort_index?;

        // Can't have right kerning if we're the last glyph
        if active_index + 1 >= buffer.len() {
            return None;
        }

        // Get next glyph
        let next_sort = buffer.get(active_index + 1)?;
        let next_name = match &next_sort.kind {
            crate::sort::SortKind::Glyph { name, .. } => name,
            crate::sort::SortKind::LineBreak => return None,
        };

        // Get current glyph name
        let curr_name = session.active_sort_name.as_ref()?;

        // Look up kerning
        let workspace = workspace_arc.read().unwrap();
        let curr_glyph = workspace.get_glyph(curr_name)?;
        let next_glyph = workspace.get_glyph(next_name)?;

        let kern_value = crate::kerning::lookup_kerning(
            &workspace.kerning,
            &workspace.groups,
            curr_name,
            curr_glyph.right_group.as_deref(),
            next_name,
            next_glyph.left_group.as_deref(),
        );

        if kern_value == 0.0 {
            None
        } else {
            Some(kern_value)
        }
    }

    /// Update the left kern value (kerning from previous glyph to current glyph)
    pub fn update_left_kern(&mut self, new_value: String) {
        let session = match &self.editor_session {
            Some(s) => s,
            None => return,
        };

        let workspace_arc = match self.active_workspace() {
            Some(w) => w,
            None => return,
        };

        let buffer = match &session.text_buffer {
            Some(b) => b,
            None => return,
        };

        let active_index = match session.active_sort_index {
            Some(i) => i,
            None => return,
        };

        // Can't set left kerning if we're the first glyph
        if active_index == 0 {
            return;
        }

        // Get previous glyph name
        let prev_sort = match buffer.get(active_index - 1) {
            Some(s) => s,
            None => return,
        };

        let prev_name = match &prev_sort.kind {
            crate::sort::SortKind::Glyph { name, .. } => name.clone(),
            crate::sort::SortKind::LineBreak => return,
        };

        // Get current glyph name
        let curr_name = match &session.active_sort_name {
            Some(n) => n.clone(),
            None => return,
        };

        // Parse the new value (empty string means remove kerning)
        let kern_value = if new_value.is_empty() || new_value == "-" {
            // Remove kerning pair
            let mut workspace = workspace_arc.write().unwrap();
            if let Some(first_pairs) = workspace.kerning.get_mut(&prev_name) {
                first_pairs.remove(&curr_name);
            }
            return;
        } else {
            match new_value.parse::<f64>() {
                Ok(v) => v,
                Err(_) => return, // Invalid number, ignore
            }
        };

        // Update kerning in workspace
        let mut workspace = workspace_arc.write().unwrap();
        workspace
            .kerning
            .entry(prev_name.clone())
            .or_insert_with(std::collections::HashMap::new)
            .insert(curr_name, kern_value);
    }

    /// Update the right kern value (kerning from current glyph to next glyph)
    pub fn update_right_kern(&mut self, new_value: String) {
        let session = match &self.editor_session {
            Some(s) => s,
            None => return,
        };

        let workspace_arc = match self.active_workspace() {
            Some(w) => w,
            None => return,
        };

        let buffer = match &session.text_buffer {
            Some(b) => b,
            None => return,
        };

        let active_index = match session.active_sort_index {
            Some(i) => i,
            None => return,
        };

        // Can't set right kerning if we're the last glyph
        if active_index + 1 >= buffer.len() {
            return;
        }

        // Get next glyph name
        let next_sort = match buffer.get(active_index + 1) {
            Some(s) => s,
            None => return,
        };

        let next_name = match &next_sort.kind {
            crate::sort::SortKind::Glyph { name, .. } => name.clone(),
            crate::sort::SortKind::LineBreak => return,
        };

        // Get current glyph name
        let curr_name = match &session.active_sort_name {
            Some(n) => n.clone(),
            None => return,
        };

        // Parse the new value (empty string means remove kerning)
        let kern_value = if new_value.is_empty() || new_value == "-" {
            // Remove kerning pair
            let mut workspace = workspace_arc.write().unwrap();
            if let Some(first_pairs) = workspace.kerning.get_mut(&curr_name) {
                first_pairs.remove(&next_name);
            }
            return;
        } else {
            match new_value.parse::<f64>() {
                Ok(v) => v,
                Err(_) => return, // Invalid number, ignore
            }
        };

        // Update kerning in workspace
        let mut workspace = workspace_arc.write().unwrap();
        workspace
            .kerning
            .entry(curr_name.clone())
            .or_insert_with(std::collections::HashMap::new)
            .insert(next_name, kern_value);
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
