// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Native execution adapter for the first live Python comparison graph.
//!
//! This module borrows the Workspace-owned Python queue and never drains jobs owned by another
//! consumer.
//! A successful recipe stages one canonical document transaction without committing it.
//! The retained base compiler input and staged transaction are handed to the compiled-proof owner;
//! applying the result remains a separate receipt-backed font command.

use std::collections::BTreeMap;
use std::fmt;

use runebender::automation::agent_edit::AgentEditRequest;
use runebender::automation::script_recipe::{ScriptRecipeInput, ScriptRecipeResult};
use runebender::font::compiler::proof::{CompileProofInput, CompiledProofRecipe};
use runebender::font::project::Project;
use runebender::workflows::nodes_session::{
    GraphCancelOutcome, GraphCancelRequest, GraphCancelResponse, GraphDocumentState,
    GraphFontCapture, GraphNodeOutput, GraphNodeOutputValue, GraphProofScope,
    GraphReceiptDisposition, GraphRunCompletion, GraphRunHandle, GraphRunIdentity,
    GraphRunInspection, GraphRunOutcome, GraphRunRequest, GraphRunResponse, GraphRunStatus,
    GraphRunWork, GraphSemanticGuard, GraphSession,
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
    /// Python job is waiting in the shared queue.
    ScriptQueued,
    /// Python process is active.
    ScriptRunning,
    /// Strict result is staged without changing the Project.
    RecipeStaged,
    /// The compiled-proof owner has accepted the comparison request.
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
    /// Frozen unchanged whole-family compiler input.
    pub(crate) base_input: CompileProofInput,
    /// Complete derived whole-family compiler input with the staged edit overlaid.
    pub(crate) derived_input: CompileProofInput,
    /// Strict Python result retained for report and later explicit Apply.
    pub(crate) recipe_result: ScriptRecipeResult,
    /// Bounded Python diagnostics.
    pub(crate) stderr: String,
    /// Identical typed recipe for both comparison proofs.
    pub(crate) proof_recipe: CompiledProofRecipe,
    /// Derived `FontVersion` output published only after both proofs finish.
    pub(crate) derived_version: GraphNodeOutput,
}

/// Exact proof artifact identities returned by the existing proof queue.
pub(crate) struct LiveGraphProofOutputs {
    /// Unchanged proof artifact handle.
    pub(crate) unchanged_artifact_id: String,
    /// SHA-256 of unchanged proof PNG bytes.
    pub(crate) unchanged_content_sha256: String,
    /// Canonical whole-family input hash used by the unchanged proof.
    pub(crate) unchanged_canonical_input_sha256: String,
    /// Compiled font hash used by the unchanged proof.
    pub(crate) unchanged_font_sha256: String,
    /// Changed proof artifact handle.
    pub(crate) changed_artifact_id: String,
    /// SHA-256 of changed proof PNG bytes.
    pub(crate) changed_content_sha256: String,
    /// Canonical whole-family input hash after staged overlay.
    pub(crate) changed_canonical_input_sha256: String,
    /// Compiled font hash used by the changed proof.
    pub(crate) changed_font_sha256: String,
}

/// Script result exposed to both native UI and the thin agent adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LiveGraphResultSummary {
    /// Bounded report returned by the strict recipe contract.
    pub(crate) report: String,
    /// Bounded Python diagnostics captured by the shared process queue.
    pub(crate) stderr: String,
    /// Whether both compiled-family proofs completed and explicit Apply is available.
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
    script: Option<ScriptJobHandle>,
    work: GraphRunWork,
    base_input: Option<CompileProofInput>,
    proof_request: Option<LiveGraphProofRequest>,
    proof_recipe: CompiledProofRecipe,
    phase: LiveGraphPhase,
}

/// Bounded state connecting `GraphSession` to existing Python and proof owners.
#[derive(Debug, Default)]
pub(crate) struct LiveGraphExecution {
    records: BTreeMap<GraphRunHandle, LiveGraphRecord>,
    requests: BTreeMap<(String, String), GraphRunHandle>,
}

