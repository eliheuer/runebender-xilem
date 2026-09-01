// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The commands that change a font and save it.
//!
//! Each one is a thin wrapper over an operation the editor runs from a
//! menu. The wrapper is the same every time: expand the paths into
//! UFOs, pick the glyphs in scope, run the operation, save what
//! changed, and report. `--dry-run` does everything but the save.

use std::collections::HashSet;
use std::path::PathBuf;

use norad::{Font, Glyph};
use serde_json::{Value, json};

use runebender_core::document::{composites, font_ops};
use runebender_core::outline::{cleanup, convert, effects, glyph_ops};

use crate::shell::{exit, fail, open};
use crate::sources;

/// The options every editing command takes.
///
/// Sources come first so a shell glob reads naturally:
/// `runebender-core clean sources/*.ufo`.
#[derive(clap::Args, Clone)]
pub(crate) struct Edit {
    /// UFO directories or designspace files. A designspace stands for
    /// its sources.
    #[arg(required = true)]
    pub sources: Vec<PathBuf>,
    /// Glyph names to work on, comma separated. Defaults to every
    /// glyph. Names a source does not have are skipped.
    #[arg(long, value_delimiter = ',', global = true)]
    pub glyphs: Option<Vec<String>>,
    /// Report what would change and write nothing. Exits 1 when there
    /// is something to change.
    #[arg(long, global = true)]
    pub dry_run: bool,
    /// Write the result here instead of over the source. One source
    /// only.
    #[arg(long, global = true)]
    pub out: Option<PathBuf>,
}

/// What one operation did to one source.
#[derive(Default)]
pub(crate) struct Tally {
    /// The glyphs the operation changed, by name. A font-level
    /// operation leaves this empty.
    pub names: Vec<String>,
    /// Individual changes, counted the way the operation counts them.
    pub edits: usize,
}

/// The glyphs in scope for a source.
///
/// A name the source does not have is dropped rather than refused, so
/// one command can sweep masters that do not hold the same glyphs.
fn scope(font: &Font, wanted: Option<&[String]>) -> Vec<String> {
    match wanted {
        Some(names) => names
            .iter()
            .filter(|n| font.default_layer().get_glyph(n.as_str()).is_some())
            .cloned()
            .collect(),
        None => font
            .default_layer()
            .iter()
            .map(|g| g.name().to_string())
            .collect(),
    }
}

/// Runs one font-level operation over every source and saves.
///
/// This is where every editing command ends up. It owns the two
/// things a batch run has to get right: a source is written only when
/// the operation changed it, and a dry run exits 1 so a script can
/// tell "nothing to do" from "work waiting".
pub(crate) fn run(
    args: &Edit,
    json: bool,
    mut op: impl FnMut(&mut Font, &[String]) -> Tally,
) -> i32 {
    let ufos = match sources::expand_all(&args.sources) {
        Ok(v) => v,
        Err(e) => return fail(json, exit::USAGE, &e),
    };
    if args.out.is_some() && ufos.len() > 1 {
        return fail(
            json,
            exit::USAGE,
            "--out takes one source; without it each source is written in place",
        );
    }
    let mut reports: Vec<Value> = Vec::new();
    let mut glyphs = 0;
    let mut edits = 0;
    for path in &ufos {
        let mut font = match open(path, json) {
            Ok(f) => f,
            Err(code) => return code,
        };
        let names = scope(&font, args.glyphs.as_deref());
        let tally = op(&mut font, &names);
        glyphs += tally.names.len();
        edits += tally.edits;
        // With --out the caller asked for a copy, so write it even
        // when the operation found nothing to do.
        let target = args.out.as_ref().unwrap_or(path);
        let write = !args.dry_run && (tally.edits > 0 || args.out.is_some());
        if write && let Err(e) = font.save(target) {
            return fail(json, exit::FAILED, &format!("{}: {e}", target.display()));
        }
        reports.push(json!({
            "source": path.display().to_string(),
            "glyphs": tally.names.len(),
            "changed": tally.names,
            "edits": tally.edits,
            "written": write,
        }));
    }
    let pending = args.dry_run && edits > 0;
    if json {
        println!(
            "{}",
            json!({
                "ok": true, "dryRun": args.dry_run,
                "glyphs": glyphs, "edits": edits,
                "sources": reports,
            })
        );
    } else {
        for report in &reports {
            println!(
                "{}  {} glyphs  {} edits{}",
                report["source"].as_str().unwrap_or("?"),
                report["glyphs"],
                report["edits"],
                if report["written"] == json!(true) {
                    ""
                } else if args.dry_run {
                    "  (dry run)"
                } else {
                    "  (unchanged)"
                },
            );
        }
        if ufos.len() > 1 {
            println!("{glyphs} glyphs, {edits} edits, {} sources", ufos.len());
        }
    }
    if pending { exit::FINDINGS } else { exit::OK }
}

