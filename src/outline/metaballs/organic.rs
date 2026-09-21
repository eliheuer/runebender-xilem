// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Organic circle silhouettes with automatic, tangent cubic bridges.
//!
//! Attachment angles follow the Paper.js/Sato construction:
//! <https://github.com/paperjs/paper.js/blob/develop/examples/Paperjs.org/MetaBalls.html>
//! Unlike a summed field, bridges preserve exposed circle arcs. Connections
//! activate continuously with proximity and never introduce a constant-width tube.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use kurbo::{BezPath, CubicBez, ParamCurve, ParamCurveExtrema, Point, Shape, Vec2};

use super::{OutlineOptions, parameters::visible_size};
use crate::formats::metaballs::MetaballGroup;

mod fitting;

const MAX_CONNECTORS: usize = 1024;
const MAX_OUTPUT_SEGMENTS: usize = 4096;
const MAX_INPUT_SEGMENTS: usize = 2048;

#[derive(Clone)]
struct Circle {
    center: Point,
    radius: f64,
    angles: Vec<f64>,
}

impl Circle {
    fn path(&self) -> BezPath {
        let mut angles = self.angles.clone();
        angles.sort_by(f64::total_cmp);
        angles.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
        let mut path = BezPath::new();
        let at = |angle: f64| self.center + Vec2::new(angle.cos(), angle.sin()) * self.radius;
        path.move_to(at(angles[0]));
        for (i, &a) in angles.iter().enumerate() {
            let b = if i + 1 == angles.len() {
                angles[0] + TAU
            } else {
                angles[i + 1]
            };
            let handle = self.radius * (4.0 / 3.0) * ((b - a) / 4.0).tan();
            let end = if i + 1 == angles.len() {
                at(angles[0])
            } else {
                at(b)
            };
            path.curve_to(
                at(a) + Vec2::new(-a.sin(), a.cos()) * handle,
                end - Vec2::new(-b.sin(), b.cos()) * handle,
                end,
            );
        }
        path.close_path();
        path
    }
}

struct Bridge {
    path: BezPath,
    angles: [[f64; 2]; 2],
}

