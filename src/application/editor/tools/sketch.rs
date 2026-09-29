// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Session-only brush ink, separate from every canonical glyph outline.
//!
//! The mask has a fixed, explicit font-space placement and is never saved with the font.
//! Its PNG is input to the existing guarded, detached candidate worker.

use std::io::Cursor;

#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use base64::Engine as _;
#[cfg(unix)]
use runebender::automation::agent::ToolCall;
#[cfg(unix)]
use runebender::automation::agent_edit::AgentLayerGuard;
#[cfg(unix)]
use runebender::automation::agent_nodes::{NodesLocalSketchSettings, NodesTraceRequest};
#[cfg(unix)]
use runebender::automation::glyph_grading::GradingReferenceRequest;
use runebender::automation::glyph_grading::{GlyphGrade, document_layer_grade};
#[cfg(unix)]
use runebender::font::compiler::proof::CompiledProofRecipe;
#[cfg(unix)]
use runebender::font::edit_batch::canonical_glyph_revision;
#[cfg(unix)]
use runebender::font::variable::{GlyphLayerAddress, LayerId, SourceId};
use runebender::formats::image_trace::TraceCalibration;
#[cfg(unix)]
use runebender::workflows::nodes_session::GraphGuard;
use runebender::workflows::nodes_session::GraphIdentity;

use crate::application::workspace::Mode;
use crate::application::workspace::Workspace;

const EDGE: u32 = 512;
const PIXEL_UNITS: f64 = 2.0;
const BASELINE_PIXEL: f64 = 400.0;

#[cfg(unix)]
static NEXT_SKETCH_SUBMISSION: AtomicU64 = AtomicU64::new(1);

/// Native presentation of one retained candidate in the existing live Nodes session.
pub(crate) struct SketchTraceUi {
    pub(crate) handle: u64,
    pub(crate) backend: SketchBackend,
    pub(crate) identity: GraphIdentity,
    pub(crate) phase: String,
    pub(crate) error: Option<String>,
}

/// The selected candidate generator for one retained brush request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SketchBackend {
    Trace,
    Virtua,
}

impl SketchBackend {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Trace => "Trace",
            Self::Virtua => "Virtua draft",
        }
    }
}

impl SketchTraceUi {
    pub(crate) fn terminal(&self) -> bool {
        matches!(
            self.phase.as_str(),
            "completed" | "failed" | "cancelled" | "stale"
        )
    }
}

/// One temporary image aligned to one glyph in one source.
pub(crate) struct SketchLayer {
    glyph: String,
    source: usize,
    left: f64,
    ink: Vec<u8>,
    ink_count: usize,
    revision: u64,
    pub(crate) brush_units: u16,
    pub(crate) erase: bool,
}

/// An immutable raster and its full-image font placement.
pub(crate) struct SketchExport {
    pub(crate) png: Vec<u8>,
    pub(crate) calibration: TraceCalibration,
    /// Half-open bounds of the dark ink within the complete padded image.
    pub(crate) ink_box_px: [u32; 4],
}

impl SketchLayer {
    pub(crate) fn new(glyph: String, source: usize, advance: f64) -> Self {
        // Centre the 1024-unit image about the advance; the baseline stays at y=0.
        let left =
            ((advance - f64::from(EDGE) * PIXEL_UNITS) / 2.0 / PIXEL_UNITS).round() * PIXEL_UNITS;
        Self {
            glyph,
            source,
            left,
            ink: vec![255; (EDGE * EDGE) as usize],
            ink_count: 0,
            revision: 0,
            brush_units: 96,
            erase: false,
        }
    }

    pub(crate) fn matches(&self, glyph: &str, source: usize) -> bool {
        self.glyph == glyph && self.source == source
    }

    /// Start a fresh glyph mask without resetting the user's selected brush controls.
    pub(crate) fn for_glyph(&self, glyph: String, source: usize, advance: f64) -> Self {
        let mut next = Self::new(glyph, source, advance);
        next.brush_units = self.brush_units;
        next.erase = self.erase;
        next.revision = self.revision.wrapping_add(1);
        next
    }

