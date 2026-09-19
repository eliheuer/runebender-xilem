// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Font outline contours → kurbo `BezPath`, shared by all Runebender editors.
//!
//! New document callers read canonical layers directly.
//! Norad entry points remain for compatibility callers and format adapters.

use kurbo::{Affine, BezPath, Point};
use norad::{Contour, ContourPoint, Font, Glyph, PointType};

use crate::document::{ContourView, LayerPointType, LayerShapeView, LayerView};

/// Why canonical component resolution could not produce an outline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentResolveError {
    /// A referenced glyph has no layer in the caller's resolution context.
    Missing(String),
    /// A component graph refers back to a glyph already being resolved.
    Cycle(Vec<String>),
    /// A component graph exceeded the defensive recursion limit.
    TooDeep,
    /// A component transform produced a coordinate outside the finite range.
    NonFinite,
}

impl std::fmt::Display for ComponentResolveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(name) => write!(formatter, "missing component base {name}"),
            Self::Cycle(names) => write!(formatter, "component cycle: {}", names.join(" -> ")),
            Self::TooDeep => formatter.write_str("component graph exceeds 64 layers"),
            Self::NonFinite => {
                formatter.write_str("component transform produced nonfinite geometry")
            }
        }
    }
}

impl std::error::Error for ComponentResolveError {}

/// Round a design-space value to whole units.
///
/// Coordinates in a font, and the distances between them, are a few
/// thousand units, so nothing here can leave `i64`. NaN rounds to
/// zero rather than to an arbitrary integer.
pub fn round_units(value: f64) -> i64 {
    if value.is_nan() {
        return 0;
    }
    let clamped = value.round().clamp(i64::MIN as f64, i64::MAX as f64);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "clamped to the range of i64 on the line above"
    )]
    {
        clamped as i64
    }
}

/// The integer key a point is found by when a rebuilt contour looks
/// up the flags of the point it came from.
pub fn point_key(x: f64, y: f64) -> (i64, i64) {
    (round_units(x), round_units(y))
}

/// Build a `BezPath` for a glyph, with components resolved
/// recursively through `font`.
///
/// Component transforms apply in full. A missing base glyph
/// contributes nothing.
pub fn glyph_to_bezpath(glyph: &Glyph, font: &Font) -> BezPath {
    let mut path = BezPath::new();
    for contour in &glyph.contours {
        append_contour(&mut path, contour);
    }
    if let Ok(preview) = super::metaballs::glyph_preview(glyph) {
        path.extend(preview);
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

/// Convert one canonical document layer's contours without constructing a UFO glyph.
///
/// Hyperbezier contours use their spline solver; ordinary cubic and quadratic contours use their
/// point types directly.
pub fn ordinary_layer_contours_to_bezpath(layer: LayerView<'_>) -> BezPath {
    let mut path = BezPath::new();
    for contour in layer.contours() {
        path.extend(ordinary_contour_to_bezpath(contour));
    }
    path
}

/// Convert one canonical contour without constructing a UFO contour.
pub fn ordinary_contour_to_bezpath(contour: ContourView<'_>) -> BezPath {
    let mut path = BezPath::new();
    append_document_contour(&mut path, contour);
    path
}

/// Convert an ordinary canonical layer with recursively resolved components.
///
/// `resolve` selects the layer used for each component base in the caller's source context.
/// Missing references and cycles are errors rather than silently omitted outlines.
/// Hyperbezier contours use the canonical spline conversion; smart components and metaballs remain
/// on their dedicated paths until their typed document metadata is available here.
pub fn ordinary_layer_to_bezpath<'a>(
    layer: LayerView<'a>,
    mut resolve: impl FnMut(&str) -> Option<LayerView<'a>>,
) -> Result<BezPath, ComponentResolveError> {
    let mut path = BezPath::new();
    let mut stack = vec![layer.glyph_name().to_owned()];
    append_document_shapes(&mut path, layer, &mut resolve, &mut stack, Affine::IDENTITY)?;
    Ok(path)
}

/// Resolve one top-level canonical component into its exact rendered path.
///
/// `root_name` keeps cycles through the containing glyph visible even though the returned path is
/// scoped to one component.
pub fn ordinary_component_to_bezpath<'a>(
    root_name: &str,
    component: crate::document::ComponentView<'a>,
    mut resolve: impl FnMut(&str) -> Option<LayerView<'a>>,
) -> Result<BezPath, ComponentResolveError> {
    let name = component.reference();
    if name == root_name {
        return Err(ComponentResolveError::Cycle(vec![
            root_name.to_owned(),
            root_name.to_owned(),
        ]));
    }
    let base = resolve(name).ok_or_else(|| ComponentResolveError::Missing(name.to_owned()))?;
    let mut path = BezPath::new();
    let mut stack = vec![root_name.to_owned(), name.to_owned()];
    append_document_shapes(
        &mut path,
        base,
        &mut resolve,
        &mut stack,
        component.transform(),
    )?;
    Ok(path)
}

