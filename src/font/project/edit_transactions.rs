// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bounded, guarded edit groups over existing objects in one canonical source.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use kurbo::Point;

use super::*;
use crate::font::history::HistoryDirection;
use crate::font::variable::GlyphId;
use crate::font::{AnchorId, CanonicalLayerSnapshot, DocumentEditError, PointId};

const MAX_EDIT_LAYERS: usize = 64;
const MAX_EDIT_OPERATIONS: usize = 256;
const MAX_HISTORY_GROUPS: usize = 128;
const MAX_HISTORY_NAME_BYTES: usize = 256;

/// One supported nonstructural operation in a canonical edit transaction.
#[derive(Clone, Debug, PartialEq)]
pub enum DocumentEditOperation {
    /// Set the exact horizontal advance.
    SetWidth(f64),
    /// Move one existing point by stable identity.
    SetPoint {
        /// Stable point identity returned by a canonical layer read.
        point: PointId,
        /// Exact replacement position.
        position: Point,
    },
    /// Move one existing anchor by stable identity.
    SetAnchor {
        /// Stable anchor identity returned by a canonical layer read.
        anchor: AnchorId,
        /// Exact replacement position.
        position: Point,
    },
}

/// One existing object that changed in a committed canonical edit transaction.
///
/// This limited transaction surface supports only the exact advance and existing points or
/// anchors, so its receipt can retain stable identities without implying structural edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentEditObjectKind {
    /// The exact horizontal advance changed.
    Width,
    /// One existing point moved.
    Point(PointId),
    /// One existing anchor moved.
    Anchor(AnchorId),
}

/// Stable canonical identity of one object changed by a committed transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentEditChangedObject {
    /// Glyph name at the committed canonical address.
    pub glyph: String,
    /// Stable glyph identity retained by the source transaction guard.
    pub glyph_id: GlyphId,
    /// Stable source and layer identity at the committed canonical address.
    pub layer: LayerId,
    /// Existing object that changed.
    pub object: DocumentEditObjectKind,
}

impl DocumentEditOperation {
    fn apply(&self, draft: &mut super::super::LayerEditDraft) -> Result<(), DocumentEditError> {
        match *self {
            Self::SetWidth(width) => {
                draft.set_width(width)?;
            }
            Self::SetPoint { point, position } => {
                draft.set_point_position(point, position)?;
            }
            Self::SetAnchor { anchor, position } => {
                draft.set_anchor_position(anchor, position)?;
            }
        }
        Ok(())
    }
}

fn changed_objects(
    before: &CanonicalLayerSnapshot,
    after: &CanonicalLayerSnapshot,
    operations: &[DocumentEditOperation],
    glyph_id: GlyphId,
) -> Vec<DocumentEditChangedObject> {
    let address = before.address();
    let mut changed = Vec::new();
    let mut push = |object| {
        let candidate = DocumentEditChangedObject {
            glyph: address.glyph.clone(),
            glyph_id,
            layer: address.layer.clone(),
            object,
        };
        if !changed.contains(&candidate) {
            changed.push(candidate);
        }
    };
    for operation in operations {
        match *operation {
            DocumentEditOperation::SetWidth(_) if before.width() != after.width() => {
                push(DocumentEditObjectKind::Width);
            }
            DocumentEditOperation::SetPoint { point, .. }
                if before.point_position(point) != after.point_position(point) =>
            {
                push(DocumentEditObjectKind::Point(point));
            }
            DocumentEditOperation::SetAnchor { anchor, .. }
                if before.anchor_position(anchor) != after.anchor_position(anchor) =>
            {
                push(DocumentEditObjectKind::Anchor(anchor));
            }
            DocumentEditOperation::SetWidth(_)
            | DocumentEditOperation::SetPoint { .. }
            | DocumentEditOperation::SetAnchor { .. } => {}
        }
    }
    changed
}

/// Ordered edits to one layer, guarded by the exact canonical state the caller read.
#[derive(Clone, Debug)]
pub struct DocumentLayerEdit {
    expected: CanonicalLayerSnapshot,
    operations: Vec<DocumentEditOperation>,
}

impl DocumentLayerEdit {
    /// Pair an opaque canonical read with the operations derived from it.
    pub fn new(expected: CanonicalLayerSnapshot, operations: Vec<DocumentEditOperation>) -> Self {
        Self {
            expected,
            operations,
        }
    }

    /// Stable address this edit targets.
    pub fn address(&self) -> &GlyphLayerAddress {
        self.expected.address()
    }
}

/// Session-stable handle for one named canonical edit history group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditHistoryGroupId(u64);

impl EditHistoryGroupId {
    /// Serialize this document-epoch-scoped handle for a receipt.
    pub fn to_wire(self) -> String {
        self.0.to_string()
    }

    /// Parse a handle previously returned by [`Self::to_wire`].
    pub fn from_wire(value: &str) -> Option<Self> {
        value
            .parse::<u64>()
            .ok()
            .filter(|value| *value != 0)
            .map(Self)
    }
}

