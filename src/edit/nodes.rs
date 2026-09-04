// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Nodes in the app: the open `.nodes.json`, the files beside the
//! font, and a run through core.
//!
//! Core owns the file, the registry, the layout and the runner
//! (`runebender_core::document::nodes`, `nodes_run` and `ui::nodes`).
//! This file is the same seam as `edit/nodes.rs` in the GPUI build:
//! find the files, open one, validate it, run it on a thread, and
//! hand the widget what it draws. The widget owns the pan, the
//! selection and the drag, and sends the graph back when it changes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use runebender_core::document::nodes::{NodeGraph, Problem, Registry};
use runebender_core::document::nodes_run::{self, Event, RunReport, Status};

use crate::{Mode, Workspace};

/// How one node looks between runs.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RowState {
    /// Not run yet this session.
    Waiting,
    /// Running; the text is the tool's progress, when it has any.
    Running(Option<String>),
    /// Ended as core said, with one line about it.
    Done(Status, Option<String>),
}

/// A file open in the app.
#[derive(Debug, Clone)]
pub(crate) struct GraphState {
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
}

/// Everything nodes-related the app holds.
#[derive(Debug, Default)]
pub(crate) struct NodesState {
    /// The open file.
    pub(crate) graph: Option<GraphState>,
    /// The `.nodes.json` files found beside the font.
    pub(crate) files: Vec<PathBuf>,
    /// What `font-ml tasks --json` answered, for the registry.
    pub(crate) tasks_json: Option<serde_json::Value>,
    /// Where font-ml is, or None when it is not installed.
    pub(crate) font_ml: Option<PathBuf>,
    /// The run going on, if one is.
    pub(crate) job: Option<NodeJob>,
    /// The selected node, mirrored from the canvas so the strip can
    /// offer its choices.
    pub(crate) selected: Option<u32>,
}

/// The pump's message: something arrived from the run thread.
#[derive(Debug)]
pub(crate) struct NodesProgress;

/// `bolden.nodes.json` as `bolden`.
pub(crate) fn file_label(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.trim_end_matches(".nodes.json").to_string())
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

impl Workspace {
    /// Asks font-ml what it can do, once, and finds the files beside
    /// the font. Called when the font opens.
    pub(crate) fn init_nodes(&mut self) {
        self.nodes.font_ml = font_ml_binary();
        self.nodes.tasks_json = self.nodes.font_ml.as_ref().and_then(|font_ml| {
            let output = std::process::Command::new(font_ml)
                .arg("tasks")
                .arg("--json")
                .output()
                .ok()?;
            serde_json::from_slice(&output.stdout).ok()
        });
        self.scan_nodes_files();
    }

    /// The registry: core's types plus what font-ml declared.
    pub(crate) fn node_registry(&self) -> Registry {
        let mut registry = Registry::core();
        if let Some(json) = &self.nodes.tasks_json {
            registry.add_tool("font-ml", json);
        }
        registry
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

    /// Every `.nodes.json` beside the font: in the sources directory,
    /// its `nodes` subdirectory, and the same two one level up. Sorted.
    pub(crate) fn scan_nodes_files(&mut self) {
        let dir = self.font_dir();
        let mut roots = vec![dir.clone(), dir.join("nodes")];
        if let Some(up) = dir.parent() {
            roots.push(up.to_path_buf());
            roots.push(up.join("nodes"));
        }
        let mut found: Vec<PathBuf> = Vec::new();
        for root in roots {
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.ends_with(".nodes.json") && !found.contains(&path) {
                    found.push(path);
                }
            }
        }
        found.sort();
        self.nodes.files = found;
    }

