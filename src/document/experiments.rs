// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Isolated canonical document versions with guarded, selective application to the root.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::canonical_metadata::CanonicalFontMetadata;
use super::edit_batch::{EditBatch, canonical_glyph_revision, proposal_draft, validate_batch};
use super::project::{DocumentEditOutcome, Project};
use super::proposal::{
    Installed, ProposalError, ProposalSummary, compatible_layers, install_replacement, layer_name,
    task_of_layer,
};
use super::variable::{GlyphLayerAddress, LayerId, SourceId};
use super::{CanonicalLayerSnapshot, DocumentEditError, LayerEditDraft, LayerPointType, LayerView};

#[derive(Clone, Debug)]
struct VersionState {
    layers: BTreeMap<GlyphLayerAddress, LayerEditDraft>,
    font_metadata: CanonicalFontMetadata,
}

impl VersionState {
    fn capture(project: &Project, source: SourceId) -> Result<(LayerId, Self), String> {
        let default_layer = project
            .document_source(source)
            .ok_or("unknown source")?
            .default_layer();
        let names: Vec<_> = project.glyph_names().map(str::to_owned).collect();
        let mut layers = BTreeMap::new();
        for glyph in names {
            let ids: Vec<_> = project
                .document_glyph(&glyph)
                .into_iter()
                .flat_map(|glyph| glyph.layer_ids())
                .filter(|layer| layer.source == source)
                .cloned()
                .collect();
            for layer in ids {
                let address = GlyphLayerAddress {
                    glyph: glyph.clone(),
                    layer,
                };
                let snapshot = project
                    .capture_document_layer(&address)
                    .ok_or("canonical layer disappeared while capturing version")?;
                let (layer, preserved) = snapshot.into_parts();
                layers.insert(address, LayerEditDraft::new(layer, preserved));
            }
        }
        let font_metadata = project
            .document_font_metadata(source)
            .ok_or("unknown source metadata")?
            .clone();
        Ok((
            default_layer,
            Self {
                layers,
                font_metadata,
            },
        ))
    }

    fn snapshot(&self, address: &GlyphLayerAddress) -> Option<CanonicalLayerSnapshot> {
        let (layer, preserved) = self.layers.get(address)?.clone().into_parts();
        Some(CanonicalLayerSnapshot::new(
            address.clone(),
            layer,
            preserved,
        ))
    }

    fn view(&self, address: &GlyphLayerAddress) -> Option<LayerView<'_>> {
        Some(self.layers.get(address)?.view())
    }

    fn edit_layer(
        &mut self,
        address: &GlyphLayerAddress,
        edit: impl FnOnce(&mut LayerEditDraft) -> Result<(), DocumentEditError>,
    ) -> Result<bool, DocumentEditError> {
        let current = self
            .layers
            .get(address)
            .ok_or(DocumentEditError::MissingLayer)?;
        let mut staged = current.clone();
        edit(&mut staged)?;
        let (before_layer, before_preserved) = current.clone().into_parts();
        let (after_layer, after_preserved) = staged.clone().into_parts();
        let before = CanonicalLayerSnapshot::new(address.clone(), before_layer, before_preserved);
        let after = CanonicalLayerSnapshot::new(address.clone(), after_layer, after_preserved);
        if before == after {
            return Ok(false);
        }
        self.layers.insert(address.clone(), staged);
        Ok(true)
    }

    fn insert_layer(
        &mut self,
        address: GlyphLayerAddress,
        draft: LayerEditDraft,
    ) -> Result<(), String> {
        if self.layers.contains_key(&address) {
            return Err("proposal task already exists; use a new task name".into());
        }
        self.layers.insert(address, draft);
        Ok(())
    }

    fn remove_layer(&mut self, address: &GlyphLayerAddress) -> bool {
        self.layers.remove(address).is_some()
    }
}

