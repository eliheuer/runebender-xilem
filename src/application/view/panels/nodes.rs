// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The nodes pane: one row of buttons over the canvas.
//!
//! The files beside the font appear as square controls, followed by New, Open,
//! Save and Run. Node types to add are on the canvas's
//! right-click menu, a layer the canvas widget opens itself. A
//! selected Master, Model or Adapter node offers its choices in a
//! second row.

use crate::application::editor::tools::nodes::file_label;
use crate::application::view::canvas::nodes::{NodesEvent, nodes_canvas};
use crate::application::view::design::{Region, Space, TextSize, column as xcolumn, row as xrow};
use crate::application::view::render::bottom_keyline;
use crate::application::view::{design, label, recipes};
use crate::application::workspace::Workspace;
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use masonry::properties::types::CrossAxisAlignment;
use runebender::ui::nodes::{
    ContentState, ImageContent, ImmutablePng, NodeContent, NodeContentMap, ScriptContent,
};
use std::sync::Arc;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::FlexExt as _;
use xilem::view::{FlexSpacer, flex_col, sized_box};

#[cfg(unix)]
use crate::application::editor::tools::nodes_execution::LiveGraphPhase;
#[cfg(unix)]
use crate::application::platform::nodes_proofs::NodeProofInspection;
#[cfg(unix)]
use runebender::document::agent::ToolCall;
#[cfg(unix)]
use runebender::document::nodes::Registry;
#[cfg(unix)]
use runebender::document::nodes_session::{
    GraphEdit, GraphGuard, GraphInteractiveMutationRequest, GraphMutation, GraphRunHandle,
    GraphRunStatus,
};

struct CanvasProjection {
    graph: Arc<runebender::document::nodes::NodeGraph>,
    registry: Arc<runebender::document::nodes::Registry>,
    rows: Arc<std::collections::BTreeMap<u32, crate::application::editor::tools::nodes::RowState>>,
    content: Arc<NodeContentMap>,
    problems: Vec<String>,
    live: bool,
    running: bool,
    can_apply: bool,
    report: Option<String>,
}

fn fixture_png() -> ImmutablePng {
    use image::{DynamicImage, Rgba, RgbaImage};
    use std::io::Cursor;
    use std::sync::OnceLock;

    static BYTES: OnceLock<Arc<[u8]>> = OnceLock::new();
    let bytes = BYTES.get_or_init(|| {
        let mut image = RgbaImage::new(192, 96);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let dark = ((x / 24) + (y / 24)) % 2 == 0;
            *pixel = if dark {
                Rgba([28, 28, 28, 255])
            } else {
                Rgba([232, 232, 232, 255])
            };
        }
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("in-memory Nodes fixture PNG encodes");
        Arc::from(bytes.into_inner())
    });
    ImmutablePng {
        bytes: bytes.clone(),
        width: 192,
        height: 96,
        output_hash: "ui-fixture-not-a-font-proof".into(),
    }
}

fn legacy_projection(app: &Workspace) -> Option<CanvasProjection> {
    let state = app.nodes.graph.as_ref()?;
    let mut content = NodeContentMap::default();
    let fixture = std::env::var_os("RUNEBENDER_NODES_CONTENT_FIXTURE").is_some();
    for node in &state.graph.nodes {
        match node.type_name.as_str() {
            "live.python" => {
                let text = node
                    .values
                    .get("code")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                content.by_node.insert(
                    node.id,
                    NodeContent::Script(ScriptContent {
                        content_hash: fixture.then_some("ui-fixture").unwrap_or_default().into(),
                        text,
                        state: if fixture {
                            ContentState::Current
                        } else {
                            ContentState::Idle
                        },
                        size: content_size(app, node.id, runebender::ui::nodes::CODE_H),
                    }),
                );
            }
            "live.proof" => {
                content.by_node.insert(
                    node.id,
                    NodeContent::Image(ImageContent {
                        image: fixture.then(fixture_png),
                        previous_image: None,
                        state: if fixture {
                            ContentState::Current
                        } else {
                            ContentState::Idle
                        },
                        size: content_size(app, node.id, runebender::ui::nodes::IMAGE_H),
                    }),
                );
            }
            _ => {}
        }
    }
    Some(CanvasProjection {
        graph: state.graph.clone(),
        registry: state.registry.clone(),
        rows: state.rows.clone(),
        content: Arc::new(content),
        problems: state.problems.iter().map(ToString::to_string).collect(),
        live: false,
        running: app.nodes.job.is_some(),
        can_apply: false,
        report: None,
    })
}

