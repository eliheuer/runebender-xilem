// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Immutable inputs and guarded outputs for external editing recipes.
//!
//! A recipe receives only this bounded capture, never a live [`Project`](crate::font::project::Project).
//! Its result is data for an application preview.
//! Applying a validated proposal remains the responsibility of the existing authorized edit path.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::agent_edit::{AgentEditOperation, AgentLayerEdits, AgentLayerGuard};
use crate::font::{CanonicalLayerSnapshot, edit_batch, project, variable};
use crate::outline::drawing::DrawingPointType;

/// The current recipe input and result JSON schema emitted by Runebender.
pub const SCRIPT_RECIPE_SCHEMA_VERSION: u32 = 2;
const LEGACY_SCRIPT_RECIPE_SCHEMA_VERSION: u32 = 1;

/// Shared authoring guidance for local chat, MCP clients and graph discovery.
pub const AUTHORING_INSTRUCTIONS: &str = "Python recipes are optional clients of the Rust editor. \
Read one JSON object with json.load(sys.stdin); there is no mutable font object or implicit input \
variable. Input schema_version is 2 and has job_id, input_hash, source, parameters and layers. \
Each layer has guard, width and anchors; nonempty contours and components are included, and \
missing arrays mean empty. guard.glyph is its glyph name. \
Contours list stable IDs, closed/hyper flags and ordered points with id, x, y, type and smooth. \
Components list references and transforms; their outlines are not flattened into contour points. \
Each anchor has id, optional name, x and y. Preserve every supplied guard and identity. Emit \
exactly one JSON object to stdout with the input schema_version, job_id and input_hash echoed \
unchanged, report as a string, reads as an \
array of unmodified layer guards, and edits as an array of {target: layer.guard, operations: [...]}. \
Use edits=[] for reports; include consulted non-edited layers in reads. Supported recipe operations \
include {op: set_width, width: number}, {op: set_anchor, anchor_id: anchor.id, x: number, y: number} \
and {op: set_point, point_id: point.id, x: number, y: number} for captured contour points. \
Use {op: append_contours, contours: [{points: [{x: number, y: number, type: string, smooth: bool}]}]} \
for new ordinary contours; use replace_contours with the same contour payload to replace all \
ordinary contours, or an empty list to clear them. Do not supply IDs. \
Point types are move, line, curve, qcurve and offcurve. \
A leading move starts an open contour; closed contours contain no moves. Cubics need two controls; \
quadratics need one or more. Generated edits share 256 contours and 4096 points in the whole result, with \
coordinates within one million font units. Append retains existing contours; replacement retains \
components, anchors, advance and metadata while minting fresh contour and point IDs. \
Old scripts emitting schema_version 1 remain accepted for width and anchor edits only; \
point or structural edits require version 2. \
JSON keys and string values must be quoted. Do not invent IDs, open font source files, or import \
an editable Babelfont wrapper. Send diagnostics to stderr, not stdout. The user chooses an explicit \
source and 1 to 64 glyphs; captures are bounded to 256 contours and 4096 points in total, and \
at most 256 edit operations are accepted. Run creates a report or \
proposal only; Apply is separate and uses ordinary editor Undo. When asked to write a script, \
return a complete python fenced block for Open in Scripts, without running or saving it unless \
requested. Nodes uses this same recipe contract and retained before/after proof images.";

const MAX_STRING_BYTES: usize = 256;
const MAX_PARAMETER_BYTES: usize = 64 * 1024;
const MAX_LAYERS: usize = 64;
const MAX_ANCHORS: usize = 4_096;
const MAX_CONTOURS: usize = 256;
const MAX_POINTS: usize = 4_096;
const MAX_COMPONENTS: usize = 4_096;
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

/// One existing point in an immutable canonical contour capture.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRecipePoint {
    /// Opaque stable point identity from the canonical layer.
    pub id: String,
    /// Horizontal coordinate in font units.
    pub x: f64,
    /// Vertical coordinate in font units.
    pub y: f64,
    /// Canonical segment role.
    #[serde(rename = "type")]
    pub point_type: DrawingPointType,
    /// Whether the stored point is marked smooth; not a measured continuity guarantee.
    pub smooth: bool,
}

/// One canonical contour in storage order, without component expansion.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRecipeContour {
    /// Opaque stable contour identity.
    pub id: String,
    /// Whether the last point connects to the first.
    pub closed: bool,
    /// Whether the contour uses editable hyperbezier semantics.
    pub hyper: bool,
    /// Canonical points in contour order.
    pub points: Vec<ScriptRecipePoint>,
}

