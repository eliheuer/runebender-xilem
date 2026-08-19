// Ported from runebender-xilem/src/path/segment.rs (Apache-2.0).

//! Path segments (lines and curves) for hit-testing and subdivision.

use kurbo::{CubicBez, Line, ParamCurve, ParamCurveNearest, Point, QuadBez};

/// A segment of a path (line, quadratic, or cubic bezier curve).
#[derive(Debug, Clone, Copy)]
pub enum Segment {
    Line(Line),
    Quadratic(QuadBez),
    Cubic(CubicBez),
}

/// Information about a segment within a path.
#[derive(Debug, Clone, Copy)]
pub struct SegmentInfo {
    pub segment: Segment,
    pub start_index: usize,
    pub end_index: usize,
    /// The index of the path within `session.paths` that owns this
    /// segment. Used to disambiguate segments with identical local
    /// indices across different contours.
    pub path_index: usize,
}

impl Segment {
    /// Find the nearest point on this segment to the given point.
    ///
    /// Returns `(t, distance_squared)`:
    /// - `t`: A value from 0.0 to 1.0 along the segment.
    /// - `distance_squared`: Squared distance from `point` to the
    ///   nearest point on this segment. Squared (not actual) to avoid
    ///   a sqrt — fine for comparisons and threshold checks.
    pub fn nearest(&self, point: Point) -> (f64, f64) {
        match self {
            Segment::Line(line) => {
                let t = line_nearest_param(*line, point);
                let nearest_pt = line.eval(t);
                let dist_sq = (nearest_pt - point).hypot2();
                (t, dist_sq)
            }
            Segment::Quadratic(quad) => {
                let result = quad.nearest(point, 1e-6);
                (result.t, result.distance_sq)
            }
            Segment::Cubic(cubic) => {
                let result = cubic.nearest(point, 1e-6);
                (result.t, result.distance_sq)
            }
        }
    }

    /// Evaluate the segment at parameter t (0.0 to 1.0).
    pub fn eval(&self, t: f64) -> Point {
        match self {
            Segment::Line(line) => line.eval(t),
            Segment::Quadratic(quad) => quad.eval(t),
            Segment::Cubic(cubic) => cubic.eval(t),
        }
    }

    /// Subdivide a cubic bezier curve at parameter t (de Casteljau).
    /// Returns `(left, right)` halves that exactly reconstruct the
    /// original curve when joined.
    pub fn subdivide_cubic(cubic: CubicBez, t: f64) -> (CubicBez, CubicBez) {
        let p0 = cubic.p0;
        let p1 = cubic.p1;
        let p2 = cubic.p2;
        let p3 = cubic.p3;

        // Level 1: interpolate adjacent control points at t.
        let q0 = p0 + (p1 - p0) * t;
        let q1 = p1 + (p2 - p1) * t;
        let q2 = p2 + (p3 - p2) * t;

        // Level 2.
        let r0 = q0 + (q1 - q0) * t;
        let r1 = q1 + (q2 - q1) * t;

        // Level 3 — the actual point on the curve at t.
        let split_point = r0 + (r1 - r0) * t;

        let left = CubicBez::new(p0, q0, r0, split_point);
        let right = CubicBez::new(split_point, r1, q2, p3);

        (left, right)
    }

    /// Subdivide a quadratic bezier curve at parameter t (de Casteljau).
    pub fn subdivide_quadratic(quad: QuadBez, t: f64) -> (QuadBez, QuadBez) {
        let p0 = quad.p0;
        let p1 = quad.p1;
        let p2 = quad.p2;

        let q0 = p0 + (p1 - p0) * t;
        let q1 = p1 + (p2 - p1) * t;

        let split_point = q0 + (q1 - q0) * t;

        let left = QuadBez::new(p0, q0, split_point);
        let right = QuadBez::new(split_point, q1, p2);

        (left, right)
    }
}

