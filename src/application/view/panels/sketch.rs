// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Brush controls for session-only ink and explicit guarded trace submission.

use crate::application::editor::tools::sketch::BRUSH_WIDTHS;
use crate::application::view::design::{Region, TextSize, column as xcolumn, row as xrow};
use crate::application::view::theme::Palette;
use crate::application::view::{label, recipes};
use crate::application::workspace::Workspace;
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
                recipes::action(
                    pal,
                    "Trace to draft in Nodes".into(),
                    |app: &mut Workspace| app.trace_sketch_to_draft(),
                )
            }),
            retained.map(|trace| {
                label(format!("Trace {} · {}", trace.handle, trace.phase))
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
                label("Draw here before tracing")
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
