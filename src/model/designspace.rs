// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Designspace project management for variable font editing
//!
//! A designspace file ties multiple UFO masters together for creating
//! variable fonts. This module handles loading, editing, and saving
//! designspace projects with their associated UFO sources.

use anyhow::{Context, Result};
use norad::designspace::{DesignSpaceDocument, Source as NoradSource};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use super::workspace::Workspace;

// ============================================================================
// DATA STRUCTURES
// ============================================================================

/// A designspace project containing multiple font masters
///
/// This wraps a norad DesignSpaceDocument and loads all referenced UFO
/// sources into Workspace instances for editing.
#[derive(Debug)]
#[allow(dead_code)]
pub struct DesignspaceProject {
    /// Path to the .designspace file
    pub path: PathBuf,

    /// Design axes (wght, wdth, etc.)
    pub axes: Vec<DesignAxis>,

    /// Font masters (one workspace per source)
    pub masters: Vec<Master>,

    /// Index of the currently active master
    pub active_master: usize,

    /// Named instances (for reference)
    pub instances: Vec<Instance>,

    /// Original designspace document (for round-tripping)
    designspace_doc: DesignSpaceDocument,
}

/// A design axis (e.g., Weight, Width)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DesignAxis {
    /// Axis tag (e.g., "wght", "wdth")
    pub tag: String,
    /// Human-readable name (e.g., "Weight")
    pub name: String,
    /// Minimum value
    pub minimum: f64,
    /// Maximum value
    pub maximum: f64,
    /// Default value
    pub default: f64,
}

/// A font master (source in designspace terms)
#[derive(Debug)]
pub struct Master {
    /// Display name (e.g., "Virtua Grotesk Regular")
    pub name: String,

    /// Style name (e.g., "Regular", "Bold")
    pub style_name: String,

    /// Location in design space (axis tag -> value)
    pub location: HashMap<String, f64>,

    /// The loaded workspace (wrapped for shared access)
    pub workspace: Arc<RwLock<Workspace>>,

    /// Path to the UFO file
    pub ufo_path: PathBuf,

    /// Whether this master has unsaved changes
    pub modified: bool,
}

/// A named instance (specific point in design space)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Instance {
    /// Instance name (e.g., "Virtua Grotesk Medium")
    pub name: String,
    /// Style name
    pub style_name: String,
    /// Location in design space
    pub location: HashMap<String, f64>,
    /// Output filename (if specified)
    pub filename: Option<String>,
}

// ============================================================================
// IMPLEMENTATION
// ============================================================================

impl DesignspaceProject {
    /// Load a designspace project from a .designspace file
    ///
    /// This parses the designspace XML and loads all referenced UFO sources.
    pub fn load(path: &Path) -> Result<Self> {
        tracing::info!("Loading designspace: {}", path.display());

        // Parse the designspace file
        let designspace_doc = DesignSpaceDocument::load(path)
            .with_context(|| format!("Failed to parse designspace: {}", path.display()))?;

        // Get the directory containing the designspace for resolving relative paths
        let base_dir = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid designspace path"))?;

        // Parse axes
        let axes = Self::parse_axes(&designspace_doc);
        tracing::debug!("Found {} axes", axes.len());

        // Load all sources as masters
        let masters = Self::load_masters(&designspace_doc, base_dir)?;
        tracing::info!("Loaded {} masters", masters.len());

        // Parse instances
        let instances = Self::parse_instances(&designspace_doc);
        tracing::debug!("Found {} instances", instances.len());

        // Find the default master based on axis defaults
        let active_master = Self::find_default_master(&axes, &masters);
        tracing::info!(
            "Default master: {} ({})",
            masters[active_master].name,
            masters[active_master].style_name
        );

        Ok(Self {
            path: path.to_path_buf(),
            axes,
            masters,
            active_master,
            instances,
            designspace_doc,
        })
    }

