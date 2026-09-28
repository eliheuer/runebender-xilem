// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bounded, guarded edit groups over canonical geometry and metrics in one source.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use kurbo::Point;

use super::*;
use crate::font::generated::{GeneratedContour, MAX_GENERATED_CONTOURS, MAX_GENERATED_POINTS};
use crate::font::history::HistoryDirection;
use crate::font::variable::GlyphId;
use crate::font::{AnchorId, CanonicalLayerSnapshot, ContourId, DocumentEditError, PointId};

const MAX_EDIT_LAYERS: usize = 64;
const MAX_EDIT_OPERATIONS: usize = 256;
const MAX_HISTORY_GROUPS: usize = 128;
const MAX_HISTORY_NAME_BYTES: usize = 256;

/// One supported operation in a canonical edit transaction.
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
    /// Append bounded ordinary contours with engine-minted stable identities.
    AppendContours(Vec<GeneratedContour>),
    /// Replace all ordinary contours with bounded generated contours and fresh identities.
    ReplaceContours(Vec<GeneratedContour>),
}

/// One object changed, inserted or removed by a committed canonical edit transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentEditObjectKind {
    /// The exact horizontal advance changed.
    Width,
    /// One existing point moved or a generated point inserted.
    Point(PointId),
    /// One generated contour was inserted.
    Contour(ContourId),
    /// One existing point was removed by contour replacement.
    RemovedPoint(PointId),
    /// One existing contour was removed by contour replacement.
    RemovedContour(ContourId),
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
    /// Object changed, inserted or removed.
    pub object: DocumentEditObjectKind,
}

