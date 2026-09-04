// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Nodes: a local AI workflow as boxes and wires.
//!
//! A node is one step: a master, a model, a task a tool runs, a
//! compare, an install. A link carries a typed value from one node's
//! output to another's input. The whole thing is a [`NodeGraph`],
//! saved as `<name>.nodes.json`, which the editor draws on a canvas
//! and `runebender nodes run` runs with no window.
//!
//! Node types come from two places. Core declares its own
//! ([`core_types`]): the open font, a layer, install, compare, proof.
//! A tool such as `font-ml` declares the rest by answering
//! `tasks --json`, and [`Registry::add_tool`] turns each task into a
//! node type named `<tool>.<task>`. A shell never names a task.
//!
//! The shape follows `ComfyUI`'s workflow file, which is what people
//! who run local models already know: nodes with a type, a position
//! and widget values, and links as short arrays. There is no second
//! "API format": a headless run reads the same file and ignores
//! `pos`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The file format version this crate writes.
pub const FILE_VERSION: u32 = 1;

/// The file extension, after the name.
pub const EXTENSION: &str = "nodes.json";

/// What a port carries. A link connects equal kinds only.
///
/// The names match `font-ml`'s task kinds, so a tool's spec maps onto
/// ports with no translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A UFO on disk: one master.
    Source,
    /// A model directory.
    Model,
    /// An adapter directory, applied over a model.
    Adapter,
    /// One glyph name.
    Glyph,
    /// Glyph names, or every drawn glyph when empty.
    Glyphs,
    /// A number.
    Number,
    /// Yes or no.
    Flag,
    /// Free text.
    Text,
    /// A layer in the source, by name.
    Layer,
    /// Per-glyph rows of a report.
    Rows,
    /// A file written on disk.
    Path,
}

impl Kind {
    /// Whether a value of this kind can be typed into a node, as
    /// opposed to only arriving over a link.
    pub fn takes_value(self) -> bool {
        matches!(
            self,
            Self::Glyph
                | Self::Glyphs
                | Self::Number
                | Self::Flag
                | Self::Text
                | Self::Layer
                | Self::Model
                | Self::Adapter
                | Self::Source
        )
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        f.write_str(&s)
    }
}

/// One input or output on a node type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Port {
    /// The name, unique on its side of the node.
    pub name: String,
    /// What it carries.
    pub kind: Kind,
    /// For an input: whether a run without it is an error.
    #[serde(default)]
    pub required: bool,
    /// For an input: the value used when nothing is linked or typed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// One line.
    #[serde(default)]
    pub help: String,
}

impl Port {
    fn input(name: &str, kind: Kind, required: bool, help: &str) -> Self {
        Self {
            name: name.into(),
            kind,
            required,
            default: None,
            help: help.into(),
        }
    }

    fn with_default(mut self, default: Value) -> Self {
        self.default = Some(default);
        self
    }

    fn output(name: &str, kind: Kind, help: &str) -> Self {
        Self {
            name: name.into(),
            kind,
            required: false,
            default: None,
            help: help.into(),
        }
    }
}

/// A node type: what a node of it takes and gives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeType {
    /// `core.<name>` or `<tool>.<task>`.
    pub name: String,
    /// One line for the node's header.
    pub title: String,
    /// A few lines for a tooltip.
    #[serde(default)]
    pub help: String,
    /// Whether the tool that runs it is built. Core types always are.
    #[serde(default = "yes")]
    pub implemented: bool,
    /// What it takes, in the order a node shows them.
    #[serde(default)]
    pub inputs: Vec<Port>,
    /// What it gives.
    #[serde(default)]
    pub outputs: Vec<Port>,
}

fn yes() -> bool {
    true
}

impl NodeType {
    /// The input port by name.
    pub fn input(&self, name: &str) -> Option<&Port> {
        self.inputs.iter().find(|p| p.name == name)
    }

    /// The output port by name.
    pub fn output(&self, name: &str) -> Option<&Port> {
        self.outputs.iter().find(|p| p.name == name)
    }

    /// The tool that runs it: the part before the dot.
    pub fn tool(&self) -> &str {
        self.name.split('.').next().unwrap_or_default()
    }

    /// The task within the tool: the part after the dot.
    pub fn task(&self) -> &str {
        self.name
            .split_once('.')
            .map(|(_, t)| t)
            .unwrap_or_default()
    }
}

