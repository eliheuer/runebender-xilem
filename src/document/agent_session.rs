// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Document-epoch-scoped agent metadata and immutable operation receipts.
//!
//! This module does not parse a transport or own font data.
//! An application supplies the document epoch and actor identity, then stages edits against the
//! one canonical [`Project`] through [`AgentSession::apply_document_edit`].
//! The session retains a bounded, non-evicting idempotency ledger so an exact retry cannot apply a
//! transaction twice.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use sha2::{Digest as _, Sha256};

use super::project::{
    CanonicalDocumentEditTransaction, DocumentChange, DocumentEditChangedObject,
    DocumentEditTransactionError, DocumentEditTransactionOutcome, EditHistoryGroupId, Project,
};

const MAX_SESSION_ID_BYTES: usize = 256;
const MAX_OPERATION_KEY_BYTES: usize = 256;

/// Largest supported in-memory receipt ledger for one actor and document epoch.
pub const MAX_AGENT_RECEIPTS: usize = 4096;

/// Transport-supplied identity for one actor working in one open document epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSessionMetadata {
    document_epoch: String,
    actor: String,
}

impl AgentSessionMetadata {
    /// Validate and retain one document epoch and actor binding.
    pub fn new(
        document_epoch: impl Into<String>,
        actor: impl Into<String>,
    ) -> Result<Self, AgentSessionError> {
        let document_epoch = document_epoch.into();
        validate_identifier("document epoch", &document_epoch, MAX_SESSION_ID_BYTES)?;
        let actor = actor.into();
        validate_identifier("actor", &actor, MAX_SESSION_ID_BYTES)?;
        Ok(Self {
            document_epoch,
            actor,
        })
    }

    /// Epoch supplied by the application for the open document lifetime.
    pub fn document_epoch(&self) -> &str {
        &self.document_epoch
    }

    /// Actor bound to this session and its operation-key namespace.
    pub fn actor(&self) -> &str {
        &self.actor
    }
}

/// Actor-local idempotency key for one requested operation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AgentOperationKey(String);

impl AgentOperationKey {
    /// Validate an operation key before admitting it to a session ledger.
    pub fn new(value: impl Into<String>) -> Result<Self, AgentSessionError> {
        let value = value.into();
        validate_identifier("operation key", &value, MAX_OPERATION_KEY_BYTES)?;
        Ok(Self(value))
    }

    /// String form supplied by the actor.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// SHA-256 identity of the complete semantic canonical typed operation payload.
///
/// Transport adapters choose and document the canonical encoding before calling this constructor.
/// The digest is an idempotency identity, not authentication or authorization.
/// This layer deliberately does not interpret JSON, MCP or socket messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AgentPayloadDigest([u8; 32]);

impl AgentPayloadDigest {
    /// Hash the exact canonical payload bytes used to identify retries.
    pub fn sha256(canonical_payload: &[u8]) -> Self {
        Self(Sha256::digest(canonical_payload).into())
    }

    /// Lowercase hexadecimal representation suitable for a transport adapter.
    pub fn to_hex(self) -> String {
        let mut encoded = String::with_capacity(self.0.len() * 2);
        for byte in self.0 {
            write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
        }
        encoded
    }
}

/// A terminal reason recorded when an admitted operation cannot be staged or committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentOperationRejection {
    /// Typed request resolution or validation failed before a transaction was staged.
    InvalidRequest(String),
    /// Canonical staging or guarded publication rejected the transaction.
    Transaction(DocumentEditTransactionError),
}

impl From<DocumentEditTransactionError> for AgentOperationRejection {
    fn from(error: DocumentEditTransactionError) -> Self {
        Self::Transaction(error)
    }
}

