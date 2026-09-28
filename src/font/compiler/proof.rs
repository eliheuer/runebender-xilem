// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Immutable compiled-font proof inputs and results.
//!
//! The application captures [`CompileProofInput`] while it owns the canonical
//! document, then moves it to a worker for [`compile`].
//! Proof rendering never borrows a [`Project`]: `HarfRust` shaping, `Skrifa`
//! outlines and the PNG all derive from the one compiled OpenType byte buffer.
//! The live-session adapter owns snapshot handles, epochs and late-result
//! rejection around these values.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::super::project::{CanonicalDocumentEditTransaction, Project};
use super::CompiledFont;
use crate::text::buffer::TextProofSelection;
use crate::text::shape::ShapingFont;

const MAX_TEXT_BYTES: usize = 4 * 1024;
const MAX_SHAPED_GLYPHS: usize = 1024;
const MAX_FEATURES: usize = 64;
const MAX_LANGUAGE_BYTES: usize = 35;
const PROOF_WIDTH: u32 = 1024;
const PROOF_HEIGHT: u32 = 1024;
const PROOF_MARGIN: f64 = 32.0;
const PROOF_LINE_HEIGHT: f64 = 180.0;

/// Pixel dimensions and colors used to paint one compiled proof.
///
/// Coordinates in a Designbot scene are y-up. The first baseline is measured
/// from the bottom edge; subsequent wrapped lines move upward by `line_height_px`.
/// Wrapping is a bounded advance-based preview, not paragraph layout or line breaking.
/// RTL runs must fit one line; wrapping them would require cluster-aware layout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CompiledProofRendering {
    /// Raster width in pixels.
    pub width_px: u32,
    /// Raster height in pixels.
    pub height_px: u32,
    /// Rendered pixels per font em, independent of the font's units per em.
    pub pixels_per_em: f64,
    /// Inset from the left and right edges in pixels.
    pub margin_px: f64,
    /// First line baseline in pixels from the bottom edge.
    pub first_baseline_px: f64,
    /// Distance between wrapped line baselines in pixels.
    pub line_height_px: f64,
    /// RGB paper color, painted beneath the glyphs.
    pub background_rgb: [u8; 3],
    /// RGB glyph color.
    pub ink_rgb: [u8; 3],
}

impl Default for CompiledProofRendering {
    fn default() -> Self {
        Self {
            width_px: PROOF_WIDTH,
            height_px: PROOF_HEIGHT,
            pixels_per_em: 160.0,
            margin_px: PROOF_MARGIN,
            first_baseline_px: 180.0,
            line_height_px: PROOF_LINE_HEIGHT,
            background_rgb: [255; 3],
            ink_rgb: [0; 3],
        }
    }
}

impl CompiledProofRendering {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Reject settings that could overflow the scene or make the layout unusable.
    pub fn validate(&self) -> Result<(), String> {
        if !(128..=2048).contains(&self.width_px)
            || !(128..=2048).contains(&self.height_px)
            || u64::from(self.width_px) * u64::from(self.height_px) > 2_097_152
        {
            return Err(
                "proof dimensions must be 128 to 2048 pixels with at most 2097152 pixels".into(),
            );
        }
        if !self.pixels_per_em.is_finite() || !(4.0..=512.0).contains(&self.pixels_per_em) {
            return Err("proof pixels per em must be finite and between 4 and 512".into());
        }
        if !self.margin_px.is_finite()
            || self.margin_px < 0.0
            || self.margin_px > 256.0
            || self.margin_px * 2.0 >= f64::from(self.width_px)
        {
            return Err("proof margin must be finite and leave positive line width".into());
        }
        if !self.first_baseline_px.is_finite()
            || self.first_baseline_px <= 0.0
            || self.first_baseline_px >= f64::from(self.height_px)
        {
            return Err("proof first baseline must be finite and inside the image".into());
        }
        if !self.line_height_px.is_finite() || !(4.0..=1024.0).contains(&self.line_height_px) {
            return Err("proof line height must be finite and between 4 and 1024 pixels".into());
        }
        Ok(())
    }
}

/// A compiler identity for a proof snapshot.
///
/// This identifies the pinned compiler sources used to make the bytes.
/// It is not a hash of an application executable; session adapters add that
/// stronger process identity when one is available.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CompilerIdentity {
    /// Stable label for this Cargo build and its resolved `Cargo.lock`.
    pub label: String,
    /// SHA-256 digest of [`Self::label`].
    pub sha256: String,
}

impl CompilerIdentity {
    fn current() -> Self {
        let lock_sha256 =
            sha256(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.lock")).as_bytes());
        let label = format!(
            "runebender={};cargo-lock={lock_sha256}",
            env!("CARGO_PKG_VERSION"),
        );
        let sha256 = sha256(label.as_bytes());
        Self { label, sha256 }
    }
}

/// Immutable canonical compile inputs captured from one document revision.
///
/// The source font is private so callers cannot change it between capture and
/// compilation or mistake it for a second editable document model.
#[derive(Clone, Debug)]
pub struct CompileProofInput {
    document_revision: u64,
    compiler: CompilerIdentity,
    canonical_input_sha256: String,
    font: babelfont::Font,
}

impl CompileProofInput {
    /// Canonical revision from which the input was captured.
    pub fn document_revision(&self) -> u64 {
        self.document_revision
    }

    /// Compiler identity captured with the canonical input.
    pub fn compiler(&self) -> &CompilerIdentity {
        &self.compiler
    }

    /// SHA-256 of the captured canonical compiler source, including resolved features.
    pub fn canonical_input_sha256(&self) -> &str {
        &self.canonical_input_sha256
    }

    /// Derive a complete family proof input from one guarded, unpublished edit transaction.
    ///
    /// The original input remains the baseline for both branches.
    /// All sources, axes, features and compiler metadata remain captured; only the transaction's
    /// widths, anchors and contours change in this private compilation projection.
    /// Replacement topology must remain compatible with the captured sources for compilation.
    /// The application must also bind this operation to the captured document epoch.
    /// A fresh capture is used only to reject changed inputs, including external feature includes;
    /// it never replaces the retained baseline or supplies the derived proof's compiler source.
    pub fn with_staged_edit(
        &self,
        project: &Project,
        transaction: &CanonicalDocumentEditTransaction,
    ) -> Result<Self, String> {
        if project.document_revision() != self.document_revision {
            return Err("document changed after the baseline proof capture".into());
        }
        let replacements = project
            .preview_document_edit_transaction(transaction)
            .map_err(|error| error.to_string())?;
        if capture(project)?.canonical_input_sha256 != self.canonical_input_sha256 {
            return Err("compiler inputs changed after the baseline proof capture".into());
        }
        let mut derived = self.clone();
        for replacement in replacements {
            let address = replacement.address().clone();
            let key = super::super::babelfont::layer_key(&address.layer);
            let target = derived
                .font
                .glyphs
                .0
                .iter_mut()
                .find(|glyph| glyph.name.as_str() == address.glyph)
                .and_then(|glyph| glyph.get_layer_mut(&key))
                .ok_or("staged layer is absent from the captured compiler input")?;
            let (layer, _) = replacement.into_parts();
            target.width = layer.width;
            target.shapes = layer.shapes;
            target.anchors = layer.anchors;
        }
        derived.canonical_input_sha256 = sha256(
            &serde_json::to_vec(&derived.font)
                .map_err(|error| format!("could not encode derived compiler input: {error}"))?,
        );
        Ok(derived)
    }
}

/// Capture canonical compile inputs without compiling or borrowing UI state.
///
/// Move the returned value to a worker and call [`compile`].
pub fn capture(project: &Project) -> Result<CompileProofInput, String> {
    let font = captured_font(project)?;
    let canonical_input_sha256 = sha256(
        &serde_json::to_vec(&font)
            .map_err(|error| format!("could not encode captured compiler input: {error}"))?,
    );
    Ok(CompileProofInput {
        document_revision: project.document_revision(),
        compiler: CompilerIdentity::current(),
        canonical_input_sha256,
        font,
    })
}

/// An immutable compiled-font snapshot suitable for any number of proofs.
#[derive(Clone, Debug)]
pub struct CompiledProofSnapshot {
    document_revision: u64,
    compiler: CompilerIdentity,
    canonical_input_sha256: String,
    font_sha256: String,
    font: Arc<CompiledFont>,
}

impl CompiledProofSnapshot {
    /// Canonical document revision captured before this compilation began.
    pub fn document_revision(&self) -> u64 {
        self.document_revision
    }

