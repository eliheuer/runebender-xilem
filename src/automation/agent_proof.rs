// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Strict requests and discovery schemas for native asynchronous compiled proofs.
//!
//! These handles retain one proof artifact, not a reusable compiled font or an export grant.
//! The application owns document binding, worker lifetime, and bounded retention.

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::agent::Tool;
use crate::font::compiler::proof::CompiledProofRecipe;

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
