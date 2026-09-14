// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The local models panel: finding models on disk and running one.
//!
//! The same seam as `edit/local_ai.rs` in the GPUI build. The model
//! runtime is `font-ml`, a separate program. This shell never links
//! it: it finds the binary, runs it over the UFO on disk, and reads
//! the proposal layer it leaves behind. What the shell owns is the
//! seam: save first, run on a thread, pull the proposal layer into the
//! open font, and hand it to core to install or discard.
//!
//! As in the GPUI build, the font is core's `Master`, and an install
//! records one undo step per glyph on its pile. "Undo install" in the
//! panel takes the most recent one back; Cmd+Z over the open glyph
//! does the same through the editor.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use runebender_core::document::proposal::{self, ProposalSummary};

use crate::{Mode, Session, Workspace, cells_of};

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

/// A run in progress: what font-ml has said so far, the child so
/// Cancel can kill it, and its result when it is done. The pump view
/// polls these.
#[derive(Debug, Clone, Default)]
pub(crate) struct AiJob {
    /// The last progress line: done, total, glyph.
    pub(crate) progress: Arc<Mutex<Option<(usize, usize, String)>>>,
    /// The running process.
    pub(crate) child: Arc<Mutex<Option<std::process::Child>>>,
    /// The report, or the error.
    pub(crate) finished: Arc<Mutex<Option<Result<serde_json::Value, String>>>>,
    /// The task and stable document targets captured at launch.
    pub(crate) task: String,
    pub(crate) source: PathBuf,
    pub(crate) master_path: PathBuf,
    /// The in-memory document session that launched the task.
    pub(crate) document_id: u64,
    pub(crate) glyph: Option<String>,
    /// The editor glyph and foreground revisions present at launch.
    pub(crate) active_glyph: String,
    pub(crate) foreground_revisions: BTreeMap<String, String>,
    pub(crate) all_glyphs: bool,
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
    /// Glyphs installed, most recent last, so Undo install knows the
    /// order.
    pub(crate) installed_order: Vec<String>,
}

/// The pump's message: something arrived from the run thread.
#[derive(Debug)]
pub(crate) struct AiProgress;

/// Capture canonical foreground revisions for named glyphs, or the whole
/// default layer when `names` is empty.
pub(crate) fn foreground_revisions(
    font: &norad::Font,
    names: &[String],
) -> Result<BTreeMap<String, String>, String> {
    let glyphs: Vec<_> = if names.is_empty() {
        font.default_layer().iter().collect()
    } else {
        names
            .iter()
            .map(|name| {
                font.get_glyph(name)
                    .ok_or_else(|| format!("{name}: foreground glyph no longer exists"))
            })
            .collect::<Result<_, _>>()?
    };
    glyphs
        .into_iter()
        .map(|glyph| {
            Ok((
                glyph.name().to_string(),
                runebender_core::document::edit_batch::glyph_revision(glyph)?,
            ))
        })
        .collect()
}

pub(crate) fn foreground_is_current(
    font: &norad::Font,
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

/// Run one font-ml task to completion on the calling thread, feeding
/// progress lines into the job and parking the child in it so it can
/// be killed. Returns the JSON object font-ml printed last.
fn run_font_ml(
    font_ml: &Path,
    task: &str,
    model: &Path,
    source: &Path,
    glyph: Option<&str>,
    strength: f64,
    reference: Option<&Path>,
    device: &str,
    job: &AiJob,
) -> Result<serde_json::Value, String> {
    use std::io::BufRead as _;
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
        .arg("--json")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    match glyph {
        Some(name) => {
            cmd.arg("--glyph").arg(name);
        }
        None => {
            cmd.arg("--all");
        }
    }
    if let Some(reference) = reference {
        cmd.arg("--reference").arg(reference);
    }
    let mut child = cmd.spawn().map_err(|e| format!("{e}"))?;
    let stderr = child.stderr.take().ok_or("no stderr")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    *job.child.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
    let stdout_reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::BufReader::new(stdout), &mut text);
        text
    });
    let mut errors = Vec::new();
    for line in std::io::BufReader::new(stderr)
        .lines()
        .map_while(Result::ok)
    {
        match parse_progress(&line) {
            Some((done, total, glyph)) => {
                *job.progress.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some((done, total, glyph.to_string()));
            }
            None if !line.trim().is_empty() => errors.push(line),
            None => {}
        }
    }
    let status = {
        let mut slot = job.child.lock().unwrap_or_else(|e| e.into_inner());
        match slot.as_mut() {
            Some(child) => child.wait().map_err(|e| format!("{e}"))?,
            None => return Err("cancelled".into()),
        }
    };
    let stdout = stdout_reader.join().unwrap_or_default();
    let report: serde_json::Value = stdout
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str(l).ok())
        .unwrap_or(serde_json::Value::Null);
    if status.success() {
        Ok(report)
    } else if status.code().is_none() {
        Err("cancelled".into())
    } else {
        let report_error = report
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string);
        Err(report_error.unwrap_or_else(|| {
            let diagnostics = errors.join("\n");
            if diagnostics.is_empty() {
                format!("font-ml exited with {status}")
            } else {
                diagnostics
            }
        }))
    }
}

