// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The nodes canvas as geometry: where each box, port and wire sits.
//!
//! A shell draws a [`crate::workflows::nodes::NodeGraph`] as boxes and
//! wires. Everything about that picture that is not a pixel lives
//! here: the grid pitch, the box sizes, one row per port, where the
//! dots go, which grid mark colour a port kind or a node type wears,
//! the cubic a wire follows, and what is under a point. A shell adds
//! the paint calls and the mouse, so two shells draw the same graph
//! the same way, and a change to the layout is made once.
//!
//! Canvas units are Y-down, like the file. Core's [`ViewPort`] is
//! Y-up, so [`canvas_affine`] and [`to_canvas`] carry the flip.

use std::collections::BTreeMap;
use std::sync::Arc;

use kurbo::{Affine, BezPath, Point, Rect, Shape as _};
use serde_json::Value;

use crate::ui::editing::viewport::ViewPort;
use crate::workflows::nodes::{Kind, Node, NodeGraph, Registry};

/// The dot grid pitch, in canvas units. Node edges sit on it: the
/// width, the header, the padding and a row are all multiples, so a
/// box's every edge lands on a dot.
pub const GRID: f64 = 16.0;
/// Box width, in canvas units.
pub const NODE_W: f64 = 176.0;
/// Header band height.
pub const HEADER_H: f64 = 24.0;
/// One port row.
pub const ROW_H: f64 = 16.0;
/// Port dot radius.
pub const PORT_R: f64 = 4.5;
/// Inner padding.
pub const PAD: f64 = 8.0;
/// The grid ring radius.
pub const RING_R: f64 = 1.75;
/// The smallest useful inline code editor height.
pub const CODE_H: f64 = 112.0;
/// The smallest useful embedded image height.
pub const IMAGE_H: f64 = 144.0;
/// The corner square used for resizing a content node.
pub const RESIZE_HANDLE: f64 = 12.0;

/// Session-only presentation for content embedded in a graph node.
///
/// The graph file remains responsible for connections and positions.
/// This projection deliberately carries no executable or mutable font handle:
/// an application scheduler publishes immutable script and proof output here,
/// and the canvas only presents it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NodeContentMap {
    /// Content by graph node identity.
    pub by_node: BTreeMap<u32, NodeContent>,
}

impl NodeContentMap {
    /// Returns the content currently projected for `node`.
    pub fn get(&self, node: u32) -> Option<&NodeContent> {
        self.by_node.get(&node)
    }
}

/// Presentation content a canvas node can host.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeContent {
    /// A Python source view backed by the shared script-artifact buffer.
    Script(ScriptContent),
    /// An immutable PNG proof image and its visible run state.
    Image(ImageContent),
}

/// The source and execution state shown inside a script node.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptContent {
    /// The source text from the shared script buffer.
    pub text: String,
    /// Stable content identity captured with the run.
    pub content_hash: String,
    /// Current status and bounded diagnostic for the node.
    pub state: ContentState,
    /// Presentation size in canvas units; it does not affect semantic identity.
    pub size: [f32; 2],
}

/// The image state shown inside a specimen node.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageContent {
    /// The latest accepted image, if a renderer has produced one.
    pub image: Option<ImmutablePng>,
    /// The last accepted image retained while a replacement is running.
    pub previous_image: Option<ImmutablePng>,
    /// Current status and bounded diagnostic for the node.
    pub state: ContentState,
    /// Presentation size in canvas units; it does not affect semantic identity.
    pub size: [f32; 2],
}

/// Immutable proof pixels and the identity that makes them comparable.
#[derive(Debug, Clone, PartialEq)]
pub struct ImmutablePng {
    /// PNG bytes captured from a completed renderer result.
    pub bytes: Arc<[u8]>,
    /// Pixel width recorded by the renderer.
    pub width: u32,
    /// Pixel height recorded by the renderer.
    pub height: u32,
    /// Renderer and input identity visible in the canvas metadata.
    pub output_hash: String,
}

