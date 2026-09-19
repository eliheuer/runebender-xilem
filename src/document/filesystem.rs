// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Validated filesystem import and staged export for UFO and Designspace documents.
//!
//! Norad is the transient source-format codec in this module.
//! Project construction consumes a completely loaded import plan, while saving first writes and
//! reloads every staged artifact before any live destination is replaced.

use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::project::Master;
use crate::formats::glyphs_import::ConversionResult;

/// Filesystem details outside canonical ownership that must survive an ordinary save.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PreservedFiles {
    files: BTreeMap<PathBuf, Vec<u8>>,
    glif_paths: BTreeMap<(String, String), PathBuf>,
}

impl PreservedFiles {
    fn write_into(&self, root: &Path) -> Result<(), String> {
        for (relative, bytes) in &self.files {
            let destination = root.join(relative);
            if destination.exists() {
                return Err(format!(
                    "preserved UFO payload conflicts with generated file {}",
                    relative.display()
                ));
            }
            let parent = destination
                .parent()
                .ok_or_else(|| format!("invalid preserved UFO path {}", relative.display()))?;
            fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
            fs::write(&destination, bytes)
                .map_err(|error| format!("{}: {error}", destination.display()))?;
        }
        Ok(())
    }

    fn restore_glif_paths(&self, root: &Path, font: &norad::Font) -> Result<(), String> {
        for layer in font.layers.iter() {
            let layer_path = root.join(layer.path());
            let contents_path = layer_path.join("contents.plist");
            let mut contents: BTreeMap<String, PathBuf> = plist::from_file(&contents_path)
                .map_err(|error| format!("{}: {error}", contents_path.display()))?;
            let mut moves = Vec::new();
            for glyph in layer.iter() {
                let key = (layer.name().to_string(), glyph.name().to_string());
                let Some(desired) = self.glif_paths.get(&key) else {
                    continue;
                };
                validate_relative_file(desired)?;
                let current = contents
                    .get(glyph.name().as_str())
                    .ok_or_else(|| format!("missing staged path for glyph {:?}", glyph.name()))?;
                if current != desired {
                    moves.push((glyph.name().to_string(), current.clone(), desired.clone()));
                }
            }
            let mut temporary = Vec::new();
            for (name, current, desired) in moves {
                let current_path = layer_path.join(&current);
                let temp = fresh_sibling(&current_path, "glif")?;
                fs::rename(&current_path, &temp)
                    .map_err(|error| format!("{}: {error}", current_path.display()))?;
                temporary.push((name, temp, desired));
            }
            for (name, temp, desired) in temporary {
                let destination = layer_path.join(&desired);
                if destination.exists() {
                    return Err(format!(
                        "preserved GLIF path conflicts with generated file {}",
                        destination.display()
                    ));
                }
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|error| format!("{}: {error}", parent.display()))?;
                }
                fs::rename(&temp, &destination)
                    .map_err(|error| format!("{}: {error}", destination.display()))?;
                contents.insert(name, desired);
            }
            plist::to_file_xml(&contents_path, &contents)
                .map_err(|error| format!("{}: {error}", contents_path.display()))?;
        }
        Ok(())
    }

    fn validate_glif_paths(&self, font: &norad::Font) -> Result<(), String> {
        for ((layer_name, glyph_name), expected) in &self.glif_paths {
            let Some(layer) = font.layers.get(layer_name) else {
                continue;
            };
            let Some(actual) = layer.get_path(glyph_name) else {
                continue;
            };
            if actual != expected {
                return Err(format!(
                    "staged glyph {glyph_name:?} changed its path from {} to {}",
                    expected.display(),
                    actual.display()
                ));
            }
        }
        Ok(())
    }
}

/// One fully decoded UFO source in a filesystem import plan.
#[derive(Debug)]
pub(crate) struct ImportedUfo {
    font: norad::Font,
    preserved: PreservedFiles,
}

impl ImportedUfo {
    pub(crate) fn into_master(self, path: PathBuf) -> Master {
        let glif_paths = self
            .font
            .default_layer()
            .iter()
            .filter_map(|glyph| {
                let name = glyph.name().to_string();
                let relative = self.font.default_layer().get_path(&name)?;
                Some((
                    name,
                    self.font
                        .default_layer()
                        .path()
                        .join(relative)
                        .to_string_lossy()
                        .into_owned(),
                ))
            })
            .collect();
        let mut master = Master::from_font(self.font, path);
        master.glif_paths = glif_paths;
        master.preserved_files = self.preserved;
        master
    }
}

