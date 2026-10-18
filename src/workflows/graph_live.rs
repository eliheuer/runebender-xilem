// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Resolve live graph connections to isolated font versions without saving the root.

use super::graph::{Kind, NodeCachePolicy, NodeEffects, NodeExecution, NodeGraph, NodeType, Port};
use crate::font::{experiments, project::Project, variable::SourceId};
use serde_json::{Value, json};
use std::collections::HashSet;

/// Default compiled specimen recipe used by both comparison branches.
pub fn default_proof_recipe() -> Value {
    json!({
        "text": "Hamburgefontsiv",
        "normalized_location": [],
        "right_to_left": false,
        "features": [],
        "script": null,
        "language": null
    })
}

/// Node types executed against an open editor, rather than the disk runner.
pub fn types() -> Vec<NodeType> {
    let port = |name: &str, input: bool| Port {
        name: name.into(),
        kind: Kind::FontVersion,
        required: input,
        default: None,
        help: "An in-memory font version.".into(),
    };
    [
        ("live.font", "Current font", false, true),
        ("live.fork", "Font version", true, true),
        ("live.python", "Python recipe", true, true),
        ("live.proof", "Designbot proof", true, false),
        ("live.apply", "Apply to current font", true, false),
    ]
    .into_iter()
    .map(|(name, title, input, output)| NodeType {
        name: name.into(),
        title: title.into(),
        help: "Runs in the live editor. Inputs are preserved; applying to the root is explicit."
            .into(),
        implemented: true,
        execution: NodeExecution::new(
            NodeCachePolicy::Never,
            match name {
                "live.python" => NodeEffects::TrustedProcess,
                "live.apply" => NodeEffects::WriteFont,
                _ => NodeEffects::LiveDocument,
            },
        ),
        inputs: {
            let mut ports = if input {
                vec![port("font", true)]
            } else {
                vec![]
            };
            if matches!(name, "live.font" | "live.fork") {
                ports.push(Port {
                    name: if name == "live.font" {
                        "source"
                    } else {
                        "branch"
                    }
                    .into(),
                    kind: if name == "live.font" {
                        Kind::Number
                    } else {
                        Kind::Text
                    },
                    required: false,
                    default: None,
                    help: "The stable source or stored session result.".into(),
                });
            }
            if name == "live.python" {
                ports.extend([
                    Port {
                        name: "code".into(),
                        kind: Kind::Text,
                        required: true,
                        default: None,
                        help: "Exact Python source submitted to the shared recipe runner.".into(),
                    },
                    Port {
                        name: "parameters".into(),
                        kind: Kind::Parameters,
                        required: false,
                        default: Some(json!({})),
                        help: "Structured parameters included in the immutable recipe input."
                            .into(),
                    },
                ]);
            }
            if name == "live.proof" {
                ports.push(Port {
                    name: "recipe".into(),
                    kind: Kind::Parameters,
                    required: false,
                    default: Some(default_proof_recipe()),
                    help: "Structured specimen settings shared by both comparison proofs.".into(),
                });
            }
            ports
        },
        outputs: if output {
            vec![port("font", false)]
        } else {
            vec![]
        },
    })
    .collect()
}

/// A loaded stable source or a named session version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// The originating source, fixed when the source node is created.
    pub source: SourceId,
    /// None denotes the live root.
    pub branch: Option<String>,
}

/// Canonical root changes made by an explicitly applied live version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyOutcome {
    /// Stable source receiving the changes.
    pub source: SourceId,
    /// Glyphs installed in the root; kerning may also have changed.
    pub installed: Vec<String>,
}

/// Create a connected starter graph with two independent version directions.
pub fn starter(source_id: SourceId) -> NodeGraph {
    let mut graph = NodeGraph::default();
    let source = graph.add("live.font", [32.0, 32.0]);
    graph
        .node_mut(source)
        .unwrap()
        .values
        .insert("source".into(), json!(source_id.0));
    add_direction(&mut graph, source, [336.0, 32.0]);
    add_direction(&mut graph, source, [336.0, 416.0]);
    graph
}

