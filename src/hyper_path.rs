// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Hyperbezier path representation
//!
//! A hyperbezier path stores only on-curve points. Off-curve control points
//! are automatically computed by the spline solver to create smooth G2
//! continuous curves.

use crate::entity_id::EntityId;
use crate::point::{PathPoint, PointType};
use crate::point_list::PathPoints;
use crate::workspace;
use kurbo::{BezPath, Point, Shape};
use spline::SplineSpec;
use std::sync::Arc;

/// A single contour represented as a hyperbezier path
///
/// Unlike cubic paths, hyperbezier paths only store on-curve points.
/// Control points are automatically computed by the spline solver.
#[derive(Debug, Clone)]
pub struct HyperPath {
    /// The on-curve points in this path
    pub points: PathPoints,

    /// Whether this path is closed
    pub closed: bool,

    /// Unique identifier for this path
    pub id: EntityId,

    /// Cached bezier path for rendering
    bezier: Arc<BezPath>,
}

impl HyperPath {
    /// Create a new hyper path with a single starting point
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

    /// Create a new hyper path from existing points
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

    /// Create a new empty hyper path
    pub fn empty() -> Self {
        Self {
            points: PathPoints::new(),
            closed: false,
            id: EntityId::next(),
            bezier: Arc::new(BezPath::new()),
        }
    }

    /// Get the number of points in this path
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Check if this path is empty
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Get a reference to the points in this path
    pub fn points(&self) -> &PathPoints {
        &self.points
    }

    /// Call this after modifying points to rebuild the bezier cache
    pub fn after_change(&mut self) {
        self.rebuild_bezier();
    }

    /// Convert this hyper path to a kurbo BezPath for rendering
    pub fn to_bezpath(&self) -> BezPath {
        (*self.bezier).clone()
    }

    /// Get the bounding box of this path
    pub fn bounding_box(&self) -> Option<kurbo::Rect> {
        if self.bezier.is_empty() {
            None
        } else {
            Some(self.bezier.bounding_box())
        }
    }

    /// Add a new on-curve point to the path
    ///
    /// This is the primary way to build a hyperbezier path.
    /// All points are smooth by default.
    pub fn add_on_curve_point(&mut self, point: Point) {
        let new_point = PathPoint {
            id: EntityId::next(),
            point,
            typ: PointType::OnCurve { smooth: true },
        };
        self.points.make_mut().push(new_point);
        self.rebuild_bezier();
    }

    /// Close the path
    pub fn close_path(&mut self) {
        self.closed = true;
        self.rebuild_bezier();
    }

    /// Get the first on-curve point (start point)
    pub fn start_point(&self) -> Option<&PathPoint> {
        self.points.iter().next()
    }