/// A UFO or Designspace whose complete source set was validated before Project construction.
#[derive(Debug)]
pub(crate) enum ImportPlan {
    Ufo {
        path: PathBuf,
        source: Box<ImportedUfo>,
    },
    Designspace {
        path: PathBuf,
        document: Box<norad::designspace::DesignSpaceDocument>,
        sources: BTreeMap<String, ImportedUfo>,
    },
}

impl ImportPlan {
    pub(crate) fn read(path: &Path) -> Result<Self, String> {
        if path
            .extension()
            .is_some_and(|extension| extension == "designspace")
        {
            let document = crate::formats::designspace::load(path)?;
            let directory = path.parent().unwrap_or(Path::new("."));
            let mut sources = BTreeMap::new();
            for source in document
                .sources
                .iter()
                .filter(|source| source.layer.is_none())
            {
                if sources.contains_key(&source.filename) {
                    continue;
                }
                let source_path = directory.join(&source.filename);
                let imported = load_ufo(&source_path)
                    .map_err(|error| format!("{}: {error}", source_path.display()))?;
                sources.insert(source.filename.clone(), imported);
            }
            Ok(Self::Designspace {
                path: path.to_path_buf(),
                document: Box::new(document),
                sources,
            })
        } else {
            Ok(Self::Ufo {
                path: path.to_path_buf(),
                source: Box::new(
                    load_ufo(path).map_err(|error| format!("{}: {error}", path.display()))?,
                ),
            })
        }
    }
}

pub(crate) fn load_ufo(path: &Path) -> Result<ImportedUfo, String> {
    let font = norad::Font::load(path).map_err(|error| error.to_string())?;
    let preserved = capture_preserved_files(path, &font)?;
    Ok(ImportedUfo { font, preserved })
}

/// One source ready to be serialized without consulting live Project state.
#[derive(Debug)]
pub(crate) struct SourceExport {
    pub(crate) destination: PathBuf,
    pub(crate) font: norad::Font,
    pub(crate) preserved: PreservedFiles,
}

/// One non-UFO file that must publish atomically with a Save As copy.
#[derive(Debug)]
pub(crate) struct FileExport {
    pub(crate) destination: PathBuf,
    pub(crate) bytes: Vec<u8>,
}

/// A complete immutable save plan for all UFOs and optional Designspace metadata.
#[derive(Debug)]
pub(crate) struct ExportPlan {
    sources: Vec<SourceExport>,
    designspace: Option<(PathBuf, norad::designspace::DesignSpaceDocument)>,
    files: Vec<FileExport>,
    replace_existing: bool,
}

impl ExportPlan {
    pub(crate) fn new(
        sources: Vec<SourceExport>,
        designspace: Option<(PathBuf, norad::designspace::DesignSpaceDocument)>,
    ) -> Result<Self, String> {
        if sources.is_empty() {
            return Err("a filesystem export needs at least one UFO source".into());
        }
        let plan = Self {
            sources,
            designspace,
            files: Vec::new(),
            replace_existing: true,
        };
        plan.validate_destinations()?;
        Ok(plan)
    }

    /// Add ordinary files that must stage and publish with the font sources.
    pub(crate) fn with_files(mut self, files: Vec<FileExport>) -> Result<Self, String> {
        self.files = files;
        self.validate_destinations()?;
        Ok(self)
    }

    /// Require every destination to remain absent through publication.
    pub(crate) fn require_new_destinations(mut self) -> Result<Self, String> {
        for destination in self.destinations() {
            if path_entry_exists(destination)? {
                return Err(format!(
                    "filesystem export destination already exists: {}",
                    destination.display()
                ));
            }
        }
        self.replace_existing = false;
        Ok(self)
    }

    fn destinations(&self) -> Vec<&Path> {
        let mut destinations = self
            .sources
            .iter()
            .map(|source| source.destination.as_path())
            .collect::<Vec<_>>();
        if let Some((path, _)) = &self.designspace {
            destinations.push(path);
        }
        destinations.extend(self.files.iter().map(|file| file.destination.as_path()));
        destinations
    }

    fn validate_destinations(&self) -> Result<(), String> {
        let destinations = self.destinations();
        let mut destination_keys: Vec<PathBuf> = Vec::with_capacity(destinations.len());
        for destination in destinations {
            if destination.as_os_str().is_empty() {
                return Err("a filesystem export destination cannot be empty".into());
            }
            let key = destination_key(destination)?;
            if destination_keys
                .iter()
                .any(|previous| paths_overlap(previous, &key))
            {
                return Err(format!(
                    "filesystem export destinations overlap at {}",
                    destination.display()
                ));
            }
            destination_keys.push(key);
        }
        Ok(())
    }

