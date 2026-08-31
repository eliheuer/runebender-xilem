// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! UFO `public.markColor` parsing and validation. The palette itself
//! lives in the shared theme (`theme_oklch`), which also owns label
//! reading, hue-snapping, and writing.

#[derive(Clone, Copy, Debug, PartialEq)]
/// A UFO `public.markColor` value: four channels in the range 0.0 to 1.0.
pub struct MarkColor {
    /// Red channel, 0.0 to 1.0.
    pub red: f32,
    /// Green channel, 0.0 to 1.0.
    pub green: f32,
    /// Blue channel, 0.0 to 1.0.
    pub blue: f32,
    /// Alpha channel, 0.0 to 1.0.
    pub alpha: f32,
}

impl MarkColor {
    /// Parses a `r,g,b,a` string. Returns `None` unless there are exactly four finite values in 0.0 to 1.0.
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

    /// Returns true when every channel is finite and within 0.0 to 1.0.
    pub fn is_valid(self) -> bool {
        [self.red, self.green, self.blue, self.alpha]
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
    }
}

/// Normalizes a `public.markColor` string by trimming whitespace around each value. Keeps the original number text. Returns `None` for invalid input; an empty string stays empty.
pub fn canonical_ufo_mark_color(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(String::new());
    }
    MarkColor::parse(trimmed)?;
    Some(
        trimmed
            .split(',')
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(","),
    )
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
}