    /// Identity of the compiler that made the OpenType bytes.
    pub fn compiler(&self) -> &CompilerIdentity {
        &self.compiler
    }

    /// SHA-256 of the canonical compiler source captured before worker compilation.
    pub fn canonical_input_sha256(&self) -> &str {
        &self.canonical_input_sha256
    }

    /// SHA-256 of the exact OpenType bytes shaped and rendered by this snapshot.
    pub fn font_sha256(&self) -> &str {
        &self.font_sha256
    }

    /// Final compiler glyph order, indexed by OpenType glyph ID.
    pub fn glyph_order(&self) -> &[String] {
        &self.font.glyph_order
    }
}

/// Compile captured inputs into an immutable byte snapshot.
///
/// A failure returns no snapshot, so a caller cannot accidentally attach a
/// previous image or hash to a failed current compilation.
pub fn compile(input: CompileProofInput) -> Result<CompiledProofSnapshot, String> {
    let font = Arc::new(CompiledFont::build(input.font)?);
    let font_sha256 = sha256(&font.bytes);
    Ok(CompiledProofSnapshot {
        document_revision: input.document_revision,
        compiler: input.compiler,
        canonical_input_sha256: input.canonical_input_sha256,
        font_sha256,
        font,
    })
}

/// A bounded shaping and rendering recipe for one compiled proof.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledProofRecipe {
    /// UTF-8 text to shape.
    pub text: String,
    /// Normalized variation coordinates in the compiled font's axis order.
    pub normalized_location: Vec<f64>,
    /// Shape and place glyphs right-to-left when true.
    pub right_to_left: bool,
    /// Explicit OpenType feature overrides over the complete text run.
    pub features: Vec<(String, bool)>,
    /// Optional ISO 15924 script override such as `arab`.
    pub script: Option<String>,
    /// Optional BCP 47 language override such as `ar`.
    pub language: Option<String>,
    /// Bounded raster scale, wrapping geometry and proof colors.
    /// Absent settings retain the original 1024-pixel, 160-pixel-per-em scene.
    #[serde(default, skip_serializing_if = "CompiledProofRendering::is_default")]
    pub rendering: CompiledProofRendering,
    /// Selected glyph form and occurrence captured from editor text, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<CompiledProofTarget>,
}

/// A selected shaped occurrence that must survive compilation of both proof branches.
///
/// Name, cluster and paint-order occurrence identify a form without reusing a snapshot-specific
/// numeric glyph ID. A changed substitution fails explicitly instead of selecting another glyph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledProofTarget {
    /// Compiler glyph name of the selected contextual form.
    pub glyph_name: String,
    /// UTF-8 byte offset of the selected source cluster.
    pub cluster: u32,
    /// Paint-order occurrence among glyphs with this name.
    pub occurrence: u32,
    /// Frozen pen position from the selected text run, used to align detail views across edits.
    /// Older recipes omit it and continue to render context proofs only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_pen_x: Option<f64>,
}

impl CompiledProofRecipe {
    /// Preserve a widget-owned text selection as an exact proof recipe.
    pub fn from_text_selection(selection: TextProofSelection) -> Result<Self, String> {
        let recipe = Self {
            text: selection.text,
            normalized_location: selection.normalized_location,
            right_to_left: selection.right_to_left,
            features: selection.features,
            script: selection.script,
            language: selection.language,
            rendering: CompiledProofRendering {
                pixels_per_em: 32.0,
                ..CompiledProofRendering::default()
            },
            target: Some(CompiledProofTarget {
                glyph_name: selection.glyph_name,
                cluster: selection.cluster,
                occurrence: selection.occurrence,
                reference_pen_x: Some(selection.reference_pen_x),
            }),
        };
        recipe.validate()?;
        Ok(recipe)
    }

    /// Validate bounded, finite inputs before expensive outline extraction.
    pub fn validate(&self) -> Result<(), String> {
        self.rendering.validate()?;
        if self.text.is_empty() || self.text.len() > MAX_TEXT_BYTES {
            return Err(format!(
                "proof text must contain 1 to {MAX_TEXT_BYTES} UTF-8 bytes"
            ));
        }
        if !self
            .normalized_location
            .iter()
            .all(|value| value.is_finite() && (-1.0..=1.0).contains(value))
        {
            return Err(
                "proof location must contain finite normalized values from -1 through 1".into(),
            );
        }
        if self.features.len() > MAX_FEATURES {
            return Err(format!(
                "proof accepts at most {MAX_FEATURES} feature overrides"
            ));
        }
        if self
            .features
            .iter()
            .any(|(tag, _)| tag.len() != 4 || !tag.is_ascii())
        {
            return Err("proof feature tags must be four ASCII bytes".into());
        }
        if self
            .script
            .as_deref()
            .is_some_and(|script| script.len() != 4 || !script.is_ascii())
        {
            return Err("proof script must be a four-byte ISO 15924 tag".into());
        }
        if self.language.as_deref().is_some_and(|language| {
            language.is_empty()
                || language.len() > MAX_LANGUAGE_BYTES
                || !language
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        }) {
            return Err(format!(
                "proof language must be 1 to {MAX_LANGUAGE_BYTES} ASCII BCP 47 bytes"
            ));
        }
        if self.target.as_ref().is_some_and(|target| {
            let cluster = usize::try_from(target.cluster).ok();
            let occurrence = usize::try_from(target.occurrence).ok();
            target.glyph_name.is_empty()
                || target.glyph_name.len() > 255
                || cluster.is_none_or(|cluster| {
                    cluster >= self.text.len() || !self.text.is_char_boundary(cluster)
                })
                || occurrence.is_none_or(|occurrence| occurrence >= MAX_SHAPED_GLYPHS)
                || target
                    .reference_pen_x
                    .is_some_and(|pen| !pen.is_finite() || pen.abs() > 1.0e9)
        }) {
            return Err("proof target has an invalid glyph name, cluster or occurrence".into());
        }
        Ok(())
    }
}

/// One shaped glyph in a compiled proof.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CompiledProofGlyph {
    /// OpenType glyph ID in the compiled snapshot.
    pub glyph_id: u16,
    /// Compiler glyph name resolved from the snapshot glyph order by glyph ID.
    pub glyph_name: Option<String>,
    /// UTF-8 byte offset of the source cluster.
    pub cluster: u32,
    /// Positioned horizontal advance in font units.
    pub x_advance: f64,
    /// Positioned horizontal offset in font units.
    pub x_offset: f64,
    /// Positioned vertical offset in font units.
    pub y_offset: f64,
}

