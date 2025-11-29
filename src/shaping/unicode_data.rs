// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

//! Unicode character property data for text shaping.
//!
//! Data sourced from Unicode Standard and ArabicShaping.txt.
//! See: https://www.unicode.org/Public/UCD/latest/ucd/ArabicShaping.txt

/// Arabic joining type from Unicode ArabicShaping.txt
///
/// Each Arabic character has a joining type that determines how it connects
/// to neighboring characters in cursive text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JoiningType {
    /// Dual-joining (D): Can connect on both sides.
    /// Examples: beh, seen, meem, noon, yeh
    /// Has 4 forms: isolated, initial, medial, final
    Dual,

    /// Right-joining (R): Connects only to the previous (right-side in RTL) letter.
    /// Examples: alef, dal, reh, waw
    /// Has 2 forms: isolated, final
    Right,

    /// Non-joining (U): Cannot connect to neighbors.
    /// Examples: hamza, Latin letters, numbers
    /// Has 1 form: isolated
    #[default]
    NonJoining,

    /// Join-causing (C): Causes adjacent letters to connect.
    /// Example: tatweel (kashida)
    JoinCausing,

    /// Transparent (T): Ignored for joining purposes.
    /// Examples: Arabic marks and diacritics (fatha, kasra, damma, etc.)
    Transparent,
}

impl JoiningType {
    /// Can this character connect forward (to the left in RTL)?
    ///
    /// Returns true for Dual-joining and Join-causing types.
    #[inline]
    pub fn joins_forward(&self) -> bool {
        matches!(self, Self::Dual | Self::JoinCausing)
    }

    /// Can this character connect backward (to the right in RTL)?
    ///
    /// Returns true for Dual-joining, Right-joining, and Join-causing types.
    #[inline]
    pub fn joins_backward(&self) -> bool {
        matches!(self, Self::Dual | Self::Right | Self::JoinCausing)
    }

    /// Is this character transparent for joining?
    ///
    /// Transparent characters (marks/diacritics) are skipped when
    /// determining joining behavior.
    #[inline]
    pub fn is_transparent(&self) -> bool {
        matches!(self, Self::Transparent)
    }
}

