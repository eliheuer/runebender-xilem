// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Strict transport requests and discovery schemas for the native live Nodes graph.
//!
//! These requests select one application-owned graph session.
//! They never accept client-authored font, script, parameter or proof hashes.
//! The application derives every run capture from the current canonical Project and graph.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::agent::Tool;
use crate::font::project::Project;
use crate::font::variable::SourceId;
use crate::workflows::nodes_session::{
    GraphCancelRequest, GraphIdentity, GraphMutationRequest, GraphRunHandle, GraphSemanticGuard,
};

/// Bootstrap discovery for the current document's graph session.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodesDiscoverRequest {
    /// Exact native endpoint lifetime returned by the application.
    pub expected_document_epoch: String,
}

/// Read one exact canonical graph snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodesSnapshotRequest {
    /// Exact native endpoint lifetime returned by the application.
    pub expected_document_epoch: String,
    /// Exact graph and document lifetime returned by discovery.
    pub identity: GraphIdentity,
}

/// Apply one receipt-backed graph mutation through the canonical session.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodesMutateRequest {
    /// Exact native endpoint lifetime returned by the application.
    pub expected_document_epoch: String,
    /// Existing guarded graph request nested without changing its typed contract.
    pub request: GraphMutationRequest,
}

/// Start one native Python comparison from host-derived captures.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodesRunRequest {
    /// Exact native endpoint lifetime returned by the application.
    pub expected_document_epoch: String,
    /// Layout-independent guard for the one canonical graph.
    pub guard: GraphSemanticGuard,
    /// Bounded actor owning the run retry key.
    pub actor: String,
    /// Actor-local idempotency key for this exact run request.
    pub operation_key: String,
    /// Explicit stable source identity from live project discovery.
    pub source: usize,
    /// Explicit bounded glyph scope captured by the host.
    pub glyphs: Vec<String>,
}

/// Inspect one retained graph run.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodesStatusRequest {
    /// Exact native endpoint lifetime returned by the application.
    pub expected_document_epoch: String,
    /// Exact graph and document lifetime that issued the handle.
    pub identity: GraphIdentity,
    /// Session-local run handle returned by `nodes_run`.
    pub handle: GraphRunHandle,
}

/// Cancel one selected retained graph run with an idempotent receipt.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodesCancelRequest {
    /// Exact native endpoint lifetime returned by the application.
    pub expected_document_epoch: String,
    /// Existing exact cancellation request nested without changing its typed contract.
    pub request: GraphCancelRequest,
}

/// Release one terminal graph run and its heavy artifacts.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodesReleaseRequest {
    /// Exact native endpoint lifetime returned by the application.
    pub expected_document_epoch: String,
    /// Exact graph and document lifetime that issued the handle.
    pub identity: GraphIdentity,
    /// Session-local terminal run handle.
    pub handle: GraphRunHandle,
}

/// Build one separate receipt-backed font Apply from a retained completed result.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodesApplyRequest {
    /// Exact native endpoint lifetime returned by the application.
    pub expected_document_epoch: String,
    /// Exact graph and document lifetime that issued the handle.
    pub identity: GraphIdentity,
    /// Completed run whose guarded staged edit is selected.
    pub handle: GraphRunHandle,
    /// Bounded actor owning the separate edit receipt.
    pub actor: String,
    /// New actor-local operation key for the common edit receipt.
    pub operation_key: String,
    /// Must be `user-approved`, reflecting existing user authorization.
    pub authorization: String,
}

/// Comparison proof branch returned by `nodes_image`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodesImageBranch {
    /// Proof of the unchanged captured family.
    Original,
    /// Proof of the staged derived family.
    Changed,
}

/// Read one exact retained comparison image without rerendering.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodesImageRequest {
    /// Exact native endpoint lifetime returned by the application.
    pub expected_document_epoch: String,
    /// Exact graph and document lifetime that issued the handle.
    pub identity: GraphIdentity,
    /// Completed run retaining both proof artifacts.
    pub handle: GraphRunHandle,
    /// Original or changed comparison branch.
    pub branch: NodesImageBranch,
}

