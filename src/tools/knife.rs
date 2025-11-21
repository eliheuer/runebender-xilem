// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! The knife tool for cutting paths
//!
//! Ported from Runebender Druid implementation.
//!
//! ## Known Issues
//!
//! ### BUG: Cubic path splitting corrupts unrelated off-curve points
//!
//! **Status**: Unresolved (as of 2025-01-19)
//!
//! **Symptoms**:
//! - When cutting a closed cubic path with the knife tool, off-curve control points
//!   in parts of the outline NOT involved in the cut are sometimes removed or corrupted
//! - This creates radiating lines and distorted curves in areas far from the cut
//! - The bug ONLY occurs with cubic curves - paths with only line segments work correctly
//!
//! **Example**:
//! - Cutting through the upper-left corner of a glyph outline causes distortion
//!   in the lower-right corner
//! - Debug logs show duplicate on-curve points being added to the split paths
//!
//! **Root Cause Investigation**:
//! The issue appears related to "wraparound cubic segments" in closed paths:
//! - In closed paths, the last cubic segment wraps back to the first point
//! - These segments have start_index == end_index
//! - The control points for wraparound segments are at indices 0 and 1 (before start_index)
//! - Multiple attempts to fix duplicate point handling have not resolved the issue
//!
//! **What Works**:
//! - Cutting paths composed only of line segments (no curves)
//! - The knife tool correctly identifies intersection points
//! - Visual preview (green X marks at intersections) works correctly
//!
//! **Attempted Fixes**:
//! 1. Extracting wraparound control points from cubic geometry instead of array indices
//! 2. Adding duplicate detection for end points
//! 3. Adding closing point explicitly for wraparound segments
//! 4. NOT adding closing point and relying on next segment to add it
//! 5. Various permutations of the above
//!
//! **Next Steps for Future Investigation**:
//! - Consider rewriting split_path_at_line() to use a different algorithm
//! - Perhaps build new paths by evaluating segments directly rather than copying points
//! - Look at how the original Runebender Druid implementation handled this
//! - Add comprehensive unit tests for wraparound segment handling
//! - Consider whether the segment iterator itself is producing incorrect wraparound segments

use crate::cubic_path::CubicPath;
use crate::edit_session::EditSession;
use crate::edit_types::EditType;
use crate::entity_id::EntityId;
use crate::mouse::{Drag, MouseDelegate, MouseEvent};
use crate::path::Path;
use crate::path_segment::{Segment, SegmentInfo};
use crate::point::{PathPoint, PointType};
use crate::point_list::PathPoints;
use crate::tools::{Tool, ToolId};
use kurbo::{Affine, CubicBez, Line, ParamCurve, ParamCurveArclen, Point};
use masonry::vello::kurbo::Stroke;
use masonry::vello::Scene;
use std::sync::Arc;

/// Maximum recursion depth for slicing paths
const MAX_RECURSE: usize = 16;

/// The knife tool for cutting paths
#[derive(Debug, Clone)]
pub struct KnifeTool {
    /// Current gesture state
    gesture: GestureState,
    /// Whether shift-lock is active (constrain to H/V)
    shift_locked: bool,
    /// Cached intersection points (design space)
    intersections: Vec<Point>,
}

/// The state of the knife gesture
#[derive(Debug, Clone, Copy, PartialEq)]
enum GestureState {
    /// Ready for a new cut
    Ready,
    /// Currently cutting - points are in design space
    Begun { start: Point, current: Point },
    /// Cut completed
    Finished,
}

/// A hit point where the knife intersects a path
#[derive(Clone, Copy, Debug)]
struct Hit {
    /// Parametric position along the knife line (0.0 to 1.0)
    line_t: f64,
    /// Parametric position along the segment (0.0 to 1.0)
    segment_t: f64,
    /// The intersection point in design space
    point: Point,
    /// Information about the intersected segment
    segment_info: SegmentInfo,
}

impl Default for KnifeTool {
    fn default() -> Self {
        Self {
            gesture: GestureState::Ready,
            shift_locked: false,
            intersections: Vec::new(),
        }
    }
}

impl Default for GestureState {
    fn default() -> Self {
        GestureState::Ready
    }
}

