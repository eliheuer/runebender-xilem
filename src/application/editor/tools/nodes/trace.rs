// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bounded off-editor image candidates for the existing live Nodes graph.
//!
//! A single process-wide worker handles at most two admitted images or local sketch jobs.
//! Workspace retains only one candidate receipt per graph session. A finished draft becomes graph
//! intent only after the captured document, source, layer and graph guards are checked again.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::time::Duration;

use base64::Engine as _;
use runebender::automation::agent::ToolCall;
use runebender::automation::agent_edit::{
    AgentEditOperation, AgentEditRequest, AgentLayerEdits, AgentLayerGuard,
};
use runebender::automation::agent_nodes::results::{
    NodesTraceBackend, NodesTracePhase, NodesTraceReleaseResult, NodesTraceStartResult,
    NodesTraceStatusResult,
};
use runebender::automation::agent_nodes::{
    NodesLocalSketchSettings, NodesTraceHandleRequest, NodesTraceRequest,
};
use runebender::automation::glyph_grading::{
    GradingContext, GradingReferenceRequest, capture_replacement_context,
};
use runebender::font::edit_batch::canonical_glyph_revision;
use runebender::font::variable::{GlyphLayerAddress, LayerId, SourceId};
use runebender::formats::image_trace::{CalibratedTrace, TraceCalibration, trace_image_calibrated};
use runebender::workflows::local_sketch::{
    SketchCandidate, SketchPlacement, SketchRequest, SketchRuntime, SketchRuntimeIdentity,
    inspect_runtime, run as run_local_sketch,
};
use runebender::workflows::nodes_session::{
    GraphEdit, GraphGuard, GraphMutation, GraphMutationRequest, GraphMutationResponse,
};
use runebender::workflows::process::ProcessCancellation;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use crate::application::workspace::Workspace;

const MAX_GLOBAL_TRACE_JOBS: usize = 2;
static GLOBAL_TRACE_JOBS: AtomicUsize = AtomicUsize::new(0);
static TRACE_WORKER: OnceLock<SyncSender<TraceWork>> = OnceLock::new();

/// One globally bounded worker message. The editor thread never decodes the image.
struct TraceWork {
    image_base64: String,
    calibration: TraceCalibration,
    backend: TraceWorkBackend,
    cancelled: ProcessCancellation,
    events: mpsc::Sender<TraceEvent>,
}

enum TraceWorkBackend {
    Calibrated {
        invert: bool,
    },
    LocalSketch {
        runtime: SketchRuntime,
        settings: NodesLocalSketchSettings,
        glyph: String,
        advance: f64,
    },
}

enum TraceOutput {
    Calibrated(CalibratedTrace),
    LocalSketch(Box<SketchCandidate>),
}

impl From<CalibratedTrace> for TraceOutput {
    fn from(trace: CalibratedTrace) -> Self {
        Self::Calibrated(trace)
    }
}

enum TraceEvent {
    Started,
    Pinned(SketchRuntimeIdentity),
    Finished(Result<TraceOutput, String>),
}

fn worker() -> &'static SyncSender<TraceWork> {
    TRACE_WORKER.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel::<TraceWork>(MAX_GLOBAL_TRACE_JOBS - 1);
        std::thread::spawn(move || {
            while let Ok(work) = receiver.recv() {
                if !work.cancelled.is_cancelled() {
                    let _ = work.events.send(TraceEvent::Started);
                }
                let result = if work.cancelled.is_cancelled() {
                    Err("trace cancelled before decoding".into())
                } else {
                    base64::engine::general_purpose::STANDARD
                        .decode(&work.image_base64)
                        .map_err(|error| format!("invalid base64 trace image: {error}"))
                        .and_then(|image| match work.backend {
                            TraceWorkBackend::Calibrated { invert } => {
                                trace_image_calibrated(&image, work.calibration, invert)
                                    .map(TraceOutput::Calibrated)
                            }
                            TraceWorkBackend::LocalSketch {
                                runtime,
                                settings,
                                glyph,
                                advance,
                            } => {
                                let pinned = inspect_runtime(&runtime)?;
                                let _ = work.events.send(TraceEvent::Pinned(pinned.clone()));
                                run_local_sketch(
                                    &runtime,
                                    &pinned,
                                    &SketchRequest {
                                        png: image,
                                        glyph,
                                        codepoint: settings.codepoint,
                                        advance,
                                        placement: SketchPlacement {
                                            calibration: work.calibration,
                                            ink_box_px: settings.ink_box_px,
                                        },
                                        candidates: settings.candidates,
                                        temperature: settings.temperature,
                                        seed: settings.seed,
                                        timeout: Duration::from_secs(settings.timeout_seconds),
                                    },
                                    &work.cancelled,
                                )
                                .map(|candidate| TraceOutput::LocalSketch(Box::new(candidate)))
                            }
                        })
                };
                let _ = work.events.send(TraceEvent::Finished(result));
                GLOBAL_TRACE_JOBS.fetch_sub(1, Ordering::AcqRel);
            }
        });
        sender
    })
}

