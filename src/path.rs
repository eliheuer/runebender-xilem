// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Path abstraction for glyph outlines

use crate::cubic_path::CubicPath;
use crate::entity_id::EntityId;
use crate::quadratic_path::QuadraticPath;
use crate::workspace;
use kurbo::BezPath;

/// A path in a glyph outline
///
/// Supports both cubic and quadratic bezier paths.
/// HyperPath support can be added later.
#[derive(Debug, Clone)]
pub enum Path {
    /// A cubic bezier path
    Cubic(CubicPath),
    /// A quadratic bezier path
    Quadratic(QuadraticPath),
}

impl Path {
    /// Convert this path to a kurbo BezPath for rendering
    pub fn to_bezpath(&self) -> BezPath {
        match self {
            Path::Cubic(cubic) => cubic.to_bezpath(),
            Path::Quadratic(quadratic) => quadratic.to_bezpath(),
        }
    }

    /// Get the unique identifier for this path
    #[allow(dead_code)]
    pub fn id(&self) -> EntityId {
        match self {
            Path::Cubic(cubic) => cubic.id,
            Path::Quadratic(quadratic) => quadratic.id,
        }
    }

    /// Get the number of points in this path
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        match self {
            Path::Cubic(cubic) => cubic.len(),
            Path::Quadratic(quadratic) => quadratic.len(),
        }
    }

    /// Check if this path is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        match self {
            Path::Cubic(cubic) => cubic.is_empty(),
            Path::Quadratic(quadratic) => quadratic.is_empty(),
        }
    }

    /// Check if this path is closed
    #[allow(dead_code)]
    pub fn is_closed(&self) -> bool {
        match self {
            Path::Cubic(cubic) => cubic.closed,
            Path::Quadratic(quadratic) => quadratic.closed,
        }
    }

    /// Get the bounding box of this path
    #[allow(dead_code)]
    pub fn bounding_box(&self) -> Option<kurbo::Rect> {
        match self {
            Path::Cubic(cubic) => cubic.bounding_box(),
            Path::Quadratic(quadratic) => quadratic.bounding_box(),
        }
    }

    /// Convert from a workspace contour (norad format)
    ///
    /// Automatically detects whether the contour contains
    /// QCurve points (quadratic) or Curve points (cubic).
    pub fn from_contour(contour: &workspace::Contour) -> Self {
        // Check if contour contains QCurve points (quadratic)
        let has_qcurve = contour.points.iter().any(|pt| {
            matches!(pt.point_type, workspace::PointType::QCurve)
        });

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
        }
    }
}