impl KnifeTool {
    /// Get the current line endpoints, applying shift-lock if active
    fn current_points(&self) -> Option<(Point, Point)> {
        if let GestureState::Begun { start, current } = self.gesture {
            let mut current = current;
            if self.shift_locked {
                let delta = current - start;
                if delta.x.abs() > delta.y.abs() {
                    current.y = start.y;
                } else {
                    current.x = start.x;
                }
            }
            Some((start, current))
        } else {
            None
        }
    }

    /// Get the current knife line in design space
    fn current_line(&self) -> Option<Line> {
        self.current_points().map(|(p1, p2)| Line::new(p1, p2))
    }

    /// Update the cached intersection points
    fn update_intersections(&mut self, data: &EditSession) {
        let line = match self.current_line() {
            Some(line) => line,
            None => return,
        };

        self.intersections.clear();

        for path in data.paths.iter() {
            match path {
                Path::Cubic(cubic_path) => {
                    for seg_info in cubic_path.iter_segments() {
                        let hits = intersect_line_segment(line, &seg_info.segment);
                        for (_seg_t, line_t) in hits {
                            let point = line.eval(line_t);
                            self.intersections.push(point);
                        }
                    }
                }
                _ => {
                    // Skip non-cubic paths
                }
            }
        }
    }
}

// ===== Tool Implementation =====

impl Tool for KnifeTool {
    fn id(&self) -> ToolId {
        ToolId::Knife
    }

    fn paint(
        &mut self,
        scene: &mut Scene,
        session: &EditSession,
        _transform: &Affine,
    ) {
        use crate::theme::tool_preview;

        if let Some((start, current)) = self.current_points() {
            // Convert to screen space
            let screen_start = session.viewport.to_screen(start);
            let screen_current = session.viewport.to_screen(current);

            // Draw knife line (dashed) - consistent with pen tool preview
            let line = Line::new(screen_start, screen_current);
            let stroke = Stroke::new(tool_preview::LINE_WIDTH).with_dashes(
                tool_preview::LINE_DASH_OFFSET,
                tool_preview::LINE_DASH,
            );
            scene.stroke(
                &stroke,
                Affine::IDENTITY,
                tool_preview::LINE_COLOR,
                None,
                &line,
            );

            // Draw intersection markers as green X marks (matching corner points)
            for &intersection in &self.intersections {
                let screen_pt = session.viewport.to_screen(intersection);
                let mark_size = 6.0;
                let green = crate::theme::point::CORNER_INNER;
                let mark_stroke = Stroke::new(3.0);

                // Draw X (two diagonal lines)
                let x1 = Line::new(
                    kurbo::Point::new(screen_pt.x - mark_size, screen_pt.y - mark_size),
                    kurbo::Point::new(screen_pt.x + mark_size, screen_pt.y + mark_size),
                );
                let x2 = Line::new(
                    kurbo::Point::new(screen_pt.x - mark_size, screen_pt.y + mark_size),
                    kurbo::Point::new(screen_pt.x + mark_size, screen_pt.y - mark_size),
                );

                scene.stroke(
                    &mark_stroke,
                    Affine::IDENTITY,
                    green,
                    None,
                    &x1,
                );
                scene.stroke(
                    &mark_stroke,
                    Affine::IDENTITY,
                    green,
                    None,
                    &x2,
                );
            }
        }
    }

    fn edit_type(&self) -> Option<EditType> {
        if self.gesture == GestureState::Finished {
            Some(EditType::Normal)
        } else {
            None
        }
    }
}

// ===== MouseDelegate Implementation =====

impl MouseDelegate for KnifeTool {
    type Data = EditSession;

    fn left_down(&mut self, event: MouseEvent, data: &mut EditSession) {
        let pt = data.viewport.screen_to_design(event.pos);
        self.gesture = GestureState::Begun {
            start: pt,
            current: pt,
        };
        self.shift_locked = event.mods.shift;
    }

    fn left_drag_began(
        &mut self,
        event: MouseEvent,
        drag: Drag,
        data: &mut EditSession,
    ) {
        let start = data.viewport.screen_to_design(drag.start);
        let current = data.viewport.screen_to_design(drag.current);
        self.gesture = GestureState::Begun { start, current };
        self.shift_locked = event.mods.shift;
        self.update_intersections(data);
    }

    fn left_drag_changed(
        &mut self,
        _event: MouseEvent,
        drag: Drag,
        data: &mut EditSession,
    ) {
        if let GestureState::Begun { current, .. } = &mut self.gesture {
            *current = data.viewport.screen_to_design(drag.current);
            self.update_intersections(data);
        }
    }

