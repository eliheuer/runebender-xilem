// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Strict requests, typed results, and discovery schemas for native compiled proof jobs.
//!
//! These handles retain one proof artifact, not a reusable compiled font or an export grant.
//! The application owns document binding, worker lifetime, and bounded retention.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::agent::Tool;
use crate::font::compiler::proof::{CompiledProofGlyph, CompiledProofRecipe, CompilerIdentity};

/// Capture and enqueue one proof from an exact live document revision.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProofStartRequest {
    /// Required endpoint lifetime guard.
    pub expected_document_epoch: String,
    /// Canonical document revision returned by a live read.
    pub expected_document_revision: u64,
    /// Document-local retry identity, retained until explicit release.
    pub operation_key: String,
    /// Text, shaping options, and normalized coordinates for the entire family.
    pub recipe: CompiledProofRecipe,
}

/// Inspect one retained proof, optionally including its already rendered image.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProofStatusRequest {
    /// Required endpoint lifetime guard.
    pub expected_document_epoch: String,
    /// Opaque handle returned by `proof_start`.
    pub proof_id: String,
    /// Include the worker's PNG when completed; defaults to metadata only.
    #[serde(default)]
    pub include_image: bool,
}

/// Cancel queued work or release a terminal proof artifact.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProofHandleRequest {
    /// Required endpoint lifetime guard.
    pub expected_document_epoch: String,
    /// Opaque handle returned by `proof_start`.
    pub proof_id: String,
}

/// Receipt for a newly enqueued proof or an identical retained retry.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ProofStartResult {
    /// Whether the request succeeded.
    pub ok: bool,
    /// Opaque identifier for the retained job.
    pub proof_id: String,
    /// Whether an identical retained operation key returned the prior handle.
    pub replayed: bool,
    /// Document lifetime captured with the job.
    pub captured_document_epoch: String,
    /// Canonical document revision captured with the job.
    pub captured_document_revision: u64,
    /// Proof requests never change the canonical font.
    pub root_changed: bool,
}

/// Receipt for releasing a terminal proof and its retry key.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ProofReleaseResult {
    /// Whether the request succeeded.
    pub ok: bool,
    /// Identifier of the released proof.
    pub proof_id: String,
    /// Whether the terminal proof was released.
    pub released: bool,
    /// Releasing a proof never changes the canonical font.
    pub root_changed: bool,
}

/// Outcome of a request to cancel a retained proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProofCancellation {
    /// Queued work was cancelled before compilation.
    CancelledBeforeStart,
    /// The worker had already started or reached a terminal state.
    TooLate,
    /// The queue no longer retains the handle.
    Unknown,
}

/// Worker state and state-specific proof data in one flat JSON object.
///
/// The `status` discriminator makes completed proof metadata and failed-job errors
/// required only for their corresponding states.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProofOutcomeResult {
    /// The worker has not begun compiling.
    Queued,
    /// The worker is compiling or rendering.
    Running,
    /// The completed proof and its metadata are retained.
    Completed(Box<CompletedProofResult>),
    /// Compilation or rendering failed.
    Failed {
        /// Worker failure message.
        proof_error: String,
    },
    /// Cancellation prevented compilation from starting.
    CancelledBeforeStart,
}

/// Required metrics and recipe of a completed proof, with an optional PNG.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct CompletedProofResult {
    /// SHA-256 of the exact compiled font bytes.
    pub font_sha256: String,
    /// SHA-256 of the captured canonical compiler input.
    pub canonical_input_sha256: String,
    /// Compiler identity of the completed font bytes.
    pub compiler: CompilerIdentity,
    /// Exact shaping and rendering recipe used by the completed job.
    pub recipe: CompiledProofRecipe,
    /// Positioned glyph metrics in paint order.
    pub glyphs: Vec<CompiledProofGlyph>,
    /// Optional PNG transport payload, omitted by MCP after image extraction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub png_base64: Option<String>,
}

/// Inspection receipt for any retained proof state.
///
/// Completion fields appear only for completed jobs; `proof_error` appears only
/// for failed jobs.
/// The PNG is optional and may be extracted into an MCP image.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ProofStatusResult {
    /// Whether the request succeeded.
    pub ok: bool,
    /// Opaque identifier for the inspected job.
    pub proof_id: String,
    /// Proof inspection never changes the canonical font.
    pub root_changed: bool,
    /// Document lifetime captured with the job.
    pub captured_document_epoch: String,
    /// Canonical document revision captured with the job.
    pub captured_document_revision: u64,
    /// Whether the captured lineage still matches the live document.
    pub current: bool,
    /// Whether the captured lineage differs from the live document.
    pub stale: bool,
    /// Cancellation result, present only on a cancellation request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellation: Option<ProofCancellation>,
    /// State and required state-specific payload, serialized into this object.
    #[serde(flatten)]
    pub outcome: ProofOutcomeResult,
}

