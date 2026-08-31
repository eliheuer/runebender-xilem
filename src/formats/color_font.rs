// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Color font data in the UFO lib: palettes, layer mapping, COLR paint.
//!
//! Uses the `com.github.googlei18n.ufo2ft.*` keys so a font built with
//! fontmake or fontc reads the same data. Fontra edits the same keys.

/// The explicit color-layers key, fontTools buildCOLR's input.
///
/// Once present, ufo2ft skips its own layer exploding, so writing
/// any v1 entry means exploding every color glyph ourselves.
pub const COLOR_LAYERS_EXPLICIT_KEY: &str = "com.github.googlei18n.ufo2ft.colorLayers";

/// A COLRv1 linear-gradient paint dict in fontTools' unbuilt form.
///
/// The gradient holds two palette stops and runs from `p0` to `p1`.
/// `x2`/`y2` is the required rotation vector, perpendicular to the
/// gradient.
pub fn linear_gradient_paint(
    stop0: usize,
    stop1: usize,
    p0: (f64, f64),
    p1: (f64, f64),
) -> plist::Value {
    let stop = |offset: f64, palette: usize| {
        let mut dict = plist::Dictionary::new();
        dict.insert("StopOffset".into(), plist::Value::Real(offset));
        dict.insert(
            "PaletteIndex".into(),
            plist::Value::Integer((palette as u64).into()),
        );
        dict.insert("Alpha".into(), plist::Value::Real(1.0));
        plist::Value::Dictionary(dict)
    };
    let mut color_line = plist::Dictionary::new();
    color_line.insert(
        "ColorStop".into(),
        plist::Value::Array(vec![stop(0.0, stop0), stop(1.0, stop1)]),
    );
    color_line.insert("Extend".into(), plist::Value::String("pad".into()));
    let mut paint = plist::Dictionary::new();
    // PaintLinearGradient.
    paint.insert("Format".into(), plist::Value::Integer(4u64.into()));
    paint.insert("ColorLine".into(), plist::Value::Dictionary(color_line));
    paint.insert("x0".into(), plist::Value::Real(p0.0));
    paint.insert("y0".into(), plist::Value::Real(p0.1));
    paint.insert("x1".into(), plist::Value::Real(p1.0));
    paint.insert("y1".into(), plist::Value::Real(p1.1));
    // Rotation vector: perpendicular to p0->p1.
    paint.insert("x2".into(), plist::Value::Real(p0.0 + (p1.1 - p0.1)));
    paint.insert("y2".into(), plist::Value::Real(p0.1 - (p1.0 - p0.0)));
    plist::Value::Dictionary(paint)
}

/// A PaintGlyph layer (Format 10) wrapping a child paint.
///
/// Together with the solid child (Format 2), these are the shapes
/// verified through ufo2ft's buildCOLR: the glyph's root is
/// PaintColrLayers (Format 1) with these as Layers.
pub fn paint_glyph_layer(glyph: &str, child: plist::Value) -> plist::Value {
    let mut dict = plist::Dictionary::new();
    dict.insert("Format".into(), plist::Value::Integer(10u64.into()));
    dict.insert("Glyph".into(), plist::Value::String(glyph.into()));
    dict.insert("Paint".into(), child);
    plist::Value::Dictionary(dict)
}

/// Builds a PaintSolid (Format 2) paint at full alpha for the given palette index.
pub fn paint_solid(palette: usize) -> plist::Value {
    let mut dict = plist::Dictionary::new();
    dict.insert("Format".into(), plist::Value::Integer(2u64.into()));
    dict.insert(
        "PaletteIndex".into(),
        plist::Value::Integer((palette as u64).into()),
    );
    dict.insert("Alpha".into(), plist::Value::Real(1.0));
    plist::Value::Dictionary(dict)
}

/// Does this font carry explicit (v1) color layers for the glyph?
pub fn has_v1_entry(font: &norad::Font, glyph: &str) -> bool {
    font.lib
        .get(COLOR_LAYERS_EXPLICIT_KEY)
        .and_then(|v| v.as_dictionary())
        .is_some_and(|d| d.contains_key(glyph))
}

/// Font lib key holding the ufo2ft color palettes as arrays of `[r, g, b, a]` rows.
pub const COLOR_PALETTES_KEY: &str = "com.github.googlei18n.ufo2ft.colorPalettes";

/// Font lib key mapping color layer names to palette indices, bottom layer first.
pub const COLOR_LAYER_MAPPING_KEY: &str = "com.github.googlei18n.ufo2ft.colorLayerMapping";

