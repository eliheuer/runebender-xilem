// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! UFO `public.markColor` serialization helpers.
//!
//! The typed value lives with canonical glyph metadata.
//! The palette itself lives in the shared theme (`ui::theme`), which owns label reading,
//! hue-snapping and writing.

pub use crate::document::model::glyph_metadata::{MARK_COLOR_KEY, MarkColor};

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