fn validate_text(name: &str, value: &str, max: usize) -> Result<(), String> {
    if value.is_empty() || value.len() > max {
        return Err(format!("{name} must contain 1..={max} UTF-8 bytes"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{name} must not contain control characters"));
    }
    Ok(())
}

fn validate_epoch(expected: &str, identity: &GraphIdentity) -> Result<(), String> {
    validate_text("expected_document_epoch", expected, 256)?;
    validate_text("graph session_id", &identity.session_id, 128)?;
    validate_text("graph document_epoch", &identity.document_epoch, 128)?;
    if expected != identity.document_epoch {
        return Err("expected_document_epoch does not match the graph identity".into());
    }
    Ok(())
}

fn validate_actor_key(actor: &str, operation_key: &str) -> Result<(), String> {
    validate_text("actor", actor, 128)?;
    validate_text("operation_key", operation_key, 128)
}

fn validate_handle(handle: GraphRunHandle) -> Result<(), String> {
    if handle.get() == 0 {
        return Err("graph run handle must be greater than zero".into());
    }
    Ok(())
}

impl NodesDiscoverRequest {
    /// Validate transport bounds before application discovery.
    pub fn validate(&self) -> Result<(), String> {
        validate_text(
            "expected_document_epoch",
            &self.expected_document_epoch,
            256,
        )
    }
}

impl NodesSnapshotRequest {
    /// Validate endpoint and graph-lifetime agreement.
    pub fn validate(&self) -> Result<(), String> {
        validate_epoch(&self.expected_document_epoch, &self.identity)
    }
}

impl NodesMutateRequest {
    /// Validate endpoint, graph identity and actor retry bounds.
    pub fn validate(&self) -> Result<(), String> {
        validate_epoch(&self.expected_document_epoch, &self.request.guard.identity)?;
        validate_actor_key(&self.request.actor, &self.request.operation_key)
    }
}

impl NodesRunRequest {
    /// Validate endpoint, semantic guard, actor retry identity and explicit glyph scope.
    ///
    /// The application additionally verifies `source` against the current Project while deriving
    /// the immutable host capture.
    pub fn validate(&self) -> Result<(), String> {
        validate_epoch(&self.expected_document_epoch, &self.guard.identity)?;
        validate_actor_key(&self.actor, &self.operation_key)?;
        if self.guard.semantic_hash.len() != 64
            || !self
                .guard
                .semantic_hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("semantic_hash must be a lowercase SHA-256 digest".into());
        }
        if self.glyphs.is_empty() || self.glyphs.len() > 64 {
            return Err("glyphs must contain 1..=64 explicit names".into());
        }
        let mut unique = BTreeSet::new();
        for glyph in &self.glyphs {
            validate_text("glyph name", glyph, 128)?;
            if !unique.insert(glyph) {
                return Err("glyphs must not repeat a name".into());
            }
        }
        Ok(())
    }

    /// Validate the explicit stable source against the current host Project.
    pub fn validate_source(&self, project: &Project) -> Result<(), String> {
        if project.document_source(SourceId(self.source)).is_none() {
            return Err("source is unknown or was removed from the current Project".into());
        }
        Ok(())
    }
}

impl NodesStatusRequest {
    /// Validate endpoint and graph-lifetime agreement.
    pub fn validate(&self) -> Result<(), String> {
        validate_epoch(&self.expected_document_epoch, &self.identity)?;
        validate_handle(self.handle)
    }
}

impl NodesCancelRequest {
    /// Validate endpoint, graph identity and cancellation retry bounds.
    pub fn validate(&self) -> Result<(), String> {
        validate_epoch(&self.expected_document_epoch, &self.request.identity)?;
        validate_actor_key(&self.request.actor, &self.request.operation_key)?;
        validate_handle(self.request.handle)
    }
}

impl NodesReleaseRequest {
    /// Validate endpoint and graph-lifetime agreement.
    pub fn validate(&self) -> Result<(), String> {
        validate_epoch(&self.expected_document_epoch, &self.identity)?;
        validate_handle(self.handle)
    }
}

impl NodesApplyRequest {
    /// Validate endpoint, graph identity, edit retry bounds and explicit authorization marker.
    pub fn validate(&self) -> Result<(), String> {
        validate_epoch(&self.expected_document_epoch, &self.identity)?;
        validate_handle(self.handle)?;
        validate_actor_key(&self.actor, &self.operation_key)?;
        if self.authorization != "user-approved" {
            return Err("nodes_apply requires user-approved authorization".into());
        }
        Ok(())
    }
}

impl NodesImageRequest {
    /// Validate endpoint and graph-lifetime agreement.
    pub fn validate(&self) -> Result<(), String> {
        validate_epoch(&self.expected_document_epoch, &self.identity)?;
        validate_handle(self.handle)
    }
}

fn string(max_length: usize) -> Value {
    json!({"type":"string","minLength":1,"maxLength":max_length})
}

fn identity() -> Value {
    json!({
        "type":"object",
        "additionalProperties":false,
        "required":["session_id","document_epoch"],
        "properties":{
            "session_id":string(128),
            "document_epoch":string(128)
        }
    })
}

fn graph_guard(semantic: bool) -> Value {
    let mut properties = json!({
        "identity":identity(),
        "semantic_revision":{"type":"integer","minimum":0},
        "semantic_hash":{"type":"string","pattern":"^[0-9a-f]{64}$"}
    });
    let required = if semantic {
        json!(["identity", "semantic_revision", "semantic_hash"])
    } else {
        properties
            .as_object_mut()
            .expect("guard properties are an object")
            .insert("revision".into(), json!({"type":"integer","minimum":0}));
        properties
            .as_object_mut()
            .expect("guard properties are an object")
            .remove("semantic_revision");
        properties
            .as_object_mut()
            .expect("guard properties are an object")
            .remove("semantic_hash");
        json!(["identity", "revision"])
    };
    json!({
        "type":"object",
        "additionalProperties":false,
        "required":required,
        "properties":properties
    })
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({
        "type":"object",
        "additionalProperties":false,
        "required":required,
        "properties":properties
    })
}

fn mutation_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(NodesMutateRequest))
        .expect("typed Nodes mutation schema serializes");
    schema["additionalProperties"] = json!(false);
    schema["properties"]["expected_document_epoch"] = string(256);
    if schema["$defs"].get("GraphMutationRequest").is_some() {
        schema["$defs"]["GraphMutationRequest"]["properties"]["actor"] = string(128);
        schema["$defs"]["GraphMutationRequest"]["properties"]["operation_key"] = string(128);
    }
    if schema["$defs"].get("GraphIdentity").is_some() {
        schema["$defs"]["GraphIdentity"] = identity();
    }
    if schema["$defs"].get("GraphGuard").is_some() {
        schema["$defs"]["GraphGuard"] = graph_guard(false);
    }
    schema
}