/// Current replay side of a retained edit history group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditHistoryGroupState {
    /// The group's after-state is live and the group may be undone.
    Applied,
    /// The group's before-state is live and the group may be redone.
    Undone,
}

/// Why a bounded canonical edit transaction or grouped replay was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentEditTransactionError {
    /// The request exceeds a documented bound or has an invalid shape.
    Invalid(String),
    /// A requested operation is invalid for its staged canonical draft.
    Operation(DocumentEditError),
    /// One target or dependency belongs to another source.
    WrongSource {
        /// The transaction's declared source.
        expected: SourceId,
        /// Source found in the rejected address.
        actual: SourceId,
    },
    /// The addressed glyph layer no longer exists.
    MissingLayer(GlyphLayerAddress),
    /// A target or read dependency changed after its guarded snapshot was captured.
    StaleLayer(GlyphLayerAddress),
    /// A grouped replay overlaps a later change or a removed target.
    HistoryConflict {
        /// Requested group.
        group: EditHistoryGroupId,
        /// First affected address that no longer matches the group's expected side.
        address: GlyphLayerAddress,
    },
    /// The bounded session history no longer retains this group.
    UnknownHistoryGroup(EditHistoryGroupId),
    /// Undo or redo was requested from the wrong side of this group.
    WrongHistoryState {
        /// Requested group.
        group: EditHistoryGroupId,
        /// Current retained state.
        state: EditHistoryGroupState,
        /// Requested replay direction.
        direction: HistoryDirection,
    },
}

impl std::fmt::Display for DocumentEditTransactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => formatter.write_str(message),
            Self::Operation(error) => write!(formatter, "canonical edit failed: {error}"),
            Self::WrongSource { expected, actual } => write!(
                formatter,
                "transaction source {} does not match target source {}",
                expected.0, actual.0
            ),
            Self::MissingLayer(address) => write!(
                formatter,
                "glyph layer {} at {} no longer exists",
                address.glyph, address.layer.name
            ),
            Self::StaleLayer(address) => write!(
                formatter,
                "glyph layer {} at {} changed after transaction capture",
                address.glyph, address.layer.name
            ),
            Self::HistoryConflict { group, address } => write!(
                formatter,
                "edit history group {} conflicts at {} in {}",
                group.0, address.glyph, address.layer.name
            ),
            Self::UnknownHistoryGroup(group) => {
                write!(formatter, "edit history group {} is not retained", group.0)
            }
            Self::WrongHistoryState {
                group,
                state,
                direction,
            } => write!(
                formatter,
                "edit history group {} is {state:?} and cannot replay {direction:?}",
                group.0
            ),
        }
    }
}

impl std::error::Error for DocumentEditTransactionError {}

/// A fully staged one-source edit guarded by its complete canonical read and write set.
#[derive(Clone, Debug)]
pub struct CanonicalDocumentEditTransaction {
    source: SourceId,
    history_name: String,
    reads: Vec<CanonicalLayerSnapshot>,
    writes: Vec<SnapshotEdit>,
}

#[derive(Clone, Debug)]
struct SnapshotEdit {
    before: CanonicalLayerSnapshot,
    after: CanonicalLayerSnapshot,
    changed_objects: Vec<DocumentEditChangedObject>,
}

/// Result of committing a bounded canonical document edit transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentEditTransactionOutcome {
    /// Every final draft matched its guarded state, so nothing was published.
    Unchanged {
        /// Current document revision.
        revision: u64,
    },
    /// Every changed draft published in one revision and one named history group.
    Changed {
        /// Document revision immediately before the atomic publication.
        before_revision: u64,
        /// Document revision after the atomic publication.
        after_revision: u64,
        /// Exact invalidation scope of the complete group.
        change: DocumentChange,
        /// Stable identities of the existing objects that actually changed.
        changed_objects: Vec<DocumentEditChangedObject>,
        /// Stable handle shared by targeted and ordinary grouped undo.
        history_group: EditHistoryGroupId,
    },
}

/// Result of atomically replaying one retained edit history group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentEditHistoryReplayOutcome {
    /// Document revision immediately before replay.
    pub before_revision: u64,
    /// Document revision after replay.
    pub after_revision: u64,
    /// Exact invalidation scope of the complete replayed group.
    pub change: DocumentChange,
    /// Retained group side after replay.
    pub state: EditHistoryGroupState,
}

#[derive(Clone, Debug)]
struct EditHistoryEntry {
    id: EditHistoryGroupId,
    name: String,
    edits: Vec<SnapshotEdit>,
    state: EditHistoryGroupState,
}

#[derive(Clone, Debug)]
struct ReplayPlan {
    expected: Vec<CanonicalLayerSnapshot>,
    replacements: Vec<CanonicalLayerSnapshot>,
}

/// One Project-owned history pile for atomic multi-layer edit groups.
#[derive(Debug)]
pub(super) struct EditTransactionHistory {
    next_id: u64,
    groups: VecDeque<EditHistoryEntry>,
}