    fn left_drag_ended(
        &mut self,
        _event: MouseEvent,
        drag: Drag,
        data: &mut EditSession,
    ) {
        if let GestureState::Begun { current, .. } = &mut self.gesture {
            let now = data.viewport.screen_to_design(drag.current);
            if now != *current {
                *current = now;
                self.update_intersections(data);
            }
        }

        if let Some(line) = self.current_line() {
            if !self.intersections.is_empty() {
                let new_paths = slice_paths(&data.paths, line);
                let paths_vec = Arc::make_mut(&mut data.paths);
                paths_vec.clear();
                paths_vec.extend(new_paths);
            }
        }

        self.gesture = GestureState::Finished;
    }

    fn cancel(&mut self, _data: &mut EditSession) {
        self.gesture = GestureState::Ready;
        self.intersections.clear();
    }
}

// ===== Path Slicing Algorithm =====

/// Slice all paths with a knife line
///
/// Checks for intersection with all paths, modifying old and adding
/// new paths as necessary.
fn slice_paths(paths: &[Path], line: Line) -> Vec<Path> {
    let mut out = Vec::new();
    for path in paths {
        match path {
            Path::Cubic(cubic_path) => {
                slice_path(cubic_path, line, &mut out);
            }
            _ => {
                // HyperPath and QuadraticPath not yet supported
                out.push(path.clone());
            }
        }
    }
    out
}

/// Slice a single cubic path with a line
///
/// Resulting paths are pushed to the `acc` vec.
fn slice_path(path: &CubicPath, line: Line, acc: &mut Vec<Path>) {
    let mut hits = Vec::new();
    slice_path_impl(path.clone(), line, acc, &mut hits, 0);
}

/// Recursive implementation of path slicing
///
/// The algorithm:
/// - Find all intersections with the line
/// - Take the first two hits (sorted by position on line)
/// - Split the path at those two points
/// - Recursively slice each new path with the remaining line
fn slice_path_impl(
    path: CubicPath,
    line: Line,
    acc: &mut Vec<Path>,
    hit_buf: &mut Vec<Hit>,
    recurse: usize,
) {
    // Find all intersections
    hit_buf.clear();
    for seg_info in path.iter_segments() {
        let hits = intersect_line_segment(line, &seg_info.segment);
        for (seg_t, line_t) in hits {
            let point = line.eval(line_t);
            hit_buf.push(Hit {
                line_t,
                segment_t: seg_t,
                point,
                segment_info: seg_info.clone(),
            });
        }
    }

    // Base case: 0 or 1 intersections, or hit recursion limit
    if hit_buf.len() <= 1 || recurse == MAX_RECURSE {
        if recurse == MAX_RECURSE {
            tracing::debug!("slice_path hit recursion limit");
        }
        acc.push(Path::Cubic(path));
        return;
    }

    // Sort hits by position along the knife line
    hit_buf.sort_by(|a, b| a.line_t.partial_cmp(&b.line_t).unwrap());

    // Take the first two intersections
    let start = hit_buf[0];
    let end = hit_buf[1];

    // Calculate where to resume cutting on the line
    // Add a small epsilon to avoid re-cutting at the same point
    let slice_ep = 1.0 / line.arclen(1e-6).max(1.0);
    let next_line_start_t = (end.line_t + slice_ep).min(1.0);

    // Order points based on their position in the path
    let (start, end) = order_points(&path, start, end);

    // Split the path at the two intersection points
    let (path_one, path_two) = split_path_at_intersections(&path, start, end);

    // Calculate the remaining line to process
    if next_line_start_t >= 1.0 {
        // No more line to process
        acc.push(Path::Cubic(path_one));
        acc.push(Path::Cubic(path_two));
        return;
    }

    let remaining_line = line_subsegment(line, next_line_start_t, 1.0);

    // Recursively slice each new path
    slice_path_impl(path_one, remaining_line, acc, hit_buf, recurse + 1);
    slice_path_impl(path_two, remaining_line, acc, hit_buf, recurse + 1);
}

/// Order two hit points based on their position in the path
///
/// This ensures we hit the start point first when iterating through segments.
fn order_points(path: &CubicPath, start: Hit, end: Hit) -> (Hit, Hit) {
    for seg_info in path.iter_segments() {
        if seg_info.start_index == start.segment_info.start_index {
            // Special case: both cuts in same segment
            if seg_info.start_index == end.segment_info.start_index
                && end.segment_t < start.segment_t
            {
                return (end, start);
            }
            return (start, end);
        } else if seg_info.start_index == end.segment_info.start_index {
            return (end, start);
        }
    }
    // Fallback
    (start, end)
}

