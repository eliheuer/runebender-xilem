// Ported from runebender-xilem/src/path/cubic.rs (Apache-2.0).

//! Cubic bezier path representation (the default curve type for UFO
//! fonts).
//!
//! A `CubicPath` stores one contour as a sequence of on-curve and
//! off-curve points. `to_bezpath()` walks the points and emits `kurbo`
//! move/curve/line commands. `from_contour()` converts from the
//! workspace's `Contour` format, assigning each point a unique
//! `EntityId` for selection and hit testing.

use super::point::{PathPoint, PointType};
use super::point_list::PathPoints;
use crate::model::entity_id::EntityId;
use crate::model::workspace;
use kurbo::BezPath;

/// A single contour represented as a cubic bezier path.
///
/// This corresponds to a UFO contour. Points are stored in order, with
/// the convention that for closed paths, the first point (index 0) is
/// conceptually the last point in the cyclic sequence.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CubicPath {
    pub points: PathPoints,
    pub closed: bool,
    pub id: EntityId,
}

#[allow(dead_code)]
impl CubicPath {
    pub fn new(points: PathPoints, closed: bool) -> Self {
        Self {
            points,
            closed,
            id: EntityId::next(),
        }
    }

    pub fn empty() -> Self {
        Self::new(PathPoints::new(), false)
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn points(&self) -> &PathPoints {
        &self.points
    }

    /// Convert this cubic path to a kurbo `BezPath` for rendering.
    pub fn to_bezpath(&self) -> BezPath {
        let mut path = BezPath::new();
        self.append_to_bezpath(&mut path);
        path
    }

    /// Append this cubic path directly into an existing `BezPath`.
    pub fn append_to_bezpath(&self, path: &mut BezPath) {
        let points = self.points.as_slice();

        if points.is_empty() {
            return;
        }

        let start_idx = points.iter().position(|p| p.is_on_curve()).unwrap_or(0);
        path.move_to(points[start_idx].point);
        Self::process_points(points, start_idx, path);

        if self.closed {
            Self::handle_closed_path_trailing_points(points, start_idx, path);
            path.close_path();
        }
    }

    /// Convert from a workspace contour (norad-shaped).
    pub fn from_contour(contour: &workspace::Contour) -> Self {
        if contour.points.is_empty() {
            return Self::empty();
        }

        // In UFO, a contour is closed unless the first point is a Move.
        let closed = !matches!(contour.points[0].point_type, workspace::PointType::Move);

        let mut path_points: Vec<PathPoint> = contour
            .points
            .iter()
            .map(PathPoint::from_contour_point)
            .collect();

        // If closed, rotate left by 1 to match Runebender's convention
        // (first point in closed path is last in vector).
        if closed && !path_points.is_empty() {
            path_points.rotate_left(1);
        }

        Self::new(PathPoints::from_vec(path_points), closed)
    }

    /// Convert this cubic path to a workspace contour (for saving).
    pub fn to_contour(&self) -> workspace::Contour {
        use crate::model::workspace::{Contour, ContourPoint, PointType as WsPointType};

        let mut contour_points: Vec<PathPoint> = self.points.to_vec();

        // If closed, rotate right by 1 to reverse `from_contour`.
        if self.closed && !contour_points.is_empty() {
            contour_points.rotate_right(1);
        }

        // Determine Curve vs Line from the actual preceding point, not
        // from the smooth flag (smooth is about tangent continuity, not
        // segment type).
        let len = contour_points.len();
        let points: Vec<ContourPoint> = contour_points
            .iter()
            .enumerate()
            .map(|(i, pt)| {
                let point_type = match pt.typ {
                    PointType::OffCurve { .. } => WsPointType::OffCurve,
                    PointType::OnCurve { .. } => {
                        if i == 0 && !self.closed {
                            WsPointType::Move
                        } else {
                            let prev = if i > 0 { i - 1 } else { len - 1 };
                            if contour_points[prev].is_off_curve() {
                                WsPointType::Curve
                            } else {
                                WsPointType::Line
                            }
                        }
                    }
                };

                let smooth = matches!(pt.typ, PointType::OnCurve { smooth: true });
                ContourPoint {
                    x: pt.point.x,
                    y: pt.point.y,
                    point_type,
                    smooth,
                }
            })
            .collect();

        Contour { points }
    }

    /// Iterate over the segments in this path.
    pub fn iter_segments(&self) -> impl Iterator<Item = super::segment::SegmentInfo> + '_ {
        SegmentIterator::new(&self.points, self.closed)
    }