impl DocumentEditOperation {
    fn apply(&self, draft: &mut super::super::LayerEditDraft) -> Result<(), DocumentEditError> {
        match self {
            Self::SetWidth(width) => {
                draft.set_width(*width)?;
            }
            Self::SetPoint { point, position } => {
                draft.set_point_position(*point, *position)?;
            }
            Self::SetAnchor { anchor, position } => {
                draft.set_anchor_position(*anchor, *position)?;
            }
            Self::AppendContours(contours) => {
                draft.append_generated_contours(contours)?;
            }
            Self::ReplaceContours(contours) => {
                draft.replace_generated_contours(contours)?;
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
        match operation {
            DocumentEditOperation::SetWidth(_) if before.width() != after.width() => {
                push(DocumentEditObjectKind::Width);
            }
            DocumentEditOperation::SetPoint { point, .. }
                if before.point_position(*point).is_some()
                    && after.point_position(*point).is_some()
                    && before.point_position(*point) != after.point_position(*point) =>
            {
                push(DocumentEditObjectKind::Point(*point));
            }
            DocumentEditOperation::SetAnchor { anchor, .. }
                if before.anchor_position(*anchor) != after.anchor_position(*anchor) =>
            {
                push(DocumentEditObjectKind::Anchor(*anchor));
            }
            DocumentEditOperation::AppendContours(_)
            | DocumentEditOperation::ReplaceContours(_) => {}
            DocumentEditOperation::SetWidth(_)
            | DocumentEditOperation::SetPoint { .. }
            | DocumentEditOperation::SetAnchor { .. } => {}
        }
    }
    if operations.iter().any(|operation| {
        matches!(
            operation,
            DocumentEditOperation::AppendContours(_) | DocumentEditOperation::ReplaceContours(_)
        )
    }) {
        let (before_contours, before_points) = before.contour_and_point_ids();
        let before_contours = before_contours.into_iter().collect::<BTreeSet<_>>();
        let before_points = before_points.into_iter().collect::<BTreeSet<_>>();
        let (after_contours, after_points) = after.contour_and_point_ids();
        let after_contours = after_contours.into_iter().collect::<BTreeSet<_>>();
        let after_points = after_points.into_iter().collect::<BTreeSet<_>>();
        for contour in &before_contours {
            if !after_contours.contains(contour) {
                push(DocumentEditObjectKind::RemovedContour(*contour));
            }
        }
        for point in &before_points {
            if !after_points.contains(point) {
                push(DocumentEditObjectKind::RemovedPoint(*point));
            }
        }
        for contour in after_contours {
            if !before_contours.contains(&contour) {
                push(DocumentEditObjectKind::Contour(contour));
            }
        }
        for point in after_points {
            if !before_points.contains(&point) {
                push(DocumentEditObjectKind::Point(point));
            }
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
    operation_count: usize,
    generated_contours: usize,
    generated_points: usize,
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
        let (generated_contours, generated_points) = edits
            .iter()
            .flat_map(|edit| &edit.operations)
            .filter_map(|operation| match operation {
                DocumentEditOperation::AppendContours(contours)
                | DocumentEditOperation::ReplaceContours(contours) => Some(contours),
                _ => None,
            })
            .fold(
                (0_usize, 0_usize),
                |(contour_count, point_count), contours| {
                    (
                        contour_count.saturating_add(contours.len()),
                        contours.iter().fold(point_count, |count, contour| {
                            count.saturating_add(contour.points.len())
                        }),
                    )
                },
            );
        if generated_contours > MAX_GENERATED_CONTOURS || generated_points > MAX_GENERATED_POINTS {
            return Err(DocumentEditTransactionError::Invalid(format!(
                "transaction may generate at most {MAX_GENERATED_CONTOURS} contours and {MAX_GENERATED_POINTS} points"
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
                    .ok_or_else(|| {
                        DocumentEditTransactionError::MissingLayer(before.address().clone())
                    })?
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

        if guarded.len() > MAX_EDIT_LAYERS {
            return Err(DocumentEditTransactionError::Invalid(format!(
                "transaction may guard at most {MAX_EDIT_LAYERS} unique layers"
            )));
        }
        Ok(CanonicalDocumentEditTransaction {
            source,
            history_name,
            reads: guarded.into_values().collect(),
            writes,
            operation_count,
            generated_contours,
            generated_points,
        })
    }

    /// Capture a layer from a staged transaction after validating its complete root read set.
    ///
    /// Edited layers use the staged after-state; untouched layers use their current root state.
    pub fn capture_document_edit_layer(
        &self,
        transaction: &CanonicalDocumentEditTransaction,
        address: &GlyphLayerAddress,
    ) -> Result<CanonicalLayerSnapshot, DocumentEditTransactionError> {
        validate_source(transaction.source, address)?;
        self.validate_transaction_reads(transaction)?;
        if let Some(edit) = transaction
            .writes
            .iter()
            .find(|edit| edit.after.address() == address)
        {
            return Ok(edit.after.clone());
        }
        self.capture_document_layer(address)
            .ok_or_else(|| DocumentEditTransactionError::MissingLayer(address.clone()))
    }

    /// Extend a staged edit from its immutable root guards and current proposed layer states.
    ///
    /// Every new dependency and edit target must match the parent's overlay exactly.
    /// The returned transaction commits the complete lineage as one history group.
    /// The parent can independently produce other branches.
    pub fn extend_document_edit_transaction(
        &self,
        parent: &CanonicalDocumentEditTransaction,
        history_name: impl Into<String>,
        reads: Vec<CanonicalLayerSnapshot>,
        edits: Vec<DocumentLayerEdit>,
    ) -> Result<CanonicalDocumentEditTransaction, DocumentEditTransactionError> {
        self.validate_transaction_reads(parent)?;
        let mut root_guards = parent
            .reads
            .iter()
            .map(|read| (read.address().clone(), read.clone()))
            .collect::<BTreeMap<_, _>>();
        for snapshot in reads.iter().chain(edits.iter().map(|edit| &edit.expected)) {
            validate_source(parent.source, snapshot.address())?;
            let address = snapshot.address();
            let overlay = parent
                .writes
                .iter()
                .find(|write| write.after.address() == address)
                .map(|write| &write.after);
            if let Some(expected) = overlay.or_else(|| root_guards.get(address)) {
                if snapshot != expected {
                    return Err(DocumentEditTransactionError::StaleLayer(address.clone()));
                }
            } else {
                self.validate_edit_snapshot(snapshot)?;
                insert_guard(&mut root_guards, snapshot)?;
            }
        }
        if root_guards.len() > MAX_EDIT_LAYERS {
            return Err(DocumentEditTransactionError::Invalid(format!(
                "transaction may guard at most {MAX_EDIT_LAYERS} unique layers"
            )));
        }

        // The ordinary staging engine applies the same typed operations to the overlay drafts.
        let child =
            self.begin_document_edit_transaction(parent.source, history_name, reads, edits)?;
        let operation_count = parent.operation_count.saturating_add(child.operation_count);
        let generated_contours = parent
            .generated_contours
            .saturating_add(child.generated_contours);
        let generated_points = parent
            .generated_points
            .saturating_add(child.generated_points);
        if operation_count > MAX_EDIT_OPERATIONS
            || generated_contours > MAX_GENERATED_CONTOURS
            || generated_points > MAX_GENERATED_POINTS
        {
            return Err(DocumentEditTransactionError::Invalid(format!(
                "transaction lineage may contain at most {MAX_EDIT_OPERATIONS} operations, {MAX_GENERATED_CONTOURS} generated contours, and {MAX_GENERATED_POINTS} generated points"
            )));
        }

        let mut writes = parent
            .writes
            .iter()
            .map(|write| (write.before.address().clone(), write.clone()))
            .collect::<BTreeMap<_, _>>();
        for child_write in child.writes {
            let address = child_write.before.address().clone();
            if let Some(previous) = writes.get_mut(&address) {
                let mut candidates = previous.changed_objects.clone();
                candidates.extend(child_write.changed_objects);
                previous.after = child_write.after;
                previous.changed_objects =
                    retain_changed_objects(&previous.before, &previous.after, candidates);
                if previous.before == previous.after {
                    writes.remove(&address);
                }
            } else {
                writes.insert(address, child_write);
            }
        }
        Ok(CanonicalDocumentEditTransaction {
            source: parent.source,
            history_name: child.history_name,
            reads: root_guards.into_values().collect(),
            writes: writes.into_values().collect(),
            operation_count,
            generated_contours,
            generated_points,
        })
    }

    fn validate_transaction_reads(
        &self,
        transaction: &CanonicalDocumentEditTransaction,
    ) -> Result<(), DocumentEditTransactionError> {
        for expected in &transaction.reads {
            self.validate_edit_snapshot(expected)?;
        }
        Ok(())
    }

    /// Read the proposed canonical layers after rechecking every transaction dependency.
    ///
    /// This does not publish changes, advance the document revision, or record history.
    /// An empty result means every staged operation was unchanged.
    pub fn preview_document_edit_transaction(
        &self,
        transaction: &CanonicalDocumentEditTransaction,
    ) -> Result<Vec<CanonicalLayerSnapshot>, DocumentEditTransactionError> {
        self.validate_transaction_reads(transaction)?;
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
        self.validate_transaction_reads(&transaction)?;
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

fn retain_changed_objects(
    before: &CanonicalLayerSnapshot,
    after: &CanonicalLayerSnapshot,
    candidates: Vec<DocumentEditChangedObject>,
) -> Vec<DocumentEditChangedObject> {
    let (before_contours, _) = before.contour_and_point_ids();
    let before_contours = before_contours.into_iter().collect::<BTreeSet<_>>();
    let (after_contours, _) = after.contour_and_point_ids();
    let after_contours = after_contours.into_iter().collect::<BTreeSet<_>>();
    let mut retained = Vec::new();
    for candidate in candidates {
        let changed = match candidate.object {
            DocumentEditObjectKind::Width => before.width() != after.width(),
            DocumentEditObjectKind::Point(id) => {
                after.point_position(id).is_some()
                    && before.point_position(id) != after.point_position(id)
            }
            DocumentEditObjectKind::Contour(id) => {
                !before_contours.contains(&id) && after_contours.contains(&id)
            }
            DocumentEditObjectKind::RemovedPoint(id) => {
                before.point_position(id).is_some() && after.point_position(id).is_none()
            }
            DocumentEditObjectKind::RemovedContour(id) => {
                before_contours.contains(&id) && !after_contours.contains(&id)
            }
            DocumentEditObjectKind::Anchor(id) => {
                before.anchor_position(id) != after.anchor_position(id)
            }
        };
        if changed && !retained.contains(&candidate) {
            retained.push(candidate);
        }
    }
    retained
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use norad::{Anchor, Contour, ContourPoint, Font, Glyph, Name, PointType};

    use super::*;
    use crate::font::LayerPointType;
    use crate::font::generated::GeneratedPoint;

    fn generated_contour() -> GeneratedContour {
        use LayerPointType::{Curve, Move, OffCurve};
        GeneratedContour {
            points: [
                (Point::new(0.0, 0.0), Move),
                (Point::new(25.0, 80.0), OffCurve),
                (Point::new(75.0, 80.0), OffCurve),
                (Point::new(100.0, 0.0), Curve),
            ]
            .into_iter()
            .map(|(position, point_type)| GeneratedPoint {
                position,
                point_type,
                smooth: false,
            })
            .collect(),
        }
    }

    fn generated_line_contour(point_count: usize) -> GeneratedContour {
        GeneratedContour {
            points: (0..point_count)
                .map(|index| GeneratedPoint {
                    position: Point::new(index as f64, 0.0),
                    point_type: if index == 0 {
                        LayerPointType::Move
                    } else {
                        LayerPointType::Line
                    },
                    smooth: false,
                })
                .collect(),
        }
    }

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
    fn chained_generated_point_keeps_identity_and_replays_exactly() {
        let mut project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let before = project.capture_document_layer(&a).unwrap();
        let parent = project
            .begin_document_edit_transaction(
                source,
                "append",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    before.clone(),
                    vec![DocumentEditOperation::AppendContours(vec![
                        generated_contour(),
                    ])],
                )],
            )
            .unwrap();
        let parent_after = project.capture_document_edit_layer(&parent, &a).unwrap();
        let generated = *parent_after.contour_and_point_ids().1.last().unwrap();
        let moved = Point::new(110.0, 10.0);
        let child = project
            .extend_document_edit_transaction(
                &parent,
                "append and move",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    parent_after.clone(),
                    vec![DocumentEditOperation::SetPoint {
                        point: generated,
                        position: moved,
                    }],
                )],
            )
            .unwrap();
        assert_eq!(
            project
                .capture_document_edit_layer(&parent, &a)
                .unwrap()
                .point_position(generated),
            parent_after.point_position(generated)
        );
        assert_eq!(
            project
                .capture_document_edit_layer(&child, &a)
                .unwrap()
                .point_position(generated),
            Some(moved)
        );
        let sibling = project
            .extend_document_edit_transaction(
                &parent,
                "other branch",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    parent_after,
                    vec![DocumentEditOperation::SetWidth(620.0)],
                )],
            )
            .unwrap();
        assert_ne!(
            project.capture_document_edit_layer(&sibling, &a).unwrap(),
            project.capture_document_edit_layer(&child, &a).unwrap()
        );
        let DocumentEditTransactionOutcome::Changed {
            changed_objects,
            history_group,
            ..
        } = project.commit_document_edit_transaction(child).unwrap()
        else {
            panic!("chained edit must change the document");
        };
        assert!(
            changed_objects
                .iter()
                .any(|item| { item.object == DocumentEditObjectKind::Point(generated) })
        );
        let after = project.capture_document_layer(&a).unwrap();
        assert_eq!(after.point_position(generated), Some(moved));
        project
            .replay_document_edit_history_group(history_group, HistoryDirection::Undo)
            .unwrap();
        assert_eq!(project.capture_document_layer(&a).unwrap(), before);
        project
            .replay_document_edit_history_group(history_group, HistoryDirection::Redo)
            .unwrap();
        assert_eq!(project.capture_document_layer(&a).unwrap(), after);
        assert_eq!(
            project
                .capture_document_layer(&a)
                .unwrap()
                .point_position(generated),
            Some(moved)
        );
    }

    #[test]
    fn chained_reads_guard_root_and_reject_old_overlay_targets() {
        let mut project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let b = address(&project, "B");
        let root_a = project.capture_document_layer(&a).unwrap();
        let root_b = project.capture_document_layer(&b).unwrap();
        let parent = project
            .begin_document_edit_transaction(
                source,
                "first",
                vec![root_b.clone()],
                vec![DocumentLayerEdit::new(
                    root_a.clone(),
                    vec![DocumentEditOperation::SetWidth(610.0)],
                )],
            )
            .unwrap();
        assert_eq!(
            project.capture_document_edit_layer(&parent, &b).unwrap(),
            root_b
        );
        assert_eq!(
            project
                .extend_document_edit_transaction(
                    &parent,
                    "old target",
                    Vec::new(),
                    vec![DocumentLayerEdit::new(
                        root_a,
                        vec![DocumentEditOperation::SetWidth(630.0)],
                    )],
                )
                .err(),
            Some(DocumentEditTransactionError::StaleLayer(a.clone()))
        );
        let staged_a = project.capture_document_edit_layer(&parent, &a).unwrap();
        let child = project
            .extend_document_edit_transaction(
                &parent,
                "second",
                vec![root_b],
                vec![DocumentLayerEdit::new(
                    staged_a,
                    vec![DocumentEditOperation::SetWidth(630.0)],
                )],
            )
            .unwrap();
        project
            .edit_document_layer("B", &b.layer, |draft| {
                draft.set_width(700.0)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            project.capture_document_edit_layer(&child, &a).err(),
            Some(DocumentEditTransactionError::StaleLayer(b.clone()))
        );
        assert_eq!(
            project
                .extend_document_edit_transaction(
                    &parent,
                    "stale root",
                    Vec::new(),
                    vec![DocumentLayerEdit::new(
                        project.capture_document_layer(&a).unwrap(),
                        vec![DocumentEditOperation::SetWidth(640.0)],
                    )],
                )
                .err(),
            Some(DocumentEditTransactionError::StaleLayer(b.clone()))
        );
        assert_eq!(
            project.commit_document_edit_transaction(child),
            Err(DocumentEditTransactionError::StaleLayer(b))
        );
        assert_eq!(width(&project, &a), 500.0);
    }

    #[test]
    fn chained_reversion_has_no_changed_object_or_history() {
        let mut project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let root = project.capture_document_layer(&a).unwrap();
        let parent = project
            .begin_document_edit_transaction(
                source,
                "forward",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    root.clone(),
                    vec![DocumentEditOperation::SetWidth(600.0)],
                )],
            )
            .unwrap();
        let child = project
            .extend_document_edit_transaction(
                &parent,
                "revert",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    project.capture_document_edit_layer(&parent, &a).unwrap(),
                    vec![DocumentEditOperation::SetWidth(root.width())],
                )],
            )
            .unwrap();
        assert!(
            project
                .preview_document_edit_transaction(&child)
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            project.commit_document_edit_transaction(child).unwrap(),
            DocumentEditTransactionOutcome::Unchanged { .. }
        ));
        assert_eq!(
            project.document_layer_history_depth(&a, HistoryDirection::Undo),
            0
        );
        assert_eq!(project.capture_document_layer(&a).unwrap(), root);
    }

    #[test]
    fn chained_limits_and_invalid_late_operation_leave_parent_usable() {
        let project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let root = project.capture_document_layer(&a).unwrap();
        let revision = project.document_revision();
        let parent = project
            .begin_document_edit_transaction(
                source,
                "many operations",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    root.clone(),
                    vec![DocumentEditOperation::SetWidth(601.0); 200],
                )],
            )
            .unwrap();
        let staged = project.capture_document_edit_layer(&parent, &a).unwrap();
        let too_many = project.extend_document_edit_transaction(
            &parent,
            "overflow",
            Vec::new(),
            vec![DocumentLayerEdit::new(
                staged.clone(),
                vec![DocumentEditOperation::SetWidth(602.0); 57],
            )],
        );
        assert!(matches!(
            too_many,
            Err(DocumentEditTransactionError::Invalid(_))
        ));

        let (point, anchor) = point_and_anchor(&project, &a);
        let invalid = project.extend_document_edit_transaction(
            &parent,
            "invalid late operation",
            Vec::new(),
            vec![DocumentLayerEdit::new(
                staged.clone(),
                vec![
                    DocumentEditOperation::SetPoint {
                        point,
                        position: Point::new(20.0, 30.0),
                    },
                    DocumentEditOperation::SetAnchor {
                        anchor,
                        position: Point::new(f64::INFINITY, 20.0),
                    },
                ],
            )],
        );
        assert!(matches!(
            invalid,
            Err(DocumentEditTransactionError::Operation(
                DocumentEditError::NonFinite
            ))
        ));
        assert_eq!(project.capture_document_layer(&a).unwrap(), root);
        assert_eq!(
            project.capture_document_edit_layer(&parent, &a).unwrap(),
            staged
        );

        let first_contours = vec![generated_contour(); MAX_GENERATED_CONTOURS / 2];
        let contour_parent = project
            .begin_document_edit_transaction(
                source,
                "many contours",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    root,
                    vec![DocumentEditOperation::AppendContours(first_contours)],
                )],
            )
            .unwrap();
        let extra_contours = vec![generated_contour(); MAX_GENERATED_CONTOURS / 2 + 1];
        let too_many_contours = project.extend_document_edit_transaction(
            &contour_parent,
            "contour overflow",
            Vec::new(),
            vec![DocumentLayerEdit::new(
                project
                    .capture_document_edit_layer(&contour_parent, &a)
                    .unwrap(),
                vec![DocumentEditOperation::AppendContours(extra_contours)],
            )],
        );
        assert!(matches!(
            too_many_contours,
            Err(DocumentEditTransactionError::Invalid(_))
        ));
        assert_eq!(project.document_revision(), revision);
    }

    #[test]
    fn chained_generated_point_limit_accepts_exact_total_and_rejects_one_more() {
        let project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let root = project.capture_document_layer(&a).unwrap();
        let parent = project
            .begin_document_edit_transaction(
                source,
                "first half",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    root.clone(),
                    vec![DocumentEditOperation::AppendContours(vec![
                        generated_line_contour(MAX_GENERATED_POINTS / 2),
                    ])],
                )],
            )
            .unwrap();
        let staged = project.capture_document_edit_layer(&parent, &a).unwrap();
        let exact = project
            .extend_document_edit_transaction(
                &parent,
                "exact point limit",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    staged.clone(),
                    vec![DocumentEditOperation::AppendContours(vec![
                        generated_line_contour(MAX_GENERATED_POINTS / 2),
                    ])],
                )],
            )
            .unwrap();
        assert_eq!(exact.generated_points, MAX_GENERATED_POINTS);
        assert_eq!(
            project
                .capture_document_edit_layer(&exact, &a)
                .unwrap()
                .contour_and_point_ids()
                .1
                .len(),
            root.contour_and_point_ids().1.len() + MAX_GENERATED_POINTS
        );
        let overflow = project.extend_document_edit_transaction(
            &parent,
            "one point over",
            Vec::new(),
            vec![DocumentLayerEdit::new(
                staged.clone(),
                vec![DocumentEditOperation::AppendContours(vec![
                    generated_line_contour(MAX_GENERATED_POINTS / 2 + 1),
                ])],
            )],
        );
        assert!(matches!(
            overflow,
            Err(DocumentEditTransactionError::Invalid(_))
        ));
        assert_eq!(
            project.capture_document_edit_layer(&parent, &a).unwrap(),
            staged
        );
        assert_eq!(project.capture_document_layer(&a).unwrap(), root);
    }

    #[test]
    fn chained_unique_layer_limit_accepts_64_and_rejects_65() {
        let mut font = Font::new();
        for index in 0..=MAX_EDIT_LAYERS {
            let name = format!("G{index}");
            font.default_layer_mut().insert_glyph(Glyph::new(&name));
        }
        let project = Project::from_source(SourceInput::from_font(
            font,
            PathBuf::from("many-layers.ufo"),
        ));
        let source = project.source_id(0).unwrap();
        let reads = (0..MAX_EDIT_LAYERS - 1)
            .map(|index| {
                project
                    .capture_document_layer(&address(&project, &format!("G{index}")))
                    .unwrap()
            })
            .collect();
        let target = address(&project, &format!("G{}", MAX_EDIT_LAYERS - 1));
        let root_target = project.capture_document_layer(&target).unwrap();
        let parent = project
            .begin_document_edit_transaction(
                source,
                "64 guarded layers",
                reads,
                vec![DocumentLayerEdit::new(
                    root_target.clone(),
                    vec![DocumentEditOperation::SetWidth(600.0)],
                )],
            )
            .unwrap();
        assert_eq!(parent.reads.len(), MAX_EDIT_LAYERS);
        let staged = project
            .capture_document_edit_layer(&parent, &target)
            .unwrap();
        let extra = address(&project, &format!("G{MAX_EDIT_LAYERS}"));
        let overflow = project.extend_document_edit_transaction(
            &parent,
            "65 guarded layers",
            vec![project.capture_document_layer(&extra).unwrap()],
            vec![DocumentLayerEdit::new(
                staged.clone(),
                vec![DocumentEditOperation::SetWidth(700.0)],
            )],
        );
        assert!(matches!(
            overflow,
            Err(DocumentEditTransactionError::Invalid(_))
        ));
        assert_eq!(
            project
                .capture_document_edit_layer(&parent, &target)
                .unwrap(),
            staged
        );
        assert_eq!(
            project.capture_document_layer(&target).unwrap(),
            root_target
        );
    }

    #[test]
    fn generated_contours_stage_commit_and_replay_with_stable_inserted_ids() {
        let mut project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let before = project.capture_document_layer(&a).unwrap();
        let before_ids = before.contour_and_point_ids();
        let before_revision = project.document_revision();
        let transaction = project
            .begin_document_edit_transaction(
                source,
                "generated outline",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    before.clone(),
                    vec![DocumentEditOperation::AppendContours(vec![
                        generated_contour(),
                    ])],
                )],
            )
            .unwrap();
        let preview = project
            .preview_document_edit_transaction(&transaction)
            .unwrap();
        assert_eq!(project.capture_document_layer(&a).unwrap(), before);
        assert_eq!(preview.len(), 1);
        assert_eq!(
            preview[0].contour_and_point_ids().0.len(),
            before_ids.0.len() + 1
        );

        let DocumentEditTransactionOutcome::Changed {
            after_revision,
            change,
            changed_objects,
            history_group,
            ..
        } = project
            .commit_document_edit_transaction(transaction)
            .unwrap()
        else {
            panic!("generated geometry must change the layer");
        };
        assert_eq!(after_revision, before_revision + 1);
        assert!(change.geometry_changed());
        assert!(!change.metrics_changed());
        let after = project.capture_document_layer(&a).unwrap();
        let after_ids = after.contour_and_point_ids();
        let added_contour = *after_ids.0.last().unwrap();
        let added_points = &after_ids.1[before_ids.1.len()..];
        assert_eq!(added_points.len(), 4);
        assert!(
            changed_objects.iter().any(|changed| {
                changed.object == DocumentEditObjectKind::Contour(added_contour)
            })
        );
        for point in added_points {
            assert!(
                changed_objects
                    .iter()
                    .any(|changed| { changed.object == DocumentEditObjectKind::Point(*point) })
            );
        }
        let layer = project.document_layer("A", &a.layer).unwrap();
        let appended = layer.contours().last().unwrap();
        assert!(!appended.is_closed());
        assert_eq!(appended.id(), added_contour);
        assert_eq!(
            appended
                .points()
                .map(|point| point.point_type())
                .collect::<Vec<_>>(),
            vec![
                LayerPointType::Move,
                LayerPointType::OffCurve,
                LayerPointType::OffCurve,
                LayerPointType::Curve,
            ]
        );

        project
            .replay_document_edit_history_group(history_group, HistoryDirection::Undo)
            .unwrap();
        assert_eq!(project.capture_document_layer(&a).unwrap(), before);
        project
            .replay_document_edit_history_group(history_group, HistoryDirection::Redo)
            .unwrap();
        assert_eq!(project.capture_document_layer(&a).unwrap(), after);
        assert_eq!(
            project
                .capture_document_layer(&a)
                .unwrap()
                .contour_and_point_ids(),
            after_ids
        );
    }

    #[test]
    fn replacement_preserves_other_layer_values_and_replays_exact_identities() {
        let mut font = Font::new();
        font.default_layer_mut().insert_glyph(Glyph::new("B"));
        let mut glyph = Glyph::new("A");
        glyph.width = 610.0;
        glyph.note = Some("keep this note".into());
        glyph.contours.push(Contour::new(
            vec![
                ContourPoint::new(0.0, 0.0, PointType::Line, false, None, None),
                ContourPoint::new(20.0, 20.0, PointType::Line, false, None, None),
            ],
            None,
        ));
        glyph.anchors.push(Anchor::new(
            30.0,
            50.0,
            Some(Name::new("top").unwrap()),
            None,
            None,
        ));
        glyph.components.push(norad::Component::new(
            Name::new("B").unwrap(),
            norad::AffineTransform::default(),
            None,
        ));
        font.default_layer_mut().insert_glyph(glyph);
        let mut project = Project::from_source(SourceInput::from_font(
            font,
            PathBuf::from("replace-fixture.ufo"),
        ));
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let b = address(&project, "B");
        let before = project.capture_document_layer(&a).unwrap();
        let before_b = project.capture_document_layer(&b).unwrap();
        let (old_contours, old_points) = before.contour_and_point_ids();
        let transaction = project
            .begin_document_edit_transaction(
                source,
                "replace A contours",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    before.clone(),
                    vec![DocumentEditOperation::ReplaceContours(vec![
                        generated_contour(),
                    ])],
                )],
            )
            .unwrap();
        let preview = project
            .preview_document_edit_transaction(&transaction)
            .unwrap();
        assert_eq!(project.capture_document_layer(&a).unwrap(), before);
        assert_eq!(project.capture_document_layer(&b).unwrap(), before_b);
        let after = &preview[0];
        let (new_contours, new_points) = after.contour_and_point_ids();
        assert_eq!(new_contours.len(), 1);
        assert_eq!(new_points.len(), 4);
        assert_ne!(old_contours, new_contours);
        assert!(old_points.iter().all(|id| !new_points.contains(id)));
        assert_eq!(after.view().width(), before.view().width());
        assert_eq!(after.view().note(), before.view().note());
        assert_eq!(
            after
                .view()
                .components()
                .map(|item| item.id())
                .collect::<Vec<_>>(),
            before
                .view()
                .components()
                .map(|item| item.id())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            after
                .view()
                .anchors()
                .map(|item| item.id())
                .collect::<Vec<_>>(),
            before
                .view()
                .anchors()
                .map(|item| item.id())
                .collect::<Vec<_>>()
        );

        let DocumentEditTransactionOutcome::Changed {
            changed_objects,
            history_group,
            ..
        } = project
            .commit_document_edit_transaction(transaction)
            .unwrap()
        else {
            panic!("replacement must change A");
        };
        for id in old_contours {
            assert!(
                changed_objects
                    .iter()
                    .any(|item| { item.object == DocumentEditObjectKind::RemovedContour(id) })
            );
        }
        for id in old_points {
            assert!(
                changed_objects
                    .iter()
                    .any(|item| { item.object == DocumentEditObjectKind::RemovedPoint(id) })
            );
        }
        for id in new_contours {
            assert!(
                changed_objects
                    .iter()
                    .any(|item| { item.object == DocumentEditObjectKind::Contour(id) })
            );
        }
        for id in new_points {
            assert!(
                changed_objects
                    .iter()
                    .any(|item| { item.object == DocumentEditObjectKind::Point(id) })
            );
        }
        assert_eq!(project.capture_document_layer(&b).unwrap(), before_b);
        project
            .replay_document_edit_history_group(history_group, HistoryDirection::Undo)
            .unwrap();
        assert_eq!(project.capture_document_layer(&a).unwrap(), before);
        project
            .replay_document_edit_history_group(history_group, HistoryDirection::Redo)
            .unwrap();
        assert_eq!(project.capture_document_layer(&a).unwrap(), *after);
    }

    #[test]
    fn replacement_rejects_bad_geometry_and_aggregates_mixed_operations() {
        let project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let before = project.capture_document_layer(&a).unwrap();
        let old_point = before.contour_and_point_ids().1[0];
        let mut invalid = generated_contour();
        invalid.points[0].position.x = f64::NAN;
        assert!(
            project
                .begin_document_edit_transaction(
                    source,
                    "invalid replacement",
                    Vec::new(),
                    vec![DocumentLayerEdit::new(
                        before.clone(),
                        vec![DocumentEditOperation::ReplaceContours(vec![invalid])],
                    )],
                )
                .is_err()
        );
        assert!(
            project
                .begin_document_edit_transaction(
                    source,
                    "stale point after replacement",
                    Vec::new(),
                    vec![DocumentLayerEdit::new(
                        before.clone(),
                        vec![
                            DocumentEditOperation::ReplaceContours(vec![generated_contour()]),
                            DocumentEditOperation::SetPoint {
                                point: old_point,
                                position: Point::new(5.0, 5.0),
                            },
                        ],
                    )],
                )
                .is_err()
        );
        let too_many = vec![generated_contour(); MAX_GENERATED_CONTOURS];
        assert!(
            project
                .begin_document_edit_transaction(
                    source,
                    "mixed geometry over limit",
                    Vec::new(),
                    vec![DocumentLayerEdit::new(
                        before.clone(),
                        vec![
                            DocumentEditOperation::ReplaceContours(too_many),
                            DocumentEditOperation::AppendContours(vec![generated_contour()]),
                        ],
                    )],
                )
                .is_err()
        );
        assert_eq!(project.capture_document_layer(&a).unwrap(), before);
    }

    #[test]
    fn closed_all_offcurve_quadratic_is_stored_as_an_ordinary_contour() {
        let mut project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let contour = GeneratedContour {
            points: [
                Point::new(0.0, 0.0),
                Point::new(50.0, 100.0),
                Point::new(100.0, 0.0),
            ]
            .into_iter()
            .map(|position| GeneratedPoint {
                position,
                point_type: LayerPointType::OffCurve,
                smooth: false,
            })
            .collect(),
        };
        let transaction = project
            .begin_document_edit_transaction(
                source,
                "closed quadratic",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    project.capture_document_layer(&a).unwrap(),
                    vec![DocumentEditOperation::AppendContours(vec![contour])],
                )],
            )
            .unwrap();
        project
            .commit_document_edit_transaction(transaction)
            .unwrap();
        let appended = project
            .document_layer("A", &a.layer)
            .unwrap()
            .contours()
            .last()
            .unwrap();
        assert!(appended.is_closed());
        assert!(!appended.is_hyper());
        assert!(
            appended
                .points()
                .all(|point| point.point_type() == LayerPointType::OffCurve)
        );
    }

    #[test]
    fn invalid_generated_draft_and_aggregate_batch_leave_document_unchanged() {
        let project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let before = project.capture_document_layer(&a).unwrap();
        let (layer, preserved) = before.clone().into_parts();
        let mut draft = super::super::super::LayerEditDraft::new(layer, preserved);
        let mut invalid = generated_contour();
        invalid.points[2].position.x = f64::INFINITY;
        assert_eq!(
            draft.append_generated_contours(&[generated_contour(), invalid]),
            Err(DocumentEditError::NonFinite)
        );
        let (layer, preserved) = draft.into_parts();
        assert_eq!(
            CanonicalLayerSnapshot::new(a.clone(), layer, preserved),
            before
        );

        let before_revision = project.document_revision();
        let batch = vec![generated_contour(); MAX_GENERATED_CONTOURS / 2 + 1];
        let result = project.begin_document_edit_transaction(
            source,
            "too many contours",
            Vec::new(),
            vec![DocumentLayerEdit::new(
                before.clone(),
                vec![
                    DocumentEditOperation::AppendContours(batch.clone()),
                    DocumentEditOperation::AppendContours(batch),
                ],
            )],
        );
        assert!(matches!(
            result,
            Err(DocumentEditTransactionError::Invalid(_))
        ));
        assert_eq!(project.document_revision(), before_revision);
        assert_eq!(project.capture_document_layer(&a).unwrap(), before);

        let long = GeneratedContour {
            points: vec![
                GeneratedPoint {
                    position: Point::new(0.0, 0.0),
                    point_type: LayerPointType::OffCurve,
                    smooth: false,
                };
                MAX_GENERATED_POINTS / 2 + 1
            ],
        };
        let result = project.begin_document_edit_transaction(
            source,
            "too many points",
            Vec::new(),
            vec![DocumentLayerEdit::new(
                before.clone(),
                vec![
                    DocumentEditOperation::AppendContours(vec![long.clone()]),
                    DocumentEditOperation::AppendContours(vec![long]),
                ],
            )],
        );
        assert!(matches!(
            result,
            Err(DocumentEditTransactionError::Invalid(_))
        ));
        assert_eq!(project.capture_document_layer(&a).unwrap(), before);
    }

    #[test]
    fn generated_contour_rejects_stale_guard_and_invalidates_component_dependents() {
        let mut project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let b = address(&project, "B");
        project
            .edit_document_layer("B", &b.layer, |draft| {
                draft.add_component("A".into(), kurbo::Affine::IDENTITY)?;
                Ok(())
            })
            .unwrap();
        let transaction = project
            .begin_document_edit_transaction(
                source,
                "guarded generated contour",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    project.capture_document_layer(&a).unwrap(),
                    vec![DocumentEditOperation::AppendContours(vec![
                        generated_contour(),
                    ])],
                )],
            )
            .unwrap();
        let DocumentEditTransactionOutcome::Changed { change, .. } = project
            .commit_document_edit_transaction(transaction)
            .unwrap()
        else {
            panic!("generated contour must publish");
        };
        assert!(change.dependent_layers().contains(&b));

        let stale = project
            .begin_document_edit_transaction(
                source,
                "stale generated contour",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    project.capture_document_layer(&a).unwrap(),
                    vec![DocumentEditOperation::AppendContours(vec![
                        generated_contour(),
                    ])],
                )],
            )
            .unwrap();
        project
            .edit_document_layer("A", &a.layer, |draft| {
                draft.set_width(777.0)?;
                Ok(())
            })
            .unwrap();
        let before_rejection = project.capture_document_layer(&a).unwrap();
        let revision = project.document_revision();
        assert_eq!(
            project.commit_document_edit_transaction(stale),
            Err(DocumentEditTransactionError::StaleLayer(a.clone()))
        );
        assert_eq!(project.document_revision(), revision);
        assert_eq!(
            project.capture_document_layer(&a).unwrap(),
            before_rejection
        );
    }

    #[test]
    fn generated_contour_rejects_deleted_target_without_panicking() {
        let mut project = project();
        let source = project.source_id(0).unwrap();
        let a = address(&project, "A");
        let captured = project.capture_document_layer(&a).unwrap();
        project.remove_document_glyph("A").unwrap();
        let revision = project.document_revision();
        let result = project.begin_document_edit_transaction(
            source,
            "stale generated contour",
            Vec::new(),
            vec![DocumentLayerEdit::new(
                captured,
                vec![DocumentEditOperation::AppendContours(vec![
                    generated_contour(),
                ])],
            )],
        );
        assert!(
            matches!(result, Err(DocumentEditTransactionError::MissingLayer(address)) if address == a)
        );
        assert_eq!(project.document_revision(), revision);
        assert!(project.document_glyph("A").is_none());
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
