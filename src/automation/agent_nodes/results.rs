// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Typed result bodies for native live and headless disk graph tools.
//!
//! These envelopes mirror the application's existing JSON fields.
//! GraphSession owns the nested snapshot, receipt, and run data; the application
//! supplies document freshness, script reports, and retained proof images.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::font::compiler::proof::{
    CompiledProofGlyph, CompiledProofRecipe, CompiledProofRendering, ProofDetailCrop, ProofView,
};
use crate::formats::designbot::RendererIdentity;
use crate::workflows::nodes_run::NodeResult;
use crate::workflows::nodes_session::{
    GraphCancelResponse, GraphDiscovery, GraphIdentity, GraphMutationResponse, GraphRunInspection,
    GraphRunResponse, GraphSnapshot,
};

/// Discovery of the current graph and its authoring contract.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct NodesDiscoverResult {
    /// Whether discovery succeeded.
    pub ok: bool,
    /// Current graph and document lifetime.
    pub identity: GraphIdentity,
    /// Supported node types, limits, and request schemas.
    pub discovery: GraphDiscovery,
    /// Instructions for authoring the strict Python recipe.
    pub recipe_authoring: String,
    /// Discovery does not mutate the canonical font.
    pub root_changed: bool,
}

/// One read of the canonical editable graph.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct NodesSnapshotResult {
    /// Whether the read succeeded.
    pub ok: bool,
    /// Exact graph state at the time of the read.
    pub snapshot: GraphSnapshot,
    /// Reading does not mutate the canonical font.
    pub root_changed: bool,
}

/// Graph mutation receipt and resulting snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct NodesMutateResult {
    /// Whether the mutation succeeded.
    pub ok: bool,
    /// Original mutation receipt or exact replay.
    pub mutation: GraphMutationResponse,
    /// Current graph state after the request.
    pub snapshot: GraphSnapshot,
    /// Graph-only mutation does not change the font.
    pub root_changed: bool,
}

/// Phase of one retained asynchronous calibrated image trace.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodesTracePhase {
    /// Accepted by the global bounded worker, not yet started.
    Queued,
    /// Image decoding or tracing is active off the editor thread.
    Running,
    /// Cancellation requested; an active worker may still occupy its global slot.
    Cancelling,
    /// A guarded recipe was installed in the existing graph exactly once.
    Completed,
    /// Worker, validation or graph publication failed.
    Failed,
    /// Cancellation prevented graph publication.
    Cancelled,
    /// Captured document, layer or graph changed before publication.
    Stale,
    /// Heavy trace state was released; its retry key remains reserved.
    Released,
}

/// Admission receipt for an asynchronous calibrated trace.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct NodesTraceStartResult {
    /// Whether submission or an exact retry succeeded.
    pub ok: bool,
    /// Opaque session-local handle.
    pub handle: u64,
    /// Current phase at the time of admission.
    pub phase: NodesTracePhase,
    /// Whether this was an exact retry of the retained request.
    pub replayed: bool,
    /// Submission does not mutate the font.
    pub root_changed: bool,
}

/// Inspection and optional graph-mutation receipt for one retained trace.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct NodesTraceStatusResult {
    /// Whether the handle was inspected.
    pub ok: bool,
    /// Opaque session-local handle.
    pub handle: u64,
    /// Current trace phase.
    pub phase: NodesTracePhase,
    /// Original graph mutation receipt, only after successful guarded publication.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutation: Option<GraphMutationResponse>,
    /// Failure or stale reason, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Inspection never changes the font.
    pub root_changed: bool,
}

/// Receipt for dropping one terminal trace and admitting the next in this session.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct NodesTraceReleaseResult {
    /// Whether the terminal trace was released.
    pub ok: bool,
    /// Opaque released handle.
    pub handle: u64,
    /// Release never changes the font.
    pub root_changed: bool,
}

