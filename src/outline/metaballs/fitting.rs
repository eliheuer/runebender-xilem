// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Supply exact metaball boundary features and tangents to img2bez for cubic fitting.

use super::evaluator::PreparedField;
use kurbo::{BezPath, Point};

#[derive(Clone, Copy)]
enum Feature {
    Horizontal,
    Vertical,
    Inflection,
}

#[derive(Clone, Copy)]
struct Knot {
    point: Point,
    feature: Option<Feature>,
}

fn feature_value(field: &PreparedField, point: Point, feature: Feature) -> f64 {
    let (g, h) = field.derivatives(point);
    let (value, scale) = match feature {
        Feature::Horizontal => (g.x, g.hypot()),
        Feature::Vertical => (g.y, g.hypot()),
        // The signed curvature numerator of an implicit level set. On a
        // straight capsule side these terms cancel analytically; use their
        // magnitude to distinguish roundoff from a genuine inflection.
        Feature::Inflection => {
            let terms = [h[0] * g.y * g.y, -2.0 * h[1] * g.x * g.y, h[2] * g.x * g.x];
            (
                terms.iter().sum(),
                terms.iter().map(|term| term.abs()).sum(),
            )
        }
    };
    if value.abs() <= 64.0 * f64::EPSILON * scale {
        0.0
    } else {
        value
    }
}

// Newton projection stays local to one sampled edge. Never jump to another branch near a pinch.
fn project(field: &PreparedField, seed: Point, edge_length: f64) -> Result<Point, String> {
    let mut point = seed;
    for _ in 0..12 {
        let (gradient, _) = field.derivatives(point);
        if gradient.hypot2() < 1e-24 {
            return Err("metaball boundary is singular; adjust the blend before conversion".into());
        }
        let residual = field.value(point) - field.threshold;
        if residual.abs() < 1e-12 * field.threshold {
            return Ok(point);
        }
        point -= gradient * (residual / gradient.hypot2());
        if !point.x.is_finite() || !point.y.is_finite() || point.distance(seed) > edge_length {
            break;
        }
    }
    Err("cannot resolve a metaball feature on this grid; decrease sampling spacing".into())
}

fn push_knot(knots: &mut Vec<Knot>, knot: Knot) {
    if let Some(last) = knots.last_mut()
        && last.point.distance_squared(knot.point) < 1e-14
    {
        if knot.feature.is_some() {
            *last = knot;
        }
        return;
    }
    knots.push(knot);
}

fn structural_knots(field: &PreparedField, points: &[Point]) -> Result<Vec<Knot>, String> {
    let mut knots = Vec::new();
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        push_knot(
            &mut knots,
            Knot {
                point: a,
                feature: None,
            },
        );
        let mut roots = Vec::new();
        for feature in [Feature::Horizontal, Feature::Vertical, Feature::Inflection] {
            let fa = feature_value(field, a, feature);
            let fb = feature_value(field, b, feature);
            if fa * fb > 0.0 || (fa == 0.0 && fb == 0.0) {
                continue;
            }
            let (mut lo, mut hi) = (0.0, 1.0);
            for _ in 0..40 {
                let mid = (lo + hi) * 0.5;
                let point = project(field, a.lerp(b, mid), a.distance(b))?;
                if feature_value(field, point, feature) * fa > 0.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let t = (lo + hi) * 0.5;
            roots.push((
                t,
                Knot {
                    point: project(field, a.lerp(b, t), a.distance(b))?,
                    feature: Some(feature),
                },
            ));
        }
        roots.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, knot) in roots {
            push_knot(&mut knots, knot);
        }
    }
    if knots.len() > 1 && knots[0].point.distance_squared(knots.last().unwrap().point) < 1e-14 {
        let last = knots.pop().expect("two knots checked above");
        if last.feature.is_some() {
            knots[0] = last;
        }
    }
    let first = knots
        .iter()
        .position(|knot| knot.feature.is_some())
        .ok_or("cannot resolve metaball extrema; decrease sampling spacing")?;
    knots.rotate_left(first);
    Ok(knots)
}

