// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The live metaball inspector, shown beside the glyph while its tool is active.

use crate::edit::metaballs::MetaballSelection;
use crate::*;

pub(crate) fn panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let fields = ["X", "Y", "Radius", "Strength", "Threshold"]
        .into_iter()
        .map(|field| {
            recipes::field_enter(
                pal,
                field,
                app.session.metaball_value(field),
                move |app: &mut Workspace, v| {
                    Arc::make_mut(&mut app.session)
                        .metaballs
                        .drafts
                        .insert(field, v);
                },
                move |app: &mut Workspace, v| {
                    app.edit_metaballs(|s| s.set_metaball_value(field, v));
                },
            )
            .boxed()
        })
        .collect::<Vec<_>>();
    xcolumn(
        Region::Form,
        (
            label("Metaballs").color(pal.text),
            label(format!(
                "{} centers selected",
                app.session.metaballs.selected.len()
            ))
            .text_size(TextSize::Caption.px())
            .color(pal.text_muted),
            label("Click to add · Shift-click to select")
                .text_size(TextSize::Caption.px())
                .color(pal.text_muted),
            xcolumn(Region::Form, fields),
            button(label("Select all centers"), |app: &mut Workspace| {
                Arc::make_mut(&mut app.session).select_all_metaballs();
            }),
            button(label("Start a new group"), |app: &mut Workspace| {
                Arc::make_mut(&mut app.session).metaballs = MetaballSelection::default();
            }),
            button(label("Groups to cubic"), |app: &mut Workspace| {
                app.edit_metaballs(|s| s.collapse_metaballs(true));
            }),
            button(label("Glyph to cubic"), |app: &mut Workspace| {
                app.edit_metaballs(|s| s.collapse_metaballs(false));
            }),
            app.session.metaballs.error.clone().map(|error| {
                label(error)
                    .text_size(TextSize::Caption.px())
                    .color(pal.role("danger"))
            }),
        ),
    )
    .boxed()
}