    pub(crate) fn execute(self) -> Result<(), String> {
        let mut staged = Vec::new();
        for source in self.sources {
            let stage = fresh_sibling(&source.destination, "stage")?;
            if let Err(error) = stage_ufo(&source, &stage) {
                remove_any(&stage);
                return Err(error);
            }
            staged.push(StagedArtifact {
                destination: source.destination,
                stage,
                created_parents: Vec::new(),
            });
        }
        if let Some((destination, document)) = self.designspace {
            let stage = fresh_sibling(&destination, "stage")?;
            if let Some(parent) = stage.parent().filter(|path| !path.as_os_str().is_empty()) {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("{}: {error}", parent.display()))?;
            }
            if let Err(error) = document.save(&stage) {
                remove_any(&stage);
                return Err(format!("{}: {error}", destination.display()));
            }
            let staged_document = match norad::designspace::DesignSpaceDocument::load(&stage) {
                Ok(document) => document,
                Err(error) => {
                    remove_any(&stage);
                    return Err(format!("staged {}: {error}", destination.display()));
                }
            };
            if staged_document != document {
                remove_any(&stage);
                return Err(format!(
                    "staged {} changed supported Designspace data",
                    destination.display()
                ));
            }
            staged.push(StagedArtifact {
                destination,
                stage,
                created_parents: Vec::new(),
            });
        }
        for file in self.files {
            let stage = fresh_sibling(&file.destination, "stage")?;
            let artifact = StagedArtifact {
                destination: file.destination,
                created_parents: create_parent_directories(&stage)?,
                stage,
            };
            fs::write(&artifact.stage, &file.bytes)
                .map_err(|error| format!("{}: {error}", artifact.destination.display()))?;
            let staged_bytes = fs::read(&artifact.stage)
                .map_err(|error| format!("staged {}: {error}", artifact.destination.display()))?;
            if staged_bytes != file.bytes {
                return Err(format!(
                    "staged {} changed file data",
                    artifact.destination.display()
                ));
            }
            staged.push(artifact);
        }
        publish(staged, self.replace_existing)
    }
}

/// Stage, validate and publish one converted Glyphs file set without replacing existing output.
pub(crate) fn publish_glyphs_import(
    source: &Path,
    result: ConversionResult,
) -> Result<PathBuf, String> {
    if !result.warnings.is_empty() {
        return Err(format!(
            "Glyphs conversion reported unsupported data:\n- {}",
            result.warnings.join("\n- ")
        ));
    }
    let stem = source
        .file_stem()
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| format!("invalid Glyphs source path {}", source.display()))?
        .to_string_lossy();
    let parent = source.parent().unwrap_or(Path::new("."));
    let preferred = parent.join(format!("{stem}-ufo"));
    let destination = unused_import_destination(&preferred)?;
    let stage = fresh_sibling(&destination, "import")?;
    let staged = StagedArtifact {
        destination: destination.clone(),
        stage,
        created_parents: Vec::new(),
    };
    let open_relative = stage_generated_files(&staged.stage, result)?;
    ImportPlan::read(&staged.stage.join(&open_relative))
        .map_err(|error| format!("staged Glyphs conversion is invalid: {error}"))?;
    if path_entry_exists(&staged.destination)? {
        return Err(format!(
            "generated import destination appeared while staging: {}",
            staged.destination.display()
        ));
    }
    fs::rename(&staged.stage, &staged.destination).map_err(|error| {
        format!(
            "could not publish generated import {}: {error}",
            staged.destination.display()
        )
    })?;
    let open = staged.destination.join(open_relative);
    Ok(open)
}

/// Choose a sibling destination for an imported source without replacing an existing path.
pub(crate) fn unused_import_destination(preferred: &Path) -> Result<PathBuf, String> {
    if !path_entry_exists(preferred)? {
        return Ok(preferred.to_path_buf());
    }
    let parent = preferred.parent().unwrap_or(Path::new("."));
    let stem = preferred
        .file_stem()
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| format!("invalid import destination {}", preferred.display()))?
        .to_string_lossy();
    let extension = preferred
        .extension()
        .map(|extension| extension.to_os_string());
    for index in 1..=1_024 {
        let mut name = OsString::from(format!("{stem}-import-{index}"));
        if let Some(extension) = &extension {
            name.push(".");
            name.push(extension);
        }
        let candidate = parent.join(name);
        if !path_entry_exists(&candidate)? {
            return Ok(candidate);
        }
    }
    Err(format!(
        "could not choose an unused import destination beside {}",
        preferred.display()
    ))
}

