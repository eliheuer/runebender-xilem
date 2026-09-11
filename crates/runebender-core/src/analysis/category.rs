// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Glyph categories for the grid-view filter sidebar.
//!
//! Eight buckets (one "All" + seven derived from Unicode general
//! category groups). The mapping in `from_codepoint` is the same one
//! runebender-xilem's old `components::category_panel::GlyphCategory`
//! used; it moved here so both editors filter the same way.

use unicode_general_category::{GeneralCategory, get_general_category};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// A coarse glyph bucket used by the grid-view category filter.
pub enum GlyphCategory {
    #[default]
    /// Every glyph; no filtering.
    All,
    /// Unicode letters (`Lu`, `Ll`, `Lt`, `Lm`, `Lo`).
    Letter,
    /// Unicode numbers (`Nd`, `Nl`, `No`).
    Number,
    /// Unicode punctuation (all `P*` categories).
    Punctuation,
    /// Unicode symbols (`Sm`, `Sc`, `Sk`, `So`).
    Symbol,
    /// Combining and enclosing marks (`Mn`, `Mc`, `Me`).
    Mark,
    /// Space, line, and paragraph separators (`Zs`, `Zl`, `Zp`).
    Separator,
    /// Everything else: controls, formats, unassigned, and glyphs with no codepoint.
    Other,
}

impl GlyphCategory {
    /// Returns the label shown in the sidebar for this category.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Letter => "Letter",
            Self::Number => "Number",
            Self::Punctuation => "Punctuation",
            Self::Symbol => "Symbol",
            Self::Mark => "Mark",
            Self::Separator => "Separator",
            Self::Other => "Other",
        }
    }

    /// Returns every category in sidebar display order, starting with `All`.
    pub fn all_categories() -> &'static [Self] {
        &[
            Self::All,
            Self::Letter,
            Self::Number,
            Self::Punctuation,
            Self::Symbol,
            Self::Mark,
            Self::Separator,
            Self::Other,
        ]
    }

    /// Map a Unicode codepoint to a `GlyphCategory`.
    ///
    /// Glyphs without a codepoint, such as `.notdef`, never reach
    /// this function. The caller should assign them `Other`.
    pub fn from_codepoint(c: char) -> Self {
        match get_general_category(c) {
            GeneralCategory::UppercaseLetter
            | GeneralCategory::LowercaseLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter => Self::Letter,

            GeneralCategory::DecimalNumber
            | GeneralCategory::LetterNumber
            | GeneralCategory::OtherNumber => Self::Number,

            GeneralCategory::ConnectorPunctuation
            | GeneralCategory::DashPunctuation
            | GeneralCategory::OpenPunctuation
            | GeneralCategory::ClosePunctuation
            | GeneralCategory::InitialPunctuation
            | GeneralCategory::FinalPunctuation
            | GeneralCategory::OtherPunctuation => Self::Punctuation,

            GeneralCategory::MathSymbol
            | GeneralCategory::CurrencySymbol
            | GeneralCategory::ModifierSymbol
            | GeneralCategory::OtherSymbol => Self::Symbol,

            GeneralCategory::NonspacingMark
            | GeneralCategory::SpacingMark
            | GeneralCategory::EnclosingMark => Self::Mark,

            GeneralCategory::SpaceSeparator
            | GeneralCategory::LineSeparator
            | GeneralCategory::ParagraphSeparator => Self::Separator,

            _ => Self::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_map_to_letter() {
        assert_eq!(GlyphCategory::from_codepoint('A'), GlyphCategory::Letter);
        assert_eq!(GlyphCategory::from_codepoint('z'), GlyphCategory::Letter);
        assert_eq!(GlyphCategory::from_codepoint('α'), GlyphCategory::Letter);
    }

    #[test]
    fn digits_map_to_number() {
        assert_eq!(GlyphCategory::from_codepoint('0'), GlyphCategory::Number);
        assert_eq!(GlyphCategory::from_codepoint('9'), GlyphCategory::Number);
    }

    #[test]
    fn punct_and_symbol() {
        assert_eq!(
            GlyphCategory::from_codepoint('.'),
            GlyphCategory::Punctuation
        );
        assert_eq!(
            GlyphCategory::from_codepoint('('),
            GlyphCategory::Punctuation
        );
        assert_eq!(GlyphCategory::from_codepoint('+'), GlyphCategory::Symbol);
        assert_eq!(GlyphCategory::from_codepoint('$'), GlyphCategory::Symbol);
    }

    #[test]
    fn separators_and_other() {
        assert_eq!(GlyphCategory::from_codepoint(' '), GlyphCategory::Separator);
        // Control character → Other
        assert_eq!(
            GlyphCategory::from_codepoint('\u{0007}'),
            GlyphCategory::Other
        );
    }
}
