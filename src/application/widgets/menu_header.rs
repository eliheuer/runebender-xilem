// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Editor controls beside the in-window application menus.
//!
//! The document's name and save state sit in the footer, so the header
//! keeps only the tools and the tabs, pushed to the trailing edge.

use crate::application::design::Space;
use crate::application::view::chrome::header_tools;
use crate::application::view::label;
use crate::application::view::panels::tabs::tab_strip;
use crate::application::workspace::{AppState, Mode, Workspace};
use masonry::properties::types::CrossAxisAlignment;
use xilem::WidgetView;
use xilem::core::lens;
use xilem::style::Style;
use xilem::view::{FlexSpacer, flex_row, sized_box};

fn document(app: &mut Workspace) -> Box<xilem::AnyWidgetView<Workspace>> {
    flex_row((
        FlexSpacer::Flex(1.0),
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
