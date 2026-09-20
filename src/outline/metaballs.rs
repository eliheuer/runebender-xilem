// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Samples compact metaball fields and explicitly converts their boundaries to cubic contours.

use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::HashSet;

use kurbo::{BezPath, Point, Vec2};

use crate::formats::metaballs::{MetaballGroup, Metaballs};
#[cfg(test)]
use crate::formats::metaballs::{read_metaballs, write_metaballs};

/// Sampling and curve fitting settings, both in font units.
#[derive(Clone, Copy, Debug)]
pub struct OutlineOptions {
    /// Maximum grid spacing. Features smaller than this can be missed.
    pub resolution: f64,
    /// Cubic fitting accuracy relative to the sampled boundary, not the analytic field.
    pub accuracy: f64,
}

impl Default for OutlineOptions {
    fn default() -> Self {
        Self {
            resolution: 2.0,
            accuracy: 0.25,
        }
    }
}

/// Evaluates `sum(stiffness * max(0, 1 - distance²/radius²)³)`.
/// Parameters must have passed [`Metaballs::validate`]. Does not mutate source data.
pub fn field(group: &MetaballGroup, point: Point) -> f64 {
    group
        .balls
        .iter()
        .map(|b| {
            let d = point - Point::new(b.x, b.y);
            let q = (1.0 - d.hypot2() / b.radius.powi(2)).max(0.0);
            b.stiffness * q.powi(3)
        })
        .sum()
}

fn tangent(group: &MetaballGroup, point: Point) -> Vec2 {
    let mut gradient = Vec2::ZERO;
    for b in &group.balls {
        let d = point - Point::new(b.x, b.y);
        let q = (1.0 - d.hypot2() / b.radius.powi(2)).max(0.0);
        gradient += d * (-6.0 * b.stiffness * q.powi(2) / b.radius.powi(2));
    }
    Vec2::new(gradient.y, -gradient.x)
}

mod fitting;

type Edge = (usize, usize);

/// Generates closed cubic preview paths for one group without editing its source.
/// Uses an oriented triangular grid (including holes) and analytic field tangents.
/// Rejects nonfinite settings, invalid source data, and grids over one million cells.
/// Sampling can omit sub-grid features; decrease `resolution` near a merge or split.
pub fn preview(group: &MetaballGroup, options: OutlineOptions) -> Result<Vec<BezPath>, String> {
    sample_outline(group, options, false)
}

/// Fits editable cubic contours through img2bez with required extrema and economical cubic spans.
///
/// Tangents at extrema are exactly horizontal or vertical.
/// Sampling and validation use the same bounded grid as [`preview`].
/// Accuracy bounds fitting against the sampled boundary, not the analytic field.
/// Returns an error when a structural feature cannot be resolved safely.
pub fn cubic_outline(
    group: &MetaballGroup,
    options: OutlineOptions,
) -> Result<Vec<BezPath>, String> {
    sample_outline(group, options, true)
}

