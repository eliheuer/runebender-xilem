// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The open variable font: glyph-local layers, axes, sources, instances and history.
//!
//! Project owns canonical glyphs through `variable`; `source` provides guarded
//! UFO compatibility projections and paint caches for existing tools.
//! Save and interpolation read canonical glyph layers. Format adapters preserve
//! source metadata and retain explicit persistence destinations.
//! No application or platform state belongs in the document model.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kurbo::BezPath;

pub use super::source::{GlyphEntry, GlyphPoint, Master, extract_anchors, extract_points};
use super::variable::{
    CanonicalSourceMetadataSnapshot, DocumentSnapshot, GlyphLayerAddress, GlyphSource, GlyphView,
    LayerId, SourceEdit, SourceId, SourceMetadataEditDraft, SourceMetadataRestoreError,
    SourcesEdit, VariableData, VariableGlyph,
};
use crate::document::var_model::{Location, VariationModel};
use crate::formats::binary_import::import_binary_font;
use crate::formats::lib_keys::{hoi_quad_at, read_hoi_intermediates};

#[path = "sources.rs"]
mod sources;

/// One designspace axis, in design coordinates.
#[derive(Debug, Clone)]
pub struct AxisInfo {
    /// User-coordinate axis and its validated mapping.
    pub user: super::axis::Axis,
    /// Axis name as written in the designspace.
    pub name: String,
    /// Four-letter OpenType axis tag.
    pub tag: Arc<str>,
    /// Minimum value in design coordinates.
    pub min: f64,
    /// Default value in design coordinates.
    pub default: f64,
    /// Maximum value in design coordinates.
    pub max: f64,
}

/// Read-only source metadata without exposing its compatibility font projection.
#[derive(Clone, Copy, Debug)]
pub struct SourceView<'a> {
    id: SourceId,
    name: &'a str,
    location: &'a Location,
    path: &'a Path,
    default_layer_name: &'a str,
}

impl<'a> SourceView<'a> {
    /// Stable identity of this source.
    pub fn id(self) -> SourceId {
        self.id
    }

    /// Display name of this source.
    pub fn name(self) -> &'a str {
        self.name
    }

    /// Normalized design location of this source.
    pub fn location(self) -> &'a Location {
        self.location
    }

    /// Persistence destination for this source.
    pub fn path(self) -> &'a Path {
        self.path
    }

    /// Stable address of this source's default layer.
    pub fn default_layer(self) -> LayerId {
        LayerId {
            source: self.id,
            name: self.default_layer_name.to_owned(),
        }
    }
}

/// Result of applying one canonical document edit draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentEditOutcome {
    /// The draft matched the current layer exactly and did not commit.
    Unchanged {
        /// Current document revision, unchanged by this operation.
        revision: u64,
    },
    /// The draft committed atomically and invalidated revision-dependent data.
    Changed {
        /// Document revision after the commit.
        revision: u64,
        /// Exact invalidation scope produced by the transaction.
        change: DocumentChange,
    },
}

/// Result of replaying one Project-owned canonical history step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentHistoryReplayOutcome {
    /// The addressed history pile had no step in the requested direction.
    Empty {
        /// Current document revision, unchanged by this operation.
        revision: u64,
    },
    /// The step committed and produced this exact invalidation scope.
    Changed {
        /// Document revision after the replay.
        revision: u64,
        /// Exact layers and metadata invalidated by the replay.
        change: DocumentChange,
    },
}

/// An owned canonical layer edit based on one guarded document snapshot.
///
/// Callers may clone and mutate the draft without borrowing Project. Commit succeeds only while
/// the addressed live layer still equals the captured base and records one Project-owned history
/// step for a real change.
#[derive(Clone, Debug)]
pub struct CanonicalLayerTransaction {
    base: super::CanonicalLayerSnapshot,
    draft: super::LayerEditDraft,
}

impl CanonicalLayerTransaction {
    /// Stable glyph-layer address captured when this transaction began.
    pub fn address(&self) -> &GlyphLayerAddress {
        self.base.address()
    }

    /// Read the owned canonical edit draft.
    pub fn draft(&self) -> &super::LayerEditDraft {
        &self.draft
    }

    /// Mutate the owned canonical edit draft.
    pub fn draft_mut(&mut self) -> &mut super::LayerEditDraft {
        &mut self.draft
    }
}

/// Why a guarded canonical layer-history replay could not commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentHistoryError {
    /// The stable glyph-layer address no longer exists.
    MissingLayer(GlyphLayerAddress),
    /// The live layer no longer equals the history entry's expected state.
    StaleLayer(GlyphLayerAddress),
    /// A snapshot captured for another glyph-layer address was supplied.
    AddressMismatch(GlyphLayerAddress),
}

/// Why a guarded whole-source metadata replay could not commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentSourceMetadataHistoryError {
    /// The live project or replacement has a different stable source set.
    SourceSetMismatch,
    /// Source metadata changed after the history entry was recorded.
    Stale,
}

impl std::fmt::Display for DocumentSourceMetadataHistoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceSetMismatch => {
                formatter.write_str("canonical source metadata has a different source set")
            }
            Self::Stale => formatter.write_str("canonical source metadata changed after capture"),
        }
    }
}

impl std::error::Error for DocumentSourceMetadataHistoryError {}

impl std::fmt::Display for DocumentHistoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingLayer(address) => write!(
                formatter,
                "glyph layer {} at {} no longer exists",
                address.glyph, address.layer.name
            ),
            Self::StaleLayer(address) => write!(
                formatter,
                "glyph layer {} at {} changed after the history entry",
                address.glyph, address.layer.name
            ),
            Self::AddressMismatch(address) => write!(
                formatter,
                "a snapshot belongs to another glyph layer than {} at {}",
                address.glyph, address.layer.name
            ),
        }
    }
}

impl std::error::Error for DocumentHistoryError {}

/// Invalidation scope produced by one committed document transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentChange {
    affected_layers: Vec<GlyphLayerAddress>,
    dependent_layers: Vec<GlyphLayerAddress>,
    source_metadata: Vec<SourceId>,
    geometry: bool,
    metrics: bool,
    metadata: bool,
    compilation: bool,
}

impl DocumentChange {
    /// Layers directly mutated by the transaction.
    pub fn affected_layers(&self) -> &[GlyphLayerAddress] {
        &self.affected_layers
    }

    /// Layers whose components reference a directly affected glyph.
    pub fn dependent_layers(&self) -> &[GlyphLayerAddress] {
        &self.dependent_layers
    }

    /// Sources whose source-wide metadata changed.
    pub fn source_metadata(&self) -> &[SourceId] {
        &self.source_metadata
    }

    /// Whether ordinary geometry or component transforms changed.
    pub fn geometry_changed(&self) -> bool {
        self.geometry
    }

    /// Whether exact horizontal or vertical metrics changed.
    pub fn metrics_changed(&self) -> bool {
        self.metrics
    }

    /// Whether layer, object or source metadata changed.
    pub fn metadata_changed(&self) -> bool {
        self.metadata
    }

    /// Whether derived compilation data must be rebuilt.
    pub fn requires_compilation(&self) -> bool {
        self.compilation
    }
}

#[derive(Debug)]
/// An open variable font with canonical glyph-local layers and source metadata.
/// UFO projections support existing tools through scoped edits.
pub struct Project {
    /// Compatibility projections, in display order; never directly mutable outside Project.
    masters: Vec<Master>,
    pub(super) variable: VariableData,
    source_history: sources::SourceHistory,
    document_history: super::history::DocumentHistory,
    source_metadata_history: super::history::SourceMetadataHistory,
    /// Index into `masters` of the master being edited.
    pub active: usize,
    /// Style names for the master switcher, one per master.
    pub master_names: Vec<Arc<str>>,
    /// The designspace axes, empty for a single UFO.
    pub axes: Vec<AxisInfo>,
    /// Normalized (-1..1) location of each master, by axis name.
    pub master_locations: Vec<Location>,
    /// Font-wide variation model; glyph interpolation uses its own participating sources.
    pub model: Option<VariationModel>,
    /// Current preview location, normalized, by axis name.
    pub location: Location,
    /// Per-glyph master point-compatibility (designspaces only).
    pub compat: HashMap<String, bool>,
    /// Original source path used to choose export and new-source destinations.
    /// Compilation reads the live document rather than this file.
    /// `None` until a new project has a home on disk.
    pub export_source: Option<PathBuf>,
    /// Named designspace instances: style name and normalized
    /// location, for the Instances rows under the axis sliders.
    pub instances: Vec<(Arc<str>, Location)>,
    /// The loaded designspace document, kept so instance edits, and
    /// later axis edits, can be written back. `None` for single-UFO
    /// projects.
    pub ds_doc: Option<norad::designspace::DesignSpaceDocument>,
    /// Source or instance edits not yet written to the Designspace file.
    pub ds_dirty: bool,
    /// Sparse "brace" sources: per-glyph intermediate masters living
    /// in a named layer of a master UFO at their own location. In
    /// the designspace these are sources with a `layer` attribute.
    pub brace: Vec<BraceSource>,
    /// Independent, session-only experimental versions of live masters.
    pub experiments: super::experiments::Experiments,
}

#[derive(Debug, Clone)]
/// One sparse intermediate source (a Glyphs brace layer).
pub struct BraceSource {
    /// Index into `masters`: the UFO holding the layer.
    pub master: usize,
    /// The UFO layer name. Glyphs writes `{500}`.
    pub layer: String,
    /// Normalized location.
    pub location: Location,
}

/// Which Glyphs form a path names, if either.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlyphsSource {
    /// A single `.glyphs` file.
    File,
    /// A `.glyphspackage` directory.
    Package,
    /// Not a Glyphs path.
    Neither,
}

