// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! One numeric review of a glyph layer for agents: stems, ink spans, node
//! continuity, grid fit, mark state, and point-structure compatibility with
//! the other sources. Everything here reads; nothing changes a font.

use kurbo::Shape as _;
use serde_json::{Value, json};

use crate::analysis::{
    curve::{self, GLevel},
    dimensions,
    measure::{self, MeasureKind},
};
use crate::font::project::Project;
use crate::font::variable::{LayerId, SourceId};
use crate::font::{LayerShapeView, LayerView};
use crate::outline::glyph_paths;

/// The grids reported for every contour point, coarsest first.
const GRIDS: [i64; 3] = [8, 4, 2];

/// Review one glyph of a live project: its own numbers, plus how its point
/// structure compares with the same glyph in every other source.
///
/// `layer` selects a named layer in that source, such as a proposal layer;
/// `None` is the source's default layer.
pub fn analyze_project_glyph(
    project: &Project,
    source: SourceId,
    name: &str,
    layer: Option<&str>,
) -> Result<Value, String> {
    let selected = project
        .document_source(source)
        .ok_or("unknown source")?
        .default_layer();
    let selected = layer.map_or(selected, |layer| LayerId {
        source,
        name: layer.to_owned(),
    });
    let view = project
        .document_layer(name, &selected)
        .ok_or_else(|| match layer {
            Some(layer) => format!("no glyph named {name} on layer {layer}"),
            None => format!("no glyph named {name}"),
        })?;
    let mut result = analyze_layer(view);
    result["ok"] = json!(true);
    result["glyph"] = json!(name);
    result["source_id"] = json!(source.0);
    result["layer"] = json!(selected.name);
    result["sources"] = json!(compare_with_other_sources(project, source, name, view));
    Ok(result)
}

/// Review one experiment layer against the root sources of the project.
pub fn analyze_experiment_glyph(
    project: &Project,
    experiment: &crate::font::experiments::Experiment,
    branch: &str,
    name: &str,
    layer: Option<&str>,
) -> Result<Value, String> {
    let (selected, view) = experiment
        .selected_layer(name, layer)
        .ok_or_else(|| format!("no glyph named {name} in experiment {branch}"))?;
    let mut result = analyze_layer(view);
    result["ok"] = json!(true);
    result["glyph"] = json!(name);
    result["source_id"] = json!(experiment.root.0);
    result["branch"] = json!(branch);
    result["layer"] = json!(selected.name);
    result["sources"] = json!(compare_with_other_sources(
        project,
        experiment.root,
        name,
        view
    ));
    Ok(result)
}

fn compare_with_other_sources(
    project: &Project,
    source: SourceId,
    name: &str,
    view: LayerView<'_>,
) -> Vec<Value> {
    project
        .document_sources()
        .filter(|other| other.id() != source)
        .map(|other| {
            let id = other.id();
            match project.document_layer(name, &other.default_layer()) {
                Some(layer) => {
                    let (stem, bar) = dimensions::stem_and_bar_from_layer(layer);
                    let reason = compatibility(view, layer).err();
                    json!({
                        "source_id": id.0,
                        "name": other.name(),
                        "present": true,
                        "width": layer.width(),
                        "stem": stem,
                        "bar": bar,
                        "compatible": reason.is_none(),
                        "reason": reason,
                    })
                }
                None => json!({
                    "source_id": id.0,
                    "name": other.name(),
                    "present": false,
                    "compatible": false,
                    "reason": "glyph missing in this source",
                }),
            }
        })
        .collect()
}