fn cancel_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(NodesCancelRequest))
        .expect("typed Nodes cancellation schema serializes");
    schema["additionalProperties"] = json!(false);
    schema["properties"]["expected_document_epoch"] = string(256);
    if schema["$defs"].get("GraphCancelRequest").is_some() {
        schema["$defs"]["GraphCancelRequest"]["properties"]["actor"] = string(128);
        schema["$defs"]["GraphCancelRequest"]["properties"]["operation_key"] = string(128);
    }
    if schema["$defs"].get("GraphIdentity").is_some() {
        schema["$defs"]["GraphIdentity"] = identity();
    }
    if schema["$defs"].get("GraphRunHandle").is_some() {
        schema["$defs"]["GraphRunHandle"] = json!({"type":"integer","minimum":1});
    }
    schema
}

/// Native live Nodes tools in stable inventory order.
pub fn tools() -> Vec<Tool> {
    let epoch = string(256);
    let actor = string(128);
    let key = string(128);
    let handle = json!({"type":"integer","minimum":1});
    let session = |extra: Value, required: &[&str]| {
        let mut properties = json!({
            "expected_document_epoch":epoch.clone(),
            "identity":identity()
        });
        properties
            .as_object_mut()
            .expect("tool properties are an object")
            .extend(
                extra
                    .as_object()
                    .expect("extra properties are an object")
                    .clone(),
            );
        object(properties, required)
    };
    vec![
        Tool {
            name: "nodes_discover".into(),
            description: "Discover the current native graph identity, actual node registry, request schemas and hard limits. This is the bootstrap call after connecting to an exact document_epoch; it does not edit or run the graph.".into(),
            parameters: object(
                json!({"expected_document_epoch":epoch.clone()}),
                &["expected_document_epoch"],
            ),
        },
        Tool {
            name: "nodes_snapshot".into(),
            description: "Read the one canonical editable live graph, full and semantic revisions, validation diagnostics and graph-only Undo state. Does not execute it.".into(),
            parameters: session(
                json!({}),
                &["expected_document_epoch", "identity"],
            ),
        },
        Tool {
            name: "nodes_mutate".into(),
            description: "Apply one atomic revision-guarded graph mutation with an actor operation-key receipt. Exact retries replay; stale revisions and changed payloads reject. Editing never runs the graph.".into(),
            parameters: mutation_schema(),
        },
        Tool {
            name: "nodes_run".into(),
            description: "Run the bounded native base/Python comparison for one explicit source and 1 to 64 glyphs. The host captures current font, code, parameters and proof identities; callers never supply hashes. Poll nodes_status.".into(),
            parameters: object(
                json!({
                    "expected_document_epoch":epoch.clone(),
                    "guard":graph_guard(true),
                    "actor":actor.clone(),
                    "operation_key":key.clone(),
                    "source":{"type":"integer","minimum":0},
                    "glyphs":{"type":"array","minItems":1,"maxItems":64,"uniqueItems":true,"items":string(128)}
                }),
                &[
                    "expected_document_epoch",
                    "guard",
                    "actor",
                    "operation_key",
                    "source",
                    "glyphs",
                ],
            ),
        },
        Tool {
            name: "nodes_status".into(),
            description: "Inspect one retained native graph run and its current, stale, failed, cancelled or completed node outputs. Does not consume the result or include PNG bytes.".into(),
            parameters: session(
                json!({"handle":handle.clone()}),
                &["expected_document_epoch", "identity", "handle"],
            ),
        },
        Tool {
            name: "nodes_cancel".into(),
            description: "Cancel one selected graph run with its own actor operation-key receipt. It never cancels another graph, script or proof job.".into(),
            parameters: cancel_schema(),
        },
        Tool {
            name: "nodes_release".into(),
            description: "Release one terminal graph run and its retained script/proof artifacts. Queued or running work rejects; the durable run receipt remains a tombstone.".into(),
            parameters: session(
                json!({"handle":handle.clone()}),
                &["expected_document_epoch", "identity", "handle"],
            ),
        },
        Tool {
            name: "nodes_apply".into(),
            description: "Apply the guarded staged edit from one completed selected run through the common receipt-backed font edit path. Supply a new edit operation_key and user-approved only within the user's granted authorization. No arbitrary result body is accepted.".into(),
            parameters: session(
                json!({
                    "handle":handle.clone(),
                    "actor":actor,
                    "operation_key":key,
                    "authorization":{"enum":["user-approved"]}
                }),
                &[
                    "expected_document_epoch",
                    "identity",
                    "handle",
                    "actor",
                    "operation_key",
                    "authorization",
                ],
            ),
        },
        Tool {
            name: "nodes_image".into(),
            description: "Return the exact already-rendered PNG for the original or changed branch of one completed run. The generic MCP response carries image/png separately from metadata; this never rerenders.".into(),
            parameters: session(
                json!({"handle":handle,"branch":{"enum":["original","changed"]}}),
                &["expected_document_epoch", "identity", "handle", "branch"],
            ),
        },
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn inventory_is_unique_bounded_and_has_no_client_run_hashes() {
        let tools = tools();
        let names: BTreeSet<_> = tools.iter().map(|tool| tool.name.as_str()).collect();
        assert_eq!(names.len(), tools.len());
        assert_eq!(tools.len(), 9);
        assert!(
            tools
                .iter()
                .all(|tool| tool.parameters["additionalProperties"] == false)
        );

        let run = tools.iter().find(|tool| tool.name == "nodes_run").unwrap();
        assert_eq!(run.parameters["properties"]["glyphs"]["maxItems"], 64);
        let encoded = run.parameters.to_string();
        assert!(!encoded.contains("capture_sha256"));
        assert!(!encoded.contains("script_sha256"));
        assert!(!encoded.contains("recipe_sha256"));
        assert!(!encoded.contains("canonical_input_sha256"));

        let mutate = tools
            .iter()
            .find(|tool| tool.name == "nodes_mutate")
            .unwrap();
        assert!(mutate.parameters["properties"].get("request").is_some());
        assert!(mutate.parameters["properties"].get("actor").is_none());
    }

    #[test]
    fn run_and_apply_reject_unknown_or_arbitrary_result_fields() {
        let run = json!({
            "expected_document_epoch":"document",
            "guard":{
                "identity":{"session_id":"graph","document_epoch":"document"},
                "semantic_revision":0,
                "semantic_hash":"0".repeat(64)
            },
            "actor":"agent",
            "operation_key":"run-1",
            "source":0,
            "glyphs":["A"],
            "capture_sha256":"1".repeat(64)
        });
        assert!(serde_json::from_value::<NodesRunRequest>(run).is_err());

        let apply = json!({
            "expected_document_epoch":"document",
            "identity":{"session_id":"graph","document_epoch":"document"},
            "handle":1,
            "actor":"agent",
            "operation_key":"apply-1",
            "authorization":"user-approved",
            "result":{"edits":[]}
        });
        assert!(serde_json::from_value::<NodesApplyRequest>(apply).is_err());
    }

    #[test]
    fn validators_bind_epoch_and_bound_explicit_scope() {
        let request: NodesRunRequest = serde_json::from_value(json!({
            "expected_document_epoch":"document",
            "guard":{
                "identity":{"session_id":"graph","document_epoch":"document"},
                "semantic_revision":0,
                "semantic_hash":"0".repeat(64)
            },
            "actor":"agent",
            "operation_key":"run-1",
            "source":0,
            "glyphs":["A"]
        }))
        .unwrap();
        assert_eq!(request.validate(), Ok(()));
        assert_eq!(
            request.validate_source(&Project::new_font("test.ufo".into())),
            Ok(())
        );

        let mut wrong_epoch = request.clone();
        wrong_epoch.expected_document_epoch = "replacement".into();
        assert!(
            wrong_epoch
                .validate()
                .unwrap_err()
                .contains("does not match")
        );

        let mut duplicate = request;
        duplicate.glyphs.push("A".into());
        assert!(duplicate.validate().unwrap_err().contains("repeat"));
    }
}