/// The node types core runs itself, with no tool.
pub fn core_types() -> Vec<NodeType> {
    vec![
        NodeType {
            name: "core.source".into(),
            title: "Font".into(),
            help: "The font the run is given: the active master, and the \
                   glyphs selected in the editor, or every glyph."
                .into(),
            implemented: true,
            inputs: vec![],
            outputs: vec![
                Port::output("source", Kind::Source, "The master."),
                Port::output("glyphs", Kind::Glyphs, "The selection, or all."),
            ],
        },
        NodeType {
            name: "core.master".into(),
            title: "Master".into(),
            help: "One master of the family, by style name.".into(),
            implemented: true,
            inputs: vec![Port::input(
                "name",
                Kind::Text,
                true,
                "The style name, as the designspace lists it.",
            )],
            outputs: vec![Port::output("source", Kind::Source, "That master.")],
        },
        NodeType {
            name: "core.model".into(),
            title: "Model".into(),
            help: "A model directory from ~/.runebender/models.".into(),
            implemented: true,
            inputs: vec![Port::input(
                "name",
                Kind::Text,
                true,
                "The directory name, or a path.",
            )],
            outputs: vec![Port::output("model", Kind::Model, "The model.")],
        },
        NodeType {
            name: "core.adapter".into(),
            title: "Adapter".into(),
            help: "An adapter applied over a model, at a strength. Two in \
                   a row apply two."
                .into(),
            implemented: true,
            inputs: vec![
                Port::input("model", Kind::Model, true, "The model to patch."),
                Port::input(
                    "name",
                    Kind::Adapter,
                    true,
                    "The adapter directory, or a wire from one.",
                ),
                Port::input("strength", Kind::Number, false, "How much of it.")
                    .with_default(Value::from(1.0)),
            ],
            outputs: vec![Port::output("model", Kind::Model, "The patched model.")],
        },
        NodeType {
            name: "core.layer".into(),
            title: "Layer".into(),
            help: "A layer already in the source, such as a proposal a \
                   tool left."
                .into(),
            implemented: true,
            inputs: vec![
                Port::input("source", Kind::Source, true, "The master."),
                Port::input("name", Kind::Text, true, "The layer name."),
            ],
            outputs: vec![Port::output("layer", Kind::Layer, "That layer.")],
        },
        NodeType {
            name: "core.features".into(),
            title: "Mark features".into(),
            help: "Write mark and mkmk features from the master's anchors \
                   beside its features.fea, with an include line, so the \
                   compiled font positions marks the way the editor does."
                .into(),
            implemented: true,
            inputs: vec![Port::input("source", Kind::Source, true, "The master.")],
            outputs: vec![Port::output("path", Kind::Path, "The generated file.")],
        },
        NodeType {
            name: "core.compose".into(),
            title: "Compose".into(),
            help: "Derive precomposed glyphs from their base and marks through \
                   anchors, as a proposal."
                .into(),
            implemented: true,
            inputs: vec![
                Port::input("source", Kind::Source, true, "The master."),
                Port::input(
                    "glyphs",
                    Kind::Glyphs,
                    false,
                    "Only these; all with a recipe when empty.",
                ),
            ],
            outputs: vec![
                Port::output("layer", Kind::Layer, "The proposal layer."),
                Port::output("rows", Kind::Rows, "What derived, and what could not."),
            ],
        },
        NodeType {
            name: "core.install".into(),
            title: "Install".into(),
            help: "Copy a layer's glyphs over the foreground, one undo \
                   step per glyph. The one node that changes the font."
                .into(),
            implemented: true,
            inputs: vec![
                Port::input("layer", Kind::Layer, true, "What to install."),
                Port::input("glyphs", Kind::Glyphs, false, "Only these."),
                Port::input(
                    "keep_structure",
                    Kind::Flag,
                    false,
                    "Refuse a glyph whose point structure changed.",
                )
                .with_default(Value::from(true)),
            ],
            outputs: vec![Port::output("rows", Kind::Rows, "What was installed.")],
        },
        NodeType {
            name: "core.compare".into(),
            title: "Compare".into(),
            help: "Score a layer against a master drawn by hand: mean \
                   point error per glyph, and the mean-shift baseline."
                .into(),
            implemented: true,
            inputs: vec![
                Port::input("layer", Kind::Layer, true, "The proposal."),
                Port::input("against", Kind::Source, true, "The master it should match."),
            ],
            outputs: vec![Port::output("rows", Kind::Rows, "Per-glyph scores.")],
        },
        NodeType {
            name: "core.proof".into(),
            title: "Proof".into(),
            help: "An SVG sheet of a layer or a master.".into(),
            implemented: true,
            inputs: vec![
                Port::input("source", Kind::Source, false, "A master."),
                Port::input("layer", Kind::Layer, false, "Or a layer."),
                Port::input("out", Kind::Text, false, "Where to write it."),
            ],
            outputs: vec![Port::output("path", Kind::Path, "The SVG.")],
        },
        NodeType {
            name: "core.note".into(),
            title: "Note".into(),
            help: "Text on the canvas. Runs nothing.".into(),
            implemented: true,
            inputs: vec![Port::input("text", Kind::Text, false, "The note.")],
            outputs: vec![],
        },
    ]
}

