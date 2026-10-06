// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Neural inspector section: train a font from the open `.nufo`, pick a version, and set
//! the text the proof strip shows.

use crate::application::view::design::{Region, TextSize, column as xcolumn};
use crate::application::view::recipes;
use crate::application::workspace::Workspace;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::label;

pub(crate) fn panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let train = &app.train;
    let running = train.job.is_some();
    let status = (!train.status.is_empty()).then(|| {
        label(train.status.clone())
            .text_size(TextSize::Body.px())
            .color(pal.text_muted)
    });
    let button = if running {
        recipes::action(pal, "Cancel".into(), |app: &mut Workspace| {
            app.cancel_train();
        })
        .boxed()
    } else {
        recipes::action(pal, "Train".into(), |app: &mut Workspace| {
            app.command_train();
        })
        .boxed()
    };
    // A short run on the open sample alone, to check one drawing.
    let sample_button = (!running && app.session.selected_sample().is_some()).then(|| {
        recipes::action(pal, "Train this sample".into(), |app: &mut Workspace| {
            app.command_train_sample();
        })
    });
    // The versions, newest first; the one the model view draws with is marked.
    let chosen = app.model_font();
    let versions = (!train.versions.is_empty()).then(|| {
        xcolumn(
            Region::List,
            train
                .versions
                .iter()
                .rev()
                .map(|version| {
                    let name = version.name.clone();
                    // The labeled match and the epochs it took, such as "0.99 after 800".
                    let score = match (version.score(), version.epochs()) {
                        (Some(score), Some(epochs)) => format!("{score} after {epochs}"),
                        (Some(score), None) => score,
                        _ => String::new(),
                    };
                    let active = version.font.is_some() && version.font == chosen;
                    recipes::list_row(
                        pal,
                        name.clone(),
                        if version.font.is_some() {
                            score
                        } else {
                            "no font".into()
                        },
                        active,
                        move |app: &mut Workspace| {
                            app.model.version = Some(name.clone());
                            app.preview_view = crate::application::pieces::PreviewView::Model;
                        },
                    )
                    .boxed()
                })
                .collect::<Vec<_>>(),
        )
    });
    let model_error = app
        .model
        .error
        .clone()
        .filter(|_| app.preview_view == crate::application::pieces::PreviewView::Model)
        .map(|error| {
            label(error)
                .text_size(TextSize::Body.px())
                .color(pal.text_muted)
        });
    let clear_pulls = (!app.model.offsets.is_empty()).then(|| {
        recipes::action(pal, "Let go".into(), |app: &mut Workspace| {
            app.model.offsets.clear();
        })
    });
    xcolumn(
        Region::Form,
        (
            recipes::field(
                pal,
                "Text",
                app.preview_text.clone(),
                |app: &mut Workspace, value| app.preview_text = value,
            ),
            versions,
            model_error,
            clear_pulls,
            status,
            button,
            sample_button,
        ),
    )
    .boxed()
}
