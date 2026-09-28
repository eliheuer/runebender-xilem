// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Saved Nodes graph commands and external node discovery.

use runebender::workflows::process::{self, ProcessCancellation, ProcessLimits, ProcessOutcome};

use super::font_commands::find_font_ml;
use super::*;

fn node_registry(tool: Option<&Path>) -> (nodes::Registry, Option<String>) {
    let mut registry = nodes::Registry::core();
    let Some(font_ml) = find_font_ml(tool) else {
        return (registry, None);
    };
    let output = process::run(
        std::process::Command::new(&font_ml).args(["tasks", "--json"]),
        &[],
        ProcessLimits {
            deadline: std::time::Duration::from_secs(2),
            ..ProcessLimits::default()
        },
        &ProcessCancellation::default(),
        |_, _| {},
    );
    if !matches!(output.outcome, ProcessOutcome::Exited { success: true, .. }) {
        return (registry, None);
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return (registry, None);
    };
    registry.add_tool("font-ml", &value);
    (registry, Some(font_ml.display().to_string()))
}

pub(super) fn nodes_types(tool: Option<&Path>, json: bool) -> i32 {
    let (registry, font_ml) = node_registry(tool);
    if json {
        println!(
            "{}",
            json!({ "ok": true, "tool": font_ml, "types": registry.types })
        );
    } else {
        for t in &registry.types {
            let ins: Vec<String> = t
                .inputs
                .iter()
                .map(|p| format!("{}:{}", p.name, p.kind))
                .collect();
            let outs: Vec<String> = t
                .outputs
                .iter()
                .map(|p| format!("{}:{}", p.name, p.kind))
                .collect();
            println!(
                "{:<18} {:<10} ({}) -> ({}){}",
                t.name,
                t.title,
                ins.join(", "),
                outs.join(", "),
                if t.implemented { "" } else { "  [not built]" }
            );
        }
        if font_ml.is_none() {
            eprintln!("font-ml not found: only core types listed");
        }
    }
    exit::OK
}

pub(super) fn nodes_check(file: &Path, tool: Option<&Path>, json: bool) -> i32 {
    let graph = match nodes::NodeGraph::load(file) {
        Ok(g) => g,
        Err(e) => return fail(json, exit::USAGE, &e),
    };
    let (registry, font_ml) = node_registry(tool);
    let problems = graph.validate(&registry);
    let order = graph.order().ok();
    if json {
        println!(
            "{}",
            json!({
                "ok": problems.is_empty(),
                "file": file,
                "tool": font_ml,
                "nodes": graph.nodes.len(),
                "links": graph.links.len(),
                "order": order,
                "problems": problems,
            })
        );
    } else {
        for p in &problems {
            eprintln!("{p}");
        }
        if problems.is_empty() {
            let order: Vec<String> = order
                .unwrap_or_default()
                .iter()
                .filter_map(|id| graph.node(*id))
                .map(|n| format!("{}:{}", n.id, n.type_name))
                .collect();
            println!(
                "{} nodes, {} links, runs: {}",
                graph.nodes.len(),
                graph.links.len(),
                order.join(" ")
            );
        }
    }
    if problems.is_empty() {
        exit::OK
    } else {
        exit::USAGE
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "one argument per flag the command takes"
)]
pub(super) fn nodes_run(
    file: &Path,
    font: &Path,
    master: Option<&str>,
    glyphs: Option<&[String]>,
    tool: Option<&Path>,
    models: Option<&Path>,
    force: bool,
    no_cache: bool,
    proposal_only: bool,
    json: bool,
) -> i32 {
    let graph = match nodes::NodeGraph::load(file) {
        Ok(g) => g,
        Err(e) => return fail(json, exit::USAGE, &e),
    };
    if !font.exists() {
        return fail(json, exit::USAGE, &format!("{}: not found", font.display()));
    }
    let (registry, _) = node_registry(tool);
    if proposal_only && let Err(e) = nodes_run::validate_proposal_workflow(&graph) {
        return fail(json, exit::USAGE, &e);
    }
    let problems = graph.validate(&registry);
    if !problems.is_empty() {
        let text: Vec<String> = problems.iter().map(ToString::to_string).collect();
        return fail(
            json,
            exit::USAGE,
            &format!("{} will not run:\n{}", file.display(), text.join("\n")),
        );
    }
    let mut tools = std::collections::BTreeMap::new();
    if let Some(font_ml) = find_font_ml(tool) {
        tools.insert("font-ml".to_string(), font_ml);
    }
    let mut on_event = |event: nodes_run::Event| match event {
        nodes_run::Event::Start {
            id,
            type_name,
            index,
            total,
        } => eprintln!("node {index}/{total} {id} {type_name}"),
        nodes_run::Event::Progress {
            done, total, label, ..
        } => eprintln!("progress {done}/{total} {label}"),
        nodes_run::Event::End {
            id,
            status,
            seconds,
            error,
        } => match error {
            Some(e) => eprintln!("node {id} failed: {e}"),
            None => eprintln!("node {id} {status:?} {seconds:.1}s"),
        },
    };
    let mut ctx = nodes_run::RunContext {
        font,
        master,
        glyphs: glyphs.map(<[String]>::to_vec).unwrap_or_default(),
        tools,
        models_dir: models
            .map(Path::to_path_buf)
            .or_else(nodes_run::default_models_dir),
        device: None,
        force,
        cache: (!no_cache).then(|| nodes_run::cache_path(file)),
        cancellation: ProcessCancellation::default(),
        process_limits: ProcessLimits::default(),
        on_event: &mut on_event,
    };
    let report = nodes_run::run(&graph, &registry, &mut ctx);
    if json {
        println!(
            "{}",
            serde_json::to_value(
                runebender::automation::agent_nodes::results::DiskNodesRunResult {
                    ok: report.ok,
                    file: file.to_owned(),
                    font: font.to_owned(),
                    nodes: report.nodes,
                }
            )
            .expect("typed disk graph result serializes")
        );
    } else {
        for n in &report.nodes {
            let note = match n.status {
                nodes_run::Status::Failed => n
                    .report
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("failed")
                    .to_string(),
                _ => n
                    .outputs
                    .iter()
                    .map(|(k, v)| match v {
                        nodes_run::RunValue::Layer { name, .. } => format!("{k}={name}"),
                        nodes_run::RunValue::Rows { rows } => format!("{k}={} rows", rows.len()),
                        nodes_run::RunValue::Path { path } => format!("{k}={}", path.display()),
                        _ => String::new(),
                    })
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" "),
            };
            println!(
                "{:<3} {:<18} {:<8} {note}",
                n.id,
                n.type_name,
                format!("{:?}", n.status).to_lowercase()
            );
        }
    }
    if report.ok { exit::OK } else { exit::FAILED }
}
