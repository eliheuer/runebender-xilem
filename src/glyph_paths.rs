// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Norad glyph contours → kurbo `BezPath`, shared by all Runebender
//! editors. Components resolve recursively through the font.

use kurbo::{Affine, BezPath, Point};
use norad::{Contour, ContourPoint, Font, Glyph, PointType};

pub fn glyph_to_bezpath(glyph: &Glyph, font: &Font) -> BezPath {
    let mut path = BezPath::new();
    for contour in &glyph.contours {
        append_contour(&mut path, contour);
    }
    append_components(&mut path, glyph, font, Affine::IDENTITY, 0);
    path
}

/// Only the glyph's own contours (no components).
pub fn contours_to_bezpath(glyph: &Glyph) -> BezPath {
    let mut path = BezPath::new();
    for contour in &glyph.contours {
        append_contour(&mut path, contour);
    }
    path
}

/// One contour as a BezPath.
pub fn contour_to_bezpath(contour: &norad::Contour) -> BezPath {
    let mut path = BezPath::new();
    append_contour(&mut path, contour);
    path
}

/// Only the glyph's components, recursively resolved.
pub fn components_to_bezpath(glyph: &Glyph, font: &Font) -> BezPath {
    let mut path = BezPath::new();
    append_components(&mut path, glyph, font, Affine::IDENTITY, 0);
    path
}

/// The affine of a norad component transform.
pub fn component_affine(t: &norad::AffineTransform) -> Affine {
    Affine::new([t.x_scale, t.xy_scale, t.yx_scale, t.y_scale, t.x_offset, t.y_offset])
}

/// Smart-component metadata: axes live on the part glyph under the
/// glyphsLib key, per-component values on the using glyph, and pole
/// layers are marked with `com.runebender.partSelection`
/// ({axis: 1 bottom, 2 top}); an unmarked default glyph acts as the
/// bottom pole.
const SMART_AXES_KEY: &str = "com.schriftgestaltung.Glyphs.smartComponentAxes";
const SMART_VALUES_KEY: &str =
    "com.schriftgestaltung.Glyphs.componentsSmartComponentValues";
const PART_SELECTION_KEY: &str = "com.runebender.partSelection";

/// The value the using glyph sets for `component_index`'s first
/// smart axis, if any: {axis: value} in a list aligned with the
/// component order.
fn smart_value_for(glyph: &Glyph, component_index: usize, axis: &str) -> Option<f64> {
    glyph
        .lib
        .get(SMART_VALUES_KEY)?
        .as_array()?
        .get(component_index)?
        .as_dictionary()?
        .get(axis)
        .and_then(|v| v.as_real().or_else(|| v.as_signed_integer().map(|n| n as f64)))
}

/// Interpolated contours of a smart part at `value` along its first
/// axis. The bottom pole is the default glyph (or a layer marked
/// {axis: 1}), the top a layer marked {axis: 2}. Point-compatible
/// poles interpolate; anything else falls back to the base.
fn smart_contours(base: &Glyph, font: &Font, value: f64) -> Option<Vec<norad::Contour>> {
    let axes = base.lib.get(SMART_AXES_KEY)?.as_array()?;
    let axis = axes.first()?.as_dictionary()?;
    let name = axis.get("name")?.as_string()?;
    let number = |v: &plist::Value| {
        v.as_real().or_else(|| v.as_signed_integer().map(|n| n as f64))
    };
    let bottom = axis.get("bottomValue").and_then(number).unwrap_or(0.0);
    let top = axis.get("topValue").and_then(number).unwrap_or(100.0);
    if (top - bottom).abs() < 1e-9 {
        return None;
    }
    let t = ((value - bottom) / (top - bottom)).clamp(0.0, 1.0);
    // Find the top pole: a layer copy marked {axis: 2}.
    let pole_of = |glyph: &Glyph| -> Option<i64> {
        glyph
            .lib
            .get(PART_SELECTION_KEY)?
            .as_dictionary()?
            .get(name)
            .and_then(|v| v.as_signed_integer())
    };
    let mut top_glyph: Option<&Glyph> = None;
    for layer in font.layers.iter() {
        let Some(candidate) = layer.get_glyph(base.name()) else {
            continue;
        };
        if pole_of(candidate) == Some(2) {
            top_glyph = Some(candidate);
            break;
        }
    }
    let top_glyph = top_glyph?;
    if top_glyph.contours.len() != base.contours.len() {
        return None;
    }
    let mut out = Vec::with_capacity(base.contours.len());
    for (a, b) in base.contours.iter().zip(top_glyph.contours.iter()) {
        if a.points.len() != b.points.len() {
            return None;
        }
        let points = a
            .points
            .iter()
            .zip(b.points.iter())
            .map(|(pa, pb)| {
                norad::ContourPoint::new(
                    pa.x + (pb.x - pa.x) * t,
                    pa.y + (pb.y - pa.y) * t,
                    pa.typ.clone(),
                    pa.smooth,
                    None,
                    None,
                )
            })
            .collect();
        out.push(norad::Contour::new(points, None));
    }
    Some(out)
}

