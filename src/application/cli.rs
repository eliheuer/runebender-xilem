// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Font operations from a shell.
//!
//! A thin shell over Runebender's font modules, where the work lives.
//! Conventions match `font-ml`, so the two are driven the same
//! way: `--json` on every command, and exit codes that separate a
//! usage mistake from a real failure.

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use runebender::document::agent;
use runebender::document::compose;
use runebender::document::nodes;
use runebender::document::nodes_run;
use runebender::document::project::Project;
use runebender::document::proposal;
use runebender::document::variable::{GlyphLayerAddress, LayerId};
use runebender::outline::embolden;
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

#[derive(Parser)]
#[command(
    name = "runebender",
    version,
    subcommand_precedence_over_arg = true,
    about = "Runebender font editor and headless font tools",
    long_about = "Runebender font editor and headless font tools.\n\nOpen the editor with no \
                  arguments, or pass a UFO or designspace path. Subcommands run without \
                  opening a window.\n\nUse --json for machine-readable output. \
                  Exit codes: 0 ok, 2 usage, 3 not built, 4 failed."
)]
struct Cli {
    /// Machine-readable output.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Option<Command>,
    /// A UFO or designspace to open in the editor.
    #[arg(exclusive = true)]
    font: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Compile a UFO or Designspace with the same Rust pipeline as live preview.
    Compile {
        /// Editable source to read without modifying it.
        source: PathBuf,
        /// New TTF file to write.
        #[arg(long)]
        out: PathBuf,
    },
    /// Convert all live metaballs in a UFO to cubic outlines in a new UFO.
    CollapseMetaballs {
        /// Input UFO; never modified.
        source: PathBuf,
        /// New output UFO directory. Must not already exist.
        #[arg(long)]
        out: PathBuf,
        /// Sampling grid spacing in font units; smaller captures finer details.
        #[arg(long, default_value = "2")]
        resolution: f64,
        /// Cubic fitting accuracy relative to the sampled boundary, in font units.
        #[arg(long, default_value = "0.25")]
        accuracy: f64,
    },

