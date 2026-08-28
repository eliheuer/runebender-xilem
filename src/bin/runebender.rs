// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

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
//! Read-only by default. Commands that write a font say so.

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use norad::Font;
use runebender_core::glyph_ops;
use serde_json::{json, Value};

/// Exit codes, matching font-ml so a caller can branch on them.
mod exit {
    /// Ran, and the answer is yes or the work is done.
    pub const OK: i32 = 0;
    /// Ran, and the answer is no. Reserved for checks.
    pub const FINDINGS: i32 = 1;
    /// The command was wrong: bad path, unknown glyph, missing flag.
    pub const USAGE: i32 = 2;
    /// The command was right and the work failed.
    pub const FAILED: i32 = 4;
}

#[derive(Parser)]
#[command(
    name = "runebender",
    about = "Font operations from a shell",
    long_about = "Font operations from a shell.\n\nThe same code the \
                  Runebender editor runs. Every command takes --json.\n\n\
                  Exit codes: 0 ok, 1 findings, 2 usage, 4 failed."
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
    /// Measure one glyph: contours, points, advance, sidebearings.
    Measure {
        /// A .ufo directory.
        source: PathBuf,
        /// Glyph name.
        #[arg(long)]
        glyph: String,
    },
    /// Compare two masters for interpolation compatibility.
    Check {
        /// The lighter master.
        #[arg(long)]
        a: PathBuf,
        /// The heavier master.
        #[arg(long)]
        b: PathBuf,
        /// Stop after this many mismatches.
        #[arg(long, default_value = "20")]
        limit: usize,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let code = match &cli.command {
        Command::Info { source } => info(source, cli.json),
        Command::Measure { source, glyph } => measure(source, glyph, cli.json),
        Command::Check { a, b, limit } => check(a, b, *limit, cli.json),
    };
    std::process::ExitCode::from(code as u8)
}

fn fail(json: bool, code: i32, message: &str) -> i32 {
    if json {
        println!("{}", json!({ "ok": false, "error": message }));
    } else {
        eprintln!("{message}");
    }
    code
}

fn emit(json: bool, value: Value, plain: impl FnOnce()) -> i32 {
    if json {
        println!("{value}");
    } else {
        plain();
    }
    exit::OK
}

fn open(path: &Path, json: bool) -> Result<Font, i32> {
    Font::load(path).map_err(|e| fail(json, exit::USAGE, &format!("{}: {e}", path.display())))
}

fn info(source: &Path, json: bool) -> i32 {
    let font = match open(source, json) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let glyphs = font.default_layer().len();
    let encoded = font
        .default_layer()
        .iter()
        .filter(|g| !g.codepoints.is_empty())
        .count();
    let composites = font
        .default_layer()
        .iter()
        .filter(|g| !g.components.is_empty())
        .count();
    let family = font.font_info.family_name.clone().unwrap_or_default();
    let style = font.font_info.style_name.clone().unwrap_or_default();
    let upm = font.font_info.units_per_em.map(|v| *v).unwrap_or(0.0);
    emit(
        json,
        json!({
            "ok": true, "family": family, "style": style, "unitsPerEm": upm,
            "glyphs": glyphs, "encoded": encoded, "composites": composites,
            "layers": font.layers.len(),
        }),
        || {
            println!("{family} {style}");
            println!("  units per em  {upm}");
            println!("  glyphs        {glyphs} ({encoded} encoded, {composites} composite)");
            println!("  layers        {}", font.layers.len());
        },
    )
}

fn measure(source: &Path, name: &str, json: bool) -> i32 {
    let font = match open(source, json) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let Some(glyph) = font.default_layer().get_glyph(name) else {
        return fail(json, exit::USAGE, &format!("no glyph {name} in {}", source.display()));
    };
    let advance = glyph.width;
    let contours = glyph.contours.len();
    let points: usize = glyph.contours.iter().map(|c| c.points.len()).sum();
    let signature = glyph_ops::glyph_signature(glyph);
    emit(
        json,
        json!({
            "ok": true, "glyph": name, "advance": advance,
            "contours": contours, "points": points,
            "components": glyph.components.len(),
            "pointsPerContour": signature.iter().map(|c| c.len()).collect::<Vec<_>>(),
        }),
        || {
            println!("{name}");
            println!("  advance     {advance}");
            println!("  contours    {contours}");
            println!("  points      {points}");
            println!("  components  {}", glyph.components.len());
        },
    )
}

/// Interpolation compatibility, the check that costs the most to get
/// wrong: a mismatch here is invisible until the family stops building.
fn check(a: &Path, b: &Path, limit: usize, json: bool) -> i32 {
    let (fa, fb) = match (open(a, json), open(b, json)) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(code), _) | (_, Err(code)) => return code,
    };
    let mut findings = Vec::new();
    let mut compared = 0usize;
    for glyph in fa.default_layer().iter() {
        let name = glyph.name().as_str();
        let Some(other) = fb.default_layer().get_glyph(name) else {
            findings.push(json!({ "glyph": name, "problem": "missing in b" }));
            continue;
        };
        compared += 1;
        let (sa, sb) = (glyph_ops::glyph_signature(glyph), glyph_ops::glyph_signature(other));
        if sa.len() != sb.len() {
            findings.push(json!({
                "glyph": name, "problem": "contour count",
                "a": sa.len(), "b": sb.len(),
            }));
        } else if sa != sb {
            let at = sa.iter().zip(&sb).position(|(x, y)| x != y).unwrap_or(0);
            findings.push(json!({
                "glyph": name, "problem": "point structure", "contour": at,
                "a": sa[at].len(), "b": sb[at].len(),
            }));
        }
        if findings.len() >= limit {
            break;
        }
    }
    let ok = findings.is_empty();
    if json {
        println!("{}", json!({ "ok": ok, "compared": compared, "findings": findings }));
    } else if ok {
        println!("{compared} glyphs compared, no mismatches");
    } else {
        for f in &findings {
            println!(
                "{}: {}",
                f["glyph"].as_str().unwrap_or("?"),
                f["problem"].as_str().unwrap_or("?")
            );
        }
        println!("{} of {compared} compared", findings.len());
    }
    if ok { exit::OK } else { exit::FINDINGS }
}

#[cfg(test)]
mod tests {
    use super::*;
    use norad::{Contour, ContourPoint, Glyph, PointType};

    fn glyph(name: &str, counts: &[usize]) -> Glyph {
        let mut g = Glyph::new(name);
        for n in counts {
            let points = (0..*n)
                .map(|i| {
                    ContourPoint::new(i as f64, 0.0, PointType::Line, false, None, None)
                })
                .collect();
            g.contours.push(Contour::new(points, None));
        }
        g
    }

    /// The signature is what the check compares, so it has to notice a
    /// difference in point counts even when the contour count matches.
    #[test]
    fn a_point_count_difference_is_a_mismatch() {
        let a = glyph("a", &[4, 4]);
        let b = glyph("a", &[4, 5]);
        assert_ne!(glyph_ops::glyph_signature(&a), glyph_ops::glyph_signature(&b));
    }

    #[test]
    fn the_same_shape_matches() {
        let a = glyph("a", &[4, 4]);
        let b = glyph("a", &[4, 4]);
        assert_eq!(glyph_ops::glyph_signature(&a), glyph_ops::glyph_signature(&b));
    }

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
