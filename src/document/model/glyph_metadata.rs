// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Kurbo-free glyph metadata shared by Runebender frontends.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

const SKIP_EXPORT_GLYPHS: &str = "public.skipExportGlyphs";

pub(crate) fn skipped_exports(font: &norad::Font) -> impl Iterator<Item = &str> {
    font.lib
        .get(SKIP_EXPORT_GLYPHS)
        .and_then(plist::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(plist::Value::as_string)
}

pub(crate) fn set_skipped_exports(font: &mut norad::Font, names: Vec<String>) {
    if names.is_empty() {
        font.lib.remove(SKIP_EXPORT_GLYPHS);
    } else {
        font.lib.insert(
            SKIP_EXPORT_GLYPHS.into(),
            plist::Value::Array(names.into_iter().map(plist::Value::String).collect()),
        );
    }
}

/// A typed value from the UFO `public.openTypeCategories` dictionary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenTypeGlyphCategory {
    /// A base glyph.
    Base,
    /// A combining or spacing mark.
    Mark,
    /// A ligature glyph.
    Ligature,
    /// A glyph intended only as a component source.
    Component,
    /// A source value this Runebender version does not interpret.
    Other(String),
}

impl OpenTypeGlyphCategory {
    /// Preserve a source category as a typed known value or an exact unknown string.
    pub fn from_source(value: impl Into<String>) -> Self {
        let value = value.into();
        match value.as_str() {
            "base" => Self::Base,
            "mark" => Self::Mark,
            "ligature" => Self::Ligature,
            "component" => Self::Component,
            _ => Self::Other(value),
        }
    }

    /// The exact source string written at the UFO boundary.
    pub fn as_source(&self) -> &str {
        match self {
            Self::Base => "base",
            Self::Mark => "mark",
            Self::Ligature => "ligature",
            Self::Component => "component",
            Self::Other(value) => value,
        }
    }
}

/// Canonical editable metadata for one glyph identity.
///
/// The glyph's mutable name remains in the document name index rather than this value.
/// Unknown glyph-lib entries remain in the format-preservation payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalGlyphMetadata {
    codepoints: Vec<char>,
    note: Option<String>,
    exported: bool,
    category: Option<OpenTypeGlyphCategory>,
}

impl Default for CanonicalGlyphMetadata {
    fn default() -> Self {
        Self {
            codepoints: Vec::new(),
            note: None,
            exported: true,
            category: None,
        }
    }
}

impl CanonicalGlyphMetadata {
    /// Construct canonical metadata, retaining codepoint order and the first occurrence of each
    /// scalar value.
    pub fn new(
        codepoints: impl IntoIterator<Item = char>,
        note: Option<String>,
        exported: bool,
        category: Option<OpenTypeGlyphCategory>,
    ) -> Self {
        let mut unique = HashSet::new();
        let codepoints = codepoints
            .into_iter()
            .filter(|codepoint| unique.insert(*codepoint))
            .collect();
        Self {
            codepoints,
            note,
            exported,
            category,
        }
    }

    /// Unicode scalar values in source order.
    pub fn codepoints(&self) -> &[char] {
        &self.codepoints
    }

    /// The optional glyph note, preserving the distinction between absent and empty.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// Whether the glyph participates in export.
    pub fn exported(&self) -> bool {
        self.exported
    }

    /// The explicit OpenType category, or `None` when it should be inferred.
    pub fn category(&self) -> Option<&OpenTypeGlyphCategory> {
        self.category.as_ref()
    }

    /// Replace the Unicode scalar values, retaining order and removing later duplicates.
    pub fn set_codepoints(&mut self, codepoints: impl IntoIterator<Item = char>) -> bool {
        let mut unique = HashSet::new();
        let codepoints: Vec<_> = codepoints
            .into_iter()
            .filter(|codepoint| unique.insert(*codepoint))
            .collect();
        if self.codepoints == codepoints {
            return false;
        }
        self.codepoints = codepoints;
        true
    }

    /// Replace the glyph note exactly.
    pub fn set_note(&mut self, note: Option<String>) -> bool {
        if self.note == note {
            return false;
        }
        self.note = note;
        true
    }

    /// Change whether the glyph participates in export.
    pub fn set_exported(&mut self, exported: bool) -> bool {
        if self.exported == exported {
            return false;
        }
        self.exported = exported;
        true
    }