/// One contour as a `BezPath`.
pub fn contour_to_bezpath(contour: &Contour) -> BezPath {
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

/// A sparse-layer pole: the axis tags it sits at the top of, and its
/// point coordinates in that layer.
type Pole = (std::collections::BTreeSet<String>, Vec<(f64, f64)>);

/// The affine of a norad component transform.
pub fn component_affine(t: &norad::AffineTransform) -> Affine {
    Affine::new([
        t.x_scale, t.xy_scale, t.yx_scale, t.y_scale, t.x_offset, t.y_offset,
    ])
}

/// Where smart-component metadata lives.
///
/// Axes sit on the part glyph under the glyphsLib key, and
/// per-component values sit on the using glyph. Pole layers are
/// marked with `com.runebender.partSelection`, where an axis value
/// of 1 means the bottom pole and 2 the top. An unmarked default
/// glyph acts as the bottom pole.
const SMART_AXES_KEY: &str = "com.schriftgestaltung.Glyphs.smartComponentAxes";
const SMART_VALUES_KEY: &str = "com.schriftgestaltung.Glyphs.componentsSmartComponentValues";
const PART_SELECTION_KEY: &str = "com.runebender.partSelection";

/// The value the using glyph sets for `component_index`'s first
/// smart axis, if any.
///
/// Values are stored as `{axis: value}` dicts in a list aligned
/// with the component order.
fn smart_value_for(glyph: &Glyph, component_index: usize, axis: &str) -> Option<f64> {
    glyph
        .lib
        .get(SMART_VALUES_KEY)?
        .as_array()?
        .get(component_index)?
        .as_dictionary()?
        .get(axis)
        .and_then(|v| {
            v.as_real()
                .or_else(|| v.as_signed_integer().map(|n| n as f64))
        })
}

/// Interpolated contours for a smart part at the given axis values.
///
/// Each smart axis has a bottom pole and a top pole. The bottom
/// pole is the default glyph, or a layer marked `{axis: 1}`. The
/// top pole is a layer marked `{axis: 2}`. A layer marked 2 on
/// several axes is a corner pole.
///
/// The blend is the standard corner-delta (variation) model: the
/// default plus, per pole layer, its inclusion-exclusion delta
/// scaled by the product of the normalized values on that layer's
/// top axes. For one axis that is plain linear interpolation; for
/// two axes with all corners, bilinear.
///
/// Only point-compatible layers take part. Anything else falls
/// back to the default outline.
fn smart_contours(
    base: &Glyph,
    font: &Font,
    values: &std::collections::BTreeMap<String, f64>,
) -> Option<Vec<Contour>> {
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;
    let axes = base.lib.get(SMART_AXES_KEY)?.as_array()?;
    let number = |v: &plist::Value| {
        v.as_real()
            .or_else(|| v.as_signed_integer().map(|n| n as f64))
    };
    // Normalized position per axis, in declaration order.
    let mut t: BTreeMap<String, f64> = BTreeMap::new();
    for axis in axes {
        let axis = axis.as_dictionary()?;
        let name = axis.get("name")?.as_string()?;
        let bottom = axis.get("bottomValue").and_then(number).unwrap_or(0.0);
        let top = axis.get("topValue").and_then(number).unwrap_or(100.0);
        if (top - bottom).abs() < 1e-9 {
            continue;
        }
        let value = values.get(name).copied().unwrap_or(bottom);
        t.insert(
            name.to_string(),
            ((value - bottom) / (top - bottom)).clamp(0.0, 1.0),
        );
    }
    if t.is_empty() {
        return None;
    }
    // Pole layers: every layer copy of this glyph whose part
    // selection marks at least one known axis with 2.
    let flat = |glyph: &Glyph| -> Option<Vec<(f64, f64)>> {
        if glyph.contours.len() != base.contours.len() {
            return None;
        }
        let mut coords = Vec::new();
        for (a, b) in base.contours.iter().zip(glyph.contours.iter()) {
            if a.points.len() != b.points.len() {
                return None;
            }
            for p in &b.points {
                coords.push((p.x, p.y));
            }
        }
        Some(coords)
    };
    let default_coords = flat(base)?;
    let mut poles: Vec<Pole> = Vec::new();
    for layer in font.layers.iter() {
        let Some(candidate) = layer.get_glyph(base.name()) else {
            continue;
        };
        let Some(plist::Value::Dictionary(sel)) = candidate.lib.get(PART_SELECTION_KEY) else {
            continue;
        };
        let tops: BTreeSet<String> = sel
            .iter()
            .filter(|(name, v)| t.contains_key(name.as_str()) && v.as_signed_integer() == Some(2))
            .map(|(name, _)| name.clone())
            .collect();
        if tops.is_empty() {
            continue;
        }
        let coords = flat(candidate)?;
        poles.push((tops, coords));
    }
    if poles.is_empty() {
        return None;
    }
    // Inclusion-exclusion deltas, singles before corners.
    poles.sort_by_key(|(tops, _)| tops.len());
    let n = default_coords.len();
    let mut deltas: Vec<Pole> = Vec::new();
    for (tops, coords) in &poles {
        let mut delta: Vec<(f64, f64)> = coords
            .iter()
            .zip(default_coords.iter())
            .map(|(c, d)| (c.0 - d.0, c.1 - d.1))
            .collect();
        for (prev_tops, prev_delta) in &deltas {
            if prev_tops.is_subset(tops) && prev_tops != tops {
                for i in 0..n {
                    delta[i].0 -= prev_delta[i].0;
                    delta[i].1 -= prev_delta[i].1;
                }
            }
        }
        deltas.push((tops.clone(), delta));
    }
    let mut coords = default_coords;
    for (tops, delta) in &deltas {
        let weight: f64 = tops.iter().map(|a| t[a]).product();
        if weight == 0.0 {
            continue;
        }
        for i in 0..n {
            coords[i].0 += weight * delta[i].0;
            coords[i].1 += weight * delta[i].1;
        }
    }
    // Reassemble along the default glyph's structure.
    let mut out = Vec::with_capacity(base.contours.len());
    let mut cursor = 0_usize;
    for contour in &base.contours {
        let points = contour
            .points
            .iter()
            .map(|p| {
                let (x, y) = coords[cursor];
                cursor += 1;
                ContourPoint::new(x, y, p.typ, p.smooth, None, None)
            })
            .collect();
        out.push(Contour::new(points, None));
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
            * Affine::new([
                t.x_scale, t.xy_scale, t.yx_scale, t.y_scale, t.x_offset, t.y_offset,
            ]);
        // A smart part with values interpolates between its poles.
        let smart = base
            .lib
            .get(SMART_AXES_KEY)
            .and_then(|v| v.as_array())
            .map(|axes| {
                axes.iter()
                    .filter_map(|axis| {
                        let name = axis.as_dictionary()?.get("name")?.as_string()?.to_string();
                        let value = smart_value_for(glyph, index, &name)?;
                        Some((name, value))
                    })
                    .collect::<std::collections::BTreeMap<_, _>>()
            })
            .filter(|values| !values.is_empty())
            .and_then(|values| smart_contours(base, font, &values));
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
        if let Ok(preview) = super::metaballs::glyph_preview(base) {
            path.extend(combined * preview);
        }
        append_components(path, base, font, combined, depth + 1);
    }
}

fn pt(p: &ContourPoint) -> Point {
    Point::new(p.x, p.y)
}

#[derive(Clone, Copy)]
struct OutlinePoint {
    position: Point,
    kind: LayerPointType,
}

fn append_document_contour(path: &mut BezPath, contour: ContourView<'_>) {
    if contour.is_hyper() {
        let contour = crate::outline::path::hyper_model::Contour {
            points: contour
                .points()
                .map(|point| crate::outline::path::hyper_model::ContourPoint {
                    x: point.position().x,
                    y: point.position().y,
                    smooth: point.is_smooth(),
                    point_type: match point.point_type() {
                        LayerPointType::Move | LayerPointType::Curve => {
                            crate::outline::path::hyper_model::PointType::Hyper
                        }
                        LayerPointType::Line => {
                            crate::outline::path::hyper_model::PointType::HyperCorner
                        }
                        LayerPointType::OffCurve => {
                            crate::outline::path::hyper_model::PointType::OffCurve
                        }
                        LayerPointType::QCurve => {
                            crate::outline::path::hyper_model::PointType::QCurve
                        }
                    },
                })
                .collect(),
        };
        crate::outline::path::Path::from_contour(&contour).append_to_bezpath(path);
        return;
    }
    let points: Vec<_> = contour
        .points()
        .map(|point| OutlinePoint {
            position: point.position(),
            kind: point.point_type(),
        })
        .collect();
    append_points(path, &points, contour.is_closed());
}

pub(crate) fn append_canonical_points(
    path: &mut BezPath,
    points: impl IntoIterator<Item = (Point, LayerPointType)>,
    closed: bool,
) {
    let points = points
        .into_iter()
        .map(|(position, kind)| OutlinePoint { position, kind })
        .collect::<Vec<_>>();
    append_points(path, &points, closed);
}

fn append_document_shapes<'a>(
    path: &mut BezPath,
    layer: LayerView<'a>,
    resolve: &mut impl FnMut(&str) -> Option<LayerView<'a>>,
    stack: &mut Vec<String>,
    transform: Affine,
) -> Result<(), ComponentResolveError> {
    if stack.len() > 64 {
        return Err(ComponentResolveError::TooDeep);
    }
    for shape in layer.shapes() {
        match shape {
            LayerShapeView::Contour(contour) => {
                let mut contour_path = BezPath::new();
                append_document_contour(&mut contour_path, contour);
                let transformed = transform * contour_path;
                if !transformed.elements().iter().all(path_element_is_finite) {
                    return Err(ComponentResolveError::NonFinite);
                }
                path.extend(transformed.elements().iter().copied());
            }
            LayerShapeView::Component(component) => {
                let name = component.reference();
                if let Some(start) = stack.iter().position(|entry| entry == name) {
                    let mut cycle = stack[start..].to_vec();
                    cycle.push(name.to_owned());
                    return Err(ComponentResolveError::Cycle(cycle));
                }
                let base =
                    resolve(name).ok_or_else(|| ComponentResolveError::Missing(name.to_owned()))?;
                let combined = transform * component.transform();
                if !combined.as_coeffs().iter().all(|value| value.is_finite()) {
                    return Err(ComponentResolveError::NonFinite);
                }
                stack.push(name.to_owned());
                let result = append_document_shapes(path, base, resolve, stack, combined);
                stack.pop();
                result?;
            }
        }
    }
    Ok(())
}

