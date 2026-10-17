// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Graph in the app: the open `.graph.json`, the files beside the
//! font, and a run through the font engine.
//!
//! The font engine owns the file, registry, layout, and runner
//! (`runebender::workflows::graph`, `graph_run` and `ui::graph`).
//! This module finds the files, opens one, validates it, runs it on a thread, and
//! hands the widget what it draws. The widget owns the pan, the
//! selection and the drag, and sends the graph back when it changes.

pub(crate) mod controls;
#[cfg(unix)]
pub(crate) mod execution;
#[cfg(unix)]
mod trace;
#[cfg(unix)]
pub(crate) mod workspace;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use runebender::font::proposal;
use runebender::ui::graph::ImmutablePng;
use runebender::workflows::graph::{NodeGraph, Problem, Registry};
use runebender::workflows::graph_run::{self, Event, RunReport, Status};
use runebender::workflows::process::{self, ProcessCancellation, ProcessLimits, ProcessOutcome};

use crate::application::editor::tools::local_ai::{foreground_is_current, foreground_revisions};
use crate::application::workspace::{Mode, Workspace};

/// How one node looks between runs.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RowState {
    /// Not run yet this session.
    Waiting,
    /// Running; the text is the tool's progress, when it has any.
    Running(Option<String>),
    /// Ended with the engine's status and one line about it.
    Done(Status, Option<String>),
}

/// A file open in the app.
#[derive(Debug, Clone)]
pub(crate) struct OpenGraph {
    /// The file.
    pub(crate) path: PathBuf,
    /// Its contents. Shared with the canvas, which draws the arc it
    /// was given and sends a new graph back on an edit.
    pub(crate) graph: Arc<NodeGraph>,
    /// Every node type the file may use.
    pub(crate) registry: Arc<Registry>,
    /// Node ids in run order, when the file has one.
    pub(crate) order: Vec<u32>,
    /// What stops it running. Empty means it runs.
    pub(crate) problems: Vec<Problem>,
    /// Per node, by id.
    pub(crate) rows: Arc<BTreeMap<u32, RowState>>,
}

/// A run in progress: what the thread has said so far, and its report
/// when it is done. The pump view polls these.
#[derive(Debug, Clone, Default)]
pub(crate) struct NodeJob {
    pub(crate) events: Arc<Mutex<Vec<Event>>>,
    pub(crate) finished: Arc<Mutex<Option<RunReport>>>,
    /// Cancellation is retained even before the background child starts.
    pub(crate) cancellation: ProcessCancellation,
    /// The active master and in-memory document session at launch.
    pub(crate) master_path: PathBuf,
    pub(crate) document_id: u64,
    pub(crate) active_glyph: String,
    pub(crate) foreground_revisions: BTreeMap<String, String>,
    pub(crate) all_glyphs: bool,
}

/// Everything graph-related the app holds.
#[derive(Debug, Default)]
pub(crate) struct GraphState {
    /// Revision of the explicit fit-to-view request.
    pub(crate) fit_request: u64,
    /// The canvas zoom: reported by the canvas, set by the footer slider.
    /// `None` until the canvas first fits, which reads as 1.
    pub(crate) zoom: Option<f64>,
    /// The open file.
    pub(crate) open: Option<OpenGraph>,
    /// The `.graph.json` files found beside the font.
    pub(crate) files: Vec<PathBuf>,
    /// What `font-ml tasks --json` answered, for the registry.
    pub(crate) tasks_json: Option<serde_json::Value>,
    /// Why the installed task registry could not be read.
    pub(crate) tasks_error: Option<String>,
    /// Where font-ml is, or None when it is not installed.
    pub(crate) font_ml: Option<PathBuf>,
    /// Device passed to model-backed nodes. `auto` in the app; real
    /// integration tests select `cpu` for deterministic headless runs.
    pub(crate) device: String,
    /// The run going on, if one is.
    pub(crate) job: Option<NodeJob>,
    /// The selected node, mirrored from the canvas so the strip can
    /// offer its choices.
    pub(crate) selected: Option<u32>,
    /// Show the unsaved native comparison session instead of a disk graph.
    ///
    /// The session itself is Workspace-owned; this is only the selected canvas surface.
    pub(crate) live_selected: bool,
    /// User-visible comma- or whitespace-separated glyph names captured by the next comparison.
    pub(crate) live_scope: String,
    /// Runs started by this native surface; agent-owned handles remain untouched by Clear.
    pub(crate) live_ui_handles: std::collections::BTreeSet<u64>,
    /// Exact proof bytes retained once per immutable artifact identity for presentation.
    pub(crate) proof_images: BTreeMap<String, ImmutablePng>,
    /// Presentation-only inline content sizes keyed by graph node identity.
    pub(crate) content_sizes: BTreeMap<u32, [f32; 2]>,
}