/// One in-memory experimental version of a stable source.
///
/// Versions exist only for the current document session and never participate in normal Save.
#[derive(Debug)]
pub struct Experiment {
    /// Stable root source identity.
    pub root: SourceId,
    /// Optional parent experiment, retained as provenance.
    pub parent: Option<String>,
    /// Brief or model information supplied by the caller.
    pub reason: String,
    /// Recent branch operations, including caller-supplied design intent.
    pub events: Vec<Value>,
    default_layer: LayerId,
    base: VersionState,
    working: VersionState,
    revision: u64,
}

impl Experiment {
    /// Current session-local revision of this isolated version.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Default layer address for one glyph in this version.
    pub fn default_address(&self, glyph: &str) -> GlyphLayerAddress {
        GlyphLayerAddress {
            glyph: glyph.to_owned(),
            layer: self.default_layer.clone(),
        }
    }

    /// Read one isolated canonical layer.
    pub fn layer(&self, address: &GlyphLayerAddress) -> Option<LayerView<'_>> {
        self.working.view(address)
    }

    /// Edit one isolated canonical layer atomically.
    pub fn edit_layer(
        &mut self,
        address: &GlyphLayerAddress,
        edit: impl FnOnce(&mut LayerEditDraft) -> Result<(), DocumentEditError>,
    ) -> Result<bool, DocumentEditError> {
        if address.layer.source != self.root {
            return Err(DocumentEditError::MissingSource);
        }
        let changed = self.working.edit_layer(address, edit)?;
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        Ok(changed)
    }

    /// Current canonical kerning and group metadata for this version.
    pub fn font_metadata(&self) -> &CanonicalFontMetadata {
        &self.working.font_metadata
    }

    /// Replace this version's canonical kerning and group metadata.
    pub fn set_font_metadata(&mut self, metadata: CanonicalFontMetadata) -> bool {
        if self.working.font_metadata == metadata {
            return false;
        }
        self.working.font_metadata = metadata;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// Glyphs whose default canonical layer differs from the shared root baseline.
    pub fn changed_glyphs(&self) -> Vec<String> {
        let names: BTreeSet<_> = self
            .base
            .layers
            .keys()
            .chain(self.working.layers.keys())
            .filter(|address| address.layer == self.default_layer)
            .map(|address| address.glyph.clone())
            .collect();
        names
            .into_iter()
            .filter(|glyph| {
                let address = self.default_address(glyph);
                self.base.snapshot(&address) != self.working.snapshot(&address)
            })
            .collect()
    }

    /// Materialize this isolated version at an explicit UFO export or proof boundary.
    ///
    /// The returned font is transient. Canonical layer drafts and metadata remain the version's
    /// only persistent editing state.
    pub fn source_snapshot(&self, project: &Project) -> Result<norad::Font, String> {
        let mut font = project
            .source_snapshot(self.root)
            .ok_or("the experiment's source is no longer loaded")?;
        for (address, draft) in &self.working.layers {
            let layer = font
                .layers
                .get_or_create_layer(&address.layer.name)
                .map_err(|error| error.to_string())?;
            layer.insert_glyph(draft.view().project());
        }
        super::font_ops::write_canonical_metadata_to_ufo(&mut font, &self.working.font_metadata)
            .map_err(|error| error.to_string())?;
        Ok(font)
    }

    fn proposal_layer(&self, task: &str) -> LayerId {
        LayerId {
            source: self.root,
            name: layer_name(task),
        }
    }

    fn summarize_proposal(&self, task: &str) -> Result<ProposalSummary, ProposalError> {
        let layer = self.proposal_layer(task);
        let mut summary = ProposalSummary {
            task: task.to_owned(),
            layer: layer.name.clone(),
            glyphs: Vec::new(),
            compatible: Vec::new(),
            incompatible: Vec::new(),
            missing: Vec::new(),
        };
        for address in self
            .working
            .layers
            .keys()
            .filter(|address| address.layer == layer)
        {
            summary.glyphs.push(address.glyph.clone());
            let foreground_address = self.default_address(&address.glyph);
            let Some(foreground) = self.working.view(&foreground_address) else {
                summary.missing.push(address.glyph.clone());
                continue;
            };
            let proposed = self
                .working
                .view(address)
                .expect("iterated canonical proposal layer");
            if compatible_layers(foreground, proposed) {
                summary.compatible.push(address.glyph.clone());
            } else {
                let shape = |layer: LayerView<'_>| {
                    let contours = layer.contours().count();
                    let points = layer
                        .contours()
                        .map(|contour| contour.points().count())
                        .sum::<usize>();
                    format!("{contours}c · {points}pt")
                };
                summary.incompatible.push((
                    address.glyph.clone(),
                    format!(
                        "foreground {} · proposed {}",
                        shape(foreground),
                        shape(proposed)
                    ),
                ));
            }
        }
        if summary.glyphs.is_empty() {
            return Err(ProposalError::NoProposal {
                task: task.to_owned(),
            });
        }
        Ok(summary)
    }

    /// Every canonical proposal held by this isolated version.
    pub fn proposals(&self) -> Vec<ProposalSummary> {
        let tasks: BTreeSet<_> = self
            .working
            .layers
            .keys()
            .filter_map(|address| task_of_layer(&address.layer.name))
            .map(str::to_owned)
            .collect();
        tasks
            .into_iter()
            .filter_map(|task| self.summarize_proposal(&task).ok())
            .collect()
    }

    /// Validate and create one proposal without mutating this version's foreground.
    pub fn propose(&mut self, batch: &EditBatch) -> Result<ProposalSummary, String> {
        validate_batch(batch)?;
        if self.summarize_proposal(&batch.task).is_ok() {
            return Err("proposal task already exists; use a new task name".into());
        }
        let layer = self.proposal_layer(&batch.task);
        let mut staged = Vec::with_capacity(batch.edits.len());
        for edit in &batch.edits {
            let foreground = self.default_address(&edit.glyph);
            let draft = self
                .working
                .layers
                .get(&foreground)
                .cloned()
                .ok_or_else(|| format!("no glyph named {}", edit.glyph))?;
            let address = GlyphLayerAddress {
                glyph: edit.glyph.clone(),
                layer: layer.clone(),
            };
            staged.push((address, proposal_draft(draft, &layer, edit, &batch.reason)?));
        }
        for (address, draft) in staged {
            self.working.insert_layer(address, draft)?;
        }
        self.revision = self.revision.wrapping_add(1);
        self.summarize_proposal(&batch.task)
            .map_err(|error| error.to_string())
    }

    /// Install selected proposal glyphs into this isolated version after revision checks.
    pub fn install_proposal(
        &mut self,
        task: &str,
        only: Option<&[String]>,
        keep_structure: bool,
    ) -> Result<Installed, ProposalError> {
        let summary = self.summarize_proposal(task)?;
        let proposal_layer = self.proposal_layer(task);
        let wanted = |name: &str| only.is_none_or(|list| list.iter().any(|item| item == name));
        let mut staged = Vec::new();
        let mut skipped = Vec::new();
        for name in summary.glyphs.iter().filter(|name| wanted(name)) {
            let foreground_address = self.default_address(name);
            let proposal_address = GlyphLayerAddress {
                glyph: name.clone(),
                layer: proposal_layer.clone(),
            };
            let Some(foreground) = self.working.view(&foreground_address) else {
                skipped.push((name.clone(), "not in the font".to_owned()));
                continue;
            };
            let proposed = self
                .working
                .view(&proposal_address)
                .expect("summarized proposal layer");
            let proposed_contract = proposed.project();
            let Some(base) = crate::formats::lib_keys::read_proposal_base(&proposed_contract)
            else {
                skipped.push((
                    name.clone(),
                    "unguarded proposal: missing foreground revision; propose again".into(),
                ));
                continue;
            };
            if canonical_glyph_revision(foreground).ok().as_deref() != Some(base) {
                skipped.push((
                    name.clone(),
                    "stale proposal: foreground changed; propose again".into(),
                ));
                continue;
            }
            if keep_structure && !compatible_layers(foreground, proposed) {
                let reason = summary
                    .incompatible
                    .iter()
                    .find(|(glyph, _)| glyph == name)
                    .map(|(_, reason)| reason.clone())
                    .unwrap_or_else(|| "point structure changed".into());
                skipped.push((name.clone(), reason));
                continue;
            }
            let before = self
                .working
                .snapshot(&foreground_address)
                .expect("foreground snapshot");
            let replacement =
                install_replacement(&foreground_address, foreground, proposed, &before);
            staged.push((foreground_address, proposal_address, replacement));
        }

        let mut installed = Vec::new();
        for (foreground, proposal, replacement) in staged {
            let (layer, preserved) = replacement.into_parts();
            self.working
                .layers
                .insert(foreground.clone(), LayerEditDraft::new(layer, preserved));
            self.working.remove_layer(&proposal);
            installed.push(foreground.glyph);
        }
        if !installed.is_empty() {
            self.revision = self.revision.wrapping_add(1);
        }
        let layer_removed = self.summarize_proposal(task).is_err();
        Ok(Installed {
            task: task.to_owned(),
            installed,
            skipped,
            layer_removed,
        })
    }

    /// Discard one proposal without changing this version's foreground.
    pub fn discard_proposal(&mut self, task: &str) -> Result<usize, ProposalError> {
        let summary = self.summarize_proposal(task)?;
        let layer = self.proposal_layer(task);
        for glyph in &summary.glyphs {
            self.working.remove_layer(&GlyphLayerAddress {
                glyph: glyph.clone(),
                layer: layer.clone(),
            });
        }
        self.revision = self.revision.wrapping_add(1);
        Ok(summary.glyphs.len())
    }
}

