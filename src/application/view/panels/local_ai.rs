// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Local AI rail: the bolden model, the tasks font-ml runs, the
//! proposals waiting, and the nodes files beside the font. Built from
//! the panel language in `recipes::rows`; a task font-ml gains appears
//! here with no change to this file.

use std::sync::Arc;

use crate::application::design::recipes::{self, Group, Row, click};
use crate::application::editor::tools::graph::file_label;
use crate::application::workspace::{Mode, Workspace};
use xilem::WidgetView;

pub(crate) fn local_ai_panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let in_editor = matches!(app.mode, Mode::Editor(_));

    // Each section here is one group, so the section title is its caption.
    // Model: which one, what it says about itself, how hard it pushes.
    let names: Vec<String> = app.ai.installed.iter().map(|(n, _)| n.clone()).collect();
    let paths: Vec<_> = app.ai.installed.iter().map(|(_, p)| p.clone()).collect();
    let chosen = app
        .ai
        .dir
        .as_ref()
        .and_then(|d| paths.iter().position(|p| p == d));
    let mut model_rows = vec![Row::Picker {
        choices: names,
        chosen,
        on_pick: Arc::new(move |app: &mut Workspace, i| {
            if let Some(path) = paths.get(i) {
                app.load_model(path);
            }
        }),
    }];
    if app.ai.installed.is_empty() {
        let where_models = Workspace::models_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "~/Runebender/models".into());
        model_rows.push(Row::Meta(format!(
            "Model folders in {where_models} appear here"
        )));
    }
    if let Some(summary) = app.ai.summary.clone() {
        model_rows.push(Row::Meta(summary));
    }
    if app.ai.dir.is_some() {
        model_rows.push(Row::Slider {
            caption: format!("Strength {:.2}\u{00d7}", app.ai.strength),
            min: 0.0,
            max: 3.0,
            value: app.ai.strength,
            on_change: Arc::new(|app: &mut Workspace, v| {
                app.ai.strength = (v * 20.0).round() / 20.0;
            }),
        });
    }
    let model = Group::new("", model_rows);

    // Tasks: one row per task font-ml says it runs. The first task on
    // the open glyph is the section's primary action.
    let mut task_rows = Vec::new();
    let mut primary_taken = false;
    for task in app
        .ai
        .tasks
        .iter()
        .filter(|t| t.implemented && !t.takes_text())
    {
        if task.takes_glyph() {
            let name = task.name.clone();
            let text = format!("{} This Glyph", task.title);
            let on_click = click(move |app| {
                if let Mode::Editor(index) = app.mode {
                    app.run_task(&name, Some(index));
                }
            });
            task_rows.push(if in_editor && !primary_taken {
                primary_taken = true;
                Row::Primary { text, on_click }
            } else {
                Row::Secondary { text, on_click }
            });
        }
        if task.takes_glyphs() {
            let name = task.name.clone();
            task_rows.push(Row::Secondary {
                text: format!("{} Every Glyph", task.title),
                on_click: click(move |app| app.run_task(&name, None)),
            });
        }
    }
    if let Some(error) = app.graph.tasks_error.clone() {
        task_rows.push(Row::Meta(error));
    }
    if let Some(note) = app.ai.busy.clone() {
        task_rows.push(Row::Status {
            text: note,
            fraction: None,
        });
        task_rows.push(Row::Quiet {
            text: "Cancel".into(),
            on_click: click(|app| app.cancel_task()),
        });
    }
    let tasks = Group::new("", task_rows);

    let debt = super::weight_debt::block(app)
        .map(|block| Group::new("", vec![Row::Legacy(Box::new(block))]));

    // Proposals: what each holds and the two answers.
    let mut proposal_rows = Vec::new();
    for p in &app.ai.proposals {
        let compare_task = p.task.clone();
        let install_task = p.task.clone();
        let discard_task = p.task.clone();
        let comparing = app.ai.preview_task.as_deref() == Some(p.task.as_str());
        proposal_rows.push(Row::Prose(format!(
            "{}: {} glyph{}",
            p.task,
            p.glyphs.len(),
            if p.glyphs.len() == 1 { "" } else { "s" }
        )));
        proposal_rows.push(Row::Meta(format!(
            "{} structurally compatible",
            p.compatible.len()
        )));
        proposal_rows.push(Row::Choice {
            options: vec!["Compare".into(), "Hide".into()],
            chosen: usize::from(!comparing),
            on_pick: Arc::new(move |app: &mut Workspace, i| {
                let showing = app.ai.preview_task.as_deref() == Some(compare_task.as_str());
                if (i == 0) != showing {
                    app.toggle_proposal_preview(&compare_task);
                }
            }),
        });
        proposal_rows.push(Row::Secondary {
            text: "Install".into(),
            on_click: click(move |app| app.install_proposal(&install_task, None)),
        });
        proposal_rows.push(Row::Quiet {
            text: "Discard".into(),
            on_click: click(move |app| app.discard_proposal(&discard_task)),
        });
    }
    if !app.ai.installed_order.is_empty() {
        proposal_rows.push(Row::Quiet {
            text: format!("Undo Install ({})", app.ai.installed_order.len()),
            on_click: click(|app| app.undo_install()),
        });
    }
    let proposals = (!proposal_rows.is_empty()).then(|| Group::new("", proposal_rows));

    // Graph files beside the font: the same files the Graph tab draws.
    let files: Vec<String> = app.graph.files.iter().map(|f| file_label(f)).collect();
    let file_paths: Vec<_> = app.graph.files.clone();
    let current = app
        .graph
        .open
        .as_ref()
        .and_then(|g| file_paths.iter().position(|f| *f == g.path));
    let nodes = (!files.is_empty()).then(|| {
        Group::new(
            "",
            vec![Row::Picker {
                choices: files,
                chosen: current,
                on_pick: Arc::new(move |app: &mut Workspace, i| {
                    if let Some(file) = file_paths.get(i) {
                        app.open_graph_file(file);
                        app.enter_graph_mode();
                    }
                }),
            }],
        )
    });

    let mut sections = vec![
        recipes::section_of(app, "Local AI model", "Model", vec![model]),
        recipes::section_of(app, "Local AI tasks", "Tasks", vec![tasks]),
    ];
    if let Some(debt) = debt {
        sections.push(recipes::section_of(
            app,
            "Local AI weight debt",
            "Weight Debt",
            vec![debt],
        ));
    }
    if let Some(proposals) = proposals {
        sections.push(recipes::section_of(
            app,
            "Local AI proposals",
            "Proposals",
            vec![proposals],
        ));
    }
    if let Some(nodes) = nodes {
        sections.push(recipes::section_of(
            app,
            "Local AI nodes",
            "Graph",
            vec![nodes],
        ));
    }
    recipes::panel_stack(sections)
}