    fn rotated_point(points: &[PathPoint], start_idx: usize, offset: usize) -> &PathPoint {
        &points[(start_idx + offset) % points.len()]
    }

    fn process_points(points: &[PathPoint], start_idx: usize, path: &mut BezPath) {
        let mut i = 1;
        while i < points.len() {
            let pt = Self::rotated_point(points, start_idx, i);

            match pt.typ {
                PointType::OnCurve { .. } => {
                    let (control_count, first_control, second_control) =
                        Self::preceding_off_curve_controls(points, start_idx, i);
                    Self::add_segment_to_path(
                        path,
                        control_count,
                        first_control,
                        second_control,
                        pt.point,
                    );
                    i += 1;
                }
                PointType::OffCurve { .. } => {
                    // Off-curve points are processed with the next
                    // on-curve point.
                    i += 1;
                }
            }
        }
    }

    fn preceding_off_curve_controls(
        points: &[PathPoint],
        start_idx: usize,
        current_offset: usize,
    ) -> (usize, Option<kurbo::Point>, Option<kurbo::Point>) {
        let mut count = 0;
        let mut newest = None;
        let mut second_newest = None;
        let mut offset = current_offset;

        while offset > 1 {
            offset -= 1;
            let point = Self::rotated_point(points, start_idx, offset);
            if !point.is_off_curve() {
                break;
            }
            count += 1;
            if count == 1 {
                newest = Some(point.point);
            } else if count == 2 {
                second_newest = Some(point.point);
            }
        }

        (count, newest, second_newest)
    }

    fn add_segment_to_path(
        path: &mut BezPath,
        control_count: usize,
        first_control: Option<kurbo::Point>,
        second_control: Option<kurbo::Point>,
        end_point: kurbo::Point,
    ) {
        match control_count {
            0 => {
                path.line_to(end_point);
            }
            1 => {
                path.quad_to(
                    first_control.expect("control count guarantees point"),
                    end_point,
                );
            }
            _ => {
                path.curve_to(
                    second_control.expect("control count guarantees first cubic point"),
                    first_control.expect("control count guarantees second cubic point"),
                    end_point,
                );
            }
        }
    }

    fn handle_closed_path_trailing_points(
        points: &[PathPoint],
        start_idx: usize,
        path: &mut BezPath,
    ) {
        let (control_count, first_control, second_control) =
            Self::trailing_off_curve_controls(points, start_idx);

        if control_count > 0 {
            let first_pt = Self::rotated_point(points, start_idx, 0);
            Self::add_segment_to_path(
                path,
                control_count,
                first_control,
                second_control,
                first_pt.point,
            );
        }
    }

    fn trailing_off_curve_controls(
        points: &[PathPoint],
        start_idx: usize,
    ) -> (usize, Option<kurbo::Point>, Option<kurbo::Point>) {
        let mut count = 0;
        let mut newest = None;
        let mut second_newest = None;
        let mut offset = points.len();

        while offset > 1 {
            offset -= 1;
            let point = Self::rotated_point(points, start_idx, offset);
            if !point.is_off_curve() {
                break;
            }
            count += 1;
            if count == 1 {
                newest = Some(point.point);
            } else if count == 2 {
                second_newest = Some(point.point);
            }
        }

        (count, newest, second_newest)
    }
}

#[allow(dead_code)]
struct SegmentIterator {
    points: Vec<PathPoint>,
    closed: bool,
    index: usize,
    prev_on_curve: kurbo::Point,
    prev_on_curve_idx: usize,
    first_on_curve_idx: usize,
    close_emitted: bool,
}

