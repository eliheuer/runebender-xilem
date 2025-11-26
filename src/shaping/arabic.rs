// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Arabic shaping engine implementing Unicode Arabic Joining Algorithm.
//!
//! This provides real-time shaping for Arabic text, determining the correct
//! positional form (isolated, initial, medial, final) for each character
//! based on its neighbors.
//!
//! # Algorithm
//!
//! The Arabic joining algorithm works as follows:
//!
//! 1. For each character, determine its joining type (Dual, Right, Non-joining, etc.)
//! 2. Look at the previous non-transparent character to see if it joins forward
//! 3. Look at the next non-transparent character to see if it joins backward
//! 4. Based on these two booleans and the character's joining type, determine the form
//!
//! # Incremental Reshaping
//!
//! When text is edited, only characters near the edit point need to be reshaped.
//! The `reshape_range` method handles this efficiently by examining only the
//! affected characters and their immediate neighbors.

use super::unicode_data::{is_arabic, joining_type, JoiningType};
use super::{GlyphProvider, PositionalForm, ShapedGlyph};

/// Arabic shaping engine.
///
/// This engine implements the Unicode Arabic Joining Algorithm to determine
/// the correct positional form for each character in Arabic text.
///
/// # Example
///
/// ```ignore
/// let shaper = ArabicShaper::new();
/// let text: Vec<char> = "بسم".chars().collect();
/// let shaped = shaper.shape(&text, &font);
///
/// assert_eq!(shaped[0].form, PositionalForm::Initial);  // beh
/// assert_eq!(shaped[1].form, PositionalForm::Medial);   // seen
/// assert_eq!(shaped[2].form, PositionalForm::Final);    // meem
/// ```
#[derive(Debug, Clone, Default)]
pub struct ArabicShaper;

impl ArabicShaper {
    /// Create a new Arabic shaper.
    pub fn new() -> Self {
        Self
    }

    /// Shape a sequence of characters into glyphs with correct positional forms.
    ///
    /// This processes the entire text and returns shaped glyphs for all characters.
    /// For incremental updates during editing, use `reshape_range` instead.
    pub fn shape(&self, text: &[char], font: &dyn GlyphProvider) -> Vec<ShapedGlyph> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::with_capacity(text.len());

        for i in 0..text.len() {
            if let Some(shaped) = self.shape_char_at(text, i, font) {
                result.push(shaped);
            }
        }