/// A content node's visible lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ContentState {
    /// No run has supplied output yet.
    #[default]
    Idle,
    /// A run is in flight; the previous image, if any, remains visible.
    Running,
    /// The projected output matches the current captured input.
    Current,
    /// The projected output belongs to an older script, parameter, or font capture.
    Stale,
    /// The last run failed or was cancelled; the message is bounded by the scheduler.
    Error(String),
}

/// A canvas coordinate moved to the nearest dot.
pub fn snap(v: f64) -> f64 {
    (v / GRID).round() * GRID
}

/// One port as laid out: where its dot sits, in canvas units.
#[derive(Debug, Clone, PartialEq)]
pub struct PortBox {
    /// The port name.
    pub name: String,
    /// What it carries.
    pub kind: Kind,
    /// The dot's centre.
    pub at: Point,
    /// Which row of the box it sits on, from the top.
    pub row: usize,
    /// A wire is on it.
    pub linked: bool,
    /// What was typed into it, shown beside the name.
    pub value: Option<String>,
}

/// One node as laid out.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeBox {
    /// The node.
    pub id: u32,
    /// The type name, for the header colour.
    pub type_name: String,
    /// The type's title, in the header.
    pub title: String,
    /// The box, in canvas units.
    pub rect: Rect,
    /// Session-only content embedded below the ports, when present.
    pub content: Option<NodeContent>,
    /// Ports down the left edge.
    pub inputs: Vec<PortBox>,
    /// Ports down the right edge.
    pub outputs: Vec<PortBox>,
}

impl NodeBox {
    /// The header band.
    pub fn header(&self) -> Rect {
        Rect::new(
            self.rect.x0,
            self.rect.y0,
            self.rect.x1,
            self.rect.y0 + HEADER_H,
        )
    }

    /// The grid mark the header wears, if the type has one.
    pub fn mark(&self) -> Option<&'static str> {
        type_mark(&self.type_name)
    }

    /// The top of a row's text line, in canvas units.
    pub fn row_top(&self, row: usize) -> f64 {
        self.rect.y0 + HEADER_H + PAD / 2.0 + ROW_H * row as f64
    }

    /// The rectangle reserved for an embedded child widget.
    pub fn content_rect(&self) -> Option<Rect> {
        self.content.as_ref().map(|content| {
            let height = match content {
                NodeContent::Script(content) => f64::from(content.size[1]),
                NodeContent::Image(content) => f64::from(content.size[1]),
            };
            Rect::new(
                self.rect.x0 + PAD,
                self.rect.y1 - height - PAD - RESIZE_HANDLE,
                self.rect.x1 - PAD,
                self.rect.y1 - PAD - RESIZE_HANDLE,
            )
        })
    }

    /// The bottom-right affordance for resizing an embedded node.
    pub fn resize_rect(&self) -> Option<Rect> {
        self.content_rect().map(|_| {
            Rect::new(
                self.rect.x1 - RESIZE_HANDLE,
                self.rect.y1 - RESIZE_HANDLE,
                self.rect.x1,
                self.rect.y1,
            )
        })
    }
}

/// A typed value as the box shows it: a whole number without its
/// `.0`, a string bare.
pub fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{f:.0}"),
            _ => n.to_string(),
        },
        other => other.to_string(),
    }
}

/// Lays out one node: header, one row per port, the dots on the
/// edges. Outputs take the top rows, inputs the rows under them, so a
/// long typed value never runs into an output's name.
pub fn node_box(graph: &NodeGraph, registry: &Registry, node: &Node) -> NodeBox {
    node_box_with_content(graph, registry, node, None)
}