    pub(crate) fn has_ink(&self) -> bool {
        self.ink_count > 0
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn left(&self) -> f64 {
        self.left
    }

    pub(crate) fn clear(&mut self) -> bool {
        if self.ink_count == 0 {
            return false;
        }
        self.ink.fill(255);
        self.ink_count = 0;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// Stamp a round design-unit brush between two font-space points.
    pub(crate) fn stroke(&mut self, from: (f64, f64), to: (f64, f64)) -> bool {
        let map = |(x, y): (f64, f64)| {
            (
                (x - self.left) / PIXEL_UNITS,
                BASELINE_PIXEL - y / PIXEL_UNITS,
            )
        };
        let a = map(from);
        let b = map(to);
        if ![a.0, a.1, b.0, b.1].iter().all(|value| value.is_finite()) {
            return false;
        }
        let distance = (b.0 - a.0).hypot(b.1 - a.1);
        let steps = distance.mul_add(2.0, 1.0).ceil().clamp(1.0, 4096.0);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "bounded 1..=4096 brush samples"
        )]
        let steps = steps as u32;
        let mut changed = false;
        for step in 0..=steps {
            let t = f64::from(step) / f64::from(steps);
            changed |= self.stamp(
                a.0 + (b.0 - a.0) * t,
                a.1 + (b.1 - a.1) * t,
                f64::from(self.brush_units) / (2.0 * PIXEL_UNITS),
            );
        }
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }

    fn stamp(&mut self, x: f64, y: f64, radius: f64) -> bool {
        let min_x = (x - radius).floor().max(0.0);
        let max_x = (x + radius).ceil().min(f64::from(EDGE - 1));
        let min_y = (y - radius).floor().max(0.0);
        let max_y = (y + radius).ceil().min(f64::from(EDGE - 1));
        if min_x > max_x || min_y > max_y {
            return false;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clipped to a 512-pixel mask"
        )]
        let (min_x, max_x, min_y, max_y) = (min_x as u32, max_x as u32, min_y as u32, max_y as u32);
        let target = if self.erase { 255 } else { 0 };
        let mut changed = false;
        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let dx = f64::from(px) + 0.5 - x;
                let dy = f64::from(py) + 0.5 - y;
                if dx.mul_add(dx, dy * dy) > radius * radius {
                    continue;
                }
                let offset = (py * EDGE + px) as usize;
                if self.ink[offset] != target {
                    self.ink[offset] = target;
                    self.ink_count = if self.erase {
                        self.ink_count - 1
                    } else {
                        self.ink_count + 1
                    };
                    changed = true;
                }
            }
        }
        changed
    }

    pub(crate) fn display_rgba(&self, color: xilem::Color) -> Vec<u8> {
        let color = color.to_rgba8();
        let mut pixels = Vec::with_capacity(self.ink.len() * 4);
        for ink in &self.ink {
            pixels.extend_from_slice(&[
                color.r,
                color.g,
                color.b,
                if *ink == 0 { color.a } else { 0 },
            ]);
        }
        pixels
    }

    pub(crate) fn export(&self) -> Result<SketchExport, String> {
        if !self.has_ink() {
            return Err("draw some brush ink before tracing".into());
        }
        let last = EDGE - 1;
        if (0..EDGE).any(|offset| {
            let top = offset as usize;
            let bottom = (last * EDGE + offset) as usize;
            let left = (offset * EDGE) as usize;
            let right = (offset * EDGE + last) as usize;
            [top, bottom, left, right]
                .into_iter()
                .any(|index| self.ink[index] == 0)
        }) {
            return Err(
                "brush ink touches the image edge; erase it or redraw inside the 1024-unit canvas"
                    .into(),
            );
        }
        let image = image::GrayImage::from_raw(EDGE, EDGE, self.ink.clone())
            .ok_or("sketch mask dimensions changed")?;
        let mut ink_box_px = [EDGE, EDGE, 0, 0];
        for y in 0..EDGE {
            for x in 0..EDGE {
                if self.ink[(y * EDGE + x) as usize] == 0 {
                    ink_box_px[0] = ink_box_px[0].min(x);
                    ink_box_px[1] = ink_box_px[1].min(y);
                    ink_box_px[2] = ink_box_px[2].max(x + 1);
                    ink_box_px[3] = ink_box_px[3].max(y + 1);
                }
            }
        }
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|error| format!("encode sketch PNG: {error}"))?;
        Ok(SketchExport {
            png: png.into_inner(),
            calibration: TraceCalibration {
                font_units_per_pixel: PIXEL_UNITS,
                pixel_baseline_y: BASELINE_PIXEL,
                font_x_at_left: self.left,
                font_baseline_y: 0.0,
            },
            ink_box_px,
        })
    }
}

