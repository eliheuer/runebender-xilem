// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The live metaball inspector, shown beside the glyph while its tool is active.

use crate::application::editor::tools::metaballs::MetaballSelection;
use crate::application::view::design::{Region, TextSize, column as xcolumn, row as xrow};
use crate::application::view::label;
use crate::application::view::recipes::button;
use crate::application::widgets::gesture_slider::gesture_slider;
use crate::application::workspace::Workspace;
use masonry::layout::Length;
use std::sync::Arc;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::FlexSpacer;

pub(crate) fn panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let selected = !app.session.metaballs.selected.is_empty();
    let upm = app.session.metrics.upm;
    let fields = ["X", "Y", "Radius", "Strength", "Threshold"]
        .into_iter()
        .map(|field| {
            let value = app.session.metaball_slider_value(field);
            let (min, max, step) = match field {
                "X" | "Y" => (-2.0 * upm, 2.0 * upm, 1.0),
                "Radius" => (1.0, upm, 1.0),
                "Strength" => (-5.0, 5.0, 0.01),
                _ => (0.01, 2.0, 0.01),
            };
            let readout = if !selected {
                "—".into()
            } else if matches!(field, "X" | "Y") {
                format!("{value:.0}")
            } else {
                let text = app.session.metaball_value(field);
                if text.is_empty() {
                    "Mixed".into()
                } else {
                    text
                }
            };
            xcolumn(
                Region::List,
                (
                    xrow(
                        Region::Inline,
                        (
                            label(field)
                                .text_size(TextSize::Body.px())
                                .color(pal.text_muted),
                            FlexSpacer::Flex(1.0),
                            label(readout)
                                .text_size(TextSize::Body.px())
                                .color(pal.text),
                        ),
                    ),
                    gesture_slider(
                        pal,
                        min,
                        max,
                        value.clamp(min, max),
                        move |app: &mut Workspace, value, drag| {
                            app.slide_metaball_value(field, value, drag);
                        },
                        |app: &mut Workspace, cancelled| app.finish_metaball_slider(cancelled),
                    )
                    .accessibility_name(field)
                    .step(step)
                    .disabled(!selected)
                    .width(Length::px(214.0)),
                ),
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
