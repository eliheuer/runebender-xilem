// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The local models panel: finding models on disk and running one.
//!
//! Model workers receive a detached export of the current canonical document.
//! Successful output becomes an immutable, guarded review candidate; only explicit Install
//! commits it to the root with one ordinary undo group. Legacy on-disk proposals keep their
//! existing review and per-glyph installation path.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use runebender::font::history::HistoryDirection;
#[cfg(not(target_arch = "wasm32"))]
use runebender::font::project::DocumentEditTransactionOutcome;
use runebender::font::project::{DocumentHistoryReplayOutcome, EditHistoryGroupId};
use runebender::font::proposal::{self, ProposalSummary};
use runebender::font::proposal::{DetachedProposalCandidate, DetachedProposalCapture};
use runebender::font::variable::GlyphLayerAddress;
use runebender::font::variable::SourceId;
use runebender::workflows::process::{
    OutputStream, ProcessCancellation, ProcessLimits, ProcessOutcome,
};

use crate::application::editor::session::Session;
use crate::application::font_model::FontModel;
use crate::application::view::canvas::grid::cells_of;
use crate::application::workspace::{Mode, Workspace};

/// One task as `font-ml tasks --json` describes it, kept to what a
/// row needs. No task name is written in this crate. Read by hand
/// from the JSON value, since this crate carries `serde_json` and not
/// `serde` itself.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TaskRow {
    /// The name `font-ml run` takes.
    pub(crate) name: String,
    /// One line for the button.
    pub(crate) title: String,
    /// Whether the installed font-ml runs it.
    pub(crate) implemented: bool,
    /// What it takes, by name and kind.
    pub(crate) inputs: Vec<TaskInput>,
}

/// One input of a task, by name and kind.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TaskInput {
    /// The flag name.
    pub(crate) name: String,
    /// The kind, as font-ml names it.
    pub(crate) kind: String,
}

impl TaskRow {
    /// One row from one entry of the `tasks` array. None when it has
    /// no name.
    pub(crate) fn from_value(v: &serde_json::Value) -> Option<Self> {
        let text = |key: &str| v.get(key).and_then(|x| x.as_str()).map(String::from);
        let name = text("name")?;
        Some(Self {
            title: text("title").unwrap_or_else(|| name.clone()),
            name,
            implemented: v
                .get("implemented")
                .and_then(|x| x.as_bool())
                .unwrap_or(false),
            inputs: v
                .get("inputs")
                .and_then(|x| x.as_array())
                .map(|list| {
                    list.iter()
                        .filter_map(|i| {
                            Some(TaskInput {
                                name: i.get("name")?.as_str()?.to_string(),
                                kind: i.get("kind")?.as_str()?.to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    /// Whether the task takes a set of glyphs, so "every drawn glyph"
    /// is a call it understands.
    pub(crate) fn takes_glyphs(&self) -> bool {
        self.inputs.iter().any(|i| i.kind == "glyphs")
    }

    /// Whether the task takes one glyph, so "this glyph" is a call.
    pub(crate) fn takes_glyph(&self) -> bool {
        self.inputs
            .iter()
            .any(|i| i.kind == "glyph" || i.kind == "glyphs")
    }
}

/// A run in progress: progress, cancellation, and a retained result for the pump.
#[derive(Debug, Clone, Default)]
pub(crate) struct AiJob {
    /// The last progress line: done, total, glyph.
    pub(crate) progress: Arc<Mutex<Option<(usize, usize, String)>>>,
    /// Stops a running process or prevents a queued worker from starting.
    pub(crate) cancellation: ProcessCancellation,
    /// The report, or the error.
    pub(crate) finished: Arc<Mutex<Option<Result<serde_json::Value, String>>>>,
    /// The task and stable document targets captured at launch.
    pub(crate) task: String,
    pub(crate) source: PathBuf,
    pub(crate) master_path: PathBuf,
    /// The in-memory document session that launched the task.
    pub(crate) document_id: u64,
    pub(crate) glyph: Option<String>,
    /// Every glyph the run names; empty means every drawn glyph.
    pub(crate) glyphs: Vec<String>,
    /// The editor glyph and foreground revisions present at launch.
    pub(crate) active_glyph: String,
    pub(crate) foreground_revisions: BTreeMap<String, String>,
    pub(crate) all_glyphs: bool,
    /// Immutable in-memory guards paired with the exported worker input.
    capture: Option<Arc<DetachedProposalCapture>>,
    /// Shared ownership keeps worker files alive through cancellation and result import.
    _temporary: Option<Arc<ModelCaptureDirectory>>,
}

/// Everything the panel holds.
#[derive(Debug, Default)]
pub(crate) struct LocalAiState {
    /// The chosen model directory.
    pub(crate) dir: Option<PathBuf>,
    /// What the directory says it is, for the panel.
    pub(crate) summary: Option<String>,
    /// Scales what a model predicts.
    pub(crate) strength: f64,
    /// The model directories found on disk, scanned when asked.
    pub(crate) installed: Vec<(String, PathBuf)>,
    /// What font-ml says it can do, from the answer `init_nodes` kept.
    pub(crate) tasks: Vec<TaskRow>,
    /// What font-ml is doing right now.
    pub(crate) busy: Option<String>,
    /// The run going on, if one is.
    pub(crate) job: Option<AiJob>,
    /// Proposals waiting in the active master, one per task.
    pub(crate) proposals: Vec<ProposalSummary>,
    /// The proposal drawn over the active glyph for comparison.
    pub(crate) preview_task: Option<String>,
    /// Canonical foreground edits installed, most recent last, so Undo install can verify and
    /// replay the exact Project history step.
    pub(crate) installed_order: Vec<InstalledProposalEdit>,
    /// Session-only model output; never serialized into the root before Install.
    pending: BTreeMap<(SourceId, String), PendingModelProposal>,
}

impl Drop for LocalAiState {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.cancellation.cancel();
        }
    }
}

/// One canonical proposal installation and its expected Project history depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstalledProposalEdit {
    pub(crate) address: GlyphLayerAddress,
    pub(crate) layer_history_depth: usize,
    pub(crate) group: Option<EditHistoryGroupId>,
}

#[derive(Debug)]
struct PendingModelProposal {
    document_id: u64,
    source: SourceId,
    candidate: DetachedProposalCandidate,
}

#[derive(Debug)]
struct ModelCaptureDirectory(PathBuf);

impl ModelCaptureDirectory {
    fn new() -> std::io::Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..32 {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "runebender-model-capture-{}-{sequence}",
                std::process::id()
            ));
            #[cfg(unix)]
            let result = {
                use std::os::unix::fs::DirBuilderExt as _;
                let mut builder = std::fs::DirBuilder::new();
                builder.mode(0o700).create(&path)
            };
            #[cfg(not(unix))]
            let result = std::fs::create_dir(&path);
            match result {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "cannot allocate model capture",
        ))
    }
}

impl Drop for ModelCaptureDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The pump's message: something arrived from the run thread.
#[derive(Debug)]
pub(crate) struct AiProgress;

/// Capture canonical foreground revisions for named glyphs, or the whole
/// default layer when `names` is empty.
pub(crate) fn foreground_revisions(
    font: &FontModel,
    names: &[String],
) -> Result<BTreeMap<String, String>, String> {
    let source = font
        .project
        .source_id(font.active())
        .ok_or_else(|| "active source no longer exists".to_owned())?;
    let layer = font
        .project
        .document_source(source)
        .ok_or_else(|| "active source no longer exists".to_owned())?
        .default_layer();
    let names: Vec<_> = if names.is_empty() {
        font.project
            .glyph_names()
            .filter(|name| font.project.document_layer(name, &layer).is_some())
            .map(str::to_owned)
            .collect()
    } else {
        names.to_vec()
    };
    names
        .into_iter()
        .map(|name| {
            let glyph = font
                .project
                .document_layer(&name, &layer)
                .ok_or_else(|| format!("{name}: foreground glyph no longer exists"))?;
            Ok((
                name,
                runebender::font::edit_batch::canonical_glyph_revision(glyph)?,
            ))
        })
        .collect()
}

pub(crate) fn foreground_is_current(
    font: &FontModel,
    expected: &BTreeMap<String, String>,
    all_glyphs: bool,
) -> bool {
    let names: Vec<_> = if all_glyphs {
        Vec::new()
    } else {
        expected.keys().cloned().collect()
    };
    foreground_revisions(font, &names).is_ok_and(|current| current == *expected)
}

/// A progress line as font-ml prints it: `progress <done>/<total> <glyph>`.
fn parse_progress(line: &str) -> Option<(usize, usize, &str)> {
    let rest = line.strip_prefix("progress ")?;
    let (count, glyph) = rest.split_once(' ').unwrap_or((rest, ""));
    let (done, total) = count.split_once('/')?;
    Some((done.parse().ok()?, total.parse().ok()?, glyph.trim()))
}

/// Run one font-ml task to completion on the calling thread, feeding progress
/// lines into the job. Returns the JSON object font-ml printed last.
fn run_font_ml(
    font_ml: &Path,
    task: &str,
    model: &Path,
    source: &Path,
    glyphs: &[String],
    strength: f64,
    reference: Option<&Path>,
    device: &str,
    job: &AiJob,
) -> Result<serde_json::Value, String> {
    run_font_ml_with_limits(
        font_ml,
        task,
        model,
        source,
        glyphs,
        strength,
        reference,
        device,
        job,
        ProcessLimits::default(),
    )
}

/// Allow offline worker fixtures to exercise deadline and output bounds.
fn run_font_ml_with_limits(
    font_ml: &Path,
    task: &str,
    model: &Path,
    source: &Path,
    glyphs: &[String],
    strength: f64,
    reference: Option<&Path>,
    device: &str,
    job: &AiJob,
    limits: ProcessLimits,
) -> Result<serde_json::Value, String> {
    let mut cmd = std::process::Command::new(font_ml);
    cmd.arg("run")
        .arg(task)
        .arg("--model")
        .arg(model)
        .arg("--source")
        .arg(source)
        .arg("--strength")
        .arg(format!("{strength}"))
        .arg("--device")
        .arg(device)
        .arg("--write")
        .arg("--json");
    if glyphs.is_empty() {
        cmd.arg("--all");
    } else {
        for name in glyphs {
            cmd.arg("--glyph").arg(name);
        }
    }
    if let Some(reference) = reference {
        cmd.arg("--reference").arg(reference);
    }
    let output = runebender::workflows::process::run(
        &mut cmd,
        &[],
        limits,
        &job.cancellation,
        |stream, line| {
            if stream == OutputStream::Stderr
                && let Some((done, total, glyph)) = parse_progress(line)
            {
                *job.progress
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) =
                    Some((done, total, glyph.to_string()));
            }
        },
    );
    if job.cancellation.is_cancelled() {
        return Err("cancelled".into());
    }
    let report: serde_json::Value = String::from_utf8_lossy(&output.stdout)
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str(l).ok())
        .unwrap_or(serde_json::Value::Null);
    match output.outcome {
        ProcessOutcome::Exited { success: true, .. } => Ok(report),
        ProcessOutcome::Exited { code, .. } => Err(report
            .get("error")
            .and_then(|error| error.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                let diagnostics = String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .filter(|line| !line.trim().is_empty() && parse_progress(line).is_none())
                    .collect::<Vec<_>>()
                    .join("\n");
                if diagnostics.is_empty() {
                    format!("font-ml exited with code {code:?}")
                } else {
                    diagnostics
                }
            })),
        outcome => Err(outcome.to_string()),
    }
}