impl Default for EditTransactionHistory {
    fn default() -> Self {
        Self {
            next_id: 1,
            groups: VecDeque::with_capacity(MAX_HISTORY_GROUPS),
        }
    }
}

impl EditTransactionHistory {
    fn record(&mut self, name: String, edits: Vec<SnapshotEdit>) -> EditHistoryGroupId {
        let id = EditHistoryGroupId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.groups.push_back(EditHistoryEntry {
            id,
            name,
            edits,
            state: EditHistoryGroupState::Applied,
        });
        if self.groups.len() > MAX_HISTORY_GROUPS {
            self.groups.pop_front();
        }
        id
    }

    fn entry(&self, id: EditHistoryGroupId) -> Option<&EditHistoryEntry> {
        self.groups.iter().find(|entry| entry.id == id)
    }

    fn replay_entry(
        &self,
        id: EditHistoryGroupId,
        direction: HistoryDirection,
    ) -> Result<&EditHistoryEntry, DocumentEditTransactionError> {
        let entry = self
            .entry(id)
            .ok_or(DocumentEditTransactionError::UnknownHistoryGroup(id))?;
        let valid = matches!(
            (entry.state, direction),
            (EditHistoryGroupState::Applied, HistoryDirection::Undo)
                | (EditHistoryGroupState::Undone, HistoryDirection::Redo)
        );
        if !valid {
            return Err(DocumentEditTransactionError::WrongHistoryState {
                group: id,
                state: entry.state,
                direction,
            });
        }
        Ok(entry)
    }

    fn replay_plan(
        &self,
        id: EditHistoryGroupId,
        direction: HistoryDirection,
    ) -> Result<ReplayPlan, DocumentEditTransactionError> {
        let entry = self.replay_entry(id, direction)?;
        let (expected, replacements) = entry
            .edits
            .iter()
            .map(|edit| match direction {
                HistoryDirection::Undo => (edit.after.clone(), edit.before.clone()),
                HistoryDirection::Redo => (edit.before.clone(), edit.after.clone()),
            })
            .unzip();
        Ok(ReplayPlan {
            expected,
            replacements,
        })
    }

    fn finish_replay(&mut self, id: EditHistoryGroupId, direction: HistoryDirection) {
        let entry = self
            .groups
            .iter_mut()
            .find(|entry| entry.id == id)
            .expect("the planned edit history group remains retained");
        entry.state = match direction {
            HistoryDirection::Undo => EditHistoryGroupState::Undone,
            HistoryDirection::Redo => EditHistoryGroupState::Applied,
        };
    }
}

impl Project {
    /// Stage a bounded one-source edit without mutating the project or any history.
    ///
    /// `reads` contains additional opaque dependencies not directly written by the transaction.
    /// Each write carries its own expected snapshot. Every dependency and target is checked again
    /// together at commit, after all fallible operations have succeeded on private drafts.
    pub fn begin_document_edit_transaction(
        &self,
        source: SourceId,
        history_name: impl Into<String>,
        reads: Vec<CanonicalLayerSnapshot>,
        edits: Vec<DocumentLayerEdit>,
    ) -> Result<CanonicalDocumentEditTransaction, DocumentEditTransactionError> {
        if self.document_source(source).is_none() {
            return Err(DocumentEditTransactionError::Invalid(
                "transaction source does not exist".into(),
            ));
        }
        let history_name = history_name.into();
        if history_name.trim().is_empty() || history_name.len() > MAX_HISTORY_NAME_BYTES {
            return Err(DocumentEditTransactionError::Invalid(format!(
                "history name must contain 1..={MAX_HISTORY_NAME_BYTES} bytes"
            )));
        }
        if edits.is_empty() {
            return Err(DocumentEditTransactionError::Invalid(
                "transaction must contain at least one edit".into(),
            ));
        }
        let operation_count = edits
            .iter()
            .map(|edit| edit.operations.len())
            .sum::<usize>();
        if operation_count == 0 || operation_count > MAX_EDIT_OPERATIONS {
            return Err(DocumentEditTransactionError::Invalid(format!(
                "transaction must contain 1..={MAX_EDIT_OPERATIONS} operations"
            )));
        }
        if reads.len().saturating_add(edits.len()) > MAX_EDIT_LAYERS {
            return Err(DocumentEditTransactionError::Invalid(format!(
                "transaction may guard at most {MAX_EDIT_LAYERS} layer entries"
            )));
        }

        let mut guarded = BTreeMap::new();
        for snapshot in &reads {
            validate_source(source, snapshot.address())?;
            insert_guard(&mut guarded, snapshot)?;
        }

        let mut targets = BTreeSet::new();
        let mut writes = Vec::with_capacity(edits.len());
        for edit in edits {
            let address = edit.expected.address().clone();
            validate_source(source, &address)?;
            if edit.operations.is_empty() {
                return Err(DocumentEditTransactionError::Invalid(format!(
                    "{} at {} has no operations",
                    address.glyph, address.layer.name
                )));
            }
            if !targets.insert(address.clone()) {
                return Err(DocumentEditTransactionError::Invalid(format!(
                    "duplicate edit target {} at {}",
                    address.glyph, address.layer.name
                )));
            }
            insert_guard(&mut guarded, &edit.expected)?;
            let before = edit.expected;
            let (layer, preserved) = before.clone().into_parts();
            let mut draft = super::super::LayerEditDraft::new(layer, preserved);
            for operation in &edit.operations {
                operation
                    .apply(&mut draft)
                    .map_err(DocumentEditTransactionError::Operation)?;
            }
            let (layer, preserved) = draft.into_parts();
            let after = CanonicalLayerSnapshot::new(address, layer, preserved);
            if before != after {
                let glyph_id = self
                    .document_glyph(&before.address().glyph)
                    .expect("an edit guard retains an existing canonical glyph")
                    .id();
                let changed_objects = changed_objects(&before, &after, &edit.operations, glyph_id);
                debug_assert!(
                    !changed_objects.is_empty(),
                    "the limited edit transaction reports every changed draft object"
                );
                writes.push(SnapshotEdit {
                    before,
                    after,
                    changed_objects,
                });
            }
        }

        Ok(CanonicalDocumentEditTransaction {
            source,
            history_name,
            reads: guarded.into_values().collect(),
            writes,
        })
    }