/// Complete compiled proof data, including PNG bytes.
#[derive(Clone, Debug)]
pub struct CompiledProof {
    /// Identity of the immutable source bytes.
    pub font_sha256: String,
    /// Document revision captured before the compiler began.
    pub document_revision: u64,
    /// Compiler identity of the byte snapshot.
    pub compiler: CompilerIdentity,
    /// SHA-256 of the canonical compiler source captured before compilation.
    pub canonical_input_sha256: String,
    /// Exact recipe used for shaping and painting.
    pub recipe: CompiledProofRecipe,
    /// SHA-256 of the exact serialized recipe shared by context and detail.
    pub recipe_sha256: String,
    /// Measured identity of the Designbot executable used for both rasters.
    pub renderer: crate::formats::designbot::RendererIdentity,
    /// Positioned glyphs from `HarfRust`, in paint order.
    pub glyphs: Vec<CompiledProofGlyph>,
    /// Paint-order selected occurrence in the full shaped run, if any.
    pub target_glyph_index: Option<usize>,
    /// Bounded PNG rasterized from Skrifa outlines of those glyphs.
    pub png: Vec<u8>,
    /// Enlarged crop around the selected occurrence, when the recipe has a target.
    pub detail: Option<CompiledProofDetail>,
}

/// Enlarged selected-target raster from the same shaped glyph sequence.
#[derive(Clone, Debug)]
pub struct CompiledProofDetail {
    /// Bounded raster settings of the detail view.
    pub rendering: CompiledProofRendering,
    /// Paint-order index in the full shaped proof, never a re-shaped occurrence.
    pub target_glyph_index: usize,
    /// Fixed translation and scale derived from the captured text pen, not candidate ink bounds.
    pub crop: ProofDetailCrop,
    /// Detail PNG bytes.
    pub png: Vec<u8>,
}

/// Geometry needed to compare context-preserving enlarged views.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProofDetailCrop {
    /// Translation of the run's font-unit origin into detail pixels.
    pub origin_px: [f64; 2],
    /// Pixel scale per font unit.
    pub font_units_to_px: f64,
    /// Immutable selected pen from the editor's original shaped run.
    pub reference_pen_x: f64,
}

/// Retained view of one immutable compiled proof.
#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProofView {
    /// Full shaped text at the recipe's context scale.
    #[default]
    Context,
    /// Enlarged crop around the exact selected occurrence.
    Detail,
}

impl ProofView {
    /// Whether this selector retains the legacy full-context request encoding.
    pub fn is_context(&self) -> bool {
        *self == Self::Context
    }
}

/// Borrowed raster and placement metadata from one retained proof view.
#[derive(Debug)]
pub struct CompiledProofImage<'a> {
    /// Already-rendered PNG bytes.
    pub bytes: &'a [u8],
    /// Raster size, scale, colors and layout settings.
    pub rendering: &'a CompiledProofRendering,
    /// Paint-order index of the selected glyph in the complete shaped run.
    pub target_glyph_index: Option<usize>,
    /// Fixed selected-pen crop transform, present only for detail.
    pub crop: Option<&'a ProofDetailCrop>,
}

impl CompiledProof {
    /// Select retained image bytes and rendering settings without rerendering.
    pub fn image(&self, view: ProofView) -> Result<CompiledProofImage<'_>, String> {
        match view {
            ProofView::Context => Ok(CompiledProofImage {
                bytes: &self.png,
                rendering: &self.recipe.rendering,
                target_glyph_index: self.target_glyph_index,
                crop: None,
            }),
            ProofView::Detail => {
                let detail = self
                    .detail
                    .as_ref()
                    .ok_or("proof has no selected-target detail")?;
                Ok(CompiledProofImage {
                    bytes: &detail.png,
                    rendering: &detail.rendering,
                    target_glyph_index: Some(detail.target_glyph_index),
                    crop: Some(&detail.crop),
                })
            }
        }
    }
}

/// Shape and render a bounded PNG using only one immutable compiled snapshot.
///
/// This operation is intentionally independent of [`Project`].
/// Call it off the application thread; the local Designbot renderer receives
/// only a data-only scene containing outlines already extracted from the bytes.
pub fn prove(
    snapshot: &CompiledProofSnapshot,
    recipe: CompiledProofRecipe,
) -> Result<CompiledProof, String> {
    recipe.validate()?;
    if recipe.normalized_location.len() != snapshot.font.axis_tags.len() {
        return Err(format!(
            "proof location has {} coordinates, but compiled font has {} axes",
            recipe.normalized_location.len(),
            snapshot.font.axis_tags.len()
        ));
    }
    let shaper = ShapingFont::from_bytes((*snapshot.font.bytes).clone())
        .map(|font| font.at_normalized(recipe.normalized_location.clone()))?;
    let shaped = shaper.shape_with_options(
        &recipe.text,
        recipe.right_to_left,
        &recipe.features,
        recipe.script.as_deref(),
        recipe.language.as_deref(),
    )?;
    if shaped.len() > MAX_SHAPED_GLYPHS {
        return Err(format!("proof shapes at most {MAX_SHAPED_GLYPHS} glyphs"));
    }
    let glyphs = shaped
        .iter()
        .map(|glyph| CompiledProofGlyph {
            glyph_id: glyph.glyph_id,
            glyph_name: snapshot
                .font
                .glyph_order
                .get(usize::from(glyph.glyph_id))
                .cloned(),
            cluster: glyph.cluster,
            x_advance: glyph.x_advance,
            x_offset: glyph.x_offset,
            y_offset: glyph.y_offset,
        })
        .collect::<Vec<_>>();
    let mut target_glyph_index = None;
    if let Some(target) = &recipe.target {
        let mut occurrence = 0_u32;
        for (index, glyph) in glyphs.iter().enumerate() {
            if glyph.glyph_name.as_deref() == Some(target.glyph_name.as_str()) {
                if occurrence == target.occurrence {
                    if glyph.cluster == target.cluster {
                        target_glyph_index = Some(index);
                    }
                    break;
                }
                occurrence += 1;
            }
        }
        if target_glyph_index.is_none() {
            return Err("selected shaped occurrence changed in the compiled proof".into());
        }
    }
    let outlines = snapshot
        .font
        .outlines(&recipe.normalized_location)?
        .into_iter()
        .map(|(_, outline)| outline)
        .collect::<Vec<_>>();
    let units_per_em = units_per_em(&snapshot.font.bytes)?;
    let scene = scene(
        &glyphs,
        &outlines,
        units_per_em,
        recipe.right_to_left,
        &recipe.rendering,
    )?;
    let rendered = crate::formats::designbot::render_with_identity(&scene, false)?;
    let png = rendered.bytes;
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("proof renderer did not return a PNG".into());
    }
    let detail = target_glyph_index
        .zip(
            recipe
                .target
                .as_ref()
                .and_then(|target| target.reference_pen_x),
        )
        .map(
            |(index, reference_pen_x)| -> Result<CompiledProofDetail, String> {
                let rendering = CompiledProofRendering {
                    width_px: 512,
                    height_px: 512,
                    pixels_per_em: 320.0,
                    margin_px: 16.0,
                    first_baseline_px: 256.0,
                    line_height_px: 320.0,
                    background_rgb: recipe.rendering.background_rgb,
                    ink_rgb: recipe.rendering.ink_rgb,
                };
                let (detail_scene, crop) = detail_scene(
                    &glyphs,
                    &outlines,
                    units_per_em,
                    index,
                    reference_pen_x,
                    &rendering,
                )?;
                let detail = crate::formats::designbot::render_with_identity(&detail_scene, false)?;
                if detail.renderer != rendered.renderer {
                    return Err("Designbot executable changed between proof views".into());
                }
                if !detail.bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                    return Err("detail renderer did not return a PNG".into());
                }
                Ok(CompiledProofDetail {
                    rendering,
                    target_glyph_index: index,
                    crop,
                    png: detail.bytes,
                })
            },
        )
        .transpose()?;
    let recipe_sha256 = sha256(
        &serde_json::to_vec(&recipe)
            .map_err(|error| format!("could not encode recipe: {error}"))?,
    );
    Ok(CompiledProof {
        font_sha256: snapshot.font_sha256.clone(),
        document_revision: snapshot.document_revision,
        compiler: snapshot.compiler.clone(),
        canonical_input_sha256: snapshot.canonical_input_sha256.clone(),
        recipe,
        recipe_sha256,
        renderer: rendered.renderer,
        glyphs,
        target_glyph_index,
        png,
        detail,
    })
}