    /// List local editor socket paths (a crashed editor may leave a stale entry).
    Sessions,
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
        /// Optional UFO layer to proof.
        #[arg(long)]
        layer: Option<String>,
    },
    /// Proposals: edits offered by a tool, waiting in the UFO as
    /// `com.runebender.proposal.<task>` layers.
    Proposal {
        #[command(subcommand)]
        action: ProposalAction,
    },
    /// The harness a language model works the font through: the
    /// prompt, the tool list, and one call at a time.
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
    /// An MCP server over stdio: the same tool list as `agent`, for
    /// a chat client that speaks the Model Context Protocol. Disk edits
    /// remain proposals; live tools can apply explicitly authorized edits.
    Mcp {
        /// The designspace or UFO the client works on.
        #[arg(long, required_unless_present_any = ["session", "live"], conflicts_with_all = ["session", "live"])]
        font: Option<PathBuf>,
        /// The live editor socket. Never reads or writes source files.
        #[arg(long)]
        session: Option<PathBuf>,
        /// Discover and connect to editor sessions through tools, without changing client config.
        #[arg(long, conflicts_with = "session")]
        live: bool,
        /// The font-ml binary, for propose.
        #[arg(long)]
        tool: Option<PathBuf>,
    },
    /// Derive precomposed glyphs from their base and marks through
    /// anchors, into the `com.runebender.proposal.compose` layer.
    Compose {
        /// The UFO.
        source: PathBuf,
        /// Which glyphs. Defaults to every glyph that has a recipe.
        #[arg(long, value_delimiter = ',')]
        glyphs: Option<Vec<String>>,
        /// Write the proposal layer. Without this, only report.
        #[arg(long)]
        write: bool,
    },
    /// Write `mark` and `mkmk` features from the font's anchors, the
    /// ones the editor shapes with, so a compiled font positions marks
    /// the same way.
    Features {
        /// The UFO.
        source: PathBuf,
        /// Write `features.generated.fea` into the UFO and add its
        /// include line to `features.fea`. Without this, print the
        /// text.
        #[arg(long)]
        write: bool,
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
enum AgentAction {
    /// Open a font as an unsaved headless editor session for scripts and MCP (Unix only).
    Serve {
        /// Designspace or UFO to load into memory. This host never saves source files.
        #[arg(long)]
        font: PathBuf,
        /// Glyph to select for state checks and ordinary undo/redo.
        #[arg(long)]
        glyph: String,
        /// Maximum lifetime; keep stdin open. All unsaved edits disappear on exit.
        #[arg(long, default_value_t = 3600, value_parser = clap::value_parser!(u64).range(1..=86400))]
        duration_seconds: u64,
    },
    /// Serve a synthetic unsaved application fixture for headless live-client tests (Unix only).
    Fixture {
        /// Maximum lifetime; keep stdin open and send newline-delimited control JSON.
        #[arg(long, default_value_t = 300, value_parser = clap::value_parser!(u64).range(1..=3600))]
        duration_seconds: u64,
    },
    /// The system prompt and every tool, as JSON.
    Tools,
    /// Run one tool call and print its result as JSON.
    Call {
        /// The tool name.
        name: String,
        /// The designspace or UFO the model is working on.
        #[arg(long, required_unless_present = "session", conflicts_with = "session")]
        font: Option<PathBuf>,
        /// The live editor socket. Never reads or writes source files.
        #[arg(long)]
        session: Option<PathBuf>,
        /// The arguments, as a JSON object.
        #[arg(long, default_value = "{}")]
        args: String,
        /// Read arguments from a JSON file, or - for stdin.
        #[arg(long, conflicts_with = "args")]
        args_file: Option<PathBuf>,
        /// The font-ml binary, for propose.
        #[arg(long)]
        tool: Option<PathBuf>,
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
    /// Every node type: the engine's, plus what font-ml declares.
    Types {
        /// The font-ml binary.
        #[arg(long)]
        tool: Option<PathBuf>,
    },
    /// Run a file against a font: every node in order, skipping what
    /// has not changed since the last run. Progress on stderr, one
    /// JSON report on stdout with --json.
    Run {
        /// The `.nodes.json` file.
        file: PathBuf,
        /// The designspace or UFO the Font node stands for.
        #[arg(long)]
        font: PathBuf,
        /// The master the Font node gives, by style name. The first
        /// when not given.
        #[arg(long)]
        master: Option<String>,
        /// The glyphs the Font node gives. Every drawn glyph when not
        /// given.
        #[arg(long, value_delimiter = ',')]
        glyphs: Option<Vec<String>>,
        /// The font-ml binary.
        #[arg(long)]
        tool: Option<PathBuf>,
        /// Where models live. Defaults to `$RUNEBENDER_MODELS`, then
        /// `~/.runebender/models`.
        #[arg(long)]
        models: Option<PathBuf>,
        /// Run every node, cached or not.
        #[arg(long)]
        force: bool,
        /// Keep no cache and read none.
        #[arg(long)]
        no_cache: bool,
        /// Reject workflow nodes that can write foreground data.
        #[arg(long)]
        proposal_only: bool,
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

/// Startup either opens the editor or completes a headless command.
#[derive(Debug)]
pub(crate) enum Startup {
    Editor(Option<PathBuf>),
    Exit(std::process::ExitCode),
}

/// Reject mixed editor paths and headless commands without restricting global flags.
fn parse_args<I, T>(args: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    use clap::CommandFactory as _;
    let cli = Cli::try_parse_from(args)?;
    if cli.font.is_some() && cli.command.is_some() {
        return Err(Cli::command().error(
            clap::error::ErrorKind::ArgumentConflict,
            "Use either an editor font path or a headless subcommand",
        ));
    }
    Ok(cli)
}

/// Parse arguments and finish headless work before any window setup.
pub(crate) fn run() -> Startup {
    let cli = match parse_args(std::env::args_os()) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return Startup::Exit(std::process::ExitCode::from(
                u8::try_from(error.exit_code()).unwrap_or(1),
            ));
        }
    };
    let json = cli.json;
    let Some(command) = cli.command else {
        if json {
            let _ = fail(true, exit::USAGE, "--json requires a headless subcommand");
            return Startup::Exit(std::process::ExitCode::from(2));
        }
        if let Some(path) = &cli.font
            && !path.exists()
        {
            let _ = fail(
                false,
                exit::USAGE,
                &format!("{}: font path does not exist", path.display()),
            );
            return Startup::Exit(std::process::ExitCode::from(2));
        }
        return Startup::Editor(cli.font);
    };
    let code = match &command {
        Command::Compile { source, out } => {
            let result = (|| -> Result<usize, String> {
                use std::io::Write as _;
                let project = Project::load(source)?;
                let compiled = project.compile()?;
                let mut file = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(out)
                    .map_err(|error| error.to_string())?;
                file.write_all(&compiled.bytes)
                    .map_err(|error| error.to_string())?;
                Ok(compiled.bytes.len())
            })();
            match result {
                Ok(bytes) => {
                    if json {
                        println!("{}", json!({"ok":true,"output":out,"bytes":bytes}));
                    } else {
                        println!("Compiled {} ({bytes} bytes)", out.display());
                    }
                    0
                }
                Err(error) => fail(json, exit::USAGE, &error),
            }
        }
        Command::Info { source, glyphs } => info(source, *glyphs, json),
        Command::Proof {
            source,
            out,
            glyphs,
            columns,
            layer,
        } => proof(
            source,
            out.as_deref(),
            glyphs.as_deref(),
            *columns,
            layer.as_deref(),
            json,
        ),
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
        Command::Agent { action } => match action {
            AgentAction::Serve {
                font,
                glyph,
                duration_seconds,
            } => {
                #[cfg(unix)]
                match crate::application::platform::live_host::serve_font(
                    font,
                    glyph,
                    std::time::Duration::from_secs(*duration_seconds),
                ) {
                    Ok(()) => exit::OK,
                    Err(error) => fail(true, exit::FAILED, &error),
                }
                #[cfg(not(unix))]
                {
                    let _ = (font, glyph, duration_seconds);
                    fail(true, exit::USAGE, "headless live sessions require Unix")
                }
            }
            AgentAction::Fixture { duration_seconds } => {
                #[cfg(unix)]
                match crate::application::platform::live_fixture::serve(
                    std::time::Duration::from_secs(*duration_seconds),
                ) {
                    Ok(()) => exit::OK,
                    Err(error) => fail(true, exit::FAILED, &error),
                }
                #[cfg(not(unix))]
                {
                    let _ = duration_seconds;
                    fail(true, exit::USAGE, "live fixtures require Unix")
                }
            }

            AgentAction::Tools => {
                let live = std::env::var_os("RUNEBENDER_LIVE_SESSION").is_some();
                let tools = if live {
                    runebender::document::live::tools()
                } else {
                    agent::tools()
                };
                let prompt = if live {
                    runebender::document::live::system_prompt(&tools)
                } else {
                    agent::system_prompt(&tools)
                };
                println!(
                    "{}",
                    json!({ "ok": true, "prompt": prompt, "tools": tools })
                );
                exit::OK
            }
            AgentAction::Call {
                name,
                font,
                args,
                args_file,
                session,
                tool,
            } => {
                let input = match args_file {
                    Some(path) if path.as_os_str() == "-" => {
                        std::io::read_to_string(std::io::stdin())
                    }
                    Some(path) => std::fs::read_to_string(path),
                    None => Ok(args.clone()),
                };
                match input {
                    Ok(input) => agent_call(
                        name,
                        font.as_deref(),
                        session.as_deref(),
                        &input,
                        tool.as_deref(),
                    ),
                    Err(e) => fail(true, exit::USAGE, &e.to_string()),
                }
            }
        },
        Command::CollapseMetaballs {
            source,
            out,
            resolution,
            accuracy,
        } => collapse_metaballs(source, out, *resolution, *accuracy, json),
        Command::Sessions => {
            #[cfg(unix)]
            println!(
                "{}",
                json!({"ok": true, "sessions": runebender::document::live_socket::sessions()})
            );
            #[cfg(not(unix))]
            println!(
                "{}",
                json!({"ok": false, "error": "live sockets require Unix"})
            );
            exit::OK
        }
        Command::Mcp {
            font,
            session,
            live,
            tool,
        } => mcp_serve(font.as_deref(), session.as_deref(), *live, tool.as_deref()),
        Command::Compose {
            source,
            glyphs,
            write,
        } => compose_cmd(source, glyphs.as_deref(), *write, json),
        Command::Features { source, write } => features_cmd(source, *write, json),
        Command::Nodes { action } => match action {
            NodesAction::Check { file, tool } => nodes_check(file, tool.as_deref(), json),
            NodesAction::Types { tool } => nodes_types(tool.as_deref(), json),
            NodesAction::Run {
                file,
                font,
                master,
                glyphs,
                tool,
                models,
                force,
                no_cache,
                proposal_only,
            } => nodes_run(
                file,
                font,
                master.as_deref(),
                glyphs.as_deref(),
                tool.as_deref(),
                models.as_deref(),
                *force,
                *no_cache,
                *proposal_only,
                json,
            ),
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
    Startup::Exit(std::process::ExitCode::from(
        u8::try_from(code).unwrap_or(1),
    ))
}

/// Load one canonical source for a headless command.
fn open_project(path: &Path, json: bool) -> Result<Project, i32> {
    let project = Project::load(path)
        .map_err(|error| fail(json, exit::USAGE, &format!("{}: {error}", path.display())))?;
    if project.document_sources().count() != 1 {
        return Err(fail(
            json,
            exit::USAGE,
            "this command requires a single source; open the variable project in the editor",
        ));
    }
    Ok(project)
}

/// Save a canonical Project, reporting a write failure as such.
fn save_project(project: &mut Project, json: bool) -> Result<(), i32> {
    let path = project
        .document_sources()
        .next()
        .map(|source| source.path().to_path_buf())
        .unwrap_or_default();
    project
        .save()
        .map_err(|error| fail(json, exit::FAILED, &format!("{}: {error}", path.display())))
}

fn codepoints(codepoints: impl Iterator<Item = char>) -> Vec<String> {
    codepoints
        .map(|c| format!("U+{:04X}", u32::from(c)))
        .collect()
}

/// What a font is, for a person or a program about to work on it.
fn info(source: &Path, list_glyphs: bool, json: bool) -> i32 {
    let project = match open_project(source, json) {
        Ok(project) => project,
        Err(code) => return code,
    };
    let source_id = project
        .source_id(0)
        .expect("one source has a stable identity");
    let default_layer = project
        .document_source(source_id)
        .expect("one source")
        .default_layer();
    let glyph_names = project
        .glyph_names()
        .filter(|name| project.document_layer(name, &default_layer).is_some())
        .collect::<Vec<_>>();
    let drawn = glyph_names
        .iter()
        .filter(|name| {
            project
                .document_layer(name, &default_layer)
                .is_some_and(|layer| {
                    layer.contours().next().is_some() || layer.components().next().is_some()
                })
        })
        .count();
    let proposals = proposal::list_project(&project, source_id);
    let layers = project
        .document_source_layer_names(source_id)
        .expect("one source retains layer structure");
    let info = project
        .document_font_info(source_id)
        .expect("one source retains canonical font information");
    let metrics = info.metrics.resolved();
    let metadata = project
        .document_font_metadata(source_id)
        .expect("one source retains canonical metadata");
    if json {
        let mut out = json!({
            "ok": true,
            "source": source,
            "family": info.names.family_name,
            "style": info.names.style_name,
            "unitsPerEm": metrics.units_per_em,
            "ascender": metrics.ascender,
            "descender": metrics.descender,
            "xHeight": info.metrics.x_height,
            "capHeight": info.metrics.cap_height,
            "glyphs": glyph_names.len(),
            "drawn": drawn,
            "layers": layers,
            "kerningPairs": metadata.kerning_pairs().count(),
            "proposals": proposals,
        });
        if list_glyphs {
            out["glyphList"] = glyph_names
                .iter()
                .map(|name| {
                    let layer = project
                        .document_layer(name, &default_layer)
                        .expect("collected default-layer glyph");
                    json!({ "name": name, "codepoints": codepoints(layer.codepoints()) })
                })
                .collect();
        }
        println!("{out}");
    } else {
        println!(
            "{} {}",
            info.names.family_name.as_deref().unwrap_or("(no family)"),
            info.names.style_name.as_deref().unwrap_or("")
        );
        println!(
            "{} upm, ascender {}, descender {}",
            metrics.units_per_em, metrics.ascender, metrics.descender
        );
        println!(
            "{} glyphs, {drawn} drawn, layers: {}",
            glyph_names.len(),
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
            for name in glyph_names {
                let glyph = project
                    .document_layer(name, &default_layer)
                    .expect("collected default-layer glyph");
                println!("  {name:<24} {}", codepoints(glyph.codepoints()).join(" "));
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
    layer: Option<&str>,
    json: bool,
) -> i32 {
    let project = match open_project(source, json) {
        Ok(project) => project,
        Err(code) => return code,
    };
    let source_id = project
        .source_id(0)
        .expect("one source has a stable identity");
    let default_layer = project
        .document_source(source_id)
        .expect("one source")
        .default_layer();
    let requested_layer = layer.map(|name| LayerId {
        source: source_id,
        name: name.into(),
    });
    let names: Vec<String> = match glyphs {
        Some(list) => list.to_vec(),
        None if requested_layer.is_some() => {
            let requested = requested_layer.as_ref().expect("checked requested layer");
            if !project
                .document_source_layer_names(source_id)
                .is_some_and(|names| names.iter().any(|name| *name == requested.name))
            {
                return fail(json, exit::USAGE, "no such layer");
            }
            project
                .glyph_names()
                .filter(|name| project.document_layer(name, requested).is_some())
                .map(str::to_owned)
                .collect()
        }
        None => project
            .glyph_names()
            .filter(|name| {
                project
                    .document_layer_path(&GlyphLayerAddress {
                        glyph: (*name).to_owned(),
                        layer: default_layer.clone(),
                    })
                    .is_ok_and(|path| !path.is_empty())
            })
            .map(str::to_owned)
            .collect(),
    };
    if names.is_empty() {
        return fail(json, exit::USAGE, "no glyph to draw");
    }
    let sheet = match runebender::formats::svg::proof_sheet_project(
        &project, source_id, layer, &names, columns,
    ) {
        Ok(s) => s,
        Err(e) => return fail(json, exit::USAGE, &e),
    };
    let (svg, metrics) = (sheet.svg, sheet.metrics);
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
    let project = match open_project(source, json) {
        Ok(project) => project,
        Err(code) => return code,
    };
    let source_id = project
        .source_id(0)
        .expect("one source has a stable identity");
    let list = proposal::list_project(&project, source_id);
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
    let mut project = match open_project(source, json) {
        Ok(project) => project,
        Err(code) => return code,
    };
    let source_id = project
        .source_id(0)
        .expect("one source has a stable identity");
    let done =
        match proposal::install_project(&mut project, source_id, task, glyphs, keep_structure) {
            Ok(done) => done.installed,
            Err(e) => {
                if json {
                    println!("{}", json!({ "ok": false, "error": e }));
                } else {
                    eprintln!("{e}");
                }
                return exit::USAGE;
            }
        };
    if let Err(code) = save_project(&mut project, json) {
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
    let mut project = match open_project(source, json) {
        Ok(project) => project,
        Err(code) => return code,
    };
    let source_id = project
        .source_id(0)
        .expect("one source has a stable identity");
    let count = match proposal::discard_project(&mut project, source_id, task) {
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
    if let Err(code) = save_project(&mut project, json) {
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
/// `compose`: derive, report, and with --write leave the proposal.
/// `features`: the mark features the anchors imply, printed or
/// written beside `features.fea` with an include line.
fn features_cmd(source: &Path, write: bool, json: bool) -> i32 {
    use runebender::text::features;
    if write
        && !source
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ufo"))
    {
        return fail(json, exit::USAGE, "features --write requires a UFO source");
    }
    let project = match Project::load(source) {
        Ok(project) => project,
        Err(error) => {
            return fail(json, exit::USAGE, &format!("{}: {error}", source.display()));
        }
    };
    let mut sources = project.document_sources();
    let Some(selected) = sources.next() else {
        return fail(json, exit::USAGE, "the font has no source");
    };
    if sources.next().is_some() {
        return fail(
            json,
            exit::USAGE,
            "features requires one UFO source, not a variable project",
        );
    }
    let source_id = selected.id();
    let Some(generated) = features::generate_project(&project, source_id) else {
        return fail(
            json,
            exit::FAILED,
            "the source has no canonical feature inputs",
        );
    };
    let own_mark = features::defines_mark_features(
        project
            .document_feature_text(source_id)
            .expect("the selected source has canonical feature text"),
    );
    let written = if write {
        match features::write(source, &generated, true) {
            Ok((path, included)) => Some((path, included)),
            Err(e) => return fail(json, exit::FAILED, &e),
        }
    } else {
        None
    };
    if json {
        println!(
            "{}",
            json!({
                "ok": true,
                "classes": generated.classes,
                "marks": generated.marks,
                "bases": generated.bases,
                "stacked": generated.stacked,
                "empty": generated.is_empty(),
                "features_fea_defines_mark": own_mark,
                "written": written.as_ref().map(|(p, _)| p),
                "included": written.as_ref().map(|(_, i)| *i),
                "fea": if write { serde_json::Value::Null } else { json!(generated.fea) },
            })
        );
    } else {
        match &written {
            Some((path, included)) => {
                println!(
                    "{}: {} classes, {} marks, {} bases, {} stacked; include line {}",
                    path.display(),
                    generated.classes.len(),
                    generated.marks,
                    generated.bases,
                    generated.stacked,
                    if *included {
                        "present"
                    } else if own_mark {
                        "not added: features.fea defines mark features"
                    } else {
                        "not added"
                    }
                );
            }
            None => print!("{}", generated.fea),
        }
    }
    exit::OK
}

fn compose_cmd(source: &Path, glyphs: Option<&[String]>, write: bool, json: bool) -> i32 {
    let mut project = match Project::load(source) {
        Ok(project) => project,
        Err(error) => {
            return fail(json, exit::USAGE, &format!("{}: {error}", source.display()));
        }
    };
    let source_id = {
        let mut sources = project.document_sources();
        let Some(selected) = sources.next() else {
            return fail(json, exit::USAGE, "the font has no source");
        };
        if sources.next().is_some() {
            return fail(
                json,
                exit::USAGE,
                "compose requires one UFO source, not a variable project",
            );
        }
        selected.id()
    };
    let plan = match compose::plan_project(&project, source_id, glyphs) {
        Ok(plan) => plan,
        Err(error) => return fail(json, exit::FAILED, &format!("compose: {error}")),
    };
    let report = if write && !plan.replacements.is_empty() {
        match proposal::write_composition_project(&mut project, source_id, plan) {
            Ok(report) => report,
            Err(error) => return fail(json, exit::FAILED, &format!("compose: {error}")),
        }
    } else {
        plan.report
    };
    if write
        && report.proposal.is_some()
        && let Err(error) = project.save()
    {
        return fail(
            json,
            exit::FAILED,
            &format!("{}: {error}", source.display()),
        );
    }
    if json {
        println!(
            "{}",
            json!({
                "ok": true,
                "derived": report.derived,
                "proposed": report.proposed(),
                "skipped": report.skipped,
                "proposal": report.proposal,
            })
        );
    } else {
        for d in &report.derived {
            let parts: Vec<String> = d
                .components
                .iter()
                .map(|(n, x, y)| format!("{n}@{x:.0},{y:.0}"))
                .collect();
            println!(
                "{:<28} {:<10} {}{}",
                d.glyph,
                format!("{:?}", d.recipe.source).to_lowercase(),
                parts.join(" + "),
                if d.up_to_date { "  (up to date)" } else { "" }
            );
        }
        for (g, why) in &report.skipped {
            println!("{g:<28} skipped: {why}");
        }
        match &report.proposal {
            Some(p) => println!("proposal {}: {} glyphs", p.task, p.glyphs.len()),
            None if write => println!("nothing to propose"),
            None => {}
        }
    }
    exit::OK
}

/// Every node type: the engine's, then font-ml's tasks when the tool
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

#[expect(
    clippy::too_many_arguments,
    reason = "one argument per flag the command takes"
)]
fn nodes_run(
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
        on_event: &mut on_event,
    };
    let report = nodes_run::run(&graph, &registry, &mut ctx);
    if json {
        println!(
            "{}",
            json!({ "ok": report.ok, "file": file, "font": font, "nodes": report.nodes })
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

/// The UFO a font path stands for: the UFO itself, or the first
/// master of a designspace.
fn font_master(font: &Path, master: Option<usize>) -> Result<PathBuf, String> {
    let project = Project::load(font)?;
    let sources = project.document_sources().collect::<Vec<_>>();
    let index = match master {
        Some(index) => index,
        None if sources.len() == 1 => 0,
        None => return Err("master is required for a family; call project_info first".into()),
    };
    sources
        .get(index)
        .map(|source| source.path().to_path_buf())
        .ok_or_else(|| format!("no master at index {index}"))
}

fn project_info(font: &Path) -> serde_json::Value {
    match Project::load(font) {
        Ok(project) => {
            json!({"ok": true, "project": font, "masters": project.document_sources().enumerate()
            .map(|(index, source)| json!({"index": index, "name": source.name(), "source": source.path()}))
            .collect::<Vec<_>>()})
        }
        Err(e) => json!({"ok": false, "error": e}),
    }
}

/// Runs this same binary with `args` and returns the last JSON line
/// it printed. Every tool is a command the binary already has, so the
/// model's reach is exactly the command line's.
fn self_json(args: &[String]) -> serde_json::Value {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("runebender"));
    let output = std::process::Command::new(exe).args(args).output();
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout
                .lines()
                .rev()
                .find_map(|l| serde_json::from_str(l).ok())
                .unwrap_or_else(
                    || json!({ "ok": false, "error": String::from_utf8_lossy(&o.stderr).trim() }),
                )
        }
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
}

/// One glyph as the model reads it.
fn read_glyph(source: &Path, name: &str, layer: Option<&str>) -> serde_json::Value {
    let project = match Project::load(source) {
        Ok(project) => project,
        Err(error) => return json!({ "ok": false, "error": error }),
    };
    let Some(source_id) = project.document_sources().next().map(|source| source.id()) else {
        return json!({ "ok": false, "error": "the font has no source" });
    };
    runebender::analysis::glyph::read_project_glyph(&project, source_id, name, layer)
}

/// Searches the documentation folders for passages that match.
///
/// Roots: `$RUNEBENDER_DOCS` (colon-separated) and
/// `~/.runebender/docs`. Every `.md`, `.txt` and `.html` file is split
/// into paragraphs; a paragraph scores one per query word it holds,
/// and the top five come back with their file. Plain and offline: a
/// model that reads the spec beats one that remembers it.
fn docs_search(query: &str) -> serde_json::Value {
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        return json!({ "ok": false, "error": "give a few words to look for" });
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(extra) = std::env::var_os("RUNEBENDER_DOCS") {
        roots.extend(std::env::split_paths(&extra));
    }
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".runebender").join("docs"));
    }
    let mut hits: Vec<(usize, String, String)> = Vec::new();
    let mut stack: Vec<PathBuf> = roots.iter().filter(|r| r.is_dir()).cloned().collect();
    let mut files = 0;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "md" | "txt" | "html" | "mdx") {
                continue;
            }
            let Ok(mut text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if ext == "html" {
                text = strip_html(&text);
            }
            files += 1;
            for para in text.split("\n\n") {
                let lower = para.to_lowercase();
                let score = words.iter().filter(|w| lower.contains(w.as_str())).count();
                if score > 0 {
                    let snippet: String = para.chars().take(600).collect();
                    hits.push((
                        score,
                        path.display().to_string(),
                        snippet.trim().to_string(),
                    ));
                }
            }
        }
    }
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    hits.truncate(5);
    json!({
        "ok": true,
        "files_searched": files,
        "roots": roots,
        "passages": hits.iter().map(|(score, file, text)| json!({ "score": score, "file": file, "text": text })).collect::<Vec<_>>(),
        "note": if files == 0 { "No documentation found. Put .md or .txt files under ~/.runebender/docs or set RUNEBENDER_DOCS." } else { "" },
    })
}

/// HTML as text: tags dropped, block ends as paragraph breaks, the
/// few entities a spec page uses decoded. Enough for a search hit to
/// read as prose.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut tag = String::new();
    let mut skip_depth = 0_usize;
    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let lower = tag.to_lowercase();
                let name = lower
                    .trim_start_matches('/')
                    .split(|c: char| c.is_whitespace() || c == '/')
                    .next()
                    .unwrap_or("");
                if matches!(name, "script" | "style" | "nav" | "header" | "footer") {
                    if lower.starts_with('/') {
                        skip_depth = skip_depth.saturating_sub(1);
                    } else {
                        skip_depth += 1;
                    }
                } else if matches!(
                    name,
                    "p" | "div" | "tr" | "li" | "h1" | "h2" | "h3" | "h4" | "pre" | "table" | "br"
                ) && !lower.starts_with('/')
                {
                    out.push_str("\n\n");
                } else if matches!(name, "td" | "th") && !lower.starts_with('/') {
                    out.push(' ');
                }
            }
            _ if in_tag => tag.push(ch),
            _ if skip_depth > 0 => {}
            _ => out.push(ch),
        }
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// `agent call`: one tool, mapped onto the command it already is.
fn agent_call(
    name: &str,
    font: Option<&Path>,
    session: Option<&Path>,
    args: &str,
    tool: Option<&Path>,
) -> i32 {
    let args: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => {
            println!(
                "{}",
                json!({ "ok": false, "error": format!("arguments: {e}") })
            );
            return exit::USAGE;
        }
    };
    let result = dispatch_call(name, font, session, &args, tool);
    let ok = result.get("ok").and_then(|v| v.as_bool()).unwrap_or(true);
    println!(
        "{}",
        json!(agent::ToolResult {
            name: name.to_string(),
            ok,
            result
        })
    );
    if ok { exit::OK } else { exit::FAILED }
}

/// Selects the explicitly requested transport; a failed live connection never uses disk.
fn dispatch_call(
    name: &str,
    font: Option<&Path>,
    session: Option<&Path>,
    args: &serde_json::Value,
    tool: Option<&Path>,
) -> serde_json::Value {
    let inherited = std::env::var_os("RUNEBENDER_LIVE_SESSION").map(PathBuf::from);
    if let Some(session) = session.or(inherited.as_deref()) {
        #[cfg(unix)]
        return runebender::document::live_socket::call(
            session,
            &agent::ToolCall {
                name: name.into(),
                arguments: args.clone(),
            },
        )
        .unwrap_or_else(|e| json!({"ok": false, "error": e.to_string()}));
        #[cfg(not(unix))]
        return json!({"ok": false, "error": format!("live sockets unsupported: {}", session.display())});
    }
    match font {
        Some(font) => agent_call_value(name, font, args, tool),
        None => json!({"ok": false, "error": "font or session required"}),
    }
}

/// Runs one tool call and returns what it gave back, as JSON with an
/// `ok` field. Every tool is a command of this binary, run through it.
fn agent_call_value(
    name: &str,
    font: &Path,
    args: &serde_json::Value,
    tool: Option<&Path>,
) -> serde_json::Value {
    if !args.is_object() {
        return json!({"ok": false, "error": "arguments must be an object"});
    }
    if name == "project_info" {
        return project_info(font);
    }
    if name == "docs" {
        return docs_search(args.get("query").and_then(|v| v.as_str()).unwrap_or(""));
    }
    let master = match args.get("master") {
        None => None,
        Some(value) => match value.as_u64().and_then(|v| usize::try_from(v).ok()) {
            Some(index) => Some(index),
            None => return json!({"ok": false, "error": "master must be a nonnegative integer"}),
        },
    };
    if args.get("glyphs").is_some_and(|v| {
        !v.as_array()
            .is_some_and(|a| a.iter().all(|n| n.is_string()))
    }) {
        return json!({"ok": false, "error": "glyphs must be an array of names"});
    }
    if args.get("layer").is_some_and(|v| !v.is_string()) {
        return json!({"ok": false, "error": "layer must be a string"});
    }
    let source = match font_master(font, master) {
        Ok(s) => s,
        Err(e) => return json!({ "ok": false, "error": e }),
    };
    let mut result = agent_call_source(name, font, &source, args, tool);
    if let Some(object) = result.as_object_mut() {
        object.insert("source".into(), json!(source));
        object.insert("master".into(), json!(master.unwrap_or(0)));
    }
    result
}

fn agent_call_source(
    name: &str,
    _font: &Path,
    source: &Path,
    args: &serde_json::Value,
    tool: Option<&Path>,
) -> serde_json::Value {
    let src = source.display().to_string();
    let glyphs = agent::glyph_list(args);
    let text = |key: &str| args.get(key).and_then(|v| v.as_str()).map(str::to_string);
    match name {
        "font_info" => self_json(&["--json".into(), "info".into(), src]),
        "read_glyph" => match text("glyph") {
            Some(g) => read_glyph(source, &g, args.get("layer").and_then(|v| v.as_str())),
            None => json!({ "ok": false, "error": "glyph is required" }),
        },
        "proof" => {
            let out = std::env::temp_dir().join(format!(
                "runebender-proof-{}-{}.svg",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let mut a = vec![
                "--json".to_string(),
                "proof".into(),
                src,
                "--out".into(),
                out.display().to_string(),
            ];
            if !glyphs.is_empty() {
                a.push("--glyphs".into());
                a.push(glyphs.join(","));
            }
            if let Some(layer) = text("layer") {
                a.extend(["--layer".into(), layer]);
            }
            let mut result = self_json(&a);
            if result["ok"] == true
                && let Some(path) = result["svg"].as_str()
                && let Ok(svg) = std::fs::read_to_string(path)
            {
                result["svg_content"] = json!(svg);
            }
            result
        }
        "propose_edits" => {
            let mut batch = args.clone();
            batch
                .as_object_mut()
                .expect("object validated")
                .remove("master");
            match serde_json::from_value::<runebender::document::edit_batch::EditBatch>(batch) {
                Ok(batch) => {
                    match runebender::formats::proposal_ufo::save_proposal(source, &batch) {
                        Ok(summary) => json!({"ok": true, "proposal": summary}),
                        Err(e) => json!({"ok": false, "error": e}),
                    }
                }
                Err(e) => json!({"ok": false, "error": e.to_string()}),
            }
        }
        "propose" => match (text("task"), text("model")) {
            (Some(task), Some(model)) => {
                let model_dir = nodes_run::installed(None, false)
                    .into_iter()
                    .find(|(n, _)| *n == model)
                    .map(|(_, p)| p)
                    .unwrap_or_else(|| PathBuf::from(&model));
                let mut a = vec![
                    "--json".to_string(),
                    "propose".into(),
                    task,
                    src,
                    "--model".into(),
                    model_dir.display().to_string(),
                ];
                if !glyphs.is_empty() {
                    a.push("--glyphs".into());
                    a.push(glyphs.join(","));
                }
                if let Some(t) = tool {
                    a.push("--tool".into());
                    a.push(t.display().to_string());
                }
                // The per-point deltas are for a tool, not a model
                // reading prose; without them the result is a few
                // hundred characters instead of thousands.
                let mut v = self_json(&a);
                if let Some(rows) = v
                    .get_mut("report")
                    .and_then(|r| r.get_mut("glyphs"))
                    .and_then(|g| g.as_array_mut())
                {
                    for row in rows {
                        if let Some(o) = row.as_object_mut() {
                            o.remove("deltas");
                        }
                    }
                }
                if let Some(r) = v.get_mut("report").and_then(|r| r.as_object_mut()) {
                    r.remove("deltas");
                }
                v
            }
            _ => json!({ "ok": false, "error": "task and model are required" }),
        },
        "nodes_run" => match text("file") {
            Some(file) => {
                let mut a = vec![
                    "--json".to_string(),
                    "nodes".into(),
                    "run".into(),
                    file,
                    "--font".into(),
                    source.display().to_string(),
                    "--proposal-only".into(),
                ];
                if !glyphs.is_empty() {
                    a.push("--glyphs".into());
                    a.push(glyphs.join(","));
                }
                if let Some(t) = tool {
                    a.push("--tool".into());
                    a.push(t.display().to_string());
                }
                self_json(&a)
            }
            None => json!({ "ok": false, "error": "file is required" }),
        },
        "proposal_list" => self_json(&["--json".into(), "proposal".into(), "list".into(), src]),
        "proposal_discard" => match text("task") {
            Some(task) => self_json(&[
                "--json".into(),
                "proposal".into(),
                "discard".into(),
                src,
                "--task".into(),
                task,
            ]),
            None => json!({ "ok": false, "error": "task is required" }),
        },
        "docs" => docs_search(&text("query").unwrap_or_default()),
        other => json!({ "ok": false, "error": format!("no tool named {other}") }),
    }
}

/// The MCP server: JSON-RPC 2.0 over stdio, one message per line,
/// the way the protocol's stdio transport works. Handles what a
/// client needs to list and call tools; everything else answers
/// "method not found". The tool list is `agent::tools()` one to one,
/// so a client sees exactly what the chat pane and the command line
/// see, and no tool writes the foreground.
fn mcp_serve(font: Option<&Path>, session: Option<&Path>, live: bool, tool: Option<&Path>) -> i32 {
    use std::io::BufRead as _;
    if font.is_some_and(|font| !font.exists()) {
        eprintln!("font not found");
        return exit::USAGE;
    }
    let live_mode = live || session.is_some();
    let connected = std::sync::Arc::new(std::sync::Mutex::new(session.map(Path::to_path_buf)));
    let output = std::sync::Arc::new(std::sync::Mutex::new(std::io::stdout()));
    let inflight = std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::<
        String,
        std::sync::Arc<McpInFlight>,
    >::new()));
    let (request_sender, request_worker) = if live_mode {
        let (sender, receiver) = std::sync::mpsc::sync_channel::<McpWork>(MAX_MCP_REQUESTS);
        let worker_font = font.map(Path::to_path_buf);
        let worker_tool = tool.map(Path::to_path_buf);
        let worker_connected = connected.clone();
        let worker_output = output.clone();
        let worker_inflight = inflight.clone();
        let worker = std::thread::spawn(move || {
            while let Ok(work) = receiver.recv() {
                let cancelled = work
                    .state
                    .cancelled
                    .load(std::sync::atomic::Ordering::Acquire);
                let semantic_cancelled = work
                    .state
                    .semantic_cancelled
                    .load(std::sync::atomic::Ordering::Acquire);
                if cancelled && !semantic_cancelled {
                    worker_inflight
                        .lock()
                        .expect("MCP inflight mutex poisoned")
                        .remove(&work.key);
                    continue;
                }
                let response = mcp_response(
                    work.id,
                    &work.method,
                    work.params,
                    worker_font.as_deref(),
                    true,
                    worker_tool.as_deref(),
                    &worker_connected,
                );
                if !work
                    .state
                    .cancelled
                    .load(std::sync::atomic::Ordering::Acquire)
                {
                    write_mcp(&worker_output, response);
                }
                worker_inflight
                    .lock()
                    .expect("MCP inflight mutex poisoned")
                    .remove(&work.key);
            }
        });
        (Some(sender), Some(worker))
    } else {
        (None, None)
    };
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    const MAX_MCP_FRAME: u64 = 8 * 1024 * 1024;
    loop {
        let mut line = String::new();
        match std::io::Read::take(&mut input, MAX_MCP_FRAME + 1).read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if line.len() as u64 > MAX_MCP_FRAME || !line.ends_with('\n') {
            write_mcp(
                &output,
                json!({"jsonrpc":"2.0","id":null,"error":{
                    "code":-32600,"message":"invalid or oversized MCP frame (limit 8 MiB)"
                }}),
            );
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let message: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                write_mcp(
                    &output,
                    json!({ "jsonrpc": "2.0", "id": null,
                    "error": { "code": -32700, "message": format!("parse error: {e}") } }),
                );
                continue;
            }
        };
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(json!({}));
        if id.is_none() {
            if live_mode && method == "notifications/cancelled" {
                cancel_mcp_request(&params, &inflight, &connected);
            }
            continue;
        }
        let id = id.expect("checked above");
        if !live_mode {
            write_mcp(
                &output,
                mcp_response(id, method, params, font, false, tool, &connected),
            );
            continue;
        }

        // Semantic cancellation must bypass the ordered request worker so it can interrupt the
        // apply currently waiting on the live endpoint.
        if method == "tools/call" && params["name"] == "agent_cancel" {
            if inflight
                .lock()
                .expect("MCP inflight mutex poisoned")
                .contains_key(&mcp_request_key(&id))
            {
                write_mcp(
                    &output,
                    json!({"jsonrpc":"2.0","id":id,"error":{
                        "code":-32600,"message":"duplicate in-progress request id"
                    }}),
                );
                continue;
            }
            write_mcp(
                &output,
                mcp_response(id, method, params, font, true, tool, &connected),
            );
            continue;
        }
        let key = mcp_request_key(&id);
        if inflight
            .lock()
            .expect("MCP inflight mutex poisoned")
            .contains_key(&key)
        {
            write_mcp(
                &output,
                json!({"jsonrpc":"2.0","id":id,"error":{
                    "code":-32600,"message":"duplicate in-progress request id"
                }}),
            );
            continue;
        }
        let cancellation = mcp_cancellation_arguments(method, &params);
        let apply_arguments = mcp_apply_arguments(method, &params).cloned();
        let reservation = apply_arguments
            .as_ref()
            .map(|arguments| live_client_call("agent_reserve", arguments, &connected))
            .unwrap_or_else(|| json!({"ok":false}));
        let semantic_reserved = reservation["ok"] == true;
        let reservation_owned = reservation["reservation_status"] == "new";
        let state = std::sync::Arc::new(McpInFlight {
            cancelled: std::sync::atomic::AtomicBool::new(false),
            semantic_cancelled: std::sync::atomic::AtomicBool::new(false),
            semantic_reserved,
            cancellation,
        });
        {
            let mut requests = inflight.lock().expect("MCP inflight mutex poisoned");
            requests.insert(key.clone(), state.clone());
        }
        let work = McpWork {
            id: id.clone(),
            method: method.to_owned(),
            params,
            key: key.clone(),
            state,
        };
        if request_sender
            .as_ref()
            .expect("live request worker")
            .try_send(work)
            .is_err()
        {
            inflight
                .lock()
                .expect("MCP inflight mutex poisoned")
                .remove(&key);
            if reservation_owned && let Some(arguments) = apply_arguments.as_ref() {
                let _ = live_client_call("agent_release", arguments, &connected);
            }
            write_mcp(
                &output,
                json!({"jsonrpc":"2.0","id":id,"error":{
                    "code":-32000,"message":"live MCP request capacity exhausted"
                }}),
            );
        }
    }
    drop(request_sender);
    if let Some(worker) = request_worker {
        let _ = worker.join();
    }
    exit::OK
}

