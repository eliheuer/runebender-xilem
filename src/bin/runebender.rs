// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Font operations from a shell.
//!
//! A thin shell over `runebender_core`, which is where the work
//! lives. Conventions match `font-ml`, so the two are driven the same
//! way: `--json` on every command, and exit codes that separate a
//! usage mistake from a real failure.

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use norad::Font;
use runebender_core::document::font_ops;
use runebender_core::document::nodes;
use runebender_core::document::project::Master;
use runebender_core::document::proposal;
use runebender_core::outline::embolden;
use runebender_core::outline::glyph_paths;
use serde_json::json;

/// Exit codes, matching font-ml so a caller can branch on them.
mod exit {
    /// Ran, and the answer is yes or the work is done.
    pub(crate) const OK: i32 = 0;
    /// The command was wrong: bad path, unknown glyph, missing flag.
    pub(crate) const USAGE: i32 = 2;
    /// The command was right and the tool it needs is not built yet.
    pub(crate) const NOT_BUILT: i32 = 3;
    /// The command was right and the work failed.
    pub(crate) const FAILED: i32 = 4;
}

/// Reports an error on stderr, or as JSON on stdout, and returns the
/// code to exit with.
fn fail(json: bool, code: i32, message: &str) -> i32 {
    if json {
        println!("{}", json!({ "ok": false, "error": message }));
    } else {
        eprintln!("{message}");
    }
    code
}

/// Loads one UFO, reporting a bad path as a usage error.
fn open(path: &Path, json: bool) -> Result<Font, i32> {
    Font::load(path).map_err(|e| fail(json, exit::USAGE, &format!("{}: {e}", path.display())))
}

#[derive(Parser)]
#[command(
    name = "runebender-core",
    about = "Font operations from a shell",
    long_about = "Font operations from a shell.\n\nThe same code the \
                  Runebender editor runs, without a window.\n\n\
                  Every command takes --json. Exit codes: 0 ok, \
                  2 usage, 4 failed."
)]
struct Cli {
    /// Machine-readable output.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// What a font is: names, metrics, counts, and any proposals
    /// waiting in it.
    Info {
        /// The UFO.
        source: PathBuf,
        /// List every glyph with its codepoints.
        #[arg(long)]
        glyphs: bool,
    },
    /// Draw a proof sheet as SVG, with metrics per glyph.
    Proof {
        /// The UFO.
        source: PathBuf,
        /// Where to write the SVG. Defaults to proof.svg next to the UFO.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Glyphs to draw. Defaults to every drawn glyph.
        #[arg(long, value_delimiter = ',')]
        glyphs: Option<Vec<String>>,
        /// Glyphs per row.
        #[arg(long, default_value = "10")]
        columns: usize,
    },
    /// Proposals: edits offered by a tool, waiting in the UFO as
    /// `com.runebender.proposal.<task>` layers.
    Proposal {
        #[command(subcommand)]
        action: ProposalAction,
    },
    /// Nodes: a workflow of tools as boxes and wires, in a
    /// `<name>.nodes.json` file.
    Nodes {
        #[command(subcommand)]
        action: NodesAction,
    },
    /// Run a font-ml task over the UFO. font-ml is its own program; it
    /// writes what it proposes into the UFO as a proposal layer, and
    /// this reports what arrived.
    Propose {
        /// The task, as `font-ml tasks` lists it.
        task: String,
        /// The UFO.
        source: PathBuf,
        /// A model directory to pass along.
        #[arg(long)]
        model: Option<PathBuf>,
        /// Glyphs to pass along. Defaults to the task's own choice.
        #[arg(long, value_delimiter = ',')]
        glyphs: Option<Vec<String>>,
        /// The font-ml binary. Defaults to `$RUNEBENDER_FONT_ML`, then
        /// `font-ml` on PATH.
        #[arg(long)]
        tool: Option<PathBuf>,
        /// Anything after `--` goes to font-ml as it is.
        #[arg(last = true)]
        rest: Vec<String>,
    },
    /// Learn how much weight a heavier master adds, from glyphs drawn
    /// in both, and report what it would do to the rest.
    Bolden {
        /// The lighter master.
        #[arg(long)]
        from: PathBuf,
        /// The heavier master, part-drawn.
        #[arg(long)]
        to: PathBuf,
        /// Glyphs to learn from. Defaults to n,o,H,O.
        #[arg(long, value_delimiter = ',')]
        references: Option<Vec<String>>,
        /// Glyphs to report on. Defaults to every one still identical
        /// in both masters, which is the work not yet done.
        #[arg(long, value_delimiter = ',')]
        glyphs: Option<Vec<String>>,
        /// Stop after this many.
        #[arg(long, default_value = "40")]
        limit: usize,
        /// Score the learned offset against glyphs drawn in both
        /// masters instead of listing what is undrawn.
        #[arg(long)]
        check: bool,
    },
}

