// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Running a [`NodeGraph`]: each node in order, skipping what has not
//! changed.
//!
//! Core nodes run here. Tool nodes run the tool as a subprocess, the
//! way `runebender propose` does, so the model runtime never enters
//! this crate. A node's inputs are hashed before it runs: the values
//! typed into it, the hashes of the nodes it reads from, and for a
//! font or a layer the glyph files themselves. The hash and the
//! outputs are kept in a cache file beside the graph, and a node
//! whose hash has not moved is skipped. That is `ComfyUI`'s rule, with
//! the cache on disk instead of in a server.
//!
//! Progress goes to a callback as [`Event`]s, so a shell shows them
//! per node and the command line prints them as lines.

use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::io::BufRead as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::document::nodes::{Kind, NodeGraph, NodeType, Registry};
use crate::document::project::{Master, Project};
use crate::document::proposal;

/// A value on a wire, after a node ran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RunValue {
    /// A UFO on disk.
    Source {
        /// Its path.
        path: PathBuf,
    },
    /// A model directory.
    Model {
        /// Its path.
        path: PathBuf,
    },
    /// Glyph names; empty means every drawn glyph.
    Glyphs {
        /// The names.
        names: Vec<String>,
    },
    /// A number.
    Number {
        /// The value.
        value: f64,
    },
    /// Yes or no.
    Flag {
        /// The value.
        value: bool,
    },
    /// Text.
    Text {
        /// The value.
        value: String,
    },
    /// A layer in a UFO.
    Layer {
        /// The UFO.
        source: PathBuf,
        /// The layer name.
        name: String,
    },
    /// Rows of a report.
    Rows {
        /// The rows.
        rows: Vec<Value>,
    },
    /// A file written.
    Path {
        /// Its path.
        path: PathBuf,
    },
}

impl RunValue {
    /// What kind of port carries this.
    pub fn kind(&self) -> Kind {
        match self {
            Self::Source { .. } => Kind::Source,
            Self::Model { .. } => Kind::Model,
            Self::Glyphs { .. } => Kind::Glyphs,
            Self::Number { .. } => Kind::Number,
            Self::Flag { .. } => Kind::Flag,
            Self::Text { .. } => Kind::Text,
            Self::Layer { .. } => Kind::Layer,
            Self::Rows { .. } => Kind::Rows,
            Self::Path { .. } => Kind::Path,
        }
    }

    /// The text of a text value.
    fn text(&self) -> Option<&str> {
        match self {
            Self::Text { value } => Some(value),
            _ => None,
        }
    }

    /// Something to hash that stands for the value. A font or a layer
    /// hashes its glyph files, so an edit upstream re-runs what read
    /// it. A model hashes its manifest's weight digest when it has
    /// one.
    fn fingerprint(&self, hasher: &mut impl Hasher, glyphs: &[String]) {
        match self {
            Self::Source { path } => {
                path.hash(hasher);
                hash_layer_files(&path.join("glyphs"), glyphs, hasher);
            }
            Self::Layer { source, name } => {
                source.hash(hasher);
                name.hash(hasher);
                if let Some(dir) = layer_dir(source, name) {
                    hash_layer_files(&dir, glyphs, hasher);
                }
            }
            Self::Model { path } => {
                path.hash(hasher);
                let manifest = std::fs::read_to_string(path.join("manifest.json"))
                    .ok()
                    .and_then(|t| serde_json::from_str::<Value>(&t).ok());
                match manifest.and_then(|m| m.get("weights_sha256")?.as_str().map(String::from)) {
                    Some(digest) => digest.hash(hasher),
                    None => hash_file(&path.join("weights.safetensors"), hasher),
                }
            }
            Self::Glyphs { names } => names.hash(hasher),
            Self::Number { value } => value.to_bits().hash(hasher),
            Self::Flag { value } => value.hash(hasher),
            Self::Text { value } => value.hash(hasher),
            Self::Rows { rows } => rows.len().hash(hasher),
            Self::Path { path } => {
                path.hash(hasher);
                hash_file(path, hasher);
            }
        }
    }
}

