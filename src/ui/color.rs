// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The one colour type core exposes.
//!
//! Every editor resolves a theme token to this and converts it to
//! whatever its toolkit paints with, so no UI-toolkit colour type
//! reaches this crate.

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