impl Workspace {
    /// Submit this glyph's temporary ink to the existing detached trace worker.
    pub(crate) fn trace_sketch_to_draft(&mut self) {
        #[cfg(unix)]
        {
            self.note = match self.submit_sketch_trace(SketchBackend::Trace) {
                Ok(handle) => format!("Brush trace {handle} queued; check its status here"),
                Err(error) => error,
            };
        }
        #[cfg(not(unix))]
        {
            self.note = "Brush tracing is available in the native editor on Unix".into();
        }
    }

    /// Submit the same guarded scratch ink to the explicitly selected local Virtua model.
    pub(crate) fn draft_sketch_with_virtua(&mut self) {
        #[cfg(unix)]
        {
            self.note = match self.submit_sketch_trace(SketchBackend::Virtua) {
                Ok(handle) => format!("Virtua draft {handle} queued; check its status here"),
                Err(error) => error,
            };
        }
        #[cfg(not(unix))]
        {
            self.note = "Local Virtua drafting is available in the native editor on Unix".into();
        }
    }

    /// Refresh the retained worker's phase and any failure reason in the brush inspector.
    pub(crate) fn refresh_sketch_trace(&mut self) {
        #[cfg(unix)]
        {
            self.note = match self.sketch_trace_call("nodes_trace_status") {
                Ok(status) => {
                    self.update_sketch_trace(&status);
                    self.sketch_trace.as_ref().map_or_else(
                        || "No retained brush trace".into(),
                        |trace| format!("Brush trace {}: {}", trace.handle, trace.phase),
                    )
                }
                Err(error) => {
                    if let Some(trace) = self.sketch_trace.as_mut() {
                        trace.error = Some(error.clone());
                    }
                    error
                }
            };
        }
        #[cfg(not(unix))]
        {
            self.note = "Brush tracing is available in the native editor on Unix".into();
        }
    }

    /// Request cancellation without releasing a worker that may still be running.
    pub(crate) fn cancel_sketch_trace(&mut self) {
        #[cfg(unix)]
        {
            self.note = match self.sketch_trace_call("nodes_trace_cancel") {
                Ok(status) => {
                    self.update_sketch_trace(&status);
                    "Cancelling brush trace; check status until it settles".into()
                }
                Err(error) => error,
            };
        }
        #[cfg(not(unix))]
        {
            self.note = "Brush tracing is available in the native editor on Unix".into();
        }
    }

    /// Drop only a terminal trace receipt; its already published graph intent remains.
    pub(crate) fn release_sketch_trace(&mut self) {
        #[cfg(unix)]
        {
            self.note = match self.release_sketch_trace_result() {
                Ok(()) => "Brush trace released; the font is unchanged".into(),
                Err(error) => error,
            };
        }
        #[cfg(not(unix))]
        {
            self.note = "Brush tracing is available in the native editor on Unix".into();
        }
    }

    /// Release a settled image worker and submit the current ink as a fresh guarded candidate.
    pub(crate) fn retry_sketch_trace(&mut self) {
        #[cfg(unix)]
        {
            let Some(backend) = self.sketch_trace.as_ref().map(|trace| trace.backend) else {
                self.note = "No retained brush candidate to retry".into();
                return;
            };
            self.note = match self.release_sketch_trace_result() {
                Ok(()) => match self.submit_sketch_trace(backend) {
                    Ok(handle) => format!("Brush candidate {handle} queued; check its status here"),
                    Err(error) => error,
                },
                Err(error) => error,
            };
        }
        #[cfg(not(unix))]
        {
            self.note = "Brush tracing is available in the native editor on Unix".into();
        }
    }

    /// Show the published comparison only after a guarded trace has completed.
    pub(crate) fn open_sketch_comparison(&mut self) {
        self.refresh_sketch_trace();
        if self
            .sketch_trace
            .as_ref()
            .is_some_and(|trace| trace.phase == "completed")
        {
            self.mode = Mode::Nodes;
            self.note = "Review both proof branches before Apply".into();
        }
    }

    #[cfg(unix)]
    fn sketch_trace_call(&mut self, name: &str) -> Result<serde_json::Value, String> {
        let request = {
            let trace = self
                .sketch_trace
                .as_ref()
                .ok_or("No retained brush trace")?;
            serde_json::json!({
                "expected_document_epoch": trace.identity.document_epoch,
                "identity": trace.identity,
                "handle": trace.handle,
            })
        };
        let response = self
            .call_agent_nodes(&ToolCall {
                name: name.into(),
                arguments: request,
            })
            .ok_or("native trace status is unavailable")?;
        if response["ok"] != true {
            return Err(response["error"]
                .as_str()
                .unwrap_or("brush trace status was rejected")
                .into());
        }
        Ok(response)
    }

