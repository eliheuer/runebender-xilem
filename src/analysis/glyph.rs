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
        "components": glyph.components.iter().map(|c| c.base.to_string()).collect::<Vec<_>>(),
        "component_transforms": glyph.components.iter().map(|c| json!({"base": c.base, "transform": [c.transform.x_scale, c.transform.xy_scale, c.transform.yx_scale, c.transform.y_scale, c.transform.x_offset, c.transform.y_offset]})).collect::<Vec<_>>(),
        "anchors": glyph.anchors.iter().map(|a| json!({ "name": a.name.as_ref().map(|n| n.to_string()), "x": a.x, "y": a.y })).collect::<Vec<_>>(),
    })
}
