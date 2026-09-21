// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Prepared radial and finite-segment fields shared by preview and cubic fitting.

use kurbo::{Point, Rect, Vec2};

use crate::formats::metaballs::MetaballGroup;

struct Source {
    start: Point,
    axis: Vec2,
    length2: f64,
    radius: f64,
    inverse_radius2: f64,
    strength: f64,
}

impl Source {
    fn new(start: Point, end: Point, radius: f64, strength: f64) -> Self {
        let axis = end - start;
        Self {
            start,
            axis,
            length2: axis.hypot2(),
            radius,
            inverse_radius2: 1.0 / (radius * radius),
            strength,
        }
    }

    fn parameter(&self, point: Point) -> f64 {
        if self.length2 == 0.0 {
            0.0
        } else {
            ((point - self.start).dot(self.axis) / self.length2).clamp(0.0, 1.0)
        }
    }

    fn offset(&self, point: Point, t: f64) -> Vec2 {
        if t > 0.0 && t < 1.0 {
            // Reconstruct only the normal component. Subtracting a projected
            // point leaves tiny longitudinal errors that invent extrema along
            // an exactly straight capsule side.
            let normal = Vec2::new(-self.axis.y, self.axis.x);
            normal * ((point - self.start).dot(normal) / self.length2)
        } else {
            point - (self.start + self.axis * t)
        }
    }
}

/// Endpoint lookup and kernel scales are resolved once per sampled outline.
pub(super) struct PreparedField {
    sources: Vec<Source>,
    pub(super) threshold: f64,
}

impl PreparedField {
    /// Source data must have passed model validation.
    pub(super) fn new(group: &MetaballGroup) -> Self {
        let mut sources: Vec<_> = group
            .balls
            .iter()
            .map(|ball| {
                let center = Point::new(ball.x, ball.y);
                Source::new(center, center, ball.radius, ball.stiffness)
            })
            .collect();
        let width_scale = 0.5 / (1.0 - 0.25_f64.cbrt()).sqrt();
        for link in &group.links {
            let start = group.balls.iter().find(|b| b.id == link.start);
            let end = group.balls.iter().find(|b| b.id == link.end);
            if let (Some(start), Some(end)) = (start, end) {
                sources.push(Source::new(
                    Point::new(start.x, start.y),
                    Point::new(end.x, end.y),
                    link.width * width_scale,
                    4.0 * group.threshold,
                ));
            }
        }
        Self {
            sources,
            threshold: group.threshold,
        }
    }

    pub(super) fn value(&self, point: Point) -> f64 {
        self.sources
            .iter()
            .map(|source| {
                let delta = source.offset(point, source.parameter(point));
                // Retain the version-one radial arithmetic exactly.
                let q = (1.0 - delta.hypot2() / source.radius.powi(2)).max(0.0);
                source.strength * q.powi(3)
            })
            .sum()
    }

    /// Gradient and symmetric Hessian `[xx, xy, yy]` of the same compact fields.
    pub(super) fn derivatives(&self, point: Point) -> (Vec2, [f64; 3]) {
        let mut gradient = Vec2::ZERO;
        let mut hessian = [0.0; 3];
        for source in &self.sources {
            let t = source.parameter(point);
            let delta = source.offset(point, t);
            let q = 1.0 - delta.hypot2() * source.inverse_radius2;
            if q <= 0.0 {
                continue;
            }
            // The squared-distance Hessian is twice the normal projector in
            // the segment interior, and twice the identity on circular endcaps.
            let projection = if t > 0.0 && t < 1.0 {
                [
                    1.0 - source.axis.x * source.axis.x / source.length2,
                    -source.axis.x * source.axis.y / source.length2,
                    1.0 - source.axis.y * source.axis.y / source.length2,
                ]
            } else {
                [1.0, 0.0, 1.0]
            };
            let radial = -6.0 * source.strength * q * q * source.inverse_radius2;
            let outer = 24.0 * source.strength * q * source.inverse_radius2.powi(2);
            gradient += radial * delta;
            hessian[0] += radial * projection[0] + outer * delta.x * delta.x;
            hessian[1] += radial * projection[1] + outer * delta.x * delta.y;
            hessian[2] += radial * projection[2] + outer * delta.y * delta.y;
        }
        (gradient, hessian)
    }

    pub(super) fn tangent(&self, point: Point) -> Vec2 {
        let (gradient, _) = self.derivatives(point);
        Vec2::new(gradient.y, -gradient.x)
    }