fn append_components(
    path: &mut BezPath,
    glyph: &Glyph,
    font: &Font,
    parent_transform: Affine,
    depth: u8,
) {
    // Guard against reference cycles in malformed UFOs.
    if depth > 8 {
        return;
    }
    for (index, component) in glyph.components.iter().enumerate() {
        let Some(base) = font.get_glyph(&component.base) else {
            continue;
        };
        let t = component.transform;
        let combined = parent_transform
            * Affine::new([t.x_scale, t.xy_scale, t.yx_scale, t.y_scale, t.x_offset, t.y_offset]);
        // A smart part with a value interpolates between its poles.
        let smart = base
            .lib
            .get(SMART_AXES_KEY)
            .and_then(|v| v.as_array())
            .and_then(|axes| axes.first()?.as_dictionary()?.get("name")?.as_string().map(str::to_string))
            .and_then(|axis| smart_value_for(glyph, index, &axis))
            .and_then(|value| smart_contours(base, font, value));
        match smart {
            Some(contours) => {
                for contour in &contours {
                    let mut contour_path = BezPath::new();
                    append_contour(&mut contour_path, contour);
                    path.extend((combined * &contour_path).elements().iter().cloned());
                }
            }
            None => {
                for contour in &base.contours {
                    let mut contour_path = BezPath::new();
                    append_contour(&mut contour_path, contour);
                    path.extend((combined * &contour_path).elements().iter().cloned());
                }
            }
        }
        append_components(path, base, font, combined, depth + 1);
    }
}

fn pt(p: &ContourPoint) -> Point {
    Point::new(p.x, p.y)
}

fn is_on_curve(p: &ContourPoint) -> bool {
    matches!(
        p.typ,
        PointType::Move | PointType::Line | PointType::Curve | PointType::QCurve
    )
}

fn append_contour(path: &mut BezPath, contour: &Contour) {
    let points = &contour.points;
    if points.is_empty() {
        return;
    }
    // Hyperbezier contours carry only on-curve points; their curves
    // come from the spline solver, not the point list.
    if crate::model::workspace::norad_contour_is_hyper(contour) {
        let ws = crate::model::workspace::Contour::from_norad(contour);
        crate::path::Path::from_contour(&ws).append_to_bezpath(path);
        return;
    }
    let Some(start_idx) = points.iter().position(is_on_curve) else {
        // All-off-curve (TrueType implied on-curve) contour: skip for now.
        return;
    };
    let open = points[0].typ == PointType::Move;
    let rotated: Vec<&ContourPoint> = points[start_idx..]
        .iter()
        .chain(points[..start_idx].iter())
        .collect();

    path.move_to(pt(rotated[0]));

    let mut off_curves: Vec<Point> = Vec::with_capacity(2);
    // For a closed contour the segment list wraps around to the start
    // point; for an open one it ends at the last point.
    let n = rotated.len();
    let idx_range: Vec<usize> = if open {
        (1..n).collect()
    } else {
        (1..=n).map(|i| i % n).collect()
    };
    for i in idx_range {
        let p = rotated[i];
        match p.typ {
            PointType::OffCurve => off_curves.push(pt(p)),
            PointType::Line | PointType::Move => {
                off_curves.clear();
                path.line_to(pt(p));
            }
            PointType::Curve => {
                match off_curves.len() {
                    2 => path.curve_to(off_curves[0], off_curves[1], pt(p)),
                    1 => path.quad_to(off_curves[0], pt(p)),
                    _ => path.line_to(pt(p)),
                }
                off_curves.clear();
            }
            PointType::QCurve => {
                // Expand implied on-curves between consecutive quad
                // off-curves.
                let target = pt(p);
                match off_curves.len() {
                    0 => path.line_to(target),
                    1 => path.quad_to(off_curves[0], target),
                    _ => {
                        for w in 0..off_curves.len() - 1 {
                            let a = off_curves[w];
                            let b = off_curves[w + 1];
                            let mid = a.midpoint(b);
                            path.quad_to(a, mid);
                        }
                        path.quad_to(*off_curves.last().unwrap(), target);
                    }
                }
                off_curves.clear();
            }
        }
    }
    if !open {
        path.close_path();
    }
}