/// Component dependency context, kept separate from direct contour points.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRecipeComponent {
    /// Opaque stable component identity.
    pub id: String,
    /// Referenced glyph name.
    pub reference: String,
    /// Exact six-coefficient affine transform in font coordinates.
    pub transform: [f64; 6],
}

/// One immutable guarded layer supplied to a recipe.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRecipeLayer {
    /// Exact layer identity and revision captured by the host.
    pub guard: AgentLayerGuard,
    /// Exact horizontal advance in font units.
    pub width: f64,
    /// Existing anchors addressable by a recipe.
    pub anchors: Vec<ScriptRecipeAnchor>,
    /// Direct canonical contours, absent from legacy version 1 captures.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contours: Vec<ScriptRecipeContour>,
    /// Component references, kept separate from direct contours.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<ScriptRecipeComponent>,
}

/// Complete bounded input written to one recipe's standard input.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRecipeInput {
    /// Version 2 for new captures; version 1 remains readable without outlines.
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
    project: &project::Project,
    source: variable::SourceId,
    glyphs: &[String],
    job_id: String,
    parameters: BTreeMap<String, Value>,
) -> Result<ScriptRecipeInput, String> {
    capture_with(project, source, glyphs, job_id, parameters, |address| {
        project
            .capture_document_layer(address)
            .ok_or_else(|| format!("glyph {} has no selected-source layer", address.glyph))
    })
}

/// Capture the detached state after a staged transaction without publishing it to the document.
/// Guards and object IDs describe the parent's exact overlay, including generated contours.
pub fn capture_staged(
    project: &project::Project,
    parent: &project::CanonicalDocumentEditTransaction,
    source: variable::SourceId,
    glyphs: &[String],
    job_id: String,
    parameters: BTreeMap<String, Value>,
) -> Result<ScriptRecipeInput, String> {
    capture_with(project, source, glyphs, job_id, parameters, |address| {
        project
            .capture_document_edit_layer(parent, address)
            .map_err(|error| error.to_string())
    })
}

