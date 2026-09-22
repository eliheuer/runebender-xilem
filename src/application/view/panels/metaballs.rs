// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The live metaball inspector, shown beside the glyph while its tool is active.

use crate::application::view::design::{Region, TextSize, column as xcolumn, row as xrow};
use crate::application::view::label;
use crate::application::view::recipes;
use crate::application::widgets::gesture_slider::gesture_slider;
use crate::application::workspace::Workspace;
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
                    let value = text.parse::<f64>().unwrap_or(value);
                    if field == "Radius" {
                        format!("{value:.0}")
                    } else {
                        format!("{:.0}%", value * 100.0)
                    }
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
                    .disabled(!selected),
                ),
            )
            .boxed()
        })
        .collect::<Vec<_>>();
    xcolumn(
        Region::Form,
        (
            label("Metaballs").color(pal.text),
            xcolumn(Region::Form, fields),
            recipes::action(pal, "Select all centers".into(), |app: &mut Workspace| {
                Arc::make_mut(&mut app.session).select_all_metaballs();
            }),
            recipes::action(pal, "Convert to cubic".into(), |app: &mut Workspace| {
                app.edit_metaballs(|s| s.collapse_metaballs(false));
            }),
            recipes::action(
                pal,
                "Convert to hyperbezier".into(),
                |app: &mut Workspace| {
                    app.edit_metaballs(|s| s.collapse_metaballs_to_hyperbezier());
                },
            ),
            app.session.metaballs.error.clone().map(|error| {
                label(error)
                    .text_size(TextSize::Caption.px())
                    .color(pal.role("danger"))
            }),
        ),
    )
    .boxed()
}