#[cfg(test)]
mod smart_component_tests {
    use super::*;

    #[test]
    fn smart_component_interpolates_between_poles() {
        use norad::{Contour, ContourPoint, PointType};
        let square = |x1: f64| {
            Contour::new(
                [(100.0, 0.0), (x1, 0.0), (x1, 600.0), (100.0, 600.0)]
                    .iter()
                    .map(|&(x, y)| ContourPoint::new(x, y, PointType::Line, false, None, None))
                    .collect(),
                None,
            )
        };
        let mut font = norad::Font::default();
        // The part: narrow default (bottom pole), wide top-pole layer.
        let mut part = norad::Glyph::new("_part.bar");
        part.contours = vec![square(200.0)];
        let mut axes = plist::Dictionary::new();
        axes.insert("name".into(), plist::Value::String("Width".into()));
        axes.insert("bottomValue".into(), plist::Value::Real(0.0));
        axes.insert("topValue".into(), plist::Value::Real(100.0));
        part.lib.insert(
            "com.schriftgestaltung.Glyphs.smartComponentAxes".into(),
            plist::Value::Array(vec![plist::Value::Dictionary(axes)]),
        );
        font.default_layer_mut().insert_glyph(part);
        let mut wide = norad::Glyph::new("_part.bar");
        wide.contours = vec![square(500.0)];
        let mut pole = plist::Dictionary::new();
        pole.insert("Width".into(), plist::Value::Integer(2u64.into()));
        wide.lib.insert(
            "com.runebender.partSelection".into(),
            plist::Value::Dictionary(pole),
        );
        font.layers
            .get_or_create_layer("part.top")
            .unwrap()
            .insert_glyph(wide);
        // The user glyph places the part at Width 50.
        let mut user = norad::Glyph::new("smartdemo");
        user.components.push(norad::Component::new(
            norad::Name::new("_part.bar").unwrap(),
            norad::AffineTransform::default(),
            None,
        ));
        let mut values = plist::Dictionary::new();
        values.insert("Width".into(), plist::Value::Real(50.0));
        user.lib.insert(
            "com.schriftgestaltung.Glyphs.componentsSmartComponentValues".into(),
            plist::Value::Array(vec![plist::Value::Dictionary(values)]),
        );
        font.default_layer_mut().insert_glyph(user.clone());
        use kurbo::Shape as _;
        let path = components_to_bezpath(&user, &font);
        let bbox = path.bounding_box();
        // Halfway between 200 and 500.
        assert!((bbox.x1 - 350.0).abs() < 1.0, "interpolated width: {}", bbox.x1);
        // No value -> the plain narrow base.
        let mut plain = user.clone();
        plain.lib.remove("com.schriftgestaltung.Glyphs.componentsSmartComponentValues");
        let plain_path = components_to_bezpath(&plain, &font);
        assert!((plain_path.bounding_box().x1 - 200.0).abs() < 1.0);
    }
}