/// Get the joining type for a Unicode codepoint.
///
/// This implements the joining type lookup from Unicode ArabicShaping.txt.
/// Non-Arabic characters return `NonJoining`.
pub fn joining_type(c: char) -> JoiningType {
    match c as u32 {
        // ===========================================
        // Right-joining (R) - connect only to previous letter
        // ===========================================

        // Alef and variants
        0x0622 => JoiningType::Right, // ARABIC LETTER ALEF WITH MADDA ABOVE
        0x0623 => JoiningType::Right, // ARABIC LETTER ALEF WITH HAMZA ABOVE
        0x0625 => JoiningType::Right, // ARABIC LETTER ALEF WITH HAMZA BELOW
        0x0627 => JoiningType::Right, // ARABIC LETTER ALEF
        0x0629 => JoiningType::Right, // ARABIC LETTER TEH MARBUTA

        // Dal group
        0x062F => JoiningType::Right, // ARABIC LETTER DAL
        0x0630 => JoiningType::Right, // ARABIC LETTER THAL

        // Reh group
        0x0631 => JoiningType::Right, // ARABIC LETTER REH
        0x0632 => JoiningType::Right, // ARABIC LETTER ZAIN

        // Waw group
        0x0648 => JoiningType::Right, // ARABIC LETTER WAW
        0x0624 => JoiningType::Right, // ARABIC LETTER WAW WITH HAMZA ABOVE

        // ===========================================
        // Dual-joining (D) - connect on both sides
        // ===========================================

        // Beh group
        0x0628 => JoiningType::Dual, // ARABIC LETTER BEH
        0x062A => JoiningType::Dual, // ARABIC LETTER TEH
        0x062B => JoiningType::Dual, // ARABIC LETTER THEH

        // Jeem group
        0x062C => JoiningType::Dual, // ARABIC LETTER JEEM
        0x062D => JoiningType::Dual, // ARABIC LETTER HAH
        0x062E => JoiningType::Dual, // ARABIC LETTER KHAH

        // Seen group
        0x0633 => JoiningType::Dual, // ARABIC LETTER SEEN
        0x0634 => JoiningType::Dual, // ARABIC LETTER SHEEN

        // Sad group
        0x0635 => JoiningType::Dual, // ARABIC LETTER SAD
        0x0636 => JoiningType::Dual, // ARABIC LETTER DAD

        // Tah group
        0x0637 => JoiningType::Dual, // ARABIC LETTER TAH
        0x0638 => JoiningType::Dual, // ARABIC LETTER ZAH

        // Ain group
        0x0639 => JoiningType::Dual, // ARABIC LETTER AIN
        0x063A => JoiningType::Dual, // ARABIC LETTER GHAIN

        // Feh
        0x0641 => JoiningType::Dual, // ARABIC LETTER FEH

        // Qaf
        0x0642 => JoiningType::Dual, // ARABIC LETTER QAF

        // Kaf
        0x0643 => JoiningType::Dual, // ARABIC LETTER KAF

        // Lam
        0x0644 => JoiningType::Dual, // ARABIC LETTER LAM

        // Meem
        0x0645 => JoiningType::Dual, // ARABIC LETTER MEEM

        // Noon
        0x0646 => JoiningType::Dual, // ARABIC LETTER NOON

        // Heh
        0x0647 => JoiningType::Dual, // ARABIC LETTER HEH

        // Yeh group
        0x064A => JoiningType::Dual, // ARABIC LETTER YEH
        0x0626 => JoiningType::Dual, // ARABIC LETTER YEH WITH HAMZA ABOVE
        0x0649 => JoiningType::Dual, // ARABIC LETTER ALEF MAKSURA

        // ===========================================
        // Non-joining (U)
        // ===========================================
        0x0621 => JoiningType::NonJoining, // ARABIC LETTER HAMZA

        // ===========================================
        // Join-causing (C)
        // ===========================================
        0x0640 => JoiningType::JoinCausing, // ARABIC TATWEEL (kashida)

        // ===========================================
        // Transparent (T) - marks and diacritics
        // ===========================================

        // Tashkil (vocalization marks)
        0x064B => JoiningType::Transparent, // ARABIC FATHATAN
        0x064C => JoiningType::Transparent, // ARABIC DAMMATAN
        0x064D => JoiningType::Transparent, // ARABIC KASRATAN
        0x064E => JoiningType::Transparent, // ARABIC FATHA
        0x064F => JoiningType::Transparent, // ARABIC DAMMA
        0x0650 => JoiningType::Transparent, // ARABIC KASRA
        0x0651 => JoiningType::Transparent, // ARABIC SHADDA
        0x0652 => JoiningType::Transparent, // ARABIC SUKUN

        // Additional marks
        0x0670 => JoiningType::Transparent, // ARABIC LETTER SUPERSCRIPT ALEF

        // Quranic marks (0x0610-0x061A)
        0x0610..=0x061A => JoiningType::Transparent,

        // Extended Arabic marks (0x06D6-0x06ED)
        0x06D6..=0x06ED => JoiningType::Transparent,

        // ===========================================
        // Default: non-joining
        // ===========================================
        _ => JoiningType::NonJoining,
    }
}

/// Check if a character is in the Arabic Unicode blocks.
///
/// Covers:
/// - Arabic (U+0600–U+06FF)
/// - Arabic Supplement (U+0750–U+077F)
/// - Arabic Extended-A (U+08A0–U+08FF)
#[inline]
pub fn is_arabic(c: char) -> bool {
    let cp = c as u32;
    (0x0600..=0x06FF).contains(&cp)
        || (0x0750..=0x077F).contains(&cp)
        || (0x08A0..=0x08FF).contains(&cp)
}

