// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Native execution adapter for bounded live Python DAGs and legacy comparisons.
//!
//! This module borrows the Workspace-owned Python queue and never drains jobs owned by another
//! consumer.
//! Each successful recipe extends an immutable canonical transaction from its direct parent.
//! Independent branches retain separate candidates and continue after a sibling failure.
//! The retained base compiler input and staged transaction are handed to the compiled-proof owner;
//! applying the result remains a separate receipt-backed font command.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use runebender::automation::agent_edit::AgentEditRequest;
use runebender::automation::agent_session::{AgentOperationRejection, AgentPayloadDigest};
use runebender::automation::script_recipe::{self, ScriptRecipeInput, ScriptRecipeResult};
use runebender::font::compiler::proof::{CompileProofInput, CompiledProofRecipe};
use runebender::font::project::{CanonicalDocumentEditTransaction, Project};
use runebender::workflows::nodes_session::{
    GraphCancelOutcome, GraphCancelRequest, GraphCancelResponse, GraphDocumentState,
    GraphFontCapture, GraphNodeOutput, GraphNodeOutputValue, GraphProofScope,
    GraphReceiptDisposition, GraphRunCompletion, GraphRunError, GraphRunHandle, GraphRunIdentity,
    GraphRunInspection, GraphRunOutcome, GraphRunRequest, GraphRunResponse, GraphRunStatus,
    GraphRunWork, GraphSemanticGuard, GraphSession, GraphVersionLineage,
};
use serde_json::Value;
#[cfg(test)]
use sha2::{Digest, Sha256};

use crate::application::platform::script_jobs::{
    ScriptJobFailure, ScriptJobHandle, ScriptJobOutcome, ScriptJobQueue, ScriptJobRequest,
    ScriptJobStatus, ScriptJobSubmitError,
};

const MAX_LIVE_GRAPH_RUNS: usize = 8;

/// Application phase layered over the engine's durable run status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LiveGraphPhase {
    /// The next captured graph node can be dispatched.
    Ready,
    /// Python job is waiting in the shared queue.
    ScriptQueued,
    /// Python process is active.
    ScriptRunning,
    /// One captured proof input is ready for the shared compiler queue.
    ProofReady,
    /// The compiled-proof owner has accepted one node request.
    ProofsRunning,
    /// `GraphSession` owns the terminal output or error.
    Terminal(GraphRunStatus),
    /// Heavy artifacts were explicitly released.
    Released,
}

/// Inputs captured on the application thread before a run starts.
pub(crate) struct LiveGraphSubmitRequest {
    /// Layout-independent graph guard.
    pub(crate) guard: GraphSemanticGuard,
    /// Actor owning the graph-run retry key.
    pub(crate) actor: String,
    /// Idempotency key for this exact run.
    pub(crate) operation_key: String,
    /// Explicit graph execution contract version.
    pub(crate) execution_version: u32,
    /// Immutable scoped recipe input captured from Project.
    pub(crate) recipe_input: ScriptRecipeInput,
    /// Frozen whole-family compiler input used by the unchanged proof.
    pub(crate) base_proof_input: CompileProofInput,
}

/// Submission result shared with native UI or a thin agent adapter.
#[derive(Clone, Debug)]
pub(crate) struct LiveGraphSubmitResponse {
    /// Durable graph run receipt.
    pub(crate) graph: GraphRunResponse,
}

/// Staged output handed to the existing compiled-proof owner.
#[derive(Clone, Debug)]
pub(crate) struct LiveGraphProofRequest {
    /// Exact graph run identity.
    pub(crate) identity: GraphRunIdentity,
    /// Producing proof node.
    pub(crate) node: u32,
    /// Frozen whole-family compiler input for this node's input version.
    pub(crate) input: CompileProofInput,
    /// Captured typed specimen recipe.
    pub(crate) proof_recipe: CompiledProofRecipe,
}

/// Explicit Apply intent paired with the exact retained canonical candidate.
#[derive(Debug)]
pub(crate) struct LiveGraphApplyRequest {
    pub(crate) request: AgentEditRequest,
    pub(crate) candidate: CanonicalDocumentEditTransaction,
    pub(crate) payload_digest: Option<AgentPayloadDigest>,
}

/// Exact proof artifact identities returned by the existing proof queue.
pub(crate) struct LiveGraphProofOutputs {
    pub(crate) node: u32,
    pub(crate) artifact_id: String,
    pub(crate) content_sha256: String,
    pub(crate) canonical_input_sha256: String,
    pub(crate) font_sha256: String,
}

/// Script result exposed to both native UI and the thin agent adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LiveGraphResultSummary {
    /// Bounded report returned by the strict recipe contract.
    pub(crate) report: String,
    /// Bounded Python diagnostics captured by the shared process queue.
    pub(crate) stderr: String,
    /// Whether exactly one successfully proofed result is eligible for implicit selection.
    pub(crate) can_apply: bool,
}

/// Cancellation effects for the queue owners.
#[derive(Clone, Debug)]
pub(crate) struct LiveGraphCancelResponse {
    /// Durable graph cancellation receipt.
    pub(crate) graph: GraphCancelResponse,
    /// Whether the proof owner must cancel its retained handles.
    pub(crate) cancel_proofs: bool,
}

/// Stable application adapter error category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LiveGraphExecutionErrorCode {
    /// Bounded adapter retention is full.
    Capacity,
    /// Host capture does not match the graph or recipe input.
    Capture,
    /// Graph session rejected a command.
    Graph,
    /// Handle is not retained by this adapter.
    UnknownRun,
    /// Action is not valid in the current phase.
    WrongPhase,
    /// The graph or document changed after this run completed.
    Stale,
    /// Python result could not stage against current canonical state.
    Staging,
    /// Proof recipe or proof outputs are invalid.
    Proof,
}

/// Error from the thin native execution adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LiveGraphExecutionError {
    pub(crate) code: LiveGraphExecutionErrorCode,
    pub(crate) message: String,
}

