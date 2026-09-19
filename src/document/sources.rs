// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Source and auxiliary-layer authoring transactions.
//!
//! Source identity survives display-order changes. Removing a source updates the
//! document; it never deletes its UFO directory. Structural undo refuses to
//! overwrite later content edits, which must be undone first.

use super::*;
use std::collections::{BTreeMap, BTreeSet};

use crate::document::CanonicalSourceStructureSnapshot;
use crate::document::LayerView;
use crate::document::canonical_metadata::{CanonicalFontMetadata, KerningParticipant};
use crate::document::history::{
    EditHistory, HistoryDirection, HistoryReplayError, HistoryReplayOutcome, TransactionHistory,
};
use crate::document::model::designspace::{
    CanonicalLocation, InstanceId, SourceDescriptor, SourceOrderEntry, SparseSourceDescriptor,
};
use crate::document::model::font_info::CanonicalFontInfo;
use crate::document::variable::source_builder;

#[derive(Debug, Clone)]
struct SourceFrame {
    canonical: CanonicalSourceStructureSnapshot,
    active: Option<SourceId>,
}

impl PartialEq for SourceFrame {
    fn eq(&self, other: &Self) -> bool {
        self.canonical == other.canonical
    }
}

impl SourceFrame {
    fn capture(project: &Project) -> Self {
        let canonical = project.variable.source_structure_snapshot();
        Self {
            active: canonical.source_ids().get(project.active).copied(),
            canonical,
        }
    }

    fn matches(&self, project: &Project) -> bool {
        self == &Self::capture(project)
    }