impl LiveGraphExecution {
    /// Submit exact graph code and immutable input to the shared Python queue.
    pub(crate) fn submit(
        &mut self,
        session: &mut GraphSession,
        queue: &ScriptJobQueue,
        project: &Project,
        request: LiveGraphSubmitRequest,
    ) -> Result<LiveGraphSubmitResponse, LiveGraphExecutionError> {
        let request_key = (request.actor.clone(), request.operation_key.clone());
        if let Some(handle) = self.requests.get(&request_key).copied() {
            let original_request = {
                let record = self.records.get(&handle).ok_or_else(|| {
                    LiveGraphExecutionError::new(
                        LiveGraphExecutionErrorCode::UnknownRun,
                        "graph run retry is retained only as a released tombstone",
                    )
                })?;
                validate_retry_request(record, &request)?;
                record.original_request()
            };
            let graph = session.start_run(original_request).map_err(graph_error)?;
            debug_assert_eq!(
                graph.disposition,
                GraphReceiptDisposition::Replayed,
                "an accepted adapter retry must replay its canonical graph receipt"
            );
            return Ok(LiveGraphSubmitResponse { graph });
        }
        if self.retained_count() >= MAX_LIVE_GRAPH_RUNS {
            return Err(LiveGraphExecutionError::new(
                LiveGraphExecutionErrorCode::Capacity,
                "live graph adapter retains at most eight unreleased runs",
            ));
        }
        validate_submit_capture(session, project, &request)?;
        let snapshot = session.snapshot();
        let capture = session
            .capture_run(
                graph_font_capture(&request),
                request.recipe_input.input_hash.clone(),
            )
            .map_err(graph_error)?;
        let proof_recipe = proof_recipe(&snapshot.graph, &capture.proofs)?;
        if proof_recipe.normalized_location.len() > 64
            || proof_recipe.normalized_location.len() != project.axes.len()
        {
            return Err(LiveGraphExecutionError::new(
                LiveGraphExecutionErrorCode::Proof,
                "provide one normalized coordinate per document axis (maximum 64)",
            ));
        }
        let script_node = capture.scripts[0].node;
        let script = snapshot
            .graph
            .node(script_node)
            .and_then(|node| node.values.get("code"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                LiveGraphExecutionError::new(
                    LiveGraphExecutionErrorCode::Capture,
                    "live.python code disappeared after guarded capture",
                )
            })?
            .to_owned();
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
        let current = GraphDocumentState {
            document_epoch: work.identity.graph.document_epoch.clone(),
            document_revision: work.identity.capture.font.document_revision,
        };
        let script_handle = match queue.submit(ScriptJobRequest {
            input: request.recipe_input,
            script,
        }) {
            Ok(handle) => handle,
            Err(error) => {
                let message = submit_error(&error);
                let inspection = session.complete_run(
                    GraphRunCompletion {
                        handle: work.handle,
                        identity: work.identity.clone(),
                        outcome: GraphRunOutcome::Failed(vec![run_error(
                            "script_submit",
                            &message,
                            Some(work.plan.python_node),
                        )]),
                    },
                    &current,
                );
                let phase = inspection
                    .map(|inspection| LiveGraphPhase::Terminal(inspection.status))
                    .unwrap_or(LiveGraphPhase::Terminal(GraphRunStatus::Failed));
                self.requests.insert(request_key, work.handle);
                self.records.insert(
                    work.handle,
                    LiveGraphRecord {
                        actor: request.actor,
                        operation_key: request.operation_key,
                        script: None,
                        work,
                        base_input: None,
                        proof_request: None,
                        proof_recipe,
                        phase,
                    },
                );
                return Ok(LiveGraphSubmitResponse { graph });
            }
        };
        self.requests.insert(request_key, work.handle);
        self.records.insert(
            work.handle,
            LiveGraphRecord {
                actor: request.actor,
                operation_key: request.operation_key,
                script: Some(script_handle),
                work,
                base_input: Some(request.base_proof_input),
                proof_request: None,
                proof_recipe,
                phase: LiveGraphPhase::ScriptQueued,
            },
        );
        Ok(LiveGraphSubmitResponse { graph })
    }

    /// Inspect only this adapter's queue handles and stage newly completed recipe results.
    pub(crate) fn poll_scripts(
        &mut self,
        session: &mut GraphSession,
        queue: &ScriptJobQueue,
        project: &Project,
    ) -> Vec<(GraphRunHandle, LiveGraphPhase)> {
        let handles: Vec<GraphRunHandle> = self.records.keys().copied().collect();
        let mut changed = Vec::new();
        for handle in handles {
            let Some(record) = self.records.get_mut(&handle) else {
                continue;
            };
            if !matches!(
                record.phase,
                LiveGraphPhase::ScriptQueued | LiveGraphPhase::ScriptRunning
            ) {
                continue;
            }
            let Some(script_handle) = record.script else {
                continue;
            };
            let Some(inspection) = queue.inspect(script_handle) else {
                finish_failed(
                    session,
                    record,
                    project,
                    "script_unavailable",
                    "shared Python queue no longer retains this job",
                );
                changed.push((handle, record.phase));
                continue;
            };
            match inspection.status {
                ScriptJobStatus::Queued => {}
                ScriptJobStatus::Running => {
                    if record.phase != LiveGraphPhase::ScriptRunning {
                        record.phase = LiveGraphPhase::ScriptRunning;
                        changed.push((handle, record.phase));
                    }
                }
                ScriptJobStatus::Completed
                | ScriptJobStatus::Failed
                | ScriptJobStatus::CancelledBeforeStart
                | ScriptJobStatus::CancelledWhileRunning => {
                    if session
                        .inspect_run(handle)
                        .is_some_and(|run| run.status == GraphRunStatus::CancellationRequested)
                    {
                        finish_cancelled(session, record, project);
                        let _ = queue.discard(script_handle);
                        record.script = None;
                        changed.push((handle, record.phase));
                        continue;
                    }
                    let Some(outcome) = inspection.outcome else {
                        continue;
                    };
                    match outcome {
                        ScriptJobOutcome::Completed { result, stderr } => {
                            match stage_result(record, project, result, stderr) {
                                Ok(()) => {}
                                Err(error) => finish_failed(
                                    session,
                                    record,
                                    project,
                                    "recipe_stage",
                                    &error.message,
                                ),
                            }
                        }
                        ScriptJobOutcome::Failed { failure, stderr } => {
                            let message = script_failure(&failure, &stderr);
                            finish_failed(session, record, project, "script_failed", &message);
                        }
                        ScriptJobOutcome::Cancelled { .. } => {
                            finish_cancelled(session, record, project);
                        }
                    }
                    let _ = queue.discard(script_handle);
                    record.script = None;
                    changed.push((handle, record.phase));
                }
            }
        }
        changed
    }