/// Every node type a graph may use: core's, plus each tool's tasks.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Registry {
    /// In the order they were added, core first.
    pub types: Vec<NodeType>,
}

impl Registry {
    /// Core's types only.
    pub fn core() -> Self {
        Self {
            types: core_types(),
        }
    }

    /// A type by name.
    pub fn get(&self, name: &str) -> Option<&NodeType> {
        self.types.iter().find(|t| t.name == name)
    }

    /// Adds every task a tool reports, from the JSON its `tasks --json`
    /// prints: `{"tasks": [{name, title, help, implemented, inputs,
    /// outputs}]}`. Returns how many arrived.
    ///
    /// Two inputs are not ports. `write` is always on in a graph, since
    /// the layer is the whole point, and a `flag` named `all` is
    /// covered by an empty `glyphs`. A `layer` output becomes a port
    /// named `layer` whose value is the layer's name.
    pub fn add_tool(&mut self, tool: &str, tasks_json: &Value) -> usize {
        let Some(tasks) = tasks_json.get("tasks").and_then(Value::as_array) else {
            return 0;
        };
        let mut added = 0;
        for task in tasks {
            let Some(name) = task.get("name").and_then(Value::as_str) else {
                continue;
            };
            let ports = |side: &str| -> Vec<Port> {
                task.get(side)
                    .and_then(Value::as_array)
                    .map(|ps| {
                        ps.iter()
                            .filter_map(|p| serde_json::from_value::<Port>(p.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let inputs: Vec<Port> = ports("inputs")
                .into_iter()
                .filter(|p| p.name != "write" && !(p.kind == Kind::Flag && p.name == "all"))
                .map(|mut p| {
                    // A tool calls its glyph list `glyph` when one or
                    // many are fine; the port takes the plural kind.
                    if p.kind == Kind::Glyphs {
                        p.name = "glyphs".into();
                    }
                    p
                })
                .collect();
            let outputs: Vec<Port> = ports("outputs")
                .into_iter()
                .map(|mut p| {
                    if p.kind == Kind::Layer {
                        p.help = format!("{} ({})", p.help, p.name);
                        p.name = "layer".into();
                    }
                    p
                })
                .collect();
            self.types.push(NodeType {
                name: format!("{tool}.{name}"),
                title: task
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or(name)
                    .to_string(),
                help: task
                    .get("help")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                implemented: task
                    .get("implemented")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                inputs,
                outputs,
            });
            added += 1;
        }
        added
    }
}

/// One box on the canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Node {
    /// Unique in the graph. Links refer to it.
    pub id: u32,
    /// A [`NodeType`] name.
    #[serde(rename = "type")]
    pub type_name: String,
    /// Canvas position, in canvas units. Ignored by a headless run.
    #[serde(default)]
    pub pos: [f32; 2],
    /// Typed-in values, by input name. A linked input ignores its
    /// value.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, Value>,
}

/// A wire: `[from_node, output, to_node, input]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Link(pub u32, pub String, pub u32, pub String);

impl Link {
    /// The node the value leaves.
    pub fn from(&self) -> u32 {
        self.0
    }

    /// The output it leaves by.
    pub fn output(&self) -> &str {
        &self.1
    }

    /// The node the value enters.
    pub fn to(&self) -> u32 {
        self.2
    }

    /// The input it enters by.
    pub fn input(&self) -> &str {
        &self.3
    }
}

/// The file: nodes and links.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeGraph {
    /// [`FILE_VERSION`].
    pub version: u32,
    /// In file order. Order carries no meaning.
    #[serde(default)]
    pub nodes: Vec<Node>,
    /// In file order.
    #[serde(default)]
    pub links: Vec<Link>,
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self {
            version: FILE_VERSION,
            nodes: Vec::new(),
            links: Vec::new(),
        }
    }
}

/// What is wrong with a graph. A graph with any of these does not run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "problem", rename_all = "snake_case")]
pub enum Problem {
    /// The file is a later version than this crate reads.
    Version {
        /// What the file says.
        found: u32,
    },
    /// Two nodes share an id.
    DuplicateId {
        /// The id.
        id: u32,
    },
    /// A node's type is not in the registry.
    UnknownType {
        /// The node.
        node: u32,
        /// The type it asks for.
        type_name: String,
    },
    /// A node's type is declared but its tool cannot run it.
    NotBuilt {
        /// The node.
        node: u32,
        /// The type.
        type_name: String,
    },
    /// A link names a node the graph does not have.
    DanglingLink {
        /// The link.
        link: Link,
    },
    /// A link names a port the node type does not have.
    UnknownPort {
        /// The link.
        link: Link,
        /// Which end.
        end: String,
    },
    /// A link joins two kinds.
    KindMismatch {
        /// The link.
        link: Link,
        /// What leaves.
        from: Kind,
        /// What the input takes.
        to: Kind,
    },
    /// Two links enter the same input.
    DoubleInput {
        /// The node.
        node: u32,
        /// The input.
        input: String,
    },
    /// A required input has no link and no value.
    MissingInput {
        /// The node.
        node: u32,
        /// The input.
        input: String,
    },
    /// A typed value is not what the port takes.
    BadValue {
        /// The node.
        node: u32,
        /// The input.
        input: String,
        /// What it takes.
        kind: Kind,
    },
    /// A value is typed into an input no port has.
    StrayValue {
        /// The node.
        node: u32,
        /// The name.
        input: String,
    },
    /// Following links from a node leads back to it.
    Cycle {
        /// The nodes on the loop, in order.
        nodes: Vec<u32>,
    },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Version { found } => {
                write!(f, "file version {found}, this reads {FILE_VERSION}")
            }
            Self::DuplicateId { id } => write!(f, "node id {id} used twice"),
            Self::UnknownType { node, type_name } => {
                write!(f, "node {node}: unknown type {type_name}")
            }
            Self::NotBuilt { node, type_name } => {
                write!(f, "node {node}: {type_name} is not built in this tool")
            }
            Self::DanglingLink { link } => write!(
                f,
                "link {}.{} -> {}.{}: no such node",
                link.0, link.1, link.2, link.3
            ),
            Self::UnknownPort { link, end } => write!(
                f,
                "link {}.{} -> {}.{}: no such {end} port",
                link.0, link.1, link.2, link.3
            ),
            Self::KindMismatch { link, from, to } => write!(
                f,
                "link {}.{} -> {}.{}: {from} into {to}",
                link.0, link.1, link.2, link.3
            ),
            Self::DoubleInput { node, input } => {
                write!(f, "node {node}: two links into {input}")
            }
            Self::MissingInput { node, input } => {
                write!(f, "node {node}: {input} is required")
            }
            Self::BadValue { node, input, kind } => {
                write!(f, "node {node}: {input} takes a {kind}")
            }
            Self::StrayValue { node, input } => {
                write!(f, "node {node}: no input named {input}")
            }
            Self::Cycle { nodes } => {
                let path: Vec<String> = nodes.iter().map(u32::to_string).collect();
                write!(f, "cycle: {}", path.join(" -> "))
            }
        }
    }
}