    fn restore_if_current(
        &self,
        project: &mut Project,
        expected: &Self,
        retired_histories: &mut BTreeMap<SourceId, EditHistory>,
        retired_layer_histories: &mut BTreeMap<LayerId, EditHistory>,
    ) -> Result<(), String> {
        if !expected.matches(project) {
            return Err("source structure changed after history capture".into());
        }
        if project.document_designspace().is_none()
            && (expected.canonical.source_ids() != self.canonical.source_ids()
                || project.masters.len() != self.canonical.source_ids().len())
        {
            return Err("standalone source history changed source identities".into());
        }
        let previous_ids = project.variable.source_ids.clone();
        let canonical_changed = project
            .variable
            .restore_source_structure_if_current(&expected.canonical, self.canonical.clone())
            .map_err(|_| "canonical source structure changed after history capture")?;
        if !canonical_changed {
            project.variable.revision = project.variable.revision.wrapping_add(1);
        }
        reconcile_compatibility_layer_histories(&mut project.variable, retired_layer_histories);
        let Some(designspace) = project.document_designspace().cloned() else {
            debug_assert_eq!(
                previous_ids, project.variable.source_ids,
                "a standalone source-history frame retains its source identities"
            );
            for (master, source) in project
                .masters
                .iter_mut()
                .zip(project.variable.source_ids.iter().copied())
            {
                master.font = project
                    .variable
                    .source_font(source)
                    .ok_or_else(|| format!("missing canonical source {}", source.0))?;
            }
            project.active = self
                .active
                .and_then(|active| {
                    project
                        .variable
                        .source_ids
                        .iter()
                        .position(|source| *source == active)
                })
                .unwrap_or(0);
            project.finish_source_restore();
            return Ok(());
        };
        let source_ids = designspace.full_source_order().collect::<Vec<_>>();
        let sources = source_ids
            .iter()
            .map(|id| {
                designspace
                    .sources()
                    .iter()
                    .find(|source| source.id() == *id)
                    .ok_or_else(|| format!("missing structural descriptor for source {}", id.0))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let previous_masters = std::mem::take(&mut project.masters);
        project.master_names = sources
            .iter()
            .map(|source| source.display_name().into())
            .collect();
        project.master_locations = sources
            .iter()
            .map(|source| source.location.to_normalized(designspace.axes()))
            .collect::<Result<_, _>>()?;
        project.ds_doc = Some(designspace.to_norad()?);
        project.brace = designspace
            .source_order()
            .iter()
            .filter_map(|entry| match entry {
                SourceOrderEntry::Full(_) => None,
                SourceOrderEntry::Sparse(layer) => Some((
                    layer,
                    designspace
                        .sparse_sources()
                        .iter()
                        .find(|source| source.layer == *layer),
                )),
            })
            .map(|(layer, source)| {
                let source = source
                    .ok_or_else(|| format!("missing sparse descriptor for layer {}", layer.name))?;
                let master = source_ids
                    .iter()
                    .position(|id| *id == layer.source)
                    .ok_or_else(|| format!("missing source for sparse layer {}", layer.name))?;
                Ok(BraceSource {
                    master,
                    layer: layer.name.clone(),
                    location: source.location.to_normalized(designspace.axes())?,
                })
            })
            .collect::<Result<_, String>>()?;
        project.active = self
            .active
            .and_then(|active| source_ids.iter().position(|source| *source == active))
            .unwrap_or(0);
        project.rebuild_source_projections(
            &sources,
            previous_ids,
            previous_masters,
            retired_histories,
        )?;
        project.refresh_instances_from_doc();
        project.finish_source_restore();
        Ok(())
    }
}

#[derive(Debug, Default)]
pub(super) struct SourceHistory {
    transactions: TransactionHistory<SourceFrame>,
    retired_histories: BTreeMap<SourceId, EditHistory>,
    retired_layer_histories: BTreeMap<LayerId, EditHistory>,
}

fn park_compatibility_layer_histories(
    variable: &mut VariableData,
    source: SourceId,
    retired: &mut BTreeMap<LayerId, EditHistory>,
) {
    let layers = variable
        .histories
        .keys()
        .filter(|layer| layer.source == source)
        .cloned()
        .collect::<Vec<_>>();
    for layer in layers {
        let history = variable
            .histories
            .remove(&layer)
            .expect("the collected compatibility layer history exists");
        retired.insert(layer, history);
    }
}

fn reconcile_compatibility_layer_histories(
    variable: &mut VariableData,
    retired: &mut BTreeMap<LayerId, EditHistory>,
) {
    let removed_sources = variable
        .histories
        .keys()
        .filter(|layer| !variable.source_ids.contains(&layer.source))
        .map(|layer| layer.source)
        .collect::<Vec<_>>();
    for source in removed_sources {
        park_compatibility_layer_histories(variable, source, retired);
    }
    let restored_layers = retired
        .keys()
        .filter(|layer| variable.source_ids.contains(&layer.source))
        .cloned()
        .collect::<Vec<_>>();
    for layer in restored_layers {
        let history = retired
            .remove(&layer)
            .expect("the collected retired layer history exists");
        variable.histories.entry(layer).or_insert(history);
    }
}

impl Project {
    fn finish_source_change(&mut self) {
        self.variable.synchronize(&self.masters);
        self.finish_source_restore();
    }

    fn finish_source_restore(&mut self) {
        self.model = (!self.axes.is_empty()).then(|| {
            VariationModel::new(&self.master_locations).expect("source locations were validated")
        });
        self.ds_dirty = self.ds_doc.is_some();
        for master in &mut self.masters {
            master.dirty = true;
            master.refresh_from_font();
        }
        self.snap_location_to_master(self.active);
        self.compute_compat();
    }

    fn rebuild_source_projections(
        &mut self,
        descriptors: &[&SourceDescriptor],
        previous_ids: Vec<SourceId>,
        previous_masters: Vec<Master>,
        retired_histories: &mut BTreeMap<SourceId, EditHistory>,
    ) -> Result<(), String> {
        let mut previous = previous_ids
            .into_iter()
            .zip(previous_masters)
            .collect::<BTreeMap<_, _>>();
        let source_directory = self
            .export_source
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| {
                previous
                    .values()
                    .next()
                    .and_then(|master| master.source_path.parent())
                    .map(Path::to_path_buf)
            });
        let rebuilt = self
            .variable
            .source_ids
            .iter()
            .copied()
            .map(|id| {
                let descriptor = descriptors
                    .iter()
                    .find(|descriptor| descriptor.id() == id)
                    .ok_or_else(|| format!("missing structural descriptor for source {}", id.0))?;
                let font = self
                    .variable
                    .source_font(id)
                    .ok_or_else(|| format!("missing canonical source {}", id.0))?;
                let old = previous.remove(&id);
                let path = old.as_ref().map_or_else(
                    || {
                        source_directory.as_ref().map_or_else(
                            || PathBuf::from(&descriptor.filename),
                            |directory| directory.join(&descriptor.filename),
                        )
                    },
                    |master| master.source_path.clone(),
                );
                let mut rebuilt = Master::from_font(font, path);
                if let Some(mut old) = old {
                    rebuilt.modified_glyphs = std::mem::take(&mut old.modified_glyphs);
                    rebuilt.glif_paths = std::mem::take(&mut old.glif_paths);
                    rebuilt.preserved_files = std::mem::take(&mut old.preserved_files);
                    rebuilt.kerning_dirty = old.kerning_dirty;
                    rebuilt.revision = old.revision.wrapping_add(1);
                    rebuilt.history = std::mem::take(&mut old.history);
                    retired_histories.remove(&id);
                } else if let Some(history) = retired_histories.remove(&id) {
                    rebuilt.history = history;
                }
                rebuilt.dirty = true;
                Ok(rebuilt)
            })
            .collect::<Result<_, String>>()?;
        for (id, mut removed) in previous {
            retired_histories.insert(id, std::mem::take(&mut removed.history));
        }
        self.masters = rebuilt;
        Ok(())
    }

    fn record_source_change(&mut self, before: SourceFrame) {
        let revision = self.variable.revision;
        self.finish_source_change();
        let after = SourceFrame::capture(self);
        if before.canonical != after.canonical && self.variable.revision == revision {
            self.variable.revision = self.variable.revision.wrapping_add(1);
        }
        self.source_history.transactions.record(before, after);
    }

    fn record_canonical_source_change(&mut self, before: SourceFrame) {
        self.finish_source_restore();
        let after = SourceFrame::capture(self);
        self.source_history.transactions.record(before, after);
    }

    fn interpolated_source_metadata(
        &self,
        default_source: SourceId,
        name: &str,
        location: &Location,
    ) -> Result<(String, CanonicalFontMetadata, CanonicalFontInfo), String> {
        let feature_text = self
            .document_feature_text(default_source)
            .ok_or("missing default source feature text")?
            .to_owned();
        let source_metadata = self
            .document_sources()
            .map(|source| {
                self.document_font_metadata(source.id())
                    .cloned()
                    .ok_or_else(|| format!("missing metadata for source {}", source.id().0))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let pairs = source_metadata
            .iter()
            .flat_map(|metadata| {
                metadata
                    .kerning_pairs()
                    .map(|(left, right, _)| (left.clone(), right.clone()))
            })
            .collect::<BTreeSet<(KerningParticipant, KerningParticipant)>>();
        let mut font_metadata = CanonicalFontMetadata::from_raw(
            self.document_font_metadata(default_source)
                .ok_or("missing default source metadata")?
                .groups()
                .clone(),
            BTreeMap::new(),
        )
        .map_err(|error| error.to_string())?;
        let model = VariationModel::new(&self.master_locations)?;
        for (left, right) in pairs {
            let values = source_metadata
                .iter()
                .map(|metadata| {
                    vec![
                        metadata
                            .kerning_pairs()
                            .find_map(|(candidate_left, candidate_right, value)| {
                                (candidate_left == &left && candidate_right == &right)
                                    .then_some(value)
                            })
                            .unwrap_or(0.0),
                    ]
                })
                .collect::<Vec<_>>();
            let value = model.interpolate(&values, location)?[0];
            font_metadata
                .set_kerning_pair(left, right, Some(value))
                .map_err(|error| error.to_string())?;
        }

        let source_info = self
            .document_sources()
            .map(|source| {
                self.document_font_info(source.id())
                    .cloned()
                    .ok_or_else(|| format!("missing font info for source {}", source.id().0))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let metrics = source_info
            .iter()
            .map(|info| {
                let resolved = info.metrics.resolved();
                vec![
                    resolved.ascender,
                    resolved.descender,
                    resolved.x_height,
                    resolved.cap_height,
                ]
            })
            .collect::<Vec<_>>();
        let metrics = model.interpolate(&metrics, location)?;
        let mut font_info = self
            .document_font_info(default_source)
            .cloned()
            .ok_or("missing default source font info")?;
        font_info.names.style_name = Some(name.to_owned());
        font_info.metrics.ascender = Some(metrics[0]);
        font_info.metrics.descender = Some(metrics[1]);
        font_info.metrics.x_height = Some(metrics[2]);
        font_info.metrics.cap_height = Some(metrics[3]);
        font_info.validate().map_err(|error| error.to_string())?;
        Ok((feature_text, font_metadata, font_info))
    }

    /// Whether a structural source/layer operation is available to undo or redo.
    pub fn has_source_history(&self, redo: bool) -> bool {
        let direction = if redo {
            HistoryDirection::Redo
        } else {
            HistoryDirection::Undo
        };
        self.source_history.transactions.can_replay(direction)
    }

    /// Undo or redo one source/layer transaction, preserving later unrelated edits.
    pub fn undo_sources(&mut self, redo: bool) -> Result<bool, String> {
        let direction = if redo {
            HistoryDirection::Redo
        } else {
            HistoryDirection::Undo
        };
        let current = SourceFrame::capture(self);
        let mut history = std::mem::take(&mut self.source_history);
        let SourceHistory {
            transactions,
            retired_histories,
            retired_layer_histories,
        } = &mut history;
        let replayed = transactions.replay(&current, direction, |expected, replacement| {
            replacement.restore_if_current(
                self,
                expected,
                retired_histories,
                retired_layer_histories,
            )
        });
        self.source_history = history;
        match replayed {
            Ok(HistoryReplayOutcome::Empty) => Ok(false),
            Ok(HistoryReplayOutcome::Applied) => Ok(true),
            Err(HistoryReplayError::Stale | HistoryReplayError::Apply(_)) => {
                Err("Undo later glyph or metadata edits before this source/layer change".into())
            }
        }
    }

    /// Add a complete source interpolated at normalized design coordinates.
    /// `filename` is a new UFO path relative to the Designspace file.
    pub fn add_interpolated_source(
        &mut self,
        name: &str,
        filename: &str,
        location: &Location,
    ) -> Result<SourceId, String> {
        let designspace = self
            .begin_source_designspace_edit()
            .ok_or("adding a source requires a Designspace")?;
        if name.trim().is_empty() {
            return Err("source name must not be empty".into());
        }
        let path = Path::new(filename);
        if path.extension().is_none_or(|extension| extension != "ufo")
            || !path
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
        {
            return Err("choose a relative .ufo filename inside the project directory".into());
        }
        if designspace
            .sources()
            .iter()
            .any(|source| source.filename.eq_ignore_ascii_case(filename))
        {
            return Err("that UFO destination is already used".into());
        }
        let directory = self
            .export_source
            .as_deref()
            .and_then(Path::parent)
            .or_else(|| self.masters[0].source_path.parent())
            .unwrap_or(Path::new("."));
        let destination = directory.join(filename);
        if destination.exists() {
            return Err("the new source destination already exists".into());
        }
        let exact_location = CanonicalLocation::from_normalized(location, designspace.axes())?;
        let location = exact_location.to_normalized(designspace.axes())?;
        let mut locations = self.master_locations.clone();
        locations.push(location.clone());
        VariationModel::new(&locations)?;
        let default = self
            .master_locations
            .iter()
            .position(|l| l.values().all(|v| *v == 0.0))
            .ok_or("missing default source")?;
        let default_source = self
            .source_id(default)
            .ok_or("missing default source identity")?;
        let default_layer = self
            .document_source(default_source)
            .ok_or("missing default source")?
            .default_layer();
        let id = SourceId(self.variable.next_source);
        let target_layer = LayerId {
            source: id,
            name: default_layer.name.clone(),
        };
        let glyph_names = self.glyph_names().map(str::to_owned).collect::<Vec<_>>();
        let mut layers = Vec::new();
        for glyph in glyph_names {
            if self.glyph_sources(&glyph)?.is_empty() {
                continue;
            }
            let interpolated = self.try_interpolated_layer_at(&glyph, &location)?;
            let base = self
                .capture_document_layer(&GlyphLayerAddress {
                    glyph: glyph.clone(),
                    layer: default_layer.clone(),
                })
                .ok_or_else(|| format!("missing default layer for {glyph}"))?;
            layers.push(source_builder::interpolated_source_layer(
                base,
                &interpolated,
                GlyphLayerAddress {
                    glyph,
                    layer: target_layer.clone(),
                },
            )?);
        }
        let (feature_text, font_metadata, font_info) =
            self.interpolated_source_metadata(default_source, name, &location)?;
        let mut descriptor =
            SourceDescriptor::new(id, filename.to_owned(), exact_location, target_layer)?;
        descriptor.name = Some(format!("source-{}", id.0));
        descriptor.style_name = Some(name.to_owned());
        let mut replacement = designspace.clone();
        let display_index = replacement.sources().len();
        // A full source at an intermediate location takes over participation.
        // Keep the original sparse layer and its metadata as an auxiliary layer.
        replacement.edit_checked(|draft| {
            draft.remove_sparse_at_normalized_location(&location)?;
            draft.insert_source(descriptor, display_index)
        })?;
        let designspace_projection = replacement.to_norad()?;
        let before = SourceFrame::capture(self);
        let mut canonical = before.canonical.clone();
        canonical.add_interpolated_source(
            id,
            default_source,
            replacement,
            feature_text,
            font_metadata,
            font_info,
            layers,
        )?;
        self.variable
            .restore_source_structure_if_current(&before.canonical, canonical)
            .map_err(|_| "canonical source structure changed while adding a source")?;
        let font = self
            .variable
            .source_font(id)
            .expect("the committed canonical source must remain projectable");
        self.ds_doc = Some(designspace_projection);
        self.ds_dirty = true;
        self.brace.retain(|source| source.location != location);
        self.masters.push(Master::from_font(font, destination));
        self.master_names.push(name.into());
        self.master_locations.push(location);
        self.active = self.masters.len() - 1;
        self.record_canonical_source_change(before);
        Ok(id)
    }

    /// Remove a non-default source and its layer-source descriptors.
    /// Its on-disk UFO is retained; Undo restores the in-memory source.
    pub fn remove_source(&mut self, id: SourceId) -> Result<(), String> {
        let index = self.source_index(id).ok_or("unknown source")?;
        if self.masters.len() == 1
            || self
                .master_locations
                .get(index)
                .is_none_or(|l| l.values().all(|v| *v == 0.0))
        {
            return Err("the default source must remain in the project".into());
        }
        let designspace = self
            .begin_source_designspace_edit()
            .ok_or("not a Designspace")?;
        let mut replacement = designspace.clone();
        replacement.edit_checked(|draft| {
            draft.remove_source(id).ok_or("missing source descriptor")?;
            Ok(())
        })?;
        let before = SourceFrame::capture(self);
        let removed_id = self.variable.source_ids.remove(index);
        if let Err(error) = self.install_source_designspace_edit(&designspace, replacement) {
            self.variable.source_ids.insert(index, removed_id);
            return Err(error);
        }
        let mut removed = self.masters.remove(index);
        self.source_history
            .retired_histories
            .insert(id, std::mem::take(&mut removed.history));
        park_compatibility_layer_histories(
            &mut self.variable,
            id,
            &mut self.source_history.retired_layer_histories,
        );
        self.master_names.remove(index);
        self.master_locations.remove(index);
        self.brace.retain_mut(|source| {
            if source.master == index {
                return false;
            }
            if source.master > index {
                source.master -= 1;
            }
            true
        });
        self.active = if self.active > index {
            self.active - 1
        } else if self.active == index {
            0
        } else {
            self.active
        };
        self.record_source_change(before);
        Ok(())
    }

    /// Move a source to a display position without changing any source identity.
    pub fn move_source(&mut self, id: SourceId, to: usize) -> Result<bool, String> {
        let from = self.source_index(id).ok_or("unknown source")?;
        if to >= self.masters.len() {
            return Err("source position is outside the source list".into());
        }
        if from == to {
            return Ok(false);
        }
        let designspace = self
            .begin_source_designspace_edit()
            .ok_or("not a Designspace")?;
        let mut replacement = designspace.clone();
        replacement.edit_checked(|draft| {
            draft.move_source(id, to)?;
            Ok(())
        })?;
        let before = SourceFrame::capture(self);
        let mut order: Vec<_> = (0..self.masters.len()).collect();
        let previous = order.remove(from);
        order.insert(to, previous);
        let moved_id = self.variable.source_ids.remove(from);
        self.variable.source_ids.insert(to, moved_id);
        if let Err(error) = self.install_source_designspace_edit(&designspace, replacement) {
            let moved_id = self.variable.source_ids.remove(to);
            self.variable.source_ids.insert(from, moved_id);
            return Err(error);
        }
        self.active = order
            .iter()
            .position(|index| *index == self.active)
            .expect("permutation");
        for source in &mut self.brace {
            source.master = order
                .iter()
                .position(|index| *index == source.master)
                .expect("permutation");
        }
        let master = self.masters.remove(from);
        self.masters.insert(to, master);
        let name = self.master_names.remove(from);
        self.master_names.insert(to, name);
        let location = self.master_locations.remove(from);
        self.master_locations.insert(to, location);
        self.record_source_change(before);
        Ok(true)
    }

    /// Rename a source and update its normalized location as one transaction.
    pub fn update_source(
        &mut self,
        id: SourceId,
        name: &str,
        location: &Location,
    ) -> Result<(), String> {
        let index = self.source_index(id).ok_or("unknown source")?;
        if name.trim().is_empty() {
            return Err("source name must not be empty".into());
        }
        let designspace = self
            .begin_source_designspace_edit()
            .ok_or("not a Designspace")?;
        let exact_location = CanonicalLocation::from_normalized(location, designspace.axes())?;
        let location = exact_location.to_normalized(designspace.axes())?;
        let mut locations = self.master_locations.clone();
        locations[index] = location.clone();
        VariationModel::new(&locations)?;
        for source in designspace.sparse_sources() {
            if source.location.to_normalized(designspace.axes())? == location {
                return Err("This location has an intermediate layer source; add an interpolated source there to promote it first".into());
            }
        }
        let before = SourceFrame::capture(self);
        let mut replacement = designspace.clone();
        replacement.edit_checked(|draft| {
            let source = draft.source_mut(id).ok_or("missing source descriptor")?;
            source.style_name = Some(name.to_owned());
            source.location = exact_location;
            Ok(())
        })?;
        self.install_source_designspace_edit(&designspace, replacement)?;
        self.master_names[index] = name.into();
        self.master_locations[index] = location;
        self.masters[index].font.font_info.style_name = Some(name.into());
        self.record_source_change(before);
        Ok(())
    }

    /// Register an existing auxiliary layer as a sparse interpolation source.
    pub fn add_sparse_source(
        &mut self,
        layer: &LayerId,
        location: &Location,
    ) -> Result<(), String> {
        let source_index = self.source_index(layer.source).ok_or("unknown source")?;
        let source = self.document_source(layer.source).ok_or("unknown source")?;
        if &source.default_layer() == layer {
            return Err("the default layer is already a full source".into());
        }
        if !self.variable.has_layer(layer) {
            return Err("the sparse source layer has no canonical glyphs".into());
        }
        let designspace = self
            .begin_source_designspace_edit()
            .ok_or("not a Designspace")?;
        let exact = CanonicalLocation::from_normalized(location, designspace.axes())?;
        let owner = designspace
            .source_order()
            .iter()
            .position(|entry| *entry == SourceOrderEntry::Full(layer.source))
            .ok_or("sparse source owner is absent from source order")?;
        let mut order_index = owner + 1;
        while designspace.source_order().get(order_index).is_some_and(|entry| {
            matches!(entry, SourceOrderEntry::Sparse(candidate) if candidate.source == layer.source)
        }) {
            order_index += 1;
        }
        let mut replacement = designspace.clone();
        replacement.edit_checked(|draft| {
            draft.insert_sparse_source(
                SparseSourceDescriptor::new(layer.clone(), exact),
                order_index,
            )
        })?;
        let before = SourceFrame::capture(self);
        self.install_source_designspace_edit(&designspace, replacement)?;
        self.brace.push(BraceSource {
            master: source_index,
            layer: layer.name.clone(),
            location: location.clone(),
        });
        self.record_canonical_source_change(before);
        Ok(())
    }

    /// Remove one named instance from the canonical Designspace.
    pub fn remove_instance(&mut self, id: InstanceId) -> Result<bool, String> {
        let designspace = self
            .begin_source_designspace_edit()
            .ok_or("not a Designspace")?;
        if !designspace
            .instances()
            .iter()
            .any(|instance| instance.id() == id)
        {
            return Ok(false);
        }
        let mut replacement = designspace.clone();
        replacement.edit_checked(|draft| {
            draft.remove_instance(id).ok_or("missing instance")?;
            Ok(())
        })?;
        let before = SourceFrame::capture(self);
        self.install_source_designspace_edit(&designspace, replacement)?;
        self.refresh_instances_from_doc();
        self.record_canonical_source_change(before);
        Ok(true)
    }

    /// Read this source's conventional background glyph layer without a UFO projection.
    pub fn document_background_layer(
        &self,
        glyph: &str,
        source: SourceId,
    ) -> Option<(LayerId, LayerView<'_>)> {
        let layer = self.variable.background_layer_id(source)?;
        let view = self.document_layer(glyph, &layer)?;
        Some((layer, view))
    }

    /// Copy one foreground layer's contours and exact width into the canonical background.
    ///
    /// The conventional background layer and glyph are created when absent.
    /// An identical copy is a no-op that retains existing stable identities and redo history.
    pub fn copy_document_layer_to_background(
        &mut self,
        address: &GlyphLayerAddress,
    ) -> Result<bool, String> {
        let before = SourceFrame::capture(self);
        let base = self.variable.source_structure_snapshot();
        let mut replacement = base.clone();
        let (_, changed) = replacement.copy_layer_to_background(&address.glyph, &address.layer)?;
        if !changed {
            return Ok(false);
        }
        self.commit_background_structure(before, base, replacement, address)
    }

    /// Atomically exchange foreground and background contours.
    ///
    /// The foreground retains its width and non-outline metadata. The background receives the
    /// foreground width, matching the existing editor command while retaining its other metadata.
    pub fn swap_document_layer_with_background(
        &mut self,
        address: &GlyphLayerAddress,
    ) -> Result<bool, String> {
        let before = SourceFrame::capture(self);
        let base = self.variable.source_structure_snapshot();
        let mut replacement = base.clone();
        let (_, changed) =
            replacement.swap_layer_with_background(&address.glyph, &address.layer)?;
        if !changed {
            return Ok(false);
        }
        self.commit_background_structure(before, base, replacement, address)
    }

    /// Remove this glyph from its canonical background layer, retaining the layer container.
    pub fn clear_document_background(
        &mut self,
        glyph: &str,
        source: SourceId,
    ) -> Result<bool, String> {
        let before = SourceFrame::capture(self);
        let base = self.variable.source_structure_snapshot();
        let mut replacement = base.clone();
        let (_, changed) = replacement.clear_background_layer(glyph, source)?;
        if !changed {
            return Ok(false);
        }
        let address = GlyphLayerAddress {
            glyph: glyph.into(),
            layer: self
                .variable
                .background_layer_id(source)
                .ok_or("unknown source")?,
        };
        self.commit_background_structure(before, base, replacement, &address)
    }

    fn commit_background_structure(
        &mut self,
        before: SourceFrame,
        base: CanonicalSourceStructureSnapshot,
        replacement: CanonicalSourceStructureSnapshot,
        address: &GlyphLayerAddress,
    ) -> Result<bool, String> {
        let changed = self
            .variable
            .restore_source_structure_if_current(&base, replacement)
            .map_err(|_| "canonical source structure changed after background staging")?;
        if !changed {
            return Ok(false);
        }
        let index = self
            .source_index(address.layer.source)
            .ok_or("background source disappeared")?;
        self.masters[index].font = self
            .variable
            .source_font(address.layer.source)
            .ok_or("committed background source is not projectable")?;
        self.masters[index].dirty = true;
        self.masters[index]
            .modified_glyphs
            .insert(address.glyph.clone());
        self.record_canonical_source_change(before);
        Ok(true)
    }

    /// Duplicate one glyph into an auxiliary UFO layer, creating that layer if needed.
    pub fn add_glyph_layer(
        &mut self,
        glyph: &str,
        from: &LayerId,
        name: &str,
    ) -> Result<LayerId, String> {
        let index = self.source_index(from.source).ok_or("unknown source")?;
        if self.document_layer(glyph, from).is_none() {
            return Err("missing source glyph layer".into());
        }
        let target_id = LayerId {
            source: from.source,
            name: name.into(),
        };
        if self.document_layer(glyph, &target_id).is_some()
            || self.masters[index]
                .font
                .layers
                .get(name)
                .is_some_and(|layer| layer.contains_glyph(glyph))
        {
            return Err("the glyph already has that layer".into());
        }
        let before = SourceFrame::capture(self);
        self.masters[index]
            .font
            .layers
            .get_or_create_layer(name)
            .map_err(|error| error.to_string())?;
        assert!(
            self.variable.copy_layer(glyph, from, &target_id),
            "validated source layer must remain copyable"
        );
        let payload = self
            .variable
            .project_layer(glyph, &target_id)
            .expect("copied layer must be projectable");
        self.masters[index]
            .font
            .layers
            .get_mut(name)
            .expect("created compatibility layer")
            .insert_glyph(payload);
        self.record_source_change(before);
        Ok(target_id)
    }

    /// Remove one auxiliary glyph layer, retaining the layer and other glyphs.
    pub fn remove_glyph_layer(&mut self, glyph: &str, id: &LayerId) -> Result<(), String> {
        let index = self.source_index(id.source).ok_or("unknown source")?;
        if self.masters[index].font.default_layer().name().as_str() == id.name {
            return Err("remove the source to remove a default layer".into());
        }
        if self.glyph_layer(glyph, id).is_none() {
            return Err("missing glyph layer".into());
        }
        let before = SourceFrame::capture(self);
        assert!(
            self.variable.remove_layer(glyph, id),
            "validated canonical layer must remain removable"
        );
        self.masters[index]
            .font
            .layers
            .get_mut(&id.name)
            .expect("validated layer")
            .remove_glyph(glyph);
        self.record_source_change(before);
        Ok(())
    }

    /// Remove an empty auxiliary layer container from canonical persistence and its projection.
    ///
    /// A default layer or a layer that still contains any glyph is rejected without mutation.
    pub fn remove_empty_auxiliary_layer(&mut self, id: &LayerId) -> Result<bool, String> {
        let index = self.source_index(id.source).ok_or("unknown source")?;
        if self.masters[index].font.default_layer().name().as_str() == id.name {
            return Err("the default layer cannot be removed".into());
        }
        let Some(layer) = self.masters[index].font.layers.get(&id.name) else {
            return Ok(false);
        };
        if !layer.is_empty() || self.variable.has_layer(id) {
            return Err("the auxiliary layer still contains glyphs".into());
        }
        let before = SourceFrame::capture(self);
        if !self.variable.remove_empty_layer_container(id) {
            return Err("canonical auxiliary layer container is inconsistent".into());
        }
        let removed = self.masters[index].font.layers.remove(&id.name).is_some();
        debug_assert!(
            removed,
            "validated compatibility layer container must remain removable"
        );
        self.record_source_change(before);
        Ok(true)
    }
}