/// Immutable original outcome of one admitted apply request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentOperationOutcome {
    /// A canonical transaction committed exactly once.
    Committed {
        /// Document revision immediately before publication.
        before_revision: u64,
        /// Document revision immediately after publication.
        after_revision: u64,
        /// Canonical invalidation scope produced by the commit.
        change: DocumentChange,
        /// Stable identities of the existing canonical objects that changed.
        changed_objects: Vec<DocumentEditChangedObject>,
        /// Project-owned handle shared by ordinary and targeted history replay.
        history_group: EditHistoryGroupId,
    },
    /// The staged transaction matched canonical state and published nothing.
    Unchanged {
        /// Current revision at the original apply attempt.
        revision: u64,
    },
    /// Cancellation won after staging and before canonical publication.
    Cancelled {
        /// Current revision when cancellation prevented publication.
        revision: u64,
    },
    /// The admitted request reached a terminal rejection without mutation.
    Rejected {
        /// Canonical revision after rejection.
        revision: u64,
        /// Stable original rejection replayed to exact retries.
        rejection: AgentOperationRejection,
    },
}

/// Immutable receipt retained under one actor and operation key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentOperationReceipt {
    document_epoch: String,
    actor: String,
    operation_key: AgentOperationKey,
    payload_digest: AgentPayloadDigest,
    outcome: AgentOperationOutcome,
}

impl AgentOperationReceipt {
    /// Document epoch in which this operation was originally attempted.
    pub fn document_epoch(&self) -> &str {
        &self.document_epoch
    }

    /// Actor that owns the operation-key namespace.
    pub fn actor(&self) -> &str {
        &self.actor
    }

    /// Actor-local idempotency key.
    pub fn operation_key(&self) -> &AgentOperationKey {
        &self.operation_key
    }

    /// Digest binding this key to its original canonical payload.
    pub fn payload_digest(&self) -> AgentPayloadDigest {
        self.payload_digest
    }

    /// Original apply outcome, independent of subsequent undo or redo state.
    pub fn outcome(&self) -> &AgentOperationOutcome {
        &self.outcome
    }

    /// Project-owned history handle when the original attempt committed.
    pub fn history_group(&self) -> Option<EditHistoryGroupId> {
        match &self.outcome {
            AgentOperationOutcome::Committed { history_group, .. } => Some(*history_group),
            AgentOperationOutcome::Unchanged { .. }
            | AgentOperationOutcome::Cancelled { .. }
            | AgentOperationOutcome::Rejected { .. } => None,
        }
    }
}

/// Whether an immutable receipt was recorded now or replayed from the ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentReceiptDisposition {
    /// The staging callback ran and this receipt was inserted now.
    Recorded,
    /// The exact payload matched an existing immutable receipt.
    Replayed,
}

/// Result of one admitted apply call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentApplyResult {
    receipt: AgentOperationReceipt,
    disposition: AgentReceiptDisposition,
}

impl AgentApplyResult {
    /// Immutable operation receipt.
    pub fn receipt(&self) -> &AgentOperationReceipt {
        &self.receipt
    }

    /// Whether the receipt was newly recorded or replayed.
    pub fn disposition(&self) -> AgentReceiptDisposition {
        self.disposition
    }

    /// Whether this call newly published a canonical transaction.
    ///
    /// Application cache refresh and UI-history insertion should happen only when this is true.
    pub fn is_new_commit(&self) -> bool {
        self.disposition == AgentReceiptDisposition::Recorded
            && matches!(
                &self.receipt.outcome,
                AgentOperationOutcome::Committed { .. }
            )
    }
}

/// Why session metadata or ledger admission was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentSessionError {
    /// A required bounded identifier was empty or too long.
    InvalidIdentifier {
        /// Name of the invalid field.
        field: &'static str,
        /// Maximum accepted UTF-8 byte count.
        max_bytes: usize,
    },
    /// Receipt capacity must fit the supported in-memory bound.
    InvalidReceiptCapacity {
        /// Requested receipt count.
        requested: usize,
        /// Largest accepted receipt count.
        maximum: usize,
    },
    /// This operation key is already bound to a different canonical payload.
    PayloadMismatch {
        /// Conflicting actor-local key.
        operation_key: AgentOperationKey,
    },
    /// The non-evicting ledger is full, so no new operation can be admitted.
    CapacityExhausted {
        /// Configured receipt capacity.
        capacity: usize,
    },
}