    /// Take one staged comparison request exactly once for the compiled-proof owner.
    pub(crate) fn take_proof_request(
        &mut self,
        handle: GraphRunHandle,
    ) -> Result<LiveGraphProofRequest, LiveGraphExecutionError> {
        let record = self.record_mut(handle)?;
        if record.phase != LiveGraphPhase::RecipeStaged {
            return Err(LiveGraphExecutionError::new(
                LiveGraphExecutionErrorCode::WrongPhase,
                "proof request is available only after recipe staging",
            ));
        }
        let request = record.proof_request.clone().ok_or_else(|| {
            LiveGraphExecutionError::new(
                LiveGraphExecutionErrorCode::WrongPhase,
                "staged proof request is unavailable",
            )
        })?;
        record.phase = LiveGraphPhase::ProofsRunning;
        Ok(request)
    }

    /// Publish both proof artifacts through the graph's exact retained identity.
    pub(crate) fn publish_proofs(
        &mut self,
        session: &mut GraphSession,
        handle: GraphRunHandle,
        current: &GraphDocumentState,
        proofs: LiveGraphProofOutputs,
    ) -> Result<GraphRunInspection, LiveGraphExecutionError> {
        let record = self.record_mut(handle)?;
        if record.phase != LiveGraphPhase::ProofsRunning {
            return Err(LiveGraphExecutionError::new(
                LiveGraphExecutionErrorCode::WrongPhase,
                "proof outputs require a dispatched proof request",
            ));
        }
        let request = record.proof_request.as_ref().ok_or_else(|| {
            LiveGraphExecutionError::new(
                LiveGraphExecutionErrorCode::Proof,
                "proof request identity is unavailable",
            )
        })?;
        let proof_hash = |node| {
            record
                .work
                .identity
                .capture
                .proofs
                .iter()
                .find(|proof| proof.node == node)
                .map(|proof| proof.recipe_sha256.clone())
                .expect("validated proof node retains a capture")
        };
        let outputs = vec![
            request.derived_version.clone(),
            GraphNodeOutput {
                node: record.work.plan.unchanged_proof,
                value: GraphNodeOutputValue::Proof {
                    artifact_id: proofs.unchanged_artifact_id,
                    content_sha256: proofs.unchanged_content_sha256,
                    canonical_input_sha256: proofs.unchanged_canonical_input_sha256,
                    font_sha256: proofs.unchanged_font_sha256,
                    recipe_sha256: proof_hash(record.work.plan.unchanged_proof),
                    scope: GraphProofScope::CompiledFamily,
                },
            },
            GraphNodeOutput {
                node: record.work.plan.changed_proof,
                value: GraphNodeOutputValue::Proof {
                    artifact_id: proofs.changed_artifact_id,
                    content_sha256: proofs.changed_content_sha256,
                    canonical_input_sha256: proofs.changed_canonical_input_sha256,
                    font_sha256: proofs.changed_font_sha256,
                    recipe_sha256: proof_hash(record.work.plan.changed_proof),
                    scope: GraphProofScope::CompiledFamily,
                },
            },
        ];
        let inspection = session
            .complete_run(
                GraphRunCompletion {
                    handle,
                    identity: record.work.identity.clone(),
                    outcome: GraphRunOutcome::Completed(outputs),
                },
                current,
            )
            .map_err(graph_error)?;
        record.phase = LiveGraphPhase::Terminal(inspection.status);
        Ok(inspection)
    }

    /// Publish a proof failure without attaching either image.
    pub(crate) fn fail_proofs(
        &mut self,
        session: &mut GraphSession,
        handle: GraphRunHandle,
        current: &GraphDocumentState,
        message: impl Into<String>,
    ) -> Result<GraphRunInspection, LiveGraphExecutionError> {
        let record = self.record_mut(handle)?;
        if record.phase != LiveGraphPhase::ProofsRunning {
            return Err(LiveGraphExecutionError::new(
                LiveGraphExecutionErrorCode::WrongPhase,
                "proof failure requires a dispatched proof request",
            ));
        }
        let message = message.into();
        let inspection = session
            .complete_run(
                GraphRunCompletion {
                    handle,
                    identity: record.work.identity.clone(),
                    outcome: GraphRunOutcome::Failed(vec![run_error(
                        "proof_failed",
                        &message,
                        None,
                    )]),
                },
                current,
            )
            .map_err(graph_error)?;
        record.phase = LiveGraphPhase::Terminal(inspection.status);
        Ok(inspection)
    }

