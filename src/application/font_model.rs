// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The application-facing font model: the engine's canonical `Project`, plus the denormalized
//! per-glyph cache the grid paints from. The shell may read the transitional active-master
//! projection, but production writes go through Project operations.

use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use kurbo::{BezPath, Rect};
use runebender::analysis::category::GlyphCategory;
use runebender::document::canonical_metadata::{CanonicalFontMetadata, KerningSide};
use runebender::document::model::font_info::CanonicalFontInfo;
use runebender::document::project::{CanonicalGlyphEntry, DocumentEditOutcome, Project};
use runebender::document::proposal;
use runebender::outline::glyph_paths;

pub(crate) use runebender::document::axis::Axis;

/// Everything the grid and previews need for one glyph, without touching norad.
#[derive(Clone)]
pub(crate) struct GlyphEntry {
    pub name: String,
    pub codepoint: Option<char>,
    pub advance: f64,
    /// Full outline (contours plus resolved components), shared with the engine's entry.
    pub outline: Arc<BezPath>,
    /// Ink box of the outline (zero when empty).
    pub ink: Rect,
    pub mark: Option<String>,
    pub category: GlyphCategory,
}

impl GlyphEntry {
    fn from_core(entry: &CanonicalGlyphEntry) -> Self {
        Self {
            name: entry.name().to_owned(),
            codepoint: entry.codepoint(),
            advance: entry.advance(),
            outline: entry.outline().clone(),
            ink: entry.ink(),
            mark: entry.mark().map(str::to_owned),
            category: entry
                .codepoint()
                .map(GlyphCategory::from_codepoint)
                .unwrap_or(GlyphCategory::Other),
        }
    }
}

pub(crate) struct FontModel {
    /// The engine project: the masters, the designspace, and the undo piles.
    pub project: Project,
    /// The active source's canonical glyph entries in display order.
    pub glyphs: Vec<GlyphEntry>,
    /// Constant-time lookup into the display-ordered canonical entries.
    name_map: HashMap<String, usize>,
    pub axes: Vec<Axis>,
}

impl FontModel {
    fn default_source_id(&self) -> runebender::document::variable::SourceId {
        let index = self
            .project
            .master_locations
            .iter()
            .position(|location| location.values().all(|value| *value == 0.0))
            .unwrap_or(0);
        self.project
            .source_id(index)
            .expect("the default source has a stable identity")
    }

    /// Canonical feature text supplied by the default source.
    pub(crate) fn feature_text(&self) -> &str {
        self.project
            .document_feature_text(self.default_source_id())
            .expect("the default source has canonical feature text")
    }

    /// Canonical group and exact kerning values for the active source.
    pub(crate) fn font_metadata(&self) -> &CanonicalFontMetadata {
        self.font_metadata_at(self.active())
            .expect("the active source has canonical font metadata")
    }

    /// Canonical group and exact kerning values for one source index.
    pub(crate) fn font_metadata_at(&self, index: usize) -> Option<&CanonicalFontMetadata> {
        self.project
            .source_id(index)
            .and_then(|source| self.project.document_font_metadata(source))
    }

    /// Canonical names, metrics and OpenType information for the active source.
    pub(crate) fn font_info(&self) -> &CanonicalFontInfo {
        self.font_info_at(self.active())
            .expect("the active source has canonical font information")
    }

    /// Canonical names, metrics and OpenType information for one source index.
    pub(crate) fn font_info_at(&self, index: usize) -> Option<&CanonicalFontInfo> {
        self.project
            .source_id(index)
            .and_then(|source| self.project.document_font_info(source))
    }

    /// Stable address of one glyph's active default layer.
    pub(crate) fn active_layer_address(
        &self,
        glyph: &str,
    ) -> Option<runebender::document::variable::GlyphLayerAddress> {
        let source = self.project.source_id(self.active())?;
        let layer = self.project.document_source(source)?.default_layer();
        self.project.document_layer(glyph, &layer).map(|_| {
            runebender::document::variable::GlyphLayerAddress {
                glyph: glyph.to_owned(),
                layer,
            }
        })
    }

    /// Project-owned history depth for one active-source glyph layer.
    pub(crate) fn history_depth(
        &self,
        glyph: &str,
        direction: runebender::document::history::HistoryDirection,
    ) -> usize {
        self.active_layer_address(glyph).map_or(0, |address| {
            self.project
                .document_layer_history_depth(&address, direction)
        })
    }