fn stage_generated_files(root: &Path, result: ConversionResult) -> Result<PathBuf, String> {
    fs::create_dir_all(root).map_err(|error| format!("{}: {error}", root.display()))?;
    let mut paths = HashSet::new();
    let mut designspaces = Vec::new();
    let mut ufo_roots = Vec::new();
    for file in result.files {
        let relative = PathBuf::from(file.path);
        validate_relative_path(&relative, "generated import")?;
        if !paths.insert(relative.clone()) {
            return Err(format!(
                "generated import contains duplicate path {}",
                relative.display()
            ));
        }
        if relative
            .extension()
            .is_some_and(|extension| extension == "designspace")
        {
            designspaces.push(relative.clone());
        } else if relative
            .file_name()
            .is_some_and(|name| name == "fontinfo.plist")
            && relative
                .parent()
                .and_then(Path::extension)
                .is_some_and(|extension| extension == "ufo")
        {
            ufo_roots.push(relative.parent().unwrap().to_path_buf());
        }
        let destination = root.join(&relative);
        let parent = destination
            .parent()
            .ok_or_else(|| format!("invalid generated path {}", relative.display()))?;
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        fs::write(&destination, file.text)
            .map_err(|error| format!("{}: {error}", destination.display()))?;
    }
    match (designspaces.as_slice(), ufo_roots.as_slice()) {
        ([designspace], [_first, ..]) => Ok(designspace.clone()),
        ([], [ufo]) => Ok(ufo.clone()),
        ([], []) => Err("Glyphs conversion produced no UFO".into()),
        ([], _) => Err("Glyphs conversion produced multiple UFOs without a Designspace".into()),
        (_, _) => Err("Glyphs conversion produced multiple Designspace files".into()),
    }
}

fn stage_ufo(source: &SourceExport, stage: &Path) -> Result<(), String> {
    if let Some(parent) = stage.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    source
        .font
        .save(stage)
        .map_err(|error| format!("{}: {error}", source.destination.display()))?;
    plist::to_file_xml(stage.join("metainfo.plist"), &source.font.meta)
        .map_err(|error| format!("{}: {error}", source.destination.display()))?;

    let mut expected = source.font.clone();
    if expected.features.as_bytes().contains(&b'\r') {
        expected.features = expected.features.replace("\r\n", "\n");
    }
    let actual = norad::Font::load(stage)
        .map_err(|error| format!("staged {}: {error}", source.destination.display()))?;
    if actual != expected {
        return Err(format!(
            "staged {} changed supported UFO data",
            source.destination.display()
        ));
    }
    source.preserved.restore_glif_paths(stage, &source.font)?;
    source.preserved.write_into(stage)?;
    let final_font = norad::Font::load(stage)
        .map_err(|error| format!("staged {}: {error}", source.destination.display()))?;
    source.preserved.validate_glif_paths(&final_font)?;
    Ok(())
}

#[derive(Debug)]
struct StagedArtifact {
    destination: PathBuf,
    stage: PathBuf,
    created_parents: Vec<PathBuf>,
}

impl Drop for StagedArtifact {
    fn drop(&mut self) {
        remove_any(&self.stage);
        for directory in self.created_parents.iter().rev() {
            let _ = fs::remove_dir(directory);
        }
    }
}

#[derive(Debug)]
struct PublishedArtifact {
    destination: PathBuf,
    backup: Option<PathBuf>,
}

fn publish(staged: Vec<StagedArtifact>, replace_existing: bool) -> Result<(), String> {
    let mut published: Vec<PublishedArtifact> = Vec::new();
    for artifact in &staged {
        let destination_exists = match path_entry_exists(&artifact.destination) {
            Ok(exists) => exists,
            Err(error) => {
                rollback(&published);
                cleanup_staged(&staged);
                return Err(error);
            }
        };
        if destination_exists && !replace_existing {
            rollback(&published);
            cleanup_staged(&staged);
            return Err(format!(
                "filesystem export destination appeared during staging: {}",
                artifact.destination.display()
            ));
        }
        let backup = if destination_exists {
            let backup = match fresh_sibling(&artifact.destination, "backup") {
                Ok(backup) => backup,
                Err(error) => {
                    rollback(&published);
                    cleanup_staged(&staged);
                    return Err(error);
                }
            };
            if let Err(error) = fs::rename(&artifact.destination, &backup) {
                rollback(&published);
                cleanup_staged(&staged);
                return Err(format!(
                    "could not stage existing {} for replacement: {error}",
                    artifact.destination.display()
                ));
            }
            Some(backup)
        } else {
            None
        };
        if let Err(error) = fs::rename(&artifact.stage, &artifact.destination) {
            if let Some(backup) = &backup {
                let _ = fs::rename(backup, &artifact.destination);
            }
            rollback(&published);
            cleanup_staged(&staged);
            return Err(format!(
                "could not publish {}: {error}",
                artifact.destination.display()
            ));
        }
        published.push(PublishedArtifact {
            destination: artifact.destination.clone(),
            backup,
        });
    }
    for artifact in &published {
        if let Some(backup) = &artifact.backup {
            remove_any(backup);
        }
    }
    Ok(())
}

