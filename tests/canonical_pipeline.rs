// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical document inputs must reach the interpolation and compiler pipeline unsaved.

use norad::{Contour, ContourPoint, Font, Glyph, Name, PointType};
use runebender::document::canonical_metadata::KerningParticipant;
use runebender::document::font_memory::designspace_from_str;
use runebender::document::model::glyph_metadata::OpenTypeGlyphCategory;
use runebender::document::project::{DocumentEditOutcome, Master, Project};
use runebender::document::variable::SourceId;
use runebender::text::shape::ShapingFont;

fn project() -> Project {
    let designspace = designspace_from_str(
        r#"<designspace format="5.0">
          <axes><axis tag="wght" name="Weight" minimum="400" default="400" maximum="900"/></axes>
          <sources>
            <source filename="Regular.ufo" name="regular"><location><dimension name="Weight" xvalue="400"/></location></source>
            <source filename="Bold.ufo" name="bold"><location><dimension name="Weight" xvalue="900"/></location></source>
          </sources>
        </designspace>"#,
    )
    .unwrap();
    Project::from_designspace(designspace, |name| {
        let bold = name == "Bold.ufo";
        let mut font = Font::new();
        font.font_info.family_name = Some("Canonical Pipeline".into());
        font.font_info.style_name = Some(if bold { "Bold" } else { "Regular" }.into());
        font.font_info.units_per_em = Some(1000_u32.into());
        for (name, unicode) in [(".notdef", None), ("A", Some('A')), ("V", Some('V'))] {
            let mut glyph = Glyph::new(name);
            glyph.width = if bold { 800.0 } else { 500.0 };
            if let Some(unicode) = unicode {
                glyph.codepoints.insert(unicode);
            }
            glyph.contours.push(Contour::new(
                vec![
                    ContourPoint::new(50.0, 0.0, PointType::Line, false, None, None),
                    ContourPoint::new(350.0, 0.0, PointType::Line, false, None, None),
                    ContourPoint::new(350.0, 700.0, PointType::Line, false, None, None),
                    ContourPoint::new(50.0, 700.0, PointType::Line, false, None, None),
                ],
                None,
            ));
            font.default_layer_mut().insert_glyph(glyph);
        }
        font.kerning
            .entry(Name::new("A").unwrap())
            .or_default()
            .insert(Name::new("V").unwrap(), if bold { -150.0 } else { -50.0 });
        Ok(Master::from_font(font, name.into()))
    })
    .unwrap()
}

fn shaped_first_advance(project: &Project, location: f64) -> f64 {
    let compiled = project.compiled_preview().unwrap();
    let shaped = ShapingFont::from_bytes((*compiled.bytes).clone())
        .unwrap()
        .at_normalized(vec![location])
        .shape("AV", false)
        .unwrap();
    assert_eq!(shaped.len(), 2, "the fixture has no substitution features");
    shaped[0].x_advance
}

#[test]
fn canonical_kerning_metadata_drives_unsaved_compilation_and_invalidation() {
    let mut project = project();
    let before = project.compiled_preview().unwrap();
    assert_eq!(shaped_first_advance(&project, 0.0), 450.0);
    assert_eq!(shaped_first_advance(&project, 1.0), 650.0);

    for (source, value) in [(SourceId(0), -75.5), (SourceId(1), -175.5)] {
        let mut metadata = project.document_font_metadata(source).unwrap().clone();
        assert!(
            metadata
                .set_kerning_pair(
                    KerningParticipant::glyph("A").unwrap(),
                    KerningParticipant::glyph("V").unwrap(),
                    Some(value),
                )
                .unwrap()
        );
        let revision = project.document_revision();
        let outcome = project
            .edit_document_source_metadata(source, |draft| {
                assert!(draft.set_font_metadata(metadata));
                Ok(())
            })
            .unwrap();
        let DocumentEditOutcome::Changed {
            revision: changed_revision,
            change,
        } = outcome
        else {
            panic!("a changed canonical kerning value must commit")
        };
        assert_eq!(changed_revision, revision.wrapping_add(1));
        assert!(change.requires_compilation());
        assert_eq!(
            project
                .document_font_metadata(source)
                .unwrap()
                .resolved_kerning("A", "V"),
            Some(value)
        );
        assert_eq!(
            project.source_snapshot(source).unwrap().kerning["A"]["V"],
            value
        );
    }

    let after = project.compiled_preview().unwrap();
    assert_ne!(before.bytes, after.bytes);
    assert_eq!(shaped_first_advance(&project, 0.0), 424.0);
    assert_eq!(shaped_first_advance(&project, 1.0), 624.0);
}

#[test]
fn compiler_snapshot_reads_canonical_source_glyph_metadata() {
    let mut font = Font::new();
    font.font_info.family_name = Some("Canonical Metadata".into());
    font.font_info.style_name = Some("Regular".into());
    font.font_info.units_per_em = Some(1000.25_f64.try_into().unwrap());
    font.font_info.ascender = Some(812.75);
    font.font_info.open_type_hhea_ascender = Some(813);
    font.default_layer_mut().insert_glyph(Glyph::new(".notdef"));
    font.default_layer_mut().insert_glyph(Glyph::new("A"));
    font.lib.insert(
        "public.skipExportGlyphs".into(),
        plist::Value::Array(vec![plist::Value::String("A".into())]),
    );
    font.lib.insert(
        "public.openTypeCategories".into(),
        plist::Value::Dictionary(plist::Dictionary::from_iter([(
            String::from("A"),
            plist::Value::String("mark".into()),
        )])),
    );
    let project = Project::from_source(Master::from_font(font, "Metadata.ufo".into()));

    let metadata = project
        .document_source_glyph_metadata(SourceId(0), "A")
        .unwrap();
    assert!(!metadata.exported());
    assert_eq!(metadata.category(), Some(&OpenTypeGlyphCategory::Mark));

    let snapshot = project.babelfont_snapshot().unwrap();
    assert_eq!(snapshot.upm, 1000);
    assert_eq!(
        snapshot.names.family_name.get_default().map(String::as_str),
        Some("Canonical Metadata")
    );
    assert_eq!(
        snapshot.masters[0]
            .metrics
            .get(&babelfont::MetricType::Ascender),
        Some(&813)
    );
    assert_eq!(
        snapshot.masters[0]
            .metrics
            .get(&babelfont::MetricType::HheaAscender),
        Some(&813)
    );
    let glyph = snapshot.glyphs.get("A").unwrap();
    assert!(!glyph.exported);
    assert_eq!(glyph.category, babelfont::GlyphCategory::Mark);
}
