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
        use crate::application::widgets::letter_chips::{Chip, ChipEvent, letter_chips};
        let colors = crate::application::view::canvas::editor::label_colors(pal);
        let letters = sample.letters();
        let active = app
            .session
            .label
            .active
            .min(letters.len().saturating_sub(1));
        let text: Vec<char> = sample.text.chars().collect();
        let chips: Vec<Chip> = letters
            .iter()
            .enumerate()
            .map(|(letter, (index, character))| Chip {
                character: *character,
                color: colors[letter % colors.len()],
                done: sample.regions_of(*index).next().is_some(),
                word_start: (*index as usize)
                    .checked_sub(1)
                    .is_some_and(|before| text[before].is_whitespace()),
            })
            .collect();
        let done = chips.iter().filter(|chip| chip.done).count();
        let cuts = sample.cuts.len();
        let summary = format!(
            "{done} of {} letters painted, {cuts} {}",
            letters.len(),
            if cuts == 1 { "cut" } else { "cuts" }
        );
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
                letter_chips(
                    Arc::new(chips),
                    active,
                    (pal.text, pal.panel, pal.text_muted),
                    |app: &mut Workspace, event| {
                        let session = Arc::make_mut(&mut app.session);
                        match event {
                            ChipEvent::Pick(letter) => {
                                session.label.active = letter;
                                session.label.selected = None;
                            }
                            ChipEvent::Hover(letter) => session.label.hover_letter = letter,
                        }
                    },
                ),
                caption(summary),
                recipes::action(pal, "Delete sample".into(), |app: &mut Workspace| {
                    app.edit_label(|session| session.delete_sample());
                }),
            ),
        )
    });
    let hints: &[&str] = if app.session.label.sample.is_some() {
        &[
            "Click ink: paint. Option-click: unpaint",
            "Drag across a stroke: cut. Drag a loop: lasso",
            "Drag a corner: move it with any that meet it",
            "Option-drag a corner: pull it away on its own",
            "Shift-drag: select corners. Delete: remove",
        ]
    } else {
        &[
            "Loop around writing: new sample",
            "Click a sample to label it",
        ]
    };
    xcolumn(
        Region::Form,
        (
            xcolumn(Region::List, samples),
            sample_controls,
            xcolumn(
                Region::List,
                hints
                    .iter()
                    .map(|hint| caption((*hint).to_string()))
                    .collect::<Vec<_>>(),
            ),
            app.session.label.error.clone().map(|error| {
                label(error)
                    .text_size(TextSize::Caption.px())
                    .color(pal.role("danger"))
            }),
        ),
    )
    .boxed()
}
