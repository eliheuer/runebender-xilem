// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Weight debt block of the Local AI panel: what the active master
//! still owes a lighter one, and two ways to draft a batch of it.
//!
//! `runebender::analysis::weight_debt` finds the debt and scores the
//! no-model offset. This module owns the cache, the batch, the font-ml
//! measurement that scores the model the same way, and the commands.
//! Every draft is a proposal: the user installs or discards it in the
//! panel, and marks what was installed orange in one more step, so the
//! Undo install bookkeeping stays exact.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use runebender::analysis::weight_debt::{self, DebtReport, OffsetFit, Script};
use runebender::font::proposal::{self, PointMoves};
use runebender::font::variable::{GlyphLayerAddress, SourceId};
use runebender::outline::embolden;
use runebender::workflows::process::{OutputStream, ProcessCancellation, ProcessLimits};

use crate::application::editor::session::semantic_mark;
use crate::application::view::canvas::grid::cells_of;
use crate::application::workspace::{Mode, Workspace};

/// The proposal task of a no-model draft. Distinct from font-ml's
/// `bolden`, so both can wait in the same master.
pub(crate) const OFFSET_TASK: &str = "embolden";
/// The font-ml task of a model draft.
pub(crate) const MODEL_TASK: &str = "bolden";
/// Glyphs per batch: enough for a grading session, few enough to grade.
pub(crate) const BATCH: usize = 20;
/// The lattice a machine draft's moves land on, in font units. The
/// human's finer grid is reached only by grading.
pub(crate) const MACHINE_GRID: f64 = 8.0;
/// Drawn pairs needed before a script learns its own offset.
const OWN_FIT_MINIMUM: usize = 3;

/// How the model fared on one script's drawn glyphs, by `font-ml eval`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ModelScore {
    pub(crate) scored: usize,
    pub(crate) mean_error: f64,
    pub(crate) baseline_error: f64,
    pub(crate) wins: usize,
}

/// A `font-ml eval` run scoring the model where the answer is drawn.
#[derive(Debug, Clone, Default)]
pub(crate) struct DebtEvalJob {
    pub(crate) finished: Arc<Mutex<Option<Result<serde_json::Value, String>>>>,
    pub(crate) cancellation: ProcessCancellation,
    pub(crate) document_id: u64,
    pub(crate) heavy_path: PathBuf,
}

/// The pump's message: the eval thread has something.
#[derive(Debug)]
pub(crate) struct DebtEvalProgress;

/// The report and what was learned from it, cached by document state.
#[derive(Debug)]
pub(crate) struct DebtCache {
    pub(crate) report: DebtReport,
    /// Each script's offset, learned from its own drawn pairs when it
    /// has enough and from every script's otherwise.
    pub(crate) fits: BTreeMap<Script, (OffsetFit, bool)>,
}

/// What a cached report was computed from: document revision, active
/// master index, and source count.
type DebtKey = (u64, usize, usize);

/// Everything the block holds.
#[derive(Debug, Default)]
pub(crate) struct WeightDebtState {
    /// The last report and what it was computed from.
    cache: Mutex<Option<(DebtKey, Arc<DebtCache>)>>,
    /// Model scores by script, from the last measurement.
    pub(crate) model_scores: BTreeMap<Script, ModelScore>,
    /// Which heavier master those scores belong to.
    pub(crate) measured_path: Option<PathBuf>,
    /// The measurement running, if one is.
    pub(crate) eval_job: Option<DebtEvalJob>,
    /// What the measurement is doing.
    pub(crate) eval_busy: Option<String>,
    /// Drafts installed from a debt task and not yet marked.
    pub(crate) unmarked: Vec<String>,
}

impl Drop for WeightDebtState {
    fn drop(&mut self) {
        if let Some(job) = &self.eval_job {
            job.cancellation.cancel();
        }
    }
}