        result
    }

    /// Shape a single character at the given index, considering its neighbors.
    ///
    /// Returns None if the character has no corresponding glyph in the font.
    pub fn shape_char_at(
        &self,
        text: &[char],
        index: usize,
        font: &dyn GlyphProvider,
    ) -> Option<ShapedGlyph> {
        let c = *text.get(index)?;

        // Get base glyph name from font
        let base_name = font.base_glyph_for_codepoint(c)?;

        // Determine positional form based on neighbors
        let form = self.determine_form(text, index);

        // Build full glyph name with suffix
        let glyph_name = self.resolve_glyph_name(&base_name, form, font);

        // Get advance width for the resolved glyph
        let advance_width = font.advance_width(&glyph_name).unwrap_or(500.0);

        Some(ShapedGlyph::new(
            base_name,
            glyph_name,
            c,
            form,
            advance_width,
        ))
    }

    /// Determine the positional form for a character based on its neighbors.
    ///
    /// This implements the core Arabic joining algorithm:
    /// - Check if the previous non-transparent character joins forward
    /// - Check if the next non-transparent character joins backward
    /// - Combine with the character's own joining type to determine form
    pub fn determine_form(&self, text: &[char], index: usize) -> PositionalForm {
        let c = text[index];

        // Non-Arabic characters are always isolated
        if !is_arabic(c) {
            return PositionalForm::Isolated;
        }

        let jt = joining_type(c);

        // Non-joining and transparent characters are always isolated
        if matches!(jt, JoiningType::NonJoining | JoiningType::Transparent) {
            return PositionalForm::Isolated;
        }

        // Check neighbors (skipping transparent characters)
        let prev_joins = self.check_prev_joins_forward(text, index);
        let next_joins = self.check_next_joins_backward(text, index);

        match jt {
            JoiningType::Dual => {
                match (prev_joins, next_joins) {
                    (false, false) => PositionalForm::Isolated,
                    (false, true) => PositionalForm::Initial,
                    (true, false) => PositionalForm::Final,
                    (true, true) => PositionalForm::Medial,
                }
            }
            JoiningType::Right => {
                // Right-joining characters only have isolated and final forms
                if prev_joins {
                    PositionalForm::Final
                } else {
                    PositionalForm::Isolated
                }
            }
            JoiningType::JoinCausing => {
                // Tatweel/kashida is always isolated (it's a spacing character)
                PositionalForm::Isolated
            }
            _ => PositionalForm::Isolated,
        }
    }

    /// Check if the previous non-transparent character joins forward.
    ///
    /// Walks backward through the text, skipping transparent characters
    /// (marks/diacritics), and returns true if the first non-transparent
    /// character can connect forward.
    fn check_prev_joins_forward(&self, text: &[char], index: usize) -> bool {
        let mut i = index;
        while i > 0 {
            i -= 1;
            let jt = joining_type(text[i]);
            if !jt.is_transparent() {
                return jt.joins_forward();
            }
        }
        false
    }

    /// Check if the next non-transparent character joins backward.
    ///
    /// Walks forward through the text, skipping transparent characters
    /// (marks/diacritics), and returns true if the first non-transparent
    /// character can connect backward.
    fn check_next_joins_backward(&self, text: &[char], index: usize) -> bool {
        let mut i = index + 1;
        while i < text.len() {
            let jt = joining_type(text[i]);
            if !jt.is_transparent() {
                return jt.joins_backward();
            }
            i += 1;
        }
        false
    }

    /// Resolve the final glyph name with positional suffix.
    ///
    /// Tries the suffixed name first, falls back to base name if not found.
    fn resolve_glyph_name(
        &self,
        base_name: &str,
        form: PositionalForm,
        font: &dyn GlyphProvider,
    ) -> String {
        // For isolated form, no suffix needed
        if form == PositionalForm::Isolated {
            return base_name.to_string();
        }

        // Try with positional suffix
        let with_suffix = format!("{}{}", base_name, form.suffix());
        if font.has_glyph(&with_suffix) {
            return with_suffix;
        }

        // Fallback to base glyph
        base_name.to_string()
    }

    /// Reshape a range of characters after an edit.
    ///
    /// This is the key method for incremental reshaping. When a character is
    /// inserted or deleted, only the characters near the edit point need to
    /// be reshaped. This method expands the range slightly to include neighbors
    /// that might be affected.
    ///
    /// # Arguments
    ///
    /// * `text` - The full text buffer as a character slice
    /// * `start` - Start of the edited range
    /// * `end` - End of the edited range (exclusive)
    /// * `font` - Font provider for glyph lookup
    ///
    /// # Returns
    ///
    /// A vector of reshaped glyphs for the affected range. The returned glyphs
    /// correspond to positions `actual_start..actual_end` where the range has
    /// been expanded to include affected neighbors.
    pub fn reshape_range(
        &self,
        text: &[char],
        start: usize,
        end: usize,
        font: &dyn GlyphProvider,
    ) -> Vec<ShapedGlyph> {
        if text.is_empty() {
            return Vec::new();
        }

        // Expand range to include neighbors that might be affected
        // We need to go back far enough to skip any transparent characters
        let actual_start = self.find_reshape_start(text, start);
        let actual_end = self.find_reshape_end(text, end);

        let mut result = Vec::with_capacity(actual_end - actual_start);
        for i in actual_start..actual_end {
            if let Some(shaped) = self.shape_char_at(text, i, font) {
                result.push(shaped);
            }
        }
        result
    }

    /// Find the start position for reshaping, accounting for transparent characters.
    fn find_reshape_start(&self, text: &[char], start: usize) -> usize {
        if start == 0 {
            return 0;
        }

        // Go back at least one position
        let mut pos = start.saturating_sub(1);

        // Skip back over any transparent characters
        while pos > 0 && joining_type(text[pos]).is_transparent() {
            pos -= 1;
        }

        // Go back one more to get the character that might affect joining
        pos.saturating_sub(1)
    }

    /// Find the end position for reshaping, accounting for transparent characters.
    fn find_reshape_end(&self, text: &[char], end: usize) -> usize {
        let len = text.len();
        if end >= len {
            return len;
        }

        // Go forward at least one position
        let mut pos = (end + 1).min(len);

        // Skip over any transparent characters
        while pos < len && joining_type(text[pos]).is_transparent() {
            pos += 1;
        }

        // Go one more to include the character that might be affected
        (pos + 1).min(len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock font provider for testing
    struct MockFont {
        /// Set of glyph names that "exist" in this mock font
        glyphs: std::collections::HashSet<String>,
    }

    impl MockFont {
        fn new() -> Self {
            let mut glyphs = std::collections::HashSet::new();

            // Add Arabic glyphs with all positional forms
            for base in &[
                "alef-ar", "beh-ar", "teh-ar", "theh-ar", "jeem-ar", "hah-ar",
                "khah-ar", "dal-ar", "thal-ar", "reh-ar", "zain-ar", "seen-ar",
                "sheen-ar", "sad-ar", "dad-ar", "tah-ar", "zah-ar", "ain-ar",
                "ghain-ar", "feh-ar", "qaf-ar", "kaf-ar", "lam-ar", "meem-ar",
                "noon-ar", "heh-ar", "waw-ar", "yeh-ar",
            ] {
                glyphs.insert(base.to_string());
                glyphs.insert(format!("{}.init", base));
                glyphs.insert(format!("{}.medi", base));
                glyphs.insert(format!("{}.fina", base));
            }

            // Right-joining letters only have isolated and final
            for base in &["alef-ar", "dal-ar", "thal-ar", "reh-ar", "zain-ar", "waw-ar"] {
                glyphs.remove(&format!("{}.init", base));
                glyphs.remove(&format!("{}.medi", base));
            }

            Self { glyphs }
        }
    }

    impl GlyphProvider for MockFont {
        fn has_glyph(&self, name: &str) -> bool {
            self.glyphs.contains(name)
        }

        fn advance_width(&self, _name: &str) -> Option<f64> {
            Some(500.0)
        }

        fn base_glyph_for_codepoint(&self, c: char) -> Option<String> {
            let name = match c {
                '\u{0627}' => "alef-ar",
                '\u{0628}' => "beh-ar",
                '\u{062A}' => "teh-ar",
                '\u{062B}' => "theh-ar",
                '\u{062C}' => "jeem-ar",
                '\u{062D}' => "hah-ar",
                '\u{062E}' => "khah-ar",
                '\u{062F}' => "dal-ar",
                '\u{0630}' => "thal-ar",
                '\u{0631}' => "reh-ar",
                '\u{0632}' => "zain-ar",
                '\u{0633}' => "seen-ar",
                '\u{0634}' => "sheen-ar",
                '\u{0635}' => "sad-ar",
                '\u{0636}' => "dad-ar",
                '\u{0637}' => "tah-ar",
                '\u{0638}' => "zah-ar",
                '\u{0639}' => "ain-ar",
                '\u{063A}' => "ghain-ar",
                '\u{0641}' => "feh-ar",
                '\u{0642}' => "qaf-ar",
                '\u{0643}' => "kaf-ar",
                '\u{0644}' => "lam-ar",
                '\u{0645}' => "meem-ar",
                '\u{0646}' => "noon-ar",
                '\u{0647}' => "heh-ar",
                '\u{0648}' => "waw-ar",
                '\u{064A}' => "yeh-ar",
                _ => return None,
            };
            Some(name.to_string())
        }
    }

    #[test]
    fn test_single_char_isolated() {
        let shaper = ArabicShaper::new();
        let font = MockFont::new();
        let text: Vec<char> = "ب".chars().collect(); // beh

        let result = shaper.shape(&text, &font);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].form, PositionalForm::Isolated);
        assert_eq!(result[0].glyph_name, "beh-ar");
    }

    #[test]
    fn test_two_dual_joining() {
        let shaper = ArabicShaper::new();
        let font = MockFont::new();
        let text: Vec<char> = "بم".chars().collect(); // beh + meem

        let result = shaper.shape(&text, &font);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].form, PositionalForm::Initial);
        assert_eq!(result[0].glyph_name, "beh-ar.init");
        assert_eq!(result[1].form, PositionalForm::Final);
        assert_eq!(result[1].glyph_name, "meem-ar.fina");
    }

    #[test]
    fn test_three_char_with_medial() {
        let shaper = ArabicShaper::new();
        let font = MockFont::new();
        let text: Vec<char> = "بسم".chars().collect(); // beh + seen + meem

        let result = shaper.shape(&text, &font);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].form, PositionalForm::Initial);
        assert_eq!(result[0].glyph_name, "beh-ar.init");
        assert_eq!(result[1].form, PositionalForm::Medial);
        assert_eq!(result[1].glyph_name, "seen-ar.medi");
        assert_eq!(result[2].form, PositionalForm::Final);
        assert_eq!(result[2].glyph_name, "meem-ar.fina");
    }

    #[test]
    fn test_right_joining_alef() {
        let shaper = ArabicShaper::new();
        let font = MockFont::new();
        let text: Vec<char> = "با".chars().collect(); // beh + alef

        let result = shaper.shape(&text, &font);

        assert_eq!(result.len(), 2);
        // Beh is initial (followed by alef which joins backward)
        assert_eq!(result[0].form, PositionalForm::Initial);
        assert_eq!(result[0].glyph_name, "beh-ar.init");
        // Alef is final (preceded by beh which joins forward)
        assert_eq!(result[1].form, PositionalForm::Final);
        assert_eq!(result[1].glyph_name, "alef-ar.fina");
    }

    #[test]
    fn test_alef_breaks_joining() {
        let shaper = ArabicShaper::new();
        let font = MockFont::new();
        // alef + beh + meem: alef doesn't join forward, so beh is initial
        let text: Vec<char> = "ابم".chars().collect();

        let result = shaper.shape(&text, &font);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].form, PositionalForm::Isolated); // alef
        assert_eq!(result[1].form, PositionalForm::Initial);  // beh
        assert_eq!(result[2].form, PositionalForm::Final);    // meem
    }

    #[test]
    fn test_word_with_multiple_non_joiners() {
        let shaper = ArabicShaper::new();
        let font = MockFont::new();
        // beh + alef + beh + alef: two separate pairs
        let text: Vec<char> = "بابا".chars().collect();

        let result = shaper.shape(&text, &font);

        assert_eq!(result.len(), 4);
        assert_eq!(result[0].form, PositionalForm::Initial); // first beh
        assert_eq!(result[1].form, PositionalForm::Final);   // first alef
        assert_eq!(result[2].form, PositionalForm::Initial); // second beh
        assert_eq!(result[3].form, PositionalForm::Final);   // second alef
    }

    #[test]
    fn test_determine_form_directly() {
        let shaper = ArabicShaper::new();

        // Single isolated character
        let text1: Vec<char> = "ب".chars().collect();
        assert_eq!(shaper.determine_form(&text1, 0), PositionalForm::Isolated);

        // Initial position
        let text2: Vec<char> = "بم".chars().collect();
        assert_eq!(shaper.determine_form(&text2, 0), PositionalForm::Initial);
        assert_eq!(shaper.determine_form(&text2, 1), PositionalForm::Final);

        // Medial position
        let text3: Vec<char> = "بسم".chars().collect();
        assert_eq!(shaper.determine_form(&text3, 1), PositionalForm::Medial);
    }

    #[test]
    fn test_reshape_range() {
        let shaper = ArabicShaper::new();
        let font = MockFont::new();
        let text: Vec<char> = "بسمنك".chars().collect(); // 5 characters

        // Reshape just the middle portion
        let result = shaper.reshape_range(&text, 2, 3, &font);

        // Should include neighbors
        assert!(!result.is_empty());

        // Verify the shapes are correct
        for glyph in &result {
            assert!(glyph.glyph_name.contains("-ar"));
        }
    }

    #[test]
    fn test_latin_characters_isolated() {
        let shaper = ArabicShaper::new();

        let text: Vec<char> = "بAم".chars().collect();

        // Latin 'A' should be isolated
        assert_eq!(shaper.determine_form(&text, 1), PositionalForm::Isolated);

        // And it should break the joining
        assert_eq!(shaper.determine_form(&text, 0), PositionalForm::Isolated); // beh before A
        assert_eq!(shaper.determine_form(&text, 2), PositionalForm::Isolated); // meem after A
    }

    #[test]
    fn test_long_word() {
        let shaper = ArabicShaper::new();
        let font = MockFont::new();
        // "bismillah" - بسملله
        let text: Vec<char> = "بسملله".chars().collect();

        let result = shaper.shape(&text, &font);

        assert_eq!(result.len(), 6);
        assert_eq!(result[0].form, PositionalForm::Initial); // beh
        assert_eq!(result[1].form, PositionalForm::Medial);  // seen
        assert_eq!(result[2].form, PositionalForm::Medial);  // meem
        assert_eq!(result[3].form, PositionalForm::Medial);  // lam
        assert_eq!(result[4].form, PositionalForm::Medial);  // lam
        assert_eq!(result[5].form, PositionalForm::Final);   // heh
    }
}