impl Workspace {
    /// Where models are looked for: `$RUNEBENDER_MODELS`, else
    /// `~/.runebender/models`, plus the roots core reads.
    pub(crate) fn models_dir() -> Option<PathBuf> {
        runebender_core::document::nodes_run::default_models_dir()
    }

    /// Look at the disk again: the model directories and the tasks.
    pub(crate) fn rescan_models(&mut self) {
        self.ai.installed =
            runebender_core::document::nodes_run::installed(Self::models_dir().as_deref(), false);
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
        self.ai.proposals = proposal::list(self.font.font())
            .into_iter()
            .filter(|p| !p.glyphs.is_empty())
            .collect();
        if self
            .ai
            .preview_task
            .as_ref()
            .is_some_and(|task| !self.ai.proposals.iter().any(|p| p.task == *task))
        {
            self.ai.preview_task = None;
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
        let on_disk = norad::Font::load(source).map_err(|e| e.to_string())?;
        let layer_name = proposal::layer_name(task);
        let glyphs: Vec<norad::Glyph> = on_disk
            .layers
            .get(&layer_name)
            .map(|l| l.iter().cloned().collect())
            .unwrap_or_default();
        if glyphs.is_empty() {
            return Err(format!("font-ml left no {layer_name} layer"));
        }
        let font = self.font.font_mut();
        font.layers.remove(&layer_name);
        let summary = proposal::write(font, task, glyphs).map_err(|e| e.to_string())?;
        self.modified = true;
        Ok(summary)
    }

    /// Install a waiting proposal: one undo step per glyph, on the
    /// master's pile.
    pub(crate) fn install_proposal(&mut self, task: &str, only: Option<Vec<String>>) {
        let result = self
            .font
            .master_mut()
            .install_proposal(task, only.as_deref(), true);
        match result {
            Ok(done) => {
                self.ai
                    .installed_order
                    .extend(done.installed.iter().cloned());
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
        let Some(name) = self.ai.installed_order.pop() else {
            self.note = "Nothing installed to undo".into();
            return;
        };
        let Some(index) = self.font.index_of(&name) else {
            return;
        };
        if self.font.master_mut().undo(index) {
            self.after_font_change(std::slice::from_ref(&name));
            self.note = format!("Undid install of {name}");
        }
    }

    /// Drop a waiting proposal without installing it.
    pub(crate) fn discard_proposal(&mut self, task: &str) {
        let font = self.font.font_mut();
        match proposal::discard(font, task) {
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
            && let Some(fresh) = Session::new(self.font.font(), &self.session.glyph_name)
        {
            // The open glyph was replaced under the session; start it
            // again on the new outline. The install's own undo is on
            // the master's pile, so Cmd+Z in the editor takes it back.
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
        if cfg!(target_arch = "wasm32") {
            self.note = "Local AI and workflow execution are available in the desktop app.".into();
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
        // font-ml reads the UFO on disk, so what is on disk has to be
        // what is on screen.
        if self.modified && !self.save() {
            return;
        }
        let source = self.font.source().to_path_buf();
        if !source.is_dir() {
            self.note = "Save the font before running a model".into();
            return;
        }
        let glyph_name = glyph.and_then(|i| self.font.glyphs.get(i).map(|g| g.name.clone()));
        if glyph.is_some() && glyph_name.is_none() {
            return;
        }
        // Reference fitting remains an explicit Nodes input. The
        // direct rail uses the visible strength control, which is the
        // dependable bounded workflow for a draft model.
        let strength = self.ai.strength;
        let device = self.nodes.device.clone();
        let target_names: Vec<_> = glyph_name.iter().cloned().collect();
        let foreground_revisions = match foreground_revisions(self.font.font(), &target_names) {
            Ok(revisions) => revisions,
            Err(error) => {
                self.note = format!("Cannot capture model target: {error}");
                return;
            }
        };
        self.ai.busy = Some(match &glyph_name {
            Some(name) => format!("Running {task} on {name}…"),
            None => format!("Running {task} on every glyph…"),
        });
        let job = AiJob {
            task: task.to_string(),
            source: source.clone(),
            master_path: self.font.source().to_path_buf(),
            document_id: self.document_id,
            glyph: glyph_name.clone(),
            active_glyph: self.session.glyph_name.clone(),
            foreground_revisions,
            all_glyphs: glyph_name.is_none(),
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
                glyph_name.as_deref(),
                strength,
                None,
                &device,
                &job,
            );
            *job.finished.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
        });
    }

    /// Stop the running task. font-ml writes its proposal only at the
    /// end, so a killed run leaves nothing behind.
    pub(crate) fn cancel_task(&mut self) {
        let Some(job) = self.ai.job.as_ref() else {
            return;
        };
        if let Some(child) = job.child.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            let _ = child.kill();
        }
        self.note = "Cancelled".into();
    }

    /// The pump: what the run thread has said since last time.
    pub(crate) fn ai_pump(&mut self) {
        let Some(job) = self.ai.job.clone() else {
            return;
        };
        if let Some((done, total, glyph)) = job
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
            match result {
                Ok(report) => self.task_finished(&job, &report),
                Err(e) => self.note = format!("font-ml: {e}"),
            }
        }
    }

    /// What happens when font-ml comes back: adopt its proposal layer
    /// from disk and leave it pending for explicit review.
    fn task_finished(&mut self, job: &AiJob, report: &serde_json::Value) {
        if self.document_id != job.document_id
            || self.font.source() != job.master_path
            || self.font.source() != job.source
            || self.session.glyph_name != job.active_glyph
            || !foreground_is_current(self.font.font(), &job.foreground_revisions, job.all_glyphs)
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
        let summary = match self.adopt_proposal_from_disk(&job.task, &job.source) {
            Ok(s) => s,
            Err(e) => {
                self.note = format!("font-ml: {e}");
                return;
            }
        };
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
                    "{} glyphs proposed ({} keep structure). Install or discard in the panel.",
                    summary.glyphs.len(),
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
        proposal::write(&mut font, "bolden", vec![proposed.clone()])
            .expect("the proposal is valid");
        font.save(&path).expect("the proposal fixture saves");

        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        let target_names = vec!["A".to_string()];
        let job = AiJob {
            task: "bolden".into(),
            source: path.clone(),
            master_path: path.clone(),
            document_id: workspace.document_id,
            glyph: Some("A".into()),
            active_glyph: workspace.session.glyph_name.clone(),
            foreground_revisions: foreground_revisions(workspace.font.font(), &target_names)
                .expect("the foreground revision is captured"),
            ..AiJob::default()
        };
        workspace.task_finished(
            &job,
            &serde_json::json!({"moved": 1, "points": 1, "advance_delta": 120}),
        );

        assert_eq!(workspace.font.font().get_glyph("A"), Some(&original));
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
        assert_eq!(workspace.font.font().get_glyph("A"), Some(&proposed));
        assert!(workspace.ai.preview_task.is_none());
        workspace.undo_install();
        assert_eq!(workspace.font.font().get_glyph("A"), Some(&original));

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
        proposal::write(&mut font, "bolden", vec![proposed]).expect("the proposal is valid");
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
            foreground_revisions: foreground_revisions(workspace.font.font(), &target_names)
                .expect("the foreground revision is captured"),
            ..AiJob::default()
        };
        workspace
            .font
            .font_mut()
            .get_glyph_mut("A")
            .expect("A remains loaded")
            .width = 540.0;

        workspace.task_finished(&job, &serde_json::json!({}));

        assert_eq!(workspace.font.font().get_glyph("A").unwrap().width, 540.0);
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
            foreground_revisions: foreground_revisions(workspace.font.font(), &target_names)
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
        let expected = foreground_revisions(&font, &[]).expect("the layer can be revised");

        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("B"));

        assert!(!foreground_is_current(&font, &expected, true));
        assert!(foreground_is_current(&font, &expected, false));
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
                .font()
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
            Some("A"),
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
        let layer_name = proposal::layer_name("bolden");
        let proposed = on_disk
            .layers
            .get(&layer_name)
            .and_then(|layer| layer.get_glyph("R"))
            .expect("the proposal layer contains R")
            .clone();
        assert_ne!(proposed, original);

        workspace.install_proposal("bolden", Some(vec!["R".into()]));
        assert_eq!(
            workspace.font.font().get_glyph("R").expect("installed R"),
            &proposed
        );
        assert_eq!(workspace.ai.installed_order, vec!["R"]);

        workspace.undo_install();
        assert_eq!(
            workspace.font.font().get_glyph("R").expect("restored R"),
            &original
        );
        assert!(workspace.ai.installed_order.is_empty());

        std::fs::remove_dir_all(root).expect("the disposable AI fixture is removed");
    }
}