/// Split a path at two intersection points
///
/// Returns two new closed paths.
fn split_path_at_intersections(
    path: &CubicPath,
    start: Hit,
    end: Hit,
) -> (CubicPath, CubicPath) {
    let mut one_points: Vec<PathPoint> = Vec::new();
    let mut two_points: Vec<PathPoint> = Vec::new();
    let mut two_is_done = false;

    let points: Vec<PathPoint> = path.points.iter().cloned().collect();
    let segments: Vec<SegmentInfo> = path.iter_segments().collect();

    // Phase 1: Copy points up to the first cut
    for seg_info in &segments {
        if seg_info.start_index != start.segment_info.start_index {
            // Copy all points of this segment
            append_segment_points(&mut one_points, &points, seg_info);
        } else {
            // This segment contains the first cut
            let cut_t = start.segment_t;

            // Add points up to the cut
            append_subsegment_points(
                &mut one_points,
                &points,
                seg_info,
                0.0,
                cut_t,
            );

            // Check if both cuts are in the same segment
            if seg_info.start_index == end.segment_info.start_index {
                // Add the part after the second cut to path one
                append_subsegment_points(
                    &mut one_points,
                    &points,
                    seg_info,
                    end.segment_t,
                    1.0,
                );

                // Add the part between cuts to path two
                append_subsegment_points(
                    &mut two_points,
                    &points,
                    seg_info,
                    cut_t,
                    end.segment_t,
                );
                two_is_done = true;
            } else {
                // Add the part after the first cut to path two
                append_subsegment_points(
                    &mut two_points,
                    &points,
                    seg_info,
                    cut_t,
                    1.0,
                );
            }

            // Add closing line point for path two
            if !path.closed {
                two_points.push(PathPoint {
                    id: EntityId::next(),
                    point: start.point,
                    typ: PointType::OnCurve { smooth: false },
                });
            }
            break;
        }
    }

    // Phase 2: Process segments between the cuts
    let mut found_start = false;
    for seg_info in &segments {
        if seg_info.start_index == start.segment_info.start_index {
            found_start = true;
            continue;
        }
        if !found_start {
            continue;
        }

        if seg_info.start_index == end.segment_info.start_index {
            // This segment contains the second cut
            let cut_t = end.segment_t;

            // Add the part after the cut to path one
            append_subsegment_points(
                &mut one_points,
                &points,
                seg_info,
                cut_t,
                1.0,
            );

            // Add the part before the cut to path two
            if !two_is_done {
                append_subsegment_points(
                    &mut two_points,
                    &points,
                    seg_info,
                    0.0,
                    cut_t,
                );
            }
            break;
        } else if !two_is_done {
            // This segment is between the cuts - add to path two
            append_segment_points(&mut two_points, &points, seg_info);
        }
    }

    // Phase 3: Copy remaining segments to path one
    let mut found_end = false;
    for seg_info in &segments {
        if seg_info.start_index == end.segment_info.start_index {
            found_end = true;
            continue;
        }
        if found_end {
            append_segment_points(&mut one_points, &points, seg_info);
        }
    }

    // Remove duplicate endpoints for closed paths
    if one_points.first().map(|p| p.point) == one_points.last().map(|p| p.point)
        && one_points.len() > 1
    {
        tracing::debug!("Removing duplicate endpoint from path one");
        one_points.pop();
    }

    tracing::debug!("Path 1 has {} points", one_points.len());
    tracing::debug!("Path 2 has {} points", two_points.len());

    // Log the points for debugging
    for (i, pt) in one_points.iter().enumerate() {
        tracing::debug!("  Path1[{}]: {:?} {:?}", i, pt.point, pt.typ);
    }
    for (i, pt) in two_points.iter().enumerate() {
        tracing::debug!("  Path2[{}]: {:?} {:?}", i, pt.point, pt.typ);
    }

    // Create the new paths
    let path1 = CubicPath::new(PathPoints::from_vec(one_points), path.closed);
    let path2 = CubicPath::new(PathPoints::from_vec(two_points), true);

    (path1, path2)
}