    #[cfg(unix)]
    fn update_sketch_trace(&mut self, status: &serde_json::Value) {
        if let Some(trace) = self.sketch_trace.as_mut() {
            trace.phase = status["phase"].as_str().unwrap_or("unknown").into();
            trace.error = status["error"].as_str().map(str::to_owned);
        }
    }

    #[cfg(unix)]
    fn release_sketch_trace_result(&mut self) -> Result<(), String> {
        if self
            .sketch_trace
            .as_ref()
            .is_none_or(|trace| !trace.terminal())
        {
            return Err("Wait for the brush trace to settle before releasing it".into());
        }
        self.sketch_trace_call("nodes_trace_release")?;
        self.sketch_trace = None;
        Ok(())
    }

    #[cfg(unix)]
    fn submit_sketch_trace(&mut self, backend: SketchBackend) -> Result<u64, String> {
        if self.sketch_trace.is_some() {
            return Err("Release the retained brush trace before submitting another".into());
        }
        let glyph = self.session.glyph_name.clone();
        let source_index = self.font.active();
        let source = self
            .font
            .project
            .source_id(source_index)
            .ok_or("active source is unavailable")?;
        let expected_recipe = self.selected_arabic_recipe(&glyph, source)?;
        let references = self.inferred_sketch_references(&glyph, source)?;
        let export = {
            let sketch = self.sketch.lock().map_err(|_| "brush ink is unavailable")?;
            if !sketch.matches(&glyph, source_index) {
                return Err("brush ink belongs to another glyph or source; draw here first".into());
            }
            sketch.export()?
        };
        let local_sketch = if backend == SketchBackend::Virtua {
            let selected = self.sketch_selected_model.trim();
            if selected.is_empty() {
                return Err("Choose an installed Virtua model before drafting".into());
            }
            if !self
                .sketch_models
                .iter()
                .any(|entry| entry.id == selected && entry.ready)
            {
                return Err(
                    "Selected Virtua model is unavailable; refresh models and choose a ready one"
                        .into(),
                );
            }
            if !self.sketch_identity.is_finite() || !(0.0..=1.5).contains(&self.sketch_identity) {
                return Err("Virtua identity must be between 0 and 1.5".into());
            }
            Some(NodesLocalSketchSettings {
                model: Some(selected.into()),
                ink_box_px: export.ink_box_px,
                codepoint: expected_recipe
                    .target
                    .as_ref()
                    .and_then(|target| usize::try_from(target.cluster).ok())
                    .and_then(|cluster| expected_recipe.text.get(cluster..))
                    .and_then(|text| text.chars().next())
                    .map(u32::from),
                candidates: 3,
                temperature: 0.5,
                identity: Some(self.sketch_identity),
                seed: 0,
                timeout_seconds: 120,
            })
        } else {
            None
        };
        let source_layer = self
            .font
            .project
            .document_source(source)
            .ok_or("active source is unavailable")?
            .default_layer();
        let target = self.sketch_layer_guard(&glyph, source, &source_layer.name)?;
        self.ensure_live_graph_for_sketch(expected_recipe.clone())?;
        let snapshot = self
            .live_graph_session()
            .ok_or("live Nodes graph is unavailable")?
            .snapshot();
        let candidates: Vec<_> = snapshot
            .graph
            .nodes
            .iter()
            .filter(|node| node.type_name == "live.python")
            .map(|node| node.id)
            .collect();
        let [node] = candidates.as_slice() else {
            return Err("select a graph with exactly one live.python candidate node".into());
        };
        self.check_sketch_proofs(&snapshot.graph, *node, source_index, &expected_recipe)?;
        let epoch = self
            .live
            .as_ref()
            .ok_or("native live endpoint is unavailable")?
            .document_epoch()
            .to_owned();
        let operation_key = format!(
            "native-brush-{}",
            NEXT_SKETCH_SUBMISSION.fetch_add(1, Ordering::Relaxed)
        );
        let request = NodesTraceRequest {
            expected_document_epoch: epoch,
            guard: GraphGuard {
                identity: snapshot.identity.clone(),
                revision: snapshot.revision,
            },
            actor: "native-brush".into(),
            operation_key,
            node: *node,
            source: source_index,
            target,
            references,
            image_base64: base64::engine::general_purpose::STANDARD.encode(export.png),
            calibration: export.calibration,
            invert: false,
            local_sketch,
        };
        let response = self
            .call_agent_nodes(&ToolCall {
                name: "nodes_trace".into(),
                arguments: serde_json::to_value(request).map_err(|error| error.to_string())?,
            })
            .ok_or("native trace request is unavailable")?;
        if response["ok"] != true {
            return Err(response["error"]
                .as_str()
                .unwrap_or("brush trace was rejected")
                .into());
        }
        let handle = response["handle"]
            .as_u64()
            .ok_or("trace worker did not return a handle")?;
        self.sketch_trace = Some(SketchTraceUi {
            handle,
            backend,
            identity: snapshot.identity,
            phase: response["phase"].as_str().unwrap_or("queued").into(),
            error: None,
        });
        self.nodes.live_selected = true;
        self.nodes.live_scope = glyph;
        self.nodes.fit_request = self.nodes.fit_request.wrapping_add(1);
        Ok(handle)
    }

