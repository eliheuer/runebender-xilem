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
    // The list of samples shows only while none is open: an open sample is its text.
    let samples = selected.is_none().then(|| {
        xcolumn(
            Region::List,
            item.samples
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
                        false,
                        move |app: &mut Workspace| {
                            Arc::make_mut(&mut app.session).select_sample(Some(position));
                        },
                    )
                    .boxed()
                })
                .collect::<Vec<_>>(),
        )
    });
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
        let summary = format!("{done} of {} letters have ink", letters.len());
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
                    crate::application::widgets::letter_chips::ChipInks {
                        ink: pal.mark_ink.unwrap_or(pal.text),
                        hollow: pal.text_muted,
                        face: pal.panel,
                        outline: pal.mark_outline.unwrap_or(pal.outline),
                        ring: pal.text,
                    },
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
            ),
        )
    });
    let hints: &[&str] = if app.session.label.sample.is_some() {
        &[
            "Pick a letter. Click its ink, or drag",
            "a loop around it. Drag corners to adjust.",
            "Option-drag pulls a shared corner away.",
        ]
    } else {
        &["Drag a loop around writing to start."]
    };
    xcolumn(
        Region::Form,
        (
            samples,
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