/// The numbers of one layer on its own.
pub fn analyze_layer(layer: LayerView<'_>) -> Value {
    let (stem, bar) = dimensions::stem_and_bar_from_layer(layer);
    let filled = glyph_paths::ordinary_layer_contours_to_bezpath(layer);
    let mut spans: Vec<Value> = measure::ordinary_layer_measurements(layer)
        .iter()
        .filter(|m| matches!(m.kind, MeasureKind::Horizontal | MeasureKind::Vertical))
        .map(|m| {
            let midpoint = kurbo::Point::new((m.a.x + m.b.x) / 2.0, (m.a.y + m.b.y) / 2.0);
            json!({
                "axis": if m.kind == MeasureKind::Horizontal { "horizontal" } else { "vertical" },
                "length": m.length,
                "ink": filled.contains(midpoint),
                "from": [m.a.x, m.a.y],
                "to": [m.b.x, m.b.y],
            })
        })
        .collect();
    spans.sort_by_key(|span| span["length"].as_i64().unwrap_or_default());
    spans.truncate(64);

    let cubics = curve::cubics_from_layer(layer);
    let nodes = curve::node_continuity(&cubics);
    let count = |level: GLevel| nodes.iter().filter(|n| n.level == level).count();
    let issues: Vec<Value> = nodes
        .iter()
        .filter(|n| matches!(n.level, GLevel::Kink | GLevel::G1))
        .map(|n| {
            json!({
                "x": n.at.x,
                "y": n.at.y,
                "level": level_name(n.level),
                "curvature_jump": (n.jump * 1000.0).round() / 1000.0,
            })
        })
        .collect();

    let mut points = 0_usize;
    let mut on_grid = [0_usize; GRIDS.len()];
    let mut off_integer = 0_usize;
    for contour in layer.contours() {
        for point in contour.points() {
            points += 1;
            let p = point.position();
            let (Some(x), Some(y)) = (integer_units(p.x), integer_units(p.y)) else {
                off_integer += 1;
                continue;
            };
            for (slot, grid) in on_grid.iter_mut().zip(GRIDS) {
                if x % grid == 0 && y % grid == 0 {
                    *slot += 1;
                }
            }
        }
    }
    let mut grid = serde_json::Map::new();
    grid.insert("points".into(), json!(points));
    for (slot, g) in on_grid.iter().zip(GRIDS) {
        grid.insert(format!("on_{g}"), json!(slot));
    }
    grid.insert("off_integer".into(), json!(off_integer));

    let width = layer.width();
    json!({
        "width": width,
        "width_on_8": integer_units(width).is_some_and(|w| w % 8 == 0),
        "stem": stem,
        "bar": bar,
        "spans": spans,
        "contours": layer.contours().count(),
        "components": layer.components().count(),
        "anchors": layer.anchors().count(),
        "nodes": {
            "total": nodes.len(),
            "corner": count(GLevel::Corner),
            "kink": count(GLevel::Kink),
            "g1_line": count(GLevel::G1Line),
            "g1": count(GLevel::G1),
            "g2": count(GLevel::G2),
            "g3": count(GLevel::G3),
            "issues": issues,
        },
        "max_curvature": curve::max_curvature(&cubics),
        "grid": grid,
        "mark": layer.mark_label().ok().flatten(),
        "mark_color": layer.mark_color().ok().flatten().map(|c| [c.red, c.green, c.blue, c.alpha]),
    })
}

/// A coordinate as whole font units, or `None` when it carries a fraction or
/// lies outside the range a glyph can use.
fn integer_units(value: f64) -> Option<i64> {
    const LIMIT: f64 = 1_000_000.0;
    (value.fract() == 0.0 && value.abs() <= LIMIT).then(|| {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "checked above: whole and within one million units, so the cast is exact"
        )]
        let units = value as i64;
        units
    })
}

fn level_name(level: GLevel) -> &'static str {
    match level {
        GLevel::Corner => "corner",
        GLevel::Kink => "kink",
        GLevel::G1Line => "g1_line",
        GLevel::G1 => "g1",
        GLevel::G2 => "g2",
        GLevel::G3 => "g3",
    }
}