/// Experiments live for the document session; root applications can be explicitly undone.
#[derive(Debug, Default)]
pub struct Experiments {
    /// Named experiments in stable order.
    pub versions: BTreeMap<String, Experiment>,
    /// Last requested proof scene per stable source/version, shared with the node preview.
    pub proofs: BTreeMap<String, Value>,
    applied: Vec<Applied>,
}

#[derive(Debug)]
struct Applied {
    root: SourceId,
    layers: Vec<(CanonicalLayerSnapshot, CanonicalLayerSnapshot)>,
    font_metadata: Option<(CanonicalFontMetadata, CanonicalFontMetadata)>,
}

/// Revision for canonical kerning and group membership.
pub fn kerning_revision(metadata: &CanonicalFontMetadata) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(metadata.groups(), metadata.raw_kerning()))
        .map_err(|error| error.to_string())?;
    Ok(format!("kerning-sha256:{:x}", Sha256::digest(bytes)))
}

/// List session versions and their changes from the common root baseline.
pub fn list(project: &Project) -> Value {
    json!({"ok":true,"session_only":true,"versions":project.experiments.versions.iter().map(|(name,version)| {
        json!({
            "name":name,
            "source":version.root.0,
            "parent":version.parent,
            "reason":version.reason,
            "events":version.events,
            "revision":version.revision,
            "changed_glyphs":version.changed_glyphs(),
            "kerning_changed":version.working.font_metadata != version.base.font_metadata,
        })
    }).collect::<Vec<_>>()})
}