fn content_size(app: &Workspace, node: u32, height: f64) -> [f32; 2] {
    app.nodes.content_sizes.get(&node).copied().unwrap_or([
        (runebender::ui::nodes::LIVE_W - runebender::ui::nodes::PAD * 2.0) as f32,
        height as f32,
    ])
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

#[cfg(unix)]
fn proof_images(
    state: &crate::application::editor::tools::nodes_workspace::LiveNodesState,
    handle: GraphRunHandle,
) -> Option<std::collections::BTreeMap<u32, ImmutablePng>> {
    let inspection = state.session.inspect_run(handle)?;
    let NodeProofInspection::Completed {
        artifact_ids,
        proofs,
    } = state.proofs.inspect(handle.get())?
    else {
        return None;
    };
    let mut images = std::collections::BTreeMap::new();
    for (index, capture) in inspection.identity.capture.proofs.iter().enumerate() {
        let proof = proofs.get(index)?;
        let (width, height) = png_dimensions(&proof.png)?;
        images.insert(
            capture.node,
            ImmutablePng {
                bytes: Arc::from(proof.png.clone()),
                width,
                height,
                output_hash: artifact_ids.get(index)?.clone(),
            },
        );
    }
    Some(images)
}

#[cfg(unix)]
fn live_projection(app: &Workspace) -> Option<CanvasProjection> {
    let state = app.live_nodes.as_ref()?;
    let snapshot = state.session.snapshot();
    let latest = state.handles.iter().next_back().copied();
    let inspection = latest.and_then(|handle| state.session.inspect_run(handle));
    let fresh = inspection.as_ref().is_some_and(|run| {
        run.identity.semantic_revision == snapshot.semantic_revision
            && run.identity.semantic_hash == snapshot.semantic_hash
            && run.identity.capture.font.document_revision == app.font.project.document_revision()
    });
    let visible_state = match inspection.as_ref().map(|run| run.status) {
        None => ContentState::Idle,
        Some(
            GraphRunStatus::Queued
            | GraphRunStatus::Running
            | GraphRunStatus::CancellationRequested,
        ) => ContentState::Running,
        Some(GraphRunStatus::Completed) if fresh => ContentState::Current,
        Some(GraphRunStatus::Completed | GraphRunStatus::Stale | GraphRunStatus::Released) => {
            ContentState::Stale
        }
        Some(GraphRunStatus::Failed | GraphRunStatus::Cancelled) => ContentState::Error(
            inspection
                .as_ref()
                .and_then(|run| run.errors.first())
                .map(|error| error.message.clone())
                .unwrap_or_else(|| "Comparison did not complete".into()),
        ),
    };
    let current_images = latest.and_then(|handle| proof_images(state, handle));
    let previous_images = latest.and_then(|latest| {
        state
            .handles
            .range(..latest)
            .rev()
            .find_map(|handle| proof_images(state, *handle))
    });
    let mut content = NodeContentMap::default();
    for node in &snapshot.graph.nodes {
        match node.type_name.as_str() {
            "live.python" => {
                let content_hash = inspection
                    .as_ref()
                    .and_then(|run| {
                        run.identity
                            .capture
                            .scripts
                            .iter()
                            .find(|capture| capture.node == node.id)
                    })
                    .map(|capture| capture.script_sha256.clone())
                    .unwrap_or_else(|| snapshot.semantic_hash.clone());
                content.by_node.insert(
                    node.id,
                    NodeContent::Script(ScriptContent {
                        text: node
                            .values
                            .get("code")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .into(),
                        content_hash,
                        state: visible_state.clone(),
                        size: content_size(app, node.id, runebender::ui::nodes::CODE_H),
                    }),
                );
            }
            "live.proof" => {
                content.by_node.insert(
                    node.id,
                    NodeContent::Image(ImageContent {
                        image: current_images
                            .as_ref()
                            .and_then(|images| images.get(&node.id))
                            .cloned(),
                        previous_image: previous_images
                            .as_ref()
                            .and_then(|images| images.get(&node.id))
                            .cloned(),
                        state: visible_state.clone(),
                        size: content_size(app, node.id, runebender::ui::nodes::IMAGE_H),
                    }),
                );
            }
            _ => {}
        }
    }
    let summary = latest.and_then(|handle| state.execution.result_summary(handle));
    let running = latest
        .and_then(|handle| state.execution.phase(handle))
        .is_some_and(|phase| {
            matches!(
                phase,
                LiveGraphPhase::ScriptQueued
                    | LiveGraphPhase::ScriptRunning
                    | LiveGraphPhase::RecipeStaged
                    | LiveGraphPhase::ProofsRunning
            )
        });
    Some(CanvasProjection {
        graph: Arc::new(snapshot.graph),
        registry: Arc::new(Registry::core()),
        rows: Arc::default(),
        content: Arc::new(content),
        problems: snapshot
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.clone())
            .collect(),
        live: true,
        running,
        can_apply: fresh && summary.as_ref().is_some_and(|summary| summary.can_apply),
        report: summary.map(|summary| {
            if summary.stderr.is_empty() {
                summary.report
            } else {
                format!("{} · {}", summary.report, summary.stderr)
            }
        }),
    })
}

