// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A blank UFO set up the way Google Fonts expects, with the GF Latin
//! Core glyph set as empty encoded glyphs. This is File > New Font in
//! the editor. A port of runebender-web's newProject.ts, built through
//! norad instead of hand-written plists.

use std::sync::OnceLock;

use serde::Deserialize;

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

fn template() -> &'static [TemplateGlyph] {
    static GLYPHS: OnceLock<Vec<TemplateGlyph>> = OnceLock::new();
    GLYPHS.get_or_init(|| {
        serde_json::from_str(include_str!("../../data/new-font-template.json"))
            .expect("new-font-template.json parses")
    })
}

/// Build a new master with GF-shaped fontinfo and the GF Latin Core
/// glyph set as empty encoded glyphs.
pub fn new_font(family: &str, style: &str, weight_class: i32) -> norad::Font {
    let mut font = norad::Font::new();
    let info = &mut font.font_info;
    info.family_name = Some(family.to_string());
    info.style_name = Some(style.to_string());
    info.units_per_em = norad::fontinfo::NonNegativeIntegerOrFloat::try_from(UPM).ok();
    info.ascender = Some(ASCENDER);
    info.descender = Some(DESCENDER);
    info.cap_height = Some(CAP_HEIGHT);
    info.x_height = Some(X_HEIGHT);
    info.open_type_os2_weight_class = Some(weight_class.max(1) as u32);

    let layer = font.default_layer_mut();
    for entry in template() {
        let mut glyph = norad::Glyph::new(entry.name.as_str());
        glyph.width = if entry.name == "space" {
            SPACE_WIDTH
        } else {
            DEFAULT_WIDTH
        };
        if let Some(codepoint) = entry
            .unicode
            .as_deref()
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .and_then(char::from_u32)
        {
            glyph.codepoints = norad::Codepoints::new([codepoint]);
        }
        layer.insert_glyph(glyph);
    }
    font
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_font_carries_the_template() {
        let font = new_font("Untitled", "Regular", 400);
        assert_eq!(font.default_layer().len(), 324);
        assert!(font.get_glyph(".notdef").is_some());
        let space = font.get_glyph("space").unwrap();
        assert_eq!(space.width, SPACE_WIDTH);
        assert_eq!(space.codepoints.iter().next(), Some(' '));
        let a = font.get_glyph("A").unwrap();
        assert_eq!(a.width, DEFAULT_WIDTH);
        assert_eq!(a.codepoints.iter().next(), Some('A'));
        assert_eq!(font.font_info.family_name.as_deref(), Some("Untitled"));
        assert_eq!(
            font.font_info.units_per_em.map(|v| v.as_f64()),
            Some(1000.0)
        );
    }
}