/// Runs one glyph-level operation over the glyphs in scope.
///
/// The glyph is cloned out, edited, and put back only when the
/// operation reports a change, so the font on disk keeps the bytes it
/// had for everything the command did not touch.
pub(crate) fn each_glyph(
    args: &Edit,
    json: bool,
    mut op: impl FnMut(&Font, &mut Glyph) -> usize,
) -> i32 {
    run(args, json, |font, names| {
        let mut tally = Tally::default();
        for name in names {
            let Some(mut glyph) = font.default_layer().get_glyph(name.as_str()).cloned() else {
                continue;
            };
            let edits = op(font, &mut glyph);
            if edits > 0 {
                font.default_layer_mut().insert_glyph(glyph);
                tally.names.push(name.clone());
                tally.edits += edits;
            }
        }
        tally
    })
}

/// Tidy contours, correct path directions, and round coordinates.
///
/// The pass a source gets before it is committed or built. With no
/// flags it does the three safe ones. Adding extreme points changes
/// how a curve is drawn rather than only how it is stored, so it is
/// asked for by name.
pub(crate) fn clean(
    args: &Edit,
    json: bool,
    tidy: bool,
    directions: bool,
    round: bool,
    extremes: bool,
) -> i32 {
    // No flag means the three that cannot change a shape.
    let none = !(tidy || directions || round || extremes);
    let (tidy, directions, round) = (tidy || none, directions || none, round || none);
    let all = HashSet::new();
    each_glyph(args, json, |_, glyph| {
        let mut edits = 0;
        if tidy {
            edits += cleanup::tidy_contours(glyph);
        }
        if extremes && cleanup::add_extreme_points(glyph, &all) {
            edits += 1;
        }
        if directions {
            edits += cleanup::correct_path_directions(glyph);
        }
        if round {
            edits += cleanup::round_glyph_coordinates(glyph);
        }
        edits
    })
}

/// A contour set reduced to what a designer would call the same
/// drawing: the points, rounded to the unit grid.
///
/// Identifiers are left out, and each contour is rotated to a fixed
/// starting point before it is compared, because an operation that
/// only renames a point or starts the same loop somewhere else has
/// not changed the drawing. Contours are sorted, so their order in
/// the file does not count either.
fn outline(contours: &[norad::Contour]) -> Vec<Vec<(i64, i64, bool)>> {
    let mut out: Vec<Vec<(i64, i64, bool)>> = contours
        .iter()
        .map(|c| {
            let points: Vec<(i64, i64, bool)> = c
                .points
                .iter()
                .map(|p| {
                    (
                        p.x.round() as i64,
                        p.y.round() as i64,
                        p.typ == norad::PointType::OffCurve,
                    )
                })
                .collect();
            rotate_to_smallest(points)
        })
        .collect();
    out.sort();
    out
}

/// Rotates a closed loop so it starts at the point that makes the
/// whole sequence smallest, which gives one loop one spelling.
fn rotate_to_smallest(points: Vec<(i64, i64, bool)>) -> Vec<(i64, i64, bool)> {
    if points.is_empty() {
        return points;
    }
    let mut best = 0;
    for start in 1..points.len() {
        let rotated = (0..points.len()).map(|i| points[(start + i) % points.len()]);
        let current = (0..points.len()).map(|i| points[(best + i) % points.len()]);
        if rotated.lt(current) {
            best = start;
        }
    }
    (0..points.len())
        .map(|i| points[(best + i) % points.len()])
        .collect()
}

/// Flatten overlapping contours into their union.
pub(crate) fn overlap(args: &Edit, json: bool) -> i32 {
    each_glyph(args, json, |_, glyph| {
        let Some(contours) = glyph_ops::remove_overlap(glyph) else {
            return 0;
        };
        // The solver re-emits every contour it is given, point
        // identifiers and all, so a byte comparison calls every glyph
        // changed. Compare the outline instead: same contours, same
        // points, same places, means the glyph had no overlap and
        // writing it back would only churn the file.
        if outline(&contours) == outline(&glyph.contours) {
            return 0;
        }
        glyph.contours = contours;
        1
    })
}

/// Move every outline out or in by a fixed distance.
pub(crate) fn offset(args: &Edit, json: bool, by: f64) -> i32 {
    each_glyph(args, json, |_, glyph| {
        usize::from(effects::offset_glyph_contours(glyph, by))
    })
}

/// Convert outlines between quadratic and cubic curves.
pub(crate) fn convert_curves(args: &Edit, json: bool, to_cubic: bool, tolerance: f64) -> i32 {
    each_glyph(args, json, move |_, glyph| {
        let changed = if to_cubic {
            convert::quads_to_cubics(glyph)
        } else {
            convert::cubics_to_quads(glyph, tolerance)
        };
        usize::from(changed)
    })
}

/// Put every component back on its anchors.
pub(crate) fn realign(args: &Edit, json: bool, seed_anchors: bool) -> i32 {
    each_glyph(args, json, move |font, glyph| {
        usize::from(composites::realign_glyph(font, glyph, seed_anchors))
    })
}

/// Rename a glyph, and every reference to it.
pub(crate) fn rename(args: &Edit, json: bool, from: &str, to: &str) -> i32 {
    run(args, json, |font, _| {
        if font_ops::rename_glyph(font, from, to) {
            Tally {
                names: vec![to.to_string()],
                edits: 1,
            }
        } else {
            Tally::default()
        }
    })
}