fn projection(app: &Workspace) -> Option<CanvasProjection> {
    #[cfg(unix)]
    if app.nodes.live_selected {
        return live_projection(app);
    }
    legacy_projection(app)
}

fn select_live_comparison(app: &mut Workspace) {
    #[cfg(unix)]
    match app.ensure_live_graph() {
        Ok(()) => {
            app.nodes.live_selected = true;
            app.nodes.fit_request = app.nodes.fit_request.wrapping_add(1);
            app.note = "Unsaved live comparison opened".into();
        }
        Err(error) => app.note = error,
    }
    #[cfg(not(unix))]
    {
        app.note = "Live comparison execution is available in the native editor".into();
    }
}

#[cfg(unix)]
fn live_interactive_edit(app: &mut Workspace, edit: GraphEdit) {
    let result = app
        .live_graph_session_mut()
        .ok_or_else(|| "Open the live comparison before editing its graph".to_string())
        .and_then(|session| {
            let snapshot = session.snapshot();
            session
                .mutate_interactive(GraphInteractiveMutationRequest {
                    guard: GraphGuard {
                        identity: snapshot.identity,
                        revision: snapshot.revision,
                    },
                    mutation: GraphMutation::Patch { edits: vec![edit] },
                })
                .map_err(|error| error.to_string())
        });
    if let Err(error) = result {
        app.note = error;
    }
}

fn edit_live_code(app: &mut Workspace, node: u32, code: String) {
    #[cfg(unix)]
    live_interactive_edit(
        app,
        GraphEdit::SetValue {
            node,
            field: "code".into(),
            value: serde_json::Value::String(code),
        },
    );
    #[cfg(not(unix))]
    {
        let _ = (node, code);
        app.note = "Live graph editing is available in the native editor".into();
    }
}

fn move_live_node(app: &mut Workspace, node: u32, pos: [f32; 2]) {
    #[cfg(unix)]
    live_interactive_edit(app, GraphEdit::MoveNode { node, pos });
    #[cfg(not(unix))]
    {
        let _ = (node, pos);
        app.note = "Live graph editing is available in the native editor".into();
    }
}