fn submit_worker(work: TraceWork) -> Result<(), String> {
    let mut current = GLOBAL_TRACE_JOBS.load(Ordering::Acquire);
    loop {
        if current >= MAX_GLOBAL_TRACE_JOBS {
            return Err("global image candidate worker capacity is full".into());
        }
        match GLOBAL_TRACE_JOBS.compare_exchange_weak(
            current,
            current + 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
    match worker().try_send(work) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
            GLOBAL_TRACE_JOBS.fetch_sub(1, Ordering::AcqRel);
            Err("global image candidate worker queue is unavailable".into())
        }
    }
}

// The trace and calibration remain graph parameters. The normal script worker turns them into
// one guarded replacement; the existing proof and Apply paths remain separate.
const CALIBRATED_TRACE_RECIPE: &str = r#"import json
import sys

data = json.load(sys.stdin)
trace = data["parameters"]["calibrated_trace"]
layers = data["layers"]
if len(layers) != 1 or layers[0]["guard"] != trace["target"]:
    raise ValueError("calibrated trace target changed; capture the layer again")
result = {
    "schema_version": data["schema_version"],
    "job_id": data["job_id"],
    "input_hash": data["input_hash"],
    "report": "Calibrated trace " + trace["image_sha256"],
    "reads": [],
    "edits": [{"target": trace["target"], "operations": [{
        "op": "replace_contours", "contours": trace["contours"]
    }]}],
}
json.dump(result, sys.stdout)
"#;

const LOCAL_SKETCH_RECIPE: &str = r#"import json
import sys

data = json.load(sys.stdin)
candidate = data["parameters"]["local_sketch"]
layers = data["layers"]
if len(layers) != 1 or layers[0]["guard"] != candidate["target"]:
    raise ValueError("local sketch target changed; capture the layer again")
result = {
    "schema_version": data["schema_version"],
    "job_id": data["job_id"],
    "input_hash": data["input_hash"],
    "report": "Unreviewed local sketch " + candidate["image_sha256"],
    "reads": [],
    "edits": [{"target": candidate["target"], "operations": [{
        "op": "replace_contours", "contours": candidate["contours"]
    }]}],
}
json.dump(result, sys.stdout)
"#;

struct TraceIntent {
    epoch: String,
    document_revision: u64,
    graph: GraphGuard,
    source: usize,
    target: AgentLayerGuard,
    references: Vec<GradingReferenceRequest>,
    grading: GradingContext,
    node: u32,
    actor: String,
    operation_key: String,
}

/// One retained handle. The terminal result remains until explicit release.
pub(super) struct TraceSession {
    pub(super) handle: u64,
    actor: String,
    operation_key: String,
    payload_sha256: String,
    intent: TraceIntent,
    backend: NodesTraceBackend,
    local_sketch_runtime: Option<SketchRuntimeIdentity>,
    cancelled: ProcessCancellation,
    events: Receiver<TraceEvent>,
    phase: NodesTracePhase,
    mutation: Option<GraphMutationResponse>,
    error: Option<String>,
}

impl TraceSession {
    pub(super) fn needs_pump(&self) -> bool {
        matches!(
            self.phase,
            NodesTracePhase::Queued | NodesTracePhase::Running | NodesTracePhase::Cancelling
        )
    }

    fn terminal(&self) -> bool {
        !self.needs_pump()
    }

    fn status(&self) -> NodesTraceStatusResult {
        NodesTraceStatusResult {
            ok: true,
            handle: self.handle,
            phase: self.phase,
            backend: self.backend,
            local_sketch_runtime: self.local_sketch_runtime.clone(),
            grading: self.intent.grading.clone(),
            mutation: self.mutation.clone(),
            error: self.error.clone(),
            root_changed: false,
        }
    }
}

fn host_sketch_runtime() -> Result<SketchRuntime, String> {
    let repository = std::env::var_os("RUNEBENDER_SKETCH_REPOSITORY")
        .filter(|value| !value.is_empty())
        .ok_or("configure RUNEBENDER_SKETCH_REPOSITORY for local sketch inference")?;
    let checkpoint = std::env::var_os("RUNEBENDER_SKETCH_CHECKPOINT")
        .filter(|value| !value.is_empty())
        .ok_or("configure a concrete RUNEBENDER_SKETCH_CHECKPOINT")?;
    let script_home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .ok_or("HOME is unavailable for the installed img2bez helper")?;
    let repository = PathBuf::from(repository);
    Ok(SketchRuntime {
        python: repository.join(".venv/bin/python"),
        repository,
        checkpoint: PathBuf::from(checkpoint),
        script_home: PathBuf::from(script_home),
    })
}