/// The directory a UFO layer's glif files live in, from layercontents.
fn layer_dir(source: &Path, layer: &str) -> Option<PathBuf> {
    let text = std::fs::read_to_string(source.join("layercontents.plist")).ok()?;
    let value: plist::Value = plist::from_bytes(text.as_bytes()).ok()?;
    let pairs = value.as_array()?;
    for pair in pairs {
        let pair = pair.as_array()?;
        if pair.first()?.as_string()? == layer {
            return Some(source.join(pair.get(1)?.as_string()?));
        }
    }
    None
}

/// Hashes the glif files in a layer directory: the named glyphs, or
/// every file when the list is empty. Names and lengths go in, then
/// the bytes.
fn hash_layer_files(dir: &Path, glyphs: &[String], hasher: &mut impl Hasher) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "glif"))
        .collect();
    files.sort();
    let contents = std::fs::read_to_string(dir.join("contents.plist")).unwrap_or_default();
    let wanted: Vec<String> = glyphs
        .iter()
        .filter_map(|g| {
            // contents.plist maps names to file names; find the file
            // for each wanted glyph the cheap way.
            let key = format!("<key>{g}</key>");
            let at = contents.find(&key)?;
            let rest = &contents[at + key.len()..];
            let start = rest.find("<string>")? + "<string>".len();
            let end = rest[start..].find("</string>")? + start;
            Some(rest[start..end].to_string())
        })
        .collect();
    for file in files {
        if !glyphs.is_empty() {
            let name = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if !wanted.iter().any(|w| w == name) {
                continue;
            }
        }
        file.file_name().hash(hasher);
        hash_file(&file, hasher);
    }
}

fn hash_file(path: &Path, hasher: &mut impl Hasher) {
    if let Ok(bytes) = std::fs::read(path) {
        bytes.len().hash(hasher);
        bytes.hash(hasher);
    }
}

/// How a node ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Ran, and gave its outputs.
    Ran,
    /// Nothing it reads changed since the cached run; outputs reused.
    Skipped,
    /// Ran and failed; nothing downstream ran.
    Failed,
    /// Not run because a node it reads from failed.
    Blocked,
}

/// What one node did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeResult {
    /// The node.
    pub id: u32,
    /// Its type.
    #[serde(rename = "type")]
    pub type_name: String,
    /// How it ended.
    pub status: Status,
    /// The input hash it ran with, as hex.
    pub hash: String,
    /// Its outputs by port.
    #[serde(default)]
    pub outputs: BTreeMap<String, RunValue>,
    /// What it said: a tool's JSON, a compare's rows, an error.
    #[serde(default)]
    pub report: Value,
    /// How long it took.
    pub seconds: f64,
}

/// The whole run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunReport {
    /// Whether every node ran or was skipped.
    pub ok: bool,
    /// In run order.
    pub nodes: Vec<NodeResult>,
}

/// What the cache file beside the graph holds: the last result per
/// node, by id.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Cache {
    nodes: BTreeMap<u32, NodeResult>,
}

/// The cache file for a graph file.
pub fn cache_path(graph: &Path) -> PathBuf {
    let name = graph
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("nodes");
    graph.with_file_name(format!(".{name}.cache"))
}

/// A step the runner reports.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A node is about to run.
    Start {
        /// Which.
        id: u32,
        /// Its type.
        type_name: String,
        /// Its place, from 1, of how many.
        index: usize,
        /// How many nodes the run has.
        total: usize,
    },
    /// A tool reported progress inside a node.
    Progress {
        /// The node.
        id: u32,
        /// Done so far.
        done: usize,
        /// Of.
        total: usize,
        /// What it is on.
        label: String,
    },
    /// A node ended.
    End {
        /// Which.
        id: u32,
        /// How.
        status: Status,
        /// How long.
        seconds: f64,
        /// An error, when it failed.
        error: Option<String>,
    },
}

