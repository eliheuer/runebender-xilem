// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The image inspector of a neural item: place a picture of calligraphy, size it, trace it.

use crate::application::view::design::{Region, TextSize, column as xcolumn, row as xrow};
use crate::application::view::label;
use crate::application::view::recipes;
use crate::application::workspace::Workspace;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::FlexExt as _;

pub(crate) fn panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let frame = app.image_frame();
    let shown = |typed: &Option<String>, value: Option<f64>| {
        typed
            .clone()
            .unwrap_or_else(|| value.map(|value| format!("{value:.0}")).unwrap_or_default())
    };
    let controls = frame.map(|(height, y)| {
        xcolumn(
            Region::Form,
            (
                xrow(
                    Region::Form,
                    (
                        recipes::field_column(recipes::field_enter(
                            pal,
                            "Height",
                            shown(&app.image_height_buf, Some(height)),
                            |app: &mut Workspace, text| app.image_height_buf = Some(text),
                            |app: &mut Workspace, text| {
                                app.image_height_buf = None;
                                app.set_image_frame(text.trim().parse().ok(), None);
                            },
                        )),
                        recipes::field_column(recipes::field_enter(
                            pal,
                            "Bottom",
                            shown(&app.image_y_buf, Some(y)),
                            |app: &mut Workspace, text| app.image_y_buf = Some(text),
                            |app: &mut Workspace, text| {
                                app.image_y_buf = None;
                                app.set_image_frame(None, text.trim().parse().ok());
                            },
                        )),
                        recipes::field_column(recipes::field(
                            pal,
                            "Threshold",
                            app.trace_threshold_buf.clone(),
                            |app: &mut Workspace, text| app.trace_threshold_buf = text,
                        )),
                    ),
                ),
                xrow(
                    Region::Inline,
                    (
                        recipes::toggle(pal, "Show".into(), app.show_background, |app| {
                            app.show_background = !app.show_background;
                        })
                        .flex(1.0),
                        recipes::toggle(pal, "Invert".into(), app.trace.invert, |app| {
                            app.trace.invert = !app.trace.invert;
                        })
                        .flex(1.0),
                        recipes::action(
                            pal,
                            app.trace.profile.name().into(),
                            |app: &mut Workspace| app.trace.profile = app.trace.profile.next(),
                        )
                        .flex(1.0),
                    ),
                ),
                recipes::action(pal, "Trace image".into(), |app: &mut Workspace| {
                    app.command_trace_placed_image();
                }),
                label("Height and Bottom are in font units. Threshold is 0 to 255; empty is automatic.")
                    .text_size(TextSize::Caption.px())
                    .color(pal.text_muted),
            ),
        )
    });
    xcolumn(
        Region::Form,
        (
            label("Image").color(pal.text),
            recipes::action(
                pal,
                if frame.is_some() {
                    "Replace image…".into()
                } else {
                    "Place image…".into()
                },
                |app: &mut Workspace| app.command_place_image(),
            ),
            controls,
        ),
    )
    .boxed()
}
