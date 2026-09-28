// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Typed wire receipts for guarded edits, history replay, and cancellation.
//!
//! Engine sessions retain canonical outcomes without transport JSON.
//! These DTOs project those outcomes into the existing live response shape.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::agent_cancellation::AgentCancellationOutcome;
use super::super::agent_session::{
    AgentOperationOutcome, AgentOperationReceipt, AgentOperationRejection,
};
use crate::font::project::{DocumentEditObjectKind, EditHistoryGroupState, Project};

/// One canonical layer affected by a committed transaction.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ChangedLayer {
    /// Glyph name at publication.
    pub glyph: String,
    /// Stable source index.
    pub source: usize,
    /// Layer name within the source.
    pub layer: String,
}

/// Stable identity of one changed canonical object.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangedObject {
    /// A glyph layer's advance width changed.
    Width {
        /// Glyph name at publication.
        glyph: String,
        /// Opaque glyph identity.
        glyph_id: String,
        /// Stable source index.
        source: usize,
        /// Layer name within the source.
        layer: String,
    },
    /// An existing contour point moved.
    Point {
        /// Glyph name at publication.
        glyph: String,
        /// Opaque glyph identity.
        glyph_id: String,
        /// Stable source index.
        source: usize,
        /// Layer name within the source.
        layer: String,
        /// Opaque point identity.
        point_id: String,
    },
    /// An existing anchor moved.
    Anchor {
        /// Glyph name at publication.
        glyph: String,
        /// Opaque glyph identity.
        glyph_id: String,
        /// Stable source index.
        source: usize,
        /// Layer name within the source.
        layer: String,
        /// Opaque anchor identity.
        anchor_id: String,
    },
}

/// Cancellation marker retained inside a prevented receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PreventedCancellation {
    /// The operation was prevented before publication.
    Prevented,
}

/// Error code retained inside a prevented receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CancelledErrorCode {
    /// Cancellation prevented publication.
    Cancelled,
}

/// Original immutable apply outcome with fields required by its lifecycle state.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentReceiptOutcome {
    /// A canonical edit published and received one history group.
    Committed(Box<CommittedOutcome>),
    /// Staging matched the current canonical document without publishing.
    Unchanged {
        /// Revision observed at the original attempt.
        revision: u64,
        /// No canonical objects changed.
        changed_objects: Vec<ChangedObject>,
    },
    /// Cancellation prevented publication after staging.
    Cancelled {
        /// Cancellation disposition.
        cancellation: PreventedCancellation,
        /// Revision when publication was prevented.
        revision: u64,
        /// Stable original cancellation message.
        error: String,
        /// Stable original cancellation error code.
        error_code: CancelledErrorCode,
        /// No canonical objects changed.
        changed_objects: Vec<ChangedObject>,
    },
    /// An admitted operation ended in a stable rejection.
    Rejected {
        /// Revision after rejection.
        revision: u64,
        /// Stable original rejection message.
        error: String,
        /// No canonical objects changed.
        changed_objects: Vec<ChangedObject>,
    },
}

/// Complete committed outcome, including exact history and changed object identities.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct CommittedOutcome {
    /// Document revision immediately before publication.
    pub before_revision: u64,
    /// Document revision immediately after publication.
    pub after_revision: u64,
    /// Project-owned history group handle.
    pub history_group: String,
    /// Canonical layers affected by publication.
    pub changed_layers: Vec<ChangedLayer>,
    /// Stable identities of changed canonical objects.
    pub changed_objects: Vec<ChangedObject>,
}

/// Immutable identity and original outcome of an admitted apply.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct AgentReceipt {
    /// Editor document lifetime in which the attempt was admitted.
    pub document_epoch: String,
    /// Caller namespace for the operation key.
    pub actor: String,
    /// Actor-local retry identity.
    pub operation_key: String,
    /// SHA-256 of the complete normalized payload.
    pub payload_sha256: String,
    /// Original immutable outcome.
    pub outcome: AgentReceiptOutcome,
}

/// Current state of the committed history group, separate from the original outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistoryState {
    /// The committed group is currently applied.
    Applied,
    /// The committed group is currently undone.
    Undone,
    /// The group is no longer available in application history.
    Unavailable,
}

