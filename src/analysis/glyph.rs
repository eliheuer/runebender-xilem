// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Structured glyph inspection shared by disk and live editor tools.

use crate::font::project::Project;
use crate::font::variable::{GlyphLayerAddress, LayerId, SourceId};
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
    let mut result = read_canonical_layer(name, &selected.name, glyph, path);
    result["source_id"] = json!(source.0);
    result["glyph_id"] = json!(
        project
            .document_glyph(name)
            .map(|glyph| glyph.id().to_wire())
    );
    result
}

/// Return geometry and metrics from one isolated canonical experiment layer.
pub fn read_experiment_glyph(
    experiment: &crate::font::experiments::Experiment,
    name: &str,
    layer: Option<&str>,
) -> serde_json::Value {
    let Some((selected, glyph)) = experiment.selected_layer(name, layer) else {
        return json!({"ok": false, "error": format!("no glyph named {name}")});
    };
    let path = match experiment.layer_path(name, &selected) {
        Ok(path) => path,
        Err(error) => return json!({"ok": false, "error": error.to_string()}),
    };
    read_canonical_layer(name, &selected.name, glyph, path)
}

fn read_canonical_layer(
    name: &str,
    layer_name: &str,
    glyph: crate::font::LayerView<'_>,
    path: kurbo::BezPath,
) -> serde_json::Value {
    let drawn = !path.is_empty();
    let bounds = {
        use kurbo::Shape as _;
        path.bounding_box()
    };
    let joins = join_rows(crate::analysis::curve::cubics_from_layer(glyph));
    json!({
        "ok": true,
        "glyph": name,
        "layer": layer_name,
        "revision": crate::font::edit_batch::canonical_glyph_revision(glyph).ok(),
        "advance": glyph.width(),
        "lsb": if drawn { Some(bounds.x0.round()) } else { None },
        "rsb": if drawn { Some((glyph.width() - bounds.x1).round()) } else { None },
        "bounds": if drawn { Some([bounds.x0, bounds.y0, bounds.x1, bounds.y1]) } else { None },
        "points": glyph.contours().map(|contour| contour.points().count()).sum::<usize>(),
        "contour_count": glyph.contours().count(),
        "unicodes": glyph.codepoints().map(|codepoint| format!("U+{:04X}", codepoint as u32)).collect::<Vec<_>>(),
        "contour_ids": glyph.contours().map(|contour| contour.id().to_wire()).collect::<Vec<_>>(),
        "contours": glyph.contours().map(|contour| json!(contour.points().map(|point| {
            let position = point.position();
            json!({
                "id": point.id().to_wire(),
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
            "id": component.id().to_wire(),
            "base": component.reference(),
            "transform": component.transform().as_coeffs(),
        })).collect::<Vec<_>>(),
        "anchors": glyph.anchors().map(|anchor| {
            let position = anchor.position();
            json!({"id": anchor.id().to_wire(), "name": anchor.name(), "x": position.x, "y": position.y})
        }).collect::<Vec<_>>(),
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
        AffineTransform, Anchor, Component, Contour, ContourPoint, Font, Glyph, Name, PointType,
    };

    use super::*;
    use crate::font::project::SourceInput;

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
        let mut project = Project::from_source(SourceInput::from_font(
            font,
            PathBuf::from("CanonicalInspect.ufo"),
        ));
        let actual = read_project_glyph(&project, SourceId(0), "A", None);
        assert_eq!(actual["ok"], true, "{actual:#}");
        assert_eq!(actual["glyph"], "A");
        assert_eq!(actual["layer"], "public.default");
        assert_eq!(actual["advance"], 640.0);
        assert_eq!(actual["lsb"], 21.0);
        assert_eq!(actual["rsb"], 495.0);
        assert_eq!(actual["bounds"], json!([20.5, 45.25, 145.5, 207.75]));
        assert_eq!(actual["points"], 0);
        assert_eq!(actual["contour_count"], 0);
        assert_eq!(actual["unicodes"], json!(["U+0041"]));
        assert_eq!(actual["components"], json!(["base"]));
        assert_eq!(
            actual["component_transforms"],
            json!([{"id":actual["component_transforms"][0]["id"],"base":"base","transform":[1.25,0.125,-0.25,0.75,13.0,29.0]}])
        );
        assert_eq!(
            actual["anchors"],
            json!([{"id":actual["anchors"][0]["id"],"name":"top","x":320.0,"y":700.0}])
        );
        assert!(actual["revision"].as_str().is_some());
        assert!(actual["component_transforms"][0]["id"].as_str().is_some());
        assert!(actual["anchors"][0]["id"].as_str().is_some());
        let base = read_project_glyph(&project, SourceId(0), "base", None);
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        project
            .edit_document_layer("base", &layer, |draft| {
                draft.set_width(500.0)?;
                Ok(())
            })
            .unwrap();
        project.rename_document_glyph("base", "renamed").unwrap();
        let renamed = read_project_glyph(&project, SourceId(0), "renamed", None);
        assert_eq!(base["glyph_id"], renamed["glyph_id"]);
        assert_eq!(base["contour_ids"], renamed["contour_ids"]);
        assert_eq!(
            base["contours"][0][0]["id"],
            renamed["contours"][0][0]["id"]
        );
        assert!(base["contours"][0][0]["id"].as_str().is_some());
        assert_ne!(base["revision"], renamed["revision"]);

        assert_eq!(
            read_project_glyph(&project, SourceId(0), "A", Some("missing")),
            json!({"ok": false, "error": "no layer named missing"}),
            "missing-layer failure changed"
        );
    }
}