impl Workspace {
    /// The debt of the active master against the lighter one, or `None`
    /// when the active master is the lightest or stands alone.
    ///
    /// Recomputed only when the document, master or source list
    /// changes, so building the panel during a drag costs nothing.
    pub(crate) fn weight_debt(&self) -> Option<Arc<DebtCache>> {
        let project = &self.font.project;
        let active = self.font.active();
        let heavy = project.source_id(active)?;
        let light = weight_debt::lighter_master(project, heavy)?;
        let key = (
            project.document_revision(),
            active,
            project.document_sources().count(),
        );
        let mut cache = self.debt.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((cached_key, cached)) = cache.as_ref()
            && *cached_key == key
        {
            return Some(cached.clone());
        }
        let report = weight_debt::report(project, light, heavy).ok()?;
        let everything: Vec<String> = report
            .groups
            .iter()
            .flat_map(|group| group.drawn.iter().cloned())
            .collect();
        let shared = weight_debt::fit_offset(project, light, heavy, &everything);
        let mut fits = BTreeMap::new();
        for group in &report.groups {
            let own = (group.drawn.len() >= OWN_FIT_MINIMUM)
                .then(|| weight_debt::fit_offset(project, light, heavy, &group.drawn))
                .flatten();
            match (own, shared) {
                (Some(fit), _) => {
                    fits.insert(group.script, (fit, true));
                }
                (None, Some(fit)) => {
                    fits.insert(group.script, (fit, false));
                }
                (None, None) => {}
            }
        }
        let computed = Arc::new(DebtCache { report, fits });
        *cache = Some((key, computed.clone()));
        Some(computed)
    }

    /// The name of the lighter master the debt is measured against.
    pub(crate) fn lighter_master_name(&self, light: SourceId) -> String {
        self.font
            .project
            .document_sources()
            .find(|source| source.id() == light)
            .map(|source| source.name().to_owned())
            .unwrap_or_else(|| "the lighter master".into())
    }

    /// Draft the next batch of one script's debt by pushing each copy
    /// outward by the learned offset, and hold it as a proposal.
    pub(crate) fn draft_debt_with_offset(&mut self, script: Script) {
        if self.session.gesture_in_progress() {
            self.note = "Finish the canvas gesture before drafting".into();
            return;
        }
        let Some(cache) = self.weight_debt() else {
            self.note = "The active master has nothing lighter to draw from".into();
            return;
        };
        let Some(group) = cache.report.groups.iter().find(|g| g.script == script) else {
            return;
        };
        let Some((fit, _)) = cache.fits.get(&script) else {
            self.note =
                "No glyph is drawn in both masters yet, so there is nothing to learn".into();
            return;
        };
        let heavy = cache.report.heavy;
        let Some(layer) = weight_debt::default_layer(&self.font.project, heavy) else {
            return;
        };
        let batch: Vec<String> = group.pending.iter().take(BATCH).cloned().collect();
        let moves: Vec<PointMoves> = batch
            .iter()
            .filter_map(|name| {
                let view = self.font.project.document_layer(name, &layer)?;
                Some(PointMoves {
                    glyph: name.clone(),
                    positions: embolden::layer_point_moves(view, fit.offset, MACHINE_GRID),
                    width: Some(embolden::snap_to_grid(
                        view.width() + fit.advance_delta,
                        MACHINE_GRID,
                    )),
                })
            })
            .collect();
        if moves.is_empty() {
            self.note = format!("{} has no debt left", script.display_name());
            return;
        }
        let reason = format!(
            "Push outward by {:.0} and {:.0} units, learned from {} glyphs drawn in both masters",
            fit.offset.x, fit.offset.y, fit.scored
        );
        match proposal::write_point_moves_project(
            &mut self.font.project,
            heavy,
            OFFSET_TASK,
            &moves,
            &reason,
        ) {
            Ok(summary) => {
                self.modified = true;
                self.refresh_proposals();
                self.ai.preview_task = Some(OFFSET_TASK.into());
                self.note = format!(
                    "{} {} glyphs drafted by offset. Review, then Install or Discard.",
                    summary.glyphs.len(),
                    script.display_name()
                );
            }
            Err(error) => self.note = format!("Cannot draft by offset: {error}"),
        }
    }

    /// Draft the next batch of one script's debt with the font-ml
    /// model. The result arrives through the Local AI pump.
    pub(crate) fn draft_debt_with_model(&mut self, script: Script) {
        let Some(cache) = self.weight_debt() else {
            self.note = "The active master has nothing lighter to draw from".into();
            return;
        };
        let Some(group) = cache.report.groups.iter().find(|g| g.script == script) else {
            return;
        };
        let batch: Vec<String> = group.pending.iter().take(BATCH).cloned().collect();
        if batch.is_empty() {
            self.note = format!("{} has no debt left", script.display_name());
            return;
        }
        if self.ai.dir.is_none() {
            let bolden: Vec<_> = self
                .ai
                .installed
                .iter()
                .filter(|(name, _)| name.contains(MODEL_TASK))
                .map(|(_, path)| path.clone())
                .collect();
            if let [only] = bolden.as_slice() {
                self.load_model(only);
            }
        }
        self.run_task_on(MODEL_TASK, batch);
    }