impl Drop for GraphState {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.cancellation.cancel();
        }
    }
}

/// The pump's message: something arrived from the run thread.
#[derive(Debug)]
pub(crate) struct GraphProgress;

/// `bolden.graph.json` as `bolden`.
pub(crate) fn file_label(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.trim_end_matches(".graph.json").to_string())
        .unwrap_or_default()
}

/// Where the font-ml binary is: `$RUNEBENDER_FONT_ML`, then PATH,
/// then `~/.cargo/bin`.
fn font_ml_binary() -> Option<PathBuf> {
    if let Some(t) = std::env::var_os("RUNEBENDER_FONT_ML").filter(|t| !t.is_empty()) {
        return Some(PathBuf::from(t));
    }
    if let Some(found) = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join("font-ml"))
            .find(|c| c.is_file())
    }) {
        return Some(found);
    }
    let home = std::env::var_os("HOME")?;
    let cargo_bin = PathBuf::from(home).join(".cargo/bin/font-ml");
    cargo_bin.is_file().then_some(cargo_bin)
}

fn font_ml_tasks(font_ml: &Path) -> Result<serde_json::Value, String> {
    let output = process::run(
        std::process::Command::new(font_ml).args(["tasks", "--json"]),
        &[],
        ProcessLimits {
            deadline: std::time::Duration::from_secs(2),
            ..ProcessLimits::default()
        },
        &ProcessCancellation::default(),
        |_, _| {},
    );
    if !matches!(output.outcome, ProcessOutcome::Exited { success: true, .. }) {
        let diagnostic = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if diagnostic.is_empty() {
            format!("font-ml tasks --json: {}", output.outcome)
        } else {
            format!("font-ml tasks --json: {}: {diagnostic}", output.outcome)
        });
    }
    parse_tasks_json(&output.stdout)
}

fn parse_tasks_json(bytes: &[u8]) -> Result<serde_json::Value, String> {
    let json: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("font-ml tasks --json returned invalid JSON: {error}"))?;
    if json
        .get("tasks")
        .and_then(serde_json::Value::as_array)
        .is_none()
    {
        return Err("font-ml tasks --json returned no tasks array".into());
    }
    Ok(json)
}

impl Workspace {
    /// Glyphs offered to a graph. An explicit overview multi-selection
    /// wins; in the editor, an otherwise empty selection means only
    /// the open glyph. An empty overview selection retains the graph
    /// runner's documented meaning of every drawn glyph.
    fn node_glyphs(&self) -> Vec<String> {
        let mut indices: Vec<usize> = self.multi_selected.iter().copied().collect();
        if indices.is_empty()
            && let Mode::Editor(index) = self.mode
        {
            indices.push(index);
        }
        indices.sort_unstable();
        indices
            .into_iter()
            .filter_map(|index| self.font.glyphs.get(index))
            .map(|glyph| glyph.name.clone())
            .collect()
    }

    /// Asks font-ml what it can do, once, and finds the files beside
    /// the font. Called when the font opens.
    pub(crate) fn init_graph(&mut self) {
        if self.graph.device.is_empty() {
            self.graph.device =
                std::env::var("RUNEBENDER_AI_DEVICE").unwrap_or_else(|_| "auto".to_string());
        }
        self.graph.font_ml = font_ml_binary();
        let tasks = self.graph.font_ml.as_deref().map(font_ml_tasks).transpose();
        match tasks {
            Ok(tasks_json) => {
                self.graph.tasks_json = tasks_json;
                self.graph.tasks_error = self.graph.font_ml.is_none().then(|| {
                    "font-ml not found. cargo install --git https://github.com/eliheuer/font-ml, \
                     or set RUNEBENDER_FONT_ML"
                        .into()
                });
            }
            Err(error) => {
                self.graph.tasks_json = None;
                self.graph.tasks_error = Some(error);
            }
        }
        self.scan_graph_files();
        if std::env::var_os("RUNEBENDER_NODES_CONTENT_FIXTURE").is_some() {
            self.open_graph_content_fixture();
        }
    }

    /// The registry: built-in engine types plus what font-ml declared.
    pub(crate) fn node_registry(&self) -> Registry {
        let mut registry = Registry::core();
        if let Some(json) = &self.graph.tasks_json {
            registry.add_tool("font-ml", json);
        }
        registry
    }