#[derive(Subcommand)]
enum NodesAction {
    /// Read a file and report every problem. Exit 0 when it would run.
    Check {
        /// The `.nodes.json` file.
        file: PathBuf,
        /// The font-ml binary, for the tasks it declares. Defaults to
        /// `$RUNEBENDER_FONT_ML`, then `font-ml` on PATH.
        #[arg(long)]
        tool: Option<PathBuf>,
    },
    /// Every node type: core's, plus what font-ml declares.
    Types {
        /// The font-ml binary.
        #[arg(long)]
        tool: Option<PathBuf>,
    },
    /// The JSON Schema for the file.
    Schema,
}

#[derive(Subcommand)]
enum ProposalAction {
    /// Every proposal in the UFO, with what it changes.
    List {
        /// The UFO.
        source: PathBuf,
    },
    /// Copy a proposal over the foreground and save. Each glyph is
    /// one undo step in an editor that has the font open.
    Install {
        /// The UFO.
        source: PathBuf,
        /// The task whose proposal to install.
        #[arg(long)]
        task: String,
        /// Only these glyphs. Defaults to every glyph proposed.
        #[arg(long, value_delimiter = ',')]
        glyphs: Option<Vec<String>>,
        /// Install a glyph even when it changes point structure.
        #[arg(long)]
        any_structure: bool,
    },
    /// Drop a proposal and save.
    Discard {
        /// The UFO.
        source: PathBuf,
        /// The task whose proposal to drop.
        #[arg(long)]
        task: String,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let json = cli.json;
    let code = match &cli.command {
        Command::Info { source, glyphs } => info(source, *glyphs, json),
        Command::Proof {
            source,
            out,
            glyphs,
            columns,
        } => proof(source, out.as_deref(), glyphs.as_deref(), *columns, json),
        Command::Proposal { action } => match action {
            ProposalAction::List { source } => proposal_list(source, json),
            ProposalAction::Install {
                source,
                task,
                glyphs,
                any_structure,
            } => proposal_install(source, task, glyphs.as_deref(), !*any_structure, json),
            ProposalAction::Discard { source, task } => proposal_discard(source, task, json),
        },
        Command::Nodes { action } => match action {
            NodesAction::Check { file, tool } => nodes_check(file, tool.as_deref(), json),
            NodesAction::Types { tool } => nodes_types(tool.as_deref(), json),
            NodesAction::Schema => {
                // One line, like every other JSON this prints, so a
                // caller reads the last line and has it all.
                println!("{}", nodes::NodeGraph::schema());
                exit::OK
            }
        },
        Command::Propose {
            task,
            source,
            model,
            glyphs,
            tool,
            rest,
        } => propose(
            task,
            source,
            model.as_deref(),
            glyphs.as_deref(),
            tool.as_deref(),
            rest,
            json,
        ),
        Command::Bolden {
            from,
            to,
            references,
            glyphs,
            limit,
            check,
        } => bolden(
            from,
            to,
            references.as_deref(),
            glyphs.as_deref(),
            *limit,
            *check,
            json,
        ),
    };
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}

/// Loads one UFO as a `Master`, reporting a bad path as a usage error.
fn open_master(path: &Path, json: bool) -> Result<Master, i32> {
    Master::load(path).map_err(|e| fail(json, exit::USAGE, &format!("{}: {e}", path.display())))
}

/// Saves a master, reporting a write failure as such.
fn save_master(master: &mut Master, json: bool) -> Result<(), i32> {
    master.save().map_err(|e| {
        fail(
            json,
            exit::FAILED,
            &format!("{}: {e}", master.source_path.display()),
        )
    })
}

fn codepoints(glyph: &norad::Glyph) -> Vec<String> {
    glyph
        .codepoints
        .iter()
        .map(|c| format!("U+{:04X}", u32::from(c)))
        .collect()
}

/// What a font is, for a person or a program about to work on it.
fn info(source: &Path, list_glyphs: bool, json: bool) -> i32 {
    let master = match open_master(source, json) {
        Ok(m) => m,
        Err(code) => return code,
    };
    let font = &master.font;
    let drawn = font
        .default_layer()
        .iter()
        .filter(|g| !g.contours.is_empty() || !g.components.is_empty())
        .count();
    let proposals = proposal::list(font);
    let layers: Vec<String> = font.layers.names().map(|n| n.to_string()).collect();
    if json {
        let mut out = json!({
            "ok": true,
            "source": source,
            "family": font.font_info.family_name,
            "style": font.font_info.style_name,
            "unitsPerEm": master.units_per_em,
            "ascender": master.ascender,
            "descender": master.descender,
            "xHeight": master.x_height,
            "capHeight": master.cap_height,
            "glyphs": font.default_layer().len(),
            "drawn": drawn,
            "layers": layers,
            "kerningPairs": font.kerning.values().map(|v| v.len()).sum::<usize>(),
            "proposals": proposals,
        });
        if list_glyphs {
            out["glyphList"] = font
                .default_layer()
                .iter()
                .map(|g| json!({ "name": g.name(), "codepoints": codepoints(g) }))
                .collect();
        }
        println!("{out}");
    } else {
        println!(
            "{} {}",
            font.font_info
                .family_name
                .as_deref()
                .unwrap_or("(no family)"),
            font.font_info.style_name.as_deref().unwrap_or("")
        );
        println!(
            "{} upm, ascender {}, descender {}",
            master.units_per_em, master.ascender, master.descender
        );
        println!(
            "{} glyphs, {drawn} drawn, layers: {}",
            font.default_layer().len(),
            layers.join(", ")
        );
        for p in &proposals {
            println!(
                "proposal {}: {} glyphs ({} compatible, {} not, {} missing)",
                p.task,
                p.glyphs.len(),
                p.compatible.len(),
                p.incompatible.len(),
                p.missing.len()
            );
        }
        if list_glyphs {
            for g in font.default_layer().iter() {
                println!("  {:<24} {}", g.name(), codepoints(g).join(" "));
            }
        }
    }
    exit::OK
}

/// A proof sheet: every glyph in a grid with its metric lines, as
/// SVG, and the numbers a reviewer wants next to it.
fn proof(
    source: &Path,
    out: Option<&Path>,
    glyphs: Option<&[String]>,
    columns: usize,
    json: bool,
) -> i32 {
    let master = match open_master(source, json) {
        Ok(m) => m,
        Err(code) => return code,
    };
    let font = &master.font;
    let names: Vec<String> = match glyphs {
        Some(list) => list.to_vec(),
        None => master
            .glyphs
            .iter()
            .filter(|g| !g.path.is_empty())
            .map(|g| g.name.to_string())
            .collect(),
    };
    if names.is_empty() {
        return fail(json, exit::USAGE, "no glyph to draw");
    }
    let columns = columns.clamp(1, names.len());
    let upm = master.units_per_em;
    let cell_w = upm * 1.2;
    let cell_h = upm * 1.4;
    let rows = names.len().div_ceil(columns);
    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" \
         viewBox=\"0 0 {} {}\">\n<rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n",
        (cell_w * columns as f64 / 4.0).round(),
        (cell_h * rows as f64 / 4.0).round(),
        cell_w * columns as f64,
        cell_h * rows as f64
    ));
    let mut metrics = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let Some(glyph) = font.get_glyph(name.as_str()) else {
            return fail(json, exit::USAGE, &format!("no glyph named {name}"));
        };
        let path = glyph_paths::glyph_to_bezpath(glyph, font);
        let col = (i % columns) as f64;
        let row = (i / columns) as f64;
        let x0 = col * cell_w + upm * 0.1;
        let baseline = row * cell_h + upm * 1.05;
        let line = |y: f64, color: &str| {
            format!(
                "<line x1=\"{x0:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" \
                 stroke=\"{color}\" stroke-width=\"2\"/>\n",
                baseline - y,
                x0 + glyph.width,
                baseline - y
            )
        };
        svg.push_str(&line(0.0, "#999"));
        svg.push_str(&line(master.ascender, "#ccc"));
        svg.push_str(&line(master.descender, "#ccc"));
        if let Some(x) = master.x_height {
            svg.push_str(&line(x, "#bbb"));
        }
        if let Some(c) = master.cap_height {
            svg.push_str(&line(c, "#bbb"));
        }
        svg.push_str(&format!(
            "<path transform=\"translate({x0:.1} {baseline:.1}) scale(1 -1)\" d=\"{}\" fill=\"black\"/>\n",
            path.to_svg()
        ));
        use kurbo::Shape as _;
        let bounds = path.bounding_box();
        let drawn = !path.is_empty();
        metrics.push(json!({
            "glyph": name,
            "advance": glyph.width,
            "lsb": if drawn { Some(bounds.x0.round()) } else { None },
            "rsb": if drawn { Some((glyph.width - bounds.x1).round()) } else { None },
            "bounds": if drawn { Some([bounds.x0, bounds.y0, bounds.x1, bounds.y1]) } else { None },
            "points": glyph.contours.iter().map(|c| c.points.len()).sum::<usize>(),
            "contours": glyph.contours.len(),
            "components": glyph.components.len(),
        }));
    }
    svg.push_str("</svg>\n");
    let out = out.map_or_else(
        || {
            source
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("proof.svg")
        },
        Path::to_path_buf,
    );
    if let Err(e) = std::fs::write(&out, svg) {
        return fail(json, exit::FAILED, &format!("{}: {e}", out.display()));
    }
    if json {
        println!("{}", json!({ "ok": true, "svg": out, "glyphs": metrics }));
    } else {
        println!("{} glyphs → {}", names.len(), out.display());
        for m in &metrics {
            println!(
                "  {:<24} advance {:>5}  lsb {:>5}  rsb {:>5}",
                m["glyph"].as_str().unwrap_or(""),
                m["advance"],
                m["lsb"],
                m["rsb"]
            );
        }
    }
    exit::OK
}

