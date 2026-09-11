// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Marks land on their anchors when the editor shapes: the features
//! written from Virtua's anchors, compiled on the fly, position a
//! mark typed after a base where the base's anchor is.
//!
//! Needs the Virtua Grotesk fixture, like `cli.rs`.

use std::path::PathBuf;

use runebender_core::text::features;
use runebender_core::text::shape::{ShapedGlyph, ShapingFont, ShapingGlyph, ShapingSource};

fn fixture() -> PathBuf {
    let dir = match std::env::var_os("RUNEBENDER_TEST_FONTS") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../virtua-grotesk/sources"),
    };
    let ufo = dir.join("VirtuaGrotesk-Regular.ufo");
    assert!(ufo.is_dir(), "fixture not found at {}", dir.display());
    ufo
}

/// The shaping font for Virtua with the generated features inlined,
/// and the glyph names in id order.
fn shaping_font(font: &norad::Font) -> ShapingFont {
    let mut glyphs: Vec<ShapingGlyph> = font
        .default_layer()
        .iter()
        .map(|g| ShapingGlyph {
            name: g.name().to_string(),
            advance: g.width,
            unicodes: g.codepoints.iter().map(|c| c as u32).collect(),
        })
        .collect();
    glyphs.sort_by(|a, b| a.name.cmp(&b.name));
    if let Some(i) = glyphs.iter().position(|g| g.name == ".notdef") {
        let notdef = glyphs.remove(i);
        glyphs.insert(0, notdef);
    }
    ShapingFont::build(&ShapingSource {
        units_per_em: 1024.0,
        glyphs,
        features: features::with_generated(font),
    })
    .expect("the features compile")
}

/// Absolute pen-relative origin of each shaped glyph: the advances
/// before it plus its own offset. Order-independent, so it holds in
/// both directions.
fn origins(shaped: &[ShapedGlyph]) -> Vec<(f64, f64)> {
    let mut pen = 0.0;
    shaped
        .iter()
        .map(|g| {
            let at = (pen + g.x_offset, g.y_offset);
            pen += g.x_advance;
            at
        })
        .collect()
}

fn anchor(font: &norad::Font, glyph: &str, name: &str) -> (f64, f64) {
    let g = font.get_glyph(glyph).expect(glyph);
    let a = g
        .anchors
        .iter()
        .find(|a| a.name.as_ref().is_some_and(|n| n.as_str() == name))
        .unwrap_or_else(|| panic!("{glyph} has no {name} anchor"));
    (a.x, a.y)
}

#[test]
fn the_generated_features_name_virtuas_classes() {
    let font = norad::Font::load(fixture()).expect("fixture loads");
    let g = features::generate(&font);
    assert!(g.classes.iter().any(|c| c == "top"), "{:?}", g.classes);
    assert!(g.classes.iter().any(|c| c == "bottom"));
    assert!(
        g.marks > 10 && g.bases > 100,
        "{} marks, {} bases",
        g.marks,
        g.bases
    );
    assert!(g.stacked > 0, "marks that carry top stack through mkmk");
    assert!(
        g.fea
            .contains("markClass fatha-ar <anchor 112 592> @MC_top;")
    );
    assert!(
        g.fea
            .contains("pos base alef-ar <anchor 108 768> mark @MC_top;")
    );
    // beh-ar has no anchors of its own; its base component's come through.
    assert!(
        g.fea.contains("pos base beh-ar <anchor "),
        "propagated anchors: {}",
        &g.fea[..600]
    );
    assert!(
        !features::defines_mark_features(&font.features),
        "Virtua's fea has no mark feature"
    );
}

#[test]
fn a_fatha_lands_on_the_alefs_top_anchor() {
    let font = norad::Font::load(fixture()).expect("fixture loads");
    let sf = shaping_font(&font);
    let shaped = sf.shape("\u{0627}\u{064E}", true).expect("shapes");
    let names: Vec<&str> = shaped
        .iter()
        .map(|g| sf.glyph_name(g.glyph_id).unwrap())
        .collect();
    assert!(
        names.contains(&"alef-ar") && names.contains(&"fatha-ar"),
        "{names:?}"
    );
    let at = origins(&shaped);
    let alef = at[names.iter().position(|n| *n == "alef-ar").unwrap()];
    let fatha = at[names.iter().position(|n| *n == "fatha-ar").unwrap()];
    let (bx, by) = anchor(&font, "alef-ar", "top");
    let (mx, my) = anchor(&font, "fatha-ar", "_top");
    assert_eq!(
        (fatha.0 - alef.0, fatha.1 - alef.1),
        (bx - mx, by - my),
        "{shaped:?}"
    );
}

