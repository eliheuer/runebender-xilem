// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The commands that read a font and report.
//!
//! Nothing here writes. Each one loads a source, answers one
//! question, and prints it as text or as JSON.

use std::path::Path;

use norad::Font;
use runebender_core::document::font_ops;
use runebender_core::{analysis::optical, analysis::spacing, outline::embolden};
use serde_json::{Value, json};

use crate::shell::{emit, exit, fail, open};

pub(crate) fn info(source: &Path, json: bool) -> i32 {
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

/// Every glyph name in a source, one per line.
///
/// The plain form is meant to be piped: `runebender-core glyphs Font.ufo |
/// xargs -P 8 -I{} runebender-core measure Font.ufo --glyph {}`.
pub(crate) fn glyphs(source: &Path, json: bool) -> i32 {
    let font = match open(source, json) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let mut names: Vec<String> = font
        .default_layer()
        .iter()
        .map(|g| g.name().to_string())
        .collect();
    names.sort();
    emit(json, json!({ "ok": true, "glyphs": names.clone() }), || {
        for name in &names {
            println!("{name}");
        }
    })
}

pub(crate) fn measure(source: &Path, name: &str, json: bool) -> i32 {
    let font = match open(source, json) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let Some(glyph) = font.default_layer().get_glyph(name) else {
        return fail(
            json,
            exit::USAGE,
            &format!("no glyph {name} in {}", source.display()),
        );
    };
    let advance = glyph.width;
    let contours = glyph.contours.len();
    let points: usize = glyph.contours.iter().map(|c| c.points.len()).sum();
    let signature = font_ops::glyph_signature(glyph);
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

/// Optical weight: the proof-reading pass, done by measurement.
///
/// Glyphs are compared within their own case, because lowercase and
/// uppercase fill their boxes differently and comparing across them
/// would flag the whole alphabet.
pub(crate) fn color(source: &Path, tolerance: f64, json: bool) -> i32 {
    let font = match open(source, json) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let x_height = font.font_info.x_height.unwrap_or(0.0);
    let cap_height = font.font_info.cap_height.unwrap_or(0.0);
    if x_height <= 0.0 || cap_height <= 0.0 {
        return fail(
            json,
            exit::USAGE,
            "the source needs xHeight and capHeight in fontinfo",
        );
    }
    let mut lower = Vec::new();
    let mut upper = Vec::new();
    for glyph in font.default_layer().iter() {
        let Some(c) = glyph.codepoints.iter().next() else {
            continue;
        };
        if c.is_lowercase() {
            lower.push(glyph.name().to_string());
        } else if c.is_uppercase() {
            upper.push(glyph.name().to_string());
        }
    }
    let mut found = optical::outliers(&font, &lower, x_height, tolerance, "lowercase");
    found.extend(optical::outliers(
        &font,
        &upper,
        cap_height,
        tolerance,
        "uppercase",
    ));
    let ok = found.is_empty();
    if json {
        let items: Vec<Value> = found
            .iter()
            .map(|o| {
                json!({
                    "glyph": o.glyph, "group": o.group,
                    "ratio": (o.ratio * 1000.0).round() / 1000.0,
                    "density": (o.density * 10000.0).round() / 10000.0,
                    "median": (o.median * 10000.0).round() / 10000.0,
                    "reads": if o.ratio > 1.0 { "darker" } else { "lighter" },
                })
            })
            .collect();
        println!(
            "{}",
            json!({
                "ok": ok, "tolerance": tolerance,
                "compared": lower.len() + upper.len(), "findings": items,
            })
        );
    } else if ok {
        println!(
            "{} glyphs compared, none more than {:.0}% off",
            lower.len() + upper.len(),
            tolerance * 100.0
        );
    } else {
        for o in &found {
            let dir = if o.ratio > 1.0 { "darker" } else { "lighter" };
            println!(
                "{:<20} {:>6.1}% {dir} than the {} median",
                o.glyph,
                (o.ratio - 1.0).abs() * 100.0,
                o.group
            );
        }
        println!("{} of {} compared", found.len(), lower.len() + upper.len());
    }
    if ok { exit::OK } else { exit::FINDINGS }
}

/// What the reference glyphs say the heavier master should do.
///
/// Reports rather than writes. Seeing the offset and the list first is
/// the difference between a tool you can trust with a font and one you
/// run once and then undo.
pub(crate) fn bolden(
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

/// Spacing against the family's own grid.
pub(crate) fn spacing_cmd(source: &Path, step: Option<f64>, json: bool) -> i32 {
    let font = match open(source, json) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let sides = spacing::sidebearings(&font);
    let Some(step) = step.or_else(|| spacing::infer_step(&sides)) else {
        return fail(
            json,
            exit::USAGE,
            "no grid step fits this spacing; pass --step to check against one",
        );
    };
    let found = spacing::off_grid(&sides, step);
    let ok = found.is_empty();
    if json {
        let items: Vec<Value> = found
            .iter()
            .map(|o| {
                json!({
                    "glyph": o.glyph, "side": o.side,
                    "value": o.value, "offBy": o.off_by,
                })
            })
            .collect();
        println!(
            "{}",
            json!({
                "ok": ok, "step": step, "glyphs": sides.len(), "findings": items,
            })
        );
    } else if ok {
        println!(
            "{} glyphs on a {step:.0}-unit grid, none off it",
            sides.len()
        );
    } else {
        for o in &found {
            println!(
                "{:<22} {:<5} {:>7.1}  off by {:>+5.1}",
                o.glyph, o.side, o.value, o.off_by
            );
        }
        println!(
            "{} of {} sidebearings off the {step:.0}-unit grid",
            found.len(),
            sides.len() * 2
        );
        // Findings that all sit exactly half a step out are not drift.
        // They are a family using a finer grid than the one inferred.
        let half = found
            .iter()
            .filter(|o| (o.off_by.abs() - step / 2.0).abs() < 0.01)
            .count();
        if half * 10 >= found.len() * 6 {
            println!(
                "{half} of them are exactly half a step out, so this family \
                 may be drawn on {:.0}s: try --step {:.0}",
                step / 2.0,
                step / 2.0
            );
        }
    }
    if ok { exit::OK } else { exit::FINDINGS }
}

/// Interpolation compatibility, the check that costs the most to get
/// wrong: a mismatch here is invisible until the family stops building.
pub(crate) fn check(a: &Path, b: &Path, limit: usize, json: bool) -> i32 {
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
        let (sa, sb) = (
            font_ops::glyph_signature(glyph),
            font_ops::glyph_signature(other),
        );
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
        println!(
            "{}",
            json!({ "ok": ok, "compared": compared, "findings": findings })
        );
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
                .map(|i| ContourPoint::new(i as f64, 0.0, PointType::Line, false, None, None))
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
        assert_ne!(font_ops::glyph_signature(&a), font_ops::glyph_signature(&b));
    }

    #[test]
    fn the_same_shape_matches() {
        let a = glyph("a", &[4, 4]);
        let b = glyph("a", &[4, 4]);
        assert_eq!(font_ops::glyph_signature(&a), font_ops::glyph_signature(&b));
    }
}