    /// Opens a file. A file that does not validate still opens, with
    /// its problems listed, so it can be fixed.
    pub(crate) fn open_nodes_file(&mut self, path: &Path) {
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
                self.nodes.graph = Some(GraphState {
                    path: path.to_path_buf(),
                    graph: Arc::new(graph),
                    registry: Arc::new(registry),
                    order,
                    problems,
                    rows: Arc::new(rows),
                });
                self.nodes.selected = None;
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
    pub(crate) fn new_nodes_file(&mut self) {
        let dir = self.font_dir().join("nodes");
        let mut n = 1;
        let mut path = dir.join("untitled.nodes.json");
        while path.exists() || self.nodes.graph.as_ref().is_some_and(|g| g.path == path) {
            n += 1;
            path = dir.join(format!("untitled-{n}.nodes.json"));
        }
        self.nodes.graph = Some(GraphState {
            path,
            graph: Arc::new(NodeGraph::default()),
            registry: Arc::new(self.node_registry()),
            order: Vec::new(),
            problems: Vec::new(),
            rows: Arc::new(BTreeMap::new()),
        });
        self.nodes.selected = None;
        self.mode = Mode::Nodes;
    }

    /// Opens the canvas. With no file open, the first one beside the
    /// font opens, or a new empty one waits to be saved.
    pub(crate) fn enter_nodes_mode(&mut self) {
        if matches!(self.mode, Mode::Editor(_)) {
            self.refresh_open_glyph();
        }
        if self.nodes.graph.is_none() {
            self.scan_nodes_files();
            match self.nodes.files.first().cloned() {
                Some(file) => self.open_nodes_file(&file),
                None => {
                    self.new_nodes_file();
                    return;
                }
            }
        }
        self.mode = Mode::Nodes;
    }

    /// Writes the open file.
    pub(crate) fn save_nodes_file(&mut self) {
        let Some(state) = self.nodes.graph.as_ref() else {
            return;
        };
        if let Some(dir) = state.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        self.note = match state.graph.save(&state.path) {
            Ok(()) => format!("Saved {}", file_label(&state.path)),
            Err(e) => e,
        };
        self.scan_nodes_files();
    }

    /// The canvas edited the graph: take it, and re-check the file.
    pub(crate) fn nodes_changed(&mut self, graph: NodeGraph) {
        let Some(state) = self.nodes.graph.as_mut() else {
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
    pub(crate) fn nodes_set_value(&mut self, id: u32, input: &str, value: serde_json::Value) {
        let Some(state) = self.nodes.graph.as_ref() else {
            return;
        };
        let mut graph = (*state.graph).clone();
        if let Some(node) = graph.node_mut(id) {
            node.values.insert(input.to_string(), value);
        }
        self.nodes_changed(graph);
    }

    /// Runs the open file over the active master and the selection on
    /// a thread. The pump view carries what it says back here.
    pub(crate) fn run_nodes(&mut self) {
        let Some(state) = self.nodes.graph.as_ref() else {
            return;
        };
        if self.nodes.job.is_some() {
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
        if self.modified {
            self.save();
        }
        let font = self.font.source().to_path_buf();
        let master = self.font.master_names().get(self.font.active()).cloned();
        let glyphs: Vec<String> = self
            .multi_selected
            .iter()
            .filter_map(|&i| self.font.glyphs.get(i))
            .map(|g| g.name.clone())
            .collect();
        let mut tools = BTreeMap::new();
        if let Some(font_ml) = self.nodes.font_ml.clone() {
            tools.insert("font-ml".to_string(), font_ml);
        }
        let job = NodeJob::default();
        let events = job.events.clone();
        let finished = job.finished.clone();
        for row in rows.values_mut() {
            *row = RowState::Waiting;
        }
        if let Some(s) = self.nodes.graph.as_mut() {
            s.rows = Arc::new(rows);
        }
        let note = format!("Running {}\u{2026}", file_label(&path));
        std::thread::spawn(move || {
            let mut on_event = |e: Event| {
                events.lock().unwrap_or_else(|e| e.into_inner()).push(e);
            };
            let mut ctx = nodes_run::RunContext {
                font: &font,
                master: master.as_deref(),
                glyphs,
                tools,
                models_dir: nodes_run::default_models_dir(),
                force: false,
                cache: Some(nodes_run::cache_path(&path)),
                on_event: &mut on_event,
            };
            let report = nodes_run::run(&graph, &registry, &mut ctx);
            *finished.lock().unwrap_or_else(|e| e.into_inner()) = Some(report);
        });
        self.nodes.job = Some(job);
        self.note = note;
    }

    /// The pump woke: apply what the thread said, and finish when it
    /// has.
    pub(crate) fn nodes_pump(&mut self) {
        let Some(job) = self.nodes.job.clone() else {
            return;
        };
        let batch: Vec<Event> =
            std::mem::take(&mut *job.events.lock().unwrap_or_else(|e| e.into_inner()));
        if let Some(state) = self.nodes.graph.as_mut()
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
            self.nodes_finished(&report);
            self.nodes.job = None;
        }
    }

    /// The run ended. Install changes the font on disk, so the font is
    /// re-read when one ran.
    fn nodes_finished(&mut self, report: &RunReport) {
        let installed = report
            .nodes
            .iter()
            .any(|n| n.type_name == "core.install" && n.status == Status::Ran);
        if let Some(state) = self.nodes.graph.as_mut() {
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
        if installed {
            self.reload_from_disk();
        }
        let failed = report
            .nodes
            .iter()
            .filter(|n| n.status == Status::Failed)
            .count();
        self.note = if report.ok {
            let ran = report
                .nodes
                .iter()
                .filter(|n| n.status == Status::Ran)
                .count();
            format!("Ran {ran} nodes, {} unchanged", report.nodes.len() - ran)
        } else {
            format!("{failed} nodes failed")
        };
    }
}

/// One line for a node that ran: what it gave, from its outputs and
/// report.
fn summary_line(n: &nodes_run::NodeResult) -> String {
    use nodes_run::RunValue;
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