    /// Read the proposed canonical layers after rechecking every transaction dependency.
    ///
    /// This does not publish changes, advance the document revision, or record history.
    /// An empty result means every staged operation was unchanged.
    pub fn preview_document_edit_transaction(
        &self,
        transaction: &CanonicalDocumentEditTransaction,
    ) -> Result<Vec<CanonicalLayerSnapshot>, DocumentEditTransactionError> {
        for expected in &transaction.reads {
            self.validate_edit_snapshot(expected)?;
        }
        Ok(transaction
            .writes
            .iter()
            .map(|edit| edit.after.clone())
            .collect())
    }

    /// Publish a fully staged transaction as one revision and one named history group.
    pub fn commit_document_edit_transaction(
        &mut self,
        transaction: CanonicalDocumentEditTransaction,
    ) -> Result<DocumentEditTransactionOutcome, DocumentEditTransactionError> {
        for expected in &transaction.reads {
            self.validate_edit_snapshot(expected)?;
        }
        if transaction.writes.is_empty() {
            return Ok(DocumentEditTransactionOutcome::Unchanged {
                revision: self.variable.revision,
            });
        }
        debug_assert!(
            transaction
                .writes
                .iter()
                .all(|edit| edit.before.address().layer.source == transaction.source),
            "every staged edit must retain the validated transaction source"
        );
        let edits = transaction.writes;
        let changed_objects = edits
            .iter()
            .flat_map(|edit| edit.changed_objects.iter().cloned())
            .collect();
        let replacements = edits
            .iter()
            .map(|edit| edit.after.clone())
            .collect::<Vec<_>>();
        let before_revision = self.variable.revision;
        let change = self.commit_edit_replacements(&replacements)?;
        let history_group = self
            .edit_transaction_history
            .record(transaction.history_name, edits);
        Ok(DocumentEditTransactionOutcome::Changed {
            before_revision,
            after_revision: self.variable.revision,
            change,
            changed_objects,
            history_group,
        })
    }

    /// Human-readable name recorded with one retained edit history group.
    pub fn document_edit_history_group_name(&self, group: EditHistoryGroupId) -> Option<&str> {
        Some(self.edit_transaction_history.entry(group)?.name.as_str())
    }

    /// Current replay side of one retained edit history group.
    pub fn document_edit_history_group_state(
        &self,
        group: EditHistoryGroupId,
    ) -> Option<EditHistoryGroupState> {
        Some(self.edit_transaction_history.entry(group)?.state)
    }

    /// Check whether a retained group can replay against the current canonical layers.
    ///
    /// This read-only check supports application history availability and conflict reporting.
    /// Replay validates again; a successful check is not a reservation or a write authorization.
    pub fn check_document_edit_history_group(
        &self,
        group: EditHistoryGroupId,
        direction: HistoryDirection,
    ) -> Result<(), DocumentEditTransactionError> {
        let entry = self
            .edit_transaction_history
            .replay_entry(group, direction)?;
        // Menu availability needs only guarded reads, not cloned replacement geometry.
        self.validate_history_group_snapshots(
            group,
            entry.edits.iter().map(|edit| match direction {
                HistoryDirection::Undo => &edit.after,
                HistoryDirection::Redo => &edit.before,
            }),
        )
    }

