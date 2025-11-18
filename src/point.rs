// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Point types for bezier paths

use crate::entity_id::EntityId;
use crate::workspace::{self, PointType as WsPointType};
use kurbo::Point;

/// A point type in a bezier path
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointType {
    /// A point on the curve
    OnCurve {
        /// Whether this is a smooth point (tangent continuity) or a corner
        smooth: bool,
    },
    /// An off-curve control point (bezier handle)
    OffCurve {
        /// Whether this is an automatically positioned handle
        auto: bool,
    },
}

impl PointType {
    /// Check if this is an on-curve point
    pub fn is_on_curve(&self) -> bool {
        matches!(self, PointType::OnCurve { .. })
    }

    /// Check if this is an off-curve point
    pub fn is_off_curve(&self) -> bool {
        matches!(self, PointType::OffCurve { .. })
    }
}

/// A point in a bezier path with unique identity
#[derive(Debug, Clone)]
pub struct PathPoint {
    /// Unique identifier for this point
    pub id: EntityId,

    /// The position in design space
    pub point: Point,

    /// The type of point (on-curve smooth/corner, or off-curve)
    pub typ: PointType,
}

impl PathPoint {
    /// Create a new path point
    fn new(point: Point, typ: PointType) -> Self {
        Self {
            id: EntityId::next(),
            point,
            typ,
        }
    }

    /// Check if this point is on the curve
    pub fn is_on_curve(&self) -> bool {
        self.typ.is_on_curve()
    }

    /// Check if this point is off the curve (control point)
    pub fn is_off_curve(&self) -> bool {
        self.typ.is_off_curve()
    }

    /// Convert from a workspace contour point (norad format)
    pub fn from_contour_point(pt: &workspace::ContourPoint) -> Self {
        let point = Point::new(pt.x, pt.y);
        let typ = PointType::from_workspace_type(pt.point_type);
        Self::new(point, typ)
    }

    /// Convert from a workspace contour point for quadratic paths
    ///
    /// QCurve points are treated as smooth on-curve points
    pub fn from_contour_point_quadratic(
        pt: &workspace::ContourPoint,
    ) -> Self {
        let point = Point::new(pt.x, pt.y);
        let typ = PointType::from_workspace_type_quadratic(pt.point_type);
        Self::new(point, typ)
    }
}

impl PointType {
    /// Convert from workspace point type (norad format)
    pub fn from_workspace_type(pt_type: WsPointType) -> Self {
        match pt_type {
            WsPointType::Move | WsPointType::Line => {
                PointType::OnCurve { smooth: false }
            }
            WsPointType::Curve => PointType::OnCurve { smooth: true },
            WsPointType::OffCurve => PointType::OffCurve { auto: false },
            WsPointType::QCurve => {
                // For now, treat QCurve as a smooth on-curve point.
                // Proper quadratic support would require more work.
                PointType::OnCurve { smooth: true }
            }
        }
    }

    fn from_workspace_type_quadratic(pt_type: WsPointType) -> Self {
        match pt_type {
            WsPointType::Move | WsPointType::Line => {
                PointType::OnCurve { smooth: false }
            }
            WsPointType::QCurve | WsPointType::Curve => {
                // In quadratic paths, Curve points behave like QCurve.
                PointType::OnCurve { smooth: true }
            }
            WsPointType::OffCurve => PointType::OffCurve { auto: false },
        }
    }
}