/// Normal response to `agent_apply`, including retained non-success receipts.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct AgentApplyResponse {
    /// Whether the original apply outcome committed or was unchanged.
    pub ok: bool,
    /// Edits remain in memory until the user saves explicitly.
    pub saved: bool,
    /// Current state of the committed history group, or null for other outcomes.
    pub history_state: Option<HistoryState>,
    /// Immutable original receipt.
    pub receipt: AgentReceipt,
    /// Whether this exact payload replayed an existing receipt.
    pub replayed: bool,
    /// Whether this call newly changed the canonical root.
    pub root_changed: bool,
}

/// Normal response to `agent_receipt`, including rejected or cancelled receipts.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct AgentReceiptResponse {
    /// A retained receipt was found, regardless of its original outcome.
    pub ok: bool,
    /// Receipt lookup never saves a font.
    pub saved: bool,
    /// Current state of the committed history group, or null for other outcomes.
    pub history_state: Option<HistoryState>,
    /// Immutable original receipt.
    pub receipt: AgentReceipt,
}

/// Normal response to a successful `agent_history` replay.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct AgentHistoryResponse {
    /// History replay succeeded.
    pub ok: bool,
    /// History replay never saves a font.
    pub saved: bool,
    /// Current state of the committed history group.
    pub history_state: Option<HistoryState>,
    /// Immutable original commit receipt.
    pub receipt: AgentReceipt,
    /// A history group was replayed.
    pub history_replayed: bool,
    /// Revision immediately before the replay.
    pub replay_before_revision: u64,
    /// Revision immediately after the replay.
    pub replay_after_revision: u64,
}

/// Outcome of a cancellation request at the socket admission boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CancellationStatus {
    /// The operation cannot now commit.
    Prevented,
    /// The same operation was already prevented.
    AlreadyPrevented,
    /// The application claimed the commit boundary.
    TooLate,
    /// The canonical transaction already committed.
    Committed,
    /// The operation completed without committing.
    Completed,
    /// No operation with this identity was admitted.
    Unknown,
}

/// Normal `agent_cancel` response, including terminal non-success outcomes.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct AgentCancelResponse {
    /// Whether cancellation prevented, or had already prevented, publication.
    pub ok: bool,
    /// Exact cancellation disposition.
    pub cancellation_status: CancellationStatus,
    /// Cancellation never saves a font.
    pub saved: bool,
}

impl AgentReceipt {
    /// Project an immutable engine receipt into its complete wire identity and outcome.
    pub fn from_engine(receipt: &AgentOperationReceipt) -> Self {
        let outcome = match receipt.outcome() {
            AgentOperationOutcome::Committed {
                before_revision,
                after_revision,
                change,
                changed_objects,
                history_group,
            } => AgentReceiptOutcome::Committed(Box::new(CommittedOutcome {
                before_revision: *before_revision,
                after_revision: *after_revision,
                history_group: history_group.to_wire(),
                changed_layers: change
                    .affected_layers()
                    .iter()
                    .map(|address| ChangedLayer {
                        glyph: address.glyph.clone(),
                        source: address.layer.source.0,
                        layer: address.layer.name.clone(),
                    })
                    .collect(),
                changed_objects: changed_objects
                    .iter()
                    .map(|changed| {
                        let glyph = changed.glyph.clone();
                        let glyph_id = changed.glyph_id.to_wire();
                        let source = changed.layer.source.0;
                        let layer = changed.layer.name.clone();
                        match changed.object {
                            DocumentEditObjectKind::Width => ChangedObject::Width {
                                glyph,
                                glyph_id,
                                source,
                                layer,
                            },
                            DocumentEditObjectKind::Point(point_id) => ChangedObject::Point {
                                glyph,
                                glyph_id,
                                source,
                                layer,
                                point_id: point_id.to_wire(),
                            },
                            DocumentEditObjectKind::Anchor(anchor_id) => ChangedObject::Anchor {
                                glyph,
                                glyph_id,
                                source,
                                layer,
                                anchor_id: anchor_id.to_wire(),
                            },
                        }
                    })
                    .collect(),
            })),
            AgentOperationOutcome::Unchanged { revision } => AgentReceiptOutcome::Unchanged {
                revision: *revision,
                changed_objects: Vec::new(),
            },
            AgentOperationOutcome::Cancelled { revision } => AgentReceiptOutcome::Cancelled {
                cancellation: PreventedCancellation::Prevented,
                revision: *revision,
                error: "operation cancelled before commit".into(),
                error_code: CancelledErrorCode::Cancelled,
                changed_objects: Vec::new(),
            },
            AgentOperationOutcome::Rejected {
                revision,
                rejection,
            } => AgentReceiptOutcome::Rejected {
                revision: *revision,
                error: match rejection {
                    AgentOperationRejection::InvalidRequest(message) => message.clone(),
                    AgentOperationRejection::Transaction(error) => error.to_string(),
                },
                changed_objects: Vec::new(),
            },
        };
        Self {
            document_epoch: receipt.document_epoch().into(),
            actor: receipt.actor().into(),
            operation_key: receipt.operation_key().as_str().into(),
            payload_sha256: receipt.payload_digest().to_hex(),
            outcome,
        }
    }
}