/// Append all points of a segment to the destination
///
/// WARNING: This function has a known bug with wraparound cubic segments.
/// See module-level documentation for details. The wraparound handling below
/// is an attempted fix that does not fully resolve the issue. When cutting
/// closed cubic paths, this can corrupt off-curve points in unrelated parts
/// of the outline.
fn append_segment_points(
    dest: &mut Vec<PathPoint>,
    points: &[PathPoint],
    seg_info: &SegmentInfo,
) {
    let start = seg_info.start_index;
    let end = seg_info.end_index;

    tracing::debug!(
        "append_segment_points: start={}, end={}, segment={:?}",
        start,
        end,
        seg_info.segment
    );

    // BUGGY: Handle wraparound cases first (start == end)
    // This code attempts to handle wraparound cubic segments in closed paths,
    // but still causes corruption. See module-level documentation.
    // For wraparound cases, we need to extract from the cubic geometry
    if start == end && matches!(seg_info.segment, Segment::Cubic(_)) {
        // Wraparound case: extract control points from geometry
        if let Segment::Cubic(cubic) = &seg_info.segment {
            tracing::debug!(
                "  Wraparound cubic detected - extracting all points from geometry"
            );

            // Add start point if not duplicate
            if dest.last().map(|p| p.point) != Some(cubic.p0) {
                tracing::debug!(
                    "  Adding wraparound start point: {:?} type={:?}",
                    cubic.p0,
                    points[start].typ
                );
                dest.push(PathPoint {
                    id: EntityId::next(),
                    point: cubic.p0,
                    typ: points[start].typ,
                });
            } else {
                tracing::debug!("  Skipping duplicate wraparound start point");
            }

            // Add control points
            tracing::debug!(
                "  Adding control points: p1={:?}, p2={:?}",
                cubic.p1,
                cubic.p2
            );
            dest.push(PathPoint {
                id: EntityId::next(),
                point: cubic.p1,
                typ: PointType::OffCurve { auto: false },
            });
            dest.push(PathPoint {
                id: EntityId::next(),
                point: cubic.p2,
                typ: PointType::OffCurve { auto: false },
            });

            // Don't add closing point - it's the same as start and will be added by next segment
            tracing::debug!(
                "  Wraparound complete - NOT adding closing point (same as start)"
            );
            return; // Early return
        }
    }

    // Normal case: add start point if not duplicate
    if dest.last().map(|p| p.point) != Some(points[start].point) {
        tracing::debug!(
            "  Adding start point [{}]: {:?} type={:?}",
            start,
            points[start].point,
            points[start].typ
        );
        dest.push(PathPoint {
            id: EntityId::next(),
            point: points[start].point,
            typ: points[start].typ,
        });
    } else {
        tracing::debug!("  Skipping duplicate start point");
    }

    // Normal case: copy points between start and end
    for i in (start + 1)..end {
        if i < points.len() {
            tracing::debug!(
                "  Adding intermediate point [{}]: {:?} type={:?}",
                i,
                points[i].point,
                points[i].typ
            );
            dest.push(PathPoint {
                id: EntityId::next(),
                point: points[i].point,
                typ: points[i].typ,
            });
        }
    }

    // Add end point (skip if it's the same as start - wraparound case)
    if end < points.len() && end != start {
        tracing::debug!(
            "  Adding end point [{}]: {:?} type={:?}",
            end,
            points[end].point,
            points[end].typ
        );
        dest.push(PathPoint {
            id: EntityId::next(),
            point: points[end].point,
            typ: points[end].typ,
        });
    } else if end == start {
        tracing::debug!("  Skipping end point (same as start - wraparound)");
    }
}