    #[cfg(unix)]
    fn selected_arabic_recipe(
        &self,
        glyph: &str,
        source: SourceId,
    ) -> Result<CompiledProofRecipe, String> {
        let instruction = "Click the Arabic letter you want to draft in the Text tool, then return to Brush. Your sketch will stay here";
        if !self.has_text_session {
            return Err(instruction.into());
        }
        let capture = self.text_proof_selection.as_ref().ok_or(instruction)?;
        if capture.context != self.text_context_id()
            || capture.source != Some(source)
            || capture.axis_values != self.axis_values
        {
            return Err(format!(
                "The text selection belongs to another tab, source or location. {instruction}"
            ));
        }
        let selection = capture
            .selection
            .clone()
            .map_err(|error| format!("{instruction}: {error}"))?;
        let disabled: std::collections::HashSet<_> = selection
            .features
            .iter()
            .filter_map(|(tag, enabled)| (!enabled).then_some(tag.as_str()))
            .collect();
        if selection.text != self.initial_text
            || disabled.len() != selection.features.len()
            || disabled
                != self
                    .text_features_disabled
                    .iter()
                    .map(String::as_str)
                    .collect()
            || selection.script != self.text_script
            || selection.language != self.text_language
            || self.text_dir.is_some_and(|direction| {
                (direction == runebender::text::buffer::TextDirection::RightToLeft)
                    != selection.right_to_left
            })
        {
            return Err(format!(
                "The text settings changed after you selected the letter. {instruction}"
            ));
        }
        if selection.glyph_name != glyph
            || !selection.right_to_left
            || !selection.text.chars().any(is_arabic_character)
        {
            return Err(format!(
                "Select {glyph} in a right-to-left Arabic line. Your sketch will stay here"
            ));
        }
        CompiledProofRecipe::from_text_selection(selection)
    }

    #[cfg(unix)]
    fn check_sketch_proofs(
        &self,
        graph: &runebender::workflows::nodes::NodeGraph,
        candidate: u32,
        source: usize,
        expected: &CompiledProofRecipe,
    ) -> Result<(), String> {
        let source_nodes: Vec<_> = graph
            .nodes
            .iter()
            .filter(|node| {
                node.type_name == "live.font"
                    && node
                        .values
                        .get("source")
                        .and_then(serde_json::Value::as_u64)
                        == u64::try_from(source).ok()
            })
            .map(|node| node.id)
            .collect();
        let mut base = 0;
        let mut changed = 0;
        for node in graph
            .nodes
            .iter()
            .filter(|node| node.type_name == "live.proof")
        {
            let upstream = graph.link_into(node.id, "font").map(|link| link.from());
            let paired = upstream == Some(candidate)
                || upstream.is_some_and(|id| source_nodes.contains(&id));
            if !paired {
                continue;
            }
            let recipe: CompiledProofRecipe = node
                .values
                .get("recipe")
                .cloned()
                .ok_or_else(|| "comparison proof has no recipe".to_owned())
                .and_then(|value| {
                    serde_json::from_value(value).map_err(|error| error.to_string())
                })?;
            if !same_sketch_context(&recipe, expected) {
                return Err("Comparison proof has different text or target; reopen the scratch font and select the Arabic occurrence before creating its comparison".into());
            }
            if upstream == Some(candidate) {
                changed += 1;
            } else {
                base += 1;
            }
        }
        if base == 0 || changed == 0 {
            return Err(
                "Brush tracing needs a live comparison with baseline and changed proofs".into(),
            );
        }
        Ok(())
    }