fn path_element_is_finite(element: &kurbo::PathEl) -> bool {
    let point_is_finite = |point: &Point| point.x.is_finite() && point.y.is_finite();
    match element {
        kurbo::PathEl::MoveTo(point) | kurbo::PathEl::LineTo(point) => point_is_finite(point),
        kurbo::PathEl::QuadTo(first, second) => point_is_finite(first) && point_is_finite(second),
        kurbo::PathEl::CurveTo(first, second, third) => {
            point_is_finite(first) && point_is_finite(second) && point_is_finite(third)
        }
        kurbo::PathEl::ClosePath => true,
    }
}

fn append_contour(path: &mut BezPath, contour: &Contour) {
    let points = &contour.points;
    if points.is_empty() {
        return;
    }
    // Hyperbezier contours carry only on-curve points; their curves
    // come from the spline solver, not the point list.
    if crate::outline::path::hyper_model::norad_contour_is_hyper(contour) {
        let ws = crate::outline::path::hyper_model::Contour::from_norad(contour);
        crate::outline::path::Path::from_contour(&ws).append_to_bezpath(path);
        return;
    }
    let points: Vec<_> = points
        .iter()
        .map(|point| OutlinePoint {
            position: pt(point),
            kind: match point.typ {
                PointType::Move => LayerPointType::Move,
                PointType::Line => LayerPointType::Line,
                PointType::OffCurve => LayerPointType::OffCurve,
                PointType::Curve => LayerPointType::Curve,
                PointType::QCurve => LayerPointType::QCurve,
            },
        })
        .collect();
    append_points(
        path,
        &points,
        points
            .first()
            .is_none_or(|point| point.kind != LayerPointType::Move),
    );
}