    /// Score the chosen model on every glyph drawn in both masters,
    /// by script, with `font-ml eval` on the saved sources.
    pub(crate) fn measure_debt_model(&mut self) {
        if cfg!(target_arch = "wasm32") {
            self.note = "Local AI is available in the desktop app.".into();
            return;
        }
        let Some(model) = self.ai.dir.clone() else {
            self.note = "Choose a model first".into();
            return;
        };
        let Some(font_ml) = self.nodes.font_ml.clone() else {
            self.note = "font-ml not found".into();
            return;
        };
        if self.debt.eval_job.is_some() {
            self.note = "A measurement is already running".into();
            return;
        }
        let Some(cache) = self.weight_debt() else {
            self.note = "The active master has nothing lighter to draw from".into();
            return;
        };
        if self.modified {
            self.note = "Save first: the measurement reads the sources on disk".into();
            return;
        }
        let paths = |id: SourceId| {
            self.font
                .project
                .document_sources()
                .find(|source| source.id() == id)
                .map(|source| source.path().to_path_buf())
        };
        let (Some(light), Some(heavy)) = (paths(cache.report.light), paths(cache.report.heavy))
        else {
            self.note = "The masters have no paths on disk".into();
            return;
        };
        let drawn: usize = cache.report.groups.iter().map(|g| g.drawn.len()).sum();
        let device = self.nodes.device.clone();
        let job = DebtEvalJob {
            document_id: self.document_id,
            heavy_path: heavy.clone(),
            ..DebtEvalJob::default()
        };
        self.debt.eval_job = Some(job.clone());
        self.debt.eval_busy = Some(format!("Measuring the model on {drawn} drawn glyphs…"));
        std::thread::spawn(move || {
            let result = run_eval(&font_ml, &model, &light, &heavy, drawn, &device, &job);
            *job.finished.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
        });
    }

    /// Stop a running measurement.
    pub(crate) fn cancel_debt_measure(&mut self) {
        if let Some(job) = &self.debt.eval_job {
            job.cancellation.cancel();
            self.note = "Cancelled".into();
        }
    }

    /// The pump: take the finished measurement into the scores.
    pub(crate) fn debt_eval_pump(&mut self) {
        let Some(job) = self.debt.eval_job.clone() else {
            return;
        };
        let finished = job
            .finished
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let Some(result) = finished else {
            return;
        };
        self.debt.eval_job = None;
        self.debt.eval_busy = None;
        if job.cancellation.is_cancelled() || job.document_id != self.document_id {
            return;
        }
        let report = match result {
            Ok(report) => report,
            Err(error) => {
                self.note = format!("font-ml eval: {error}");
                return;
            }
        };
        let Some(cache) = self.weight_debt() else {
            return;
        };
        let scripts: BTreeMap<&str, Script> = cache
            .report
            .groups
            .iter()
            .flat_map(|group| {
                group
                    .drawn
                    .iter()
                    .map(move |name| (name.as_str(), group.script))
            })
            .collect();
        self.debt.model_scores = scores_by_script(&report, &scripts);
        self.debt.measured_path = Some(job.heavy_path);
        let scored: usize = self.debt.model_scores.values().map(|s| s.scored).sum();
        self.note = format!("Model measured on {scored} drawn glyphs");
    }

    /// Remember what a debt task installed, so it can be marked.
    pub(crate) fn remember_debt_install(&mut self, task: &str, names: &[String]) {
        if task == OFFSET_TASK || task == MODEL_TASK {
            self.debt.unmarked.extend(names.iter().cloned());
        }
    }

    /// Mark every installed, unmarked draft orange: a machine drawing
    /// worth keeping that needs a human's eyes. One transaction per
    /// glyph, after Undo install has had its chance.
    pub(crate) fn mark_debt_drafts(&mut self) {
        let names = std::mem::take(&mut self.debt.unmarked);
        let Some(source) = self.font.project.source_id(self.font.active()) else {
            return;
        };
        let Some(layer) = weight_debt::default_layer(&self.font.project, source) else {
            return;
        };
        let (label, color) = semantic_mark(Some("orange"));
        let mut changed = Vec::new();
        for name in names {
            let address = GlyphLayerAddress {
                glyph: name.clone(),
                layer: layer.clone(),
            };
            let Ok(mut transaction) = self.font.project.begin_document_layer_transaction(&address)
            else {
                continue;
            };
            if transaction.draft_mut().set_mark(label, color) != Ok(true) {
                continue;
            }
            if self
                .font
                .project
                .commit_document_layer_transaction(transaction)
                .is_ok()
            {
                changed.push(name);
            }
        }
        if changed.is_empty() {
            self.note = "Nothing to mark".into();
            return;
        }
        self.after_font_change(&changed);
        if matches!(self.mode, Mode::Overview) {
            self.font.rebuild_cache();
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
        }
        self.modified = true;
        self.note = format!("Marked {} drafts orange", changed.len());
    }
}