pub(super) fn fit(
    field: &PreparedField,
    points: &[Point],
    accuracy: f64,
) -> Result<BezPath, String> {
    let samples = structural_knots(field, points)?
        .into_iter()
        .map(|knot| {
            let tangent = field.tangent(knot.point);
            img2bez::BoundarySample {
                position: [knot.point.x, knot.point.y],
                tangent: [tangent.x, tangent.y],
                feature: knot.feature.map(|feature| match feature {
                    Feature::Horizontal => img2bez::BoundaryFeature::ExtremumY,
                    Feature::Vertical => img2bez::BoundaryFeature::ExtremumX,
                    Feature::Inflection => img2bez::BoundaryFeature::Inflection,
                }),
            }
        })
        .collect();
    img2bez::fit_smooth_contours(&[samples], accuracy)
        .map_err(|error| error.to_string())?
        .to_bezpaths()
        .into_iter()
        .next()
        .ok_or_else(|| "img2bez did not produce a metaball contour".into())
}

#[cfg(test)]
mod tests {
    use super::{Feature, feature_value};
    use crate::formats::metaballs::{Metaball, MetaballGroup, MetaballLink};
    use crate::outline::metaballs::evaluator::PreparedField;
    use crate::outline::metaballs::{OutlineOptions, cubic_outline};
    use crate::outline::metaballs::{field, tangent};
    use kurbo::ParamCurve;

    #[test]
    fn straight_capsule_sides_do_not_invent_inflections() {
        for rise in [0.0, 120.0] {
            let group = MetaballGroup {
                id: 1,
                threshold: 0.5,
                balls: [(0.0, 0.0), (600.0, rise)]
                    .into_iter()
                    .enumerate()
                    .map(|(id, (x, y))| Metaball {
                        id: u32::try_from(id).unwrap(),
                        x,
                        y,
                        radius: 100.0,
                        stiffness: 0.0,
                    })
                    .collect(),
                links: vec![MetaballLink {
                    id: 1,
                    start: 0,
                    end: 1,
                    width: 12.0,
                }],
            };
            let field = PreparedField::new(&group);
            let axis = kurbo::Vec2::new(600.0, rise);
            let normal = kurbo::Vec2::new(-axis.y, axis.x).normalize();
            for i in 1..100 {
                let point = kurbo::Point::ORIGIN + axis * (f64::from(i) / 100.0) + normal * 6.0;
                assert_eq!(feature_value(&field, point, Feature::Inflection), 0.0);
                if rise == 0.0 {
                    assert_eq!(feature_value(&field, point, Feature::Horizontal), 0.0);
                }
            }
        }
    }

    #[test]
    fn blends_use_few_nodes_without_sacrificing_shape() {
        for (dx, dy, radius, max_nodes) in [(0.0, 210.0, 180.0, 8), (95.0, 252.0, 205.0, 10)] {
            let group = MetaballGroup {
                id: 1,
                threshold: 0.5,
                links: vec![],
                balls: [(100.0, 100.0), (100.0 + dx, 100.0 + dy)]
                    .into_iter()
                    .enumerate()
                    .map(|(i, (x, y))| Metaball {
                        id: u32::try_from(i).unwrap(),
                        x,
                        y,
                        radius,
                        stiffness: 2.0,
                    })
                    .collect(),
            };
            let paths = cubic_outline(&group, OutlineOptions::default()).unwrap();
            assert_eq!(paths.len(), 1);
            let segments: Vec<_> = paths[0].segments().map(|s| s.to_cubic()).collect();
            assert!(
                segments.len() <= max_nodes,
                "{} nodes for offset {dx}",
                segments.len()
            );
            let start = segments[0].p0;
            for (i, cubic) in segments.iter().enumerate() {
                assert!(start.y <= cubic.p0.y + 1e-9, "start at the lowest node");
                let next = segments[(i + 1) % segments.len()];
                assert_eq!(cubic.p3, next.p0);
                assert!(
                    (cubic.p3 - cubic.p2)
                        .normalize()
                        .dot((next.p1 - next.p0).normalize())
                        > 1.0 - 1e-8
                );
                for j in 0..=200 {
                    let p = cubic.eval(f64::from(j) / 200.0);
                    let error =
                        (field(&group, p) - group.threshold).abs() / tangent(&group, p).hypot();
                    assert!(error < 0.25, "normal discrepancy {error}");
                    assert!(p.y >= start.y - 1e-8, "start at the bottom of the curve");
                }
            }
        }
    }
}
