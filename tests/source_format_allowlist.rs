// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Executable coverage for the reviewed UFO/Designspace preservation allowlist.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use norad::{
    AffineTransform, Anchor, Color, Component, Contour, ContourPoint, Font, Glyph, Guideline,
    Identifier, Image, Line, Name, PointType,
};
use runebender::document::project::Project;
use runebender::document::variable::SourceId;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "runebender-source-format-allowlist-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn populated_ufo_allowlist_survives_canonical_edit_save_and_reopen() {
    let scratch = Scratch::new();
    let path = scratch.0.join("Allowlist.ufo");
    let font = populated_font();
    font.save(&path).unwrap();
    std::fs::create_dir(path.join("vendor")).unwrap();
    std::fs::write(path.join("vendor/opaque.bin"), [0, 1, 2, 255]).unwrap();

    let mut expected = Font::load(&path).unwrap();
    expected.get_glyph_mut("A").unwrap().width = 612.5;
    let mut project = Project::load(&path).unwrap();
    let layer = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    project
        .edit_document_layer("A", &layer, |draft| {
            assert!(draft.set_width(612.5)?);
            Ok(())
        })
        .unwrap();

    project.save().unwrap();

    assert_eq!(
        Font::load(&path).unwrap(),
        expected,
        "a populated supported UFO field family was silently lost"
    );
    assert_eq!(
        std::fs::read(path.join("vendor/opaque.bin")).unwrap(),
        [0, 1, 2, 255]
    );
}

fn populated_font() -> Font {
    let mut font = Font::new();
    font.meta.creator = Some("com.example.allowlist".into());
    font.meta.format_version_minor = 1;
    font.font_info.family_name = Some("Allowlist Fixture".into());
    font.font_info.style_name = Some("Regular".into());
    font.font_info.units_per_em =
        Some(norad::fontinfo::NonNegativeIntegerOrFloat::try_from(1000.125).unwrap());
    font.font_info.ascender = Some(812.75);
    font.font_info.open_type_name_compatible_full_name = Some("Preserved Compatible Name".into());
    font.font_info.postscript_blue_scale = Some(0.039_625);
    font.features = "feature kern { pos A A -80.5; } kern;\n".into();
    font.groups.insert(
        Name::new("public.kern1.A").unwrap(),
        vec![Name::new("A").unwrap(), Name::new("base").unwrap()],
    );
    font.kerning
        .entry(Name::new("public.kern1.A").unwrap())
        .or_default()
        .insert(Name::new("A").unwrap(), -80.5);
    font.lib.insert(
        "com.example.unknown".into(),
        plist::Value::Data(vec![0, 1, 255]),
    );
    font.lib.insert(
        "public.skipExportGlyphs".into(),
        plist::Value::Array(vec!["A".into(), "future".into()]),
    );
    let mut categories = plist::Dictionary::new();
    categories.insert("A".into(), plist::Value::String("mark".into()));
    categories.insert(
        "future".into(),
        plist::Value::String("future-category".into()),
    );
    font.lib.insert(
        "public.openTypeCategories".into(),
        plist::Value::Dictionary(categories),
    );
    font.data
        .insert("private/payload.bin".into(), vec![1, 7, 19])
        .unwrap();
    font.images
        .insert(
            "reference.png".into(),
            include_bytes!("fixtures/variable/reference.png").to_vec(),
        )
        .unwrap();

    font.default_layer_mut().insert_glyph(Glyph::new("base"));
    font.default_layer_mut().insert_glyph(populated_glyph());
    let background = font.layers.new_layer("Background").unwrap();
    background.color = Some(Color::new(0.1, 0.2, 0.3, 0.4).unwrap());
    background
        .lib
        .insert("com.example.layer".into(), "preserved".into());
    let mut background_glyph = Glyph::new("A");
    background_glyph.width = 510.25;
    background_glyph.note = Some("background payload".into());
    background.insert_glyph(background_glyph);
    font
}

fn populated_glyph() -> Glyph {
    let mut glyph = Glyph::new("A");
    glyph.width = 600.125;
    glyph.height = 1000.25;
    glyph.codepoints = norad::Codepoints::new(['A', '\u{391}']);
    glyph.note = Some(String::new());
    glyph.image = Some(
        Image::new(
            "reference.png".into(),
            Some(Color::new(0.8, 0.1, 0.2, 0.7).unwrap()),
            AffineTransform {
                x_scale: 0.9,
                xy_scale: 0.1,
                yx_scale: -0.2,
                y_scale: 1.1,
                x_offset: 12.25,
                y_offset: -9.5,
            },
        )
        .unwrap(),
    );
    glyph.lib.insert(
        "com.example.glyph".into(),
        plist::Value::String("preserved".into()),
    );

    let mut guideline = Guideline::new(
        Line::Angle {
            x: 20.25,
            y: 30.5,
            degrees: 12.75,
        },
        Some(Name::new("slant reference").unwrap()),
        Some(Color::new(0.2, 0.4, 0.6, 0.8).unwrap()),
        Some(Identifier::new("guide.1").unwrap()),
    );
    guideline.replace_lib(object_lib("guideline"));
    glyph.guidelines.push(guideline);

    let mut anchor = Anchor::new(
        100.25,
        800.5,
        Some(Name::new("top").unwrap()),
        Some(Color::new(0.1, 0.2, 0.3, 0.4).unwrap()),
        Some(Identifier::new("anchor.1").unwrap()),
    );
    anchor.replace_lib(object_lib("anchor"));
    glyph.anchors.push(anchor);

    let mut first = ContourPoint::new(
        0.25,
        0.5,
        PointType::Line,
        false,
        Some(Name::new("start").unwrap()),
        Some(Identifier::new("point.1").unwrap()),
    );
    first.replace_lib(object_lib("point"));
    let second = ContourPoint::new(
        50.75,
        100.125,
        PointType::Line,
        true,
        None,
        Some(Identifier::new("point.2").unwrap()),
    );
    let mut contour = Contour::new(
        vec![first, second],
        Some(Identifier::new("contour.1").unwrap()),
    );
    contour.replace_lib(object_lib("contour"));
    glyph.contours.push(contour);

    let mut component = Component::new(
        Name::new("base").unwrap(),
        AffineTransform {
            x_scale: 1.0,
            xy_scale: 0.125,
            yx_scale: -0.25,
            y_scale: 0.875,
            x_offset: 25.5,
            y_offset: -12.75,
        },
        Some(Identifier::new("component.1").unwrap()),
    );
    component.replace_lib(object_lib("component"));
    glyph.components.push(component);
    glyph
}

fn object_lib(owner: &str) -> plist::Dictionary {
    let mut lib = plist::Dictionary::new();
    lib.insert(
        "com.example.object".into(),
        plist::Value::String(owner.into()),
    );
    lib
}
