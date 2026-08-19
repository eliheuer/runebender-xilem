// Ported from runebender-xilem/src/path/hyper.rs (Apache-2.0).

//! Hyperbezier path representation.
//!
//! A hyperbezier path stores only on-curve points. Off-curve control
//! points are automatically computed by the `spline` crate's solver to
//! create smooth G2-continuous curves.

use super::point::{PathPoint, PointType};
use super::point_list::PathPoints;
use crate::model::entity_id::EntityId;
use crate::model::workspace;
use kurbo::{BezPath, Point};
use spline::SplineSpec;
use std::sync::Arc;

/// A single contour represented as a hyperbezier path.
///
/// Unlike cubic paths, hyperbezier paths only store on-curve points.
/// Control points are automatically computed by the spline solver.
#[derive(Debug, Clone)]
pub struct HyperPath {
    pub points: PathPoints,
    pub closed: bool,
    pub id: EntityId,
    /// Cached bezier path for rendering.
    bezier: Arc<BezPath>,
}

impl HyperPath {
    /// Create a new hyper path with a single starting point.
    pub fn new(point: Point) -> Self {
        let start_point = PathPoint {
            id: EntityId::next(),
            point,
            typ: PointType::OnCurve { smooth: true },
        };

        let mut path = Self {
            points: PathPoints::from_vec(vec![start_point]),
            closed: false,
            id: EntityId::next(),
            bezier: Arc::new(BezPath::new()),
        };

        path.rebuild_bezier();
        path
    }

    /// Create a new hyper path from existing points.
    pub fn from_points(points: PathPoints, closed: bool) -> Self {
        let mut path = Self {
            points,
            closed,
            id: EntityId::next(),
            bezier: Arc::new(BezPath::new()),
        };

        path.rebuild_bezier();
        path
    }

    pub fn empty() -> Self {
        Self {
            points: PathPoints::new(),
            closed: false,
            id: EntityId::next(),
            bezier: Arc::new(BezPath::new()),
        }
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

    /// Call after mutating points to rebuild the bezier cache.
    pub fn after_change(&mut self) {
        self.rebuild_bezier();
    }

    pub fn to_bezpath(&self) -> BezPath {
        (*self.bezier).clone()
    }

    pub fn append_to_bezpath(&self, path: &mut BezPath) {
        for el in self.bezier.elements() {
            path.push(*el);
        }
    }

    /// Expand the solved spline into a `CubicPath` with explicit
    /// on-curve and off-curve control points.
    pub fn to_cubic(&self) -> super::cubic::CubicPath {
        use super::cubic::CubicPath;

        let mut points = Vec::new();

        for el in self.bezier.elements() {
            match el {
                kurbo::PathEl::MoveTo(p) => {
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p,
                        typ: PointType::OnCurve { smooth: true },
                    });
                }
                kurbo::PathEl::LineTo(p) => {
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p,
                        typ: PointType::OnCurve { smooth: false },
                    });
                }
                kurbo::PathEl::CurveTo(p1, p2, p3) => {
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p1,
                        typ: PointType::OffCurve { auto: false },
                    });
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p2,
                        typ: PointType::OffCurve { auto: false },
                    });
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p3,
                        typ: PointType::OnCurve { smooth: true },
                    });
                }
                kurbo::PathEl::QuadTo(p1, p2) => {
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p1,
                        typ: PointType::OffCurve { auto: false },
                    });
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p2,
                        typ: PointType::OnCurve { smooth: true },
                    });
                }
                kurbo::PathEl::ClosePath => {
                    // Handled by closed flag.
                }
            }
        }

        CubicPath::new(super::point_list::PathPoints::from_vec(points), self.closed)
    }

    /// Add a new on-curve point to the path. All points are smooth by
    /// default.
    pub fn add_on_curve_point(&mut self, point: Point) {
        let new_point = PathPoint {
            id: EntityId::next(),
            point,
            typ: PointType::OnCurve { smooth: true },
        };
        self.points.make_mut().push(new_point);
        self.rebuild_bezier();
    }

    pub fn close_path(&mut self) {
        self.closed = true;
        self.rebuild_bezier();
    }

    pub fn start_point(&self) -> Option<&PathPoint> {
        self.points.iter().next()
    }

    /// Rebuild the bezier cache from points using the spline solver.
    fn rebuild_bezier(&mut self) {
        let num_points = self.points.len();

        if num_points == 0 || num_points < 2 {
            self.bezier = Arc::new(BezPath::new());
            return;
        }

        // spline solves on kurbo 0.9 — convert at the boundary.
        let mut spec = SplineSpec::new();

        #[inline(always)]
        fn to_spline_point(p: Point) -> kurbo_09::Point {
            kurbo_09::Point::new(p.x, p.y)
        }

        let mut points_iter = self.points.iter();

        let first_point = points_iter.next().unwrap().point;
        spec.move_to(to_spline_point(first_point));

        for pt in points_iter {
            spec.spline_to(None, None, to_spline_point(pt.point), true);
        }

        if self.closed && num_points >= 3 {
            spec.spline_to(None, None, to_spline_point(first_point), true);
            spec.close();
        }

        let spline = spec.solve();
        let spline_bezpath = spline.render();

        // Convert spline's kurbo 0.9 BezPath back to kurbo 0.12.
        let mut result = BezPath::new();
        for el in spline_bezpath.elements() {
            match el {
                kurbo_09::PathEl::MoveTo(p) => {
                    result.move_to(Point::new(p.x, p.y));
                }
                kurbo_09::PathEl::LineTo(p) => {
                    result.line_to(Point::new(p.x, p.y));
                }
                kurbo_09::PathEl::QuadTo(p1, p2) => {
                    result.quad_to(Point::new(p1.x, p1.y), Point::new(p2.x, p2.y));
                }
                kurbo_09::PathEl::CurveTo(p1, p2, p3) => {
                    result.curve_to(
                        Point::new(p1.x, p1.y),
                        Point::new(p2.x, p2.y),
                        Point::new(p3.x, p3.y),
                    );
                }
                kurbo_09::PathEl::ClosePath => {
                    result.close_path();
                }
            }
        }

        self.bezier = Arc::new(result);
    }

    /// Convert from a workspace contour. Only on-curve points are kept;
    /// off-curve handles will be recomputed by the spline solver.
    pub fn from_contour(contour: &workspace::Contour) -> Self {
        if contour.points.is_empty() {
            return Self::empty();
        }

        let closed = !matches!(contour.points[0].point_type, workspace::PointType::Move);

        let mut path_points: Vec<PathPoint> = contour
            .points
            .iter()
            .filter(|pt| !matches!(pt.point_type, workspace::PointType::OffCurve))
            .map(|pt| {
                let smooth = match pt.point_type {
                    workspace::PointType::Hyper => true,
                    workspace::PointType::HyperCorner => false,
                    _ => true,
                };
                PathPoint {
                    id: EntityId::next(),
                    point: Point::new(pt.x, pt.y),
                    typ: PointType::OnCurve { smooth },
                }
            })
            .collect();

        if closed && !path_points.is_empty() {
            path_points.rotate_left(1);
        }

        Self::from_points(PathPoints::from_vec(path_points), closed)
    }

    /// Save only the on-curve hyperbezier points with smooth/corner
    /// flags. Off-curve handles are recomputed by the solver on load.
    pub fn to_contour(&self) -> workspace::Contour {
        use crate::model::workspace::{Contour, ContourPoint, PointType as WsPointType};

        let mut points = Vec::new();

        for (i, path_point) in self.points.iter().enumerate() {
            let point_type = if i == 0 && !self.closed {
                WsPointType::Move
            } else {
                match path_point.typ {
                    PointType::OnCurve { smooth: true } => WsPointType::Hyper,
                    PointType::OnCurve { smooth: false } => WsPointType::HyperCorner,
                    PointType::OffCurve { .. } => {
                        // HyperPath should only contain on-curve points;
                        // skip any stray off-curve as a data-integrity
                        // safeguard.
                        continue;
                    }
                }
            };

            points.push(ContourPoint {
                x: path_point.point.x,
                y: path_point.point.y,
                point_type,
                smooth: false,
            });
        }

        if self.closed && !points.is_empty() {
            points.rotate_right(1);
        }

        Contour { points }
    }

    /// Iterate over the solved bezier segments (not the original
    /// on-curve points).
    pub fn iter_segments(&self) -> impl Iterator<Item = super::segment::SegmentInfo> + '_ {
        HyperSegmentIterator::new(&self.bezier)
    }
}