fn captured_font(project: &Project) -> Result<babelfont::Font, String> {
    use babelfont::filters::{FontFilter as _, ResolveIncludes};

    let mut font = project.babelfont_snapshot()?;
    let base = font
        .source
        .as_ref()
        .and_then(|source| source.parent())
        .map(PathBuf::from);
    ResolveIncludes::new(base)
        .apply(&mut font)
        .map_err(|error| format!("could not capture feature includes: {error}"))?;
    if has_unresolved_feature_include(&font.features.to_fea()) {
        return Err(
            "could not capture feature includes: unresolved include directive remains after resolution"
                .into(),
        );
    }
    // No compiler work after this capture may read a live feature source or include path.
    font.source = None;
    font.features.include_paths.clear();
    Ok(font)
}

fn has_unresolved_feature_include(features: &str) -> bool {
    let mut code = String::with_capacity(features.len());
    let mut comment = false;
    let mut quoted = false;
    let mut escaped = false;
    for character in features.chars() {
        if comment {
            if character == '\n' {
                comment = false;
                code.push('\n');
            }
        } else if quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
            code.push(' ');
        } else if character == '#' {
            comment = true;
        } else if character == '"' {
            quoted = true;
            code.push(' ');
        } else {
            code.push(character);
        }
    }
    let mut remaining = code.as_str();
    while let Some(index) = remaining.find("include") {
        let (prefix, after_prefix) = remaining.split_at(index);
        let suffix = &after_prefix["include".len()..];
        let starts_identifier = prefix
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
        if !starts_identifier && suffix.trim_start().starts_with('(') {
            return true;
        }
        remaining = suffix;
    }
    false
}

fn scene(
    glyphs: &[CompiledProofGlyph],
    outlines: &[Arc<kurbo::BezPath>],
    units_per_em: f64,
    right_to_left: bool,
    rendering: &CompiledProofRendering,
) -> Result<serde_json::Value, String> {
    use kurbo::{Affine, Shape as _};
    use serde_json::json;

    if !units_per_em.is_finite() || units_per_em <= 0.0 {
        return Err("compiled font has an invalid units-per-em value".into());
    }
    rendering.validate()?;
    let scale = rendering.pixels_per_em / units_per_em;
    if !scale.is_finite() {
        return Err("proof scale is non-finite".into());
    }
    let mut paths = Vec::with_capacity(glyphs.len());
    if rendering.background_rgb != [255; 3] {
        paths.push(json!({
            "d": format!("M0 0H{}V{}H0Z", rendering.width_px, rendering.height_px),
            "color": rendering.background_rgb,
        }));
    }
    // HarfRust returns RTL glyphs in visual order with positive advances. Start their pen at
    // the left edge of the right-anchored run, then move forward through that exact order.
    // A zero-advance mark can precede its base and still use its original GPOS offset.
    let mut x = rendering.margin_px;
    if right_to_left {
        let total_advance = glyphs.iter().try_fold(0.0, |total, glyph| {
            if !glyph.x_advance.is_finite() {
                return Err("compiled glyph positioning contains a non-finite value".to_owned());
            }
            let next = total + glyph.x_advance * scale;
            if !next.is_finite() {
                return Err("RTL proof has a non-finite total advance".to_owned());
            }
            Ok(next)
        })?;
        if total_advance < 0.0
            || total_advance > f64::from(rendering.width_px) - 2.0 * rendering.margin_px
        {
            return Err("RTL proof exceeds the bounded line width; wrapping is unsupported".into());
        }
        x = f64::from(rendering.width_px) - rendering.margin_px - total_advance;
    }
    let mut baseline = rendering.first_baseline_px;
    for glyph in glyphs {
        let path = outlines
            .get(usize::from(glyph.glyph_id))
            .ok_or_else(|| format!("compiled outline missing for glyph {}", glyph.glyph_id))?;
        if !glyph.x_advance.is_finite()
            || !glyph.x_offset.is_finite()
            || !glyph.y_offset.is_finite()
        {
            return Err("compiled glyph positioning contains a non-finite value".into());
        }
        let advance = if right_to_left {
            glyph.x_advance * scale
        } else {
            glyph.x_advance.max(0.0) * scale
        };
        if !right_to_left && advance > f64::from(rendering.width_px) - 2.0 * rendering.margin_px {
            return Err("one shaped glyph exceeds the bounded proof width".into());
        }
        if !right_to_left && x + advance > f64::from(rendering.width_px) - rendering.margin_px {
            x = rendering.margin_px;
            baseline += rendering.line_height_px;
        }
        if baseline + rendering.line_height_px > f64::from(rendering.height_px) {
            return Err("proof exceeds the bounded image height".into());
        }
        let translated = Affine::translate((
            x + glyph.x_offset * scale,
            baseline + glyph.y_offset * scale,
        )) * Affine::scale(scale)
            * path.as_ref();
        let bounds = translated.bounding_box();
        if !translated.is_empty()
            && (bounds.x0 < 0.0
                || bounds.y0 < 0.0
                || bounds.x1 > f64::from(rendering.width_px)
                || bounds.y1 > f64::from(rendering.height_px))
        {
            return Err("proof outline would be clipped by the bounded image".into());
        }
        if rendering.ink_rgb == [0; 3] {
            paths.push(json!({"d": translated.to_svg()}));
        } else {
            paths.push(json!({"d": translated.to_svg(), "color": rendering.ink_rgb}));
        }
        x += advance;
    }
    Ok(json!({
        "version": 1,
        "width": rendering.width_px,
        "height": rendering.height_px,
        "paths": paths,
        "labels": [],
    }))
}