fn proposal_list(source: &Path, json: bool) -> i32 {
    let font = match open(source, json) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let list = proposal::list(&font);
    if json {
        println!("{}", json!({ "ok": true, "proposals": list }));
    } else if list.is_empty() {
        println!("no proposals");
    } else {
        for p in &list {
            println!(
                "{}: {} glyphs, {} compatible, {} change structure, {} missing",
                p.task,
                p.glyphs.len(),
                p.compatible.len(),
                p.incompatible.len(),
                p.missing.len()
            );
            for (name, why) in &p.incompatible {
                println!("  {name}: {why}");
            }
        }
    }
    exit::OK
}

fn proposal_install(
    source: &Path,
    task: &str,
    glyphs: Option<&[String]>,
    keep_structure: bool,
    json: bool,
) -> i32 {
    let mut master = match open_master(source, json) {
        Ok(m) => m,
        Err(code) => return code,
    };
    let done = match master.install_proposal(task, glyphs, keep_structure) {
        Ok(done) => done,
        Err(e) => {
            if json {
                println!("{}", json!({ "ok": false, "error": e }));
            } else {
                eprintln!("{e}");
            }
            return exit::USAGE;
        }
    };
    if let Err(code) = save_master(&mut master, json) {
        return code;
    }
    if json {
        println!("{}", json!({ "ok": true, "installed": done }));
    } else {
        println!(
            "{}: installed {} glyphs, skipped {}",
            done.task,
            done.installed.len(),
            done.skipped.len()
        );
        for (name, why) in &done.skipped {
            println!("  {name}: {why}");
        }
    }
    exit::OK
}

