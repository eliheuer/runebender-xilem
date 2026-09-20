// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The non-executing Scripts rail: draft editing and runtime availability.

use crate::application::view::design::{Region, Space, Stroke, TextSize, column as xcolumn};
use crate::application::view::{label, text_input};
use crate::application::workspace::Workspace;
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use xilem::InsertNewline;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::sized_box;

/// Show the current script draft without providing a second persistence or
/// execution path before the native runtime is registered.
pub(crate) fn scripts_panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let draft = app.scripts.draft.clone();
    let editor = draft.map(|draft| {
        let dirty = draft
            .dirty
            .then_some("Unsaved changes")
            .unwrap_or("Not saved");
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
                    text_input(draft.content, |app: &mut Workspace, value| {
                        app.script_content_changed(value);
                    })
                    .insert_newline(InsertNewline::OnEnter)
                    .clip(true)
                    .placeholder("Python recipe source")
                    .text_color(pal.text)
                    .placeholder_color(pal.text_muted)
                    .background_color(pal.field())
                    .border_color(pal.field_outline)
                    .border_width(Stroke::Hairline.length())
                    .corner_radius(crate::application::view::design::Radius::None.length()),
                )
                .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(280.0)))),
            ),
        )
    });
    let notice = app
        .scripts
        .notice
        .clone()
        .map(|text| label(text).color(pal.text_muted));

    xcolumn(
        Region::Panel,
        (
            label("Scripts").color(pal.text),
            label("Python drafts are not run when opened.")
                .text_size(TextSize::Body.px())
                .color(pal.text_muted),
            editor,
            (!app.scripts.draft.is_some()).then(|| {
                label("Open a completed Python artifact from Chat to begin editing.")
                    .text_size(TextSize::Body.px())
                    .color(pal.text_muted)
            }),
            label("Script storage and Run controls appear after the native recipe runtime is connected.")
                .text_size(TextSize::Body.px())
                .color(pal.text_muted),
            notice,
        ),
    )
    .gap(Space::Md)
    .background_color(pal.panel)
}