/// Why two layers would not interpolate, in one sentence, or `Ok` when their
/// shapes, point types, component references and anchor names line up.
pub fn compatibility(first: LayerView<'_>, second: LayerView<'_>) -> Result<(), String> {
    let a: Vec<_> = first.shapes().collect();
    let b: Vec<_> = second.shapes().collect();
    if a.len() != b.len() {
        return Err(format!("{} shapes against {}", a.len(), b.len()));
    }
    for (index, (x, y)) in a.iter().zip(&b).enumerate() {
        match (*x, *y) {
            (LayerShapeView::Contour(x), LayerShapeView::Contour(y)) => {
                if x.is_closed() != y.is_closed() {
                    return Err(format!("contour {index} is closed in one layer only"));
                }
                if x.is_hyper() != y.is_hyper() {
                    return Err(format!(
                        "contour {index} is a hyperbezier in one layer only"
                    ));
                }
                let xs: Vec<_> = x.points().map(|p| p.point_type()).collect();
                let ys: Vec<_> = y.points().map(|p| p.point_type()).collect();
                if xs.len() != ys.len() {
                    return Err(format!(
                        "contour {index} has {} points against {}",
                        xs.len(),
                        ys.len()
                    ));
                }
                if let Some(at) = xs.iter().zip(&ys).position(|(p, q)| p != q) {
                    return Err(format!(
                        "contour {index} point {at} is {:?} against {:?}",
                        xs[at], ys[at]
                    ));
                }
            }
            (LayerShapeView::Component(x), LayerShapeView::Component(y)) => {
                if x.reference() != y.reference() {
                    return Err(format!(
                        "component {index} references {} against {}",
                        x.reference(),
                        y.reference()
                    ));
                }
            }
            _ => return Err(format!("shape {index} is a contour in one layer only")),
        }
    }
    let anchors_a = first.anchors().count();
    let anchors_b = second.anchors().count();
    if anchors_a != anchors_b {
        return Err(format!("{anchors_a} anchors against {anchors_b}"));
    }
    for anchor in first.anchors() {
        if second
            .anchors()
            .filter(|o| o.name() == anchor.name())
            .count()
            != 1
        {
            return Err(format!("anchor {} has no single match", anchor.name()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fonts;

    #[test]
    fn virtua_h_reports_stems_and_compares_both_masters() {
        let project = Project::load(&fonts::designspace()).expect("the designspace loads");
        let ids: Vec<_> = project.document_sources().map(|s| s.id()).collect();
        let report = analyze_project_glyph(&project, ids[0], "H", None).unwrap();
        assert_eq!(report["ok"], true);
        assert!(report["stem"].as_i64().unwrap() > 0, "{report}");
        assert!(report["bar"].as_i64().unwrap() > 0, "{report}");
        assert!(report["grid"]["points"].as_u64().unwrap() >= 12);
        let sources = report["sources"].as_array().unwrap();
        assert_eq!(sources.len(), ids.len() - 1);
        assert_eq!(sources[0]["present"], true);
        assert_eq!(sources[0]["compatible"], true, "{}", sources[0]);
        assert!(report["nodes"]["total"].as_u64().unwrap() >= 12);
    }

    #[test]
    fn missing_glyphs_and_layers_are_errors_not_panics() {
        let project = Project::load(&fonts::designspace()).expect("the designspace loads");
        let id = project.document_sources().next().unwrap().id();
        assert!(analyze_project_glyph(&project, id, "nothere", None).is_err());
        assert!(analyze_project_glyph(&project, id, "H", Some("no.such.layer")).is_err());
        assert!(analyze_project_glyph(&project, SourceId(99), "H", None).is_err());
    }

    #[test]
    fn compatibility_names_the_first_difference() {
        let project = Project::load(&fonts::designspace()).expect("the designspace loads");
        let id = project.document_sources().next().unwrap().id();
        let default = project.document_source(id).unwrap().default_layer();
        let h = project.document_layer("H", &default).unwrap();
        let o = project.document_layer("O", &default).unwrap();
        assert_eq!(compatibility(h, h), Ok(()));
        let reason = compatibility(h, o).unwrap_err();
        assert!(
            reason.contains("shapes") || reason.contains("points") || reason.contains("point"),
            "{reason}"
        );
    }
}
