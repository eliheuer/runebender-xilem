// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Platform-independent shaping helpers shared by Runebender frontends.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PositionalForm {
    Isolated,
    Initial,
    Medial,
    Final,
}

impl PositionalForm {
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Isolated => "",
            Self::Initial => ".init",
            Self::Medial => ".medi",
            Self::Final => ".fina",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArabicJoiningType {
    Dual,
    Right,
    NonJoining,
    JoinCausing,
    Transparent,
}

impl ArabicJoiningType {
    pub const fn joins_forward(self) -> bool {
        matches!(self, Self::Dual | Self::JoinCausing)
    }

    pub const fn joins_backward(self) -> bool {
        matches!(self, Self::Dual | Self::Right | Self::JoinCausing)
    }

    pub const fn is_transparent(self) -> bool {
        matches!(self, Self::Transparent)
    }
}

pub fn arabic_positional_form(chars: &[char], index: usize) -> PositionalForm {
    let Some(&char) = chars.get(index) else {
        return PositionalForm::Isolated;
    };
    if !is_arabic(char) {
        return PositionalForm::Isolated;
    }
    let joining_type = arabic_joining_type(char);
    if matches!(
        joining_type,
        ArabicJoiningType::NonJoining | ArabicJoiningType::Transparent
    ) {
        return PositionalForm::Isolated;
    }

    let prev_joins = previous_joining_type(chars, index)
        .map(ArabicJoiningType::joins_forward)
        .unwrap_or(false);
    let next_joins = next_joining_type(chars, index)
        .map(ArabicJoiningType::joins_backward)
        .unwrap_or(false);

    match joining_type {
        ArabicJoiningType::Dual if prev_joins && next_joins => PositionalForm::Medial,
        ArabicJoiningType::Dual if prev_joins => PositionalForm::Final,
        ArabicJoiningType::Dual if next_joins => PositionalForm::Initial,
        ArabicJoiningType::Right if prev_joins => PositionalForm::Final,
        _ => PositionalForm::Isolated,
    }
}

fn previous_joining_type(chars: &[char], index: usize) -> Option<ArabicJoiningType> {
    for char in chars[..index].iter().rev() {
        let joining_type = arabic_joining_type(*char);
        if !joining_type.is_transparent() {
            return Some(joining_type);
        }
    }
    None
}

fn next_joining_type(chars: &[char], index: usize) -> Option<ArabicJoiningType> {
    for char in chars.iter().skip(index + 1) {
        let joining_type = arabic_joining_type(*char);
        if !joining_type.is_transparent() {
            return Some(joining_type);
        }
    }
    None
}

pub fn is_arabic(char: char) -> bool {
    let cp = char as u32;
    (0x0600..=0x06ff).contains(&cp)
        || (0x0750..=0x077f).contains(&cp)
        || (0x08a0..=0x08ff).contains(&cp)
}

pub fn arabic_joining_type(char: char) -> ArabicJoiningType {
    match char as u32 {
        0x0622 | 0x0623 | 0x0625 | 0x0627 | 0x0629 | 0x062f | 0x0630 | 0x0631 | 0x0632 | 0x0648
        | 0x0624 => ArabicJoiningType::Right,
        0x0628 | 0x062a | 0x062b | 0x062c | 0x062d | 0x062e | 0x0633 | 0x0634 | 0x0635 | 0x0636
        | 0x0637 | 0x0638 | 0x0639 | 0x063a | 0x0641 | 0x0642 | 0x0643 | 0x0644 | 0x0645
        | 0x0646 | 0x0647 | 0x064a | 0x0626 | 0x0649 => ArabicJoiningType::Dual,
        0x0640 => ArabicJoiningType::JoinCausing,
        0x064b..=0x0652 | 0x0670 | 0x0610..=0x061a | 0x06d6..=0x06ed => {
            ArabicJoiningType::Transparent
        }
        _ => ArabicJoiningType::NonJoining,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joining_types_match_xilem_subset() {
        assert_eq!(arabic_joining_type('\u{0627}'), ArabicJoiningType::Right);
        assert_eq!(arabic_joining_type('\u{0628}'), ArabicJoiningType::Dual);
        assert_eq!(arabic_joining_type('\u{0621}'), ArabicJoiningType::NonJoining);
        assert_eq!(arabic_joining_type('\u{0640}'), ArabicJoiningType::JoinCausing);
        assert_eq!(arabic_joining_type('\u{064e}'), ArabicJoiningType::Transparent);
    }

    #[test]
    fn positional_forms_skip_transparent_marks() {
        let chars = ['\u{0628}', '\u{064e}', '\u{0633}', '\u{0645}'];

        assert_eq!(arabic_positional_form(&chars, 0), PositionalForm::Initial);
        assert_eq!(arabic_positional_form(&chars, 1), PositionalForm::Isolated);
        assert_eq!(arabic_positional_form(&chars, 2), PositionalForm::Medial);
        assert_eq!(arabic_positional_form(&chars, 3), PositionalForm::Final);
    }

    #[test]
    fn right_joining_letters_only_form_final() {
        let chars = ['\u{0628}', '\u{0627}', '\u{0628}'];

        assert_eq!(arabic_positional_form(&chars, 0), PositionalForm::Initial);
        assert_eq!(arabic_positional_form(&chars, 1), PositionalForm::Final);
        assert_eq!(arabic_positional_form(&chars, 2), PositionalForm::Isolated);
    }
}
