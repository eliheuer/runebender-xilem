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
//! One thing differs from the GPUI build. That shell's font is core's
//! `Master`, which carries the undo pile an install records into.
//! This shell's font is still its own `model.rs`, so installs record
//! into a pile held here, and "Undo install" in the panel takes them
//! back one glyph at a time. When `model.rs` is replaced by `Master`
//! the pile moves with it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use runebender_core::document::history::EditHistory;
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
    /// The task and the glyph it was run on, for what happens after.
    pub(crate) task: String,
    pub(crate) glyph: Option<usize>,
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
    /// The undo pile for installs, keyed by glyph name.
    pub(crate) installs: EditHistory,
    /// Glyphs installed, most recent last, so Undo install knows the
    /// order.
    pub(crate) installed_order: Vec<String>,
}

/// The pump's message: something arrived from the run thread.
#[derive(Debug)]
pub(crate) struct AiProgress;

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
    let mut last_error = String::new();
    for line in std::io::BufReader::new(stderr)
        .lines()
        .map_while(Result::ok)
    {
        match parse_progress(&line) {
            Some((done, total, glyph)) => {
                *job.progress.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some((done, total, glyph.to_string()));
            }
            None if !line.trim().is_empty() => last_error = line,
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
        Err(report
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
            .unwrap_or(last_error))
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
    }

    /// Pull a proposal layer from the UFO on disk into the open font,
    /// replacing any earlier proposal for the task.
    fn adopt_proposal_from_disk(
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

    /// Install a waiting proposal: one undo step per glyph, into the
    /// panel's pile.
    pub(crate) fn install_proposal(&mut self, task: &str, only: Option<Vec<String>>) {
        let font = self.font.font_mut();
        let installs = &mut self.ai.installs;
        let order = &mut self.ai.installed_order;
        let mut before = |name: &str, glyph: &norad::Glyph| {
            installs.record(name, glyph);
            order.push(name.to_string());
        };
        let result = proposal::install(font, task, only.as_deref(), true, &mut before);
        match result {
            Ok(done) => {
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
        self.refresh_proposals();
    }

    /// Take back the most recent install, one glyph.
    pub(crate) fn undo_install(&mut self) {
        let Some(name) = self.ai.installed_order.pop() else {
            self.note = "Nothing installed to undo".into();
            return;
        };
        let font = self.font.font_mut();
        let Some(glyph) = font.get_glyph_mut(name.as_str()) else {
            return;
        };
        if self.ai.installs.undo(&name, glyph) {
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
        self.refresh_proposals();
    }

    /// The font changed under the cache: rebuild the cells, and the
    /// open session when its glyph was one of them.
    fn after_font_change(&mut self, names: &[String]) {
        for name in names {
            if let Some(index) = self.font.index_of(name)
                && let Some(glyph) = self.font.font().get_glyph(name.as_str()).cloned()
            {
                self.font.replace_glyph(index, glyph);
            }
        }
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        self.modified = true;
        if matches!(self.mode, Mode::Editor(_))
            && names.iter().any(|n| *n == self.session.glyph_name)
            && let Some(fresh) = Session::new(self.font.font(), &self.session.glyph_name)
        {
            // The open glyph was replaced under the session; start it
            // again on the new outline. The install's own undo is in
            // the panel's pile.
            let mut fresh = fresh;
            fresh.viewport = self.session.viewport.clone();
            fresh.fitted = self.session.fitted;
            self.session = Arc::new(fresh);
        }
    }

    /// Run the task with font-ml over the open master. `glyph` names
    /// one glyph, installed as soon as it arrives; None runs every
    /// drawn glyph and leaves the result waiting in the panel.
    pub(crate) fn run_task(&mut self, task: &str, glyph: Option<usize>) {
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
        if self.modified {
            self.save();
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
        // The other master, where it says what weight it carries.
        let reference = (self.font.master_paths().len() > 1).then(|| {
            let other = if self.font.active() == 0 {
                self.font.master_paths().len() - 1
            } else {
                0
            };
            self.font.master_paths()[other].clone()
        });
        let strength = self.ai.strength;
        self.ai.busy = Some(match &glyph_name {
            Some(name) => format!("Running {task} on {name}…"),
            None => format!("Running {task} on every glyph…"),
        });
        let job = AiJob {
            task: task.to_string(),
            glyph,
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
                reference.as_deref(),
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
                Ok(report) => self.task_finished(&job.task, job.glyph, &report),
                Err(e) => self.note = format!("font-ml: {e}"),
            }
        }
    }

    /// What happens when font-ml comes back: the proposal layer is
    /// adopted from disk, and a single glyph is installed at once.
    fn task_finished(&mut self, task: &str, glyph: Option<usize>, report: &serde_json::Value) {
        let source = self.font.source().to_path_buf();
        let summary = match self.adopt_proposal_from_disk(task, &source) {
            Ok(s) => s,
            Err(e) => {
                self.note = format!("font-ml: {e}");
                return;
            }
        };
        match glyph {
            Some(index) => {
                let name = self.font.glyphs.get(index).map(|g| g.name.clone());
                let moved = report.get("moved").and_then(|v| v.as_u64()).unwrap_or(0);
                let points = report.get("points").and_then(|v| v.as_u64()).unwrap_or(0);
                let advance = report
                    .get("advance_delta")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                self.install_proposal(task, name.clone().map(|n| vec![n]));
                self.note = format!(
                    "{task} on {}: {moved}/{points} points moved, advance {advance:+}. \
                     Undo install to reject.",
                    name.unwrap_or_default()
                );
            }
            None => {
                self.note = format!(
                    "{} glyphs proposed ({} keep structure). Install or discard in the panel.",
                    summary.glyphs.len(),
                    summary.compatible.len()
                );
                self.refresh_proposals();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