/// Create the first supported Python comparison graph.
///
/// One captured base feeds an unchanged proof and a Python-derived proof.
/// The Python code remains empty until the user or agent authors it, and opening
/// the graph never executes it.
pub fn comparison_starter(source_id: SourceId) -> NodeGraph {
    let mut graph = NodeGraph::default();
    let source = graph.add("live.font", [32.0, 32.0]);
    graph
        .node_mut(source)
        .unwrap()
        .values
        .insert("source".into(), json!(source_id.0));
    let unchanged = graph.add("live.proof", [640.0, 32.0]);
    graph
        .node_mut(unchanged)
        .unwrap()
        .values
        .insert("recipe".into(), default_proof_recipe());
    graph.connect(source, "font", unchanged, "font");
    let python = graph.add("live.python", [336.0, 32.0]);
    graph
        .node_mut(python)
        .unwrap()
        .values
        .insert("code".into(), json!(""));
    graph
        .node_mut(python)
        .unwrap()
        .values
        .insert("parameters".into(), json!({}));
    graph.connect(source, "font", python, "font");
    let changed = graph.add("live.proof", [928.0, 32.0]);
    graph
        .node_mut(changed)
        .unwrap()
        .values
        .insert("recipe".into(), default_proof_recipe());
    graph.connect(python, "font", changed, "font");
    graph
}

/// Specimen paper: near black, so light type reads as it would on a dark
/// screen and the image stands out on the canvas.
pub const SPECIMEN_PAPER: [u8; 3] = [15, 15, 15];
/// Specimen ink: an off-white that does not glare on the dark paper.
pub const SPECIMEN_INK: [u8; 3] = [235, 235, 235];
/// How many pages [`specimen_recipes`] makes.
pub const SPECIMEN_PAGES: usize = 2;

const PAGE_SIZE: f64 = 2048.0;
const PAGE_MARGIN: f64 = 144.0;
/// Room left on each line for edits that widen it. A line that grows past
/// the page is clipped, not failed.
const PAGE_SLACK: f64 = 1.06;

/// The default specimen pages, top to bottom: a poster of the character set
/// and a waterfall of one sentence. Both are 2048 pixels wide, light type
/// on dark paper, sized from `advance_em`, which measures a line's advance
/// in ems.
pub fn specimen_recipes(
    advance_em: impl Fn(&str) -> f64,
    normalized_location: &[f64],
) -> Vec<Value> {
    vec![
        poster_recipe(&advance_em, normalized_location),
        waterfall_recipe(&advance_em, normalized_location),
    ]
}

/// The largest size at which every line fits the page width.
fn fit_size(advance_em: &dyn Fn(&str) -> f64, lines: &[&str], width: f64) -> f64 {
    let widest = lines
        .iter()
        .map(|line| advance_em(line))
        .fold(0.5, f64::max);
    width / (widest * PAGE_SLACK)
}

/// One display word over the alphabet and figures, centered on a square page.
fn poster_recipe(advance_em: &dyn Fn(&str) -> f64, normalized_location: &[f64]) -> Value {
    const HEADLINE: &str = "Hamburgefontsiv";
    const BODY: [&str; 5] = [
        "ABCDEFGHIJKLM",
        "NOPQRSTUVWXYZ",
        "abcdefghijklm",
        "nopqrstuvwxyz",
        "0123456789",
    ];
    // Lines sit 1.2 em apart; a baseline is 0.95 em below its line's top,
    // which clears accents above and descenders below.
    const LEADING: f64 = 1.2;
    const DROP: f64 = 0.95;
    let width = PAGE_SIZE - 2.0 * PAGE_MARGIN;
    let mut headline = fit_size(advance_em, &[HEADLINE], width).min(400.0);
    let mut body = fit_size(advance_em, &BODY, width).min(headline * 0.6);
    let gap = |body: f64| body * 0.8;
    let total = |headline: f64, body: f64| {
        headline * LEADING + gap(body) + BODY.len() as f64 * body * LEADING
    };
    let room = PAGE_SIZE - 2.0 * PAGE_MARGIN;
    if total(headline, body) > room {
        let shrink = room / total(headline, body);
        headline *= shrink;
        body *= shrink;
    }
    let mut top = (PAGE_SIZE - total(headline, body)) / 2.0;
    let mut lines = vec![(HEADLINE, headline, top + headline * DROP)];
    top += headline * LEADING + gap(body);
    for (index, text) in BODY.iter().enumerate() {
        lines.push((text, body, top + body * (DROP + LEADING * index as f64)));
    }
    page_recipe(&lines, PAGE_SIZE, "center", normalized_location)
}

