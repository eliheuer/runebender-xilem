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
    let mut project = Project::from_source(Master::from_font(font, "Metadata.ufo".into()));

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

    let before = project.compiled_preview().unwrap();
    let revision = project.document_revision();
    let mut font_info = project.document_font_info(SourceId(0)).unwrap().clone();
    font_info.metrics.units_per_em = Some(1001.5);
    font_info.names.family_name = Some("Canonical Metadata Edited".into());
    let outcome = project
        .edit_document_source_metadata(SourceId(0), |draft| {
            assert!(draft.set_font_info(font_info));
            Ok(())
        })
        .unwrap();
    let DocumentEditOutcome::Changed {
        revision: changed_revision,
        change,
    } = outcome
    else {
        panic!("canonical font-info edit reported no change")
    };
    assert_eq!(changed_revision, revision.wrapping_add(1));
    assert!(change.requires_compilation());
    let exact = project.document_font_info(SourceId(0)).unwrap();
    assert_eq!(exact.metrics.units_per_em, Some(1001.5));
    assert_eq!(
        exact.names.family_name.as_deref(),
        Some("Canonical Metadata Edited")
    );

    let edited_snapshot = project.babelfont_snapshot().unwrap();
    assert_eq!(edited_snapshot.upm, 1002);
    assert_eq!(
        edited_snapshot
            .names
            .family_name
            .get_default()
            .map(String::as_str),
        Some("Canonical Metadata Edited")
    );
    let after = project.compiled_preview().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&before, &after));
    assert_ne!(before.bytes, after.bytes);
}

#[test]
fn compiler_snapshot_drops_cleared_canonical_font_info() {
    let mut font = Font::new();
    font.font_info.family_name = Some("Snapshot Clearing".into());
    font.font_info.style_name = Some("Regular".into());
    font.font_info.units_per_em = Some(1000_u32.into());
    font.font_info.ascender = Some(800.0);
    font.font_info.open_type_hhea_ascender = Some(810);
    font.font_info.copyright = Some("Stale copyright".into());
    font.font_info.open_type_name_designer = Some("Stale designer".into());
    font.font_info.open_type_head_flags = Some(vec![0, 3]);
    font.font_info.open_type_os2_vendor_id = Some("TEST".into());
    font.font_info.note = Some("Stale note".into());
    font.font_info.version_major = Some(7);
    font.font_info.version_minor = Some(8);
    let mut glyph = Glyph::new(".notdef");
    glyph.width = 600.0;
    glyph.contours.push(Contour::new(
        vec![
            ContourPoint::new(50.0, 0.0, PointType::Line, false, None, None),
            ContourPoint::new(550.0, 0.0, PointType::Line, false, None, None),
            ContourPoint::new(550.0, 700.0, PointType::Line, false, None, None),
            ContourPoint::new(50.0, 700.0, PointType::Line, false, None, None),
        ],
        None,
    ));
    font.default_layer_mut().insert_glyph(glyph);
    let mut project = Project::from_source(Master::from_font(font, "Clearing.ufo".into()));

    let before = project.babelfont_snapshot().unwrap();
    assert!(!before.names.copyright.is_empty());
    assert!(!before.names.designer.is_empty());
    assert_eq!(
        before.custom_ot_values.os2_vendor_id,
        Some(babelfont::Tag::new(b"TEST"))
    );
    assert_eq!(before.version, (7, 8));
    assert_eq!(
        before.masters[0]
            .metrics
            .get(&babelfont::MetricType::HheaAscender),
        Some(&810)
    );

    let mut info = project.document_font_info(SourceId(0)).unwrap().clone();
    info.metrics.ascender = None;
    info.open_type_metrics.hhea_ascender = None;
    info.names.copyright = None;
    info.names.designer = None;
    info.open_type.head_flags = None;
    info.open_type.vendor_id = None;
    info.note = None;
    info.version_major = None;
    info.version_minor = None;
    let outcome = project
        .edit_document_source_metadata(SourceId(0), |draft| {
            assert!(draft.set_font_info(info));
            Ok(())
        })
        .unwrap();
    assert!(matches!(outcome, DocumentEditOutcome::Changed { .. }));

    let snapshot = project.babelfont_snapshot().unwrap();
    assert!(snapshot.names.copyright.is_empty());
    assert!(snapshot.names.designer.is_empty());
    assert_eq!(snapshot.note, None);
    assert_eq!(snapshot.version, (1, 0));
    assert_eq!(snapshot.custom_ot_values.head_flags, None);
    assert_eq!(snapshot.custom_ot_values.os2_vendor_id, None);
    assert!(
        !snapshot.masters[0]
            .metrics
            .contains_key(&babelfont::MetricType::Ascender)
    );
    assert!(
        !snapshot.masters[0]
            .metrics
            .contains_key(&babelfont::MetricType::HheaAscender)
    );

    assert_eq!(snapshot.masters.len(), 1);
    let glyph = snapshot.glyphs.get(".notdef").unwrap();
    assert_eq!(glyph.layers.len(), 1);
    assert_eq!(glyph.layers[0].width, 600.0);
    assert_eq!(glyph.layers[0].paths().count(), 1);
}