/// What a run is given.
pub struct RunContext<'a> {
    /// The designspace or UFO the Font node stands for.
    pub font: &'a Path,
    /// The master the Font node gives, by style name. The first when
    /// None.
    pub master: Option<&'a str>,
    /// The glyphs the Font node gives. Empty means all.
    pub glyphs: Vec<String>,
    /// Where `<tool>` binaries are: name to path. `font-ml` is looked
    /// up here first, then on PATH.
    pub tools: BTreeMap<String, PathBuf>,
    /// Where models live, for a Model node given a bare name.
    pub models_dir: Option<PathBuf>,
    /// Run every node, cache or not.
    pub force: bool,
    /// Where the cache lives. None keeps nothing.
    pub cache: Option<PathBuf>,
    /// Hears every step.
    pub on_event: &'a mut dyn FnMut(Event),
}

impl fmt::Debug for RunContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunContext")
            .field("font", &self.font)
            .field("master", &self.master)
            .field("glyphs", &self.glyphs.len())
            .field("tools", &self.tools)
            .field("models_dir", &self.models_dir)
            .field("force", &self.force)
            .field("cache", &self.cache)
            .finish_non_exhaustive()
    }
}

/// A node's inputs, resolved: what came over a link or was typed.
struct Inputs {
    values: BTreeMap<String, RunValue>,
}

impl Inputs {
    fn get(&self, name: &str) -> Option<&RunValue> {
        self.values.get(name)
    }

    fn text(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(RunValue::text)
    }

    fn flag(&self, name: &str) -> Option<bool> {
        match self.get(name)? {
            RunValue::Flag { value } => Some(*value),
            _ => None,
        }
    }

    fn glyphs(&self, name: &str) -> Vec<String> {
        match self.get(name) {
            Some(RunValue::Glyphs { names }) => names.clone(),
            _ => Vec::new(),
        }
    }

    fn source(&self, name: &str) -> Option<&Path> {
        match self.get(name)? {
            RunValue::Source { path } => Some(path),
            _ => None,
        }
    }

    fn layer(&self, name: &str) -> Option<(&Path, &str)> {
        match self.get(name)? {
            RunValue::Layer { source, name } => Some((source, name)),
            _ => None,
        }
    }
}

/// A typed-in JSON value as a wire value, by the port's kind.
fn value_of(kind: Kind, value: &Value, models_dir: Option<&Path>) -> Option<RunValue> {
    Some(match kind {
        Kind::Number => RunValue::Number {
            value: value.as_f64()?,
        },
        Kind::Flag => RunValue::Flag {
            value: value.as_bool()?,
        },
        Kind::Text | Kind::Glyph => RunValue::Text {
            value: value.as_str()?.to_string(),
        },
        Kind::Glyphs => RunValue::Glyphs {
            names: match value {
                Value::String(s) => s
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect(),
                Value::Array(a) => a
                    .iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect(),
                _ => return None,
            },
        },
        Kind::Source => RunValue::Source {
            path: PathBuf::from(value.as_str()?),
        },
        Kind::Model | Kind::Adapter => RunValue::Model {
            path: resolve_model(value.as_str()?, models_dir),
        },
        Kind::Layer => return None,
        Kind::Rows => RunValue::Rows {
            rows: value.as_array()?.clone(),
        },
        Kind::Path => RunValue::Path {
            path: PathBuf::from(value.as_str()?),
        },
    })
}

/// A model name as a path: a path that exists as it is, else a
/// directory under the models directory.
fn resolve_model(name: &str, models_dir: Option<&Path>) -> PathBuf {
    let direct = PathBuf::from(name);
    if direct.is_dir() {
        return direct;
    }
    match models_dir {
        Some(dir) => dir.join(name),
        None => direct,
    }
}

/// The models directory: `$RUNEBENDER_MODELS`, else
/// `~/.runebender/models`.
pub fn default_models_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("RUNEBENDER_MODELS").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".runebender").join("models"))
}