/// Fork a stable source or an existing experiment without touching the root.
pub fn fork(
    project: &mut Project,
    root: SourceId,
    name: &str,
    parent: Option<&str>,
    reason: &str,
) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
    {
        return Err("name must use 1 to 64 ASCII letters, digits, hyphens or underscores".into());
    }
    if project.experiments.versions.len() >= 16 || project.experiments.versions.contains_key(name) {
        return Err("experiment already exists or the 16-version limit was reached".into());
    }
    if project.document_source(root).is_none() {
        return Err("unknown source".into());
    }
    let (default_layer, base, working) = match parent {
        Some(parent) => {
            let version = project
                .experiments
                .versions
                .get(parent)
                .ok_or("unknown parent")?;
            if version.root != root {
                return Err("parent belongs to another source".into());
            }
            (
                version.default_layer.clone(),
                version.base.clone(),
                version.working.clone(),
            )
        }
        None => {
            let (default_layer, state) = VersionState::capture(project, root)?;
            (default_layer, state.clone(), state)
        }
    };
    project.experiments.versions.insert(
        name.into(),
        Experiment {
            root,
            parent: parent.map(str::to_owned),
            reason: reason.into(),
            events: Vec::new(),
            default_layer,
            base,
            working,
            revision: 0,
        },
    );
    Ok(())
}