/// One sentence from the largest size that fits down to text sizes.
fn waterfall_recipe(advance_em: &dyn Fn(&str) -> f64, normalized_location: &[f64]) -> Value {
    const TEXT: &str = "The quick brown fox jumps over the lazy dog";
    const LEADING: f64 = 1.3;
    let mut size = fit_size(advance_em, &[TEXT], PAGE_SIZE - 2.0 * PAGE_MARGIN).min(200.0);
    let mut top = PAGE_MARGIN;
    let mut lines = Vec::new();
    // The first line always shows, however wide the font. A baseline sits
    // 0.8 em below its line's top, about where the cap height lands.
    while lines.is_empty() || (size >= 16.0 && lines.len() < 12) {
        lines.push((TEXT, size, top + size * 0.8));
        top += size * LEADING;
        size *= 0.8;
    }
    let height = (top + PAGE_MARGIN).min(PAGE_SIZE);
    page_recipe(&lines, height, "start", normalized_location)
}

/// A page recipe from `(text, pixels per em, baseline from the top)` lines.
fn page_recipe(
    lines: &[(&str, f64, f64)],
    height: f64,
    align: &str,
    normalized_location: &[f64],
) -> Value {
    let height = height.round().clamp(128.0, PAGE_SIZE);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "both sizes are whole numbers clamped to 128..=2048"
    )]
    let (width_px, height_px) = (PAGE_SIZE as u32, height as u32);
    let text = lines
        .iter()
        .map(|(text, ..)| *text)
        .collect::<Vec<_>>()
        .join("\n");
    let settings = lines
        .iter()
        .map(|&(_, size, baseline)| {
            json!({
                "pixels_per_em": size.clamp(4.0, 512.0).round(),
                "baseline_px": (height - baseline).clamp(0.0, height).round(),
                "align": align,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "text": text,
        "normalized_location": normalized_location,
        "right_to_left": false,
        "features": [],
        "script": null,
        "language": null,
        "rendering": {
            "width_px": width_px,
            "height_px": height_px,
            "pixels_per_em": 160.0,
            "margin_px": PAGE_MARGIN,
            "first_baseline_px": 180.0,
            "line_height_px": 180.0,
            "background_rgb": SPECIMEN_PAPER,
            "ink_rgb": SPECIMEN_INK,
            "lines": settings,
        },
    })
}

/// Whether a proof recipe is a waterfall: one text repeated down a page.
pub fn is_waterfall(recipe: &Value) -> bool {
    let lines = recipe["text"].as_str().map(|text| text.split('\n'));
    lines.is_some_and(|mut lines| {
        let first = lines.next();
        let mut rest = lines.peekable();
        rest.peek().is_some() && rest.all(|line| Some(line) == first)
    })
}

/// The proof's raster size in pixels, when its recipe sets one.
pub fn recipe_raster(recipe: &Value) -> Option<[f64; 2]> {
    let rendering = recipe.get("rendering")?;
    Some([
        rendering.get("width_px")?.as_f64()?,
        rendering.get("height_px")?.as_f64()?,
    ])
}

/// The default live graph: the current font feeding one proof per specimen page.
///
/// `recipes` come from [`specimen_recipes`], top to bottom.
pub fn specimen_starter(source_id: SourceId, recipes: Vec<Value>) -> NodeGraph {
    // A proof node shows its image at the standard content width, 240 px,
    // inside a header, two port rows, and padding: 80 px in all
    // (see `ui::graph::node_box_with_content`). Then a gap.
    const CONTENT_WIDTH: f64 = 240.0;
    const NODE_CHROME: f32 = 80.0;
    const GAP: f32 = 32.0;
    let mut graph = NodeGraph::default();
    let source = graph.add("live.font", [32.0, 32.0]);
    graph
        .node_mut(source)
        .unwrap()
        .values
        .insert("source".into(), json!(source_id.0));
    let mut y = 32.0_f32;
    for recipe in recipes {
        let height = recipe_raster(&recipe).map_or(144.0, |[w, h]| CONTENT_WIDTH * h / w.max(1.0));
        let proof = graph.add("live.proof", [336.0, y]);
        graph
            .node_mut(proof)
            .unwrap()
            .values
            .insert("recipe".into(), recipe);
        graph.connect(source, "font", proof, "font");
        #[allow(
            clippy::cast_possible_truncation,
            reason = "canvas positions are small finite numbers"
        )]
        {
            y += (height as f32) + NODE_CHROME + GAP;
        }
    }
    graph
}