    /// Rebuild the bezier cache from points using the spline solver
    fn rebuild_bezier(&mut self) {
        let num_points = self.points.len();

        if num_points == 0 {
            self.bezier = Arc::new(BezPath::new());
            return;
        }

        // Need at least 2 points to draw anything
        if num_points < 2 {
            // Just a single point - nothing to draw
            self.bezier = Arc::new(BezPath::new());
            return;
        }

        // Build the spline specification
        // Note: We need to convert from kurbo 0.12 Point to spline's kurbo 0.9 Point
        let mut spec = SplineSpec::new();

        // Helper to convert Point versions (kurbo 0.12 -> kurbo 0.9)
        #[inline(always)]
        fn to_spline_point(p: Point) -> kurbo_09::Point {
            kurbo_09::Point::new(p.x, p.y)
        }

        // Iterate directly over points without collecting into Vec
        let mut points_iter = self.points.iter();

        // Move to the first point
        let first_point = points_iter.next().unwrap().point;
        spec.move_to(to_spline_point(first_point));

        // Add spline segments for each subsequent point
        for pt in points_iter {
            // Use spline_to with auto control points (None, None)
            // This lets the solver compute optimal handle positions
            spec.spline_to(None, None, to_spline_point(pt.point), true);
        }

        // Close the path if needed
        if self.closed && num_points >= 3 {
            // Add a spline segment back to the first point to ensure the closing
            // segment is also a smooth hyperbezier curve
            spec.spline_to(
                None,
                None,
                to_spline_point(first_point),
                true,
            );
            spec.close();
        }

        // Solve the spline and render to bezier
        let spline = spec.solve();
        let spline_bezpath = spline.render();

        // Convert from spline's kurbo 0.9 BezPath to our kurbo 0.12 BezPath
        let elements = spline_bezpath.elements();
        let mut result = BezPath::new();

        for el in elements {
            match el {
                kurbo_09::PathEl::MoveTo(p) => {
                    result.move_to(Point::new(p.x, p.y));
                }
                kurbo_09::PathEl::LineTo(p) => {
                    result.line_to(Point::new(p.x, p.y));
                }
                kurbo_09::PathEl::QuadTo(p1, p2) => {
                    result.quad_to(
                        Point::new(p1.x, p1.y),
                        Point::new(p2.x, p2.y),
                    );
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

    /// Convert from a workspace contour (norad format)
    pub fn from_contour(contour: &workspace::Contour) -> Self {
        if contour.points.is_empty() {
            return Self::empty();
        }

        // Determine if the path is closed
        let closed = !matches!(
            contour.points[0].point_type,
            workspace::PointType::Move
        );

        // Convert only on-curve points (skip off-curve)
        let mut path_points: Vec<PathPoint> = contour
            .points
            .iter()
            .filter(|pt| {
                !matches!(pt.point_type, workspace::PointType::OffCurve)
            })
            .map(|pt| PathPoint {
                id: EntityId::next(),
                point: Point::new(pt.x, pt.y),
                typ: PointType::OnCurve { smooth: true },
            })
            .collect();

        // If closed, rotate left by 1 to match Runebender's convention
        if closed && !path_points.is_empty() {
            path_points.rotate_left(1);
        }

        Self::from_points(PathPoints::from_vec(path_points), closed)
    }

    /// Convert this hyper path to a workspace contour (for saving)
    ///
    /// This converts the solved spline to cubic beziers for UFO format.
    pub fn to_contour(&self) -> workspace::Contour {
        use crate::workspace::{
            Contour, ContourPoint, PointType as WsPointType,
        };

        // Convert the bezpath to contour points
        let mut points = Vec::new();
        let mut first = true;

        for el in self.bezier.elements() {
            match el {
                kurbo::PathEl::MoveTo(p) => {
                    if first {
                        points.push(ContourPoint {
                            x: p.x,
                            y: p.y,
                            point_type: if self.closed {
                                WsPointType::Curve
                            } else {
                                WsPointType::Move
                            },
                        });
                        first = false;
                    }
                }
                kurbo::PathEl::LineTo(p) => {
                    points.push(ContourPoint {
                        x: p.x,
                        y: p.y,
                        point_type: WsPointType::Line,
                    });
                }
                kurbo::PathEl::QuadTo(p1, p2) => {
                    points.push(ContourPoint {
                        x: p1.x,
                        y: p1.y,
                        point_type: WsPointType::OffCurve,
                    });
                    points.push(ContourPoint {
                        x: p2.x,
                        y: p2.y,
                        point_type: WsPointType::QCurve,
                    });
                }
                kurbo::PathEl::CurveTo(p1, p2, p3) => {
                    points.push(ContourPoint {
                        x: p1.x,
                        y: p1.y,
                        point_type: WsPointType::OffCurve,
                    });
                    points.push(ContourPoint {
                        x: p2.x,
                        y: p2.y,
                        point_type: WsPointType::OffCurve,
                    });
                    points.push(ContourPoint {
                        x: p3.x,
                        y: p3.y,
                        point_type: WsPointType::Curve,
                    });
                }
                kurbo::PathEl::ClosePath => {
                    // Already handled by closed flag
                }
            }
        }

        // Rotate right if closed to match UFO convention
        if self.closed && !points.is_empty() {
            points.rotate_right(1);
        }

        Contour { points }
    }

    /// Iterate over the segments in this path
    ///
    /// Note: This iterates over the solved bezier segments, not the
    /// original on-curve points.
    pub fn iter_segments(
        &self,
    ) -> impl Iterator<Item = crate::path_segment::SegmentInfo> + '_ {
        HyperSegmentIterator::new(&self.bezier)
    }
}

/// Iterator over hyper path segments (from the solved bezier)
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
    type Item = crate::path_segment::SegmentInfo;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let el = self.elements.next()?;

            match el {
                kurbo::PathEl::MoveTo(p) => {
                    self.prev_point = *p;
                    self.index = 0;
                    // Continue to next element
                }
                kurbo::PathEl::LineTo(p) => {
                    let segment = crate::path_segment::Segment::Line(
                        kurbo::Line::new(self.prev_point, *p),
                    );
                    let start_idx = self.index;
                    self.prev_point = *p;
                    self.index += 1;
                    return Some(crate::path_segment::SegmentInfo {
                        segment,
                        start_index: start_idx,
                        end_index: self.index,
                    });
                }
                kurbo::PathEl::CurveTo(p1, p2, p3) => {
                    let segment = crate::path_segment::Segment::Cubic(
                        kurbo::CubicBez::new(self.prev_point, *p1, *p2, *p3),
                    );
                    let start_idx = self.index;
                    self.prev_point = *p3;
                    self.index += 1;
                    return Some(crate::path_segment::SegmentInfo {
                        segment,
                        start_index: start_idx,
                        end_index: self.index,
                    });
                }
                kurbo::PathEl::QuadTo(p1, p2) => {
                    let segment = crate::path_segment::Segment::Quadratic(
                        kurbo::QuadBez::new(self.prev_point, *p1, *p2),
                    );
                    let start_idx = self.index;
                    self.prev_point = *p2;
                    self.index += 1;
                    return Some(crate::path_segment::SegmentInfo {
                        segment,
                        start_index: start_idx,
                        end_index: self.index,
                    });
                }
                kurbo::PathEl::ClosePath => {
                    // Skip close path elements
                }
            }
        }
    }
}
