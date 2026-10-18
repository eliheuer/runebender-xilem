// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The nodes pane: one row of buttons over the canvas.
//!
//! The files beside the font appear as square controls, followed by New, Open,
//! Save and Run. Node types to add are on the canvas's
//! right-click menu, a layer the canvas widget opens itself. A
//! selected Master, Model or Adapter node offers its choices in a
//! second row.

use crate::application::design;
use crate::application::design::recipes;
use crate::application::design::{Region, Space, TextSize, row as xrow};
use crate::application::editor::tools::graph::controls::{
    apply_live_comparison, cancel_live_comparison, change_live_graph, clear_live_results,
    edit_live_code, edit_live_scope, move_live_node, open_live_comparison_file,
    run_live_comparison, save_live_comparison_file, select_live_comparison,
};
use crate::application::editor::tools::graph::file_label;
use crate::application::view::canvas::graph::{GraphEvent, graph_canvas};
use crate::application::view::render::{bottom_keyline, px32};
use crate::application::view::{label, text_input};
use crate::application::widgets::input_typography;
use crate::application::workspace::Workspace;
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use masonry::properties::LineBreaking;
use masonry::properties::types::CrossAxisAlignment;
use runebender::font::variable::SourceId;
use runebender::ui::graph::{
    ContentState, ImageContent, ImmutablePng, NodeContent, NodeContentMap, ScriptContent,
};
use runebender::workflows::graph::{NodeGraph, Registry};
use runebender::workflows::graph_session::GraphGuard;
use std::sync::Arc;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::FlexExt as _;
use xilem::view::{FlexSpacer, flex_col, portal, sized_box};

#[cfg(unix)]
use crate::application::editor::tools::graph::execution::LiveGraphPhase;
#[cfg(unix)]
use runebender::workflows::graph_session::{GraphRunHandle, GraphRunStatus};

struct CanvasProjection {
    graph: Arc<NodeGraph>,

    registry: Arc<Registry>,
    rows: Arc<std::collections::BTreeMap<u32, crate::application::editor::tools::graph::RowState>>,
    content: Arc<NodeContentMap>,
    problems: Vec<String>,
    live: bool,
    running: bool,
    can_cancel: bool,
    can_clear: bool,
    can_apply: bool,
    report: Option<String>,
    live_guard: Option<GraphGuard>,
    source: Option<usize>,
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
            .expect("in-memory Graph fixture PNG encodes");
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
    let state = app.graph.open.as_ref()?;
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
                        content_hash: if fixture { "ui-fixture" } else { "" }.into(),
                        text,
                        state: if fixture {
                            ContentState::Current
                        } else {
                            ContentState::Idle
                        },
                        size: content_size(app, node.id, runebender::ui::graph::CODE_H),
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
                        size: content_size(app, node.id, runebender::ui::graph::IMAGE_H),
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
        running: app.graph.job.is_some(),
        can_cancel: false,
        can_clear: false,
        can_apply: false,
        report: None,
        live_guard: None,
        source: None,
    })
}

fn content_size(app: &Workspace, node: u32, height: f64) -> [f32; 2] {
    app.graph.content_sizes.get(&node).copied().unwrap_or([
        px32(runebender::ui::graph::LIVE_W - runebender::ui::graph::PAD * 2.0),
        px32(height),
    ])
}

#[cfg(unix)]
fn proof_images(
    state: &crate::application::editor::tools::graph::workspace::LiveGraphState,
    handle: GraphRunHandle,
    cache: &std::collections::BTreeMap<String, ImmutablePng>,
) -> Option<std::collections::BTreeMap<u32, ImmutablePng>> {
    let inspection = state.session.inspect_run(handle)?;
    let mut images = std::collections::BTreeMap::new();
    for (node, artifact, _) in state.proofs.completed(handle.get()) {
        if inspection.outputs.iter().any(|output| {
            output.node == node
                && matches!(
                    output.value,
                    runebender::workflows::graph_session::GraphNodeOutputValue::Proof { .. }
                )
        }) && let Some(image) = cache.get(&artifact)
        {
            images.insert(node, image.clone());
        }
    }
    (!images.is_empty()).then_some(images)
}

