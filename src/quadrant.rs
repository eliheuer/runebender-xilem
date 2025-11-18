// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Quadrant selection for coordinate reference points
//!
//! This module defines quadrants within a rectangular space, used for
//! selecting which corner/edge/center to use as the reference point when
//! displaying or editing coordinates of multi-point selections.

use kurbo::{Point, Rect};

/// A quadrant within a 2D rectangular space
///
/// Used to determine which point in a bounding box should be used as the
/// reference for coordinate display and editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(Default)]
pub enum Quadrant {
    /// Top-left corner
    TopLeft,
    /// Top center
    Top,
    /// Top-right corner
    TopRight,
    /// Center left
    Left,
    /// Center
    #[default]
    Center,
    /// Center right
    Right,
    /// Bottom-left corner
    BottomLeft,
    /// Bottom center
    Bottom,
    /// Bottom-right corner
    BottomRight,
}


#[allow(dead_code)]
impl Quadrant {
    /// Returns the point within a rectangle corresponding to this quadrant
    ///
    /// For screen space coordinates (y increases downward)
    pub fn point_in_rect(&self, rect: Rect) -> Point {
        match self {
            Quadrant::TopLeft => Point::new(rect.min_x(), rect.min_y()),
            Quadrant::Top => Point::new(rect.center().x, rect.min_y()),
            Quadrant::TopRight => Point::new(rect.max_x(), rect.min_y()),
            Quadrant::Left => Point::new(rect.min_x(), rect.center().y),
            Quadrant::Center => rect.center(),
            Quadrant::Right => Point::new(rect.max_x(), rect.center().y),
            Quadrant::BottomLeft => Point::new(rect.min_x(), rect.max_y()),
            Quadrant::Bottom => Point::new(rect.center().x, rect.max_y()),
            Quadrant::BottomRight => Point::new(rect.max_x(), rect.max_y()),
        }
    }

    /// Returns the point within a design space rectangle corresponding to this quadrant
    ///
    /// Design space has y increasing upward (opposite of screen space)
    pub fn point_in_dspace_rect(&self, rect: Rect) -> Point {
        match self {
            Quadrant::TopLeft => Point::new(rect.min_x(), rect.max_y()),
            Quadrant::Top => Point::new(rect.center().x, rect.max_y()),
            Quadrant::TopRight => Point::new(rect.max_x(), rect.max_y()),
            Quadrant::Left => Point::new(rect.min_x(), rect.center().y),
            Quadrant::Center => rect.center(),
            Quadrant::Right => Point::new(rect.max_x(), rect.center().y),
            Quadrant::BottomLeft => Point::new(rect.min_x(), rect.min_y()),
            Quadrant::Bottom => Point::new(rect.center().x, rect.min_y()),
            Quadrant::BottomRight => Point::new(rect.max_x(), rect.min_y()),
        }
    }

    /// Determines which quadrant a point falls within based on a rectangular bounds
    ///
    /// Divides the rectangle into a 3x3 grid and returns the quadrant
    pub fn for_point_in_bounds(point: Point, bounds: Rect) -> Self {
        let third_width = bounds.width() / 3.0;
        let third_height = bounds.height() / 3.0;

        let left_edge = bounds.min_x() + third_width;
        let right_edge = bounds.max_x() - third_width;
        let top_edge = bounds.min_y() + third_height;
        let bottom_edge = bounds.max_y() - third_height;

        let x_zone = if point.x < left_edge {
            0 // Left
        } else if point.x > right_edge {
            2 // Right
        } else {
            1 // Center
        };

        let y_zone = if point.y < top_edge {
            0 // Top
        } else if point.y > bottom_edge {
            2 // Bottom
        } else {
            1 // Middle
        };

        match (x_zone, y_zone) {
            (0, 0) => Quadrant::TopLeft,
            (1, 0) => Quadrant::Top,
            (2, 0) => Quadrant::TopRight,
            (0, 1) => Quadrant::Left,
            (1, 1) => Quadrant::Center,
            (2, 1) => Quadrant::Right,
            (0, 2) => Quadrant::BottomLeft,
            (1, 2) => Quadrant::Bottom,
            (2, 2) => Quadrant::BottomRight,
            _ => Quadrant::Center,
        }
    }

    /// Returns the inverse/opposite quadrant
    ///
    /// Useful for determining the opposite corner during transformations
    pub fn inverse(&self) -> Self {
        match self {
            Quadrant::TopLeft => Quadrant::BottomRight,
            Quadrant::Top => Quadrant::Bottom,
            Quadrant::TopRight => Quadrant::BottomLeft,
            Quadrant::Left => Quadrant::Right,
            Quadrant::Center => Quadrant::Center,
            Quadrant::Right => Quadrant::Left,
            Quadrant::BottomLeft => Quadrant::TopRight,
            Quadrant::Bottom => Quadrant::Top,
            Quadrant::BottomRight => Quadrant::TopLeft,
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

        // Test corners
        assert_eq!(
            Quadrant::for_point_in_bounds(Point::new(10.0, 10.0), bounds),
            Quadrant::TopLeft
        );
        assert_eq!(
            Quadrant::for_point_in_bounds(Point::new(80.0, 80.0), bounds),
            Quadrant::BottomRight
        );

        // Test center
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