    /// Negative fields cannot introduce ink outside positive field support.
    pub(super) fn bounds(&self) -> Option<Rect> {
        self.sources
            .iter()
            .filter(|source| source.strength > 0.0)
            .map(|source| {
                Rect::from_points(source.start, source.start + source.axis)
                    .inflate(source.radius, source.radius)
            })
            .reduce(|a, b| a.union(b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::metaballs::{Metaball, MetaballLink};

    fn linked_group() -> MetaballGroup {
        MetaballGroup {
            blend: None,
            id: 1,
            threshold: 0.5,
            balls: [(0.0, 0.0), (200.0, 150.0)]
                .into_iter()
                .enumerate()
                .map(|(id, (x, y))| Metaball {
                    id: u32::try_from(id).unwrap(),
                    x,
                    y,
                    radius: 40.0,
                    stiffness: 0.0,
                })
                .collect(),
            links: vec![MetaballLink {
                id: 0,
                start: 0,
                end: 1,
                width: 20.0,
            }],
        }
    }

    #[test]
    fn link_has_constant_isolated_width_and_circular_endcaps() {
        for threshold in [0.01, 0.5, 100.0] {
            let mut group = linked_group();
            group.threshold = threshold;
            let field = PreparedField::new(&group);
            let axis = Vec2::new(0.8, 0.6);
            let normal = Vec2::new(-0.6, 0.8);
            for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let boundary = Point::ORIGIN + axis * (250.0 * t) + normal * 10.0;
                assert!((field.value(boundary) - threshold).abs() < 1e-10);
            }
            for boundary in [
                Point::ORIGIN - axis * 10.0,
                Point::new(200.0, 150.0) + axis * 10.0,
            ] {
                assert!((field.value(boundary) - threshold).abs() < 1e-10);
            }
            assert_eq!(field.value(Point::new(100.0, 75.0) + normal * 30.0), 0.0);
        }
    }

    #[test]
    fn mixed_ball_and_link_derivatives_match_finite_differences() {
        let mut group = linked_group();
        group.balls[0].stiffness = 2.0;
        group.balls[1].stiffness = -0.5;
        let field = PreparedField::new(&group);
        // Test the capsule interior, both endcaps, blended neighborhoods and exterior.
        for p in [
            Point::new(95.0, 80.0),
            Point::new(-5.0, -8.0),
            Point::new(205.0, 155.0),
            Point::new(10.0, 12.0),
            Point::new(700.0, 700.0),
        ] {
            let epsilon = 1e-4;
            let dx = Vec2::new(epsilon, 0.0);
            let dy = Vec2::new(0.0, epsilon);
            let (gradient, hessian) = field.derivatives(p);
            let numerical = Vec2::new(
                (field.value(p + dx) - field.value(p - dx)) / (2.0 * epsilon),
                (field.value(p + dy) - field.value(p - dy)) / (2.0 * epsilon),
            );
            assert!((gradient - numerical).hypot() < 1e-7);
            let gx = (field.derivatives(p + dx).0 - field.derivatives(p - dx).0) / (2.0 * epsilon);
            let gy = (field.derivatives(p + dy).0 - field.derivatives(p - dy).0) / (2.0 * epsilon);
            assert!((hessian[0] - gx.x).abs() < 1e-7);
            assert!((hessian[1] - gx.y).abs() < 1e-7);
            assert!((hessian[1] - gy.x).abs() < 1e-7);
            assert!((hessian[2] - gy.y).abs() < 1e-7);
        }
    }

    #[test]
    fn legacy_radial_values_are_bit_exact() {
        let mut group = linked_group();
        group.links.clear();
        group.balls[0].radius = 127.3;
        group.balls[0].stiffness = 1.7;
        group.balls[1].radius = 98.7;
        group.balls[1].stiffness = -0.6;
        let field = PreparedField::new(&group);
        for x in [-300.5, -27.4, 0.0, 68.1, 141.2, 233.7, 700.0] {
            for y in [-151.9, 0.0, 12.3, 89.7, 179.1, 401.0] {
                let point = Point::new(x, y);
                let legacy: f64 = group
                    .balls
                    .iter()
                    .map(|ball| {
                        let delta = point - Point::new(ball.x, ball.y);
                        let q = (1.0 - delta.hypot2() / ball.radius.powi(2)).max(0.0);
                        ball.stiffness * q.powi(3)
                    })
                    .sum();
                assert_eq!(field.value(point).to_bits(), legacy.to_bits());
            }
        }
    }

    #[test]
    fn capsule_endcap_seams_are_c1_with_piecewise_hessian() {
        let mut group = linked_group();
        group.balls[1].y = 0.0;
        let field = PreparedField::new(&group);
        let epsilon = 1e-7;
        for x in [0.0, 200.0] {
            let left = Point::new(x - epsilon, 5.0);
            let right = Point::new(x + epsilon, 5.0);
            let (gl, hl) = field.derivatives(left);
            let (gr, hr) = field.derivatives(right);
            assert!((field.value(left) - field.value(right)).abs() < 1e-10);
            assert!((gl - gr).hypot() < 1e-8);
            // Along the segment, the field has no longitudinal curvature;
            // the circular endcap has nonzero longitudinal curvature.
            let (cap, interior) = if x == 0.0 { (hl, hr) } else { (hr, hl) };
            assert!(cap[0] < -0.01);
            assert!(interior[0].abs() < 1e-14);
            assert!((hl[2] - hr[2]).abs() < 1e-10);
        }
    }

    #[test]
    fn coincident_link_endpoints_are_a_finite_circle() {
        let mut group = linked_group();
        group.balls[1].x = 0.0;
        group.balls[1].y = 0.0;
        let field = PreparedField::new(&group);
        assert!((field.value(Point::new(10.0, 0.0)) - group.threshold).abs() < 1e-12);
        let (gradient, hessian) = field.derivatives(Point::new(5.0, 3.0));
        assert!(gradient.is_finite());
        assert!(hessian.iter().all(|value| value.is_finite()));
    }
}
