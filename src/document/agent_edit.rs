// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Strict live edit requests resolved against canonical document identities.
//!
//! The application owns authorization, epoch binding, receipts and UI history.
//! This adapter only validates typed wire input and stages one canonical transaction.

use kurbo::Point;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::CanonicalLayerSnapshot;
use super::agent::Tool;
use super::agent_session::{AgentOperationRejection, AgentPayloadDigest};
use super::edit_batch::canonical_glyph_revision;
use super::project::{
    CanonicalDocumentEditTransaction, DocumentEditOperation, DocumentLayerEdit, Project,
};
use super::variable::{GlyphLayerAddress, LayerId, SourceId};

/// An explicit layer read, including identity and the revision returned by `read_glyph`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentLayerGuard {
    /// Glyph name at the time of the read.
    pub glyph: String,
    /// Opaque glyph identity; protects against delete-and-recreate under the same name.
    pub glyph_id: String,
    /// Exact layer name within the request's stable source.
    pub layer: String,
    /// Canonical glyph revision returned by the read adapter.
    pub expected_revision: String,
}

/// One supported nonstructural operation, addressed by stable object identity.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentEditOperation {
    /// Replace the exact horizontal advance.
    SetWidth {
        /// New advance in font units.
        width: f64,
    },
    /// Move one existing contour point.
    SetPoint {
        /// Opaque point identity from the guarded read.
        point_id: String,
        /// New horizontal coordinate.
        x: f64,
        /// New vertical coordinate.
        y: f64,
    },
    /// Move one existing anchor.
    SetAnchor {
        /// Opaque anchor identity from the guarded read.
        anchor_id: String,
        /// New horizontal coordinate.
        x: f64,
        /// New vertical coordinate.
        y: f64,
    },
}

/// Guarded operations for one layer.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentLayerEdits {
    /// Layer state used to derive these operations.
    pub target: AgentLayerGuard,
    /// Operations in execution order.
    pub operations: Vec<AgentEditOperation>,
}

/// Complete semantic payload for one receipt-backed live edit.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentEditRequest {
    /// Required endpoint lifetime guard.
    pub expected_document_epoch: String,
    /// Bounded caller label, not an authentication credential.
    pub actor: String,
    /// Actor-local retry identity.
    pub operation_key: String,
    /// Must be `user-approved`, reflecting existing user authorization.
    pub authorization: String,
    /// Explicit stable source ID, never an active-source default.
    pub source: usize,
    /// Name used for the grouped history entry.
    pub history_name: String,
    /// Additional layer reads on which the edit depends.
    #[serde(default)]
    pub reads: Vec<AgentLayerGuard>,
    /// Complete bounded write set.
    pub edits: Vec<AgentLayerEdits>,
}

impl AgentEditRequest {
    /// Hash the normalized complete typed payload, including guards and authorization.
    pub fn payload_digest(&self) -> AgentPayloadDigest {
        AgentPayloadDigest::sha256(&serde_json::to_vec(self).expect("typed request serializes"))
    }

    /// Resolve opaque identities and revisions, then stage without mutating the document.
    pub fn stage(
        &self,
        project: &Project,
    ) -> Result<CanonicalDocumentEditTransaction, AgentOperationRejection> {
        let invalid = |message: &str| AgentOperationRejection::InvalidRequest(message.into());
        if self.edits.is_empty() || self.edits.len() + self.reads.len() > 64 {
            return Err(invalid("supply edits and at most 64 guarded layer entries"));
        }
        if self.edits.iter().any(|edit| edit.operations.is_empty())
            || self
                .edits
                .iter()
                .map(|edit| edit.operations.len())
                .sum::<usize>()
                > 256
        {
            return Err(invalid(
                "supply 1..=256 operations, with at least one per edited layer",
            ));
        }
        let source = SourceId(self.source);
        let reads = self
            .reads
            .iter()
            .map(|guard| guard.resolve(project, source))
            .collect::<Result<Vec<_>, _>>()?;
        let edits = self
            .edits
            .iter()
            .map(|edit| {
                let snapshot = edit.target.resolve(project, source)?;
                let layer = project
                    .document_layer(&snapshot.address().glyph, &snapshot.address().layer)
                    .expect("resolved canonical layer remains present during immutable staging");
                let operations = edit
                    .operations
                    .iter()
                    .map(|operation| match operation {
                        AgentEditOperation::SetWidth { width } => {
                            Ok(DocumentEditOperation::SetWidth(*width))
                        }
                        AgentEditOperation::SetPoint { point_id, x, y } => {
                            let point = layer
                                .contours()
                                .flat_map(|contour| contour.points())
                                .find(|point| point.id().to_wire() == *point_id)
                                .ok_or_else(|| {
                                    invalid("point identity is absent from the guarded layer")
                                })?;
                            Ok(DocumentEditOperation::SetPoint {
                                point: point.id(),
                                position: Point::new(*x, *y),
                            })
                        }
                        AgentEditOperation::SetAnchor { anchor_id, x, y } => {
                            let anchor = layer
                                .anchors()
                                .find(|anchor| anchor.id().to_wire() == *anchor_id)
                                .ok_or_else(|| {
                                    invalid("anchor identity is absent from the guarded layer")
                                })?;
                            Ok(DocumentEditOperation::SetAnchor {
                                anchor: anchor.id(),
                                position: Point::new(*x, *y),
                            })
                        }
                    })
                    .collect::<Result<Vec<_>, AgentOperationRejection>>()?;
                Ok(DocumentLayerEdit::new(snapshot, operations))
            })
            .collect::<Result<Vec<_>, AgentOperationRejection>>()?;
        project
            .begin_document_edit_transaction(source, &self.history_name, reads, edits)
            .map_err(Into::into)
    }
}

