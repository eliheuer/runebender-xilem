// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Weight debt rows of the Local AI panel: what the active master
//! still owes a lighter one, by script, with the measured error of each
//! way of drafting it, and a batch button for each. Rows only, so every
//! line wraps at the dock and every button is a control the design
//! system knows.

use crate::application::design::recipes::{Row, click};
use crate::application::editor::tools::weight_debt::{BATCH, ModelScore};
use crate::application::workspace::Workspace;
use runebender::analysis::weight_debt::OffsetFit;

/// The rows, or nothing when the font has one master. The section
/// caption names the block, so no row repeats "Weight debt".
pub(crate) fn rows(app: &Workspace) -> Option<Vec<Row>> {
    if app.font.project.document_sources().count() < 2 {
        return None;
    }
    let Some(cache) = app.weight_debt() else {
        return Some(vec![Row::Meta(
            "Switch to a heavier master to see its debt".into(),
        )]);
    };
    let light_name = app.lighter_master_name(cache.report.light);
    let total = cache.report.pending_total();
    let summary = if total == 0 {
        format!("Nothing left matches {light_name}")
    } else {
        format!("{total} glyphs still match {light_name}")
    };
    let model_ready = app.ai.dir.is_some() && app.graph.font_ml.is_some();
    let model_fresh = app
        .debt
        .measured_path
        .as_deref()
        .is_some_and(|path| path == app.font.source());

    let mut rows = vec![Row::Meta(summary)];

    // One row for the measurement: its state, and the one button that
    // starts or stops it.
    let measuring = app.debt.eval_busy.is_some();
    if measuring || (model_ready && total > 0) {
        let (text, button, active) = match &app.debt.eval_busy {
            Some(note) => (note.clone(), "Cancel", false),
            None if model_fresh => ("Model measured on disk".to_owned(), "Again", false),
            None => ("Model not measured here".to_owned(), "Measure", true),
        };
        rows.push(Row::Prose(text));
        let on_click = click(|app: &mut Workspace| {
            if app.debt.eval_job.is_some() {
                app.cancel_debt_measure();
            } else {
                app.measure_debt_model();
            }
        });
        rows.push(if active {
            Row::Secondary {
                text: button.into(),
                on_click,
            }
        } else {
            Row::Quiet {
                text: button.into(),
                on_click,
            }
        });
    }

    for group in cache
        .report
        .groups
        .iter()
        .filter(|group| !group.pending.is_empty())
    {
        let script = group.script;
        let count = group.pending.len().min(BATCH);
        rows.push(Row::Prose(format!(
            "{}: {} to draw",
            script.display_name(),
            group.pending.len()
        )));
        rows.push(Row::Meta(match cache.fits.get(&script) {
            Some((fit, own)) => offset_text(fit, *own),
            None => "Offset: nothing drawn in both masters".to_owned(),
        }));
        if model_fresh && let Some(score) = app.debt.model_scores.get(&script) {
            rows.push(Row::Meta(model_text(score)));
        }
        if cache.fits.contains_key(&script) {
            rows.push(Row::Secondary {
                text: format!("Draft {count} by offset"),
                on_click: click(move |app: &mut Workspace| app.draft_debt_with_offset(script)),
            });
        }
        if model_ready {
            rows.push(Row::Secondary {
                text: format!("Draft {count} with the model"),
                on_click: click(move |app: &mut Workspace| app.draft_debt_with_model(script)),
            });
        }
    }

    if !app.debt.unmarked.is_empty() {
        rows.push(Row::Secondary {
            text: format!("Mark {} installed drafts orange", app.debt.unmarked.len()),
            on_click: click(|app: &mut Workspace| app.mark_debt_drafts()),
        });
    }
    Some(rows)
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
