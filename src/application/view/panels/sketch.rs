// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Brush controls for session-only ink and explicit guarded trace submission.

use crate::application::editor::tools::sketch::BRUSH_WIDTHS;
use crate::application::view::design::{Region, TextSize, column as xcolumn, row as xrow};
use crate::application::view::theme::Palette;
use crate::application::view::{label, recipes};
use crate::application::workspace::Workspace;
use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style;

pub(crate) fn panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let (width, erase, has_ink) = app.sketch.lock().map_or((96, false, false), |sketch| {
        (sketch.brush_units, sketch.erase, sketch.has_ink())
    });
    let retained = app.sketch_trace.as_ref();
    let terminal = retained.is_some_and(|trace| trace.terminal());
    let completed = retained.is_some_and(|trace| trace.phase == "completed");
    xcolumn(
        Region::Form,
        (
            label("Brush sketch").color(pal.text),
            label("Ink only · width in font units")
                .text_size(TextSize::Caption.px())
                .color(pal.text_muted),
            xrow(
                Region::Inline,
                BRUSH_WIDTHS[..4]
                    .iter()
                    .map(|&units| width_button(pal, units, width))
                    .collect::<Vec<_>>(),
            ),
            xrow(
                Region::Inline,
                BRUSH_WIDTHS[4..]
                    .iter()
                    .map(|&units| width_button(pal, units, width))
                    .collect::<Vec<_>>(),
            ),
            xrow(
                Region::Inline,
                (
                    recipes::action(
                        pal,
                        if erase { "Erase ✓" } else { "Erase" }.into(),
                        |app: &mut Workspace| {
                            if let Ok(mut sketch) = app.sketch.lock() {
                                sketch.erase = !sketch.erase;
                            }
                        },
                    ),
                    recipes::action(pal, "Clear ink".into(), |app: &mut Workspace| {
                        if let Ok(mut sketch) = app.sketch.lock()
                            && sketch.clear()
                        {
                            app.note = "Temporary brush ink cleared".into();
                        }
                    }),
                ),
            ),
            recipes::field(
                pal,
                "Approved reference glyph",
                app.reference_buf.clone(),
                |app: &mut Workspace, value| app.reference_buf = value,
            ),
            recipes::field(
                pal,
                "Why this reference fits",
                app.sketch_reference_rationale.clone(),
                |app: &mut Workspace, value| app.sketch_reference_rationale = value,
            ),
            retained.is_none().then(|| {
                xcolumn(
                    Region::Form,
                    (
                        recipes::action(pal, "Trace to draft".into(), |app: &mut Workspace| {
                            app.trace_sketch_to_draft();
                        }),
                        recipes::action(pal, "Draft with Virtua".into(), |app: &mut Workspace| {
                            app.draft_sketch_with_virtua();
                        }),
                    ),
                )
            }),
            retained.map(|trace| {
                label(format!(
                    "{} {} · {}",
                    trace.backend.label(),
                    trace.handle,
                    trace.phase
                ))
                .text_size(TextSize::Caption.px())
                .color(pal.text)
            }),
            retained
                .and_then(|trace| trace.error.as_ref())
                .map(|error| {
                    label(error.clone())
                        .text_size(TextSize::Caption.px())
                        .color(pal.role("danger"))
                }),
            retained.map(|_| {
                xrow(
                    Region::Inline,
                    (
                        recipes::action(pal, "Check status".into(), |app: &mut Workspace| {
                            app.refresh_sketch_trace();
                        }),
                        (!terminal).then(|| {
                            recipes::action(pal, "Cancel".into(), |app: &mut Workspace| {
                                app.cancel_sketch_trace();
                            })
                        }),
                        terminal.then(|| {
                            recipes::action(pal, "Release".into(), |app: &mut Workspace| {
                                app.release_sketch_trace();
                            })
                        }),
                    ),
                )
            }),
            terminal.then(|| {
                recipes::action(pal, "Retry current ink".into(), |app: &mut Workspace| {
                    app.retry_sketch_trace();
                })
            }),
            completed.then(|| {
                recipes::action(pal, "Open comparison".into(), |app: &mut Workspace| {
                    app.open_sketch_comparison();
                })
            }),
            (!has_ink && retained.is_none()).then(|| {
                label("Draw here before tracing or drafting")
                    .text_size(TextSize::Caption.px())
                    .color(pal.text_muted)
            }),
            model_controls(app),
        ),
    )
}

fn model_controls(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let selected = app.sketch_selected_model.clone();
    let model_rows: Vec<_> = app
        .sketch_models
        .iter()
        .map(|model| {
            let id = model.id.clone();
            let ready = model.ready;
            let status = model.status.clone();
            let name = if model.name.chars().count() > 20 {
                format!("{}…", model.name.chars().take(19).collect::<String>())
            } else {
                model.name.clone()
            };
            recipes::toggle(pal, name, id == selected, move |app: &mut Workspace| {
                if ready {
                    app.sketch_selected_model.clone_from(&id);
                } else {
                    app.note = status.clone();
                }
            })
        })
        .collect();
    let selected_status: String = app
        .sketch_models
        .iter()
        .find(|model| model.id == selected)
        .map_or_else(
            || "No model · Refresh or open folder".into(),
            |model| {
                if model.ready {
                    "Ready · Virtua sketch".into()
                } else {
                    "Unavailable · click model for details".into()
                }
            },
        );
    let identity = app.sketch_identity;
    xcolumn(
        Region::Form,
        (
            label("Local Virtua model").color(pal.text),
            xcolumn(Region::List, model_rows),
            label(selected_status)
                .text_size(TextSize::Caption.px())
                .color(pal.text_muted),
            xrow(
                Region::Inline,
                (
                    recipes::action(pal, "Refresh".into(), |app: &mut Workspace| {
                        app.sketch_models = crate::application::local_models::discover();
                        app.note = "Local Virtua models refreshed".into();
                    }),
                    recipes::action(pal, "Open folder".into(), |app: &mut Workspace| {
                        app.note = crate::application::local_models::open_folder()
                            .map(|()| "Opened local models folder".to_owned())
                            .unwrap_or_else(|error| error);
                    }),
                ),
            ),
            xcolumn(
                Region::Form,
                (
                    label(format!("Letter identity {identity:.1}"))
                        .text_size(TextSize::Caption.px())
                        .color(pal.text_muted),
                    recipes::neutral_slider(
                        pal,
                        0.0,
                        1.5,
                        identity,
                        |app: &mut Workspace, value| {
                            app.sketch_identity = (value * 10.0).round() / 10.0;
                        },
                    )
                    .width(Length::px(200.0)),
                ),
            ),
            recipes::field(
                pal,
                "Codepoint (optional U+hex)",
                app.sketch_codepoint_buf.clone(),
                |app: &mut Workspace, value| app.sketch_codepoint_buf = value,
            ),
            label("Blank: use glyph name only")
                .text_size(TextSize::Caption.px())
                .color(pal.text_muted),
            (app.session.glyph_name == "kaf-ar.medi").then(|| {
                label("Kaf: U+0643 if intended")
                    .text_size(TextSize::Caption.px())
                    .color(pal.text_muted)
            }),
        ),
    )
}

fn width_button(pal: &Palette, units: u16, selected: u16) -> impl WidgetView<Workspace> + use<> {
    recipes::action(
        pal,
        if units == selected {
            format!("[{units}]")
        } else {
            format!("{units}")
        },
        move |app: &mut Workspace| {
            if let Ok(mut sketch) = app.sketch.lock() {
                sketch.brush_units = units;
                sketch.erase = false;
            }
        },
    )
}