/// Edit one named version layer without borrowing or mutating the root document.
pub fn edit_layer(
    project: &mut Project,
    name: &str,
    address: &GlyphLayerAddress,
    edit: impl FnOnce(&mut LayerEditDraft) -> Result<(), DocumentEditError>,
) -> Result<bool, String> {
    project
        .experiments
        .versions
        .get_mut(name)
        .ok_or_else(|| "unknown experiment".to_owned())?
        .edit_layer(address, edit)
        .map_err(|error| error.to_string())
}

/// Apply selected canonical glyph layers and optional kerning after checking every conflict.
///
/// Unrelated root edits survive. All conflicts are checked before the first root mutation, and
/// each changed glyph enters Project-owned layer history. The version remains available.
pub fn apply(
    project: &mut Project,
    name: &str,
    names: &[String],
    kerning: bool,
    keep_structure: bool,
) -> Result<Vec<String>, String> {
    let (root, mut changes, metadata_change) = {
        let version = project
            .experiments
            .versions
            .get(name)
            .ok_or("unknown experiment")?;
        if project.document_source(version.root).is_none() {
            return Err("the experiment's source is no longer loaded".into());
        }
        let mut seen = HashSet::new();
        let mut changes = Vec::new();
        for glyph in names {
            if !seen.insert(glyph) {
                return Err("duplicate glyph".into());
            }
            let address = version.default_address(glyph);
            let base = version
                .base
                .snapshot(&address)
                .ok_or("glyph absent from base")?;
            let after = version
                .working
                .snapshot(&address)
                .ok_or("glyph absent from experiment")?;
            if after == base {
                continue;
            }
            let before = project
                .capture_document_layer(&address)
                .ok_or("glyph absent from root")?;
            if before != base {
                return Err(format!("conflict: {glyph} changed in the root"));
            }
            if keep_structure && !compatible_snapshots(&before, &after) {
                return Err(format!("{glyph}: point structure changed"));
            }
            changes.push((before, after));
        }
        let metadata_change =
            if kerning && version.working.font_metadata != version.base.font_metadata {
                let current = project
                    .document_font_metadata(version.root)
                    .ok_or("the experiment's source is no longer loaded")?
                    .clone();
                if current != version.base.font_metadata {
                    return Err("conflict: root kerning or groups changed".into());
                }
                Some((current, version.working.font_metadata.clone()))
            } else {
                None
            };
        (version.root, changes, metadata_change)
    };
    if changes.is_empty() && metadata_change.is_none() {
        return Err("no selected changes to apply".into());
    }

    for (before, after) in &changes {
        let address = before.address().clone();
        match project
            .restore_document_layer_if_current(&address, before, after.clone())
            .map_err(|error| error.to_string())?
        {
            DocumentEditOutcome::Changed { .. } => {
                project
                    .record_document_layer_history(&address, before.clone())
                    .map_err(|error| error.to_string())?;
            }
            DocumentEditOutcome::Unchanged { .. } => {
                return Err("experiment application unexpectedly produced no change".into());
            }
        }
    }
    if let Some((before, after)) = &metadata_change {
        let history = project.begin_document_source_metadata_history();
        let outcome = project
            .edit_document_source_metadata(root, |draft| {
                draft.set_font_metadata(after.clone());
                Ok(())
            })
            .map_err(|error| error.to_string())?;
        if !matches!(outcome, DocumentEditOutcome::Changed { .. }) {
            return Err("experiment metadata application unexpectedly produced no change".into());
        }
        if !project.record_document_source_metadata_history(history) {
            return Err("experiment metadata history was not recorded".into());
        }
        debug_assert_eq!(
            project.document_font_metadata(root),
            Some(after),
            "successful experiment metadata apply must publish the staged canonical value"
        );
        debug_assert_ne!(
            before, after,
            "metadata application must not record a no-op transaction"
        );
    }
    let installed = changes
        .iter()
        .map(|(_, after)| after.address().glyph.clone())
        .collect::<Vec<_>>();
    project.experiments.applied.push(Applied {
        root,
        layers: std::mem::take(&mut changes),
        font_metadata: metadata_change,
    });
    Ok(installed)
}