/// Crop the unchanged shaped run around a frozen selected pen at a larger scale.
/// Neighboring glyphs may cross the crop edge; selected ink, when present, must fit completely.
fn detail_scene(
    glyphs: &[CompiledProofGlyph],
    outlines: &[Arc<kurbo::BezPath>],
    units_per_em: f64,
    target: usize,
    reference_pen_x: f64,
    rendering: &CompiledProofRendering,
) -> Result<(serde_json::Value, ProofDetailCrop), String> {
    use kurbo::{Affine, Shape as _};
    use serde_json::json;

    rendering.validate()?;
    if !units_per_em.is_finite() || units_per_em <= 0.0 {
        return Err("compiled font has an invalid units-per-em value".into());
    }
    let scale = rendering.pixels_per_em / units_per_em;
    if !scale.is_finite() {
        return Err("proof scale is non-finite".into());
    }
    let mut pen = 0.0;
    let mut positions = Vec::with_capacity(glyphs.len());
    for glyph in glyphs {
        if !glyph.x_advance.is_finite()
            || !glyph.x_offset.is_finite()
            || !glyph.y_offset.is_finite()
        {
            return Err("compiled glyph positioning contains a non-finite value".into());
        }
        positions.push(pen);
        pen += glyph.x_advance;
        if !pen.is_finite() {
            return Err("detail proof has a non-finite total advance".into());
        }
    }
    let selected = glyphs
        .get(target)
        .ok_or("selected proof occurrence is unavailable")?;
    outlines
        .get(usize::from(selected.glyph_id))
        .ok_or("compiled target outline is missing")?;
    if !reference_pen_x.is_finite() || reference_pen_x.abs() > 1.0e9 {
        return Err("detail proof reference pen is invalid".into());
    }
    let origin_x = f64::from(rendering.width_px) / 2.0 - reference_pen_x * scale;
    let origin_y = rendering.first_baseline_px;
    let crop = ProofDetailCrop {
        origin_px: [origin_x, origin_y],
        font_units_to_px: scale,
        reference_pen_x,
    };
    let mut paths = Vec::new();
    if rendering.background_rgb != [255; 3] {
        paths.push(json!({
            "d": format!("M0 0H{}V{}H0Z", rendering.width_px, rendering.height_px),
            "color": rendering.background_rgb,
        }));
    }
    for (index, glyph) in glyphs.iter().enumerate() {
        let path = outlines
            .get(usize::from(glyph.glyph_id))
            .ok_or("compiled detail outline is missing")?;
        if path.is_empty() {
            continue;
        }
        let translated = Affine::translate((
            origin_x + (positions[index] + glyph.x_offset) * scale,
            origin_y + glyph.y_offset * scale,
        )) * Affine::scale(scale)
            * path.as_ref();
        let bounds = translated.bounding_box();
        if !bounds.x0.is_finite()
            || !bounds.y0.is_finite()
            || !bounds.x1.is_finite()
            || !bounds.y1.is_finite()
        {
            return Err("detail proof has non-finite outline bounds".into());
        }
        if index == target
            && (bounds.x0 < rendering.margin_px
                || bounds.x1 > f64::from(rendering.width_px) - rendering.margin_px
                || bounds.y0 < rendering.margin_px
                || bounds.y1 > f64::from(rendering.height_px) - rendering.margin_px)
        {
            return Err("selected proof occurrence exceeds enlarged detail bounds".into());
        }
        if bounds.x1 > 0.0
            && bounds.y1 > 0.0
            && bounds.x0 < f64::from(rendering.width_px)
            && bounds.y0 < f64::from(rendering.height_px)
        {
            if rendering.ink_rgb == [0; 3] {
                paths.push(json!({"d": translated.to_svg()}));
            } else {
                paths.push(json!({"d": translated.to_svg(), "color": rendering.ink_rgb}));
            }
        }
    }
    Ok((
        json!({
            "version": 1,
            "width": rendering.width_px,
            "height": rendering.height_px,
            "paths": paths,
            "labels": [],
        }),
        crop,
    ))
}