/// Lays out one node with optional session-only embedded content.
pub fn node_box_with_content(
    graph: &NodeGraph,
    registry: &Registry,
    node: &Node,
    content: Option<NodeContent>,
) -> NodeBox {
    let ty = registry.get(&node.type_name);
    let fallback_title = ty
        .map(|t| t.title.clone())
        .unwrap_or_else(|| node.type_name.clone());
    let title = if node.type_name == "live.proof" {
        graph
            .link_into(node.id, "font")
            .and_then(|link| graph.node(link.from()))
            .and_then(|upstream| match upstream.type_name.as_str() {
                "live.font" => Some("Original specimen".into()),
                "live.python" => Some("Scripted specimen".into()),
                _ => None,
            })
            .unwrap_or(fallback_title)
    } else {
        fallback_title
    };
    let inputs: Vec<_> = ty.map(|t| t.inputs.clone()).unwrap_or_default();
    let outputs: Vec<_> = ty.map(|t| t.outputs.clone()).unwrap_or_default();
    let rows = (inputs.len() + outputs.len()).max(1);
    let x = f64::from(node.pos[0]);
    let y = f64::from(node.pos[1]);
    let live = node.type_name.starts_with("live.");
    let content_width = match &content {
        Some(NodeContent::Script(content)) => f64::from(content.size[0]) + PAD * 2.0,
        Some(NodeContent::Image(content)) => f64::from(content.size[0]) + PAD * 2.0,
        None => 0.0,
    };
    let width = if live { LIVE_W } else { NODE_W }.max(content_width);
    let content_height = match &content {
        Some(NodeContent::Script(content)) => {
            f64::from(content.size[1]) + PAD * 2.0 + RESIZE_HANDLE
        }
        Some(NodeContent::Image(content)) => f64::from(content.size[1]) + PAD * 2.0 + RESIZE_HANDLE,
        None => 0.0,
    };
    // The old live-node shell reserved space for canvas action buttons and a
    // proof preview. Embedded children now own that content area, so retaining
    // both allocations leaves a large blank gap before the real editor/image.
    // Keep the legacy reservation only for content-free nodes until those
    // actions have an actual painted surface again.
    let legacy_live_height = if live && content.is_none() {
        ROW_H * 2.0
            + ACTION_H * actions(&node.type_name).len() as f64
            + if node.type_name == "live.proof" {
                PREVIEW_H
            } else {
                0.0
            }
    } else {
        0.0
    };
    let h = HEADER_H + PAD + ROW_H * rows as f64 + legacy_live_height + content_height;
    let rect = Rect::new(x, y, x + width, y + h);
    let row_y = |i: usize| y + HEADER_H + PAD / 2.0 + ROW_H * (i as f64 + 0.5);
    let first_input = outputs.len();
    let inputs = inputs
        .iter()
        .enumerate()
        .map(|(i, p)| PortBox {
            name: p.name.clone(),
            kind: p.kind,
            at: Point::new(x, row_y(first_input + i)),
            row: first_input + i,
            linked: graph.link_into(node.id, &p.name).is_some(),
            value: node.values.get(&p.name).map(value_text),
        })
        .collect();
    let outputs = outputs
        .iter()
        .enumerate()
        .map(|(i, p)| PortBox {
            name: p.name.clone(),
            kind: p.kind,
            at: Point::new(x + width, row_y(i)),
            row: i,
            linked: graph
                .links
                .iter()
                .any(|l| l.from() == node.id && l.output() == p.name),
            value: None,
        })
        .collect();
    NodeBox {
        id: node.id,
        type_name: node.type_name.clone(),
        title,
        rect,
        content,
        inputs,
        outputs,
    }
}

/// Every node laid out, in file order, which is also paint order.
pub fn layout(graph: &NodeGraph, registry: &Registry) -> Vec<NodeBox> {
    layout_with_content(graph, registry, &NodeContentMap::default())
}

/// Lays out every node with the session-only content supplied by a scheduler.
pub fn layout_with_content(
    graph: &NodeGraph,
    registry: &Registry,
    content: &NodeContentMap,
) -> Vec<NodeBox> {
    graph
        .nodes
        .iter()
        .map(|n| node_box_with_content(graph, registry, n, content.get(n.id).cloned()))
        .collect()
}

