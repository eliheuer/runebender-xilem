// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Font operations from a shell.
//!
//! The editor and this command run the same code: everything here is a
//! thin shell over `runebender_core`, which is where the work lives.
//! That is the point. An operation you can only reach by opening a
//! window is one a script, a build, or an agent cannot use.
//!
//! Conventions match `font-ml`, so the two are driven the same way:
//! `--json` on every command, and exit codes that separate a usage
//! mistake from a real failure.
//!
//! The commands in `read` report and never write. The ones in `edit`
//! change a font and save it, take any number of sources, and take
//! `--dry-run`.

mod edit;
mod read;
mod shell;
mod sources;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use edit::Edit;

#[derive(Parser)]
#[command(
    name = "runebender-core",
    about = "Font operations from a shell",
    long_about = "Font operations from a shell.\n\nThe same code the \
                  Runebender editor runs, without a window.\n\n\
                  info, glyphs, color, spacing and bolden read a font \
                  and report. The rest change a font and save it: they \
                  take any number of UFOs or designspaces, a --glyphs \
                  filter, and --dry-run, which writes nothing and \
                  exits 1 when there is work waiting.\n\n\
                  Every command takes --json. Exit codes: 0 ok, \
                  1 findings, 2 usage, 4 failed.\n\n\
                  `runebender-core help <command>` explains one."
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
    /// What a source holds: glyphs, masters, unicodes.
    Info {
        /// A .ufo directory.
        source: PathBuf,
    },
    /// Every glyph name in a source, one per line.
    Glyphs {
        /// A .ufo directory.
        source: PathBuf,
    },
    /// Find glyphs that read darker or lighter than the rest.
    Color {
        /// A .ufo directory.
        source: PathBuf,
        /// How far off the group's median counts, as a fraction.
        #[arg(long, default_value = "0.15")]
        tolerance: f64,
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
    /// Find sidebearings off the grid the family is drawn on.
    Spacing {
        /// A .ufo directory.
        source: PathBuf,
        /// Grid step in units. Inferred from the font when not given.
        #[arg(long)]
        step: Option<f64>,
    },
    /// Tidy contours, correct directions, round coordinates.
    Clean {
        #[command(flatten)]
        edit: Edit,
        /// Drop degenerate contours and duplicate points.
        #[arg(long)]
        tidy: bool,
        /// Make outer contours counter-clockwise and counters the
        /// other way.
        #[arg(long)]
        directions: bool,
        /// Round coordinates to whole units.
        #[arg(long)]
        round: bool,
        /// Add an on-curve point at every extremum. Off by default:
        /// this one changes the drawing, not just the file.
        #[arg(long)]
        extremes: bool,
    },
    /// Flatten overlapping contours into their union.
    Overlap {
        #[command(flatten)]
        edit: Edit,
    },
    /// Move every outline out or in by a fixed distance.
    Offset {
        #[command(flatten)]
        edit: Edit,
        /// Units to move by. Negative moves inward.
        #[arg(long, allow_negative_numbers = true)]
        by: f64,
    },
    /// Convert outlines between quadratic and cubic curves.
    Convert {
        #[command(flatten)]
        edit: Edit,
        /// Which curves to end up with.
        #[arg(long, value_parser = ["cubic", "quad"])]
        to: String,
        /// How far a quadratic may stray from the cubic it replaces.
        #[arg(long, default_value = "1.0")]
        tolerance: f64,
    },
    /// Put every component back on its anchors.
    Realign {
        #[command(flatten)]
        edit: Edit,
        /// Start from the composite's own anchors instead of the
        /// base glyph's.
        #[arg(long)]
        seed_anchors: bool,
    },
    /// Rename a glyph, and every reference to it.
    Rename {
        #[command(flatten)]
        edit: Edit,
        /// The name it has now.
        #[arg(long)]
        from: String,
        /// The name it should have.
        #[arg(long)]
        to: String,
    },
    /// Set a glyph's Unicode value.
    Unicode {
        #[command(flatten)]
        edit: Edit,
        /// Glyph name.
        #[arg(long)]
        glyph: String,
        /// The codepoint, as U+0041 or 0041.
        #[arg(long)]
        to: String,
    },
    /// Read or write one kerning pair.
    Kern {
        #[command(flatten)]
        edit: Edit,
        /// Left glyph.
        #[arg(long)]
        left: String,
        /// Right glyph.
        #[arg(long)]
        right: String,
        /// The value to write. Without it the pair is read.
        #[arg(long, allow_negative_numbers = true)]
        set: Option<f64>,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let json = cli.json;
    let code = match &cli.command {
        Command::Info { source } => read::info(source, json),
        Command::Glyphs { source } => read::glyphs(source, json),
        Command::Color { source, tolerance } => read::color(source, *tolerance, json),
        Command::Bolden {
            from,
            to,
            references,
            glyphs,
            limit,
            check,
        } => read::bolden(
            from,
            to,
            references.as_deref(),
            glyphs.as_deref(),
            *limit,
            *check,
            json,
        ),
        Command::Spacing { source, step } => read::spacing_cmd(source, *step, json),
        Command::Clean {
            edit,
            tidy,
            directions,
            round,
            extremes,
        } => edit::clean(edit, json, *tidy, *directions, *round, *extremes),
        Command::Overlap { edit } => edit::overlap(edit, json),
        Command::Offset { edit, by } => edit::offset(edit, json, *by),
        Command::Convert {
            edit,
            to,
            tolerance,
        } => edit::convert_curves(edit, json, to == "cubic", *tolerance),
        Command::Realign { edit, seed_anchors } => edit::realign(edit, json, *seed_anchors),
        Command::Rename { edit, from, to } => edit::rename(edit, json, from, to),
        Command::Unicode { edit, glyph, to } => edit::unicode(edit, json, glyph, to),
        Command::Kern {
            edit,
            left,
            right,
            set,
        } => edit::kern(edit, json, left, right, *set),
    };
    std::process::ExitCode::from(code as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::exit;

    /// Exit codes are the interface for a script, so they are pinned.
    /// A caller branching on 1 (findings) versus 2 (usage) is the whole
    /// reason to separate them.
    #[test]
    fn exit_codes_are_distinct() {
        let codes = [exit::OK, exit::FINDINGS, exit::USAGE, exit::FAILED];
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
