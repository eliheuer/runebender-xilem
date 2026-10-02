// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The label inspector, shown beside the glyph while the label tool is active.

use crate::application::view::design::{Region, TextSize, column as xcolumn};
use crate::application::view::label;
use crate::application::view::recipes;
use crate::application::workspace::Workspace;
use xilem::WidgetView;
use xilem::style::Style;

pub(crate) fn panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let item = app.session.neural_item();
    let letters = item.letters();
    let active = app
        .session
        .label
        .active
        .min(letters.len().saturating_sub(1));
    let done = letters.len() - item.unlabeled().len();
    let rows = letters
        .iter()
        .enumerate()
        .map(|(position, (index, letter))| {
            let regions = item.regions_of(*index).count();
            let trailing = if regions == 0 {
                "—".into()
            } else {
                regions.to_string()
            };
            recipes::list_row(
                pal,
                letter.to_string(),
                trailing,
                position == active,
                move |app: &mut Workspace| {
                    let session = std::sync::Arc::make_mut(&mut app.session);
                    session.label.active = position;
                    session.label.draft.clear();
                },
            )
            .boxed()
        })
        .collect::<Vec<_>>();
    xcolumn(
        Region::Form,
        (
            label("Label").color(pal.text),
            recipes::field_enter(
                pal,
                "Text",
                app.label_text(),
                |app: &mut Workspace, text| {
                    app.label_buf = Some((app.session.glyph_name.clone(), text));
                },
                |app: &mut Workspace, text| {
                    app.label_buf = None;
                    app.edit_label(|session| session.set_label_text(text));
                },
            ),
            (!letters.is_empty()).then(|| {
                label(format!("{done} of {} letters have ink", letters.len()))
                    .text_size(TextSize::Caption.px())
                    .color(pal.text_muted)
            }),
            xcolumn(Region::List, rows),
            xcolumn(
                Region::List,
                [
                    "Drag: draw a loop",
                    "Click: place a corner",
                    "Double-click or Enter: close",
                    "Option-click: whole contour",
                    "Tab: next letter",
                    "Delete: remove the last region",
                ]
                .map(|hint| {
                    label(hint)
                        .text_size(TextSize::Caption.px())
                        .color(pal.text_muted)
                }),
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
