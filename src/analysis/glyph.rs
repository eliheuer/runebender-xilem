// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Structured glyph inspection shared by disk and live editor tools.

use crate::document::project::Project;
use crate::document::proposal;
use crate::document::variable::{GlyphLayerAddress, LayerId, SourceId};
use crate::outline::glyph_paths;
use norad::Font;
use serde_json::json;

/// Return geometry, metrics and the stable GLIF revision from one canonical document layer.
///
/// An unknown source, glyph or exact layer returns an object with `ok: false`.
pub fn read_project_glyph(
    project: &Project,
    source: SourceId,
    name: &str,
    layer: Option<&str>,
) -> serde_json::Value {
    let default = match project.document_source(source) {
        Some(source) => source.default_layer(),
        None => return json!({"ok": false, "error": "unknown source"}),
    };
    let selected = layer.map_or(default, |name| LayerId {
        source,
        name: name.to_owned(),
    });
    if let Some(layer) = layer
        && !project
            .glyph_names()
            .any(|name| project.document_layer(name, &selected).is_some())
    {
        return json!({"ok": false, "error": format!("no layer named {layer}")});
    }
    let Some(glyph) = project.document_layer(name, &selected) else {
        return json!({"ok": false, "error": format!("no glyph named {name}")});
    };
    let address = GlyphLayerAddress {
        glyph: name.to_owned(),
        layer: selected.clone(),
    };
    let path = match project.document_layer_path(&address) {
        Ok(path) => path,
        Err(error) => return json!({"ok": false, "error": error.to_string()}),
    };
    let drawn = !path.is_empty();
    let bounds = {
        use kurbo::Shape as _;
        path.bounding_box()
    };
    let joins = join_rows(crate::analysis::curve::cubics_from_layer(glyph));
    json!({
        "ok": true,
        "glyph": name,
        "layer": selected.name,
        "revision": crate::document::edit_batch::canonical_glyph_revision(glyph).ok(),
        "advance": glyph.width(),
        "lsb": if drawn { Some(bounds.x0.round()) } else { None },
        "rsb": if drawn { Some((glyph.width() - bounds.x1).round()) } else { None },
        "bounds": if drawn { Some([bounds.x0, bounds.y0, bounds.x1, bounds.y1]) } else { None },
        "points": glyph.contours().map(|contour| contour.points().count()).sum::<usize>(),
        "contour_count": glyph.contours().count(),
        "unicodes": glyph.codepoints().map(|codepoint| format!("U+{:04X}", codepoint as u32)).collect::<Vec<_>>(),
        "contours": glyph.contours().map(|contour| json!(contour.points().map(|point| {
            let position = point.position();
            json!({
                "x": position.x,
                "y": position.y,
                "type": format!("{:?}", point.point_type()).to_lowercase(),
                "smooth": point.is_smooth(),
            })
        }).collect::<Vec<_>>())).collect::<Vec<_>>(),
        "joins": joins,
        "join_notes": "Direct contours only; components excluded. Contour indices count nonempty contours. Curvature is signed inverse font units. Degenerate tangents are null; no G2 guarantee or optical quality score is inferred.",
        "components": glyph.components().map(|component| component.reference()).collect::<Vec<_>>(),
        "component_transforms": glyph.components().map(|component| json!({
            "base": component.reference(),
            "transform": component.transform().as_coeffs(),
        })).collect::<Vec<_>>(),
        "anchors": glyph.anchors().map(|anchor| {
            let position = anchor.position();
            json!({"name": anchor.name(), "x": position.x, "y": position.y})
        }).collect::<Vec<_>>(),
    })
}

