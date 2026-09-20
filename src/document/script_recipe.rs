// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Immutable inputs and guarded outputs for external editing recipes.
//!
//! A recipe receives only this bounded capture, never a live [`Project`](super::project::Project).
//! Its result is data for an application preview.
//! Applying a validated proposal remains the responsibility of the existing authorized edit path.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::agent_edit::{AgentEditOperation, AgentLayerEdits, AgentLayerGuard};

/// The only recipe JSON schema accepted by this version of Runebender.
pub const SCRIPT_RECIPE_SCHEMA_VERSION: u32 = 1;

/// Shared authoring guidance for local chat, MCP clients and graph discovery.
pub const AUTHORING_INSTRUCTIONS: &str = "Python recipes are optional clients of the Rust editor. \
Read one JSON object with json.load(sys.stdin); there is no mutable font object or implicit input \
variable. Input has schema_version, job_id, input_hash, source, parameters and layers. Each layer \
has guard, width and anchors; guard.glyph is its glyph name. Each anchor has id, optional name, \
x and y. Preserve every supplied guard and identity. Emit exactly one JSON object to stdout \
with schema_version, job_id and input_hash echoed unchanged, report as a string, reads as an \
array of unmodified layer guards, and edits as an array of {target: layer.guard, operations: [...]}. \
Use edits=[] for reports; include consulted non-edited layers in reads. Supported recipe operations \
are {op: set_width, width: number} and {op: set_anchor, anchor_id: anchor.id, x: number, y: number}; \
JSON keys and string values must be quoted. Do not invent IDs, open font source files, or import \
an editable Babelfont wrapper. Send diagnostics to stderr, not stdout. The user chooses an explicit \
source and 1 to 64 glyphs; at most 256 edit operations are accepted. Run creates a report or \
proposal only; Apply is separate and uses ordinary editor Undo. When asked to write a script, \
return a complete python fenced block for Open in Scripts, without running or saving it unless \
requested. Nodes uses this same recipe contract and retained before/after proof images.";

const MAX_STRING_BYTES: usize = 256;
const MAX_PARAMETER_BYTES: usize = 64 * 1024;
const MAX_LAYERS: usize = 64;
const MAX_ANCHORS: usize = 4_096;
const MAX_OPERATIONS: usize = 256;
const MAX_REPORT_BYTES: usize = 64 * 1024;

/// One existing anchor in an immutable recipe capture.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRecipeAnchor {
    /// Opaque stable anchor identity from the canonical layer.
    pub id: String,
    /// Optional anchor name.
    pub name: Option<String>,
    /// Horizontal coordinate in font units.
    pub x: f64,
    /// Vertical coordinate in font units.
    pub y: f64,
}

/// One immutable guarded layer supplied to a recipe.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRecipeLayer {
    /// Exact layer identity and revision captured by the host.
    pub guard: AgentLayerGuard,
    /// Exact horizontal advance in font units.
    pub width: f64,
    /// Existing anchors addressable by a version 1 recipe.
    pub anchors: Vec<ScriptRecipeAnchor>,
}

/// Complete bounded input written to one recipe's standard input.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRecipeInput {
    /// Must equal [`SCRIPT_RECIPE_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Host-generated identity for one run.
    pub job_id: String,
    /// SHA-256 of every other typed field in this input.
    pub input_hash: String,
    /// Explicit stable source ID shared by every captured layer.
    pub source: usize,
    /// Bounded user parameters for this run.
    pub parameters: BTreeMap<String, Value>,
    /// Immutable guarded layers in the explicitly selected scope.
    pub layers: Vec<ScriptRecipeLayer>,
}

/// Capture an explicit glyph scope from one canonical source for any recipe client.
/// No live font wrapper or mutable source object crosses the Python boundary.
pub fn capture(
    project: &super::project::Project,
    source: super::variable::SourceId,
    glyphs: &[String],
    job_id: String,
    parameters: BTreeMap<String, Value>,
) -> Result<ScriptRecipeInput, String> {
    if glyphs.is_empty() || glyphs.len() > MAX_LAYERS {
        return Err(format!("select between one and {MAX_LAYERS} glyphs"));
    }
    let layer_id = project
        .document_source(source)
        .ok_or("source is unavailable")?
        .default_layer();
    let mut layers = Vec::with_capacity(glyphs.len());
    for name in glyphs {
        let glyph = project
            .document_glyph(name)
            .ok_or_else(|| format!("glyph {name} is unavailable"))?;
        let layer = project
            .document_layer(name, &layer_id)
            .ok_or_else(|| format!("glyph {name} has no selected-source layer"))?;
        layers.push(ScriptRecipeLayer {
            guard: AgentLayerGuard {
                glyph: name.clone(),
                glyph_id: glyph.id().to_wire(),
                layer: layer_id.name.clone(),
                expected_revision: super::edit_batch::canonical_glyph_revision(layer)?,
            },
            width: layer.width(),
            anchors: layer
                .anchors()
                .map(|anchor| {
                    let position = anchor.position();
                    ScriptRecipeAnchor {
                        id: anchor.id().to_wire(),
                        name: (!anchor.name().is_empty()).then(|| anchor.name().to_owned()),
                        x: position.x,
                        y: position.y,
                    }
                })
                .collect(),
        });
    }
    ScriptRecipeInput::new(job_id, source.0, parameters, layers).map_err(|error| error.to_string())
}