impl LiveGraphExecutionError {
    fn new(code: LiveGraphExecutionErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for LiveGraphExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for LiveGraphExecutionError {}

#[derive(Debug)]
struct LiveGraphRecord {
    actor: String,
    operation_key: String,
    execution_version: u32,
    script: Option<ScriptJobHandle>,
    active_script_node: Option<u32>,
    active_script_input: Option<ScriptRecipeInput>,
    work: GraphRunWork,
    root_input: Option<ScriptRecipeInput>,
    root_input_sha256: String,
    base_input: Option<CompileProofInput>,
    proof_request: Option<LiveGraphProofRequest>,
    candidates: BTreeMap<u32, CanonicalDocumentEditTransaction>,
    results: BTreeMap<u32, (ScriptRecipeResult, String)>,
    outputs: Vec<GraphNodeOutput>,
    errors: Vec<GraphRunError>,
    finished: BTreeSet<u32>,
    phase: LiveGraphPhase,
}

/// Bounded state connecting `GraphSession` to the shared Python and compiled-proof owners.
#[derive(Debug, Default)]
pub(crate) struct LiveGraphExecution {
    records: BTreeMap<GraphRunHandle, LiveGraphRecord>,
    requests: BTreeMap<(String, String), GraphRunHandle>,
}

impl LiveGraphExecution {
    /// Capture a validated graph and start its first available operation.
    pub(crate) fn submit(
        &mut self,
        session: &mut GraphSession,
        queue: &ScriptJobQueue,
        project: &Project,
        request: LiveGraphSubmitRequest,
    ) -> Result<LiveGraphSubmitResponse, LiveGraphExecutionError> {
        let request_key = (request.actor.clone(), request.operation_key.clone());
        if let Some(handle) = self.requests.get(&request_key).copied() {
            let record = self.records.get(&handle).ok_or_else(|| {
                err(
                    LiveGraphExecutionErrorCode::UnknownRun,
                    "graph retry has only a released tombstone",
                )
            })?;
            validate_retry_request(record, &request)?;
            let graph = session
                .start_run(record.original_request())
                .map_err(graph_error)?;
            debug_assert_eq!(
                graph.disposition,
                GraphReceiptDisposition::Replayed,
                "an exact retained run must replay its receipt"
            );
            return Ok(LiveGraphSubmitResponse { graph });
        }
        if self.retained_count() >= MAX_LIVE_GRAPH_RUNS {
            return Err(err(
                LiveGraphExecutionErrorCode::Capacity,
                "at most eight live graph runs may be retained",
            ));
        }
        validate_submit_capture(session, project, &request)?;
        let capture = session
            .capture_run_version(
                graph_font_capture(&request),
                request.recipe_input.input_hash.clone(),
                request.execution_version,
            )
            .map_err(graph_error)?;
        let graph = session
            .start_run(GraphRunRequest {
                guard: request.guard,
                actor: request.actor.clone(),
                operation_key: request.operation_key.clone(),
                capture,
            })
            .map_err(graph_error)?;
        let work = session
            .claim_run(graph.receipt.handle)
            .map_err(graph_error)?;
        let record = LiveGraphRecord {
            actor: request.actor,
            operation_key: request.operation_key,
            execution_version: request.execution_version,
            script: None,
            active_script_node: None,
            active_script_input: None,
            work,
            root_input_sha256: request.recipe_input.input_hash.clone(),
            root_input: Some(request.recipe_input),
            base_input: Some(request.base_proof_input),
            proof_request: None,
            candidates: BTreeMap::new(),
            results: BTreeMap::new(),
            outputs: Vec::new(),
            errors: Vec::new(),
            finished: BTreeSet::new(),
            phase: LiveGraphPhase::Ready,
        };
        let handle = record.work.handle;
        self.requests.insert(request_key, handle);
        self.records.insert(handle, record);
        if let Some(record) = self.records.get_mut(&handle) {
            drive_ready(session, queue, project, record);
        }
        Ok(LiveGraphSubmitResponse { graph })
    }

    /// Poll only owned queue handles and schedule at most one operation per run.
    pub(crate) fn poll_scripts(
        &mut self,
        session: &mut GraphSession,
        queue: &ScriptJobQueue,
        project: &Project,
    ) -> Vec<(GraphRunHandle, LiveGraphPhase)> {
        let mut changed = Vec::new();
        for (handle, record) in &mut self.records {
            let old = record.phase;
            match record.phase {
                LiveGraphPhase::Ready => drive_ready(session, queue, project, record),
                LiveGraphPhase::ScriptQueued | LiveGraphPhase::ScriptRunning => {
                    poll_one_script(session, queue, project, record);
                }
                _ => {}
            }
            if record.phase != old {
                changed.push((*handle, record.phase));
            }
        }
        changed
    }

    /// Take one ready proof for the existing compiler queue.
    pub(crate) fn take_proof_request(
        &mut self,
        handle: GraphRunHandle,
    ) -> Result<LiveGraphProofRequest, LiveGraphExecutionError> {
        let record = self.record_mut(handle)?;
        if record.phase != LiveGraphPhase::ProofReady {
            return Err(err(
                LiveGraphExecutionErrorCode::WrongPhase,
                "no proof is ready",
            ));
        }
        let proof = record.proof_request.clone().ok_or_else(|| {
            err(
                LiveGraphExecutionErrorCode::WrongPhase,
                "ready proof request is missing",
            )
        })?;
        record.phase = LiveGraphPhase::ProofsRunning;
        Ok(proof)
    }

    /// Node of the proof currently owned by the compiler queue.
    pub(crate) fn active_proof_node(&self, handle: GraphRunHandle) -> Option<u32> {
        let record = self.records.get(&handle)?;
        (record.phase == LiveGraphPhase::ProofsRunning)
            .then(|| record.proof_request.as_ref().map(|request| request.node))
            .flatten()
    }

    /// Check the exact proof identity and retain its artifact for terminal publication.
    pub(crate) fn publish_proofs(
        &mut self,
        session: &mut GraphSession,
        handle: GraphRunHandle,
        current: &GraphDocumentState,
        proof: LiveGraphProofOutputs,
    ) -> Result<GraphRunInspection, LiveGraphExecutionError> {
        let record = self.record_mut(handle)?;
        if record.phase != LiveGraphPhase::ProofsRunning {
            return Err(err(
                LiveGraphExecutionErrorCode::WrongPhase,
                "no proof is running",
            ));
        }
        let request = record.proof_request.as_ref().ok_or_else(|| {
            err(
                LiveGraphExecutionErrorCode::Proof,
                "proof request is missing",
            )
        })?;
        if proof.node != request.node
            || proof.canonical_input_sha256 != request.input.canonical_input_sha256()
        {
            return Err(err(
                LiveGraphExecutionErrorCode::Proof,
                "proof output does not match its captured node and canonical compiler input",
            ));
        }
        if proof.artifact_id.is_empty()
            || !valid_sha(&proof.content_sha256)
            || !valid_sha(&proof.font_sha256)
        {
            return Err(err(
                LiveGraphExecutionErrorCode::Proof,
                "proof artifact identity is invalid",
            ));
        }
        let recipe_sha256 = record
            .work
            .identity
            .capture
            .proofs
            .iter()
            .find(|item| item.node == proof.node)
            .ok_or_else(|| {
                err(
                    LiveGraphExecutionErrorCode::Proof,
                    "proof capture is missing",
                )
            })?
            .recipe_sha256
            .clone();
        record.outputs.push(GraphNodeOutput {
            node: proof.node,
            value: GraphNodeOutputValue::Proof {
                artifact_id: proof.artifact_id,
                content_sha256: proof.content_sha256,
                canonical_input_sha256: proof.canonical_input_sha256,
                font_sha256: proof.font_sha256,
                recipe_sha256,
                scope: GraphProofScope::CompiledFamily,
            },
        });
        record.finished.insert(proof.node);
        record.proof_request = None;
        record.phase = LiveGraphPhase::Ready;
        inspect_running_or_stale(session, record, current)
    }

    /// Record a failed proof; independent branches may still complete.
    pub(crate) fn fail_proofs(
        &mut self,
        session: &mut GraphSession,
        handle: GraphRunHandle,
        current: &GraphDocumentState,
        message: impl Into<String>,
    ) -> Result<GraphRunInspection, LiveGraphExecutionError> {
        let record = self.record_mut(handle)?;
        if record.phase != LiveGraphPhase::ProofsRunning {
            return Err(err(
                LiveGraphExecutionErrorCode::WrongPhase,
                "no proof is running",
            ));
        }
        let node = record
            .proof_request
            .as_ref()
            .ok_or_else(|| {
                err(
                    LiveGraphExecutionErrorCode::Proof,
                    "proof request is missing",
                )
            })?
            .node;
        node_failed(record, node, "proof_failed", &message.into());
        record.proof_request = None;
        record.phase = LiveGraphPhase::Ready;
        inspect_running_or_stale(session, record, current)
    }

    /// Cancel queued or active work and suppress any later dispatch.
    pub(crate) fn cancel(
        &mut self,
        session: &mut GraphSession,
        queue: &ScriptJobQueue,
        request: GraphCancelRequest,
        current: &GraphDocumentState,
    ) -> Result<LiveGraphCancelResponse, LiveGraphExecutionError> {
        let handle = request.handle;
        let graph = session.cancel_run(request).map_err(graph_error)?;
        let record = self.record_mut(handle)?;
        let cancel_proofs = record.phase == LiveGraphPhase::ProofsRunning;
        match record.phase {
            LiveGraphPhase::ScriptQueued | LiveGraphPhase::ScriptRunning => {
                if let Some(script) = record.script {
                    queue.cancel(script);
                }
            }
            LiveGraphPhase::Ready | LiveGraphPhase::ProofReady => {
                finish_cancelled(session, record, current);
            }
            _ => {}
        }
        if graph.receipt.outcome == GraphCancelOutcome::CancelledBeforeStart {
            record.phase = LiveGraphPhase::Terminal(GraphRunStatus::Cancelled);
            discard_candidate_state(record);
        }
        Ok(LiveGraphCancelResponse {
            graph,
            cancel_proofs,
        })
    }

    /// Finish cancellation after the compiler owner releases its active proof.
    pub(crate) fn finish_proof_cancellation(
        &mut self,
        session: &mut GraphSession,
        handle: GraphRunHandle,
        current: &GraphDocumentState,
    ) -> Result<GraphRunInspection, LiveGraphExecutionError> {
        let record = self.record_mut(handle)?;
        if record.phase != LiveGraphPhase::ProofsRunning {
            return Err(err(
                LiveGraphExecutionErrorCode::WrongPhase,
                "no proof cancellation is pending",
            ));
        }
        let inspection = session
            .complete_run(
                GraphRunCompletion {
                    handle,
                    identity: record.work.identity.clone(),
                    outcome: GraphRunOutcome::Cancelled,
                },
                current,
            )
            .map_err(graph_error)?;
        record.phase = LiveGraphPhase::Terminal(inspection.status);
        discard_candidate_state(record);
        Ok(inspection)
    }

    pub(crate) fn phase(&self, handle: GraphRunHandle) -> Option<LiveGraphPhase> {
        Some(self.records.get(&handle)?.phase)
    }

    /// Resolve an explicit proof node or an unambiguous comparison alias.
    pub(crate) fn proof_node(
        &self,
        handle: GraphRunHandle,
        requested: Option<u32>,
        changed: Option<bool>,
    ) -> Result<u32, LiveGraphExecutionError> {
        let record = self.records.get(&handle).ok_or_else(|| {
            err(
                LiveGraphExecutionErrorCode::UnknownRun,
                "live graph run is not retained",
            )
        })?;
        let output_nodes: Vec<u32> = record
            .outputs
            .iter()
            .filter_map(|output| {
                matches!(output.value, GraphNodeOutputValue::Proof { .. }).then_some(output.node)
            })
            .collect();
        if let Some(node) = requested {
            return output_nodes.contains(&node).then_some(node).ok_or_else(|| {
                err(
                    LiveGraphExecutionErrorCode::Proof,
                    "requested proof node has no retained artifact",
                )
            });
        }
        if let Some(changed) = changed {
            if let Some(comparison) = record.work.plan.comparison() {
                let node = if changed {
                    comparison.changed_proof
                } else {
                    comparison.unchanged_proof
                };
                return output_nodes.contains(&node).then_some(node).ok_or_else(|| {
                    err(
                        LiveGraphExecutionErrorCode::Proof,
                        "comparison proof has no retained artifact",
                    )
                });
            }
            let matches: Vec<u32> = record
                .work
                .plan
                .proofs
                .iter()
                .filter(|proof| {
                    (proof.input != record.work.plan.source_node) == changed
                        && output_nodes.contains(&proof.node)
                })
                .map(|proof| proof.node)
                .collect();
            return unique_node(&matches, "proof alias is ambiguous or unavailable");
        }
        unique_node(&output_nodes, "select one of the retained proof nodes")
    }

    /// Whether a completed run has a unique or explicitly selected proven transform.
    pub(crate) fn can_apply(&self, handle: GraphRunHandle, node: Option<u32>) -> bool {
        let Some(record) = self.records.get(&handle) else {
            return false;
        };
        if !matches!(
            record.phase,
            LiveGraphPhase::Terminal(GraphRunStatus::Completed | GraphRunStatus::PartiallyFailed)
        ) {
            return false;
        }
        let eligible = eligible_apply_nodes(record);
        node.map_or(eligible.len() == 1, |node| eligible.contains(&node))
    }

    pub(crate) fn result_summary(&self, handle: GraphRunHandle) -> Option<LiveGraphResultSummary> {
        let record = self.records.get(&handle)?;
        if record.results.is_empty() && record.errors.is_empty() {
            return None;
        }
        let mut report = String::new();
        let mut stderr = String::new();
        if record.execution_version == 1 {
            if let Some((result, diagnostics)) = record.results.values().next() {
                report = result.report.clone();
                stderr = diagnostics.clone();
            }
        } else {
            for (node, (result, diagnostics)) in &record.results {
                append_bounded(
                    &mut report,
                    &format!("node {node}: {}\n", result.report),
                    65_536,
                );
                if !diagnostics.is_empty() {
                    append_bounded(
                        &mut stderr,
                        &format!("node {node}: {diagnostics}\n"),
                        65_536,
                    );
                }
            }
        }
        for error in &record.errors {
            append_bounded(
                &mut stderr,
                &format!("node {:?}: {}\n", error.node, error.message),
                65_536,
            );
        }
        let terminal = matches!(
            record.phase,
            LiveGraphPhase::Terminal(GraphRunStatus::Completed | GraphRunStatus::PartiallyFailed)
        );
        let can_apply = terminal && eligible_apply_nodes(record).len() == 1;
        Some(LiveGraphResultSummary {
            report,
            stderr,
            can_apply,
        })
    }

    /// Return the exact retained candidate and a graph-bound retry digest.
    pub(crate) fn apply_request(
        &self,
        session: &GraphSession,
        current: &GraphDocumentState,
        handle: GraphRunHandle,
        node: Option<u32>,
        actor: impl Into<String>,
        operation_key: impl Into<String>,
        authorization: impl Into<String>,
    ) -> Result<LiveGraphApplyRequest, LiveGraphExecutionError> {
        let record = self.records.get(&handle).ok_or_else(|| {
            err(
                LiveGraphExecutionErrorCode::UnknownRun,
                "live graph run is not retained",
            )
        })?;
        if !matches!(
            record.phase,
            LiveGraphPhase::Terminal(GraphRunStatus::Completed | GraphRunStatus::PartiallyFailed)
        ) {
            return Err(err(
                LiveGraphExecutionErrorCode::WrongPhase,
                "Apply requires a completed direct proof of the selected transform",
            ));
        }
        let snapshot = session.snapshot();
        let identity = &record.work.identity;
        if current.document_epoch != identity.graph.document_epoch
            || current.document_revision != identity.capture.font.document_revision
            || snapshot.semantic_revision != identity.semantic_revision
            || snapshot.semantic_hash != identity.semantic_hash
        {
            return Err(err(
                LiveGraphExecutionErrorCode::Stale,
                "the graph or document changed; rerun before Apply",
            ));
        }
        let eligible = eligible_apply_nodes(record);
        let selected = if let Some(node) = node {
            eligible.contains(&node).then_some(node).ok_or_else(|| {
                err(
                    LiveGraphExecutionErrorCode::WrongPhase,
                    "selected transform has no successful direct proof",
                )
            })?
        } else {
            unique_node(&eligible, "select a successfully proven transform node")?
        };
        let candidate = record
            .candidates
            .get(&selected)
            .ok_or_else(|| {
                err(
                    LiveGraphExecutionErrorCode::Staging,
                    "selected candidate was released",
                )
            })?
            .clone();
        let result = &record.results[&selected].0;
        let request = AgentEditRequest {
            expected_document_epoch: identity.graph.document_epoch.clone(),
            actor: actor.into(),
            operation_key: operation_key.into(),
            authorization: authorization.into(),
            source: identity.capture.font.source,
            history_name: "Apply Nodes Python result".into(),
            reads: result.reads.clone(),
            edits: result.edits.clone(),
        };
        let payload_digest = if record.execution_version == 1 {
            None
        } else {
            let content_hash = record
                .outputs
                .iter()
                .find(|output| output.node == selected)
                .and_then(|output| match &output.value {
                    GraphNodeOutputValue::FontVersion { content_sha256, .. } => {
                        Some(content_sha256)
                    }
                    _ => None,
                })
                .ok_or_else(|| {
                    err(
                        LiveGraphExecutionErrorCode::Staging,
                        "selected version hash is missing",
                    )
                })?;
            let bytes = serde_json::to_vec(&serde_json::json!({
                "request": request,
                "run_identity": identity,
                "selected_node": selected,
                "content_hash": content_hash,
            }))
            .expect("typed apply payload serializes");
            Some(AgentPayloadDigest::sha256(&bytes))
        };
        Ok(LiveGraphApplyRequest {
            request,
            candidate,
            payload_digest,
        })
    }

    pub(crate) fn release(
        &mut self,
        session: &mut GraphSession,
        queue: &ScriptJobQueue,
        handle: GraphRunHandle,
    ) -> Result<bool, LiveGraphExecutionError> {
        let released = session.release_run(handle).map_err(graph_error)?;
        let record = self.record_mut(handle)?;
        if released {
            if let Some(script) = record.script.take() {
                let _ = queue.discard(script);
            }
            record.root_input = None;
            record.base_input = None;
            record.active_script_input = None;
            record.proof_request = None;
            record.candidates.clear();
            record.results.clear();
            record.outputs.clear();
            record.errors.clear();
            record.work.graph.nodes.clear();
            record.work.graph.links.clear();
            record.phase = LiveGraphPhase::Released;
        }
        Ok(released)
    }

    fn retained_count(&self) -> usize {
        self.records
            .values()
            .filter(|record| record.phase != LiveGraphPhase::Released)
            .count()
    }

    fn record_mut(
        &mut self,
        handle: GraphRunHandle,
    ) -> Result<&mut LiveGraphRecord, LiveGraphExecutionError> {
        self.records.get_mut(&handle).ok_or_else(|| {
            err(
                LiveGraphExecutionErrorCode::UnknownRun,
                "live graph run is not retained",
            )
        })
    }
}

impl LiveGraphRecord {
    fn original_request(&self) -> GraphRunRequest {
        let identity = &self.work.identity;
        GraphRunRequest {
            guard: GraphSemanticGuard {
                identity: identity.graph.clone(),
                semantic_revision: identity.semantic_revision,
                semantic_hash: identity.semantic_hash.clone(),
            },
            actor: self.actor.clone(),
            operation_key: self.operation_key.clone(),
            capture: identity.capture.clone(),
        }
    }
}

fn drive_ready(
    session: &mut GraphSession,
    queue: &ScriptJobQueue,
    project: &Project,
    record: &mut LiveGraphRecord,
) {
    if record.phase != LiveGraphPhase::Ready {
        return;
    }
    let current = project.graph_document_state(&record.work.identity.graph.document_epoch);
    if session
        .inspect_run(record.work.handle)
        .is_some_and(|run| run.status == GraphRunStatus::CancellationRequested)
    {
        finish_cancelled(session, record, &current);
        return;
    }
    if is_stale(session, record, &current) {
        complete_record(session, record, &current);
        return;
    }
    if record.execution_version == 1 && !record.errors.is_empty() {
        complete_record(session, record, &current);
        return;
    }
    let mut order = record.work.plan.order.clone();
    if record.execution_version == 1 {
        let script = record.work.plan.scripts[0].node;
        order.retain(|node| *node != script);
        order.insert(1, script);
    }
    for node in order {
        if node == record.work.plan.source_node || record.finished.contains(&node) {
            continue;
        }
        let script = record
            .work
            .plan
            .scripts
            .iter()
            .find(|item| item.node == node)
            .copied();
        let proof = record
            .work
            .plan
            .proofs
            .iter()
            .find(|item| item.node == node)
            .copied();
        let parent = script
            .or(proof)
            .expect("plan order contains only executable nodes")
            .input;
        if parent != record.work.plan.source_node && !record.finished.contains(&parent) {
            continue;
        }
        if parent != record.work.plan.source_node && !record.candidates.contains_key(&parent) {
            node_failed(
                record,
                node,
                "dependency_failed",
                "input transform did not produce a version",
            );
            continue;
        }
        if let Some(script) = script {
            match script_input(record, project, script.node, script.input) {
                Ok(input) => {
                    let code = record
                        .work
                        .graph
                        .node(node)
                        .and_then(|node| node.values.get("code"))
                        .and_then(Value::as_str)
                        .expect("validated script retains code")
                        .to_owned();
                    match queue.submit(ScriptJobRequest {
                        input: input.clone(),
                        script: code,
                    }) {
                        Ok(handle) => {
                            record.script = Some(handle);
                            record.active_script_node = Some(node);
                            record.active_script_input = Some(input);
                            record.phase = LiveGraphPhase::ScriptQueued;
                        }
                        Err(error) => {
                            node_failed(record, node, "script_submit", &submit_error(&error));
                        }
                    }
                }
                Err(message) => node_failed(record, node, "script_capture", &message),
            }
            if record.phase != LiveGraphPhase::Ready {
                return;
            }
            if record.execution_version == 1 && !record.errors.is_empty() {
                complete_record(session, record, &current);
                return;
            }
        } else if let Some(proof) = proof {
            let Some(base) = record.base_input.as_ref() else {
                node_failed(
                    record,
                    node,
                    "proof_capture",
                    "base compiler input is missing",
                );
                continue;
            };
            let input = if proof.input == record.work.plan.source_node {
                Ok(base.clone())
            } else {
                base.with_staged_edit(project, &record.candidates[&proof.input])
            };
            let recipe = proof_recipe(&record.work.graph, node);
            match (input, recipe) {
                (Ok(input), Ok(recipe)) => {
                    record.proof_request = Some(LiveGraphProofRequest {
                        identity: record.work.identity.clone(),
                        node,
                        input,
                        proof_recipe: recipe,
                    });
                    record.phase = LiveGraphPhase::ProofReady;
                    return;
                }
                (Err(message), _) | (_, Err(message)) => {
                    node_failed(record, node, "proof_capture", &message);
                }
            }
        }
    }
    complete_record(session, record, &current);
}

fn poll_one_script(
    session: &mut GraphSession,
    queue: &ScriptJobQueue,
    project: &Project,
    record: &mut LiveGraphRecord,
) {
    let Some(handle) = record.script else {
        return;
    };
    let node = record.active_script_node.expect("active job has a node");
    let Some(inspection) = queue.inspect(handle) else {
        node_failed(
            record,
            node,
            "script_unavailable",
            "shared Python queue no longer retains this job",
        );
        record.script = None;
        record.active_script_node = None;
        record.active_script_input = None;
        record.phase = LiveGraphPhase::Ready;
        return;
    };
    match inspection.status {
        ScriptJobStatus::Queued => return,
        ScriptJobStatus::Running => {
            record.phase = LiveGraphPhase::ScriptRunning;
            return;
        }
        _ => {}
    }
    let current = project.graph_document_state(&record.work.identity.graph.document_epoch);
    if session
        .inspect_run(record.work.handle)
        .is_some_and(|run| run.status == GraphRunStatus::CancellationRequested)
    {
        finish_cancelled(session, record, &current);
    } else if is_stale(session, record, &current) {
        complete_record(session, record, &current);
    } else if let Some(outcome) = inspection.outcome {
        match outcome {
            ScriptJobOutcome::Completed { result, stderr } => {
                if let Err(message) = stage_result(record, project, node, result, stderr) {
                    node_failed(record, node, "recipe_stage", &message);
                }
            }
            ScriptJobOutcome::Failed { failure, stderr } => {
                node_failed(
                    record,
                    node,
                    "script_failed",
                    &script_failure(&failure, &stderr),
                );
            }
            ScriptJobOutcome::Cancelled { .. } => finish_cancelled(session, record, &current),
        }
        if !matches!(record.phase, LiveGraphPhase::Terminal(_)) {
            record.phase = LiveGraphPhase::Ready;
        }
    }
    let _ = queue.discard(handle);
    record.script = None;
    record.active_script_node = None;
    record.active_script_input = None;
}

fn script_input(
    record: &LiveGraphRecord,
    project: &Project,
    node: u32,
    parent: u32,
) -> Result<ScriptRecipeInput, String> {
    let root = record
        .root_input
        .as_ref()
        .ok_or("root recipe capture is missing")?;
    let parameters = record
        .work
        .graph
        .node(node)
        .and_then(|node| node.values.get("parameters"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let Value::Object(parameters) = parameters else {
        return Err("script parameters must be an object".into());
    };
    let parameters = parameters.into_iter().collect();
    if record.execution_version == 1 {
        return Ok(root.clone());
    }
    let job_id = format!("graph-{}-{node}", record.work.handle.get());
    if parent == record.work.plan.source_node {
        ScriptRecipeInput::new(job_id, root.source, parameters, root.layers.clone())
            .map_err(|error| error.to_string())
    } else {
        let glyphs: Vec<String> = root
            .layers
            .iter()
            .map(|layer| layer.guard.glyph.clone())
            .collect();
        script_recipe::capture_staged(
            project,
            &record.candidates[&parent],
            runebender::font::variable::SourceId(root.source),
            &glyphs,
            job_id,
            parameters,
        )
    }
}

fn stage_result(
    record: &mut LiveGraphRecord,
    project: &Project,
    node: u32,
    result: ScriptRecipeResult,
    stderr: String,
) -> Result<(), String> {
    if result.edits.is_empty() {
        return Err("live.python returned no guarded edits".into());
    }
    let parent = record
        .work
        .plan
        .scripts
        .iter()
        .find(|item| item.node == node)
        .expect("script is in plan")
        .input;
    let request = AgentEditRequest {
        expected_document_epoch: record.work.identity.graph.document_epoch.clone(),
        actor: record.actor.clone(),
        operation_key: format!("{}-{node}", record.operation_key),
        authorization: "preview-only".into(),
        source: record.work.identity.capture.font.source,
        history_name: "Apply Nodes Python result".into(),
        reads: result.reads.clone(),
        edits: result.edits.clone(),
    };
    let staged = if parent == record.work.plan.source_node {
        request.stage(project)
    } else {
        request.stage_after(project, &record.candidates[&parent])
    }
    .map_err(|error| match error {
        AgentOperationRejection::InvalidRequest(message) => message,
        AgentOperationRejection::Transaction(error) => error.to_string(),
    })?;
    let derived = record
        .base_input
        .as_ref()
        .ok_or("base compiler input is missing")?
        .with_staged_edit(project, &staged)?;
    let content_sha256 = derived.canonical_input_sha256().to_owned();
    let lineage = if record.execution_version == 1 {
        None
    } else {
        let parent_hash = if parent == record.work.plan.source_node {
            record.work.identity.capture.font.capture_sha256.clone()
        } else {
            record
                .outputs
                .iter()
                .find(|output| output.node == parent)
                .and_then(|output| match &output.value {
                    GraphNodeOutputValue::FontVersion { content_sha256, .. } => {
                        Some(content_sha256.clone())
                    }
                    _ => None,
                })
                .ok_or("parent version hash is missing")?
        };
        Some(GraphVersionLineage {
            parent_node: parent,
            parent_content_sha256: parent_hash,
            recipe_input_sha256: record
                .active_script_input
                .as_ref()
                .ok_or("script input is missing")?
                .input_hash
                .clone(),
        })
    };
    record.outputs.push(GraphNodeOutput {
        node,
        value: GraphNodeOutputValue::FontVersion {
            source: record.work.identity.capture.font.source,
            version_id: if record.execution_version == 1 {
                format!("graph-run-{}-staged", record.work.handle.get())
            } else {
                format!("graph-run-{}-node-{node}", record.work.handle.get())
            },
            version_revision: 0,
            content_sha256,
            lineage,
        },
    });
    record.candidates.insert(node, staged);
    if record.execution_version == 2 {
        record.outputs.push(GraphNodeOutput {
            node,
            value: GraphNodeOutputValue::Report {
                text: result.report.clone(),
            },
        });
    }
    record.results.insert(node, (result, stderr));
    record.finished.insert(node);
    Ok(())
}

fn eligible_apply_nodes(record: &LiveGraphRecord) -> Vec<u32> {
    record
        .work
        .plan
        .scripts
        .iter()
        .filter(|script| {
            record.candidates.contains_key(&script.node)
                && record.work.plan.proofs.iter().any(|proof| {
                    proof.input == script.node
                        && record.outputs.iter().any(|output| {
                            output.node == proof.node
                                && matches!(output.value, GraphNodeOutputValue::Proof { .. })
                        })
                })
        })
        .map(|script| script.node)
        .collect()
}

fn node_failed(record: &mut LiveGraphRecord, node: u32, code: &str, message: &str) {
    record.errors.push(run_error(code, message, Some(node)));
    record.finished.insert(node);
}

fn complete_record(
    session: &mut GraphSession,
    record: &mut LiveGraphRecord,
    current: &GraphDocumentState,
) {
    let outputs = record.outputs.clone();
    let errors = record.errors.clone();
    let outcome = if errors.is_empty() {
        GraphRunOutcome::Completed(outputs)
    } else if outputs.is_empty() || record.execution_version == 1 {
        GraphRunOutcome::Failed(errors)
    } else {
        GraphRunOutcome::Partial { outputs, errors }
    };
    if let Ok(inspection) = session.complete_run(
        GraphRunCompletion {
            handle: record.work.handle,
            identity: record.work.identity.clone(),
            outcome,
        },
        current,
    ) {
        record.phase = LiveGraphPhase::Terminal(inspection.status);
        if !matches!(
            inspection.status,
            GraphRunStatus::Completed | GraphRunStatus::PartiallyFailed
        ) {
            discard_candidate_state(record);
        }
    }
}

fn finish_cancelled(
    session: &mut GraphSession,
    record: &mut LiveGraphRecord,
    current: &GraphDocumentState,
) {
    if let Ok(inspection) = session.complete_run(
        GraphRunCompletion {
            handle: record.work.handle,
            identity: record.work.identity.clone(),
            outcome: GraphRunOutcome::Cancelled,
        },
        current,
    ) {
        record.phase = LiveGraphPhase::Terminal(inspection.status);
        discard_candidate_state(record);
    }
}

fn discard_candidate_state(record: &mut LiveGraphRecord) {
    record.outputs.clear();
    record.candidates.clear();
    record.results.clear();
    record.proof_request = None;
}

fn inspect_running_or_stale(
    session: &mut GraphSession,
    record: &mut LiveGraphRecord,
    current: &GraphDocumentState,
) -> Result<GraphRunInspection, LiveGraphExecutionError> {
    if is_stale(session, record, current) {
        complete_record(session, record, current);
    }
    session.inspect_run(record.work.handle).ok_or_else(|| {
        err(
            LiveGraphExecutionErrorCode::UnknownRun,
            "graph run disappeared",
        )
    })
}

fn is_stale(
    session: &GraphSession,
    record: &LiveGraphRecord,
    current: &GraphDocumentState,
) -> bool {
    let identity = &record.work.identity;
    let snapshot = session.snapshot();
    current.document_epoch != identity.graph.document_epoch
        || current.document_revision != identity.capture.font.document_revision
        || snapshot.semantic_revision != identity.semantic_revision
        || snapshot.semantic_hash != identity.semantic_hash
}

fn validate_retry_request(
    record: &LiveGraphRecord,
    request: &LiveGraphSubmitRequest,
) -> Result<(), LiveGraphExecutionError> {
    let identity = &record.work.identity;
    let guard = GraphSemanticGuard {
        identity: identity.graph.clone(),
        semantic_revision: identity.semantic_revision,
        semantic_hash: identity.semantic_hash.clone(),
    };
    if request.guard != guard
        || request.execution_version != record.execution_version
        || request.recipe_input.source != identity.capture.font.source
        || request.recipe_input.input_hash != record.root_input_sha256
        || request.base_proof_input.document_revision() != identity.capture.font.document_revision
        || request.base_proof_input.canonical_input_sha256() != identity.capture.font.capture_sha256
    {
        return Err(err(
            LiveGraphExecutionErrorCode::Capture,
            "retry does not match the original captured run",
        ));
    }
    Ok(())
}

fn graph_font_capture(request: &LiveGraphSubmitRequest) -> GraphFontCapture {
    GraphFontCapture {
        source: request.recipe_input.source,
        document_revision: request.base_proof_input.document_revision(),
        capture_sha256: request.base_proof_input.canonical_input_sha256().to_owned(),
    }
}

fn validate_submit_capture(
    session: &GraphSession,
    project: &Project,
    request: &LiveGraphSubmitRequest,
) -> Result<(), LiveGraphExecutionError> {
    request
        .recipe_input
        .validate()
        .map_err(|error| err(LiveGraphExecutionErrorCode::Capture, error.to_string()))?;
    let plan = session
        .execution_plan(request.execution_version)
        .map_err(graph_error)?;
    if request.execution_version == 1 {
        let node = plan
            .scripts
            .first()
            .expect("version one plan has a script")
            .node;
        let parameters = record_parameters(&session.snapshot().graph, node)?;
        if parameters != request.recipe_input.parameters {
            return Err(err(
                LiveGraphExecutionErrorCode::Capture,
                "script recipe parameters do not match graph parameters",
            ));
        }
    }
    let graph = session.snapshot().graph;
    let mut recipes = Vec::with_capacity(plan.proofs.len());
    for proof in &plan.proofs {
        let recipe = proof_recipe(&graph, proof.node)
            .map_err(|message| err(LiveGraphExecutionErrorCode::Proof, message))?;
        if recipe.normalized_location.len() > 64
            || recipe.normalized_location.len() != project.axes.len()
        {
            return Err(err(
                LiveGraphExecutionErrorCode::Proof,
                "provide one normalized coordinate per document axis (maximum 64)",
            ));
        }
        recipes.push(recipe);
    }
    if plan.comparison().is_some() && recipes[0] != recipes[1] {
        return Err(err(
            LiveGraphExecutionErrorCode::Proof,
            "comparison proofs must use the same recipe",
        ));
    }
    if request.base_proof_input.document_revision() != project.document_revision() {
        return Err(err(
            LiveGraphExecutionErrorCode::Capture,
            "base proof capture does not match the current document revision",
        ));
    }
    if !valid_sha(request.base_proof_input.canonical_input_sha256()) {
        return Err(err(
            LiveGraphExecutionErrorCode::Capture,
            "base proof capture lacks a canonical SHA-256 identity",
        ));
    }
    Ok(())
}

fn record_parameters(
    graph: &runebender::workflows::nodes::NodeGraph,
    node: u32,
) -> Result<BTreeMap<String, Value>, LiveGraphExecutionError> {
    let parameters = graph
        .node(node)
        .and_then(|node| node.values.get("parameters"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let Value::Object(parameters) = parameters else {
        return Err(err(
            LiveGraphExecutionErrorCode::Capture,
            "script parameters must be an object",
        ));
    };
    Ok(parameters.into_iter().collect())
}

fn proof_recipe(
    graph: &runebender::workflows::nodes::NodeGraph,
    node: u32,
) -> Result<CompiledProofRecipe, String> {
    let value = graph
        .node(node)
        .and_then(|node| node.values.get("recipe"))
        .cloned()
        .unwrap_or_else(runebender::workflows::nodes_live::default_proof_recipe);
    let recipe: CompiledProofRecipe =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    recipe.validate()?;
    Ok(recipe)
}

fn unique_node(nodes: &[u32], message: &str) -> Result<u32, LiveGraphExecutionError> {
    match nodes {
        [node] => Ok(*node),
        _ => Err(err(LiveGraphExecutionErrorCode::Proof, message)),
    }
}

fn append_bounded(target: &mut String, value: &str, bound: usize) {
    for ch in value.chars() {
        if target.len() + ch.len_utf8() > bound {
            break;
        }
        target.push(ch);
    }
}

fn valid_sha(value: &str) -> bool {
    let digest = value.strip_prefix("sha256:").unwrap_or(value);
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

trait CurrentProject {
    fn graph_document_state(&self, epoch: &str) -> GraphDocumentState;
}
impl CurrentProject for Project {
    fn graph_document_state(&self, epoch: &str) -> GraphDocumentState {
        GraphDocumentState {
            document_epoch: epoch.into(),
            document_revision: self.document_revision(),
        }
    }
}

fn err(code: LiveGraphExecutionErrorCode, message: impl Into<String>) -> LiveGraphExecutionError {
    LiveGraphExecutionError::new(code, message)
}
fn graph_error(
    error: runebender::workflows::nodes_session::GraphSessionError,
) -> LiveGraphExecutionError {
    err(LiveGraphExecutionErrorCode::Graph, error.to_string())
}
fn submit_error(error: &ScriptJobSubmitError) -> String {
    match error {
        ScriptJobSubmitError::InvalidConfiguration(message)
        | ScriptJobSubmitError::InvalidRequest(message) => message.clone(),
        ScriptJobSubmitError::QueueFull => "shared Python queue is full".into(),
        ScriptJobSubmitError::RetainedJobsFull => {
            "shared Python queue retained-job limit is full".into()
        }
        ScriptJobSubmitError::Stopped => "shared Python queue is stopped".into(),
        ScriptJobSubmitError::BrowserUnavailable => {
            "native Python subprocesses are unavailable in the browser".into()
        }
    }
}
fn script_failure(failure: &ScriptJobFailure, stderr: &str) -> String {
    if stderr.trim().is_empty() {
        failure.to_string()
    } else {
        format!("{failure}: {}", stderr.trim())
    }
}
fn run_error(code: &str, message: &str, node: Option<u32>) -> GraphRunError {
    let mut message = message.to_owned();
    while message.len() > 4096 {
        message.pop();
    }
    GraphRunError {
        code: code.into(),
        message,
        node,
        port: None,
        field: None,
    }
}
#[cfg(test)]
fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::application::platform::script_jobs::{ScriptJobConfig, ScriptRuntimeAvailability};
    use runebender::automation::agent_edit::AgentLayerGuard;
    use runebender::automation::script_recipe::ScriptRecipeLayer;
    use runebender::font::compiler::proof as compiled_proof;
    use runebender::font::edit_batch::canonical_glyph_revision;
    use runebender::workflows::nodes::Registry;
    use runebender::workflows::nodes_live;
    use runebender::workflows::nodes_session::{
        GraphEdit, GraphGuard, GraphInteractiveMutationRequest, GraphMutation,
    };

    fn python() -> Option<PathBuf> {
        let executable = std::env::var_os("PYTHON")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("python3"));
        matches!(
            ScriptRuntimeAvailability::check(&executable),
            ScriptRuntimeAvailability::Available(_)
        )
        .then_some(executable)
    }

    fn project() -> Project {
        Project::new_font("test.ufo".into())
    }

    fn input(project: &Project, job: &str) -> ScriptRecipeInput {
        let source = project.source_id(0).unwrap();
        let layer_id = project.document_source(source).unwrap().default_layer();
        let layer = project.document_layer("A", &layer_id).unwrap();
        ScriptRecipeInput::new(
            job.into(),
            source.0,
            BTreeMap::new(),
            vec![ScriptRecipeLayer {
                guard: AgentLayerGuard {
                    glyph: "A".into(),
                    glyph_id: project.document_glyph("A").unwrap().id().to_wire(),
                    layer: layer_id.name,
                    expected_revision: canonical_glyph_revision(layer).unwrap(),
                },
                width: layer.width(),
                anchors: Vec::new(),
                contours: Vec::new(),
                components: Vec::new(),
            }],
        )
        .unwrap()
    }

    fn width_script(width: u32) -> String {
        format!(
            "import json,sys\nd=json.load(sys.stdin)\njson.dump({{'schema_version':d['schema_version'],'job_id':d['job_id'],'input_hash':d['input_hash'],'report':'width {width}','reads':[],'edits':[{{'target':d['layers'][0]['guard'],'operations':[{{'op':'set_width','width':{width}}}]}}]}},sys.stdout)\n"
        )
    }

    fn append_script() -> String {
        "import json,sys\nd=json.load(sys.stdin)\np=[{'x':0,'y':0,'type':'line','smooth':False},{'x':100,'y':0,'type':'line','smooth':False},{'x':50,'y':100,'type':'line','smooth':False}]\njson.dump({'schema_version':d['schema_version'],'job_id':d['job_id'],'input_hash':d['input_hash'],'report':'append','reads':[],'edits':[{'target':d['layers'][0]['guard'],'operations':[{'op':'append_contours','contours':[{'points':p}]}]}]},sys.stdout)\n".into()
    }

    fn move_script() -> String {
        "import json,sys\nd=json.load(sys.stdin)\np=d['layers'][0]['contours'][0]['points'][0]\njson.dump({'schema_version':d['schema_version'],'job_id':d['job_id'],'input_hash':d['input_hash'],'report':'move','reads':[],'edits':[{'target':d['layers'][0]['guard'],'operations':[{'op':'set_point','point_id':p['id'],'x':p['x']+10,'y':p['y']}]}]},sys.stdout)\n".into()
    }

    fn setup(
        project: &Project,
        version: u32,
        graph: runebender::workflows::nodes::NodeGraph,
    ) -> (GraphSession, LiveGraphSubmitRequest) {
        let session = GraphSession::new("graph", "document", graph, Registry::core()).unwrap();
        let snapshot = session.snapshot();
        (
            session,
            LiveGraphSubmitRequest {
                guard: GraphSemanticGuard {
                    identity: snapshot.identity,
                    semantic_revision: snapshot.semantic_revision,
                    semantic_hash: snapshot.semantic_hash,
                },
                actor: "test".into(),
                operation_key: "run".into(),
                execution_version: version,
                recipe_input: input(project, "job-1"),
                base_proof_input: compiled_proof::capture(project).unwrap(),
            },
        )
    }

    fn current(project: &Project) -> GraphDocumentState {
        GraphDocumentState {
            document_epoch: "document".into(),
            document_revision: project.document_revision(),
        }
    }

    fn proof_output(request: &LiveGraphProofRequest) -> LiveGraphProofOutputs {
        LiveGraphProofOutputs {
            node: request.node,
            artifact_id: format!("proof-{}", request.node),
            content_sha256: sha256(format!("proof-{}", request.node).as_bytes()),
            canonical_input_sha256: request.input.canonical_input_sha256().to_owned(),
            font_sha256: sha256(b"font"),
        }
    }

    #[test]
    fn version_two_pair_requires_one_recipe_but_distinct_dag_proofs_are_allowed() {
        let project = project();
        let mut graph = nodes_live::comparison_starter(project.source_id(0).unwrap());
        let changed = graph
            .nodes
            .iter_mut()
            .filter(|node| node.type_name == "live.proof")
            .nth(1)
            .unwrap();
        changed.values.get_mut("recipe").unwrap()["text"] = serde_json::json!("different");
        let (session, request) = setup(&project, 2, graph.clone());
        let mismatch = validate_submit_capture(&session, &project, &request).unwrap_err();
        assert!(
            mismatch
                .to_string()
                .contains("comparison proofs must use the same recipe")
        );

        let source_node = graph
            .nodes
            .iter()
            .find(|node| node.type_name == "live.font")
            .unwrap()
            .id;
        let extra = graph.add("live.proof", [640.0, 720.0]);
        graph
            .node_mut(extra)
            .unwrap()
            .values
            .insert("recipe".into(), nodes_live::default_proof_recipe());
        graph.connect(source_node, "font", extra, "font");
        let (session, request) = setup(&project, 2, graph);
        let result = validate_submit_capture(&session, &project, &request);
        assert!(
            result.is_ok(),
            "distinct DAG proof recipes should be valid: {result:?}"
        );
    }

    fn finish(
        adapter: &mut LiveGraphExecution,
        session: &mut GraphSession,
        queue: &ScriptJobQueue,
        project: &Project,
        handle: GraphRunHandle,
        fail_proof: Option<u32>,
    ) -> GraphRunInspection {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            adapter.poll_scripts(session, queue, project);
            match adapter.phase(handle).unwrap() {
                LiveGraphPhase::ProofReady => {
                    let request = adapter.take_proof_request(handle).unwrap();
                    if fail_proof == Some(request.node) {
                        adapter
                            .fail_proofs(session, handle, &current(project), "proof failed")
                            .unwrap();
                    } else {
                        adapter
                            .publish_proofs(
                                session,
                                handle,
                                &current(project),
                                proof_output(&request),
                            )
                            .unwrap();
                    }
                }
                LiveGraphPhase::Terminal(_) => return session.inspect_run(handle).unwrap(),
                _ => {}
            }
            assert!(Instant::now() < deadline, "graph did not finish");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn legacy_comparison_proves_and_applies_exact_candidate_with_retry_and_release() {
        let Some(python) = python() else {
            return;
        };
        let project = project();
        let root_revision = project.document_revision();
        let mut graph = nodes_live::comparison_starter(project.source_id(0).unwrap());
        graph
            .node_mut(3)
            .unwrap()
            .values
            .insert("code".into(), Value::String(width_script(777)));
        let (mut session, request) = setup(&project, 1, graph);
        let retry = LiveGraphSubmitRequest {
            guard: request.guard.clone(),
            actor: request.actor.clone(),
            operation_key: request.operation_key.clone(),
            execution_version: 1,
            recipe_input: request.recipe_input.clone(),
            base_proof_input: request.base_proof_input.clone(),
        };
        let queue = ScriptJobQueue::new(ScriptJobConfig::new(python)).unwrap();
        let mut adapter = LiveGraphExecution::default();
        let run = adapter
            .submit(&mut session, &queue, &project, request)
            .unwrap()
            .graph;
        let handle = run.receipt.handle;
        let inspection = finish(&mut adapter, &mut session, &queue, &project, handle, None);
        assert_eq!(inspection.status, GraphRunStatus::Completed);
        assert_eq!(inspection.outputs.len(), 3);
        assert_eq!(project.document_revision(), root_revision);
        assert_eq!(
            adapter.result_summary(handle).unwrap().report.trim(),
            "width 777"
        );
        assert!(adapter.can_apply(handle, None));
        let apply = adapter
            .apply_request(
                &session,
                &current(&project),
                handle,
                None,
                "test",
                "apply",
                "approved",
            )
            .unwrap();
        assert_eq!(apply.request.edits.len(), 1);
        assert!(apply.payload_digest.is_none());
        assert_eq!(adapter.proof_node(handle, None, Some(false)).unwrap(), 2);
        assert_eq!(adapter.proof_node(handle, None, Some(true)).unwrap(), 4);
        let replay = adapter
            .submit(&mut session, &queue, &project, retry)
            .unwrap();
        assert_eq!(replay.graph.disposition, GraphReceiptDisposition::Replayed);
        assert_eq!(replay.graph.receipt, run.receipt);
        assert!(adapter.release(&mut session, &queue, handle).unwrap());
        assert_eq!(adapter.phase(handle), Some(LiveGraphPhase::Released));
        let released_retry = LiveGraphSubmitRequest {
            guard: GraphSemanticGuard {
                identity: run.receipt.identity.graph.clone(),
                semantic_revision: run.receipt.identity.semantic_revision,
                semantic_hash: run.receipt.identity.semantic_hash.clone(),
            },
            actor: "test".into(),
            operation_key: "run".into(),
            execution_version: 1,
            recipe_input: input(&project, "job-1"),
            base_proof_input: compiled_proof::capture(&project).unwrap(),
        };
        let replay = adapter
            .submit(&mut session, &queue, &project, released_retry)
            .unwrap();
        assert_eq!(replay.graph.receipt, run.receipt);
    }

    #[test]
    fn legacy_script_failure_does_not_dispatch_any_proofs() {
        let Some(python) = python() else {
            return;
        };
        let project = project();
        let mut graph = nodes_live::comparison_starter(project.source_id(0).unwrap());
        graph.node_mut(3).unwrap().values.insert(
            "code".into(),
            Value::String("raise RuntimeError('fixture failure')".into()),
        );
        let (mut session, request) = setup(&project, 1, graph);
        let queue = ScriptJobQueue::new(ScriptJobConfig::new(python)).unwrap();
        let mut adapter = LiveGraphExecution::default();
        let handle = adapter
            .submit(&mut session, &queue, &project, request)
            .unwrap()
            .graph
            .receipt
            .handle;
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            adapter.poll_scripts(&mut session, &queue, &project);
            let phase = adapter.phase(handle).unwrap();
            assert!(
                !matches!(
                    phase,
                    LiveGraphPhase::ProofReady | LiveGraphPhase::ProofsRunning
                ),
                "legacy failure must not dispatch a proof"
            );
            if phase == LiveGraphPhase::Terminal(GraphRunStatus::Failed) {
                break;
            }
            assert!(Instant::now() < deadline, "failed script did not settle");
            std::thread::sleep(Duration::from_millis(10));
        }
        let run = session.inspect_run(handle).unwrap();
        assert!(run.outputs.is_empty());
        assert_eq!(run.errors.len(), 1);
        assert_eq!(run.errors[0].node, Some(3));
    }

    #[test]
    fn exact_retry_after_graph_change_replays_but_changed_capture_is_rejected() {
        let Some(python) = python() else {
            return;
        };
        let project = project();
        let mut graph = nodes_live::comparison_starter(project.source_id(0).unwrap());
        graph
            .node_mut(3)
            .unwrap()
            .values
            .insert("code".into(), Value::String(width_script(777)));
        let (mut session, request) = setup(&project, 1, graph);
        let retry = LiveGraphSubmitRequest {
            guard: request.guard.clone(),
            actor: request.actor.clone(),
            operation_key: request.operation_key.clone(),
            execution_version: 1,
            recipe_input: request.recipe_input.clone(),
            base_proof_input: request.base_proof_input.clone(),
        };
        let queue = ScriptJobQueue::new(ScriptJobConfig::new(python)).unwrap();
        let mut adapter = LiveGraphExecution::default();
        let run = adapter
            .submit(&mut session, &queue, &project, request)
            .unwrap()
            .graph;
        let snapshot = session.snapshot();
        session
            .mutate_interactive(GraphInteractiveMutationRequest {
                guard: GraphGuard {
                    identity: snapshot.identity,
                    revision: snapshot.revision,
                },
                mutation: GraphMutation::Patch {
                    edits: vec![GraphEdit::SetValue {
                        node: 3,
                        field: "code".into(),
                        value: Value::String(width_script(778)),
                    }],
                },
            })
            .unwrap();
        let replay = adapter
            .submit(
                &mut session,
                &queue,
                &project,
                LiveGraphSubmitRequest {
                    guard: retry.guard.clone(),
                    actor: retry.actor.clone(),
                    operation_key: retry.operation_key.clone(),
                    execution_version: retry.execution_version,
                    recipe_input: retry.recipe_input.clone(),
                    base_proof_input: retry.base_proof_input.clone(),
                },
            )
            .unwrap();
        assert_eq!(replay.graph.disposition, GraphReceiptDisposition::Replayed);
        assert_eq!(replay.graph.receipt, run.receipt);
        let mut changed = input(&project, "job-1");
        changed.input_hash = sha256(b"changed input");
        let error = adapter
            .submit(
                &mut session,
                &queue,
                &project,
                LiveGraphSubmitRequest {
                    guard: retry.guard.clone(),
                    actor: retry.actor.clone(),
                    operation_key: retry.operation_key.clone(),
                    execution_version: 1,
                    recipe_input: changed,
                    base_proof_input: retry.base_proof_input.clone(),
                },
            )
            .unwrap_err();
        assert_eq!(error.code, LiveGraphExecutionErrorCode::Capture);
        let mut altered_project = self::project();
        let source = altered_project.source_id(0).unwrap();
        let layer = altered_project
            .document_source(source)
            .unwrap()
            .default_layer();
        altered_project
            .edit_document_layer("A", &layer, |draft| {
                draft.set_width(701.0)?;
                Ok(())
            })
            .unwrap();
        let error = adapter
            .submit(
                &mut session,
                &queue,
                &project,
                LiveGraphSubmitRequest {
                    guard: retry.guard,
                    actor: retry.actor,
                    operation_key: retry.operation_key,
                    execution_version: 1,
                    recipe_input: retry.recipe_input,
                    base_proof_input: compiled_proof::capture(&altered_project).unwrap(),
                },
            )
            .unwrap_err();
        assert_eq!(error.code, LiveGraphExecutionErrorCode::Capture);
        let _ = adapter.cancel(
            &mut session,
            &queue,
            GraphCancelRequest {
                identity: run.receipt.identity.graph,
                handle: run.receipt.handle,
                actor: "test".into(),
                operation_key: "cancel".into(),
            },
            &current(&project),
        );
    }

    #[test]
    fn completed_worker_cancelled_before_host_poll_never_stages() {
        let Some(python) = python() else {
            return;
        };
        let project = project();
        let mut graph = nodes_live::comparison_starter(project.source_id(0).unwrap());
        graph
            .node_mut(3)
            .unwrap()
            .values
            .insert("code".into(), Value::String(width_script(777)));
        let (mut session, request) = setup(&project, 1, graph);
        let queue = ScriptJobQueue::new(ScriptJobConfig::new(python)).unwrap();
        let mut adapter = LiveGraphExecution::default();
        let run = adapter
            .submit(&mut session, &queue, &project, request)
            .unwrap()
            .graph;
        let handle = run.receipt.handle;
        let script = adapter.records[&handle].script.unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while queue.inspect(script).unwrap().status != ScriptJobStatus::Completed {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        let response = adapter
            .cancel(
                &mut session,
                &queue,
                GraphCancelRequest {
                    identity: run.receipt.identity.graph,
                    handle,
                    actor: "test".into(),
                    operation_key: "late-cancel".into(),
                },
                &current(&project),
            )
            .unwrap();
        assert_eq!(
            response.graph.receipt.outcome,
            GraphCancelOutcome::CancellationRequested
        );
        adapter.poll_scripts(&mut session, &queue, &project);
        assert_eq!(
            adapter.phase(handle),
            Some(LiveGraphPhase::Terminal(GraphRunStatus::Cancelled))
        );
        assert!(session.inspect_run(handle).unwrap().outputs.is_empty());
        assert!(adapter.take_proof_request(handle).is_err());
    }

    #[test]
    fn proof_stage_cancellation_suppresses_retained_outputs() {
        let Some(python) = python() else {
            return;
        };
        let project = project();
        let mut graph = nodes_live::comparison_starter(project.source_id(0).unwrap());
        graph
            .node_mut(3)
            .unwrap()
            .values
            .insert("code".into(), Value::String(width_script(777)));
        let (mut session, request) = setup(&project, 1, graph);
        let queue = ScriptJobQueue::new(ScriptJobConfig::new(python)).unwrap();
        let mut adapter = LiveGraphExecution::default();
        let run = adapter
            .submit(&mut session, &queue, &project, request)
            .unwrap()
            .graph;
        let handle = run.receipt.handle;
        let deadline = Instant::now() + Duration::from_secs(5);
        while adapter.phase(handle) != Some(LiveGraphPhase::ProofReady) {
            adapter.poll_scripts(&mut session, &queue, &project);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        let proof = adapter.take_proof_request(handle).unwrap();
        assert_eq!(adapter.active_proof_node(handle), Some(proof.node));
        let response = adapter
            .cancel(
                &mut session,
                &queue,
                GraphCancelRequest {
                    identity: run.receipt.identity.graph,
                    handle,
                    actor: "test".into(),
                    operation_key: "cancel-proof".into(),
                },
                &current(&project),
            )
            .unwrap();
        assert!(response.cancel_proofs);
        let inspection = adapter
            .finish_proof_cancellation(&mut session, handle, &current(&project))
            .unwrap();
        assert_eq!(inspection.status, GraphRunStatus::Cancelled);
        assert!(inspection.outputs.is_empty());
        assert!(!adapter.can_apply(handle, None));
    }

    #[test]
    fn full_shared_queue_returns_releasable_failed_receipt() {
        let Some(python) = python() else {
            return;
        };
        let project = project();
        let mut config = ScriptJobConfig::new(python);
        config.queue_capacity = 1;
        config.retained_capacity = 4;
        let queue = ScriptJobQueue::new(config).unwrap();
        let blocker = queue
            .submit(ScriptJobRequest {
                input: input(&project, "blocker"),
                script: format!("import time\ntime.sleep(30)\n{}", width_script(700)),
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while queue.inspect(blocker).unwrap().status != ScriptJobStatus::Running {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        let filler = queue
            .submit(ScriptJobRequest {
                input: input(&project, "filler"),
                script: width_script(701),
            })
            .unwrap();
        let mut graph = nodes_live::comparison_starter(project.source_id(0).unwrap());
        graph
            .node_mut(3)
            .unwrap()
            .values
            .insert("code".into(), Value::String(width_script(777)));
        let (mut session, request) = setup(&project, 1, graph);
        let retry = LiveGraphSubmitRequest {
            guard: request.guard.clone(),
            actor: request.actor.clone(),
            operation_key: request.operation_key.clone(),
            execution_version: 1,
            recipe_input: request.recipe_input.clone(),
            base_proof_input: request.base_proof_input.clone(),
        };
        let mut adapter = LiveGraphExecution::default();
        let run = adapter
            .submit(&mut session, &queue, &project, request)
            .unwrap()
            .graph;
        let handle = run.receipt.handle;
        assert_eq!(
            adapter.phase(handle),
            Some(LiveGraphPhase::Terminal(GraphRunStatus::Failed))
        );
        assert!(adapter.records[&handle].script.is_none());
        assert_eq!(
            session.inspect_run(handle).unwrap().errors[0].code,
            "script_submit"
        );
        assert_eq!(
            adapter
                .submit(&mut session, &queue, &project, retry)
                .unwrap()
                .graph
                .receipt,
            run.receipt
        );
        assert!(adapter.release(&mut session, &queue, handle).unwrap());
        let _ = queue.cancel(filler);
        let _ = queue.cancel(blocker);
    }

    #[test]
    fn chained_contour_recipe_uses_parent_overlay_and_requires_explicit_branch_selection() {
        let Some(python) = python() else {
            return;
        };
        let project = project();
        let root_revision = project.document_revision();
        let mut graph = nodes_live::comparison_starter(project.source_id(0).unwrap());
        graph
            .node_mut(3)
            .unwrap()
            .values
            .insert("code".into(), Value::String(append_script()));
        let next = graph.add("live.python", [1000.0, 0.0]);
        graph
            .node_mut(next)
            .unwrap()
            .values
            .insert("code".into(), Value::String(move_script()));
        graph
            .node_mut(next)
            .unwrap()
            .values
            .insert("parameters".into(), serde_json::json!({}));
        graph.connect(3, "font", next, "font");
        let last_proof = graph.add("live.proof", [1300.0, 0.0]);
        graph.connect(next, "font", last_proof, "font");
        let (mut session, request) = setup(&project, 2, graph);
        let queue = ScriptJobQueue::new(ScriptJobConfig::new(python)).unwrap();
        let mut adapter = LiveGraphExecution::default();
        let handle = adapter
            .submit(&mut session, &queue, &project, request)
            .unwrap()
            .graph
            .receipt
            .handle;
        let inspection = finish(&mut adapter, &mut session, &queue, &project, handle, None);
        assert_eq!(inspection.status, GraphRunStatus::Completed);
        assert_eq!(project.document_revision(), root_revision);
        assert!(!adapter.can_apply(handle, None));
        assert!(adapter.can_apply(handle, Some(next)));
        let first = adapter
            .apply_request(
                &session,
                &current(&project),
                handle,
                Some(3),
                "test",
                "apply-first",
                "approved",
            )
            .unwrap();
        let last = adapter
            .apply_request(
                &session,
                &current(&project),
                handle,
                Some(next),
                "test",
                "apply-last",
                "approved",
            )
            .unwrap();
        assert_ne!(first.payload_digest, last.payload_digest);
        let hashes: Vec<&str> = inspection
            .outputs
            .iter()
            .filter_map(|output| match &output.value {
                GraphNodeOutputValue::FontVersion { content_sha256, .. } => {
                    Some(content_sha256.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(hashes.len(), 2);
        assert_ne!(hashes[0], hashes[1]);
        assert!(adapter.proof_node(handle, Some(last_proof), None).is_ok());
        assert!(adapter.proof_node(handle, None, Some(true)).is_err());
        assert_eq!(
            adapter
                .result_summary(handle)
                .unwrap()
                .report
                .lines()
                .count(),
            2
        );
        assert!(last.payload_digest.is_some());
        let overlay = compiled_proof::capture(&project)
            .unwrap()
            .with_staged_edit(&project, &last.candidate)
            .unwrap();
        let final_hash = inspection
            .outputs
            .iter()
            .find_map(|output| {
                if output.node == next
                    && let GraphNodeOutputValue::FontVersion { content_sha256, .. } = &output.value
                {
                    return Some(content_sha256.as_str());
                }
                None
            })
            .unwrap();
        assert_eq!(overlay.canonical_input_sha256(), final_hash);
    }

    #[test]
    fn cancellation_before_downstream_launch_suppresses_outputs() {
        let Some(python) = python() else {
            return;
        };
        let project = project();
        let mut graph = nodes_live::comparison_starter(project.source_id(0).unwrap());
        let slow = format!("import time\ntime.sleep(2)\n{}", width_script(777));
        graph
            .node_mut(3)
            .unwrap()
            .values
            .insert("code".into(), Value::String(slow));
        let (mut session, request) = setup(&project, 1, graph);
        let queue = ScriptJobQueue::new(ScriptJobConfig::new(python)).unwrap();
        let mut adapter = LiveGraphExecution::default();
        let run = adapter
            .submit(&mut session, &queue, &project, request)
            .unwrap()
            .graph;
        let handle = run.receipt.handle;
        let cancel = adapter
            .cancel(
                &mut session,
                &queue,
                GraphCancelRequest {
                    identity: run.receipt.identity.graph.clone(),
                    handle,
                    actor: "test".into(),
                    operation_key: "cancel".into(),
                },
                &current(&project),
            )
            .unwrap();
        assert!(!cancel.cancel_proofs);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            adapter.poll_scripts(&mut session, &queue, &project);
            if matches!(adapter.phase(handle), Some(LiveGraphPhase::Terminal(_))) {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        let inspection = session.inspect_run(handle).unwrap();
        assert_eq!(inspection.status, GraphRunStatus::Cancelled);
        assert!(inspection.outputs.is_empty());
        assert!(adapter.take_proof_request(handle).is_err());
    }

    #[test]
    fn independent_branch_failure_retains_successful_direct_proof() {
        let Some(python) = python() else {
            return;
        };
        let project = project();
        let mut graph = nodes_live::comparison_starter(project.source_id(0).unwrap());
        graph
            .node_mut(3)
            .unwrap()
            .values
            .insert("code".into(), Value::String(width_script(777)));
        let bad = graph.add("live.python", [400.0, 400.0]);
        graph.node_mut(bad).unwrap().values.insert(
            "code".into(),
            Value::String("raise RuntimeError('failure')".into()),
        );
        graph
            .node_mut(bad)
            .unwrap()
            .values
            .insert("parameters".into(), serde_json::json!({}));
        graph.connect(1, "font", bad, "font");
        let blocked = graph.add("live.proof", [700.0, 400.0]);
        graph.connect(bad, "font", blocked, "font");
        let (mut session, request) = setup(&project, 2, graph);
        let queue = ScriptJobQueue::new(ScriptJobConfig::new(python)).unwrap();
        let mut adapter = LiveGraphExecution::default();
        let handle = adapter
            .submit(&mut session, &queue, &project, request)
            .unwrap()
            .graph
            .receipt
            .handle;
        let inspection = finish(&mut adapter, &mut session, &queue, &project, handle, None);
        assert_eq!(inspection.status, GraphRunStatus::PartiallyFailed);
        assert_eq!(inspection.errors.len(), 2);
        assert!(
            inspection
                .errors
                .iter()
                .any(|error| error.node == Some(blocked) && error.code == "dependency_failed")
        );
        assert!(adapter.can_apply(handle, Some(3)));
        assert!(
            adapter
                .apply_request(
                    &session,
                    &current(&project),
                    handle,
                    Some(bad),
                    "test",
                    "bad",
                    "approved"
                )
                .is_err()
        );
    }

    #[test]
    fn late_stale_result_publishes_no_outputs() {
        let Some(python) = python() else {
            return;
        };
        let mut project = project();
        let mut graph = nodes_live::comparison_starter(project.source_id(0).unwrap());
        graph
            .node_mut(3)
            .unwrap()
            .values
            .insert("code".into(), Value::String(width_script(777)));
        let (mut session, request) = setup(&project, 1, graph);
        let queue = ScriptJobQueue::new(ScriptJobConfig::new(python)).unwrap();
        let mut adapter = LiveGraphExecution::default();
        let handle = adapter
            .submit(&mut session, &queue, &project, request)
            .unwrap()
            .graph
            .receipt
            .handle;
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        project
            .edit_document_layer("A", &layer, |draft| {
                draft.set_width(701.0)?;
                Ok(())
            })
            .unwrap();
        let inspection = finish(&mut adapter, &mut session, &queue, &project, handle, None);
        assert_eq!(inspection.status, GraphRunStatus::Stale);
        assert!(inspection.outputs.is_empty());
        assert!(!adapter.can_apply(handle, None));
    }
}
