// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! File I/O operations for AppState (open, load, save)

use super::AppState;
use crate::model::designspace::{DesignspaceProject, is_designspace_file};
use crate::model::workspace::Workspace;
use chrono::Local;
use std::path::PathBuf;
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
                .map(|ws| ws.read().unwrap().path.clone())
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
                .map(|w| w.read().unwrap().display_name())
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
}
