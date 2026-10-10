// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Weight debt block of the Local AI panel: what the active master
//! still owes a lighter one, by script, with the measured error of each
//! way of drafting it, and a batch button for each.

use crate::application::editor::tools::weight_debt::{BATCH, ModelScore};
use crate::application::view::design::{Region, TextSize, column as xcolumn};
use crate::application::view::{label, recipes};
use crate::application::workspace::Workspace;
use runebender::analysis::weight_debt::OffsetFit;
use xilem::WidgetView;
use xilem::style::Style;

/// The block, or nothing when the font has one master.
pub(crate) fn block(app: &Workspace) -> Option<impl WidgetView<Workspace> + use<>> {
    if app.font.project.document_sources().count() < 2 {
        return None;
    }
    let pal = &app.palette;
    let muted = |text: String| {
        label(text)
            .text_size(TextSize::Body.px())
            .color(pal.text_muted)
    };
    let plain = |text: String| label(text).text_size(TextSize::Body.px()).color(pal.text);

    let Some(cache) = app.weight_debt() else {
        let hint = "Switch to a heavier master to see its debt";
        return Some(xcolumn(
            Region::List,
            (
                muted("Weight debt".into()),
                muted(hint.into()),
                None,
                Vec::new(),
                None,
            ),
        ));
    };
    let light_name = app.lighter_master_name(cache.report.light);
    let total = cache.report.pending_total();
    let summary = if total == 0 {
        format!("Nothing left matches {light_name}")
    } else {
        format!("{total} glyphs still match {light_name}")
    };
    let model_ready = app.ai.dir.is_some() && app.nodes.font_ml.is_some();
    let model_fresh = app
        .debt
        .measured_path
        .as_deref()
        .is_some_and(|path| path == app.font.source());

    let groups: Vec<_> = cache
        .report
        .groups
        .iter()
        .filter(|group| !group.pending.is_empty())
        .map(|group| {
            let script = group.script;
            let count = group.pending.len().min(BATCH);
            let offset_line = match cache.fits.get(&script) {
                Some((fit, own)) => offset_text(fit, *own),
                None => "Offset: nothing drawn in both masters".to_owned(),
            };
            let model_line = model_fresh
                .then(|| app.debt.model_scores.get(&script).map(model_text))
                .flatten();
            xcolumn(
                Region::List,
                (
                    plain(format!(
                        "{}: {} to draw",
                        script.display_name(),
                        group.pending.len()
                    )),
                    muted(offset_line),
                    model_line.map(muted),
                    cache.fits.contains_key(&script).then(|| {
                        recipes::toggle(
                            pal,
                            format!("Draft {count} by offset"),
                            true,
                            move |app: &mut Workspace| app.draft_debt_with_offset(script),
                        )
                    }),
                    model_ready.then(|| {
                        recipes::toggle(
                            pal,
                            format!("Draft {count} with the model"),
                            true,
                            move |app: &mut Workspace| app.draft_debt_with_model(script),
                        )
                    }),
                ),
            )
        })
        .collect();

    // One row for the measurement: its state, and the one button that
    // starts or stops it.
    let measuring = app.debt.eval_busy.is_some();
    let measure = (measuring || (model_ready && total > 0)).then(|| {
        let (text, button, active) = match &app.debt.eval_busy {
            Some(note) => (note.clone(), "Cancel", false),
            None if model_fresh => ("Model measured on disk".to_owned(), "Again", false),
            None => ("Model not measured here".to_owned(), "Measure", true),
        };
        // The rail is narrow: the button takes its own line.
        xcolumn(
            Region::List,
            (
                plain(text),
                recipes::toggle(pal, button.into(), active, |app: &mut Workspace| {
                    if app.debt.eval_job.is_some() {
                        app.cancel_debt_measure();
                    } else {
                        app.measure_debt_model();
                    }
                }),
            ),
        )
    });
    let mark = (!app.debt.unmarked.is_empty()).then(|| {
        recipes::toggle(
            pal,
            format!("Mark {} installed drafts orange", app.debt.unmarked.len()),
            true,
            |app: &mut Workspace| app.mark_debt_drafts(),
        )
    });

    Some(xcolumn(
        Region::List,
        (
            muted("Weight debt".into()),
            muted(summary),
            measure,
            groups,
            mark.map(|mark| xcolumn(Region::List, (mark,))),
        ),
    ))
}

/// Error in font units against the plain shift, and how often it won,
/// on one line: the number a grader reads before pressing Draft.
fn offset_text(fit: &OffsetFit, own: bool) -> String {
    format!(
        "{} err {:.0} · shift {:.0} · wins {}/{}",
        if own { "Offset" } else { "Shared offset" },
        fit.mean_error,
        fit.baseline_error,
        fit.wins,
        fit.scored
    )
}

fn model_text(score: &ModelScore) -> String {
    format!(
        "Model err {:.0} · shift {:.0} · wins {}/{}",
        score.mean_error, score.baseline_error, score.wins, score.scored
    )
}
