// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Native intent commands for the live Nodes comparison surface.
//!
//! Views project immutable graph and proof state; this module owns selection, guarded graph
//! edits, explicit run/cancel/release/apply requests, and the presentation byte cache.

use crate::application::workspace::Workspace;
use runebender::document::nodes_session::GraphGuard;

#[cfg(unix)]
use crate::application::platform::nodes_proofs::NodeProofInspection;
#[cfg(unix)]
use runebender::document::agent::ToolCall;
#[cfg(unix)]
use runebender::document::nodes::NodeGraph;
#[cfg(unix)]
use runebender::document::nodes_session::{
    GraphEdit, GraphInteractiveMutationRequest, GraphMutation, GraphRunStatus,
};
#[cfg(unix)]
use runebender::ui::nodes::ImmutablePng;
#[cfg(unix)]
use std::sync::Arc;

/// Select the one unsaved comparison graph and initialize its explicit glyph scope.
pub(crate) fn select_live_comparison(app: &mut Workspace) {
    #[cfg(unix)]
    match app.ensure_live_graph() {
        Ok(()) => {
            if app.nodes.live_scope.trim().is_empty() {
                app.nodes.live_scope = selected_glyph_names(app).join(", ");
            }
            app.nodes.live_selected = true;
            app.nodes.fit_request = app.nodes.fit_request.wrapping_add(1);
            app.note = if app.nodes.live_scope.is_empty() {
                "Live comparison opened; enter at least one glyph before Run".into()
            } else {
                "Unsaved live comparison opened".into()
            };
        }
        Err(error) => app.note = error,
    }
    #[cfg(not(unix))]
    {
        app.note = "Live comparison execution is available in the native editor".into();
    }
}

/// Keep the explicit comparison scope as visible presentation input until Run.
pub(crate) fn edit_live_scope(app: &mut Workspace, scope: String) {
    app.nodes.live_scope = scope;
}

/// Commit one focused `TextArea` value through the canonical graph guard.
pub(crate) fn edit_live_code(app: &mut Workspace, guard: GraphGuard, node: u32, code: String) {
    #[cfg(unix)]
    live_interactive_mutation(
        app,
        guard,
        vec![GraphEdit::SetValue {
            node,
            field: "code".into(),
            value: serde_json::Value::String(code),
        }],
    );
    #[cfg(not(unix))]
    {
        let _ = (guard, node, code);
        app.note = "Live graph editing is available in the native editor".into();
    }
}

/// Commit only the final header-drag position through the canonical graph guard.
pub(crate) fn move_live_node(app: &mut Workspace, guard: GraphGuard, node: u32, pos: [f32; 2]) {
    #[cfg(unix)]
    live_interactive_mutation(app, guard, vec![GraphEdit::MoveNode { node, pos }]);
    #[cfg(not(unix))]
    {
        let _ = (guard, node, pos);
        app.note = "Live graph editing is available in the native editor".into();
    }
}

/// Apply an authored topology/value graph change against the exact snapshot the canvas displayed.
#[cfg(unix)]
pub(crate) fn change_live_graph(
    app: &mut Workspace,
    guard: GraphGuard,
    before: NodeGraph,
    after: NodeGraph,
) {
    let edits = match graph_edits(&before, &after) {
        Ok(edits) => edits,
        Err(error) => {
            app.note = error;
            return;
        }
    };
    if edits.is_empty() {
        return;
    }
    live_interactive_mutation(app, guard, edits);
}

/// Browser builds never select the native comparison, but keep the view callback portable.
#[cfg(not(unix))]
pub(crate) fn change_live_graph(
    app: &mut Workspace,
    _guard: GraphGuard,
    _before: runebender::document::nodes::NodeGraph,
    _after: runebender::document::nodes::NodeGraph,
) {
    app.note = "Live graph editing is available in the native editor".into();
}