/// Runs the graph. Validate first; this assumes the graph is sound
/// and reports a problem it meets as a failed node.
pub fn run(graph: &NodeGraph, registry: &Registry, ctx: &mut RunContext<'_>) -> RunReport {
    let mut cache: Cache = ctx
        .cache
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let order = graph.order().unwrap_or_default();
    let total = order.len();
    let mut results: BTreeMap<u32, NodeResult> = BTreeMap::new();
    let mut ok = true;
    for (index, id) in order.iter().copied().enumerate() {
        let Some(node) = graph.node(id) else {
            continue;
        };
        let started = Instant::now();
        let Some(node_type) = registry.get(&node.type_name) else {
            let result = failed(node.id, &node.type_name, "unknown type", 0.0);
            (ctx.on_event)(Event::End {
                id,
                status: Status::Failed,
                seconds: 0.0,
                error: Some("unknown type".into()),
            });
            results.insert(id, result);
            ok = false;
            continue;
        };
        (ctx.on_event)(Event::Start {
            id,
            type_name: node.type_name.clone(),
            index: index + 1,
            total,
        });

        // Gather inputs: links first, then typed values, then defaults.
        let mut values = BTreeMap::new();
        let mut blocked = false;
        let mut hasher = std::hash::DefaultHasher::new();
        node.type_name.hash(&mut hasher);
        for port in &node_type.inputs {
            let value = if let Some(link) = graph.link_into(id, &port.name) {
                match results.get(&link.from()) {
                    Some(up) if matches!(up.status, Status::Ran | Status::Skipped) => {
                        up.hash.hash(&mut hasher);
                        up.outputs.get(link.output()).cloned()
                    }
                    _ => {
                        blocked = true;
                        None
                    }
                }
            } else if let Some(v) = node.values.get(&port.name) {
                value_of(port.kind, v, ctx.models_dir.as_deref())
            } else if let Some(d) = &port.default {
                value_of(port.kind, d, ctx.models_dir.as_deref())
            } else {
                None
            };
            if let Some(v) = value {
                port.name.hash(&mut hasher);
                v.fingerprint(&mut hasher, &ctx.glyphs);
                values.insert(port.name.clone(), v);
            }
        }
        if node.type_name == "core.source" {
            ctx.font.hash(&mut hasher);
            ctx.master.hash(&mut hasher);
            ctx.glyphs.hash(&mut hasher);
            hash_layer_files(
                &source_master(ctx).unwrap_or_default().join("glyphs"),
                &ctx.glyphs,
                &mut hasher,
            );
        }
        if blocked {
            let result = NodeResult {
                id,
                type_name: node.type_name.clone(),
                status: Status::Blocked,
                hash: String::new(),
                outputs: BTreeMap::new(),
                report: Value::Null,
                seconds: 0.0,
            };
            (ctx.on_event)(Event::End {
                id,
                status: Status::Blocked,
                seconds: 0.0,
                error: None,
            });
            results.insert(id, result);
            ok = false;
            continue;
        }
        let hash = format!("{:016x}", hasher.finish());

        // The cache answers when nothing moved. Install is never
        // skipped: it changes the font, and the cache does not know
        // what the designer did to the foreground since.
        let cached = cache.nodes.get(&id);
        if !ctx.force
            && node.type_name != "core.install"
            && let Some(c) = cached
            && c.hash == hash
            && c.status == Status::Ran
            && outputs_still_there(&c.outputs)
        {
            let mut result = c.clone();
            result.status = Status::Skipped;
            result.seconds = 0.0;
            (ctx.on_event)(Event::End {
                id,
                status: Status::Skipped,
                seconds: 0.0,
                error: None,
            });
            results.insert(id, result);
            continue;
        }

        let inputs = Inputs { values };
        let outcome = run_node(node.id, node_type, &inputs, ctx);
        let seconds = started.elapsed().as_secs_f64();
        let result = match outcome {
            Ok((outputs, report)) => NodeResult {
                id,
                type_name: node.type_name.clone(),
                status: Status::Ran,
                hash: hash.clone(),
                outputs,
                report,
                seconds,
            },
            Err(e) => {
                ok = false;
                failed(id, &node.type_name, &e, seconds)
            }
        };
        (ctx.on_event)(Event::End {
            id,
            status: result.status,
            seconds,
            error: match &result.status {
                Status::Failed => result
                    .report
                    .get("error")
                    .and_then(Value::as_str)
                    .map(String::from),
                _ => None,
            },
        });
        if result.status == Status::Ran {
            let mut stored = result.clone();
            stored.status = Status::Ran;
            cache.nodes.insert(id, stored);
        } else {
            cache.nodes.remove(&id);
        }
        results.insert(id, result);
    }
    if let Some(path) = &ctx.cache
        && let Ok(text) = serde_json::to_string(&cache)
    {
        // A cache that cannot be written is a slower next run, not
        // a failed one.
        let _ = std::fs::write(path, text);
    }
    RunReport {
        ok,
        nodes: order.iter().filter_map(|id| results.remove(id)).collect(),
    }
}

