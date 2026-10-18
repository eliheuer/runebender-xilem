// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Native intent commands for the live Graph comparison surface.
//!
//! Views project immutable graph and proof state; this module owns selection, guarded graph
//! edits, explicit run/cancel/release/apply requests, and the presentation byte cache.

use crate::application::workspace::Workspace;
use runebender::font::variable::SourceId;
use runebender::workflows::graph_session::GraphGuard;

use crate::application::platform::dialogs;
#[cfg(unix)]
use runebender::automation::agent::ToolCall;
use runebender::ui::graph::ImmutablePng;
#[cfg(unix)]
use runebender::workflows::graph::NodeGraph;
#[cfg(unix)]
use runebender::workflows::graph_session::{
    GraphEdit, GraphInteractiveMutationRequest, GraphMutation, GraphRunStatus,
};
#[cfg(unix)]
use std::sync::Arc;

/// Select the one unsaved comparison graph and initialize its explicit glyph scope.
pub(crate) fn select_live_comparison(app: &mut Workspace) {
    #[cfg(unix)]
    match app.ensure_live_graph() {
        Ok(()) => {
            if app.graph.live_scope.trim().is_empty() {
                app.graph.live_scope = selected_glyph_names(app).join(", ");
            }
            app.graph.live_selected = true;
            app.graph.fit_request = app.graph.fit_request.wrapping_add(1);
            app.note = if app.graph.live_scope.is_empty() {
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

/// Open graph intent for the exact source named by the invoking control.
pub(crate) fn open_live_comparison_file(app: &mut Workspace, explicit_source: SourceId) {
    #[cfg(unix)]
    {
        if app.font.project.document_source(explicit_source).is_none() {
            app.note = "The source selected by Open is no longer loaded".into();
            return;
        }
        if app.live_graph.as_ref().is_some_and(|state| {
            let revision = state.session.snapshot().revision;
            state
                .saved_revision
                .map_or(revision != 0, |saved| saved != revision)
        }) {
            app.note = "Save the edited comparison graph before opening another one".into();
            return;
        }
        if app
            .live_graph
            .as_ref()
            .is_some_and(|state| !state.handles.is_empty())
        {
            app.note = "Release current comparison results before opening another graph".into();
            return;
        }
        let start = app
            .live_graph
            .as_ref()
            .and_then(|state| state.file.as_ref())
            .and_then(|metadata| metadata.path.parent())
            .or_else(|| app.font.document_source().parent())
            .unwrap_or_else(|| std::path::Path::new("."));
        if let Some(path) = dialogs::nodes(start) {
            match app.open_live_graph_file(&path, explicit_source) {
                Ok(_) => {
                    app.graph.fit_request = app.graph.fit_request.wrapping_add(1);
                }
                Err(error) => app.note = error,
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = explicit_source;
        app.note = "Live graph files are available in the native editor".into();
    }
}

/// Save live graph intent to an exact user-selected path.
pub(crate) fn save_live_comparison_file(app: &mut Workspace) {
    #[cfg(unix)]
    {
        let start = app
            .live_graph
            .as_ref()
            .and_then(|state| state.file.as_ref())
            .and_then(|metadata| metadata.path.parent())
            .or_else(|| app.font.document_source().parent())
            .unwrap_or_else(|| std::path::Path::new("."));
        let suggested = app
            .live_graph
            .as_ref()
            .and_then(|state| state.file.as_ref())
            .and_then(|metadata| metadata.path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("comparison.graph.json");
        if let Some(path) = dialogs::save_graph(start, suggested) {
            match app.save_live_graph_file(&path) {
                Ok(metadata) => {
                    app.note = format!(
                        "Saved {}",
                        metadata
                            .path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("comparison graph")
                    );
                }
                Err(error) => app.note = error,
            }
        }
    }
    #[cfg(not(unix))]
    {
        app.note = "Live graph files are available in the native editor".into();
    }
}

/// Write a rendered proof to a PNG file the person picks, named after the
/// family and the node, beside the font by default.
pub(crate) fn save_proof_image(app: &mut Workspace, png: &ImmutablePng, title: &str) {
    let start = app
        .font
        .document_source()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();
    let family = app
        .font
        .font_info()
        .names
        .family_name
        .clone()
        .unwrap_or_else(|| "Font".into());
    let suggested = format!("{family} {title}.png");
    let Some(path) = dialogs::save_png(&start, &suggested) else {
        return;
    };
    app.note = match std::fs::write(&path, &png.bytes) {
        Ok(()) => format!(
            "Saved {}",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("image")
        ),
        Err(error) => format!("Could not save the image: {error}"),
    };
}

/// Keep the explicit comparison scope as visible presentation input until Run.
pub(crate) fn edit_live_scope(app: &mut Workspace, scope: String) {
    app.graph.live_scope = scope;
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
    _before: runebender::workflows::graph::NodeGraph,
    _after: runebender::workflows::graph::NodeGraph,
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
pub(crate) fn graph_edits(before: &NodeGraph, after: &NodeGraph) -> Result<Vec<GraphEdit>, String> {
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
            app.note = "Open the live graph before running it".into();
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
            .live_graph
            .as_ref()
            .map(|state| state.next_job)
            .unwrap_or_default();
        let response = app.call_live(&ToolCall {
            name: "graph_run".into(),
            arguments: serde_json::json!({
                "expected_document_epoch": snapshot.identity.document_epoch,
                "guard": {
                    "identity": snapshot.identity,
                    "semantic_revision": snapshot.semantic_revision,
                    "semantic_hash": snapshot.semantic_hash,
                },
                "execution_version": 2,
                "actor": "native-graph-ui",
                "operation_key": format!("run-{operation}"),
                "source": source,
                "glyphs": glyphs,
            }),
        });
        if response["ok"] == serde_json::Value::Bool(true) {
            if let Some(handle) = response["run"]["receipt"]["handle"].as_u64() {
                app.graph.live_ui_handles.insert(handle);
            }
            app.note = "Rendering the live graph".into();
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
        let Some(state) = app.live_graph.as_ref() else {
            app.note = "No live comparison is open".into();
            return;
        };
        let Some(handle) = state.handles.iter().rev().copied().find(|handle| {
            app.graph.live_ui_handles.contains(&handle.get())
                && state.session.inspect_run(*handle).is_some_and(|run| {
                    matches!(run.status, GraphRunStatus::Queued | GraphRunStatus::Running)
                })
        }) else {
            app.note = "No native Graph comparison is running".into();
            return;
        };
        let identity = state.session.snapshot().identity;
        let response = app.call_live(&ToolCall {
            name: "graph_cancel".into(),
            arguments: serde_json::json!({
                "expected_document_epoch": identity.document_epoch,
                "request": {
                    "identity": identity,
                    "handle": handle,
                    "actor": "native-graph-ui",
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

/// Release this surface's finished live results and drop their images.
pub(crate) fn clear_live_results(app: &mut Workspace) {
    #[cfg(unix)]
    {
        if app.live_graph.is_none() {
            app.note = "No live graph is open".into();
            return;
        }
        let released = release_live_results(app, false);
        app.note = if released == 0 {
            "No finished Graph results to clear".into()
        } else {
            format!("Cleared {released} Graph result(s)")
        };
    }
    #[cfg(not(unix))]
    {
        app.note = "Live graph execution is available in the native editor".into();
    }
}

/// Release finished runs this surface started, and their images. With
/// `keep_newest`, the newest finished run stays, so its proofs remain on the
/// canvas until a newer run replaces them. Returns how many were released.
#[cfg(unix)]
fn release_live_results(app: &mut Workspace, keep_newest: bool) -> usize {
    let Some(state) = app.live_graph.as_ref() else {
        return 0;
    };
    let identity = state.session.snapshot().identity;
    let mut handles: Vec<_> = state
        .handles
        .iter()
        .copied()
        .filter(|handle| app.graph.live_ui_handles.contains(&handle.get()))
        .filter(|handle| {
            state.session.inspect_run(*handle).is_some_and(|run| {
                matches!(
                    run.status,
                    GraphRunStatus::Completed
                        | GraphRunStatus::PartiallyFailed
                        | GraphRunStatus::Failed
                        | GraphRunStatus::Cancelled
                        | GraphRunStatus::Stale
                        | GraphRunStatus::Released
                )
            })
        })
        .collect();
    if keep_newest {
        handles.sort();
        handles.pop();
    }
    let artifact_ids: Vec<String> = handles
        .iter()
        .flat_map(|handle| {
            state
                .proofs
                .completed(handle.get())
                .into_iter()
                .map(|(_, artifact, _)| artifact)
        })
        .collect();
    let mut released = 0;
    for handle in handles {
        let response = app.call_live(&ToolCall {
            name: "graph_release".into(),
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
            app.graph.live_ui_handles.remove(&handle.get());
        }
    }
    for artifact in artifact_ids {
        app.graph.proof_images.remove(&artifact);
    }
    released
}

/// Show the font's live graph, created as the type specimen when there is
/// none. It renders on its own; see [`live_autorun_tick`].
pub(crate) fn select_live_specimen(app: &mut Workspace) {
    #[cfg(unix)]
    match app.ensure_live_specimen() {
        Ok(()) => {
            if !app.graph.live_selected {
                app.graph.live_selected = true;
                app.graph.fit_request = app.graph.fit_request.wrapping_add(1);
            }
        }
        Err(error) => app.note = error,
    }
    #[cfg(not(unix))]
    {
        app.note = "The live specimen is available in the native editor".into();
    }
}

/// How long the font must stay unedited before the live graph renders again.
#[cfg(unix)]
const RENDER_SETTLE: std::time::Duration = std::time::Duration::from_secs(1);

/// Whether the shown live graph is waiting to render the current font.
///
/// Only a graph without Python renders on its own: a Python recipe runs code
/// and needs a glyph scope, so it waits for Run.
#[cfg(unix)]
pub(crate) fn live_autorun_waiting(app: &Workspace) -> bool {
    matches!(app.mode, crate::application::workspace::Mode::Graph)
        && app.graph.live_selected
        && !app.live_graph_need_pump()
        && app
            .graph
            .rendered_revision
            .is_none_or(|revision| revision != app.font.project.document_revision())
        && app.live_graph.as_ref().is_some_and(|state| {
            !state
                .session
                .snapshot()
                .graph
                .nodes
                .iter()
                .any(|node| node.type_name == "live.python")
        })
}

/// Render the live graph when it first shows, and again once edits settle.
#[cfg(unix)]
pub(crate) fn live_autorun_tick(app: &mut Workspace) {
    if !live_autorun_waiting(app) {
        app.graph.render_after = None;
        return;
    }
    let revision = app.font.project.document_revision();
    let due = match (app.graph.rendered_revision, app.graph.render_after) {
        // The first render does not wait.
        (None, _) => true,
        (Some(_), Some((seen, since))) if seen == revision => since.elapsed() >= RENDER_SETTLE,
        (Some(_), _) => {
            app.graph.render_after = Some((revision, std::time::Instant::now()));
            false
        }
    };
    if due {
        app.graph.render_after = None;
        app.graph.rendered_revision = Some(revision);
        release_live_results(app, true);
        // The nodes' title bars show the run; only a failure to start
        // reaches the footer.
        let note = std::mem::take(&mut app.note);
        run_live_comparison(app);
        if app.note == "Rendering the live graph" {
            app.note = note;
        }
    }
}

/// Map canvas selection to its transform, including selecting a proof of that transform.
#[cfg(unix)]
pub(crate) fn live_result_node(graph: &NodeGraph, selected: Option<u32>) -> Option<u32> {
    let selected = selected?;
    let node = graph.node(selected)?;
    if node.type_name == "live.proof" {
        graph
            .link_into(selected, "font")
            .map(runebender::workflows::graph::Link::from)
    } else {
        Some(selected)
    }
}

/// Apply the newest current retained comparison through the shared guarded edit adapter.
pub(crate) fn apply_live_comparison(app: &mut Workspace) {
    #[cfg(unix)]
    {
        let Some(state) = app.live_graph.as_ref() else {
            app.note = "No live comparison is open".into();
            return;
        };
        let Some(handle) = state.handles.iter().next_back().copied() else {
            app.note = "Run the comparison before applying it".into();
            return;
        };
        let snapshot = state.session.snapshot();
        let node = live_result_node(&snapshot.graph, app.graph.selected);
        let identity = snapshot.identity;
        let epoch = identity.document_epoch.clone();
        let revision = app.font.project.document_revision();
        let mut arguments = serde_json::json!({
            "expected_document_epoch": epoch,
            "identity": identity,
            "handle": handle,
            "actor": "native-graph-ui",
            "operation_key": format!("apply-{}-{node:?}-{revision}", handle.get()),
            "authorization": "user-approved",
        });
        if let Some(node) = node {
            arguments["node"] = serde_json::json!(node);
        }
        let response = app.call_live(&ToolCall {
            name: "graph_apply".into(),
            arguments,
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
    pub(crate) fn sync_live_graph_presentation(&mut self) {
        let Some(state) = self.live_graph.as_ref() else {
            self.graph.proof_images.clear();
            return;
        };
        let completed: Vec<_> = state
            .handles
            .iter()
            .flat_map(|handle| state.proofs.completed(handle.get()))
            .collect();
        let retained: std::collections::BTreeSet<_> = completed
            .iter()
            .map(|(_, artifact, _)| artifact.clone())
            .collect();
        self.graph
            .proof_images
            .retain(|artifact, _| retained.contains(artifact));
        let images: Vec<_> = completed
            .into_iter()
            .map(|(_, artifact, proof)| (artifact, proof))
            .filter(|(artifact, _)| !self.graph.proof_images.contains_key(artifact))
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
        self.graph.proof_images.extend(images);
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
        .graph
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
    let has_python = app.live_graph_session().is_some_and(|session| {
        session
            .snapshot()
            .graph
            .nodes
            .iter()
            .any(|node| node.type_name == "live.python")
    });
    if glyphs.is_empty() && has_python {
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