#[cfg(unix)]
fn live_interactive_mutation(app: &mut Workspace, guard: GraphGuard, edits: Vec<GraphEdit>) {
    let result = app
        .live_graph_session_mut()
        .ok_or_else(|| "Open the live comparison before editing its graph".to_string())
        .and_then(|session| {
            session
                .mutate_interactive(GraphInteractiveMutationRequest {
                    guard,
                    mutation: GraphMutation::Patch { edits },
                })
                .map_err(|error| error.to_string())
        });
    if let Err(error) = result {
        app.note = error;
    }
}

#[cfg(unix)]
fn graph_edits(before: &NodeGraph, after: &NodeGraph) -> Result<Vec<GraphEdit>, String> {
    if before.version != after.version {
        return Err("The live canvas cannot change the graph file version".into());
    }
    let before_nodes: std::collections::BTreeMap<_, _> =
        before.nodes.iter().map(|node| (node.id, node)).collect();
    let after_nodes: std::collections::BTreeMap<_, _> =
        after.nodes.iter().map(|node| (node.id, node)).collect();
    let mut edits = Vec::new();

    // Disconnect before replacing or removing endpoints, then reconnect after additions.
    for link in before
        .links
        .iter()
        .filter(|link| !after.links.contains(link))
    {
        if after_nodes.contains_key(&link.to()) {
            edits.push(GraphEdit::Disconnect {
                node: link.to(),
                input: link.input().into(),
            });
        }
    }
    for id in before_nodes
        .keys()
        .filter(|id| !after_nodes.contains_key(*id))
    {
        edits.push(GraphEdit::RemoveNode { node: *id });
    }
    for node in after
        .nodes
        .iter()
        .filter(|node| !before_nodes.contains_key(&node.id))
    {
        edits.push(GraphEdit::AddNode { node: node.clone() });
    }
    for (id, old) in &before_nodes {
        let Some(new) = after_nodes.get(id) else {
            continue;
        };
        if old.type_name != new.type_name {
            return Err("Change a node type by removing it and adding a new node".into());
        }
        if old.pos != new.pos {
            edits.push(GraphEdit::MoveNode {
                node: *id,
                pos: new.pos,
            });
        }
        for field in old
            .values
            .keys()
            .filter(|field| !new.values.contains_key(*field))
        {
            edits.push(GraphEdit::RemoveValue {
                node: *id,
                field: field.clone(),
            });
        }
        for (field, value) in &new.values {
            if old.values.get(field) != Some(value) {
                edits.push(GraphEdit::SetValue {
                    node: *id,
                    field: field.clone(),
                    value: value.clone(),
                });
            }
        }
    }
    edits.extend(
        after
            .links
            .iter()
            .filter(|link| !before.links.contains(link))
            .cloned()
            .map(|link| GraphEdit::Connect { link }),
    );
    Ok(edits)
}

/// Start one comparison using the graph's stable source and the visible glyph scope.
pub(crate) fn run_live_comparison(app: &mut Workspace) {
    #[cfg(unix)]
    {
        let Some(session) = app.live_graph_session() else {
            app.note = "Open the live comparison before running it".into();
            return;
        };
        let snapshot = session.snapshot();
        let Some(source) = snapshot
            .graph
            .nodes
            .iter()
            .find(|node| node.type_name == "live.font")
            .and_then(|node| node.values.get("source"))
            .and_then(serde_json::Value::as_u64)
            .and_then(|source| usize::try_from(source).ok())
        else {
            app.note = "The comparison graph has no valid live.font source".into();
            return;
        };
        let glyphs = match parse_scope(app) {
            Ok(glyphs) => glyphs,
            Err(error) => {
                app.note = error;
                return;
            }
        };
        let operation = app
            .live_nodes
            .as_ref()
            .map(|state| state.next_job)
            .unwrap_or_default();
        let response = app.call_live(&ToolCall {
            name: "nodes_run".into(),
            arguments: serde_json::json!({
                "expected_document_epoch": snapshot.identity.document_epoch,
                "guard": {
                    "identity": snapshot.identity,
                    "semantic_revision": snapshot.semantic_revision,
                    "semantic_hash": snapshot.semantic_hash,
                },
                "actor": "native-nodes-ui",
                "operation_key": format!("run-{operation}"),
                "source": source,
                "glyphs": glyphs,
            }),
        });
        if response["ok"] == serde_json::Value::Bool(true) {
            if let Some(handle) = response["run"]["receipt"]["handle"].as_u64() {
                app.nodes.live_ui_handles.insert(handle);
            }
            app.note = "Live comparison running".into();
        } else {
            app.note = response["error"]
                .as_str()
                .unwrap_or("Live comparison could not start")
                .into();
        }
    }
    #[cfg(not(unix))]
    {
        app.note = "Live comparison execution is available in the native editor".into();
    }
}