/// Strict recipe output read from one standard-output JSON value.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRecipeResult {
    /// Must equal [`SCRIPT_RECIPE_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Exact echoed input job identity.
    pub job_id: String,
    /// Exact echoed input hash.
    pub input_hash: String,
    /// Bounded human-readable report shown before any apply action.
    pub report: String,
    /// Captured layers on which the proposal depends but does not edit.
    #[serde(default)]
    pub reads: Vec<AgentLayerGuard>,
    /// Optional guarded proposal using the canonical live-edit schema.
    #[serde(default)]
    pub edits: Vec<AgentLayerEdits>,
}

/// A malformed, forged, stale, or oversized recipe value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptRecipeValidationError {
    message: String,
}

impl ScriptRecipeValidationError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ScriptRecipeValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ScriptRecipeValidationError {}

#[derive(Serialize)]
struct InputHashPayload<'a> {
    schema_version: u32,
    job_id: &'a str,
    source: usize,
    parameters: &'a BTreeMap<String, Value>,
    layers: &'a [ScriptRecipeLayer],
}

impl ScriptRecipeInput {
    /// Build and hash one immutable version 1 capture.
    pub fn new(
        job_id: String,
        source: usize,
        parameters: BTreeMap<String, Value>,
        layers: Vec<ScriptRecipeLayer>,
    ) -> Result<Self, ScriptRecipeValidationError> {
        let mut input = Self {
            schema_version: SCRIPT_RECIPE_SCHEMA_VERSION,
            job_id,
            input_hash: String::new(),
            source,
            parameters,
            layers,
        };
        input.input_hash = input.computed_hash()?;
        input.validate()?;
        Ok(input)
    }

    /// Recompute the content hash and validate the complete bounded capture.
    pub fn validate(&self) -> Result<(), ScriptRecipeValidationError> {
        if self.schema_version != SCRIPT_RECIPE_SCHEMA_VERSION {
            return Err(ScriptRecipeValidationError::new(
                "unsupported recipe input schema_version",
            ));
        }
        validate_string("job_id", &self.job_id)?;
        if self.layers.is_empty() || self.layers.len() > MAX_LAYERS {
            return Err(ScriptRecipeValidationError::new(
                "recipe input must contain 1..=64 layers",
            ));
        }
        let parameter_bytes = serde_json::to_vec(&self.parameters)
            .map_err(|error| ScriptRecipeValidationError::new(error.to_string()))?;
        if parameter_bytes.len() > MAX_PARAMETER_BYTES {
            return Err(ScriptRecipeValidationError::new(
                "recipe parameters exceed 65536 serialized bytes",
            ));
        }
        for key in self.parameters.keys() {
            validate_string("parameter name", key)?;
        }

        let mut guards = BTreeSet::new();
        let mut anchor_count = 0_usize;
        for layer in &self.layers {
            validate_guard(&layer.guard)?;
            if !layer.width.is_finite() {
                return Err(ScriptRecipeValidationError::new(
                    "captured layer width must be finite",
                ));
            }
            if !guards.insert(guard_key(&layer.guard)) {
                return Err(ScriptRecipeValidationError::new(
                    "recipe input repeats a guarded layer",
                ));
            }
            let mut anchor_ids = BTreeSet::new();
            for anchor in &layer.anchors {
                anchor_count = anchor_count.saturating_add(1);
                validate_string("anchor id", &anchor.id)?;
                if let Some(name) = &anchor.name
                    && name.len() > MAX_STRING_BYTES
                {
                    return Err(ScriptRecipeValidationError::new(
                        "anchor name exceeds 256 UTF-8 bytes",
                    ));
                }
                if !anchor.x.is_finite() || !anchor.y.is_finite() {
                    return Err(ScriptRecipeValidationError::new(
                        "captured anchor coordinates must be finite",
                    ));
                }
                if !anchor_ids.insert(anchor.id.as_str()) {
                    return Err(ScriptRecipeValidationError::new(
                        "recipe input repeats an anchor identity in one layer",
                    ));
                }
            }
        }
        if anchor_count > MAX_ANCHORS {
            return Err(ScriptRecipeValidationError::new(
                "recipe input exceeds 4096 captured anchors",
            ));
        }
        if self.input_hash != self.computed_hash()? {
            return Err(ScriptRecipeValidationError::new(
                "input_hash does not match the immutable recipe capture",
            ));
        }
        Ok(())
    }