fn append_points(path: &mut BezPath, points: &[OutlinePoint], closed: bool) {
    if points.is_empty() {
        return;
    }
    let Some(start_idx) = points
        .iter()
        .position(|point| point.kind != LayerPointType::OffCurve)
    else {
        if closed {
            let start = points[points.len() - 1]
                .position
                .midpoint(points[0].position);
            path.move_to(start);
            for (index, point) in points.iter().enumerate() {
                let next = points[(index + 1) % points.len()].position;
                path.quad_to(point.position, point.position.midpoint(next));
            }
            path.close_path();
        }
        return;
    };
    let rotated: Vec<_> = points[start_idx..]
        .iter()
        .chain(points[..start_idx].iter())
        .copied()
        .collect();

    path.move_to(rotated[0].position);

    let mut off_curves: Vec<Point> = Vec::with_capacity(2);
    // For a closed contour the segment list wraps around to the start
    // point; for an open one it ends at the last point.
    let n = rotated.len();
    let idx_range: Vec<usize> = if closed {
        (1..=n).map(|i| i % n).collect()
    } else {
        (1..n).collect()
    };
    for i in idx_range {
        let p = rotated[i];
        match p.kind {
            LayerPointType::OffCurve => off_curves.push(p.position),
            LayerPointType::Line | LayerPointType::Move => {
                off_curves.clear();
                path.line_to(p.position);
            }
            LayerPointType::Curve => {
                match off_curves.len() {
                    2 => path.curve_to(off_curves[0], off_curves[1], p.position),
                    1 => path.quad_to(off_curves[0], p.position),
                    _ => path.line_to(p.position),
                }
                off_curves.clear();
            }
            LayerPointType::QCurve => {
                // Expand implied on-curves between consecutive quad
                // off-curves.
                let target = p.position;
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
                        if let Some(last) = off_curves.last() {
                            path.quad_to(*last, target);
                        }
                    }
                }
                off_curves.clear();
            }
        }
    }
    if closed {
        path.close_path();
    }
}