    /// Open the deterministic code-and-image layout used by headless UI review.
    ///
    /// The image supplied by the view is a fixture, not a computed font proof.
    fn open_graph_content_fixture(&mut self) {
        let mut graph = runebender::workflows::graph_live::comparison_starter(
            runebender::font::variable::SourceId(0),
        );
        if let Some(python) = graph
            .nodes
            .iter_mut()
            .find(|node| node.type_name == "live.python")
        {
            python.values.insert(
                "code".into(),
                serde_json::json!(
                    "for layer in input['layers']:\n    print(layer['glyph'])\n\nresult = input"
                ),
            );
        }
        let registry = self.node_registry();
        let problems = graph.validate(&registry);
        let order = graph.order().unwrap_or_default();
        let rows = graph
            .nodes
            .iter()
            .map(|node| (node.id, RowState::Waiting))
            .collect();
        self.graph.open = Some(OpenGraph {
            path: PathBuf::from("graph-content-fixture.graph.json"),
            graph: Arc::new(graph),
            registry: Arc::new(registry),
            order,
            problems,
            rows: Arc::new(rows),
        });
        self.graph.selected = None;
        self.graph.fit_request = self.graph.fit_request.wrapping_add(1);
        self.mode = Mode::Graph;
        self.note = "Opened Graph content fixture".into();
    }

    /// The directory the font's sources sit in: the parent of the
    /// designspace, or of the one UFO.
    fn font_dir(&self) -> PathBuf {
        self.font
            .source()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }

    /// Every `.graph.json` beside the font: in the sources directory,
    /// its `graphs` subdirectory, and the same two one level up. Sorted.
    pub(crate) fn scan_graph_files(&mut self) {
        let dir = self.font_dir();
        let mut roots = vec![dir.clone(), dir.join("graphs")];
        if let Some(up) = dir.parent() {
            roots.push(up.to_path_buf());
            roots.push(up.join("graphs"));
        }
        let mut found: Vec<PathBuf> = Vec::new();
        for root in roots {
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.ends_with(".graph.json") && !found.contains(&path) {
                    found.push(path);
                }
            }
        }
        found.sort();
        self.graph.files = found;
    }

    /// Opens a file. A file that does not validate still opens, with
    /// its problems listed, so it can be fixed.
    pub(crate) fn open_graph_file(&mut self, path: &Path) {
        match NodeGraph::load(path) {
            Ok(graph) => {
                let registry = self.node_registry();
                let problems = graph.validate(&registry);
                let order = graph.order().unwrap_or_default();
                let rows = graph
                    .nodes
                    .iter()
                    .map(|n| (n.id, RowState::Waiting))
                    .collect();
                let n = problems.len();
                self.graph.open = Some(OpenGraph {
                    path: path.to_path_buf(),
                    graph: Arc::new(graph),
                    registry: Arc::new(registry),
                    order,
                    problems,
                    rows: Arc::new(rows),
                });
                self.graph.live_selected = false;
                self.graph.selected = None;
                self.graph.fit_request = self.graph.fit_request.wrapping_add(1);
                self.note = if n == 0 {
                    format!("Opened {}", file_label(path))
                } else {
                    format!("{}: {n} problems", file_label(path))
                };
            }
            Err(e) => self.note = e,
        }
    }

    /// A new empty file beside the font, named so it does not collide.
    /// Written on Save.
    pub(crate) fn new_graph_file(&mut self) {
        let dir = self.font_dir().join("graphs");
        let mut n = 1;
        let mut path = dir.join("untitled.graph.json");
        while path.exists() || self.graph.open.as_ref().is_some_and(|g| g.path == path) {
            n += 1;
            path = dir.join(format!("untitled-{n}.graph.json"));
        }
        self.graph.open = Some(OpenGraph {
            path,
            graph: Arc::new(NodeGraph::default()),
            registry: Arc::new(self.node_registry()),
            order: Vec::new(),
            problems: Vec::new(),
            rows: Arc::new(BTreeMap::new()),
        });
        self.graph.live_selected = false;
        self.graph.selected = None;
        self.graph.fit_request = self.graph.fit_request.wrapping_add(1);
        self.mode = Mode::Graph;
    }

    /// Opens the canvas. With no file open, the first one beside the
    /// font opens, or a new empty one waits to be saved.
    pub(crate) fn enter_graph_mode(&mut self) {
        if matches!(self.mode, Mode::Editor(_)) {
            self.refresh_open_glyph();
        }
        if self.graph.open.is_none() {
            self.scan_graph_files();
            match self.graph.files.first().cloned() {
                Some(file) => self.open_graph_file(&file),
                None => {
                    self.new_graph_file();
                    return;
                }
            }
        }
        self.mode = Mode::Graph;
    }

    /// Writes the open file.
    pub(crate) fn save_graph_file(&mut self) {
        let Some(state) = self.graph.open.as_ref() else {
            return;
        };
        if let Some(dir) = state.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        self.note = match state.graph.save(&state.path) {
            Ok(()) => format!("Saved {}", file_label(&state.path)),
            Err(e) => e,
        };
        self.scan_graph_files();
    }

    /// The canvas edited the graph: take it, and re-check the file.
    pub(crate) fn graph_changed(&mut self, graph: NodeGraph) {
        let Some(state) = self.graph.open.as_mut() else {
            return;
        };
        state.problems = graph.validate(&state.registry);
        state.order = graph.order().unwrap_or_default();
        let mut rows = (*state.rows).clone();
        let ids: Vec<u32> = graph.nodes.iter().map(|n| n.id).collect();
        rows.retain(|id, _| ids.contains(id));
        for id in ids {
            rows.entry(id).or_insert(RowState::Waiting);
        }
        state.rows = Arc::new(rows);
        state.graph = Arc::new(graph);
    }

    /// Types a value into a node's input and re-checks the file.
    pub(crate) fn graph_set_value(&mut self, id: u32, input: &str, value: serde_json::Value) {
        let Some(state) = self.graph.open.as_ref() else {
            return;
        };
        let mut graph = (*state.graph).clone();
        if let Some(node) = graph.node_mut(id) {
            node.values.insert(input.to_string(), value);
        }
        self.graph_changed(graph);
    }

    /// Runs the open file over the active master and the selection on
    /// a thread. The pump view carries what it says back here.
    pub(crate) fn run_graph(&mut self) {
        if cfg!(target_arch = "wasm32") {
            self.note = "Local AI and workflow execution are available in the desktop app.".into();
            return;
        }
        let Some(state) = self.graph.open.as_ref() else {
            return;
        };
        if state
            .graph
            .nodes
            .iter()
            .any(|n| n.type_name.starts_with("live."))
        {
            self.note = "Select Comparison to run a native live graph".into();
            return;
        }
        if self.graph.job.is_some() {
            self.note = "Already running".into();
            return;
        }
        if let Some(p) = state.problems.first() {
            self.note = p.to_string();
            return;
        }
        let graph = state.graph.clone();
        let registry = state.registry.clone();
        let path = state.path.clone();
        let mut rows = (*state.rows).clone();
        if self.modified && !self.save() {
            return;
        }
        // Give the engine the project source so `core.master` can resolve a
        // sibling designspace master. The job identity remains the
        // active UFO path below.
        let font = self.font.document_source().to_path_buf();
        let master_path = self.font.source().to_path_buf();
        let master = self.font.master_names().get(self.font.active()).cloned();
        let device = self.graph.device.clone();
        let glyphs = self.node_glyphs();
        let foreground_revisions = match foreground_revisions(&self.font, &glyphs) {
            Ok(revisions) => revisions,
            Err(error) => {
                self.note = format!("Cannot capture node target: {error}");
                return;
            }
        };
        let mut tools = BTreeMap::new();
        if let Some(font_ml) = self.graph.font_ml.clone() {
            tools.insert("font-ml".to_string(), font_ml);
        }
        let job = NodeJob {
            master_path,
            document_id: self.document_id,
            active_glyph: self.session.glyph_name.clone(),
            foreground_revisions,
            all_glyphs: glyphs.is_empty(),
            ..NodeJob::default()
        };
        let events = job.events.clone();
        let finished = job.finished.clone();
        let cancellation = job.cancellation.clone();
        for row in rows.values_mut() {
            *row = RowState::Waiting;
        }
        if let Some(s) = self.graph.open.as_mut() {
            s.rows = Arc::new(rows);
        }
        let note = format!("Running {}\u{2026}", file_label(&path));
        std::thread::spawn(move || {
            let mut on_event = |e: Event| {
                let mut events = events.lock().unwrap_or_else(|e| e.into_inner());
                // Only the latest progress per node is useful to the next UI pump.
                if let Event::Progress { id, .. } = &e
                    && let Some(previous) = events.iter_mut().rev().find(|previous| {
                        matches!(previous, Event::Progress { id: previous_id, .. } if previous_id == id)
                    })
                {
                    *previous = e;
                } else {
                    events.push(e);
                }
            };
            let mut ctx = graph_run::RunContext {
                font: &font,
                master: master.as_deref(),
                glyphs,
                tools,
                models_dir: graph_run::default_models_dir(),
                device: Some(device),
                force: false,
                cache: Some(graph_run::cache_path(&path)),
                cancellation,
                process_limits: ProcessLimits::default(),
                on_event: &mut on_event,
            };
            let report = graph_run::run(&graph, &registry, &mut ctx);
            *finished.lock().unwrap_or_else(|e| e.into_inner()) = Some(report);
        });
        self.graph.job = Some(job);
        self.note = note;
    }

    /// The pump woke: apply what the thread said, and finish when it
    /// has.
    pub(crate) fn graph_pump(&mut self) {
        let Some(job) = self.graph.job.clone() else {
            return;
        };
        let batch: Vec<Event> =
            std::mem::take(&mut *job.events.lock().unwrap_or_else(|e| e.into_inner()));
        if let Some(state) = self.graph.open.as_mut()
            && !batch.is_empty()
        {
            let mut rows = (*state.rows).clone();
            for event in batch {
                match event {
                    Event::Start { id, .. } => {
                        rows.insert(id, RowState::Running(None));
                    }
                    Event::Progress {
                        id,
                        done,
                        total,
                        label,
                    } => {
                        rows.insert(
                            id,
                            RowState::Running(Some(format!("{done}/{total} {label}"))),
                        );
                        self.note = format!("{done}/{total} {label}");
                    }
                    Event::End {
                        id,
                        status,
                        seconds,
                        error,
                    } => {
                        let note = match (status, error) {
                            (_, Some(e)) => Some(e),
                            (Status::Ran, None) => Some(format!("{seconds:.1}s")),
                            _ => None,
                        };
                        rows.insert(id, RowState::Done(status, note));
                    }
                }
            }
            state.rows = Arc::new(rows);
        }
        let report = job
            .finished
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(report) = report {
            self.graph_finished(&job, &report);
            self.graph.job = None;
        }
    }

    /// The run ended. Install changes the font on disk, so the font is
    /// re-read when one ran.
    fn graph_finished(&mut self, job: &NodeJob, report: &RunReport) {
        if job.cancellation.is_cancelled() {
            self.note = "Node run cancelled; disk effects are not undone".into();
            return;
        }
        let installed = report
            .nodes
            .iter()
            .any(|n| n.type_name == "core.install" && n.status == Status::Ran);
        let proposal_outputs: std::collections::BTreeSet<_> = report
            .nodes
            .iter()
            .flat_map(|node| node.outputs.values())
            .filter_map(|output| match output {
                graph_run::RunValue::Layer { source, name } => {
                    Some((source.clone(), proposal::task_of_layer(name)?.to_string()))
                }
                _ => None,
            })
            .collect();
        if let Some(state) = self.graph.open.as_mut() {
            let mut rows = (*state.rows).clone();
            for n in &report.nodes {
                let note = match n.status {
                    Status::Failed => n
                        .report
                        .get("error")
                        .and_then(|e| e.as_str())
                        .map(str::to_string),
                    Status::Ran => Some(summary_line(n)),
                    Status::Skipped => Some("unchanged".into()),
                    Status::Blocked => None,
                };
                rows.insert(n.id, RowState::Done(n.status, note));
            }
            state.rows = Arc::new(rows);
        }
        let current = self.document_id == job.document_id
            && self.font.source() == job.master_path
            && self.session.glyph_name == job.active_glyph
            && foreground_is_current(&self.font, &job.foreground_revisions, job.all_glyphs);
        if installed && current {
            self.reload_from_disk();
        }
        let mut reviewed = 0;
        if !installed && current {
            for (source, task) in &proposal_outputs {
                if source != self.font.source() {
                    continue;
                }
                if let Ok(summary) = self.adopt_proposal_from_disk(task, source) {
                    reviewed += 1;
                    if summary
                        .glyphs
                        .iter()
                        .any(|glyph| glyph == &self.session.glyph_name)
                    {
                        self.ai.preview_task = Some(task.clone());
                    }
                }
            }
            self.refresh_proposals();
        }
        let failed = report
            .nodes
            .iter()
            .filter(|n| n.status == Status::Failed)
            .count();
        self.note = if (installed || !proposal_outputs.is_empty()) && !current {
            "Node result is stale after a document, master, glyph, or revision change".into()
        } else if report.ok {
            let ran = report
                .nodes
                .iter()
                .filter(|n| n.status == Status::Ran)
                .count();
            let base = format!("Ran {ran} nodes, {} unchanged", report.nodes.len() - ran);
            if reviewed == 0 {
                base
            } else {
                format!("{base}. Review {reviewed} proposal before installing")
            }
        } else {
            format!("{failed} nodes failed")
        };
    }
}