/// Find the parameter t on a line segment nearest to a point. Uses
/// vector projection: project the point onto the line, clamp to [0, 1].
fn line_nearest_param(line: Line, point: Point) -> f64 {
    let p0 = line.p0;
    let p1 = line.p1;

    let line_vec = p1 - p0;
    let pt_vec = point - p0;

    let line_len_sq = line_vec.hypot2();

    // Degenerate (zero-length) line — return start.
    if line_len_sq < 1e-12 {
        return 0.0;
    }

    let dot_product = pt_vec.x * line_vec.x + pt_vec.y * line_vec.y;
    let t = dot_product / line_len_sq;

    t.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- nearest ---

    #[test]
    fn line_nearest_at_midpoint() {
        let seg = Segment::Line(Line::new((0.0, 0.0), (10.0, 0.0)));
        let (t, dist_sq) = seg.nearest(Point::new(5.0, 3.0));
        assert!((t - 0.5).abs() < 1e-6);
        assert!((dist_sq - 9.0).abs() < 1e-6);
    }

    #[test]
    fn line_nearest_clamps_to_endpoint() {
        let seg = Segment::Line(Line::new((0.0, 0.0), (10.0, 0.0)));
        let (t, _) = seg.nearest(Point::new(20.0, 0.0));
        assert!((t - 1.0).abs() < 1e-6);
        let (t, _) = seg.nearest(Point::new(-5.0, 0.0));
        assert!(t.abs() < 1e-6);
    }

    #[test]
    fn cubic_nearest_at_start_and_end() {
        let cubic = CubicBez::new((0.0, 0.0), (1.0, 2.0), (3.0, 2.0), (4.0, 0.0));
        let seg = Segment::Cubic(cubic);
        let (t, dist_sq) = seg.nearest(Point::new(0.0, 0.0));
        assert!(t < 0.01);
        assert!(dist_sq < 1e-6);
        let (t, dist_sq) = seg.nearest(Point::new(4.0, 0.0));
        assert!(t > 0.99);
        assert!(dist_sq < 1e-6);
    }

    // --- eval ---

    #[test]
    fn eval_line_endpoints() {
        let seg = Segment::Line(Line::new((1.0, 2.0), (5.0, 8.0)));
        let start = seg.eval(0.0);
        let end = seg.eval(1.0);
        assert!((start.x - 1.0).abs() < 1e-10);
        assert!((start.y - 2.0).abs() < 1e-10);
        assert!((end.x - 5.0).abs() < 1e-10);
        assert!((end.y - 8.0).abs() < 1e-10);
    }

    #[test]
    fn eval_line_midpoint() {
        let seg = Segment::Line(Line::new((0.0, 0.0), (10.0, 10.0)));
        let mid = seg.eval(0.5);
        assert!((mid.x - 5.0).abs() < 1e-10);
        assert!((mid.y - 5.0).abs() < 1e-10);
    }

    // --- subdivide_cubic ---

    #[test]
    fn subdivide_cubic_at_midpoint() {
        let cubic = CubicBez::new((0.0, 0.0), (1.0, 3.0), (3.0, 3.0), (4.0, 0.0));
        let (left, right) = Segment::subdivide_cubic(cubic, 0.5);

        assert!((left.p0.x - 0.0).abs() < 1e-10);
        assert!((left.p0.y - 0.0).abs() < 1e-10);
        assert!((right.p3.x - 4.0).abs() < 1e-10);
        assert!((right.p3.y - 0.0).abs() < 1e-10);
        assert!((left.p3.x - right.p0.x).abs() < 1e-10);
        assert!((left.p3.y - right.p0.y).abs() < 1e-10);
    }

    #[test]
    fn subdivide_cubic_preserves_curve() {
        let cubic = CubicBez::new((0.0, 0.0), (1.0, 3.0), (3.0, 3.0), (4.0, 0.0));
        let t_split = 0.3;
        let (left, right) = Segment::subdivide_cubic(cubic, t_split);

        for i in 0..=10 {
            let t = i as f64 / 10.0;
            let original_pt = cubic.eval(t);
            let sub_pt = if t <= t_split {
                left.eval(t / t_split)
            } else {
                right.eval((t - t_split) / (1.0 - t_split))
            };
            assert!((original_pt.x - sub_pt.x).abs() < 1e-6);
            assert!((original_pt.y - sub_pt.y).abs() < 1e-6);
        }
    }

    #[test]
    fn subdivide_cubic_at_extremes() {
        let cubic = CubicBez::new((0.0, 0.0), (1.0, 2.0), (3.0, 2.0), (4.0, 0.0));

        let (left, right) = Segment::subdivide_cubic(cubic, 0.0);
        assert!((left.p0.x - left.p3.x).abs() < 1e-10);
        assert!((right.p3.x - 4.0).abs() < 1e-10);

        let (left, right) = Segment::subdivide_cubic(cubic, 1.0);
        assert!((left.p0.x - 0.0).abs() < 1e-10);
        assert!((right.p0.x - right.p3.x).abs() < 1e-10);
    }

    // --- subdivide_quadratic ---

    #[test]
    fn subdivide_quadratic_preserves_curve() {
        let quad = QuadBez::new((0.0, 0.0), (2.0, 4.0), (4.0, 0.0));
        let t_split = 0.4;
        let (left, right) = Segment::subdivide_quadratic(quad, t_split);

        assert!((left.p2.x - right.p0.x).abs() < 1e-10);
        assert!((left.p2.y - right.p0.y).abs() < 1e-10);

        for i in 0..=10 {
            let t = i as f64 / 10.0;
            let original_pt = quad.eval(t);
            let sub_pt = if t <= t_split {
                left.eval(t / t_split)
            } else {
                right.eval((t - t_split) / (1.0 - t_split))
            };
            assert!((original_pt.x - sub_pt.x).abs() < 1e-6);
            assert!((original_pt.y - sub_pt.y).abs() < 1e-6);
        }
    }

    // --- line_nearest_param ---

    #[test]
    fn line_nearest_param_degenerate() {
        let line = Line::new((5.0, 5.0), (5.0, 5.0));
        let t = line_nearest_param(line, Point::new(10.0, 10.0));
        assert!((t - 0.0).abs() < 1e-10);
    }
}