/// Returns geometry, metrics and an edit revision from the supplied in-memory font.
/// An unknown glyph or layer returns an object with `ok: false`.
pub fn read_glyph(font: &Font, name: &str, layer: Option<&str>) -> serde_json::Value {
    let selected = match layer {
        Some(name) => match font.layers.get(name) {
            Some(layer) => layer,
            None => return json!({"ok": false, "error": format!("no layer named {name}")}),
        },
        None => font.default_layer(),
    };
    let Some(glyph) = selected.get_glyph(name) else {
        return json!({ "ok": false, "error": format!("no glyph named {name}") });
    };
    let contours: Vec<serde_json::Value> = glyph
        .contours
        .iter()
        .map(|c| {
            json!(
                c.points
                    .iter()
                    .map(|p| json!({
                        "x": p.x, "y": p.y,
                        "type": format!("{:?}", p.typ).to_lowercase(),
                        "smooth": p.smooth,
                    }))
                    .collect::<Vec<_>>()
            )
        })
        .collect();
    // The numbers a question is usually about come first, computed
    // the way `proof` computes them, so one tool answers width and
    // spacing without a second call.
    let preview = match layer
        .map(|name| proposal::preview_font(font, name))
        .transpose()
    {
        Ok(preview) => preview,
        Err(e) => return json!({"ok": false, "error": e}),
    };
    let path = glyph_paths::glyph_to_bezpath(glyph, preview.as_ref().unwrap_or(font));
    let drawn = !path.is_empty();
    let bounds = {
        use kurbo::Shape as _;
        path.bounding_box()
    };
    let joins = join_rows(crate::analysis::curve::cubics_from_norad(glyph));
    json!({
        "ok": true,
        "glyph": name,
        "layer": selected.name(),
        "revision": crate::document::edit_batch::glyph_revision(glyph).ok(),
        "advance": glyph.width,
        "lsb": if drawn { Some(bounds.x0.round()) } else { None },
        "rsb": if drawn { Some((glyph.width - bounds.x1).round()) } else { None },
        "bounds": if drawn { Some([bounds.x0, bounds.y0, bounds.x1, bounds.y1]) } else { None },
        "points": glyph.contours.iter().map(|c| c.points.len()).sum::<usize>(),
        "contour_count": glyph.contours.len(),
        "unicodes": glyph.codepoints.iter().map(|c| format!("U+{:04X}", c as u32)).collect::<Vec<_>>(),
        "contours": contours,
        "joins": joins,
        "join_notes": "Direct contours only; components excluded. Contour indices count nonempty contours. Curvature is signed inverse font units. Degenerate tangents are null; no G2 guarantee or optical quality score is inferred.",
        "components": glyph.components.iter().map(|c| c.base.to_string()).collect::<Vec<_>>(),
        "component_transforms": glyph.components.iter().map(|c| json!({"base": c.base, "transform": [c.transform.x_scale, c.transform.xy_scale, c.transform.yx_scale, c.transform.y_scale, c.transform.x_offset, c.transform.y_offset]})).collect::<Vec<_>>(),
        "anchors": glyph.anchors.iter().map(|a| json!({ "name": a.name.as_ref().map(|n| n.to_string()), "x": a.x, "y": a.y })).collect::<Vec<_>>(),
    })
}

fn join_rows(contours: Vec<Vec<crate::analysis::curve::Cubic>>) -> Vec<serde_json::Value> {
    contours
        .iter()
        .enumerate()
        .flat_map(|(contour, segments)| {
            segments
                .iter()
                .enumerate()
                .filter_map(move |(segment, next)| {
                    let previous = &segments[(segment + segments.len() - 1) % segments.len()];
                    if previous.p3.distance(next.p0) > 1e-6 {
                        return None;
                    }
                    let incoming = previous.p3 - previous.p2;
                    let outgoing = next.p1 - next.p0;
                    let angle = if incoming.hypot() > 1e-9 && outgoing.hypot() > 1e-9 {
                        Some(incoming.cross(outgoing).atan2(incoming.dot(outgoing)).abs())
                    } else {
                        None
                    };
                    Some(json!({"nonempty_contour":contour,"segment":segment,
                    "at":[next.p0.x,next.p0.y],"intended_smooth":next.start_smooth,
                    "tangent_angle_radians":angle,
                    "incoming_curvature":previous.curvature(1.0),
                    "outgoing_curvature":next.curvature(0.0)}))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use norad::{
        AffineTransform, Anchor, Component, Contour, ContourPoint, Glyph, Name, PointType,
    };

    use super::*;
    use crate::document::project::SourceInput;

    #[test]
    fn canonical_glyph_inspection_matches_the_ufo_boundary_contract() {
        let mut font = Font::new();
        let mut base = Glyph::new("base");
        base.contours.push(Contour::new(
            vec![
                ContourPoint::new(10.0, 20.0, PointType::Line, false, None, None),
                ContourPoint::new(110.0, 20.0, PointType::Line, false, None, None),
                ContourPoint::new(110.0, 220.0, PointType::Line, false, None, None),
            ],
            None,
        ));
        font.default_layer_mut().insert_glyph(base);
        let mut glyph = Glyph::new("A");
        glyph.width = 640.0;
        glyph.codepoints.insert('A');
        glyph.components.push(Component::new(
            Name::new("base").unwrap(),
            AffineTransform {
                x_scale: 1.25,
                xy_scale: 0.125,
                yx_scale: -0.25,
                y_scale: 0.75,
                x_offset: 13.0,
                y_offset: 29.0,
            },
            None,
        ));
        glyph.anchors.push(Anchor::new(
            320.0,
            700.0,
            Some(Name::new("top").unwrap()),
            None,
            None,
        ));
        font.default_layer_mut().insert_glyph(glyph);
        let expected = read_glyph(&font, "A", None);
        let project = Project::from_source(SourceInput::from_font(
            font,
            PathBuf::from("CanonicalInspect.ufo"),
        ));
        let actual = read_project_glyph(&project, SourceId(0), "A", None);
        assert_eq!(
            actual, expected,
            "canonical inspection changed its JSON contract"
        );
        assert_eq!(
            read_project_glyph(&project, SourceId(0), "A", Some("missing")),
            json!({"ok": false, "error": "no layer named missing"}),
            "missing-layer failure changed"
        );
    }
}