const MAX_MCP_REQUESTS: usize = 32;

struct McpInFlight {
    cancelled: std::sync::atomic::AtomicBool,
    semantic_cancelled: std::sync::atomic::AtomicBool,
    semantic_reserved: bool,
    cancellation: Option<serde_json::Value>,
}

struct McpWork {
    id: serde_json::Value,
    method: String,
    params: serde_json::Value,
    key: String,
    state: std::sync::Arc<McpInFlight>,
}

fn mcp_response(
    id: serde_json::Value,
    method: &str,
    params: serde_json::Value,
    font: Option<&Path>,
    live_mode: bool,
    tool: Option<&Path>,
    connected: &std::sync::Arc<std::sync::Mutex<Option<PathBuf>>>,
) -> serde_json::Value {
    let result = match method {
        "initialize" => {
            let version = params
                .get("protocolVersion")
                .and_then(|v| v.as_str())
                .filter(|version| {
                    matches!(
                        *version,
                        "2024-11-05" | "2025-03-26" | "2025-06-18" | "2025-11-25"
                    )
                })
                .unwrap_or("2025-11-25");
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "runebender",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "instructions": if live_mode { runebender::document::live::INSTRUCTIONS.into() } else { mcp_instructions(font.expect("font or session")) },
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": mcp_tools(live_mode).iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.parameters,
                "annotations": {"readOnlyHint": matches!(t.name.as_str(), "agent_receipt" | "proof_status" | "editor_context" | "project_info" | "font_info" | "read_glyph" | "glyph_inventory" | "design_context" | "experiment_list" | "read_kerning" | "specimen" | "editor_sessions" | "editor_connect" | "proposal_list") || (live_mode && t.name == "proof"), "openWorldHint": !live_mode},
            })).collect::<Vec<_>>()
        })),
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let value = if live_mode {
                live_client_call(name, &args, connected)
            } else {
                dispatch_call(name, font, None, &args, tool)
            };
            let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(true);
            Ok(json!({
                "content": proof_content(value),
                "isError": !ok,
            }))
        }
        "resources/list" => Ok(json!({ "resources": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        other => Err(json!({
            "code": -32601,
            "message": format!("method not found: {other}")
        })),
    };
    match result {
        Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
        Err(error) => json!({"jsonrpc":"2.0","id":id,"error":error}),
    }
}

fn write_mcp(output: &std::sync::Arc<std::sync::Mutex<std::io::Stdout>>, value: serde_json::Value) {
    use std::io::Write as _;
    let mut output = output.lock().expect("MCP output mutex poisoned");
    let _ = writeln!(output, "{value}");
    let _ = output.flush();
}

fn mcp_request_key(id: &serde_json::Value) -> String {
    id.to_string()
}

fn mcp_cancellation_arguments(
    method: &str,
    params: &serde_json::Value,
) -> Option<serde_json::Value> {
    if method != "tools/call" || params["name"] != "agent_apply" {
        return None;
    }
    let arguments = params.get("arguments")?;
    Some(json!({
        "expected_document_epoch":arguments.get("expected_document_epoch")?.as_str()?,
        "actor":arguments.get("actor")?.as_str()?,
        "operation_key":arguments.get("operation_key")?.as_str()?,
    }))
}

fn mcp_apply_arguments<'a>(
    method: &str,
    params: &'a serde_json::Value,
) -> Option<&'a serde_json::Value> {
    (method == "tools/call" && params["name"] == "agent_apply")
        .then(|| params.get("arguments"))
        .flatten()
}