impl SegmentIterator {
    fn new(points: &super::point_list::PathPoints, closed: bool) -> Self {
        let points_vec: Vec<PathPoint> = points.iter().cloned().collect();

        let (start_idx, start_pt) = points_vec
            .iter()
            .enumerate()
            .find(|(_, p)| p.is_on_curve())
            .map(|(i, p)| (i, p.point))
            .unwrap_or((0, kurbo::Point::ZERO));

        let index = start_idx + 1;

        Self {
            points: points_vec,
            closed,
            index,
            prev_on_curve: start_pt,
            prev_on_curve_idx: start_idx,
            first_on_curve_idx: start_idx,
            close_emitted: false,
        }
    }

    fn next_line_segment_at(
        &mut self,
        point_idx: usize,
        point: kurbo::Point,
    ) -> Option<super::segment::SegmentInfo> {
        let start_idx = self.prev_on_curve_idx;
        let end_idx = point_idx;
        let segment = super::segment::Segment::Line(kurbo::Line::new(self.prev_on_curve, point));

        self.prev_on_curve = point;
        self.prev_on_curve_idx = point_idx;
        self.index = point_idx + 1;

        Some(super::segment::SegmentInfo {
            segment,
            start_index: start_idx,
            end_index: end_idx,
            path_index: 0,
        })
    }

    fn next_cubic_segment_at(
        &mut self,
        point_idx: usize,
        cp1: kurbo::Point,
    ) -> Option<super::segment::SegmentInfo> {
        // Cubic curve: need 2 off-curve + 1 on-curve.
        if point_idx + 2 >= self.points.len() {
            return None;
        }

        let cp2 = self.points[point_idx + 1].point;
        let end = self.points[point_idx + 2].point;

        let start_idx = self.prev_on_curve_idx;
        let end_idx = point_idx + 2;
        let segment =
            super::segment::Segment::Cubic(kurbo::CubicBez::new(self.prev_on_curve, cp1, cp2, end));

        self.prev_on_curve = end;
        self.prev_on_curve_idx = point_idx + 2;
        self.index = point_idx + 3;

        Some(super::segment::SegmentInfo {
            segment,
            start_index: start_idx,
            end_index: end_idx,
            path_index: 0,
        })
    }
}

impl Iterator for SegmentIterator {
    type Item = super::segment::SegmentInfo;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.points.len() {
            let is_on_curve = self.points[self.index].is_on_curve();
            let point = self.points[self.index].point;
            let point_idx = self.index;

            if is_on_curve {
                return self.next_line_segment_at(point_idx, point);
            } else if let Some(seg) = self.next_cubic_segment_at(point_idx, point) {
                return Some(seg);
            } else {
                // Trailing off-curves are part of the closing segment.
                self.index = self.points.len();
            }
        }

        // Emit the closing segment for closed paths.
        if self.closed && !self.close_emitted && self.prev_on_curve_idx != self.first_on_curve_idx {
            self.close_emitted = true;
            let first = &self.points[self.first_on_curve_idx];

            let off_curves: Vec<_> = (self.prev_on_curve_idx + 1..self.points.len())
                .filter(|&i| self.points[i].is_off_curve())
                .collect();

            if off_curves.len() >= 2 {
                let cp1 = self.points[off_curves[0]].point;
                let cp2 = self.points[off_curves[1]].point;
                let segment = super::segment::Segment::Cubic(kurbo::CubicBez::new(
                    self.prev_on_curve,
                    cp1,
                    cp2,
                    first.point,
                ));
                return Some(super::segment::SegmentInfo {
                    segment,
                    start_index: self.prev_on_curve_idx,
                    end_index: self.first_on_curve_idx,
                    path_index: 0,
                });
            }

            let segment =
                super::segment::Segment::Line(kurbo::Line::new(self.prev_on_curve, first.point));
            return Some(super::segment::SegmentInfo {
                segment,
                start_index: self.prev_on_curve_idx,
                end_index: self.first_on_curve_idx,
                path_index: 0,
            });
        }

        None
    }
}