fn proposal_discard(source: &Path, task: &str, json: bool) -> i32 {
    let mut master = match open_master(source, json) {
        Ok(m) => m,
        Err(code) => return code,
    };
    let count = match master.discard_proposal(task) {
        Ok(n) => n,
        Err(e) => {
            if json {
                println!("{}", json!({ "ok": false, "error": e }));
            } else {
                eprintln!("{e}");
            }
            return exit::USAGE;
        }
    };
    if let Err(code) = save_master(&mut master, json) {
        return code;
    }
    if json {
        println!(
            "{}",
            json!({ "ok": true, "task": task, "discarded": count })
        );
    } else {
        println!("{task}: dropped {count} proposed glyphs");
    }
    exit::OK
}

/// Where font-ml is: the flag, then `$RUNEBENDER_FONT_ML`, then PATH.
/// Every node type: core's, then font-ml's tasks when the tool
/// answers. `tool` is Some(name) when it answered, so a caller can
/// tell "not installed" from "declares nothing".
fn node_registry(tool: Option<&Path>) -> (nodes::Registry, Option<String>) {
    let mut registry = nodes::Registry::core();
    let Some(font_ml) = find_font_ml(tool) else {
        return (registry, None);
    };
    let Ok(output) = std::process::Command::new(&font_ml)
        .arg("tasks")
        .arg("--json")
        .output()
    else {
        return (registry, None);
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return (registry, None);
    };
    registry.add_tool("font-ml", &value);
    (registry, Some(font_ml.display().to_string()))
}