#[cfg(test)]
mod canonical_render_tests {
    use super::*;
    use crate::document::project::Project;
    use crate::document::source::Master;
    use crate::document::variable::{LayerId, SourceId};
    use kurbo::Shape;
    use norad::{AffineTransform, Component, Name};

    fn rectangle(name: &str, x1: f64, y1: f64) -> Glyph {
        let mut glyph = Glyph::new(name);
        glyph.contours.push(Contour::new(
            vec![
                ContourPoint::new(0.0, 0.0, PointType::Line, false, None, None),
                ContourPoint::new(x1, 0.0, PointType::Line, false, None, None),
                ContourPoint::new(x1, y1, PointType::Line, false, None, None),
                ContourPoint::new(0.0, y1, PointType::Line, false, None, None),
            ],
            None,
        ));
        glyph
    }

    fn component(base: &str, transform: AffineTransform) -> Component {
        Component::new(Name::new(base).unwrap(), transform, None)
    }

    fn canonical_path(project: &Project, glyph: &str, layer: &LayerId) -> BezPath {
        ordinary_layer_to_bezpath(project.document_layer(glyph, layer).unwrap(), |name| {
            project.document_layer(name, layer)
        })
        .unwrap()
    }

    #[test]
    fn canonical_hyperbezier_matches_the_ufo_boundary() {
        let mut glyph = Glyph::new("hyper");
        glyph.contours.push(Contour::new(
            vec![
                ContourPoint::new(0.0, 0.0, PointType::Curve, true, None, None),
                ContourPoint::new(100.0, 200.0, PointType::Curve, true, None, None),
                ContourPoint::new(200.0, 0.0, PointType::Line, false, None, None),
            ],
            Some(norad::Identifier::new("hyper-canonical-parity").unwrap()),
        ));
        let mut font = Font::default();
        font.default_layer_mut().insert_glyph(glyph.clone());
        let project = Project::from_source(Master::from_font(font, "Hyper.ufo".into()));
        let id = project
            .document_source(SourceId(0))
            .expect("default source")
            .default_layer();
        let canonical = project.document_layer("hyper", &id).expect("hyper layer");

        assert_eq!(
            ordinary_layer_contours_to_bezpath(canonical),
            contours_to_bezpath(&glyph)
        );
    }