/// Schema of the success receipt for one native proof tool.
///
/// The host composes these generated contracts with its generic error envelope.
pub fn success_schema(name: &str) -> Option<Value> {
    let schema = match name {
        "proof_start" => schemars::schema_for!(ProofStartResult),
        "proof_status" | "proof_cancel" => schemars::schema_for!(ProofStatusResult),
        "proof_release" => schemars::schema_for!(ProofReleaseResult),
        _ => return None,
    };
    Some(serde_json::to_value(schema).expect("proof result schema serializes"))
}

/// Return strict native proof schemas without legacy source/branch decorations.
pub fn tools() -> Vec<Tool> {
    let epoch = json!({"type":"string","minLength":1,"maxLength":256});
    let handle = json!({"type":"string","minLength":1,"maxLength":32});
    let mut result = vec![Tool {
        name: "proof_start".into(),
        description: "Capture the exact canonical live revision and enqueue a full-family compiled PNG proof. Does not mutate or save the font. Repeating a retained operation_key with the identical request returns the original handle; different payloads reject. Poll proof_status. At most eight retained jobs per document; release terminal jobs explicitly. The handle retains this proof only, not reusable font bytes.".into(),
        parameters: json!({"type":"object","additionalProperties":false,
            "required":["expected_document_epoch","expected_document_revision","operation_key","recipe"],
            "properties":{
                "expected_document_epoch":epoch,
                "expected_document_revision":{"type":"integer","minimum":0},
                "operation_key":{"type":"string","minLength":1,"maxLength":128},
                "recipe":{"type":"object","additionalProperties":false,
                    "required":["text","normalized_location","right_to_left","features"],
                    "properties":{
                        "text":{"type":"string","minLength":1,"maxLength":4096},
                        "normalized_location":{"type":"array","maxItems":64,"items":{"type":"number","minimum":-1,"maximum":1}},
                        "right_to_left":{"type":"boolean"},
                        "features":{"type":"array","maxItems":64,"items":{"type":"array","minItems":2,"maxItems":2,"prefixItems":[{"type":"string","minLength":4,"maxLength":4},{"type":"boolean"}]}},
                        "script":{"type":["string","null"],"minLength":4,"maxLength":4},
                        "language":{"type":["string","null"],"minLength":1,"maxLength":35}
                    }}
            }}),
    }];
    for (name, description, image) in [
        (
            "proof_status",
            "Read one proof's captured lineage and status. include_image returns its compiled PNG directly when completed, even if stale; current compares captured epoch/revision with the live document. Does not recapture or render.",
            true,
        ),
        (
            "proof_cancel",
            "Cancel queued proof work. Running compilation cannot be interrupted and returns too_late; it retains its original lineage and cannot become a current result for a newer revision.",
            false,
        ),
        (
            "proof_release",
            "Release a terminal proof and its retry key. Queued/running jobs reject as proof_busy. After release the key may start a new proof; old handles become unknown.",
            false,
        ),
    ] {
        let mut parameters = json!({"type":"object","additionalProperties":false,
            "required":["expected_document_epoch","proof_id"],
            "properties":{"expected_document_epoch":epoch,"proof_id":handle}});
        if image {
            parameters["properties"]["include_image"] = json!({"type":"boolean","default":false});
        }
        result.push(Tool {
            name: name.into(),
            description: description.into(),
            parameters,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(outcome: ProofOutcomeResult) -> ProofStatusResult {
        ProofStatusResult {
            ok: true,
            proof_id: "7".into(),
            root_changed: false,
            captured_document_epoch: "epoch".into(),
            captured_document_revision: 3,
            current: false,
            stale: true,
            cancellation: None,
            outcome,
        }
    }

    #[test]
    fn proof_result_states_keep_the_existing_flat_wire_shape() {
        let start = serde_json::to_value(ProofStartResult {
            ok: true,
            proof_id: "7".into(),
            replayed: false,
            captured_document_epoch: "epoch".into(),
            captured_document_revision: 3,
            root_changed: false,
        })
        .unwrap();
        assert_eq!(
            start,
            json!({
                "ok":true,"proof_id":"7","replayed":false,
                "captured_document_epoch":"epoch","captured_document_revision":3,
                "root_changed":false
            })
        );

        let pending = serde_json::to_value(status(ProofOutcomeResult::Queued)).unwrap();
        assert_eq!(pending["status"], "queued");
        assert!(pending.get("cancellation").is_none());
        assert!(pending.get("recipe").is_none());
        assert!(pending.get("png_base64").is_none());

        let mut cancelled = status(ProofOutcomeResult::CancelledBeforeStart);
        cancelled.cancellation = Some(ProofCancellation::CancelledBeforeStart);
        let cancelled = serde_json::to_value(cancelled).unwrap();
        assert_eq!(cancelled["status"], "cancelled_before_start");
        assert_eq!(cancelled["cancellation"], "cancelled_before_start");

        let failed = status(ProofOutcomeResult::Failed {
            proof_error: "compiler failed".into(),
        });
        let failed = serde_json::to_value(failed).unwrap();
        assert_eq!(failed["proof_error"], "compiler failed");
        assert!(failed.get("font_sha256").is_none());

        let completed = status(ProofOutcomeResult::Completed(Box::new(
            CompletedProofResult {
                font_sha256: "font-hash".into(),
                canonical_input_sha256: "input-hash".into(),
                compiler: CompilerIdentity {
                    label: "compiler".into(),
                    sha256: "compiler-hash".into(),
                },
                recipe: CompiledProofRecipe {
                    text: "A".into(),
                    normalized_location: vec![],
                    right_to_left: false,
                    features: vec![],
                    script: None,
                    language: None,
                },
                glyphs: vec![CompiledProofGlyph {
                    glyph_id: 1,
                    glyph_name: Some("A".into()),
                    cluster: 0,
                    x_advance: 500.0,
                    x_offset: 0.0,
                    y_offset: 0.0,
                }],
                png_base64: Some("iVBORw0KGgo=".into()),
            },
        )));
        let completed = serde_json::to_value(completed).unwrap();
        assert_eq!(completed["status"], "completed");
        assert_eq!(completed["recipe"]["text"], "A");
        assert_eq!(completed["recipe"]["script"], Value::Null);
        assert_eq!(completed["glyphs"][0]["x_advance"], 500.0);
        assert_eq!(completed["png_base64"], "iVBORw0KGgo=");
        assert!(completed.get("proof_error").is_none());
        assert!(serde_json::from_value::<ProofStatusResult>(completed.clone()).is_ok());

        let mut missing_metric = completed;
        missing_metric.as_object_mut().unwrap().remove("glyphs");
        assert!(serde_json::from_value::<ProofStatusResult>(missing_metric).is_err());
        let mut missing_error = failed;
        missing_error.as_object_mut().unwrap().remove("proof_error");
        assert!(serde_json::from_value::<ProofStatusResult>(missing_error).is_err());
    }

    #[test]
    fn proof_schema_describes_typed_completion_fields_and_optional_image() {
        let schema = success_schema("proof_status").unwrap();
        let states = schema["oneOf"].as_array().expect("tagged proof states");
        let branch = |status: &str| {
            states
                .iter()
                .find(|state| state["properties"]["status"]["const"] == status)
                .unwrap_or_else(|| panic!("missing {status} branch in {schema:#}"))
        };
        fn required_fields(root: &Value, node: &Value, fields: &mut Vec<String>) {
            // In draft 2020-12, a $ref applies alongside its sibling constraints.
            if let Some(required) = node["required"].as_array() {
                fields.extend(required.iter().filter_map(Value::as_str).map(str::to_owned));
            }
            if let Some(reference) = node["$ref"].as_str() {
                let pointer = reference.strip_prefix('#').expect("local proof definition");
                required_fields(
                    root,
                    root.pointer(pointer).expect("proof definition"),
                    fields,
                );
            }
        }

        let completed = branch("completed");
        let mut completed_required = Vec::new();
        required_fields(&schema, completed, &mut completed_required);
        for field in [
            "status",
            "font_sha256",
            "canonical_input_sha256",
            "compiler",
            "recipe",
            "glyphs",
        ] {
            assert!(
                completed_required.contains(&field.to_owned()),
                "missing {field}"
            );
        }
        assert!(!completed_required.contains(&"png_base64".to_owned()));

        let failed = branch("failed");
        let mut failed_required = Vec::new();
        required_fields(&schema, failed, &mut failed_required);
        assert!(failed_required.contains(&"proof_error".to_owned()));

        let queued = branch("queued");
        let mut queued_required = Vec::new();
        required_fields(&schema, queued, &mut queued_required);
        assert!(!queued_required.contains(&"proof_error".to_owned()));
        assert!(!queued_required.contains(&"glyphs".to_owned()));
        assert!(success_schema("proof_unknown").is_none());
    }
}