/// Receipt for a new or replayed graph run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct NodesRunResult {
    /// Whether the submission succeeded.
    pub ok: bool,
    /// Retained session-local run receipt and its original disposition.
    pub run: GraphRunResponse,
    /// Whether this transport retry returned the originally retained response.
    pub replayed: bool,
    /// Running a graph does not change the canonical font.
    pub root_changed: bool,
}

/// Retained run state and application-owned report metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct NodesStatusResult {
    /// Whether inspection succeeded.
    pub ok: bool,
    /// Typed run state, outputs, errors, and immutable lineage.
    pub run: GraphRunInspection,
    /// Whether captured lineage still matches the current graph and font.
    pub current: bool,
    /// Whether captured lineage differs from the current graph or font.
    pub stale: bool,
    /// Bounded recipe report, or JSON null before a result is available.
    pub report: Option<String>,
    /// Bounded Python diagnostics, or JSON null before a result is available.
    pub stderr: Option<String>,
    /// Whether exactly one current, proven transform is eligible without an explicit node selector.
    pub can_apply: bool,
    /// Inspection does not mutate the canonical font.
    pub root_changed: bool,
}

/// Cancellation receipt for one retained run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct NodesCancelResult {
    /// Whether cancellation succeeded.
    pub ok: bool,
    /// Original cancellation effect or exact replay.
    pub cancellation: GraphCancelResponse,
    /// Cancellation does not mutate the canonical font.
    pub root_changed: bool,
}

/// Receipt for releasing one terminal graph run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct NodesReleaseResult {
    /// Whether the request succeeded.
    pub ok: bool,
    /// Whether the terminal run and its heavy artifacts were released.
    pub released: bool,
    /// Release does not mutate the canonical font.
    pub root_changed: bool,
}

/// Metadata and optional transport bytes for one retained node proof image.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct NodesImageResult {
    /// Whether the read succeeded.
    pub ok: bool,
    /// Opaque retained proof artifact identity.
    pub artifact_id: String,
    /// Whether captured lineage still matches the current graph and font.
    pub current: bool,
    /// Whether captured lineage differs from the current graph or font.
    pub stale: bool,
    /// Document epoch captured by the run.
    pub captured_document_epoch: String,
    /// Canonical font revision captured before proof compilation.
    pub captured_document_revision: u64,
    /// SHA-256 of the compiled font bytes rendered into this image.
    pub font_sha256: String,
    /// SHA-256 of the exact canonical compiler input.
    pub canonical_input_sha256: String,
    /// Exact recipe used for shaping and rendering.
    pub recipe: CompiledProofRecipe,
    /// SHA-256 of the exact recipe shared by the retained views.
    pub recipe_sha256: String,
    /// Measured Designbot entrypoint used for this image.
    pub renderer: RendererIdentity,
    /// Which retained view was selected.
    pub view: ProofView,
    /// Raster settings of the selected view.
    pub rendering: CompiledProofRendering,
    /// Paint-order target index in the unchanged shaping result, when selected.
    pub target_glyph_index: Option<usize>,
    /// Fixed crop transform for a selected detail view.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crop: Option<ProofDetailCrop>,
    /// Positioned glyph metrics in paint order.
    pub glyphs: Vec<CompiledProofGlyph>,
    /// Base64 PNG, omitted from structured MCP data after image extraction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub png_base64: Option<String>,
    /// Reading an image does not change the canonical font.
    pub root_changed: bool,
}

/// One headless disk graph run with its resolved file and font paths.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct DiskNodesRunResult {
    /// Whether every node ran or reused a valid cached output.
    pub ok: bool,
    /// Graph file that was executed.
    pub file: PathBuf,
    /// Font source selected for the run.
    pub font: PathBuf,
    /// Per-node results in execution order.
    pub nodes: Vec<NodeResult>,
}