    /// Cancel one graph run and signal whichever shared queue currently owns work.
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
        let mut cancel_proofs = false;
        match record.phase {
            LiveGraphPhase::ScriptQueued | LiveGraphPhase::ScriptRunning => {
                if let Some(script_handle) = record.script {
                    queue.cancel(script_handle);
                }
            }
            LiveGraphPhase::RecipeStaged => {
                finish_cancelled(session, record, &ProjectRevision(current));
            }
            LiveGraphPhase::ProofsRunning => cancel_proofs = true,
            LiveGraphPhase::Terminal(_) | LiveGraphPhase::Released => {}
        }
        if graph.receipt.outcome == GraphCancelOutcome::CancelledBeforeStart {
            record.phase = LiveGraphPhase::Terminal(GraphRunStatus::Cancelled);
        }
        Ok(LiveGraphCancelResponse {
            graph,
            cancel_proofs,
        })
    }

    /// Finish cancellation after the compiled-proof owner reaps both handles.
    pub(crate) fn finish_proof_cancellation(
        &mut self,
        session: &mut GraphSession,
        handle: GraphRunHandle,
        current: &GraphDocumentState,
    ) -> Result<GraphRunInspection, LiveGraphExecutionError> {
        let record = self.record_mut(handle)?;
        if record.phase != LiveGraphPhase::ProofsRunning {
            return Err(LiveGraphExecutionError::new(
                LiveGraphExecutionErrorCode::WrongPhase,
                "proof cancellation requires dispatched proof jobs",
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
        Ok(inspection)
    }

    /// Read one adapter phase.
    pub(crate) fn phase(&self, handle: GraphRunHandle) -> Option<LiveGraphPhase> {
        Some(self.records.get(&handle)?.phase)
    }

    /// Read the retained script report without consuming proof or Apply state.
    pub(crate) fn result_summary(&self, handle: GraphRunHandle) -> Option<LiveGraphResultSummary> {
        let record = self.records.get(&handle)?;
        let proof = record.proof_request.as_ref()?;
        Some(LiveGraphResultSummary {
            report: proof.recipe_result.report.clone(),
            stderr: proof.stderr.clone(),
            can_apply: record.phase == LiveGraphPhase::Terminal(GraphRunStatus::Completed),
        })
    }

    /// Build the separate receipt-backed Apply request for one completed selected result.
    ///
    /// This never commits directly.
    /// The caller passes the authorization from the native action or agent boundary to the common
    /// Workspace edit adapter, which remains responsible for receipts and ordinary font undo.
    pub(crate) fn apply_request(
        &self,
        session: &GraphSession,
        current: &GraphDocumentState,
        handle: GraphRunHandle,
        actor: impl Into<String>,
        operation_key: impl Into<String>,
        authorization: impl Into<String>,
    ) -> Result<AgentEditRequest, LiveGraphExecutionError> {
        let record = self.records.get(&handle).ok_or_else(|| {
            LiveGraphExecutionError::new(
                LiveGraphExecutionErrorCode::UnknownRun,
                "live graph adapter does not retain this run",
            )
        })?;
        if record.phase != LiveGraphPhase::Terminal(GraphRunStatus::Completed) {
            return Err(LiveGraphExecutionError::new(
                LiveGraphExecutionErrorCode::WrongPhase,
                "Apply is available only after both comparison proofs complete",
            ));
        }
        let snapshot = session.snapshot();
        let identity = &record.work.identity;
        if current.document_epoch != identity.graph.document_epoch
            || current.document_revision != identity.capture.font.document_revision
            || snapshot.semantic_revision != identity.semantic_revision
            || snapshot.semantic_hash != identity.semantic_hash
        {
            return Err(LiveGraphExecutionError::new(
                LiveGraphExecutionErrorCode::Stale,
                "the graph or document changed after this run; rerun before Apply",
            ));
        }
        let proof = record.proof_request.as_ref().ok_or_else(|| {
            LiveGraphExecutionError::new(
                LiveGraphExecutionErrorCode::WrongPhase,
                "completed graph run has no retained staged recipe",
            )
        })?;
        Ok(AgentEditRequest {
            expected_document_epoch: record.work.identity.graph.document_epoch.clone(),
            actor: actor.into(),
            operation_key: operation_key.into(),
            authorization: authorization.into(),
            source: record.work.identity.capture.font.source,
            history_name: "Apply Nodes Python result".into(),
            reads: proof.recipe_result.reads.clone(),
            edits: proof.recipe_result.edits.clone(),
        })
    }

    /// Release heavy adapter and `GraphSession` artifacts without enabling re-execution.
    pub(crate) fn release(
        &mut self,
        session: &mut GraphSession,
        queue: &ScriptJobQueue,
        handle: GraphRunHandle,
    ) -> Result<bool, LiveGraphExecutionError> {
        let released = session.release_run(handle).map_err(graph_error)?;
        let record = self.record_mut(handle)?;
        if released {
            if let Some(script) = record.script {
                let _ = queue.discard(script);
            }
            record.script = None;
            record.base_input = None;
            record.proof_request = None;
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
            LiveGraphExecutionError::new(
                LiveGraphExecutionErrorCode::UnknownRun,
                "live graph adapter does not retain this run",
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

fn validate_retry_request(
    record: &LiveGraphRecord,
    request: &LiveGraphSubmitRequest,
) -> Result<(), LiveGraphExecutionError> {
    let identity = &record.work.identity;
    let expected_guard = GraphSemanticGuard {
        identity: identity.graph.clone(),
        semantic_revision: identity.semantic_revision,
        semantic_hash: identity.semantic_hash.clone(),
    };
    if request.guard != expected_guard {
        return Err(LiveGraphExecutionError::new(
            LiveGraphExecutionErrorCode::Capture,
            "graph retry guard does not match the original submitted run",
        ));
    }
    let script = identity.capture.scripts.first().ok_or_else(|| {
        LiveGraphExecutionError::new(
            LiveGraphExecutionErrorCode::Capture,
            "original graph run has no retained recipe capture",
        )
    })?;
    if request.recipe_input.source != identity.capture.font.source
        || request.recipe_input.input_hash != script.input_sha256
    {
        return Err(LiveGraphExecutionError::new(
            LiveGraphExecutionErrorCode::Capture,
            "recipe input does not match the original submitted run",
        ));
    }
    if request.base_proof_input.document_revision() != identity.capture.font.document_revision
        || request.base_proof_input.canonical_input_sha256() != identity.capture.font.capture_sha256
    {
        return Err(LiveGraphExecutionError::new(
            LiveGraphExecutionErrorCode::Capture,
            "base proof capture does not match the original submitted run",
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
    request.recipe_input.validate().map_err(|error| {
        LiveGraphExecutionError::new(LiveGraphExecutionErrorCode::Capture, error.to_string())
    })?;
    let snapshot = session.snapshot();
    let python = snapshot
        .graph
        .nodes
        .iter()
        .find(|node| node.type_name == "live.python")
        .ok_or_else(|| {
            LiveGraphExecutionError::new(
                LiveGraphExecutionErrorCode::Capture,
                "graph has no live.python node",
            )
        })?;
    let graph_parameters = python
        .values
        .get("parameters")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let captured_parameters =
        serde_json::to_value(&request.recipe_input.parameters).map_err(|error| {
            LiveGraphExecutionError::new(
                LiveGraphExecutionErrorCode::Capture,
                format!("could not encode captured parameters: {error}"),
            )
        })?;
    if graph_parameters != captured_parameters {
        return Err(LiveGraphExecutionError::new(
            LiveGraphExecutionErrorCode::Capture,
            "script recipe parameters do not match live.python parameters",
        ));
    }
    if request.base_proof_input.document_revision() != project.document_revision() {
        return Err(LiveGraphExecutionError::new(
            LiveGraphExecutionErrorCode::Capture,
            "base compiled proof capture does not match the current document revision",
        ));
    }
    let proof_hash = request.base_proof_input.canonical_input_sha256();
    let proof_digest = proof_hash.strip_prefix("sha256:").unwrap_or(proof_hash);
    if proof_digest.len() != 64
        || !proof_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(LiveGraphExecutionError::new(
            LiveGraphExecutionErrorCode::Capture,
            "base compiled proof capture has no valid canonical SHA-256 identity",
        ));
    }
    Ok(())
}

fn stage_result(
    record: &mut LiveGraphRecord,
    project: &Project,
    result: ScriptRecipeResult,
    stderr: String,
) -> Result<(), LiveGraphExecutionError> {
    if result.edits.is_empty() {
        return Err(LiveGraphExecutionError::new(
            LiveGraphExecutionErrorCode::Staging,
            "live.python must return at least one guarded edit for a derived version",
        ));
    }
    let request = AgentEditRequest {
        expected_document_epoch: record.work.identity.graph.document_epoch.clone(),
        actor: record.actor.clone(),
        operation_key: record.operation_key.clone(),
        authorization: "preview-only".into(),
        source: record.work.identity.capture.font.source,
        history_name: "Nodes Python preview".into(),
        reads: result.reads.clone(),
        edits: result.edits.clone(),
    };
    let staged_edit = request.stage(project).map_err(|error| {
        let message = match error {
            runebender::automation::agent_session::AgentOperationRejection::InvalidRequest(
                message,
            ) => message,
            runebender::automation::agent_session::AgentOperationRejection::Transaction(error) => {
                error.to_string()
            }
        };
        LiveGraphExecutionError::new(LiveGraphExecutionErrorCode::Staging, message)
    })?;
    let proof_recipe = record.proof_recipe.clone();
    let base_input = record.base_input.clone().ok_or_else(|| {
        LiveGraphExecutionError::new(
            LiveGraphExecutionErrorCode::Staging,
            "base compiled proof input is unavailable",
        )
    })?;
    let derived_input = base_input
        .with_staged_edit(project, &staged_edit)
        .map_err(|error| {
            LiveGraphExecutionError::new(LiveGraphExecutionErrorCode::Staging, error)
        })?;
    let derived_version = GraphNodeOutput {
        node: record.work.plan.python_node,
        value: GraphNodeOutputValue::FontVersion {
            source: record.work.identity.capture.font.source,
            version_id: format!("graph-run-{}-staged", record.work.handle.get()),
            version_revision: 0,
            content_sha256: derived_input.canonical_input_sha256().to_owned(),
        },
    };
    record.proof_request = Some(LiveGraphProofRequest {
        identity: record.work.identity.clone(),
        base_input,
        derived_input,
        recipe_result: result,
        stderr,
        proof_recipe,
        derived_version,
    });
    record.phase = LiveGraphPhase::RecipeStaged;
    Ok(())
}

fn proof_recipe(
    graph: &runebender::workflows::nodes::NodeGraph,
    proofs: &[runebender::workflows::nodes_session::GraphProofCapture],
) -> Result<CompiledProofRecipe, LiveGraphExecutionError> {
    let read = |node| {
        graph
            .node(node)
            .and_then(|node| node.values.get("recipe"))
            .cloned()
            .unwrap_or_else(runebender::workflows::nodes_live::default_proof_recipe)
    };
    let [unchanged, changed] = proofs else {
        return Err(LiveGraphExecutionError::new(
            LiveGraphExecutionErrorCode::Proof,
            "comparison run must capture exactly two proof nodes",
        ));
    };
    let unchanged = read(unchanged.node);
    let changed = read(changed.node);
    if unchanged != changed {
        return Err(LiveGraphExecutionError::new(
            LiveGraphExecutionErrorCode::Proof,
            "comparison proof recipes changed after graph validation",
        ));
    }
    let recipe: CompiledProofRecipe = serde_json::from_value(unchanged).map_err(|error| {
        LiveGraphExecutionError::new(
            LiveGraphExecutionErrorCode::Proof,
            format!("invalid compiled proof recipe: {error}"),
        )
    })?;
    recipe
        .validate()
        .map_err(|error| LiveGraphExecutionError::new(LiveGraphExecutionErrorCode::Proof, error))?;
    Ok(recipe)
}

fn finish_failed(
    session: &mut GraphSession,
    record: &mut LiveGraphRecord,
    project: &impl CurrentProject,
    code: &str,
    message: &str,
) {
    let current = project.graph_document_state(&record.work.identity.graph.document_epoch);
    if let Ok(inspection) = session.complete_run(
        GraphRunCompletion {
            handle: record.work.handle,
            identity: record.work.identity.clone(),
            outcome: GraphRunOutcome::Failed(vec![run_error(
                code,
                message,
                Some(record.work.plan.python_node),
            )]),
        },
        &current,
    ) {
        record.phase = LiveGraphPhase::Terminal(inspection.status);
    }
}

fn finish_cancelled(
    session: &mut GraphSession,
    record: &mut LiveGraphRecord,
    project: &impl CurrentProject,
) {
    let current = project.graph_document_state(&record.work.identity.graph.document_epoch);
    if let Ok(inspection) = session.complete_run(
        GraphRunCompletion {
            handle: record.work.handle,
            identity: record.work.identity.clone(),
            outcome: GraphRunOutcome::Cancelled,
        },
        &current,
    ) {
        record.phase = LiveGraphPhase::Terminal(inspection.status);
    }
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

struct ProjectRevision<'a>(&'a GraphDocumentState);

impl CurrentProject for ProjectRevision<'_> {
    fn graph_document_state(&self, _epoch: &str) -> GraphDocumentState {
        self.0.clone()
    }
}

fn graph_error(
    error: runebender::workflows::nodes_session::GraphSessionError,
) -> LiveGraphExecutionError {
    LiveGraphExecutionError::new(LiveGraphExecutionErrorCode::Graph, error.to_string())
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

fn run_error(
    code: &str,
    message: &str,
    node: Option<u32>,
) -> runebender::workflows::nodes_session::GraphRunError {
    let mut message = message.to_owned();
    while message.len() > 4096 {
        message.pop();
    }
    runebender::workflows::nodes_session::GraphRunError {
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
    use crate::application::platform::script_jobs::ScriptJobCancelOutcome;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::application::platform::script_jobs::{
        ScriptJobConfig, ScriptJobStatus, ScriptRuntimeAvailability,
    };
    use runebender::automation::agent_edit::AgentLayerGuard;
    use runebender::automation::script_recipe::{SCRIPT_RECIPE_SCHEMA_VERSION, ScriptRecipeLayer};
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
            }],
        )
        .unwrap()
    }

    fn script() -> String {
        format!(
            "import json, sys\ndata=json.load(sys.stdin)\nresult={{'schema_version':{},'job_id':data['job_id'],'input_hash':data['input_hash'],'report':'staged width','reads':[],'edits':[{{'target':data['layers'][0]['guard'],'operations':[{{'op':'set_width','width':777.0}}]}}]}}\njson.dump(result,sys.stdout)\n",
            SCRIPT_RECIPE_SCHEMA_VERSION
        )
    }

    fn setup(project: &Project) -> (GraphSession, LiveGraphSubmitRequest) {
        let source = project.source_id(0).unwrap();
        let mut graph = nodes_live::comparison_starter(source);
        graph
            .node_mut(3)
            .unwrap()
            .values
            .insert("code".into(), Value::String(script()));
        let session = GraphSession::new("graph", "document", graph, Registry::core()).unwrap();
        let snapshot = session.snapshot();
        let request = LiveGraphSubmitRequest {
            guard: GraphSemanticGuard {
                identity: snapshot.identity,
                semantic_revision: snapshot.semantic_revision,
                semantic_hash: snapshot.semantic_hash,
            },
            actor: "test".into(),
            operation_key: "run".into(),
            recipe_input: input(project, "job-1"),
            base_proof_input: compiled_proof::capture(project).unwrap(),
        };
        (session, request)
    }

    fn wait_for_staging(
        adapter: &mut LiveGraphExecution,
        session: &mut GraphSession,
        queue: &ScriptJobQueue,
        project: &Project,
        handle: GraphRunHandle,
    ) {
        let started = Instant::now();
        loop {
            adapter.poll_scripts(session, queue, project);
            if !matches!(
                adapter.phase(handle),
                Some(LiveGraphPhase::ScriptQueued | LiveGraphPhase::ScriptRunning)
            ) {
                return;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn exact_retry_replays_retained_capture_after_graph_change() {
        let Some(python) = python() else {
            return;
        };
        let project = project();
        let (mut session, request) = setup(&project);
        let retry = LiveGraphSubmitRequest {
            guard: request.guard.clone(),
            actor: request.actor.clone(),
            operation_key: request.operation_key.clone(),
            recipe_input: request.recipe_input.clone(),
            base_proof_input: request.base_proof_input.clone(),
        };
        let queue = ScriptJobQueue::new(ScriptJobConfig::new(python)).unwrap();
        let mut adapter = LiveGraphExecution::default();
        let submitted = adapter
            .submit(&mut session, &queue, &project, request)
            .unwrap();

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
                        value: Value::String("print('changed after submit')".into()),
                    }],
                },
            })
            .unwrap();

        let replay = adapter
            .submit(&mut session, &queue, &project, retry)
            .unwrap();
        assert_eq!(replay.graph.disposition, GraphReceiptDisposition::Replayed);
        assert_eq!(replay.graph.receipt, submitted.graph.receipt);
        assert_eq!(replay.graph.receipt.handle, submitted.graph.receipt.handle);

        let mut changed_input = LiveGraphSubmitRequest {
            guard: GraphSemanticGuard {
                identity: submitted.graph.receipt.identity.graph.clone(),
                semantic_revision: submitted.graph.receipt.identity.semantic_revision,
                semantic_hash: submitted.graph.receipt.identity.semantic_hash.clone(),
            },
            actor: "test".into(),
            operation_key: "run".into(),
            recipe_input: input(&project, "job-1"),
            base_proof_input: compiled_proof::capture(&project).unwrap(),
        };
        changed_input.recipe_input.input_hash = sha256(b"different-input");
        let error = adapter
            .submit(&mut session, &queue, &project, changed_input)
            .unwrap_err();
        assert_eq!(error.code, LiveGraphExecutionErrorCode::Capture);

        let mut changed_project = self::project();
        let source = changed_project.source_id(0).unwrap();
        let layer = changed_project
            .document_source(source)
            .unwrap()
            .default_layer();
        changed_project
            .edit_document_layer("A", &layer, |draft| {
                draft.set_width(701.0)?;
                Ok(())
            })
            .unwrap();
        let changed_base = LiveGraphSubmitRequest {
            guard: GraphSemanticGuard {
                identity: submitted.graph.receipt.identity.graph.clone(),
                semantic_revision: submitted.graph.receipt.identity.semantic_revision,
                semantic_hash: submitted.graph.receipt.identity.semantic_hash.clone(),
            },
            actor: "test".into(),
            operation_key: "run".into(),
            recipe_input: input(&project, "job-1"),
            base_proof_input: compiled_proof::capture(&changed_project).unwrap(),
        };
        let error = adapter
            .submit(&mut session, &queue, &project, changed_base)
            .unwrap_err();
        assert_eq!(error.code, LiveGraphExecutionErrorCode::Capture);
    }

    #[test]
    fn queue_full_submission_returns_a_releasable_failed_receipt() {
        let Some(python) = python() else {
            return;
        };
        let project = project();
        let mut config = ScriptJobConfig::new(python);
        config.queue_capacity = 1;
        config.retained_capacity = 4;
        config.deadline = Duration::from_secs(30);
        let queue = ScriptJobQueue::new(config).unwrap();
        let blocker = queue
            .submit(ScriptJobRequest {
                input: input(&project, "queue-blocker"),
                script: format!("import time\ntime.sleep(30)\n{}", script()),
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while queue.inspect(blocker).unwrap().status != ScriptJobStatus::Running {
            assert!(Instant::now() < deadline, "blocking script did not start");
            std::thread::sleep(Duration::from_millis(10));
        }
        let queued = queue
            .submit(ScriptJobRequest {
                input: input(&project, "queue-filler"),
                script: script(),
            })
            .unwrap();

        let (mut session, request) = setup(&project);
        let retry = LiveGraphSubmitRequest {
            guard: request.guard.clone(),
            actor: request.actor.clone(),
            operation_key: request.operation_key.clone(),
            recipe_input: request.recipe_input.clone(),
            base_proof_input: request.base_proof_input.clone(),
        };
        let mut adapter = LiveGraphExecution::default();
        let submitted = adapter
            .submit(&mut session, &queue, &project, request)
            .unwrap();
        let handle = submitted.graph.receipt.handle;
        assert_eq!(
            submitted.graph.disposition,
            GraphReceiptDisposition::Applied
        );
        assert_eq!(adapter.records[&handle].script, None);
        assert_eq!(
            adapter.phase(handle),
            Some(LiveGraphPhase::Terminal(GraphRunStatus::Failed))
        );
        let inspection = session.inspect_run(handle).unwrap();
        assert_eq!(inspection.status, GraphRunStatus::Failed);
        assert_eq!(inspection.errors[0].code, "script_submit");
        assert!(adapter.records.get(&handle).unwrap().base_input.is_none());

        let replay = adapter
            .submit(
                &mut session,
                &queue,
                &project,
                LiveGraphSubmitRequest {
                    guard: retry.guard.clone(),
                    actor: retry.actor.clone(),
                    operation_key: retry.operation_key.clone(),
                    recipe_input: retry.recipe_input.clone(),
                    base_proof_input: retry.base_proof_input.clone(),
                },
            )
            .unwrap();
        assert_eq!(replay.graph.disposition, GraphReceiptDisposition::Replayed);
        assert_eq!(replay.graph.receipt, submitted.graph.receipt);
        assert_eq!(adapter.records[&handle].script, None);

        assert!(adapter.release(&mut session, &queue, handle).unwrap());
        assert_eq!(adapter.phase(handle), Some(LiveGraphPhase::Released));
        let released_replay = adapter
            .submit(&mut session, &queue, &project, retry)
            .unwrap();
        assert_eq!(
            released_replay.graph.disposition,
            GraphReceiptDisposition::Replayed
        );
        assert_eq!(released_replay.graph.receipt, submitted.graph.receipt);
        assert_eq!(adapter.records[&handle].script, None);

        assert_eq!(
            queue.cancel(queued),
            ScriptJobCancelOutcome::CancelledBeforeStart
        );
        assert_eq!(
            queue.cancel(blocker),
            ScriptJobCancelOutcome::CancellationRequested
        );
    }

    #[test]
    fn cancellation_discards_terminal_script_before_staging() {
        let Some(python) = python() else {
            return;
        };
        let project = project();
        let (mut session, request) = setup(&project);
        let queue = ScriptJobQueue::new(ScriptJobConfig::new(python)).unwrap();
        let mut adapter = LiveGraphExecution::default();
        let submitted = adapter
            .submit(&mut session, &queue, &project, request)
            .unwrap();
        let script = adapter.records[&submitted.graph.receipt.handle]
            .script
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if queue.inspect(script).unwrap().status == ScriptJobStatus::Completed {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "script did not finish before cancellation"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        let identity = submitted.graph.receipt.identity.clone();
        let cancelled = adapter
            .cancel(
                &mut session,
                &queue,
                GraphCancelRequest {
                    identity: identity.graph.clone(),
                    handle: submitted.graph.receipt.handle,
                    actor: "test".into(),
                    operation_key: "cancel-before-poll".into(),
                },
                &GraphDocumentState {
                    document_epoch: identity.graph.document_epoch,
                    document_revision: identity.capture.font.document_revision,
                },
            )
            .unwrap();
        assert_eq!(
            cancelled.graph.receipt.outcome,
            GraphCancelOutcome::CancellationRequested
        );

        adapter.poll_scripts(&mut session, &queue, &project);
        assert_eq!(
            adapter.phase(submitted.graph.receipt.handle),
            Some(LiveGraphPhase::Terminal(GraphRunStatus::Cancelled))
        );
        assert!(
            adapter
                .take_proof_request(submitted.graph.receipt.handle)
                .is_err()
        );
    }

    #[test]
    fn stages_recipe_without_mutating_root_then_publishes_both_proofs() {
        let Some(python) = python() else {
            return;
        };
        let project = project();
        let root_revision = project.document_revision();
        let (mut session, request) = setup(&project);
        let queue = ScriptJobQueue::new(ScriptJobConfig::new(python)).unwrap();
        let mut adapter = LiveGraphExecution::default();
        let submitted = adapter
            .submit(&mut session, &queue, &project, request)
            .unwrap();
        let handle = submitted.graph.receipt.handle;
        wait_for_staging(&mut adapter, &mut session, &queue, &project, handle);
        assert_eq!(adapter.phase(handle), Some(LiveGraphPhase::RecipeStaged));
        assert_eq!(project.document_revision(), root_revision);
        let proof = adapter.take_proof_request(handle).unwrap();
        assert_eq!(proof.recipe_result.report, "staged width");
        assert_eq!(proof.base_input.document_revision(), root_revision);
        assert_ne!(
            proof.base_input.canonical_input_sha256(),
            proof.derived_input.canonical_input_sha256()
        );
        let inspection = adapter
            .publish_proofs(
                &mut session,
                handle,
                &GraphDocumentState {
                    document_epoch: "document".into(),
                    document_revision: root_revision,
                },
                LiveGraphProofOutputs {
                    unchanged_artifact_id: "proof-base".into(),
                    unchanged_content_sha256: sha256(b"base"),
                    unchanged_canonical_input_sha256: proof
                        .identity
                        .capture
                        .font
                        .capture_sha256
                        .clone(),
                    unchanged_font_sha256: sha256(b"base-font"),
                    changed_artifact_id: "proof-derived".into(),
                    changed_content_sha256: sha256(b"derived"),
                    changed_canonical_input_sha256: proof
                        .derived_input
                        .canonical_input_sha256()
                        .to_owned(),
                    changed_font_sha256: sha256(b"derived-font"),
                },
            )
            .unwrap();
        assert_eq!(inspection.status, GraphRunStatus::Completed);
        assert_eq!(inspection.outputs.len(), 3);
        assert_eq!(project.document_revision(), root_revision);
        assert_eq!(
            adapter.result_summary(handle).unwrap().report,
            "staged width"
        );
        let apply = adapter
            .apply_request(
                &session,
                &GraphDocumentState {
                    document_epoch: "document".into(),
                    document_revision: root_revision,
                },
                handle,
                "test",
                "apply-1",
                "user-approved",
            )
            .unwrap();
        assert_eq!(apply.expected_document_epoch, "document");
        assert_eq!(apply.source, project.source_id(0).unwrap().0);
        assert_eq!(apply.edits.len(), 1);

        let stale_document = adapter
            .apply_request(
                &session,
                &GraphDocumentState {
                    document_epoch: "document".into(),
                    document_revision: root_revision + 1,
                },
                handle,
                "test",
                "apply-after-document-change",
                "user-approved",
            )
            .unwrap_err();
        assert_eq!(stale_document.code, LiveGraphExecutionErrorCode::Stale);

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
                        value: Value::String("print('changed after proof')".into()),
                    }],
                },
            })
            .unwrap();
        let stale = adapter
            .apply_request(
                &session,
                &GraphDocumentState {
                    document_epoch: "document".into(),
                    document_revision: root_revision,
                },
                handle,
                "test",
                "apply-after-change",
                "user-approved",
            )
            .unwrap_err();
        assert_eq!(stale.code, LiveGraphExecutionErrorCode::Stale);
    }

    #[test]
    fn changed_project_makes_a_late_recipe_stale() {
        let Some(python) = python() else {
            return;
        };
        let mut project = project();
        let (mut session, request) = setup(&project);
        let queue = ScriptJobQueue::new(ScriptJobConfig::new(python)).unwrap();
        let mut adapter = LiveGraphExecution::default();
        let submitted = adapter
            .submit(&mut session, &queue, &project, request)
            .unwrap();
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        project
            .edit_document_layer("A", &layer, |draft| {
                draft.set_width(701.0)?;
                Ok(())
            })
            .unwrap();
        let handle = submitted.graph.receipt.handle;
        wait_for_staging(&mut adapter, &mut session, &queue, &project, handle);
        assert_eq!(
            adapter.phase(handle),
            Some(LiveGraphPhase::Terminal(GraphRunStatus::Stale))
        );
        assert!(session.inspect_run(handle).unwrap().outputs.is_empty());
    }
}