/// One line for a node that ran: what it gave, from its outputs and
/// report.
fn summary_line(n: &graph_run::NodeResult) -> String {
    use graph_run::RunValue;
    let mut parts: Vec<String> = Vec::new();
    for (name, value) in &n.outputs {
        match value {
            RunValue::Layer { name: layer, .. } => {
                parts.push(
                    layer
                        .trim_start_matches("com.runebender.proposal.")
                        .to_string(),
                );
            }
            RunValue::Rows { rows } => parts.push(format!("{} {name}", rows.len())),
            RunValue::Path { path } => parts.push(file_label(path)),
            RunValue::Glyphs { names } if !names.is_empty() => {
                parts.push(format!("{} glyphs", names.len()));
            }
            _ => {}
        }
    }
    if let (Some(model), Some(shift)) = (
        n.report.get("model").and_then(|v| v.as_f64()),
        n.report.get("shift").and_then(|v| v.as_f64()),
    ) {
        let wins = n.report.get("wins").and_then(|v| v.as_u64()).unwrap_or(0);
        let glyphs = n.report.get("glyphs").and_then(|v| v.as_u64()).unwrap_or(0);
        parts.push(format!(
            "model {model:.1} vs shift {shift:.1}, {wins}/{glyphs} closer"
        ));
    }
    if parts.is_empty() {
        format!("{:.1}s", n.seconds)
    } else {
        parts.join(" \u{00b7} ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_rows_and_node_types_share_the_declared_registry() {
        let json = serde_json::json!({"tasks": [
            {
                "name": "bolden", "title": "Bolden", "implemented": true,
                "inputs": [{"name": "glyph", "kind": "glyphs"}], "outputs": []
            },
            {
                "name": "spacing", "title": "Spacing", "implemented": false,
                "inputs": [{"name": "glyph", "kind": "glyph"}], "outputs": []
            }
        ]});
        let rows: Vec<_> = json["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(crate::application::editor::tools::local_ai::TaskRow::from_value)
            .collect();
        let mut registry = Registry::core();
        assert_eq!(registry.add_tool("font-ml", &json), rows.len());

        for row in rows {
            let node = registry
                .get(&format!("font-ml.{}", row.name))
                .expect("every panel task has a node type");
            assert_eq!(node.title, row.title);
            assert_eq!(node.implemented, row.implemented);
        }
    }

    #[test]
    fn malformed_task_registry_reports_why_it_is_unavailable() {
        assert!(
            parse_tasks_json(b"not json")
                .unwrap_err()
                .contains("invalid JSON")
        );
        assert_eq!(
            parse_tasks_json(br#"{"schema": {}}"#).unwrap_err(),
            "font-ml tasks --json returned no tasks array"
        );
    }

    #[test]
    fn train_adapter_graph_round_trips_without_running_training() {
        let root = std::env::temp_dir().join(format!(
            "runebender-xilem-train-graph-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let font_path = root.join("font.ufo");
        std::fs::create_dir_all(&root).expect("the fixture root is created");
        norad::Font::new()
            .save(&font_path)
            .expect("the empty font fixture saves");
        let mut workspace = Workspace::open(&font_path).expect("the fixture opens");
        workspace.graph.tasks_json = Some(serde_json::json!({"tasks": [{
            "name": "train", "title": "Train bolden", "implemented": true,
            "inputs": [
                {"name":"source", "kind":"source", "required":true, "help":"Light master"},
                {"name":"target", "kind":"source", "required":true, "help":"Bold master"},
                {"name":"out", "kind":"text", "required":true, "help":"Model output"},
                {"name":"steps", "kind":"number", "required":false, "help":"Steps", "default":2000.0},
                {"name":"init", "kind":"model", "required":false, "help":"Base model"},
                {"name":"adapter_out", "kind":"text", "required":false, "help":"Adapter output"}
            ],
            "outputs": [
                {"name":"model", "kind":"model", "help":"Model"},
                {"name":"adapter", "kind":"adapter", "help":"Adapter"}
            ]
        }]}));
        workspace.new_graph_file();
        let path = workspace.graph.open.as_ref().unwrap().path.clone();
        let mut graph = NodeGraph::default();
        let source = graph.add("core.source", [0.0, 0.0]);
        let target = graph.add("core.master", [0.0, 120.0]);
        let model = graph.add("core.model", [0.0, 240.0]);
        let train = graph.add("font-ml.train", [280.0, 80.0]);
        graph
            .node_mut(target)
            .unwrap()
            .values
            .insert("name".into(), "Bold".into());
        graph
            .node_mut(model)
            .unwrap()
            .values
            .insert("name".into(), "virtua-12m-bolden".into());
        let values = &mut graph.node_mut(train).unwrap().values;
        values.insert("out".into(), "choose-disposable-model-output".into());
        values.insert(
            "adapter_out".into(),
            "choose-disposable-adapter-output".into(),
        );
        values.insert("steps".into(), 1.into());
        graph.connect(source, "source", train, "source");
        graph.connect(target, "source", train, "target");
        graph.connect(model, "model", train, "init");
        workspace.graph_changed(graph);
        assert!(workspace.graph.open.as_ref().unwrap().problems.is_empty());

        workspace.graph_set_value(train, "steps", serde_json::json!(12.0));
        workspace.save_graph_file();
        assert!(workspace.graph.files.contains(&path));
        workspace.open_graph_file(&path);
        let reopened = workspace.graph.open.as_ref().expect("the graph reopens");
        assert!(reopened.problems.is_empty());
        assert_eq!(
            reopened
                .graph
                .node(train)
                .and_then(|node| node.values.get("steps")),
            Some(&serde_json::json!(12.0))
        );

        let job = NodeJob {
            master_path: font_path,
            document_id: workspace.document_id,
            active_glyph: workspace.session.glyph_name.clone(),
            foreground_revisions: foreground_revisions(&workspace.font, &[])
                .expect("the empty foreground can be revised"),
            all_glyphs: true,
            ..NodeJob::default()
        };
        let report = RunReport {
            ok: false,
            nodes: vec![graph_run::NodeResult {
                id: train,
                type_name: "font-ml.train".into(),
                status: Status::Failed,
                hash: String::new(),
                outputs: BTreeMap::new(),
                report: serde_json::json!({"error": "training deliberately not run"}),
                seconds: 0.0,
            }],
        };
        workspace.graph_finished(&job, &report);
        assert_eq!(
            workspace.graph.open.as_ref().unwrap().rows.get(&train),
            Some(&RowState::Done(
                Status::Failed,
                Some("training deliberately not run".into())
            ))
        );
        assert_eq!(workspace.note, "1 nodes failed");
        std::fs::remove_dir_all(root).expect("the graph fixture is removed");
    }

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
    fn completed_install_does_not_reload_a_replacement_document() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-graph-session-{}-{}.ufo",
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
        let job = NodeJob {
            master_path: path.clone(),
            document_id: workspace.document_id,
            ..NodeJob::default()
        };
        let report = RunReport {
            ok: true,
            nodes: vec![graph_run::NodeResult {
                id: 1,
                type_name: "core.install".into(),
                status: Status::Ran,
                hash: String::new(),
                outputs: BTreeMap::new(),
                report: serde_json::Value::Null,
                seconds: 0.0,
            }],
        };

        workspace.revert_to_saved();
        workspace.graph_finished(&job, &report);

        assert_eq!(
            workspace.note,
            "Node result is stale after a document, master, glyph, or revision change"
        );
        std::fs::remove_dir_all(path).expect("the empty UFO fixture is removed");
    }

    #[test]
    fn proposal_only_graph_hands_the_result_to_explicit_review() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-node-review-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        let mut original = norad::Glyph::new("A");
        original.width = 500.0;
        font.default_layer_mut().insert_glyph(original.clone());
        font.save(&path).expect("the foreground fixture saves");
        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        let mut proposed = original.clone();
        proposed.width = 620.0;
        runebender::formats::proposal_ufo::write_proposal_layer(
            &mut font,
            "bolden",
            vec![proposed],
        )
        .expect("the proposal is valid");
        font.save(&path)
            .expect("the external proposal fixture saves");
        assert!(workspace.node_glyphs().is_empty());
        workspace.open_glyph(0);
        assert_eq!(workspace.node_glyphs(), vec!["A"]);
        let target_names = workspace.node_glyphs();
        let job = NodeJob {
            master_path: path.clone(),
            document_id: workspace.document_id,
            active_glyph: workspace.session.glyph_name.clone(),
            foreground_revisions: foreground_revisions(&workspace.font, &target_names)
                .expect("the foreground revision is captured"),
            ..NodeJob::default()
        };
        let report = RunReport {
            ok: true,
            nodes: vec![graph_run::NodeResult {
                id: 1,
                type_name: "font-ml.bolden".into(),
                status: Status::Ran,
                hash: String::new(),
                outputs: BTreeMap::from([(
                    "layer".into(),
                    graph_run::RunValue::Layer {
                        source: path.clone(),
                        name: proposal::layer_name("bolden"),
                    },
                )]),
                report: serde_json::Value::Null,
                seconds: 0.0,
            }],
        };

        // Cancellation after worker completion but before the UI pump must not import it.
        job.cancellation.cancel();
        workspace.graph_finished(&job, &report);
        assert!(
            workspace.ai.proposals.is_empty(),
            "cancelled late result imported a proposal"
        );
        assert!(workspace.note.contains("cancelled"));
        let job = NodeJob {
            cancellation: ProcessCancellation::default(),
            ..job
        };
        workspace.graph_finished(&job, &report);

        assert_eq!(
            workspace.font.font_snapshot().get_glyph("A"),
            Some(&original)
        );
        assert_eq!(workspace.ai.proposals.len(), 1);
        assert_eq!(
            workspace.ai.preview_task.as_deref(),
            Some("bolden"),
            "note: {}; rows: {:?}",
            workspace.note,
            workspace
                .graph
                .open
                .as_ref()
                .map(|graph| graph.rows.as_ref())
        );
        assert!(workspace.note.contains("Review 1 proposal"));
        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn completed_install_does_not_reload_over_a_newer_glyph_revision() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-graph-revision-{}-{}.ufo",
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
        let target_names = workspace.node_glyphs();
        let job = NodeJob {
            master_path: path.clone(),
            document_id: workspace.document_id,
            active_glyph: workspace.session.glyph_name.clone(),
            foreground_revisions: foreground_revisions(&workspace.font, &target_names)
                .expect("the foreground revision is captured"),
            ..NodeJob::default()
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
        let report = RunReport {
            ok: true,
            nodes: vec![graph_run::NodeResult {
                id: 1,
                type_name: "core.install".into(),
                status: Status::Ran,
                hash: String::new(),
                outputs: BTreeMap::new(),
                report: serde_json::Value::Null,
                seconds: 0.0,
            }],
        };

        workspace.graph_finished(&job, &report);

        assert_eq!(
            workspace.font.font_snapshot().get_glyph("A").unwrap().width,
            540.0
        );
        assert_eq!(
            workspace.note,
            "Node result is stale after a document, master, glyph, or revision change"
        );
        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    #[ignore = "runs the review-only bolden graph over a disposable Virtua designspace"]
    fn real_review_graph_proposes_selected_glyphs_then_installs_and_undoes() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../virtua-grotesk/sources");
        assert!(
            source.is_dir(),
            "clone Virtua Grotesk beside this repository"
        );
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/graphs/bolden-review.graph.json");
        let root = std::env::temp_dir().join(format!(
            "runebender-xilem-real-graph-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let sources = root.join("sources");
        copy_tree(&source, &sources);
        let graph_path = root.join("graphs/bolden-review.graph.json");
        std::fs::create_dir_all(graph_path.parent().expect("the graph has a parent"))
            .expect("the nodes directory is created");
        std::fs::copy(&fixture, &graph_path).expect("the review graph is copied");

        let designspace = sources.join("VirtuaGrotesk.designspace");
        let mut workspace = Workspace::open(&designspace).expect("the disposable project opens");
        workspace.graph.device = "cpu".into();
        workspace.open_graph_file(&graph_path);
        let graph = workspace.graph.open.as_ref().expect("the graph opens");
        assert!(graph.problems.is_empty());
        assert!(
            graph
                .graph
                .nodes
                .iter()
                .all(|node| node.type_name != "core.install"),
            "the review graph must not install before comparison"
        );
        workspace.graph_set_value(3, "strength", serde_json::json!(1.0));
        workspace.save_graph_file();
        let saved = NodeGraph::load(&graph_path).expect("the edited graph reopens");
        assert_eq!(
            saved.node(3).and_then(|node| node.values.get("strength")),
            Some(&serde_json::json!(1.0))
        );

        let r = workspace.font.index_of("R").expect("Virtua contains R");
        let s = workspace.font.index_of("S").expect("Virtua contains S");
        workspace.open_glyph(r);
        workspace.multi_selected = Arc::new(std::collections::HashSet::from([r, s]));
        assert_eq!(workspace.node_glyphs(), vec!["R", "S"]);
        let original = workspace
            .font
            .font_snapshot()
            .get_glyph("R")
            .expect("the Regular master contains R")
            .clone();
        workspace.run_graph();
        let started = std::time::Instant::now();
        while workspace.graph.job.as_ref().is_some_and(|job| {
            job.finished
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_none()
        }) && started.elapsed().as_secs() < 60
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let events = workspace
            .graph
            .job
            .as_ref()
            .expect("the job remains until the pump adopts it")
            .events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::Start { id: 3, .. }))
        );
        assert!(events.iter().any(|event| matches!(
            event,
            Event::End {
                id: 3,
                status: Status::Ran,
                ..
            }
        )));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::Progress { id: 3, label, .. } if label == "S")),
            "the UI retains the latest progress per node between pumps"
        );
        workspace.graph_pump();

        assert!(workspace.graph.job.is_none(), "the bounded graph finishes");
        assert_eq!(
            workspace.font.font_snapshot().get_glyph("R"),
            Some(&original),
            "the proposal-only graph must not change the foreground"
        );
        assert_eq!(
            workspace.ai.preview_task.as_deref(),
            Some("bolden"),
            "note: {}; rows: {:?}",
            workspace.note,
            workspace
                .graph
                .open
                .as_ref()
                .map(|graph| graph.rows.as_ref())
        );
        assert!(workspace.note.contains("Review 1 proposal"));
        let rows = workspace
            .graph
            .open
            .as_ref()
            .expect("the graph remains open")
            .rows
            .clone();
        assert!(
            rows.values()
                .all(|row| matches!(row, RowState::Done(Status::Ran | Status::Skipped, _)))
        );
        assert!(matches!(
            rows.get(&5),
            Some(RowState::Done(_, Some(summary))) if summary.contains("model")
        ));
        workspace.install_proposal("bolden", Some(vec!["R".into()]));
        assert_ne!(
            workspace.font.font_snapshot().get_glyph("R"),
            Some(&original)
        );
        workspace.undo_install();
        assert_eq!(
            workspace.font.font_snapshot().get_glyph("R"),
            Some(&original)
        );

        std::fs::remove_dir_all(root).expect("the disposable node fixture is removed");
    }
}