fn run_live_comparison(app: &mut Workspace) {
    #[cfg(unix)]
    {
        let Some(session) = app.live_graph_session() else {
            app.note = "Open the live comparison before running it".into();
            return;
        };
        let snapshot = session.snapshot();
        let Some(source) = app.font.project.source_id(app.font.active()) else {
            app.note = "The active source is unavailable".into();
            return;
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
                "source": source.0,
                "glyphs": [app.session.glyph_name.clone()],
            }),
        });
        app.note = if response["ok"] == serde_json::Value::Bool(true) {
            "Live comparison running".into()
        } else {
            response["error"]
                .as_str()
                .unwrap_or("Live comparison could not start")
                .into()
        };
    }
    #[cfg(not(unix))]
    {
        app.note = "Live comparison execution is available in the native editor".into();
    }
}

fn apply_live_comparison(app: &mut Workspace) {
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

pub(crate) fn nodes_pane(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use xilem::core::one_of::Either;
    let pal = &app.palette;
    let Some(view) = projection(app) else {
        return Either::A(
            flex_col((
                label("No nodes file open")
                    .text_size(TextSize::Body.px())
                    .color(pal.text_muted),
                recipes::action(pal, "New live comparison".into(), select_live_comparison),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Space::Md)
            .padding(Space::Md),
        );
    };
    let open_path = app.nodes.graph.as_ref().map(|state| state.path.clone());
    let files: Vec<_> = app
        .nodes
        .files
        .iter()
        .cloned()
        .map(|file| {
            let current = !view.live && open_path.as_ref() == Some(&file);
            recipes::toggle(
                pal,
                file_label(&file),
                current,
                move |app: &mut Workspace| {
                    app.nodes.live_selected = false;
                    if app.nodes.graph.as_ref().is_none_or(|g| g.path != file) {
                        app.open_nodes_file(&file);
                    }
                },
            )
        })
        .collect();
    let unlisted = open_path.as_ref().and_then(|path| {
        (!view.live && !app.nodes.files.contains(path)).then(|| {
            recipes::toggle(pal, file_label(path), true, |app: &mut Workspace| {
                app.nodes.live_selected = false;
            })
        })
    });
    let live = view.live;
    let running = view.running;
    let can_apply = view.can_apply;
    // The left tab rail and the inspector's first header occupy this same
    // 36-pixel band. One square, keylined control style keeps the file tabs
    // and commands from reading as two unrelated toolbars.
    let strip = bottom_keyline(
        sized_box(
            xrow(
                Region::Inline,
                (
                    xrow(Region::Inline, files),
                    unlisted,
                    recipes::toggle(
                        pal,
                        "Comparison".into(),
                        live,
                        select_live_comparison,
                    ),
                    FlexSpacer::Flex(1.0),
                    recipes::toggle(pal, "New".into(), false, |app: &mut Workspace| {
                        app.nodes.live_selected = false;
                        app.new_nodes_file();
                    }),
                    recipes::toggle(pal, "Open".into(), false, |app: &mut Workspace| {
                        app.nodes.live_selected = false;
                        app.command_open_nodes();
                    }),
                    (!live).then(|| {
                        recipes::toggle(pal, "Save".into(), false, |app: &mut Workspace| {
                            app.save_nodes_file();
                        })
                    }),
                    recipes::toggle(
                        pal,
                        if running {
                            "Running\u{2026}"
                        } else {
                            "Run"
                        }
                        .into(),
                        running,
                        move |app: &mut Workspace| {
                            if running {
                                app.note = "A Nodes comparison is already running".into();
                            } else if live {
                                run_live_comparison(app);
                            } else {
                                app.run_nodes();
                            }
                        },
                    ),
                    live.then(|| {
                        recipes::toggle(
                            pal,
                            if can_apply { "Apply" } else { "Apply unavailable" }.into(),
                            can_apply,
                            move |app: &mut Workspace| {
                                if can_apply {
                                    apply_live_comparison(app);
                                } else {
                                    app.note = "Apply becomes available after a current comparison finishes".into();
                                }
                            },
                        )
                    }),
                ),
            )
            .padding(Space::Sm)
            .gap(Space::Sm)
            .background_color(pal.panel),
        )
        .dims(Dimensions::new(
            Dim::Stretch,
            Dim::Fixed(Length::px(design::RAIL_TAB_HEIGHT)),
        )),
        pal.outline,
    );
    let choices = (!live).then(|| nodes_choices(app)).flatten();
    let report = view.report.map(|report| {
        label(report)
            .text_size(TextSize::Caption.px())
            .color(pal.text)
            .padding(Space::Sm)
    });
    let problems: Vec<_> = view
        .problems
        .iter()
        .map(|p| {
            label(p.clone())
                .text_size(TextSize::Caption.px())
                .color(pal.text)
        })
        .collect();
    let canvas = nodes_canvas(
        view.graph,
        view.registry,
        app.palette.clone(),
        view.rows,
        view.content,
        app.nodes.selected,
        app.nodes.fit_request,
        move |app: &mut Workspace, ev| match ev {
            NodesEvent::Changed(graph) => {
                if live {
                    app.note =
                        "The live comparison topology is fixed; edit code or move its nodes".into();
                } else {
                    app.nodes_changed(graph);
                }
            }
            NodesEvent::Selected(id) => app.nodes.selected = id,
            NodesEvent::EditCode { node, code } => {
                if live {
                    edit_live_code(app, node, code);
                } else {
                    app.nodes_set_value(node, "code", serde_json::Value::String(code));
                }
            }
            NodesEvent::MoveNode { node, pos } => {
                if live {
                    move_live_node(app, node, pos);
                } else {
                    let Some(state) = app.nodes.graph.as_ref() else {
                        return;
                    };
                    let mut graph = (*state.graph).clone();
                    if let Some(node) = graph.node_mut(node) {
                        node.pos = pos;
                        app.nodes_changed(graph);
                    }
                }
            }
            NodesEvent::Resize { node, size } => {
                app.nodes.content_sizes.insert(node, size);
            }
            NodesEvent::Note(note) => app.note = note,
        },
    );
    Either::B(
        flex_col((
            strip,
            choices,
            report,
            xcolumn(Region::List, problems),
            sized_box(canvas)
                .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
                .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Space::None),
    )
}

/// One toggle per choice for the selected Master, Model or Adapter
/// node; nothing for any other node.
fn nodes_choices(app: &Workspace) -> Option<impl WidgetView<Workspace> + use<>> {
    let pal = &app.palette;
    let state = app.nodes.graph.as_ref()?;
    let id = app.nodes.selected?;
    let node = state.graph.node(id)?;
    let current = node
        .values
        .get("name")
        .and_then(|v| v.as_str())
        .map(String::from);
    let options: Vec<String> = match node.type_name.as_str() {
        "core.master" => app.font.master_names().clone(),
        "core.model" => runebender::document::nodes_run::installed(None, false)
            .into_iter()
            .map(|(n, _)| n)
            .collect(),
        "core.adapter" => runebender::document::nodes_run::installed(None, true)
            .into_iter()
            .map(|(n, _)| n)
            .collect(),
        _ => return None,
    };
    let chips: Vec<_> = options
        .into_iter()
        .map(|option| {
            let on = current.as_deref() == Some(option.as_str());
            let value = option.clone();
            recipes::toggle(pal, option, on, move |app: &mut Workspace| {
                app.nodes_set_value(id, "name", serde_json::Value::String(value.clone()));
            })
        })
        .collect();
    Some(xrow(Region::List, chips).background_color(pal.panel))
}