struct HyperSegmentIterator<'a> {
    elements: std::slice::Iter<'a, kurbo::PathEl>,
    prev_point: Point,
    index: usize,
}

impl<'a> HyperSegmentIterator<'a> {
    fn new(bezier: &'a BezPath) -> Self {
        Self {
            elements: bezier.elements().iter(),
            prev_point: Point::ZERO,
            index: 0,
        }
    }
}

impl<'a> Iterator for HyperSegmentIterator<'a> {
    type Item = super::segment::SegmentInfo;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let el = self.elements.next()?;

            match el {
                kurbo::PathEl::MoveTo(p) => {
                    self.prev_point = *p;
                    self.index = 0;
                }
                kurbo::PathEl::LineTo(p) => {
                    let segment =
                        super::segment::Segment::Line(kurbo::Line::new(self.prev_point, *p));
                    let start_idx = self.index;
                    self.prev_point = *p;
                    self.index += 1;
                    return Some(super::segment::SegmentInfo {
                        segment,
                        start_index: start_idx,
                        end_index: self.index,
                        path_index: 0,
                    });
                }
                kurbo::PathEl::CurveTo(p1, p2, p3) => {
                    let segment = super::segment::Segment::Cubic(kurbo::CubicBez::new(
                        self.prev_point,
                        *p1,
                        *p2,
                        *p3,
                    ));
                    let start_idx = self.index;
                    self.prev_point = *p3;
                    self.index += 1;
                    return Some(super::segment::SegmentInfo {
                        segment,
                        start_index: start_idx,
                        end_index: self.index,
                        path_index: 0,
                    });
                }
                kurbo::PathEl::QuadTo(p1, p2) => {
                    let segment = super::segment::Segment::Quadratic(kurbo::QuadBez::new(
                        self.prev_point,
                        *p1,
                        *p2,
                    ));
                    let start_idx = self.index;
                    self.prev_point = *p2;
                    self.index += 1;
                    return Some(super::segment::SegmentInfo {
                        segment,
                        start_index: start_idx,
                        end_index: self.index,
                        path_index: 0,
                    });
                }
                kurbo::PathEl::ClosePath => {}
            }
        }
    }
}