    /// Set or clear the explicit OpenType category.
    pub fn set_category(&mut self, category: Option<OpenTypeGlyphCategory>) -> bool {
        if self.category == category {
            return false;
        }
        self.category = category;
        true
    }
}

/// A rejected glyph-metadata input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlyphMetadataError {
    /// A token was not a valid Unicode scalar written in hexadecimal.
    InvalidCodepoint(String),
}

impl fmt::Display for GlyphMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCodepoint(value) => write!(f, "invalid Unicode scalar {value:?}"),
        }
    }
}

impl std::error::Error for GlyphMetadataError {}

/// Parse one or more hexadecimal Unicode scalar values.
///
/// Values can be separated by commas or whitespace and may use `U+` or `0x` prefixes.
/// Empty input clears the encoding.
/// Duplicate values retain their first position.
pub fn parse_codepoints(input: &str) -> Result<Vec<char>, GlyphMetadataError> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let mut codepoints = Vec::new();
    for token in input.split(|character: char| character == ',' || character.is_whitespace()) {
        if token.is_empty() {
            continue;
        }
        let hex = token
            .strip_prefix("U+")
            .or_else(|| token.strip_prefix("u+"))
            .or_else(|| token.strip_prefix("0x"))
            .or_else(|| token.strip_prefix("0X"))
            .unwrap_or(token);
        let Some(codepoint) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) else {
            return Err(GlyphMetadataError::InvalidCodepoint(token.to_owned()));
        };
        if !codepoints.contains(&codepoint) {
            codepoints.push(codepoint);
        }
    }
    if codepoints.is_empty() {
        return Err(GlyphMetadataError::InvalidCodepoint(input.to_owned()));
    }
    Ok(codepoints)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Summary data for one glyph, enough to draw a glyph-grid cell without loading outlines.
pub struct GlyphMetadata {
    /// The glyph name, as in the UFO.
    pub name: String,
    /// Advance width in font units.
    pub width: f64,
    /// Number of contours in the glyph outline.
    pub contours: usize,
    /// The first codepoint as an uppercase hex string, or `None` when the glyph has no codepoint.
    pub unicode: Option<String>,
    #[serde(default)]
    /// All codepoints as uppercase hex strings; empty when the glyph is unencoded.
    pub unicodes: Vec<String>,
}

impl GlyphMetadata {
    /// Builds metadata from its parts and derives `unicode` from the first entry of `unicodes`.
    pub fn new(
        name: impl Into<String>,
        width: f64,
        contours: usize,
        unicodes: Vec<String>,
    ) -> Self {
        let unicode = unicodes.first().cloned();
        Self {
            name: name.into(),
            width,
            contours,
            unicode,
            unicodes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_unicode_is_compatibility_field() {
        let metadata =
            GlyphMetadata::new("A", 600.0, 2, vec!["0041".to_string(), "0391".to_string()]);

        assert_eq!(metadata.unicode.as_deref(), Some("0041"));
        assert_eq!(metadata.unicodes, ["0041", "0391"]);
    }

    #[test]
    fn glyph_without_codepoint_has_no_first_unicode() {
        let metadata = GlyphMetadata::new("glyph", 500.0, 0, Vec::new());

        assert_eq!(metadata.unicode, None);
        assert!(metadata.unicodes.is_empty());
    }

    #[test]
    fn canonical_metadata_retains_order_and_exact_unknown_category() {
        let mut metadata = CanonicalGlyphMetadata::new(
            ['A', '\u{391}', 'A'],
            Some(String::new()),
            false,
            Some(OpenTypeGlyphCategory::from_source("future-category")),
        );

        assert_eq!(metadata.codepoints(), ['A', '\u{391}']);
        assert_eq!(metadata.note(), Some(""));
        assert!(!metadata.exported());
        assert_eq!(
            metadata.category().map(OpenTypeGlyphCategory::as_source),
            Some("future-category")
        );
        assert!(!metadata.set_codepoints(['A', '\u{391}']));
        assert!(metadata.set_exported(true));
        assert!(!metadata.set_exported(true));
    }

    #[test]
    fn parses_multiple_codepoint_spellings_atomically() {
        assert_eq!(
            parse_codepoints("U+0041, 0x0391 0041").unwrap(),
            ['A', '\u{391}']
        );
        assert_eq!(parse_codepoints("").unwrap(), Vec::<char>::new());
        assert_eq!(
            parse_codepoints("0041 D800"),
            Err(GlyphMetadataError::InvalidCodepoint("D800".into()))
        );
    }
}