#[test]
fn an_acute_lands_on_the_b() {
    let font = norad::Font::load(fixture()).expect("fixture loads");
    let sf = shaping_font(&font);
    // b, not a: the font has aacute, so a plus U+0301 composes to it
    // before positioning runs, which is right and not what this tests.
    let shaped = sf.shape("b\u{0301}", false).expect("shapes");
    let names: Vec<&str> = shaped
        .iter()
        .map(|g| sf.glyph_name(g.glyph_id).unwrap())
        .collect();
    assert_eq!(names, ["b", "acutecomb"], "{names:?}");
    let at = origins(&shaped);
    let (bx, by) = anchor(&font, "b", "top");
    let (mx, my) = anchor(&font, "acutecomb", "_top");
    assert_eq!(
        (at[1].0 - at[0].0, at[1].1 - at[0].1),
        (bx - mx, by - my),
        "{shaped:?}"
    );
}

#[test]
fn a_sukun_stacks_on_a_shadda_through_mkmk() {
    // sukun, not fatha: shadda plus fatha is a ligature the font draws
    // as one glyph; shadda plus sukun stays two marks, one on the other.
    let font = norad::Font::load(fixture()).expect("fixture loads");
    let sf = shaping_font(&font);
    let shaped = sf.shape("\u{0627}\u{0651}\u{0652}", true).expect("shapes");
    let names: Vec<&str> = shaped
        .iter()
        .map(|g| sf.glyph_name(g.glyph_id).unwrap())
        .collect();
    let at = origins(&shaped);
    let find = |n: &str| {
        at[names
            .iter()
            .position(|x| *x == n)
            .unwrap_or_else(|| panic!("{n} in {names:?}"))]
    };
    let (alef, shadda, sukun) = (find("alef-ar"), find("shadda-ar"), find("sukun-ar"));
    let (atx, aty) = anchor(&font, "alef-ar", "top");
    let (s_x, s_y) = anchor(&font, "shadda-ar", "_top");
    let (stx, sty) = anchor(&font, "shadda-ar", "top");
    let (k_x, k_y) = anchor(&font, "sukun-ar", "_top");
    assert_eq!(
        (shadda.0 - alef.0, shadda.1 - alef.1),
        (atx - s_x, aty - s_y),
        "shadda on alef"
    );
    assert_eq!(
        (sukun.0 - shadda.0, sukun.1 - shadda.1),
        (stx - k_x, sty - k_y),
        "sukun on shadda"
    );
}

#[test]
fn the_text_buffer_lays_a_fatha_on_the_beh() {
    use runebender_core::text::buffer::{TextBuffer, TextGlyphInventory};
    let font = norad::Font::load(fixture()).expect("fixture loads");
    // The shaper's answer, to hold the buffer to.
    let sf = shaping_font(&font);
    let shaped = sf.shape("\u{0628}\u{064E}", true).expect("shapes");
    let names: Vec<&str> = shaped
        .iter()
        .map(|g| sf.glyph_name(g.glyph_id).unwrap())
        .collect();
    let at = origins(&shaped);
    let beh = at[names
        .iter()
        .position(|n| *n == "beh-ar")
        .unwrap_or_else(|| panic!("{names:?}"))];
    let fatha = at[names.iter().position(|n| *n == "fatha-ar").unwrap()];
    // beh-ar has no anchors of its own; its top comes from behDotless-ar
    // through the component, and the fatha's _top lands on it.
    let beh_glyph = font.get_glyph("beh-ar").unwrap();
    let (tx, ty) = features::anchors(&font, beh_glyph)
        .into_iter()
        .find(|(n, _, _)| n == "top")
        .map(|(_, x, y)| (x, y))
        .expect("beh-ar offers top through its components");
    let (mx, my) = anchor(&font, "fatha-ar", "_top");
    assert_eq!(
        (fatha.0 - beh.0, fatha.1 - beh.1),
        (tx - mx, ty - my),
        "{names:?} {shaped:?}"
    );

    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(TextGlyphInventory::from_font(&font));
    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{064E}'));
    let layout = buffer.layout(1200.0);
    let item = |index: usize| {
        layout
            .items
            .iter()
            .find(|i| i.index == index)
            .copied()
            .unwrap_or_else(|| panic!("no item for sort {index}: {layout:?}"))
    };
    let (b, f) = (item(0), item(1));
    assert_eq!(
        buffer.shaped_offset(1).1,
        fatha.1 - beh.1,
        "the offset shaping gave"
    );
    assert_eq!(
        f.y - b.y,
        fatha.1 - beh.1,
        "laid out where the shaper put it"
    );
    assert_eq!(f.x - b.x, fatha.0 - beh.0);
}