/// Whether a cached node's outputs still exist on disk, so a layer a
/// designer discarded is not handed on as if it were there.
fn outputs_still_there(outputs: &BTreeMap<String, RunValue>) -> bool {
    outputs.values().all(|v| match v {
        RunValue::Layer { source, name } => layer_dir(source, name).is_some_and(|d| d.is_dir()),
        RunValue::Source { path } | RunValue::Model { path } | RunValue::Path { path } => {
            path.exists()
        }
        _ => true,
    })
}

fn failed(id: u32, type_name: &str, error: &str, seconds: f64) -> NodeResult {
    NodeResult {
        id,
        type_name: type_name.to_string(),
        status: Status::Failed,
        hash: String::new(),
        outputs: BTreeMap::new(),
        report: json!({ "error": error }),
        seconds,
    }
}

/// The UFO the Font node stands for.
fn source_master(ctx: &RunContext<'_>) -> Result<PathBuf, String> {
    if ctx.font.extension().is_some_and(|x| x == "ufo") {
        return Ok(ctx.font.to_path_buf());
    }
    let project = Project::load(ctx.font)?;
    master_path(&project, ctx.master)
}

/// A master's UFO path by style name, or the first.
fn master_path(project: &Project, name: Option<&str>) -> Result<PathBuf, String> {
    let index = match name {
        None => 0,
        Some(n) => project
            .master_names
            .iter()
            .position(|m| m.as_ref() == n)
            .ok_or_else(|| {
                let names: Vec<&str> = project.master_names.iter().map(AsRef::as_ref).collect();
                format!("no master named {n}; the family has: {}", names.join(", "))
            })?,
    };
    project
        .masters
        .get(index)
        .map(|m| m.source_path.clone())
        .ok_or_else(|| "the family has no master".to_string())
}

type Outputs = BTreeMap<String, RunValue>;

