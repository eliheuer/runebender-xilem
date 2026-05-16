// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Platform-independent theme tokens shared by Runebender frontends.
//!
//! These constants mirror `themes/runebender.json` but avoid exposing
//! any UI-toolkit color type from core.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorRgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl ColorRgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 0xff }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

pub mod base {
    use super::ColorRgba;

    pub const A: ColorRgba = ColorRgba::rgb(0x10, 0x10, 0x10);
    pub const C: ColorRgba = ColorRgba::rgb(0x30, 0x30, 0x30);
    pub const F: ColorRgba = ColorRgba::rgb(0x60, 0x60, 0x60);
    pub const H: ColorRgba = ColorRgba::rgb(0x80, 0x80, 0x80);
    pub const I: ColorRgba = ColorRgba::rgb(0x90, 0x90, 0x90);
    pub const J: ColorRgba = ColorRgba::rgb(0xa0, 0xa0, 0xa0);
}

pub mod app {
    pub const BACKGROUND: super::ColorRgba = super::base::A;
}

pub mod grid {
    use super::ColorRgba;

    pub const CELL_SELECTED_OUTLINE: ColorRgba = ColorRgba::rgb(0x66, 0xee, 0x88);
    pub const GLYPH: ColorRgba = super::base::J;
}

pub mod path {
    pub const FILL: super::ColorRgba = super::base::F;
    pub const PREVIEW_FILL: super::ColorRgba = super::base::H;
}

pub mod component {
    use super::ColorRgba;

    pub const FILL: ColorRgba = ColorRgba::rgb(0x66, 0x99, 0xcc);
    pub const SELECTED_FILL: ColorRgba = ColorRgba::rgb(0x88, 0xbb, 0xff);
}

pub mod metrics {
    pub const GUIDE: super::ColorRgba = super::grid::CELL_SELECTED_OUTLINE;
}

pub mod kerning {
    use super::ColorRgba;

    pub const ACTIVE_GLYPH: ColorRgba = ColorRgba::rgb(0x00, 0xff, 0xcc);
    pub const PREVIOUS_GLYPH: ColorRgba = ColorRgba::rgb(0xff, 0xaa, 0x33);
}

pub mod design_grid {
    use super::ColorRgba;

    pub const FINE: ColorRgba = ColorRgba::rgba(0x88, 0x88, 0x88, 0x40);
    pub const COARSE: ColorRgba = ColorRgba::rgba(0x88, 0x88, 0x88, 0x58);
}

pub mod handle {
    pub const LINE: super::ColorRgba = super::base::I;
}

pub mod point {
    use super::ColorRgba;

    pub const SMOOTH_INNER: ColorRgba = ColorRgba::rgb(0x57, 0x9a, 0xff);
    pub const SMOOTH_OUTER: ColorRgba = ColorRgba::rgb(0x44, 0x28, 0xec);
    pub const CORNER_INNER: ColorRgba = ColorRgba::rgb(0x6a, 0xe7, 0x56);
    pub const CORNER_OUTER: ColorRgba = ColorRgba::rgb(0x20, 0x8e, 0x56);
    pub const OFFCURVE_INNER: ColorRgba = ColorRgba::rgb(0xcc, 0x99, 0xff);
    pub const OFFCURVE_OUTER: ColorRgba = ColorRgba::rgb(0x99, 0x00, 0xff);
    pub const HYPER_INNER: ColorRgba = ColorRgba::rgb(0x66, 0xcc, 0xdd);
    pub const HYPER_OUTER: ColorRgba = ColorRgba::rgb(0x00, 0x99, 0xaa);
    pub const START_NODE_INNER: ColorRgba = ColorRgba::rgb(0x6a, 0xe7, 0x56);
    pub const START_NODE_OUTER: ColorRgba = ColorRgba::rgb(0x20, 0x8e, 0x56);
    pub const SELECTED_INNER: ColorRgba = ColorRgba::rgb(0xff, 0xee, 0x55);
    pub const SELECTED_OUTER: ColorRgba = ColorRgba::rgb(0xff, 0xaa, 0x33);
}

pub mod segment {
    use super::ColorRgba;

    pub const HOVER: ColorRgba = ColorRgba::rgb(0xff, 0xaa, 0x33);
}

pub mod selection {
    use super::ColorRgba;

    pub const RECT_FILL: ColorRgba = ColorRgba::rgba(0xff, 0xaa, 0x33, 0x20);
    pub const RECT_STROKE: ColorRgba = ColorRgba::rgb(0xff, 0xaa, 0x33);
}

pub mod cursor {
    use super::ColorRgba;

    pub const TEXT: ColorRgba = ColorRgba::rgb(0x00, 0x7a, 0xff);
}