    /// Find the default master based on axis default values
    ///
    /// First tries to find a master that matches all axis defaults exactly.
    /// If no exact match, finds the master closest to weight 400 (standard Regular).
    fn find_default_master(axes: &[DesignAxis], masters: &[Master]) -> usize {
        if masters.is_empty() {
            return 0;
        }

        // Build default location from axes
        let default_location: HashMap<String, f64> = axes
            .iter()
            .map(|axis| (axis.name.clone(), axis.default))
            .collect();

        // Try to find exact match for default location
        for (index, master) in masters.iter().enumerate() {
            if Self::location_matches(&master.location, &default_location) {
                tracing::debug!("Found exact default master match at index {}", index);
                return index;
            }
        }

        // No exact match - find master closest to weight 400
        // Look for weight axis (could be named "Weight" or have tag "wght")
        let weight_axis = axes
            .iter()
            .find(|a| a.tag.eq_ignore_ascii_case("wght") || a.name.eq_ignore_ascii_case("weight"));

        if let Some(weight_axis) = weight_axis {
            let target_weight = 400.0;
            let mut best_index = 0;
            let mut best_distance = f64::MAX;

            for (index, master) in masters.iter().enumerate() {
                if let Some(&weight) = master.location.get(&weight_axis.name) {
                    let distance = (weight - target_weight).abs();
                    if distance < best_distance {
                        best_distance = distance;
                        best_index = index;
                    }
                }
            }

            tracing::debug!(
                "No exact default match, using master {} closest to weight 400",
                best_index
            );
            return best_index;
        }

        // No weight axis found, default to first master
        tracing::debug!("No weight axis found, defaulting to first master");
        0
    }

    /// Check if a master's location matches the target location
    fn location_matches(
        master_loc: &HashMap<String, f64>,
        target_loc: &HashMap<String, f64>,
    ) -> bool {
        // Check that all target axes are matched (with small epsilon for float comparison)
        for (axis_name, &target_value) in target_loc {
            match master_loc.get(axis_name) {
                Some(&master_value) => {
                    if (master_value - target_value).abs() > 0.001 {
                        return false;
                    }
                }
                None => return false,
            }
        }
        true
    }

    /// Parse axes from the designspace document
    fn parse_axes(doc: &DesignSpaceDocument) -> Vec<DesignAxis> {
        doc.axes
            .iter()
            .map(|axis| DesignAxis {
                tag: axis.tag.clone(),
                name: axis.name.clone(),
                minimum: axis.minimum.unwrap_or(0.0) as f64,
                maximum: axis.maximum.unwrap_or(1000.0) as f64,
                default: axis.default as f64,
            })
            .collect()
    }

    /// Load all source UFOs as masters
    fn load_masters(doc: &DesignSpaceDocument, base_dir: &Path) -> Result<Vec<Master>> {
        let mut masters = Vec::new();

        for source in &doc.sources {
            let master = Self::load_master(source, base_dir)?;
            masters.push(master);
        }

        if masters.is_empty() {
            anyhow::bail!("Designspace has no sources");
        }

        Ok(masters)
    }

    /// Load a single master from a source element
    fn load_master(source: &NoradSource, base_dir: &Path) -> Result<Master> {
        // Resolve the UFO path (relative to designspace directory)
        let ufo_path = base_dir.join(&source.filename);

        tracing::debug!(
            "Loading master '{}' from {}",
            source.name.as_deref().unwrap_or("unnamed"),
            ufo_path.display()
        );

        // Load the UFO as a workspace
        let workspace = Workspace::load(&ufo_path)
            .with_context(|| format!("Failed to load UFO: {}", ufo_path.display()))?;

        // Extract location from source
        let location = Self::parse_location(source);

        // Get names
        let name = source.name.clone().unwrap_or_else(|| {
            ufo_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string()
        });

        let style_name = source
            .stylename
            .clone()
            .unwrap_or_else(|| "Regular".to_string());

        Ok(Master {
            name,
            style_name,
            location,
            workspace: Arc::new(RwLock::new(workspace)),
            ufo_path,
            modified: false,
        })
    }