impl std::fmt::Display for AgentSessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIdentifier { field, max_bytes } => write!(
                formatter,
                "{field} must be non-empty and at most {max_bytes} bytes"
            ),
            Self::InvalidReceiptCapacity { requested, maximum } => write!(
                formatter,
                "receipt capacity {requested} must be in 1..={maximum}"
            ),
            Self::PayloadMismatch { operation_key } => write!(
                formatter,
                "operation key {} is already bound to another payload",
                operation_key.as_str()
            ),
            Self::CapacityExhausted { capacity } => {
                write!(formatter, "receipt capacity {capacity} is exhausted")
            }
        }
    }
}

impl std::error::Error for AgentSessionError {}

/// In-memory idempotency ledger for one actor and document epoch.
#[derive(Debug)]
pub struct AgentSession {
    metadata: AgentSessionMetadata,
    receipt_capacity: usize,
    receipts: BTreeMap<AgentOperationKey, AgentOperationReceipt>,
}

impl AgentSession {
    /// Create one bounded non-evicting ledger.
    pub fn new(
        metadata: AgentSessionMetadata,
        receipt_capacity: usize,
    ) -> Result<Self, AgentSessionError> {
        if !(1..=MAX_AGENT_RECEIPTS).contains(&receipt_capacity) {
            return Err(AgentSessionError::InvalidReceiptCapacity {
                requested: receipt_capacity,
                maximum: MAX_AGENT_RECEIPTS,
            });
        }
        Ok(Self {
            metadata,
            receipt_capacity,
            receipts: BTreeMap::new(),
        })
    }

    /// Immutable actor and document-epoch binding.
    pub fn metadata(&self) -> &AgentSessionMetadata {
        &self.metadata
    }

    /// Maximum number of retained receipts.
    pub fn receipt_capacity(&self) -> usize {
        self.receipt_capacity
    }

    /// Number of immutable receipts currently retained.
    pub fn receipt_count(&self) -> usize {
        self.receipts.len()
    }

    /// Look up an original receipt without consulting current history state.
    pub fn receipt(&self, operation_key: &AgentOperationKey) -> Option<&AgentOperationReceipt> {
        self.receipts.get(operation_key)
    }

    /// Admit, stage and atomically commit one canonical document transaction.
    ///
    /// Exact retries return the original receipt without invoking `stage`.
    /// Payload conflicts and capacity exhaustion are detected before `stage`, so they cannot
    /// mutate the project.
    /// Staging receives only an immutable Project borrow and publication always uses the guarded
    /// canonical transaction API.
    pub fn apply_document_edit<F>(
        &mut self,
        project: &mut Project,
        operation_key: AgentOperationKey,
        payload_digest: AgentPayloadDigest,
        stage: F,
    ) -> Result<AgentApplyResult, AgentSessionError>
    where
        F: FnOnce(&Project) -> Result<CanonicalDocumentEditTransaction, AgentOperationRejection>,
    {
        self.apply_document_edit_with_precommit(
            project,
            operation_key,
            payload_digest,
            stage,
            || Ok(true),
        )
    }