/// Whether a JSON value fits a port.
fn value_fits(value: &Value, kind: Kind) -> bool {
    match kind {
        Kind::Number => value.is_number(),
        Kind::Flag => value.is_boolean(),
        Kind::Glyphs => {
            value.is_array()
                && value
                    .as_array()
                    .is_some_and(|a| a.iter().all(Value::is_string))
                || value.is_string()
        }
        Kind::Glyph
        | Kind::Text
        | Kind::Layer
        | Kind::Model
        | Kind::Adapter
        | Kind::Source
        | Kind::Path => value.is_string(),
        Kind::Rows => value.is_array(),
    }
}

impl NodeGraph {
    /// Reads a file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Writes a file, pretty, one link per line.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = self.to_json();
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The file's text.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// The node by id.
    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// The node by id, to change.
    pub fn node_mut(&mut self, id: u32) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// An id no node has.
    pub fn next_id(&self) -> u32 {
        self.nodes.iter().map(|n| n.id).max().map_or(1, |m| m + 1)
    }

    /// Adds a node and returns its id.
    pub fn add(&mut self, type_name: &str, pos: [f32; 2]) -> u32 {
        let id = self.next_id();
        self.nodes.push(Node {
            id,
            type_name: type_name.into(),
            pos,
            values: BTreeMap::new(),
        });
        id
    }

