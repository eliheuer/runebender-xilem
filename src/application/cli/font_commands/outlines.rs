// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Outline analysis and metaball conversion commands.

use super::*;

/// What the reference glyphs say the heavier master should do.
///
/// Reports rather than writes. Seeing the offset and the list first is
/// the difference between a tool you can trust with a font and one you
/// run once and then undo.
pub(in crate::application::cli) fn bolden(
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
    layer: runebender::font::LayerView<'_>,
) -> Vec<Vec<(kurbo::Point, runebender::font::LayerPointType, bool)>> {
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
    first: runebender::font::LayerView<'_>,
    second: runebender::font::LayerView<'_>,
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

fn flat_layer_points(layer: runebender::font::LayerView<'_>) -> Vec<(f64, f64)> {
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
    layer: runebender::font::LayerView<'_>,
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

pub(in crate::application::cli) fn collapse_metaballs(
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
