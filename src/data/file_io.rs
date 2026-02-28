// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! File I/O operations for AppState (open, load, save)

use super::AppState;
use crate::model::designspace::{DesignspaceProject, is_designspace_file};
use crate::model::{read_workspace, write_workspace};
use crate::model::workspace::Workspace;
use chrono::Local;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};

#[allow(dead_code)]
impl AppState {
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

    /// Get the path of the loaded file (designspace or UFO)
    pub fn loaded_file_path(&self) -> Option<PathBuf> {
        if let Some(ds) = &self.designspace {
            Some(ds.path.clone())
        } else {
            self.workspace
                .as_ref()
                .map(|ws| read_workspace(ws).path.clone())
        }
    }

    /// Get all UFO directory paths to watch for external changes.
    ///
    /// For a designspace, returns every master's UFO path. For a
    /// single UFO, returns just that one path.
    pub fn watched_ufo_paths(&self) -> Vec<PathBuf> {
        if let Some(ds) = &self.designspace {
            ds.masters.iter().map(|m| m.ufo_path.clone()).collect()
        } else if let Some(ws) = &self.workspace {
            vec![read_workspace(ws).path.clone()]
        } else {
            vec![]
        }
    }

    /// Get the last saved time string
    pub fn last_saved_display(&self) -> Option<String> {
        self.last_saved.clone()
    }

    /// Create a new empty font
    pub fn create_new_font(&mut self) {
        // TODO: Implement new font creation
        self.error_message = Some("New font creation not yet implemented".to_string());
    }

    /// Get the current font display name
    pub fn font_display_name(&self) -> Option<String> {
        // For designspace, include master info
        if let Some(ds) = &self.designspace {
            let master = ds.active_master();
            Some(format!("{} - {}", ds.display_name(), master.style_name))
        } else {
            self.workspace
                .as_ref()
                .map(|w| read_workspace(w).display_name())
        }
    }

    /// Save the current workspace to disk
    pub fn save_workspace(&mut self) {
        // Set self-save flag so the file watcher ignores our own writes
        self.save_in_progress.store(true, Ordering::SeqCst);

        // Handle designspace saving
        if let Some(ref mut designspace) = self.designspace {
            // Mark current master as modified before saving
            designspace.mark_active_modified();

            match designspace.save() {
                Ok(()) => {
                    tracing::info!(
                        "Saved designspace: {}",
                        designspace.path.display()
                    );
                    self.error_message = None;
                    self.last_saved = Some(
                        Local::now().format("%I:%M %p").to_string(),
                    );
                }
                Err(e) => {
                    self.save_in_progress
                        .store(false, Ordering::SeqCst);
                    let error =
                        format!("Failed to save designspace: {}", e);
                    tracing::error!("{}", error);
                    self.error_message = Some(error);
                }
            }
            return;
        }

        // Handle single UFO saving
        let Some(workspace_arc) = &self.workspace else {
            self.save_in_progress.store(false, Ordering::SeqCst);
            self.error_message =
                Some("No workspace to save".to_string());
            return;
        };

        let workspace = read_workspace(workspace_arc);
        match workspace.save() {
            Ok(()) => {
                tracing::info!("Saved: {}", workspace.path.display());
                self.error_message = None;
                self.last_saved = Some(
                    Local::now().format("%I:%M %p").to_string(),
                );
            }
            Err(e) => {
                self.save_in_progress
                    .store(false, Ordering::SeqCst);
                let error = format!("Failed to save: {}", e);
                tracing::error!("{}", error);
                self.error_message = Some(error);
            }
        }
    }

    /// Reload the workspace from disk after external changes.
    ///
    /// Re-reads the UFO directory and replaces the workspace contents.
    /// If an editor session is active, reloads its glyph data while
    /// preserving viewport, tool, and text buffer state.
    pub fn reload_workspace_from_disk(&mut self) {
        // Get the active workspace (works for both single UFO and
        // designspace active master)
        let Some(workspace_arc) = self.active_workspace() else {
            return;
        };

        // Get the path from the current workspace
        let path = read_workspace(&workspace_arc).path.clone();

        // Reload from disk
        let fresh_workspace = match Workspace::load(&path) {
            Ok(ws) => ws,
            Err(e) => {
                tracing::error!(
                    "Failed to reload workspace: {}",
                    e
                );
                return;
            }
        };

        tracing::info!(
            "Reloaded workspace: {} ({} glyphs)",
            fresh_workspace.display_name(),
            fresh_workspace.glyph_count()
        );

        // Replace workspace contents
        *write_workspace(&workspace_arc) = fresh_workspace;

        // Reload the active editor session if one exists
        self.reload_active_editor_session(&workspace_arc);
    }

    /// Reload the active editor session from the refreshed workspace.
    ///
    /// Follows the same pattern as `switch_editor_master()`: looks up
    /// the active glyph by name, updates paths and metrics, clears
    /// selection, but preserves viewport/tool/text buffer state.
    fn reload_active_editor_session(
        &mut self,
        workspace_arc: &Arc<RwLock<Workspace>>,
    ) {
        let Some(ref mut session) = self.editor_session else {
            return;
        };

        // Reload the active sort's glyph from the refreshed workspace
        let Some(glyph_name) = session.active_sort_name.clone() else {
            return;
        };

        let workspace = read_workspace(workspace_arc);

        let Some(glyph) = workspace.get_glyph(&glyph_name) else {
            tracing::warn!(
                "Glyph '{}' not found after reload",
                glyph_name
            );
            return;
        };

        // Update glyph data
        session.glyph = Arc::new(glyph.clone());

        // Convert contours to editable paths
        let paths: Vec<crate::path::Path> = glyph
            .contours
            .iter()
            .map(crate::path::Path::from_contour)
            .collect();
        session.paths = Arc::new(paths);

        // Update font metrics from refreshed workspace
        session.units_per_em =
            workspace.units_per_em.unwrap_or(1000.0);
        session.ascender = workspace.ascender.unwrap_or(800.0);
        session.descender = workspace.descender.unwrap_or(-200.0);
        session.x_height = workspace.x_height;
        session.cap_height = workspace.cap_height;

        // Clear selection since point IDs have changed
        session.selection = crate::editing::Selection::new();
        session.selected_component = None;

        tracing::info!(
            "Reloaded editor session for glyph '{}'",
            glyph_name
        );
    }
}
