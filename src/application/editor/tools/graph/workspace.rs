// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! One live graph shared by native authoring, Python execution and agent commands.
//! The existing disk graph workflow remains separate; these commands never save a font.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use runebender::automation::agent_graph::results::{
    GraphCancelResult, GraphDiscoverResult, GraphImageResult, GraphMutateResult,
    GraphReleaseResult, GraphRunResult, GraphSnapshotResult, GraphStatusResult,
};
use runebender::font::compiler::proof::CompiledProofRecipe;
use runebender::font::variable::SourceId;
use runebender::workflows::graph::Registry;
use runebender::workflows::graph_live;
use runebender::workflows::graph_session::{
    GraphDocumentState, GraphGuard, GraphInteractiveMutationRequest, GraphMutation, GraphRunHandle,
    GraphSession,
};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::execution::{LiveGraphExecution, LiveGraphPhase, LiveGraphProofOutputs};
use super::trace::TraceSession;
use crate::application::platform::graph_file::{self, LiveGraphFileMetadata};
use crate::application::platform::graph_proofs::{NodeProofInspection, NodeProofJobs};
use crate::application::workspace::Workspace;

static NEXT_GRAPH_SESSION: AtomicU64 = AtomicU64::new(1);

/// Per-document live graph, retained jobs and bounded transport retry responses.
pub(crate) struct LiveGraphState {
    pub(crate) session: GraphSession,
    pub(crate) execution: LiveGraphExecution,
    pub(crate) proofs: NodeProofJobs,
    pub(crate) handles: BTreeSet<GraphRunHandle>,
    pub(crate) requests: BTreeMap<(String, String), (Value, RetainedGraphResponse)>,
    pub(crate) next_job: u64,
    pub(crate) file: Option<LiveGraphFileMetadata>,
    pub(crate) saved_revision: Option<u64>,
    pub(super) trace: Option<TraceSession>,
    pub(super) trace_receipts: BTreeMap<(String, String), (String, u64)>,
    pub(super) next_trace_id: u64,
}

/// Retained transport retry body for graph runs or delegated font Apply.
#[derive(Clone)]
pub(crate) enum RetainedGraphResponse {
    Run(Box<GraphRunResult>),
    Apply(Value),
}

impl Workspace {
    /// Whether the live loop must keep waking: jobs to observe, or a render
    /// waiting for edits to settle.
    pub(crate) fn live_graph_wants_wake(&self) -> bool {
        self.live_graph_need_pump() || super::controls::live_autorun_waiting(self)
    }

    /// Whether a retained live comparison still needs its owned jobs observed.
    pub(crate) fn live_graph_need_pump(&self) -> bool {
        self.live_graph.as_ref().is_some_and(|state| {
            state.trace.as_ref().is_some_and(TraceSession::needs_pump)
                || state.handles.iter().copied().any(|handle| {
                    matches!(
                        state.execution.phase(handle),
                        Some(
                            LiveGraphPhase::Ready
                                | LiveGraphPhase::ScriptQueued
                                | LiveGraphPhase::ScriptRunning
                                | LiveGraphPhase::ProofReady
                                | LiveGraphPhase::ProofsRunning
                        )
                    )
                })
        })
    }

    /// Access the canonical live graph used by both UI and agent commands.
    pub(crate) fn live_graph_session(&self) -> Option<&GraphSession> {
        self.live_graph.as_ref().map(|state| &state.session)
    }

    /// Mutate through `GraphSession`'s guarded methods, never a copied canvas graph.
    pub(crate) fn live_graph_session_mut(&mut self) -> Option<&mut GraphSession> {
        self.live_graph.as_mut().map(|state| &mut state.session)
    }