/// Undo the newest experiment application if every affected canonical value is unchanged.
///
/// Later unrelated edits survive. A conflict leaves the root and the undo stack untouched.
pub fn undo_apply(project: &mut Project) -> Result<(SourceId, Vec<String>), String> {
    let applied = project
        .experiments
        .applied
        .last()
        .ok_or("no experiment application to undo")?;
    if project.document_source(applied.root).is_none() {
        return Err("the applied source is no longer loaded".into());
    }
    for (_, after) in &applied.layers {
        if project.capture_document_layer(after.address()).as_ref() != Some(after) {
            return Err(format!(
                "cannot undo: {} changed after application",
                after.address().glyph
            ));
        }
    }
    if let Some((_, after)) = &applied.font_metadata
        && project.document_font_metadata(applied.root) != Some(after)
    {
        return Err("cannot undo: kerning or groups changed after application".into());
    }

    let applied = project
        .experiments
        .applied
        .pop()
        .expect("validated apply entry");
    for (before, after) in &applied.layers {
        let address = before.address().clone();
        match project
            .restore_document_layer_if_current(&address, after, before.clone())
            .map_err(|error| error.to_string())?
        {
            DocumentEditOutcome::Changed { .. } => {
                project
                    .record_document_layer_history(&address, after.clone())
                    .map_err(|error| error.to_string())?;
            }
            DocumentEditOutcome::Unchanged { .. } => {
                return Err("experiment undo unexpectedly produced no change".into());
            }
        }
    }
    if let Some((before, _)) = &applied.font_metadata {
        let history = project.begin_document_source_metadata_history();
        let outcome = project
            .edit_document_source_metadata(applied.root, |draft| {
                draft.set_font_metadata(before.clone());
                Ok(())
            })
            .map_err(|error| error.to_string())?;
        if !matches!(outcome, DocumentEditOutcome::Changed { .. })
            || !project.record_document_source_metadata_history(history)
        {
            return Err("experiment metadata undo did not record a change".into());
        }
    }
    let names = applied
        .layers
        .iter()
        .map(|(_, after)| after.address().glyph.clone())
        .collect();
    Ok((applied.root, names))
}

