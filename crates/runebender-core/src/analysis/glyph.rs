// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Structured glyph inspection shared by disk and live editor tools.

use crate::document::proposal;
use crate::outline::glyph_paths;
use norad::Font;
use serde_json::json;

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
    let joins: Vec<_> = crate::analysis::curve::cubics_from_norad(glyph)
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
        .collect();
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