fn cleanup_staged(staged: &[StagedArtifact]) {
    for artifact in staged {
        remove_any(&artifact.stage);
    }
    let mut directories = staged
        .iter()
        .flat_map(|artifact| artifact.created_parents.iter().cloned())
        .collect::<Vec<_>>();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    directories.dedup();
    for directory in directories {
        let _ = fs::remove_dir(directory);
    }
}

fn rollback(published: &[PublishedArtifact]) {
    for artifact in published.iter().rev() {
        remove_any(&artifact.destination);
        if let Some(backup) = &artifact.backup {
            let _ = fs::rename(backup, &artifact.destination);
        }
    }
}

fn create_parent_directories(path: &Path) -> Result<Vec<PathBuf>, String> {
    let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) else {
        return Ok(Vec::new());
    };
    let mut missing = Vec::new();
    let mut current = parent;
    while !path_entry_exists(current)? {
        missing.push(current.to_path_buf());
        current = current
            .parent()
            .ok_or_else(|| format!("invalid filesystem destination {}", path.display()))?;
    }
    let mut created = Vec::new();
    for directory in missing.into_iter().rev() {
        if let Err(error) = fs::create_dir(&directory) {
            for created in created.iter().rev() {
                let _ = fs::remove_dir(created);
            }
            return Err(format!("{}: {error}", directory.display()));
        }
        created.push(directory);
    }
    Ok(created)
}

fn fresh_sibling(path: &Path, purpose: &str) -> Result<PathBuf, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| format!("invalid filesystem destination {}", path.display()))?
        .to_string_lossy();
    for _ in 0..1_024 {
        let candidate = parent.join(format!(
            ".{name}.runebender-{purpose}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        if !path_entry_exists(&candidate)? {
            return Ok(candidate);
        }
    }
    Err(format!(
        "could not reserve a temporary path beside {}",
        path.display()
    ))
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

pub(super) fn path_entry_exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

pub(super) fn destination_key(path: &Path) -> Result<PathBuf, String> {
    let mut key = if path.is_absolute() {
        PathBuf::new()
    } else {
        let current =
            std::env::current_dir().map_err(|error| format!("current directory: {error}"))?;
        fs::canonicalize(&current).map_err(|error| format!("{}: {error}", current.display()))?
    };
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => key.push(prefix.as_os_str()),
            Component::RootDir => key.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                key.pop();
            }
            Component::Normal(part) => {
                let candidate = key.join(part);
                match fs::symlink_metadata(&candidate) {
                    Ok(_) => {
                        key = fs::canonicalize(&candidate)
                            .map_err(|error| format!("{}: {error}", candidate.display()))?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        key = candidate;
                    }
                    Err(error) => return Err(format!("{}: {error}", candidate.display())),
                }
            }
        }
    }
    Ok(key)
}

fn remove_any(path: &Path) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.file_type().is_dir() {
        let _ = fs::remove_dir_all(path);
    } else {
        let _ = fs::remove_file(path);
    }
}

fn capture_preserved_files(root: &Path, font: &norad::Font) -> Result<PreservedFiles, String> {
    let managed = managed_ufo_files(font);
    let mut files = BTreeMap::new();
    collect_preserved(root, root, &managed, &mut files)?;
    let glif_paths = font
        .layers
        .iter()
        .flat_map(|layer| {
            layer.iter().filter_map(|glyph| {
                layer.get_path(glyph.name().as_str()).map(|path| {
                    (
                        (layer.name().to_string(), glyph.name().to_string()),
                        path.to_path_buf(),
                    )
                })
            })
        })
        .collect();
    Ok(PreservedFiles { files, glif_paths })
}

fn validate_relative_file(path: &Path) -> Result<(), String> {
    validate_relative_path(path, "preserved GLIF")
}