fn sample_outline(
    group: &MetaballGroup,
    options: OutlineOptions,
    structured: bool,
) -> Result<Vec<BezPath>, String> {
    Metaballs {
        version: 1,
        groups: vec![group.clone()],
    }
    .validate()?;
    if !options.resolution.is_finite()
        || options.resolution < 0.01
        || !options.accuracy.is_finite()
        || !(0.001..=100.0).contains(&options.accuracy)
    {
        return Err("invalid metaball resolution or fitting accuracy".into());
    }
    let positive: Vec<_> = group.balls.iter().filter(|b| b.stiffness > 0.0).collect();
    if positive.is_empty() {
        return Ok(Vec::new());
    }
    let step = options.resolution;
    let x0 = positive
        .iter()
        .map(|b| b.x - b.radius)
        .fold(f64::INFINITY, f64::min)
        - step;
    let y0 = positive
        .iter()
        .map(|b| b.y - b.radius)
        .fold(f64::INFINITY, f64::min)
        - step;
    let x1 = positive
        .iter()
        .map(|b| b.x + b.radius)
        .fold(f64::NEG_INFINITY, f64::max)
        + step;
    let y1 = positive
        .iter()
        .map(|b| b.y + b.radius)
        .fold(f64::NEG_INFINITY, f64::max)
        + step;
    let nx = ((x1 - x0) / step).ceil();
    let ny = ((y1 - y0) / step).ceil();
    if nx * ny > 1_000_000.0 {
        return Err("metaball sampling grid exceeds one million cells; increase resolution spacing or split the group".into());
    }
    // The checked cell budget bounds both dimensions and their integer conversions.
    #[allow(clippy::cast_possible_truncation, reason = "bounded grid dimensions")]
    let (nx, ny) = (nx as usize, ny as usize);
    let stride = nx + 1;
    let point = |i: usize| {
        Point::new(
            x0 + (i % stride) as f64 * step,
            y0 + (i / stride) as f64 * step,
        )
    };
    let values: Vec<_> = (0..stride * (ny + 1))
        .map(|i| field(group, point(i)) - group.threshold)
        .collect();
    let mut links = BTreeMap::<Edge, Edge>::new();
    let mut crossings = BTreeMap::<Edge, Point>::new();
    for y in 0..ny {
        for x in 0..nx {
            let a = y * stride + x;
            for triangle in [[a, a + 1, a + stride + 1], [a, a + stride + 1, a + stride]] {
                let mut enter = None;
                let mut leave = None;
                for k in 0..3 {
                    let i = triangle[k];
                    let j = triangle[(k + 1) % 3];
                    if (values[i] >= 0.0) == (values[j] >= 0.0) {
                        continue;
                    }
                    let edge = (i.min(j), i.max(j));
                    crossings.entry(edge).or_insert_with(|| {
                        let mut lo = point(edge.0);
                        let mut hi = point(edge.1);
                        let inside = values[edge.0] >= 0.0;
                        for _ in 0..24 {
                            let mid = lo.lerp(hi, 0.5);
                            if (field(group, mid) >= group.threshold) == inside {
                                lo = mid;
                            } else {
                                hi = mid;
                            }
                        }
                        lo.lerp(hi, 0.5)
                    });
                    if values[i] >= 0.0 {
                        leave = Some(edge);
                    } else {
                        enter = Some(edge);
                    }
                }
                if let (Some(a), Some(b)) = (leave, enter) {
                    links.insert(a, b);
                }
            }
        }
    }
    let mut paths = Vec::new();
    while let Some((&start, _)) = links.first_key_value() {
        let mut current = start;
        let mut points = Vec::new();
        loop {
            points.push(crossings[&current]);
            current = links
                .remove(&current)
                .ok_or("metaball boundary is not closed")?;
            if current == start {
                break;
            }
        }
        points.dedup_by(|a, b| a.distance_squared(*b) < 1e-16);
        if points.len() < 3 {
            continue;
        }
        let fitted = if structured {
            fitting::fit(group, &points, options.accuracy)?
        } else {
            let mut path = BezPath::new();
            path.move_to(points[0]);
            for i in 0..points.len() {
                let a = points[i];
                let b = points[(i + 1) % points.len()];
                let chord = b - a;
                let control = |p| {
                    let t = tangent(group, p);
                    if t.hypot() > 1e-12 {
                        t.normalize() * (chord.hypot() / 3.0)
                    } else {
                        chord / 3.0
                    }
                };
                path.curve_to(a + control(a), b - control(b), b);
            }
            path.close_path();
            kurbo::simplify::simplify_bezpath(
                path,
                options.accuracy,
                &kurbo::simplify::SimplifyOptions::default(),
            )
        };
        // Kurbo can leave a floating-point-sized closing line. Snap that seam
        // instead of turning it into an extra, effectively coincident UFO node.
        let mut segments: Vec<_> = fitted
            .segments()
            .map(|s| s.to_cubic())
            .filter(|c| {
                c.p0.distance_squared(c.p1)
                    .max(c.p0.distance_squared(c.p2))
                    .max(c.p0.distance_squared(c.p3))
                    > 1e-16
            })
            .collect();
        if let Some(start) = segments.first().map(|c| c.p0)
            && let Some(last) = segments.last_mut()
        {
            last.p2 += start - last.p3;
            last.p3 = start;
        }
        let mut cubic = BezPath::new();
        for c in segments {
            if cubic.is_empty() {
                cubic.move_to(c.p0);
            }
            cubic.curve_to(c.p1, c.p2, c.p3);
        }
        cubic.close_path();
        paths.push(cubic);
    }
    Ok(paths)
}

#[cfg(test)]
fn contours(paths: &[BezPath]) -> Vec<norad::Contour> {
    paths
        .iter()
        .map(|path| {
            let mut points = Vec::new();
            for segment in path.segments() {
                let c = segment.to_cubic();
                for (p, typ) in [
                    (c.p1, norad::PointType::OffCurve),
                    (c.p2, norad::PointType::OffCurve),
                    (c.p3, norad::PointType::Curve),
                ] {
                    let smooth = typ == norad::PointType::Curve;
                    points.push(norad::ContourPoint::new(p.x, p.y, typ, smooth, None, None));
                }
            }
            norad::Contour::new(points, None)
        })
        .collect()
}

