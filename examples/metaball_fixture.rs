// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Writes a small, editable metaball UFO for testing an editor without touching a real font.

use runebender::formats::metaballs::{Metaball, MetaballGroup, Metaballs, write_metaballs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .ok_or("usage: metaball_fixture <new.ufo>")?;
    if std::path::Path::new(&out).exists() {
        return Err("output already exists".into());
    }
    let mut font = norad::Font::new();
    font.font_info.family_name = Some("Metaball Study".into());
    font.font_info.style_name = Some("Regular".into());
    font.font_info.units_per_em = Some(1000.into());
    font.font_info.ascender = Some(800.0);
    font.font_info.descender = Some(-200.0);
    font.font_info.x_height = Some(500.0);
    font.font_info.cap_height = Some(700.0);
    let mut glyph = norad::Glyph::new("i");
    glyph.codepoints.insert('i');
    glyph.width = 500.0;
    let ball = |id, y, radius| Metaball {
        id,
        x: 250.0,
        y,
        radius,
        stiffness: 2.0,
    };
    write_metaballs(
        &mut glyph,
        &Metaballs {
            version: 1,
            groups: vec![
                MetaballGroup {
                    blend: None,
                    id: 1,
                    threshold: 0.5,
                    balls: vec![ball(1, 700.0, 110.0)],
                    links: Vec::new(),
                },
                MetaballGroup {
                    blend: None,
                    id: 2,
                    threshold: 0.5,
                    balls: vec![ball(1, 160.0, 200.0), ball(2, 370.0, 200.0)],
                    links: Vec::new(),
                },
            ],
        },
    )?;
    font.default_layer_mut().insert_glyph(glyph);
    font.save(&out)?;
    println!("{out}");
    Ok(())
}
