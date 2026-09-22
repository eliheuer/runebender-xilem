// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! One live graph shared by native authoring, Python execution and agent commands.
//! The existing disk graph workflow remains separate; these commands never save a font.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use runebender::font::variable::SourceId;
use runebender::workflows::nodes::Registry;
use runebender::workflows::nodes_live;
use runebender::workflows::nodes_session::{GraphDocumentState, GraphRunHandle, GraphSession};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::execution::{LiveGraphExecution, LiveGraphPhase, LiveGraphProofOutputs};
use crate::application::platform::nodes_file::{self, LiveGraphFileMetadata};
use crate::application::platform::nodes_proofs::{NodeProofInspection, NodeProofJobs};
use crate::application::workspace::Workspace;

static NEXT_GRAPH_SESSION: AtomicU64 = AtomicU64::new(1);

/// Per-document live graph, retained jobs and bounded transport retry responses.
pub(crate) struct LiveNodesState {
    pub(crate) session: GraphSession,
    pub(crate) execution: LiveGraphExecution,
    pub(crate) proofs: NodeProofJobs,
    pub(crate) handles: BTreeSet<GraphRunHandle>,
    pub(crate) requests: BTreeMap<(String, String), (Value, Value)>,
    pub(crate) next_job: u64,
    pub(crate) file: Option<LiveGraphFileMetadata>,
    pub(crate) saved_revision: Option<u64>,
}

impl Workspace {
    /// Access the canonical live graph used by both UI and agent commands.
    pub(crate) fn live_graph_session(&self) -> Option<&GraphSession> {
        self.live_nodes.as_ref().map(|state| &state.session)
    }

    /// Mutate through `GraphSession`'s guarded methods, never a copied canvas graph.
    pub(crate) fn live_graph_session_mut(&mut self) -> Option<&mut GraphSession> {
        self.live_nodes.as_mut().map(|state| &mut state.session)
    }

    /// Create the unsaved comparison starter lazily for this document lifetime.
    pub(crate) fn ensure_live_graph(&mut self) -> Result<(), String> {
        if self.live_nodes.is_some() {
            return Ok(());
        }
        let epoch = self
            .live
            .as_ref()
            .ok_or("native live endpoint is unavailable")?
            .document_epoch()
            .to_owned();
        let source = self
            .font
            .project
            .source_id(self.font.active())
            .ok_or("active source is unavailable")?;
        let source_location = self
            .font
            .project
            .document_source(source)
            .ok_or("selected source is unavailable")?
            .location();
        let normalized_location: Vec<f64> = self
            .font
            .project
            .axes
            .iter()
            .map(|axis| source_location.get(&axis.name).copied().unwrap_or(0.0))
            .collect();
        let mut graph = nodes_live::comparison_starter(source);
        for node in &mut graph.nodes {
            if node.type_name == "live.proof" {
                node.values
                    .get_mut("recipe")
                    .expect("starter proof has recipe")["normalized_location"] =
                    serde_json::json!(normalized_location);
            }
        }
        self.live_nodes = Some(fresh_live_nodes(self.document_id, epoch, graph, None)?);
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
        self.ensure_live_graph()?;
        let state = self
            .live_nodes
            .as_mut()
            .expect("live graph was initialized");
        let expected_revision = state
            .file
            .as_ref()
            .filter(|metadata| metadata.path == path)
            .map(|metadata| metadata.revision.as_str());
        let snapshot = state.session.snapshot();
        let saved = nodes_file::save(path, &snapshot.graph, expected_revision)
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
            .live_nodes
            .as_ref()
            .is_some_and(|state| !state.handles.is_empty())
        {
            return Err("release current live graph runs before opening another graph".into());
        }
        let mut document = nodes_file::load(path).map_err(|error| error.to_string())?;
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
        let replacement = fresh_live_nodes(
            self.document_id,
            epoch,
            document.graph,
            Some(metadata.clone()),
        )?;
        self.live_nodes = Some(replacement);
        self.nodes.live_selected = true;
        self.nodes.selected = None;
        self.nodes.live_ui_handles.clear();
        self.nodes.proof_images.clear();
        self.nodes.content_sizes.clear();
        self.note = format!(
            "Opened {} for source {source_name}; no comparison was run",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("live graph")
        );
        Ok(metadata)
    }

