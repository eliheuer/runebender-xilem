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
use runebender_core::outline::embolden;
use serde_json::{Value, json};

/// Exit codes, matching font-ml so a caller can branch on them.
mod exit {
    /// Ran, and the answer is yes or the work is done.
    pub const OK: i32 = 0;
    /// The command was wrong: bad path, unknown glyph, missing flag.
    pub const USAGE: i32 = 2;
    /// The command was right and the work failed.
    pub const FAILED: i32 = 4;
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

/// Prints the JSON form, or runs `plain` for the human form.
fn emit(json: bool, value: Value, plain: impl FnOnce()) -> i32 {
    if json {
        println!("{value}");
    } else {
        plain();
    }
    exit::OK
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
    /// What a source holds: glyphs, masters, unicodes.
    Info {
        /// A .ufo directory.
        source: PathBuf,
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

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let json = cli.json;
    let code = match &cli.command {
        Command::Info { source } => info(source, json),
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
    std::process::ExitCode::from(code as u8)
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
    let (mut sum_dx, mut sum_dy, mut n) = (0.0, 0.0, 0usize);
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
    let mut wins = 0usize;
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
        let codes = [exit::OK, exit::USAGE, exit::FAILED];
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