    /// Create the unsaved comparison starter lazily for this document lifetime.
    /// The font's live graph with its Python comparison, grown from the
    /// specimen when the comparison is missing.
    pub(crate) fn ensure_live_graph(&mut self) -> Result<(), String> {
        if self.live_graph_has_comparison() {
            return Ok(());
        }
        let source = self
            .font
            .project
            .source_id(self.font.active())
            .ok_or("active source is unavailable")?;
        let selected_recipe = if self.has_text_session {
            let captured = self
                .text_proof_selection
                .as_ref()
                .ok_or("select a shaped text sort before opening a proof comparison")?;
            if captured.context != self.text_context_id() {
                return Err("text proof selection belongs to another tab".into());
            }
            if captured.source != Some(source) {
                return Err("text proof selection belongs to another source".into());
            }
            if captured.document_revision != self.font.project.document_revision() {
                return Err("text proof selection belongs to an older document revision".into());
            }
            if captured.axis_values != self.axis_values {
                return Err("text proof location changed after the selection was captured".into());
            }
            let selection = captured.selection.clone()?;
            if selection.text != self.initial_text {
                return Err("text proof selection changed after it was captured".into());
            }
            if selection.normalized_location.len() != self.font.project.axes.len() {
                return Err("text proof location no longer matches the document axes".into());
            }
            let disabled: std::collections::HashSet<_> = selection
                .features
                .iter()
                .filter_map(|(tag, enabled)| (!enabled).then_some(tag.as_str()))
                .collect();
            if disabled.len() != selection.features.len()
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
                return Err("text proof settings changed after the selection was captured".into());
            }
            Some(CompiledProofRecipe::from_text_selection(selection)?)
        } else {
            None
        };
        self.ensure_live_comparison(source, selected_recipe)
    }

    /// Seed a Brush comparison from the validated selected occurrence.
    ///
    /// Brush candidates may be generated after an unrelated document edit. The proof runner
    /// checks the captured occurrence against the current compiled font before any comparison.
    pub(crate) fn ensure_live_graph_for_sketch(
        &mut self,
        recipe: CompiledProofRecipe,
    ) -> Result<(), String> {
        if self.live_graph_has_comparison() {
            return Ok(());
        }
        let source = self
            .font
            .project
            .source_id(self.font.active())
            .ok_or("active source is unavailable")?;
        self.ensure_live_comparison(source, Some(recipe))
    }

    /// The font's live graph, created as the type specimen when there is none.
    ///
    /// This is what the Graph tab shows first. It runs no Python, so it needs
    /// no glyph scope and no text selection.
    pub(crate) fn ensure_live_specimen(&mut self) -> Result<(), String> {
        if self.live_graph.is_some() {
            return Ok(());
        }
        let source = self
            .font
            .project
            .source_id(self.font.active())
            .ok_or("active source is unavailable")?;
        self.create_live_graph(source)
    }

    fn live_graph_has_comparison(&self) -> bool {
        self.live_graph
            .as_ref()
            .is_some_and(|state| graph_live::has_comparison(&state.session.snapshot().graph))
    }

    /// Add the Python comparison to the live graph, creating the specimen first
    /// when there is no graph. Both comparison proofs render `selected_recipe`,
    /// or the default line at the source's location.
    fn ensure_live_comparison(
        &mut self,
        source: SourceId,
        selected_recipe: Option<CompiledProofRecipe>,
    ) -> Result<(), String> {
        if self.live_graph.is_none() {
            self.create_live_graph(source)?;
        }
        let recipe = match selected_recipe {
            Some(recipe) => serde_json::to_value(recipe).map_err(|error| error.to_string())?,
            None => {
                let mut recipe = graph_live::default_proof_recipe();
                recipe["normalized_location"] =
                    serde_json::json!(self.source_normalized_location(source)?);
                recipe
            }
        };
        let state = self
            .live_graph
            .as_mut()
            .expect("live graph was initialized");
        let snapshot = state.session.snapshot();
        if graph_live::has_comparison(&snapshot.graph) {
            return Ok(());
        }
        let mut grown = snapshot.graph.clone();
        graph_live::add_comparison(&mut grown, &recipe)
            .ok_or("the live graph has no live.font node to compare")?;
        let edits = super::controls::graph_edits(&snapshot.graph, &grown)?;
        state
            .session
            .mutate_interactive(GraphInteractiveMutationRequest {
                guard: GraphGuard {
                    identity: snapshot.identity.clone(),
                    revision: snapshot.revision,
                },
                mutation: GraphMutation::Patch { edits },
            })
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// The source's axis location, one value per document axis.
    fn source_normalized_location(&self, source: SourceId) -> Result<Vec<f64>, String> {
        let location = self
            .font
            .project
            .document_source(source)
            .ok_or("selected source is unavailable")?
            .location();
        Ok(self
            .font
            .project
            .axes
            .iter()
            .map(|axis| location.get(&axis.name).copied().unwrap_or(0.0))
            .collect())
    }

    /// A line's advance in ems from the active master's glyph advances.
    /// A character the font lacks counts as half an em.
    fn line_advance_em(&self, text: &str) -> f64 {
        let upm = self.font.units_per_em().max(1.0);
        text.chars()
            .map(|character| {
                self.font
                    .glyphs
                    .iter()
                    .find(|glyph| glyph.codepoint == Some(character))
                    .map_or(0.5, |glyph| glyph.advance / upm)
            })
            .sum()
    }

    fn create_live_graph(&mut self, source: SourceId) -> Result<(), String> {
        let epoch = self
            .live
            .as_ref()
            .ok_or("native live endpoint is unavailable")?
            .document_epoch()
            .to_owned();
        let location = self.source_normalized_location(source)?;
        let recipes = graph_live::specimen_recipes(|text| self.line_advance_em(text), &location);
        let graph = graph_live::specimen_starter(source, recipes);
        self.live_graph = Some(fresh_live_graph(self.document_id, epoch, graph, None)?);
        Ok(())
    }

    /// Save the current native live graph intent to an explicit user-selected path.
    ///
    /// Saving the same open path uses its observed revision.
    /// A different path must remain absent, so Save As cannot silently overwrite a graph.
    pub(crate) fn save_live_graph_file(
        &mut self,
        path: &Path,
    ) -> Result<LiveGraphFileMetadata, String> {
        self.ensure_live_specimen()?;
        let state = self
            .live_graph
            .as_mut()
            .expect("live graph was initialized");
        let expected_revision = state
            .file
            .as_ref()
            .filter(|metadata| metadata.path == path)
            .map(|metadata| metadata.revision.as_str());
        let snapshot = state.session.snapshot();
        let saved = graph_file::save(path, &snapshot.graph, expected_revision)
            .map_err(|error| error.to_string())?;
        state.file = Some(saved.metadata.clone());
        state.saved_revision = Some(snapshot.revision);
        Ok(saved.metadata)
    }

    /// Open live graph intent into a fresh guarded session bound to one explicit source.
    ///
    /// Retained runs must be released first so replacing the session cannot orphan their work or
    /// make old results appear to belong to the newly opened graph.
    pub(crate) fn open_live_graph_file(
        &mut self,
        path: &Path,
        explicit_source: SourceId,
    ) -> Result<LiveGraphFileMetadata, String> {
        let source_name = self
            .font
            .project
            .document_source(explicit_source)
            .ok_or("selected live graph source is not loaded in this document")?
            .name()
            .to_owned();
        if self
            .live_graph
            .as_ref()
            .is_some_and(|state| !state.handles.is_empty() || state.trace.is_some())
        {
            return Err(
                "release current live graph runs and traces before opening another graph".into(),
            );
        }
        let mut document = graph_file::load(path).map_err(|error| error.to_string())?;
        let fonts: Vec<usize> = document
            .graph
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| (node.type_name == "live.font").then_some(index))
            .collect();
        if fonts.len() != 1 {
            return Err("saved live graph must contain exactly one live font node".into());
        }
        document.graph.nodes[fonts[0]]
            .values
            .insert("source".into(), serde_json::json!(explicit_source.0));
        let epoch = self
            .live
            .as_ref()
            .ok_or("native live endpoint is unavailable")?
            .document_epoch()
            .to_owned();
        let metadata = document.metadata;
        let replacement = fresh_live_graph(
            self.document_id,
            epoch,
            document.graph,
            Some(metadata.clone()),
        )?;
        self.live_graph = Some(replacement);
        self.graph.live_selected = true;
        self.graph.selected = None;
        self.graph.live_ui_handles.clear();
        self.graph.proof_images.clear();
        self.graph.content_sizes.clear();
        self.note = format!(
            "Opened {} for source {source_name}; no comparison was run",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("live graph")
        );
        Ok(metadata)
    }

    /// Observe only owned Python/proof jobs and publish results with their captured identities.
    pub(crate) fn live_graph_pump(&mut self) {
        self.poll_live_trace();
        let Some(state) = self.live_graph.as_mut() else {
            return;
        };
        let Some(queue) = self.script_jobs.as_ref() else {
            return;
        };
        let current = GraphDocumentState {
            document_epoch: self
                .live
                .as_ref()
                .map(|live| live.document_epoch().to_owned())
                .unwrap_or_default(),
            document_revision: self.font.project.document_revision(),
        };
        state
            .execution
            .poll_scripts(&mut state.session, queue, &self.font.project);
        for handle in state.handles.iter().copied() {
            if matches!(
                state.execution.phase(handle),
                Some(LiveGraphPhase::Terminal(
                    runebender::workflows::graph_session::GraphRunStatus::Stale
                        | runebender::workflows::graph_session::GraphRunStatus::Cancelled
                ))
            ) {
                state.proofs.release(handle.get());
                continue;
            }
            if state.execution.phase(handle) == Some(LiveGraphPhase::ProofReady) {
                match state.execution.take_proof_request(handle) {
                    Ok(request) => {
                        if let Err(error) = state.proofs.submit_node(
                            handle.get(),
                            request.node,
                            request.identity.graph.document_epoch,
                            request.input,
                            request.proof_recipe,
                        ) {
                            let _ = state.execution.fail_proofs(
                                &mut state.session,
                                handle,
                                &current,
                                error,
                            );
                        }
                    }
                    Err(error) => {
                        let _ = state.execution.fail_proofs(
                            &mut state.session,
                            handle,
                            &current,
                            error.to_string(),
                        );
                    }
                }
            }
            if state.execution.phase(handle) != Some(LiveGraphPhase::ProofsRunning) {
                continue;
            }
            let Some(node) = state.execution.active_proof_node(handle) else {
                continue;
            };
            match state.proofs.inspect_node(handle.get(), node) {
                Some(NodeProofInspection::Completed { artifact_id, proof }) => {
                    let outputs = LiveGraphProofOutputs {
                        node,
                        artifact_id,
                        content_sha256: format!("sha256:{:x}", Sha256::digest(&proof.png)),
                        canonical_input_sha256: proof.canonical_input_sha256.clone(),
                        font_sha256: proof.font_sha256.clone(),
                    };
                    if let Err(error) = state.execution.publish_proofs(
                        &mut state.session,
                        handle,
                        &current,
                        outputs,
                    ) {
                        let _ = state.execution.fail_proofs(
                            &mut state.session,
                            handle,
                            &current,
                            error.to_string(),
                        );
                    }
                }
                Some(NodeProofInspection::Failed(error)) => {
                    let _ =
                        state
                            .execution
                            .fail_proofs(&mut state.session, handle, &current, error);
                    state.proofs.release_node(handle.get(), node);
                }
                None => {
                    let _ = state.execution.fail_proofs(
                        &mut state.session,
                        handle,
                        &current,
                        "specimen job is no longer retained",
                    );
                }
                Some(NodeProofInspection::Pending) => {}
            }
        }
        // A proof callback can terminalize a run after the loop's initial state check.
        // Release its bytes now, since an idle Workspace will not schedule another pump.
        for handle in state.handles.iter().copied() {
            if matches!(
                state.execution.phase(handle),
                Some(LiveGraphPhase::Terminal(
                    runebender::workflows::graph_session::GraphRunStatus::Stale
                        | runebender::workflows::graph_session::GraphRunStatus::Cancelled
                        | runebender::workflows::graph_session::GraphRunStatus::Failed
                ))
            ) {
                state.proofs.release(handle.get());
            }
        }
    }
}

fn fresh_live_graph(
    document_id: u64,
    epoch: String,
    graph: runebender::workflows::graph::NodeGraph,
    file: Option<LiveGraphFileMetadata>,
) -> Result<LiveGraphState, String> {
    let generation = NEXT_GRAPH_SESSION.fetch_add(1, Ordering::Relaxed);
    let session = GraphSession::new(
        format!("graph-{document_id}-{generation}"),
        epoch,
        graph,
        Registry::core(),
    )
    .map_err(|error| error.to_string())?;
    Ok(LiveGraphState {
        session,
        execution: LiveGraphExecution::default(),
        proofs: NodeProofJobs::default(),
        handles: BTreeSet::new(),
        requests: BTreeMap::new(),
        next_job: 0,
        saved_revision: file.as_ref().map(|_| 0),
        file,
        trace: None,
        trace_receipts: BTreeMap::new(),
        next_trace_id: 1,
    })
}