/// Return the current history state of one immutable receipt.
pub fn history_state(receipt: &AgentOperationReceipt, project: &Project) -> Option<HistoryState> {
    receipt.history_group().map(
        |group| match project.document_edit_history_group_state(group) {
            Some(EditHistoryGroupState::Applied) => HistoryState::Applied,
            Some(EditHistoryGroupState::Undone) => HistoryState::Undone,
            None => HistoryState::Unavailable,
        },
    )
}

impl AgentApplyResponse {
    /// Construct an apply result without changing the original retained receipt.
    pub fn from_engine(
        receipt: &AgentOperationReceipt,
        project: &Project,
        replayed: bool,
        root_changed: bool,
    ) -> Self {
        Self {
            ok: !matches!(
                receipt.outcome(),
                AgentOperationOutcome::Rejected { .. } | AgentOperationOutcome::Cancelled { .. }
            ),
            saved: false,
            history_state: history_state(receipt, project),
            receipt: AgentReceipt::from_engine(receipt),
            replayed,
            root_changed,
        }
    }
}

impl AgentReceiptResponse {
    /// Construct a successful lookup even when the original apply did not succeed.
    pub fn from_engine(receipt: &AgentOperationReceipt, project: &Project) -> Self {
        Self {
            ok: true,
            saved: false,
            history_state: history_state(receipt, project),
            receipt: AgentReceipt::from_engine(receipt),
        }
    }
}

impl AgentHistoryResponse {
    /// Construct a successful history replay with both revision boundaries.
    pub fn from_engine(
        receipt: &AgentOperationReceipt,
        project: &Project,
        before_revision: u64,
        after_revision: u64,
    ) -> Self {
        Self {
            ok: true,
            saved: false,
            history_state: history_state(receipt, project),
            receipt: AgentReceipt::from_engine(receipt),
            history_replayed: true,
            replay_before_revision: before_revision,
            replay_after_revision: after_revision,
        }
    }
}

impl From<AgentCancellationOutcome> for AgentCancelResponse {
    fn from(outcome: AgentCancellationOutcome) -> Self {
        let (ok, cancellation_status) = match outcome {
            AgentCancellationOutcome::Prevented => (true, CancellationStatus::Prevented),
            AgentCancellationOutcome::AlreadyPrevented => {
                (true, CancellationStatus::AlreadyPrevented)
            }
            AgentCancellationOutcome::TooLate => (false, CancellationStatus::TooLate),
            AgentCancellationOutcome::Committed => (false, CancellationStatus::Committed),
            AgentCancellationOutcome::Completed => (false, CancellationStatus::Completed),
            AgentCancellationOutcome::Unknown => (false, CancellationStatus::Unknown),
        };
        Self {
            ok,
            cancellation_status,
            saved: false,
        }
    }
}

