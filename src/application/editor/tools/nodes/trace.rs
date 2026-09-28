// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bounded off-editor calibrated tracing for the existing live Nodes graph.
//!
//! A single process-wide worker decodes and traces at most two admitted images.
//! Workspace retains only one trace receipt per graph session. A finished trace becomes graph
//! intent only after the captured document, source, layer and graph guards are checked again.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, OnceLock};

use base64::Engine as _;
use runebender::automation::agent::ToolCall;
use runebender::automation::agent_edit::{
    AgentEditOperation, AgentEditRequest, AgentLayerEdits, AgentLayerGuard,
};
use runebender::automation::agent_nodes::results::{
    NodesTracePhase, NodesTraceReleaseResult, NodesTraceStartResult, NodesTraceStatusResult,
};
use runebender::automation::agent_nodes::{NodesTraceHandleRequest, NodesTraceRequest};
use runebender::font::edit_batch::canonical_glyph_revision;
use runebender::font::variable::{GlyphLayerAddress, LayerId, SourceId};
use runebender::formats::image_trace::{CalibratedTrace, TraceCalibration, trace_image_calibrated};
use runebender::workflows::nodes_session::{
    GraphEdit, GraphGuard, GraphMutation, GraphMutationRequest, GraphMutationResponse,
};
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
    invert: bool,
    cancelled: Arc<AtomicBool>,
    events: mpsc::Sender<TraceEvent>,
}

enum TraceEvent {
    Started,
    Finished(Result<CalibratedTrace, String>),
}

fn worker() -> &'static SyncSender<TraceWork> {
    TRACE_WORKER.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel::<TraceWork>(MAX_GLOBAL_TRACE_JOBS - 1);
        std::thread::spawn(move || {
            while let Ok(work) = receiver.recv() {
                if !work.cancelled.load(Ordering::Acquire) {
                    let _ = work.events.send(TraceEvent::Started);
                }
                let result = if work.cancelled.load(Ordering::Acquire) {
                    Err("trace cancelled before decoding".into())
                } else {
                    base64::engine::general_purpose::STANDARD
                        .decode(&work.image_base64)
                        .map_err(|error| format!("invalid base64 trace image: {error}"))
                        .and_then(|image| {
                            trace_image_calibrated(&image, work.calibration, work.invert)
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
            return Err("global calibrated trace worker capacity is full".into());
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
            Err("global calibrated trace worker queue is unavailable".into())
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

struct TraceIntent {
    epoch: String,
    document_revision: u64,
    graph: GraphGuard,
    source: usize,
    target: AgentLayerGuard,
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
    cancelled: Arc<AtomicBool>,
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
            mutation: self.mutation.clone(),
            error: self.error.clone(),
            root_changed: false,
        }
    }
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
                            replayed: true,
                            root_changed: false,
                        }));
                    }
                    return Err(
                        "release the retained calibrated trace before submitting another".into(),
                    );
                }
                let snapshot = state.session.snapshot();
                if snapshot.identity != request.guard.identity
                    || snapshot.revision != request.guard.revision
                {
                    return Err("graph changed after the calibrated trace request".into());
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
                let intent = TraceIntent {
                    epoch: request.expected_document_epoch,
                    document_revision: self.font.project.document_revision(),
                    graph: request.guard,
                    source: request.source,
                    target: request.target,
                    node: request.node,
                    actor: request.actor.clone(),
                    operation_key: request.operation_key.clone(),
                };
                validate_target(&self.font.project, &intent)?;
                let (sender, events) = mpsc::channel();
                let cancelled = Arc::new(AtomicBool::new(false));
                let handle = state.next_trace_id;
                let next_handle = handle
                    .checked_add(1)
                    .ok_or("trace handle space exhausted")?;
                if state.trace_receipts.len() >= 64 {
                    return Err("trace retry retention is full; open a new graph session".into());
                }
                submit_worker(TraceWork {
                    image_base64: request.image_base64,
                    calibration: request.calibration,
                    invert: request.invert,
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
                    trace.cancelled.store(true, Ordering::Release);
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
            _ => Err("unknown calibrated trace command".into()),
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
                    if !trace.cancelled.load(Ordering::Acquire) {
                        trace.phase = NodesTracePhase::Running;
                    }
                }
                Ok(TraceEvent::Finished(result)) => {
                    if trace.cancelled.load(Ordering::Acquire) {
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
                                trace.error = Some(error);
                            }
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    trace.phase = NodesTracePhase::Failed;
                    trace.error = Some("calibrated trace worker disconnected".into());
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
    trace: CalibratedTrace,
) -> Result<GraphMutationResponse, (bool, String)> {
    let stale = |message: &str| (true, message.to_owned());
    if current_epoch != Some(intent.epoch.as_str())
        || project.document_revision() != intent.document_revision
    {
        return Err(stale(
            "document changed before calibrated trace publication",
        ));
    }
    let snapshot = session.snapshot();
    if snapshot.identity != intent.graph.identity || snapshot.revision != intent.graph.revision {
        return Err(stale("graph changed before calibrated trace publication"));
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
    let request = AgentEditRequest {
        expected_document_epoch: intent.epoch.clone(),
        actor: intent.actor.clone(),
        operation_key: intent.operation_key.clone(),
        authorization: "preview-only".into(),
        source: intent.source,
        history_name: "Calibrated trace candidate".into(),
        reads: Vec::new(),
        edits: vec![AgentLayerEdits {
            target: intent.target.clone(),
            operations: vec![AgentEditOperation::from_calibrated_trace(&trace)],
        }],
    };
    request
        .stage(project)
        .map_err(|error| (true, format!("{error:?}")))?;
    let mut trace_value =
        serde_json::to_value(&trace).map_err(|error| (false, error.to_string()))?;
    trace_value["target"] =
        serde_json::to_value(&intent.target).map_err(|error| (false, error.to_string()))?;
    let parameters = json!({"calibrated_trace":trace_value});
    if serde_json::to_vec(&parameters)
        .map_err(|error| (false, error.to_string()))?
        .len()
        > 64 * 1024
    {
        return Err((
            false,
            "calibrated trace exceeds live Python parameter bounds".into(),
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
                        value: json!(CALIBRATED_TRACE_RECIPE),
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
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        let address = GlyphLayerAddress {
            glyph: "A".into(),
            layer: layer.clone(),
        };
        let target = AgentLayerGuard {
            glyph: "A".into(),
            glyph_id: project.document_glyph("A").unwrap().id().to_wire(),
            layer: layer.name,
            expected_revision: canonical_glyph_revision(
                project.capture_document_layer(&address).unwrap().view(),
            )
            .unwrap(),
        };
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
                cancelled: Arc::new(AtomicBool::new(false)),
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
            events.send(TraceEvent::Finished(Ok(trace))).unwrap();
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
            image_base64: "AA==".into(),
            calibration: TraceCalibration {
                font_units_per_pixel: 2.0,
                pixel_baseline_y: 12.0,
                font_x_at_left: 0.0,
                font_baseline_y: 0.0,
            },
            invert: false,
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
            cancelled: Arc::new(AtomicBool::new(false)),
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