impl Workspace {
    /// Where models are looked for: `$RUNEBENDER_MODELS`, else
    /// `~/.runebender/models`, plus the roots core reads.
    pub(crate) fn models_dir() -> Option<PathBuf> {
        runebender::workflows::nodes_run::default_models_dir()
    }

    /// Look at the disk again: the model directories and the tasks.
    pub(crate) fn rescan_models(&mut self) {
        self.ai.installed =
            runebender::workflows::nodes_run::installed(Self::models_dir().as_deref(), false);
        self.ai.tasks = self
            .nodes
            .tasks_json
            .as_ref()
            .and_then(|v| v.get("tasks"))
            .and_then(|t| t.as_array())
            .map(|list| list.iter().filter_map(TaskRow::from_value).collect())
            .unwrap_or_default();
        if self.ai.strength == 0.0 {
            self.ai.strength = 1.0;
        }
    }

    /// Remember a model directory and describe it from its
    /// `config.json`, without loading the weights.
    pub(crate) fn load_model(&mut self, dir: &Path) {
        let config = match std::fs::read_to_string(dir.join("config.json")) {
            Ok(text) => text,
            Err(e) => {
                self.note = format!("Model: {e}");
                return;
            }
        };
        let parsed: serde_json::Value = match serde_json::from_str(&config) {
            Ok(v) => v,
            Err(e) => {
                self.note = format!("Model: config.json: {e}");
                return;
            }
        };
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "model".into());
        let kind = parsed
            .get("kind")
            .and_then(|k| k.as_str())
            .unwrap_or("outline");
        let shape = match (parsed.get("layers"), parsed.get("dims")) {
            (Some(l), Some(d)) => format!(", {l} layers × {d}"),
            _ => String::new(),
        };
        self.ai.summary = Some(format!("{name}: {kind}{shape}"));
        self.ai.dir = Some(dir.to_path_buf());
        self.note = "Model chosen".into();
    }

    /// What the active master has waiting, from any task.
    pub(crate) fn refresh_proposals(&mut self) {
        self.ai.proposals = self
            .font
            .project
            .source_id(self.font.active())
            .map(|source| proposal::list_project(&self.font.project, source))
            .unwrap_or_default()
            .into_iter()
            .filter(|proposal| !proposal.glyphs.is_empty())
            .collect();
        let source = self.font.project.source_id(self.font.active());
        self.ai
            .pending
            .retain(|_, pending| pending.document_id == self.document_id);
        for pending in self
            .ai
            .pending
            .values()
            .filter(|pending| Some(pending.source) == source)
        {
            let summary = pending.candidate.summary();
            self.ai
                .proposals
                .retain(|existing| existing.task != summary.task);
            self.ai.proposals.push(summary.clone());
        }
        if self
            .ai
            .preview_task
            .as_ref()
            .is_some_and(|task| !self.ai.proposals.iter().any(|p| p.task == *task))
        {
            self.ai.preview_task = None;
        }
    }

    /// Resolve a session candidate without exposing it as a root proposal layer.
    pub(crate) fn model_proposal_outline(&self, task: &str, glyph: &str) -> Option<kurbo::BezPath> {
        let source = self.font.project.source_id(self.font.active())?;
        let Some(pending) = self.ai.pending.get(&(source, task.to_owned())) else {
            return self.font.proposal_outline(task, glyph);
        };
        if pending.document_id != self.document_id
            || Some(pending.source) != self.font.project.source_id(self.font.active())
            || self
                .font
                .project
                .preview_document_edit_transaction(pending.candidate.transaction())
                .is_err()
        {
            return None;
        }
        pending
            .candidate
            .snapshots()
            .iter()
            .find(|snapshot| snapshot.address().glyph == glyph)
            .map(|snapshot| {
                runebender::outline::glyph_paths::ordinary_layer_contours_to_bezpath(
                    snapshot.view(),
                )
            })
    }

    #[cfg(target_arch = "wasm32")]
    fn install_model_candidate(&mut self, _task: &str, _only: Option<&[String]>) {
        self.note = "Model candidate installation is available in the desktop application.".into();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn install_model_candidate(&mut self, task: &str, only: Option<&[String]>) {
        if self.session.gesture_in_progress() {
            self.note = "Finish the canvas gesture before installing".into();
            return;
        }
        let Some(source) = self.font.project.source_id(self.font.active()) else {
            self.note = "The active source is unavailable".into();
            return;
        };
        let key = (source, task.to_owned());
        let pending = self.ai.pending.get(&key).expect("selected model candidate");
        if pending.document_id != self.document_id
            || Some(pending.source) != self.font.project.source_id(self.font.active())
        {
            self.note = "Model candidate belongs to another document or master".into();
            return;
        }
        if only.is_some_and(|names| {
            let expected: std::collections::BTreeSet<_> =
                pending.candidate.summary().glyphs.iter().collect();
            names.iter().collect::<std::collections::BTreeSet<_>>() != expected
        }) {
            self.note = "Install the complete model candidate as one edit".into();
            return;
        }
        let transaction = pending.candidate.transaction().clone();
        match self
            .font
            .project
            .commit_document_edit_transaction(transaction)
        {
            Ok(DocumentEditTransactionOutcome::Changed {
                change,
                history_group,
                ..
            }) => {
                let address = change.affected_layers()[0].clone();
                let installed: Vec<String> = change
                    .affected_layers()
                    .iter()
                    .map(|layer| layer.glyph.clone())
                    .collect();
                self.remember_debt_install(task, &installed);
                self.record_agent_group(history_group, &change);
                self.ai.installed_order.push(InstalledProposalEdit {
                    layer_history_depth: 0,
                    address,
                    group: Some(history_group),
                });
                self.ai.pending.remove(&key);
                self.ai.preview_task = None;
                self.refresh_proposals();
                self.note =
                    format!("Installed {task} as one edit. Undo restores the complete candidate.");
            }
            Ok(DocumentEditTransactionOutcome::Unchanged { .. }) => {
                self.ai.pending.remove(&key);
                self.refresh_proposals();
                self.note = "Model candidate has no changes".into();
            }
            Err(error) => self.note = format!("Cannot install model candidate: {error}"),
        }
    }

    /// Show or hide one proposal over the active glyph. This changes
    /// only review state; it never installs the proposed outline.
    pub(crate) fn toggle_proposal_preview(&mut self, task: &str) {
        if self.ai.preview_task.as_deref() == Some(task) {
            self.ai.preview_task = None;
            self.note = "Proposal comparison hidden".into();
        } else {
            self.ai.preview_task = Some(task.to_string());
            self.note = format!("Comparing {task} proposal with the current glyph");
        }
    }

    /// Pull a proposal layer from the UFO on disk into the open font,
    /// replacing any earlier proposal for the task.
    pub(crate) fn adopt_proposal_from_disk(
        &mut self,
        task: &str,
        source: &Path,
    ) -> Result<ProposalSummary, String> {
        let on_disk = runebender::font::project::Project::load(source)?;
        let on_disk_source = on_disk
            .document_sources()
            .next()
            .ok_or("the proposal source has no font source")?
            .id();
        let source = self
            .font
            .project
            .source_id(self.font.active())
            .ok_or("the active source is unavailable")?;
        if proposal::find_project(&self.font.project, source, task).is_ok() {
            proposal::discard_project(&mut self.font.project, source, task)
                .map_err(|error| error.to_string())?;
        }
        let summary = proposal::adopt_external_project(
            &mut self.font.project,
            source,
            &on_disk,
            on_disk_source,
            task,
        )
        .map_err(|error| error.to_string())?;
        self.modified = true;
        Ok(summary)
    }

    /// Install a waiting proposal with one canonical history step per changed glyph.
    pub(crate) fn install_proposal(&mut self, task: &str, only: Option<Vec<String>>) {
        if self
            .font
            .project
            .source_id(self.font.active())
            .is_some_and(|source| self.ai.pending.contains_key(&(source, task.to_owned())))
        {
            self.install_model_candidate(task, only.as_deref());
            return;
        }
        let Some(source) = self.font.project.source_id(self.font.active()) else {
            self.note = "The active source is unavailable".into();
            return;
        };
        let result =
            proposal::install_project(&mut self.font.project, source, task, only.as_deref(), true);
        match result {
            Ok(result) => {
                let done = result.installed;
                self.remember_debt_install(task, &done.installed);
                self.ai
                    .installed_order
                    .extend(result.affected.into_iter().map(|address| {
                        InstalledProposalEdit {
                            group: None,
                            layer_history_depth: self
                                .font
                                .project
                                .document_layer_history_depth(&address, HistoryDirection::Undo),
                            address,
                        }
                    }));
                self.after_font_change(&done.installed);
                self.note = format!(
                    "Installed {} glyphs from {}{}. Undo install takes them back one at a time.",
                    done.installed.len(),
                    done.task,
                    if done.skipped.is_empty() {
                        String::new()
                    } else {
                        format!(", {} skipped", done.skipped.len())
                    }
                );
            }
            Err(e) => self.note = format!("{e}"),
        }
        if self.ai.preview_task.as_deref() == Some(task) {
            self.ai.preview_task = None;
        }
        self.refresh_proposals();
    }

    /// Take back the most recent install, one glyph.
    pub(crate) fn undo_install(&mut self) {
        let Some(edit) = self.ai.installed_order.pop() else {
            self.note = "Nothing installed to undo".into();
            return;
        };
        #[cfg(target_arch = "wasm32")]
        if edit.group.is_some() {
            self.ai.installed_order.push(edit);
            self.note = "Model candidate history is available in the desktop application.".into();
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(group) = edit.group {
            if let Err(error) = self.replay_agent_group(group, HistoryDirection::Undo) {
                self.ai.installed_order.push(edit);
                self.note = format!("Cannot undo model install: {error}");
            } else {
                self.note = "Undid model install".into();
            }
            return;
        }
        if self
            .font
            .project
            .document_layer_history_depth(&edit.address, HistoryDirection::Undo)
            != edit.layer_history_depth
            || !self
                .font
                .project
                .can_replay_document_layer_history(&edit.address, HistoryDirection::Undo)
        {
            self.ai.installed_order.push(edit);
            self.note = "The installed glyph changed before Undo install".into();
            return;
        }
        let name = edit.address.glyph.clone();
        if matches!(
            self.font
                .project
                .replay_document_layer_history(&edit.address, HistoryDirection::Undo),
            Ok(DocumentHistoryReplayOutcome::Changed { .. })
        ) {
            self.after_font_change(std::slice::from_ref(&name));
            self.note = format!("Undid install of {name}");
        } else {
            self.ai.installed_order.push(edit);
            self.note = "The installed glyph could not be restored".into();
        }
    }

    /// Drop a waiting proposal without installing it.
    pub(crate) fn discard_proposal(&mut self, task: &str) {
        let key = self
            .font
            .project
            .source_id(self.font.active())
            .map(|source| (source, task.to_owned()));
        if key
            .as_ref()
            .and_then(|key| self.ai.pending.get(key))
            .is_some_and(|pending| {
                pending.document_id == self.document_id
                    && Some(pending.source) == self.font.project.source_id(self.font.active())
            })
        {
            self.ai
                .pending
                .remove(key.as_ref().expect("selected active candidate"));
            self.ai.preview_task = None;
            self.refresh_proposals();
            self.note = "Discarded model candidate".into();
            return;
        }
        let Some(source) = self.font.project.source_id(self.font.active()) else {
            self.note = "The active source is unavailable".into();
            return;
        };
        match proposal::discard_project(&mut self.font.project, source, task) {
            Ok(n) => {
                self.modified = true;
                self.note = format!("Discarded {n} proposed glyphs");
            }
            Err(e) => self.note = format!("{e}"),
        }
        if self.ai.preview_task.as_deref() == Some(task) {
            self.ai.preview_task = None;
        }
        self.refresh_proposals();
    }

    /// The font changed under the cache: rebuild the cells, and the
    /// open session when its glyph was one of them.
    pub(crate) fn after_font_change(&mut self, names: &[String]) {
        for name in names {
            if let Some(index) = self.font.index_of(name) {
                self.font.refresh_entry(index);
            }
        }
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        self.modified = true;
        if matches!(self.mode, Mode::Editor(_))
            && names.iter().any(|n| *n == self.session.glyph_name)
            && let Some(fresh) = Session::new_from_model(&self.font, &self.session.glyph_name)
        {
            // The open glyph was replaced under the session; start it
            // again on the new canonical outline.
            let mut fresh = fresh;
            fresh.viewport = self.session.viewport.clone();
            fresh.fitted = self.session.fitted;
            self.session = Arc::new(fresh);
        }
    }

    /// Run the task with font-ml over the open master. `glyph` names
    /// one glyph; `None` runs every drawn glyph. Every result remains a
    /// proposal until the user explicitly installs or discards it.
    pub(crate) fn run_task(&mut self, task: &str, glyph: Option<usize>) {
        let names: Vec<String> = match glyph {
            Some(index) => match self.font.glyphs.get(index) {
                Some(entry) => vec![entry.name.clone()],
                None => return,
            },
            None => Vec::new(),
        };
        self.run_task_on(task, names);
    }

    /// Run the task with font-ml over named glyphs of the open master; an
    /// empty list runs every drawn glyph. The Weight debt section sends a
    /// batch this way. Every result remains a proposal until the user
    /// explicitly installs or discards it.
    pub(crate) fn run_task_on(&mut self, task: &str, names: Vec<String>) {
        if cfg!(target_arch = "wasm32") {
            self.note = "Local AI and workflow execution are available in the desktop app.".into();
            return;
        }
        if self.session.gesture_in_progress() {
            self.note = "Finish the canvas gesture before running a model".into();
            return;
        }
        let Some(model) = self.ai.dir.clone() else {
            self.note = "Choose a model first".into();
            return;
        };
        let Some(font_ml) = self.nodes.font_ml.clone() else {
            self.note =
                "font-ml not found: cargo install --git https://github.com/eliheuer/font-ml, \
                         or set RUNEBENDER_FONT_ML"
                    .into();
            return;
        };
        if self.ai.job.is_some() {
            self.note = "A model is already running".into();
            return;
        }
        let glyph_name = (names.len() == 1).then(|| names[0].clone());
        // Reference fitting remains an explicit Nodes input. The
        // direct rail uses the visible strength control, which is the
        // dependable bounded workflow for a draft model.
        let strength = self.ai.strength;
        let device = self.nodes.device.clone();
        let target_names = names;
        let foreground_revisions = match foreground_revisions(&self.font, &target_names) {
            Ok(revisions) => revisions,
            Err(error) => {
                self.note = format!("Cannot capture model target: {error}");
                return;
            }
        };
        let captured = (|| {
            let source_id = self
                .font
                .project
                .source_id(self.font.active())
                .ok_or("missing active source")?;
            if self.ai.pending.len() >= 16
                && !self.ai.pending.contains_key(&(source_id, task.to_owned()))
            {
                return Err(
                    "discard a pending model candidate before retaining another (limit 16)".into(),
                );
            }
            if proposal::find_project(&self.font.project, source_id, task).is_ok() {
                return Err(
                    "review or discard the existing saved proposal for this task first".into(),
                );
            }
            let capture = DetachedProposalCapture::capture(
                &self.font.project,
                source_id,
                task,
                &target_names,
            )?;
            let temporary =
                Arc::new(ModelCaptureDirectory::new().map_err(|error| error.to_string())?);
            let exports = self
                .font
                .project
                .export_worker_capture(&temporary.0, task)?;
            let source = exports
                .get(&source_id)
                .ok_or("missing detached active source")?
                .clone();
            Ok::<_, String>((Arc::new(capture), temporary, source))
        })();
        let (capture, temporary, source) = match captured {
            Ok(captured) => captured,
            Err(error) => {
                self.note = format!("Cannot capture model input: {error}");
                return;
            }
        };
        self.ai.busy = Some(match (&glyph_name, target_names.len()) {
            (Some(name), _) => format!("Running {task} on {name}…"),
            (None, 0) => format!("Running {task} on every glyph…"),
            (None, count) => format!("Running {task} on {count} glyphs…"),
        });
        let job = AiJob {
            task: task.to_string(),
            source: source.clone(),
            master_path: self.font.source().to_path_buf(),
            document_id: self.document_id,
            glyph: glyph_name.clone(),
            glyphs: target_names.clone(),
            active_glyph: self.session.glyph_name.clone(),
            foreground_revisions,
            all_glyphs: target_names.is_empty(),
            capture: Some(capture),
            _temporary: Some(temporary),
            ..AiJob::default()
        };
        self.ai.job = Some(job.clone());
        let task = task.to_string();
        std::thread::spawn(move || {
            let result = run_font_ml(
                &font_ml,
                &task,
                &model,
                &source,
                &target_names,
                strength,
                None,
                &device,
                &job,
            );
            *job.finished.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
        });
    }

    /// Stop the direct worker child and discard its detached result.
    /// Trusted programs may still have side effects outside their supplied capture.
    pub(crate) fn cancel_task(&mut self) {
        let Some(job) = self.ai.job.as_ref() else {
            return;
        };
        job.cancellation.cancel();
        self.note = "Cancelled".into();
    }

    /// The pump: what the run thread has said since last time.
    pub(crate) fn ai_pump(&mut self) {
        let Some(job) = self.ai.job.clone() else {
            return;
        };
        if !job.cancellation.is_cancelled()
            && let Some((done, total, glyph)) = job
                .progress
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        {
            self.ai.busy = Some(format!("{}: {done}/{total} ({glyph})", job.task));
        }
        let finished = job
            .finished
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(result) = finished {
            self.ai.busy = None;
            self.ai.job = None;
            // Cancel can arrive after the worker's success is retained but
            // before the UI pumps it. Never import that late candidate.
            if job.cancellation.is_cancelled() {
                self.note = "font-ml: cancelled".into();
            } else {
                match result {
                    Ok(report) => self.task_finished(&job, &report),
                    Err(e) => self.note = format!("font-ml: {e}"),
                }
            }
        }
    }

    /// Validate the detached result and retain it outside the document until Install.
    fn task_finished(&mut self, job: &AiJob, report: &serde_json::Value) {
        if job.cancellation.is_cancelled() {
            self.note = "font-ml: cancelled".into();
            return;
        }
        if self.document_id != job.document_id
            || self.font.source() != job.master_path
            || self.session.glyph_name != job.active_glyph
            || !foreground_is_current(&self.font, &job.foreground_revisions, job.all_glyphs)
        {
            self.note =
                "font-ml result is stale after a document, master, glyph, or revision change"
                    .into();
            return;
        }
        if let Some(name) = &job.glyph
            && self.font.index_of(name).is_none()
        {
            self.note = "font-ml result target no longer exists".into();
            return;
        }
        let staged = (|| {
            let capture = job
                .capture
                .as_ref()
                .ok_or("model job has no immutable capture")?;
            let external = runebender::font::project::Project::load(&job.source)?;
            let external_source = external
                .document_sources()
                .next()
                .ok_or("missing worker source")?
                .id();
            capture.stage(&self.font.project, &external, external_source)
        })();
        let candidate = match staged {
            Ok(candidate) => candidate,
            Err(error) => {
                self.note = format!("font-ml: {error}");
                return;
            }
        };
        let summary = candidate.summary().clone();
        let source = self
            .font
            .project
            .source_id(self.font.active())
            .expect("validated active source");
        self.ai.pending.insert(
            (source, job.task.clone()),
            PendingModelProposal {
                document_id: job.document_id,
                source,
                candidate,
            },
        );
        match &job.glyph {
            Some(name) => {
                let moved = report.get("moved").and_then(|v| v.as_u64()).unwrap_or(0);
                let points = report.get("points").and_then(|v| v.as_u64()).unwrap_or(0);
                let advance = report
                    .get("advance_delta")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                self.note = format!(
                    "{} on {}: {moved}/{points} points moved, advance {advance:+}. \
                     Review the proposal, then Install or Discard.",
                    job.task, name
                );
                self.refresh_proposals();
                self.ai.preview_task = Some(job.task.clone());
            }
            None => {
                self.note = format!(
                    "{} of {} glyphs proposed ({} keep structure). Install or discard in the panel.",
                    summary.glyphs.len(),
                    if job.glyphs.is_empty() {
                        job.foreground_revisions.len()
                    } else {
                        job.glyphs.len()
                    },
                    summary.compatible.len()
                );
                self.refresh_proposals();
                if summary
                    .glyphs
                    .iter()
                    .any(|glyph| glyph == &self.session.glyph_name)
                {
                    self.ai.preview_task = Some(job.task.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy_tree(source: &Path, destination: &Path) {
        std::fs::create_dir_all(destination).expect("the destination directory is created");
        for entry in std::fs::read_dir(source).expect("the source directory is readable") {
            let entry = entry.expect("the source entry is readable");
            let from = entry.path();
            let to = destination.join(entry.file_name());
            if entry
                .file_type()
                .expect("the source type is readable")
                .is_dir()
            {
                copy_tree(&from, &to);
            } else {
                std::fs::copy(&from, &to).expect("the source file is copied");
            }
        }
    }

    #[cfg(unix)]
    fn offline_model_worker(root: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let path = root.join("font-ml");
        std::fs::write(&path, r#"#!/usr/bin/env python3
import json, pathlib, plistlib, sys, xml.etree.ElementTree as ET
args = sys.argv
source = pathlib.Path(args[args.index('--source') + 1])
model = pathlib.Path(args[args.index('--model') + 1]).name
if model == 'noop':
    print(json.dumps({'ok': True}))
    sys.exit(0)
with (source / 'layercontents.plist').open('rb') as f:
    layers = plistlib.load(f)
default = next(folder for name, folder in layers if name == 'public.default')
with (source / default / 'contents.plist').open('rb') as f:
    contents = plistlib.load(f)
name = 'B' if model == 'outside' else 'A'
glyph_file = source / default / contents[name]
glyph = ET.parse(glyph_file)
advance = glyph.getroot().find('advance')
width = float(advance.attrib.get('width', '0'))
if model == 'mutate':
    advance.set('width', '999')
    glyph.write(glyph_file, encoding='utf-8', xml_declaration=True)
advance.set('width', str(width + 80))
proposal = source / 'glyphs.detached'
proposal.mkdir()
glyph.write(proposal / 'result.glif', encoding='utf-8', xml_declaration=True)
with (proposal / 'contents.plist').open('wb') as f:
    plistlib.dump({name: 'result.glif'}, f)
layers.append(['com.runebender.proposal.bolden', 'glyphs.detached'])
with (source / 'layercontents.plist').open('wb') as f:
    plistlib.dump(layers, f)
print(json.dumps({'ok': True, 'input_width': width, 'capture_source': str(source), 'moved': 0, 'points': 4, 'advance_delta': 80}))
"#).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[cfg(unix)]
    fn wait_model(workspace: &mut Workspace) -> serde_json::Value {
        let started = std::time::Instant::now();
        loop {
            let job = workspace
                .ai
                .job
                .as_ref()
                .expect("model worker was launched");
            let finished = job.finished.lock().unwrap().clone();
            if let Some(result) = finished {
                let report = result.expect("offline worker succeeds");
                workspace.ai_pump();
                return report;
            }
            assert!(started.elapsed().as_secs() < 10, "worker did not finish");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[cfg(unix)]
    #[test]
    fn offline_model_reads_unsaved_capture_and_waits_for_atomic_install() {
        let root = ModelCaptureDirectory::new().unwrap();
        let source = root.0.join("Original.ufo");
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("A");
        glyph.width = 500.0;
        font.default_layer_mut().insert_glyph(glyph);
        font.save(&source).unwrap();
        let original = norad::Font::load(&source).unwrap();
        let mut workspace = Workspace::open(&source).unwrap();
        let index = workspace.font.index_of("A").unwrap();
        workspace.open_glyph(index);
        let address = workspace.font.active_layer_address("A").unwrap();
        let mut edit = workspace
            .font
            .project
            .begin_document_layer_transaction(&address)
            .unwrap();
        edit.draft_mut().set_width(540.0).unwrap();
        workspace
            .font
            .project
            .commit_document_layer_transaction(edit)
            .unwrap();
        workspace.modified = true;
        workspace.open_glyph(index);
        let before = workspace
            .font
            .project
            .capture_document_layer(&address)
            .unwrap();
        let revision = workspace.font.project.document_revision();
        let history = workspace.metadata_undo.len();
        workspace.ai.dir = Some(root.0.join("valid"));
        workspace.nodes.font_ml = Some(offline_model_worker(&root.0));
        workspace.run_task("bolden", Some(index));
        assert!(workspace.ai.job.is_some(), "{}", workspace.note);
        let report = wait_model(&mut workspace);
        assert_eq!(report["input_width"], 540.0);
        let capture_source = PathBuf::from(report["capture_source"].as_str().unwrap());
        assert_ne!(capture_source, source);
        assert!(
            !capture_source.exists(),
            "capture released after candidate import"
        );
        assert_eq!(
            workspace
                .font
                .project
                .capture_document_layer(&address)
                .unwrap(),
            before
        );
        assert_eq!(workspace.font.project.document_revision(), revision);
        assert_eq!(workspace.metadata_undo.len(), history);
        assert!(workspace.modified);
        assert_eq!(workspace.font.source(), source);
        assert!(
            proposal::find_project(&workspace.font.project, address.layer.source, "bolden")
                .is_err()
        );
        assert_eq!(workspace.ai.proposals.len(), 1, "{}", workspace.note);
        assert_eq!(norad::Font::load(&source).unwrap(), original);
        workspace.install_proposal("bolden", None);
        assert_eq!(
            workspace
                .font
                .project
                .document_layer("A", &address.layer)
                .unwrap()
                .width(),
            620.0
        );
        assert_eq!(workspace.metadata_undo.len(), history + 1);
        assert_eq!(norad::Font::load(&source).unwrap(), original);
        workspace.undo_open_glyph(false);
        assert_eq!(
            workspace
                .font
                .project
                .capture_document_layer(&address)
                .unwrap(),
            before
        );
        workspace.undo_open_glyph(true);
        assert_eq!(
            workspace
                .font
                .project
                .document_layer("A", &address.layer)
                .unwrap()
                .width(),
            620.0
        );
        workspace.undo_install();
        assert_eq!(
            workspace
                .font
                .project
                .capture_document_layer(&address)
                .unwrap(),
            before
        );
    }

    #[cfg(unix)]
    #[test]
    fn offline_model_rejects_missing_out_of_scope_and_rewritten_foreground_results() {
        let root = ModelCaptureDirectory::new().unwrap();
        let source = root.0.join("Original.ufo");
        let mut font = norad::Font::new();
        for name in ["A", "B"] {
            let mut glyph = norad::Glyph::new(name);
            glyph.width = 500.0;
            font.default_layer_mut().insert_glyph(glyph);
        }
        font.save(&source).unwrap();
        let worker = offline_model_worker(&root.0);
        for (mode, diagnostic) in [
            ("noop", "no proposal"),
            ("outside", "outside captured scope"),
            ("mutate", "external foreground"),
        ] {
            let mut workspace = Workspace::open(&source).unwrap();
            let index = workspace.font.index_of("A").unwrap();
            workspace.open_glyph(index);
            let before = workspace.font.project.document_snapshot();
            workspace.ai.dir = Some(root.0.join(mode));
            workspace.nodes.font_ml = Some(worker.clone());
            workspace.run_task("bolden", Some(index));
            wait_model(&mut workspace);
            assert!(workspace.note.contains(diagnostic), "{}", workspace.note);
            assert_eq!(workspace.font.project.document_snapshot(), before);
            assert!(workspace.ai.pending.is_empty());
            assert!(!workspace.modified);
            assert!(workspace.metadata_undo.is_empty());
        }
    }

    #[cfg(unix)]
    #[test]
    fn detached_candidates_with_the_same_task_remain_independent_across_masters() {
        let root = ModelCaptureDirectory::new().unwrap();
        for (filename, width) in [("Regular.ufo", 500.0), ("Bold.ufo", 700.0)] {
            let mut font = norad::Font::new();
            let mut glyph = norad::Glyph::new("A");
            glyph.width = width;
            font.default_layer_mut().insert_glyph(glyph);
            font.save(root.0.join(filename)).unwrap();
        }
        let path = root.0.join("Family.designspace");
        std::fs::write(&path, r#"<designspace format="5.0">
<axes><axis tag="wght" name="Weight" minimum="0" default="0" maximum="1"/></axes>
<sources>
<source filename="Regular.ufo" stylename="Regular"><location><dimension name="Weight" xvalue="0"/></location></source>
<source filename="Bold.ufo" stylename="Bold"><location><dimension name="Weight" xvalue="1"/></location></source>
</sources></designspace>"#).unwrap();
        let mut workspace = Workspace::open(&path).unwrap();
        workspace.nodes.font_ml = Some(offline_model_worker(&root.0));
        workspace.ai.dir = Some(root.0.join("valid"));
        for source in [0, 1] {
            workspace.font.set_active(source);
            let index = workspace.font.index_of("A").unwrap();
            workspace.open_glyph(index);
            workspace.run_task("bolden", Some(index));
            wait_model(&mut workspace);
            assert_eq!(workspace.ai.proposals.len(), 1, "{}", workspace.note);
        }
        assert_eq!(workspace.ai.pending.len(), 2);
        workspace.install_proposal("bolden", None);
        assert_eq!(
            workspace.font.font_snapshot().get_glyph("A").unwrap().width,
            780.0
        );
        assert_eq!(workspace.ai.pending.len(), 1);
        workspace.font.set_active(0);
        workspace.refresh_proposals();
        assert_eq!(workspace.ai.proposals.len(), 1);
        workspace.install_proposal("bolden", None);
        assert_eq!(
            workspace.font.font_snapshot().get_glyph("A").unwrap().width,
            580.0
        );
        assert!(workspace.ai.pending.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn cancelling_a_finished_detached_worker_discards_its_files_and_candidate() {
        let root = ModelCaptureDirectory::new().unwrap();
        let source = root.0.join("Original.ufo");
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("A");
        glyph.width = 500.0;
        font.default_layer_mut().insert_glyph(glyph);
        font.save(&source).unwrap();
        let mut workspace = Workspace::open(&source).unwrap();
        workspace.nodes.font_ml = Some(offline_model_worker(&root.0));
        workspace.ai.dir = Some(root.0.join("valid"));
        let index = workspace.font.index_of("A").unwrap();
        workspace.open_glyph(index);
        let before = workspace.font.project.document_snapshot();
        workspace.run_task("bolden", Some(index));
        let capture_path = workspace.ai.job.as_ref().unwrap().source.clone();
        let started = std::time::Instant::now();
        while workspace
            .ai
            .job
            .as_ref()
            .unwrap()
            .finished
            .lock()
            .unwrap()
            .is_none()
        {
            assert!(started.elapsed().as_secs() < 10);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        workspace.cancel_task();
        workspace.ai_pump();
        assert!(!capture_path.exists());
        assert!(workspace.ai.pending.is_empty());
        assert!(workspace.ai.job.is_none());
        assert_eq!(workspace.font.project.document_snapshot(), before);
        assert!(!workspace.modified);
    }

    #[test]
    fn progress_lines_parse() {
        assert_eq!(parse_progress("progress 3/40 H"), Some((3, 40, "H")));
        assert_eq!(parse_progress("wrote layer"), None);
    }

    #[test]
    fn a_task_row_knows_what_it_takes() {
        let row = TaskRow::from_value(&serde_json::json!({
            "name": "bolden", "title": "Bolden", "implemented": true,
            "inputs": [{"name": "glyph", "kind": "glyphs"}]
        }))
        .unwrap();
        assert!(row.takes_glyphs() && row.takes_glyph());
    }

    #[test]
    fn completed_task_does_not_apply_after_the_document_reloads() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-ai-session-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        norad::Font::new()
            .save(&path)
            .expect("the empty UFO fixture saves");
        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        let job = AiJob {
            task: "bolden".into(),
            source: path.clone(),
            master_path: path.clone(),
            document_id: workspace.document_id,
            ..AiJob::default()
        };

        workspace.revert_to_saved();
        workspace.task_finished(&job, &serde_json::json!({}));

        assert_eq!(
            workspace.note,
            "font-ml result is stale after a document, master, glyph, or revision change"
        );
        std::fs::remove_dir_all(path).expect("the empty UFO fixture is removed");
    }

    #[test]
    fn completed_single_glyph_task_waits_for_explicit_install() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-ai-review-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        let mut original = norad::Glyph::new("A");
        original.width = 500.0;
        let mut contour = norad::Contour::default();
        for (x, y) in [(0.0, 0.0), (400.0, 0.0), (400.0, 700.0), (0.0, 700.0)] {
            contour.points.push(norad::ContourPoint::new(
                x,
                y,
                norad::PointType::Line,
                false,
                None,
                None,
            ));
        }
        original.contours.push(contour);
        font.default_layer_mut().insert_glyph(original.clone());
        let mut proposed = original.clone();
        proposed.width = 620.0;
        proposed.contours[0].points[1].x += 20.0;
        let revision = runebender::formats::ufo::glyph_revision(&original)
            .expect("the foreground revision is available");
        runebender::formats::metadata::lib_keys::write_proposal_base(
            &mut proposed,
            &revision,
            "test canonical proposal install",
        );
        runebender::formats::proposal_ufo::write_proposal_layer(
            &mut font,
            "bolden",
            vec![proposed.clone()],
        )
        .expect("the proposal is valid");
        font.save(&path).expect("the proposal fixture saves");

        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        let target_names = vec!["A".to_string()];
        let capture = DetachedProposalCapture::capture(
            &workspace.font.project,
            workspace
                .font
                .project
                .source_id(workspace.font.active())
                .unwrap(),
            "bolden",
            &target_names,
        )
        .unwrap();
        let job = AiJob {
            capture: Some(Arc::new(capture)),
            task: "bolden".into(),
            source: path.clone(),
            master_path: path.clone(),
            document_id: workspace.document_id,
            glyph: Some("A".into()),
            active_glyph: workspace.session.glyph_name.clone(),
            foreground_revisions: foreground_revisions(&workspace.font, &target_names)
                .expect("the foreground revision is captured"),
            ..AiJob::default()
        };
        workspace.task_finished(
            &job,
            &serde_json::json!({"moved": 1, "points": 1, "advance_delta": 120}),
        );

        assert_eq!(
            workspace.font.font_snapshot().get_glyph("A"),
            Some(&original)
        );
        assert!(workspace.ai.installed_order.is_empty());
        assert_eq!(workspace.ai.proposals.len(), 1);
        assert_eq!(workspace.ai.preview_task.as_deref(), Some("bolden"));
        assert!(workspace.note.contains("Review the proposal"));

        workspace.open_glyph(0);
        let underlay = workspace.underlay();
        assert!(underlay.proposal.is_some());
        assert_ne!(
            underlay.proposal,
            workspace.font.glyph_outline("A"),
            "comparison must show the proposed, not current, outline"
        );
        workspace.toggle_proposal_preview("bolden");
        assert!(workspace.underlay().proposal.is_none());
        workspace.toggle_proposal_preview("bolden");
        assert!(workspace.underlay().proposal.is_some());

        workspace.install_proposal("bolden", Some(vec!["A".into()]));
        let mut installed = original.clone();
        installed.width = proposed.width;
        installed.contours[0].points[1].x = proposed.contours[0].points[1].x;
        assert_eq!(
            workspace.font.font_snapshot().get_glyph("A"),
            Some(&installed)
        );
        assert!(workspace.ai.preview_task.is_none());
        assert!(workspace.ai.installed_order[0].group.is_some());
        workspace.undo_open_glyph(false);
        assert_eq!(
            workspace.font.font_snapshot().get_glyph("A"),
            Some(&original)
        );
        workspace.undo_open_glyph(true);
        assert_eq!(
            workspace.font.font_snapshot().get_glyph("A"),
            Some(&installed)
        );
        workspace.undo_install();
        assert_eq!(
            workspace.font.font_snapshot().get_glyph("A"),
            Some(&original)
        );

        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn proposal_history_does_not_consume_an_older_editor_label() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-proposal-history-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("A");
        glyph.width = 500.0;
        font.default_layer_mut().insert_glyph(glyph);
        font.save(&path).expect("the fixture saves");

        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        workspace.open_glyph(0);
        workspace.set_advance_from_buf("510".into());
        assert_eq!(workspace.metadata_undo.len(), 1);
        let source = workspace.font.project.source_id(0).unwrap();
        let layer = workspace
            .font
            .project
            .document_source(source)
            .unwrap()
            .default_layer();
        let revision = runebender::font::edit_batch::canonical_glyph_revision(
            workspace.font.project.document_layer("A", &layer).unwrap(),
        )
        .unwrap();
        let batch = runebender::font::edit_batch::EditBatch {
            task: "spacing".into(),
            reason: "test application history ordering".into(),
            edits: vec![runebender::font::edit_batch::GlyphEdit {
                glyph: "A".into(),
                expected_revision: revision,
                operations: vec![runebender::font::edit_batch::Operation::SetWidth {
                    width: 620.0,
                }],
            }],
        };
        runebender::font::edit_batch::propose_project(&mut workspace.font.project, source, &batch)
            .unwrap();
        workspace.refresh_proposals();
        workspace.install_proposal(&batch.task, None);
        assert_eq!(workspace.session.advance(), 620.0);
        assert_eq!(workspace.metadata_undo.len(), 1);

        workspace.undo_open_glyph(false);
        assert_eq!(workspace.session.advance(), 510.0);
        assert_eq!(workspace.metadata_undo.len(), 1);
        workspace.undo_open_glyph(false);
        assert_eq!(workspace.session.advance(), 500.0);
        assert!(workspace.metadata_undo.is_empty());

        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn completed_task_rejects_a_changed_foreground_revision() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-ai-revision-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        let mut original = norad::Glyph::new("A");
        original.width = 500.0;
        font.default_layer_mut().insert_glyph(original);
        let mut proposed = norad::Glyph::new("A");
        proposed.width = 620.0;
        runebender::formats::proposal_ufo::write_proposal_layer(
            &mut font,
            "bolden",
            vec![proposed],
        )
        .expect("the proposal is valid");
        font.save(&path).expect("the fixture saves");

        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        workspace.open_glyph(0);
        let target_names = vec!["A".to_string()];
        let job = AiJob {
            task: "bolden".into(),
            source: path.clone(),
            master_path: path.clone(),
            document_id: workspace.document_id,
            glyph: Some("A".into()),
            active_glyph: workspace.session.glyph_name.clone(),
            foreground_revisions: foreground_revisions(&workspace.font, &target_names)
                .expect("the foreground revision is captured"),
            ..AiJob::default()
        };
        let address = workspace.font.active_layer_address("A").unwrap();
        let mut transaction = workspace
            .font
            .project
            .begin_document_layer_transaction(&address)
            .unwrap();
        transaction
            .draft_mut()
            .set_width(540.0)
            .expect("the finite width is valid");
        workspace
            .font
            .project
            .commit_document_layer_transaction(transaction)
            .unwrap();

        workspace.task_finished(&job, &serde_json::json!({}));

        assert_eq!(
            workspace.font.font_snapshot().get_glyph("A").unwrap().width,
            540.0
        );
        assert_eq!(
            workspace.note,
            "font-ml result is stale after a document, master, glyph, or revision change"
        );
        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn completed_task_rejects_an_editor_glyph_switch() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-ai-glyph-switch-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("A"));
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("B"));
        font.save(&path).expect("the fixture saves");
        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        let a = workspace.font.index_of("A").expect("A is indexed");
        let b = workspace.font.index_of("B").expect("B is indexed");
        workspace.open_glyph(a);
        let target_names = vec!["A".to_string()];
        let job = AiJob {
            task: "bolden".into(),
            source: path.clone(),
            master_path: path.clone(),
            document_id: workspace.document_id,
            glyph: Some("A".into()),
            active_glyph: workspace.session.glyph_name.clone(),
            foreground_revisions: foreground_revisions(&workspace.font, &target_names)
                .expect("the foreground revision is captured"),
            ..AiJob::default()
        };

        workspace.open_glyph(b);
        workspace.task_finished(&job, &serde_json::json!({}));

        assert_eq!(workspace.session.glyph_name, "B");
        assert_eq!(
            workspace.note,
            "font-ml result is stale after a document, master, glyph, or revision change"
        );
        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn whole_font_revision_capture_detects_a_new_glyph() {
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("A"));
        let mut model = FontModel::from_project(runebender::font::project::Project::from_source(
            runebender::font::project::SourceInput::from_font(font, PathBuf::from("Revision.ufo")),
        ));
        let expected = foreground_revisions(&model, &[]).expect("the layer can be revised");

        assert!(model.add_glyph("B", 500.0, None));

        assert!(!foreground_is_current(&model, &expected, true));
        assert!(foreground_is_current(&model, &expected, false));
    }

    #[cfg(unix)]
    #[test]
    fn cancelled_task_stops_and_never_leaves_a_proposal() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = std::env::temp_dir().join(format!(
            "runebender-xilem-ai-cancel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let source = root.join("Cancel.ufo");
        let tool = root.join("font-ml-fake");
        let model = root.join("model");
        std::fs::create_dir_all(&model).expect("the fake model directory is created");
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("A"));
        font.save(&source).expect("the fixture saves");
        std::fs::write(
            &tool,
            "#!/bin/sh\nwhile true; do printf 'progress 1/2 A\\n' >&2; sleep 0.05; done\n",
        )
        .expect("the fake worker is written");
        let mut permissions = std::fs::metadata(&tool)
            .expect("the fake worker has metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&tool, permissions).expect("the fake worker is executable");

        let mut workspace = Workspace::open(&source).expect("the fixture opens");
        workspace.ai.dir = Some(model);
        workspace.nodes.font_ml = Some(tool);
        let index = workspace.font.index_of("A").expect("A is indexed");
        workspace.run_task("bolden", Some(index));
        let started = std::time::Instant::now();
        while workspace.ai.job.as_ref().is_some_and(|job| {
            job.progress
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_none()
        }) && started.elapsed().as_secs() < 5
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(workspace.ai.job.as_ref().is_some_and(|job| {
            job.progress
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
        }));
        workspace.cancel_task();
        let cancelled = std::time::Instant::now();
        while workspace.ai.job.as_ref().is_some_and(|job| {
            job.finished
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_none()
        }) && cancelled.elapsed().as_secs() < 5
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        workspace.ai_pump();

        assert!(workspace.ai.job.is_none());
        assert!(workspace.ai.proposals.is_empty());
        assert!(
            workspace
                .font
                .font_snapshot()
                .layers
                .iter()
                .all(|layer| { !layer.name().as_str().starts_with(proposal::LAYER_PREFIX) })
        );
        assert_eq!(workspace.note, "font-ml: cancelled");
        std::fs::remove_dir_all(root).expect("the cancellation fixture is removed");
    }

    #[cfg(unix)]
    #[test]
    fn failed_task_keeps_all_worker_diagnostics() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = std::env::temp_dir().join(format!(
            "runebender-xilem-ai-failure-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        std::fs::create_dir_all(&root).expect("the fixture directory is created");
        let tool = root.join("font-ml-fake");
        std::fs::write(
            &tool,
            "#!/bin/sh\nprintf 'first failure\\nsecond detail\\n' >&2\nexit 7\n",
        )
        .expect("the fake worker is written");
        let mut permissions = std::fs::metadata(&tool)
            .expect("the fake worker has metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&tool, permissions).expect("the fake worker is executable");
        let job = AiJob::default();

        let error = run_font_ml(
            &tool,
            "bolden",
            &root,
            &root,
            &["A".to_owned()],
            1.0,
            None,
            "cpu",
            &job,
        )
        .expect_err("the fake worker fails");

        assert!(error.contains("first failure"));
        assert!(error.contains("second detail"));
        std::fs::remove_dir_all(root).expect("the failure fixture is removed");
    }

    #[cfg(unix)]
    #[test]
    fn task_runner_preserves_command_and_progress() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = std::env::temp_dir().join(format!(
            "runebender-xilem-ai-adapter-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        std::fs::create_dir(&root).expect("the fixture directory is created");
        let tool = root.join("font-ml-fake");
        std::fs::write(
            &tool,
            "#!/bin/sh\n[ \"$1\" = run ] && [ \"$2\" = bolden ] && [ \"$3\" = --model ] && [ \"$5\" = --source ] && [ \"$7\" = --strength ] && [ \"$8\" = 1 ] && [ \"$9\" = --device ] && [ \"${10}\" = cpu ] && [ \"${11}\" = --write ] && [ \"${12}\" = --json ] && [ \"${13}\" = --glyph ] && [ \"${14}\" = A ] || exit 17\nprintf 'progress 1/2 A\\n' >&2\nprintf '%s\\n' '{\"moved\":1,\"points\":2}'\n",
        )
        .expect("the fake worker is written");
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o700))
            .expect("the fake worker is executable");
        let job = AiJob::default();
        let report = run_font_ml(
            &tool,
            "bolden",
            &root,
            &root,
            &["A".to_owned()],
            1.0,
            None,
            "cpu",
            &job,
        )
        .expect("the fake worker succeeds");
        assert_eq!(report["moved"], 1);
        assert_eq!(
            *job.progress
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
            Some((1, 2, "A".into()))
        );
        std::fs::remove_dir_all(root).expect("the fixture directory is removed");
    }

    #[cfg(unix)]
    #[test]
    fn task_runner_bounds_hung_and_noisy_workers() {
        use std::os::unix::fs::PermissionsExt as _;
        use std::time::Duration;

        let root = std::env::temp_dir().join(format!(
            "runebender-xilem-ai-bounds-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        std::fs::create_dir(&root).expect("the fixture directory is created");
        let tool = root.join("font-ml-fake");
        std::fs::write(&tool, "#!/bin/sh\nexec sleep 10\n").expect("the hung worker is written");
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o700))
            .expect("the fake worker is executable");
        let job = AiJob::default();
        let limits = ProcessLimits {
            deadline: Duration::from_millis(100),
            ..ProcessLimits::default()
        };
        let error = run_font_ml_with_limits(
            &tool,
            "bolden",
            &root,
            &root,
            &["A".to_owned()],
            1.0,
            None,
            "cpu",
            &job,
            limits,
        )
        .expect_err("the hung worker exceeds its deadline");
        assert!(error.contains("deadline"), "{error}");

        std::fs::write(&tool, "#!/bin/sh\nprintf '12345678901234567890\\n' >&2\n")
            .expect("the noisy worker is written");
        let limits = ProcessLimits {
            stderr_bytes: 8,
            ..ProcessLimits::default()
        };
        let error = run_font_ml_with_limits(
            &tool,
            "bolden",
            &root,
            &root,
            &["A".to_owned()],
            1.0,
            None,
            "cpu",
            &job,
            limits,
        )
        .expect_err("the noisy worker exceeds its output limit");
        assert!(error.contains("stderr"), "{error}");
        std::fs::remove_dir_all(root).expect("the fixture directory is removed");
    }

    #[test]
    fn cancelled_completed_task_never_imports_a_late_candidate() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-ai-late-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        norad::Font::new()
            .save(&path)
            .expect("the empty UFO fixture saves");
        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        let job = AiJob {
            task: "bolden".into(),
            source: path.clone(),
            master_path: path.clone(),
            document_id: workspace.document_id,
            ..AiJob::default()
        };
        *job.finished
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(Ok(serde_json::json!({"moved": 1})));
        workspace.ai.job = Some(job.clone());
        workspace.cancel_task();
        workspace.ai_pump();
        assert!(workspace.ai.job.is_none());
        assert!(workspace.ai.proposals.is_empty());
        assert_eq!(workspace.note, "font-ml: cancelled");
        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    #[ignore = "runs the installed bolden model over a disposable Virtua UFO"]
    fn real_bolden_proposal_waits_for_install_and_undo_restores_the_glyph() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../virtua-grotesk/sources/VirtuaGrotesk-Regular.ufo");
        assert!(
            source.is_dir(),
            "clone Virtua Grotesk beside this repository"
        );
        let model = Workspace::models_dir()
            .expect("the Runebender model directory is configured")
            .join("virtua-12m-bolden");
        assert!(
            model.join("config.json").is_file(),
            "missing model dependency: {}",
            model.display()
        );
        let root = std::env::temp_dir().join(format!(
            "runebender-xilem-real-ai-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let disposable = root.join("Regular.ufo");
        copy_tree(&source, &disposable);
        let original = norad::Font::load(&disposable)
            .expect("the disposable UFO opens")
            .get_glyph("R")
            .expect("Virtua contains R")
            .clone();
        let mut workspace = Workspace::open(&disposable).expect("the disposable UFO opens");
        workspace.load_model(&model);
        workspace.nodes.device = "cpu".into();
        let index = workspace.font.index_of("R").expect("Virtua contains R");
        workspace.open_glyph(index);
        workspace.run_task("bolden", Some(index));
        let started = std::time::Instant::now();
        while workspace.ai.job.as_ref().is_some_and(|job| {
            job.finished
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_none()
        }) && started.elapsed().as_secs() < 60
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        workspace.ai_pump();
        assert!(workspace.ai.job.is_none(), "the bounded model run finishes");
        assert!(workspace.note.contains("points moved"));
        assert_eq!(workspace.ai.preview_task.as_deref(), Some("bolden"));
        assert_eq!(workspace.ai.proposals.len(), 1);

        let on_disk = norad::Font::load(&disposable).expect("the proposal UFO reopens");
        assert_eq!(
            on_disk.get_glyph("R").expect("foreground R remains"),
            &original,
            "inference must not auto-install into the foreground"
        );
        assert!(
            on_disk
                .layers
                .get(&proposal::layer_name("bolden"))
                .is_none()
        );
        let source_id = workspace
            .font
            .project
            .source_id(workspace.font.active())
            .unwrap();
        let proposed = workspace.ai.pending[&(source_id, "bolden".into())]
            .candidate
            .snapshots()
            .iter()
            .find(|snapshot| snapshot.address().glyph == "R")
            .unwrap()
            .clone();
        workspace.install_proposal("bolden", Some(vec!["R".into()]));
        assert_eq!(
            workspace
                .font
                .project
                .capture_document_layer(proposed.address())
                .unwrap(),
            proposed
        );
        assert_eq!(workspace.ai.installed_order.len(), 1);
        assert_eq!(workspace.ai.installed_order[0].address.glyph, "R");

        workspace.undo_install();
        assert_eq!(
            workspace
                .font
                .font_snapshot()
                .get_glyph("R")
                .expect("restored R"),
            &original
        );
        assert!(workspace.ai.installed_order.is_empty());

        std::fs::remove_dir_all(root).expect("the disposable AI fixture is removed");
    }
}
