// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Path abstraction for glyph outlines — the editable representation.
//!
//! The `Path` enum wraps three curve types: `Cubic` (standard UFO beziers),
//! `Quadratic` (TrueType-style), and `Hyper` (hyperbezier splines with only
//! on-curve points). All three convert to `kurbo::BezPath` for rendering.
//! Paths are created from `workspace::Contour` data when a glyph is opened
//! for editing, and converted back when the session is saved.

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

use crate::model::workspace;
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