/// Cancel the newest running comparison started from this native surface.
pub(crate) fn cancel_live_comparison(app: &mut Workspace) {
    #[cfg(unix)]
    {
        let Some(state) = app.live_nodes.as_ref() else {
            app.note = "No live comparison is open".into();
            return;
        };
        let Some(handle) = state.handles.iter().rev().copied().find(|handle| {
            app.nodes.live_ui_handles.contains(&handle.get())
                && state.session.inspect_run(*handle).is_some_and(|run| {
                    matches!(run.status, GraphRunStatus::Queued | GraphRunStatus::Running)
                })
        }) else {
            app.note = "No native Nodes comparison is running".into();
            return;
        };
        let identity = state.session.snapshot().identity;
        let response = app.call_live(&ToolCall {
            name: "nodes_cancel".into(),
            arguments: serde_json::json!({
                "expected_document_epoch": identity.document_epoch,
                "request": {
                    "identity": identity,
                    "handle": handle,
                    "actor": "native-nodes-ui",
                    "operation_key": format!("cancel-{}", handle.get()),
                },
            }),
        });
        app.note = if response["ok"] == serde_json::Value::Bool(true) {
            "Cancelling live comparison".into()
        } else {
            response["error"]
                .as_str()
                .unwrap_or("Live comparison could not be cancelled")
                .into()
        };
    }
    #[cfg(not(unix))]
    {
        app.note = "Live comparison execution is available in the native editor".into();
    }
}

/// Release terminal runs started from this surface and their cached proof bytes.
pub(crate) fn clear_live_results(app: &mut Workspace) {
    #[cfg(unix)]
    {
        let Some(state) = app.live_nodes.as_ref() else {
            app.note = "No live comparison is open".into();
            return;
        };
        let identity = state.session.snapshot().identity;
        let handles: Vec<_> = state
            .handles
            .iter()
            .copied()
            .filter(|handle| app.nodes.live_ui_handles.contains(&handle.get()))
            .filter(|handle| {
                state.session.inspect_run(*handle).is_some_and(|run| {
                    matches!(
                        run.status,
                        GraphRunStatus::Completed
                            | GraphRunStatus::Failed
                            | GraphRunStatus::Cancelled
                            | GraphRunStatus::Stale
                            | GraphRunStatus::Released
                    )
                })
            })
            .collect();
        let artifact_ids: Vec<String> = handles
            .iter()
            .flat_map(|handle| match state.proofs.inspect(handle.get()) {
                Some(NodeProofInspection::Completed { artifact_ids, .. }) => {
                    artifact_ids.into_iter().collect()
                }
                _ => Vec::new(),
            })
            .collect();
        let mut released = 0;
        for handle in handles {
            let response = app.call_live(&ToolCall {
                name: "nodes_release".into(),
                arguments: serde_json::json!({
                    "expected_document_epoch": identity.document_epoch,
                    "identity": identity,
                    "handle": handle,
                }),
            });
            if response["ok"] == serde_json::Value::Bool(true)
                && response["released"] == serde_json::Value::Bool(true)
            {
                released += 1;
                app.nodes.live_ui_handles.remove(&handle.get());
            }
        }
        for artifact in artifact_ids {
            app.nodes.proof_images.remove(&artifact);
        }
        app.note = if released == 0 {
            "No terminal native Nodes results to clear".into()
        } else {
            format!("Cleared {released} native Nodes result(s)")
        };
    }
    #[cfg(not(unix))]
    {
        app.note = "Live comparison execution is available in the native editor".into();
    }
}