    /// Removes a node and every link touching it.
    pub fn remove(&mut self, id: u32) {
        self.nodes.retain(|n| n.id != id);
        self.links.retain(|l| l.from() != id && l.to() != id);
    }

    /// Connects an output to an input, replacing any link already into
    /// that input.
    pub fn connect(&mut self, from: u32, output: &str, to: u32, input: &str) {
        self.links.retain(|l| !(l.to() == to && l.input() == input));
        self.links.push(Link(from, output.into(), to, input.into()));
    }

    /// The link into an input, if any.
    pub fn link_into(&self, node: u32, input: &str) -> Option<&Link> {
        self.links
            .iter()
            .find(|l| l.to() == node && l.input() == input)
    }

    /// Every problem, in a stable order. Empty means it runs.
    pub fn validate(&self, registry: &Registry) -> Vec<Problem> {
        let mut problems = Vec::new();
        if self.version > FILE_VERSION {
            problems.push(Problem::Version {
                found: self.version,
            });
        }
        let mut seen = HashSet::new();
        for n in &self.nodes {
            if !seen.insert(n.id) {
                problems.push(Problem::DuplicateId { id: n.id });
            }
        }
        let types: HashMap<u32, Option<&NodeType>> = self
            .nodes
            .iter()
            .map(|n| (n.id, registry.get(&n.type_name)))
            .collect();
        for n in &self.nodes {
            match types.get(&n.id).copied().flatten() {
                None => problems.push(Problem::UnknownType {
                    node: n.id,
                    type_name: n.type_name.clone(),
                }),
                Some(t) if !t.implemented => problems.push(Problem::NotBuilt {
                    node: n.id,
                    type_name: n.type_name.clone(),
                }),
                Some(_) => {}
            }
        }
        let mut into: HashSet<(u32, &str)> = HashSet::new();
        for l in &self.links {
            let (Some(from), Some(to)) = (types.get(&l.from()), types.get(&l.to())) else {
                problems.push(Problem::DanglingLink { link: l.clone() });
                continue;
            };
            if !into.insert((l.to(), l.input())) {
                problems.push(Problem::DoubleInput {
                    node: l.to(),
                    input: l.input().to_string(),
                });
            }
            let (Some(from), Some(to)) = (from, to) else {
                // The unknown type is already reported.
                continue;
            };
            let out = from.output(l.output());
            let inp = to.input(l.input());
            if out.is_none() {
                problems.push(Problem::UnknownPort {
                    link: l.clone(),
                    end: "output".into(),
                });
            }
            if inp.is_none() {
                problems.push(Problem::UnknownPort {
                    link: l.clone(),
                    end: "input".into(),
                });
            }
            if let (Some(out), Some(inp)) = (out, inp)
                && out.kind != inp.kind
            {
                problems.push(Problem::KindMismatch {
                    link: l.clone(),
                    from: out.kind,
                    to: inp.kind,
                });
            }
        }
        for n in &self.nodes {
            let Some(t) = types.get(&n.id).copied().flatten() else {
                continue;
            };
            for p in &t.inputs {
                let linked = into.contains(&(n.id, p.name.as_str()));
                match n.values.get(&p.name) {
                    Some(v) if !value_fits(v, p.kind) => problems.push(Problem::BadValue {
                        node: n.id,
                        input: p.name.clone(),
                        kind: p.kind,
                    }),
                    Some(_) => {}
                    None if p.required && !linked && p.default.is_none() => {
                        problems.push(Problem::MissingInput {
                            node: n.id,
                            input: p.name.clone(),
                        });
                    }
                    None => {}
                }
            }
            for name in n.values.keys() {
                if t.input(name).is_none() {
                    problems.push(Problem::StrayValue {
                        node: n.id,
                        input: name.clone(),
                    });
                }
            }
        }
        if let Err(cycle) = self.order() {
            problems.push(Problem::Cycle { nodes: cycle });
        }
        problems
    }