fn parse<T: serde::de::DeserializeOwned>(arguments: &Value) -> Result<T, String> {
    serde_json::from_value(arguments.clone()).map_err(|error| error.to_string())
}

fn success(result: impl serde::Serialize) -> Value {
    serde_json::to_value(result).expect("typed graph result serializes")
}

impl Workspace {
    pub(crate) fn call_agent_graph(
        &mut self,
        call: &runebender::automation::agent::ToolCall,
    ) -> Option<Value> {
        if !matches!(
            call.name.as_str(),
            "graph_discover"
                | "graph_snapshot"
                | "graph_mutate"
                | "graph_trace"
                | "graph_trace_status"
                | "graph_trace_cancel"
                | "graph_trace_release"
                | "graph_run"
                | "graph_status"
                | "graph_cancel"
                | "graph_release"
                | "graph_apply"
                | "graph_image"
        ) {
            return None;
        }
        Some(match self.handle_agent_graph(call) {
            Ok(value) => value,
            Err(error) => {
                serde_json::json!({"ok":false,"error_code":"graph_rejected","error":error,"root_changed":false})
            }
        })
    }

    fn handle_agent_graph(
        &mut self,
        call: &runebender::automation::agent::ToolCall,
    ) -> Result<Value, String> {
        use super::execution::LiveGraphSubmitRequest;
        use base64::Engine as _;
        use runebender::automation::agent_graph::*;
        use runebender::automation::script_recipe;
        use runebender::font::compiler::proof as compiled_proof;
        use runebender::font::variable::SourceId;
        use serde_json::json;

        let epoch = self
            .live
            .as_ref()
            .ok_or("native live endpoint is unavailable")?
            .document_epoch()
            .to_owned();
        if call
            .arguments
            .get("expected_document_epoch")
            .and_then(Value::as_str)
            != Some(epoch.as_str())
        {
            return Err("stale document epoch; reconnect to the intended native session".into());
        }
        if call.name == "graph_run" {
            self.ensure_script_job_queue()?;
        }
        // Tools work on the graph the person sees. With none yet, they get the
        // specimen with a comparison, the shape agents start from.
        if self.live_graph.is_none() {
            self.ensure_live_graph()?;
        }
        if call.name != "graph_trace_cancel" {
            self.live_graph_pump();
        }
        if matches!(
            call.name.as_str(),
            "graph_trace" | "graph_trace_status" | "graph_trace_cancel" | "graph_trace_release"
        ) {
            return self.handle_trace_call(call);
        }
        let current = GraphDocumentState {
            document_epoch: epoch,
            document_revision: self.font.project.document_revision(),
        };
        let state = self
            .live_graph
            .as_mut()
            .expect("live graph was initialized");
        match call.name.as_str() {
            "graph_discover" => {
                let request: GraphDiscoverRequest = parse(&call.arguments)?;
                request.validate()?;
                Ok(success(GraphDiscoverResult {
                    ok: true,
                    identity: state.session.snapshot().identity,
                    discovery: state.session.discovery(),
                    recipe_authoring: script_recipe::AUTHORING_INSTRUCTIONS.into(),
                    root_changed: false,
                }))
            }
            "graph_snapshot" => {
                let request: GraphSnapshotRequest = parse(&call.arguments)?;
                request.validate()?;
                check_identity(&state.session, &request.identity)?;
                Ok(success(GraphSnapshotResult {
                    ok: true,
                    snapshot: state.session.snapshot(),
                    root_changed: false,
                }))
            }
            "graph_mutate" => {
                let request: GraphMutateRequest = parse(&call.arguments)?;
                request.validate()?;
                let response = state
                    .session
                    .mutate(request.request)
                    .map_err(|error| error.to_string())?;
                Ok(success(GraphMutateResult {
                    ok: true,
                    mutation: response,
                    snapshot: state.session.snapshot(),
                    root_changed: false,
                }))
            }
            "graph_run" => {
                let request: GraphRunRequest = parse(&call.arguments)?;
                request.validate()?;
                request.validate_source(&self.font.project)?;
                let key = (
                    format!("run:{}", request.actor),
                    request.operation_key.clone(),
                );
                if let Some((payload, response)) = state.requests.get(&key) {
                    if payload != &call.arguments {
                        return Err("run key already has a different request".into());
                    }
                    let RetainedGraphResponse::Run(response) = response else {
                        return Err("run key collides with a retained Apply response".into());
                    };
                    let mut replay = (**response).clone();
                    replay.replayed = true;
                    return Ok(success(replay));
                }
                if state.requests.len() >= 64 {
                    return Err("graph retry retention is full; open a new graph session".into());
                }
                let snapshot = state.session.snapshot();
                if request.glyphs.is_empty()
                    && snapshot
                        .graph
                        .nodes
                        .iter()
                        .any(|node| node.type_name == "live.python")
                {
                    return Err("enter at least one glyph for the Python recipe to change".into());
                }
                let parameters = if request.execution_version == 1 {
                    snapshot
                        .graph
                        .nodes
                        .iter()
                        .find(|node| node.type_name == "live.python")
                        .ok_or("graph has no Python recipe node")?
                        .values
                        .get("parameters")
                        .cloned()
                        .unwrap_or_else(|| json!({}))
                } else {
                    json!({})
                };
                let Value::Object(parameters) = parameters else {
                    return Err("Python parameters must be an object".into());
                };
                let input = script_recipe::capture(
                    &self.font.project,
                    SourceId(request.source),
                    &request.glyphs,
                    format!("graph-{}-{}", self.document_id, state.next_job),
                    parameters.into_iter().collect(),
                )?;
                let baseline = compiled_proof::capture(&self.font.project)?;
                let queue = self
                    .script_jobs
                    .as_ref()
                    .ok_or("Python runner is unavailable")?;
                let submitted = state
                    .execution
                    .submit(
                        &mut state.session,
                        queue,
                        &self.font.project,
                        LiveGraphSubmitRequest {
                            execution_version: request.execution_version,
                            guard: request.guard,
                            actor: request.actor,
                            operation_key: request.operation_key,
                            recipe_input: input,
                            base_proof_input: baseline,
                        },
                    )
                    .map_err(|error| error.to_string())?;
                state.next_job = state.next_job.saturating_add(1);
                state.handles.insert(submitted.graph.receipt.handle);
                let response = GraphRunResult {
                    ok: true,
                    run: submitted.graph,
                    replayed: false,
                    root_changed: false,
                };
                state.requests.insert(
                    key,
                    (
                        call.arguments.clone(),
                        RetainedGraphResponse::Run(Box::new(response.clone())),
                    ),
                );
                Ok(success(response))
            }
            "graph_status" => {
                let request: GraphStatusRequest = parse(&call.arguments)?;
                request.validate()?;
                check_identity(&state.session, &request.identity)?;
                let inspection = state
                    .session
                    .inspect_run(request.handle)
                    .ok_or("unknown graph run")?;
                let fresh = run_is_current(&state.session, &inspection, &current);
                let summary = state.execution.result_summary(request.handle);
                Ok(success(GraphStatusResult {
                    ok: true,
                    run: inspection,
                    current: fresh,
                    stale: !fresh,
                    report: summary.as_ref().map(|summary| summary.report.clone()),
                    stderr: summary.as_ref().map(|summary| summary.stderr.clone()),
                    can_apply: fresh && summary.is_some_and(|summary| summary.can_apply),
                    root_changed: false,
                }))
            }
            "graph_cancel" => {
                let request: GraphCancelToolRequest = parse(&call.arguments)?;
                request.validate()?;
                let handle = request.request.handle;
                let queue = self
                    .script_jobs
                    .as_ref()
                    .ok_or("Python runner is unavailable")?;
                let response = state
                    .execution
                    .cancel(&mut state.session, queue, request.request, &current)
                    .map_err(|error| error.to_string())?;
                if response.cancel_proofs {
                    state.proofs.release(handle.get());
                    state
                        .execution
                        .finish_proof_cancellation(&mut state.session, handle, &current)
                        .map_err(|error| error.to_string())?;
                }
                if state.session.inspect_run(handle).is_some_and(|run| {
                    matches!(
                        run.status,
                        runebender::workflows::graph_session::GraphRunStatus::Cancelled
                            | runebender::workflows::graph_session::GraphRunStatus::CancellationRequested
                    )
                }) {
                    state.proofs.release(handle.get());
                }
                Ok(success(GraphCancelResult {
                    ok: true,
                    cancellation: response.graph,
                    root_changed: false,
                }))
            }
            "graph_release" => {
                let request: GraphReleaseRequest = parse(&call.arguments)?;
                request.validate()?;
                check_identity(&state.session, &request.identity)?;
                let queue = self
                    .script_jobs
                    .as_ref()
                    .ok_or("Python runner is unavailable")?;
                let released = state
                    .execution
                    .release(&mut state.session, queue, request.handle)
                    .map_err(|error| error.to_string())?;
                if released {
                    state.proofs.release(request.handle.get());
                    state.handles.remove(&request.handle);
                }
                Ok(success(GraphReleaseResult {
                    ok: true,
                    released,
                    root_changed: false,
                }))
            }
            "graph_apply" => {
                let request: GraphApplyRequest = parse(&call.arguments)?;
                request.validate()?;
                check_identity(&state.session, &request.identity)?;
                let key = (
                    format!("apply:{}", request.actor),
                    request.operation_key.clone(),
                );
                if let Some((payload, response)) = state.requests.get(&key) {
                    if payload != &call.arguments {
                        return Err("Apply key already has a different request".into());
                    }
                    let RetainedGraphResponse::Apply(response) = response else {
                        return Err("Apply key collides with a retained run response".into());
                    };
                    let mut replay = response.clone();
                    if response.get("receipt").is_some() {
                        replay = self.call_agent_edit(&runebender::automation::agent::ToolCall {
                            name: "agent_receipt".into(),
                            arguments: json!({"expected_document_epoch":request.expected_document_epoch,
                                "actor":request.actor,"operation_key":request.operation_key}),
                        }).ok_or("edit receipt adapter unavailable")?;
                    }
                    replay["replayed"] = json!(true);
                    replay["root_changed"] = json!(false);
                    return Ok(replay);
                }
                if state.requests.len() >= 64 {
                    return Err("graph retry retention is full".into());
                }
                let inspection = state
                    .session
                    .inspect_run(request.handle)
                    .ok_or("unknown graph run")?;
                if !run_is_current(&state.session, &inspection, &current) {
                    return Err("graph result is stale; rerun before Apply".into());
                }
                let edit = state
                    .execution
                    .apply_request(
                        &state.session,
                        &current,
                        request.handle,
                        request.node,
                        request.actor,
                        request.operation_key,
                        request.authorization,
                    )
                    .map_err(|error| error.to_string())?;
                let response = match edit.payload_digest {
                    Some(digest) => self.apply_retained_agent_edit_bound(
                        edit.request,
                        edit.candidate,
                        Some(digest),
                    ),
                    None => self.apply_retained_agent_edit(edit.request, edit.candidate),
                };
                self.live_graph
                    .as_mut()
                    .expect("Apply does not replace the graph")
                    .requests
                    .insert(
                        key,
                        (
                            call.arguments.clone(),
                            RetainedGraphResponse::Apply(response.clone()),
                        ),
                    );
                Ok(response)
            }
            "graph_image" => {
                let request: GraphImageRequest = parse(&call.arguments)?;
                request.validate()?;
                check_identity(&state.session, &request.identity)?;
                let inspection = state
                    .session
                    .inspect_run(request.handle)
                    .ok_or("unknown graph run")?;
                let fresh = run_is_current(&state.session, &inspection, &current);
                let node = state
                    .execution
                    .proof_node(
                        request.handle,
                        request.node,
                        request
                            .branch
                            .map(|branch| branch == GraphImageBranch::Changed),
                    )
                    .map_err(|error| error.to_string())?;
                if !inspection.outputs.iter().any(|output| {
                    output.node == node
                        && matches!(
                            output.value,
                            runebender::workflows::graph_session::GraphNodeOutputValue::Proof { .. }
                        )
                }) {
                    return Err("proof has no publishable completed output".into());
                }
                let Some(NodeProofInspection::Completed { artifact_id, proof }) =
                    state.proofs.inspect_node(request.handle.get(), node)
                else {
                    return Err("proof image is not available".into());
                };
                let image = proof.image(request.view)?;
                if image.bytes.len() > 5 * 1024 * 1024 {
                    return Err("specimen exceeds the transport image limit".into());
                }
                Ok(success(GraphImageResult {
                    ok: true,
                    artifact_id,
                    current: fresh,
                    stale: !fresh,
                    captured_document_epoch: inspection.identity.graph.document_epoch,
                    captured_document_revision: proof.document_revision,
                    font_sha256: proof.font_sha256.clone(),
                    canonical_input_sha256: proof.canonical_input_sha256.clone(),
                    recipe: proof.recipe.clone(),
                    recipe_sha256: proof.recipe_sha256.clone(),
                    renderer: proof.renderer.clone(),
                    view: request.view,
                    rendering: image.rendering.clone(),
                    target_glyph_index: image.target_glyph_index,
                    crop: image.crop.cloned(),
                    glyphs: proof.glyphs.clone(),
                    png_base64: Some(base64::engine::general_purpose::STANDARD.encode(image.bytes)),
                    root_changed: false,
                }))
            }
            _ => Err("unknown Graph command".into()),
        }
    }
}

