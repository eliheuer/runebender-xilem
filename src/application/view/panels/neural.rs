// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Neural inspector section: train a font from the open `.nufo`, and see its versions.

use crate::application::view::design::{Region, TextSize, column as xcolumn};
use crate::application::view::recipes;
use crate::application::workspace::Workspace;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::label;

pub(crate) fn panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let train = &app.train;
    let latest = train.versions.last();
    let version = latest.map_or_else(|| "none yet".to_string(), |v| v.name.clone());
    let score = latest.and_then(|v| {
        let score = v.score()?;
        let epochs = v.epochs()?;
        Some(format!("{score} match after {epochs} epochs"))
    });
    let running = train.job.is_some();
    let status = (!train.status.is_empty()).then(|| {
        label(train.status.clone())
            .text_size(TextSize::Body.px())
            .color(pal.text_muted)
    });
    let button = if running {
        recipes::action(pal, "Cancel".into(), |app: &mut Workspace| {
            app.cancel_train();
        })
        .boxed()
    } else {
        recipes::action(pal, "Train".into(), |app: &mut Workspace| {
            app.command_train();
        })
        .boxed()
    };
    xcolumn(
        Region::Form,
        (
            recipes::kv(pal, "Latest".into(), version),
            score.map(|score| recipes::kv(pal, "Score".into(), score)),
            status,
            button,
        ),
    )
    .boxed()
}