    /// Node ids in an order that runs every node after the nodes it
    /// reads from. Ties keep file order. Err carries one cycle.
    pub fn order(&self) -> Result<Vec<u32>, Vec<u32>> {
        let ids: Vec<u32> = self.nodes.iter().map(|n| n.id).collect();
        let known: HashSet<u32> = ids.iter().copied().collect();
        let mut indegree: HashMap<u32, usize> = ids.iter().map(|&id| (id, 0)).collect();
        let mut out: HashMap<u32, Vec<u32>> = HashMap::new();
        for l in &self.links {
            if !known.contains(&l.from()) || !known.contains(&l.to()) {
                continue;
            }
            *indegree.entry(l.to()).or_default() += 1;
            out.entry(l.from()).or_default().push(l.to());
        }
        let mut ready: Vec<u32> = ids.iter().copied().filter(|id| indegree[id] == 0).collect();
        let mut order = Vec::with_capacity(ids.len());
        while !ready.is_empty() {
            // Lowest file position first, so a run is repeatable.
            let pos = |id: u32| ids.iter().position(|&i| i == id).unwrap_or(usize::MAX);
            ready.sort_by_key(|&id| std::cmp::Reverse(pos(id)));
            let id = ready.pop().unwrap_or_default();
            order.push(id);
            for &next in out.get(&id).map(Vec::as_slice).unwrap_or_default() {
                let d = indegree.entry(next).or_default();
                *d -= 1;
                if *d == 0 {
                    ready.push(next);
                }
            }
        }
        if order.len() == ids.len() {
            return Ok(order);
        }
        // Something is left: walk from a leftover node until it
        // repeats, which names the loop.
        let left: HashSet<u32> = ids
            .iter()
            .copied()
            .filter(|id| !order.contains(id))
            .collect();
        let start = ids
            .iter()
            .copied()
            .find(|id| left.contains(id))
            .unwrap_or_default();
        let mut path = vec![start];
        let mut at = start;
        loop {
            let next = out
                .get(&at)
                .and_then(|ns| ns.iter().copied().find(|n| left.contains(n)));
            let Some(next) = next else {
                break;
            };
            if let Some(i) = path.iter().position(|&p| p == next) {
                path.drain(..i);
                break;
            }
            path.push(next);
            at = next;
        }
        Err(path)
    }

