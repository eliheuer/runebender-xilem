// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! UFO `public.markColor` helpers.

pub const PRESET_UFO_RGBA: [&str; 7] = [
    "1,0.3,0.3,1",
    "1,0.6,0.2,1",
    "1,0.9,0.2,1",
    "0.3,0.7,0.3,1",
    "0.1,0.3,0.8,1",
    "0.6,0.3,0.9,1",
    "0.9,0.3,0.7,1",
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkColor {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl MarkColor {
    pub fn parse(value: &str) -> Option<Self> {
        let mut values = value.split(',').map(str::trim).map(str::parse::<f32>);
        let red = values.next()?.ok()?;
        let green = values.next()?.ok()?;
        let blue = values.next()?.ok()?;
        let alpha = values.next()?.ok()?;
        if values.next().is_some() {
            return None;
        }
        let color = Self {
            red,
            green,
            blue,
            alpha,
        };
        color.is_valid().then_some(color)
    }

    pub fn is_valid(self) -> bool {
        [self.red, self.green, self.blue, self.alpha]
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
    }
}

pub fn canonical_ufo_mark_color(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(String::new());
    }
    MarkColor::parse(trimmed)?;
    Some(trimmed.split(',').map(str::trim).collect::<Vec<_>>().join(","))
}

pub fn palette_index(value: &str) -> Option<usize> {
    let canonical = canonical_ufo_mark_color(value)?;
    PRESET_UFO_RGBA.iter().position(|preset| *preset == canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ufo_rgba_with_whitespace() {
        assert_eq!(
            MarkColor::parse(" 1, 0.3, 0.3, 1 "),
            Some(MarkColor {
                red: 1.0,
                green: 0.3,
                blue: 0.3,
                alpha: 1.0,
            })
        );
    }

    #[test]
    fn rejects_invalid_ufo_rgba() {
        assert_eq!(MarkColor::parse("1,0.3,1"), None);
        assert_eq!(MarkColor::parse("1,0.3,0.3,2"), None);
        assert_eq!(MarkColor::parse("1,0.3,0.3,nan"), None);
    }

    #[test]
    fn canonicalizes_storage_string_without_changing_precision() {
        assert_eq!(
            canonical_ufo_mark_color(" 1, 0.30, 0.3, 1 "),
            Some("1,0.30,0.3,1".to_string())
        );
        assert_eq!(canonical_ufo_mark_color(""), Some(String::new()));
    }

    #[test]
    fn matches_xilem_palette_by_exact_canonical_string() {
        assert_eq!(palette_index("1,0.3,0.3,1"), Some(0));
        assert_eq!(palette_index(" 1, 0.3, 0.3, 1 "), Some(0));
        assert_eq!(palette_index("1,0.30,0.3,1"), None);
    }
}