/// Whether the graph holds a Python comparison: a recipe fed by the
/// font, with a proof of its result.
pub fn has_comparison(graph: &NodeGraph) -> bool {
    let Some(source) = graph.nodes.iter().find(|n| n.type_name == "live.font") else {
        return false;
    };
    graph.nodes.iter().any(|python| {
        python.type_name == "live.python"
            && graph
                .link_into(python.id, "font")
                .is_some_and(|link| link.from() == source.id)
            && graph.links.iter().any(|link| {
                link.from() == python.id
                    && graph
                        .node(link.to())
                        .is_some_and(|n| n.type_name == "live.proof")
            })
    })
}

/// Add the Python comparison beside whatever the graph already shows:
/// an unchanged proof of the font, an empty Python recipe, and a proof
/// of its result, both proofs rendering `recipe`. Returns the Python node.
///
/// The pair is placed below the lowest node, so a specimen keeps its place.
pub fn add_comparison(graph: &mut NodeGraph, recipe: &Value) -> Option<u32> {
    let source = graph.nodes.iter().find(|n| n.type_name == "live.font")?.id;
    let below = graph
        .nodes
        .iter()
        .map(|node| node.pos[1])
        .fold(32.0_f32, f32::max);
    let y = below + 320.0;
    let unchanged = graph.add("live.proof", [640.0, y]);
    graph
        .node_mut(unchanged)?
        .values
        .insert("recipe".into(), recipe.clone());
    graph.connect(source, "font", unchanged, "font");
    let python = graph.add("live.python", [336.0, y]);
    let node = graph.node_mut(python)?;
    node.values.insert("code".into(), json!(""));
    node.values.insert("parameters".into(), json!({}));
    graph.connect(source, "font", python, "font");
    let changed = graph.add("live.proof", [928.0, y]);
    graph
        .node_mut(changed)?
        .values
        .insert("recipe".into(), recipe.clone());
    graph.connect(python, "font", changed, "font");
    Some(python)
}

/// Add a fork and proof connected to an existing font output. Returns the fork id.
pub fn add_direction(graph: &mut NodeGraph, source: u32, pos: [f32; 2]) -> u32 {
    let fork = graph.add("live.fork", pos);
    graph.connect(source, "font", fork, "font");
    let proof = graph.add("live.proof", [pos[0] + 304.0, pos[1]]);
    graph.connect(fork, "font", proof, "font");
    fork
}

/// Resolve an output or a sink's input. Missing runs and cycles are errors;
/// reconnecting a previously run fork does not replace its stored result.
pub fn resolve(graph: &NodeGraph, project: &Project, id: u32) -> Result<Version, String> {
    fn walk(
        graph: &NodeGraph,
        project: &Project,
        id: u32,
        seen: &mut HashSet<u32>,
    ) -> Result<Version, String> {
        if !seen.insert(id) {
            return Err("cycle in live font connections".into());
        }
        let n = graph.node(id).ok_or("missing node")?;
        if n.type_name == "live.font" {
            let source = usize::try_from(
                n.values
                    .get("source")
                    .and_then(Value::as_u64)
                    .ok_or("Choose a stable source for this live font node")?,
            )
            .map_err(|_| "source identity is too large")?;
            let source = SourceId(source);
            project
                .document_source(source)
                .ok_or("source is no longer loaded")?;
            return Ok(Version {
                source,
                branch: None,
            });
        }
        if n.type_name == "live.fork" {
            let branch = n
                .values
                .get("branch")
                .and_then(Value::as_str)
                .ok_or("Create this version first")?;
            let v = project
                .experiments
                .versions
                .get(branch)
                .ok_or("This session version is unavailable; create a new version")?;
            return Ok(Version {
                source: v.root,
                branch: Some(branch.into()),
            });
        }
        if !matches!(n.type_name.as_str(), "live.proof" | "live.apply") {
            return Err("This node does not carry a live font".into());
        }
        let link = graph
            .link_into(id, "font")
            .ok_or("Connect a font output first")?;
        if link.output() != "font" {
            return Err("Expected a live font output".into());
        }
        walk(graph, project, link.from(), seen)
    }
    walk(graph, project, id, &mut HashSet::new())
}