/// Append a subsegment (portion of a segment) to the destination
fn append_subsegment_points(
    dest: &mut Vec<PathPoint>,
    _points: &[PathPoint],
    seg_info: &SegmentInfo,
    t_start: f64,
    t_end: f64,
) {
    if t_start >= t_end {
        return;
    }

    match &seg_info.segment {
        Segment::Line(line) => {
            // For lines, just add the endpoints
            let p_start = line.eval(t_start);
            let p_end = line.eval(t_end);

            if dest.last().map(|p| p.point) != Some(p_start) {
                dest.push(PathPoint {
                    id: EntityId::next(),
                    point: p_start,
                    typ: PointType::OnCurve { smooth: false },
                });
            }
            dest.push(PathPoint {
                id: EntityId::next(),
                point: p_end,
                typ: PointType::OnCurve { smooth: false },
            });
        }
        Segment::Cubic(cubic) => {
            // For cubic beziers, use de Casteljau to get the subsegment
            let sub = cubic_subsegment(*cubic, t_start, t_end);

            if dest.last().map(|p| p.point) != Some(sub.p0) {
                dest.push(PathPoint {
                    id: EntityId::next(),
                    point: sub.p0,
                    typ: PointType::OnCurve { smooth: false },
                });
            }
            dest.push(PathPoint {
                id: EntityId::next(),
                point: sub.p1,
                typ: PointType::OffCurve { auto: false },
            });
            dest.push(PathPoint {
                id: EntityId::next(),
                point: sub.p2,
                typ: PointType::OffCurve { auto: false },
            });
            dest.push(PathPoint {
                id: EntityId::next(),
                point: sub.p3,
                typ: PointType::OnCurve { smooth: false },
            });
        }
        Segment::Quadratic(quad) => {
            // Convert quadratic to cubic and handle similarly
            let cubic = quad.raise();
            let sub = cubic_subsegment(cubic, t_start, t_end);

            if dest.last().map(|p| p.point) != Some(sub.p0) {
                dest.push(PathPoint {
                    id: EntityId::next(),
                    point: sub.p0,
                    typ: PointType::OnCurve { smooth: false },
                });
            }
            dest.push(PathPoint {
                id: EntityId::next(),
                point: sub.p1,
                typ: PointType::OffCurve { auto: false },
            });
            dest.push(PathPoint {
                id: EntityId::next(),
                point: sub.p2,
                typ: PointType::OffCurve { auto: false },
            });
            dest.push(PathPoint {
                id: EntityId::next(),
                point: sub.p3,
                typ: PointType::OnCurve { smooth: false },
            });
        }
    }
}

// ===== Math Utilities =====

/// Get a subsegment of a line
fn line_subsegment(line: Line, t_start: f64, t_end: f64) -> Line {
    Line::new(line.eval(t_start), line.eval(t_end))
}

/// Get a subsegment of a cubic bezier
fn cubic_subsegment(cubic: CubicBez, t_start: f64, t_end: f64) -> CubicBez {
    // First split at t_start
    let (_, right) = Segment::subdivide_cubic(cubic, t_start);

    // Adjust t_end for the new curve
    let adjusted_t = if t_start < 1.0 {
        (t_end - t_start) / (1.0 - t_start)
    } else {
        1.0
    };

    // Then split at adjusted t_end
    let (left, _) = Segment::subdivide_cubic(right, adjusted_t.min(1.0));

    left
}

/// Find intersections between a line and a segment
/// Returns Vec of (segment_t, line_t) pairs
pub(crate) fn intersect_line_segment(line: Line, segment: &Segment) -> Vec<(f64, f64)> {
    match segment {
        Segment::Line(seg_line) => intersect_line_line(line, *seg_line),
        Segment::Cubic(cubic) => intersect_line_cubic(line, *cubic),
        Segment::Quadratic(quad) => {
            let cubic = quad.raise();
            intersect_line_cubic(line, cubic)
        }
    }
}

/// Find intersection between two lines
fn intersect_line_line(knife: Line, segment: Line) -> Vec<(f64, f64)> {
    let d1 = knife.p1 - knife.p0;
    let d2 = segment.p1 - segment.p0;
    let cross = d1.x * d2.y - d1.y * d2.x;

    const EPSILON: f64 = 1e-9;

    // Lines are parallel
    if cross.abs() < EPSILON {
        return Vec::new();
    }

    let d = segment.p0 - knife.p0;
    let t1 = (d.x * d2.y - d.y * d2.x) / cross;
    let t2 = (d.x * d1.y - d.y * d1.x) / cross;

    // Check if intersection is within both segments
    if (0.0..=1.0).contains(&t1) && (0.0..=1.0).contains(&t2) {
        vec![(t2, t1)]
    } else {
        Vec::new()
    }
}