/// The first palette: [r, g, b, a] float rows.
pub fn read_color_palette(font: &norad::Font) -> Vec<[f64; 4]> {
    let number = |v: &plist::Value| {
        v.as_real()
            .or_else(|| v.as_signed_integer().map(|n| n as f64))
    };
    font.lib
        .get(COLOR_PALETTES_KEY)
        .and_then(|v| v.as_array())
        .and_then(|palettes| palettes.first())
        .and_then(|p| p.as_array())
        .map(|colors| {
            colors
                .iter()
                .filter_map(|c| {
                    let arr = c.as_array()?;
                    let mut out = [0.0, 0.0, 0.0, 1.0];
                    for (i, v) in arr.iter().take(4).enumerate() {
                        out[i] = number(v)?;
                    }
                    Some(out)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Writes `palette` as the font's single color palette under [`COLOR_PALETTES_KEY`].
pub fn write_color_palette(font: &mut norad::Font, palette: &[[f64; 4]]) {
    let value = plist::Value::Array(vec![plist::Value::Array(
        palette
            .iter()
            .map(|c| plist::Value::Array(c.iter().map(|&v| plist::Value::Real(v)).collect()))
            .collect(),
    )]);
    font.lib.insert(COLOR_PALETTES_KEY.into(), value);
}

/// The font-level layer mapping: (layer name, palette index), bottom
/// layer first.
pub fn read_color_mapping(font: &norad::Font) -> Vec<(String, usize)> {
    font.lib
        .get(COLOR_LAYER_MAPPING_KEY)
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let arr = row.as_array()?;
                    let layer = arr.first()?.as_string()?.to_string();
                    let color = arr
                        .get(1)?
                        .as_signed_integer()
                        .or_else(|| arr.get(1)?.as_real().map(|v| v as i64))?;
                    Some((layer, color.max(0) as usize))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Writes the layer-to-palette mapping under [`COLOR_LAYER_MAPPING_KEY`]. An empty `mapping` removes the key.
pub fn write_color_mapping(font: &mut norad::Font, mapping: &[(String, usize)]) {
    if mapping.is_empty() {
        font.lib.remove(COLOR_LAYER_MAPPING_KEY);
        return;
    }
    let value = plist::Value::Array(
        mapping
            .iter()
            .map(|(layer, color)| {
                plist::Value::Array(vec![
                    plist::Value::String(layer.clone()),
                    plist::Value::Integer((*color as u64).into()),
                ])
            })
            .collect(),
    );
    font.lib.insert(COLOR_LAYER_MAPPING_KEY.into(), value);
}

/// Parse #RRGGBB or #RRGGBBAA (the # optional).
pub fn parse_hex_color(text: &str) -> Option<[f64; 4]> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }
    let byte = |i: usize| {
        u8::from_str_radix(&hex[i..i + 2], 16)
            .ok()
            .map(|v| v as f64 / 255.0)
    };
    Some([
        byte(0)?,
        byte(2)?,
        byte(4)?,
        if hex.len() == 8 { byte(6)? } else { 1.0 },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_lib_keys_roundtrip() {
        // Palette + mapping written through the helpers must read
        // back identically after a norad save/load, in the exact
        // shape ufo2ft's COLR builder consumes.
        let mut font = crate::document::new_font::new_font("Col", "Regular", 400);
        let palette = vec![[1.0, 0.2, 0.0, 1.0], [0.0, 0.4, 1.0, 0.5]];
        write_color_palette(&mut font, &palette);
        let mapping = vec![("color.0".into(), 0usize), ("color.1".into(), 1)];
        write_color_mapping(&mut font, &mapping);
        // A layer glyph so the layers round-trip too.
        let glyph_name = font
            .default_layer()
            .iter()
            .next()
            .map(|g| g.name().to_string())
            .unwrap();
        let seed = font.default_layer().get_glyph(&glyph_name).unwrap().clone();
        for layer in ["color.0", "color.1"] {
            let mut copy = norad::Glyph::new(glyph_name.as_str());
            copy.width = seed.width;
            font.layers
                .get_or_create_layer(layer)
                .unwrap()
                .insert_glyph(copy);
        }
        let dir = std::env::temp_dir().join("rb-color-roundtrip.ufo");
        std::fs::remove_dir_all(&dir).ok();
        font.save(&dir).expect("saves");
        let back = norad::Font::load(&dir).expect("reloads");
        assert_eq!(read_color_palette(&back), palette);
        assert_eq!(read_color_mapping(&back), mapping);
        assert!(back.layers.get("color.0").is_some());
        std::fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn colrv1_paint_shapes() {
        // The exact structures verified against ufo2ft's buildCOLR:
        // PaintColrLayers root (1), PaintGlyph layers (10), solid (2)
        // and linear-gradient (4) children.
        let solid = paint_solid(3);
        let d = solid.as_dictionary().unwrap();
        assert_eq!(d.get("Format").unwrap().as_signed_integer(), Some(2));
        assert_eq!(d.get("PaletteIndex").unwrap().as_signed_integer(), Some(3));
        let layer = paint_glyph_layer("A.color.0", solid);
        let d = layer.as_dictionary().unwrap();
        assert_eq!(d.get("Format").unwrap().as_signed_integer(), Some(10));
        assert_eq!(d.get("Glyph").unwrap().as_string(), Some("A.color.0"));
        let grad = linear_gradient_paint(1, 0, (0.0, 0.0), (0.0, 800.0));
        let d = grad.as_dictionary().unwrap();
        assert_eq!(d.get("Format").unwrap().as_signed_integer(), Some(4));
        // Rotation vector is perpendicular to the vertical gradient.
        assert_eq!(d.get("x2").unwrap().as_real(), Some(800.0));
        assert_eq!(d.get("y2").unwrap().as_real(), Some(0.0));
        let stops = d
            .get("ColorLine")
            .and_then(|v| v.as_dictionary())
            .and_then(|c| c.get("ColorStop"))
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(stops.len(), 2);
    }
}