/// Run `font-ml eval` to completion on the calling thread.
fn run_eval(
    font_ml: &Path,
    model: &Path,
    light: &Path,
    heavy: &Path,
    limit: usize,
    device: &str,
    job: &DebtEvalJob,
) -> Result<serde_json::Value, String> {
    let mut cmd = std::process::Command::new(font_ml);
    cmd.arg("eval")
        .arg("--model")
        .arg(model)
        .arg("--regular")
        .arg(light)
        .arg("--bold")
        .arg(heavy)
        .arg("--fit-stems")
        .arg("--limit")
        .arg(limit.max(1).to_string())
        .arg("--device")
        .arg(device)
        .arg("--json");
    let limits = ProcessLimits {
        deadline: std::time::Duration::from_secs(60 * 20),
        ..ProcessLimits::default()
    };
    let output = runebender::workflows::process::run(
        &mut cmd,
        &[],
        limits,
        &job.cancellation,
        |_: OutputStream, _: &str| {},
    );
    if job.cancellation.is_cancelled() {
        return Err("cancelled".into());
    }
    let report: serde_json::Value = String::from_utf8_lossy(&output.stdout)
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str(line).ok())
        .unwrap_or(serde_json::Value::Null);
    match output.outcome {
        runebender::workflows::process::ProcessOutcome::Exited { success: true, .. } => Ok(report),
        outcome => Err(report
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| outcome.to_string())),
    }
}