fn units_per_em(bytes: &[u8]) -> Result<f64, String> {
    use skrifa::raw::TableProvider as _;
    let font = skrifa::FontRef::new(bytes).map_err(|error| error.to_string())?;
    Ok(f64::from(
        font.head()
            .map_err(|error| error.to_string())?
            .units_per_em(),
    ))
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn project() -> Project {
        Project::load(&crate::testing::fonts::designspace()).expect("fixture designspace loads")
    }

    fn recipe(
        text: &str,
        normalized_location: f64,
        right_to_left: bool,
        script: Option<&str>,
    ) -> CompiledProofRecipe {
        CompiledProofRecipe {
            text: text.into(),
            normalized_location: vec![normalized_location],
            right_to_left,
            features: Vec::new(),
            script: script.map(str::to_owned),
            language: None,
            rendering: CompiledProofRendering::default(),
            target: None,
        }
    }

    #[test]
    fn captured_compile_input_can_move_to_a_worker() {
        fn assert_send<T: Send>() {}
        assert_send::<CompileProofInput>();
        assert_send::<CompiledProofSnapshot>();
    }

    #[test]
    fn recipe_rejects_unbounded_location_features_and_language() {
        let mut recipe = recipe("A", 1.1, false, Some("latn"));
        assert!(recipe.validate().is_err());

        recipe.normalized_location = vec![0.0];
        recipe.features = vec![("kern".into(), true); MAX_FEATURES + 1];
        assert!(recipe.validate().is_err());

        recipe.features.clear();
        recipe.language = Some("x".repeat(MAX_LANGUAGE_BYTES + 1));
        assert!(recipe.validate().is_err());
    }

    #[test]
    fn old_recipe_json_keeps_the_original_rendering_and_serialization() {
        let old = serde_json::json!({
            "text": "A",
            "normalized_location": [0.0],
            "right_to_left": false,
            "features": [],
            "script": "latn",
            "language": null,
        });
        let recipe: CompiledProofRecipe = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(recipe.rendering, CompiledProofRendering::default());
        assert_eq!(serde_json::to_value(recipe).unwrap(), old);
    }

    #[test]
    fn rendering_rejects_nonfinite_oversized_and_unusable_layout() {
        let mut recipe = recipe("A", 0.0, false, None);
        recipe.rendering.pixels_per_em = f64::NAN;
        assert!(recipe.validate().is_err());
        recipe.rendering.pixels_per_em = 513.0;
        assert!(recipe.validate().is_err());
        recipe.rendering.pixels_per_em = 16.0;
        recipe.rendering.width_px = 2049;
        assert!(recipe.validate().is_err());
        recipe.rendering.width_px = 2048;
        recipe.rendering.height_px = 2048;
        assert!(recipe.validate().is_err());
        recipe.rendering.width_px = 128;
        recipe.rendering.height_px = 512;
        recipe.rendering.margin_px = 64.0;
        assert!(recipe.validate().is_err());
        recipe.rendering.margin_px = 8.0;
        recipe.rendering.line_height_px = f64::INFINITY;
        assert!(recipe.validate().is_err());
    }

    #[test]
    fn rendering_scale_and_layout_change_path_placement() {
        use kurbo::{BezPath, Rect, Shape as _};

        let glyph = CompiledProofGlyph {
            glyph_id: 0,
            glyph_name: Some("A".into()),
            cluster: 0,
            x_advance: 500.0,
            x_offset: 0.0,
            y_offset: 0.0,
        };
        let outlines = [Arc::new(Rect::new(0.0, 0.0, 100.0, 100.0).to_path(0.1))];
        let mut rendering = CompiledProofRendering {
            width_px: 128,
            height_px: 512,
            pixels_per_em: 100.0,
            margin_px: 8.0,
            first_baseline_px: 100.0,
            line_height_px: 120.0,
            background_rgb: [240, 240, 240],
            ink_rgb: [20, 20, 20],
        };
        let fit = scene(
            &[glyph.clone(), glyph.clone()],
            &outlines,
            1000.0,
            false,
            &rendering,
        )
        .unwrap();
        assert_eq!(fit["width"], 128);
        assert_eq!(fit["height"], 512);
        assert_eq!(fit["paths"][0]["color"], serde_json::json!([240, 240, 240]));
        assert_eq!(fit["paths"][1]["color"], serde_json::json!([20, 20, 20]));
        let fit_second = BezPath::from_svg(fit["paths"][2]["d"].as_str().unwrap())
            .unwrap()
            .bounding_box();
        assert_eq!(fit_second.x0, 58.0);
        assert_eq!(fit_second.y0, 100.0);

        rendering.pixels_per_em = 200.0;
        let wrapped = scene(
            &[glyph.clone(), glyph],
            &outlines,
            1000.0,
            false,
            &rendering,
        )
        .unwrap();
        let wrapped_second = BezPath::from_svg(wrapped["paths"][2]["d"].as_str().unwrap())
            .unwrap()
            .bounding_box();
        assert_eq!(wrapped_second.x0, 8.0);
        assert_eq!(wrapped_second.y0, 220.0);
        assert_eq!(wrapped_second.width(), 20.0);
    }

    #[test]
    fn rtl_scene_keeps_harfrust_order_and_mark_offset_on_one_right_anchored_run() {
        use kurbo::{BezPath, Rect, Shape as _};

        let rendering = CompiledProofRendering {
            width_px: 128,
            height_px: 512,
            pixels_per_em: 100.0,
            margin_px: 8.0,
            first_baseline_px: 100.0,
            line_height_px: 120.0,
            ..CompiledProofRendering::default()
        };
        let outlines = [
            Arc::new(Rect::new(0.0, 0.0, 100.0, 100.0).to_path(0.1)),
            Arc::new(Rect::new(0.0, 0.0, 20.0, 20.0).to_path(0.1)),
        ];
        let mark = CompiledProofGlyph {
            glyph_id: 1,
            glyph_name: Some("kasra".into()),
            cluster: 2,
            x_advance: 0.0,
            x_offset: 40.0,
            y_offset: 60.0,
        };
        let base = CompiledProofGlyph {
            glyph_id: 0,
            glyph_name: Some("beh".into()),
            cluster: 0,
            x_advance: 500.0,
            x_offset: 0.0,
            y_offset: 0.0,
        };
        let image = scene(&[mark, base.clone()], &outlines, 1000.0, true, &rendering).unwrap();
        assert_eq!(image["paths"].as_array().unwrap().len(), 2);
        let mark_bounds = BezPath::from_svg(image["paths"][0]["d"].as_str().unwrap())
            .unwrap()
            .bounding_box();
        let base_bounds = BezPath::from_svg(image["paths"][1]["d"].as_str().unwrap())
            .unwrap()
            .bounding_box();
        assert_eq!((mark_bounds.x0, mark_bounds.y0), (74.0, 106.0));
        assert_eq!((base_bounds.x0, base_bounds.y0), (70.0, 100.0));

        let mut too_wide = base.clone();
        too_wide.x_advance = 2000.0;
        assert!(
            scene(&[too_wide], &outlines, 1000.0, true, &rendering)
                .unwrap_err()
                .contains("wrapping is unsupported")
        );
        let mut huge = base;
        huge.x_advance = f64::MAX;
        assert!(
            scene(&vec![huge; 11], &outlines, 1000.0, true, &rendering)
                .unwrap_err()
                .contains("non-finite total advance")
        );
    }

    #[test]
    fn detail_crop_uses_frozen_pen_and_rejects_selected_ink_clipping() {
        use kurbo::{BezPath, Rect, Shape as _};

        let rendering = CompiledProofRendering {
            width_px: 512,
            height_px: 512,
            pixels_per_em: 320.0,
            margin_px: 16.0,
            first_baseline_px: 256.0,
            ..CompiledProofRendering::default()
        };
        let glyphs = [
            CompiledProofGlyph {
                glyph_id: 0,
                glyph_name: Some("preceding".into()),
                cluster: 0,
                x_advance: 500.0,
                x_offset: 0.0,
                y_offset: 0.0,
            },
            CompiledProofGlyph {
                glyph_id: 1,
                glyph_name: Some("selected".into()),
                cluster: 1,
                x_advance: 500.0,
                x_offset: 0.0,
                y_offset: 0.0,
            },
        ];
        let preceding = Arc::new(Rect::new(0.0, 0.0, 100.0, 100.0).to_path(0.1));
        let original = Arc::new(Rect::new(0.0, 0.0, 100.0, 100.0).to_path(0.1));
        let changed = Arc::new(Rect::new(100.0, 0.0, 200.0, 100.0).to_path(0.1));
        let (before, before_crop) = detail_scene(
            &glyphs,
            &[preceding.clone(), original],
            1000.0,
            1,
            500.0,
            &rendering,
        )
        .unwrap();
        let (after, after_crop) = detail_scene(
            &glyphs,
            &[preceding.clone(), changed],
            1000.0,
            1,
            500.0,
            &rendering,
        )
        .unwrap();
        assert_eq!(before_crop, after_crop);
        assert_eq!(before_crop.origin_px, [96.0, 256.0]);
        assert_eq!(before_crop.font_units_to_px, 0.32);
        let before_ink = BezPath::from_svg(before["paths"][1]["d"].as_str().unwrap())
            .unwrap()
            .bounding_box();
        let after_ink = BezPath::from_svg(after["paths"][1]["d"].as_str().unwrap())
            .unwrap()
            .bounding_box();
        assert_eq!(before_ink.x0, 256.0);
        assert_eq!(after_ink.x0, 288.0);

        let empty = Arc::new(BezPath::new());
        let (blank, blank_crop) = detail_scene(
            &glyphs,
            &[preceding.clone(), empty],
            1000.0,
            1,
            500.0,
            &rendering,
        )
        .unwrap();
        assert_eq!(blank_crop, before_crop);
        assert_eq!(blank["paths"].as_array().unwrap().len(), 1);

        let oversized = Arc::new(Rect::new(0.0, 0.0, 1000.0, 100.0).to_path(0.1));
        assert!(
            detail_scene(
                &glyphs,
                &[preceding, oversized],
                1000.0,
                1,
                500.0,
                &rendering
            )
            .unwrap_err()
            .contains("selected proof occurrence exceeds")
        );
    }

    #[test]
    fn scene_rejects_missing_or_clipped_outlines() {
        use kurbo::{Rect, Shape as _};

        let glyph = CompiledProofGlyph {
            glyph_id: 0,
            glyph_name: Some("A".into()),
            cluster: 0,
            x_advance: 500.0,
            x_offset: 0.0,
            y_offset: 0.0,
        };
        let rendering = CompiledProofRendering::default();
        assert!(scene(std::slice::from_ref(&glyph), &[], 1000.0, false, &rendering).is_err());

        let too_wide = Arc::new(Rect::new(0.0, 0.0, 10_000.0, 1.0).to_path(0.1));
        assert!(scene(&[glyph], &[too_wide], 1000.0, false, &rendering).is_err());
    }

    #[test]
    fn capture_freezes_feature_include_content_before_worker_compilation() {
        use super::super::super::project::{DocumentEditOperation, DocumentLayerEdit};
        use super::super::super::variable::GlyphLayerAddress;

        let root = std::env::temp_dir().join(format!(
            "runebender-compiled-proof-features-{}",
            std::process::id()
        ));
        let ufo = root.join("Fixture.ufo");
        fs::create_dir_all(ufo.join("includes")).unwrap();
        let include = ufo.join("includes/captured.fea");
        fs::write(&include, "# captured include\n").unwrap();
        let mut font = norad::Font::new();
        font.features = "include(includes/captured.fea);\n".into();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("A"));
        let project = Project::from_source(super::super::super::project::SourceInput::from_font(
            font, ufo,
        ));

        let input = capture(&project).unwrap();
        let source = project.document_sources().next().unwrap();
        let address = GlyphLayerAddress {
            glyph: "A".into(),
            layer: source.default_layer(),
        };
        let transaction = project
            .begin_document_edit_transaction(
                source.id(),
                "Preview",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    project.capture_document_layer(&address).unwrap(),
                    vec![DocumentEditOperation::SetWidth(100.0)],
                )],
            )
            .unwrap();
        fs::write(&include, "# changed after capture\n").unwrap();
        assert_eq!(project.document_revision(), input.document_revision());
        assert!(
            input
                .with_staged_edit(&project, &transaction)
                .unwrap_err()
                .contains("compiler inputs changed")
        );

        let captured = input.font.features.to_fea();
        assert!(captured.contains("captured include"));
        assert!(!captured.contains("changed after capture"));
        assert!(!captured.contains("include("));
        assert!(input.font.source.is_none());
        assert!(input.font.features.include_paths.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn capture_rejects_whitespace_feature_include_that_resolver_cannot_freeze() {
        assert!(!has_unresolved_feature_include(
            "# include (commented.fea);"
        ));
        assert!(!has_unresolved_feature_include(
            "myinclude (identifier.fea);"
        ));
        assert!(has_unresolved_feature_include("include (live.fea);"));
        assert!(has_unresolved_feature_include(
            "include\n# comment\n(live.fea);"
        ));
        assert!(has_unresolved_feature_include(
            "nameid 1 \"Hash#name\"; include\n(live.fea);"
        ));
        assert!(!has_unresolved_feature_include(
            "nameid 1 \"include (only a string)\";"
        ));

        let root = std::env::temp_dir().join(format!(
            "runebender-compiled-proof-whitespace-features-{}",
            std::process::id()
        ));
        let ufo = root.join("Test.ufo");
        let include = ufo.join("includes/captured.fea");
        fs::create_dir_all(include.parent().unwrap()).unwrap();
        fs::write(&include, "# captured include\n").unwrap();
        let mut font = norad::Font::new();
        font.features = "include (includes/captured.fea);\n".into();
        let project = Project::from_source(super::super::super::project::SourceInput::from_font(
            font, ufo,
        ));

        let error = capture(&project).unwrap_err();
        assert!(error.contains("unresolved include directive"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compiled_proof_uses_one_immutable_byte_snapshot_for_multilingual_text() {
        let project = project();
        let snapshot = compile(capture(&project).unwrap()).unwrap();
        let latin = prove(
            &snapshot,
            recipe("AVATAR office", -1.0, false, Some("latn")),
        )
        .unwrap();
        let hebrew = prove(&snapshot, recipe("שָׁלוֹם", 0.0, true, Some("hebr"))).unwrap();
        let arabic = prove(&snapshot, recipe("سلام", 0.0, true, Some("arab"))).unwrap();
        let unencoded = prove(&snapshot, recipe("\u{10ffff}", 0.0, false, Some("latn"))).unwrap();
        assert!(unencoded.glyphs.iter().all(|glyph| glyph.glyph_id == 0));
        for proof in [&latin, &hebrew, &arabic, &unencoded] {
            assert_eq!(proof.font_sha256, snapshot.font_sha256());
            assert_eq!(proof.document_revision, snapshot.document_revision());
            assert_eq!(
                proof.canonical_input_sha256,
                snapshot.canonical_input_sha256()
            );
            assert!(proof.png.starts_with(b"\x89PNG\r\n\x1a\n"));
            assert!(!proof.glyphs.is_empty());
            assert!(proof.glyphs.iter().all(|glyph| {
                glyph.glyph_name.as_deref()
                    == snapshot
                        .glyph_order()
                        .get(usize::from(glyph.glyph_id))
                        .map(String::as_str)
            }));
        }
        assert!(
            hebrew
                .glyphs
                .iter()
                .all(|glyph| glyph.glyph_name.as_deref() != Some(".notdef"))
        );
        assert!(
            arabic
                .glyphs
                .iter()
                .all(|glyph| glyph.glyph_name.as_deref() != Some(".notdef"))
        );
    }

    #[test]
    fn selected_arabic_occurrence_survives_real_compiled_proof_and_rejects_drift() {
        use crate::text::buffer::{TextBuffer, TextDirection, TextGlyphInventory};

        let project = project();
        let source = project.document_sources().next().unwrap().id();
        let snapshot = compile(capture(&project).unwrap()).unwrap();
        let mut buffer = TextBuffer::new();
        buffer.set_glyph_inventory(TextGlyphInventory::from_project(&project, source).unwrap());
        buffer.set_compiled_font(Some(snapshot.font.bytes.clone()), vec![0.0]);
        buffer.set_direction(TextDirection::RightToLeft);
        for character in "سلامسلام".chars() {
            assert!(buffer.insert_character(character));
        }
        buffer.shape_arabic_if_rtl();
        assert!(buffer.sort(6).unwrap().is_absorbed());
        buffer.select_range(6, 7);
        assert!(buffer.proof_selection().is_err());
        buffer.select_range(5, 6);
        let selection = buffer.proof_selection().unwrap();
        assert!(
            selection.cluster > 0,
            "second word has its own byte cluster"
        );
        let recipe = CompiledProofRecipe::from_text_selection(selection.clone()).unwrap();
        let proof = prove(&snapshot, recipe.clone()).unwrap();
        assert_eq!(recipe.rendering.pixels_per_em, 32.0);
        let detail = proof.detail.as_ref().expect("selected detail was retained");
        assert_eq!(detail.rendering.pixels_per_em, 320.0);
        assert_eq!(detail.crop.reference_pen_x, selection.reference_pen_x);
        assert_eq!(proof.image(ProofView::Context).unwrap().bytes, proof.png);
        assert_eq!(proof.image(ProofView::Detail).unwrap().bytes, detail.png);
        assert_eq!(
            proof.image(ProofView::Context).unwrap().target_glyph_index,
            proof.image(ProofView::Detail).unwrap().target_glyph_index
        );
        assert_eq!(proof.font_sha256, snapshot.font_sha256);
        assert_eq!(
            proof.recipe_sha256,
            sha256(&serde_json::to_vec(&recipe).unwrap())
        );
        let target = recipe.target.as_ref().unwrap();
        assert!(proof.glyphs.iter().any(|glyph| {
            glyph.glyph_name.as_deref() == Some(target.glyph_name.as_str())
                && glyph.cluster == target.cluster
        }));
        let mut drifted = recipe;
        drifted.target.as_mut().unwrap().cluster = 0;
        assert!(prove(&snapshot, drifted).is_err());

        buffer.select_range(5, 7);
        assert!(buffer.proof_selection().is_err());
        buffer.select_range(5, 6);
        buffer.insert_glyph("manual-only", None, 500.0);
        assert!(buffer.proof_selection().is_err());

        buffer.clear();
        for character in "بِ".chars() {
            assert!(buffer.insert_character(character));
        }
        buffer.shape_arabic_if_rtl();
        buffer.select_range(1, 2);
        let mark = buffer.proof_selection().unwrap();
        assert!(
            mark.right_to_left,
            "auto direction follows the Arabic line after clear"
        );
        assert!(mark.glyph_name.contains("kasra"));
        let mark_recipe = CompiledProofRecipe::from_text_selection(mark).unwrap();
        prove(&snapshot, mark_recipe).expect("selected Arabic mark proof");
    }

    #[test]
    fn staged_edit_proofs_preserve_the_family_and_do_not_publish() {
        use super::super::super::project::{DocumentEditOperation, DocumentLayerEdit};
        use super::super::super::variable::GlyphLayerAddress;

        let project = project();
        let source = project.document_sources().next().unwrap();
        let source_id = source.id();
        let layer = source.default_layer();
        let address = GlyphLayerAddress {
            glyph: "n".into(),
            layer: layer.clone(),
        };
        let expected = project.capture_document_layer(&address).unwrap();
        let revision = project.document_revision();
        let width = project.document_layer("n", &layer).unwrap().width();
        let (anchor_id, anchor_position) = {
            let anchor = project
                .document_layer("n", &layer)
                .unwrap()
                .anchors()
                .find(|anchor| anchor.name() == "top")
                .expect("fixture n top anchor");
            (anchor.id(), anchor.position())
        };
        let baseline = capture(&project).unwrap();
        let original_hash = baseline.canonical_input_sha256().to_owned();
        let transaction = project
            .begin_document_edit_transaction(
                source_id,
                "Preview width",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    expected.clone(),
                    vec![
                        DocumentEditOperation::SetWidth(width + 12.0),
                        DocumentEditOperation::SetAnchor {
                            anchor: anchor_id,
                            position: anchor_position + kurbo::Vec2::new(20.0, 10.0),
                        },
                    ],
                )],
            )
            .unwrap();
        let derived = baseline.with_staged_edit(&project, &transaction).unwrap();
        assert_eq!(project.document_revision(), revision);
        assert_eq!(project.capture_document_layer(&address).unwrap(), expected);
        assert_eq!(baseline.canonical_input_sha256(), original_hash);
        assert_ne!(derived.canonical_input_sha256(), original_hash);
        assert_eq!(derived.document_revision(), baseline.document_revision());
        assert_eq!(
            serde_json::to_value(&derived.font.masters).unwrap(),
            serde_json::to_value(&baseline.font.masters).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&derived.font.axes).unwrap(),
            serde_json::to_value(&baseline.font.axes).unwrap()
        );
        assert_eq!(
            derived.font.features.to_fea(),
            baseline.font.features.to_fea()
        );
        let old = compile(baseline).unwrap();
        let changed = compile(derived).unwrap();
        assert_ne!(old.font_sha256(), changed.font_sha256());
        let advance = |font: &CompiledProofSnapshot, location: f64| {
            font.font
                .advances(&[location])
                .unwrap()
                .into_iter()
                .find(|(name, _)| name == "n")
                .unwrap()
                .1
        };
        assert_eq!(advance(&changed, 0.0) - advance(&old, 0.0), 12.0);
        assert_eq!(advance(&changed, 1.0), advance(&old, 1.0));
        let specimen = recipe("n\u{030a}", 0.0, false, Some("latn"));
        let before_proof = prove(&old, specimen.clone()).unwrap();
        let after_proof = prove(&changed, specimen).unwrap();
        let mark_position = |proof: &CompiledProof| {
            let mut pen = 0.0;
            for glyph in &proof.glyphs {
                if glyph.glyph_name.as_deref() == Some("ringcomb") {
                    return kurbo::Point::new(pen + glyph.x_offset, glyph.y_offset);
                }
                pen += glyph.x_advance;
            }
            panic!("fixture specimen must contain ringcomb");
        };
        assert_eq!(
            mark_position(&after_proof) - mark_position(&before_proof),
            kurbo::Vec2::new(20.0, 10.0)
        );
        assert_ne!(before_proof.png, after_proof.png);
        assert_eq!(project.document_revision(), revision);
    }

    #[test]
    fn single_source_candidate_compiles_replaced_contours_without_publishing() {
        use kurbo::Shape as _;

        use super::super::super::generated::{GeneratedContour, GeneratedPoint};
        use super::super::super::project::{DocumentEditOperation, DocumentLayerEdit};
        use super::super::super::variable::GlyphLayerAddress;
        use crate::font::LayerPointType;

        let project = Project::load(&crate::testing::fonts::regular_ufo()).unwrap();
        let source = project.document_sources().next().unwrap();
        let address = GlyphLayerAddress {
            glyph: "n".into(),
            layer: source.default_layer(),
        };
        let before = project.capture_document_layer(&address).unwrap();
        let baseline = capture(&project).unwrap();
        let rectangle = GeneratedContour {
            points: [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
                .into_iter()
                .map(|(x, y)| GeneratedPoint {
                    position: kurbo::Point::new(x, y),
                    point_type: LayerPointType::Line,
                    smooth: false,
                })
                .collect(),
        };
        let transaction = project
            .begin_document_edit_transaction(
                source.id(),
                "preview n replacement",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    before.clone(),
                    vec![DocumentEditOperation::ReplaceContours(vec![rectangle])],
                )],
            )
            .unwrap();
        let candidate = baseline.with_staged_edit(&project, &transaction).unwrap();
        assert_eq!(project.capture_document_layer(&address).unwrap(), before);
        let old = compile(baseline).unwrap();
        let changed = compile(candidate).unwrap();
        let old_n = old
            .font
            .outlines(&[])
            .unwrap()
            .into_iter()
            .find(|item| item.0 == "n")
            .unwrap()
            .1;
        let changed_n = changed
            .font
            .outlines(&[])
            .unwrap()
            .into_iter()
            .find(|item| item.0 == "n")
            .unwrap()
            .1;
        assert_ne!(old_n.bounding_box(), changed_n.bounding_box());
        assert_eq!(changed_n.bounding_box().width(), 100.0);
        assert_eq!(changed_n.bounding_box().height(), 100.0);
        assert_eq!(project.capture_document_layer(&address).unwrap(), before);
    }

    #[test]
    fn staged_edit_proof_rejects_a_changed_baseline() {
        use super::super::super::project::{DocumentEditOperation, DocumentLayerEdit};
        use super::super::super::variable::GlyphLayerAddress;

        let mut project = project();
        let source = project.document_sources().next().unwrap();
        let source_id = source.id();
        let layer = source.default_layer();
        let address = GlyphLayerAddress {
            glyph: "n".into(),
            layer: layer.clone(),
        };
        let expected = project.capture_document_layer(&address).unwrap();
        let baseline = capture(&project).unwrap();
        let transaction = project
            .begin_document_edit_transaction(
                source_id,
                "Preview width",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    expected,
                    vec![DocumentEditOperation::SetWidth(777.0)],
                )],
            )
            .unwrap();
        project
            .edit_document_layer("n", &layer, |draft| draft.set_width(888.0).map(|_| ()))
            .unwrap();
        assert!(baseline.with_staged_edit(&project, &transaction).is_err());
        assert!(
            project
                .preview_document_edit_transaction(&transaction)
                .is_err()
        );
        assert_eq!(project.document_layer("n", &layer).unwrap().width(), 888.0);
    }

    #[test]
    fn captured_input_is_not_changed_by_a_later_unsaved_width_edit() {
        let mut project = project();
        let source = project.document_sources().next().unwrap();
        let layer = source.default_layer();
        let before = capture(&project).unwrap();
        let width = project.document_layer("n", &layer).unwrap().width();
        let (anchor, anchor_position) = {
            let anchor = project
                .document_layer("n", &layer)
                .unwrap()
                .anchors()
                .find(|anchor| anchor.name() == "top")
                .expect("fixture n top anchor");
            (anchor.id(), anchor.position())
        };
        let disk = fs::read(crate::testing::fonts::regular_ufo().join("glyphs/n_.glif")).unwrap();
        project
            .edit_document_layer("n", &layer, |draft| {
                draft.set_width(width + 12.0).map(|_| ())
            })
            .unwrap();
        project
            .edit_document_layer("n", &layer, |draft| {
                draft
                    .set_anchor_position(anchor, anchor_position + kurbo::Vec2::new(1.0, 0.0))
                    .map(|_| ())
            })
            .unwrap();
        let after = capture(&project).unwrap();
        assert_ne!(
            before.canonical_input_sha256(),
            after.canonical_input_sha256(),
            "the unsaved anchor change participates in the captured canonical source"
        );
        let old = compile(before).unwrap();
        let new = compile(after).unwrap();
        assert_ne!(old.font_sha256(), new.font_sha256());
        assert_eq!(
            disk,
            fs::read(crate::testing::fonts::regular_ufo().join("glyphs/n_.glif")).unwrap()
        );
    }

    #[test]
    fn failed_compile_returns_no_snapshot_or_image() {
        let project = Project::from_source(super::super::super::project::SourceInput::from_font(
            norad::Font::new(),
            "Empty.ufo".into(),
        ));
        assert!(compile(capture(&project).unwrap()).is_err());
    }
}
