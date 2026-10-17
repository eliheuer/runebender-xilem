// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Local AI Shapes section of the inspector, shown while the tool
//! is active. The reference section for the panel language in
//! `recipes::rows`: a model group, a words group, a training group,
//! and the facts behind Details.

use crate::application::editor::tools::ai_shapes::{TrainSource, fit_path, sample_shapes_ufo};
use crate::application::view::recipes::{self, Group, Row, Tile, click};
use crate::application::workspace::Workspace;
use xilem::WidgetView;

/// The time a quick or thorough run takes from scratch, in minutes.
const QUICK_MINUTES: u32 = 12;
const THOROUGH_MINUTES: u32 = 30;

pub(crate) fn panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let st = &app.shapes;
    let mut groups = Vec::new();

    // Which model. The font's own first, as scanned.
    let names: Vec<String> = st.installed.iter().map(|m| m.name.clone()).collect();
    let chosen = st
        .model
        .as_ref()
        .and_then(|p| st.installed.iter().position(|m| &m.path == p));
    let paths: Vec<_> = st.installed.iter().map(|m| m.path.clone()).collect();
    let mut model_rows = vec![Row::Picker {
        choices: names,
        chosen,
        on_pick: std::sync::Arc::new(move |app: &mut Workspace, i| {
            if let Some(path) = paths.get(i) {
                app.choose_shapes_model(path);
            }
        }),
    }];
    model_rows.push(Row::Meta(match st.chosen() {
        Some(m) => format!(
            "{} word{}, {} spare",
            m.words.len(),
            if m.words.len() == 1 { "" } else { "s" },
            m.spare
        ),
        None => "No shapes model yet".to_owned(),
    }));
    if !st.note.is_empty() {
        model_rows.push(Row::Prose(st.note.clone()));
    }
    groups.push(Group::new("Model", model_rows));

    if !st.words.is_empty() {
        groups.push(Group::new("Words", vec![Row::Chips(st.words.clone())]));
    }

    // Training: what runs now, or how to start one.
    match &st.training {
        Some(t) => {
            let (done, total, _) = t.progress.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let elapsed = t.started.elapsed().as_secs_f64();
            let left = if done > 0 {
                (elapsed / done as f64 * (total.saturating_sub(done)) as f64 / 60.0).ceil()
            } else {
                0.0
            };
            let text = if done == 0 {
                format!("Training {}, starting", t.name)
            } else if left > 0.0 {
                format!("Training {}, about {left:.0} min left", t.name)
            } else {
                format!("Training {}", t.name)
            };
            let fraction = (total > 0).then(|| done as f64 / total as f64);
            let tiles: Vec<Tile> = t
                .samples
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .map(|(word, path)| Tile {
                    caption: word.clone(),
                    path: fit_path(path, recipes::rows::TILE_PX, 6.0),
                })
                .collect();
            let mut rows = vec![Row::Status { text, fraction }];
            if !tiles.is_empty() {
                rows.push(Row::Tiles(tiles));
            }
            rows.push(Row::Quiet {
                text: "Stop".into(),
                on_click: click(|app| app.shapes_cancel_training()),
            });
            groups.push(Group::new("Training", rows));
        }
        None => {
            let thorough = st.thorough;
            let continuing = st.chosen().is_some();
            let mut rows = vec![Row::Choice {
                options: vec![
                    format!("Quick, {QUICK_MINUTES} min"),
                    format!("Thorough, {THOROUGH_MINUTES} min"),
                ],
                chosen: usize::from(thorough),
                on_pick: std::sync::Arc::new(|app: &mut Workspace, i| {
                    app.shapes.thorough = i == 1;
                }),
            }];
            rows.push(Row::Primary {
                text: if continuing {
                    "Add This Font's Shapes".into()
                } else {
                    "Train From This Font".into()
                },
                on_click: click(|app| app.shapes_train(TrainSource::ThisFont)),
            });
            if sample_shapes_ufo().is_some() {
                rows.push(Row::Secondary {
                    text: "Train From the Sample Shapes".into(),
                    on_click: click(|app| app.shapes_train(TrainSource::SampleFont)),
                });
            }
            groups.push(Group::new("Train", rows));
        }
    }

    if let Some(m) = st.chosen() {
        groups.push(Group::new(
            "",
            vec![Row::Details {
                key: "Local AI Shapes details",
                rows: vec![Row::KeyValue(vec![
                    ("Words".into(), m.words.len().to_string()),
                    ("Spare slots".into(), m.spare.to_string()),
                    ("Folder".into(), m.path.display().to_string()),
                ])],
            }],
        ));
    }

    recipes::section(
        app,
        "Local AI Shapes",
        "Local AI Shapes",
        recipes::body_of(app, groups),
    )
}
