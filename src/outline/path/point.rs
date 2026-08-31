// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Ported from runebender-xilem/src/path/point.rs (Apache-2.0).

//! Point types for bezier paths: the atoms of editable outlines.
//!
//! `PathPoint` pairs a `kurbo::Point` position with a `PointType`
//! (on-curve smooth/corner, or off-curve handle) and a unique
//! `EntityId`. Points are stored in `PathPoints` collections inside
//! each `CubicPath`, `QuadraticPath`, or `HyperPath`. The `PointType`
//! determines drawing style (filled circle vs. open square) and drag
//! behavior (smooth points maintain tangent continuity).

use crate::document::model::entity_id::EntityId;
use crate::outline::path::hyper_model::{self as workspace, PointType as WsPointType};
use kurbo::Point;

/// A point type in a bezier path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointType {
    /// A point on the curve.
    OnCurve {
        /// Whether this is a smooth point (tangent continuity) or a
        /// corner.
        smooth: bool,
    },
    /// An off-curve control point (bezier handle).
    OffCurve {
        /// Whether this is an automatically positioned handle.
        auto: bool,
    },
}

impl PointType {
    /// Returns `true` for `OnCurve` points.
    pub fn is_on_curve(&self) -> bool {
        matches!(self, PointType::OnCurve { .. })
    }

    /// Returns `true` for `OffCurve` handles.
    pub fn is_off_curve(&self) -> bool {
        matches!(self, PointType::OffCurve { .. })
    }
}

/// A point in a bezier path with unique identity.
#[derive(Debug, Clone)]
pub struct PathPoint {
    /// Unique identity for selection and hit testing.
    pub id: EntityId,
    /// Position in design space.
    pub point: Point,
    /// On-curve or off-curve classification.
    pub typ: PointType,
}

impl PathPoint {
    fn new(point: Point, typ: PointType) -> Self {
        Self {
            id: EntityId::next(),
            point,
            typ,
        }
    }

    /// Returns `true` when this point lies on the curve.
    pub fn is_on_curve(&self) -> bool {
        self.typ.is_on_curve()
    }

    /// Returns `true` when this point is a control handle.
    pub fn is_off_curve(&self) -> bool {
        self.typ.is_off_curve()
    }

    /// Convert from a workspace contour point (norad-shaped).
    pub fn from_contour_point(pt: &workspace::ContourPoint) -> Self {
        let point = Point::new(pt.x, pt.y);
        let typ = match pt.point_type {
            WsPointType::OffCurve => PointType::OffCurve { auto: false },
            WsPointType::Hyper => PointType::OnCurve { smooth: true },
            WsPointType::HyperCorner => PointType::OnCurve { smooth: false },
            _ => PointType::OnCurve { smooth: pt.smooth },
        };
        Self::new(point, typ)
    }

    /// Convert from a workspace contour point for quadratic paths.
    pub fn from_contour_point_quadratic(pt: &workspace::ContourPoint) -> Self {
        let point = Point::new(pt.x, pt.y);
        let typ = match pt.point_type {
            WsPointType::OffCurve => PointType::OffCurve { auto: false },
            _ => PointType::OnCurve { smooth: pt.smooth },
        };
        Self::new(point, typ)
    }
}