/// Apply the newest current retained comparison through the shared guarded edit adapter.
pub(crate) fn apply_live_comparison(app: &mut Workspace) {
    #[cfg(unix)]
    {
        let Some(state) = app.live_nodes.as_ref() else {
            app.note = "No live comparison is open".into();
            return;
        };
        let Some(handle) = state.handles.iter().next_back().copied() else {
            app.note = "Run the comparison before applying it".into();
            return;
        };
        let identity = state.session.snapshot().identity;
        let epoch = identity.document_epoch.clone();
        let revision = app.font.project.document_revision();
        let response = app.call_live(&ToolCall {
            name: "nodes_apply".into(),
            arguments: serde_json::json!({
                "expected_document_epoch": epoch,
                "identity": identity,
                "handle": handle,
                "actor": "native-nodes-ui",
                "operation_key": format!("apply-{}-{revision}", handle.get()),
                "authorization": "user-approved",
            }),
        });
        app.note = if response["ok"] == serde_json::Value::Bool(true) {
            "Applied the selected live comparison; use Undo to revert it".into()
        } else {
            response["error"]
                .as_str()
                .unwrap_or("Live comparison could not be applied")
                .into()
        };
    }
    #[cfg(not(unix))]
    {
        app.note = "Live comparison execution is available in the native editor".into();
    }
}

#[cfg(unix)]
impl Workspace {
    /// Cache each retained exact PNG once, keyed by its immutable proof artifact identity.
    pub(crate) fn sync_live_nodes_presentation(&mut self) {
        let Some(state) = self.live_nodes.as_ref() else {
            self.nodes.proof_images.clear();
            return;
        };
        let completed: Vec<_> = state
            .handles
            .iter()
            .filter_map(|handle| match state.proofs.inspect(handle.get()) {
                Some(NodeProofInspection::Completed {
                    artifact_ids,
                    proofs,
                }) => Some((artifact_ids, proofs)),
                _ => None,
            })
            .collect();
        let retained: std::collections::BTreeSet<_> = completed
            .iter()
            .flat_map(|(artifact_ids, _)| artifact_ids.iter().cloned())
            .collect();
        self.nodes
            .proof_images
            .retain(|artifact, _| retained.contains(artifact));
        let images: Vec<_> = completed
            .into_iter()
            .flat_map(|(artifact_ids, proofs)| artifact_ids.into_iter().zip(proofs))
            .filter(|(artifact, _)| !self.nodes.proof_images.contains_key(artifact))
            .filter_map(|(artifact, proof)| {
                let (width, height) = png_dimensions(&proof.png)?;
                Some((
                    artifact.clone(),
                    ImmutablePng {
                        bytes: Arc::from(proof.png.clone()),
                        width,
                        height,
                        output_hash: artifact,
                    },
                ))
            })
            .collect();
        self.nodes.proof_images.extend(images);
    }
}

#[cfg(unix)]
fn selected_glyph_names(app: &Workspace) -> Vec<String> {
    let mut indices: Vec<_> = app.multi_selected.iter().copied().collect();
    if indices.is_empty()
        && let Some(selected) = app.selected
    {
        indices.push(selected);
    }
    indices.sort_unstable();
    indices.dedup();
    indices
        .into_iter()
        .filter_map(|index| app.font.glyphs.get(index))
        .map(|glyph| glyph.name.clone())
        .collect()
}

#[cfg(unix)]
fn parse_scope(app: &Workspace) -> Result<Vec<String>, String> {
    let mut glyphs = Vec::new();
    for name in app
        .nodes
        .live_scope
        .split(|character: char| character == ',' || character.is_whitespace())
        .filter(|name| !name.is_empty())
    {
        if app.font.project.document_glyph(name).is_none() {
            return Err(format!("Comparison scope contains unknown glyph {name}"));
        }
        if !glyphs.iter().any(|glyph| glyph == name) {
            glyphs.push(name.to_string());
        }
    }
    if glyphs.is_empty() {
        return Err("Enter at least one glyph in the comparison scope".into());
    }
    Ok(glyphs)
}

#[cfg(unix)]
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    (bytes.len() >= 24 && &bytes[..8] == SIGNATURE).then(|| {
        (
            u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
            u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
        )
    })
}
