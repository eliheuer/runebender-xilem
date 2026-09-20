// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Fit between exact field features so simplification cannot move font nodes off extrema.

use super::{field, tangent};
use crate::formats::metaballs::MetaballGroup;
use kurbo::{BezPath, Point, Vec2};

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

// Gradient and symmetric Hessian of sum(strength * (1 - distance² / radius²)³).
fn derivatives(group: &MetaballGroup, point: Point) -> (Vec2, [f64; 3]) {
    let mut gradient = Vec2::ZERO;
    let mut hessian = [0.0; 3];
    for ball in &group.balls {
        let delta = point - Point::new(ball.x, ball.y);
        let radius2 = ball.radius * ball.radius;
        let q = 1.0 - delta.hypot2() / radius2;
        if q <= 0.0 {
            continue;
        }
        let radial = -6.0 * ball.stiffness * q * q / radius2;
        let outer = 24.0 * ball.stiffness * q / (radius2 * radius2);
        gradient += radial * delta;
        hessian[0] += radial + outer * delta.x * delta.x;
        hessian[1] += outer * delta.x * delta.y;
        hessian[2] += radial + outer * delta.y * delta.y;
    }
    (gradient, hessian)
}

fn feature_value(group: &MetaballGroup, point: Point, feature: Feature) -> f64 {
    let (g, h) = derivatives(group, point);
    match feature {
        Feature::Horizontal => g.x,
        Feature::Vertical => g.y,
        // The signed curvature numerator of an implicit level set.
        Feature::Inflection => h[0] * g.y * g.y - 2.0 * h[1] * g.x * g.y + h[2] * g.x * g.x,
    }
}

// Newton projection stays local to one sampled edge. Never jump to another branch near a pinch.
fn project(group: &MetaballGroup, seed: Point, edge_length: f64) -> Result<Point, String> {
    let mut point = seed;
    for _ in 0..12 {
        let (gradient, _) = derivatives(group, point);
        if gradient.hypot2() < 1e-24 {
            return Err("metaball boundary is singular; adjust the blend before conversion".into());
        }
        let residual = field(group, point) - group.threshold;
        if residual.abs() < 1e-12 * group.threshold {
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

fn structural_knots(group: &MetaballGroup, points: &[Point]) -> Result<Vec<Knot>, String> {
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
            let fa = feature_value(group, a, feature);
            let fb = feature_value(group, b, feature);
            if fa * fb > 0.0 || (fa == 0.0 && fb == 0.0) {
                continue;
            }
            let (mut lo, mut hi) = (0.0, 1.0);
            for _ in 0..40 {
                let mid = (lo + hi) * 0.5;
                let point = project(group, a.lerp(b, mid), a.distance(b))?;
                if feature_value(group, point, feature) * fa > 0.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let t = (lo + hi) * 0.5;
            roots.push((
                t,
                Knot {
                    point: project(group, a.lerp(b, t), a.distance(b))?,
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

fn control(group: &MetaballGroup, knot: Knot, chord: Vec2) -> Vec2 {
    let mut direction = tangent(group, knot.point);
    match knot.feature {
        Some(Feature::Horizontal) => direction.y = 0.0,
        Some(Feature::Vertical) => direction.x = 0.0,
        _ => (),
    }
    if direction.hypot() > 1e-12 {
        direction.normalize() * (chord.hypot() / 3.0)
    } else {
        chord / 3.0
    }
}

pub(super) fn fit(
    group: &MetaballGroup,
    points: &[Point],
    accuracy: f64,
) -> Result<BezPath, String> {
    let knots = structural_knots(group, points)?;
    let mut stops: Vec<_> = knots
        .iter()
        .enumerate()
        .filter_map(|(i, knot)| knot.feature.map(|_| i))
        .collect();
    stops.push(knots.len());
    let mut output = BezPath::new();
    output.move_to(knots[0].point);
    for pair in stops.windows(2) {
        let mut span = BezPath::new();
        span.move_to(knots[pair[0]].point);
        for i in pair[0]..pair[1] {
            let a = knots[i];
            let b = knots[(i + 1) % knots.len()];
            let chord = b.point - a.point;
            span.curve_to(
                a.point + control(group, a, chord),
                b.point - control(group, b, chord),
                b.point,
            );
        }
        // An open span preserves both endpoint positions and endpoint tangent directions.
        let fitted = kurbo::simplify::simplify_bezpath(
            span,
            accuracy,
            &kurbo::simplify::SimplifyOptions::default(),
        );
        let mut cubics: Vec<_> = fitted
            .segments()
            .map(|segment| segment.to_cubic())
            .collect();
        // Kurbo's angular representation can leave roundoff in an axis tangent.
        // Restore the explicit constraints so editors recognize truly axis-aligned handles.
        let align = |handle: &mut Point, knot: Knot| match knot.feature {
            Some(Feature::Horizontal) => handle.y = knot.point.y,
            Some(Feature::Vertical) => handle.x = knot.point.x,
            _ => (),
        };
        if let Some(first) = cubics.first_mut() {
            let knot = knots[pair[0]];
            first.p1 += knot.point - first.p0;
            first.p0 = knot.point;
            align(&mut first.p1, knot);
        }
        if let Some(last) = cubics.last_mut() {
            let knot = knots[pair[1] % knots.len()];
            last.p2 += knot.point - last.p3;
            last.p3 = knot.point;
            align(&mut last.p2, knot);
        }
        for cubic in cubics {
            output.curve_to(cubic.p1, cubic.p2, cubic.p3);
        }
    }
    output.close_path();
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::metaballs::Metaball;
    use crate::outline::metaballs::{OutlineOptions, cubic_outline};
    use kurbo::ParamCurve;

    #[test]
    fn blended_stem_keeps_four_inflection_nodes() {
        let group = MetaballGroup {
            id: 1,
            threshold: 0.5,
            balls: [160.0, 370.0]
                .into_iter()
                .enumerate()
                .map(|(i, y)| Metaball {
                    id: u32::try_from(i).unwrap(),
                    x: 250.0,
                    y,
                    radius: 180.0,
                    stiffness: 2.0,
                })
                .collect(),
        };
        let paths = cubic_outline(&group, OutlineOptions::default()).unwrap();
        let inflections = paths[0]
            .segments()
            .filter(|s| {
                let point = s.start();
                let (gradient, _) = derivatives(&group, point);
                let curvature =
                    feature_value(&group, point, Feature::Inflection) / gradient.hypot().powi(3);
                curvature.abs() < 1e-10
            })
            .count();
        assert_eq!(
            inflections, 4,
            "nodes at all four transitions into the neck"
        );
    }
}
