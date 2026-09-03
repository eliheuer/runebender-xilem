// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Local AI panel: installed models, the tasks font-ml runs, and
//! the proposals waiting.
//!
//! The same panel as `view/panels/local_ai.rs` in the GPUI build, in
//! the editor's left rail. Rows come from the tool: a task font-ml
//! gains appears here with no change to this file.

use crate::edit::nodes::file_label;
use crate::*;

pub(crate) fn local_ai_panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let in_editor = matches!(app.mode, Mode::Editor(_));
    let muted = |text: String| {
        label(text)
            .text_size(TextSize::Body.px())
            .color(pal.text_muted)
    };
    let plain = |text: String| label(text).text_size(TextSize::Body.px()).color(pal.text);

    // Which model, from the installed list.
    let chosen = app.ai.dir.clone();
    let models: Vec<_> = app
        .ai
        .installed
        .iter()
        .cloned()
        .map(|(name, path)| {
            let current = chosen.as_deref() == Some(path.as_path());
            recipes::toggle(pal, name, current, move |app: &mut Workspace| {
                app.load_model(&path);
            })
        })
        .collect();
    let where_models = Workspace::models_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~/.runebender/models".into());
    let no_models = app.ai.installed.is_empty().then(|| {
        muted(format!(
            "Drop a model folder in {where_models} and it appears here. Nothing is downloaded."
        ))
    });
    let summary = app.ai.summary.clone().map(plain);
    // Strength, because a model can be right about direction and
    // short on distance. The GPUI build's slider row.
    let strength = app.ai.strength;
    let strength_row = app.ai.dir.is_some().then(|| {
        xrow(
            Region::Inline,
            (
                muted(format!("Strength {strength:.2}\u{00d7}")),
                slider(0.0, 3.0, strength, |app: &mut Workspace, v| {
                    app.ai.strength = (v * 20.0).round() / 20.0;
                })
                .width(Length::px(96.0)),
            ),
        )
    });

    // One row per task font-ml says it runs.
    let tasks: Vec<_> = app
        .ai
        .tasks
        .iter()
        .filter(|t| t.implemented)
        .map(|task| {
            let one = task.takes_glyph();
            let all = task.takes_glyphs();
            let name_one = task.name.clone();
            let name_all = task.name.clone();
            xrow(
                Region::Inline,
                (
                    one.then(|| {
                        recipes::toggle(
                            pal,
                            format!("{}: this glyph", task.title),
                            in_editor,
                            move |app: &mut Workspace| {
                                if let Mode::Editor(index) = app.mode {
                                    app.run_task(&name_one, Some(index));
                                }
                            },
                        )
                    }),
                    all.then(|| {
                        recipes::toggle(
                            pal,
                            format!("{}: every glyph", task.title),
                            true,
                            move |app: &mut Workspace| app.run_task(&name_all, None),
                        )
                    }),
                ),
            )
        })
        .collect();
    let no_tool = app.nodes.font_ml.is_none().then(|| {
        muted(
            "font-ml not found. cargo install --git https://github.com/eliheuer/font-ml, \
             or set RUNEBENDER_FONT_ML"
                .into(),
        )
    });

    // What is running, and a way to stop it.
    let busy = app.ai.busy.clone().map(|note| {
        xrow(
            Region::Inline,
            (
                plain(note),
                FlexSpacer::Flex(1.0),
                recipes::toggle(pal, "Cancel".into(), false, |app: &mut Workspace| {
                    app.cancel_task();
                }),
            ),
        )
    });

    // Proposals waiting: what each holds, and the two answers.
    let proposals: Vec<_> = app
        .ai
        .proposals
        .iter()
        .map(|p| {
            let install_task = p.task.clone();
            let discard_task = p.task.clone();
            xcolumn(
                Region::List,
                (
                    plain(format!(
                        "{} proposed: {} glyphs, {} keep structure",
                        p.task,
                        p.glyphs.len(),
                        p.compatible.len()
                    )),
                    xrow(
                        Region::Inline,
                        (
                            recipes::toggle(
                                pal,
                                "Install".into(),
                                true,
                                move |app: &mut Workspace| {
                                    app.install_proposal(&install_task, None);
                                },
                            ),
                            recipes::toggle(
                                pal,
                                "Discard".into(),
                                false,
                                move |app: &mut Workspace| {
                                    app.discard_proposal(&discard_task);
                                },
                            ),
                        ),
                    ),
                ),
            )
        })
        .collect();
    let undo = (!app.ai.installed_order.is_empty()).then(|| {
        recipes::toggle(
            pal,
            format!("Undo install ({})", app.ai.installed_order.len()),
            false,
            |app: &mut Workspace| app.undo_install(),
        )
    });

    // Nodes files beside the font, as rows: the same files the Nodes
    // tab draws.
    let files: Vec<_> = app
        .nodes
        .files
        .iter()
        .cloned()
        .map(|file| {
            let current = app.nodes.graph.as_ref().is_some_and(|g| g.path == file);
            recipes::toggle(
                pal,
                file_label(&file),
                current,
                move |app: &mut Workspace| {
                    app.open_nodes_file(&file);
                    app.enter_nodes_mode();
                },
            )
        })
        .collect();

    xcolumn(
        Region::Panel,
        (
            muted("Model".into()),
            xcolumn(Region::List, models),
            no_models,
            summary,
            strength_row,
            muted("Tasks".into()),
            xcolumn(Region::List, tasks),
            no_tool,
            busy,
            xcolumn(Region::List, proposals),
            undo,
            muted("Nodes".into()),
            xrow(Region::List, files),
        ),
    )
    .background_color(pal.panel)
}