fn serialized_schema<T: schemars::JsonSchema>() -> schemars::Schema {
    schemars::generate::SchemaSettings::default()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<T>()
}

/// Generated success schema for a native graph tool.
///
/// `nodes_apply` delegates to the common guarded edit contract and has no graph-specific body.
pub fn response_schema(name: &str) -> Option<Value> {
    let schema = match name {
        "nodes_discover" => serialized_schema::<NodesDiscoverResult>(),
        "nodes_snapshot" => serialized_schema::<NodesSnapshotResult>(),
        "nodes_mutate" => serialized_schema::<NodesMutateResult>(),
        "nodes_trace" => serialized_schema::<NodesTraceStartResult>(),
        "nodes_trace_status" | "nodes_trace_cancel" => {
            serialized_schema::<NodesTraceStatusResult>()
        }
        "nodes_trace_release" => serialized_schema::<NodesTraceReleaseResult>(),
        "nodes_run" => serialized_schema::<NodesRunResult>(),
        "nodes_status" => serialized_schema::<NodesStatusResult>(),
        "nodes_cancel" => serialized_schema::<NodesCancelResult>(),
        "nodes_release" => serialized_schema::<NodesReleaseResult>(),
        "nodes_image" => serialized_schema::<NodesImageResult>(),
        _ => return None,
    };
    Some(serde_json::to_value(schema).expect("graph result schema serializes"))
}

