// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Document identity and controls beside the in-window application menus.

use crate::application::view::chrome::header_tools;
use crate::application::view::design::{Space, TextSize};
use crate::application::view::label;
use crate::application::view::panels::tabs::tab_strip;
use crate::application::widgets::text_label::{self, Anchor};
use crate::application::workspace::{AppState, Mode, Workspace};
use masonry::layout::Dim;
use masonry::properties::Dimensions;
use masonry::properties::types::CrossAxisAlignment;
use xilem::WidgetView;
use xilem::core::lens;
use xilem::style::Style;
use xilem::view::FlexExt as _;
use xilem::view::{canvas, flex_row, sized_box};

fn document(app: &mut Workspace) -> Box<xilem::AnyWidgetView<Workspace>> {
    let title = app
        .font
        .source()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let status = if app.modified { "Not saved" } else { "Saved" };
    let pal = app.palette.clone();
    let description = format!("{title}, {status}");
    let identity = sized_box(
        xilem::view::portal(
            canvas(move |_: &mut Workspace, _, scene, size| {
                let mut painter: masonry::imaging::Painter<'_> =
                    masonry::imaging::Painter::new(scene);
                let text_size = TextSize::Body.px();
                let gap = Space::Md.px();
                let status_width = text_label::width(status, text_size);
                let at = kurbo::Point::new(size.width, size.height / 2.0);
                text_label::draw(
                    &mut painter,
                    at,
                    status,
                    text_size,
                    pal.header_ink,
                    Anchor::End,
                );
                text_label::draw_elided(
                    &mut painter,
                    kurbo::Point::new(size.width - status_width - gap, at.y),
                    &title,
                    text_size,
                    pal.header_ink,
                    Anchor::End,
                    (size.width - status_width - gap).max(0.0),
                );
            })
            .alt_text(description),
        )
        .constrain_horizontal(true)
        .constrain_vertical(true)
        .must_fill(true),
    )
    .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
    .flex(1.0);
    flex_row((
        identity,
        matches!(app.mode, Mode::Editor(_)).then(|| header_tools(app)),
        tab_strip(app),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Space::Md)
    .boxed()
}

pub(super) fn view(app: &AppState) -> Box<xilem::AnyWidgetView<AppState>> {
    if app.workspace.is_some() {
        lens(document, |app: &mut AppState| {
            app.workspace
                .as_mut()
                .expect("document header has a workspace")
        })
        .boxed()
    } else {
        sized_box(label("")).boxed()
    }
}
