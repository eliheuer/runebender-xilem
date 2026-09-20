// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Transient UFO import payloads and glyph-free source persistence state.
//!
//! A [`SourceInput`] exists only while a format adapter hands a decoded UFO to
//! [`Project`](super::project::Project).
//! The live project stores [`SourceState`] instead: paths, opaque filesystem payloads and save
//! status that are not canonical font data.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// One decoded UFO at an explicit import or fixture boundary.
///
/// Constructing a project consumes this value; it is never retained as editable state.
#[derive(Debug)]
pub struct SourceInput {
    pub(super) font: norad::Font,
    pub(super) source_path: PathBuf,
    pub(super) glif_paths: HashMap<String, String>,
    pub(super) preserved_files: super::filesystem::PreservedFiles,
    pub(super) dirty: bool,
}

impl SourceInput {
    /// Wrap a decoded UFO for immediate canonical project construction.
    pub fn from_font(font: norad::Font, source_path: PathBuf) -> Self {
        Self {
            font,
            source_path,
            glif_paths: HashMap::new(),
            preserved_files: super::filesystem::PreservedFiles::default(),
            dirty: false,
        }
    }

    /// Load a UFO as a transient project-construction input.
    pub fn load(path: &Path) -> Result<Self, String> {
        super::filesystem::load_ufo(path).map(|source| source.into_source_input(path.to_path_buf()))
    }
}

/// Glyph-free state retained for one canonical source.
#[derive(Debug, Clone)]
pub(super) struct SourceState {
    /// Names of glyphs edited since load/save, retained for external-change accounting.
    pub(super) modified_glyphs: HashSet<String>,
    /// Glyph name to GLIF path relative to the UFO root for compatibility with memory hosts.
    pub(super) glif_paths: HashMap<String, String>,
    /// Filesystem details outside canonical ownership that must survive saves.
    pub(super) preserved_files: super::filesystem::PreservedFiles,
    /// Whether kerning changed since load/save.
    pub(super) kerning_dirty: bool,
    /// Path of the UFO on disk, or a virtual path for in-memory hosts.
    pub(super) source_path: PathBuf,
    /// Whether canonical or persistence state changed since the last save.
    pub(super) dirty: bool,
}

impl SourceState {
    pub(super) fn from_input(input: SourceInput) -> Self {
        Self {
            modified_glyphs: HashSet::new(),
            glif_paths: input.glif_paths,
            preserved_files: input.preserved_files,
            kerning_dirty: false,
            source_path: input.source_path,
            dirty: input.dirty,
        }
    }

    pub(super) fn new(source_path: PathBuf, dirty: bool) -> Self {
        Self {
            modified_glyphs: HashSet::new(),
            glif_paths: HashMap::new(),
            preserved_files: super::filesystem::PreservedFiles::default(),
            kerning_dirty: false,
            source_path,
            dirty,
        }
    }
}