fn check_identity(
    session: &GraphSession,
    identity: &runebender::workflows::graph_session::GraphIdentity,
) -> Result<(), String> {
    if &session.snapshot().identity != identity {
        return Err("stale graph session".into());
    }
    Ok(())
}

fn run_is_current(
    session: &GraphSession,
    run: &runebender::workflows::graph_session::GraphRunInspection,
    current: &GraphDocumentState,
) -> bool {
    let snapshot = session.snapshot();
    run.identity.graph == snapshot.identity
        && run.identity.graph.document_epoch == current.document_epoch
        && run.identity.capture.font.document_revision == current.document_revision
        && run.identity.semantic_revision == snapshot.semantic_revision
        && run.identity.semantic_hash == snapshot.semantic_hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::font_model::FontModel;
    use runebender::automation::agent::ToolCall;
    use runebender::font::project::Project;
    use runebender::workflows::graph_session::{
        GraphEdit, GraphGuard, GraphInteractiveMutationRequest, GraphMutation,
    };
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(std::path::PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "runebender-live-graph-workspace-{}-{sequence}",
                std::process::id()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn call(app: &mut Workspace, name: &str, arguments: Value) -> Value {
        app.call_live(&ToolCall {
            name: name.into(),
            arguments,
        })
    }

    #[test]
    fn the_specimen_renders_on_open_and_again_after_edits_settle() {
        use crate::application::editor::tools::graph::controls::{
            live_autorun_tick, live_autorun_waiting, select_live_specimen,
        };
        let mut project = Project::new_font(std::env::temp_dir().join("graph-specimen.ufo"));
        let layer = project
            .document_source(project.source_id(0).unwrap())
            .unwrap()
            .default_layer();
        for name in ["H", "a", "m"] {
            project
                .add_document_glyph(name, 500.0, Some(u32::from(name.chars().next().unwrap())))
                .unwrap();
            project
                .edit_document_layer(name, &layer, |draft| {
                    draft.add_shape_contour(kurbo::Rect::new(40.0, 0.0, 460.0, 700.0), false)?;
                    Ok(())
                })
                .unwrap();
        }
        let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
        app.mode = crate::application::workspace::Mode::Graph;
        select_live_specimen(&mut app);
        let graph = app.live_graph_session().unwrap().snapshot().graph;
        assert!(
            graph
                .nodes
                .iter()
                .all(|node| node.type_name != "live.python")
        );
        assert_eq!(
            graph
                .nodes
                .iter()
                .filter(|node| node.type_name == "live.proof")
                .count(),
            graph_live::SPECIMEN_PAGES
        );
        assert!(live_autorun_waiting(&app), "the first render does not wait");
        live_autorun_tick(&mut app);
        assert!(!app.graph.live_ui_handles.is_empty(), "{}", app.note);
        let deadline = Instant::now() + Duration::from_secs(60);
        while app.graph.proof_images.len() < graph_live::SPECIMEN_PAGES {
            assert!(
                Instant::now() < deadline,
                "specimen did not render: {}",
                app.note
            );
            app.live_graph_pump();
            app.sync_live_graph_presentation();
            std::thread::sleep(Duration::from_millis(20));
        }
        while app.live_graph_need_pump() {
            assert!(Instant::now() < deadline, "specimen run did not finish");
            app.live_graph_pump();
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!live_autorun_waiting(&app));

        // An edit waits for the font to settle, then renders again.
        app.font
            .project
            .edit_document_layer("H", &layer, |draft| {
                draft.set_width(520.0)?;
                Ok(())
            })
            .unwrap();
        assert!(live_autorun_waiting(&app));
        let handles = app.graph.live_ui_handles.len();
        live_autorun_tick(&mut app);
        assert!(app.graph.render_after.is_some());
        assert_eq!(app.graph.live_ui_handles.len(), handles, "not yet");
        let (revision, _) = app.graph.render_after.unwrap();
        app.graph.render_after = Some((revision, Instant::now() - Duration::from_secs(2)));
        live_autorun_tick(&mut app);
        assert_eq!(app.graph.rendered_revision, Some(revision));
        assert!(
            app.graph.live_ui_handles.len() <= 2,
            "older results are released"
        );
    }

    #[test]
    fn python_needs_a_glyph_scope_but_a_proof_only_run_does_not() {
        let mut project = Project::new_font(std::env::temp_dir().join("graph-scope.ufo"));
        project
            .add_document_glyph("A", 400.0, Some(u32::from('A')))
            .unwrap();
        let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
        let epoch = app.live.as_ref().unwrap().document_epoch().to_owned();
        let run = |app: &mut Workspace, key: &str| {
            let snapshot = app.live_graph_session().unwrap().snapshot();
            call(
                app,
                "graph_run",
                json!({
                    "execution_version":2,"expected_document_epoch":epoch,
                    "guard":{"identity":snapshot.identity,"semantic_revision":snapshot.semantic_revision,
                        "semantic_hash":snapshot.semantic_hash},
                    "actor":"scope-test","operation_key":key,"source":0,"glyphs":[]
                }),
            )
        };
        app.ensure_live_specimen().unwrap();
        let proofs_only = run(&mut app, "specimen");
        assert_eq!(proofs_only["ok"], true, "{proofs_only}");
        app.ensure_live_graph().unwrap();
        let with_python = run(&mut app, "comparison");
        assert_eq!(with_python["ok"], false);
        assert!(
            with_python["error"]
                .as_str()
                .unwrap()
                .contains("at least one glyph"),
            "{with_python}"
        );
    }

    #[test]
    fn starter_uses_one_captured_text_recipe_for_both_proofs() {
        let project = Project::new_font(std::env::temp_dir().join("live-text-proof.ufo"));
        let source = project.source_id(0).unwrap();
        let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
        app.has_text_session = true;
        app.initial_text = "AA".into();
        assert!(
            app.ensure_live_graph()
                .unwrap_err()
                .contains("select a shaped text sort")
        );
        let selection = runebender::text::buffer::TextProofSelection {
            text: "AA".into(),
            normalized_location: Vec::new(),
            right_to_left: false,
            features: Vec::new(),
            script: None,
            language: None,
            glyph_name: "A".into(),
            cluster: 1,
            occurrence: 1,
            reference_pen_x: 500.0,
        };
        app.text_proof_selection = Some(crate::application::workspace::TextProofCapture {
            context: (0, 0),
            source: Some(source),
            document_revision: app.font.project.document_revision(),
            axis_values: app.axis_values.clone(),
            selection: Ok(selection),
        });
        assert!(app.ensure_live_graph().unwrap_err().contains("another tab"));
        let context = app.text_context_id();
        app.text_proof_selection.as_mut().unwrap().context = context;
        app.initial_text = "AB".into();
        assert!(
            app.ensure_live_graph()
                .unwrap_err()
                .contains("selection changed")
        );
        app.initial_text = "AA".into();
        app.text_proof_selection.as_mut().unwrap().document_revision += 1;
        assert!(
            app.ensure_live_graph()
                .unwrap_err()
                .contains("older document revision")
        );
        app.text_proof_selection.as_mut().unwrap().document_revision -= 1;
        app.ensure_live_graph().unwrap();
        let session = app.live_graph_session().unwrap();
        let comparison = session.execution_plan(1).unwrap().comparison().unwrap();
        let graph = session.snapshot().graph;
        let recipes: Vec<_> = [comparison.unchanged_proof, comparison.changed_proof]
            .into_iter()
            .map(|node| graph.node(node).unwrap().values["recipe"].clone())
            .collect();
        assert_eq!(recipes[0], recipes[1]);
        assert_eq!(recipes[0]["text"], "AA");
        assert_eq!(recipes[0]["target"]["cluster"], 1);
        assert_eq!(recipes[0]["target"]["occurrence"], 1);
    }

    #[test]
    fn live_graph_save_and_open_preserve_intent_with_fresh_identity() {
        let project = Project::new_font(std::env::temp_dir().join("live-graph-unsaved.ufo"));
        let source = project.source_id(0).unwrap();
        let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
        let document_revision = app.font.project.document_revision();
        let modified = app.font.project.is_modified();
        let queue_was_initialized = app.script_jobs.is_some();
        app.ensure_live_graph().unwrap();
        let before = app.live_graph_session().unwrap().snapshot();
        let python = before
            .graph
            .nodes
            .iter()
            .find(|node| node.type_name == "live.python")
            .unwrap()
            .id;
        let proof = before
            .graph
            .nodes
            .iter()
            .find(|node| node.type_name == "live.proof")
            .unwrap();
        let proof_id = proof.id;
        let mut proof_recipe = proof.values["recipe"].clone();
        proof_recipe["text"] = json!("Saved specimen");
        app.live_graph_session_mut()
            .unwrap()
            .mutate_interactive(GraphInteractiveMutationRequest {
                guard: GraphGuard {
                    identity: before.identity.clone(),
                    revision: before.revision,
                },
                mutation: GraphMutation::Patch {
                    edits: vec![
                        GraphEdit::MoveNode {
                            node: python,
                            pos: [123.0, 456.0],
                        },
                        GraphEdit::SetValue {
                            node: python,
                            field: "code".into(),
                            value: json!("print('persisted')\n"),
                        },
                        GraphEdit::SetValue {
                            node: python,
                            field: "parameters".into(),
                            value: json!({"weight": 725}),
                        },
                        GraphEdit::SetValue {
                            node: proof_id,
                            field: "recipe".into(),
                            value: proof_recipe,
                        },
                    ],
                },
            })
            .unwrap();
        let root = TestDirectory::new();
        let path = root.0.join("comparison.graph.json");
        let saved = app.save_live_graph_file(&path).unwrap();
        assert_eq!(app.font.project.document_revision(), document_revision);
        assert_eq!(app.font.project.is_modified(), modified);
        assert_eq!(app.script_jobs.is_some(), queue_was_initialized);

        let metadata = app.open_live_graph_file(&path, source).unwrap();
        assert_eq!(metadata, saved);
        assert!(app.note.contains("for source"));
        assert!(app.note.contains("no comparison was run"));
        let reopened = app.live_graph_session().unwrap().snapshot();
        assert_ne!(reopened.identity, before.identity);
        assert_eq!(
            reopened.identity.document_epoch,
            before.identity.document_epoch
        );
        assert_eq!(reopened.revision, 0);
        assert_eq!(reopened.semantic_revision, 0);
        assert!(!reopened.can_undo);
        assert!(!reopened.can_redo);
        let python = reopened
            .graph
            .nodes
            .iter()
            .find(|node| node.type_name == "live.python")
            .unwrap();
        assert_eq!(python.pos, [123.0, 456.0]);
        assert_eq!(python.values["code"], "print('persisted')\n");
        assert_eq!(python.values["parameters"], json!({"weight": 725}));
        let proof = reopened
            .graph
            .nodes
            .iter()
            .find(|node| node.id == proof_id)
            .unwrap();
        assert_eq!(proof.values["recipe"]["text"], "Saved specimen");
        let font = reopened
            .graph
            .nodes
            .iter()
            .find(|node| node.type_name == "live.font")
            .unwrap();
        assert_eq!(font.values["source"], source.0);
        let state = app.live_graph.as_ref().unwrap();
        assert!(state.handles.is_empty());
        assert!(state.requests.is_empty());
        assert_eq!(state.next_job, 0);
        assert!(app.graph.proof_images.is_empty());
        assert_eq!(app.font.project.document_revision(), document_revision);
        assert_eq!(app.font.project.is_modified(), modified);
        assert_eq!(app.script_jobs.is_some(), queue_was_initialized);

        let mut oversized = reopened.graph.clone();
        while oversized.nodes.len() <= 64 {
            oversized.add("live.proof", [0.0, 0.0]);
        }
        let oversized_path = root.0.join("oversized.graph.json");
        std::fs::write(&oversized_path, serde_json::to_vec(&oversized).unwrap()).unwrap();
        let current_identity = reopened.identity;
        let error = app
            .open_live_graph_file(&oversized_path, source)
            .unwrap_err();
        assert!(error.contains("exceeds 64 nodes"), "{error}");
        assert_eq!(
            app.live_graph_session().unwrap().snapshot().identity,
            current_identity
        );

        let retained: GraphRunHandle = serde_json::from_value(json!(99)).unwrap();
        app.live_graph.as_mut().unwrap().handles.insert(retained);
        let error = app.open_live_graph_file(&path, source).unwrap_err();
        assert!(error.contains("release current"));
        assert_eq!(
            app.live_graph_session().unwrap().snapshot().identity,
            current_identity
        );
        app.live_graph.as_mut().unwrap().handles.remove(&retained);

        let error = app
            .open_live_graph_file(&path, SourceId(usize::MAX))
            .unwrap_err();
        assert!(error.contains("not loaded"));
        assert_eq!(
            app.live_graph_session().unwrap().snapshot().identity,
            current_identity
        );

        std::fs::write(&path, "external edit\n").unwrap();
        let error = app.save_live_graph_file(&path).unwrap_err();
        assert!(error.contains("changed on disk"));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "external edit\n");
    }

    fn capture_completed_comparison(mut app: Workspace) -> Workspace {
        let Some(directory) = std::env::var_os("RUNEBENDER_NODES_PROOFS") else {
            return app;
        };
        use crate::application::design::theme::Palette;
        use crate::application::workspace::Mode;
        use std::sync::Arc;
        use xilem::WidgetView as _;

        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let mode = std::mem::replace(&mut app.mode, Mode::Graph);
        app.graph.live_selected = true;
        app.graph.live_scope = "A".into();
        app.sync_live_graph_presentation();
        for theme in ["gray", "light"] {
            app.theme_id = theme;
            app.palette = Arc::new(Palette::load(theme));
            let background = app.palette.app;
            app = crate::application::platform::screenshot::render_to(
                app,
                background,
                |workspace: &mut Workspace| {
                    xilem::view::sized_box(
                        crate::application::view::render::app_logic(workspace).boxed(),
                    )
                },
                (1440, 900),
                1.0,
                directory
                    .join(format!("graph-{theme}.png"))
                    .to_str()
                    .unwrap(),
            );
        }
        app.mode = mode;
        app
    }

    #[test]
    fn cancelling_a_dag_releases_earlier_proofs_while_script_cancellation_settles() {
        let mut project = Project::new_font(std::env::temp_dir().join("graph-cancel-unsaved.ufo"));
        project
            .add_document_glyph("A", 400.0, Some(u32::from('A')))
            .unwrap();
        let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
        app.ensure_live_graph().unwrap();
        let epoch = app.live.as_ref().unwrap().document_epoch().to_owned();
        let snapshot = app.live_graph_session().unwrap().snapshot();
        let identity = snapshot.identity.clone();
        let script = snapshot
            .graph
            .nodes
            .iter()
            .find(|node| node.type_name == "live.python")
            .unwrap()
            .id;
        let original = snapshot
            .graph
            .nodes
            .iter()
            .find(|node| node.type_name == "live.proof")
            .unwrap()
            .id;
        app.live_graph_session_mut()
            .unwrap()
            .mutate_interactive(GraphInteractiveMutationRequest {
                guard: GraphGuard {
                    identity: identity.clone(),
                    revision: snapshot.revision,
                },
                mutation: GraphMutation::Patch {
                    edits: vec![GraphEdit::SetValue {
                        node: script,
                        field: "code".into(),
                        value: json!("import time\ntime.sleep(20)\n"),
                    }],
                },
            })
            .unwrap();
        let snapshot = app.live_graph_session().unwrap().snapshot();
        let started = call(
            &mut app,
            "graph_run",
            json!({
                "execution_version":2,"expected_document_epoch":epoch,
                "guard":{"identity":identity,"semantic_revision":snapshot.semantic_revision,"semantic_hash":snapshot.semantic_hash},
                "actor":"cancel-test","operation_key":"run","source":0,"glyphs":["A"]
            }),
        );
        assert_eq!(started["ok"], true, "{started}");
        let handle: GraphRunHandle =
            serde_json::from_value(started["run"]["receipt"]["handle"].clone()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            app.live_graph_pump();
            let state = app.live_graph.as_ref().unwrap();
            if matches!(
                state.execution.phase(handle),
                Some(LiveGraphPhase::ScriptQueued | LiveGraphPhase::ScriptRunning)
            ) {
                assert!(matches!(
                    state.proofs.inspect_node(handle.get(), original),
                    Some(NodeProofInspection::Completed { .. })
                ));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "script did not start after original proof"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let cancelled = call(
            &mut app,
            "graph_cancel",
            json!({
                "expected_document_epoch":epoch,
                "request":{"identity":identity,"handle":handle,"actor":"cancel-test","operation_key":"cancel"}
            }),
        );
        assert_eq!(cancelled["ok"], true, "{cancelled}");
        assert!(
            app.live_graph
                .as_ref()
                .unwrap()
                .proofs
                .inspect_node(handle.get(), original)
                .is_none()
        );
        while app.live_graph_need_pump() {
            app.live_graph_pump();
            assert!(Instant::now() < deadline, "cancellation did not settle");
            std::thread::sleep(Duration::from_millis(10));
        }
        let state = app.live_graph.as_ref().unwrap();
        let run = state.session.inspect_run(handle).unwrap();
        assert_eq!(
            run.status,
            runebender::workflows::graph_session::GraphRunStatus::Cancelled
        );
        assert!(run.outputs.is_empty());
    }

    #[test]
    fn node_commands_share_images_retry_receipts_apply_and_ordinary_undo() {
        check_node_commands(false);
    }

    #[test]
    fn generated_node_geometry_applies_the_exact_preview_and_replays_its_ids() {
        check_node_commands(true);
    }

    #[test]
    fn calibrated_trace_draws_empty_glyph_through_graph_proof_and_apply_history() {
        check_calibrated_trace_candidate(false);
    }

    #[test]
    fn calibrated_trace_replaces_junk_through_graph_proof_and_apply_history() {
        check_calibrated_trace_candidate(true);
    }

    fn check_calibrated_trace_candidate(junk: bool) {
        use base64::Engine as _;

        let mut project = Project::new_font(std::env::temp_dir().join("graph-trace-unsaved.ufo"));
        for name in ["A", "G"] {
            project
                .add_document_glyph(name, 400.0, Some(u32::from(name.chars().next().unwrap())))
                .unwrap();
        }
        let layer = project
            .document_source(project.source_id(0).unwrap())
            .unwrap()
            .default_layer();
        if junk {
            project
                .edit_document_layer("A", &layer, |draft| {
                    draft.add_shape_contour(kurbo::Rect::new(5.0, 0.0, 390.0, 700.0), false)?;
                    Ok(())
                })
                .unwrap();
        }
        project
            .edit_document_layer("G", &layer, |draft| {
                draft.add_shape_contour(kurbo::Rect::new(30.0, 0.0, 300.0, 600.0), false)?;
                Ok(())
            })
            .unwrap();
        for (glyph, label) in [("A", "red"), ("G", "green")] {
            let color = runebender::font::model::glyph_metadata::MarkColor::parse(
                &runebender::ui::theme::ufo_rgba_for_label(label).unwrap(),
            )
            .unwrap();
            project
                .edit_document_layer(glyph, &layer, |draft| {
                    draft.set_mark(Some(label), Some(color))?;
                    Ok(())
                })
                .unwrap();
        }
        let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
        app.ensure_live_graph().unwrap();
        let epoch = app.live.as_ref().unwrap().document_epoch().to_owned();
        let initial = app.live_graph_session().unwrap().snapshot();
        let node = initial
            .graph
            .nodes
            .iter()
            .find(|node| node.type_name == "live.python")
            .unwrap()
            .id;
        let edits = initial
            .graph
            .nodes
            .iter()
            .filter(|node| node.type_name == "live.proof")
            .map(|node| {
                let mut recipe = node.values["recipe"].clone();
                recipe["text"] = json!("AA");
                GraphEdit::SetValue {
                    node: node.id,
                    field: "recipe".into(),
                    value: recipe,
                }
            })
            .collect();
        app.live_graph_session_mut()
            .unwrap()
            .mutate_interactive(GraphInteractiveMutationRequest {
                guard: GraphGuard {
                    identity: initial.identity.clone(),
                    revision: initial.revision,
                },
                mutation: GraphMutation::Patch { edits },
            })
            .unwrap();
        let graph = app.live_graph_session().unwrap().snapshot();
        let address = runebender::font::variable::GlyphLayerAddress {
            glyph: "A".into(),
            layer: layer.clone(),
        };
        let green_address = runebender::font::variable::GlyphLayerAddress {
            glyph: "G".into(),
            layer: layer.clone(),
        };
        let before = app.font.project.capture_document_layer(&address).unwrap();
        let green = app
            .font
            .project
            .capture_document_layer(&green_address)
            .unwrap();
        let revision = app.font.project.document_revision();
        let target = json!({
            "glyph":"A",
            "glyph_id":app.font.project.document_glyph("A").unwrap().id().to_wire(),
            "layer":layer.name.clone(),
            "expected_revision":runebender::font::edit_batch::canonical_glyph_revision(before.view()).unwrap(),
        });
        let reference = json!({
            "guard":{
                "glyph":"G",
                "glyph_id":app.font.project.document_glyph("G").unwrap().id().to_wire(),
                "layer":layer.name.clone(),
                "expected_revision":runebender::font::edit_batch::canonical_glyph_revision(green.view()).unwrap(),
            },
            "rationale":"Approved stroke weight and baseline for this Arabic form",
        });
        let raster = image::GrayImage::from_fn(64, 64, |x, y| {
            image::Luma([if (20..44).contains(&x) && (18..46).contains(&y) {
                0
            } else {
                255
            }])
        });
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(raster)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let request = json!({
            "expected_document_epoch":epoch,
            "guard":{"identity":graph.identity,"revision":graph.revision},
            "actor":"trace-test","operation_key":"draft-one","node":node,"source":0,
            "target":target,
            "references":[reference],
            "image_base64":base64::engine::general_purpose::STANDARD.encode(bytes.into_inner()),
            "calibration":{"font_units_per_pixel":2.0,"pixel_baseline_y":50.0,
                "font_x_at_left":0.0,"font_baseline_y":0.0}
        });
        let mut stale = request.clone();
        stale["target"]["expected_revision"] = json!("stale");
        assert_eq!(call(&mut app, "graph_trace", stale)["ok"], false);
        assert_eq!(app.live_graph_session().unwrap().snapshot(), graph);
        let traced = call(&mut app, "graph_trace", request.clone());
        assert_eq!(traced["ok"], true, "{traced}");
        assert_eq!(traced["root_changed"], false);
        assert_eq!(traced["phase"], "queued");
        assert_eq!(app.font.project.document_revision(), revision);
        assert_eq!(
            app.font.project.capture_document_layer(&address).unwrap(),
            before
        );
        assert_eq!(
            app.font
                .project
                .capture_document_layer(&green_address)
                .unwrap(),
            green
        );
        assert_eq!(app.live_graph_session().unwrap().snapshot(), graph);
        let trace_status_request = json!({
            "expected_document_epoch":epoch,"identity":graph.identity,"handle":traced["handle"]
        });
        let trace_deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let status = call(&mut app, "graph_trace_status", trace_status_request.clone());
            assert_eq!(status["ok"], true, "{status}");
            match status["phase"].as_str() {
                Some("completed") => {
                    assert!(status["mutation"]["receipt"].is_object());
                    assert_eq!(status["grading"]["target"]["grade"], "red");
                    assert_eq!(
                        status["grading"]["references"][0]["layer"]["grade"],
                        "green"
                    );
                    break;
                }
                Some("queued" | "running") => {
                    assert!(Instant::now() < trace_deadline, "{status}");
                    std::thread::sleep(Duration::from_millis(10));
                }
                _ => panic!("{status}"),
            }
        }
        let graph = app.live_graph_session().unwrap().snapshot();
        let trace = &graph.graph.node(node).unwrap().values["parameters"]["calibrated_trace"];
        assert_eq!(trace["target"], target);
        assert_eq!(trace["grading"]["target"]["grade"], "red");
        assert_eq!(trace["grading"]["references"][0]["layer"]["grade"], "green");
        assert_eq!(trace["calibration"]["font_units_per_pixel"], 2.0);
        assert!(
            trace["image_sha256"]
                .as_str()
                .unwrap()
                .starts_with("sha256:")
        );
        let mut mismatched_retry = request.clone();
        mismatched_retry["calibration"]["font_x_at_left"] = json!(10.0);
        assert_eq!(
            call(&mut app, "graph_trace", mismatched_retry.clone())["ok"],
            false
        );
        let replay = call(&mut app, "graph_trace", request.clone());
        assert_eq!(replay["replayed"], true, "{replay}");
        assert_eq!(replay["handle"], traced["handle"]);
        let run = call(
            &mut app,
            "graph_run",
            json!({
                "expected_document_epoch":epoch,
                "guard":{"identity":graph.identity,"semantic_revision":graph.semantic_revision,
                    "semantic_hash":graph.semantic_hash},
                "actor":"trace-test","operation_key":"run-one","source":0,"glyphs":["A"]
            }),
        );
        assert_eq!(run["ok"], true, "{run}");
        let handle = run["run"]["receipt"]["handle"].clone();
        let status_request =
            json!({"expected_document_epoch":epoch,"identity":graph.identity,"handle":handle});
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let status = call(&mut app, "graph_status", status_request.clone());
            match status["run"]["status"].as_str() {
                Some("completed") => break,
                Some("queued" | "running") => {
                    assert!(Instant::now() < deadline, "{status}");
                    std::thread::sleep(Duration::from_millis(10));
                }
                _ => panic!("{status}"),
            }
        }
        let original = call(
            &mut app,
            "graph_image",
            json!({
                "expected_document_epoch":epoch,"identity":graph.identity,"handle":handle,"branch":"original"
            }),
        );
        let changed = call(
            &mut app,
            "graph_image",
            json!({
                "expected_document_epoch":epoch,"identity":graph.identity,"handle":handle,"branch":"changed"
            }),
        );
        assert_eq!(original["ok"], true, "{original}");
        assert_eq!(changed["ok"], true, "{changed}");
        assert_eq!(original["recipe_sha256"], changed["recipe_sha256"]);
        assert_ne!(original["font_sha256"], changed["font_sha256"]);
        assert_eq!(
            app.font.project.capture_document_layer(&address).unwrap(),
            before
        );
        let apply = call(
            &mut app,
            "graph_apply",
            json!({
                "expected_document_epoch":epoch,"identity":graph.identity,"handle":handle,
                "actor":"trace-test","operation_key":"apply-one","authorization":"user-approved"
            }),
        );
        assert_eq!(apply["ok"], true, "{apply}");
        let after = app.font.project.capture_document_layer(&address).unwrap();
        assert_ne!(after, before);
        assert_eq!(
            after.view().width(),
            before.view().width(),
            "calibrated replacement must preserve the captured advance"
        );
        assert_eq!(
            app.font
                .project
                .capture_document_layer(&green_address)
                .unwrap(),
            green
        );
        app.mode = crate::application::workspace::Mode::Graph;
        app.undo_active_edit(false);
        assert_eq!(
            app.font.project.capture_document_layer(&address).unwrap(),
            before
        );
        app.undo_active_edit(true);
        assert_eq!(
            app.font.project.capture_document_layer(&address).unwrap(),
            after
        );
        assert_eq!(
            call(&mut app, "graph_trace_release", trace_status_request)["ok"],
            true
        );
        let released_replay = call(&mut app, "graph_trace", request);
        assert_eq!(released_replay["phase"], "released");
        assert_eq!(released_replay["replayed"], true);
        assert_eq!(call(&mut app, "graph_trace", mismatched_retry)["ok"], false);
    }

    fn check_node_commands(generate_geometry: bool) {
        let mut project = Project::new_font(std::env::temp_dir().join("graph-command-unsaved.ufo"));
        project
            .add_document_glyph("A", 400.0, Some(u32::from('A')))
            .unwrap();
        let layer = project
            .document_source(project.source_id(0).unwrap())
            .unwrap()
            .default_layer();
        project
            .edit_document_layer("A", &layer, |draft| {
                draft.add_shape_contour(kurbo::Rect::new(40.0, 0.0, 360.0, 700.0), false)?;
                draft.set_width(400.0)?;
                Ok(())
            })
            .unwrap();
        let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
        let epoch = app.live.as_ref().unwrap().document_epoch().to_owned();
        let discovery = call(
            &mut app,
            "graph_discover",
            json!({"expected_document_epoch":epoch}),
        );
        assert_eq!(discovery["ok"], true, "{discovery}");
        assert_eq!(discovery["discovery"]["schema_version"], 1);
        assert!(discovery["discovery"]["node_types"].is_array());
        assert!(discovery["recipe_authoring"].is_string());
        let identity = discovery["identity"].clone();
        let graph_snapshot = call(
            &mut app,
            "graph_snapshot",
            json!({"expected_document_epoch":epoch,"identity":identity}),
        );
        assert_eq!(graph_snapshot["ok"], true, "{graph_snapshot}");
        assert_eq!(graph_snapshot["snapshot"]["identity"], identity);
        assert!(graph_snapshot["snapshot"]["graph"]["nodes"].is_array());
        let script = if generate_geometry {
            r#"import json, sys
p=json.load(sys.stdin)
contours=[{"points":[{"x":420,"y":0,"type":"line","smooth":False},{"x":480,"y":0,"type":"line","smooth":False},{"x":480,"y":600,"type":"line","smooth":False}]}]
edits=[{"target":layer["guard"],"operations":[{"op":"set_width","width":layer["width"]+100},{"op":"append_contours","contours":contours}]} for layer in p["layers"]]
json.dump({"schema_version":2,"job_id":p["job_id"],"input_hash":p["input_hash"],"report":"Append geometry and increase widths","reads":[],"edits":edits},sys.stdout)
"#
        } else {
            r#"import json, sys
p=json.load(sys.stdin)
edits=[{"target":layer["guard"],"operations":[{"op":"set_width","width":layer["width"]+100}]} for layer in p["layers"]]
json.dump({"schema_version":1,"job_id":p["job_id"],"input_hash":p["input_hash"],"report":"Increase selected widths by 100","reads":[],"edits":edits},sys.stdout)
"#
        };
        let session = app.live_graph_session_mut().unwrap();
        let snapshot = session.snapshot();
        let edits = snapshot
            .graph
            .nodes
            .iter()
            .filter_map(|node| {
                if node.type_name == "live.python" {
                    Some(GraphEdit::SetValue {
                        node: node.id,
                        field: "code".into(),
                        value: json!(script),
                    })
                } else if node.type_name == "live.proof" {
                    let mut recipe = node.values["recipe"].clone();
                    recipe["text"] = json!("AA");
                    Some(GraphEdit::SetValue {
                        node: node.id,
                        field: "recipe".into(),
                        value: recipe,
                    })
                } else {
                    None
                }
            })
            .collect();
        session
            .mutate_interactive(GraphInteractiveMutationRequest {
                guard: GraphGuard {
                    identity: snapshot.identity,
                    revision: snapshot.revision,
                },
                mutation: GraphMutation::Patch { edits },
            })
            .unwrap();
        let snapshot = app.live_graph_session().unwrap().snapshot();
        let request = json!({"expected_document_epoch":epoch,"guard":{"identity":identity,"semantic_revision":snapshot.semantic_revision,"semantic_hash":snapshot.semantic_hash},"actor":"graph-test","operation_key":"run-one","source":0,"glyphs":["A"]});
        let started = call(&mut app, "graph_run", request.clone());
        assert_eq!(started["ok"], true, "{started}");
        assert_eq!(started["replayed"], false);
        assert_eq!(started["run"]["disposition"], "applied");
        let handle = started["run"]["receipt"]["handle"].clone();
        let status_request =
            json!({"expected_document_epoch":epoch,"identity":identity,"handle":handle});
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let status = call(&mut app, "graph_status", status_request.clone());
            assert_eq!(status["ok"], true, "{status}");
            assert!(status.get("report").is_some());
            assert!(status.get("stderr").is_some());
            assert_eq!(status["report"].is_null(), status["stderr"].is_null());
            match status["run"]["status"].as_str() {
                Some("completed") => {
                    assert_eq!(status["can_apply"], true);
                    for output in status["run"]["outputs"].as_array().unwrap() {
                        if output["value"]["kind"] == "proof" {
                            assert!(
                                output["value"]["content_sha256"]
                                    .as_str()
                                    .unwrap()
                                    .starts_with("sha256:")
                            );
                        }
                    }
                    break;
                }
                Some("queued" | "running") => {
                    assert!(Instant::now() < deadline, "{status}");
                    std::thread::sleep(Duration::from_millis(10));
                }
                _ => panic!("{status}"),
            }
        }
        assert_eq!(
            app.font
                .project
                .document_layer("A", &layer)
                .unwrap()
                .width(),
            400.0
        );
        let mut image_request = status_request.clone();
        image_request["branch"] = json!("original");
        let original = call(&mut app, "graph_image", image_request.clone());
        image_request["branch"] = json!("changed");
        let changed = call(&mut app, "graph_image", image_request);
        assert_eq!(original["ok"], true, "{original}");
        assert_eq!(changed["ok"], true, "{changed}");
        assert_eq!(original["recipe"]["text"], "AA");
        assert!(original["glyphs"].is_array());
        assert!(original["png_base64"].is_string());
        assert_ne!(original["font_sha256"], changed["font_sha256"]);
        assert_ne!(original["png_base64"], changed["png_base64"]);
        app = capture_completed_comparison(app);
        let apply = json!({"expected_document_epoch":epoch,"identity":identity,"handle":handle,"actor":"graph-test","operation_key":"apply-one","authorization":"user-approved"});
        let address = runebender::font::variable::GlyphLayerAddress {
            glyph: "A".into(),
            layer: layer.clone(),
        };
        let before = app.font.project.capture_document_layer(&address).unwrap();
        let state = app.live_graph.as_ref().unwrap();
        let retained = state
            .execution
            .apply_request(
                &state.session,
                &GraphDocumentState {
                    document_epoch: epoch.clone(),
                    document_revision: app.font.project.document_revision(),
                },
                serde_json::from_value(handle.clone()).unwrap(),
                None,
                "graph-test",
                "apply-one",
                "user-approved",
            )
            .unwrap();
        let preview = app
            .font
            .project
            .preview_document_edit_transaction(&retained.candidate)
            .unwrap();
        assert_eq!(preview.len(), 1);
        let applied = call(&mut app, "graph_apply", apply.clone());
        assert_eq!(applied["ok"], true, "{applied}");
        assert_eq!(
            app.font.project.capture_document_layer(&address).unwrap(),
            preview[0],
            "Apply must retain the exact preview geometry and stable identities"
        );
        assert_eq!(
            app.font
                .project
                .document_layer("A", &layer)
                .unwrap()
                .width(),
            500.0
        );
        app.mode = crate::application::workspace::Mode::Graph;
        let graph_before_undo =
            serde_json::to_value(app.live_graph_session().unwrap().snapshot()).unwrap();
        assert!(app.can_metadata_history_step(false));
        app.undo_active_edit(false);
        assert_eq!(
            app.font
                .project
                .document_layer("A", &layer)
                .unwrap()
                .width(),
            400.0
        );
        assert_eq!(
            app.font.project.capture_document_layer(&address).unwrap(),
            before
        );
        assert!(app.can_metadata_history_step(true));
        app.undo_active_edit(true);
        assert_eq!(
            app.font.project.capture_document_layer(&address).unwrap(),
            preview[0]
        );
        assert_eq!(
            app.font
                .project
                .document_layer("A", &layer)
                .unwrap()
                .width(),
            500.0
        );
        app.undo_active_edit(false);
        assert_eq!(
            serde_json::to_value(app.live_graph_session().unwrap().snapshot()).unwrap(),
            graph_before_undo
        );
        let replay = call(&mut app, "graph_apply", apply);
        assert_eq!(replay["replayed"], true);
        assert_eq!(replay["receipt"], applied["receipt"]);
        assert_eq!(replay["history_state"], "undone");
        assert_eq!(replay["root_changed"], false);
        assert_eq!(
            app.font
                .project
                .document_layer("A", &layer)
                .unwrap()
                .width(),
            400.0
        );
        let run_replay = call(&mut app, "graph_run", request);
        assert_eq!(run_replay["replayed"], true);
        assert_eq!(run_replay["run"], started["run"]);
        assert_eq!(
            call(&mut app, "graph_status", status_request.clone())["stale"],
            true
        );
        assert_eq!(
            call(&mut app, "graph_release", status_request)["released"],
            true
        );
    }
}