    /// Whether Project-owned history can replay one active-source glyph layer.
    pub(crate) fn can_replay_history(
        &self,
        glyph: &str,
        direction: runebender::document::history::HistoryDirection,
    ) -> bool {
        self.active_layer_address(glyph).is_some_and(|address| {
            self.project
                .can_replay_document_layer_history(&address, direction)
        })
    }

    pub(crate) fn preview_font(
        &self,
    ) -> Result<Option<Arc<runebender::document::compile::CompiledFont>>, String> {
        if cfg!(test) || std::env::var_os("RUNEBENDER_SCREENSHOT").is_some() {
            self.project.compiled_preview().map(Some)
        } else {
            self.project.request_preview()
        }
    }
    pub(crate) fn open(path: &FsPath) -> Result<Self, String> {
        let project = Project::load(path)?;
        Ok(Self::from_project(project))
    }

    pub(crate) fn from_project(project: Project) -> Self {
        let axes = project.axes.iter().map(|axis| axis.user.clone()).collect();
        let mut model = Self {
            project,
            glyphs: Vec::new(),
            name_map: HashMap::new(),
            axes,
        };
        model.rebuild_cache();
        model
    }

    /// Rebuild every shell entry from the active canonical source.
    pub(crate) fn rebuild_cache(&mut self) {
        let source = self
            .project
            .source_id(self.active())
            .expect("the active source has a stable identity");
        self.glyphs = self
            .project
            .document_source_glyph_entries(source)
            .expect("the active source remains canonical")
            .iter()
            .map(GlyphEntry::from_core)
            .collect();
        self.name_map = self
            .glyphs
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.name.clone(), index))
            .collect();
    }

    /// Refresh one shell entry from its canonical default layer after an edit.
    pub(crate) fn refresh_entry(&mut self, index: usize) {
        let Some(name) = self.glyphs.get(index).map(|entry| entry.name.clone()) else {
            return;
        };
        let source = self
            .project
            .source_id(self.active())
            .expect("the active source has a stable identity");
        let Some(entry) = self
            .project
            .document_source_glyph_entry(source, &name)
            .expect("the active source remains canonical")
        else {
            return;
        };
        self.glyphs[index] = GlyphEntry::from_core(&entry);
    }

    /// Materialize the active source only for assertions at the UFO boundary.
    #[cfg(test)]
    pub(crate) fn font_snapshot(&self) -> norad::Font {
        self.project
            .source_id(self.active())
            .and_then(|source| self.project.encode_ufo_source(source))
            .expect("the active source remains materializable")
    }

    pub(crate) fn source(&self) -> &FsPath {
        self.project
            .source_id(self.active())
            .and_then(|source| self.project.document_source(source))
            .expect("the active source remains in the document")
            .path()
    }

    /// The source that defines the whole document: the designspace when this
    /// is a multi-master project, otherwise the active UFO.
    pub(crate) fn document_source(&self) -> &FsPath {
        self.project
            .export_source
            .as_deref()
            .unwrap_or_else(|| self.source())
    }

    /// Whether every master source is writable according to its filesystem mode.
    pub(crate) fn is_writable(&self) -> bool {
        self.project
            .document_sources()
            .all(|source| save_target_is_writable(source.path()))
    }

    pub(crate) fn active(&self) -> usize {
        self.project.active
    }

    pub(crate) fn units_per_em(&self) -> f64 {
        self.font_info().metrics.resolved().units_per_em
    }

    pub(crate) fn ascender(&self) -> f64 {
        self.font_info().metrics.resolved().ascender
    }

    pub(crate) fn descender(&self) -> f64 {
        self.font_info().metrics.resolved().descender
    }

    pub(crate) fn master_names(&self) -> Vec<String> {
        self.project
            .master_names
            .iter()
            .map(|n| n.to_string())
            .collect()
    }

    pub(crate) fn master_name(&self, index: usize) -> String {
        self.project
            .master_names
            .get(index)
            .map(|n| n.to_string())
            .unwrap_or_default()
    }

    pub(crate) fn master_paths(&self) -> Vec<PathBuf> {
        self.project
            .document_sources()
            .map(|source| source.path().to_owned())
            .collect()
    }

    /// Switch the active master. Each master keeps its own edits, so
    /// nothing is flushed; the cache is rebuilt for the new one.
    pub(crate) fn set_active(&mut self, index: usize) {
        if index >= self.project.document_sources().count() || index == self.project.active {
            return;
        }
        self.project.active = index;
        self.project.snap_location_to_master(index);
        self.rebuild_cache();
    }

    pub(crate) fn index_of(&self, name: &str) -> Option<usize> {
        self.name_map.get(name).copied()
    }

    /// Add an empty glyph to every master.
    ///
    /// Encoded when a codepoint is given, or when the name is a single
    /// character. A glyph that exists in one master and not another is a
    /// designspace that does not build, so this writes all of them.
    pub(crate) fn add_glyph(
        &mut self,
        name: &str,
        default_advance: f64,
        unicode: Option<u32>,
    ) -> bool {
        let Ok(DocumentEditOutcome::Changed { .. }) =
            self.project
                .add_document_glyph(name, default_advance, unicode)
        else {
            return false;
        };
        self.rebuild_cache();
        true
    }

    /// Add every glyph in `targets` that the font does not have yet, and
    /// report how many were added.
    pub(crate) fn add_missing(&mut self, targets: &[(String, Option<u32>)]) -> usize {
        let advance = (self.units_per_em() * 0.5).round();
        let Ok((added, DocumentEditOutcome::Changed { .. })) =
            self.project.add_missing_document_glyphs(targets, advance)
        else {
            return 0;
        };
        self.rebuild_cache();
        added
    }

    /// Duplicate `source` into the first free `stem.NNN` name in every master.
    ///
    /// The copy carries the editable glyph data used by the GPUI command but is
    /// deliberately unencoded. Returns the new name, or `None` when the source
    /// is absent from the active master.
    pub(crate) fn duplicate_glyph(&mut self, source: &str) -> Option<String> {
        let Ok((name, DocumentEditOutcome::Changed { .. })) =
            self.project.duplicate_document_glyph(source)
        else {
            return None;
        };
        self.rebuild_cache();
        Some(name)
    }

    /// Remove `name` from every master and refresh the active-master cache.
    pub(crate) fn remove_glyph(&mut self, name: &str) -> bool {
        let Ok(DocumentEditOutcome::Changed { .. }) = self.project.remove_document_glyph(name)
        else {
            return false;
        };
        self.rebuild_cache();
        true
    }

    /// Rename a glyph, in every master.
    ///
    /// `runebender` does the work inside one font: the glyph, the
    /// components that place it, group memberships, and kerning keys on
    /// either side. This applies that to all the masters, because a
    /// designspace whose sources disagree about a glyph name does not
    /// build.
    pub(crate) fn rename_glyph(&mut self, old: &str, new: &str) -> bool {
        let Ok(DocumentEditOutcome::Changed { .. }) = self.project.rename_document_glyph(old, new)
        else {
            return false;
        };
        self.rebuild_cache();
        true
    }

    /// Save every master to its UFO.
    pub(crate) fn save(&mut self) -> Result<(), String> {
        self.project.save()
    }

    /// The given master's axis location in user coordinates, one per axis,
    /// mapping the engine's stored normalized location back through the axis map.
    pub(crate) fn master_axis_values(&self, index: usize) -> Vec<f64> {
        let loc = self.project.master_locations.get(index);
        self.axes
            .iter()
            .map(|ax| match loc.and_then(|l| l.get(&ax.name)) {
                Some(value) => ax.normalized_to_user(*value),
                None => ax.default,
            })
            .collect()
    }

    /// Interpolate `glyph_name` at the given user-unit axis location. Returns
    /// the interpolated outline (design space) or None if incompatible.
    /// Composite glyphs interpolate both their component offsets and, through
    /// recursion, each component's base outline.
    pub(crate) fn interpolate_outline(
        &self,
        glyph_name: &str,
        location: &HashMap<String, f64>,
    ) -> Option<BezPath> {
        if self.project.model.is_none() || self.axes.is_empty() {
            return None;
        }
        // The engine already stores master locations normalized. Normalize the
        // user-coordinate slider location once, preserving any axis map.
        let target: HashMap<String, f64> = self
            .axes
            .iter()
            .map(|ax| {
                let v = location.get(&ax.name).copied().unwrap_or(ax.default);
                (ax.name.clone(), ax.user_to_normalized(v))
            })
            .collect();
        self.project
            .interpolated_outline_at(glyph_name, &target)
            .ok()
    }

    /// How many masters the family has.
    pub(crate) fn master_count(&self) -> usize {
        self.project.document_sources().count()
    }

    /// Short display names for the masters: the common family prefix is
    /// dropped, so "Bricolage Grotesque 96pt `ExtraBold`" reads as
    /// "96pt `ExtraBold`" in a narrow inspector.
    pub(crate) fn short_master_names(&self) -> Vec<String> {
        let names = self.master_names();
        if names.len() < 2 {
            return names;
        }
        // The longest common prefix, cut back to a word boundary.
        let first = names[0].as_str();
        let mut cut = first.len();
        for other in &names[1..] {
            let common = first
                .char_indices()
                .zip(other.chars())
                .take_while(|((_, a), b)| a == b)
                .map(|((i, a), _)| i + a.len_utf8())
                .last()
                .unwrap_or(0);
            cut = cut.min(common);
        }
        let cut = first[..cut].rfind(' ').map(|i| i + 1).unwrap_or(0);
        names
            .iter()
            .map(|n| {
                let short = n[cut.min(n.len())..].trim();
                if short.is_empty() {
                    n.clone()
                } else {
                    short.to_string()
                }
            })
            .collect()
    }

    /// Outlines of `glyph_name` in the chosen masters, for the
    /// ghost overlay. The inspector's Layers section owns that set.
    pub(crate) fn reference_outlines(
        &self,
        glyph_name: &str,
        which: &std::collections::HashSet<usize>,
    ) -> Vec<BezPath> {
        self.project
            .document_sources()
            .enumerate()
            .filter(|(index, _)| which.contains(index) && *index != self.project.active)
            .filter_map(|(_, source)| {
                self.project
                    .document_source_glyph_entry(source.id(), glyph_name)
                    .ok()
                    .flatten()
                    .map(|glyph| glyph.outline().as_ref().clone())
            })
            .collect()
    }

    /// The glyph's outline in the background layer, as a path.
    pub(crate) fn background_outline(&self, glyph: &str) -> Option<BezPath> {
        let source = self.project.source_id(self.active())?;
        let (_, background) = self.project.document_background_layer(glyph, source)?;
        Some(glyph_paths::ordinary_layer_contours_to_bezpath(background))
    }

    /// A glyph from a waiting proposal layer, as a path for the
    /// read-only comparison overlay.
    pub(crate) fn proposal_outline(&self, task: &str, glyph: &str) -> Option<BezPath> {
        let source = self.project.source_id(self.active())?;
        let address = runebender::document::variable::GlyphLayerAddress {
            glyph: glyph.to_owned(),
            layer: runebender::document::variable::LayerId {
                source,
                name: proposal::layer_name(task),
            },
        };
        self.project.document_layer_path(&address).ok()
    }

    /// Another glyph's outline, for the reference underlay.
    pub(crate) fn glyph_outline(&self, glyph: &str) -> Option<Arc<BezPath>> {
        let index = self.index_of(glyph)?;
        Some(self.glyphs[index].outline.clone())
    }

    /// The kerning group this glyph belongs to on one side, if any.
    ///
    /// `first_side` is the left side in left-to-right text: `public.kern1`.
    pub(crate) fn kern_group(&self, glyph: &str, first_side: bool) -> String {
        let side = if first_side {
            KerningSide::First
        } else {
            KerningSide::Second
        };
        self.font_metadata()
            .kerning_group(glyph, side)
            .map(|name| name.to_string())
            .unwrap_or_default()
    }

    /// Put the glyph in a kerning group on one side, in every master.
    ///
    /// Kerning groups are font-wide, and a designspace's masters have to
    /// agree about them or the kerning will not interpolate, so this
    /// writes all of them rather than only the active one.
    pub(crate) fn set_kern_group(&mut self, glyph: &str, first_side: bool, group: &str) -> bool {
        let side = if first_side {
            KerningSide::First
        } else {
            KerningSide::Second
        };
        let mut changed = false;
        let sources: Vec<_> = self
            .project
            .document_sources()
            .map(|source| source.id())
            .collect();
        for source in sources {
            let Some(mut metadata) = self.project.document_font_metadata(source).cloned() else {
                continue;
            };
            let Ok(edited) = metadata.set_kerning_group(glyph, side, Some(group)) else {
                continue;
            };
            if !edited {
                continue;
            }
            changed |= matches!(
                self.project.edit_document_source_metadata(source, |draft| {
                    draft.set_font_metadata(metadata);
                    Ok(())
                }),
                Ok(DocumentEditOutcome::Changed { .. })
            );
        }
        changed
    }

    /// How many glyphs are marked for export.
    ///
    /// A glyph is skipped when its lib says so, which is how both Glyphs
    /// and the UFO spec record it. The filter list shows the resulting count.
    pub(crate) fn exporting_count(&self) -> usize {
        let source = self
            .project
            .source_id(self.active())
            .expect("the active source has a stable identity");
        self.glyphs
            .iter()
            .filter(|entry| {
                self.project
                    .document_source_glyph_metadata(source, &entry.name)
                    .is_none_or(|metadata| metadata.exported())
            })
            .count()
    }

    /// How many glyphs the masters disagree about, by the engine's check.
    pub(crate) fn incompatible_count(&self) -> usize {
        if self.project.document_sources().count() < 2 {
            return 0;
        }
        self.glyphs
            .iter()
            .filter(|entry| !self.project.check_compat(&entry.name))
            .count()
    }

    /// Compare one source's default-layer coverage and advances with the active source.
    pub(crate) fn source_geometry_comparison(&self, index: usize) -> Option<(usize, usize, usize)> {
        let project = &self.project;
        let reference_layer = project
            .source_id(self.active())
            .and_then(|source| project.document_source(source))?
            .default_layer();
        let layer = project
            .source_id(index)
            .and_then(|source| project.document_source(source))?
            .default_layer();
        let reference_glyphs: Vec<_> = project
            .glyph_names()
            .filter(|name| project.document_layer(name, &reference_layer).is_some())
            .collect();
        let glyph_count = project
            .glyph_names()
            .filter(|name| project.document_layer(name, &layer).is_some())
            .count();
        let missing = reference_glyphs
            .iter()
            .filter(|name| project.document_layer(name, &layer).is_none())
            .count();
        let advance_differences = reference_glyphs
            .iter()
            .filter(|name| {
                project
                    .document_layer(name, &layer)
                    .zip(project.document_layer(name, &reference_layer))
                    .is_some_and(|(candidate, reference)| {
                        (candidate.width() - reference.width()).abs() > 0.5
                    })
            })
            .count();
        Some((glyph_count, missing, advance_differences))
    }

    /// The font's headline metadata, as label and value pairs.
    ///
    /// The overview shows this whenever no glyph is picked. These values are
    /// read here rather than edited: writing them back means a form per
    /// master and a rule about which values are per-master, which the
    /// editor does not have yet.
    pub(crate) fn info_rows(&self) -> Vec<(&'static str, String)> {
        let info = self.font_info();
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        let number = |value: Option<f64>| value.map(|v| format!("{v:.0}")).unwrap_or_default();
        vec![
            ("Family name", text(&info.names.family_name)),
            ("Style name", text(&info.names.style_name)),
            ("UPM", number(info.metrics.units_per_em)),
            ("Italic angle", number(info.metrics.italic_angle)),
            ("Ascender", number(info.metrics.ascender)),
            ("Descender", number(info.metrics.descender)),
            ("x-height", number(info.metrics.x_height)),
            ("Cap height", number(info.metrics.cap_height)),
            (
                "typoAsc",
                number(info.open_type_metrics.typo_ascender.map(f64::from)),
            ),
            (
                "typoDesc",
                number(info.open_type_metrics.typo_descender.map(f64::from)),
            ),
            (
                "hheaAsc",
                number(info.open_type_metrics.hhea_ascender.map(f64::from)),
            ),
            (
                "hheaDesc",
                number(info.open_type_metrics.hhea_descender.map(f64::from)),
            ),
            (
                "winAsc",
                number(info.open_type_metrics.win_ascent.map(f64::from)),
            ),
            (
                "winDesc",
                number(info.open_type_metrics.win_descent.map(f64::from)),
            ),
        ]
    }

    /// Set the advance of a glyph that is not open in the editor.
    ///
    /// The overview panel edits the selected cell directly, so this works
    /// from an index rather than from a session. Only the active master
    /// changes: an advance is a per-master measurement.
    pub(crate) fn set_glyph_advance(&mut self, index: usize, width: f64) -> bool {
        if !width.is_finite() {
            return false;
        }
        let width = width.max(0.0);
        let Some(name) = self.glyphs.get(index).map(|entry| entry.name.clone()) else {
            return false;
        };
        let Some(address) = self.active_layer_address(&name) else {
            return false;
        };
        let Ok(mut transaction) = self.project.begin_document_layer_transaction(&address) else {
            return false;
        };
        if transaction.draft_mut().set_width(width) != Ok(true)
            || !matches!(
                self.project.commit_document_layer_transaction(transaction),
                Ok(DocumentEditOutcome::Changed { .. })
            )
        {
            return false;
        }
        self.refresh_entry(index);
        true
    }

    /// Exact codepoint lists for `name`, one per document source.
    pub(crate) fn glyph_codepoints(&self, name: &str) -> Option<Vec<Vec<char>>> {
        self.project.document_glyph_codepoints(name)
    }

    /// Replace `name`'s codepoints in every document source from an exact snapshot.
    pub(crate) fn set_glyph_codepoints(&mut self, name: &str, values: &[Vec<char>]) -> bool {
        let changed = matches!(
            self.project.set_document_glyph_codepoints(name, values),
            Ok(DocumentEditOutcome::Changed { .. })
        );
        if !changed {
            return false;
        }
        self.rebuild_cache();
        true
    }
}