    fn validate_history_group_snapshots<'a>(
        &self,
        group: EditHistoryGroupId,
        snapshots: impl IntoIterator<Item = &'a CanonicalLayerSnapshot>,
    ) -> Result<(), DocumentEditTransactionError> {
        for expected in snapshots {
            if self.validate_edit_snapshot(expected).is_err() {
                return Err(DocumentEditTransactionError::HistoryConflict {
                    group,
                    address: expected.address().clone(),
                });
            }
        }
        Ok(())
    }

    /// Atomically undo or redo one named edit group after validating every affected layer.
    ///
    /// Ordinary editor undo and targeted agent undo must call this same method with the same
    /// handle. A successful replay changes the retained side, so a second undo cannot reverse the
    /// operation again. Later edits to unrelated layers survive; an overlapping edit is stale.
    pub fn replay_document_edit_history_group(
        &mut self,
        group: EditHistoryGroupId,
        direction: HistoryDirection,
    ) -> Result<DocumentEditHistoryReplayOutcome, DocumentEditTransactionError> {
        let mut history = std::mem::take(&mut self.edit_transaction_history);
        let result = (|| {
            let plan = history.replay_plan(group, direction)?;
            self.validate_history_group_snapshots(group, &plan.expected)?;
            let before_revision = self.variable.revision;
            let change = self.commit_edit_replacements(&plan.replacements)?;
            history.finish_replay(group, direction);
            let state = match direction {
                HistoryDirection::Undo => EditHistoryGroupState::Undone,
                HistoryDirection::Redo => EditHistoryGroupState::Applied,
            };
            Ok(DocumentEditHistoryReplayOutcome {
                before_revision,
                after_revision: self.variable.revision,
                change,
                state,
            })
        })();
        self.edit_transaction_history = history;
        result
    }

    fn validate_edit_snapshot(
        &self,
        expected: &CanonicalLayerSnapshot,
    ) -> Result<(), DocumentEditTransactionError> {
        let address = expected.address();
        let current = self
            .capture_document_layer(address)
            .ok_or_else(|| DocumentEditTransactionError::MissingLayer(address.clone()))?;
        if &current != expected {
            return Err(DocumentEditTransactionError::StaleLayer(address.clone()));
        }
        Ok(())
    }

    fn commit_edit_replacements(
        &mut self,
        replacements: &[CanonicalLayerSnapshot],
    ) -> Result<DocumentChange, DocumentEditTransactionError> {
        for replacement in replacements {
            let Some(current) = self.capture_document_layer(replacement.address()) else {
                return Err(DocumentEditTransactionError::MissingLayer(
                    replacement.address().clone(),
                ));
            };
            if current == *replacement {
                return Err(DocumentEditTransactionError::Invalid(format!(
                    "replacement for {} at {} is unchanged",
                    replacement.address().glyph,
                    replacement.address().layer.name
                )));
            }
        }
        let drafts = replacements
            .iter()
            .cloned()
            .map(|replacement| {
                let address = replacement.address().clone();
                let (layer, preserved) = replacement.into_parts();
                (address, super::super::LayerEditDraft::new(layer, preserved))
            })
            .collect::<Vec<_>>();
        let changed = self.variable.commit_layer_edits(drafts).ok_or_else(|| {
            DocumentEditTransactionError::Invalid(
                "validated edit targets disappeared before publication".into(),
            )
        })?;
        debug_assert_eq!(
            changed.len(),
            replacements.len(),
            "every validated history replacement must change its layer"
        );

        let mut affected_layers = Vec::with_capacity(changed.len());
        let mut dependent_layers = BTreeSet::new();
        let mut geometry = false;
        let mut metrics = false;
        let mut metadata = false;
        for (address, delta) in changed {
            geometry |= delta.geometry;
            metrics |= delta.metrics;
            metadata |= delta.metadata;
            dependent_layers.extend(self.variable.dependent_component_layers(&address.glyph));
            self.record_layer_change(&address.glyph, &address.layer);
            affected_layers.push(address);
        }
        Ok(DocumentChange {
            affected_layers,
            dependent_layers: dependent_layers.into_iter().collect(),
            source_metadata: Vec::new(),
            geometry,
            metrics,
            metadata,
            compilation: true,
        })
    }
}

fn validate_source(
    source: SourceId,
    address: &GlyphLayerAddress,
) -> Result<(), DocumentEditTransactionError> {
    if address.layer.source != source {
        return Err(DocumentEditTransactionError::WrongSource {
            expected: source,
            actual: address.layer.source,
        });
    }
    Ok(())
}