    #[test]
    fn canonical_nested_full_affines_match_the_ufo_boundary() {
        let base = rectangle("base", 120.25, 70.75);
        let mut middle = rectangle("middle", 33.0, 44.0);
        middle.components.push(component(
            "base",
            AffineTransform {
                x_scale: 1.25,
                xy_scale: 0.375,
                yx_scale: -0.625,
                y_scale: 0.875,
                x_offset: 17.5,
                y_offset: -23.25,
            },
        ));
        let mut top = Glyph::new("top");
        top.components.push(component(
            "middle",
            AffineTransform {
                x_scale: -0.75,
                xy_scale: 0.2,
                yx_scale: 0.45,
                y_scale: 1.5,
                x_offset: 411.125,
                y_offset: 92.625,
            },
        ));
        let mut font = Font::default();
        for glyph in [base, middle, top] {
            font.default_layer_mut().insert_glyph(glyph);
        }
        let expected = glyph_to_bezpath(font.get_glyph("top").unwrap(), &font);
        let project = Project::from_source(Master::from_font(font, "Affine.ufo".into()));
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        let actual = canonical_path(&project, "top", &layer);

        assert_eq!(actual, expected);
        assert_eq!(actual.bounding_box(), expected.bounding_box());
    }

    #[test]
    fn selected_auxiliary_layer_falls_back_to_the_source_default_for_components() {
        let mut font = Font::default();
        font.default_layer_mut()
            .insert_glyph(rectangle("base", 80.0, 50.0));
        let proposal = font
            .layers
            .new_layer("com.runebender.proposal.test")
            .unwrap();
        let mut top = Glyph::new("top");
        top.components.push(component(
            "base",
            AffineTransform {
                x_scale: 0.8,
                xy_scale: 0.25,
                yx_scale: -0.1,
                y_scale: 1.2,
                x_offset: 123.0,
                y_offset: 45.0,
            },
        ));
        proposal.insert_glyph(top.clone());
        let expected = glyph_to_bezpath(&top, &font);
        let project = Project::from_source(Master::from_font(font, "Overlay.ufo".into()));
        let default = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        let selected = LayerId {
            source: SourceId(0),
            name: "com.runebender.proposal.test".into(),
        };
        let actual =
            ordinary_layer_to_bezpath(project.document_layer("top", &selected).unwrap(), |name| {
                project
                    .document_layer(name, &selected)
                    .or_else(|| project.document_layer(name, &default))
            })
            .unwrap();

        assert_eq!(actual, expected);
        assert_eq!(actual.bounding_box(), expected.bounding_box());
    }

    #[test]
    fn canonical_component_render_rejects_nonfinite_transforms() {
        let mut top = Glyph::new("top");
        top.components.push(component(
            "base",
            AffineTransform {
                x_scale: f64::INFINITY,
                ..AffineTransform::default()
            },
        ));
        let mut font = Font::default();
        font.default_layer_mut()
            .insert_glyph(rectangle("base", 80.0, 50.0));
        font.default_layer_mut().insert_glyph(top);
        let project = Project::from_source(Master::from_font(font, "NonFinite.ufo".into()));
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();

        assert_eq!(
            ordinary_layer_to_bezpath(project.document_layer("top", &layer).unwrap(), |name| {
                project.document_layer(name, &layer)
            }),
            Err(ComponentResolveError::NonFinite)
        );
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
        let mut font = Font::default();
        // The part: narrow default (bottom pole), wide top-pole layer.
        let mut part = Glyph::new("_part.bar");
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
        let mut wide = Glyph::new("_part.bar");
        wide.contours = vec![square(500.0)];
        let mut pole = plist::Dictionary::new();
        pole.insert("Width".into(), plist::Value::Integer(2_u64.into()));
        wide.lib.insert(
            "com.runebender.partSelection".into(),
            plist::Value::Dictionary(pole),
        );
        font.layers
            .get_or_create_layer("part.top")
            .unwrap()
            .insert_glyph(wide);
        // The user glyph places the part at Width 50.
        let mut user = Glyph::new("smartdemo");
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
        assert!(
            (bbox.x1 - 350.0).abs() < 1.0,
            "interpolated width: {}",
            bbox.x1
        );
        // No value -> the plain narrow base.
        let mut plain = user.clone();
        plain
            .lib
            .remove("com.schriftgestaltung.Glyphs.componentsSmartComponentValues");
        let plain_path = components_to_bezpath(&plain, &font);
        assert!((plain_path.bounding_box().x1 - 200.0).abs() < 1.0);
    }