/// Check if a character is an Arabic base letter (not a mark).
#[inline]
pub fn is_arabic_letter(c: char) -> bool {
    is_arabic(c) && !joining_type(c).is_transparent()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alef_is_right_joining() {
        assert_eq!(joining_type('\u{0627}'), JoiningType::Right);
        assert!(!joining_type('\u{0627}').joins_forward());
        assert!(joining_type('\u{0627}').joins_backward());
    }

    #[test]
    fn test_beh_is_dual_joining() {
        assert_eq!(joining_type('\u{0628}'), JoiningType::Dual);
        assert!(joining_type('\u{0628}').joins_forward());
        assert!(joining_type('\u{0628}').joins_backward());
    }

    #[test]
    fn test_hamza_is_non_joining() {
        assert_eq!(joining_type('\u{0621}'), JoiningType::NonJoining);
        assert!(!joining_type('\u{0621}').joins_forward());
        assert!(!joining_type('\u{0621}').joins_backward());
    }

    #[test]
    fn test_tatweel_is_join_causing() {
        assert_eq!(joining_type('\u{0640}'), JoiningType::JoinCausing);
        assert!(joining_type('\u{0640}').joins_forward());
        assert!(joining_type('\u{0640}').joins_backward());
    }

    #[test]
    fn test_fatha_is_transparent() {
        assert_eq!(joining_type('\u{064E}'), JoiningType::Transparent);
        assert!(joining_type('\u{064E}').is_transparent());
    }

    #[test]
    fn test_latin_is_non_joining() {
        assert_eq!(joining_type('A'), JoiningType::NonJoining);
        assert_eq!(joining_type('z'), JoiningType::NonJoining);
        assert_eq!(joining_type('5'), JoiningType::NonJoining);
    }

    #[test]
    fn test_is_arabic() {
        assert!(is_arabic('\u{0627}')); // Alef
        assert!(is_arabic('\u{0628}')); // Beh
        assert!(is_arabic('\u{064E}')); // Fatha mark
        assert!(!is_arabic('A'));
        assert!(!is_arabic('5'));
    }

    #[test]
    fn test_is_arabic_letter() {
        assert!(is_arabic_letter('\u{0627}')); // Alef
        assert!(is_arabic_letter('\u{0628}')); // Beh
        assert!(!is_arabic_letter('\u{064E}')); // Fatha mark (transparent)
        assert!(!is_arabic_letter('A'));
    }

    #[test]
    fn test_right_joining_letters() {
        // All right-joining letters
        let right_joining = [
            '\u{0622}', // Alef with madda
            '\u{0623}', // Alef with hamza above
            '\u{0625}', // Alef with hamza below
            '\u{0627}', // Alef
            '\u{0629}', // Teh marbuta
            '\u{062F}', // Dal
            '\u{0630}', // Thal
            '\u{0631}', // Reh
            '\u{0632}', // Zain
            '\u{0648}', // Waw
            '\u{0624}', // Waw with hamza
        ];

        for c in right_joining {
            assert_eq!(
                joining_type(c),
                JoiningType::Right,
                "Expected {:?} (U+{:04X}) to be Right-joining",
                c,
                c as u32
            );
        }
    }

    #[test]
    fn test_dual_joining_letters() {
        // Sample of dual-joining letters
        let dual_joining = [
            '\u{0628}', // Beh
            '\u{062A}', // Teh
            '\u{062C}', // Jeem
            '\u{0633}', // Seen
            '\u{0639}', // Ain
            '\u{0644}', // Lam
            '\u{0645}', // Meem
            '\u{0646}', // Noon
            '\u{064A}', // Yeh
        ];

        for c in dual_joining {
            assert_eq!(
                joining_type(c),
                JoiningType::Dual,
                "Expected {:?} (U+{:04X}) to be Dual-joining",
                c,
                c as u32
            );
        }
    }
}
