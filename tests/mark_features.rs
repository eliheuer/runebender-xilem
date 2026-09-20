// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Marks land on their anchors when the editor shapes: the features
//! written from Virtua's anchors, compiled on the fly, position a
//! mark typed after a base where the base's anchor is.
//!
//! Needs the Virtua Grotesk fixture, like `cli.rs`.

use std::path::PathBuf;

use runebender::document::project::Project;
use runebender::document::variable::SourceId;
use runebender::text::features;
use runebender::text::shape::{ShapedGlyph, ShapingFont, ShapingGlyph, ShapingSource};

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
fn project() -> (Project, SourceId) {
    let project = Project::load(&fixture()).expect("fixture loads");
    let source = project.source_id(0).unwrap();
    (project, source)
}

fn shaping_font(project: &Project, source: SourceId) -> ShapingFont {
    let layer = project.document_source(source).unwrap().default_layer();
    let mut glyphs: Vec<ShapingGlyph> = project
        .glyph_names()
        .filter_map(|name| {
            let glyph = project.document_layer(name, &layer)?;
            Some(ShapingGlyph {
                name: name.to_owned(),
                advance: glyph.width(),
                unicodes: glyph.codepoints().map(u32::from).collect(),
            })
        })
        .collect();
    glyphs.sort_by(|a, b| a.name.cmp(&b.name));
    if let Some(i) = glyphs.iter().position(|g| g.name == ".notdef") {
        let notdef = glyphs.remove(i);
        glyphs.insert(0, notdef);
    }
    ShapingFont::build(&ShapingSource {
        units_per_em: project
            .document_font_info(source)
            .unwrap()
            .metrics
            .resolved()
            .units_per_em,
        glyphs,
        features: features::with_generated_project(project, source).unwrap(),
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

fn anchor(project: &Project, source: SourceId, glyph: &str, name: &str) -> (f64, f64) {
    features::anchors_project(project, source, glyph)
        .unwrap()
        .into_iter()
        .find(|(candidate, _, _)| candidate == name)
        .map(|(_, x, y)| (x, y))
        .unwrap_or_else(|| panic!("{glyph} has no {name} anchor"))
}

#[test]
fn the_generated_features_name_virtuas_classes() {
    let (project, source) = project();
    let g = features::generate_project(&project, source).unwrap();
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
        !features::defines_mark_features(project.document_feature_text(source).unwrap()),
        "Virtua's fea has no mark feature"
    );
}

#[test]
fn a_fatha_lands_on_the_alefs_top_anchor() {
    let (project, source) = project();
    let sf = shaping_font(&project, source);
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
    let (bx, by) = anchor(&project, source, "alef-ar", "top");
    let (mx, my) = anchor(&project, source, "fatha-ar", "_top");
    assert_eq!(
        (fatha.0 - alef.0, fatha.1 - alef.1),
        (bx - mx, by - my),
        "{shaped:?}"
    );
}

#[test]
fn an_acute_lands_on_the_b() {
    let (project, source) = project();
    let sf = shaping_font(&project, source);
    // b, not a: the font has aacute, so a plus U+0301 composes to it
    // before positioning runs, which is right and not what this tests.
    let shaped = sf.shape("b\u{0301}", false).expect("shapes");
    let names: Vec<&str> = shaped
        .iter()
        .map(|g| sf.glyph_name(g.glyph_id).unwrap())
        .collect();
    assert_eq!(names, ["b", "acutecomb"], "{names:?}");
    let at = origins(&shaped);
    let (bx, by) = anchor(&project, source, "b", "top");
    let (mx, my) = anchor(&project, source, "acutecomb", "_top");
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
    let (project, source) = project();
    let sf = shaping_font(&project, source);
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
    let (atx, aty) = anchor(&project, source, "alef-ar", "top");
    let (s_x, s_y) = anchor(&project, source, "shadda-ar", "_top");
    let (stx, sty) = anchor(&project, source, "shadda-ar", "top");
    let (k_x, k_y) = anchor(&project, source, "sukun-ar", "_top");
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
    use runebender::text::buffer::{TextBuffer, TextGlyphInventory};
    let (project, source) = project();
    // The shaper's answer, to hold the buffer to.
    let sf = shaping_font(&project, source);
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
    let (tx, ty) = features::anchors_project(&project, source, "beh-ar")
        .unwrap()
        .into_iter()
        .find(|(n, _, _)| n == "top")
        .map(|(_, x, y)| (x, y))
        .expect("beh-ar offers top through its components");
    let (mx, my) = anchor(&project, source, "fatha-ar", "_top");
    assert_eq!(
        (fatha.0 - beh.0, fatha.1 - beh.1),
        (tx - mx, ty - my),
        "{names:?} {shaped:?}"
    );

    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(TextGlyphInventory::from_project(&project, source).unwrap());
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
