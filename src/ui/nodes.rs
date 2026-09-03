// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The nodes canvas as geometry: where each box, port and wire sits.
//!
//! A shell draws a [`crate::document::nodes::NodeGraph`] as boxes and
//! wires. Everything about that picture that is not a pixel lives
//! here: the grid pitch, the box sizes, one row per port, where the
//! dots go, which grid mark colour a port kind or a node type wears,
//! the cubic a wire follows, and what is under a point. A shell adds
//! the paint calls and the mouse, so two shells draw the same graph
//! the same way, and a change to the layout is made once.
//!
//! Canvas units are Y-down, like the file. Core's [`ViewPort`] is
//! Y-up, so [`canvas_affine`] and [`to_canvas`] carry the flip.

use kurbo::{Affine, BezPath, Point, Rect, Shape as _};
use serde_json::Value;

use crate::document::nodes::{Kind, Node, NodeGraph, Registry};
use crate::ui::editing::viewport::ViewPort;

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
    let ty = registry.get(&node.type_name);
    let title = ty
        .map(|t| t.title.clone())
        .unwrap_or_else(|| node.type_name.clone());
    let inputs: Vec<_> = ty.map(|t| t.inputs.clone()).unwrap_or_default();
    let outputs: Vec<_> = ty.map(|t| t.outputs.clone()).unwrap_or_default();
    let rows = (inputs.len() + outputs.len()).max(1);
    let x = f64::from(node.pos[0]);
    let y = f64::from(node.pos[1]);
    let h = HEADER_H + PAD + ROW_H * rows as f64;
    let rect = Rect::new(x, y, x + NODE_W, y + h);
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
            at: Point::new(x + NODE_W, row_y(i)),
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
        inputs,
        outputs,
    }
}

/// Every node laid out, in file order, which is also paint order.
pub fn layout(graph: &NodeGraph, registry: &Registry) -> Vec<NodeBox> {
    graph
        .nodes
        .iter()
        .map(|n| node_box(graph, registry, n))
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
        Kind::Source => "green",
        Kind::Layer | Kind::Path => "yellow",
        Kind::Model => "blue",
        Kind::Adapter => "purple",
        Kind::Glyph | Kind::Glyphs => "orange",
        Kind::Rows => "pink",
        Kind::Number | Kind::Flag | Kind::Text => return None,
    })
}

/// The grid mark colour a node type's header carries: what the node
/// mostly gives, or what it does to the font.
pub fn type_mark(type_name: &str) -> Option<&'static str> {
    Some(match type_name {
        "core.source" | "core.master" => "green",
        "core.layer" | "core.proof" => "yellow",
        "core.model" => "blue",
        "core.adapter" => "purple",
        "core.install" => "red",
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
}