fn capture_with(
    project: &project::Project,
    source: variable::SourceId,
    glyphs: &[String],
    job_id: String,
    parameters: BTreeMap<String, Value>,
    mut snapshot_for: impl FnMut(&variable::GlyphLayerAddress) -> Result<CanonicalLayerSnapshot, String>,
) -> Result<ScriptRecipeInput, String> {
    if glyphs.is_empty() || glyphs.len() > MAX_LAYERS {
        return Err(format!("select between one and {MAX_LAYERS} glyphs"));
    }
    let layer_id = project
        .document_source(source)
        .ok_or("source is unavailable")?
        .default_layer();
    let mut layers = Vec::with_capacity(glyphs.len());
    let mut contour_count = 0_usize;
    let mut point_count = 0_usize;
    let mut component_count = 0_usize;
    for name in glyphs {
        let glyph = project
            .document_glyph(name)
            .ok_or_else(|| format!("glyph {name} is unavailable"))?;
        let address = variable::GlyphLayerAddress {
            glyph: name.clone(),
            layer: layer_id.clone(),
        };
        let snapshot = snapshot_for(&address)?;
        let layer = snapshot.view();
        let mut contours = Vec::new();
        for contour in layer.contours() {
            contour_count += 1;
            if contour_count > MAX_CONTOURS {
                return Err("recipe capture exceeds 256 contours".into());
            }
            let mut points = Vec::new();
            for point in contour.points() {
                point_count += 1;
                if point_count > MAX_POINTS {
                    return Err("recipe capture exceeds 4096 points".into());
                }
                let position = point.position();
                points.push(ScriptRecipePoint {
                    id: point.id().to_wire(),
                    x: position.x,
                    y: position.y,
                    point_type: point.point_type().into(),
                    smooth: point.is_smooth(),
                });
            }
            contours.push(ScriptRecipeContour {
                id: contour.id().to_wire(),
                closed: contour.is_closed(),
                hyper: contour.is_hyper(),
                points,
            });
        }
        let mut components = Vec::new();
        for component in layer.components() {
            component_count += 1;
            if component_count > MAX_COMPONENTS {
                return Err("recipe capture exceeds 4096 components".into());
            }
            components.push(ScriptRecipeComponent {
                id: component.id().to_wire(),
                reference: component.reference().into(),
                transform: component.transform().as_coeffs(),
            });
        }
        layers.push(ScriptRecipeLayer {
            guard: AgentLayerGuard {
                glyph: name.clone(),
                glyph_id: glyph.id().to_wire(),
                layer: layer_id.name.clone(),
                expected_revision: edit_batch::canonical_glyph_revision(layer)?,
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
            contours,
            components,
        });
    }
    ScriptRecipeInput::new(job_id, source.0, parameters, layers).map_err(|error| error.to_string())
}

/// Strict recipe output read from one standard-output JSON value.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRecipeResult {
    /// Echo version 2 for current recipes; legacy version 1 has limited operations.
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
    /// Build and hash one immutable current-version capture.
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
        if !matches!(
            self.schema_version,
            LEGACY_SCRIPT_RECIPE_SCHEMA_VERSION | SCRIPT_RECIPE_SCHEMA_VERSION
        ) {
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
        let mut contour_count = 0_usize;
        let mut point_count = 0_usize;
        let mut component_count = 0_usize;
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
            if self.schema_version == LEGACY_SCRIPT_RECIPE_SCHEMA_VERSION
                && (!layer.contours.is_empty() || !layer.components.is_empty())
            {
                return Err(ScriptRecipeValidationError::new(
                    "version 1 recipe inputs cannot contain outline captures",
                ));
            }
            let mut contour_ids = BTreeSet::new();
            let mut point_ids = BTreeSet::new();
            for contour in &layer.contours {
                contour_count = contour_count.saturating_add(1);
                validate_string("contour id", &contour.id)?;
                if !contour_ids.insert(contour.id.as_str()) {
                    return Err(ScriptRecipeValidationError::new(
                        "recipe input repeats a contour identity in one layer",
                    ));
                }
                for point in &contour.points {
                    point_count = point_count.saturating_add(1);
                    validate_string("point id", &point.id)?;
                    if !point.x.is_finite() || !point.y.is_finite() {
                        return Err(ScriptRecipeValidationError::new(
                            "captured point coordinates must be finite",
                        ));
                    }
                    if !point_ids.insert(point.id.as_str()) {
                        return Err(ScriptRecipeValidationError::new(
                            "recipe input repeats a point identity in one layer",
                        ));
                    }
                }
            }
            let mut component_ids = BTreeSet::new();
            for component in &layer.components {
                component_count = component_count.saturating_add(1);
                validate_string("component id", &component.id)?;
                validate_string("component reference", &component.reference)?;
                if !component.transform.iter().all(|value| value.is_finite()) {
                    return Err(ScriptRecipeValidationError::new(
                        "captured component transform must be finite",
                    ));
                }
                if !component_ids.insert(component.id.as_str()) {
                    return Err(ScriptRecipeValidationError::new(
                        "recipe input repeats a component identity in one layer",
                    ));
                }
            }
        }
        if anchor_count > MAX_ANCHORS {
            return Err(ScriptRecipeValidationError::new(
                "recipe input exceeds 4096 captured anchors",
            ));
        }
        if contour_count > MAX_CONTOURS || point_count > MAX_POINTS {
            return Err(ScriptRecipeValidationError::new(
                "recipe input exceeds 256 contours or 4096 points",
            ));
        }
        if component_count > MAX_COMPONENTS {
            return Err(ScriptRecipeValidationError::new(
                "recipe input exceeds 4096 components",
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
        if self.schema_version != input.schema_version
            && !(self.schema_version == LEGACY_SCRIPT_RECIPE_SCHEMA_VERSION
                && input.schema_version == SCRIPT_RECIPE_SCHEMA_VERSION)
        {
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
        let (mut generated_contours, mut generated_points) = (0_usize, 0_usize);
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
                    AgentEditOperation::SetPoint { point_id, x, y } => {
                        if self.schema_version != SCRIPT_RECIPE_SCHEMA_VERSION {
                            return Err(ScriptRecipeValidationError::new(
                                "point edits require recipe result schema_version 2",
                            ));
                        }
                        validate_string("proposed point id", point_id)?;
                        if !x.is_finite() || !y.is_finite() {
                            return Err(ScriptRecipeValidationError::new(
                                "proposed point coordinates must be finite",
                            ));
                        }
                        if !layer
                            .contours
                            .iter()
                            .flat_map(|contour| &contour.points)
                            .any(|point| point.id == *point_id)
                        {
                            return Err(ScriptRecipeValidationError::new(
                                "proposed point is outside the captured layer scope",
                            ));
                        }
                    }
                    AgentEditOperation::AppendContours { contours }
                    | AgentEditOperation::ReplaceContours { contours } => {
                        use crate::font::generated;
                        if self.schema_version != SCRIPT_RECIPE_SCHEMA_VERSION {
                            return Err(ScriptRecipeValidationError::new(
                                "generated contour edits require recipe result schema_version 2",
                            ));
                        }
                        generated_contours = generated_contours.saturating_add(contours.len());
                        for contour in contours {
                            generated_points =
                                generated_points.saturating_add(contour.points.len());
                        }
                        if generated_contours > generated::MAX_GENERATED_CONTOURS
                            || generated_points > generated::MAX_GENERATED_POINTS
                        {
                            return Err(ScriptRecipeValidationError::new(
                                "recipe result exceeds the whole-batch generated geometry limit",
                            ));
                        }
                        let contours = contours.iter().map(Into::into).collect::<Vec<_>>();
                        if contours.is_empty()
                            && matches!(operation, AgentEditOperation::AppendContours { .. })
                        {
                            return Err(ScriptRecipeValidationError::new(
                                "contour append needs at least one contour",
                            ));
                        }
                        if !contours.is_empty() {
                            generated::validate_contours(&contours).map_err(|error| {
                                ScriptRecipeValidationError::new(error.to_string())
                            })?;
                        }
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
                contours: Vec::new(),
                components: Vec::new(),
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

    #[test]
    fn canonical_outline_capture_allows_only_its_point_id() {
        let mut project = project::Project::new_font("recipe-capture-never-saved.ufo".into());
        project.add_document_glyph("A", 600.0, None).unwrap();
        let source = variable::SourceId(0);
        let layer_id = project.document_source(source).unwrap().default_layer();
        project
            .edit_document_layer("A", &layer_id, |draft| {
                draft.add_shape_contour(kurbo::Rect::new(0.0, 0.0, 100.0, 100.0), false)?;
                Ok(())
            })
            .unwrap();
        let input = capture(
            &project,
            source,
            &["A".into()],
            "point-job".into(),
            BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(input.schema_version, 2);
        assert_eq!(input.layers[0].contours.len(), 1);
        assert!(input.layers[0].contours[0].closed);
        assert!(!input.layers[0].contours[0].hyper);
        assert_eq!(input.layers[0].contours[0].points.len(), 4);
        assert!(input.layers[0].components.is_empty());
        let point_id = input.layers[0].contours[0].points[0].id.clone();
        let make_result = |point_id: String| ScriptRecipeResult {
            schema_version: 2,
            job_id: input.job_id.clone(),
            input_hash: input.input_hash.clone(),
            report: "Move captured point".into(),
            reads: Vec::new(),
            edits: vec![AgentLayerEdits {
                target: input.layers[0].guard.clone(),
                operations: vec![AgentEditOperation::SetPoint {
                    point_id,
                    x: 12.0,
                    y: 15.0,
                }],
            }],
        };
        make_result(point_id).validate_against(&input).unwrap();
        assert!(
            make_result("forged-point".into())
                .validate_against(&input)
                .is_err()
        );

        let mut changed = input.clone();
        changed.layers[0].contours[0].points[0].x += 1.0;
        assert_ne!(changed.computed_hash().unwrap(), input.input_hash);
        assert!(changed.validate().is_err());
    }

    #[test]
    fn rejects_oversized_nonfinite_and_duplicate_outline_captures() {
        let mut input = input();
        input.layers[0].contours = (0..=MAX_CONTOURS)
            .map(|index| ScriptRecipeContour {
                id: format!("contour-{index}"),
                closed: false,
                hyper: false,
                points: vec![ScriptRecipePoint {
                    id: format!("point-{index}"),
                    x: 0.0,
                    y: 0.0,
                    point_type: DrawingPointType::Move,
                    smooth: false,
                }],
            })
            .collect();
        assert!(
            input
                .validate()
                .unwrap_err()
                .to_string()
                .contains("256 contours")
        );

        input.layers[0].contours.truncate(1);
        input.layers[0].contours[0].points[0].x = f64::INFINITY;
        assert!(input.validate().unwrap_err().to_string().contains("finite"));

        input.layers[0].contours[0].points[0].x = 0.0;
        let duplicate = input.layers[0].contours[0].points[0].clone();
        input.layers[0].contours[0].points.push(duplicate);
        assert!(
            input
                .validate()
                .unwrap_err()
                .to_string()
                .contains("repeats a point")
        );

        input.layers[0].contours[0].points = (0..=MAX_POINTS)
            .map(|index| ScriptRecipePoint {
                id: format!("point-{index}"),
                x: 0.0,
                y: 0.0,
                point_type: DrawingPointType::Line,
                smooth: false,
            })
            .collect();
        assert!(
            input
                .validate()
                .unwrap_err()
                .to_string()
                .contains("4096 points")
        );
    }

    #[test]
    fn component_dependency_context_is_not_flattened_and_is_hashed() {
        let mut input = input();
        input.layers[0].components.push(ScriptRecipeComponent {
            id: "component-1".into(),
            reference: "base".into(),
            transform: [1.0, 0.0, 0.0, 1.0, 20.0, 30.0],
        });
        input.input_hash = input.computed_hash().unwrap();
        input.validate().unwrap();
        assert!(input.layers[0].contours.is_empty());

        input.layers[0].components[0].reference = "other-base".into();
        assert_ne!(input.computed_hash().unwrap(), input.input_hash);
        assert!(input.validate().is_err());
        input.layers[0].components[0].transform[4] = f64::NAN;
        assert!(input.validate().unwrap_err().to_string().contains("finite"));
    }

    #[test]
    fn version_one_capture_keeps_original_hash_and_wire_shape() {
        let input: ScriptRecipeInput = serde_json::from_str(include_str!(
            "../../tests/fixtures/recipes/fixture-move-input.json"
        ))
        .unwrap();
        input.validate().unwrap();
        assert_eq!(input.schema_version, 1);
        assert_eq!(
            input.input_hash,
            "81b724e9788b5bb4c365095867ee81766d095b00201d63c578c3b5867cb5d8d6"
        );
        let wire = serde_json::to_value(&input).unwrap();
        assert!(
            wire["layers"].as_array().unwrap().iter().all(|layer| {
                layer.get("contours").is_none() && layer.get("components").is_none()
            })
        );
        let mut forged = input;
        forged.layers[0].contours.push(ScriptRecipeContour {
            id: "forged".into(),
            closed: false,
            hyper: false,
            points: Vec::new(),
        });
        assert!(forged.validate().is_err());
    }

    #[test]
    fn legacy_result_on_version_two_capture_is_limited_to_width_and_anchor() {
        let input = input();
        let mut result = ScriptRecipeResult {
            schema_version: 1,
            job_id: input.job_id.clone(),
            input_hash: input.input_hash.clone(),
            report: "Legacy width".into(),
            reads: Vec::new(),
            edits: vec![AgentLayerEdits {
                target: input.layers[0].guard.clone(),
                operations: vec![AgentEditOperation::SetWidth { width: 620.0 }],
            }],
        };
        result.validate_against(&input).unwrap();
        result.edits[0].operations = vec![AgentEditOperation::SetAnchor {
            anchor_id: "anchor-top".into(),
            x: 310.0,
            y: 700.0,
        }];
        result.validate_against(&input).unwrap();
        result.edits[0].operations = vec![AgentEditOperation::SetPoint {
            point_id: "point-1".into(),
            x: 0.0,
            y: 0.0,
        }];
        assert!(result.validate_against(&input).is_err());
    }

    #[test]
    fn generated_recipes_validate_geometry_scope_version_and_aggregate_bounds() {
        let input = input();
        let contour = serde_json::json!({"points":[
            {"x":0,"y":0,"type":"line"},
            {"x":100,"y":0,"type":"line"},
            {"x":0,"y":100,"type":"line"}
        ]});
        let mut result: ScriptRecipeResult = serde_json::from_value(serde_json::json!({
            "schema_version":2,"job_id":input.job_id,"input_hash":input.input_hash,"report":"shape",
            "edits":[{"target":input.layers[0].guard,"operations":[
                {"op":"append_contours","contours":[contour]}
            ]}]
        }))
        .unwrap();
        result.validate_against(&input).unwrap();
        result.schema_version = 1;
        assert!(result.validate_against(&input).is_err());
        result.schema_version = 2;
        result.edits[0].target.glyph = "outside-capture".into();
        assert!(result.validate_against(&input).is_err());
        result.edits[0].target = input.layers[0].guard.clone();
        let AgentEditOperation::AppendContours { contours } = &mut result.edits[0].operations[0]
        else {
            unreachable!();
        };
        contours[0].points[0].kind = DrawingPointType::Curve;
        assert!(result.validate_against(&input).is_err());
        let valid = serde_json::from_value(contour).unwrap();
        result.edits[0].operations = vec![
            AgentEditOperation::AppendContours {
                contours: vec![valid; 129]
            };
            2
        ];
        assert!(
            result.validate_against(&input).is_err(),
            "multiple appends share one geometry budget"
        );
    }
}