    fn computed_hash(&self) -> Result<String, ScriptRecipeValidationError> {
        let payload = InputHashPayload {
            schema_version: self.schema_version,
            job_id: &self.job_id,
            source: self.source,
            parameters: &self.parameters,
            layers: &self.layers,
        };
        let bytes = serde_json::to_vec(&payload)
            .map_err(|error| ScriptRecipeValidationError::new(error.to_string()))?;
        Ok(hex_digest(Sha256::digest(bytes)))
    }
}

impl ScriptRecipeResult {
    /// Validate identity, limits, and every proposed target against one capture.
    pub fn validate_against(
        &self,
        input: &ScriptRecipeInput,
    ) -> Result<(), ScriptRecipeValidationError> {
        input.validate()?;
        if self.schema_version != SCRIPT_RECIPE_SCHEMA_VERSION {
            return Err(ScriptRecipeValidationError::new(
                "unsupported recipe result schema_version",
            ));
        }
        if self.job_id != input.job_id || self.input_hash != input.input_hash {
            return Err(ScriptRecipeValidationError::new(
                "recipe result does not echo the captured job_id and input_hash",
            ));
        }
        if self.report.len() > MAX_REPORT_BYTES {
            return Err(ScriptRecipeValidationError::new(
                "recipe report exceeds 65536 UTF-8 bytes",
            ));
        }
        if self.reads.len() + self.edits.len() > MAX_LAYERS {
            return Err(ScriptRecipeValidationError::new(
                "recipe result exceeds 64 guarded layer entries",
            ));
        }

        for guard in &self.reads {
            captured_layer(input, guard)?;
        }
        let mut edited_guards = BTreeSet::new();
        let mut operation_count = 0_usize;
        for edit in &self.edits {
            let layer = captured_layer(input, &edit.target)?;
            if edit.operations.is_empty() {
                return Err(ScriptRecipeValidationError::new(
                    "each edited layer must contain at least one operation",
                ));
            }
            if !edited_guards.insert(guard_key(&edit.target)) {
                return Err(ScriptRecipeValidationError::new(
                    "recipe result repeats an edited layer",
                ));
            }
            operation_count = operation_count.saturating_add(edit.operations.len());
            for operation in &edit.operations {
                match operation {
                    AgentEditOperation::SetWidth { width } if width.is_finite() => {}
                    AgentEditOperation::SetWidth { .. } => {
                        return Err(ScriptRecipeValidationError::new(
                            "proposed width must be finite",
                        ));
                    }
                    AgentEditOperation::SetAnchor { anchor_id, x, y } => {
                        validate_string("proposed anchor id", anchor_id)?;
                        if !x.is_finite() || !y.is_finite() {
                            return Err(ScriptRecipeValidationError::new(
                                "proposed anchor coordinates must be finite",
                            ));
                        }
                        if !layer.anchors.iter().any(|anchor| anchor.id == *anchor_id) {
                            return Err(ScriptRecipeValidationError::new(
                                "proposed anchor is outside the captured layer scope",
                            ));
                        }
                    }
                    AgentEditOperation::SetPoint { .. } => {
                        return Err(ScriptRecipeValidationError::new(
                            "schema version 1 does not capture point identities",
                        ));
                    }
                }
            }
        }
        if operation_count > MAX_OPERATIONS {
            return Err(ScriptRecipeValidationError::new(
                "recipe result exceeds 256 operations",
            ));
        }
        Ok(())
    }
}

fn captured_layer<'a>(
    input: &'a ScriptRecipeInput,
    guard: &AgentLayerGuard,
) -> Result<&'a ScriptRecipeLayer, ScriptRecipeValidationError> {
    validate_guard(guard)?;
    input
        .layers
        .iter()
        .find(|layer| guards_equal(&layer.guard, guard))
        .ok_or_else(|| {
            ScriptRecipeValidationError::new(
                "recipe result references a layer outside the captured guarded scope",
            )
        })
}

