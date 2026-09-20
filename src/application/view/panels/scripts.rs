// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The non-executing Scripts rail: draft editing and runtime availability.

#[cfg(not(target_arch = "wasm32"))]
use crate::application::view::design::ControlSize;
use crate::application::view::design::{Region, Space, Stroke, TextSize, column as xcolumn};
use crate::application::view::recipes;
use crate::application::view::{label, text_input};
use crate::application::widgets::scroll_viewport::portal;
use crate::application::widgets::selectable_text::selectable_text;
use crate::application::widgets::source_text_area::source_text_area;
use crate::application::workspace::Workspace;
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::sized_box;

/// Show the current script draft without providing a second persistence or
/// execution path before the native runtime is registered.
pub(crate) fn scripts_panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let draft = app.scripts.draft.clone();
    let editor = draft.map(|draft| {
        let dirty = if draft.dirty {
            "Unsaved changes"
        } else {
            "Not saved"
        };
        xcolumn(
            Region::List,
            (
                label(dirty)
                    .text_size(TextSize::Caption.px())
                    .color(if draft.dirty {
                        pal.role("warning")
                    } else {
                        pal.text_muted
                    }),
                text_input(draft.name, |app: &mut Workspace, value| {
                    app.script_name_changed(value);
                })
                .placeholder("Script name.py")
                .text_color(pal.text)
                .placeholder_color(pal.text_muted)
                .background_color(pal.field())
                .border_color(pal.field_outline)
                .border_width(Stroke::Hairline.length())
                .corner_radius(crate::application::view::design::Radius::None.length()),
                sized_box(
                    portal(
                        source_text_area(draft.content, |app: &mut Workspace, value| {
                            app.script_content_changed(value);
                        })
                        .text_color(pal.text),
                    )
                    .must_fill(true),
                )
                .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(280.0))))
                .padding(Space::Sm)
                .background_color(pal.field())
                .border_color(pal.field_outline)
                .border_width(Stroke::Hairline.length())
                .corner_radius(crate::application::view::design::Radius::None.length()),
                label("Parameters · JSON object")
                    .text_size(TextSize::Caption.px())
                    .color(pal.text_muted),
                sized_box(
                    portal(
                        source_text_area(
                            app.scripts.parameters.clone(),
                            |app: &mut Workspace, value| {
                                app.script_parameters_changed(value);
                            },
                        )
                        .text_color(pal.text),
                    )
                    .must_fill(true),
                )
                .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(88.0))))
                .padding(Space::Sm)
                .background_color(pal.field())
                .border_color(pal.field_outline)
                .border_width(Stroke::Hairline.length())
                .corner_radius(crate::application::view::design::Radius::None.length()),
                selectable_text::<Workspace, ()>(app.script_scope_label())
                    .text_size(TextSize::Body.px())
                    .color(pal.text_muted),
            ),
        )
    });
    let notice = app
        .scripts
        .notice
        .clone()
        .map(|text| selectable_text::<Workspace, ()>(text).color(pal.text_muted));
    #[cfg(not(target_arch = "wasm32"))]
    let saved_rows = app
        .scripts
        .library_items
        .iter()
        .filter(|item| {
            let filter = app.scripts.library_filter.trim().to_lowercase();
            filter.is_empty()
                || item.name.to_lowercase().contains(&filter)
                || item
                    .description
                    .as_deref()
                    .is_some_and(|description| description.to_lowercase().contains(&filter))
        })
        .map(|item| {
            let name = item.name.clone();
            let age = item
                .modified
                .and_then(|modified| modified.elapsed().ok())
                .map_or_else(
                    || "date unavailable".into(),
                    |elapsed| {
                        let seconds = elapsed.as_secs();
                        if seconds < 60 {
                            "updated just now".into()
                        } else if seconds < 3_600 {
                            format!("updated {} min ago", seconds / 60)
                        } else if seconds < 86_400 {
                            format!("updated {} hr ago", seconds / 3_600)
                        } else {
                            format!("updated {} days ago", seconds / 86_400)
                        }
                    },
                );
            let description = item.description.clone().unwrap_or_else(|| "Python".into());
            let detail = format!("{description} · {} B · {age}", item.size);
            recipes::list_row(pal, item.name.clone(), detail, false, move |app| {
                app.open_saved_script(&name);
            })
        })
        .collect::<Vec<_>>();
    #[cfg(not(target_arch = "wasm32"))]
    let library_controls = xcolumn(
        Region::List,
        (
            sized_box(recipes::toggle(
                pal,
                "Choose folder".into(),
                false,
                |app: &mut Workspace| {
                    app.choose_script_library();
                },
            ))
            .dims(Dimensions::new(
                Dim::Stretch,
                Dim::from(ControlSize::Control),
            )),
            app.scripts.draft.is_some().then(|| {
                sized_box(recipes::toggle(
                    pal,
                    "Save".into(),
                    false,
                    |app: &mut Workspace| app.save_script_draft(),
                ))
                .dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::from(ControlSize::Control),
                ))
            }),
            app.scripts.library.as_ref().map(|_| {
                sized_box(recipes::toggle(
                    pal,
                    "Refresh".into(),
                    false,
                    |app: &mut Workspace| app.refresh_script_library(),
                ))
                .dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::from(ControlSize::Control),
                ))
            }),
            app.scripts.library.as_ref().map(|_| {
                recipes::field_bare(
                    pal,
                    "Search scripts",
                    app.scripts.library_filter.clone(),
                    |app, value| app.scripts.library_filter = value,
                    |_, _| {},
                )
            }),
            xcolumn(Region::List, saved_rows),
        ),
    );
    #[cfg(target_arch = "wasm32")]
    let library_controls = label("Saving and Python execution are available in the desktop app.")
        .color(pal.text_muted);

    #[cfg(not(target_arch = "wasm32"))]
    let run_controls = xcolumn(
        Region::List,
        (
            (app.scripts.running.is_none() && app.scripts.draft.is_some()).then(|| {
                sized_box(recipes::toggle(
                    pal,
                    "Run".into(),
                    true,
                    |app: &mut Workspace| app.run_script_draft(),
                ))
                .dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::from(ControlSize::Control),
                ))
            }),
            app.scripts.running.as_ref().map(|run| {
                sized_box(recipes::toggle(
                    pal,
                    format!("Cancel job {}", run.handle.get()),
                    false,
                    |app: &mut Workspace| app.cancel_script_run(),
                ))
                .dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::from(ControlSize::Control),
                ))
            }),
        ),
    );
    #[cfg(target_arch = "wasm32")]
    let run_controls = label("Run is unavailable in the browser.").color(pal.text_muted);

    let proposal = app.scripts.proposal.as_ref().map(|proposal| {
        let operations = proposal
            .result
            .edits
            .iter()
            .map(|edit| edit.operations.len())
            .sum::<usize>();
        let stale = app.script_proposal_stale_reason();
        xcolumn(
            Region::List,
            (
                selectable_text::<Workspace, ()>(format!(
                    "Report · inspected {} layers · proposes {} changes in {} layers",
                    proposal.input.layers.len(),
                    operations,
                    proposal.result.edits.len()
                ))
                .color(pal.text),
                selectable_text::<Workspace, ()>(proposal.result.report.clone())
                    .color(pal.text)
                    .text_size(TextSize::Body.px()),
                (!proposal.stderr.trim().is_empty()).then(|| {
                    selectable_text::<Workspace, ()>(format!(
                        "Diagnostics\n{}",
                        proposal.stderr.trim()
                    ))
                    .color(pal.text_muted)
                    .text_size(TextSize::Body.px())
                }),
                stale.clone().map(|reason| {
                    selectable_text::<Workspace, ()>(format!("Preview stale · {reason}"))
                        .text_size(TextSize::Body.px())
                        .color(pal.role("danger"))
                }),
                #[cfg(unix)]
                (stale.is_none() && !proposal.result.edits.is_empty()).then(|| {
                    recipes::toggle(
                        pal,
                        format!("Apply {operations} changes"),
                        true,
                        |app: &mut Workspace| app.apply_script_proposal(),
                    )
                }),
                proposal.applied.then(|| {
                    recipes::toggle(
                        pal,
                        "Undo applied script".into(),
                        false,
                        |app: &mut Workspace| {
                            app.undo_script_apply();
                        },
                    )
                }),
            ),
        )
    });

    portal(
        xcolumn(
            Region::Panel,
            (
                label("Scripts").color(pal.text),
                selectable_text::<Workspace, ()>("Opening never runs a draft.")
                    .text_size(TextSize::Body.px())
                    .color(pal.text_muted),
                editor,
                app.scripts.draft.is_none().then(|| {
                    selectable_text::<Workspace, ()>(
                        "Open a completed Python artifact from Chat to begin editing.",
                    )
                    .text_size(TextSize::Body.px())
                    .color(pal.text_muted)
                }),
                library_controls,
                run_controls,
                proposal,
                notice,
            ),
        )
        .gap(Space::Md)
        .background_color(pal.panel),
    )
    .constrain_horizontal(true)
}