/// Fold `font-ml eval` rows into one score per script. A row names a
/// glyph with its model and baseline error; glyphs outside `scripts`
/// are ignored.
fn scores_by_script(
    report: &serde_json::Value,
    scripts: &BTreeMap<&str, Script>,
) -> BTreeMap<Script, ModelScore> {
    let mut sums: BTreeMap<Script, (usize, f64, f64, usize)> = BTreeMap::new();
    let rows = report
        .get("glyphs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for row in rows {
        let (Some(glyph), Some(model), Some(baseline)) = (
            row.get("glyph").and_then(|v| v.as_str()),
            row.get("model").and_then(|v| v.as_f64()),
            row.get("baseline").and_then(|v| v.as_f64()),
        ) else {
            continue;
        };
        let Some(script) = scripts.get(glyph) else {
            continue;
        };
        let entry = sums.entry(*script).or_default();
        entry.0 += 1;
        entry.1 += model;
        entry.2 += baseline;
        entry.3 += usize::from(model < baseline);
    }
    sums.into_iter()
        .map(|(script, (n, model, baseline, wins))| {
            (
                script,
                ModelScore {
                    scored: n,
                    mean_error: model / n as f64,
                    baseline_error: baseline / n as f64,
                    wins,
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_rows_fold_into_one_score_per_script() {
        let report = serde_json::json!({
            "glyphs": [
                {"glyph": "a", "model": 10.0, "baseline": 20.0},
                {"glyph": "b", "model": 30.0, "baseline": 20.0},
                {"glyph": "alef-hb", "model": 5.0, "baseline": 25.0},
                {"glyph": "unknown", "model": 1.0, "baseline": 1.0},
            ]
        });
        let scripts = BTreeMap::from([
            ("a", Script::Latin),
            ("b", Script::Latin),
            ("alef-hb", Script::Hebrew),
        ]);
        let scores = scores_by_script(&report, &scripts);
        let latin = scores[&Script::Latin];
        assert_eq!(latin.scored, 2);
        assert_eq!(latin.mean_error, 20.0);
        assert_eq!(latin.baseline_error, 20.0);
        assert_eq!(latin.wins, 1);
        assert_eq!(scores[&Script::Hebrew].wins, 1);
        assert!(!scores.contains_key(&Script::Other));
    }

    fn copy_tree(source: &Path, destination: &Path) {
        std::fs::create_dir_all(destination).expect("the destination directory is created");
        for entry in std::fs::read_dir(source).expect("the source directory is readable") {
            let entry = entry.expect("the source entry is readable");
            let (from, to) = (entry.path(), destination.join(entry.file_name()));
            if from.is_dir() {
                copy_tree(&from, &to);
            } else {
                std::fs::copy(&from, &to).expect("the source file is copied");
            }
        }
    }

    /// A disposable copy of the Virtua sources, open on the Bold master.
    fn virtua_on_bold() -> Workspace {
        let source = match std::env::var_os("RUNEBENDER_TEST_FONTS") {
            Some(dir) => PathBuf::from(dir),
            None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../virtua-grotesk/sources"),
        };
        assert!(
            source.join("VirtuaGrotesk.designspace").is_file(),
            "clone Virtua Grotesk beside this repository, or set RUNEBENDER_TEST_FONTS"
        );
        let root = std::env::temp_dir().join(format!(
            "runebender-xilem-weight-debt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        copy_tree(&source, &root);
        let mut workspace =
            Workspace::open(&root.join("VirtuaGrotesk.designspace")).expect("Virtua opens");
        let bold = workspace
            .font
            .master_names()
            .iter()
            .position(|name| name == "Bold")
            .expect("Virtua has a Bold master");
        workspace.set_master(bold);
        workspace
    }

    #[test]
    fn the_lightest_master_shows_no_debt_and_the_heavier_one_does() {
        let mut workspace = virtua_on_bold();
        let cache = workspace.weight_debt().expect("Bold owes Regular");
        assert!(cache.report.pending_total() > 0);
        assert!(cache.fits.contains_key(&Script::Latin));
        workspace.set_master(0);
        assert!(
            workspace.weight_debt().is_none(),
            "Regular has nothing lighter"
        );
    }

    #[test]
    fn an_offset_batch_becomes_a_proposal_that_installs_on_the_machine_grid() {
        let mut workspace = virtua_on_bold();
        let cache = workspace.weight_debt().expect("Bold owes Regular");
        let (script, first) = cache
            .report
            .groups
            .iter()
            .filter(|group| cache.fits.contains_key(&group.script))
            .find_map(|group| Some((group.script, group.pending.first()?.clone())))
            .expect("some script has debt and a fit");
        let heavy = cache.report.heavy;
        let layer = weight_debt::default_layer(&workspace.font.project, heavy).unwrap();
        let before: Vec<_> = workspace
            .font
            .project
            .document_layer(&first, &layer)
            .unwrap()
            .contours()
            .flat_map(|c| c.points().map(|p| p.position()).collect::<Vec<_>>())
            .collect();

        workspace.draft_debt_with_offset(script);
        let proposal = workspace
            .ai
            .proposals
            .iter()
            .find(|p| p.task == OFFSET_TASK)
            .expect("the offset draft waits as a proposal");
        assert!(!proposal.glyphs.is_empty() && proposal.glyphs.len() <= BATCH);
        assert!(proposal.glyphs.contains(&first));
        assert_eq!(proposal.compatible.len(), proposal.glyphs.len());
        assert_eq!(workspace.ai.preview_task.as_deref(), Some(OFFSET_TASK));

        workspace.install_proposal(OFFSET_TASK, Some(vec![first.clone()]));
        let after: Vec<_> = workspace
            .font
            .project
            .document_layer(&first, &layer)
            .unwrap()
            .contours()
            .flat_map(|c| c.points().map(|p| p.position()).collect::<Vec<_>>())
            .collect();
        assert_eq!(before.len(), after.len(), "structure is kept");
        assert_ne!(before, after, "the glyph gained weight");
        for (a, b) in before.iter().zip(&after) {
            assert_eq!(
                (b.x - a.x) % MACHINE_GRID,
                0.0,
                "{first}: move on the 8 grid"
            );
            assert_eq!(
                (b.y - a.y) % MACHINE_GRID,
                0.0,
                "{first}: move on the 8 grid"
            );
        }
        assert_eq!(workspace.debt.unmarked, std::slice::from_ref(&first));

        workspace.mark_debt_drafts();
        assert!(workspace.debt.unmarked.is_empty());
        let entry = workspace
            .font
            .project
            .document_source_glyph_entries(heavy)
            .unwrap()
            .into_iter()
            .find(|entry| entry.name() == first)
            .unwrap();
        assert_eq!(entry.mark_label(), Some("orange"));
        // The debt shrank by what was installed.
        let again = workspace.weight_debt().unwrap();
        assert!(
            !again
                .report
                .groups
                .iter()
                .any(|group| group.pending.contains(&first))
        );
    }

    #[test]
    fn a_report_without_rows_scores_nothing() {
        let scores = scores_by_script(&serde_json::Value::Null, &BTreeMap::new());
        assert!(scores.is_empty());
    }
}