impl AgentLayerGuard {
    fn resolve(
        &self,
        project: &Project,
        source: SourceId,
    ) -> Result<CanonicalLayerSnapshot, AgentOperationRejection> {
        let invalid = |message: &str| AgentOperationRejection::InvalidRequest(message.into());
        if [
            &self.glyph,
            &self.glyph_id,
            &self.layer,
            &self.expected_revision,
        ]
        .iter()
        .any(|value| value.is_empty() || value.len() > 256)
        {
            return Err(invalid("guard strings must contain 1..=256 bytes"));
        }
        let glyph = project
            .document_glyph(&self.glyph)
            .ok_or_else(|| invalid("glyph no longer exists"))?;
        if glyph.id().to_wire() != self.glyph_id {
            return Err(invalid(
                "glyph identity changed; read the intended glyph again",
            ));
        }
        let address = GlyphLayerAddress {
            glyph: self.glyph.clone(),
            layer: LayerId {
                source,
                name: self.layer.clone(),
            },
        };
        let layer = project
            .document_layer(&address.glyph, &address.layer)
            .ok_or_else(|| invalid("explicit source/layer does not contain the glyph"))?;
        let revision =
            canonical_glyph_revision(layer).map_err(AgentOperationRejection::InvalidRequest)?;
        if revision != self.expected_revision {
            return Err(invalid(
                "guarded layer changed; read it again before a new operation",
            ));
        }
        project
            .capture_document_layer(&address)
            .ok_or_else(|| invalid("guarded layer is unavailable"))
    }
}

/// Generated live tool inventory for the strict application edit boundary.
pub fn tools() -> Vec<Tool> {
    let string = json!({"type":"string","minLength":1,"maxLength":256});
    let guard = json!({"type":"object","properties":{"glyph":string,"glyph_id":string,"layer":string,"expected_revision":string},"required":["glyph","glyph_id","layer","expected_revision"],"additionalProperties":false});
    let operation = json!({"oneOf":[
        {"type":"object","properties":{"op":{"const":"set_width"},"width":{"type":"number"}},"required":["op","width"],"additionalProperties":false},
        {"type":"object","properties":{"op":{"const":"set_point"},"point_id":string,"x":{"type":"number"},"y":{"type":"number"}},"required":["op","point_id","x","y"],"additionalProperties":false},
        {"type":"object","properties":{"op":{"const":"set_anchor"},"anchor_id":string,"x":{"type":"number"},"y":{"type":"number"}},"required":["op","anchor_id","x","y"],"additionalProperties":false}
    ]});
    let mut identity =
        json!({"expected_document_epoch":string,"actor":string,"operation_key":string});
    let mut apply = identity.clone();
    apply["authorization"] = json!({"enum":["user-approved"]});
    apply["source"] = json!({"type":"integer","minimum":0});
    apply["history_name"] = string;
    apply["reads"] = json!({"type":"array","maxItems":64,"items":guard});
    apply["edits"] = json!({"type":"array","minItems":1,"maxItems":64,"items":{"type":"object","properties":{"target":guard,"operations":{"type":"array","minItems":1,"maxItems":256,"items":operation}},"required":["target","operations"],"additionalProperties":false}});
    let make = |name: &str, description: &str, properties: Value, required: Value| Tool {
        name: name.into(),
        description: description.into(),
        parameters: json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}),
    };
    let mut result = vec![
        make(
            "agent_apply",
            "Apply one authorized guarded batch to the unsaved root with one history group. Read explicit glyph/source/layer identities first. Reuse exactly the same actor, operation_key and payload after a lost response; a different payload under that key rejects. In-memory receipts do not survive document closure.",
            apply,
            json!([
                "expected_document_epoch",
                "actor",
                "operation_key",
                "authorization",
                "source",
                "history_name",
                "edits"
            ]),
        ),
        make(
            "agent_receipt",
            "Look up the immutable original apply receipt and separate current history state. Never reapplies or saves an edit.",
            identity.clone(),
            json!(["expected_document_epoch", "actor", "operation_key"]),
        ),
        make(
            "agent_cancel",
            "Prevent an admitted agent_apply from committing, addressed by its exact document epoch, actor and operation key. Returns prevented, already_prevented, too_late, committed, completed or unknown. Cancellation never undoes a committed edit; use agent_history for that.",
            identity.clone(),
            json!(["expected_document_epoch", "actor", "operation_key"]),
        ),
    ];
    identity["direction"] = json!({"enum":["undo","redo"]});
    identity["authorization"] = json!({"enum":["user-approved"]});
    result.push(make("agent_history", "Undo or redo the group associated with an apply receipt, sharing ordinary editor history. Overlapping later changes reject. After a lost response inspect agent_receipt history_state before retrying; this command is not an idempotent apply.", identity, json!(["expected_document_epoch","actor","operation_key","direction","authorization"])));
    result
}
