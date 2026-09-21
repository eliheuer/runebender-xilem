// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The live metaball inspector, shown beside the glyph while its tool is active.

use crate::application::editor::tools::metaballs::MetaballSelection;
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
    let selected =
        !app.session.metaballs.selected.is_empty() || app.session.metaballs.selected_link.is_some();
    let field_names: &[&str] = if app.session.metaballs.selected_link.is_some() {
        &["Width"]
    } else if app.session.metaball_uses_size_controls() {
        &["X", "Y", "Size", "Blend reach"]
    } else {
        &["X", "Y", "Radius", "Strength", "Threshold"]
    };
    let fields = field_names
        .iter()
        .copied()
        .map(|field| {
            let value = app.session.metaball_slider_value(field);
            let (min, max, step) = app.session.metaball_slider_range(field);
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
                    if matches!(field, "Radius" | "Size" | "Blend reach" | "Width") {
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
            recipes::action_enabled(
                pal,
                "Connect selected".into(),
                app.session.metaball_connect_pair().is_some(),
                |app: &mut Workspace| {
                    app.edit_metaballs(|s| s.connect_metaballs());
                },
            ),
            recipes::action_enabled(
                pal,
                if app.session.metaball_selection_has_negative() {
                    "Add ink"
                } else {
                    "Subtract ink"
                }
                .into(),
                !app.session.metaballs.selected.is_empty(),
                |app: &mut Workspace| {
                    let positive = app.session.metaball_selection_has_negative();
                    app.edit_metaballs(|s| s.set_metaball_sign(positive));
                },
            ),
            recipes::action(pal, "Select all centers".into(), |app: &mut Workspace| {
                Arc::make_mut(&mut app.session).select_all_metaballs();
            }),
            recipes::action(pal, "Start a new group".into(), |app: &mut Workspace| {
                Arc::make_mut(&mut app.session).metaballs = MetaballSelection::default();
            }),
            recipes::action(pal, "Groups to cubic".into(), |app: &mut Workspace| {
                app.edit_metaballs(|s| s.collapse_metaballs(true));
            }),
            recipes::action(pal, "Glyph to cubic".into(), |app: &mut Workspace| {
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

#[cfg(test)]
#[path = "metaballs_tests.rs"]
mod tests;