/// Complete normal response schema for one guarded edit tool.
///
/// The host may compose this with its generic top-level transport error envelope.
/// Rejected and cancelled apply receipts, and late cancellation results, belong to this schema.
pub fn response_schema(name: &str) -> Option<Value> {
    fn serializing_schema<T: schemars::JsonSchema>() -> Value {
        let schema = schemars::generate::SchemaSettings::default()
            .for_serialize()
            .into_generator()
            .into_root_schema_for::<T>();
        serde_json::to_value(schema).expect("guarded edit response schema serializes")
    }
    Some(match name {
        "agent_apply" => serializing_schema::<AgentApplyResponse>(),
        "agent_receipt" => serializing_schema::<AgentReceiptResponse>(),
        "agent_history" => serializing_schema::<AgentHistoryResponse>(),
        "agent_cancel" => serializing_schema::<AgentCancelResponse>(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn committed_receipt_requires_history_and_changed_object_identity() {
        let committed = json!({
            "status":"committed",
            "before_revision":3,
            "after_revision":4,
            "history_group":"2",
            "changed_layers":[{"glyph":"A","source":0,"layer":"foreground"}],
            "changed_objects":[
                {"kind":"width","glyph":"A","glyph_id":"g1","source":0,"layer":"foreground"},
                {"kind":"point","glyph":"A","glyph_id":"g1","source":0,"layer":"foreground","point_id":"p1"},
                {"kind":"anchor","glyph":"A","glyph_id":"g1","source":0,"layer":"foreground","anchor_id":"a1"}
            ]
        });
        let parsed: AgentReceiptOutcome = serde_json::from_value(committed.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), committed);

        let mut incomplete = committed.clone();
        incomplete.as_object_mut().unwrap().remove("history_group");
        assert!(serde_json::from_value::<AgentReceiptOutcome>(incomplete).is_err());
        let mut incomplete = committed;
        incomplete["changed_objects"][1]
            .as_object_mut()
            .unwrap()
            .remove("point_id");
        assert!(serde_json::from_value::<AgentReceiptOutcome>(incomplete).is_err());
    }

    #[test]
    fn every_noncommit_outcome_and_cancellation_status_round_trips() {
        for outcome in [
            json!({"status":"unchanged","revision":3,"changed_objects":[]}),
            json!({"status":"cancelled","cancellation":"prevented","revision":3,
                "error":"operation cancelled before commit","error_code":"cancelled",
                "changed_objects":[]}),
            json!({"status":"rejected","revision":3,"error":"stale guard",
                "changed_objects":[]}),
        ] {
            let parsed: AgentReceiptOutcome = serde_json::from_value(outcome.clone()).unwrap();
            assert_eq!(serde_json::to_value(parsed).unwrap(), outcome);
        }
        for (outcome, ok, status) in [
            (AgentCancellationOutcome::Prevented, true, "prevented"),
            (
                AgentCancellationOutcome::AlreadyPrevented,
                true,
                "already_prevented",
            ),
            (AgentCancellationOutcome::TooLate, false, "too_late"),
            (AgentCancellationOutcome::Committed, false, "committed"),
            (AgentCancellationOutcome::Completed, false, "completed"),
            (AgentCancellationOutcome::Unknown, false, "unknown"),
        ] {
            let value = serde_json::to_value(AgentCancelResponse::from(outcome)).unwrap();
            assert_eq!(
                value,
                json!({"ok":ok,"cancellation_status":status,"saved":false})
            );
            assert_eq!(
                serde_json::from_value::<AgentCancelResponse>(value)
                    .unwrap()
                    .ok,
                ok
            );
        }
    }

    #[test]
    fn response_schemas_require_complete_normal_bodies() {
        for (name, fields) in [
            (
                "agent_apply",
                &[
                    "ok",
                    "saved",
                    "history_state",
                    "receipt",
                    "replayed",
                    "root_changed",
                ][..],
            ),
            (
                "agent_receipt",
                &["ok", "saved", "history_state", "receipt"][..],
            ),
            (
                "agent_history",
                &[
                    "ok",
                    "saved",
                    "history_state",
                    "receipt",
                    "history_replayed",
                    "replay_before_revision",
                    "replay_after_revision",
                ][..],
            ),
            ("agent_cancel", &["ok", "saved", "cancellation_status"][..]),
        ] {
            let schema = response_schema(name).unwrap();
            let required = schema["required"].as_array().unwrap();
            for field in fields {
                assert!(
                    required.contains(&json!(field)),
                    "{name} omits required {field}"
                );
            }
            if name != "agent_cancel" {
                assert!(
                    schema["properties"]["history_state"]["anyOf"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|branch| branch["type"] == "null"),
                    "{name} must emit a nullable history_state"
                );
            }
        }
        assert!(response_schema("unknown").is_none());

        let committed = serde_json::to_value(schemars::schema_for!(CommittedOutcome)).unwrap();
        for field in [
            "before_revision",
            "after_revision",
            "history_group",
            "changed_layers",
            "changed_objects",
        ] {
            assert!(
                committed["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(field))
            );
        }
        let outcomes = serde_json::to_value(schemars::schema_for!(AgentReceiptOutcome)).unwrap();
        assert_eq!(outcomes["oneOf"].as_array().unwrap().len(), 4);
    }
}
