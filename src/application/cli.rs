// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Font operations from a shell.
//!
//! A thin shell over Runebender's font modules, where the work lives.
//! Conventions match `font-ml`, so the two are driven the same
//! way: `--json` on every command, and exit codes that separate a
//! usage mistake from a real failure.
//! Parsing and dispatch stay here; command behavior lives in the named child modules.

use std::path::{Path, PathBuf};

mod agent_commands;
mod font_commands;
mod mcp;
mod nodes_commands;
mod theme;

use agent_commands::agent_call;
use font_commands::{
    bolden, collapse_metaballs, compose_cmd, features_cmd, info, phrases, proof, proposal_discard,
    proposal_install, proposal_list, propose,
};
use mcp::mcp_serve;
use nodes_commands::{nodes_check, nodes_run, nodes_types};
use theme::theme_command;

use clap::{Parser, Subcommand};
use runebender::automation::agent;
use runebender::font::compose;
use runebender::font::project::Project;
use runebender::font::proposal;
use runebender::font::variable::{GlyphLayerAddress, LayerId};
use runebender::outline::embolden;
use runebender::workflows::nodes;
use runebender::workflows::nodes_run;
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
    /// Inspect, create, or validate portable editor themes.
    Theme {
        #[command(subcommand)]
        action: ThemeAction,
    },
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
    /// Write a phrase file for every labeled neural item: the JSON the `NeuralType` tools turn
    /// into training rows.
    Phrases {
        /// The UFO.
        source: PathBuf,
        /// The directory to write into; `phrases` beside the source when omitted.
        #[arg(long)]
        out: Option<PathBuf>,
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
enum ThemeAction {
    /// List built-in and locally installed themes.
    List,
    /// Check a theme file and report its resolved identity.
    Validate {
        /// A .theme.toml or existing .theme.json file.
        file: PathBuf,
    },
    /// Copy a built-in theme into a new, editable file.
    Init {
        /// Built-in starting theme; use "theme list" to see the available IDs.
        #[arg(long, default_value = "gray")]
        from: String,
        /// Stable identifier for the new theme.
        #[arg(long)]
        id: String,
        /// Human-readable display name.
        #[arg(long)]
        name: String,
        /// Destination .theme.toml file; must not already exist.
        #[arg(long)]
        out: PathBuf,
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
    Tools {
        /// Include versioned result contracts and effect metadata alongside legacy tool schemas.
        #[arg(long)]
        contracts: bool,
    },
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
        Command::Theme { action } => theme_command(action, json),
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
        Command::Phrases { source, out } => phrases(source, out.as_deref(), json),
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

            AgentAction::Tools { contracts } => {
                let live = std::env::var_os("RUNEBENDER_LIVE_SESSION").is_some();
                let tools = if live {
                    runebender::automation::live::tools()
                } else {
                    agent::tools()
                };
                let prompt = if live {
                    runebender::automation::live::system_prompt(&tools)
                } else {
                    agent::system_prompt(&tools)
                };
                let mut result = json!({ "ok": true, "prompt": prompt, "tools": tools });
                if *contracts {
                    use runebender::automation::tool_contracts::{self, ToolSurface};
                    let surface = if live {
                        ToolSurface::Live
                    } else {
                        ToolSurface::Disk
                    };
                    result["contracts"] = json!(
                        tools
                            .into_iter()
                            .map(|tool| tool_contracts::describe(tool, surface))
                            .collect::<Vec<_>>()
                    );
                }
                println!("{result}");
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
                json!({"ok": true, "sessions": runebender::automation::live_socket::sessions()})
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