fn request_digest(request: &NodesTraceRequest) -> Result<String, String> {
    let bytes = serde_json::to_vec(request).map_err(|error| error.to_string())?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn validate_target(
    project: &runebender::font::project::Project,
    intent: &TraceIntent,
) -> Result<(), String> {
    let source = SourceId(intent.source);
    let glyph = project
        .document_glyph(&intent.target.glyph)
        .ok_or("trace target glyph is absent")?;
    if glyph.id().to_wire() != intent.target.glyph_id {
        return Err("trace target glyph identity changed".into());
    }
    let address = GlyphLayerAddress {
        glyph: intent.target.glyph.clone(),
        layer: LayerId {
            source,
            name: intent.target.layer.clone(),
        },
    };
    let layer = project
        .capture_document_layer(&address)
        .ok_or("trace target layer is absent")?;
    if canonical_glyph_revision(layer.view())? != intent.target.expected_revision {
        return Err("trace target layer revision changed".into());
    }
    Ok(())
}

impl Workspace {
    pub(super) fn handle_trace_call(&mut self, call: &ToolCall) -> Result<Value, String> {
        match call.name.as_str() {
            "nodes_trace" => {
                let request: NodesTraceRequest = serde_json::from_value(call.arguments.clone())
                    .map_err(|error| error.to_string())?;
                request.validate()?;
                let backend = if request.local_sketch.is_some() {
                    NodesTraceBackend::LocalSketch
                } else {
                    NodesTraceBackend::CalibratedTrace
                };
                let digest = request_digest(&request)?;
                let state = self
                    .live_nodes
                    .as_mut()
                    .ok_or("live graph is unavailable")?;
                let key = (request.actor.clone(), request.operation_key.clone());
                if let Some((retained_digest, handle)) = state.trace_receipts.get(&key) {
                    if retained_digest != &digest {
                        return Err("trace operation_key already has a different request".into());
                    }
                    if state
                        .trace
                        .as_ref()
                        .is_none_or(|trace| trace.handle != *handle)
                    {
                        return Ok(json!(NodesTraceStartResult {
                            ok: true,
                            handle: *handle,
                            phase: NodesTracePhase::Released,
                            backend,
                            replayed: true,
                            root_changed: false,
                        }));
                    }
                }
                if let Some(existing) = &state.trace {
                    if existing.actor == request.actor
                        && existing.operation_key == request.operation_key
                    {
                        if existing.payload_sha256 != digest {
                            return Err(
                                "trace operation_key already has a different request".into()
                            );
                        }
                        return Ok(json!(NodesTraceStartResult {
                            ok: true,
                            handle: existing.handle,
                            phase: existing.phase,
                            backend: existing.backend,
                            replayed: true,
                            root_changed: false,
                        }));
                    }
                    return Err(
                        "release the retained image candidate before submitting another".into(),
                    );
                }
                let snapshot = state.session.snapshot();
                if snapshot.identity != request.guard.identity
                    || snapshot.revision != request.guard.revision
                {
                    return Err("graph changed after the image candidate request".into());
                }
                if !snapshot.graph.nodes.iter().any(|node| {
                    node.type_name == "live.font"
                        && node.values.get("source").and_then(Value::as_u64)
                            == u64::try_from(request.source).ok()
                }) {
                    return Err("trace source does not match the live graph".into());
                }
                if snapshot
                    .graph
                    .node(request.node)
                    .is_none_or(|node| node.type_name != "live.python")
                {
                    return Err("select an existing live.python candidate node".into());
                }
                let grading = capture_replacement_context(
                    &self.font.project,
                    SourceId(request.source),
                    &request.target,
                    &request.references,
                )?;
                let intent = TraceIntent {
                    epoch: request.expected_document_epoch,
                    document_revision: self.font.project.document_revision(),
                    graph: request.guard,
                    source: request.source,
                    target: request.target,
                    references: request.references,
                    grading,
                    node: request.node,
                    actor: request.actor.clone(),
                    operation_key: request.operation_key.clone(),
                };
                validate_target(&self.font.project, &intent)?;
                let (sender, events) = mpsc::channel();
                let cancelled = ProcessCancellation::default();
                let handle = state.next_trace_id;
                let next_handle = handle
                    .checked_add(1)
                    .ok_or("trace handle space exhausted")?;
                if state.trace_receipts.len() >= 64 {
                    return Err("trace retry retention is full; open a new graph session".into());
                }
                let worker_backend = match request.local_sketch {
                    Some(settings) => TraceWorkBackend::LocalSketch {
                        runtime: host_sketch_runtime()?,
                        settings,
                        glyph: intent.target.glyph.clone(),
                        advance: intent.grading.target.advance,
                    },
                    None => TraceWorkBackend::Calibrated {
                        invert: request.invert,
                    },
                };
                submit_worker(TraceWork {
                    image_base64: request.image_base64,
                    calibration: request.calibration,
                    backend: worker_backend,
                    cancelled: cancelled.clone(),
                    events: sender,
                })?;
                state.next_trace_id = next_handle;
                state.trace_receipts.insert(key, (digest.clone(), handle));
                state.trace = Some(TraceSession {
                    handle,
                    actor: request.actor,
                    operation_key: request.operation_key,
                    payload_sha256: digest,
                    intent,
                    backend,
                    local_sketch_runtime: None,
                    cancelled,
                    events,
                    phase: NodesTracePhase::Queued,
                    mutation: None,
                    error: None,
                });
                Ok(json!(NodesTraceStartResult {
                    ok: true,
                    handle,
                    phase: NodesTracePhase::Queued,
                    backend,
                    replayed: false,
                    root_changed: false,
                }))
            }
            "nodes_trace_status" | "nodes_trace_cancel" | "nodes_trace_release" => {
                let request: NodesTraceHandleRequest =
                    serde_json::from_value(call.arguments.clone())
                        .map_err(|error| error.to_string())?;
                request.validate()?;
                let state = self
                    .live_nodes
                    .as_mut()
                    .ok_or("live graph is unavailable")?;
                let trace = state.trace.as_mut().ok_or("trace handle is not retained")?;
                if trace.handle != request.handle || trace.intent.graph.identity != request.identity
                {
                    return Err("trace handle belongs to another graph session".into());
                }
                if call.name == "nodes_trace_cancel" && !trace.terminal() {
                    trace.cancelled.cancel();
                    trace.phase = NodesTracePhase::Cancelling;
                }
                if call.name == "nodes_trace_release" {
                    if !trace.terminal() {
                        return Err(
                            "trace is still queued or running; cancel and wait for settlement"
                                .into(),
                        );
                    }
                    state.trace = None;
                    return Ok(json!(NodesTraceReleaseResult {
                        ok: true,
                        handle: request.handle,
                        root_changed: false,
                    }));
                }
                Ok(json!(trace.status()))
            }
            _ => Err("unknown image candidate command".into()),
        }
    }

    pub(super) fn poll_live_trace(&mut self) {
        let Some(state) = self.live_nodes.as_mut() else {
            return;
        };
        let Some(mut trace) = state.trace.take() else {
            return;
        };
        while trace.needs_pump() {
            match trace.events.try_recv() {
                Ok(TraceEvent::Started) => {
                    if !trace.cancelled.is_cancelled() {
                        trace.phase = NodesTracePhase::Running;
                    }
                }
                Ok(TraceEvent::Pinned(runtime)) => {
                    trace.local_sketch_runtime = Some(runtime);
                }
                Ok(TraceEvent::Finished(result)) => {
                    if trace.cancelled.is_cancelled() {
                        trace.phase = NodesTracePhase::Cancelled;
                    } else {
                        match result {
                            Ok(result) => match publish_trace(
                                &self.font.project,
                                self.live.as_ref().map(|live| live.document_epoch()),
                                &mut state.session,
                                &trace.intent,
                                result,
                            ) {
                                Ok(mutation) => {
                                    trace.mutation = Some(mutation);
                                    trace.phase = NodesTracePhase::Completed;
                                }
                                Err((stale, message)) => {
                                    trace.phase = if stale {
                                        NodesTracePhase::Stale
                                    } else {
                                        NodesTracePhase::Failed
                                    };
                                    trace.error = Some(message);
                                }
                            },
                            Err(error) => {
                                trace.phase = NodesTracePhase::Failed;
                                trace.error = Some(format!("{:?}: {error}", trace.backend));
                            }
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    trace.phase = NodesTracePhase::Failed;
                    trace.error = Some("candidate worker disconnected".into());
                }
            }
        }
        state.trace = Some(trace);
    }
}

fn publish_trace(
    project: &runebender::font::project::Project,
    current_epoch: Option<&str>,
    session: &mut runebender::workflows::nodes_session::GraphSession,
    intent: &TraceIntent,
    output: impl Into<TraceOutput>,
) -> Result<GraphMutationResponse, (bool, String)> {
    let stale = |message: &str| (true, message.to_owned());
    if current_epoch != Some(intent.epoch.as_str())
        || project.document_revision() != intent.document_revision
    {
        return Err(stale("document changed before image candidate publication"));
    }
    let snapshot = session.snapshot();
    if snapshot.identity != intent.graph.identity || snapshot.revision != intent.graph.revision {
        return Err(stale("graph changed before image candidate publication"));
    }
    if !snapshot.graph.nodes.iter().any(|node| {
        node.type_name == "live.font"
            && node.values.get("source").and_then(Value::as_u64)
                == u64::try_from(intent.source).ok()
    }) || snapshot
        .graph
        .node(intent.node)
        .is_none_or(|node| node.type_name != "live.python")
    {
        return Err(stale("trace source or candidate node changed"));
    }
    validate_target(project, intent).map_err(|error| (true, error))?;
    let current_grading = capture_replacement_context(
        project,
        SourceId(intent.source),
        &intent.target,
        &intent.references,
    )
    .map_err(|error| (true, error))?;
    if current_grading != intent.grading {
        return Err(stale("graded target or reference geometry changed"));
    }
    let (operation, parameter_name, mut candidate_value, code, history_name) = match output.into() {
        TraceOutput::Calibrated(trace) => (
            AgentEditOperation::from_calibrated_trace(&trace),
            "calibrated_trace",
            serde_json::to_value(&trace).map_err(|error| (false, error.to_string()))?,
            CALIBRATED_TRACE_RECIPE,
            "Calibrated trace candidate",
        ),
        TraceOutput::LocalSketch(candidate) => {
            let contours = candidate.contours.clone();
            let value = json!({
                "image_sha256": candidate.image_sha256,
                "image_size_px": candidate.image_size_px,
                "calibration": candidate.placement.calibration,
                "ink_box_px": candidate.placement.ink_box_px,
                "runtime": candidate.runtime,
                "codepoint": candidate.codepoint,
                "candidates": candidate.candidates,
                "temperature": candidate.temperature,
                "seed": candidate.seed,
                "script_score_not_visual_approval": candidate.script_score,
                "model_input_sha256": candidate.model_input_sha256,
                "model_input_tracer": candidate.model_input_tracer,
                "contours": candidate.contours,
            });
            (
                AgentEditOperation::ReplaceContours { contours },
                "local_sketch",
                value,
                LOCAL_SKETCH_RECIPE,
                "Unreviewed local sketch candidate",
            )
        }
    };
    let request = AgentEditRequest {
        expected_document_epoch: intent.epoch.clone(),
        actor: intent.actor.clone(),
        operation_key: intent.operation_key.clone(),
        authorization: "preview-only".into(),
        source: intent.source,
        history_name: history_name.into(),
        reads: Vec::new(),
        edits: vec![AgentLayerEdits {
            target: intent.target.clone(),
            operations: vec![operation],
        }],
    };
    request
        .stage(project)
        .map_err(|error| (true, format!("{error:?}")))?;
    candidate_value["target"] =
        serde_json::to_value(&intent.target).map_err(|error| (false, error.to_string()))?;
    candidate_value["grading"] =
        serde_json::to_value(&intent.grading).map_err(|error| (false, error.to_string()))?;
    let mut parameters = serde_json::Map::new();
    parameters.insert(parameter_name.into(), candidate_value);
    let parameters = Value::Object(parameters);
    if serde_json::to_vec(&parameters)
        .map_err(|error| (false, error.to_string()))?
        .len()
        > 64 * 1024
    {
        return Err((
            false,
            "generated candidate exceeds live Python parameter bounds".into(),
        ));
    }
    session
        .mutate(GraphMutationRequest {
            guard: intent.graph.clone(),
            actor: intent.actor.clone(),
            operation_key: intent.operation_key.clone(),
            mutation: GraphMutation::Patch {
                edits: vec![
                    GraphEdit::SetValue {
                        node: intent.node,
                        field: "code".into(),
                        value: json!(code),
                    },
                    GraphEdit::SetValue {
                        node: intent.node,
                        field: "parameters".into(),
                        value: parameters,
                    },
                ],
            },
        })
        .map_err(|error| (false, error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::font_model::FontModel;
    use runebender::font::model::glyph_metadata::MarkColor;
    use runebender::font::project::Project;
    use runebender::outline::drawing::{DrawingContour, DrawingPoint, DrawingPointType};
    use runebender::workflows::nodes::Registry;
    use runebender::workflows::nodes_live;
    use runebender::workflows::nodes_session::{GraphInteractiveMutationRequest, GraphSession};

    fn fixture() -> (Project, GraphSession, TraceIntent, CalibratedTrace) {
        let mut project = Project::new_font(std::env::temp_dir().join("trace-guard-unsaved.ufo"));
        project
            .add_document_glyph("A", 400.0, Some(u32::from('A')))
            .unwrap();
        project
            .add_document_glyph("G", 400.0, Some(u32::from('G')))
            .unwrap();
        project
            .add_document_glyph("ReferenceBase", 400.0, None)
            .unwrap();
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        for (glyph, label) in [("A", "red"), ("G", "green")] {
            let color =
                MarkColor::parse(&runebender::ui::theme::ufo_rgba_for_label(label).unwrap())
                    .unwrap();
            project
                .edit_document_layer(glyph, &layer, |draft| {
                    draft.set_mark(Some(label), Some(color))?;
                    Ok(())
                })
                .unwrap();
        }
        project
            .edit_document_layer("G", &layer, |draft| {
                draft.add_shape_contour(kurbo::Rect::new(20.0, 0.0, 90.0, 110.0), false)?;
                draft.add_component("ReferenceBase".into(), kurbo::Affine::IDENTITY)?;
                Ok(())
            })
            .unwrap();
        project
            .edit_document_layer("ReferenceBase", &layer, |draft| {
                draft.add_shape_contour(kurbo::Rect::new(10.0, 0.0, 40.0, 50.0), false)?;
                Ok(())
            })
            .unwrap();
        let address = GlyphLayerAddress {
            glyph: "A".into(),
            layer: layer.clone(),
        };
        let target = AgentLayerGuard {
            glyph: "A".into(),
            glyph_id: project.document_glyph("A").unwrap().id().to_wire(),
            layer: layer.name.clone(),
            expected_revision: canonical_glyph_revision(
                project.capture_document_layer(&address).unwrap().view(),
            )
            .unwrap(),
        };
        let reference = AgentLayerGuard {
            glyph: "G".into(),
            glyph_id: project.document_glyph("G").unwrap().id().to_wire(),
            layer: layer.name.clone(),
            expected_revision: canonical_glyph_revision(
                project
                    .capture_document_layer(&GlyphLayerAddress {
                        glyph: "G".into(),
                        layer: layer.clone(),
                    })
                    .unwrap()
                    .view(),
            )
            .unwrap(),
        };
        let references = vec![GradingReferenceRequest {
            guard: reference,
            rationale: "Approved stroke weight for this Arabic construction".into(),
        }];
        let grading = capture_replacement_context(&project, source, &target, &references).unwrap();
        let session = GraphSession::new(
            "trace-test",
            "test-epoch",
            nodes_live::comparison_starter(source),
            Registry::core(),
        )
        .unwrap();
        let snapshot = session.snapshot();
        let node = snapshot
            .graph
            .nodes
            .iter()
            .find(|node| node.type_name == "live.python")
            .unwrap()
            .id;
        let intent = TraceIntent {
            epoch: "test-epoch".into(),
            document_revision: project.document_revision(),
            graph: GraphGuard {
                identity: snapshot.identity,
                revision: snapshot.revision,
            },
            source: source.0,
            target,
            references,
            grading,
            node,
            actor: "trace-test".into(),
            operation_key: "one".into(),
        };
        let trace = CalibratedTrace {
            image_sha256: "sha256:test".into(),
            image_width_px: 16,
            image_height_px: 16,
            calibration: TraceCalibration {
                font_units_per_pixel: 2.0,
                pixel_baseline_y: 12.0,
                font_x_at_left: 0.0,
                font_baseline_y: 0.0,
            },
            invert: false,
            tracer: "test".into(),
            cargo_lock_sha256: "sha256:test".into(),
            contours: vec![DrawingContour {
                points: [(20.0, 0.0), (80.0, 0.0), (80.0, 100.0), (20.0, 100.0)]
                    .into_iter()
                    .map(|(x, y)| DrawingPoint {
                        x,
                        y,
                        kind: DrawingPointType::Line,
                        smooth: false,
                    })
                    .collect(),
            }],
        };
        (project, session, intent, trace)
    }

    fn sketch_candidate(trace: CalibratedTrace) -> SketchCandidate {
        SketchCandidate {
            runtime: SketchRuntimeIdentity {
                python_path: "/tmp/fake-venv/bin/python".into(),
                python_resolved_path: "/tmp/fake-python".into(),
                python_sha256: "sha256:python".into(),
                repository: "/tmp/fake-model".into(),
                modules_sha256: "sha256:modules".into(),
                checkpoint: "/tmp/fake-model/runs/one".into(),
                checkpoint_sha256: "sha256:weights".into(),
                script_home: "/tmp/fake-home".into(),
                img2bez_path: "/tmp/fake-home/.cargo/bin/img2bez".into(),
                img2bez_sha256: "sha256:tracer".into(),
                launcher_sha256: "sha256:launcher".into(),
                tracer_cargo_lock_sha256: "sha256:lockfile".into(),
            },
            image_sha256: trace.image_sha256,
            image_size_px: [trace.image_width_px, trace.image_height_px],
            placement: SketchPlacement {
                calibration: trace.calibration,
                ink_box_px: [2, 2, 12, 12],
            },
            codepoint: None,
            candidates: 1,
            temperature: 0.0,
            seed: 7,
            script_score: -17.0,
            model_input_sha256: "sha256:pretrace".into(),
            model_input_tracer: "img2bez-crate/clean/grid2/full-image-v1".into(),
            contours: trace.contours,
        }
    }

    #[test]
    fn unreviewed_local_sketch_publishes_once_with_model_provenance_and_no_font_change() {
        let (project, mut session, intent, trace) = fixture();
        let original_revision = project.document_revision();
        let candidate = sketch_candidate(trace);
        let result = publish_trace(
            &project,
            Some("test-epoch"),
            &mut session,
            &intent,
            TraceOutput::LocalSketch(Box::new(candidate)),
        )
        .unwrap();
        assert!(result.receipt.changed);
        assert_eq!(project.document_revision(), original_revision);
        let graph = session.snapshot();
        let node = graph.graph.node(intent.node).unwrap();
        let model = &node.values["parameters"]["local_sketch"];
        assert_eq!(model["runtime"]["checkpoint_sha256"], "sha256:weights");
        assert_eq!(model["runtime"]["launcher_sha256"], "sha256:launcher");
        assert_eq!(model["model_input_sha256"], "sha256:pretrace");
        assert_eq!(model["script_score_not_visual_approval"], -17.0);
        assert_eq!(model["target"]["glyph"], "A");
        assert_eq!(model["grading"]["references"][0]["layer"]["grade"], "green");
        assert!(node.values["parameters"].get("calibrated_trace").is_none());
        let retry = publish_trace(
            &project,
            Some("test-epoch"),
            &mut session,
            &intent,
            TraceOutput::LocalSketch(Box::new(sketch_candidate(fixture().3))),
        );
        assert!(retry.unwrap_err().0);
        assert_eq!(session.snapshot(), graph);
    }

    #[test]
    fn publication_checks_document_layer_and_graph_then_changes_graph_once() {
        let (project, mut session, intent, trace) = fixture();
        let original = session.snapshot();
        assert!(
            publish_trace(
                &project,
                Some("wrong-epoch"),
                &mut session,
                &intent,
                trace.clone()
            )
            .unwrap_err()
            .0
        );
        assert_eq!(session.snapshot(), original);

        let (mut changed_project, mut session, intent, trace) = fixture();
        assert!(
            changed_project
                .document_glyph("traceStaleFixture")
                .is_none()
        );
        let add = changed_project
            .begin_add_glyph("traceStaleFixture", 400.0, None)
            .unwrap();
        changed_project.commit_glyph_transaction(add).unwrap();
        assert_ne!(
            changed_project.document_revision(),
            intent.document_revision
        );
        assert!(
            publish_trace(
                &changed_project,
                Some("test-epoch"),
                &mut session,
                &intent,
                trace.clone(),
            )
            .unwrap_err()
            .0
        );
        assert_eq!(session.snapshot(), original);

        let (mut changed_project, mut session, mut intent, trace) = fixture();
        let layer = changed_project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        changed_project
            .edit_document_layer("A", &layer, |draft| {
                draft.set_width(450.0)?;
                Ok(())
            })
            .unwrap();
        intent.document_revision = changed_project.document_revision();
        assert!(
            publish_trace(
                &changed_project,
                Some("test-epoch"),
                &mut session,
                &intent,
                trace.clone(),
            )
            .unwrap_err()
            .0
        );
        assert_eq!(session.snapshot(), original);

        let (mut changed_project, mut session, intent, trace) = fixture();
        let layer = changed_project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        changed_project
            .edit_document_layer("ReferenceBase", &layer, |draft| {
                draft.add_shape_contour(kurbo::Rect::new(50.0, 0.0, 70.0, 50.0), false)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            canonical_glyph_revision(changed_project.document_layer("G", &layer).unwrap()).unwrap(),
            intent.references[0].guard.expected_revision,
            "the reference layer's shallow guard remains unchanged"
        );
        let updated = capture_replacement_context(
            &changed_project,
            SourceId(0),
            &intent.target,
            &intent.references,
        )
        .unwrap();
        assert_ne!(
            updated.references[0].layer.resolved_outline_sha256,
            intent.grading.references[0].layer.resolved_outline_sha256
        );
        assert!(
            publish_trace(
                &changed_project,
                Some("test-epoch"),
                &mut session,
                &intent,
                trace,
            )
            .unwrap_err()
            .0
        );
        assert_eq!(session.snapshot(), original);

        let (project, mut session, intent, trace) = fixture();
        session
            .mutate_interactive(GraphInteractiveMutationRequest {
                guard: intent.graph.clone(),
                mutation: GraphMutation::Patch {
                    edits: vec![GraphEdit::SetValue {
                        node: intent.node,
                        field: "code".into(),
                        value: json!("print('changed')"),
                    }],
                },
            })
            .unwrap();
        let graph_changed = session.snapshot();
        assert!(
            publish_trace(&project, Some("test-epoch"), &mut session, &intent, trace)
                .unwrap_err()
                .0
        );
        assert_eq!(session.snapshot(), graph_changed);

        let (project, mut session, intent, trace) = fixture();
        let published = publish_trace(&project, Some("test-epoch"), &mut session, &intent, trace)
            .expect("first publication");
        assert!(published.receipt.changed);
        let completed = session.snapshot();
        let retry = publish_trace(
            &project,
            Some("test-epoch"),
            &mut session,
            &intent,
            fixture().3,
        );
        assert!(retry.unwrap_err().0);
        assert_eq!(session.snapshot(), completed);
    }

    #[test]
    fn queued_and_running_cancellation_suppress_completed_worker_results() {
        for initial_phase in [NodesTracePhase::Queued, NodesTracePhase::Running] {
            let (project, _, mut intent, trace) = fixture();
            let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
            app.ensure_live_graph().unwrap();
            let graph = app.live_graph_session().unwrap().snapshot();
            intent.epoch = app.live.as_ref().unwrap().document_epoch().into();
            intent.graph = GraphGuard {
                identity: graph.identity.clone(),
                revision: graph.revision,
            };
            intent.document_revision = app.font.project.document_revision();
            intent.node = graph
                .graph
                .nodes
                .iter()
                .find(|node| node.type_name == "live.python")
                .unwrap()
                .id;
            let (events, receiver) = mpsc::channel();
            app.live_nodes.as_mut().unwrap().trace = Some(TraceSession {
                handle: 1,
                actor: "trace-test".into(),
                operation_key: "one".into(),
                payload_sha256: "sha256:test".into(),
                intent,
                backend: NodesTraceBackend::CalibratedTrace,
                local_sketch_runtime: None,
                cancelled: ProcessCancellation::default(),
                events: receiver,
                phase: initial_phase,
                mutation: None,
                error: None,
            });
            let epoch = app.live.as_ref().unwrap().document_epoch().to_owned();
            let handle_arguments = json!({
                "expected_document_epoch":epoch,
                "identity":graph.identity.clone(),
                "handle":1,
            });
            assert!(
                app.handle_trace_call(&ToolCall {
                    name: "nodes_trace_release".into(),
                    arguments: handle_arguments.clone(),
                })
                .is_err()
            );
            let cancel = app
                .handle_trace_call(&ToolCall {
                    name: "nodes_trace_cancel".into(),
                    arguments: handle_arguments.clone(),
                })
                .unwrap();
            assert_eq!(cancel["phase"], "cancelling");
            assert!(
                app.handle_trace_call(&ToolCall {
                    name: "nodes_trace_release".into(),
                    arguments: handle_arguments.clone(),
                })
                .is_err()
            );
            events.send(TraceEvent::Finished(Ok(trace.into()))).unwrap();
            app.poll_live_trace();
            assert_eq!(
                app.live_nodes
                    .as_ref()
                    .unwrap()
                    .trace
                    .as_ref()
                    .unwrap()
                    .phase,
                NodesTracePhase::Cancelled
            );
            assert_eq!(app.live_graph_session().unwrap().snapshot(), graph);
            assert_eq!(
                app.handle_trace_call(&ToolCall {
                    name: "nodes_trace_release".into(),
                    arguments: handle_arguments,
                })
                .unwrap()["ok"],
                true
            );
        }
    }

    #[test]
    fn local_sketch_cancellation_retains_pin_but_never_publishes() {
        let (project, _, mut intent, trace) = fixture();
        let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
        app.ensure_live_graph().unwrap();
        let graph = app.live_graph_session().unwrap().snapshot();
        intent.epoch = app.live.as_ref().unwrap().document_epoch().into();
        intent.graph = GraphGuard {
            identity: graph.identity.clone(),
            revision: graph.revision,
        };
        intent.document_revision = app.font.project.document_revision();
        intent.node = graph
            .graph
            .nodes
            .iter()
            .find(|node| node.type_name == "live.python")
            .unwrap()
            .id;
        let candidate = sketch_candidate(trace);
        let runtime = candidate.runtime.clone();
        let (events, receiver) = mpsc::channel();
        app.live_nodes.as_mut().unwrap().trace = Some(TraceSession {
            handle: 11,
            actor: "sketch-test".into(),
            operation_key: "one".into(),
            payload_sha256: "sha256:test".into(),
            intent,
            backend: NodesTraceBackend::LocalSketch,
            local_sketch_runtime: None,
            cancelled: ProcessCancellation::default(),
            events: receiver,
            phase: NodesTracePhase::Running,
            mutation: None,
            error: None,
        });
        let arguments = json!({
            "expected_document_epoch":app.live.as_ref().unwrap().document_epoch(),
            "identity":graph.identity,"handle":11,
        });
        let cancelled = app
            .handle_trace_call(&ToolCall {
                name: "nodes_trace_cancel".into(),
                arguments: arguments.clone(),
            })
            .unwrap();
        assert_eq!(cancelled["phase"], "cancelling");
        events.send(TraceEvent::Pinned(runtime)).unwrap();
        events
            .send(TraceEvent::Finished(Ok(TraceOutput::LocalSketch(
                Box::new(candidate),
            ))))
            .unwrap();
        app.poll_live_trace();
        let status = app
            .handle_trace_call(&ToolCall {
                name: "nodes_trace_status".into(),
                arguments,
            })
            .unwrap();
        assert_eq!(status["backend"], "local_sketch");
        assert_eq!(status["phase"], "cancelled");
        assert_eq!(
            status["local_sketch_runtime"]["checkpoint_sha256"],
            "sha256:weights"
        );
        assert_eq!(app.live_graph_session().unwrap().snapshot(), graph);
    }

    #[test]
    fn released_retry_is_distinct_from_a_new_active_trace() {
        let (project, _, mut intent, _) = fixture();
        let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
        app.ensure_live_graph().unwrap();
        let graph = app.live_graph_session().unwrap().snapshot();
        let epoch = app.live.as_ref().unwrap().document_epoch().to_owned();
        let request = NodesTraceRequest {
            expected_document_epoch: epoch.clone(),
            guard: GraphGuard {
                identity: graph.identity.clone(),
                revision: graph.revision,
            },
            actor: "trace-test".into(),
            operation_key: "old".into(),
            node: intent.node,
            source: intent.source,
            target: intent.target.clone(),
            references: intent.references.clone(),
            image_base64: "AA==".into(),
            calibration: TraceCalibration {
                font_units_per_pixel: 2.0,
                pixel_baseline_y: 12.0,
                font_x_at_left: 0.0,
                font_baseline_y: 0.0,
            },
            invert: false,
            local_sketch: None,
        };
        let digest = request_digest(&request).unwrap();
        let (sender, events) = mpsc::channel();
        intent.epoch = epoch;
        intent.graph = request.guard.clone();
        intent.operation_key = "new".into();
        let state = app.live_nodes.as_mut().unwrap();
        state
            .trace_receipts
            .insert(("trace-test".into(), "old".into()), (digest, 7));
        state.trace = Some(TraceSession {
            handle: 8,
            actor: "trace-test".into(),
            operation_key: "new".into(),
            payload_sha256: "sha256:new".into(),
            intent,
            backend: NodesTraceBackend::CalibratedTrace,
            local_sketch_runtime: None,
            cancelled: ProcessCancellation::default(),
            events,
            phase: NodesTracePhase::Queued,
            mutation: None,
            error: None,
        });
        let replay = app
            .handle_trace_call(&ToolCall {
                name: "nodes_trace".into(),
                arguments: serde_json::to_value(&request).unwrap(),
            })
            .unwrap();
        assert_eq!(replay["phase"], "released");
        assert_eq!(replay["handle"], 7);
        assert_eq!(replay["replayed"], true);
        let mut changed = request;
        changed.calibration.font_x_at_left = 10.0;
        assert!(
            app.handle_trace_call(&ToolCall {
                name: "nodes_trace".into(),
                arguments: serde_json::to_value(changed).unwrap(),
            })
            .is_err()
        );
        assert_eq!(app.live_graph_session().unwrap().snapshot(), graph);
        drop(sender);
    }
}