/// The wires as `(from box, output index, to box, input index)` into
/// a layout, skipping any a box or port is missing for.
pub fn wires(graph: &NodeGraph, boxes: &[NodeBox]) -> Vec<(usize, usize, usize, usize)> {
    let index_of = |id: u32| boxes.iter().position(|b| b.id == id);
    graph
        .links
        .iter()
        .filter_map(|l| {
            let a = index_of(l.from())?;
            let b = index_of(l.to())?;
            let o = boxes[a].outputs.iter().position(|p| p.name == l.output())?;
            let i = boxes[b].inputs.iter().position(|p| p.name == l.input())?;
            Some((a, o, b, i))
        })
        .collect()
}

/// The grid mark colour a port kind carries, so a wire says what it
/// holds the way a cell says its mark. Values typed by hand carry no
/// colour.
pub fn kind_mark(kind: Kind) -> Option<&'static str> {
    Some(match kind {
        Kind::Source | Kind::FontVersion => "green",
        Kind::Layer | Kind::Path => "yellow",
        Kind::Model => "blue",
        Kind::Adapter => "purple",
        Kind::Glyph | Kind::Glyphs => "orange",
        Kind::Rows => "pink",
        Kind::Number | Kind::Flag | Kind::Text | Kind::Parameters => return None,
    })
}

/// The grid mark colour a node type's header carries: what the node
/// mostly gives, or what it does to the font.
pub fn type_mark(type_name: &str) -> Option<&'static str> {
    Some(match type_name {
        "core.source" | "core.master" | "live.font" | "live.fork" => "green",
        "core.layer" | "core.proof" | "live.proof" => "yellow",
        "core.model" => "blue",
        "core.adapter" => "purple",
        "core.install" | "live.apply" => "red",
        "core.compare" => "pink",
        "core.note" => return None,
        _ => "orange",
    })
}

/// A cubic between two ports with horizontal tangents, in canvas
/// units: the shape a node editor reader expects.
pub fn wire_path(a: Point, b: Point) -> BezPath {
    let dx = ((b.x - a.x).abs() * 0.5).clamp(24.0, 120.0);
    let mut path = BezPath::new();
    path.move_to(a);
    path.curve_to(Point::new(a.x + dx, a.y), Point::new(b.x - dx, b.y), b);
    path
}

/// A circle as a path, in canvas units.
pub fn circle(center: Point, r: f64) -> BezPath {
    kurbo::Circle::new(center, r).to_path(0.05)
}

/// The grid rings inside a visible canvas rectangle, as one path.
pub fn grid_rings(visible: Rect) -> BezPath {
    let mut rings = BezPath::new();
    let mut y = (visible.y0 / GRID).floor() * GRID;
    while y <= visible.y1 {
        let mut x = (visible.x0 / GRID).floor() * GRID;
        while x <= visible.x1 {
            rings.extend(circle(Point::new(x, y), RING_R));
            x += GRID;
        }
        y += GRID;
    }
    rings
}

/// Canvas units to local pixels: the viewport's affine after a flip
/// that puts the file's Y-down space into the viewport's Y-up one.
pub fn canvas_affine(vp: &ViewPort) -> Affine {
    vp.affine() * Affine::FLIP_Y
}

/// A local pixel point to canvas units.
pub fn to_canvas(vp: &ViewPort, local: Point) -> Point {
    let d = vp.screen_to_design(local);
    Point::new(d.x, -d.y)
}

/// The canvas rectangle a local pixel rectangle shows.
pub fn visible_canvas(vp: &ViewPort, local: Rect) -> Rect {
    let inverse = canvas_affine(vp).inverse();
    let a = inverse * Point::new(local.x0, local.y0);
    let b = inverse * Point::new(local.x1, local.y1);
    Rect::from_points(a, b)
}

