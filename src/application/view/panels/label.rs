// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The label inspector, shown beside a neural canvas while the label tool is active.

use crate::application::view::design::{Region, TextSize, column as xcolumn};
use crate::application::view::label;
use crate::application::view::recipes;
use crate::application::workspace::Workspace;
use std::sync::Arc;
use xilem::WidgetView;
use xilem::style::Style;

pub(crate) fn panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let item = app.session.neural_item();
    let selected = app.session.selected_sample();
    let caption = |text: String| {
        label(text)
            .text_size(TextSize::Caption.px())
            .color(pal.text_muted)
    };
    let samples = item
        .samples
        .iter()
        .enumerate()
        .map(|(position, sample)| {
            let letters = sample.letters().len();
            let done = letters - sample.unlabeled().len();
            let text = if sample.text.trim().is_empty() {
                format!("{}  (no text)", position + 1)
            } else {
                format!("{}  {}", position + 1, sample.text)
            };
            recipes::list_row(
                pal,
                text,
                format!("{done}/{letters}"),
                selected.as_ref().is_some_and(|(at, _)| *at == position),
                move |app: &mut Workspace| {
                    Arc::make_mut(&mut app.session).select_sample(Some(position));
                },
            )
            .boxed()
        })
        .collect::<Vec<_>>();
    let sample_controls = selected.map(|(position, sample)| {
        let letters = sample.letters();
        let active = app
            .session
            .label
            .active
            .min(letters.len().saturating_sub(1));
        let rows = letters
            .iter()
            .enumerate()
            .map(|(letter, (index, character))| {
                let regions = sample.regions_of(*index).count();
                let trailing = if regions == 0 {
                    "—".into()
                } else {
                    regions.to_string()
                };
                recipes::list_row(
                    pal,
                    character.to_string(),
                    trailing,
                    letter == active,
                    move |app: &mut Workspace| {
                        let session = Arc::make_mut(&mut app.session);
                        session.label.active = letter;
                        session.label.draft.clear();
                    },
                )
                .boxed()
            })
            .collect::<Vec<_>>();
        xcolumn(
            Region::Form,
            (
                recipes::field_enter(
                    pal,
                    "Text",
                    app.label_text(),
                    move |app: &mut Workspace, text| {
                        app.label_buf = Some((app.session.glyph_name.clone(), position, text));
                    },
                    |app: &mut Workspace, text| {
                        app.label_buf = None;
                        app.edit_label(|session| session.set_sample_text(text));
                    },
                ),
                xcolumn(Region::List, rows),
                recipes::action(pal, "Delete sample".into(), |app: &mut Workspace| {
                    app.edit_label(|session| session.delete_sample());
                }),
            ),
        )
    });
    let hints: &[&str] = if app.session.label.sample.is_some() {
        &[
            "Drag: loop a letter's ink",
            "Click: place a corner",
            "Double-click or Enter: close",
            "Option-click: whole contour",
            "Tab: next letter",
            "Delete: remove the last region",
            "Escape: leave the sample",
        ]
    } else {
        &[
            "Loop around writing: new sample",
            "Click a sample above to label it",
        ]
    };
    xcolumn(
        Region::Form,
        (
            label("Samples").color(pal.text),
            xcolumn(Region::List, samples),
            sample_controls,
            xcolumn(
                Region::List,
                hints
                    .iter()
                    .map(|hint| caption((*hint).to_string()))
                    .collect::<Vec<_>>(),
            ),
            recipes::action(pal, "Export phrase files".into(), |app: &mut Workspace| {
                app.command_export_phrases();
            }),
            app.session.label.error.clone().map(|error| {
                label(error)
                    .text_size(TextSize::Caption.px())
                    .color(pal.role("danger"))
            }),
        ),
    )
    .boxed()
}