    #[cfg(unix)]
    fn sketch_layer_guard(
        &self,
        glyph: &str,
        source: SourceId,
        layer: &str,
    ) -> Result<AgentLayerGuard, String> {
        let glyph_id = self
            .font
            .project
            .document_glyph(glyph)
            .ok_or_else(|| format!("{glyph} is not in the document"))?
            .id()
            .to_wire();
        let address = GlyphLayerAddress {
            glyph: glyph.into(),
            layer: LayerId {
                source,
                name: layer.into(),
            },
        };
        let snapshot = self
            .font
            .project
            .capture_document_layer(&address)
            .ok_or_else(|| format!("{glyph} has no layer in the active source"))?;
        Ok(AgentLayerGuard {
            glyph: glyph.into(),
            glyph_id,
            layer: layer.into(),
            expected_revision: canonical_glyph_revision(snapshot.view())?,
        })
    }

    /// Suggest frozen green Arabic references from this source, preferring the same letter.
    pub(crate) fn sketch_reference_names(&self) -> Vec<String> {
        let Some(source) = self.font.project.source_id(self.font.active()) else {
            return Vec::new();
        };
        let glyph = &self.session.glyph_name;
        let family = glyph.split('.').next().unwrap_or(glyph);
        let form = glyph.rsplit_once('.').map(|(_, form)| form);
        let layer = self
            .font
            .project
            .document_source(source)
            .map(|source| source.default_layer());
        let target_width = layer
            .as_ref()
            .and_then(|layer| self.font.project.document_layer(glyph, layer))
            .map_or(0.0, |layer| layer.width());
        let mut same_letter = Vec::new();
        let mut same_form = Vec::new();
        for name in self.font.project.glyph_names() {
            if name == glyph || !name.contains("-ar") {
                continue;
            }
            if document_layer_grade(&self.font.project, source, name) != GlyphGrade::Green {
                continue;
            }
            let Some(reference_layer) = layer
                .as_ref()
                .and_then(|layer| self.font.project.document_layer(name, layer))
            else {
                continue;
            };
            if reference_layer.contours().next().is_none()
                && reference_layer.components().next().is_none()
            {
                continue;
            }
            if name == family || name.starts_with(&format!("{family}.")) {
                same_letter.push(name.to_owned());
            } else if form.is_some_and(|form| name.ends_with(&format!(".{form}"))) {
                same_form.push((
                    (reference_layer.width() - target_width).abs(),
                    name.to_owned(),
                ));
            }
        }
        if !same_letter.is_empty() {
            same_letter.sort();
            same_letter.truncate(3);
            return same_letter;
        }
        same_form.sort_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
        });
        same_form
            .into_iter()
            .take(2)
            .map(|(_, name)| name)
            .collect()
    }

    #[cfg(unix)]
    fn inferred_sketch_references(
        &self,
        glyph: &str,
        source: SourceId,
    ) -> Result<Vec<GradingReferenceRequest>, String> {
        let layer = self
            .font
            .project
            .document_source(source)
            .ok_or("active source is unavailable")?
            .default_layer();
        let family = glyph.split('.').next().unwrap_or(glyph);
        let names = self.sketch_reference_names();
        if names.is_empty() {
            return Err(format!(
                "No green Arabic reference is available for {glyph} in this source"
            ));
        }
        names
            .into_iter()
            .map(|name| {
                let same_letter = name == family || name.starts_with(&format!("{family}."));
                Ok(GradingReferenceRequest {
                    guard: self.sketch_layer_guard(&name, source, &layer.name)?,
                    rationale: if same_letter {
                        "Same Arabic letter, graded green; reference for its stroke weight, terminals and joining behavior".into()
                    } else {
                        "Graded green Arabic glyph in the same positional form with similar advance; reference for stroke weight and joining, not letter skeleton".into()
                    },
                })
            })
            .collect()
    }
}

#[cfg(unix)]
fn is_arabic_character(character: char) -> bool {
    matches!(
        character as u32,
        0x0600..=0x06ff | 0x0750..=0x077f | 0x08a0..=0x08ff | 0xfb50..=0xfdff | 0xfe70..=0xfeff
    )
}