/// Set a glyph's Unicode value.
pub(crate) fn unicode(args: &Edit, json: bool, glyph: &str, to: &str) -> i32 {
    let mut found = false;
    let code = each_glyph(args, json, |_, g| {
        if g.name().as_str() != glyph {
            return 0;
        }
        found = true;
        usize::from(font_ops::set_glyph_unicode(g, to))
    });
    if !found && code == exit::OK {
        return fail(
            json,
            exit::USAGE,
            &format!("no glyph {glyph} in any source"),
        );
    }
    code
}

/// Read or write one kerning pair.
///
/// The pair is the glyph names, and the value follows the same group
/// fallback the editor shows, so a script sees what a designer sees.
pub(crate) fn kern(args: &Edit, json: bool, left: &str, right: &str, set: Option<f64>) -> i32 {
    let Some(value) = set else {
        let mut code = exit::OK;
        for path in match sources::expand_all(&args.sources) {
            Ok(v) => v,
            Err(e) => return fail(json, exit::USAGE, &e),
        } {
            let font = match open(&path, json) {
                Ok(f) => f,
                Err(c) => return c,
            };
            let value = font_ops::kern_value(&font, left, right);
            if json {
                println!(
                    "{}",
                    json!({ "ok": true, "source": path.display().to_string(),
                            "left": left, "right": right, "value": value })
                );
            } else {
                println!("{}  {left} {right}  {value}", path.display());
            }
            code = exit::OK;
        }
        return code;
    };
    run(args, json, |font, _| {
        if (font_ops::kern_value(font, left, right) - value).abs() < f64::EPSILON {
            return Tally::default();
        }
        font_ops::set_kern_pair(font, left, right, value);
        Tally {
            names: Vec::new(),
            edits: 1,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use norad::{Contour, ContourPoint, PointType};

    /// A square, starting at whichever corner is asked for.
    fn square(start: usize) -> Contour {
        let corners = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let points = (0..4)
            .map(|i| {
                let (x, y) = corners[(start + i) % 4];
                ContourPoint::new(x, y, PointType::Line, false, None, None)
            })
            .collect();
        Contour::new(points, None)
    }

    /// The solver is free to start a loop anywhere, and that is the
    /// difference the editing commands must not mistake for work.
    #[test]
    fn the_same_loop_from_a_different_corner_is_the_same_outline() {
        assert_eq!(outline(&[square(0)]), outline(&[square(2)]));
    }

    #[test]
    fn a_moved_point_is_a_different_outline() {
        let mut moved = square(0);
        moved.points[1].x = 200.0;
        assert_ne!(outline(&[square(0)]), outline(&[moved]));
    }

    /// Sub-unit differences are below what a UFO records after
    /// rounding, so they must not count as an edit.
    #[test]
    fn a_hair_of_movement_is_the_same_outline() {
        let mut nudged = square(0);
        nudged.points[1].x = 100.2;
        assert_eq!(outline(&[square(0)]), outline(&[nudged]));
    }

    /// A name a source does not have is skipped rather than refused,
    /// so one command can sweep masters that hold different glyphs.
    #[test]
    fn scope_keeps_only_the_glyphs_a_source_has() {
        let mut font = Font::new();
        font.default_layer_mut().insert_glyph(Glyph::new("a"));
        let wanted = vec!["a".to_string(), "b".to_string()];
        assert_eq!(scope(&font, Some(&wanted)), vec!["a".to_string()]);
        assert_eq!(scope(&font, None), vec!["a".to_string()]);
    }

    /// The whole write path, on a font with a point off the grid: a
    /// dry run reports and exits 1, the real run writes, and running
    /// it again finds nothing. Batch scripts depend on all three.
    #[test]
    fn a_dry_run_reports_and_a_real_run_writes() {
        let dir = std::env::temp_dir().join(format!("rb-cli-clean-{}.ufo", std::process::id()));
        let mut font = Font::new();
        let mut glyph = Glyph::new("a");
        let points = [(0.4, 0.0), (100.0, 0.0), (100.0, 100.0)]
            .into_iter()
            .map(|(x, y)| ContourPoint::new(x, y, PointType::Line, false, None, None))
            .collect();
        glyph.contours.push(Contour::new(points, None));
        font.default_layer_mut().insert_glyph(glyph);
        font.save(&dir).expect("saves");

        let args = Edit {
            sources: vec![dir.clone()],
            glyphs: None,
            dry_run: true,
            out: None,
        };
        assert_eq!(
            clean(&args, true, false, false, true, false),
            exit::FINDINGS,
            "a dry run with work waiting must exit 1"
        );

        let write = Edit {
            dry_run: false,
            ..args.clone()
        };
        assert_eq!(clean(&write, true, false, false, true, false), exit::OK);
        let reloaded = Font::load(&dir).expect("reloads");
        let saved = reloaded.default_layer().get_glyph("a").expect("glyph");
        assert_eq!(saved.contours[0].points[0].x, 0.0, "the point was rounded");

        assert_eq!(
            clean(&args, true, false, false, true, false),
            exit::OK,
            "a second dry run has nothing left to do"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