fn cancel_mcp_request(
    params: &serde_json::Value,
    inflight: &std::sync::Arc<
        std::sync::Mutex<std::collections::BTreeMap<String, std::sync::Arc<McpInFlight>>>,
    >,
    connected: &std::sync::Arc<std::sync::Mutex<Option<PathBuf>>>,
) {
    let Some(request_id) = params.get("requestId") else {
        return;
    };
    let state = inflight
        .lock()
        .expect("MCP inflight mutex poisoned")
        .get(&mcp_request_key(request_id))
        .cloned();
    let Some(state) = state else {
        return;
    };
    if state.semantic_reserved
        && let Some(arguments) = &state.cancellation
    {
        let result = live_client_call("agent_cancel", arguments, connected);
        if matches!(
            result
                .get("cancellation_status")
                .and_then(serde_json::Value::as_str),
            Some("prevented" | "already_prevented")
        ) {
            state
                .semantic_cancelled
                .store(true, std::sync::atomic::Ordering::Release);
        }
    }
    state
        .cancelled
        .store(true, std::sync::atomic::Ordering::Release);
}

/// Live tools include explicit discovery and connection, so clients need one stable config.
fn mcp_tools(live: bool) -> Vec<agent::Tool> {
    if !live {
        return agent::tools();
    }
    let mut tools = runebender::document::live::tools();
    if let Some(proof) = tools.iter_mut().find(|tool| tool.name == "proof") {
        proof.description = "Return a PNG proof image and metrics from the live unsaved source. Supply 1 to 256 explicit glyph names; use layer to view a proposal. Use small groups for legible images. Images are required for visual judgment; report if your client does not deliver them.".into();
    }
    tools.push(agent::Tool {name:"export_proof".into(),description:"Export an explicit live or branch glyph/text proof using Designbot. Writes a new PNG or PDF file; refuses overwrite. Does not save the font. Supply either glyphs or text, an explicit output path, and format.".into(),parameters:json!({"type":"object","properties":{"source":{"type":"integer","minimum":0},"expected_document_epoch":{"type":"string"},"branch":{"type":"string"},"layer":{"type":"string"},"glyphs":{"type":"array","items":{"type":"string"}},"text":{"type":"string"},"output":{"type":"string"},"format":{"enum":["png","pdf"]}},"required":["output","format"],"additionalProperties":false})});
    tools.push(agent::Tool { name: "editor_sessions".into(), description: "List local editor endpoint paths. Connect to inspect the project. Never assume a different window is the requested font.".into(), parameters: json!({"type":"object", "properties":{}}) });
    tools.push(agent::Tool { name: "editor_connect".into(), description: "Connect this agent to a listed editor endpoint and return its live project/source information. Opening another font closes the old connection; reconnect explicitly.".into(), parameters: json!({"type":"object", "properties":{"session":{"type":"string"}}, "required":["session"]}) });
    tools
}