/// Runs one node.
fn run_node(
    id: u32,
    node_type: &NodeType,
    inputs: &Inputs,
    ctx: &mut RunContext<'_>,
) -> Result<(Outputs, Value), String> {
    let mut out = Outputs::new();
    match node_type.name.as_str() {
        "core.source" => {
            let path = source_master(ctx)?;
            out.insert("source".into(), RunValue::Source { path: path.clone() });
            out.insert(
                "glyphs".into(),
                RunValue::Glyphs {
                    names: ctx.glyphs.clone(),
                },
            );
            Ok((out, json!({ "source": path, "glyphs": ctx.glyphs.len() })))
        }
        "core.master" => {
            let name = inputs.text("name").ok_or("name is required")?;
            let project = Project::load(ctx.font)?;
            let path = master_path(&project, Some(name))?;
            out.insert("source".into(), RunValue::Source { path: path.clone() });
            Ok((out, json!({ "source": path })))
        }
        "core.model" => {
            let path = match inputs.get("name") {
                Some(RunValue::Model { path }) => path.clone(),
                Some(RunValue::Text { value }) => resolve_model(value, ctx.models_dir.as_deref()),
                _ => return Err("name is required".into()),
            };
            if !path.join("config.json").is_file() {
                return Err(format!("{}: not a model directory", path.display()));
            }
            out.insert("model".into(), RunValue::Model { path: path.clone() });
            Ok((out, json!({ "model": path })))
        }
        "core.adapter" => Err("adapters are not built yet".into()),
        "core.layer" => {
            let source = inputs.source("source").ok_or("source is required")?;
            let name = inputs.text("name").ok_or("name is required")?;
            if layer_dir(source, name).is_none() {
                return Err(format!("{}: no layer named {name}", source.display()));
            }
            out.insert(
                "layer".into(),
                RunValue::Layer {
                    source: source.to_path_buf(),
                    name: name.to_string(),
                },
            );
            Ok((out, json!({ "layer": name })))
        }
        "core.install" => {
            let (source, layer) = inputs.layer("layer").ok_or("layer is required")?;
            let task = proposal::task_of_layer(layer)
                .ok_or_else(|| format!("{layer} is not a proposal layer; install takes one"))?;
            let only = inputs.glyphs("glyphs");
            let keep = inputs.flag("keep_structure").unwrap_or(true);
            let mut master =
                Master::load(source).map_err(|e| format!("{}: {e}", source.display()))?;
            let done = master
                .install_proposal(task, (!only.is_empty()).then_some(only.as_slice()), keep)
                .map_err(|e| e.to_string())?;
            master
                .save()
                .map_err(|e| format!("{}: {e}", source.display()))?;
            let rows: Vec<Value> = done
                .installed
                .iter()
                .map(|g| json!({ "glyph": g, "installed": true }))
                .chain(
                    done.skipped
                        .iter()
                        .map(|(g, why)| json!({ "glyph": g, "installed": false, "why": why })),
                )
                .collect();
            out.insert("rows".into(), RunValue::Rows { rows });
            Ok((out, serde_json::to_value(&done).unwrap_or_default()))
        }
        "core.compare" => {
            let (source, layer) = inputs.layer("layer").ok_or("layer is required")?;
            let against = inputs.source("against").ok_or("against is required")?;
            let rows = compare_layer(source, layer, against)?;
            let n = rows.iter().filter(|r| r.get("model").is_some()).count();
            let mean = |key: &str| {
                let sum: f64 = rows.iter().filter_map(|r| r.get(key)?.as_f64()).sum();
                if n == 0 { 0.0 } else { sum / n as f64 }
            };
            let wins = rows
                .iter()
                .filter(|r| r.get("better").and_then(Value::as_bool) == Some(true))
                .count();
            let summary = json!({
                "glyphs": n,
                "model": mean("model"),
                "unchanged": mean("unchanged"),
                "wins": wins,
            });
            out.insert("rows".into(), RunValue::Rows { rows });
            Ok((out, summary))
        }
        "core.proof" => {
            let (path, layer) = match (inputs.layer("layer"), inputs.source("source")) {
                (Some((s, l)), _) => (s, Some(l)),
                (None, Some(s)) => (s, None),
                (None, None) => return Err("source or layer is required".into()),
            };
            let master = Master::load(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let names: Vec<String> = match layer {
                Some(l) => {
                    let layer = master
                        .font
                        .layers
                        .get(l)
                        .ok_or_else(|| format!("no layer named {l}"))?;
                    layer.iter().map(|g| g.name().to_string()).collect()
                }
                None => master
                    .glyphs
                    .iter()
                    .filter(|g| !g.path.is_empty())
                    .map(|g| g.name.to_string())
                    .collect(),
            };
            let sheet = crate::formats::svg::proof_sheet(&master, layer, &names, 8)?;
            let out_path = match inputs.text("out") {
                Some(o) => PathBuf::from(o),
                None => path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(format!("proof-{id}.svg")),
            };
            std::fs::write(&out_path, sheet.svg)
                .map_err(|e| format!("{}: {e}", out_path.display()))?;
            out.insert(
                "path".into(),
                RunValue::Path {
                    path: out_path.clone(),
                },
            );
            Ok((out, json!({ "path": out_path, "glyphs": names.len() })))
        }
        "core.note" => Ok((out, Value::Null)),
        _ => run_tool_node(id, node_type, inputs, ctx),
    }
}

/// Runs a tool's task as a subprocess: `<tool> run <task> --<input>
/// <value>... --write --json`, forwarding its progress lines.
fn run_tool_node(
    id: u32,
    node_type: &NodeType,
    inputs: &Inputs,
    ctx: &mut RunContext<'_>,
) -> Result<(Outputs, Value), String> {
    let tool = node_type.tool();
    let task = node_type.task();
    let binary = ctx
        .tools
        .get(tool)
        .cloned()
        .or_else(|| find_on_path(tool))
        .ok_or_else(|| format!("{tool} is not installed"))?;
    let mut cmd = std::process::Command::new(&binary);
    cmd.arg("run").arg(task);
    let mut source: Option<PathBuf> = None;
    for port in &node_type.inputs {
        let Some(value) = inputs.get(&port.name) else {
            continue;
        };
        match value {
            RunValue::Source { path } => {
                if port.name == "source" {
                    source = Some(path.clone());
                }
                cmd.arg(format!("--{}", port.name)).arg(path);
            }
            RunValue::Model { path } | RunValue::Path { path } => {
                cmd.arg(format!("--{}", port.name)).arg(path);
            }
            RunValue::Glyphs { names } => {
                if names.is_empty() {
                    cmd.arg("--all");
                } else {
                    for g in names {
                        cmd.arg("--glyph").arg(g);
                    }
                }
            }
            RunValue::Number { value } => {
                cmd.arg(format!("--{}", port.name)).arg(format!("{value}"));
            }
            RunValue::Flag { value } => {
                if *value {
                    cmd.arg(format!("--{}", port.name));
                }
            }
            RunValue::Text { value } => {
                cmd.arg(format!("--{}", port.name)).arg(value);
            }
            RunValue::Layer { name, .. } => {
                cmd.arg(format!("--{}", port.name)).arg(name);
            }
            RunValue::Rows { .. } => {}
        }
    }
    cmd.arg("--write").arg("--json");
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not run {}: {e}", binary.display()))?;
    let stderr = child.stderr.take();
    let stdout = child.stdout.take();
    // stderr carries progress; read it as it comes. stdout is the
    // report, read after.
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(out) = stdout {
            let mut out = out;
            let _ = std::io::Read::read_to_string(&mut out, &mut text);
        }
        text
    });
    let mut err_lines = Vec::new();
    if let Some(err) = stderr {
        for line in std::io::BufReader::new(err).lines().map_while(Result::ok) {
            if let Some((done, total, label)) = parse_progress(&line) {
                (ctx.on_event)(Event::Progress {
                    id,
                    done,
                    total,
                    label: label.to_string(),
                });
            } else {
                err_lines.push(line);
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let stdout = reader.join().unwrap_or_default();
    let report: Value = stdout
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str(l).ok())
        .unwrap_or_else(|| json!({ "raw": stdout.trim() }));
    if !status.success() {
        let message = report
            .get("error")
            .and_then(Value::as_str)
            .map(String::from)
            .unwrap_or_else(|| err_lines.join("\n"));
        return Err(format!(
            "{tool} {task} exited {}: {message}",
            status.code().unwrap_or(-1)
        ));
    }
    let mut out = Outputs::new();
    for port in &node_type.outputs {
        match port.kind {
            Kind::Layer => {
                let Some(source) = source.clone() else {
                    continue;
                };
                // The tool names the layer in its report when it wrote
                // one; the task name is the fallback.
                let name = report
                    .get("proposal")
                    .and_then(|p| p.get("layer"))
                    .and_then(Value::as_str)
                    .map(String::from)
                    .unwrap_or_else(|| proposal::layer_name(task));
                out.insert(port.name.clone(), RunValue::Layer { source, name });
            }
            Kind::Rows => {
                let rows = report
                    .get(&port.name)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                out.insert(port.name.clone(), RunValue::Rows { rows });
            }
            Kind::Path => {
                if let Some(p) = report.get(&port.name).and_then(Value::as_str) {
                    out.insert(
                        port.name.clone(),
                        RunValue::Path {
                            path: PathBuf::from(p),
                        },
                    );
                }
            }
            _ => {}
        }
    }
    Ok((out, report))
}

/// `progress <done>/<total> <label>`, the line font-ml prints.
pub fn parse_progress(line: &str) -> Option<(usize, usize, &str)> {
    let rest = line.strip_prefix("progress ")?;
    let (count, label) = rest.split_once(' ').unwrap_or((rest, ""));
    let (done, total) = count.split_once('/')?;
    Some((done.parse().ok()?, total.parse().ok()?, label.trim()))
}

/// A binary by name on PATH, or `$RUNEBENDER_<NAME>`.
fn find_on_path(tool: &str) -> Option<PathBuf> {
    let var = format!("RUNEBENDER_{}", tool.to_uppercase().replace('-', "_"));
    if let Some(t) = std::env::var_os(&var).filter(|t| !t.is_empty()) {
        return Some(PathBuf::from(t));
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(tool))
        .find(|candidate| candidate.is_file())
}

/// Scores a layer against a master drawn by hand.
///
/// For every glyph in the layer that the other master also has, with
/// the same point structure: the mean distance from each proposed
/// point to the hand-drawn one (`model`), and the same for the
/// foreground the proposal came from (`unchanged`), which is the
/// score of doing nothing. `better` is whether the proposal moved
/// closer.
pub fn compare_layer(source: &Path, layer: &str, against: &Path) -> Result<Vec<Value>, String> {
    let font = norad::Font::load(source).map_err(|e| format!("{}: {e}", source.display()))?;
    let other = norad::Font::load(against).map_err(|e| format!("{}: {e}", against.display()))?;
    let proposed = font
        .layers
        .get(layer)
        .ok_or_else(|| format!("{}: no layer named {layer}", source.display()))?;
    let mut rows = Vec::new();
    for glyph in proposed.iter() {
        let name = glyph.name().as_str();
        let Some(target) = other.get_glyph(name) else {
            rows.push(json!({ "glyph": name, "why": "not in the other master" }));
            continue;
        };
        let before = font.get_glyph(name);
        if !proposal::compatible(glyph, target) {
            rows.push(json!({ "glyph": name, "why": "point structure differs" }));
            continue;
        }
        let model = mean_distance(glyph, target);
        let unchanged = before
            .filter(|b| proposal::compatible(b, target))
            .map(|b| mean_distance(b, target));
        rows.push(json!({
            "glyph": name,
            "points": glyph.contours.iter().map(|c| c.points.len()).sum::<usize>(),
            "model": model,
            "unchanged": unchanged,
            "better": unchanged.map(|u| model < u),
        }));
    }
    Ok(rows)
}

/// Mean distance between matching points of two structure-compatible
/// glyphs.
fn mean_distance(a: &norad::Glyph, b: &norad::Glyph) -> f64 {
    let mut sum = 0.0;
    let mut n = 0_usize;
    for (ca, cb) in a.contours.iter().zip(&b.contours) {
        for (pa, pb) in ca.points.iter().zip(&cb.points) {
            sum += ((pa.x - pb.x).powi(2) + (pa.y - pb.y).powi(2)).sqrt();
            n += 1;
        }
    }
    if n == 0 { 0.0 } else { sum / n as f64 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_lines_parse() {
        assert_eq!(parse_progress("progress 3/40 H"), Some((3, 40, "H")));
        assert_eq!(parse_progress("progress 40/40"), Some((40, 40, "")));
        assert_eq!(parse_progress("wrote layer"), None);
    }

    #[test]
    fn typed_values_take_their_kind() {
        assert!(matches!(
            value_of(Kind::Glyphs, &json!("a, b"), None),
            Some(RunValue::Glyphs { names }) if names == ["a", "b"]
        ));
        assert!(matches!(
            value_of(Kind::Number, &json!(1.5), None),
            Some(RunValue::Number { value }) if value == 1.5
        ));
        assert!(value_of(Kind::Number, &json!("loud"), None).is_none());
        assert!(matches!(
            value_of(Kind::Model, &json!("m"), Some(Path::new("/models"))),
            Some(RunValue::Model { path }) if path == Path::new("/models/m")
        ));
    }

    #[test]
    fn cache_sits_hidden_beside_the_file() {
        let p = cache_path(Path::new("/x/bolden.nodes.json"));
        assert_eq!(p, Path::new("/x/.bolden.nodes.json.cache"));
    }
}