fn nodes_types(tool: Option<&Path>, json: bool) -> i32 {
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

fn nodes_check(file: &Path, tool: Option<&Path>, json: bool) -> i32 {
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

fn find_font_ml(tool: Option<&Path>) -> Option<PathBuf> {
    if let Some(t) = tool {
        return Some(t.to_path_buf());
    }
    if let Some(t) = std::env::var_os("RUNEBENDER_FONT_ML").filter(|t| !t.is_empty()) {
        return Some(PathBuf::from(t));
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("font-ml"))
        .find(|candidate| candidate.is_file())
}

/// What font-ml says it can do: each task name with whether it is
/// built. None when the tool does not answer, in which case the run
/// itself will say.
fn known_tasks(font_ml: &Path) -> Option<Vec<(String, bool)>> {
    let output = std::process::Command::new(font_ml)
        .arg("tasks")
        .arg("--json")
        .output()
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let tasks = value.get("tasks")?.as_array()?;
    Some(
        tasks
            .iter()
            .filter_map(|t| {
                Some((
                    t.get("name")?.as_str()?.to_string(),
                    t.get("implemented")?.as_bool().unwrap_or(false),
                ))
            })
            .collect(),
    )
}

/// Runs a font-ml task and reports the proposal it left behind.
///
/// font-ml is a separate program on purpose: it carries the model
/// runtime, and this crate does not. The seam is the UFO on disk and
/// the JSON font-ml prints: the task runs with `--write`, so what it
/// predicts lands in the UFO as a proposal layer and nothing touches
/// the foreground. Its exit codes are passed through, so a caller
/// that branches on them sees the same answers either way.
fn propose(
    task: &str,
    source: &Path,
    model: Option<&Path>,
    glyphs: Option<&[String]>,
    tool: Option<&Path>,
    rest: &[String],
    json: bool,
) -> i32 {
    if !source.is_dir() {
        return fail(
            json,
            exit::USAGE,
            &format!("{}: not a UFO directory", source.display()),
        );
    }
    let Some(font_ml) = find_font_ml(tool) else {
        return fail(
            json,
            exit::NOT_BUILT,
            "font-ml is not installed: set RUNEBENDER_FONT_ML, pass --tool, or put \
             font-ml on PATH (cargo install --git https://github.com/eliheuer/font-ml)",
        );
    };
    // The tool says what it can do; ask it before asking it to do
    // something, so an unknown task is a usage error with the list.
    if let Some(known) = known_tasks(&font_ml) {
        if !known.iter().any(|(name, _)| name == task) {
            let names: Vec<&str> = known.iter().map(|(n, _)| n.as_str()).collect();
            return fail(
                json,
                exit::USAGE,
                &format!("unknown task {task}; font-ml knows: {}", names.join(", ")),
            );
        }
        if known.iter().any(|(name, built)| name == task && !built) {
            return fail(
                json,
                exit::NOT_BUILT,
                &format!("{task} is a task font-ml names but has not built yet"),
            );
        }
    }
    let mut cmd = std::process::Command::new(&font_ml);
    cmd.arg("run").arg(task).arg("--source").arg(source);
    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }
    for g in glyphs.into_iter().flatten() {
        cmd.arg("--glyph").arg(g);
    }
    cmd.args(rest).arg("--write").arg("--json");
    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            return fail(
                json,
                exit::FAILED,
                &format!("could not run {}: {e}", font_ml.display()),
            );
        }
    };
    let code = output.status.code().unwrap_or(exit::FAILED);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let tool_report: serde_json::Value = stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str(line).ok())
        .unwrap_or_else(|| json!({ "raw": stdout.trim() }));
    let arrived = Font::load(source)
        .ok()
        .and_then(|f| proposal::find(&f, task).ok());
    if json {
        println!(
            "{}",
            json!({
                "ok": code == exit::OK,
                "tool": font_ml,
                "exit": code,
                "report": tool_report,
                "proposal": arrived,
            })
        );
    } else {
        print!("{stdout}");
        if code != exit::OK {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }
        match arrived {
            Some(p) => println!(
                "proposal {}: {} glyphs waiting ({} compatible)",
                p.task,
                p.glyphs.len(),
                p.compatible.len()
            ),
            None => println!("no proposal layer written for {task}"),
        }
    }
    code
}