/// Changes only this MCP client's chosen endpoint; all font work stays on the editor thread.
fn live_client_call(
    name: &str,
    args: &serde_json::Value,
    connected: &std::sync::Arc<std::sync::Mutex<Option<PathBuf>>>,
) -> serde_json::Value {
    #[cfg(not(unix))]
    {
        let _ = (name, args, connected);
        json!({"ok": false, "error":"live editors require Unix"})
    }
    #[cfg(unix)]
    {
        use runebender::document::live_socket;
        if name == "export_proof" {
            let run = (|| -> Result<serde_json::Value, String> {
                use std::io::Write as _;
                let path = args
                    .get("output")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .ok_or("output path required")?;
                let pdf = match args.get("format").and_then(|v| v.as_str()) {
                    Some("pdf") => true,
                    Some("png") => false,
                    _ => return Err("format must be png or pdf".into()),
                };
                if args.get("text").is_some() == args.get("glyphs").is_some() {
                    return Err("supply exactly one of text or glyphs".into());
                }
                if args.get("text").is_some() && args.get("layer").is_some() {
                    return Err("text proofs use branch foreground; install the proposal into the branch first".into());
                }
                let value = live_client_call(
                    if args.get("text").is_some() {
                        "specimen"
                    } else {
                        "proof"
                    },
                    args,
                    connected,
                );
                if value["ok"] != true {
                    return Ok(value);
                }
                let scene = value.get("scene").ok_or("proof has no scene")?;
                let bytes = runebender::formats::designbot::render(scene, pdf)?;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map_err(|e| e.to_string())?;
                if let Err(e) = file.write_all(&bytes) {
                    drop(file);
                    let _ = std::fs::remove_file(path);
                    return Err(e.to_string());
                }
                Ok(
                    json!({"ok":true,"output":path,"bytes":bytes.len(),"source_id":value["source_id"],"document_epoch":value["document_epoch"],"document_revision":value["document_revision"],"branch":value["branch"]}),
                )
            })();
            return run.unwrap_or_else(|error| json!({"ok":false,"error":error}));
        }
        if name == "editor_sessions" {
            let selected = connected
                .lock()
                .expect("MCP connection mutex poisoned")
                .clone();
            return json!({"ok":true, "sessions":live_socket::sessions(), "connected":selected});
        }
        if name == "editor_connect" {
            let Some(path) = args
                .get("session")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
            else {
                return json!({"ok":false, "error":"session path is required"});
            };
            if !live_socket::sessions().contains(&path) {
                return json!({"ok":false, "error":"choose an endpoint returned by editor_sessions"});
            }
            let value = dispatch_call("project_info", None, Some(&path), &json!({}), None);
            if value["ok"] == true {
                *connected.lock().expect("MCP connection mutex poisoned") = Some(path);
            }
            return value;
        }
        let selected = connected
            .lock()
            .expect("MCP connection mutex poisoned")
            .clone();
        match selected {
            Some(path) => dispatch_call(name, None, Some(&path), args, None),
            None => json!({"ok":false, "error":"call editor_sessions, then editor_connect first"}),
        }
    }
}

