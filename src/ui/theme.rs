// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Platform-independent theme tokens shared by Runebender frontends.
//!
//! These constants mirror `themes/runebender.json` but avoid exposing
//! any UI-toolkit color type from core.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// An sRGB color with 8-bit channels, the only color type core exposes.
pub struct ColorRgba {
    /// Red channel, 0 to 255.
    pub r: u8,
    /// Green channel, 0 to 255.
    pub g: u8,
    /// Blue channel, 0 to 255.
    pub b: u8,
    /// Alpha channel, 0 (transparent) to 255 (opaque).
    pub a: u8,
}

impl ColorRgba {
    /// Builds an opaque color from red, green, and blue channels.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 0xff }
    }

    /// Builds a color from red, green, blue, and alpha channels.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

/// The neutral gray ramp every other token is built from, dark to light.
pub mod base {
    use super::ColorRgba;

    /// Darkest gray, `#101010`.
    pub const A: ColorRgba = ColorRgba::rgb(0x10, 0x10, 0x10);
    /// Gray `#303030`.
    pub const C: ColorRgba = ColorRgba::rgb(0x30, 0x30, 0x30);
    /// Gray `#606060`.
    pub const F: ColorRgba = ColorRgba::rgb(0x60, 0x60, 0x60);
    /// Gray `#808080`.
    pub const H: ColorRgba = ColorRgba::rgb(0x80, 0x80, 0x80);
    /// Gray `#909090`.
    pub const I: ColorRgba = ColorRgba::rgb(0x90, 0x90, 0x90);
    /// Gray `#a0a0a0`.
    pub const J: ColorRgba = ColorRgba::rgb(0xa0, 0xa0, 0xa0);
    /// Lightest gray, `#c0c0c0`.
    pub const L: ColorRgba = ColorRgba::rgb(0xc0, 0xc0, 0xc0);
}

/// Application window tokens.
pub mod app {
    /// Background color of the main window.
    pub const BACKGROUND: super::ColorRgba = super::base::A;
}

/// Glyph grid tokens.
pub mod grid {
    use super::ColorRgba;

    /// Outline drawn around the selected grid cell.
    pub const CELL_SELECTED_OUTLINE: ColorRgba = ColorRgba::rgb(0x66, 0xee, 0x88);
    /// Fill color of glyph outlines in the grid.
    pub const GLYPH: ColorRgba = super::base::J;
}

/// Glyph outline tokens for the editor and preview.
pub mod path {
    /// Outline stroke color used in edit mode (the glyph is stroked,
    /// not filled, while editing — see runebender-xilem path::STROKE).
    pub const STROKE: super::ColorRgba = super::base::L;
    /// Fill color of the glyph when not editing.
    pub const FILL: super::ColorRgba = super::base::F;
    /// Fill color of the glyph in preview mode.
    pub const PREVIEW_FILL: super::ColorRgba = super::base::H;
}

/// Component tokens for the editor.
pub mod component {
    use super::ColorRgba;

    /// Fill color of resolved component outlines.
    pub const FILL: ColorRgba = ColorRgba::rgb(0x66, 0x99, 0xcc);
    /// Fill color of a selected component.
    pub const SELECTED_FILL: ColorRgba = ColorRgba::rgb(0x88, 0xbb, 0xff);
}

/// Metrics line tokens.
pub mod metrics {
    /// Color of ascender, descender, and other metric guides.
    pub const GUIDE: super::ColorRgba = super::grid::CELL_SELECTED_OUTLINE;
}

/// Kerning view tokens.
pub mod kerning {
    use super::ColorRgba;

    /// Highlight for the glyph whose kerning is being edited.
    pub const ACTIVE_GLYPH: ColorRgba = ColorRgba::rgb(0x00, 0xff, 0xcc);
    /// Highlight for the glyph before the active one in the pair.
    pub const PREVIOUS_GLYPH: ColorRgba = ColorRgba::rgb(0xff, 0xaa, 0x33);
}

/// Design grid tokens.
pub mod design_grid {
    use super::ColorRgba;

    /// Color of the fine grid lines, semi-transparent.
    pub const FINE: ColorRgba = ColorRgba::rgba(0x88, 0x88, 0x88, 0x48);
    /// Color of the coarse grid lines, slightly more opaque.
    pub const COARSE: ColorRgba = ColorRgba::rgba(0x88, 0x88, 0x88, 0x58);
}

/// Control handle tokens.
pub mod handle {
    /// Color of the line from an on-curve point to its handle.
    pub const LINE: super::ColorRgba = super::base::I;
}

/// Point marker tokens. Each point kind has an inner fill and an outer ring.
pub mod point {
    use super::ColorRgba;

    /// Inner fill of a smooth on-curve point.
    pub const SMOOTH_INNER: ColorRgba = ColorRgba::rgb(0x57, 0x9a, 0xff);
    /// Outer ring of a smooth on-curve point.
    pub const SMOOTH_OUTER: ColorRgba = ColorRgba::rgb(0x44, 0x28, 0xec);
    /// Inner fill of a corner on-curve point.
    pub const CORNER_INNER: ColorRgba = ColorRgba::rgb(0x6a, 0xe7, 0x56);
    /// Outer ring of a corner on-curve point.
    pub const CORNER_OUTER: ColorRgba = ColorRgba::rgb(0x20, 0x8e, 0x56);
    /// Inner fill of an off-curve control point.
    pub const OFFCURVE_INNER: ColorRgba = ColorRgba::rgb(0xcc, 0x99, 0xff);
    /// Outer ring of an off-curve control point.
    pub const OFFCURVE_OUTER: ColorRgba = ColorRgba::rgb(0x99, 0x00, 0xff);
    /// Inner fill of a hyperbezier point.
    pub const HYPER_INNER: ColorRgba = ColorRgba::rgb(0x66, 0xcc, 0xdd);
    /// Outer ring of a hyperbezier point.
    pub const HYPER_OUTER: ColorRgba = ColorRgba::rgb(0x00, 0x99, 0xaa);
    /// Inner fill of a contour's first point.
    pub const START_NODE_INNER: ColorRgba = ColorRgba::rgb(0x6a, 0xe7, 0x56);
    /// Outer ring of a contour's first point.
    pub const START_NODE_OUTER: ColorRgba = ColorRgba::rgb(0x20, 0x8e, 0x56);
    /// Inner fill of a selected point, any kind.
    pub const SELECTED_INNER: ColorRgba = ColorRgba::rgb(0xff, 0xee, 0x55);
    /// Outer ring of a selected point, any kind.
    pub const SELECTED_OUTER: ColorRgba = ColorRgba::rgb(0xff, 0xaa, 0x33);
}

/// Segment tokens.
pub mod segment {
    use super::ColorRgba;

    /// Color of a path segment under the pointer.
    pub const HOVER: ColorRgba = ColorRgba::rgb(0xff, 0xaa, 0x33);
}

/// Marquee selection tokens.
pub mod selection {
    use super::ColorRgba;

    /// Fill of the drag-selection rectangle, semi-transparent.
    pub const RECT_FILL: ColorRgba = ColorRgba::rgba(0xff, 0xaa, 0x33, 0x20);
    /// Stroke of the drag-selection rectangle.
    pub const RECT_STROKE: ColorRgba = ColorRgba::rgb(0xff, 0xaa, 0x33);
}

/// Text cursor tokens.
pub mod cursor {
    use super::ColorRgba;

    /// Color of the caret in text fields and the text preview.
    pub const TEXT: ColorRgba = ColorRgba::rgb(0x00, 0x7a, 0xff);
}