/// Read a `.glyphspackage` into the entries the importer wants: paths
/// relative to the package root, so `glyphs/A.glyph` stays
/// `glyphs/A.glyph`.
pub fn read_glyphspackage(root: &Path) -> Result<HashMap<String, String>, String> {
    pub(crate) fn walk(
        dir: &Path,
        root: &Path,
        out: &mut HashMap<String, String>,
    ) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|e| format!("{e}"))? {
            let path = entry.map_err(|e| format!("{e}"))?.path();
            if path.is_dir() {
                walk(&path, root, out)?;
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                // Anything that is not UTF-8 is not part of the
                // source; skip it rather than failing the open.
                let rel = path
                    .strip_prefix(root)
                    .map_err(|e| format!("{e}"))?
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, text);
            }
        }
        Ok(())
    }
    let mut out = HashMap::new();
    walk(root, root, &mut out)?;
    if out.is_empty() {
        return Err(format!("{} is empty", root.display()));
    }
    Ok(out)
}

impl Project {
    pub(super) fn default_source_index(&self) -> usize {
        self.master_locations
            .iter()
            .position(|location| location.values().all(|value| *value == 0.0))
            .unwrap_or(0)
    }

    /// Source carrying the font-wide feature text, independent of editor selection.
    pub fn feature_source(&self) -> &Master {
        &self.masters[self.default_source_index()]
    }

    /// Apply shared feature text to the default source without rewriting other sources.
    pub fn set_feature_text(&mut self, text: String) -> bool {
        let id = self
            .source_id(self.default_source_index())
            .expect("default source identity");
        matches!(
            self.edit_document_source_metadata(id, |draft| {
                draft.set_feature_text(text);
                Ok(())
            }),
            Ok(DocumentEditOutcome::Changed { .. })
        )
    }

    /// The canonical Babelfont geometry shared by all sources and compiler snapshots.
    pub(super) fn variable_font(&self) -> &babelfont::Font {
        &self.variable.font
    }

    /// The master sitting exactly at `location`, if any. Landing on a
    /// master is a master switch, not an interpolation: the web treats
    /// it that way so the outline stays editable.
    pub fn master_at_location(&self) -> Option<usize> {
        if self.axes.is_empty() {
            return None;
        }
        self.master_locations.iter().position(|there| {
            self.axes.iter().all(|axis| {
                let a = there.get(&axis.name).copied().unwrap_or(0.0);
                let b = self.location.get(&axis.name).copied().unwrap_or(0.0);
                (a - b).abs() < 1e-6
            })
        })
    }

    /// True while the sliders sit between masters: what the canvas
    /// shows is an interpolated instance, and nothing there is
    /// editable.
    pub fn showing_instance(&self) -> bool {
        self.model.is_some() && !self.axes.is_empty() && self.master_at_location().is_none()
    }

    /// Put `location` back on a master, for a master switch.
    pub fn snap_location_to_master(&mut self, master: usize) {
        if let Some(there) = self.master_locations.get(master) {
            self.location = there.clone();
        }
    }

    /// File → New Font: one master from the GF-shaped template. The
    /// source path is where Save will write; Save As picks it.
    pub fn new_font(path: PathBuf) -> Self {
        let font = crate::document::new_font::new_font("Untitled", "Regular", 400);
        let mut model = Master::from_font(font, path);
        model.dirty = true;
        Self::from_source(model)
    }

    /// Build a project from one format-adapter source projection.
    pub fn from_source(model: Master) -> Self {
        let name = model
            .font
            .font_info
            .style_name
            .clone()
            .unwrap_or_else(|| "Regular".into());
        let mut project = Self {
            variable: VariableData::default(),
            source_history: sources::SourceHistory::default(),
            document_history: super::history::DocumentHistory::default(),
            source_metadata_history: super::history::SourceMetadataHistory::default(),
            masters: vec![model],
            active: 0,
            master_names: vec![name.into()],
            axes: Vec::new(),
            master_locations: vec![Location::new()],
            model: None,
            location: Location::new(),
            compat: HashMap::new(),
            export_source: None,
            instances: Vec::new(),
            ds_doc: None,
            ds_dirty: false,
            brace: Vec::new(),
            experiments: super::experiments::Experiments::default(),
        };
        project.variable = VariableData::from_sources(&project.masters);
        project.compute_compat();
        project
    }