/// What is under a canvas point.
#[derive(Debug, Clone, PartialEq)]
pub enum Hit {
    /// A node's box.
    Node(u32),
    /// An input dot: node, port, kind.
    Input(u32, String, Kind),
    /// An output dot: node, port, kind.
    Output(u32, String, Kind),
    /// Nothing.
    Empty,
}

/// The non-port region under a pointer within a node.
///
/// Headers are the only regions that begin a graph move.
/// Content bodies belong to their child widgets, so ordinary text selection
/// and image interaction cannot accidentally move a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRegion {
    /// The title strip; graph dragging starts here.
    Header,
    /// Ordinary noninteractive node chrome.
    Body,
    /// Embedded code or image content.
    Content,
    /// The resize affordance of an embedded-content node.
    Resize,
}

/// A node and its non-port region under a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeRegionHit {
    /// The graph node identity.
    pub node: u32,
    /// The node region at the point.
    pub region: NodeRegion,
}

/// What sits under a canvas point, top box first. A dot reaches twice
/// its radius, so it is easier to land on than it looks.
pub fn hit(boxes: &[NodeBox], at: Point) -> Hit {
    let reach = PORT_R * 2.0;
    for nb in boxes.iter().rev() {
        for p in &nb.outputs {
            if (p.at - at).hypot() <= reach {
                return Hit::Output(nb.id, p.name.clone(), p.kind);
            }
        }
        for p in &nb.inputs {
            if (p.at - at).hypot() <= reach {
                return Hit::Input(nb.id, p.name.clone(), p.kind);
            }
        }
        if nb.rect.contains(at) {
            return Hit::Node(nb.id);
        }
    }
    Hit::Empty
}

/// Finds a node's interaction region without changing the established port hit
/// priority in [`hit`].
pub fn node_region_hit(boxes: &[NodeBox], at: Point) -> Option<NodeRegionHit> {
    for node in boxes.iter().rev() {
        if node.resize_rect().is_some_and(|rect| rect.contains(at)) {
            return Some(NodeRegionHit {
                node: node.id,
                region: NodeRegion::Resize,
            });
        }
        if node.header().contains(at) {
            return Some(NodeRegionHit {
                node: node.id,
                region: NodeRegion::Header,
            });
        }
        if node.content_rect().is_some_and(|rect| rect.contains(at)) {
            return Some(NodeRegionHit {
                node: node.id,
                region: NodeRegion::Content,
            });
        }
        if node.rect.contains(at) {
            return Some(NodeRegionHit {
                node: node.id,
                region: NodeRegion::Body,
            });
        }
    }
    None
}

/// Live nodes have room for a proof and explicit actions inside the canvas.
pub const LIVE_W: f64 = 256.0;
/// Fixed preview allocation, in canvas units.
pub const PREVIEW_H: f64 = 144.0;
/// One compact action row, in canvas units.
pub const ACTION_H: f64 = 24.0;

/// Explicit actions rendered and hit-tested by the shell; none run during painting.
pub fn actions(type_name: &str) -> &'static [&'static str] {
    match type_name {
        "live.font" => &["Fork direction", "Undo last application"],
        "live.fork" => &[
            "Create version",
            "Fork direction",
            "Add apply node",
            "Save as new UFO…",
            "Discard version",
        ],
        "live.proof" => &[
            "Render glyphs",
            "Render kerning",
            "Latest agent proof",
            "Export PDF…",
            "Export PNG…",
        ],
        "live.apply" => &["Apply changes"],
        _ => &[],
    }
}