    #[test]
    fn two_axis_smart_component_blends_bilinearly() {
        use norad::{Contour, ContourPoint, PointType};
        let rect = |w: f64, h: f64| {
            Contour::new(
                [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]
                    .iter()
                    .map(|&(x, y)| ContourPoint::new(x, y, PointType::Line, false, None, None))
                    .collect(),
                None,
            )
        };
        let axis = |name: &str| {
            let mut d = plist::Dictionary::new();
            d.insert("name".into(), plist::Value::String(name.into()));
            d.insert("bottomValue".into(), plist::Value::Real(0.0));
            d.insert("topValue".into(), plist::Value::Real(100.0));
            plist::Value::Dictionary(d)
        };
        let pole = |tops: &[&str]| {
            let mut d = plist::Dictionary::new();
            for name in tops {
                d.insert((*name).into(), plist::Value::Integer(2_u64.into()));
            }
            plist::Value::Dictionary(d)
        };
        let mut font = Font::default();
        // Default 100x100; Width top 400x100; Height top 100x300;
        // corner 500x350 (more than additive, so the corner delta
        // is what proves bilinear).
        let mut part = Glyph::new("_part.box");
        part.contours = vec![rect(100.0, 100.0)];
        part.lib.insert(
            "com.schriftgestaltung.Glyphs.smartComponentAxes".into(),
            plist::Value::Array(vec![axis("Width"), axis("Height")]),
        );
        font.default_layer_mut().insert_glyph(part);
        for (layer, w, h, tops) in [
            ("box.w", 400.0, 100.0, vec!["Width"]),
            ("box.h", 100.0, 300.0, vec!["Height"]),
            ("box.wh", 500.0, 350.0, vec!["Width", "Height"]),
        ] {
            let mut g = Glyph::new("_part.box");
            g.contours = vec![rect(w, h)];
            g.lib
                .insert("com.runebender.partSelection".into(), pole(&tops));
            font.layers
                .get_or_create_layer(layer)
                .unwrap()
                .insert_glyph(g);
        }
        let mut user = Glyph::new("boxdemo");
        user.components.push(norad::Component::new(
            norad::Name::new("_part.box").unwrap(),
            norad::AffineTransform::default(),
            None,
        ));
        let mut values = plist::Dictionary::new();
        values.insert("Width".into(), plist::Value::Real(50.0));
        values.insert("Height".into(), plist::Value::Real(50.0));
        user.lib.insert(
            "com.schriftgestaltung.Glyphs.componentsSmartComponentValues".into(),
            plist::Value::Array(vec![plist::Value::Dictionary(values)]),
        );
        font.default_layer_mut().insert_glyph(user.clone());
        use kurbo::Shape as _;
        let bbox = components_to_bezpath(&user, &font).bounding_box();
        // Bilinear at (.5,.5): w = 100 + .5*300 + .5*0 + .25*(500-400-100+100)
        //                        = 100 + 150 + 25 = 275
        //                      h = 100 + 0 + .5*200 + .25*(350-100-300+100)
        //                        = 100 + 100 + 12.5 = 212.5
        assert!((bbox.x1 - 275.0).abs() < 0.5, "w: {}", bbox.x1);
        assert!((bbox.y1 - 212.5).abs() < 0.5, "h: {}", bbox.y1);
    }
}