    /// Admit and stage one edit, then consult a single hook immediately before publication.
    ///
    /// `Ok(false)` records an immutable cancelled receipt and never invokes the canonical commit
    /// operation.
    /// A hook error records a rejected receipt, keeping unavailable or inconsistent cancellation
    /// state distinct from a confirmed cancellation.
    /// Exact retries return that receipt without staging or consulting the hook again.
    pub fn apply_document_edit_with_precommit<F, C>(
        &mut self,
        project: &mut Project,
        operation_key: AgentOperationKey,
        payload_digest: AgentPayloadDigest,
        stage: F,
        precommit: C,
    ) -> Result<AgentApplyResult, AgentSessionError>
    where
        F: FnOnce(&Project) -> Result<CanonicalDocumentEditTransaction, AgentOperationRejection>,
        C: FnOnce() -> Result<bool, AgentOperationRejection>,
    {
        if let Some(receipt) = self.receipts.get(&operation_key) {
            if receipt.payload_digest != payload_digest {
                return Err(AgentSessionError::PayloadMismatch { operation_key });
            }
            return Ok(AgentApplyResult {
                receipt: receipt.clone(),
                disposition: AgentReceiptDisposition::Replayed,
            });
        }
        if self.receipts.len() == self.receipt_capacity {
            return Err(AgentSessionError::CapacityExhausted {
                capacity: self.receipt_capacity,
            });
        }

        let outcome = match stage(project) {
            Ok(transaction) => match precommit() {
                Ok(false) => AgentOperationOutcome::Cancelled {
                    revision: project.document_revision(),
                },
                Err(rejection) => AgentOperationOutcome::Rejected {
                    revision: project.document_revision(),
                    rejection,
                },
                Ok(true) => match project.commit_document_edit_transaction(transaction) {
                    Ok(DocumentEditTransactionOutcome::Changed {
                        before_revision,
                        after_revision,
                        change,
                        changed_objects,
                        history_group,
                    }) => AgentOperationOutcome::Committed {
                        before_revision,
                        after_revision,
                        change,
                        changed_objects,
                        history_group,
                    },
                    Ok(DocumentEditTransactionOutcome::Unchanged { revision }) => {
                        AgentOperationOutcome::Unchanged { revision }
                    }
                    Err(error) => AgentOperationOutcome::Rejected {
                        revision: project.document_revision(),
                        rejection: error.into(),
                    },
                },
            },
            Err(rejection) => AgentOperationOutcome::Rejected {
                revision: project.document_revision(),
                rejection,
            },
        };
        let receipt = AgentOperationReceipt {
            document_epoch: self.metadata.document_epoch.clone(),
            actor: self.metadata.actor.clone(),
            operation_key: operation_key.clone(),
            payload_digest,
            outcome,
        };
        self.receipts.insert(operation_key, receipt.clone());
        Ok(AgentApplyResult {
            receipt,
            disposition: AgentReceiptDisposition::Recorded,
        })
    }
}