impl NodeBox {
    /// First content row below the input and output sockets.
    pub fn content_top(&self) -> f64 {
        self.rect.y0
            + HEADER_H
            + PAD
            + ROW_H * (self.inputs.len() + self.outputs.len()).max(1) as f64
    }
    /// Action rectangle, shared by painting and pointer handling.
    pub fn action_rect(&self, index: usize) -> Rect {
        let y = self.content_top() + ROW_H * 2.0 + index as f64 * ACTION_H;
        Rect::new(self.rect.x0 + PAD, y, self.rect.x1 - PAD, y + ACTION_H)
    }
    /// Preview rectangle below the actions of a proof node.
    pub fn preview_rect(&self) -> Rect {
        let y = self.content_top() + ROW_H * 2.0 + actions(&self.type_name).len() as f64 * ACTION_H;
        Rect::new(
            self.rect.x0 + PAD,
            y + PAD,
            self.rect.x1 - PAD,
            y + PREVIEW_H - PAD,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph() -> (NodeGraph, Registry) {
        let registry = Registry::core();
        let mut g = NodeGraph::default();
        let font = g.add("core.source", [0.0, 0.0]);
        let install = g.add("core.install", [320.0, 0.0]);
        g.connect(font, "glyphs", install, "glyphs");
        (g, registry)
    }

    #[test]
    fn live_controls_and_previews_share_nonoverlapping_hit_geometry() {
        let g = crate::workflows::nodes_live::starter(crate::font::variable::SourceId(0));
        let boxes = layout(&g, &Registry::core());
        for node in &boxes {
            for (index, _) in actions(&node.type_name).iter().enumerate() {
                let r = node.action_rect(index);
                assert!(node.rect.contains(r.origin()));
                assert!(r.x1 <= node.rect.x1 && r.y1 <= node.rect.y1);
                assert_eq!(hit(&boxes, r.center()), Hit::Node(node.id));
                if index > 0 {
                    assert!(node.action_rect(index - 1).y1 <= r.y0);
                }
            }
            if node.type_name == "live.proof" {
                assert!(
                    node.preview_rect().y0
                        >= node.action_rect(actions(&node.type_name).len() - 1).y1
                );
                assert!(node.preview_rect().y1 <= node.rect.y1);
            }
        }
        assert_eq!(wires(&g, &boxes).len(), 4);
    }

    #[test]
    fn every_edge_lands_on_the_grid() {
        let (g, r) = graph();
        for nb in layout(&g, &r) {
            for v in [nb.rect.x0, nb.rect.x1, nb.rect.y0, nb.rect.y1] {
                assert_eq!(v % GRID, 0.0, "{v} is off the grid");
            }
        }
    }

    #[test]
    fn outputs_sit_above_inputs_and_wires_join_them() {
        let (g, r) = graph();
        let boxes = layout(&g, &r);
        let install = &boxes[1];
        assert!(
            install
                .outputs
                .iter()
                .all(|o| o.row < install.inputs[0].row)
        );
        let w = wires(&g, &boxes);
        assert_eq!(w.len(), 1);
        let (a, o, b, i) = w[0];
        assert_eq!(boxes[a].outputs[o].name, "glyphs");
        assert_eq!(boxes[b].inputs[i].name, "glyphs");
        assert!(boxes[b].inputs[i].linked);
    }

    #[test]
    fn hit_prefers_a_dot_to_the_box_under_it() {
        let (g, r) = graph();
        let boxes = layout(&g, &r);
        let dot = boxes[0].outputs[0].at;
        assert!(matches!(hit(&boxes, dot), Hit::Output(1, ref n, Kind::Source) if n == "source"));
        assert!(matches!(hit(&boxes, boxes[0].rect.center()), Hit::Node(1)));
        assert_eq!(hit(&boxes, Point::new(-500.0, -500.0)), Hit::Empty);
    }

    #[test]
    fn the_flip_round_trips() {
        let mut vp = ViewPort::new();
        vp.zoom = 2.0;
        vp.offset = kurbo::Vec2::new(10.0, 20.0);
        let p = Point::new(48.0, 96.0);
        let local = canvas_affine(&vp) * p;
        let back = to_canvas(&vp, local);
        assert!((back - p).hypot() < 1e-9);
        assert_eq!(snap(23.0), 16.0);
        assert_eq!(value_text(&serde_json::json!(200.0)), "200");
    }

    #[test]
    fn embedded_content_reserves_a_child_region_and_header_only_drag_target() {
        let (mut graph, registry) = graph();
        let node = graph.add("core.note", [320.0, 0.0]);
        let mut content = NodeContentMap::default();
        content.by_node.insert(
            node,
            NodeContent::Script(ScriptContent {
                text: "print('specimen')\n".into(),
                content_hash: "script-1".into(),
                state: ContentState::Current,
                size: [240.0, 112.0],
            }),
        );
        let boxes = layout_with_content(&graph, &registry, &content);
        let node = boxes.iter().find(|box_| box_.id == node).unwrap();
        let content_rect = node.content_rect().unwrap();
        assert!(node.rect.height() >= CODE_H + HEADER_H + PAD * 2.0);
        assert_eq!(
            node_region_hit(&boxes, node.header().center()),
            Some(NodeRegionHit {
                node: node.id,
                region: NodeRegion::Header,
            })
        );
        assert_eq!(
            node_region_hit(&boxes, content_rect.center()),
            Some(NodeRegionHit {
                node: node.id,
                region: NodeRegion::Content,
            })
        );
        assert_eq!(
            node_region_hit(&boxes, node.resize_rect().unwrap().center()),
            Some(NodeRegionHit {
                node: node.id,
                region: NodeRegion::Resize,
            })
        );
        assert_eq!(hit(&boxes, content_rect.center()), Hit::Node(node.id));
    }

    #[test]
    fn comparison_starter_content_boxes_do_not_overlap() {
        assert_eq!(LIVE_W - PAD * 2.0, 240.0);
        assert_eq!(CODE_H, 112.0);
        assert_eq!(IMAGE_H, 144.0);
        let graph =
            crate::workflows::nodes_live::comparison_starter(crate::font::variable::SourceId(0));
        let mut content = NodeContentMap::default();
        for node in &graph.nodes {
            match node.type_name.as_str() {
                "live.python" => {
                    content.by_node.insert(
                        node.id,
                        NodeContent::Script(ScriptContent {
                            text: String::new(),
                            content_hash: String::new(),
                            state: ContentState::Idle,
                            size: [240.0, 112.0],
                        }),
                    );
                }
                "live.proof" => {
                    content.by_node.insert(
                        node.id,
                        NodeContent::Image(ImageContent {
                            image: None,
                            previous_image: None,
                            state: ContentState::Idle,
                            size: [240.0, 144.0],
                        }),
                    );
                }
                _ => {}
            }
        }
        let boxes = layout_with_content(&graph, &Registry::core(), &content);
        let proof_titles: Vec<_> = boxes
            .iter()
            .filter(|node| node.type_name == "live.proof")
            .map(|node| node.title.as_str())
            .collect();
        assert_eq!(proof_titles, ["Original specimen", "Scripted specimen"]);
        for (index, left) in boxes.iter().enumerate() {
            for right in boxes.iter().skip(index + 1) {
                assert!(
                    left.rect.x1 <= right.rect.x0
                        || right.rect.x1 <= left.rect.x0
                        || left.rect.y1 <= right.rect.y0
                        || right.rect.y1 <= left.rect.y0,
                    "starter nodes overlap: {:?} and {:?}",
                    left.rect,
                    right.rect
                );
            }
        }
    }

    #[test]
    fn image_projection_keeps_a_previous_proof_while_current_output_runs() {
        let image = ImmutablePng {
            bytes: Arc::from([137, 80, 78, 71]),
            width: 4,
            height: 1,
            output_hash: "proof-1".into(),
        };
        let content = ImageContent {
            image: None,
            previous_image: Some(image.clone()),
            state: ContentState::Running,
            size: [240.0, 144.0],
        };
        assert_eq!(content.previous_image, Some(image));
        assert_eq!(content.state, ContentState::Running);
    }
}