fn compatible_snapshots(first: &CanonicalLayerSnapshot, second: &CanonicalLayerSnapshot) -> bool {
    fn signature(snapshot: &CanonicalLayerSnapshot) -> Vec<Vec<LayerPointType>> {
        let (layer, preserved) = snapshot.clone().into_parts();
        LayerEditDraft::new(layer, preserved)
            .view()
            .contours()
            .map(|contour| contour.points().map(|point| point.point_type()).collect())
            .collect()
    }
    signature(first) == signature(second)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::history::HistoryDirection;
    use crate::document::project::{DocumentHistoryReplayOutcome, Master};

    fn two_source_project() -> Project {
        let font = Project::new_font("synthetic.ufo".into())
            .source_snapshot(SourceId(0))
            .unwrap();
        let document = crate::document::font_memory::designspace_from_str(
            r#"<designspace format="5.0"><axes><axis name="Weight" tag="wght" minimum="0" default="0" maximum="1"/></axes><sources><source filename="first.ufo"><location><dimension name="Weight" xvalue="0"/></location></source><source filename="second.ufo"><location><dimension name="Weight" xvalue="1"/></location></source></sources></designspace>"#,
        )
        .unwrap();
        Project::from_designspace(document, |path| {
            Ok(Master::from_font(font.clone(), path.into()))
        })
        .unwrap()
    }

    fn default_address(project: &Project, source: SourceId, glyph: &str) -> GlyphLayerAddress {
        GlyphLayerAddress {
            glyph: glyph.into(),
            layer: project.document_source(source).unwrap().default_layer(),
        }
    }

    #[test]
    fn source_reorder_does_not_redirect_an_isolated_version() {
        let mut project = two_source_project();
        let source = project.source_id(0).unwrap();
        fork(&mut project, source, "stable", None, "test").unwrap();
        assert!(project.move_source(source, 1).unwrap());
        assert_eq!(project.experiments.versions["stable"].root, source);
        assert_eq!(project.source_index(source), Some(1));
        let address = default_address(&project, source, "A");
        assert!(
            project.experiments.versions["stable"]
                .layer(&address)
                .is_some()
        );
    }

    #[test]
    fn source_reorder_does_not_redirect_a_canonical_proposal() {
        let mut project = two_source_project();
        let source = project.source_id(0).unwrap();
        let other = project.source_id(1).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        let width = project.document_layer("A", &layer).unwrap().width();
        let batch = EditBatch {
            task: "stable-proposal".into(),
            reason: "verify stable source routing".into(),
            edits: vec![super::super::edit_batch::GlyphEdit {
                glyph: "A".into(),
                expected_revision: canonical_glyph_revision(
                    project.document_layer("A", &layer).unwrap(),
                )
                .unwrap(),
                operations: vec![super::super::edit_batch::Operation::SetWidth {
                    width: width + 1.0,
                }],
            }],
        };
        super::super::edit_batch::propose_project(&mut project, source, &batch).unwrap();
        assert!(project.move_source(source, 1).unwrap());
        assert_eq!(project.source_index(source), Some(1));
        assert!(
            super::super::proposal::find_project(&project, source, &batch.task).is_ok(),
            "stable source proposal disappeared after display reorder"
        );
        assert!(super::super::proposal::find_project(&project, other, &batch.task).is_err());
    }

    #[test]
    fn selective_apply_and_guarded_undo_preserve_unrelated_root_edits() {
        let mut project = Project::new_font("synthetic.ufo".into());
        let source = project.source_id(0).unwrap();
        fork(&mut project, source, "width", None, "test").unwrap();
        let address_a = default_address(&project, source, "A");
        let address_b = default_address(&project, source, "B");
        edit_layer(&mut project, "width", &address_a, |draft| {
            draft.set_width(700.0)?;
            Ok(())
        })
        .unwrap();
        project
            .edit_document_layer("B", &address_b.layer, |draft| {
                draft.set_width(731.0)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            apply(&mut project, "width", &["A".into()], false, true).unwrap(),
            ["A"]
        );
        assert_eq!(
            project
                .document_layer("A", &address_a.layer)
                .unwrap()
                .width(),
            700.0
        );
        assert_eq!(
            project
                .document_layer("B", &address_b.layer)
                .unwrap()
                .width(),
            731.0
        );
        assert!(project.can_replay_document_layer_history(&address_a, HistoryDirection::Undo));
        let (undone_source, names) = undo_apply(&mut project).unwrap();
        assert_eq!(undone_source, source);
        assert_eq!(names, ["A"]);
        assert_eq!(
            project
                .document_layer("B", &address_b.layer)
                .unwrap()
                .width(),
            731.0
        );
        assert!(matches!(
            project
                .replay_document_layer_history(&address_a, HistoryDirection::Undo)
                .unwrap(),
            DocumentHistoryReplayOutcome::Changed { .. }
        ));
        assert_eq!(
            project
                .document_layer("A", &address_a.layer)
                .unwrap()
                .width(),
            700.0
        );
    }

    #[test]
    fn conflicts_reject_the_whole_apply_without_revision_or_history_changes() {
        let mut project = Project::new_font("synthetic.ufo".into());
        let source = project.source_id(0).unwrap();
        fork(&mut project, source, "two", None, "test").unwrap();
        let address_a = default_address(&project, source, "A");
        let address_b = default_address(&project, source, "B");
        for (address, width) in [(&address_a, 700.0), (&address_b, 710.0)] {
            edit_layer(&mut project, "two", address, |draft| {
                draft.set_width(width)?;
                Ok(())
            })
            .unwrap();
        }
        project
            .edit_document_layer("B", &address_b.layer, |draft| {
                draft.set_width(999.0)?;
                Ok(())
            })
            .unwrap();
        let revision = project.document_revision();
        let before_a = project.capture_document_layer(&address_a).unwrap();
        assert!(
            apply(&mut project, "two", &["A".into(), "B".into()], false, true)
                .unwrap_err()
                .contains("conflict")
        );
        assert_eq!(project.document_revision(), revision);
        assert_eq!(project.capture_document_layer(&address_a), Some(before_a));
        assert_eq!(
            project.document_layer_history_depth(&address_a, HistoryDirection::Undo),
            0
        );
    }

    #[test]
    fn child_versions_share_the_root_baseline_but_isolate_working_state() {
        let mut project = Project::new_font("synthetic.ufo".into());
        let source = project.source_id(0).unwrap();
        fork(&mut project, source, "baseline", None, "same source").unwrap();
        fork(&mut project, source, "a", Some("baseline"), "direction A").unwrap();
        fork(&mut project, source, "b", Some("baseline"), "direction B").unwrap();
        let address = default_address(&project, source, "A");
        edit_layer(&mut project, "a", &address, |draft| {
            draft.set_width(700.0)?;
            Ok(())
        })
        .unwrap();
        assert_eq!(
            project.experiments.versions["a"]
                .layer(&address)
                .unwrap()
                .width(),
            700.0
        );
        assert_ne!(
            project.experiments.versions["b"]
                .layer(&address)
                .unwrap()
                .width(),
            700.0
        );
        assert_eq!(project.experiments.versions["a"].changed_glyphs(), ["A"]);
        assert!(
            project.experiments.versions["b"]
                .changed_glyphs()
                .is_empty()
        );
    }

    #[test]
    fn missing_sources_fail_cleanly_and_versions_remain_session_only() {
        let mut project = two_source_project();
        let source = project.source_id(1).unwrap();
        fork(&mut project, source, "removed", None, "test").unwrap();
        project.remove_source(source).unwrap();
        assert!(apply(&mut project, "removed", &[], true, true).is_err());
        assert_eq!(list(&project)["session_only"], true);
    }

    #[test]
    fn canonical_kerning_revisions_are_stable() {
        let project = Project::new_font("synthetic.ufo".into());
        let source = project.source_id(0).unwrap();
        let metadata = project.document_font_metadata(source).unwrap();
        assert_eq!(
            kerning_revision(metadata).unwrap(),
            kerning_revision(metadata).unwrap()
        );
    }
}