/// What the reference glyphs say the heavier master should do.
///
/// Reports rather than writes. Seeing the offset and the list first is
/// the difference between a tool you can trust with a font and one you
/// run once and then undo.
fn bolden(
    from: &Path,
    to: &Path,
    references: Option<&[String]>,
    glyphs: Option<&[String]>,
    limit: usize,
    check: bool,
    json: bool,
) -> i32 {
    let (light, heavy) = match (open(from, json), open(to, json)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(code), _) | (_, Err(code)) => return code,
    };
    let default_refs = [
        "n".to_string(),
        "o".to_string(),
        "H".to_string(),
        "O".to_string(),
    ];
    let refs: &[String] = references.unwrap_or(&default_refs);
    let pairs: Vec<_> = refs
        .iter()
        .filter_map(|n| {
            Some((
                light.default_layer().get_glyph(n.as_str())?,
                heavy.default_layer().get_glyph(n.as_str())?,
            ))
        })
        .collect();
    let Some(offset) = embolden::learn_offset(&pairs) else {
        return fail(
            json,
            exit::USAGE,
            "no reference glyph is drawn and compatible in both masters",
        );
    };
    // What is left to do: glyphs whose heavier master still matches
    // the lighter one point for point.
    let todo: Vec<String> = match glyphs {
        Some(list) => list.to_vec(),
        None => light
            .default_layer()
            .iter()
            .filter(|g| !g.contours.is_empty() && g.components.is_empty())
            .filter(|g| {
                heavy
                    .default_layer()
                    .get_glyph(g.name().as_str())
                    .is_some_and(|h| {
                        font_ops::glyph_signature(h) == font_ops::glyph_signature(g)
                            && h.contours == g.contours
                    })
            })
            .map(|g| g.name().to_string())
            .collect(),
    };
    if check {
        return bolden_check(&light, &heavy, offset, glyphs, limit, json);
    }
    let mut rows = Vec::new();
    for name in todo.iter().take(limit) {
        let Some(g) = light.default_layer().get_glyph(name.as_str()) else {
            continue;
        };
        let out = embolden::embolden(g, offset);
        let moved = g
            .contours
            .iter()
            .flat_map(|c| c.points.iter())
            .zip(out.contours.iter().flat_map(|c| c.points.iter()))
            .filter(|(a, b)| a.x != b.x || a.y != b.y)
            .count();
        let points: usize = g.contours.iter().map(|c| c.points.len()).sum();
        rows.push((name.clone(), moved, points));
    }
    if json {
        println!(
            "{}",
            json!({
                "ok": true,
                "offset": { "x": offset.x, "y": offset.y },
                "references": refs,
                "pending": todo.len(),
                "glyphs": rows.iter().map(|(n, m, p)| json!({
                    "glyph": n, "pointsMoved": m, "points": p,
                })).collect::<Vec<_>>(),
            })
        );
    } else {
        println!(
            "learned from {} reference glyphs: push out {:.1} horizontally, \
             {:.1} vertically",
            pairs.len(),
            offset.x,
            offset.y
        );
        println!("{} glyphs still undrawn in the heavier master", todo.len());
        for (name, moved, points) in &rows {
            println!("  {name:<22} {moved}/{points} points would move");
        }
        if todo.len() > rows.len() {
            println!("  ... and {} more", todo.len() - rows.len());
        }
    }
    exit::OK
}