#[cfg(unix)]
#[cfg(unix)]
fn same_sketch_context(left: &CompiledProofRecipe, right: &CompiledProofRecipe) -> bool {
    left.text == right.text
        && left.normalized_location == right.normalized_location
        && left.right_to_left == right.right_to_left
        && left.features == right.features
        && left.script == right.script
        && left.language == right.language
        && left.target == right.target
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strokes_erase_and_clear_leave_canonical_font_out_of_scope() {
        let mut sketch = SketchLayer::new("kaf-ar.medi".into(), 0, 808.0);
        assert!(sketch.stroke((0.0, 0.0), (700.0, 600.0)));
        assert!(sketch.has_ink());
        let export = sketch.export().unwrap();
        assert_eq!(export.calibration.font_units_per_pixel, 2.0);
        assert_eq!(export.calibration.pixel_baseline_y, 400.0);
        assert_eq!(export.calibration.font_x_at_left, -108.0);
        sketch.erase = true;
        assert!(sketch.stroke((0.0, 0.0), (0.0, 0.0)));
        assert!(sketch.clear());
        assert!(!sketch.has_ink());
        assert!(!sketch.clear());
        assert!(sketch.export().is_err());
        assert!(!sketch.matches("kaf-ar.fina", 0));
    }

    #[test]
    fn exported_padded_ink_keeps_exact_font_placement() {
        let mut sketch = SketchLayer::new("kaf-ar.medi".into(), 0, 808.0);
        sketch.brush_units = 16;
        assert!(sketch.stroke((0.0, 0.0), (0.0, 0.0)));
        let export = sketch.export().unwrap();
        assert_eq!(export.ink_box_px, [50, 396, 58, 404]);
        let png = image::load_from_memory_with_format(&export.png, image::ImageFormat::Png)
            .unwrap()
            .to_luma8();
        assert_eq!(png.dimensions(), (512, 512));
        assert_eq!(png.get_pixel(54, 400).0[0], 0);
        assert_eq!(png.get_pixel(0, 400).0[0], 255);
        assert_eq!(png.get_pixel(54, 0).0[0], 255);
        let font_x =
            export.calibration.font_x_at_left + 54.0 * export.calibration.font_units_per_pixel;
        let font_y = export.calibration.font_baseline_y
            + (export.calibration.pixel_baseline_y - 400.0)
                * export.calibration.font_units_per_pixel;
        assert_eq!((font_x, font_y), (0.0, 0.0));
    }

    #[test]
    fn changing_glyph_clears_ink_but_keeps_brush_settings() {
        let mut original = SketchLayer::new("A".into(), 0, 500.0);
        original.brush_units = 152;
        assert!(original.stroke((0.0, 0.0), (0.0, 0.0)));
        original.erase = true;
        let next = original.for_glyph("B".into(), 1, 600.0);
        assert!(next.matches("B", 1));
        assert!(!next.has_ink());
        assert_eq!(next.brush_units, 152);
        assert!(next.erase);
        assert_ne!(next.revision(), original.revision());
    }

    #[test]
    fn edge_ink_refuses_a_clipped_trace() {
        let mut sketch = SketchLayer::new("A".into(), 0, 500.0);
        sketch.brush_units = 16;
        assert!(sketch.stroke((sketch.left(), 0.0), (sketch.left(), 0.0)));
        assert!(
            sketch
                .export()
                .err()
                .expect("edge ink must refuse export")
                .contains("image edge")
        );
        sketch.erase = true;
        assert!(sketch.stroke((sketch.left(), 0.0), (sketch.left(), 0.0)));
        assert!(!sketch.has_ink());
    }

    #[cfg(unix)]
    #[test]
    fn comparison_keeps_selected_arabic_occurrence_across_view_scales() {
        use runebender::font::compiler::proof::{CompiledProofRendering, CompiledProofTarget};

        let mut reading = CompiledProofRecipe {
            text: "مكتبة".into(),
            normalized_location: vec![],
            right_to_left: true,
            features: vec![],
            script: Some("arab".into()),
            language: Some("ar".into()),
            rendering: CompiledProofRendering::default(),
            target: Some(CompiledProofTarget {
                glyph_name: "kaf-ar.medi".into(),
                cluster: 2,
                occurrence: 0,
                reference_pen_x: Some(320.0),
            }),
        };
        assert!(reading.text.chars().any(is_arabic_character));
        assert!(!is_arabic_character('A'));
        let mut detail = reading.clone();
        detail.rendering.pixels_per_em = 160.0;
        assert!(same_sketch_context(&reading, &detail));
        detail.target.as_mut().unwrap().occurrence += 1;
        assert!(!same_sketch_context(&reading, &detail));
        reading.right_to_left = false;
        assert!(!same_sketch_context(&reading, &detail));
    }

    #[cfg(unix)]
    #[test]
    fn brush_keeps_selected_arabic_occurrence_after_document_revision_changes() {
        use crate::application::font_model::FontModel;
        use crate::application::workspace::TextProofCapture;
        use runebender::font::project::Project;
        use runebender::text::buffer::TextProofSelection;

        let project = Project::new_font(std::env::temp_dir().join("brush-stale-selection.ufo"));
        let source = project.source_id(0).unwrap();
        let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
        app.has_text_session = true;
        app.initial_text = "مكتبة".into();
        app.text_proof_selection = Some(TextProofCapture {
            context: app.text_context_id(),
            source: Some(source),
            document_revision: app.font.project.document_revision().wrapping_add(1),
            axis_values: app.axis_values.clone(),
            selection: Ok(TextProofSelection {
                text: "مكتبة".into(),
                normalized_location: Vec::new(),
                right_to_left: true,
                features: Vec::new(),
                script: None,
                language: None,
                glyph_name: "kaf-ar.medi".into(),
                cluster: 2,
                occurrence: 0,
                reference_pen_x: 320.0,
            }),
        });
        let recipe = app.selected_arabic_recipe("kaf-ar.medi", source).unwrap();
        assert_eq!(recipe.target.as_ref().unwrap().glyph_name, "kaf-ar.medi");
        assert!(app.selected_arabic_recipe("beh-ar.medi", source).is_err());
        app.ensure_live_graph_for_sketch(recipe.clone()).unwrap();
        let graph = app.live_graph_session().unwrap().snapshot().graph;
        let proof_recipes: Vec<CompiledProofRecipe> = graph
            .nodes
            .iter()
            .filter(|node| node.type_name == "live.proof")
            .map(|node| serde_json::from_value(node.values["recipe"].clone()).unwrap())
            .collect();
        assert_eq!(proof_recipes, vec![recipe.clone(), recipe]);
    }

    #[cfg(unix)]
    #[test]
    fn brush_infers_green_reference_from_the_same_arabic_letter() {
        use crate::application::font_model::FontModel;
        use runebender::automation::glyph_grading::capture_replacement_context;
        use runebender::font::model::glyph_metadata::MarkColor;
        use runebender::font::project::Project;

        let mut project = Project::new_font(std::env::temp_dir().join("brush-green-reference.ufo"));
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        for (name, label) in [
            ("kaf-ar.init", "green"),
            ("kaf-ar.medi", "red"),
            ("kaf-ar.fina", "yellow"),
            ("beh-ar.medi", "green"),
        ] {
            project.add_document_glyph(name, 600.0, None).unwrap();
            let color =
                MarkColor::parse(&runebender::ui::theme::ufo_rgba_for_label(label).unwrap())
                    .unwrap();
            project
                .edit_document_layer(name, &layer, |draft| {
                    draft.set_mark(Some(label), Some(color))?;
                    if label == "green" {
                        draft
                            .add_shape_contour(kurbo::Rect::new(20.0, 0.0, 300.0, 500.0), false)?;
                    }
                    Ok(())
                })
                .unwrap();
        }
        let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
        app.edit_text_sort_glyph(
            app.font.index_of("kaf-ar.medi").unwrap(),
            crate::application::workspace::Tool::Sketch,
        );
        assert_eq!(app.sketch_reference_names(), vec!["kaf-ar.init"]);
        let references = app
            .inferred_sketch_references("kaf-ar.medi", source)
            .unwrap();
        let target = app
            .sketch_layer_guard("kaf-ar.medi", source, &layer.name)
            .unwrap();
        let grading =
            capture_replacement_context(&app.font.project, source, &target, &references).unwrap();
        assert_eq!(grading.references[0].layer.glyph, "kaf-ar.init");
    }

    #[test]
    fn retained_trace_requires_a_terminal_phase_before_release() {
        let mut trace = SketchTraceUi {
            handle: 1,
            backend: SketchBackend::Virtua,
            identity: GraphIdentity {
                session_id: "graph".into(),
                document_epoch: "epoch".into(),
            },
            phase: "cancelling".into(),
            error: None,
        };
        assert!(!trace.terminal());
        for phase in ["completed", "failed", "cancelled", "stale"] {
            trace.phase = phase.into();
            assert!(trace.terminal());
        }
    }
}