#[cfg(unix)]
fn live_projection(app: &Workspace) -> Option<CanvasProjection> {
    let state = app.live_graph.as_ref()?;
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
        Some(GraphRunStatus::Completed | GraphRunStatus::PartiallyFailed) if fresh => {
            ContentState::Current
        }
        Some(
            GraphRunStatus::Completed
            | GraphRunStatus::PartiallyFailed
            | GraphRunStatus::Stale
            | GraphRunStatus::Released,
        ) => ContentState::Stale,
        Some(GraphRunStatus::Failed | GraphRunStatus::Cancelled) => ContentState::Error(
            inspection
                .as_ref()
                .and_then(|run| run.errors.first())
                .map(|error| error.message.clone())
                .unwrap_or_else(|| "Comparison did not complete".into()),
        ),
    };
    let current_images =
        latest.and_then(|handle| proof_images(state, handle, &app.graph.proof_images));
    let previous_images = latest.and_then(|latest| {
        state
            .handles
            .range(..latest)
            .rev()
            .find_map(|handle| proof_images(state, *handle, &app.graph.proof_images))
    });
    let mut content = NodeContentMap::default();
    for node in &snapshot.graph.nodes {
        let node_state = inspection
            .as_ref()
            .filter(|run| run.status == GraphRunStatus::PartiallyFailed && fresh)
            .and_then(|run| run.errors.iter().find(|error| error.node == Some(node.id)))
            .map_or_else(
                || visible_state.clone(),
                |error| ContentState::Error(error.message.clone()),
            );
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
                        state: node_state.clone(),
                        size: content_size(app, node.id, runebender::ui::graph::CODE_H),
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
                        state: node_state.clone(),
                        size: content_size(app, node.id, runebender::ui::graph::IMAGE_H),
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
                LiveGraphPhase::Ready
                    | LiveGraphPhase::ScriptQueued
                    | LiveGraphPhase::ScriptRunning
                    | LiveGraphPhase::ProofReady
                    | LiveGraphPhase::ProofsRunning
            )
        });
    let can_cancel = state.handles.iter().any(|handle| {
        app.graph.live_ui_handles.contains(&handle.get())
            && state.session.inspect_run(*handle).is_some_and(|run| {
                matches!(run.status, GraphRunStatus::Queued | GraphRunStatus::Running)
            })
    });
    let can_clear = state.handles.iter().any(|handle| {
        app.graph.live_ui_handles.contains(&handle.get())
            && state.session.inspect_run(*handle).is_some_and(|run| {
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
    });
    let live_guard = GraphGuard {
        identity: snapshot.identity.clone(),
        revision: snapshot.revision,
    };
    let source = snapshot
        .graph
        .nodes
        .iter()
        .find(|node| node.type_name == "live.font")
        .and_then(|node| node.values.get("source"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|source| usize::try_from(source).ok());
    let selected_result = crate::application::editor::tools::graph::controls::live_result_node(
        &snapshot.graph,
        app.graph.selected,
    );
    let can_apply =
        fresh && latest.is_some_and(|handle| state.execution.can_apply(handle, selected_result));
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
        can_cancel,
        can_clear,
        can_apply,
        report: summary.map(|summary| {
            if summary.stderr.is_empty() {
                summary.report
            } else {
                format!("{} · {}", summary.report, summary.stderr)
            }
        }),
        live_guard: Some(live_guard),
        source,
    })
}

fn projection(app: &Workspace) -> Option<CanvasProjection> {
    #[cfg(unix)]
    if app.graph.live_selected {
        return live_projection(app);
    }
    legacy_projection(app)
}

fn bounded_message(
    pal: &design::theme::Palette,
    text: String,
) -> impl WidgetView<Workspace> + use<> {
    sized_box(portal(
        label(text)
            .text_size(TextSize::Caption.px())
            .color(pal.text)
            .prop(LineBreaking::WordWrap)
            .padding(Space::Sm),
    ))
    .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(64.0))))
}

