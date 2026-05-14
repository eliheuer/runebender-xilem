// Glyph categories for the grid-view filter sidebar.
//
// Eight buckets (one "All" + seven derived from Unicode general
// category groups). The mapping in `from_codepoint` is the same one
// runebender-xilem's old `components::category_panel::GlyphCategory`
// used; it moved here so both editors filter the same way.

use unicode_general_category::{GeneralCategory, get_general_category};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlyphCategory {
    #[default]
    All,
    Letter,
    Number,
    Punctuation,
    Symbol,
    Mark,
    Separator,
    Other,
}

impl GlyphCategory {
    pub fn display_name(&self) -> &'static str {
        match self {
            GlyphCategory::All => "All",
            GlyphCategory::Letter => "Letter",
            GlyphCategory::Number => "Number",
            GlyphCategory::Punctuation => "Punctuation",
            GlyphCategory::Symbol => "Symbol",
            GlyphCategory::Mark => "Mark",
            GlyphCategory::Separator => "Separator",
            GlyphCategory::Other => "Other",
        }
    }

    pub fn all_categories() -> &'static [GlyphCategory] {
        &[
            GlyphCategory::All,
            GlyphCategory::Letter,
            GlyphCategory::Number,
            GlyphCategory::Punctuation,
            GlyphCategory::Symbol,
            GlyphCategory::Mark,
            GlyphCategory::Separator,
            GlyphCategory::Other,
        ]
    }

    /// Map a Unicode codepoint to a `GlyphCategory`. Glyphs without
    /// a codepoint (e.g. `.notdef`) should be assigned `Other` by
    /// the caller — this fn doesn't see that case.
    pub fn from_codepoint(c: char) -> GlyphCategory {
        match get_general_category(c) {
            GeneralCategory::UppercaseLetter
            | GeneralCategory::LowercaseLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter => GlyphCategory::Letter,

            GeneralCategory::DecimalNumber
            | GeneralCategory::LetterNumber
            | GeneralCategory::OtherNumber => GlyphCategory::Number,

            GeneralCategory::ConnectorPunctuation
            | GeneralCategory::DashPunctuation
            | GeneralCategory::OpenPunctuation
            | GeneralCategory::ClosePunctuation
            | GeneralCategory::InitialPunctuation
            | GeneralCategory::FinalPunctuation
            | GeneralCategory::OtherPunctuation => GlyphCategory::Punctuation,

            GeneralCategory::MathSymbol
            | GeneralCategory::CurrencySymbol
            | GeneralCategory::ModifierSymbol
            | GeneralCategory::OtherSymbol => GlyphCategory::Symbol,

            GeneralCategory::NonspacingMark
            | GeneralCategory::SpacingMark
            | GeneralCategory::EnclosingMark => GlyphCategory::Mark,

            GeneralCategory::SpaceSeparator
            | GeneralCategory::LineSeparator
            | GeneralCategory::ParagraphSeparator => GlyphCategory::Separator,

            _ => GlyphCategory::Other,
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
        assert_eq!(GlyphCategory::from_codepoint('.'), GlyphCategory::Punctuation);
        assert_eq!(GlyphCategory::from_codepoint('('), GlyphCategory::Punctuation);
        assert_eq!(GlyphCategory::from_codepoint('+'), GlyphCategory::Symbol);
        assert_eq!(GlyphCategory::from_codepoint('$'), GlyphCategory::Symbol);
    }

    #[test]
    fn separators_and_other() {
        assert_eq!(GlyphCategory::from_codepoint(' '), GlyphCategory::Separator);
        // Control character → Other
        assert_eq!(GlyphCategory::from_codepoint('\u{0007}'), GlyphCategory::Other);
    }
}
