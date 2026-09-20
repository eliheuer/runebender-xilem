// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical File > New Font input data.
//!
//! The GF Latin Core template is decoded into typed font information and empty glyph-layer
//! records, then installed directly into a canonical Project.

use std::sync::OnceLock;

use serde::Deserialize;

use super::model::font_info::{CanonicalFontInfo, CanonicalFontMetrics, CanonicalFontNames};

/// Units per em for a new font.
pub const UPM: f64 = 1000.0;
/// Default ascender for a new font, in font units.
pub const ASCENDER: f64 = 800.0;
/// Default descender for a new font, in font units. Negative, below the baseline.
pub const DESCENDER: f64 = -200.0;
/// Default cap height for a new font, in font units.
pub const CAP_HEIGHT: f64 = 700.0;
/// Default x-height for a new font, in font units.
pub const X_HEIGHT: f64 = 500.0;
/// Placeholder advance width for new glyphs, in font units. A
/// starting point, not a design.
pub const DEFAULT_WIDTH: f64 = 600.0;
/// Placeholder advance width of the space glyph, in font units.
pub const SPACE_WIDTH: f64 = 260.0;

#[derive(Deserialize)]
struct TemplateGlyph {
    name: String,
    #[serde(default)]
    unicode: Option<String>,
}

fn template_glyphs() -> &'static [TemplateGlyph] {
    static GLYPHS: OnceLock<Vec<TemplateGlyph>> = OnceLock::new();
    GLYPHS.get_or_init(|| {
        serde_json::from_str(include_str!("../../data/new-font-template.json"))
            .expect("new-font-template.json parses")
    })
}

/// One empty canonical glyph in the new-font template.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct NewFontGlyph {
    pub(super) name: String,
    pub(super) width: f64,
    pub(super) codepoint: Option<char>,
}

/// Complete typed input for constructing a new canonical document.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct NewFontSpecification {
    pub(super) font_info: CanonicalFontInfo,
    pub(super) glyphs: Vec<NewFontGlyph>,
}

/// Decode the checked-in GF-shaped template without constructing a UFO font.
pub(super) fn specification(
    family: &str,
    style: &str,
    weight_class: u32,
) -> Result<NewFontSpecification, String> {
    let font_info = CanonicalFontInfo {
        names: CanonicalFontNames {
            family_name: Some(family.to_owned()),
            style_name: Some(style.to_owned()),
            ..CanonicalFontNames::default()
        },
        metrics: CanonicalFontMetrics {
            units_per_em: Some(UPM),
            ascender: Some(ASCENDER),
            descender: Some(DESCENDER),
            x_height: Some(X_HEIGHT),
            cap_height: Some(CAP_HEIGHT),
            italic_angle: None,
        },
        open_type: super::model::font_info::CanonicalOpenTypeInfo {
            weight_class: Some(weight_class.max(1)),
            ..super::model::font_info::CanonicalOpenTypeInfo::default()
        },
        ..CanonicalFontInfo::default()
    };
    font_info.validate().map_err(|error| error.to_string())?;
    let glyphs = template_glyphs()
        .iter()
        .map(|entry| {
            let codepoint = entry
                .unicode
                .as_deref()
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .and_then(char::from_u32);
            if entry.unicode.is_some() && codepoint.is_none() {
                return Err(format!("{} has an invalid Unicode scalar", entry.name));
            }
            Ok(NewFontGlyph {
                name: entry.name.clone(),
                width: if entry.name == "space" {
                    SPACE_WIDTH
                } else {
                    DEFAULT_WIDTH
                },
                codepoint,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(NewFontSpecification { font_info, glyphs })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_specification_carries_the_template() {
        let specification = specification("Untitled", "Regular", 400).unwrap();
        assert_eq!(specification.glyphs.len(), 324);
        assert!(
            specification
                .glyphs
                .iter()
                .any(|glyph| glyph.name == ".notdef")
        );
        let space = specification
            .glyphs
            .iter()
            .find(|glyph| glyph.name == "space")
            .unwrap();
        assert_eq!(space.width, SPACE_WIDTH);
        assert_eq!(space.codepoint, Some(' '));
        let a = specification
            .glyphs
            .iter()
            .find(|glyph| glyph.name == "A")
            .unwrap();
        assert_eq!(a.width, DEFAULT_WIDTH);
        assert_eq!(a.codepoint, Some('A'));
        assert_eq!(
            specification.font_info.names.family_name.as_deref(),
            Some("Untitled")
        );
        assert_eq!(specification.font_info.metrics.units_per_em, Some(1000.0));
    }
}