pub(crate) fn graph_pane(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
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
    let open_path = app.graph.open.as_ref().map(|state| state.path.clone());
    let files: Vec<_> = app
        .graph
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
                    app.graph.live_selected = false;
                    if app.graph.open.as_ref().is_none_or(|g| g.path != file) {
                        app.open_graph_file(&file);
                    }
                },
            )
        })
        .collect();
    let unlisted = open_path.as_ref().and_then(|path| {
        (!view.live && !app.graph.files.contains(path)).then(|| {
            recipes::toggle(pal, file_label(path), true, |app: &mut Workspace| {
                app.graph.live_selected = false;
            })
        })
    });
    let live = view.live;
    let running = view.running;
    let can_cancel = view.can_cancel;
    let can_clear = view.can_clear;
    let can_apply = view.can_apply;
    let live_guard = view.live_guard.clone();
    let source = view.source;
    let source_name = source.and_then(|source| {
        app.font
            .project
            .document_source(SourceId(source))
            .map(|source| source.name().to_owned())
    });
    let original_graph = view.graph.clone();
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
                    (!live).then(|| {
                        recipes::toggle(pal, "New".into(), false, |app: &mut Workspace| {
                            app.graph.live_selected = false;
                            app.new_graph_file();
                        })
                    }),
                    (!live).then(|| {
                        recipes::toggle(pal, "Open".into(), false, |app: &mut Workspace| {
                            app.graph.live_selected = false;
                            app.command_open_graph();
                        })
                    }),
                    (!live).then(|| {
                        recipes::toggle(pal, "Save".into(), false, |app: &mut Workspace| {
                            app.save_graph_file();
                        })
                    }),
                    live.then(|| {
                        recipes::toggle(
                            pal,
                            "Save comparison as\u{2026}".into(),
                            false,
                            save_live_comparison_file,
                        )
                    }),
                    (live && source.is_some()).then(|| {
                        let source = source.expect("checked above");
                        let label = source_name
                            .clone()
                            .map_or_else(|| format!("Open for source {source}"), |name| {
                                format!("Open for {name}")
                            });
                        recipes::toggle(pal, label, false, move |app: &mut Workspace| {
                            open_live_comparison_file(app, SourceId(source));
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
                                app.note = "A Graph comparison is already running".into();
                            } else if live {
                                run_live_comparison(app);
                            } else {
                                app.run_graph();
                            }
                        },
                    ),
                    (live && can_cancel).then(|| {
                        recipes::toggle(
                            pal,
                            "Cancel".into(),
                            true,
                            cancel_live_comparison,
                        )
                    }),
                    live.then(|| {
                        recipes::toggle(
                            pal,
                            if can_clear { "Clear results" } else { "Nothing to clear" }.into(),
                            can_clear,
                            move |app: &mut Workspace| {
                                if can_clear {
                                    clear_live_results(app);
                                } else {
                                    app.note = "No terminal native Graph results to clear".into();
                                }
                            },
                        )
                    }),
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
    let choices = (!live).then(|| graph_choices(app)).flatten();
    let scope = live.then(|| {
        xrow(
            Region::List,
            (
                label(source.map_or_else(
                    || "Source unavailable".into(),
                    |source| {
                        source_name
                            .clone()
                            .unwrap_or_else(|| format!("Source {source}"))
                    },
                ))
                .text_size(TextSize::Caption.px())
                .color(pal.text_muted),
                label("Glyph scope")
                    .text_size(TextSize::Caption.px())
                    .color(pal.text_muted),
                sized_box(input_typography::input_typography(
                    text_input(
                        app.graph.live_scope.clone(),
                        |app: &mut Workspace, scope| {
                            edit_live_scope(app, scope);
                        },
                    )
                    .text_color(pal.text)
                    .placeholder_color(pal.text_muted),
                    pal.text,
                ))
                .dims(Dimensions::new(Dim::Stretch, Dim::Auto))
                .flex(1.0),
            ),
        )
        .padding(Space::Sm)
        .gap(Space::Sm)
        .background_color(pal.panel)
    });
    let report = view.report.map(|report| bounded_message(pal, report));
    let problems =
        (!view.problems.is_empty()).then(|| bounded_message(pal, view.problems.join("\n")));
    let canvas = graph_canvas(
        view.graph,
        view.registry,
        app.palette.clone(),
        view.rows,
        view.content,
        app.graph.selected,
        app.graph.fit_request,
        app.graph.zoom.unwrap_or(1.0),
        move |app: &mut Workspace, ev| match ev {
            GraphEvent::Changed(graph) => {
                if live {
                    if let Some(guard) = live_guard.clone() {
                        change_live_graph(app, guard, (*original_graph).clone(), graph);
                    } else {
                        app.note = "The live comparison snapshot is unavailable".into();
                    }
                } else {
                    app.graph_changed(graph);
                }
            }
            GraphEvent::Selected(id) => app.graph.selected = id,
            GraphEvent::EditCode { node, code } => {
                if live {
                    if let Some(guard) = live_guard.clone() {
                        edit_live_code(app, guard, node, code);
                    } else {
                        app.note = "The live comparison snapshot is unavailable".into();
                    }
                } else {
                    app.graph_set_value(node, "code", serde_json::Value::String(code));
                }
            }
            GraphEvent::MoveNode { node, pos } => {
                if live {
                    if let Some(guard) = live_guard.clone() {
                        move_live_node(app, guard, node, pos);
                    } else {
                        app.note = "The live comparison snapshot is unavailable".into();
                    }
                } else {
                    let Some(state) = app.graph.open.as_ref() else {
                        return;
                    };
                    let mut graph = (*state.graph).clone();
                    if let Some(node) = graph.node_mut(node) {
                        node.pos = pos;
                        app.graph_changed(graph);
                    }
                }
            }
            GraphEvent::Resize { node, size } => {
                app.graph.content_sizes.insert(node, size);
            }
            GraphEvent::Note(note) => app.note = note,
            GraphEvent::Zoom(zoom) => app.graph.zoom = Some(zoom),
        },
    );
    Either::B(
        flex_col((
            strip,
            scope,
            choices,
            report,
            problems,
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
fn graph_choices(app: &Workspace) -> Option<impl WidgetView<Workspace> + use<>> {
    let pal = &app.palette;
    let state = app.graph.open.as_ref()?;
    let id = app.graph.selected?;
    let node = state.graph.node(id)?;
    let current = node
        .values
        .get("name")
        .and_then(|v| v.as_str())
        .map(String::from);
    let options: Vec<String> = match node.type_name.as_str() {
        "core.master" => app.font.master_names().clone(),
        "core.model" => runebender::workflows::graph_run::installed(None, false)
            .into_iter()
            .map(|(n, _)| n)
            .collect(),
        "core.adapter" => runebender::workflows::graph_run::installed(None, true)
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
                app.graph_set_value(id, "name", serde_json::Value::String(value.clone()));
            })
        })
        .collect();
    Some(xrow(Region::List, chips).background_color(pal.panel))
}
