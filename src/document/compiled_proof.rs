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

use super::compile::CompiledFont;
use super::project::Project;
use crate::text::shape::ShapingFont;

const MAX_TEXT_BYTES: usize = 4 * 1024;
const MAX_SHAPED_GLYPHS: usize = 1024;
const MAX_FEATURES: usize = 64;
const MAX_LANGUAGE_BYTES: usize = 35;
const PROOF_WIDTH: u32 = 1024;
const PROOF_HEIGHT: u32 = 1024;
const PROOF_MARGIN: f64 = 32.0;
const PROOF_LINE_HEIGHT: f64 = 180.0;

/// A compiler identity for a proof snapshot.
///
/// This identifies the pinned compiler sources used to make the bytes.
/// It is not a hash of an application executable; session adapters add that
/// stronger process identity when one is available.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
}

impl CompiledProofRecipe {
    /// Validate bounded, finite inputs before expensive outline extraction.
    pub fn validate(&self) -> Result<(), String> {
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
        Ok(())
    }
}

/// One shaped glyph in a compiled proof.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    /// Positioned glyphs from `HarfRust`, in paint order.
    pub glyphs: Vec<CompiledProofGlyph>,
    /// Bounded PNG rasterized from Skrifa outlines of those glyphs.
    pub png: Vec<u8>,
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
    let outlines = snapshot
        .font
        .outlines(&recipe.normalized_location)?
        .into_iter()
        .map(|(_, outline)| outline)
        .collect::<Vec<_>>();
    let units_per_em = units_per_em(&snapshot.font.bytes)?;
    let scene = scene(&glyphs, &outlines, units_per_em, recipe.right_to_left)?;
    let png = crate::formats::designbot::render(&scene, false)?;
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("proof renderer did not return a PNG".into());
    }
    Ok(CompiledProof {
        font_sha256: snapshot.font_sha256.clone(),
        document_revision: snapshot.document_revision,
        compiler: snapshot.compiler.clone(),
        canonical_input_sha256: snapshot.canonical_input_sha256.clone(),
        recipe,
        glyphs,
        png,
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
) -> Result<serde_json::Value, String> {
    use kurbo::{Affine, Shape as _};
    use serde_json::json;

    if !units_per_em.is_finite() || units_per_em <= 0.0 {
        return Err("compiled font has an invalid units-per-em value".into());
    }
    let scale = 160.0 / units_per_em;
    let mut paths = Vec::with_capacity(glyphs.len());
    let mut x = if right_to_left {
        f64::from(PROOF_WIDTH) - PROOF_MARGIN
    } else {
        PROOF_MARGIN
    };
    let mut baseline = 180.0;
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
        let advance = glyph.x_advance.max(0.0) * scale;
        if advance > f64::from(PROOF_WIDTH) - 2.0 * PROOF_MARGIN {
            return Err("one shaped glyph exceeds the bounded proof width".into());
        }
        let overflow = if right_to_left {
            x - advance < PROOF_MARGIN
        } else {
            x + advance > f64::from(PROOF_WIDTH) - PROOF_MARGIN
        };
        if overflow {
            x = if right_to_left {
                f64::from(PROOF_WIDTH) - PROOF_MARGIN
            } else {
                PROOF_MARGIN
            };
            baseline += PROOF_LINE_HEIGHT;
        }
        if baseline + PROOF_LINE_HEIGHT > f64::from(PROOF_HEIGHT) {
            return Err("proof exceeds the bounded 1024 by 1024 image".into());
        }
        if right_to_left {
            x -= advance;
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
                || bounds.x1 > f64::from(PROOF_WIDTH)
                || bounds.y1 > f64::from(PROOF_HEIGHT))
        {
            return Err("proof outline would be clipped by the bounded image".into());
        }
        paths.push(json!({"d": translated.to_svg()}));
        if !right_to_left {
            x += advance;
        }
    }
    Ok(json!({
        "version": 1,
        "width": PROOF_WIDTH,
        "height": PROOF_HEIGHT,
        "paths": paths,
        "labels": [],
    }))
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
        assert!(scene(std::slice::from_ref(&glyph), &[], 1000.0, false).is_err());

        let too_wide = Arc::new(Rect::new(0.0, 0.0, 10_000.0, 1.0).to_path(0.1));
        assert!(scene(&[glyph], &[too_wide], 1000.0, false).is_err());
    }

    #[test]
    fn capture_freezes_feature_include_content_before_worker_compilation() {
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
        let project =
            Project::from_source(super::super::project::SourceInput::from_font(font, ufo));

        let input = capture(&project).unwrap();
        fs::write(&include, "# changed after capture\n").unwrap();

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
        let project =
            Project::from_source(super::super::project::SourceInput::from_font(font, ufo));

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
        let project = Project::from_source(super::super::project::SourceInput::from_font(
            norad::Font::new(),
            "Empty.ufo".into(),
        ));
        assert!(compile(capture(&project).unwrap()).is_err());
    }
}
