// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Ported from runebender-xilem/src/path/quadrant.rs (Apache-2.0).

//! Quadrant selection for coordinate reference points.
//!
//! Defines quadrants within a rectangular space, used for selecting
//! which corner/edge/center to use as the reference point when
//! displaying or editing coordinates of multi-point selections.

use kurbo::{Point, Rect};

/// A quadrant within a 2D rectangular space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Quadrant {
    /// The top-left corner.
    TopLeft,
    /// The middle of the top edge.
    Top,
    /// The top-right corner.
    TopRight,
    /// The middle of the left edge.
    Left,
    #[default]
    /// The center of the rect; the default reference point.
    Center,
    /// The middle of the right edge.
    Right,
    /// The bottom-left corner.
    BottomLeft,
    /// The middle of the bottom edge.
    Bottom,
    /// The bottom-right corner.
    BottomRight,
}

impl Quadrant {
    /// Point within a screen-space rect (y increases downward).
    pub fn point_in_rect(&self, rect: Rect) -> Point {
        match self {
            Self::TopLeft => Point::new(rect.min_x(), rect.min_y()),
            Self::Top => Point::new(rect.center().x, rect.min_y()),
            Self::TopRight => Point::new(rect.max_x(), rect.min_y()),
            Self::Left => Point::new(rect.min_x(), rect.center().y),
            Self::Center => rect.center(),
            Self::Right => Point::new(rect.max_x(), rect.center().y),
            Self::BottomLeft => Point::new(rect.min_x(), rect.max_y()),
            Self::Bottom => Point::new(rect.center().x, rect.max_y()),
            Self::BottomRight => Point::new(rect.max_x(), rect.max_y()),
        }
    }

    /// Point within a design-space rect (y increases upward).
    pub fn point_in_dspace_rect(&self, rect: Rect) -> Point {
        match self {
            Self::TopLeft => Point::new(rect.min_x(), rect.max_y()),
            Self::Top => Point::new(rect.center().x, rect.max_y()),
            Self::TopRight => Point::new(rect.max_x(), rect.max_y()),
            Self::Left => Point::new(rect.min_x(), rect.center().y),
            Self::Center => rect.center(),
            Self::Right => Point::new(rect.max_x(), rect.center().y),
            Self::BottomLeft => Point::new(rect.min_x(), rect.min_y()),
            Self::Bottom => Point::new(rect.center().x, rect.min_y()),
            Self::BottomRight => Point::new(rect.max_x(), rect.min_y()),
        }
    }

    /// Determine which quadrant a point falls in by dividing `bounds`
    /// into a 3x3 grid.
    pub fn for_point_in_bounds(point: Point, bounds: Rect) -> Self {
        let third_width = bounds.width() / 3.0;
        let third_height = bounds.height() / 3.0;

        let left_edge = bounds.min_x() + third_width;
        let right_edge = bounds.max_x() - third_width;
        let top_edge = bounds.min_y() + third_height;
        let bottom_edge = bounds.max_y() - third_height;

        let x_zone = if point.x < left_edge {
            0
        } else if point.x > right_edge {
            2
        } else {
            1
        };

        let y_zone = if point.y < top_edge {
            0
        } else if point.y > bottom_edge {
            2
        } else {
            1
        };

        match (x_zone, y_zone) {
            (0, 0) => Self::TopLeft,
            (1, 0) => Self::Top,
            (2, 0) => Self::TopRight,
            (0, 1) => Self::Left,
            (1, 1) => Self::Center,
            (2, 1) => Self::Right,
            (0, 2) => Self::BottomLeft,
            (1, 2) => Self::Bottom,
            (2, 2) => Self::BottomRight,
            _ => Self::Center,
        }
    }

    /// Opposite corner, useful during transforms.
    pub fn inverse(&self) -> Self {
        match self {
            Self::TopLeft => Self::BottomRight,
            Self::Top => Self::Bottom,
            Self::TopRight => Self::BottomLeft,
            Self::Left => Self::Right,
            Self::Center => Self::Center,
            Self::Right => Self::Left,
            Self::BottomLeft => Self::TopRight,
            Self::Bottom => Self::Top,
            Self::BottomRight => Self::TopLeft,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_in_dspace_rect() {
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);

        assert_eq!(
            Quadrant::TopLeft.point_in_dspace_rect(rect),
            Point::new(0.0, 100.0)
        );
        assert_eq!(
            Quadrant::BottomRight.point_in_dspace_rect(rect),
            Point::new(100.0, 0.0)
        );
        assert_eq!(
            Quadrant::Center.point_in_dspace_rect(rect),
            Point::new(50.0, 50.0)
        );
    }

    #[test]
    fn test_for_point_in_bounds() {
        let bounds = Rect::new(0.0, 0.0, 90.0, 90.0);

        assert_eq!(
            Quadrant::for_point_in_bounds(Point::new(10.0, 10.0), bounds),
            Quadrant::TopLeft
        );
        assert_eq!(
            Quadrant::for_point_in_bounds(Point::new(80.0, 80.0), bounds),
            Quadrant::BottomRight
        );
        assert_eq!(
            Quadrant::for_point_in_bounds(Point::new(45.0, 45.0), bounds),
            Quadrant::Center
        );
    }

    #[test]
    fn test_inverse() {
        assert_eq!(Quadrant::TopLeft.inverse(), Quadrant::BottomRight);
        assert_eq!(Quadrant::Center.inverse(), Quadrant::Center);
        assert_eq!(Quadrant::Right.inverse(), Quadrant::Left);
    }
}
