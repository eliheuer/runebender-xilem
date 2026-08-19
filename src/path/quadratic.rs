// Ported from runebender-xilem/src/path/quadratic.rs (Apache-2.0).

//! Quadratic bezier path representation.

use super::point::{PathPoint, PointType};
use super::point_list::PathPoints;
use crate::model::entity_id::EntityId;
use crate::model::workspace;
use kurbo::BezPath;

/// A single contour represented as a quadratic bezier path.
///
/// Corresponds to a UFO contour with QCurve points. Points are stored
/// in order, with the convention that for closed paths, the first point
/// (index 0) is conceptually the last point in the cyclic sequence.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct QuadraticPath {
    pub points: PathPoints,
    pub closed: bool,
    pub id: EntityId,
}

#[allow(dead_code)]
impl QuadraticPath {
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

    /// Convert this quadratic path to a kurbo `BezPath` for rendering.
    pub fn to_bezpath(&self) -> BezPath {
        let mut path = BezPath::new();
        self.append_to_bezpath(&mut path);
        path
    }

    /// Append this quadratic path directly into an existing `BezPath`.
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

    /// Convert from a workspace contour (assumes QCurve points).
    pub fn from_contour(contour: &workspace::Contour) -> Self {
        if contour.points.is_empty() {
            return Self::empty();
        }

        let closed = !matches!(contour.points[0].point_type, workspace::PointType::Move);

        let mut path_points: Vec<PathPoint> = contour
            .points
            .iter()
            .map(PathPoint::from_contour_point_quadratic)
            .collect();

        if closed && !path_points.is_empty() {
            path_points.rotate_left(1);
        }

        Self::new(PathPoints::from_vec(path_points), closed)
    }

    /// Convert this quadratic path to a workspace contour (for saving).
    pub fn to_contour(&self) -> workspace::Contour {
        use crate::model::workspace::{Contour, ContourPoint, PointType as WsPointType};

        let mut contour_points: Vec<PathPoint> = self.points.to_vec();

        if self.closed && !contour_points.is_empty() {
            contour_points.rotate_right(1);
        }

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
                                WsPointType::QCurve
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
                    let control = Self::preceding_off_curve_control(points, start_idx, i);
                    Self::add_segment_to_path(path, control, pt.point);
                    i += 1;
                }
                PointType::OffCurve { .. } => {
                    i += 1;
                }
            }
        }
    }

    /// For quadratic paths, expect at most one off-curve point before
    /// each on-curve.
    fn preceding_off_curve_control(
        points: &[PathPoint],
        start_idx: usize,
        current_offset: usize,
    ) -> Option<kurbo::Point> {
        if current_offset <= 1 {
            return None;
        }
        let point = Self::rotated_point(points, start_idx, current_offset - 1);
        point.is_off_curve().then_some(point.point)
    }

    /// 0 control points = line, 1 = quadratic curve.
    fn add_segment_to_path(
        path: &mut BezPath,
        control: Option<kurbo::Point>,
        end_point: kurbo::Point,
    ) {
        if let Some(control) = control {
            path.quad_to(control, end_point);
        } else {
            path.line_to(end_point);
        }
    }

    fn handle_closed_path_trailing_points(
        points: &[PathPoint],
        start_idx: usize,
        path: &mut BezPath,
    ) {
        if let Some(control) = Self::trailing_off_curve_control(points, start_idx) {
            let first_pt = Self::rotated_point(points, start_idx, 0);
            Self::add_segment_to_path(path, Some(control), first_pt.point);
        }
    }

    /// For quadratic paths, expect at most one trailing off-curve point.
    fn trailing_off_curve_control(points: &[PathPoint], start_idx: usize) -> Option<kurbo::Point> {
        let len = points.len();
        if len <= 1 {
            return None;
        }
        let point = Self::rotated_point(points, start_idx, len - 1);
        point.is_off_curve().then_some(point.point)
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

    fn next_quadratic_segment_at(
        &mut self,
        point_idx: usize,
        cp: kurbo::Point,
    ) -> Option<super::segment::SegmentInfo> {
        // Quadratic curve: need 1 off-curve + 1 on-curve.
        if point_idx + 1 >= self.points.len() {
            return None;
        }

        let end = self.points[point_idx + 1].point;

        let start_idx = self.prev_on_curve_idx;
        let end_idx = point_idx + 1;
        let segment =
            super::segment::Segment::Quadratic(kurbo::QuadBez::new(self.prev_on_curve, cp, end));

        self.prev_on_curve = end;
        self.prev_on_curve_idx = point_idx + 1;
        self.index = point_idx + 2;

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
            } else if let Some(seg) = self.next_quadratic_segment_at(point_idx, point) {
                return Some(seg);
            } else {
                self.index = self.points.len();
            }
        }

        if self.closed && !self.close_emitted && self.prev_on_curve_idx != self.first_on_curve_idx {
            self.close_emitted = true;
            let first = &self.points[self.first_on_curve_idx];

            let trailing_off = (self.prev_on_curve_idx + 1..self.points.len())
                .find(|&i| self.points[i].is_off_curve());

            if let Some(off_idx) = trailing_off {
                let cp = self.points[off_idx].point;
                let segment = super::segment::Segment::Quadratic(kurbo::QuadBez::new(
                    self.prev_on_curve,
                    cp,
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
