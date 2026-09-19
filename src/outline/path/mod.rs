// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Path representation & geometry. Ported from
// runebender-xilem/src/path/.

//! Path abstraction for glyph outlines: the editable representation.
//!
//! The `Path` enum wraps three curve types: `Cubic` (standard UFO
//! beziers), `Quadratic` (TrueType-style), and `Hyper` (hyperbezier
//! splines with only on-curve points). All three convert to
//! `kurbo::BezPath` for rendering. Paths are created from
//! `workspace::Contour` data when a glyph is opened for editing, and
//! converted back when the session is saved.

pub mod cubic;
pub mod hyper;
pub mod hyper_model;
pub mod point;
pub mod point_list;
pub mod quadrant;
pub mod quadratic;
pub mod segment;

pub use cubic::CubicPath;
pub use hyper::HyperPath;
pub use point::{PathPoint, PointType};
pub use point_list::PathPoints;
pub use quadrant::Quadrant;
pub use quadratic::QuadraticPath;
pub use segment::{Segment, SegmentInfo};

use self::hyper_model as workspace;
use crate::document::model::entity_id::EntityId;
use crate::document::{ContourView, LayerPointType};
use kurbo::BezPath;

/// A path in a glyph outline. Supports cubic, quadratic, and
/// hyperbezier paths.
#[derive(Debug, Clone)]
pub enum Path {
    /// A contour containing cubic segments, with mixed quadratic and line segments retained.
    Cubic(CubicPath),
    /// A contour with quadratic control points (UFO `qcurve` segments).
    Quadratic(QuadraticPath),
    /// A hyperbezier contour; only on-curve points are stored and handles are solved.
    Hyper(HyperPath),
}

impl Path {
    /// Build an editable outline path directly from a canonical document contour.
    pub fn from_document_contour(contour: ContourView<'_>) -> Self {
        let closed = contour.is_closed();
        let hyper = contour.is_hyper();
        let point_types: Vec<_> = contour.points().map(|point| point.point_type()).collect();
        let all_off_curve = !hyper
            && closed
            && !point_types.is_empty()
            && point_types
                .iter()
                .all(|point_type| *point_type == LayerPointType::OffCurve);
        let has_cubic = point_types.contains(&LayerPointType::Curve);
        let quadratic =
            !has_cubic && (all_off_curve || point_types.contains(&LayerPointType::QCurve));
        if all_off_curve {
            let controls: Vec<_> = contour.points().map(|point| point.position()).collect();
            let mut points = Vec::with_capacity(controls.len() * 2);
            for (index, control) in controls.iter().copied().enumerate() {
                let previous = controls[(index + controls.len() - 1) % controls.len()];
                points.push(PathPoint {
                    id: EntityId::next(),
                    point: previous.midpoint(control),
                    typ: PointType::OnCurve { smooth: false },
                });
                points.push(PathPoint {
                    id: EntityId::next(),
                    point: control,
                    typ: PointType::OffCurve { auto: false },
                });
            }
            return Self::Quadratic(QuadraticPath::new(PathPoints::from_vec(points), true));
        }
        let mut points: Vec<_> = contour
            .points()
            .filter(|point| !hyper || point.point_type() != LayerPointType::OffCurve)
            .map(|point| PathPoint {
                id: EntityId::next(),
                point: point.position(),
                typ: if point.point_type() == LayerPointType::OffCurve {
                    PointType::OffCurve { auto: false }
                } else {
                    PointType::OnCurve {
                        smooth: if hyper {
                            point.point_type() != LayerPointType::Line
                        } else {
                            point.is_smooth()
                        },
                    }
                },
            })
            .collect();
        if closed && !points.is_empty() {
            points.rotate_left(1);
        }
        let points = PathPoints::from_vec(points);
        if hyper {
            Self::Hyper(HyperPath::from_points(points, closed))
        } else if quadratic {
            Self::Quadratic(QuadraticPath::new(points, closed))
        } else {
            Self::Cubic(CubicPath::new(points, closed))
        }
    }

    pub(crate) fn entity_id(&self) -> EntityId {
        match self {
            Self::Cubic(path) => path.id,
            Self::Quadratic(path) => path.id,
            Self::Hyper(path) => path.id,
        }
    }

    /// Renders this path as a new `kurbo::BezPath`.
    pub fn to_bezpath(&self) -> BezPath {
        match self {
            Self::Cubic(cubic) => cubic.to_bezpath(),
            Self::Quadratic(quadratic) => quadratic.to_bezpath(),
            Self::Hyper(hyper) => hyper.to_bezpath(),
        }
    }

    /// Appends this path's elements to an existing `BezPath` without clearing it.
    pub fn append_to_bezpath(&self, path: &mut BezPath) {
        match self {
            Self::Cubic(cubic) => cubic.append_to_bezpath(path),
            Self::Quadratic(quadratic) => quadratic.append_to_bezpath(path),
            Self::Hyper(hyper) => hyper.append_to_bezpath(path),
        }
    }

    /// Detect the curve type from the contour and dispatch.
    pub fn from_contour(contour: &workspace::Contour) -> Self {
        let has_hyper = contour.points.iter().any(|pt| {
            matches!(
                pt.point_type,
                workspace::PointType::Hyper | workspace::PointType::HyperCorner
            )
        });

        if has_hyper {
            return Self::Hyper(HyperPath::from_contour(contour));
        }

        let has_curve = contour
            .points
            .iter()
            .any(|pt| matches!(pt.point_type, workspace::PointType::Curve));
        let has_qcurve = contour
            .points
            .iter()
            .any(|pt| matches!(pt.point_type, workspace::PointType::QCurve));
        let all_off_curve = !contour.points.is_empty()
            && contour
                .points
                .iter()
                .all(|pt| matches!(pt.point_type, workspace::PointType::OffCurve));

        if !has_curve && (has_qcurve || all_off_curve) {
            Self::Quadratic(QuadraticPath::from_contour(contour))
        } else {
            Self::Cubic(CubicPath::from_contour(contour))
        }
    }

    /// Returns the path's editable points, whatever the curve type.
    pub fn points(&self) -> &PathPoints {
        match self {
            Self::Cubic(cubic) => cubic.points(),
            Self::Quadratic(quadratic) => quadratic.points(),
            Self::Hyper(hyper) => hyper.points(),
        }
    }

    /// Whether the last segment connects back to the first point.
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Cubic(cubic) => cubic.is_closed(),
            Self::Quadratic(quadratic) => quadratic.is_closed(),
            Self::Hyper(hyper) => hyper.is_closed(),
        }
    }

    /// Converts back to the `workspace::Contour` form used for saving and conversion.
    pub fn to_contour(&self) -> workspace::Contour {
        match self {
            Self::Cubic(cubic) => cubic.to_contour(),
            Self::Quadratic(quadratic) => quadratic.to_contour(),
            Self::Hyper(hyper) => hyper.to_contour(),
        }
    }
}