    /// Observe only owned Python/proof jobs and publish results with their captured identities.
    pub(crate) fn live_nodes_pump(&mut self) {
        let Some(state) = self.live_nodes.as_mut() else {
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
            if state.execution.phase(handle) == Some(LiveGraphPhase::RecipeStaged) {
                match state.execution.take_proof_request(handle) {
                    Ok(request) => {
                        if let Err(error) = state.proofs.submit(
                            handle.get(),
                            request.identity.graph.document_epoch,
                            [request.base_input, request.derived_input],
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
                        self.note = error.to_string();
                    }
                }
            }
            if state.execution.phase(handle) != Some(LiveGraphPhase::ProofsRunning) {
                continue;
            }
            match state.proofs.inspect(handle.get()) {
                Some(NodeProofInspection::Completed {
                    artifact_ids,
                    proofs,
                }) => {
                    let [original, changed] = proofs;
                    let [original_id, changed_id] = artifact_ids;
                    let result = state.execution.publish_proofs(
                        &mut state.session,
                        handle,
                        &current,
                        LiveGraphProofOutputs {
                            unchanged_artifact_id: original_id,
                            unchanged_content_sha256: format!(
                                "sha256:{:x}",
                                Sha256::digest(&original.png)
                            ),
                            unchanged_canonical_input_sha256: original
                                .canonical_input_sha256
                                .clone(),
                            unchanged_font_sha256: original.font_sha256.clone(),
                            changed_artifact_id: changed_id,
                            changed_content_sha256: format!(
                                "sha256:{:x}",
                                Sha256::digest(&changed.png)
                            ),
                            changed_canonical_input_sha256: changed.canonical_input_sha256.clone(),
                            changed_font_sha256: changed.font_sha256.clone(),
                        },
                    );
                    if let Err(error) = result {
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
                    state.proofs.release(handle.get());
                }
                None => {
                    let _ = state.execution.fail_proofs(
                        &mut state.session,
                        handle,
                        &current,
                        "comparison jobs are no longer retained",
                    );
                }
                Some(NodeProofInspection::Pending) => {}
            }
        }
    }
}

fn fresh_live_nodes(
    document_id: u64,
    epoch: String,
    graph: runebender::workflows::nodes::NodeGraph,
    file: Option<LiveGraphFileMetadata>,
) -> Result<LiveNodesState, String> {
    let generation = NEXT_GRAPH_SESSION.fetch_add(1, Ordering::Relaxed);
    let session = GraphSession::new(
        format!("nodes-{document_id}-{generation}"),
        epoch,
        graph,
        Registry::core(),
    )
    .map_err(|error| error.to_string())?;
    Ok(LiveNodesState {
        session,
        execution: LiveGraphExecution::default(),
        proofs: NodeProofJobs::default(),
        handles: BTreeSet::new(),
        requests: BTreeMap::new(),
        next_job: 0,
        saved_revision: file.as_ref().map(|_| 0),
        file,
    })
}

fn parse<T: serde::de::DeserializeOwned>(arguments: &Value) -> Result<T, String> {
    serde_json::from_value(arguments.clone()).map_err(|error| error.to_string())
}

impl Workspace {
    pub(crate) fn call_agent_nodes(
        &mut self,
        call: &runebender::automation::agent::ToolCall,
    ) -> Option<Value> {
        if !matches!(
            call.name.as_str(),
            "nodes_discover"
                | "nodes_snapshot"
                | "nodes_mutate"
                | "nodes_run"
                | "nodes_status"
                | "nodes_cancel"
                | "nodes_release"
                | "nodes_apply"
                | "nodes_image"
        ) {
            return None;
        }
        Some(match self.handle_agent_nodes(call) {
            Ok(value) => value,
            Err(error) => {
                serde_json::json!({"ok":false,"error_code":"nodes_rejected","error":error,"root_changed":false})
            }
        })
    }

    fn handle_agent_nodes(
        &mut self,
        call: &runebender::automation::agent::ToolCall,
    ) -> Result<Value, String> {
        use super::execution::LiveGraphSubmitRequest;
        use base64::Engine as _;
        use runebender::automation::agent_nodes::*;
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
        if call.name == "nodes_run" {
            self.ensure_script_job_queue()?;
        }
        self.ensure_live_graph()?;
        self.live_nodes_pump();
        let current = GraphDocumentState {
            document_epoch: epoch,
            document_revision: self.font.project.document_revision(),
        };
        let state = self
            .live_nodes
            .as_mut()
            .expect("live graph was initialized");
        match call.name.as_str() {
            "nodes_discover" => {
                let request: NodesDiscoverRequest = parse(&call.arguments)?;
                request.validate()?;
                Ok(
                    json!({"ok":true,"identity":state.session.snapshot().identity,"discovery":state.session.discovery(),"recipe_authoring":script_recipe::AUTHORING_INSTRUCTIONS,"root_changed":false}),
                )
            }
            "nodes_snapshot" => {
                let request: NodesSnapshotRequest = parse(&call.arguments)?;
                request.validate()?;
                check_identity(&state.session, &request.identity)?;
                Ok(json!({"ok":true,"snapshot":state.session.snapshot(),"root_changed":false}))
            }
            "nodes_mutate" => {
                let request: NodesMutateRequest = parse(&call.arguments)?;
                request.validate()?;
                let response = state
                    .session
                    .mutate(request.request)
                    .map_err(|error| error.to_string())?;
                Ok(
                    json!({"ok":true,"mutation":response,"snapshot":state.session.snapshot(),"root_changed":false}),
                )
            }
            "nodes_run" => {
                let request: NodesRunRequest = parse(&call.arguments)?;
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
                    let mut replay = response.clone();
                    replay["replayed"] = json!(true);
                    return Ok(replay);
                }
                if state.requests.len() >= 64 {
                    return Err("graph retry retention is full; open a new graph session".into());
                }
                let snapshot = state.session.snapshot();
                let python = snapshot
                    .graph
                    .nodes
                    .iter()
                    .find(|node| node.type_name == "live.python")
                    .ok_or("graph has no Python recipe node")?;
                let parameters = python
                    .values
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let Value::Object(parameters) = parameters else {
                    return Err("Python parameters must be an object".into());
                };
                let input = script_recipe::capture(
                    &self.font.project,
                    SourceId(request.source),
                    &request.glyphs,
                    format!("nodes-{}-{}", self.document_id, state.next_job),
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
                let response =
                    json!({"ok":true,"run":submitted.graph,"replayed":false,"root_changed":false});
                state
                    .requests
                    .insert(key, (call.arguments.clone(), response.clone()));
                Ok(response)
            }
            "nodes_status" => {
                let request: NodesStatusRequest = parse(&call.arguments)?;
                request.validate()?;
                check_identity(&state.session, &request.identity)?;
                let inspection = state
                    .session
                    .inspect_run(request.handle)
                    .ok_or("unknown graph run")?;
                let fresh = run_is_current(&state.session, &inspection, &current);
                let summary = state.execution.result_summary(request.handle);
                Ok(
                    json!({"ok":true,"run":inspection,"current":fresh,"stale":!fresh,
                    "report":summary.as_ref().map(|summary|&summary.report),
                    "stderr":summary.as_ref().map(|summary|&summary.stderr),
                    "can_apply":fresh && summary.is_some_and(|summary|summary.can_apply),"root_changed":false}),
                )
            }
            "nodes_cancel" => {
                let request: NodesCancelRequest = parse(&call.arguments)?;
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
                Ok(json!({"ok":true,"cancellation":response.graph,"root_changed":false}))
            }
            "nodes_release" => {
                let request: NodesReleaseRequest = parse(&call.arguments)?;
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
                Ok(json!({"ok":true,"released":released,"root_changed":false}))
            }
            "nodes_apply" => {
                let request: NodesApplyRequest = parse(&call.arguments)?;
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
                        request.actor,
                        request.operation_key,
                        request.authorization,
                    )
                    .map_err(|error| error.to_string())?;
                let response = self
                    .call_agent_edit(&runebender::automation::agent::ToolCall {
                        name: "agent_apply".into(),
                        arguments: serde_json::to_value(edit).map_err(|error| error.to_string())?,
                    })
                    .ok_or("edit adapter unavailable")?;
                self.live_nodes
                    .as_mut()
                    .expect("Apply does not replace the graph")
                    .requests
                    .insert(key, (call.arguments.clone(), response.clone()));
                Ok(response)
            }
            "nodes_image" => {
                let request: NodesImageRequest = parse(&call.arguments)?;
                request.validate()?;
                check_identity(&state.session, &request.identity)?;
                let inspection = state
                    .session
                    .inspect_run(request.handle)
                    .ok_or("unknown graph run")?;
                let fresh = run_is_current(&state.session, &inspection, &current);
                let Some(NodeProofInspection::Completed {
                    artifact_ids,
                    proofs,
                }) = state.proofs.inspect(request.handle.get())
                else {
                    return Err("comparison images are not available".into());
                };
                let index = match request.branch {
                    NodesImageBranch::Original => 0,
                    NodesImageBranch::Changed => 1,
                };
                let proof = &proofs[index];
                if proof.png.len() > 5 * 1024 * 1024 {
                    return Err("specimen exceeds the transport image limit".into());
                }
                Ok(
                    json!({"ok":true,"artifact_id":artifact_ids[index],"current":fresh,"stale":!fresh,
                    "captured_document_epoch":inspection.identity.graph.document_epoch,
                    "captured_document_revision":proof.document_revision,"font_sha256":proof.font_sha256,
                    "canonical_input_sha256":proof.canonical_input_sha256,"recipe":proof.recipe,"glyphs":proof.glyphs,
                    "png_base64":base64::engine::general_purpose::STANDARD.encode(&proof.png),"root_changed":false}),
                )
            }
            _ => Err("unknown Nodes command".into()),
        }
    }
}

fn check_identity(
    session: &GraphSession,
    identity: &runebender::workflows::nodes_session::GraphIdentity,
) -> Result<(), String> {
    if &session.snapshot().identity != identity {
        return Err("stale graph session".into());
    }
    Ok(())
}

fn run_is_current(
    session: &GraphSession,
    run: &runebender::workflows::nodes_session::GraphRunInspection,
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
    use runebender::workflows::nodes_session::{
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
        let path = root.0.join("comparison.nodes.json");
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
        let state = app.live_nodes.as_ref().unwrap();
        assert!(state.handles.is_empty());
        assert!(state.requests.is_empty());
        assert_eq!(state.next_job, 0);
        assert!(app.nodes.proof_images.is_empty());
        assert_eq!(app.font.project.document_revision(), document_revision);
        assert_eq!(app.font.project.is_modified(), modified);
        assert_eq!(app.script_jobs.is_some(), queue_was_initialized);

        let mut oversized = reopened.graph.clone();
        while oversized.nodes.len() <= 64 {
            oversized.add("live.proof", [0.0, 0.0]);
        }
        let oversized_path = root.0.join("oversized.nodes.json");
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
        app.live_nodes.as_mut().unwrap().handles.insert(retained);
        let error = app.open_live_graph_file(&path, source).unwrap_err();
        assert!(error.contains("release current"));
        assert_eq!(
            app.live_graph_session().unwrap().snapshot().identity,
            current_identity
        );
        app.live_nodes.as_mut().unwrap().handles.remove(&retained);

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
        use crate::application::view::theme::Palette;
        use crate::application::workspace::Mode;
        use std::sync::Arc;
        use xilem::WidgetView as _;

        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let mode = std::mem::replace(&mut app.mode, Mode::Nodes);
        app.nodes.live_selected = true;
        app.nodes.live_scope = "A".into();
        app.sync_live_nodes_presentation();
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
                    .join(format!("nodes-{theme}.png"))
                    .to_str()
                    .unwrap(),
            );
        }
        app.mode = mode;
        app
    }

    #[test]
    fn node_commands_share_images_retry_receipts_apply_and_ordinary_undo() {
        let mut project = Project::new_font(std::env::temp_dir().join("nodes-command-unsaved.ufo"));
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
            "nodes_discover",
            json!({"expected_document_epoch":epoch}),
        );
        assert_eq!(discovery["ok"], true, "{discovery}");
        let identity = discovery["identity"].clone();
        let script = r#"import json, sys