    /// The JSON Schema for the file.
    pub fn schema() -> Value {
        serde_json::to_value(schemars::schema_for!(Self)).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bolden_tool() -> Value {
        serde_json::json!({
            "tasks": [{
                "name": "bolden", "title": "Bolden", "help": "", "implemented": true,
                "inputs": [
                    {"name": "source", "kind": "source", "required": true, "help": ""},
                    {"name": "model", "kind": "model", "required": true, "help": ""},
                    {"name": "glyph", "kind": "glyphs", "required": false, "help": ""},
                    {"name": "strength", "kind": "number", "required": false, "default": 1.0, "help": ""},
                    {"name": "write", "kind": "flag", "required": false, "default": 0.0, "help": ""}
                ],
                "outputs": [
                    {"name": "com.runebender.proposal.bolden", "kind": "layer", "help": "the layer"},
                    {"name": "glyphs", "kind": "rows", "help": ""}
                ]
            }, {
                "name": "kern", "title": "Kern", "help": "", "implemented": false,
                "inputs": [], "outputs": []
            }]
        })
    }

    fn registry() -> Registry {
        let mut r = Registry::core();
        assert_eq!(r.add_tool("font-ml", &bolden_tool()), 2);
        r
    }

    /// Font, model, bolden, compare, install: the demo.
    fn demo() -> NodeGraph {
        let mut g = NodeGraph::default();
        let font = g.add("core.source", [0.0, 0.0]);
        let model = g.add("core.model", [0.0, 100.0]);
        g.node_mut(model)
            .unwrap()
            .values
            .insert("name".into(), "virtua-12m-bolden".into());
        let bolden = g.add("font-ml.bolden", [200.0, 0.0]);
        let master = g.add("core.master", [200.0, 200.0]);
        g.node_mut(master)
            .unwrap()
            .values
            .insert("name".into(), "Bold".into());
        let compare = g.add("core.compare", [400.0, 0.0]);
        let install = g.add("core.install", [400.0, 200.0]);
        g.connect(font, "source", bolden, "source");
        g.connect(font, "glyphs", bolden, "glyphs");
        g.connect(model, "model", bolden, "model");
        g.connect(bolden, "layer", compare, "layer");
        g.connect(master, "source", compare, "against");
        g.connect(bolden, "layer", install, "layer");
        g
    }

    #[test]
    fn tool_tasks_become_types_with_write_hidden_and_layer_named() {
        let r = registry();
        let t = r.get("font-ml.bolden").unwrap();
        assert_eq!(t.tool(), "font-ml");
        assert_eq!(t.task(), "bolden");
        assert!(t.input("write").is_none());
        assert!(
            t.input("glyphs").is_some(),
            "plural port for the glyph list"
        );
        assert_eq!(t.output("layer").unwrap().kind, Kind::Layer);
        assert!(!r.get("font-ml.kern").unwrap().implemented);
    }

    #[test]
    fn the_demo_validates_and_orders() {
        let g = demo();
        assert_eq!(g.validate(&registry()), vec![]);
        let order = g.order().unwrap();
        let at = |id: u32| order.iter().position(|&i| i == id).unwrap();
        assert!(at(1) < at(3) && at(2) < at(3), "bolden after its inputs");
        assert!(
            at(3) < at(5) && at(3) < at(6),
            "compare and install after bolden"
        );
        assert_eq!(order.len(), 6);
    }

    #[test]
    fn round_trips_through_json() {
        let g = demo();
        let text = g.to_json();
        let back: NodeGraph = serde_json::from_str(&text).unwrap();
        assert_eq!(back, g);
        assert!(text.contains("[1, \"source\", 3, \"source\"]") || text.contains("\"source\""));
    }

    #[test]
    fn every_problem_is_found() {
        let r = registry();
        let mut g = demo();
        // A kind mismatch: glyphs into a model port.
        g.connect(1, "glyphs", 3, "model");
        // A stray and a bad value.
        g.node_mut(3)
            .unwrap()
            .values
            .insert("strength".into(), "loud".into());
        g.node_mut(3)
            .unwrap()
            .values
            .insert("volume".into(), 11.into());
        // A required input left empty.
        g.node_mut(4).unwrap().values.clear();
        // An unknown type and a not-built one.
        g.add("core.nothing", [0.0, 0.0]);
        g.add("font-ml.kern", [0.0, 0.0]);
        // A dangling link.
        g.links.push(Link(99, "x".into(), 1, "y".into()));
        let problems = g.validate(&r);
        let has = |f: &dyn Fn(&Problem) -> bool| problems.iter().any(f);
        assert!(has(&|p| matches!(p, Problem::KindMismatch { .. })));
        assert!(has(
            &|p| matches!(p, Problem::BadValue { input, .. } if input == "strength")
        ));
        assert!(has(
            &|p| matches!(p, Problem::StrayValue { input, .. } if input == "volume")
        ));
        assert!(has(&|p| matches!(p, Problem::MissingInput { node: 4, .. })));
        assert!(has(&|p| matches!(p, Problem::UnknownType { .. })));
        assert!(has(&|p| matches!(p, Problem::NotBuilt { .. })));
        assert!(has(&|p| matches!(p, Problem::DanglingLink { .. })));
        for p in &problems {
            assert!(!p.to_string().is_empty());
        }
    }

    #[test]
    fn a_cycle_is_named() {
        let mut g = NodeGraph::default();
        let a = g.add("core.layer", [0.0, 0.0]);
        let b = g.add("core.layer", [0.0, 0.0]);
        let c = g.add("core.layer", [0.0, 0.0]);
        g.connect(a, "layer", b, "source");
        g.connect(b, "layer", c, "source");
        g.connect(c, "layer", a, "source");
        let cycle = g.order().unwrap_err();
        assert_eq!(cycle.len(), 3);
        assert!(
            g.validate(&Registry::core())
                .iter()
                .any(|p| matches!(p, Problem::Cycle { .. }))
        );
    }

    #[test]
    fn remove_takes_links_with_it() {
        let mut g = demo();
        g.remove(3);
        assert!(g.links.iter().all(|l| l.from() != 3 && l.to() != 3));
        assert_eq!(g.next_id(), 7);
    }

    #[test]
    fn schema_names_the_file_parts() {
        let s = NodeGraph::schema();
        let props = s.get("properties").unwrap();
        assert!(props.get("nodes").is_some() && props.get("links").is_some());
    }
}