fn validate_relative_path(path: &Path, label: &str) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("unsafe {label} path {}", path.display()));
    }
    Ok(())
}

fn collect_preserved(
    root: &Path,
    directory: &Path,
    managed: &HashSet<PathBuf>,
    preserved: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> Result<(), String> {
    let entries =
        fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", directory.display()))?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|error| format!("{}: {error}", path.display()))?
            .to_path_buf();
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "unsupported UFO symlink payload: {}",
                relative.display()
            ));
        }
        if metadata.is_dir() {
            collect_preserved(root, &path, managed, preserved)?;
        } else if metadata.is_file()
            && !managed.contains(&relative)
            && !relative.starts_with("data")
            && !relative.starts_with("images")
        {
            let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            preserved.insert(relative, bytes);
        } else if !metadata.is_file() {
            return Err(format!(
                "unsupported UFO filesystem payload: {}",
                relative.display()
            ));
        }
    }
    Ok(())
}

fn managed_ufo_files(font: &norad::Font) -> HashSet<PathBuf> {
    let mut managed = [
        "metainfo.plist",
        "fontinfo.plist",
        "lib.plist",
        "groups.plist",
        "kerning.plist",
        "features.fea",
        "layercontents.plist",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect::<HashSet<_>>();
    for layer in font.layers.iter() {
        managed.insert(layer.path().join("contents.plist"));
        managed.insert(layer.path().join("layerinfo.plist"));
        managed.extend(
            layer
                .iter()
                .filter_map(|glyph| layer.get_path(glyph.name().as_str()))
                .map(|path| layer.path().join(path)),
        );
    }
    managed
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::document::project::Project;
    use crate::document::variable::SourceId;
    use crate::formats::glyphs_import::{ConversionResult, ConvertedFile};

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "runebender-filesystem-{label}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn project_save_preserves_filesystem_payload_and_ufo_paths() {
        let scratch = Scratch::new("preservation");
        let path = scratch.0.join("Preserved.ufo");
        write_ufo(&path, "Regular", true);

        let mut project = Project::load(&path).unwrap();
        let source = project.document_source(SourceId(0)).unwrap();
        let default_layer = source.default_layer();
        assert!(project.edit_layer("A", &default_layer, |glyph| glyph.width = 612.5));
        project.save().unwrap();

        let reloaded = norad::Font::load(&path).unwrap();
        assert_eq!(
            reloaded.meta.creator.as_deref(),
            Some("com.example.original")
        );
        assert_eq!(
            reloaded
                .layers
                .iter()
                .map(|layer| (layer.name().as_str(), layer.path()))
                .collect::<Vec<_>>(),
            [
                ("public.default", Path::new("glyphs")),
                ("Background", Path::new("glyphs.background-custom")),
                ("Sketch", Path::new("glyphs.sketch-custom")),
            ]
        );
        assert_eq!(
            reloaded.default_layer().get_path("A"),
            Some(Path::new("A.custom-name.glif"))
        );
        assert_eq!(reloaded.get_glyph("A").unwrap().width, 612.5);
        assert_eq!(
            reloaded
                .lib
                .get("com.example.unknown")
                .and_then(plist::Value::as_string),
            Some("preserved")
        );
        assert_eq!(
            reloaded
                .data
                .get(Path::new("private/payload.bin"))
                .unwrap()
                .unwrap()
                .as_ref(),
            [0, 1, 2, 255]
        );
        assert_eq!(
            reloaded
                .images
                .get(Path::new("reference.png"))
                .unwrap()
                .unwrap()
                .as_ref(),
            include_bytes!("../../tests/fixtures/variable/reference.png")
        );
        assert_eq!(
            fs::read(path.join("vendor/opaque.bin")).unwrap(),
            b"unknown root payload"
        );
    }

    #[test]
    fn project_save_validates_every_source_before_replacing_any_destination() {
        let scratch = Scratch::new("atomic");
        let regular = scratch.0.join("Regular.ufo");
        let bold = scratch.0.join("Bold.ufo");
        write_ufo(&regular, "Regular", false);
        write_ufo(&bold, "Bold", false);
        let designspace = scratch.0.join("Font.designspace");
        fs::write(
            &designspace,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<designspace format="5.0">
  <axes><axis tag="wght" name="Weight" minimum="0" default="0" maximum="1"/></axes>
  <sources>
    <source filename="Regular.ufo" name="regular"><location><dimension name="Weight" xvalue="0"/></location></source>
    <source filename="Bold.ufo" name="bold"><location><dimension name="Weight" xvalue="1"/></location></source>
  </sources>
</designspace>
"#,
        )
        .unwrap();
        let regular_before = fs::read(regular.join("glyphs/A.custom-name.glif")).unwrap();
        let bold_before = fs::read(bold.join("glyphs/A.custom-name.glif")).unwrap();

        let mut project = Project::load(&designspace).unwrap();
        assert_eq!(
            project.document_source_path(SourceId(0)),
            Some(regular.as_path())
        );
        assert_eq!(
            project.document_source_path(SourceId(1)),
            Some(bold.as_path())
        );
        let regular_layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        assert!(project.edit_layer("A", &regular_layer, |glyph| glyph.width = 777.0));
        {
            let mut sources = project.edit_sources();
            sources[1]
                .font
                .lib
                .insert("public.objectLibs".into(), "invalid staged payload".into());
            sources[1].dirty = true;
        }

        let error = project.save().unwrap_err();
        assert!(error.contains("public.objectLibs"), "{error}");
        assert_eq!(
            fs::read(regular.join("glyphs/A.custom-name.glif")).unwrap(),
            regular_before,
            "the first source must not publish before every source validates"
        );
        assert_eq!(
            fs::read(bold.join("glyphs/A.custom-name.glif")).unwrap(),
            bold_before
        );
        assert!(project.sources()[0].dirty);
        assert!(project.sources()[1].dirty);
        assert!(
            fs::read_dir(&scratch.0)
                .unwrap()
                .flatten()
                .all(|entry| !entry
                    .file_name()
                    .to_string_lossy()
                    .contains("runebender-stage")),
            "failed staging must clean its temporary artifacts"
        );
    }

    #[test]
    fn export_preflight_rejects_normalized_destination_aliases() {
        let scratch = Scratch::new("destination-alias");
        let error = ExportPlan::new(
            vec![
                empty_source(scratch.0.join("masters/../Regular.ufo")),
                empty_source(scratch.0.join("Regular.ufo")),
            ],
            None,
        )
        .unwrap_err();
        assert!(error.contains("destinations overlap"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn export_preflight_rejects_existing_symlink_aliases() {
        let scratch = Scratch::new("destination-symlink");
        let alias = scratch.0.join("alias");
        std::os::unix::fs::symlink(&scratch.0, &alias).unwrap();
        let error = ExportPlan::new(
            vec![
                empty_source(scratch.0.join("Regular.ufo")),
                empty_source(alias.join("Regular.ufo")),
            ],
            None,
        )
        .unwrap_err();
        assert!(error.contains("destinations overlap"), "{error}");
    }

    #[test]
    fn glyphs_warnings_fail_before_creating_output() {
        let scratch = Scratch::new("glyphs-warning");
        let source = scratch.0.join("Example.glyphs");
        let result = ConversionResult {
            family_name: "Example".into(),
            files: Vec::new(),
            warnings: vec!["A (Regular): unsupported layer".into()],
        };
        let error = publish_glyphs_import(&source, result).unwrap_err();
        assert!(error.contains("unsupported layer"), "{error}");
        assert!(!scratch.0.join("Example-ufo").exists());
    }

    #[test]
    fn glyphs_output_uses_a_validated_collision_free_destination() {
        let scratch = Scratch::new("glyphs-collision");
        let source = scratch.0.join("Example.glyphs");
        let occupied = scratch.0.join("Example-ufo");
        fs::create_dir(&occupied).unwrap();
        fs::write(occupied.join("sentinel"), b"keep").unwrap();

        let open = publish_glyphs_import(&source, generated_ufo()).unwrap();

        assert_eq!(
            open,
            scratch.0.join("Example-ufo-import-1/Example-Regular.ufo")
        );
        assert_eq!(fs::read(occupied.join("sentinel")).unwrap(), b"keep");
        assert_eq!(
            norad::Font::load(&open)
                .unwrap()
                .font_info
                .style_name
                .as_deref(),
            Some("Regular")
        );
    }

    #[cfg(unix)]
    #[test]
    fn imported_output_does_not_replace_a_dangling_symlink() {
        let scratch = Scratch::new("import-symlink");
        let preferred = scratch.0.join("Imported.ufo");
        std::os::unix::fs::symlink(scratch.0.join("missing"), &preferred).unwrap();

        let destination = unused_import_destination(&preferred).unwrap();

        assert_eq!(destination, scratch.0.join("Imported-import-1.ufo"));
        assert!(
            fs::symlink_metadata(preferred)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn glyphs_output_rejects_unsafe_paths_without_publication() {
        let scratch = Scratch::new("glyphs-path");
        let source = scratch.0.join("Example.glyphs");
        let result = ConversionResult {
            family_name: "Example".into(),
            files: vec![ConvertedFile {
                path: "../outside".into(),
                text: "payload".into(),
            }],
            warnings: Vec::new(),
        };
        let error = publish_glyphs_import(&source, result).unwrap_err();
        assert!(error.contains("unsafe generated import path"), "{error}");
        assert!(!scratch.0.join("Example-ufo").exists());
        assert!(!scratch.0.join("outside").exists());
    }

    fn empty_source(destination: PathBuf) -> SourceExport {
        SourceExport {
            destination,
            font: norad::Font::default(),
            preserved: PreservedFiles::default(),
        }
    }

    fn generated_ufo() -> ConversionResult {
        let files = [
            (
                "Example-Regular.ufo/metainfo.plist",
                r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>creator</key><string>org.runebender.test</string>
  <key>formatVersion</key><integer>3</integer>
</dict></plist>
"#,
            ),
            (
                "Example-Regular.ufo/fontinfo.plist",
                r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>familyName</key><string>Example</string>
  <key>styleName</key><string>Regular</string>
</dict></plist>
"#,
            ),
            (
                "Example-Regular.ufo/layercontents.plist",
                r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><array>
  <array><string>public.default</string><string>glyphs</string></array>
</array></plist>
"#,
            ),
            (
                "Example-Regular.ufo/glyphs/contents.plist",
                r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>A</key><string>A_.glif</string></dict></plist>
"#,
            ),
            (
                "Example-Regular.ufo/glyphs/A_.glif",
                r#"<?xml version="1.0" encoding="UTF-8"?>
<glyph name="A" format="2"><advance width="600"/><unicode hex="0041"/></glyph>
"#,
            ),
        ]
        .into_iter()
        .map(|(path, text)| ConvertedFile {
            path: path.into(),
            text: text.into(),
        })
        .collect();
        ConversionResult {
            family_name: "Example".into(),
            files,
            warnings: Vec::new(),
        }
    }

    fn write_ufo(path: &Path, style: &str, extra_layers: bool) {
        fs::create_dir_all(path).unwrap();
        fs::write(
            path.join("metainfo.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>creator</key><string>com.example.original</string>
  <key>formatVersion</key><integer>3</integer>
</dict></plist>
"#,
        )
        .unwrap();
        fs::write(
            path.join("fontinfo.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>familyName</key><string>Filesystem Fixture</string>
  <key>styleName</key><string>{style}</string>
</dict></plist>
"#
            ),
        )
        .unwrap();
        let layer_contents = if extra_layers {
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><array>
  <array><string>public.default</string><string>glyphs</string></array>
  <array><string>Background</string><string>glyphs.background-custom</string></array>
  <array><string>Sketch</string><string>glyphs.sketch-custom</string></array>
</array></plist>
"#
        } else {
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><array>
  <array><string>public.default</string><string>glyphs</string></array>
</array></plist>
"#
        };
        fs::write(path.join("layercontents.plist"), layer_contents).unwrap();
        write_layer(path.join("glyphs"), "A.custom-name.glif", 500.0);
        if extra_layers {
            write_layer(
                path.join("glyphs.background-custom"),
                "A.background.glif",
                510.0,
            );
            write_layer(path.join("glyphs.sketch-custom"), "A.sketch.glif", 520.0);
            fs::write(
                path.join("lib.plist"),
                r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>com.example.unknown</key><string>preserved</string>
</dict></plist>
"#,
            )
            .unwrap();
            fs::create_dir_all(path.join("data/private")).unwrap();
            fs::write(path.join("data/private/payload.bin"), [0, 1, 2, 255]).unwrap();
            fs::create_dir(path.join("images")).unwrap();
            fs::write(
                path.join("images/reference.png"),
                include_bytes!("../../tests/fixtures/variable/reference.png"),
            )
            .unwrap();
            fs::create_dir(path.join("vendor")).unwrap();
            fs::write(path.join("vendor/opaque.bin"), b"unknown root payload").unwrap();
        }
    }

    fn write_layer(path: PathBuf, glif_name: &str, width: f64) {
        fs::create_dir(&path).unwrap();
        fs::write(
            path.join("contents.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>A</key><string>{glif_name}</string></dict></plist>
"#
            ),
        )
        .unwrap();
        fs::write(
            path.join(glif_name),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<glyph name="A" format="2">
  <advance width="{width}"/>
  <unicode hex="0041"/>
</glyph>
"#
            ),
        )
        .unwrap();
    }
}