fn insert_guard(
    guarded: &mut BTreeMap<GlyphLayerAddress, CanonicalLayerSnapshot>,
    snapshot: &CanonicalLayerSnapshot,
) -> Result<(), DocumentEditTransactionError> {
    if let Some(previous) = guarded.get(snapshot.address()) {
        if previous != snapshot {
            return Err(DocumentEditTransactionError::Invalid(format!(
                "conflicting guarded snapshots for {} at {}",
                snapshot.address().glyph,
                snapshot.address().layer.name
            )));
        }
        return Ok(());
    }
    guarded.insert(snapshot.address().clone(), snapshot.clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use norad::{Anchor, Contour, ContourPoint, Font, Glyph, Name, PointType};

    use super::*;

    fn project() -> Project {
        let mut font = Font::new();
        for (index, name) in ["A", "B", "C"].into_iter().enumerate() {
            let offset = index as f64 * 20.0;
            let mut glyph = Glyph::new(name);
            glyph.width = 500.0 + offset;
            glyph.contours.push(Contour::new(
                vec![
                    ContourPoint::new(offset, 0.0, PointType::Line, false, None, None),
                    ContourPoint::new(100.0 + offset, 100.0, PointType::Line, false, None, None),
                ],
                None,
            ));
            glyph.anchors.push(Anchor::new(
                50.0 + offset,
                120.0,
                Some(Name::new("top").unwrap()),
                None,
                None,
            ));
            font.default_layer_mut().insert_glyph(glyph);
        }
        Project::from_source(SourceInput::from_font(
            font,
            PathBuf::from("transaction-fixture.ufo"),
        ))
    }

    fn address(project: &Project, glyph: &str) -> GlyphLayerAddress {
        let source = project.source_id(0).unwrap();
        GlyphLayerAddress {
            glyph: glyph.into(),
            layer: project.document_source(source).unwrap().default_layer(),
        }
    }

    fn point_and_anchor(project: &Project, address: &GlyphLayerAddress) -> (PointId, AnchorId) {
        let layer = project
            .document_layer(&address.glyph, &address.layer)
            .unwrap();
        (
            layer
                .contours()
                .next()
                .unwrap()
                .points()
                .next()
                .unwrap()
                .id(),
            layer.anchors().next().unwrap().id(),
        )
    }

    fn width(project: &Project, address: &GlyphLayerAddress) -> f64 {
        project
            .document_layer(&address.glyph, &address.layer)
            .unwrap()
            .width()
    }

    #[test]
    fn changed_object_receipt_excludes_noops_and_deduplicates_final_delta() {
        let mut project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let glyph_id = project.document_glyph("A").unwrap().id();
        let layer = project.document_layer(&a.glyph, &a.layer).unwrap();
        let point = layer.contours().next().unwrap().points().next().unwrap();
        let point_id = point.id();
        let point_position = point.position();
        let anchor = layer.anchors().next().unwrap();
        let anchor_id = anchor.id();
        let transaction = project
            .begin_document_edit_transaction(
                source,
                "agent: final object delta",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    project.capture_document_layer(&a).unwrap(),
                    vec![
                        DocumentEditOperation::SetWidth(600.0),
                        DocumentEditOperation::SetWidth(500.0),
                        DocumentEditOperation::SetPoint {
                            point: point_id,
                            position: Point::new(12.0, 6.0),
                        },
                        DocumentEditOperation::SetPoint {
                            point: point_id,
                            position: point_position,
                        },
                        DocumentEditOperation::SetAnchor {
                            anchor: anchor_id,
                            position: Point::new(70.0, 150.0),
                        },
                        DocumentEditOperation::SetAnchor {
                            anchor: anchor_id,
                            position: Point::new(75.0, 155.0),
                        },
                    ],
                )],
            )
            .unwrap();

        let DocumentEditTransactionOutcome::Changed {
            changed_objects, ..
        } = project
            .commit_document_edit_transaction(transaction)
            .unwrap()
        else {
            panic!("the anchor must change");
        };

        assert_eq!(
            changed_objects,
            vec![DocumentEditChangedObject {
                glyph: "A".into(),
                glyph_id,
                layer: a.layer.clone(),
                object: DocumentEditObjectKind::Anchor(anchor_id),
            }]
        );
        assert_eq!(width(&project, &a), 500.0);
        assert_eq!(
            project
                .document_layer("A", &a.layer)
                .unwrap()
                .anchors()
                .next()
                .unwrap()
                .position(),
            Point::new(75.0, 155.0)
        );
    }

    #[test]
    fn invalid_third_operation_leaves_document_dirty_state_and_histories_untouched() {
        let project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let b = address(&project, "B");
        let (a_point, a_anchor) = point_and_anchor(&project, &a);
        let before_revision = project.document_revision();
        let before_a = project.capture_document_layer(&a).unwrap();
        let before_b = project.capture_document_layer(&b).unwrap();

        let result = project.begin_document_edit_transaction(
            source,
            "agent: invalid operation three",
            Vec::new(),
            vec![
                DocumentLayerEdit::new(
                    before_a.clone(),
                    vec![
                        DocumentEditOperation::SetWidth(550.0),
                        DocumentEditOperation::SetPoint {
                            point: a_point,
                            position: Point::new(12.0, 3.0),
                        },
                    ],
                ),
                DocumentLayerEdit::new(
                    before_b.clone(),
                    vec![DocumentEditOperation::SetAnchor {
                        anchor: a_anchor,
                        position: Point::new(60.0, 140.0),
                    }],
                ),
            ],
        );

        assert!(matches!(
            result,
            Err(DocumentEditTransactionError::Operation(
                DocumentEditError::MissingAnchor(_)
            ))
        ));
        assert_eq!(project.document_revision(), before_revision);
        assert_eq!(project.capture_document_layer(&a).unwrap(), before_a);
        assert_eq!(project.capture_document_layer(&b).unwrap(), before_b);
        assert_eq!(project.document_source_is_modified(source), Some(false));
        for target in [&a, &b] {
            assert_eq!(
                project.document_layer_history_depth(target, HistoryDirection::Undo),
                0
            );
            assert_eq!(
                project.document_layer_history_depth(target, HistoryDirection::Redo),
                0
            );
        }
    }

    #[test]
    fn stale_read_dependency_rejects_every_write_without_touching_existing_history() {
        let mut project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let b = address(&project, "B");
        let c = address(&project, "C");
        let transaction = project
            .begin_document_edit_transaction(
                source,
                "agent: guarded widths",
                vec![project.capture_document_layer(&c).unwrap()],
                vec![
                    DocumentLayerEdit::new(
                        project.capture_document_layer(&a).unwrap(),
                        vec![DocumentEditOperation::SetWidth(551.0)],
                    ),
                    DocumentLayerEdit::new(
                        project.capture_document_layer(&b).unwrap(),
                        vec![DocumentEditOperation::SetWidth(571.0)],
                    ),
                ],
            )
            .unwrap();

        let mut human = project.begin_document_layer_transaction(&c).unwrap();
        human.draft_mut().set_width(777.0).unwrap();
        project.commit_document_layer_transaction(human).unwrap();
        let revision_before_rejection = project.document_revision();
        let dirty_before_rejection = project.document_source_is_modified(source);
        let a_before_rejection = project.capture_document_layer(&a).unwrap();
        let b_before_rejection = project.capture_document_layer(&b).unwrap();
        let c_history_before = project.document_layer_history_depth(&c, HistoryDirection::Undo);

        assert_eq!(
            project.commit_document_edit_transaction(transaction),
            Err(DocumentEditTransactionError::StaleLayer(c.clone()))
        );
        assert_eq!(project.document_revision(), revision_before_rejection);
        assert_eq!(
            project.document_source_is_modified(source),
            dirty_before_rejection
        );
        assert_eq!(
            project.capture_document_layer(&a).unwrap(),
            a_before_rejection
        );
        assert_eq!(
            project.capture_document_layer(&b).unwrap(),
            b_before_rejection
        );
        assert_eq!(
            project.document_layer_history_depth(&c, HistoryDirection::Undo),
            c_history_before
        );
        assert_eq!(
            project.document_layer_history_depth(&a, HistoryDirection::Undo),
            0
        );
        assert_eq!(
            project.document_layer_history_depth(&b, HistoryDirection::Undo),
            0
        );
    }

    #[test]
    fn one_group_replays_once_through_ordinary_and_targeted_paths() {
        let mut project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let b = address(&project, "B");
        let (a_point, a_anchor) = point_and_anchor(&project, &a);
        let point_id_before = a_point;
        let anchor_id_before = a_anchor;
        let a_glyph_id = project.document_glyph("A").unwrap().id();
        let b_glyph_id = project.document_glyph("B").unwrap().id();
        let before_revision = project.document_revision();
        let transaction = project
            .begin_document_edit_transaction(
                source,
                "agent: widen A and B",
                Vec::new(),
                vec![
                    DocumentLayerEdit::new(
                        project.capture_document_layer(&a).unwrap(),
                        vec![
                            DocumentEditOperation::SetWidth(560.0),
                            DocumentEditOperation::SetPoint {
                                point: a_point,
                                position: Point::new(11.0, 7.0),
                            },
                            DocumentEditOperation::SetAnchor {
                                anchor: a_anchor,
                                position: Point::new(66.0, 144.0),
                            },
                        ],
                    ),
                    DocumentLayerEdit::new(
                        project.capture_document_layer(&b).unwrap(),
                        vec![DocumentEditOperation::SetWidth(580.0)],
                    ),
                ],
            )
            .unwrap();
        let committed = project
            .commit_document_edit_transaction(transaction)
            .unwrap();
        let DocumentEditTransactionOutcome::Changed {
            before_revision: committed_before,
            after_revision: committed_after,
            change,
            changed_objects,
            history_group,
        } = committed
        else {
            panic!("the transaction must change both layers");
        };

        assert_eq!(committed_before, before_revision);
        assert_eq!(committed_after, before_revision + 1);
        assert_eq!(change.affected_layers(), [a.clone(), b.clone()]);
        assert_eq!(
            changed_objects,
            vec![
                DocumentEditChangedObject {
                    glyph: "A".into(),
                    glyph_id: a_glyph_id,
                    layer: a.layer.clone(),
                    object: DocumentEditObjectKind::Width,
                },
                DocumentEditChangedObject {
                    glyph: "A".into(),
                    glyph_id: a_glyph_id,
                    layer: a.layer.clone(),
                    object: DocumentEditObjectKind::Point(a_point),
                },
                DocumentEditChangedObject {
                    glyph: "A".into(),
                    glyph_id: a_glyph_id,
                    layer: a.layer.clone(),
                    object: DocumentEditObjectKind::Anchor(a_anchor),
                },
                DocumentEditChangedObject {
                    glyph: "B".into(),
                    glyph_id: b_glyph_id,
                    layer: b.layer.clone(),
                    object: DocumentEditObjectKind::Width,
                },
            ]
        );
        assert_eq!(
            project.document_edit_history_group_name(history_group),
            Some("agent: widen A and B")
        );
        assert_eq!(
            project.document_edit_history_group_state(history_group),
            Some(EditHistoryGroupState::Applied)
        );
        assert_eq!(
            project.document_layer_history_depth(&a, HistoryDirection::Undo),
            0,
            "the group must not be duplicated into a per-layer pile"
        );
        assert_eq!(
            point_and_anchor(&project, &a),
            (point_id_before, anchor_id_before)
        );
        assert_eq!(width(&project, &a), 560.0);
        assert_eq!(width(&project, &b), 580.0);

        // The application calls the same grouped API for ordinary undo.
        assert_eq!(
            project.check_document_edit_history_group(history_group, HistoryDirection::Undo),
            Ok(())
        );
        assert_eq!(project.document_revision(), committed_after);
        let ordinary = project
            .replay_document_edit_history_group(history_group, HistoryDirection::Undo)
            .unwrap();
        assert_eq!(ordinary.before_revision, committed_after);
        assert_eq!(ordinary.after_revision, committed_after + 1);
        assert_eq!(ordinary.state, EditHistoryGroupState::Undone);
        assert_eq!(width(&project, &a), 500.0);
        assert_eq!(width(&project, &b), 520.0);
        assert_eq!(
            point_and_anchor(&project, &a),
            (point_id_before, anchor_id_before)
        );

        let revision_before_duplicate = project.document_revision();
        assert_eq!(
            project.check_document_edit_history_group(history_group, HistoryDirection::Undo),
            Err(DocumentEditTransactionError::WrongHistoryState {
                group: history_group,
                state: EditHistoryGroupState::Undone,
                direction: HistoryDirection::Undo,
            })
        );
        assert_eq!(
            project.replay_document_edit_history_group(history_group, HistoryDirection::Undo),
            Err(DocumentEditTransactionError::WrongHistoryState {
                group: history_group,
                state: EditHistoryGroupState::Undone,
                direction: HistoryDirection::Undo,
            })
        );
        assert_eq!(project.document_revision(), revision_before_duplicate);

        let targeted_redo = project
            .replay_document_edit_history_group(history_group, HistoryDirection::Redo)
            .unwrap();
        assert_eq!(targeted_redo.state, EditHistoryGroupState::Applied);
        assert_eq!(width(&project, &a), 560.0);
        assert_eq!(width(&project, &b), 580.0);
    }

    #[test]
    fn targeted_undo_preserves_unrelated_edits_and_rejects_overlap() {
        let mut project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let b = address(&project, "B");
        let c = address(&project, "C");
        let transaction = project
            .begin_document_edit_transaction(
                source,
                "agent: edit A and B",
                Vec::new(),
                vec![
                    DocumentLayerEdit::new(
                        project.capture_document_layer(&a).unwrap(),
                        vec![DocumentEditOperation::SetWidth(601.0)],
                    ),
                    DocumentLayerEdit::new(
                        project.capture_document_layer(&b).unwrap(),
                        vec![DocumentEditOperation::SetWidth(602.0)],
                    ),
                ],
            )
            .unwrap();
        let DocumentEditTransactionOutcome::Changed { history_group, .. } = project
            .commit_document_edit_transaction(transaction)
            .unwrap()
        else {
            panic!("the transaction must change");
        };

        project
            .edit_document_layer("C", &c.layer, |draft| {
                draft.set_width(703.0)?;
                Ok(())
            })
            .unwrap();
        project
            .replay_document_edit_history_group(history_group, HistoryDirection::Undo)
            .unwrap();
        assert_eq!(width(&project, &a), 500.0);
        assert_eq!(width(&project, &b), 520.0);
        assert_eq!(width(&project, &c), 703.0);

        project
            .replay_document_edit_history_group(history_group, HistoryDirection::Redo)
            .unwrap();
        project
            .edit_document_layer("A", &a.layer, |draft| {
                draft.set_width(604.0)?;
                Ok(())
            })
            .unwrap();
        let revision_before_conflict = project.document_revision();
        let b_before_conflict = width(&project, &b);
        assert_eq!(
            project.check_document_edit_history_group(history_group, HistoryDirection::Undo),
            Err(DocumentEditTransactionError::HistoryConflict {
                group: history_group,
                address: a.clone(),
            })
        );
        assert_eq!(project.document_revision(), revision_before_conflict);
        assert_eq!(
            project.replay_document_edit_history_group(history_group, HistoryDirection::Undo),
            Err(DocumentEditTransactionError::HistoryConflict {
                group: history_group,
                address: a.clone(),
            })
        );
        assert_eq!(project.document_revision(), revision_before_conflict);
        assert_eq!(width(&project, &a), 604.0);
        assert_eq!(width(&project, &b), b_before_conflict);
        assert_eq!(width(&project, &c), 703.0);
        assert_eq!(
            project.document_edit_history_group_state(history_group),
            Some(EditHistoryGroupState::Applied)
        );
    }
}
