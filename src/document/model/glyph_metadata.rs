// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Kurbo-free glyph metadata shared by Runebender frontends.

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
}
