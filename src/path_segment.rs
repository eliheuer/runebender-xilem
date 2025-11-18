// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Path segments (lines and curves) for hit-testing and subdivision

use kurbo::{
    CubicBez, Line, ParamCurve, ParamCurveNearest, Point, QuadBez,
};

/// A segment of a path (line, quadratic, or cubic bezier curve)
#[derive(Debug, Clone, Copy)]
pub enum Segment {
    /// A line segment
    Line(Line),
    /// A quadratic bezier curve segment
    Quadratic(QuadBez),
    /// A cubic bezier curve segment
    Cubic(CubicBez),
}

/// Information about a segment within a path
#[derive(Debug, Clone, Copy)]
pub struct SegmentInfo {
    /// The geometric segment
    pub segment: Segment,
    /// The index of the starting point in the path
    pub start_index: usize,
    /// The index of the ending point in the path
    pub end_index: usize,
}

impl Segment {
    /// Find the nearest point on this segment to the given point
    ///
    /// Returns `(t, distance_squared)`:
    /// - `t`: A value from 0.0 to 1.0 indicating where along the
    ///   segment the nearest point is (0.0 = start, 1.0 = end)
    /// - `distance_squared`: The squared distance from the given point
    ///   to the nearest point on this segment.
    ///
    /// Note: We return the squared distance instead of the actual
    /// distance to avoid computing a square root. For comparisons and
    /// threshold checks (e.g., "is this closer than X?"), comparing
    /// squared distances is mathematically equivalent and faster.
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

    /// Evaluate the segment at parameter t (0.0 to 1.0)
    ///
    /// Returns the point on this segment at the given parametric
    /// position. Used for converting parametric coordinates (like from
    /// hit-testing) into actual point positions on the segment.
    pub fn eval(&self, t: f64) -> Point {
        match self {
            Segment::Line(line) => line.eval(t),
            Segment::Quadratic(quad) => quad.eval(t),
            Segment::Cubic(cubic) => cubic.eval(t),
        }
    }

    /// Subdivide a cubic bezier curve at a value between 0.0 and 1.0.
    ///
    /// The parameter `t` (provided by the caller, typically from
    /// hit-testing) ranges from 0.0 (start of curve) to 1.0 (end of
    /// curve) and determines where the curve is split.
    ///
    /// This uses the de Casteljau algorithm for numerically stable
    /// subdivision. The two returned curves are exactly the same as
    /// the original curve, just split at the given parameter `t`.
    /// The algorithm subdivides a bezier curve by repeatedly
    /// interpolating between control points using the same parameter
    /// `t` at each level.
    ///
    /// ## How it works:
    ///
    /// Given a cubic bezier with control points P0, P1, P2, P3, we
    /// want to split it at parameter `t` (where 0 ≤ t ≤ 1) into two
    /// curves that together form the original.
    ///
    /// The algorithm builds a "pyramid" by repeatedly interpolating
    /// between adjacent points using parameter `t`:
    ///
    /// ```text
    /// Level 0 (control points):      P0      P1    P2     P3
    ///                                  \    /  \  /  \   /
    /// Level 1 (interpolate at t):       Q0      Q1     Q2
    ///                                     \   /  \    /
    /// Level 2 (interpolate at t):          R0      R1
    ///                                        \    /
    /// Level 3 (interpolate at t):         split_point
    /// ```
    ///
    /// The left edge of the pyramid (P0 → Q0 → R0 → split_point)
    /// gives us the control points for the first half of the curve.
    /// This new curve matches the original curve from the start up to
    /// the split point.
    ///
    /// The right edge (split_point → R1 → Q2 → P3) gives us the
    /// control points for the second half of the curve. This new
    /// curve matches the original curve from the split point to the
    /// end.
    ///
    /// These two new curves, when combined, are
    /// exactly identical to the original curve. No distortion, no
    /// approximation - just a perfect split!
    ///
    /// When a user clicks on a curve to add a point, we need to
    /// insert that point without changing the curve's shape. The de
    /// Casteljau algorithm gives us the exact control points needed
    /// to maintain the curve perfectly.
    pub fn subdivide_cubic(
        cubic: CubicBez,
        t: f64,
    ) -> (CubicBez, CubicBez) {
        // Extract the original curve's control points
        // P0 is the start point, P3 is the end point
        // P1 and P2 are the off-curve control points
        let p0 = cubic.p0;
        let p1 = cubic.p1;
        let p2 = cubic.p2;
        let p3 = cubic.p3;

        // Level 1: Linearly interpolate between adjacent control
        // points. This creates three new points at parameter t along
        // the three line segments:
        // - Q0 is at position t along the line from P0 to P1
        // - Q1 is at position t along the line from P1 to P2
        // - Q2 is at position t along the line from P2 to P3
        let q0 = p0 + (p1 - p0) * t;
        let q1 = p1 + (p2 - p1) * t;
        let q2 = p2 + (p3 - p2) * t;

        // Level 2: Interpolate between the Level 1 points.
        // This creates two new points:
        // - R0 is at position t along the line from Q0 to Q1
        // - R1 is at position t along the line from Q1 to Q2
        let r0 = q0 + (q1 - q0) * t;
        let r1 = q1 + (q2 - q1) * t;

        // Level 3: Final interpolation to find the split point.
        // The split point is at position t along the line from R0
        // to R1. This is the actual point on the curve at parameter
        // t.
        let split_point = r0 + (r1 - r0) * t;

        // Construct the left subcurve from the left edge of the
        // pyramid. This curve goes from the original start (P0) to
        // the split point with control points Q0 and R0.
        let left = CubicBez::new(p0, q0, r0, split_point);

        // Construct the right subcurve from the right edge of the
        // pyramid. This curve goes from the split point to the
        // original end (P3) with control points R1 and Q2.
        let right = CubicBez::new(split_point, r1, q2, p3);

        (left, right)
    }

