// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

//! Real-time text shaping engine for font editing.
//!
//! This module provides script-specific shaping that works directly with
//! font source files (UFO, Glyphs, etc.) without requiring font compilation.
//! The shaping is designed for interactive editing, with incremental updates
//! as the user types.
//!
//! # Architecture
//!
//! The shaping engine is built around a few key concepts:
//!
//! - **GlyphProvider**: A trait that abstracts font data access, allowing
//!   the shaper to work with any font source format.
//!
//! - **PositionalForm**: Represents the contextual form of a character
//!   (isolated, initial, medial, final) for cursive scripts like Arabic.
//!
//! - **ShapedGlyph**: The result of shaping a single character, containing
//!   the resolved glyph name and positioning information.
//!
//! # Supported Scripts
//!
//! Currently implemented:
//! - Arabic (with full contextual joining)
//!
//! Planned for future:
//! - Hebrew
//! - Other RTL scripts
//!
//! # Example
//!
//! ```ignore
//! use crate::shaping::{ArabicShaper, GlyphProvider, PositionalForm};
//!
//! let shaper = ArabicShaper::new();
//! let text: Vec<char> = "بسم".chars().collect();
//! let shaped = shaper.shape(&text, &font_provider);
//!
//! // shaped[0].form == PositionalForm::Initial  (beh)
//! // shaped[1].form == PositionalForm::Medial   (seen)
//! // shaped[2].form == PositionalForm::Final    (meem)
//! ```

pub mod arabic;
pub mod unicode_data;

pub use arabic::ArabicShaper;
pub use unicode_data::is_arabic;

/// Text direction for rendering and input handling
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextDirection {
    /// Left-to-right text (Latin, Cyrillic, etc.)
    #[default]
    LeftToRight,
    /// Right-to-left text (Arabic, Hebrew, etc.)
    RightToLeft,
}

impl TextDirection {
    /// Returns true if this is RTL direction
    pub fn is_rtl(&self) -> bool {
        matches!(self, Self::RightToLeft)
    }

    /// Returns true if this is LTR direction
    pub fn is_ltr(&self) -> bool {
        matches!(self, Self::LeftToRight)
    }

    /// Get a short name for display
    pub fn short_name(&self) -> &'static str {
        match self {
            Self::LeftToRight => "LTR",
            Self::RightToLeft => "RTL",
        }
    }
}

/// Positional forms for cursive scripts (Arabic, Syriac, etc.)
///
/// In cursive scripts, characters change shape depending on their position
/// within a connected sequence. This enum represents the four possible forms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PositionalForm {
    /// Standalone form - character not connected to neighbors
    #[default]
    Isolated,
    /// Beginning of a connected sequence
    Initial,
    /// Middle of a connected sequence
    Medial,
    /// End of a connected sequence
    Final,
}

impl PositionalForm {
    /// Get the standard glyph name suffix for this form.
    ///
    /// These suffixes follow the Adobe/OpenType naming convention:
    /// - Isolated: no suffix (base glyph name)
    /// - Initial: `.init`
    /// - Medial: `.medi`
    /// - Final: `.fina`
    pub fn suffix(&self) -> &'static str {
        match self {
            Self::Isolated => "",
            Self::Initial => ".init",
            Self::Medial => ".medi",
            Self::Final => ".fina",
        }
    }

    /// Get a human-readable name for this form.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Isolated => "isolated",
            Self::Initial => "initial",
            Self::Medial => "medial",
            Self::Final => "final",
        }
    }
}

/// Result of shaping a single character.
///
/// Contains all the information needed to render the shaped glyph.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapedGlyph {
    /// Base glyph name (without positional suffix)
    pub base_name: String,
    /// Full glyph name (with positional suffix if applicable)
    pub glyph_name: String,
    /// Unicode codepoint of the original character
    pub codepoint: char,
    /// Positional form (for cursive scripts)
    pub form: PositionalForm,
    /// Horizontal advance width
    pub advance_width: f64,
}

impl ShapedGlyph {
    /// Create a new shaped glyph.
    pub fn new(
        base_name: String,
        glyph_name: String,
        codepoint: char,
        form: PositionalForm,
        advance_width: f64,
    ) -> Self {
        Self {
            base_name,
            glyph_name,
            codepoint,
            form,
            advance_width,
        }
    }
}

/// Trait for providing glyph information from any font source format.
///
/// This abstraction allows the shaping engine to work with UFO, Glyphs,
/// or any other font format without coupling to a specific implementation.
///
/// # Implementation Notes
///
/// Implementations should:
/// - Return glyph names following standard naming conventions
/// - Handle missing glyphs gracefully (return None)
/// - Be efficient for repeated lookups (consider caching)
pub trait GlyphProvider {
    /// Check if a glyph with this name exists in the font.
    fn has_glyph(&self, name: &str) -> bool;

    /// Get the horizontal advance width for a glyph.
    ///
    /// Returns None if the glyph doesn't exist.
    fn advance_width(&self, name: &str) -> Option<f64>;

    /// Get the base glyph name for a Unicode codepoint.
    ///
    /// This should return the glyph name without any positional suffix.
    /// For example, for Arabic beh (U+0628), this should return "beh-ar"
    /// not "beh-ar.init".
    ///
    /// Returns None if no glyph exists for this codepoint.
    fn base_glyph_for_codepoint(&self, c: char) -> Option<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_positional_form_suffix() {
        assert_eq!(PositionalForm::Isolated.suffix(), "");
        assert_eq!(PositionalForm::Initial.suffix(), ".init");
        assert_eq!(PositionalForm::Medial.suffix(), ".medi");
        assert_eq!(PositionalForm::Final.suffix(), ".fina");
    }

    #[test]
    fn test_positional_form_name() {
        assert_eq!(PositionalForm::Isolated.name(), "isolated");
        assert_eq!(PositionalForm::Initial.name(), "initial");
        assert_eq!(PositionalForm::Medial.name(), "medial");
        assert_eq!(PositionalForm::Final.name(), "final");
    }

    #[test]
    fn test_shaped_glyph_new() {
        let glyph = ShapedGlyph::new(
            "beh-ar".to_string(),
            "beh-ar.init".to_string(),
            '\u{0628}',
            PositionalForm::Initial,
            500.0,
        );

        assert_eq!(glyph.base_name, "beh-ar");
        assert_eq!(glyph.glyph_name, "beh-ar.init");
        assert_eq!(glyph.codepoint, '\u{0628}');
        assert_eq!(glyph.form, PositionalForm::Initial);
        assert_eq!(glyph.advance_width, 500.0);
    }
}