p=json.load(sys.stdin)
edits=[{"target":layer["guard"],"operations":[{"op":"set_width","width":layer["width"]+100}]} for layer in p["layers"]]
json.dump({"schema_version":1,"job_id":p["job_id"],"input_hash":p["input_hash"],"report":"Increase selected widths by 100","reads":[],"edits":edits},sys.stdout)
"#;
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
        let request = json!({"expected_document_epoch":epoch,"guard":{"identity":identity,"semantic_revision":snapshot.semantic_revision,"semantic_hash":snapshot.semantic_hash},"actor":"nodes-test","operation_key":"run-one","source":0,"glyphs":["A"]});
        let started = call(&mut app, "nodes_run", request.clone());
        assert_eq!(started["ok"], true, "{started}");
        let handle = started["run"]["receipt"]["handle"].clone();
        let status_request =
            json!({"expected_document_epoch":epoch,"identity":identity,"handle":handle});
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let status = call(&mut app, "nodes_status", status_request.clone());
            assert_eq!(status["ok"], true, "{status}");
            match status["run"]["status"].as_str() {
                Some("completed") => {
                    assert_eq!(status["can_apply"], true);
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
        let original = call(&mut app, "nodes_image", image_request.clone());
        image_request["branch"] = json!("changed");
        let changed = call(&mut app, "nodes_image", image_request);
        assert_eq!(original["ok"], true, "{original}");
        assert_eq!(changed["ok"], true, "{changed}");
        assert_ne!(original["font_sha256"], changed["font_sha256"]);
        assert_ne!(original["png_base64"], changed["png_base64"]);
        app = capture_completed_comparison(app);
        let apply = json!({"expected_document_epoch":epoch,"identity":identity,"handle":handle,"actor":"nodes-test","operation_key":"apply-one","authorization":"user-approved"});
        let applied = call(&mut app, "nodes_apply", apply.clone());
        assert_eq!(applied["ok"], true, "{applied}");
        assert_eq!(
            app.font
                .project
                .document_layer("A", &layer)
                .unwrap()
                .width(),
            500.0
        );
        app.mode = crate::application::workspace::Mode::Nodes;
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
        assert!(app.can_metadata_history_step(true));
        app.undo_active_edit(true);
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
        let replay = call(&mut app, "nodes_apply", apply);
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
        let run_replay = call(&mut app, "nodes_run", request);
        assert_eq!(run_replay["replayed"], true);
        assert_eq!(run_replay["run"], started["run"]);
        assert_eq!(
            call(&mut app, "nodes_status", status_request.clone())["stale"],
            true
        );
        assert_eq!(
            call(&mut app, "nodes_release", status_request)["released"],
            true
        );
    }
}
