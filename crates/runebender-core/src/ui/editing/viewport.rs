// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Viewport transformation between design space and screen space.
//!
//! Font coordinates are Y-up with the origin at the baseline; screen
//! coordinates are Y-down with the origin at the top left. `ViewPort`
//! stores an offset and a zoom level, and its `to_screen` and
//! `screen_to_design` conversions handle the Y-flip, the scaling, and
//! the translation. Front-ends use it to map pointer events into
//! design coordinates and to position glyphs on screen. Shared by
//! every Runebender editor.

/// Viewport transformation between design space and screen space.
#[derive(Debug, Clone)]
pub struct ViewPort {
    /// Scroll offset in screen space.
    pub offset: kurbo::Vec2,

    /// Zoom level, in screen pixels per design unit.
    pub zoom: f64,
}

impl ViewPort {
    /// Create a viewport with zoom `1.0` and no offset.
    pub fn new() -> Self {
        Self {
            offset: kurbo::Vec2::ZERO,
            zoom: 1.0,
        }
    }

    /// Convert a point from design space to screen space.
    pub fn to_screen(&self, point: kurbo::Point) -> kurbo::Point {
        // Design space: Y increases upward (font coordinates)
        // Screen space: Y increases downward (UI coordinates)
        // Apply: scale, flip Y, translate by offset
        kurbo::Point::new(
            point.x * self.zoom + self.offset.x,
            -point.y * self.zoom + self.offset.y,
        )
    }

    /// Convert a point from screen space to design space.
    pub fn screen_to_design(&self, point: kurbo::Point) -> kurbo::Point {
        kurbo::Point::new(
            (point.x - self.offset.x) / self.zoom,
            -(point.y - self.offset.y) / self.zoom,
        )
    }

    /// The affine transformation from design space to screen space.
    pub fn affine(&self) -> kurbo::Affine {
        // Build transformation: scale, flip Y, translate
        kurbo::Affine::new([
            self.zoom,     // x scale
            0.0,           // x skew
            0.0,           // y skew
            -self.zoom,    // y scale (negative for Y-flip)
            self.offset.x, // x translation
            self.offset.y, // y translation
        ])
    }
}

impl Default for ViewPort {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewPort {
    /// Fit the glyph box into a canvas, centered.
    ///
    /// The box spans the advance width across and descender to
    /// ascender up. `fill` is the fraction of the canvas height the
    /// box uses; `0.62` leaves comfortable margins.
    pub fn fit_to_canvas(
        &mut self,
        canvas_width: f64,
        canvas_height: f64,
        advance: f64,
        ascender: f64,
        descender: f64,
        fill: f64,
    ) {
        let design_height = (ascender - descender).max(1.0);
        self.zoom = (canvas_height * fill) / design_height;
        let design_center_y = (ascender + descender) / 2.0;
        self.offset = kurbo::Vec2::new(
            (canvas_width - advance * self.zoom) / 2.0,
            canvas_height / 2.0 + design_center_y * self.zoom,
        );
    }

    /// Zoom by `factor`, keeping the design point under the given
    /// screen position fixed. The new zoom is clamped to `min..=max`.
    pub fn zoom_about(&mut self, screen: kurbo::Point, factor: f64, min: f64, max: f64) {
        let anchor = self.screen_to_design(screen);
        self.zoom = (self.zoom * factor).clamp(min, max);
        self.offset = kurbo::Vec2::new(
            screen.x - anchor.x * self.zoom,
            screen.y + anchor.y * self.zoom,
        );
    }

    /// Pan by a screen-space delta.
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.offset += kurbo::Vec2::new(dx, dy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_zoom_anchor() {
        let mut vp = ViewPort::new();
        vp.fit_to_canvas(1000.0, 800.0, 600.0, 800.0, -200.0, 0.62);
        let d = kurbo::Point::new(300.0, 400.0);
        let s = vp.to_screen(d);
        let back = vp.screen_to_design(s);
        assert!((back - d).hypot() < 1e-9);
        // Affine agrees with to_screen.
        let via_affine = vp.affine() * d;
        assert!((via_affine - s).hypot() < 1e-9);
        // Cursor-anchored zoom keeps the anchor fixed on screen.
        let anchor_screen = kurbo::Point::new(420.0, 260.0);
        let anchor_design = vp.screen_to_design(anchor_screen);
        vp.zoom_about(anchor_screen, 1.7, 0.01, 100.0);
        let after = vp.to_screen(anchor_design);
        assert!((after - anchor_screen).hypot() < 1e-6, "anchor drifted");
    }
}