/// Converts every metaball group in every layer of a font, returning groups converted.
/// All changes are prepared on a clone first; an error leaves the entire font untouched.
/// This does not write files. A live editor must supply a font-wide undo transaction.
#[cfg(test)]
pub fn collapse_font(font: &mut norad::Font, options: OutlineOptions) -> Result<usize, String> {
    let mut candidate = font.clone();
    let mut count = 0;
    for layer in candidate.layers.iter_mut() {
        for glyph in layer.iter_mut() {
            count +=
                collapse(glyph, None, options).map_err(|e| format!("{}: {e}", glyph.name()))?;
        }
    }
    if count != 0 {
        *font = candidate;
    }
    Ok(count)
}

/// Converts selected whole groups to cubic UFO contours and removes their live data.
/// `None` converts every group; an empty slice converts none. Returns groups converted.
/// All validation and geometry finish before mutation; any error preserves the glyph.
/// Empty sampled outlines are rejected to avoid silently discarding live sources.
/// Existing contours, anchors, components, lib keys and fractional coordinates are preserved.
/// The editor must wrap this operation in its normal undo transaction.
#[cfg(test)]
pub fn collapse(
    glyph: &mut norad::Glyph,
    groups: Option<&[u32]>,
    options: OutlineOptions,
) -> Result<usize, String> {
    let mut data = read_metaballs(glyph)?;
    let selected: HashSet<_> = groups
        .map(|ids| ids.iter().copied().collect())
        .unwrap_or_else(|| data.groups.iter().map(|g| g.id).collect());
    if selected
        .iter()
        .any(|id| !data.groups.iter().any(|g| g.id == *id))
    {
        return Err("unknown metaball group".into());
    }
    let mut generated = Vec::new();
    for group in data.groups.iter().filter(|g| selected.contains(&g.id)) {
        let paths = cubic_outline(group, options)?;
        if paths.is_empty() {
            return Err("metaball group has no sampled outline; source preserved".into());
        }
        generated.extend(contours(&paths));
    }
    data.groups.retain(|g| !selected.contains(&g.id));
    write_metaballs(glyph, &data)?;
    glyph.contours.extend(generated);
    Ok(selected.len())
}