/// Snapshot the connected input once. Repeated calls preserve an existing result.
pub fn create_version(
    graph: &mut NodeGraph,
    project: &mut Project,
    id: u32,
) -> Result<Version, String> {
    let n = graph.node(id).ok_or("missing node")?;
    if n.type_name != "live.fork" {
        return Err("Select a Font version node".into());
    }
    if n.values
        .get("branch")
        .and_then(Value::as_str)
        .is_some_and(|s| project.experiments.versions.contains_key(s))
    {
        return resolve(graph, project, id);
    }
    // A saved graph's expired binding is not a request to reuse the old name.
    let link = graph
        .link_into(id, "font")
        .ok_or("Connect a font output first")?;
    let input = resolve(graph, project, link.from())?;
    let mut i = 1;
    while project
        .experiments
        .versions
        .contains_key(&format!("version-{i}"))
    {
        i += 1;
    }
    let name = format!("version-{i}");
    experiments::fork(
        project,
        input.source,
        &name,
        input.branch.as_deref(),
        "Node graph experiment; edit this branch through MCP",
    )?;
    let n = graph.node_mut(id).unwrap();
    n.values.insert("branch".into(), json!(name));
    Ok(Version {
        source: input.source,
        branch: Some(name),
    })
}

/// Apply the connected version with the font engine's conflict and undo rules.
/// Does not save the source files.
pub fn apply(graph: &NodeGraph, project: &mut Project, id: u32) -> Result<ApplyOutcome, String> {
    let v = resolve(graph, project, id)?;
    let name = v
        .branch
        .ok_or("Connect an experimental version, not the root")?;
    let version = &project.experiments.versions[&name];
    let names = version.changed_glyphs();
    let installed = experiments::apply(project, &name, &names, true, false)?;
    Ok(ApplyOutcome {
        source: v.source,
        installed,
    })
}

/// Discard a leaf version. Children must be discarded first; the root is untouched.
pub fn discard(project: &mut Project, name: &str) -> Result<(), String> {
    if project
        .experiments
        .versions
        .values()
        .any(|v| v.parent.as_deref() == Some(name))
    {
        return Err("Discard this version's children first".into());
    }
    let v = project
        .experiments
        .versions
        .remove(name)
        .ok_or("unknown version")?;
    project
        .experiments
        .proofs
        .remove(&format!("{}:{name}", v.root.0));
    Ok(())
}

