// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Resolve live graph connections to isolated font versions without saving the root.

use super::nodes::{Kind, NodeGraph, NodeType, Port};
use crate::automation::live;
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

/// Apply the connected version with the same conflict and undo rules as MCP.
/// Returns the live-command result. Does not save the source files.
pub fn apply(graph: &NodeGraph, project: &mut Project, id: u32) -> Result<Value, String> {
    let v = resolve(graph, project, id)?;
    let name = v
        .branch
        .ok_or("Connect an experimental version, not the root")?;
    let version = &project.experiments.versions[&name];
    let names = version.changed_glyphs();
    Ok(live::call(
        project,
        "experiment_apply",
        &json!({"source":v.source.0,"branch":name,"glyphs":names,"kerning":true,"keep_structure":false,"authorization":"user-approved"}),
    ))
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
            g.validate(&super::super::nodes::Registry::core())
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
            g.validate(&super::super::nodes::Registry::core())
                .iter()
                .any(|p| matches!(p, super::super::nodes::Problem::KindMismatch { .. }))
        );
    }

    #[test]
    fn forks_follow_wires_and_preserve_previous_results() {
        let mut p = Project::new_font("test.ufo".into());
        let source = p.source_id(0).unwrap();
        let mut g = starter(source);
        assert!(
            g.validate(&super::super::nodes::Registry::core())
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
}