/// Find intersections between a line and a cubic bezier
fn intersect_line_cubic(knife: Line, cubic: CubicBez) -> Vec<(f64, f64)> {
    // Convert line to implicit form: ax + by + c = 0
    let d = knife.p1 - knife.p0;
    let a = -d.y;
    let b = d.x;
    let c = -(a * knife.p0.x + b * knife.p0.y);

    // Evaluate signed distances from control points to line
    let d0 = a * cubic.p0.x + b * cubic.p0.y + c;
    let d1 = a * cubic.p1.x + b * cubic.p1.y + c;
    let d2 = a * cubic.p2.x + b * cubic.p2.y + c;
    let d3 = a * cubic.p3.x + b * cubic.p3.y + c;

    // Find roots of the cubic polynomial
    let coeff_a = -d0 + 3.0 * d1 - 3.0 * d2 + d3;
    let coeff_b = 3.0 * d0 - 6.0 * d1 + 3.0 * d2;
    let coeff_c = -3.0 * d0 + 3.0 * d1;
    let coeff_d = d0;

    let roots = solve_cubic(coeff_a, coeff_b, coeff_c, coeff_d);

    let mut results = Vec::new();
    let knife_len_sq = d.hypot2();

    const EPSILON: f64 = 1e-9;

    for t in roots {
        if !(0.0..=1.0).contains(&t) {
            continue;
        }

        let pt = cubic.eval(t);

        let line_t = if knife_len_sq > EPSILON {
            let v = pt - knife.p0;
            (v.x * d.x + v.y * d.y) / knife_len_sq
        } else {
            0.0
        };

        if (0.0..=1.0).contains(&line_t) {
            results.push((t, line_t));
        }
    }

    results
}

/// Solve cubic equation ax³ + bx² + cx + d = 0
fn solve_cubic(a: f64, b: f64, c: f64, d: f64) -> Vec<f64> {
    let mut roots = Vec::new();
    const EPSILON: f64 = 1e-9;

    // Handle degenerate cases
    if a.abs() < EPSILON {
        if b.abs() < EPSILON {
            // Linear
            if c.abs() > EPSILON {
                let t = -d / c;
                if (0.0..=1.0).contains(&t) {
                    roots.push(t);
                }
            }
        } else {
            // Quadratic
            let disc = c * c - 4.0 * b * d;
            if disc >= 0.0 {
                let sqrt_disc = disc.sqrt();
                let t1 = (-c + sqrt_disc) / (2.0 * b);
                let t2 = (-c - sqrt_disc) / (2.0 * b);
                if (0.0..=1.0).contains(&t1) {
                    roots.push(t1);
                }
                if (0.0..=1.0).contains(&t2) && (t1 - t2).abs() > EPSILON {
                    roots.push(t2);
                }
            }
        }
        return roots;
    }

    // Normalize to t³ + pt² + qt + r = 0
    let p = b / a;
    let q = c / a;
    let r = d / a;

    // Convert to depressed cubic
    let p1 = q - p * p / 3.0;
    let q1 = r - p * q / 3.0 + 2.0 * p * p * p / 27.0;

    // Cardano's formula
    let disc = q1 * q1 / 4.0 + p1 * p1 * p1 / 27.0;

    if disc > EPSILON {
        // One real root
        let sqrt_disc = disc.sqrt();
        let u = (-q1 / 2.0 + sqrt_disc).cbrt();
        let v = (-q1 / 2.0 - sqrt_disc).cbrt();
        let t = u + v - p / 3.0;
        if (0.0..=1.0).contains(&t) {
            roots.push(t);
        }
    } else if disc.abs() <= EPSILON {
        // Multiple roots
        if q1.abs() < EPSILON {
            let t = -p / 3.0;
            if (0.0..=1.0).contains(&t) {
                roots.push(t);
            }
        } else {
            let u = (q1 / 2.0).cbrt();
            let t1 = 2.0 * u - p / 3.0;
            let t2 = -u - p / 3.0;
            if (0.0..=1.0).contains(&t1) {
                roots.push(t1);
            }
            if (0.0..=1.0).contains(&t2) && (t1 - t2).abs() > EPSILON {
                roots.push(t2);
            }
        }
    } else {
        // Three real roots
        let m = 2.0 * (-p1 / 3.0).sqrt();
        let theta = (3.0 * q1 / (p1 * m)).acos() / 3.0;
        let pi = std::f64::consts::PI;

        for k in 0..3 {
            let t = m * (theta - 2.0 * pi * k as f64 / 3.0).cos() - p / 3.0;
            if (0.0..=1.0).contains(&t) {
                let is_dup = roots.iter().any(|&r| (r - t).abs() < EPSILON);
                if !is_dup {
                    roots.push(t);
                }
            }
        }
    }

    roots
}