    /// Opens a designspace, UFO, Glyphs source, Babelfont package, or binary font. Sets `export_source` to `path` when the loader left it unset.
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut project = Self::load_inner(path)?;
        if project.export_source.is_none() {
            project.export_source = Some(path.to_path_buf());
        }
        project.variable = VariableData::from_sources(&project.masters);
        project.compute_compat();
        Ok(project)
    }

    /// Loads a project by file type without filling in `export_source` or computing compatibility. Prefer [`Project::load`].
    fn load_inner(path: &Path) -> Result<Self, String> {
        let glyphs_ext = path.extension().and_then(|e| e.to_str()).map(|e| {
            if e.eq_ignore_ascii_case("glyphspackage") {
                GlyphsSource::Package
            } else if e.eq_ignore_ascii_case("glyphs") {
                GlyphsSource::File
            } else {
                GlyphsSource::Neither
            }
        });
        if let Some(kind @ (GlyphsSource::File | GlyphsSource::Package)) = glyphs_ext {
            // Convert the Glyphs source to UFO + designspace files in
            // a sibling directory, then open the converted project.
            let result = match kind {
                GlyphsSource::Package => {
                    let entries = read_glyphspackage(path)?;
                    crate::formats::glyphs_import::glyphs_package_to_ufo_files(&entries)?
                }
                _ => {
                    let text = std::fs::read_to_string(path).map_err(|e| format!("{e}"))?;
                    crate::formats::glyphs_import::glyphs_to_ufo_files(&text)?
                }
            };
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "glyphs-import".into());
            let out_dir = path
                .parent()
                .unwrap_or(Path::new("."))
                .join(format!("{stem}-ufo"));
            let mut designspace: Option<PathBuf> = None;
            let mut first_ufo: Option<PathBuf> = None;
            for file in &result.files {
                let target = out_dir.join(&file.path);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| format!("{e}"))?;
                }
                std::fs::write(&target, &file.text).map_err(|e| format!("{e}"))?;
                if file.path.ends_with(".designspace") {
                    designspace = Some(target);
                } else if first_ufo.is_none() && file.path.ends_with("fontinfo.plist") {
                    first_ufo = target.parent().map(|p| p.to_path_buf());
                }
            }
            let open = designspace
                .or(first_ufo)
                .ok_or_else(|| "conversion produced no font".to_string())?;
            // Export compiles the converted files, not the .glyphs.
            let mut project = Self::load_inner(&open)?;
            project.export_source = Some(open);
            return Ok(project);
        }
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("babelfont"))
        {
            return crate::formats::babelfont_import::import_project(path);
        }
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("otf"))
        {
            let font = import_binary_font(path)?;
            let name: Arc<str> = font
                .font_info
                .style_name
                .clone()
                .unwrap_or_else(|| "Regular".into())
                .into();
            let ufo_path = path.with_extension("ufo");
            let mut model = Master::from_font(font, ufo_path.clone());
            model.dirty = true;
            let mut project = Self {
                variable: VariableData::default(),
                source_history: sources::SourceHistory::default(),
                document_history: super::history::DocumentHistory::default(),
                source_metadata_history: super::history::SourceMetadataHistory::default(),
                masters: vec![model],
                active: 0,
                master_names: vec![name],
                axes: Vec::new(),
                master_locations: vec![Location::new()],
                model: None,
                location: Location::new(),
                compat: HashMap::new(),
                export_source: Some(ufo_path),
                instances: Vec::new(),
                ds_doc: None,
                ds_dirty: false,
                brace: Vec::new(),
                experiments: super::experiments::Experiments::default(),
            };
            project.variable = VariableData::from_sources(&project.masters);
            project.compute_compat();
            return Ok(project);
        }
        if path.extension().is_some_and(|e| e == "designspace") {
            let doc = crate::formats::designspace::load(path)?;
            let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
            return Self::from_designspace(doc, move |filename| {
                let ufo_path = dir.join(filename);
                Master::load(&ufo_path).map_err(|e| format!("{}: {e}", ufo_path.display()))
            });
        }
        {
            let model = Master::load(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let name: Arc<str> = model
                .font
                .font_info
                .style_name
                .clone()
                .unwrap_or_else(|| "Regular".into())
                .into();
            Ok(Self {
                variable: VariableData::default(),
                source_history: sources::SourceHistory::default(),
                document_history: super::history::DocumentHistory::default(),
                source_metadata_history: super::history::SourceMetadataHistory::default(),
                masters: vec![model],
                active: 0,
                master_names: vec![name],
                axes: Vec::new(),
                master_locations: vec![Location::new()],
                model: None,
                location: Location::new(),
                compat: HashMap::new(),
                export_source: None,
                instances: Vec::new(),
                ds_doc: None,
                ds_dirty: false,
                brace: Vec::new(),
                experiments: super::experiments::Experiments::default(),
            })
        }
    }

    /// Assemble a designspace project; `load_master` maps a source
    /// filename to its font model (filesystem or in-memory host).
    pub fn from_designspace(
        doc: norad::designspace::DesignSpaceDocument,
        mut load_master: impl FnMut(&str) -> Result<Master, String>,
    ) -> Result<Self, String> {
        if doc
            .axis_mappings
            .as_ref()
            .is_some_and(|mappings| !mappings.is_empty())
        {
            return Err("cross-axis mappings are not supported by the preview model".into());
        }
        let mut names = HashSet::new();
        let mut tags = HashSet::new();
        let mut axes = Vec::new();
        for a in &doc.axes {
            if !names.insert(a.name.clone()) || !tags.insert(a.tag.clone()) {
                return Err("designspace axes must have unique names and tags".into());
            }
            if a.values.as_ref().is_some_and(|values| !values.is_empty()) {
                return Err(format!("{}: discrete axes are not yet supported", a.name));
            }
            let mut map: Vec<_> = a
                .map
                .iter()
                .flatten()
                .map(|m| (f64::from(m.input), f64::from(m.output)))
                .collect();
            map.sort_by(|a, b| a.0.total_cmp(&b.0));
            let user = super::axis::Axis {
                name: a.name.clone(),
                tag: a.tag.clone(),
                min: f64::from(a.minimum.unwrap_or(a.default)),
                default: f64::from(a.default),
                max: f64::from(a.maximum.unwrap_or(a.default)),
                map,
            };
            user.validate()?;
            axes.push(AxisInfo {
                name: a.name.clone(),
                tag: a.tag.clone().into(),
                min: user.user_to_design(user.min),
                default: user.user_to_design(user.default),
                max: user.user_to_design(user.max),
                user,
            });
        }
        let normalize = |dimensions: &[norad::designspace::Dimension]| -> Result<Location, String> {
            let mut seen = HashSet::new();
            for dimension in dimensions {
                if !names.contains(&dimension.name) || !seen.insert(&dimension.name) {
                    return Err(format!(
                        "unknown or duplicate source axis {}",
                        dimension.name
                    ));
                }
                if dimension.yvalue.is_some()
                    || (dimension.xvalue.is_some() && dimension.uservalue.is_some())
                {
                    return Err(format!(
                        "{}: anisotropic or ambiguous source coordinates are unsupported",
                        dimension.name
                    ));
                }
                if dimension.xvalue.is_none() && dimension.uservalue.is_none() {
                    return Err(format!("{}: missing coordinate value", dimension.name));
                }
                if dimension
                    .xvalue
                    .or(dimension.uservalue)
                    .is_some_and(|value| !value.is_finite())
                {
                    return Err(format!("{}: non-finite source coordinate", dimension.name));
                }
            }
            Ok(axes
                .iter()
                .map(|axis| {
                    let dimension = dimensions.iter().find(|d| d.name == axis.name);
                    let value = if let Some(user) = dimension.and_then(|d| d.uservalue) {
                        axis.user.user_to_normalized(f64::from(user))
                    } else {
                        axis.user.design_to_normalized(
                            dimension
                                .and_then(|d| d.xvalue)
                                .map(f64::from)
                                .unwrap_or(axis.default),
                        )
                    };
                    (axis.name.clone(), value)
                })
                .collect())
        };
        for instance in &doc.instances {
            normalize(&instance.location)?;
        }
        let mut masters = Vec::new();
        let mut master_names = Vec::new();
        let mut master_locations = Vec::new();
        let mut files = Vec::new();
        for source in doc.sources.iter().filter(|s| s.layer.is_none()) {
            if files.contains(&source.filename) {
                return Err(format!(
                    "duplicate full source file {} is not editable independently",
                    source.filename
                ));
            }
            let location = normalize(&source.location)?;
            if master_locations.contains(&location) {
                return Err("duplicate full source locations are ambiguous".into());
            }
            masters.push(load_master(&source.filename)?);
            files.push(source.filename.clone());
            master_names.push(
                source
                    .stylename
                    .clone()
                    .unwrap_or_else(|| source.filename.clone())
                    .into(),
            );
            master_locations.push(location);
        }
        if masters.is_empty() {
            return Err("designspace has no full sources".into());
        }
        let default_index = master_locations
            .iter()
            .position(|loc| loc.values().all(|v| v.abs() < 1e-9))
            .ok_or("designspace has no source at the mapped default location")?;
        let mut brace = Vec::new();
        for source in doc.sources.iter().filter(|s| s.layer.is_some()) {
            let layer = source.layer.as_ref().expect("filtered layer source");
            let master = files
                .iter()
                .position(|file| file == &source.filename)
                .ok_or_else(|| {
                    format!(
                        "layer source {} requires a full source from the same UFO",
                        source.filename
                    )
                })?;
            if masters[master].font.layers.get(layer).is_none() {
                return Err(format!("{}: missing source layer {layer}", source.filename));
            }
            brace.push(BraceSource {
                master,
                layer: layer.clone(),
                location: normalize(&source.location)?,
            });
        }
        let variable = VariableData::from_sources(&masters);
        let model = if masters.len() > 1 || !brace.is_empty() {
            Some(VariationModel::new(&master_locations)?)
        } else {
            None
        };
        let location = axes.iter().map(|axis| (axis.name.clone(), 0.0)).collect();
        let mut project = Self {
            masters,
            variable,
            source_history: sources::SourceHistory::default(),
            document_history: super::history::DocumentHistory::default(),
            source_metadata_history: super::history::SourceMetadataHistory::default(),
            active: default_index,
            master_names,
            axes,
            master_locations,
            model,
            location,
            compat: HashMap::new(),
            export_source: None,
            instances: Vec::new(),
            ds_doc: Some(doc),
            ds_dirty: false,
            brace,
            experiments: super::experiments::Experiments::default(),
        };
        project.refresh_instances_from_doc();
        Ok(project)
    }

    /// Structural signature used for interpolation compatibility:
    /// per contour, the ordered list of point types.
    pub fn glyph_signature(font: &Master, name: &str) -> Option<Vec<Vec<norad::PointType>>> {
        font.font
            .get_glyph(name)
            .map(crate::document::font_ops::glyph_signature)
    }

    /// Why a glyph does not interpolate: the first master pair whose
    /// structure disagrees, with contour and point counts. None when
    /// compatible or single-master.
    pub fn compat_detail(&self, name: &str) -> Option<String> {
        let error = self.try_interpolated_at(name, &Location::new()).err()?;
        let first_sig = Self::glyph_signature(&self.masters[0], name);
        let first_name = &self.master_names[0];
        let describe = |sig: &Option<Vec<Vec<norad::PointType>>>| match sig {
            None => "missing".to_string(),
            Some(contours) => {
                let points: usize = contours.iter().map(|c| c.len()).sum();
                format!("{}c · {}pt", contours.len(), points)
            }
        };
        for (master, master_name) in self.masters.iter().zip(&self.master_names).skip(1) {
            let sig = Self::glyph_signature(master, name);
            if sig == first_sig {
                continue;
            }
            return Some(format!(
                "{first_name} {} · {master_name} {}",
                describe(&first_sig),
                describe(&sig),
            ));
        }
        Some(error)
    }

    /// Rebuild the Instances display rows (name + normalized
    /// location) from the designspace document.
    pub fn refresh_instances_from_doc(&mut self) {
        let Some(doc) = self.ds_doc.as_ref() else {
            return;
        };
        self.instances = doc
            .instances
            .iter()
            .map(|inst| {
                let name: Arc<str> = inst
                    .stylename
                    .clone()
                    .or_else(|| inst.name.clone())
                    .unwrap_or_else(|| "Instance".into())
                    .into();
                let mut location = Location::new();
                for axis in &self.axes {
                    let dimension = inst.location.iter().find(|d| d.name == axis.name);
                    let value = if let Some(user) = dimension.and_then(|d| d.uservalue) {
                        axis.user.user_to_normalized(f64::from(user))
                    } else {
                        axis.user.design_to_normalized(
                            dimension
                                .and_then(|d| d.xvalue)
                                .map(f64::from)
                                .unwrap_or(axis.default),
                        )
                    };
                    location.insert(axis.name.clone(), value);
                }
                (name, location)
            })
            .collect();
    }

    /// Check one glyph's compatibility across all masters.
    pub fn check_compat(&self, name: &str) -> bool {
        self.try_interpolated_at(name, &Location::new()).is_ok()
    }

    /// Recompute the whole compatibility map (load / reload).
    pub fn compute_compat(&mut self) {
        self.compat.clear();
        let names: Vec<String> = self.glyph_names().map(str::to_owned).collect();
        for name in names {
            let ok = self.check_compat(&name);
            self.compat.insert(name, ok);
        }
    }

    /// Recheck one glyph after editing.
    pub fn recheck_compat(&mut self, name: &str) {
        let ok = self.check_compat(name);
        self.compat.insert(name.to_string(), ok);
    }

    /// Rebuild a glyph from every source except the active master,
    /// evaluated at the active master's own location.
    ///
    /// This repairs one broken master from the others. With one
    /// other source it is a straight copy. This is Re-Interpolate in
    /// Glyphs.
    pub fn reinterpolated_from_others(&self, glyph_name: &str) -> Result<norad::Glyph, String> {
        let (layers, locations) =
            self.interpolation_layers(glyph_name, self.source_id(self.active))?;
        if layers.len() == 1 {
            return Ok(layers[0].project());
        }
        super::interpolation::interpolate_projected(
            &layers,
            &locations,
            &self.master_locations[self.active],
        )
    }

    /// The current instance's path and advance, resolving every component at that location.
    pub fn interpolated_glyph(&self, glyph_name: &str) -> Option<(BezPath, f64)> {
        let glyph = self.interpolated_norad_glyph(glyph_name)?;
        Some((
            self.interpolated_outline_at(glyph_name, &self.location)
                .ok()?,
            glyph.width,
        ))
    }

    /// Resolve an interpolated outline, failing on missing or cyclic components.
    pub fn interpolated_outline_at(
        &self,
        glyph_name: &str,
        location: &Location,
    ) -> Result<BezPath, String> {
        fn resolve(
            project: &Project,
            name: &str,
            location: &Location,
            seen: &mut HashSet<String>,
        ) -> Result<BezPath, String> {
            if seen.len() >= 64 || !seen.insert(name.to_owned()) {
                return Err(format!(
                    "{name}: cyclic or excessively deep component graph"
                ));
            }
            let glyph = project.try_interpolated_at(name, location)?;
            let mut path = crate::outline::glyph_paths::contours_to_bezpath(&glyph);
            for component in &glyph.components {
                let outline = resolve(project, &component.base, location, seen)?;
                let transform = crate::outline::glyph_paths::component_affine(&component.transform);
                path.extend((transform * outline).elements().iter().copied());
            }
            seen.remove(name);
            Ok(path)
        }
        resolve(self, glyph_name, location, &mut HashSet::new())
    }

    /// Interpolate resolved glyph-pair kerning, including each source's group fallback.
    pub fn interpolated_kerning_at(
        &self,
        left: &str,
        right: &str,
        location: &Location,
    ) -> Result<f64, String> {
        if location
            .keys()
            .any(|name| !self.axes.iter().any(|axis| &axis.name == name))
            || location.values().any(|value| !value.is_finite())
        {
            return Err("invalid kerning interpolation location".into());
        }
        let values: Vec<_> = self
            .variable
            .source_ids
            .iter()
            .map(|source| {
                vec![
                    self.document_font_metadata(*source)
                        .expect("source identity retains canonical metadata")
                        .resolved_kerning(left, right)
                        .unwrap_or(0.0),
                ]
            })
            .collect();
        if values.iter().flatten().any(|value| !value.is_finite()) {
            return Err("non-finite source kerning".into());
        }
        if values.len() == 1 {
            return Ok(values[0][0]);
        }
        let model = self
            .model
            .as_ref()
            .ok_or("project has no variation model")?;
        Ok(model.interpolate(&values, location)?[0])
    }

    /// The interpolation at the current location as a norad glyph,
    /// point structure kept: the working form for the ghost, the
    /// strip, and for freezing into a brace layer.
    pub fn interpolated_norad_glyph(&self, glyph_name: &str) -> Option<norad::Glyph> {
        if self.location.values().all(|v| v.abs() < 1e-9) {
            return None;
        }
        self.interpolated_at(glyph_name, &self.location)
    }

    /// The interpolation at an arbitrary normalized location.
    ///
    /// The default location is included, where it returns the
    /// default master's own coordinates: trajectory sampling needs
    /// the whole axis, ends included.
    pub fn interpolated_at(&self, glyph_name: &str, location: &Location) -> Option<norad::Glyph> {
        self.try_interpolated_at(glyph_name, location).ok()
    }

    /// Interpolate this glyph's sources, with explicit failure reasons.
    /// Missing glyphs in non-default sources produce a sparse per-glyph model.
    pub fn try_interpolated_at(
        &self,
        glyph_name: &str,
        location: &Location,
    ) -> Result<norad::Glyph, String> {
        if location
            .keys()
            .any(|name| !self.axes.iter().any(|axis| &axis.name == name))
        {
            return Err("interpolation references an unknown axis".into());
        }
        let (layers, locations) = self.interpolation_layers(glyph_name, None)?;
        let default = locations
            .iter()
            .position(|source| source.values().all(|value| value.abs() < 1e-9))
            .ok_or("glyph has no layer at the default location")?;
        let base = layers
            .get(default)
            .copied()
            .ok_or("missing default layer")?;
        let mut interpolated =
            super::interpolation::interpolate_layers(&layers, &locations, location)?;
        // HOI: nodes with an intermediate point follow their exact
        // quadratic, overriding the piecewise answer the baked brace
        // layers gave the model — the bake stays for compilers, the
        // preview is exact.
        if let (Some(axis), Some((lo, hi))) = (self.axes.first(), self.axis_end_masters()) {
            let curves = self.masters[lo]
                .font
                .get_glyph(glyph_name)
                .map(read_hoi_intermediates)
                .unwrap_or_default();
            if !curves.is_empty() {
                let normalized = location.get(&axis.name).copied().unwrap_or(0.0);
                let design = crate::document::var_model::denormalize_value(
                    normalized,
                    axis.min,
                    axis.default,
                    axis.max,
                );
                let t01 = ((design - axis.min) / (axis.max - axis.min)).clamp(0.0, 1.0);
                let endpoint_layer = |index| {
                    self.source_id(index)
                        .and_then(|source| self.document_source(source))
                        .and_then(|source| self.document_layer(glyph_name, &source.default_layer()))
                };
                if let (Some(a_layer), Some(b_layer)) = (endpoint_layer(lo), endpoint_layer(hi)) {
                    for (&(ci, pi), &q) in &curves {
                        let (Some(pa), Some(pb)) = (
                            a_layer
                                .contours()
                                .nth(ci)
                                .and_then(|contour| contour.points().nth(pi)),
                            b_layer
                                .contours()
                                .nth(ci)
                                .and_then(|contour| contour.points().nth(pi)),
                        ) else {
                            continue;
                        };
                        let pos = hoi_quad_at(
                            (pa.position().x, pa.position().y),
                            (pb.position().x, pb.position().y),
                            q,
                            t01,
                        );
                        if let Some(point) = interpolated.point_at_mut(ci, pi) {
                            point.position = pos.into();
                        }
                    }
                }
            }
        }
        super::interpolation::project_interpolated(&interpolated, base)
    }

    fn interpolation_layers(
        &self,
        glyph_name: &str,
        excluding: Option<SourceId>,
    ) -> Result<(Vec<super::LayerView<'_>>, Vec<Location>), String> {
        let sources = self.glyph_sources(glyph_name)?;
        let mut layers = Vec::new();
        let mut locations = Vec::new();
        for source in sources {
            if excluding == Some(source.layer.source) {
                continue;
            }
            layers.push(
                self.document_layer(glyph_name, &source.layer)
                    .expect("validated source layer"),
            );
            locations.push(source.location);
        }
        Ok((layers, locations))
    }

    /// The sources participating in one glyph, independent of active editor selection.
    /// Missing non-default layers are sparse; auxiliary layers are not sources.
    pub fn glyph_sources(&self, glyph_name: &str) -> Result<Vec<GlyphSource>, String> {
        let glyph = self
            .variable_glyph(glyph_name)
            .ok_or_else(|| format!("unknown glyph {glyph_name}"))?;
        let mut sources = Vec::new();
        for (index, source) in self.masters.iter().enumerate() {
            let id = LayerId {
                source: self.source_id(index).expect("source index"),
                name: source.font.default_layer().name().to_string(),
            };
            if glyph.has_layer(&id) {
                sources.push(GlyphSource {
                    layer: id,
                    location: self
                        .master_locations
                        .get(index)
                        .cloned()
                        .unwrap_or_default(),
                });
            }
        }
        for source in &self.brace {
            let id = LayerId {
                source: self.source_id(source.master).expect("brace source index"),
                name: source.layer.clone(),
            };
            if glyph.has_layer(&id) {
                sources.push(GlyphSource {
                    layer: id,
                    location: source.location.clone(),
                });
            }
        }
        Ok(sources)
    }

    /// The masters at the low and high end of the first axis (by
    /// normalized location), for HOI endpoints.
    pub fn axis_end_masters(&self) -> Option<(usize, usize)> {
        let axis = self.axes.first()?;
        if self.masters.len() < 2 {
            return None;
        }
        let value = |i: usize| {
            self.master_locations
                .get(i)
                .and_then(|l| l.get(&axis.name).copied())
                .unwrap_or(0.0)
        };
        let lo = (0..self.masters.len()).min_by(|&a, &b| value(a).total_cmp(&value(b)))?;
        let hi = (0..self.masters.len()).max_by(|&a, &b| value(a).total_cmp(&value(b)))?;
        (lo != hi).then_some((lo, hi))
    }

    /// Sample every point's position at `steps + 1` equal stops
    /// along the first axis, min to max.
    ///
    /// Sampling goes through the same per-glyph model the ghost
    /// uses, so brace layers bend the trajectories. The outer index
    /// is the point, in flattened contour order; the inner is the
    /// stop.
    pub fn trajectory_samples(
        &self,
        glyph_name: &str,
        steps: usize,
    ) -> Option<Vec<Vec<kurbo::Point>>> {
        self.model.as_ref()?;
        let axis = self.axes.first()?;
        let mut per_point: Vec<Vec<kurbo::Point>> = Vec::new();
        for step in 0..=steps {
            let t = step as f64 / steps as f64;
            let design = axis.min + (axis.max - axis.min) * t;
            let mut location = self.location.clone();
            location.insert(
                axis.name.clone(),
                crate::document::var_model::normalize_value(
                    design,
                    axis.min,
                    axis.default,
                    axis.max,
                ),
            );
            let glyph = self.interpolated_at(glyph_name, &location)?;
            let mut flat = Vec::new();
            for contour in &glyph.contours {
                for p in &contour.points {
                    flat.push(kurbo::Point::new(p.x, p.y));
                }
            }
            if per_point.is_empty() {
                per_point = flat.into_iter().map(|p| vec![p]).collect();
            } else {
                if flat.len() != per_point.len() {
                    return None;
                }
                for (track, p) in per_point.iter_mut().zip(flat) {
                    track.push(p);
                }
            }
        }
        Some(per_point)
    }

    /// The glyph a designspace rule shows at the current preview
    /// location, if any: bracket layers and shape switches.
    ///
    /// A rule applies when every condition of any condition set
    /// holds; an empty condition set always holds.
    pub fn rule_substitute(&self, glyph_name: &str) -> Option<String> {
        let doc = self.ds_doc.as_ref()?;
        // Current location in design coordinates.
        let design: HashMap<&str, f64> = self
            .axes
            .iter()
            .map(|axis| {
                let normalized = self.location.get(&axis.name).copied().unwrap_or(0.0);
                (
                    axis.name.as_str(),
                    crate::document::var_model::denormalize_value(
                        normalized,
                        axis.min,
                        axis.default,
                        axis.max,
                    ),
                )
            })
            .collect();
        let mut result = glyph_name.to_owned();
        for rule in &doc.rules.rules {
            let applies = rule.condition_sets.iter().any(|set| {
                set.conditions.iter().all(|c| {
                    let Some(&value) = design.get(c.name.as_str()) else {
                        return false;
                    };
                    c.minimum.is_none_or(|min| value >= f64::from(min))
                        && c.maximum.is_none_or(|max| value <= f64::from(max))
                })
            });
            if applies
                && let Some(sub) = rule
                    .substitutions
                    .iter()
                    .find(|sub| sub.name.as_str() == result)
            {
                result = sub.with.to_string();
            }
        }
        (result != glyph_name).then_some(result)
    }

    /// The master being edited.
    pub fn active_font(&self) -> &Master {
        &self.masters[self.active]
    }

    /// The master being edited, mutably.
    pub fn active_font_mut(&mut self) -> SourceEdit<'_> {
        self.edit_source(self.source_id(self.active).expect("active source identity"))
            .expect("active source exists")
    }

    /// Read-only source projections for rendering and legacy outline algorithms.
    pub fn sources(&self) -> &[Master] {
        &self.masters
    }

    /// Stable identity of the source at a display index.
    pub fn source_id(&self, index: usize) -> Option<SourceId> {
        self.variable.source_ids.get(index).copied()
    }

    /// Current display index of a stable source identity.
    pub fn source_index(&self, id: SourceId) -> Option<usize> {
        self.variable
            .source_ids
            .iter()
            .position(|candidate| *candidate == id)
    }

    /// Edit one source projection and reconcile its changes into glyph-local layers.
    pub fn edit_source(&mut self, id: SourceId) -> Option<SourceEdit<'_>> {
        let index = self.source_index(id)?;
        Some(SourceEdit {
            source: self.masters.get_mut(index)?,
            data: &mut self.variable,
            id,
        })
    }

    /// Edit multiple source projections in one scope.
    pub fn edit_sources(&mut self) -> SourcesEdit<'_> {
        SourcesEdit {
            sources: &mut self.masters,
            data: &mut self.variable,
        }
    }

    pub(crate) fn editing_parts(
        &mut self,
    ) -> (SourcesEdit<'_>, &mut super::experiments::Experiments) {
        (
            SourcesEdit {
                sources: &mut self.masters,
                data: &mut self.variable,
            },
            &mut self.experiments,
        )
    }

    /// The variable glyph, independent of the active source or preview location.
    pub fn variable_glyph(&self, name: &str) -> Option<&VariableGlyph> {
        self.variable.glyphs.get(name)
    }

    /// Read one glyph and its canonical layers without constructing UFO values.
    pub fn document_glyph(&self, name: &str) -> Option<GlyphView<'_>> {
        self.variable.glyph_view(name)
    }

    /// Read one canonical layer without constructing a UFO glyph.
    pub fn document_layer(&self, name: &str, layer: &LayerId) -> Option<super::LayerView<'_>> {
        self.variable.layer_view(name, layer)
    }

    /// Read one source's stable identity and metadata without its UFO projection.
    pub fn document_source(&self, id: SourceId) -> Option<SourceView<'_>> {
        let index = self.source_index(id)?;
        let source = self.masters.get(index)?;
        Some(SourceView {
            id,
            name: self.master_names.get(index)?.as_ref(),
            location: self.master_locations.get(index)?,
            path: &source.source_path,
            default_layer_name: source.font.default_layer().name().as_str(),
        })
    }

    /// Read every source in current display order without its UFO projection.
    pub fn document_sources(&self) -> impl DoubleEndedIterator<Item = SourceView<'_>> {
        self.variable
            .source_ids
            .iter()
            .copied()
            .filter_map(|id| self.document_source(id))
    }

    /// Read one source's canonical OpenType feature text.
    pub fn document_feature_text(&self, source: SourceId) -> Option<&str> {
        self.variable.feature_text(source)
    }

    /// Read one source's canonical group and kerning metadata.
    pub fn document_font_metadata(
        &self,
        source: SourceId,
    ) -> Option<&super::canonical_metadata::CanonicalFontMetadata> {
        self.variable.font_metadata(source)
    }

    /// Current canonical document revision used by derived compiler data.
    pub fn document_revision(&self) -> u64 {
        self.variable.revision
    }

    /// Clone the canonical editing state without UFO templates or Master projections.
    pub fn document_snapshot(&self) -> DocumentSnapshot {
        self.variable.snapshot()
    }

    /// Begin an owned canonical layer transaction without borrowing the project.
    pub fn begin_document_layer_transaction(
        &self,
        address: &GlyphLayerAddress,
    ) -> Result<CanonicalLayerTransaction, DocumentHistoryError> {
        let base = self
            .capture_document_layer(address)
            .ok_or_else(|| DocumentHistoryError::MissingLayer(address.clone()))?;
        let (layer, preserved) = base.clone().into_parts();
        Ok(CanonicalLayerTransaction {
            base,
            draft: super::LayerEditDraft::new(layer, preserved),
        })
    }

    /// Commit an owned canonical layer transaction and record one Project-owned history step.
    ///
    /// A stale base or address mismatch leaves the document and its history unchanged. An
    /// unchanged draft records no step and does not advance the document revision.
    pub fn commit_document_layer_transaction(
        &mut self,
        transaction: CanonicalLayerTransaction,
    ) -> Result<DocumentEditOutcome, DocumentHistoryError> {
        let address = transaction.base.address().clone();
        let before = transaction.base;
        let (layer, preserved) = transaction.draft.into_parts();
        let replacement = super::CanonicalLayerSnapshot::new(address.clone(), layer, preserved);
        let outcome = self.restore_document_layer_if_current(&address, &before, replacement)?;
        if matches!(outcome, DocumentEditOutcome::Changed { .. }) {
            let recorded = self.record_document_layer_history(&address, before)?;
            debug_assert!(
                recorded,
                "a changed transaction must record one history step"
            );
        }
        Ok(outcome)
    }

    /// Capture canonical metadata for the complete current source set.
    ///
    /// Stable source identities make the snapshot independent of display order.
    pub fn capture_document_source_metadata(&self) -> CanonicalSourceMetadataSnapshot {
        self.variable.source_metadata_snapshot()
    }

    /// Capture the complete canonical source-metadata set before a history transaction.
    pub fn begin_document_source_metadata_history(&self) -> CanonicalSourceMetadataSnapshot {
        super::history::SourceMetadataHistory::capture(self)
    }

    /// Record the live source-metadata set after a completed transaction.
    pub fn record_document_source_metadata_history(
        &mut self,
        before: CanonicalSourceMetadataSnapshot,
    ) -> bool {
        let mut history = std::mem::take(&mut self.source_metadata_history);
        let recorded = history.record_completed(self, before);
        self.source_metadata_history = history;
        recorded
    }

    /// Extend the newest source-metadata history transaction with the live state.
    pub fn coalesce_document_source_metadata_history(
        &mut self,
        previous: &CanonicalSourceMetadataSnapshot,
    ) -> bool {
        let mut history = std::mem::take(&mut self.source_metadata_history);
        let coalesced = history.coalesce_completed(self, previous);
        self.source_metadata_history = history;
        coalesced
    }

    /// Drop the newest source-metadata undo transaction.
    pub fn discard_document_source_metadata_history(&mut self) -> bool {
        self.source_metadata_history.discard_last()
    }

    /// Whether Project-owned source-metadata history can replay in `direction`.
    pub fn can_replay_document_source_metadata_history(
        &self,
        direction: super::history::HistoryDirection,
    ) -> bool {
        self.source_metadata_history.can_replay(direction)
    }

    /// Number of Project-owned source-metadata steps available in `direction`.
    pub fn document_source_metadata_history_depth(
        &self,
        direction: super::history::HistoryDirection,
    ) -> usize {
        self.source_metadata_history.depth(direction)
    }

    /// Replay one Project-owned whole-source metadata transaction.
    pub fn replay_document_source_metadata_history(
        &mut self,
        direction: super::history::HistoryDirection,
    ) -> Result<
        DocumentHistoryReplayOutcome,
        super::history::HistoryReplayError<DocumentSourceMetadataHistoryError>,
    > {
        let before = self.capture_document_source_metadata();
        let mut history = std::mem::take(&mut self.source_metadata_history);
        let replayed = history.replay(self, direction);
        self.source_metadata_history = history;
        match replayed? {
            super::history::HistoryReplayOutcome::Empty => {
                Ok(DocumentHistoryReplayOutcome::Empty {
                    revision: self.variable.revision,
                })
            }
            super::history::HistoryReplayOutcome::Applied => {
                let after = self.capture_document_source_metadata();
                let affected = before.changed_sources(&after);
                Ok(DocumentHistoryReplayOutcome::Changed {
                    revision: self.variable.revision,
                    change: DocumentChange {
                        affected_layers: Vec::new(),
                        dependent_layers: Vec::new(),
                        source_metadata: affected,
                        geometry: false,
                        metrics: false,
                        metadata: true,
                        compilation: true,
                    },
                })
            }
        }
    }

    /// Forget all Project-owned source-metadata history.
    pub fn clear_document_source_metadata_history(&mut self) {
        self.source_metadata_history.clear();
    }

    /// Restore all canonical source metadata only when the live snapshot still matches.
    ///
    /// Source-set mismatches and stale values leave the document, revision and compatibility
    /// projections unchanged. A changed replacement commits once and refreshes every affected
    /// source projection.
    pub fn restore_document_source_metadata_if_current(
        &mut self,
        expected: &CanonicalSourceMetadataSnapshot,
        replacement: CanonicalSourceMetadataSnapshot,
    ) -> Result<DocumentEditOutcome, DocumentSourceMetadataHistoryError> {
        let affected = self
            .variable
            .restore_source_metadata_if_current(expected, replacement)
            .map_err(|error| match error {
                SourceMetadataRestoreError::SourceSetMismatch => {
                    DocumentSourceMetadataHistoryError::SourceSetMismatch
                }
                SourceMetadataRestoreError::Stale => DocumentSourceMetadataHistoryError::Stale,
            })?;
        if affected.is_empty() {
            return Ok(DocumentEditOutcome::Unchanged {
                revision: self.variable.revision,
            });
        }
        for source in &affected {
            self.synchronize_compatibility_source_metadata(*source);
        }
        Ok(DocumentEditOutcome::Changed {
            revision: self.variable.revision,
            change: DocumentChange {
                affected_layers: Vec::new(),
                dependent_layers: Vec::new(),
                source_metadata: affected,
                geometry: false,
                metrics: false,
                metadata: true,
                compilation: true,
            },
        })
    }

    /// Capture one complete canonical layer for undo, redo or guarded replacement.
    ///
    /// The opaque snapshot contains Babelfont geometry and exact preservation extensions without
    /// constructing a UFO glyph.
    pub fn capture_document_layer(
        &self,
        address: &GlyphLayerAddress,
    ) -> Option<super::CanonicalLayerSnapshot> {
        self.variable.layer_snapshot(address)
    }

    /// Capture one canonical layer before a Project-owned history transaction.
    pub fn begin_document_layer_history(
        &self,
        address: &GlyphLayerAddress,
    ) -> Result<super::CanonicalLayerSnapshot, DocumentHistoryError> {
        super::history::DocumentHistory::capture(self, address)
    }

    /// Record the live layer after a completed Project-owned history transaction.
    pub fn record_document_layer_history(
        &mut self,
        address: &GlyphLayerAddress,
        before: super::CanonicalLayerSnapshot,
    ) -> Result<bool, DocumentHistoryError> {
        let mut history = std::mem::take(&mut self.document_history);
        let recorded = history.record_completed(self, address, before);
        self.document_history = history;
        recorded
    }

    /// Extend the newest history step with the live result of a repeated gesture edit.
    pub fn coalesce_document_layer_history(
        &mut self,
        address: &GlyphLayerAddress,
        previous: &super::CanonicalLayerSnapshot,
    ) -> Result<bool, DocumentHistoryError> {
        let mut history = std::mem::take(&mut self.document_history);
        let coalesced = history.coalesce_completed(self, address, previous);
        self.document_history = history;
        coalesced
    }

    /// Drop the newest undo step for an operation that produced no usable edit.
    pub fn discard_document_layer_history(&mut self, address: &GlyphLayerAddress) -> bool {
        self.document_history.discard_last(address)
    }

    /// Whether one addressed Project-owned layer pile can replay in `direction`.
    pub fn can_replay_document_layer_history(
        &self,
        address: &GlyphLayerAddress,
        direction: super::history::HistoryDirection,
    ) -> bool {
        self.document_history.can_replay(address, direction)
    }

    /// Number of Project-owned layer steps available in `direction`.
    pub fn document_layer_history_depth(
        &self,
        address: &GlyphLayerAddress,
        direction: super::history::HistoryDirection,
    ) -> usize {
        self.document_history.depth(address, direction)
    }

    /// Replay one Project-owned canonical layer step with its exact invalidation scope.
    pub fn replay_document_layer_history(
        &mut self,
        address: &GlyphLayerAddress,
        direction: super::history::HistoryDirection,
    ) -> Result<
        DocumentHistoryReplayOutcome,
        super::history::HistoryReplayError<DocumentHistoryError>,
    > {
        let before = self.capture_document_layer(address).ok_or_else(|| {
            super::history::HistoryReplayError::Apply(DocumentHistoryError::MissingLayer(
                address.clone(),
            ))
        })?;
        let mut history = std::mem::take(&mut self.document_history);
        let replayed = history.replay(self, address, direction);
        self.document_history = history;
        match replayed? {
            super::history::HistoryReplayOutcome::Empty => {
                Ok(DocumentHistoryReplayOutcome::Empty {
                    revision: self.variable.revision,
                })
            }
            super::history::HistoryReplayOutcome::Applied => {
                let after = self.capture_document_layer(address).ok_or_else(|| {
                    super::history::HistoryReplayError::Apply(DocumentHistoryError::MissingLayer(
                        address.clone(),
                    ))
                })?;
                let delta = after.delta_from(&before).ok_or_else(|| {
                    super::history::HistoryReplayError::Apply(
                        DocumentHistoryError::AddressMismatch(address.clone()),
                    )
                })?;
                Ok(DocumentHistoryReplayOutcome::Changed {
                    revision: self.variable.revision,
                    change: DocumentChange {
                        affected_layers: vec![address.clone()],
                        dependent_layers: self.variable.dependent_component_layers(&address.glyph),
                        source_metadata: Vec::new(),
                        geometry: delta.geometry,
                        metrics: delta.metrics,
                        metadata: delta.metadata,
                        compilation: true,
                    },
                })
            }
        }
    }

    /// Move all Project-owned layer histories during one committed glyph rename.
    pub fn rename_document_layer_history_glyph(&mut self, old: &str, new: &str) -> bool {
        self.document_history.rename_glyph(old, new)
    }

    /// Forget every Project-owned layer history for a permanently removed glyph.
    pub fn clear_document_layer_history_glyph(&mut self, name: &str) {
        self.document_history.clear_glyph(name);
    }

    /// Forget one Project-owned layer history pile after permanent layer removal.
    pub fn clear_document_layer_history(&mut self, address: &GlyphLayerAddress) {
        self.document_history.clear_layer(address);
    }

    /// Restore a canonical layer only when its live state still equals `expected`.
    ///
    /// Missing, stale and address-mismatched snapshots leave document contents, revisions,
    /// compatibility projections and history unchanged. A changed restore advances the canonical
    /// revision once and refreshes the transitional projection and invalidation scope.
    pub fn restore_document_layer_if_current(
        &mut self,
        address: &GlyphLayerAddress,
        expected: &super::CanonicalLayerSnapshot,
        replacement: super::CanonicalLayerSnapshot,
    ) -> Result<DocumentEditOutcome, DocumentHistoryError> {
        let current = self
            .variable
            .layer_snapshot(address)
            .ok_or_else(|| DocumentHistoryError::MissingLayer(address.clone()))?;
        if expected.address() != address || replacement.address() != address {
            return Err(DocumentHistoryError::AddressMismatch(address.clone()));
        }
        if &current != expected {
            return Err(DocumentHistoryError::StaleLayer(address.clone()));
        }
        let (layer, preserved) = replacement.into_parts();
        let draft = super::LayerEditDraft::new(layer, preserved);
        let Some(delta) = self
            .variable
            .commit_layer_edit(&address.glyph, &address.layer, draft)
        else {
            return Ok(DocumentEditOutcome::Unchanged {
                revision: self.variable.revision,
            });
        };
        let change = DocumentChange {
            affected_layers: vec![address.clone()],
            dependent_layers: self.variable.dependent_component_layers(&address.glyph),
            source_metadata: Vec::new(),
            geometry: delta.geometry,
            metrics: delta.metrics,
            metadata: delta.metadata,
            compilation: true,
        };
        self.synchronize_compatibility_layer(&address.glyph, &address.layer);
        Ok(DocumentEditOutcome::Changed {
            revision: self.variable.revision,
            change,
        })
    }

    /// Apply an owned canonical layer draft atomically.
    ///
    /// Returning an error from `edit` discards the draft.
    /// An unchanged draft does not advance the document revision or update compatibility data.
    pub fn edit_document_layer(
        &mut self,
        name: &str,
        layer: &LayerId,
        edit: impl FnOnce(&mut super::LayerEditDraft) -> Result<(), super::DocumentEditError>,
    ) -> Result<DocumentEditOutcome, super::DocumentEditError> {
        let mut draft = self
            .variable
            .layer_edit_draft(name, layer)
            .ok_or(super::DocumentEditError::MissingLayer)?;
        edit(&mut draft)?;
        let Some(delta) = self.variable.commit_layer_edit(name, layer, draft) else {
            return Ok(DocumentEditOutcome::Unchanged {
                revision: self.variable.revision,
            });
        };
        let change = DocumentChange {
            affected_layers: vec![GlyphLayerAddress {
                glyph: name.to_owned(),
                layer: layer.clone(),
            }],
            dependent_layers: self.variable.dependent_component_layers(name),
            source_metadata: Vec::new(),
            geometry: delta.geometry,
            metrics: delta.metrics,
            metadata: delta.metadata,
            compilation: true,
        };
        self.synchronize_compatibility_layer(name, layer);
        Ok(DocumentEditOutcome::Changed {
            revision: self.variable.revision,
            change,
        })
    }

    /// Apply an owned source-metadata draft atomically.
    ///
    /// Returning an error from `edit` discards the draft.
    /// An unchanged draft does not advance the document revision or update compatibility data.
    pub fn edit_document_source_metadata(
        &mut self,
        source: SourceId,
        edit: impl FnOnce(&mut SourceMetadataEditDraft) -> Result<(), super::DocumentEditError>,
    ) -> Result<DocumentEditOutcome, super::DocumentEditError> {
        let mut draft = self
            .variable
            .source_metadata_edit_draft(source)
            .ok_or(super::DocumentEditError::MissingSource)?;
        edit(&mut draft)?;
        if !self.variable.commit_source_metadata_edit(source, draft) {
            return Ok(DocumentEditOutcome::Unchanged {
                revision: self.variable.revision,
            });
        }
        self.synchronize_compatibility_source_metadata(source);
        Ok(DocumentEditOutcome::Changed {
            revision: self.variable.revision,
            change: DocumentChange {
                affected_layers: Vec::new(),
                dependent_layers: Vec::new(),
                source_metadata: vec![source],
                geometry: false,
                metrics: false,
                metadata: true,
                compilation: true,
            },
        })
    }

    fn synchronize_compatibility_source_metadata(&mut self, source: SourceId) {
        let index = self
            .source_index(source)
            .expect("canonical source metadata retains its source");
        let feature_text = self
            .variable
            .feature_text(source)
            .expect("committed metadata")
            .to_owned();
        let font_metadata = self
            .variable
            .font_metadata(source)
            .expect("committed metadata")
            .clone();
        let source = &mut self.masters[index];
        let kerning_changed = match super::font_ops::canonical_metadata_from_ufo(&source.font) {
            Ok(current) => current != font_metadata,
            Err(_) => true,
        };
        source.font.features = feature_text;
        super::font_ops::write_canonical_metadata_to_ufo(&mut source.font, &font_metadata)
            .expect("canonical source metadata must remain writable as UFO");
        source.dirty = true;
        source.kerning_dirty |= kerning_changed;
    }

    fn synchronize_compatibility_layer(&mut self, name: &str, layer: &LayerId) {
        let payload = self
            .variable
            .project_layer(name, layer)
            .expect("committed layer remains projectable");
        let index = self
            .source_index(layer.source)
            .expect("committed layer retains its source");
        let source = &mut self.masters[index];
        source
            .font
            .layers
            .get_mut(&layer.name)
            .expect("committed layer retains its compatibility projection")
            .insert_glyph(payload);
        source.dirty = true;
        source.modified_glyphs.insert(name.to_owned());
        if let Some(&index) = source.name_map.get(name) {
            source.rebuild_entry(index);
        }
        self.recheck_compat(name);
    }

    /// Materialize one glyph layer for a format boundary or transitional caller.
    pub fn glyph_layer(&self, name: &str, layer: &LayerId) -> Option<norad::Glyph> {
        self.variable.project_layer(name, layer)
    }

    /// All glyph names, including glyphs found only in sparse or auxiliary layers.
    pub fn glyph_names(&self) -> impl Iterator<Item = &str> {
        self.variable.glyphs.keys().map(String::as_str)
    }

    /// Materialize a source for a format adapter from canonical glyph/layer storage.
    pub fn source_snapshot(&self, source: SourceId) -> Option<norad::Font> {
        self.variable.source_font(source)
    }

    /// Edit a particular glyph layer without changing the active editor source.
    /// Returns false for a missing layer or an unchanged payload.
    pub fn edit_layer(
        &mut self,
        name: &str,
        layer: &LayerId,
        edit: impl FnOnce(&mut norad::Glyph),
    ) -> bool {
        let Some(before) = self.glyph_layer(name, layer) else {
            return false;
        };
        let mut after = before.clone();
        edit(&mut after);
        if after == before || after.name() != before.name() {
            return false;
        }
        let index = self
            .source_index(layer.source)
            .expect("layer source identity");
        let default_layer = self.masters[index].font.default_layer().name().as_str() == layer.name;
        if default_layer {
            self.masters[index].history.record(name, &before);
        } else {
            self.variable
                .histories
                .entry(layer.clone())
                .or_default()
                .record(name, &before);
        }
        self.install_layer_payload(name, layer, after);
        true
    }

    fn install_layer_payload(&mut self, name: &str, layer: &LayerId, payload: norad::Glyph) {
        {
            let mut source = self.edit_source(layer.source).expect("validated source");
            source
                .font
                .layers
                .get_mut(&layer.name)
                .expect("validated layer")
                .insert_glyph(payload);
            source.dirty = true;
            source.modified_glyphs.insert(name.to_owned());
            if let Some(&index) = source.name_map.get(name) {
                source.rebuild_entry(index);
            }
        }
        self.recheck_compat(name);
    }

    /// Replay the history belonging to a glyph layer, without switching editor sources.
    /// Set `redo` to replay a previously undone edit.
    pub fn undo_layer(&mut self, name: &str, layer: &LayerId, redo: bool) -> bool {
        let Some(mut glyph) = self.glyph_layer(name, layer) else {
            return false;
        };
        let index = self
            .source_index(layer.source)
            .expect("layer source identity");
        let default_layer = self.masters[index].font.default_layer().name().as_str() == layer.name;
        let history = if default_layer {
            &mut self.masters[index].history
        } else {
            self.variable.histories.entry(layer.clone()).or_default()
        };
        let changed = if redo {
            history.redo(name, &mut glyph)
        } else {
            history.undo(name, &mut glyph)
        };
        if changed {
            self.install_layer_payload(name, layer, glyph);
        }
        changed
    }

    /// Save every source from the variable project, then its Designspace metadata.
    pub fn save(&mut self) -> Result<(), String> {
        for index in 0..self.masters.len() {
            let font = self
                .source_snapshot(self.source_id(index).expect("source identity"))
                .ok_or("missing source data")?;
            let source = &mut self.masters[index];
            if let Some(parent) = source
                .source_path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("{}: {error}", parent.display()))?;
            }
            font.save(&source.source_path)
                .map_err(|error| format!("{}: {error}", source.source_path.display()))?;
            source.dirty = false;
            source.modified_glyphs.clear();
            source.kerning_dirty = false;
        }
        if self.ds_dirty {
            let path = self
                .export_source
                .as_deref()
                .ok_or("designspace has no save destination")?;
            self.ds_doc
                .as_ref()
                .ok_or("designspace document is unavailable")?
                .save(path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            self.ds_dirty = false;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::measure::joining_band;
    use crate::formats::lib_keys::write_hoi_intermediates;
    use crate::formats::metrics_keys::{read_metrics_key, write_metrics_key};
    use crate::testing::fonts;

    #[test]
    fn designspace_loads_with_masters() {
        let project = Project::load(&fonts::designspace()).expect("designspace loads");
        assert_eq!(project.sources().len(), 2, "regular + bold");
        assert!(project.master_names.iter().any(|n| n.contains("Bold")));
        // Active master is the default location (Regular).
        assert!(!project.master_names[project.active].contains("Bold"));
        // Named instances come along, normalized: the extremes sit on
        // the axis ends.
        assert_eq!(project.instances.len(), 4, "four named instances");
        let bold = project
            .instances
            .iter()
            .find(|(name, _)| name.as_ref() == "Bold")
            .expect("a Bold instance");
        let weight = bold.1.values().next().copied().unwrap_or(0.0);
        assert!((weight - 1.0).abs() < 1e-6, "Bold sits at the axis max");
    }

    #[test]
    fn designspace_roundtrip_and_instance_edit() {
        // The saved document must equal the loaded one: instance
        // editing rewrites the whole file, so nothing may be lost.
        let path = fonts::designspace();
        let doc = norad::designspace::DesignSpaceDocument::load(&path).expect("designspace loads");
        let tmp = std::env::temp_dir().join("rb-ds-roundtrip.designspace");
        doc.save(&tmp).expect("designspace saves");
        let doc2 =
            norad::designspace::DesignSpaceDocument::load(&tmp).expect("saved designspace loads");
        assert_eq!(doc, doc2, "designspace round-trips losslessly");
        std::fs::remove_file(&tmp).ok();

        // Upsert against the project: renaming at an existing
        // location, adding at a fresh one, deleting.
        let mut project = Project::load(&path).expect("designspace loads");
        let before = project.instances.len();
        let doc = project.ds_doc.as_mut().expect("designspace doc kept");
        doc.instances.remove(0);
        project.ds_dirty = true;
        project.refresh_instances_from_doc();
        assert_eq!(project.instances.len(), before - 1);
    }

    #[test]
    fn brace_layer_refines_interpolation() {
        let mut project = Project::load(&fonts::designspace()).expect("loads");
        // Freeze n's Regular outline into a {500} brace layer, then
        // nudge its first point +40: at wght 500 the interpolation
        // must hit the brace exactly, not the linear blend.
        let name = "n";
        let loc_500 = {
            let axis = &project.axes[0];
            let mut l = Location::new();
            l.insert(
                axis.name.clone(),
                crate::document::var_model::normalize_value(
                    500.0,
                    axis.min,
                    axis.default,
                    axis.max,
                ),
            );
            l
        };
        let mut frozen = project.sources()[0]
            .font
            .get_glyph(name)
            .expect("has n")
            .clone();
        let orig = frozen.contours[0].points[0].x;
        frozen.contours[0].points[0].x = orig + 40.0;
        project.edit_sources()[0]
            .font
            .layers
            .get_or_create_layer("{500}")
            .unwrap()
            .insert_glyph(frozen);
        project.brace.push(BraceSource {
            master: 0,
            layer: "{500}".into(),
            location: loc_500.clone(),
        });
        project.location = loc_500;
        let refined = project
            .interpolated_norad_glyph(name)
            .expect("interpolates");
        assert!(
            (refined.contours[0].points[0].x - (orig + 40.0)).abs() < 0.6,
            "brace layer pins the outline at its location: {} vs {}",
            refined.contours[0].points[0].x,
            orig + 40.0,
        );
    }

    #[test]
    fn reinterpolate_rebuilds_a_master_from_the_others() {
        let mut project = Project::load(&fonts::designspace()).expect("loads");
        // Two masters: rebuilding the active one from "the others"
        // must reproduce the other master exactly.
        assert_eq!(project.sources().len(), 2);
        project.active = 0;
        let expected = project.sources()[1]
            .font
            .get_glyph("H")
            .expect("bold has H")
            .clone();
        let rebuilt = project
            .reinterpolated_from_others("H")
            .expect("reinterpolates");
        assert_eq!(rebuilt.width, expected.width);
        assert_eq!(rebuilt.contours.len(), expected.contours.len());
        for (a, b) in rebuilt.contours.iter().zip(expected.contours.iter()) {
            for (pa, pb) in a.points.iter().zip(b.points.iter()) {
                assert!((pa.x - pb.x).abs() < 1e-6);
                assert!((pa.y - pb.y).abs() < 1e-6);
            }
        }
        // A glyph missing everywhere else reports, not panics.
        assert!(project.reinterpolated_from_others("no.such.glyph").is_err());
    }

    #[test]
    fn joining_bands_measure_the_connecting_stroke() {
        use norad::{Contour, ContourPoint, PointType};
        let stroke = Contour::new(
            [(0.0, 40.0), (200.0, 40.0), (200.0, 120.0), (0.0, 120.0)]
                .iter()
                .map(|&(x, y)| ContourPoint::new(x, y, PointType::Line, false, None, None))
                .collect(),
            None,
        );
        let mut glyph = norad::Glyph::new("joined");
        glyph.contours = vec![stroke];
        let path = crate::outline::glyph_paths::contour_to_bezpath(&glyph.contours[0]);
        assert_eq!(joining_band(&path, 200.0, true, 2.0), Some((40.0, 120.0)));
        assert_eq!(joining_band(&path, 200.0, false, 2.0), Some((40.0, 120.0)));
        // Pull the ink off the edge: no band.
        for p in glyph.contours[0].points.iter_mut() {
            p.x += 10.0;
        }
        let moved = crate::outline::glyph_paths::contour_to_bezpath(&glyph.contours[0]);
        assert_eq!(joining_band(&moved, 200.0, true, 2.0), None);

        // And the real Arabic set: a medial beh (a composite —
        // components must resolve) touches both edges.
        let project = Project::load(&fonts::designspace()).expect("loads");
        let font = project.active_font();
        if let Some(g) = font.font.get_glyph("beh-ar.medi") {
            let i = font.name_map["beh-ar.medi"];
            let advance = font.glyphs[i].advance;
            let outline = crate::outline::glyph_paths::glyph_to_bezpath(g, &font.font);
            assert!(
                joining_band(&outline, advance, true, 2.0).is_some(),
                "medial joins left"
            );
            assert!(
                joining_band(&outline, advance, false, 2.0).is_some(),
                "medial joins right"
            );
        }
    }

    #[test]
    fn metrics_keys_sync_roundtrip() {
        // n's LSB copied onto h in both masters through the lib key.
        let mut project = Project::load(&fonts::designspace()).expect("loads");
        for master in project.edit_sources().iter_mut() {
            let glyph = master.font.get_glyph_mut("h").expect("has h");
            write_metrics_key(glyph, true, "=n+10");
        }
        // Emulate command_sync_metrics' inner pass directly.
        for master in project.edit_sources().iter_mut() {
            let n = master.name_map["n"];
            let h = master.name_map["h"];
            let target = master.ink_bounds(n).unwrap().x0 + 10.0;
            let delta = (target - master.ink_bounds(h).unwrap().x0).round();
            master.shift_ink(h, delta);
            let lsb = master.ink_bounds(h).unwrap().x0;
            assert!(
                (lsb - target).abs() < 1.0,
                "h LSB follows n+10: {lsb} vs {target}"
            );
            let back = read_metrics_key(master.font.get_glyph("h").unwrap(), true);
            assert_eq!(back.as_deref(), Some("=n+10"));
        }
    }

    #[test]
    fn hoi_preview_is_exact_without_baking() {
        // An intermediate point in the lib key alone (no baked brace
        // layers) must already curve the preview: at mid-axis the
        // node sits exactly on Q, at quarter-axis on the quadratic.
        let mut project = Project::load(&fonts::designspace()).expect("loads");
        let name = "n";
        let axis = project.axes[0].clone();
        let (lo, hi) = project.axis_end_masters().expect("two ends");
        let a = {
            let g = project.sources()[lo].font.get_glyph(name).unwrap();
            let p = &g.contours[0].points[0];
            (p.x, p.y)
        };
        let b = {
            let g = project.sources()[hi].font.get_glyph(name).unwrap();
            let p = &g.contours[0].points[0];
            (p.x, p.y)
        };
        let q = ((a.0 + b.0) / 2.0 + 80.0, (a.1 + b.1) / 2.0 + 40.0);
        {
            let mut sources = project.edit_sources();
            let g = sources[lo].font.get_glyph_mut(name).unwrap();
            let mut map = HashMap::new();
            map.insert((0_usize, 0_usize), q);
            write_hoi_intermediates(g, &map);
        }
        let at = |project: &Project, design: f64| {
            let mut location = Location::new();
            location.insert(
                axis.name.clone(),
                crate::document::var_model::normalize_value(
                    design,
                    axis.min,
                    axis.default,
                    axis.max,
                ),
            );
            let g = project.interpolated_at(name, &location).unwrap();
            let p = &g.contours[0].points[0];
            (p.x, p.y)
        };
        let mid_design = axis.min + (axis.max - axis.min) * 0.5;
        let mid = at(&project, mid_design);
        assert!(
            (mid.0 - q.0).abs() < 1e-6 && (mid.1 - q.1).abs() < 1e-6,
            "mid-axis sits on Q: {mid:?} vs {q:?}"
        );
        let quarter_design = axis.min + (axis.max - axis.min) * 0.25;
        let quarter = at(&project, quarter_design);
        let expected = hoi_quad_at(a, b, q, 0.25);
        assert!(
            (quarter.0 - expected.0).abs() < 1e-6 && (quarter.1 - expected.1).abs() < 1e-6,
            "quarter-axis on the quadratic: {quarter:?} vs {expected:?}"
        );
    }

    #[test]
    fn trajectories_sample_the_axis_and_bend_with_braces() {
        let mut project = Project::load(&fonts::designspace()).expect("loads");
        let name = "n";
        let tracks = project
            .trajectory_samples(name, 10)
            .expect("samples with plain masters");
        let regular = project.sources()[0].font.get_glyph(name).unwrap();
        let first_point = &regular.contours[0].points[0];
        // The t=0 end of every track is the Regular master exactly.
        assert!(
            (tracks[0][0].x - first_point.x).abs() < 1e-6
                && (tracks[0][0].y - first_point.y).abs() < 1e-6
        );
        // Straight interpolation: the midpoint sample is the average
        // of the ends.
        let mid_linear = tracks[0][5].x;
        let expected = (tracks[0][0].x + tracks[0][10].x) / 2.0;
        assert!((mid_linear - expected).abs() < 1.0, "linear before braces");
        // A brace at wght 550 (the axis midpoint) pushing the point
        // +60 bends the track's middle away from the straight line.
        let axis = project.axes[0].clone();
        let mut frozen = regular.clone();
        frozen.contours[0].points[0].x += 60.0;
        project.edit_sources()[0]
            .font
            .layers
            .get_or_create_layer("{550}")
            .unwrap()
            .insert_glyph(frozen);
        let mut loc = Location::new();
        loc.insert(
            axis.name.clone(),
            crate::document::var_model::normalize_value(550.0, axis.min, axis.default, axis.max),
        );
        project.brace.push(BraceSource {
            master: 0,
            layer: "{550}".into(),
            location: loc,
        });
        let bent = project.trajectory_samples(name, 10).expect("still samples");
        assert!(
            (bent[0][5].x - mid_linear).abs() > 20.0,
            "brace bends the middle: {} vs {}",
            bent[0][5].x,
            mid_linear
        );
    }

    #[test]
    fn rule_substitute_switches_past_the_condition() {
        let mut project = Project::load(&fonts::designspace()).expect("loads");
        let axis = project.axes[0].clone();
        let doc = project.ds_doc.as_mut().expect("doc kept");
        doc.rules.rules.push(norad::designspace::Rule {
            name: Some("a bold".into()),
            condition_sets: vec![norad::designspace::ConditionSet {
                conditions: vec![norad::designspace::Condition {
                    name: axis.name.clone(),
                    minimum: Some(500.0),
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "designspace axis values are written as f32"
                    )]
                    maximum: Some(axis.max as f32),
                }],
            }],
            substitutions: vec![norad::designspace::Substitution {
                name: norad::Name::new("a").unwrap(),
                with: norad::Name::new("a.bold").unwrap(),
            }],
        });
        let at = |project: &mut Project, design: f64| {
            let axis = &project.axes[0];
            let normalized = crate::document::var_model::normalize_value(
                design,
                axis.min,
                axis.default,
                axis.max,
            );
            let name = axis.name.clone();
            project.location.insert(name, normalized);
        };
        at(&mut project, 450.0);
        assert_eq!(project.rule_substitute("a"), None, "below the switch");
        at(&mut project, 600.0);
        assert_eq!(
            project.rule_substitute("a").as_deref(),
            Some("a.bold"),
            "past the switch"
        );
        assert_eq!(project.rule_substitute("b"), None, "other glyphs untouched");
    }

    #[test]
    fn measures_reference_stems() {
        use crate::analysis::measure::{self, MeasureKind};
        use crate::outline::path::hyper_model::Contour as WContour;
        // Measured straight from the test font's H, the same path the
        // Dimensions section walks.
        let project = Project::load(&fonts::designspace()).expect("loads");
        let font = project.active_font();
        let g = font.font.get_glyph("H").expect("has H");
        let paths: Vec<crate::outline::path::Path> = g
            .contours
            .iter()
            .map(|c| crate::outline::path::Path::from_contour(&WContour::from_norad(c)))
            .collect();
        let stems: Vec<i64> = measure::glyph_measurements(&paths)
            .into_iter()
            .filter(|m| m.kind == MeasureKind::Horizontal)
            .map(|m| m.length)
            .collect();
        assert!(!stems.is_empty(), "H yields horizontal spans");
        let narrowest = stems.iter().min().copied().unwrap();
        assert!(
            (10..400).contains(&narrowest),
            "stem in a plausible range: {narrowest}"
        );
    }

    #[test]
    fn interpolation_at_midpoint() {
        let mut project = Project::load(&fonts::designspace()).expect("designspace");
        assert!(project.model.is_some(), "two masters, model expected");
        // Move every axis to its normalized midpoint toward max.
        let axis_names: Vec<String> = project.axes.iter().map(|a| a.name.clone()).collect();
        for name in &axis_names {
            project.location.insert(name.clone(), 0.5);
        }
        let (path, advance) = project
            .interpolated_glyph("n")
            .expect("compatible masters interpolate");
        assert!(!path.elements().is_empty());
        // The interpolated advance sits between the two masters'.
        let a0 = project.sources()[0].font.get_glyph("n").unwrap().width;
        let a1 = project.sources()[1].font.get_glyph("n").unwrap().width;
        let (lo, hi) = (a0.min(a1), a0.max(a1));
        assert!(
            advance >= lo - 1e-6 && advance <= hi + 1e-6,
            "advance {advance} outside [{lo}, {hi}]"
        );
        // Default location yields no ghost.
        for name in &axis_names {
            project.location.insert(name.clone(), 0.0);
        }
        assert!(project.interpolated_glyph("n").is_none());
    }

    #[test]
    fn compat_map_flags_structure_changes() {
        let mut project = Project::load(&fonts::designspace()).expect("designspace");
        // Demo masters are interpolation-compatible for letters.
        assert_eq!(project.compat.get("n"), Some(&true));
        // Break compatibility in one master and recheck.
        let idx = project.sources()[0]
            .glyphs
            .iter()
            .position(|g| g.name.as_ref() == "n")
            .unwrap();
        let rect = kurbo::Rect::new(0.0, 0.0, 50.0, 50.0);
        project.edit_sources()[0].add_shape_contour(idx, rect, false);
        project.recheck_compat("n");
        assert_eq!(project.compat.get("n"), Some(&false));
    }
}