/// Add existing or MCP-created session versions to a live graph, preserving all
/// existing node positions and connections. Disk-only graphs are left unchanged.
/// Returns the number of imported versions.
pub fn import_versions(graph: &mut NodeGraph, project: &Project) -> usize {
    if !graph.nodes.iter().any(|n| n.type_name == "live.font") {
        return 0;
    }
    let mut imported = Vec::new();
    for (name, v) in &project.experiments.versions {
        if graph.nodes.iter().any(|n| {
            n.type_name == "live.fork"
                && n.values.get("branch").and_then(Value::as_str) == Some(name)
        }) {
            continue;
        }
        let y = graph.nodes.iter().map(|n| n.pos[1]).fold(0.0_f32, f32::max) + 384.0;
        let source = match graph.nodes.iter().find(|n| {
            n.type_name == "live.font"
                && n.values.get("source").and_then(Value::as_u64) == Some(v.root.0 as u64)
        }) {
            Some(n) => n.id,
            None => {
                let id = graph.add("live.font", [32.0, y]);
                graph
                    .node_mut(id)
                    .unwrap()
                    .values
                    .insert("source".into(), json!(v.root.0));
                id
            }
        };
        let id = add_direction(graph, source, [336.0, y]);
        graph
            .node_mut(id)
            .unwrap()
            .values
            .insert("branch".into(), json!(name));
        imported.push((id, v.parent.clone()));
    }
    for (id, parent) in &imported {
        if let Some(parent) = parent
            && let Some(node) = graph.nodes.iter().find(|n| {
                n.type_name == "live.fork"
                    && n.values.get("branch").and_then(Value::as_str) == Some(parent)
            })
        {
            graph.connect(node.id, "font", *id, "font");
        }
    }
    imported.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::history::HistoryDirection;
    use crate::font::persistence::memory::designspace_from_str;
    use crate::font::project::SourceInput;

    fn save_new(font: &norad::Font, path: &std::path::Path) -> Result<(), String> {
        if path.extension().and_then(|extension| extension.to_str()) != Some("ufo") {
            return Err("Choose a new .ufo directory".into());
        }
        std::fs::create_dir(path).map_err(|error| format!("{}: {error}", path.display()))?;
        font.save(path)
            .map_err(|error| format!("{}: {error}", path.display()))
    }

    fn two_source_project() -> Project {
        let font = Project::new_font("synthetic.ufo".into())
            .encode_ufo_source(SourceId(0))
            .unwrap();
        let document = designspace_from_str(
            r#"<designspace format="5.0"><axes><axis name="Weight" tag="wght" minimum="0" default="0" maximum="1"/></axes><sources><source filename="first.ufo"><location><dimension name="Weight" xvalue="0"/></location></source><source filename="second.ufo"><location><dimension name="Weight" xvalue="1"/></location></source></sources></designspace>"#,
        )
        .unwrap();
        Project::from_designspace(document, |path| {
            Ok(SourceInput::from_font(font.clone(), path.into()))
        })
        .unwrap()
    }

    #[test]
    fn apply_installs_and_undoes_a_version_without_saving() {
        let path = std::env::temp_dir().join(format!(
            "runebender-live-apply-{}-{}.ufo",
            std::process::id(),
            line!()
        ));
        assert!(!path.exists());
        let mut project = Project::new_font(path.clone());
        let source = project.source_id(0).unwrap();
        let mut graph = starter(source);
        let version = create_version(&mut graph, &mut project, 2).unwrap();
        let branch = version.branch.unwrap();
        let address = project.experiments.versions[&branch].default_address("A");
        let original = project.document_layer("A", &address.layer).unwrap().width();
        experiments::edit_layer(&mut project, &branch, &address, |draft| {
            draft.set_width(original + 100.0)?;
            Ok(())
        })
        .unwrap();
        let sink = graph.add("live.apply", [640.0, 32.0]);
        graph.connect(2, "font", sink, "font");

        let outcome = apply(&graph, &mut project, sink).unwrap();
        assert_eq!(outcome.source, source);
        assert_eq!(outcome.installed, ["A"]);
        assert_eq!(
            project.document_layer("A", &address.layer).unwrap().width(),
            original + 100.0
        );
        assert!(project.can_replay_document_layer_history(&address, HistoryDirection::Undo));
        assert!(!path.exists());

        assert_eq!(
            experiments::undo_apply(&mut project).unwrap(),
            (source, vec!["A".into()])
        );
        assert_eq!(
            project.document_layer("A", &address.layer).unwrap().width(),
            original
        );
        assert!(!path.exists());
    }

    #[test]
    fn apply_rejects_root_conflicts_before_changing_any_glyph() {
        let mut project = Project::new_font("synthetic.ufo".into());
        let source = project.source_id(0).unwrap();
        let mut graph = starter(source);
        let branch = create_version(&mut graph, &mut project, 2)
            .unwrap()
            .branch
            .unwrap();
        for (glyph, width) in [("A", 700.0), ("B", 710.0)] {
            let address = project.experiments.versions[&branch].default_address(glyph);
            experiments::edit_layer(&mut project, &branch, &address, |draft| {
                draft.set_width(width)?;
                Ok(())
            })
            .unwrap();
        }
        let address_a = project.experiments.versions[&branch].default_address("A");
        let address_b = project.experiments.versions[&branch].default_address("B");
        project
            .edit_document_layer("B", &address_b.layer, |draft| {
                draft.set_width(999.0)?;
                Ok(())
            })
            .unwrap();
        let before_a = project.capture_document_layer(&address_a).unwrap();
        let revision = project.document_revision();
        let sink = graph.add("live.apply", [640.0, 32.0]);
        graph.connect(2, "font", sink, "font");

        assert!(
            apply(&graph, &mut project, sink)
                .unwrap_err()
                .contains("conflict")
        );
        assert_eq!(project.document_revision(), revision);
        assert_eq!(project.capture_document_layer(&address_a), Some(before_a));
        assert_eq!(
            project.document_layer_history_depth(&address_a, HistoryDirection::Undo),
            0
        );
        assert!(experiments::undo_apply(&mut project).is_err());
    }

    #[test]
    fn apply_requires_a_connected_available_version() {
        let mut project = Project::new_font("synthetic.ufo".into());
        let source = project.source_id(0).unwrap();
        let mut graph = starter(source);
        let sink = graph.add("live.apply", [640.0, 32.0]);
        assert!(
            apply(&graph, &mut project, sink)
                .unwrap_err()
                .contains("Connect")
        );
        graph.connect(1, "font", sink, "font");
        assert!(
            apply(&graph, &mut project, sink)
                .unwrap_err()
                .contains("not the root")
        );
        graph.connect(2, "font", sink, "font");
        assert!(
            apply(&graph, &mut project, sink)
                .unwrap_err()
                .contains("Create")
        );
        graph
            .node_mut(2)
            .unwrap()
            .values
            .insert("branch".into(), json!("expired"));
        assert!(
            apply(&graph, &mut project, sink)
                .unwrap_err()
                .contains("unavailable")
        );
        assert!(experiments::undo_apply(&mut project).is_err());
    }
    #[test]
    fn agent_versions_import_once_with_their_parent_connections() {
        let mut p = Project::new_font("test.ufo".into());
        let source = p.source_id(0).unwrap();
        experiments::fork(&mut p, source, "z-parent", None, "baseline").unwrap();
        experiments::fork(&mut p, source, "a-child", Some("z-parent"), "direction").unwrap();
        let mut g = starter(source);
        assert_eq!(import_versions(&mut g, &p), 2);
        assert_eq!(import_versions(&mut g, &p), 0);
        let parent = g
            .nodes
            .iter()
            .find(|n| n.values.get("branch") == Some(&json!("z-parent")))
            .unwrap()
            .id;
        let child = g
            .nodes
            .iter()
            .find(|n| n.values.get("branch") == Some(&json!("a-child")))
            .unwrap()
            .id;
        assert_eq!(g.link_into(child, "font").unwrap().from(), parent);
        assert!(discard(&mut p, "z-parent").is_err());
        assert!(
            g.validate(&super::super::graph::Registry::core())
                .is_empty()
        );
        discard(&mut p, "a-child").unwrap();
        assert!(resolve(&g, &p, child).is_err());
        assert!(resolve(&g, &p, parent).is_ok());
    }

    #[test]
    fn new_ufo_export_preserves_live_edits_and_refuses_overwrites() {
        let mut p = Project::new_font("never-written.ufo".into());
        let source = p.source_id(0).unwrap();
        let mut g = starter(source);
        let version = create_version(&mut g, &mut p, 2).unwrap();
        let address = p.experiments.versions[version.branch.as_ref().unwrap()].default_address("A");
        experiments::edit_layer(
            &mut p,
            version.branch.as_ref().unwrap(),
            &address,
            |draft| {
                draft.set_width(777.0)?;
                Ok(())
            },
        )
        .unwrap();
        let font = p.experiments.versions[version.branch.as_ref().unwrap()]
            .encode_ufo_source(&p)
            .unwrap();
        let dir =
            std::env::temp_dir().join(format!("runebender-node-export-{}.ufo", std::process::id()));
        assert!(!dir.exists());
        save_new(&font, &dir).unwrap();
        let saved = norad::Font::load(&dir).unwrap();
        assert_eq!(saved.get_glyph("A").unwrap().width, 777.0);
        assert!(save_new(&font, &dir).is_err());
        assert_eq!(
            p.document_source(source).unwrap().path(),
            std::path::Path::new("never-written.ufo")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn live_outputs_cannot_connect_to_disk_tasks() {
        let mut g = starter(SourceId(0));
        let target = g.add("core.layer", [0.0, 0.0]);
        g.connect(1, "font", target, "source");
        assert!(
            g.validate(&super::super::graph::Registry::core())
                .iter()
                .any(|p| matches!(p, super::super::graph::Problem::KindMismatch { .. }))
        );
    }

    #[test]
    fn forks_follow_wires_and_preserve_previous_results() {
        let mut p = Project::new_font("test.ufo".into());
        let source = p.source_id(0).unwrap();
        let mut g = starter(source);
        assert!(
            g.validate(&super::super::graph::Registry::core())
                .is_empty()
        );
        assert!(resolve(&g, &p, 3).is_err());
        let a = create_version(&mut g, &mut p, 2).unwrap();
        let address = p.experiments.versions[a.branch.as_ref().unwrap()].default_address("A");
        experiments::edit_layer(&mut p, a.branch.as_ref().unwrap(), &address, |draft| {
            draft.set_width(701.0)?;
            Ok(())
        })
        .unwrap();
        let b = create_version(&mut g, &mut p, 4).unwrap();
        assert_ne!(a, b);
        assert_eq!(resolve(&g, &p, 3).unwrap(), a);
        assert_ne!(
            p.experiments.versions[b.branch.as_ref().unwrap()]
                .layer(&address)
                .unwrap()
                .width(),
            701.0
        );
        assert_eq!(create_version(&mut g, &mut p, 2).unwrap(), a);
        g.connect(2, "font", 4, "font");
        assert_eq!(resolve(&g, &p, 4).unwrap(), b);
        assert!(discard(&mut p, a.branch.as_ref().unwrap()).is_ok());
        assert!(resolve(&g, &p, 3).is_err());
        assert_eq!(resolve(&g, &p, 5).unwrap(), b);
    }

    #[test]
    fn live_source_nodes_never_redirect_after_reorder_or_removal() {
        let mut project = two_source_project();
        let source = project.source_id(1).unwrap();
        let graph = starter(source);
        assert_eq!(resolve(&graph, &project, 1).unwrap().source, source);
        assert!(project.move_source(source, 0).unwrap());
        assert_eq!(resolve(&graph, &project, 1).unwrap().source, source);
        project.remove_source(source).unwrap();
        assert!(resolve(&graph, &project, 1).is_err());

        let mut unbound = NodeGraph::default();
        let live = unbound.add("live.font", [0.0, 0.0]);
        assert!(
            resolve(&unbound, &project, live)
                .unwrap_err()
                .contains("stable source")
        );
    }

    #[test]
    fn specimen_pages_are_valid_dark_proofs_whose_lines_fit() {
        let advance = |text: &str| text.chars().count() as f64 * 0.55;
        let recipes = specimen_recipes(advance, &[]);
        assert_eq!(recipes.len(), SPECIMEN_PAGES);
        for recipe in &recipes {
            let parsed: crate::font::compiler::proof::CompiledProofRecipe =
                serde_json::from_value(recipe.clone()).unwrap();
            parsed.validate().unwrap();
            let rendering = &parsed.rendering;
            assert_eq!(rendering.background_rgb, SPECIMEN_PAPER);
            assert_eq!(rendering.width_px, 2048);
            for (text, line) in parsed.text.split('\n').zip(&rendering.lines) {
                assert!(
                    advance(text) * line.pixels_per_em <= 2048.0 - 2.0 * PAGE_MARGIN,
                    "{text} fits the page"
                );
            }
        }
        assert!(!is_waterfall(&recipes[0]));
        assert!(is_waterfall(&recipes[1]));
    }

    #[test]
    fn a_very_wide_font_still_gets_a_valid_page() {
        for recipe in specimen_recipes(|text| text.chars().count() as f64 * 3.0, &[]) {
            let parsed: crate::font::compiler::proof::CompiledProofRecipe =
                serde_json::from_value(recipe).unwrap();
            parsed.validate().unwrap();
        }
    }

    #[test]
    fn the_specimen_is_the_font_feeding_one_proof_per_page_and_grows_a_comparison() {
        let recipes = specimen_recipes(|_| 10.0, &[]);
        let mut graph = specimen_starter(SourceId(0), recipes);
        let proofs: Vec<_> = graph
            .nodes
            .iter()
            .filter(|node| node.type_name == "live.proof")
            .collect();
        assert_eq!(proofs.len(), SPECIMEN_PAGES);
        assert!(
            proofs
                .iter()
                .all(|proof| graph.link_into(proof.id, "font").is_some())
        );
        assert!(!has_comparison(&graph));
        let before = graph.nodes.len();
        add_comparison(&mut graph, &default_proof_recipe()).unwrap();
        assert!(has_comparison(&graph));
        assert_eq!(graph.nodes.len(), before + 3);
        assert!(
            graph
                .validate(&super::super::graph::Registry::core())
                .is_empty()
        );
    }
}