/// What a client is told at `initialize`: the rule of the tool list.
fn mcp_instructions(font: &Path) -> String {
    format!(
        "Runebender font editor, working on {}. The tools read the font, render \
         proofs, run local models, and propose changes. No tool edits the font: a \
         proposal is a UFO layer the person installs or discards in the editor, \
         one glyph at a time. Call project_info first and choose an explicit master; read a glyph before you \
         talk about its shape.",
        font.display()
    )
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
    let arrived = Project::load(source).ok().and_then(|project| {
        let source = project.source_id(0)?;
        proposal::find_project(&project, source, task).ok()
    });
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
    let (light, heavy) = match (open_project(from, json), open_project(to, json)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(code), _) | (_, Err(code)) => return code,
    };
    let light_source = light.source_id(0).expect("one source");
    let heavy_source = heavy.source_id(0).expect("one source");
    let light_layer = light.document_source(light_source).unwrap().default_layer();
    let heavy_layer = heavy.document_source(heavy_source).unwrap().default_layer();
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
                light.document_layer(n, &light_layer)?,
                heavy.document_layer(n, &heavy_layer)?,
            ))
        })
        .collect();
    let Some(offset) = embolden::learn_layer_offset(&pairs) else {
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
            .glyph_names()
            .filter(|name| {
                let Some(light_glyph) = light.document_layer(name, &light_layer) else {
                    return false;
                };
                if light_glyph.contours().next().is_none()
                    || light_glyph.components().next().is_some()
                {
                    return false;
                }
                heavy
                    .document_layer(name, &heavy_layer)
                    .is_some_and(|heavy_glyph| {
                        canonical_outline(heavy_glyph) == canonical_outline(light_glyph)
                    })
            })
            .map(str::to_owned)
            .collect(),
    };
    if check {
        return bolden_check(
            &light,
            &heavy,
            &light_layer,
            &heavy_layer,
            offset,
            glyphs,
            limit,
            json,
        );
    }
    let mut rows = Vec::new();
    for name in todo.iter().take(limit) {
        let Some(glyph) = light.document_layer(name, &light_layer) else {
            continue;
        };
        let original = flat_layer_points(glyph);
        let predicted = emboldened_layer_points(glyph, offset);
        let moved = original
            .iter()
            .zip(&predicted)
            .filter(|(a, b)| a != b)
            .count();
        let points = original.len();
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
    light: &Project,
    heavy: &Project,
    light_layer: &LayerId,
    heavy_layer: &LayerId,
    offset: embolden::Offset,
    glyphs: Option<&[String]>,
    limit: usize,
    json: bool,
) -> i32 {
    let names: Vec<String> = match glyphs {
        Some(list) => list.to_vec(),
        None => light
            .glyph_names()
            .filter(|name| {
                let Some(light_glyph) = light.document_layer(name, light_layer) else {
                    return false;
                };
                if light_glyph.contours().next().is_none()
                    || light_glyph.components().next().is_some()
                {
                    return false;
                }
                heavy
                    .document_layer(name, heavy_layer)
                    .is_some_and(|heavy_glyph| {
                        compatible_outlines(light_glyph, heavy_glyph)
                            && canonical_outline(light_glyph) != canonical_outline(heavy_glyph)
                    })
            })
            .map(str::to_owned)
            .collect(),
    };
    let mut rows = Vec::new();
    let (mut sum_dx, mut sum_dy, mut n) = (0.0, 0.0, 0_usize);
    for name in names.iter().take(limit) {
        let (Some(l), Some(h)) = (
            light.document_layer(name, light_layer),
            heavy.document_layer(name, heavy_layer),
        ) else {
            continue;
        };
        if !compatible_outlines(l, h) {
            continue;
        }
        let (a, b) = (flat_layer_points(l), flat_layer_points(h));
        if a.len() != b.len() || a.is_empty() {
            continue;
        }
        for (p, q) in a.iter().zip(&b) {
            sum_dx += q.0 - p.0;
            sum_dy += q.1 - p.1;
            n += 1;
        }
        let pred = emboldened_layer_points(l, offset);
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

fn canonical_outline(
    layer: runebender::document::LayerView<'_>,
) -> Vec<Vec<(kurbo::Point, runebender::document::LayerPointType, bool)>> {
    layer
        .contours()
        .map(|contour| {
            contour
                .points()
                .map(|point| (point.position(), point.point_type(), point.is_smooth()))
                .collect()
        })
        .collect()
}

fn compatible_outlines(
    first: runebender::document::LayerView<'_>,
    second: runebender::document::LayerView<'_>,
) -> bool {
    let first = canonical_outline(first);
    let second = canonical_outline(second);
    first.len() == second.len()
        && first.iter().zip(second).all(|(first, second)| {
            first.len() == second.len()
                && first
                    .iter()
                    .zip(second)
                    .all(|(first, second)| first.1 == second.1)
        })
}

fn flat_layer_points(layer: runebender::document::LayerView<'_>) -> Vec<(f64, f64)> {
    layer
        .contours()
        .flat_map(|contour| {
            contour
                .points()
                .map(|point| (point.position().x, point.position().y))
        })
        .collect()
}

fn emboldened_layer_points(
    layer: runebender::document::LayerView<'_>,
    offset: embolden::Offset,
) -> Vec<(f64, f64)> {
    layer
        .contours()
        .flat_map(|contour| {
            let points = contour
                .points()
                .map(|point| point.position())
                .collect::<Vec<_>>();
            points
                .iter()
                .zip(embolden::outward_normals_for_points(&points))
                .map(move |(point, (nx, ny))| (point.x + nx * offset.x, point.y + ny * offset.y))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Return an actual MCP image alongside proof metadata, without external resources.
#[allow(
    clippy::cast_possible_truncation,
    reason = "Raster dimensions are rounded and bounded to 2048 pixels"
)]
fn proof_content(mut value: serde_json::Value) -> Vec<serde_json::Value> {
    use base64::Engine as _;
    let mut content = Vec::new();
    if let Some(png) = value
        .as_object_mut()
        .and_then(|object| object.remove("png_base64"))
    {
        content.push(serde_json::json!({"type":"image", "mimeType":"image/png", "data":png}));
    } else if let Some(scene) = value.get("scene") {
        let rendered = runebender::formats::designbot::render(scene, false);
        match rendered {
            Ok(png) => {
                content.push(serde_json::json!({"type":"image", "mimeType":"image/png",
                    "data":base64::engine::general_purpose::STANDARD.encode(png)}));
                value.as_object_mut().unwrap().remove("svg_content");
                value.as_object_mut().unwrap().remove("scene");
            }
            Err(error) => value["image_error"] = serde_json::json!(error),
        }
    }
    content.insert(
        0,
        serde_json::json!({"type":"text", "text":value.to_string()}),
    );
    content
}

fn collapse_metaballs(
    source: &Path,
    out: &Path,
    resolution: f64,
    accuracy: f64,
    json: bool,
) -> i32 {
    use runebender::outline::metaballs::OutlineOptions;
    if out.exists() {
        return fail(
            json,
            exit::USAGE,
            "output already exists; choose a new UFO path",
        );
    }
    let mut project = match open_project(source, json) {
        Ok(project) => project,
        Err(code) => return code,
    };
    let source_id = project.source_id(0).expect("one source");
    let layer = project
        .document_source(source_id)
        .expect("one source")
        .default_layer();
    let names = project.glyph_names().map(str::to_owned).collect::<Vec<_>>();
    let mut count = 0;
    for glyph in names {
        let address = GlyphLayerAddress {
            glyph,
            layer: layer.clone(),
        };
        let Ok(mut transaction) = project.begin_document_layer_transaction(&address) else {
            continue;
        };
        let converted = match transaction.draft_mut().collapse_metaballs(
            None,
            OutlineOptions {
                resolution,
                accuracy,
            },
        ) {
            Ok(converted) => converted,
            Err(error) => return fail(json, exit::FAILED, &error),
        };
        if converted == 0 {
            continue;
        }
        if let Err(error) = project.commit_document_layer_transaction(transaction) {
            return fail(json, exit::FAILED, &error.to_string());
        }
        count += converted;
    }
    if let Err(error) = project.save_as(out) {
        return fail(json, exit::FAILED, &error);
    }
    if json {
        println!(
            "{}",
            json!({"ok": true, "groups_converted": count, "output": out})
        );
    } else {
        println!(
            "Converted {count} metaball groups to cubic outlines in {}",
            out.display()
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

#[cfg(test)]
mod startup_tests {
    use super::*;

    #[test]
    fn editor_paths_and_subcommands_are_unambiguous() {
        let empty = parse_args(["runebender"]).unwrap();
        assert!(empty.command.is_none());
        assert!(empty.font.is_none());
        let font = parse_args(["runebender", "My Font.designspace"]).unwrap();
        assert_eq!(font.font.as_deref(), Some(Path::new("My Font.designspace")));
        assert!(font.command.is_none());
        let info = parse_args(["runebender", "info", "Font.ufo", "--json"]).unwrap();
        assert!(matches!(info.command, Some(Command::Info { .. })));
        assert!(info.font.is_none());
        assert!(info.json);
        let info = parse_args(["runebender", "--json", "info", "Font.ufo"]).unwrap();
        assert!(matches!(info.command, Some(Command::Info { .. })));
        assert!(info.json);
        assert!(parse_args(["runebender", "info"]).is_err());
        assert!(parse_args(["runebender", "Font.ufo", "info", "Other.ufo"]).is_err());
    }
}
