// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Editor session management for AppState

use super::{AppState, Tab};
use crate::editing::EditSession;
use crate::model::{read_workspace, write_workspace};
use std::sync::Arc;

#[allow(dead_code)]
impl AppState {
    /// Create an edit session for a glyph
    pub fn create_edit_session(&self, glyph_name: &str) -> Option<EditSession> {
        let workspace_arc = self.active_workspace()?;
        let workspace = read_workspace(&workspace_arc);
        let glyph = workspace.get_glyph(glyph_name)?;

        // Create session with text buffer for text editing support
        let metrics = crate::editing::FontMetrics {
            units_per_em: workspace.units_per_em.unwrap_or(1000.0),
            ascender: workspace.ascender.unwrap_or(800.0),
            descender: workspace.descender.unwrap_or(-200.0),
            x_height: workspace.x_height,
            cap_height: workspace.cap_height,
        };
        let mut session = EditSession::new_with_text_buffer(
            glyph_name.to_string(),
            workspace.path.clone(),
            glyph.clone(),
            metrics,
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
        let Some(session) = &self.editor_session else {
            return;
        };
        let Some(workspace_arc) = self.active_workspace() else {
            return;
        };

        let updated_glyph = session.to_glyph();

        // Save to the active sort's glyph (if there is one)
        if let Some(active_name) = &session.active_sort_name {
            write_workspace(&workspace_arc).update_glyph(active_name, updated_glyph);
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
        let Some(workspace_arc) = self.active_workspace() else {
            return;
        };

        // Update the editor session to use the new master's data
        if let Some(ref mut session) = self.editor_session {
            // Update workspace reference
            session.workspace = Some(Arc::clone(&workspace_arc));

            // Reload the active sort's glyph from the new master
            if let Some(glyph_name) = session.active_sort_name.clone() {
                let workspace = read_workspace(&workspace_arc);

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
                    session.selection = crate::editing::Selection::new();
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
    pub fn set_editor_tool(&mut self, tool_id: crate::tools::ToolId) {
        let Some(session) = &mut self.editor_session else {
            return;
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
    pub fn set_shape_type(&mut self, shape_type: crate::tools::shapes::ShapeType) {
        let Some(session) = &mut self.editor_session else {
            return;
        };

        // Update the shape type if the current tool is the shapes tool
        if let crate::tools::ToolBox::Shapes(shapes_tool) = &mut session.current_tool {
            shapes_tool.set_shape_type(shape_type);
        }
    }

    /// Set the text direction for RTL/LTR text editing
    pub fn set_text_direction(&mut self, direction: crate::shaping::TextDirection) {
        let Some(session) = &mut self.editor_session else {
            return;
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
        let Some(workspace_arc) = self.active_workspace() else {
            return;
        };

        // Save to the active sort's glyph (if there is one)
        if let Some(active_name) = &session.active_sort_name {
            let updated_glyph = session.to_glyph();
            write_workspace(&workspace_arc).update_glyph(active_name, updated_glyph);
        }
    }
}