fn validate_identifier(
    field: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), AgentSessionError> {
    if value.trim().is_empty() || value.len() > max_bytes {
        return Err(AgentSessionError::InvalidIdentifier { field, max_bytes });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use norad::{Anchor, Contour, ContourPoint, Font, Glyph, Name, PointType};

    use super::*;
    use crate::document::history::HistoryDirection;
    use crate::document::project::{
        DocumentEditOperation, DocumentLayerEdit, EditHistoryGroupState, SourceInput,
    };
    use crate::document::variable::GlyphLayerAddress;

    fn project() -> Project {
        let mut font = Font::new();
        for (index, name) in ["A", "B"].into_iter().enumerate() {
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
            PathBuf::from("agent-session-fixture.ufo"),
        ))
    }

    fn session(capacity: usize) -> AgentSession {
        AgentSession::new(
            AgentSessionMetadata::new("document-epoch", "test-actor").unwrap(),
            capacity,
        )
        .unwrap()
    }

    fn key(value: &str) -> AgentOperationKey {
        AgentOperationKey::new(value).unwrap()
    }

    fn digest(value: &str) -> AgentPayloadDigest {
        AgentPayloadDigest::sha256(value.as_bytes())
    }

    fn address(project: &Project, glyph: &str) -> GlyphLayerAddress {
        let source = project.source_id(0).unwrap();
        GlyphLayerAddress {
            glyph: glyph.into(),
            layer: project.document_source(source).unwrap().default_layer(),
        }
    }

    fn width(project: &Project, address: &GlyphLayerAddress) -> f64 {
        project
            .document_layer(&address.glyph, &address.layer)
            .unwrap()
            .width()
    }

    fn stage_width(
        project: &Project,
        address: &GlyphLayerAddress,
        width: f64,
    ) -> Result<CanonicalDocumentEditTransaction, AgentOperationRejection> {
        project
            .begin_document_edit_transaction(
                address.layer.source,
                format!("agent: set {} width", address.glyph),
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    project.capture_document_layer(address).unwrap(),
                    vec![DocumentEditOperation::SetWidth(width)],
                )],
            )
            .map_err(Into::into)
    }

    #[test]
    fn exact_retry_replays_committed_receipt_without_staging() {
        let mut project = project();
        let a = address(&project, "A");
        let mut session = session(4);
        let operation_key = key("widen-a");
        let payload = digest("set A width 540");

        let first = session
            .apply_document_edit(&mut project, operation_key.clone(), payload, |project| {
                stage_width(project, &a, 540.0)
            })
            .unwrap();
        let revision_after_first = project.document_revision();
        let changed_objects = match first.receipt().outcome() {
            AgentOperationOutcome::Committed {
                changed_objects, ..
            } => changed_objects,
            AgentOperationOutcome::Unchanged { .. }
            | AgentOperationOutcome::Cancelled { .. }
            | AgentOperationOutcome::Rejected { .. } => panic!("the width edit must commit"),
        };
        assert_eq!(
            changed_objects,
            &[DocumentEditChangedObject {
                glyph: "A".into(),
                glyph_id: project.document_glyph("A").unwrap().id(),
                layer: a.layer.clone(),
                object: crate::document::project::DocumentEditObjectKind::Width,
            }]
        );
        let replay = session
            .apply_document_edit(&mut project, operation_key, payload, |_| {
                panic!("an exact retry must not stage again")
            })
            .unwrap();

        assert!(first.is_new_commit());
        assert_eq!(first.disposition(), AgentReceiptDisposition::Recorded);
        assert_eq!(replay.disposition(), AgentReceiptDisposition::Replayed);
        assert!(!replay.is_new_commit());
        assert_eq!(replay.receipt(), first.receipt());
        assert_eq!(project.document_revision(), revision_after_first);
        assert_eq!(width(&project, &a), 540.0);
    }

    #[test]
    fn same_key_with_different_payload_rejects_before_staging() {
        let mut project = project();
        let a = address(&project, "A");
        let mut session = session(4);
        let operation_key = key("widen-a");
        session
            .apply_document_edit(
                &mut project,
                operation_key.clone(),
                digest("set A width 540"),
                |project| stage_width(project, &a, 540.0),
            )
            .unwrap();
        let before_revision = project.document_revision();

        let result = session.apply_document_edit(
            &mut project,
            operation_key.clone(),
            digest("set A width 560"),
            |_| panic!("a payload mismatch must not stage"),
        );

        assert_eq!(
            result,
            Err(AgentSessionError::PayloadMismatch { operation_key })
        );
        assert_eq!(project.document_revision(), before_revision);
        assert_eq!(width(&project, &a), 540.0);
        assert_eq!(session.receipt_count(), 1);
    }

    #[test]
    fn capacity_exhaustion_rejects_before_staging_or_mutation() {
        let mut project = project();
        let a = address(&project, "A");
        let mut session = session(1);
        let initial_revision = project.document_revision();
        let unchanged = session
            .apply_document_edit(
                &mut project,
                key("unchanged-a"),
                digest("keep A width 500"),
                |project| stage_width(project, &a, 500.0),
            )
            .unwrap();
        let before_revision = project.document_revision();

        let result = session.apply_document_edit(
            &mut project,
            key("widen-a"),
            digest("set A width 540"),
            |_| panic!("a full ledger must reject before staging"),
        );

        assert!(matches!(
            unchanged.receipt().outcome(),
            AgentOperationOutcome::Unchanged { revision } if *revision == initial_revision
        ));
        assert!(!unchanged.is_new_commit());
        assert_eq!(
            result,
            Err(AgentSessionError::CapacityExhausted { capacity: 1 })
        );
        assert_eq!(project.document_revision(), before_revision);
        assert_eq!(width(&project, &a), 500.0);
    }

    #[test]
    fn stale_guard_records_atomic_terminal_rejection() {
        let mut project = project();
        let a = address(&project, "A");
        let b = address(&project, "B");
        let source = a.layer.source;
        let transaction = project
            .begin_document_edit_transaction(
                source,
                "agent: guarded widths",
                Vec::new(),
                vec![
                    DocumentLayerEdit::new(
                        project.capture_document_layer(&a).unwrap(),
                        vec![DocumentEditOperation::SetWidth(540.0)],
                    ),
                    DocumentLayerEdit::new(
                        project.capture_document_layer(&b).unwrap(),
                        vec![DocumentEditOperation::SetWidth(560.0)],
                    ),
                ],
            )
            .unwrap();
        project
            .edit_document_layer("B", &b.layer, |draft| {
                draft.set_width(525.0)?;
                Ok(())
            })
            .unwrap();
        let revision_before_rejection = project.document_revision();
        let mut session = session(4);
        let operation_key = key("guarded-widths");
        let payload = digest("guarded widths");

        let rejected = session
            .apply_document_edit(&mut project, operation_key.clone(), payload, |_| {
                Ok(transaction)
            })
            .unwrap();
        let replay = session
            .apply_document_edit(&mut project, operation_key, payload, |_| {
                panic!("a rejected operation retry must not stage again")
            })
            .unwrap();

        assert!(matches!(
            rejected.receipt().outcome(),
            AgentOperationOutcome::Rejected {
                revision,
                rejection: AgentOperationRejection::Transaction(
                    DocumentEditTransactionError::StaleLayer(address)
                ),
            } if *revision == revision_before_rejection && address == &b
        ));
        assert_eq!(replay.disposition(), AgentReceiptDisposition::Replayed);
        assert_eq!(replay.receipt(), rejected.receipt());
        assert_eq!(project.document_revision(), revision_before_rejection);
        assert_eq!(width(&project, &a), 500.0);
        assert_eq!(width(&project, &b), 525.0);
    }

    #[test]
    fn staging_rejection_is_retained_without_mutation() {
        let mut project = project();
        let a = address(&project, "A");
        let before_revision = project.document_revision();
        let mut session = session(4);

        let result = session
            .apply_document_edit(
                &mut project,
                key("invalid-request"),
                digest("invalid request"),
                |_| {
                    Err(AgentOperationRejection::InvalidRequest(
                        "missing point".into(),
                    ))
                },
            )
            .unwrap();

        assert!(matches!(
            result.receipt().outcome(),
            AgentOperationOutcome::Rejected {
                revision,
                rejection: AgentOperationRejection::InvalidRequest(message),
            } if *revision == before_revision && message == "missing point"
        ));
        assert!(!result.is_new_commit());
        assert_eq!(project.document_revision(), before_revision);
        assert_eq!(width(&project, &a), 500.0);
    }

    #[test]
    fn retry_after_undo_returns_original_receipt_without_redo() {
        let mut project = project();
        let a = address(&project, "A");
        let mut session = session(4);
        let operation_key = key("widen-a");
        let payload = digest("set A width 540");
        let applied = session
            .apply_document_edit(&mut project, operation_key.clone(), payload, |project| {
                stage_width(project, &a, 540.0)
            })
            .unwrap();
        let history_group = applied.receipt().history_group().unwrap();
        let original_after_revision = match applied.receipt().outcome() {
            AgentOperationOutcome::Committed { after_revision, .. } => *after_revision,
            AgentOperationOutcome::Unchanged { .. }
            | AgentOperationOutcome::Cancelled { .. }
            | AgentOperationOutcome::Rejected { .. } => panic!("the first operation must commit"),
        };
        project
            .replay_document_edit_history_group(history_group, HistoryDirection::Undo)
            .unwrap();
        let undone_revision = project.document_revision();

        let replay = session
            .apply_document_edit(&mut project, operation_key, payload, |_| {
                panic!("retry after undo must not stage or redo")
            })
            .unwrap();

        assert_eq!(replay.disposition(), AgentReceiptDisposition::Replayed);
        assert!(!replay.is_new_commit());
        assert_eq!(replay.receipt(), applied.receipt());
        assert!(matches!(
            replay.receipt().outcome(),
            AgentOperationOutcome::Committed { after_revision, .. }
                if *after_revision == original_after_revision
        ));
        assert_eq!(project.document_revision(), undone_revision);
        assert_eq!(width(&project, &a), 500.0);
        assert_eq!(
            project.document_edit_history_group_state(history_group),
            Some(EditHistoryGroupState::Undone)
        );
    }

    #[test]
    fn precommit_cancellation_is_terminal_and_never_publishes() {
        let mut project = project();
        let address = address(&project, "A");
        let before_revision = project.document_revision();
        let mut session = session(4);
        let operation_key = key("cancel-before-commit");
        let payload = digest("cancelled payload");
        let cancelled = session
            .apply_document_edit_with_precommit(
                &mut project,
                operation_key.clone(),
                payload,
                |project| stage_width(project, &address, 540.0),
                || Ok(false),
            )
            .unwrap();
        assert!(matches!(
            cancelled.receipt().outcome(),
            AgentOperationOutcome::Cancelled { revision } if *revision == before_revision
        ));
        assert_eq!(width(&project, &address), 500.0);
        assert_eq!(project.document_revision(), before_revision);
        assert!(cancelled.receipt().history_group().is_none());

        let retry = session
            .apply_document_edit_with_precommit(
                &mut project,
                operation_key,
                payload,
                |_| panic!("cancelled retry must not stage"),
                || panic!("cancelled retry must not claim commit"),
            )
            .unwrap();
        assert_eq!(retry.disposition(), AgentReceiptDisposition::Replayed);
        assert_eq!(retry.receipt(), cancelled.receipt());
        assert_eq!(width(&project, &address), 500.0);
    }

    #[test]
    fn precommit_failure_is_rejected_not_mislabeled_as_cancelled() {
        let mut project = project();
        let address = address(&project, "A");
        let before_revision = project.document_revision();
        let mut session = session(1);
        let rejected = session
            .apply_document_edit_with_precommit(
                &mut project,
                key("missing-cancellation-state"),
                digest("payload"),
                |project| stage_width(project, &address, 540.0),
                || {
                    Err(AgentOperationRejection::InvalidRequest(
                        "cancellation state unavailable".into(),
                    ))
                },
            )
            .unwrap();
        assert!(matches!(
            rejected.receipt().outcome(),
            AgentOperationOutcome::Rejected { revision, rejection }
                if *revision == before_revision
                    && matches!(rejection, AgentOperationRejection::InvalidRequest(message)
                        if message == "cancellation state unavailable")
        ));
        assert_eq!(width(&project, &address), 500.0);
        assert_eq!(project.document_revision(), before_revision);
    }

    #[test]
    fn metadata_and_payload_digest_are_bounded_and_stable() {
        assert!(AgentSessionMetadata::new("", "actor").is_err());
        assert!(AgentSessionMetadata::new("epoch", " ").is_err());
        assert!(AgentOperationKey::new(" ").is_err());
        assert!(matches!(
            AgentSession::new(
                AgentSessionMetadata::new("epoch", "actor").unwrap(),
                MAX_AGENT_RECEIPTS + 1,
            ),
            Err(AgentSessionError::InvalidReceiptCapacity { .. })
        ));
        assert_eq!(
            digest("payload").to_hex(),
            "239f59ed55e737c77147cf55ad0c1b030b6d7ee748a7426952f9b852d5a935e5"
        );
    }
}