fn bridge(a: &Circle, b: &Circle, rate: f64, visibility: f64) -> Option<Bridge> {
    let axis = b.center - a.center;
    let distance = axis.hypot();
    let total = a.radius + b.radius;
    if distance <= (a.radius - b.radius).abs() + 1e-9 {
        return None;
    }
    let onset = ((distance / total - 1.0) / 4.0).max(0.0);
    if rate <= onset || onset >= 1.0 {
        return None;
    }
    let activation = ((rate - onset) / (1.0 - onset)).clamp(0.0, 1.0);
    let v = activation * activation * (3.0 - 2.0 * activation) * visibility;
    // Sub-nanometric activation has no stable Boolean interpretation.
    if v * a.radius.min(b.radius) < 1e-7 {
        return None;
    }
    let (u1, u2) = if distance < total {
        (
            ((a.radius.powi(2) + distance.powi(2) - b.radius.powi(2))
                / (2.0 * a.radius * distance))
                .clamp(-1.0, 1.0)
                .acos(),
            ((b.radius.powi(2) + distance.powi(2) - a.radius.powi(2))
                / (2.0 * b.radius * distance))
                .clamp(-1.0, 1.0)
                .acos(),
        )
    } else {
        (0.0, 0.0)
    };
    let alpha = ((a.radius - b.radius) / distance).clamp(-1.0, 1.0).acos();
    let angle_a = u1 + (alpha - u1) * v;
    let angle_b = PI - u2 - (PI - u2 - alpha) * v;
    let p0 = Point::new(a.radius * angle_a.cos(), a.radius * angle_a.sin());
    let p3 = Point::new(
        distance + b.radius * angle_b.cos(),
        b.radius * angle_b.sin(),
    );
    if p3.x <= p0.x + 1e-9 {
        return None;
    }
    let mut handle = (v * 2.4).min(p0.distance(p3) / total) * (2.0 * distance / total).min(1.0);
    let make_top = |handle| {
        CubicBez::new(
            p0,
            p0 + Vec2::new(angle_a.sin(), -angle_a.cos()) * (a.radius * handle),
            p3 + Vec2::new(-angle_b.sin(), angle_b.cos()) * (b.radius * handle),
            p3,
        )
    };
    // Exact transverse extrema and monotone longitudinal control coordinates
    // prevent both crossing the reflected lower bridge and local lens loops.
    let minimum = 0.05 * p0.y.min(p3.y);
    let safe = |top: CubicBez| {
        let min_y = top
            .extrema()
            .iter()
            .map(|&t| top.eval(t).y)
            .fold(p0.y.min(p3.y), f64::min);
        min_y >= minimum && top.p0.x <= top.p1.x && top.p1.x <= top.p2.x && top.p2.x <= top.p3.x
    };
    if !safe(make_top(handle)) {
        // Find the continuous admissible handle cap, rather than halving in
        // visible jumps while the user drags Blend through a contact event.
        let (mut lo, mut hi) = (0.0, handle);
        for _ in 0..48 {
            let mid = (lo + hi) * 0.5;
            if safe(make_top(mid)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        handle = lo;
    }
    let unit = axis / distance;
    let theta = unit.y.atan2(unit.x);
    let angles = [
        [
            (theta + angle_a).rem_euclid(TAU),
            (theta - angle_a).rem_euclid(TAU),
        ],
        [
            (theta + angle_b).rem_euclid(TAU),
            (theta - angle_b).rem_euclid(TAU),
        ],
    ];
    // Compute endpoints and tangent vectors using the exact same global-angle
    // arithmetic as Circle::path. Rotating local points instead differs by a
    // few ulps and can manufacture microscopic holes at tangent Boolean joins.
    let point = |circle: &Circle, angle: f64| {
        circle.center + Vec2::new(angle.cos(), angle.sin()) * circle.radius
    };
    let tangent = |circle: &Circle, angle: f64| {
        Vec2::new(-angle.sin(), angle.cos()) * (circle.radius * handle)
    };
    let upper_a = point(a, angles[0][0]);
    let lower_a = point(a, angles[0][1]);
    let upper_b = point(b, angles[1][0]);
    let lower_b = point(b, angles[1][1]);
    // Counter-clockwise patch, matching the circle winding for nonzero union.
    let mut path = BezPath::new();
    path.move_to(lower_a);
    path.curve_to(
        lower_a + tangent(a, angles[0][1]),
        lower_b - tangent(b, angles[1][1]),
        lower_b,
    );
    path.line_to(upper_b);
    path.curve_to(
        upper_b + tangent(b, angles[1][0]),
        upper_a - tangent(a, angles[0][0]),
        upper_a,
    );
    path.close_path();
    Some(Bridge { path, angles })
}

fn primitives(mut circles: Vec<Circle>, rate: f64, count: &mut usize) -> Result<BezPath, String> {
    let mut patches = Vec::new();
    for i in 0..circles.len() {
        for j in i + 1..circles.len() {
            let distance = circles[i].center.distance(circles[j].center);
            // A closer third center gradually occludes a far pair. Fade over
            // one fifth of the pair's total radius, avoiding an abrupt bridge
            // deletion when a symmetric junction is moved by a tiny amount.
            let margin = 0.2 * (circles[i].radius + circles[j].radius);
            let occlusion = circles
                .iter()
                .enumerate()
                .filter(|(k, _)| *k != i && *k != j)
                .map(|(_, c)| {
                    let nearer = c
                        .center
                        .distance(circles[i].center)
                        .max(c.center.distance(circles[j].center));
                    ((distance - nearer) / margin).clamp(0.0, 1.0)
                })
                .fold(0.0, f64::max);
            let visibility = 1.0 - occlusion * occlusion * (3.0 - 2.0 * occlusion);
            if let Some(bridge) = bridge(&circles[i], &circles[j], rate, visibility) {
                *count += 1;
                if *count > MAX_CONNECTORS {
                    return Err("too many organic metaball connections; split the group".into());
                }
                circles[i].angles.extend(bridge.angles[0]);
                circles[j].angles.extend(bridge.angles[1]);
                patches.push(bridge.path);
            }
        }
    }
    let mut combined = BezPath::new();
    for circle in circles {
        combined.extend(circle.path());
    }
    for patch in patches {
        combined.extend(patch);
    }
    Ok(combined)
}

pub(super) fn outline(
    group: &MetaballGroup,
    options: OutlineOptions,
    structured: bool,
) -> Result<Vec<BezPath>, String> {
    let rate = group.blend.ok_or("organic metaballs need a blend rate")?;
    if !rate.is_finite()
        || !(0.0..=1.0).contains(&rate)
        || !group.links.is_empty()
        || group.balls.len() > 256
        || !options.accuracy.is_finite()
        || !(0.001..=100.0).contains(&options.accuracy)
    {
        return Err("invalid organic metaball parameters".into());
    }
    let (mut positive, mut negative) = (Vec::new(), Vec::new());
    for ball in &group.balls {
        let mut absolute = ball.clone();
        absolute.stiffness = absolute.stiffness.abs();
        let Some((radius, _)) = visible_size(&absolute, group.threshold) else {
            continue;
        };
        let circle = Circle {
            center: Point::new(ball.x, ball.y),
            radius,
            angles: vec![0.0, FRAC_PI_2, PI, PI + FRAC_PI_2],
        };
        if ball.stiffness > 0.0 {
            positive.push(circle);
        } else {
            negative.push(circle);
        }
    }
    if positive.is_empty() {
        return Ok(Vec::new());
    }
    let mut count = 0;
    let positive = primitives(positive, rate, &mut count)?;
    let negative = primitives(negative, rate, &mut count)?;
    if positive.segments().count() + negative.segments().count() > MAX_INPUT_SEGMENTS {
        return Err("too many organic metaball curves before union; split the group".into());
    }
    // Match linesweeper 0.4's documented default precision. Tangent joins can
    // leave two-edge loops smaller than that precision; remove only such tiny
    // loops at actual input vertices, never ordinary counters or narrow necks.
    let input_bounds = positive.bounding_box().union(negative.bounding_box());
    let magnitude = input_bounds
        .x0
        .abs()
        .max(input_bounds.y0.abs())
        .max(input_bounds.x1.abs())
        .max(input_bounds.y1.abs());
    let boolean_eps = (magnitude * (f64::EPSILON * 64.0)).max(1e-6);
    let attachments: Vec<_> = positive
        .segments()
        .chain(negative.segments())
        .map(|segment| segment.start())
        .collect();
    let result = linesweeper::binary_op(
        &positive,
        &negative,
        linesweeper::FillRule::NonZero,
        linesweeper::BinaryOp::Difference,
    )
    .map_err(|error| format!("cannot resolve organic metaball silhouette: {error:?}"))?;
    let paths: Vec<_> = result
        .contours()
        .filter(|contour| !numerical_join_loop(&contour.path, boolean_eps, &attachments))
        .map(|contour| contour.path.clone())
        .collect();
    if paths
        .iter()
        .map(|path| path.segments().count())
        .sum::<usize>()
        > MAX_OUTPUT_SEGMENTS
    {
        return Err("organic metaball silhouette is too complex; split the group".into());
    }
    if structured {
        fitting::convert(&paths, options.accuracy)
    } else {
        Ok(paths)
    }
}

// The precision floor concerns Boolean topology, independently of cubic fitting
// accuracy. Limit cleanup to at most three edges, a thickness and area at the
// operation precision floor, and contact with an input attachment. A tangent
// join can produce a long-but-submicroscopic two-edge sliver, so requiring both
// dimensions to be tiny is insufficient. In particular, an ordinary small
// circular counter does not meet these conditions.
fn numerical_join_loop(path: &BezPath, epsilon: f64, attachments: &[Point]) -> bool {
    let segments = path.segments().count();
    if segments > 3 {
        return false;
    }
    let bounds = path.bounding_box();
    let width = bounds.width();
    let height = bounds.height();
    let span = width.max(height);
    let at_attachment = attachments
        .iter()
        .any(|point| point.distance(bounds.center()) <= 0.5 * span + 8.0 * epsilon);
    let precision_thin = width.min(height) <= 4.0 * epsilon;
    at_attachment && precision_thin && (segments <= 2 || path.area().abs() <= 4.0 * epsilon * span)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::metaballs::Metaball;
    use kurbo::{ParamCurveNearest, Shape};

    fn group(balls: &[(f64, f64, f64)], rate: f64) -> MetaballGroup {
        let scale = (1.0 - 0.25_f64.cbrt()).sqrt();
        MetaballGroup {
            id: 1,
            threshold: 0.5,
            blend: Some(rate),
            links: vec![],
            balls: balls
                .iter()
                .enumerate()
                .map(|(id, &(x, y, radius))| Metaball {
                    id: u32::try_from(id).unwrap(),
                    x,
                    y,
                    radius: radius.abs() / scale,
                    stiffness: if radius > 0.0 { 2.0 } else { -2.0 },
                })
                .collect(),
        }
    }

    fn preview(group: &MetaballGroup) -> Vec<BezPath> {
        outline(group, OutlineOptions::default(), false).unwrap()
    }

    fn ink(paths: &[BezPath], p: Point) -> bool {
        paths.iter().map(|path| path.winding(p)).sum::<i32>() != 0
    }

    fn half_width(paths: &[BezPath], x: f64) -> f64 {
        if !ink(paths, Point::new(x, 0.0)) {
            return 0.0;
        }
        let (mut lo, mut hi) = (0.0, 300.0);
        for _ in 0..50 {
            let mid = (lo + hi) * 0.5;
            if ink(paths, Point::new(x, mid)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        (lo + hi) * 0.5
    }

    fn no_crossings(path: &BezPath) {
        let points: Vec<_> = path
            .segments()
            .flat_map(|segment| (0..16).map(move |i| segment.eval(f64::from(i) / 16.0)))
            .collect();
        let cross = |a: Point, b: Point, c: Point| (b - a).cross(c - a);
        for i in 0..points.len() {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            for j in i + 2..points.len() {
                if i == 0 && j + 1 == points.len() {
                    continue;
                }
                let (c, d) = (points[j], points[(j + 1) % points.len()]);
                assert!(
                    !(cross(a, b, c) * cross(a, b, d) < -1e-12
                        && cross(c, d, a) * cross(c, d, b) < -1e-12),
                    "crossed contour edges {i}/{j}"
                );
            }
        }
    }

    #[test]
    fn organic_equal_pair_sweeps_from_separate_through_narrow_to_broad() {
        let source = [(0.0, 0.0, 100.0), (280.0, 0.0, 100.0)];
        assert_eq!(preview(&group(&source, 0.0)).len(), 2);
        assert_eq!(preview(&group(&source, 0.1)).len(), 2);
        let mut previous = 0.0;
        for rate in [0.12, 0.25, 0.5, 0.85, 1.0] {
            let paths = preview(&group(&source, rate));
            assert_eq!(
                paths.len(),
                1,
                "rate {rate}: {:?}",
                paths
                    .iter()
                    .map(|p| (p.area(), p.bounding_box(), p.segments().count()))
                    .collect::<Vec<_>>()
            );
            let neck = half_width(&paths, 140.0);
            assert!(neck > previous, "neck {neck} follows {previous} at {rate}");
            previous = neck;
            let bounds = paths[0].bounding_box();
            assert!((bounds.x0 + 100.0).abs() < 0.1 && (bounds.x1 - 380.0).abs() < 0.1);
            assert!((half_width(&paths, 0.0) - 100.0).abs() < 0.1);
            assert!((half_width(&paths, 280.0) - 100.0).abs() < 0.1);
            no_crossings(&paths[0]);
        }
    }

    #[test]
    fn organic_long_unequal_pair_preserves_lobes_and_has_a_curved_waist() {
        let paths = preview(&group(&[(0.0, 0.0, 100.0), (480.0, 0.0, 60.0)], 0.75));
        assert_eq!(paths.len(), 1);
        assert!((half_width(&paths, 0.0) - 100.0).abs() < 0.1);
        assert!((half_width(&paths, 480.0) - 60.0).abs() < 0.1);
        let middle = half_width(&paths, 240.0);
        assert!(middle > 0.0 && middle < 60.0, "waist {middle}");
        assert!(
            (half_width(&paths, 140.0) - middle).abs() > 1.0,
            "bridge must curve, not form a tube"
        );
        let bounds = paths[0].bounding_box();
        assert!((bounds.x0 + 100.0).abs() < 0.1 && (bounds.x1 - 540.0).abs() < 0.1);
        no_crossings(&paths[0]);
    }

    #[test]
    fn organic_junction_is_continuous_under_small_position_changes() {
        let y = 300.0 * 3.0_f64.sqrt() / 2.0;
        let mut g = group(
            &[(0.0, 0.0, 90.0), (300.0, 0.0, 90.0), (150.0, y, 90.0)],
            0.95,
        );
        let initial = preview(&g);
        assert_eq!(
            initial.len(),
            1,
            "junction: {:?}",
            initial
                .iter()
                .map(|p| (p.area(), p.bounding_box(), p.segments().count()))
                .collect::<Vec<_>>()
        );
        assert!(ink(&initial, Point::new(150.0, y / 3.0)));
        let area: f64 = initial.iter().map(|p| p.area()).sum();
        g.balls[2].x += 0.01;
        let changed = preview(&g);
        assert_eq!(changed.len(), 1);
        let changed_area: f64 = changed.iter().map(|p| p.area()).sum();
        assert!(
            (changed_area - area).abs() < area.abs() * 1e-4,
            "junction jumped: {area} -> {changed_area}"
        );
        no_crossings(&changed[0]);
    }

    #[test]
    fn organic_bent_chain_and_negative_hole_have_resolved_silhouettes() {
        let chain = group(
            &[
                (-190.0, -100.0, 75.0),
                (0.0, -70.0, 60.0),
                (40.0, 140.0, 65.0),
                (230.0, 170.0, 80.0),
            ],
            0.7,
        );
        let paths = preview(&chain);
        assert_eq!(paths.len(), 1);
        no_crossings(&paths[0]);
        let ring = preview(&group(&[(0.0, 0.0, 150.0), (0.0, 0.0, -50.0)], 0.5));
        assert_eq!(ring.len(), 2);
        assert!(!ink(&ring, Point::ORIGIN));
        assert!(ink(&ring, Point::new(100.0, 0.0)));
        assert!(ring[0].area() * ring[1].area() < 0.0);
    }

    #[test]
    fn organic_boolean_precision_cleanup_preserves_small_counters() {
        let mut lens = BezPath::new();
        lens.move_to((100.0, 0.0));
        lens.line_to((100.000001, 0.0000003));
        lens.line_to((100.0, 0.0000003));
        lens.close_path();
        assert!(numerical_join_loop(&lens, 1e-6, &[Point::new(100.0, 0.0)]));
        let mut tangent_sliver = BezPath::new();
        tangent_sliver.move_to((280.0, -100.0));
        tangent_sliver.curve_to(
            (279.999, -99.9999995),
            (279.998, -99.9999995),
            (279.99747275258846, -100.0),
        );
        tangent_sliver.line_to((280.0, -100.0));
        tangent_sliver.close_path();
        assert!(numerical_join_loop(
            &tangent_sliver,
            1e-6,
            &[Point::new(280.0, -100.0)]
        ));
        assert!(
            !numerical_join_loop(&lens, 1e-6, &[Point::ORIGIN]),
            "cleanup requires an actual input join"
        );
        let mut g = group(&[(0.0, 0.0, 150.0), (0.0, 0.0, -50.0)], 0.5);
        g.balls[1].radius = 1.0;
        g.balls[1].stiffness = -0.5 / (1.0 - 0.01_f64.powi(2)).powi(3);
        let paths = preview(&g);
        assert_eq!(paths.len(), 2, "a .01-unit circular counter must survive");
        assert!(!ink(&paths, Point::ORIGIN));
        assert!(ink(&paths, Point::new(0.02, 0.0)));
    }

    #[test]
    fn organic_conversion_preserves_geometry_bottom_starts_and_node_economy() {
        for g in [
            group(&[(0.0, 0.0, 100.0), (280.0, 0.0, 100.0)], 0.25),
            group(&[(0.0, 0.0, 100.0), (480.0, 0.0, 60.0)], 0.75),
            group(&[(0.0, 0.0, 100.0), (140.0, 0.0, 100.0)], 0.0),
        ] {
            let before = preview(&g);
            let converted = outline(&g, OutlineOptions::default(), true).unwrap();
            assert_eq!(before.len(), converted.len());
            for path in &converted {
                let first = path.segments().next().unwrap().start();
                assert!(
                    (first.y - path.bounding_box().y0).abs() < 1e-5,
                    "start {first:?}"
                );
                assert!(
                    path.segments().count() <= 24,
                    "{} nodes",
                    path.segments().count()
                );
                no_crossings(path);
            }
            for (source, target) in [(&before, &converted), (&converted, &before)] {
                let target: Vec<_> = target.iter().flat_map(|path| path.segments()).collect();
                for segment in source.iter().flat_map(|p| p.segments()) {
                    for i in 0..=32 {
                        let p = segment.eval(f64::from(i) / 32.0);
                        assert!(
                            target
                                .iter()
                                .any(|s| s.nearest(p, 1e-5).distance_sq < 0.26_f64.powi(2)),
                            "conversion shifted {p:?}"
                        );
                    }
                }
            }
        }
    }
}
