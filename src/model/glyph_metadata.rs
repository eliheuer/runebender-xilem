// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Kurbo-free glyph metadata shared by Runebender frontends.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlyphMetadata {
    pub name: String,
    pub width: f64,
    pub contours: usize,
    pub unicode: Option<String>,
    #[serde(default)]
    pub unicodes: Vec<String>,
}

impl GlyphMetadata {
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
        let metadata = GlyphMetadata::new(
            "A",
            600.0,
            2,
            vec!["0041".to_string(), "0391".to_string()],
        );

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