    /// Parse location from a source's dimensions
    fn parse_location(source: &NoradSource) -> HashMap<String, f64> {
        source
            .location
            .iter()
            .filter_map(|dim| dim.xvalue.map(|v| (dim.name.clone(), v as f64)))
            .collect()
    }

    /// Parse instances from the designspace document
    fn parse_instances(doc: &DesignSpaceDocument) -> Vec<Instance> {
        doc.instances
            .iter()
            .map(|inst| {
                let location = inst
                    .location
                    .iter()
                    .filter_map(|dim| dim.xvalue.map(|v| (dim.name.clone(), v as f64)))
                    .collect();

                Instance {
                    name: inst.name.clone().unwrap_or_default(),
                    style_name: inst.stylename.clone().unwrap_or_default(),
                    location,
                    filename: inst.filename.clone(),
                }
            })
            .collect()
    }

    /// Get the currently active workspace
    pub fn active_workspace(&self) -> Arc<RwLock<Workspace>> {
        Arc::clone(&self.masters[self.active_master].workspace)
    }

    /// Get the active master
    pub fn active_master(&self) -> &Master {
        &self.masters[self.active_master]
    }

    /// Get the active master mutably
    #[allow(dead_code)]
    pub fn active_master_mut(&mut self) -> &mut Master {
        &mut self.masters[self.active_master]
    }

    /// Switch to a different master by index
    ///
    /// Returns true if the switch was successful, false if index out of bounds
    pub fn switch_master(&mut self, index: usize) -> bool {
        if index < self.masters.len() {
            self.active_master = index;
            tracing::info!(
                "Switched to master: {} ({})",
                self.masters[index].name,
                self.masters[index].style_name
            );
            true
        } else {
            false
        }
    }

    /// Mark the active master as modified
    pub fn mark_active_modified(&mut self) {
        self.masters[self.active_master].modified = true;
    }

    /// Check if any master has unsaved changes
    #[allow(dead_code)]
    pub fn has_unsaved_changes(&self) -> bool {
        self.masters.iter().any(|m| m.modified)
    }

    /// Get the display name for this designspace
    pub fn display_name(&self) -> String {
        self.path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string()
    }

    /// Get the number of glyphs in the active master
    pub fn glyph_count(&self) -> usize {
        self.active_workspace()
            .read()
            .map(|ws| ws.glyph_count())
            .unwrap_or(0)
    }

    /// Get glyph names from the active master
    #[allow(dead_code)]
    pub fn glyph_names(&self) -> Vec<String> {
        self.active_workspace()
            .read()
            .map(|ws| ws.glyph_names())
            .unwrap_or_default()
    }

    /// Save all modified masters and the designspace file
    pub fn save(&mut self) -> Result<()> {
        tracing::info!("Saving designspace project: {}", self.path.display());

        // Save each modified master
        for master in &mut self.masters {
            if master.modified {
                tracing::info!("Saving modified master: {}", master.name);
                let workspace = master
                    .workspace
                    .read()
                    .map_err(|e| anyhow::anyhow!("Failed to lock workspace: {}", e))?;
                workspace.save().with_context(|| {
                    format!("Failed to save UFO: {}", master.ufo_path.display())
                })?;
                master.modified = false;
            }
        }

        // Save the designspace document (preserves structure)
        self.designspace_doc
            .save(&self.path)
            .with_context(|| format!("Failed to save designspace: {}", self.path.display()))?;

        tracing::info!("Designspace saved successfully");
        Ok(())
    }
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Check if a path is a designspace file
pub fn is_designspace_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("designspace"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_designspace_file() {
        assert!(is_designspace_file(Path::new("font.designspace")));
        assert!(is_designspace_file(Path::new("font.DESIGNSPACE")));
        assert!(is_designspace_file(Path::new("/path/to/font.designspace")));
        assert!(!is_designspace_file(Path::new("font.ufo")));
        assert!(!is_designspace_file(Path::new("font.txt")));
    }
}
