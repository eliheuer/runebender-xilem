// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Viewport transformation between design space and screen space

/// Viewport transformation between design space and screen space
#[derive(Debug, Clone)]
pub struct ViewPort {
    /// Scroll offset in screen space
    pub offset: kurbo::Vec2,

    /// Zoom level (screen pixels per design unit)
    pub zoom: f64,
}

impl ViewPort {
    /// Create a new viewport with default settings
    pub fn new() -> Self {
        Self {
            offset: kurbo::Vec2::ZERO,
            zoom: 1.0,
        }
    }

    /// Convert a point from design space to screen space
    pub fn to_screen(&self, point: kurbo::Point) -> kurbo::Point {
        // Design space: Y increases upward (font coordinates)
        // Screen space: Y increases downward (UI coordinates)
        // Apply: scale, flip Y, translate by offset
        kurbo::Point::new(
            point.x * self.zoom + self.offset.x,
            -point.y * self.zoom + self.offset.y,
        )
    }

    /// Convert a point from screen space to design space
    pub fn screen_to_design(&self, point: kurbo::Point) -> kurbo::Point {
        kurbo::Point::new(
            (point.x - self.offset.x) / self.zoom,
            -(point.y - self.offset.y) / self.zoom,
        )
    }

    /// Get the affine transformation from design space to screen
    /// space
    pub fn affine(&self) -> kurbo::Affine {
        // Build transformation: scale, flip Y, translate
        kurbo::Affine::new([
            self.zoom,      // x scale
            0.0,            // x skew
            0.0,            // y skew
            -self.zoom,     // y scale (negative for Y-flip)
            self.offset.x,  // x translation
            self.offset.y,  // y translation
        ])
    }
}

impl Default for ViewPort {
    fn default() -> Self {
        Self::new()
    }
}