/// Generated response schema for a headless disk graph tool, including failed node runs.
pub fn disk_response_schema(name: &str) -> Option<Value> {
    let schema = match name {
        "nodes_run" => serialized_schema::<DiskNodesRunResult>(),
        _ => return None,
    };
    Some(serde_json::to_value(schema).expect("disk graph result schema serializes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn required(schema: &Value) -> Vec<&str> {
        schema["required"]
            .as_array()
            .expect("object schema has required fields")
            .iter()
            .map(|field| field.as_str().expect("required field is a string"))
            .collect()
    }

    fn field_schema<'a>(root: &'a Value, field: &str) -> &'a Value {
        let field_schema = &root["properties"][field];
        if let Some(reference) = field_schema["$ref"].as_str() {
            root.pointer(reference.strip_prefix('#').expect("local schema reference"))
                .expect("nested schema definition exists")
        } else {
            field_schema
        }
    }

    fn allows_null(schema: &Value) -> bool {
        schema["type"] == "null"
            || schema["type"]
                .as_array()
                .is_some_and(|types| types.iter().any(|kind| kind == "null"))
            || schema["anyOf"]
                .as_array()
                .is_some_and(|branches| branches.iter().any(allows_null))
    }

    #[test]
    fn graph_result_schemas_require_typed_nested_contracts() {
        let discovery = response_schema("nodes_discover").unwrap();
        let nested_discovery = field_schema(&discovery, "discovery");
        for field in ["schema_version", "node_types", "limits", "request_schemas"] {
            assert!(required(nested_discovery).contains(&field));
        }

        let snapshot = response_schema("nodes_snapshot").unwrap();
        let nested_snapshot = field_schema(&snapshot, "snapshot");
        for field in [
            "identity",
            "revision",
            "semantic_revision",
            "graph",
            "diagnostics",
        ] {
            assert!(required(nested_snapshot).contains(&field));
        }

        let mutation = response_schema("nodes_mutate").unwrap();
        assert!(required(&mutation).contains(&"mutation"));
        assert!(required(field_schema(&mutation, "mutation")).contains(&"receipt"));

        let run = response_schema("nodes_run").unwrap();
        assert!(required(&run).contains(&"replayed"));
        assert!(required(field_schema(&run, "run")).contains(&"receipt"));

        let status = response_schema("nodes_status").unwrap();
        for field in ["run", "current", "stale", "report", "stderr", "can_apply"] {
            assert!(required(&status).contains(&field));
        }
        assert!(allows_null(&status["properties"]["report"]));
        assert!(allows_null(&status["properties"]["stderr"]));
        for field in ["status", "identity", "outputs", "errors"] {
            assert!(required(field_schema(&status, "run")).contains(&field));
        }

        let cancel = response_schema("nodes_cancel").unwrap();
        assert!(required(field_schema(&cancel, "cancellation")).contains(&"receipt"));

        let image = response_schema("nodes_image").unwrap();
        for field in ["recipe", "glyphs", "font_sha256", "canonical_input_sha256"] {
            assert!(required(&image).contains(&field));
        }
        assert!(!required(&image).contains(&"png_base64"));
        assert!(response_schema("nodes_apply").is_none());
    }

    #[test]
    fn image_result_keeps_flat_wire_fields_and_optional_mcp_image() {
        let result = NodesImageResult {
            ok: true,
            artifact_id: "proof-1".into(),
            current: false,
            stale: true,
            captured_document_epoch: "epoch".into(),
            captured_document_revision: 3,
            font_sha256: "font-hash".into(),
            canonical_input_sha256: "input-hash".into(),
            recipe: CompiledProofRecipe {
                text: "A".into(),
                normalized_location: vec![],
                right_to_left: false,
                features: vec![],
                script: None,
                language: None,
                rendering: CompiledProofRendering::default(),
                target: None,
            },
            recipe_sha256: "recipe-hash".into(),
            renderer: RendererIdentity {
                executable_path: "/tmp/designbot".into(),
                executable_sha256: "renderer-hash".into(),
                command: "render-scene --png".into(),
            },
            view: ProofView::Context,
            rendering: CompiledProofRendering::default(),
            target_glyph_index: None,
            crop: None,
            glyphs: vec![CompiledProofGlyph {
                glyph_id: 1,
                glyph_name: Some("A".into()),
                cluster: 0,
                x_advance: 500.0,
                x_offset: 0.0,
                y_offset: 0.0,
            }],
            png_base64: Some("iVBORw0KGgo=".into()),
            root_changed: false,
        };
        let with_image = serde_json::to_value(&result).unwrap();
        assert_eq!(with_image["artifact_id"], "proof-1");
        assert_eq!(with_image["recipe"]["script"], Value::Null);
        assert_eq!(with_image["glyphs"][0]["x_advance"], 500.0);
        assert_eq!(with_image["png_base64"], "iVBORw0KGgo=");

        let without_image = serde_json::to_value(NodesImageResult {
            png_base64: None,
            ..result
        })
        .unwrap();
        assert!(without_image.get("png_base64").is_none());
        assert_eq!(without_image["root_changed"], json!(false));
    }

    #[test]
    fn disk_run_result_requires_paths_and_typed_node_results() {
        let schema = disk_response_schema("nodes_run").unwrap();
        for field in ["ok", "file", "font", "nodes"] {
            assert!(required(&schema).contains(&field));
        }
        let node_items = &schema["properties"]["nodes"]["items"];
        let node_reference = node_items["$ref"]
            .as_str()
            .expect("node results use their typed definition");
        let node_schema = schema
            .pointer(node_reference.strip_prefix('#').unwrap())
            .expect("node result schema is defined");
        for field in ["id", "type", "status", "hash", "seconds"] {
            assert!(required(node_schema).contains(&field));
        }

        let result = DiskNodesRunResult {
            ok: true,
            file: PathBuf::from("comparison.nodes.json"),
            font: PathBuf::from("font.ufo"),
            nodes: Vec::new(),
        };
        let wire = serde_json::to_value(&result).unwrap();
        assert_eq!(
            wire,
            json!({
                "ok": true,
                "file": "comparison.nodes.json",
                "font": "font.ufo",
                "nodes": []
            })
        );
        assert_eq!(
            serde_json::from_value::<DiskNodesRunResult>(wire).unwrap(),
            result
        );
        assert!(disk_response_schema("nodes_status").is_none());
    }
}