/// Score the learned offset where the answer is already known.
///
/// The same protocol the model is scored with: mean point error
/// against the heavier master somebody drew, next to the error from
/// shifting every point by the average amount. A method that cannot
/// beat that constant is not carrying its weight.
fn bolden_check(
    light: &Font,
    heavy: &Font,
    offset: embolden::Offset,
    glyphs: Option<&[String]>,
    limit: usize,
    json: bool,
) -> i32 {
    let names: Vec<String> = match glyphs {
        Some(list) => list.to_vec(),
        None => light
            .default_layer()
            .iter()
            .filter(|g| !g.contours.is_empty() && g.components.is_empty())
            .filter(|g| {
                heavy
                    .default_layer()
                    .get_glyph(g.name().as_str())
                    .is_some_and(|h| {
                        font_ops::glyph_signature(h) == font_ops::glyph_signature(g)
                            && h.contours != g.contours
                    })
            })
            .map(|g| g.name().to_string())
            .collect(),
    };
    let flat = |g: &norad::Glyph| -> Vec<(f64, f64)> {
        g.contours
            .iter()
            .flat_map(|c| c.points.iter().map(|p| (p.x, p.y)))
            .collect()
    };
    let mut rows = Vec::new();
    let (mut sum_dx, mut sum_dy, mut n) = (0.0, 0.0, 0_usize);
    for name in names.iter().take(limit) {
        let (Some(l), Some(h)) = (
            light.default_layer().get_glyph(name.as_str()),
            heavy.default_layer().get_glyph(name.as_str()),
        ) else {
            continue;
        };
        let (a, b) = (flat(l), flat(h));
        if a.len() != b.len() || a.is_empty() {
            continue;
        }
        for (p, q) in a.iter().zip(&b) {
            sum_dx += q.0 - p.0;
            sum_dy += q.1 - p.1;
            n += 1;
        }
        let pred = flat(&embolden::embolden(l, offset));
        let err = pred
            .iter()
            .zip(&b)
            .map(|(p, q)| (p.0 - q.0).abs() + (p.1 - q.1).abs())
            .sum::<f64>()
            / (a.len() as f64 * 2.0);
        rows.push((name.clone(), err, a, b));
    }
    if rows.is_empty() || n == 0 {
        return fail(json, exit::FAILED, "no glyph is drawn in both masters");
    }
    let (mx, my) = (sum_dx / n as f64, sum_dy / n as f64);
    let mut offset_total = 0.0;
    let mut base_total = 0.0;
    let mut wins = 0_usize;
    let mut per = Vec::new();
    for (name, err, a, b) in &rows {
        let base = a
            .iter()
            .zip(b)
            .map(|(p, q)| (p.0 + mx - q.0).abs() + (p.1 + my - q.1).abs())
            .sum::<f64>()
            / (a.len() as f64 * 2.0);
        offset_total += err;
        base_total += base;
        if *err < base {
            wins += 1;
        }
        per.push(json!({ "glyph": name, "offset": err, "baseline": base }));
    }
    let count = rows.len() as f64;
    if json {
        println!(
            "{}",
            json!({
                "ok": true, "glyphs": rows.len(),
                "offset_mae": offset_total / count,
                "baseline_mae": base_total / count,
                "beats_baseline": wins,
                "per_glyph": per,
            })
        );
    } else {
        println!(
            "{} glyphs drawn in both: offset {:.1}, baseline {:.1}, \
             offset wins on {wins}",
            rows.len(),
            offset_total / count,
            base_total / count
        );
    }
    exit::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exit codes are the interface for a script, so they are pinned.
    #[test]
    fn exit_codes_are_distinct() {
        let codes = [exit::OK, exit::USAGE, exit::NOT_BUILT, exit::FAILED];
        let mut sorted = codes.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), codes.len(), "exit codes must not collide");
        assert_eq!(exit::OK, 0, "0 must mean success");
    }

    #[test]
    fn the_cli_parses() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