    /// Subdivide a quadratic bezier curve at a value between 0.0 and 1.0.
    ///
    /// The parameter `t` (provided by the caller, typically from
    /// hit-testing) ranges from 0.0 (start of curve) to 1.0 (end of
    /// curve) and determines where the curve is split.
    ///
    /// This uses the de Casteljau algorithm for numerically stable
    /// subdivision. The two returned curves are exactly the same as
    /// the original curve, just split at the given parameter `t`.
    /// The algorithm subdivides a bezier curve by repeatedly
    /// interpolating between control points using the same parameter
    /// `t` at each level.
    ///
    /// ## How it works:
    ///
    /// Given a quadratic bezier with control points P0, P1, P2, we
    /// want to split it at parameter `t` (where 0 ≤ t ≤ 1) into two
    /// curves that together form the original.
    ///
    /// The algorithm builds a "pyramid" by repeatedly interpolating
    /// between adjacent points using parameter `t`:
    ///
    /// ```text
    /// Level 0 (control points):      P0      P1     P2
    ///                                  \    /  \   /
    /// Level 1 (interpolate at t):       Q0      Q1
    ///                                     \    /
    /// Level 2 (interpolate at t):      split_point
    /// ```
    ///
    /// The left edge of the pyramid (P0 → Q0 → split_point) gives us
    /// the control points for the first half of the curve. This new
    /// curve matches the original curve from the start up to the split
    /// point.
    ///
    /// The right edge (split_point → Q1 → P2) gives us the control
    /// points for the second half of the curve. This new curve matches
    /// the original curve from the split point to the end.
    ///
    /// These two new curves, when combined, are exactly identical to
    /// the original curve. No distortion, no approximation - just a
    /// perfect split!
    ///
    /// When a user clicks on a curve to add a point, we need to insert
    /// that point without changing the curve's shape. The de Casteljau
    /// algorithm gives us the exact control points needed to maintain
    /// the curve perfectly.
    pub fn subdivide_quadratic(
        quad: QuadBez,
        t: f64,
    ) -> (QuadBez, QuadBez) {
        let p0 = quad.p0;
        let p1 = quad.p1;
        let p2 = quad.p2;

        // Level 1: Linearly interpolate between adjacent control
        // points
        let q0 = p0 + (p1 - p0) * t;
        let q1 = p1 + (p2 - p1) * t;

        // Level 2: Final interpolation to find the split point
        let split_point = q0 + (q1 - q0) * t;

        // Construct the left subcurve
        let left = QuadBez::new(p0, q0, split_point);

        // Construct the right subcurve
        let right = QuadBez::new(split_point, q1, p2);

        (left, right)
    }
}

/// Find the parameter t on a line segment nearest to a point
///
/// This function finds where along a line segment (from t=0.0 at the
/// start to t=1.0 at the end) the closest point to the query point is
/// located.
///
/// Uses vector projection: we project the point onto the line and find
/// where along the line that projection lands.
fn line_nearest_param(line: Line, point: Point) -> f64 {
    // Get the two endpoints of the line segment
    let p0 = line.p0;
    let p1 = line.p1;

    // Create vectors:
    // - line_vec: points from p0 to p1 (the direction of our line)
    // - pt_vec: points from p0 to the query point
    let line_vec = p1 - p0;
    let pt_vec = point - p0;

    // Calculate the squared length of the line segment
    // (we use squared length to avoid a square root operation)
    let line_len_sq = line_vec.hypot2();

    // Check if the line segment is degenerate (zero length or
    // effectively zero length due to floating-point precision)
    // If so, just return 0.0 (the start point)
    if line_len_sq < 1e-12 {
        return 0.0;
    }

    // Project pt_vec onto line_vec using the dot product
    // The dot product tells us how much of pt_vec points in the
    // same direction as line_vec
    let dot_product = pt_vec.x * line_vec.x + pt_vec.y * line_vec.y;

    // Divide by the squared length to get the parameter t
    // This tells us where along the line (from 0.0 to 1.0) the
    // projection lands. If the result is outside [0, 1], it means
    // the nearest point is actually one of the endpoints.
    let t = dot_product / line_len_sq;

    // Clamp t to [0, 1] to ensure we're on the segment
    // (not before the start or after the end)
    t.clamp(0.0, 1.0)
}