fn validate_guard(guard: &AgentLayerGuard) -> Result<(), ScriptRecipeValidationError> {
    validate_string("guard glyph", &guard.glyph)?;
    validate_string("guard glyph_id", &guard.glyph_id)?;
    validate_string("guard layer", &guard.layer)?;
    validate_string("guard expected_revision", &guard.expected_revision)
}

fn validate_string(name: &str, value: &str) -> Result<(), ScriptRecipeValidationError> {
    if value.is_empty() || value.len() > MAX_STRING_BYTES {
        return Err(ScriptRecipeValidationError::new(format!(
            "{name} must contain 1..=256 UTF-8 bytes"
        )));
    }
    Ok(())
}

fn guards_equal(left: &AgentLayerGuard, right: &AgentLayerGuard) -> bool {
    left.glyph == right.glyph
        && left.glyph_id == right.glyph_id
        && left.layer == right.layer
        && left.expected_revision == right.expected_revision
}

fn guard_key(guard: &AgentLayerGuard) -> (&str, &str, &str, &str) {
    (
        &guard.glyph,
        &guard.glyph_id,
        &guard.layer,
        &guard.expected_revision,
    )
}

fn hex_digest(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard(revision: &str) -> AgentLayerGuard {
        AgentLayerGuard {
            glyph: "A".into(),
            glyph_id: "glyph-a".into(),
            layer: "public.default".into(),
            expected_revision: revision.into(),
        }
    }

    fn input() -> ScriptRecipeInput {
        ScriptRecipeInput::new(
            "job-1".into(),
            3,
            BTreeMap::new(),
            vec![ScriptRecipeLayer {
                guard: guard("revision-1"),
                width: 600.0,
                anchors: vec![ScriptRecipeAnchor {
                    id: "anchor-top".into(),
                    name: Some("top".into()),
                    x: 300.0,
                    y: 700.0,
                }],
            }],
        )
        .expect("valid input")
    }

    #[test]
    fn input_hash_covers_parameters_and_scope() {
        let mut changed = input();
        changed.parameters.insert("dx".into(), Value::from(10));
        assert_ne!(changed.computed_hash().expect("hash"), changed.input_hash);
        assert!(changed.validate().is_err());
    }

    #[test]
    fn validates_captured_anchor_edit_without_live_mutation() {
        let input = input();
        let result = ScriptRecipeResult {
            schema_version: SCRIPT_RECIPE_SCHEMA_VERSION,
            job_id: input.job_id.clone(),
            input_hash: input.input_hash.clone(),
            report: "Moved top".into(),
            reads: Vec::new(),
            edits: vec![AgentLayerEdits {
                target: guard("revision-1"),
                operations: vec![AgentEditOperation::SetAnchor {
                    anchor_id: "anchor-top".into(),
                    x: 310.0,
                    y: 700.0,
                }],
            }],
        };
        result.validate_against(&input).expect("valid proposal");
    }

    #[test]
    fn rejects_forged_or_stale_scope() {
        let input = input();
        let result = ScriptRecipeResult {
            schema_version: SCRIPT_RECIPE_SCHEMA_VERSION,
            job_id: input.job_id.clone(),
            input_hash: input.input_hash.clone(),
            report: String::new(),
            reads: vec![guard("forged-revision")],
            edits: Vec::new(),
        };
        assert!(result.validate_against(&input).is_err());
    }

    #[test]
    fn rejects_uncaptured_objects_and_point_edits() {
        let input = input();
        let unknown_anchor = ScriptRecipeResult {
            schema_version: SCRIPT_RECIPE_SCHEMA_VERSION,
            job_id: input.job_id.clone(),
            input_hash: input.input_hash.clone(),
            report: String::new(),
            reads: Vec::new(),
            edits: vec![AgentLayerEdits {
                target: guard("revision-1"),
                operations: vec![AgentEditOperation::SetAnchor {
                    anchor_id: "anchor-outside-capture".into(),
                    x: 0.0,
                    y: 0.0,
                }],
            }],
        };
        assert!(unknown_anchor.validate_against(&input).is_err());

        let point = ScriptRecipeResult {
            edits: vec![AgentLayerEdits {
                target: guard("revision-1"),
                operations: vec![AgentEditOperation::SetPoint {
                    point_id: "point-1".into(),
                    x: 0.0,
                    y: 0.0,
                }],
            }],
            ..unknown_anchor
        };
        assert!(point.validate_against(&input).is_err());
    }
}