/// Builds the live metaball preview without changing the glyph or adding contours.
/// Invalid source or a preview beyond the sampling budget returns an error.
#[cfg(test)]
pub fn glyph_preview(glyph: &norad::Glyph) -> Result<BezPath, String> {
    let source = read_metaballs(glyph)?;
    let mut path = BezPath::new();
    for group in &source.groups {
        for contour in preview(group, OutlineOptions::default())? {
            path.extend(contour);
        }
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::metaballs::{METABALLS_KEY, Metaball};
    use kurbo::{ParamCurve, Shape};

    fn ball(id: u32, x: f64) -> Metaball {
        Metaball {
            id,
            x,
            y: 0.0,
            radius: 100.0,
            stiffness: 2.0,
        }
    }

    fn group(balls: Vec<Metaball>) -> MetaballGroup {
        MetaballGroup {
            id: 1,
            threshold: 0.5,
            balls,
        }
    }

    #[test]
    fn isolated_circle_has_correct_radius_and_cubic_accuracy() {
        let g = group(vec![ball(1, 0.0)]);
        let paths = preview(&g, OutlineOptions::default()).unwrap();
        assert_eq!(paths.len(), 1);
        let radius = 100.0 * (1.0 - 0.25_f64.cbrt()).sqrt();
        assert!(
            paths[0].area() > 0.0,
            "outer contour must be counterclockwise"
        );
        assert!(
            paths[0].segments().count() < 40,
            "conversion should fit curves, not retain grid edges"
        );
        for segment in paths[0].segments() {
            assert!(matches!(segment, kurbo::PathSeg::Cubic(_)));
            assert!(
                segment.start().distance_squared(segment.end()) > 1e-8,
                "no coincident closing node"
            );
            for i in 0..=20 {
                let distance = segment.eval(f64::from(i) / 20.0).distance(Point::ZERO);
                assert!(
                    (distance - radius).abs() < 0.6,
                    "radial error {}",
                    distance - radius
                );
            }
        }
    }

    #[test]
    fn conversion_places_four_circle_nodes_at_exact_extrema() {
        // Fractional centers and several grid spacings must not rotate the node layout.
        for resolution in [1.0, 2.0, 3.0] {
            let mut b = ball(1, 123.125);
            b.y = -47.375;
            let center = Point::new(b.x, b.y);
            let radius = b.radius * (1.0 - 0.25_f64.cbrt()).sqrt();
            let paths = cubic_outline(
                &group(vec![b]),
                OutlineOptions {
                    resolution,
                    ..OutlineOptions::default()
                },
            )
            .unwrap();
            assert_eq!(paths.len(), 1);
            let segments: Vec<_> = paths[0].segments().map(|s| s.to_cubic()).collect();
            assert_eq!(segments.len(), 4, "one cubic per quadrant");
            for c in &segments {
                let delta = c.p0 - center;
                assert!(
                    delta.x.abs().min(delta.y.abs()) < 1e-8,
                    "node at an extremum"
                );
                assert!((delta.hypot() - radius).abs() < 1e-8, "node on the field");
                for handle in [c.p1 - c.p0, c.p3 - c.p2] {
                    assert!(
                        handle.x == 0.0 || handle.y == 0.0,
                        "exact axis handles: {handle:?}, curve {c:?}, spacing {resolution}"
                    );
                }
                for i in 0..=100 {
                    let error = (c.eval(f64::from(i) / 100.0).distance(center) - radius).abs();
                    assert!(error < 0.025, "circle radial error {error}");
                }
            }
        }
    }

    #[test]
    fn conversion_preserves_blends_holes_and_smooth_joins() {
        let mut diagonal = ball(2, 140.0);
        diagonal.y = 150.0;
        diagonal.radius = 140.0;
        let mut large = ball(1, 0.0);
        large.radius = 180.0;
        let mut negative = ball(2, 0.0);
        negative.radius = 40.0;
        negative.stiffness = -4.0;
        for g in [
            group(vec![ball(1, -45.0), ball(2, 45.0)]),
            group(vec![ball(1, -150.0), ball(2, 150.0)]),
            group(vec![large, diagonal]),
            group(vec![ball(1, 0.0), negative]),
        ] {
            let reference = preview(&g, OutlineOptions::default()).unwrap();
            let paths = cubic_outline(&g, OutlineOptions::default()).unwrap();
            assert_eq!(paths.len(), reference.len(), "preserve sampled topology");
            for (path, reference) in paths.iter().zip(reference.iter()) {
                assert_eq!(
                    path.area().signum(),
                    reference.area().signum(),
                    "preserve winding"
                );
                let segments: Vec<_> = path.segments().map(|s| s.to_cubic()).collect();
                assert!(segments.len() <= 20, "compact editable output");
                for (i, c) in segments.iter().enumerate() {
                    let next = segments[(i + 1) % segments.len()];
                    assert_eq!(c.p3, next.p0, "closed joins");
                    let incoming = (c.p3 - c.p2).normalize();
                    let outgoing = (next.p1 - next.p0).normalize();
                    assert!(incoming.dot(outgoing) > 1.0 - 1e-8, "smooth join");
                    for j in 0..=100 {
                        let point = c.eval(f64::from(j) / 100.0);
                        // First-order normal distance, a regression metric, not a Hausdorff bound.
                        let error =
                            (field(&g, point) - g.threshold).abs() / tangent(&g, point).hypot();
                        assert!(error < 0.25, "sampled normal discrepancy {error}");
                    }
                }
            }
        }
    }

    #[test]
    fn balls_merge_and_separate_and_negative_field_makes_a_hole() {
        let options = OutlineOptions::default();
        assert_eq!(
            preview(&group(vec![ball(1, -45.0), ball(2, 45.0)]), options)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            preview(&group(vec![ball(1, -150.0), ball(2, 150.0)]), options)
                .unwrap()
                .len(),
            2
        );
        let mut negative = ball(2, 0.0);
        negative.radius = 40.0;
        negative.stiffness = -4.0;
        let paths = preview(&group(vec![ball(1, 0.0), negative]), options).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(
            paths[0].area() * paths[1].area() < 0.0,
            "hole winding must oppose outer winding"
        );
    }

    #[test]
    fn source_survives_glif_save_and_selected_conversion_preserves_other_data() {
        let mut glyph = norad::Glyph::new("metaball-test");
        glyph.width = 500.0;
        glyph.lib.insert("example.keep".into(), "untouched".into());
        let first = group(vec![ball(7, 0.25)]);
        let mut second = first.clone();
        second.id = 2;
        second.balls[0].x = 350.25;
        let source = Metaballs {
            version: 1,
            groups: vec![first, second.clone()],
        };
        assert!(write_metaballs(&mut glyph, &source).unwrap());
        assert!(!write_metaballs(&mut glyph, &source).unwrap());
        assert!(glyph.contours.is_empty());
        let bytes = glyph.encode_xml().unwrap();
        let mut glyph = norad::Glyph::parse_raw(&bytes).unwrap();
        assert_eq!(read_metaballs(&glyph).unwrap(), source);
        assert_eq!(
            collapse(&mut glyph, Some(&[1]), OutlineOptions::default()).unwrap(),
            1
        );
        assert_eq!(glyph.contours.len(), 1);
        assert_eq!(read_metaballs(&glyph).unwrap().groups, vec![second]);
        assert_eq!(glyph.width, 500.0);
        assert_eq!(
            glyph
                .lib
                .get("example.keep")
                .and_then(plist::Value::as_string),
            Some("untouched")
        );
        let first_contour = glyph.contours[0].clone();
        assert_eq!(
            collapse(&mut glyph, None, OutlineOptions::default()).unwrap(),
            1
        );
        assert_eq!(glyph.contours[0], first_contour);
        assert!(!glyph.lib.contains_key(METABALLS_KEY));
        assert!(
            glyph
                .contours
                .iter()
                .flat_map(|c| &c.points)
                .all(|p| matches!(p.typ, norad::PointType::Curve | norad::PointType::OffCurve))
        );
        assert!(norad::Glyph::parse_raw(&glyph.encode_xml().unwrap()).is_ok());
    }

    #[test]
    fn failed_conversion_and_unknown_schema_leave_glyph_unchanged() {
        let mut glyph = norad::Glyph::new("invalid");
        let good = group(vec![ball(1, 0.0)]);
        let mut invisible = good.clone();
        invisible.id = 2;
        invisible.threshold = 10.0;
        write_metaballs(
            &mut glyph,
            &Metaballs {
                version: 1,
                groups: vec![good, invisible],
            },
        )
        .unwrap();
        let before = glyph.encode_xml().unwrap();
        assert!(collapse(&mut glyph, None, OutlineOptions::default()).is_err());
        assert_eq!(glyph.encode_xml().unwrap(), before);
        assert!(collapse(&mut glyph, Some(&[99]), OutlineOptions::default()).is_err());
        assert_eq!(glyph.encode_xml().unwrap(), before);
        let mut invalid = read_metaballs(&glyph).unwrap();
        invalid.groups[0].balls[0].x = f64::NAN;
        assert!(write_metaballs(&mut glyph, &invalid).is_err());
        assert_eq!(glyph.encode_xml().unwrap(), before);
        glyph.lib.insert(
            METABALLS_KEY.into(),
            plist::to_value(&Metaballs {
                version: 2,
                groups: vec![],
            })
            .unwrap(),
        );
        let before = glyph.encode_xml().unwrap();
        assert!(collapse(&mut glyph, None, OutlineOptions::default()).is_err());
        assert_eq!(glyph.encode_xml().unwrap(), before);
    }

    #[test]
    fn work_budget_rejects_excessive_grids_before_allocation() {
        let g = group(vec![ball(1, -100_000.0), ball(2, 100_000.0)]);
        assert!(preview(&g, OutlineOptions::default()).is_err());
    }

    #[test]
    fn whole_font_conversion_is_atomic_across_glyphs() {
        let mut font = norad::Font::new();
        for name in ["a", "b"] {
            let mut glyph = norad::Glyph::new(name);
            let mut g = group(vec![ball(1, 0.0)]);
            if name == "b" {
                g.threshold = 10.0;
            }
            write_metaballs(
                &mut glyph,
                &Metaballs {
                    version: 1,
                    groups: vec![g],
                },
            )
            .unwrap();
            font.default_layer_mut().insert_glyph(glyph);
        }
        assert!(collapse_font(&mut font, OutlineOptions::default()).is_err());
        assert!(
            font.default_layer()
                .get_glyph("a")
                .unwrap()
                .contours
                .is_empty()
        );
        let glyph = font.default_layer_mut().get_glyph_mut("b").unwrap();
        let mut data = read_metaballs(glyph).unwrap();
        data.groups[0].threshold = 0.5;
        write_metaballs(glyph, &data).unwrap();
        assert_eq!(
            collapse_font(&mut font, OutlineOptions::default()).unwrap(),
            2
        );
        assert!(
            font.default_layer()
                .iter()
                .all(|g| !g.lib.contains_key(METABALLS_KEY))
        );
    }
}
