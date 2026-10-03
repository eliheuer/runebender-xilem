// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The image inspector, shown while the picture behind the glyph is selected.

use crate::application::editor::tools::trace::ImageAdjust;
use crate::application::view::design::{Region, TextSize, column as xcolumn, row as xrow};
use crate::application::view::label;
use crate::application::view::recipes;
use crate::application::widgets::gesture_slider::gesture_slider;
use crate::application::workspace::Workspace;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::FlexExt as _;

/// One display adjustment: its name, range, readout and the field it sets.
fn adjust_slider(
    app: &Workspace,
    name: &'static str,
    (min, max): (f64, f64),
    get: fn(&ImageAdjust) -> f64,
    set: fn(&mut ImageAdjust, f64),
) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let value = get(&app.image_adjust);
    xcolumn(
        Region::List,
        (
            xrow(
                Region::Inline,
                (
                    label(name)
                        .text_size(TextSize::Body.px())
                        .color(pal.text_muted),
                    xilem::view::FlexSpacer::Flex(1.0),
                    label(format!("{value:.2}"))
                        .text_size(TextSize::Body.px())
                        .color(pal.text),
                ),
            ),
            gesture_slider(
                pal,
                min,
                max,
                value.clamp(min, max),
                move |app: &mut Workspace, value, _| set(&mut app.image_adjust, value),
                |_: &mut Workspace, _| {},
            )
            .accessibility_name(name)
            .step(0.01),
        ),
    )
}

pub(crate) fn panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let frame = app.image_frame();
    let locked = app.session.image_locked();
    let shown =
        |typed: &Option<String>, value: f64| typed.clone().unwrap_or_else(|| format!("{value:.0}"));
    let controls = frame.map(|(x, y, height)| {
        xcolumn(
            Region::Form,
            (
                xrow(
                    Region::Form,
                    (
                        recipes::field_column(recipes::field_enter(
                            pal,
                            "X",
                            shown(&app.image_x_buf, x),
                            |app: &mut Workspace, text| app.image_x_buf = Some(text),
                            |app: &mut Workspace, text| {
                                app.image_x_buf = None;
                                app.set_image_frame(text.trim().parse().ok(), None, None);
                            },
                        )),
                        recipes::field_column(recipes::field_enter(
                            pal,
                            "Y",
                            shown(&app.image_y_buf, y),
                            |app: &mut Workspace, text| app.image_y_buf = Some(text),
                            |app: &mut Workspace, text| {
                                app.image_y_buf = None;
                                app.set_image_frame(None, text.trim().parse().ok(), None);
                            },
                        )),
                        recipes::field_column(recipes::field_enter(
                            pal,
                            "Height",
                            shown(&app.image_height_buf, height),
                            |app: &mut Workspace, text| app.image_height_buf = Some(text),
                            |app: &mut Workspace, text| {
                                app.image_height_buf = None;
                                app.set_image_frame(None, None, text.trim().parse().ok());
                            },
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
                        recipes::toggle(pal, "Lock".into(), locked, |app| {
                            app.apply_op(|session| session.toggle_image_lock());
                        })
                        .flex(1.0),
                        (!locked).then(|| {
                            recipes::action(pal, "Remove".into(), |app: &mut Workspace| {
                                app.apply_op(|session| session.remove_image());
                            })
                            .flex(1.0)
                        }),
                    ),
                ),
                xrow(
                    Region::Form,
                    (
                        recipes::field_column(recipes::field(
                            pal,
                            "Threshold",
                            app.trace_threshold_buf.clone(),
                            |app: &mut Workspace, text| app.trace_threshold_buf = text,
                        )),
                        recipes::field_column(recipes::labeled_control(
                            pal,
                            "Profile",
                            recipes::action(
                                pal,
                                app.trace.profile.name().into(),
                                |app: &mut Workspace| app.trace.profile = app.trace.profile.next(),
                            ),
                        )),
                        recipes::field_column(recipes::labeled_control(
                            pal,
                            "Ink",
                            recipes::toggle(pal, "Invert".into(), app.trace.invert, |app| {
                                app.trace.invert = !app.trace.invert;
                            }),
                        )),
                    ),
                ),
                recipes::action(pal, "Trace image".into(), |app: &mut Workspace| {
                    app.command_trace_placed_image();
                }),
                app.session.image_selected.then(|| {
                    xcolumn(
                        Region::Form,
                        (
                            adjust_slider(
                                app,
                                "Brightness",
                                (-1.0, 1.0),
                                |a| a.brightness,
                                |a, v| a.brightness = v,
                            ),
                            adjust_slider(
                                app,
                                "Contrast",
                                (0.0, 2.0),
                                |a| a.contrast,
                                |a, v| a.contrast = v,
                            ),
                            adjust_slider(
                                app,
                                "Saturation",
                                (0.0, 2.0),
                                |a| a.saturation,
                                |a, v| a.saturation = v,
                            ),
                            adjust_slider(
                                app,
                                "Opacity",
                                (0.0, 1.0),
                                |a| a.opacity,
                                |a, v| a.opacity = v,
                            ),
                            recipes::action(pal, "Reset".into(), |app: &mut Workspace| {
                                app.image_adjust = ImageAdjust::default();
                            }),
                        ),
                    )
                }),
                xcolumn(
                    Region::List,
                    [
                        "Drag to move, drag a corner to resize",
                        "Locked: select only",
                        "Threshold: 0 to 255, empty: auto",
                    ]
                    .map(|hint| {
                        label(hint)
                            .text_size(TextSize::Caption.px())
                            .color(pal.text_muted)
                    }),
                ),
            ),
        )
    });
    xcolumn(Region::Form, (label("Image").color(pal.text), controls)).boxed()
}