/// Whether an existing save target, or the directory for a new one, is writable.
fn save_target_is_writable(target: &FsPath) -> bool {
    if target.exists() {
        return std::fs::metadata(target)
            .map(|metadata| !metadata.permissions().readonly())
            .unwrap_or(false);
    }
    target
        .parent()
        .and_then(|parent| std::fs::metadata(parent).ok())
        .is_some_and(|metadata| metadata.is_dir() && !metadata.permissions().readonly())
}

#[cfg(test)]
mod tests {
    use super::*;
    use runebender::document::canonical_metadata::KerningParticipant;
    use runebender::document::project::SourceInput;

    fn two_master_model() -> (PathBuf, FontModel) {
        let dir = std::env::temp_dir().join(format!(
            "runebender-xilem-glyph-commands-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        std::fs::create_dir_all(&dir).expect("the fixture directory is created");
        for (file, width) in [("Regular.ufo", 500.0), ("Bold.ufo", 620.0)] {
            let mut font = norad::Font::new();
            let mut glyph = norad::Glyph::new("A");
            glyph.width = width;
            let mut contour = norad::Contour::default();
            contour.points.push(norad::ContourPoint::new(
                width,
                0.0,
                norad::PointType::Move,
                false,
                None,
                None,
            ));
            glyph.contours.push(contour);
            glyph.codepoints = norad::Codepoints::new(['A']);
            font.default_layer_mut().insert_glyph(glyph);
            font.save(dir.join(file)).expect("the master saves");
        }
        let designspace = dir.join("Test.designspace");
        std::fs::write(
            &designspace,
            r#"<?xml version='1.0' encoding='UTF-8'?>
<designspace format="4.0">
  <axes><axis name="Weight" tag="wght" minimum="400" default="400" maximum="700"/></axes>
  <sources>
    <source familyname="Test" stylename="Regular" filename="Regular.ufo">
      <location><dimension name="Weight" xvalue="400"/></location>
    </source>
    <source familyname="Test" stylename="Bold" filename="Bold.ufo">
      <location><dimension name="Weight" xvalue="700"/></location>
    </source>
  </sources>
</designspace>"#,
        )
        .expect("the designspace saves");
        let model = FontModel::open(&designspace).expect("the designspace opens");
        (dir, model)
    }

    #[test]
    fn duplicate_and_remove_glyph_apply_to_every_master() {
        let (dir, mut model) = two_master_model();

        let copy = model.duplicate_glyph("A").expect("A duplicates");
        assert_eq!(copy, "A.001");
        for (source, width) in model.project.document_sources().zip([500.0, 620.0]) {
            let glyph = model
                .project
                .document_layer(&copy, &source.default_layer())
                .expect("the copy exists");
            assert_eq!(glyph.width(), width);
            assert_eq!(glyph.contours().count(), 1);
            assert_eq!(glyph.codepoints().count(), 0);
        }
        assert!(model.remove_glyph(&copy));
        assert!(model.project.document_glyph(&copy).is_none());

        std::fs::remove_dir_all(dir).expect("the fixture is removed");
    }

    #[test]
    fn source_metadata_queries_read_canonical_unsaved_values() {
        let (_, mut model) = two_master_model();
        let source = model
            .project
            .source_id(model.active())
            .expect("active source identity");
        let mut metadata = model
            .project
            .document_font_metadata(source)
            .expect("canonical source metadata")
            .clone();
        metadata
            .set_kerning_group("A", KerningSide::First, Some("A"))
            .expect("valid group edit");
        metadata
            .set_kerning_pair(
                KerningParticipant::group(KerningSide::First, "A").expect("valid group"),
                KerningParticipant::glyph("V").expect("valid glyph"),
                Some(-80.25),
            )
            .expect("valid exact kerning pair");
        let mut font_info = model
            .project
            .document_font_info(source)
            .expect("canonical font information")
            .clone();
        font_info.names.family_name = Some("Unsaved Family".into());
        font_info.metrics.units_per_em = Some(2048.0);
        model
            .project
            .edit_document_source_metadata(source, |draft| {
                draft.set_font_metadata(metadata);
                draft.set_font_info(font_info);
                Ok(())
            })
            .expect("canonical metadata edit commits");

        assert_eq!(model.kern_group("A", true), "public.kern1.A");
        assert!(
            model
                .font_metadata()
                .kerning_pairs()
                .any(
                    |(first, second, value)| first.as_raw_name() == "public.kern1.A"
                        && second.as_raw_name() == "V"
                        && value == -80.25
                )
        );
        assert_eq!(model.units_per_em(), 2048.0);
        let session = crate::application::editor::session::Session::new_from_model(&model, "A")
            .expect("the canonical source glyph opens");
        assert_eq!(session.metrics.upm, 2048.0);
        assert_eq!(session.metrics.ascender, 2048.0 * 0.8);
        assert_eq!(
            model.info_rows().into_iter().take(3).collect::<Vec<_>>(),
            [
                ("Family name", "Unsaved Family".into()),
                ("Style name", String::new()),
                ("UPM", "2048".into()),
            ]
        );
    }

    #[test]
    fn master_locations_are_presented_on_the_user_axis_scale() {
        let (dir, model) = two_master_model();

        assert_eq!(model.master_axis_values(0), vec![400.0]);
        assert_eq!(model.master_axis_values(1), vec![700.0]);
        assert_eq!(model.source_geometry_comparison(1), Some((1, 0, 1)));

        std::fs::remove_dir_all(dir).expect("the fixture is removed");
    }

    #[test]
    fn mapped_axis_coordinates_round_trip_through_normalized_space() {
        let axis = Axis {
            name: "Weight".into(),
            tag: "wght".into(),
            min: 100.0,
            default: 400.0,
            max: 900.0,
            map: vec![(100.0, 0.0), (400.0, 50.0), (900.0, 100.0)],
        };

        for (user, normalized) in [(100.0, -1.0), (400.0, 0.0), (650.0, 0.5), (900.0, 1.0)] {
            assert!((axis.user_to_normalized(user) - normalized).abs() < 1e-9);
            assert!((axis.normalized_to_user(normalized) - user).abs() < 1e-9);
        }
    }

    #[test]
    fn fixture_keeps_an_incompatible_master_pair() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/incompatible/Test.designspace");
        let model = FontModel::open(&path).expect("the checked-in test fixture opens");

        assert_eq!(model.incompatible_count(), 1);
        let detail = model
            .project
            .compat_detail("A")
            .expect("A is deliberately incompatible");
        assert!(detail.contains("Regular 0c"));
        assert!(detail.contains("Bold 1c"));
    }

    #[test]
    fn save_targets_require_a_writable_directory() {
        let (dir, model) = two_master_model();
        assert!(model.is_writable());

        let file = dir.join("not-a-directory");
        std::fs::write(&file, "fixture").expect("the ordinary file is created");
        let invalid = FontModel::from_project(Project::from_source(SourceInput::from_font(
            norad::Font::new(),
            file.join("New.ufo"),
        )));
        assert!(!invalid.is_writable());

        std::fs::remove_dir_all(dir).expect("the fixture is removed");
    }
}
