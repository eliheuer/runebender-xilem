// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Path abstraction for glyph outlines

pub mod cubic;
pub mod hyper;
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

use crate::entity_id::EntityId;
use crate::workspace;
use kurbo::BezPath;

/// A path in a glyph outline
///
/// Supports cubic, quadratic, and hyperbezier paths.
#[derive(Debug, Clone)]
pub enum Path {
    /// A cubic bezier path
    Cubic(CubicPath),
    /// A quadratic bezier path
    Quadratic(QuadraticPath),
    /// A hyperbezier path (uses spline solver)
    Hyper(HyperPath),
}

impl Path {
    /// Convert this path to a kurbo BezPath for rendering
    pub fn to_bezpath(&self) -> BezPath {
        match self {
            Path::Cubic(cubic) => cubic.to_bezpath(),
            Path::Quadratic(quadratic) => quadratic.to_bezpath(),
            Path::Hyper(hyper) => hyper.to_bezpath(),
        }
    }

    /// Get the unique identifier for this path
    #[allow(dead_code)]
    pub fn id(&self) -> EntityId {
        match self {
            Path::Cubic(cubic) => cubic.id,
            Path::Quadratic(quadratic) => quadratic.id,
            Path::Hyper(hyper) => hyper.id,
        }
    }

    /// Get the number of points in this path
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        match self {
            Path::Cubic(cubic) => cubic.len(),
            Path::Quadratic(quadratic) => quadratic.len(),
            Path::Hyper(hyper) => hyper.len(),
        }
    }

    /// Check if this path is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        match self {
            Path::Cubic(cubic) => cubic.is_empty(),
            Path::Quadratic(quadratic) => quadratic.is_empty(),
            Path::Hyper(hyper) => hyper.is_empty(),
        }
    }

    /// Check if this path is closed
    #[allow(dead_code)]
    pub fn is_closed(&self) -> bool {
        match self {
            Path::Cubic(cubic) => cubic.closed,
            Path::Quadratic(quadratic) => quadratic.closed,
            Path::Hyper(hyper) => hyper.closed,
        }
    }

    /// Get the bounding box of this path
    #[allow(dead_code)]
    pub fn bounding_box(&self) -> Option<kurbo::Rect> {
        match self {
            Path::Cubic(cubic) => cubic.bounding_box(),
            Path::Quadratic(quadratic) => quadratic.bounding_box(),
            Path::Hyper(hyper) => hyper.bounding_box(),
        }
    }

    /// Check if this path is a hyperbezier path
    #[allow(dead_code)]
    pub fn is_hyper(&self) -> bool {
        matches!(self, Path::Hyper(_))
    }

    /// Convert from a workspace contour (norad format)
    ///
    /// Automatically detects whether the contour contains
    /// QCurve points (quadratic) or Curve points (cubic).
    pub fn from_contour(contour: &workspace::Contour) -> Self {
        // Check if contour contains hyperbezier points
        let has_hyper = contour.points.iter().any(|pt| {
            matches!(
                pt.point_type,
                workspace::PointType::Hyper | workspace::PointType::HyperCorner
            )
        });

        if has_hyper {
            return Path::Hyper(HyperPath::from_contour(contour));
        }

        // Check if contour contains QCurve points (quadratic)
        let has_qcurve = contour
            .points
            .iter()
            .any(|pt| matches!(pt.point_type, workspace::PointType::QCurve));

        if has_qcurve {
            Path::Quadratic(QuadraticPath::from_contour(contour))
        } else {
            Path::Cubic(CubicPath::from_contour(contour))
        }
    }

    /// Convert this path to a workspace contour (for saving)
    pub fn to_contour(&self) -> workspace::Contour {
        match self {
            Path::Cubic(cubic) => cubic.to_contour(),
            Path::Quadratic(quadratic) => quadratic.to_contour(),
            Path::Hyper(hyper) => hyper.to_contour(),
        }
    }
}
