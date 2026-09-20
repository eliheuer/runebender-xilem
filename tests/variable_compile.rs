// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Live variable compilation must agree with shaping and exported font tables.

use norad::{Contour, ContourPoint, Font, Glyph, Name, PointType};
use runebender::document::DocumentEditError;
use runebender::document::font_memory::designspace_from_str;
use runebender::document::project::{DocumentEditOutcome, Master, Project};
use runebender::document::variable::{LayerId, SourceId};
use runebender::text::shape::ShapingFont;
use skrifa::raw::TableProvider as _;

fn designspace() -> norad::designspace::DesignSpaceDocument {
    designspace_from_str(r#"<designspace format="5.0">
      <axes><axis tag="wght" name="Weight" minimum="400" default="400" maximum="900"/></axes>
      <sources>
        <source filename="Regular.ufo" name="regular"><location><dimension name="Weight" xvalue="400"/></location></source>
        <source filename="Bold.ufo" name="bold"><location><dimension name="Weight" xvalue="900"/></location></source>
      </sources></designspace>"#).unwrap()
}

fn project_from_designspace(doc: norad::designspace::DesignSpaceDocument) -> Project {
    Project::from_designspace(doc, |name| {
        let bold = name == "Bold.ufo";
        let mut font = Font::new();
        font.font_info.family_name = Some("Live Variable".into());
        font.font_info.style_name = Some(if bold { "Bold" } else { "Regular" }.into());
        font.font_info.units_per_em = Some(1000_u32.into());
        font.features = "feature liga { sub A V by AV; } liga;".into();
        for (name, unicode) in [
            (".notdef", None),
            ("A", Some('A')),
            ("V", Some('V')),
            ("AV", None),
        ] {
            let mut glyph = Glyph::new(name);
            glyph.width = if bold { 800.0 } else { 500.0 };
            if let Some(unicode) = unicode {
                glyph.codepoints.insert(unicode);
            }
            let right = if bold { 650.0 } else { 350.0 };
            glyph.contours.push(Contour::new(
                vec![
                    ContourPoint::new(50.0, 0.0, PointType::Line, false, None, None),
                    ContourPoint::new(right, 0.0, PointType::Line, false, None, None),
                    ContourPoint::new(right, 700.0, PointType::Line, false, None, None),
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

fn project() -> Project {
    project_from_designspace(designspace())
}

#[test]
fn unsaved_variable_document_compiles_outlines_advances_kerning_and_ligatures() {
    let mut project = project();
    let compiled = project.compile().unwrap();
    let font = skrifa::FontRef::new(&compiled.bytes).unwrap();
    assert!(font.fvar().is_ok());
    assert!(font.gvar().is_ok());
    assert!(font.hvar().is_ok());
    for (location, expected) in [(0.0, 450.0), (0.5, 550.0), (1.0, 650.0)] {
        use kurbo::Shape as _;
        let outlines = compiled.outlines(&[location]).unwrap();
        let outline = &outlines.iter().find(|(name, _)| name == "A").unwrap().1;
        assert_eq!(outline.bounding_box().x1, 350.0 + location * 300.0);
        let shaping = ShapingFont::from_bytes((*compiled.bytes).clone())
            .unwrap()
            .at_normalized(vec![location]);
        let shaped = shaping
            .shape_with_features("AV", false, &[("liga".into(), false)])
            .unwrap();
        assert_eq!(shaped.len(), 2);
        assert_eq!(
            shaped[0].x_advance, expected,
            "variable GPOS must contribute kerning at {location}"
        );
        let ligature = shaping.shape("AV", false).unwrap();
        assert_eq!(ligature.len(), 1);
        assert_eq!(shaping.glyph_name(ligature[0].glyph_id), Some("AV"));
    }
    project
        .edit_source(SourceId(1))
        .unwrap()
        .font
        .default_layer_mut()
        .get_glyph_mut("A")
        .unwrap()
        .width = 1000.0;
    let edited = project.compile().unwrap();
    let shaped = ShapingFont::from_bytes((*edited.bytes).clone())
        .unwrap()
        .at_normalized(vec![1.0])
        .shape("A", false)
        .unwrap();
    assert_eq!(
        shaped[0].x_advance, 1000.0,
        "compile must see unsaved source edits"
    );
    assert_ne!(edited.bytes, compiled.bytes);
}

#[test]
fn compiler_quantizes_exact_editable_metrics_only_in_its_snapshot() {
    let mut project = project();
    {
        let mut source = project.edit_source(SourceId(0)).unwrap();
        source
            .font
            .default_layer_mut()
            .get_glyph_mut("A")
            .unwrap()
            .width = 500.6;
        *source
            .font
            .kerning
            .get_mut(&Name::new("A").unwrap())
            .unwrap()
            .get_mut(&Name::new("V").unwrap())
            .unwrap() = -50.5;
    }
    let exact = project.source_snapshot(SourceId(0)).unwrap();
    assert_eq!(
        exact.get_glyph("A").unwrap().width,
        500.6,
        "editable advance must remain exact"
    );
    assert_eq!(
        exact.kerning[&Name::new("A").unwrap()][&Name::new("V").unwrap()],
        -50.5,
        "editable kerning must remain exact"
    );

    let compiled = project.compile().unwrap();
    let shaped = ShapingFont::from_bytes((*compiled.bytes).clone())
        .unwrap()
        .at_normalized(vec![0.0])
        .shape_with_features("AV", false, &[("liga".into(), false)])
        .unwrap();
    assert_eq!(
        shaped[0].x_advance, 450.0,
        "compiled advance and kerning must use rounded OpenType values"
    );
    let still_exact = project.source_snapshot(SourceId(0)).unwrap();
    assert_eq!(
        still_exact.get_glyph("A").unwrap().width,
        500.6,
        "compilation must not rewrite the editable advance"
    );
    assert_eq!(
        still_exact.kerning[&Name::new("A").unwrap()][&Name::new("V").unwrap()],
        -50.5,
        "compilation must not rewrite editable kerning"
    );
}

#[test]
fn canonical_layer_transaction_invalidates_compiled_preview() {
    let mut project = project();
    let initial = project.compiled_preview().unwrap();
    let revision = project.document_revision();
    let layer = LayerId {
        source: SourceId(0),
        name: "public.default".into(),
    };
    let outcome = project
        .edit_document_layer("A", &layer, |draft| {
            assert!(draft.set_width(720.4)?, "advance did not change");
            Ok(())
        })
        .unwrap();
    let DocumentEditOutcome::Changed {
        revision: changed_revision,
        change,
    } = outcome
    else {
        panic!("canonical edit reported no change");
    };
    assert_eq!(
        changed_revision,
        revision + 1,
        "canonical edit reported the wrong revision"
    );
    assert!(
        change.requires_compilation(),
        "advance edit did not invalidate compilation"
    );
    assert_eq!(
        project.document_layer("A", &layer).unwrap().width(),
        720.4,
        "canonical advance lost precision"
    );

    let edited = project.compiled_preview().unwrap();
    assert!(
        !std::sync::Arc::ptr_eq(&initial, &edited),
        "canonical edit reused a stale compiled preview"
    );
    let shaped = ShapingFont::from_bytes((*edited.bytes).clone())
        .unwrap()
        .at_normalized(vec![0.0])
        .shape("A", false)
        .unwrap();
    assert_eq!(
        shaped[0].x_advance, 720.0,
        "compiled preview missed the canonical advance edit"
    );
}

#[test]
fn text_buffer_applies_variable_kerning_once_and_reuses_compilation_across_locations() {
    use runebender::text::buffer::{TextBuffer, TextGlyphInventory, TextKerningModel};
    let mut project = project();
    let compiled = project.compiled_preview().unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &compiled,
        &project.compiled_preview().unwrap()
    ));
    project.location.insert("Weight".into(), 1.0);
    assert!(
        std::sync::Arc::ptr_eq(&compiled, &project.compiled_preview().unwrap()),
        "slider-only location changes must reuse immutable compiled inputs"
    );
    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(TextGlyphInventory::from_font(&project.sources()[0].font));
    buffer.set_kerning_model(TextKerningModel::from_font(&project.sources()[0].font));
    buffer.set_feature_overrides(vec![("liga".into(), false)]);
    buffer.set_compiled_font(Some(compiled.bytes.clone()), vec![1.0]);
    buffer.insert_character('A');
    buffer.insert_character('V');
    let layout = buffer.layout(1000.0);
    assert_eq!(
        layout.items[1].x, 650.0,
        "GPOS must not receive an additional UFO kerning adjustment"
    );
    buffer.set_compiled_font(Some(compiled.bytes.clone()), vec![0.0]);
    buffer.refresh_shaping();
    assert_eq!(buffer.layout(1000.0).items[1].x, 450.0);
}

#[test]
fn designspace_rules_are_present_in_the_compiled_variable_shaper() {
    let weight_rule = |minimum| norad::designspace::Rule {
        name: Some("weight alternate".into()),
        condition_sets: vec![norad::designspace::ConditionSet {
            conditions: vec![norad::designspace::Condition {
                name: "Weight".into(),
                minimum: Some(minimum),
                maximum: None,
            }],
        }],
        substitutions: vec![norad::designspace::Substitution {
            name: Name::new("A").unwrap(),
            with: Name::new("V").unwrap(),
        }],
    };
    let overlap_rule = || {
        let mut rule = weight_rule(800.0);
        rule.substitutions[0].name = Name::new("V").unwrap();
        rule.substitutions[0].with = Name::new("AV").unwrap();
        rule
    };

    let mut doc = designspace();
    doc.rules.rules.push(weight_rule(650.0));
    let mut project = project_from_designspace(doc);
    project.axes.clear();
    project.master_locations.clear();
    project.master_names.clear();
    project.instances.clear();
    project.brace.clear();
    project.ds_doc.as_mut().unwrap().rules.rules.clear();
    let compiled = project.compile().unwrap();
    for (location, name) in [(0.0, "A"), (1.0, "V")] {
        let shaping = ShapingFont::from_bytes((*compiled.bytes).clone())
            .unwrap()
            .at_normalized(vec![location]);
        let shaped = shaping.shape("A", false).unwrap();
        assert_eq!(shaping.glyph_name(shaped[0].glyph_id), Some(name));
    }

    let mut doc = designspace();
    doc.rules.rules.push(weight_rule(650.0));
    doc.rules.rules.push(overlap_rule());
    let project = project_from_designspace(doc);
    let compiled = project.compile().unwrap();
    for (location, name) in [(0.0, "A"), (0.6, "V"), (0.9, "AV")] {
        let shaping = ShapingFont::from_bytes((*compiled.bytes).clone())
            .unwrap()
            .at_normalized(vec![location]);
        let shaped = shaping.shape("A", false).unwrap();
        assert_eq!(shaping.glyph_name(shaped[0].glyph_id), Some(name));
    }

    let mut doc = designspace();
    doc.rules.rules.push(weight_rule(650.25));
    doc.rules.rules.push(overlap_rule());
    let project = project_from_designspace(doc);
    let compiled = project.compile().unwrap();
    for (location, name) in [(0.5, "A"), (0.501, "V"), (0.9, "AV")] {
        let shaping = ShapingFont::from_bytes((*compiled.bytes).clone())
            .unwrap()
            .at_normalized(vec![location]);
        let shaped = shaping.shape("A", false).unwrap();
        assert_eq!(shaping.glyph_name(shaped[0].glyph_id), Some(name));
    }
}

#[test]
fn canonical_source_metadata_transaction_is_atomic_and_invalidates_compile() {
    let mut project = project();
    let source = SourceId(0);
    let original = project.document_feature_text(source).unwrap().to_owned();
    let revision = project.document_revision();
    let initial = project.compiled_preview().unwrap();

    let unchanged = project
        .edit_document_source_metadata(source, |draft| {
            assert!(
                !draft.set_feature_text(original.clone()),
                "equal feature text changed"
            );
            Ok(())
        })
        .unwrap();
    assert_eq!(
        unchanged,
        DocumentEditOutcome::Unchanged { revision },
        "no-op metadata draft committed"
    );

    let rejected = project
        .edit_document_source_metadata(source, |draft| {
            assert!(
                draft.set_feature_text("feature liga { sub A by V; } liga;".into()),
                "rejected draft did not change"
            );
            Err(DocumentEditError::Rejected)
        })
        .unwrap_err();
    assert_eq!(
        rejected,
        DocumentEditError::Rejected,
        "metadata draft returned the wrong error"
    );
    assert_eq!(
        project.document_feature_text(source),
        Some(original.as_str()),
        "rejected metadata draft leaked its value"
    );
    assert_eq!(
        project.document_revision(),
        revision,
        "rejected metadata draft advanced the revision"
    );

    let feature_text =
        "conditionset Heavy { wght 650.25 900; } Heavy; variation rvrn Heavy { sub A by V; } rvrn;";
    let changed = project
        .edit_document_source_metadata(source, |draft| {
            assert!(
                draft.set_feature_text(feature_text.into()),
                "feature text did not change"
            );
            Ok(())
        })
        .unwrap();
    let DocumentEditOutcome::Changed {
        revision: changed_revision,
        change,
    } = changed
    else {
        panic!("metadata edit reported no change");
    };
    assert_eq!(
        changed_revision,
        revision + 1,
        "metadata edit reported the wrong revision"
    );
    assert!(
        change.affected_layers().is_empty() && change.dependent_layers().is_empty(),
        "source metadata edit reported layer changes"
    );
    assert_eq!(
        change.source_metadata(),
        &[source],
        "source metadata invalidation is wrong"
    );
    assert!(
        change.metadata_changed(),
        "metadata change was not reported"
    );
    assert!(
        !change.geometry_changed() && !change.metrics_changed(),
        "metadata edit reported geometry or metrics"
    );
    assert!(
        change.requires_compilation(),
        "feature edit did not invalidate compilation"
    );
    assert_eq!(
        project.document_feature_text(source),
        Some(feature_text),
        "canonical feature text was not committed"
    );
    assert_eq!(
        project.sources()[0].font.features,
        feature_text,
        "compatibility projection was not refreshed"
    );
    assert_eq!(
        project.source_snapshot(source).unwrap().features,
        feature_text,
        "format projection missed canonical feature text"
    );
    let edited = project.compiled_preview().unwrap();
    assert!(
        !std::sync::Arc::ptr_eq(&initial, &edited),
        "metadata edit reused a stale compiled preview"
    );
}

#[test]
fn source_structure_history_invalidates_compiled_preview() {
    let mut project = project();
    let initial = project.compiled_preview().unwrap();
    let original_order: Vec<_> = project
        .compiler_structure()
        .unwrap()
        .sources
        .iter()
        .map(|source| source.id())
        .collect();
    assert_eq!(original_order, vec![SourceId(0), SourceId(1)]);

    let location = std::collections::HashMap::from([("Weight".into(), 0.5)]);
    let added = project
        .add_interpolated_source("Medium", "M09Medium.ufo", &location)
        .unwrap();
    let after_add = project.compiled_preview().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&initial, &after_add));
    assert_eq!(
        project
            .compiler_structure()
            .unwrap()
            .sources
            .iter()
            .map(|source| source.id())
            .collect::<Vec<_>>(),
        vec![SourceId(0), SourceId(1), added]
    );

    assert!(project.move_source(added, 0).unwrap());
    let after_move = project.compiled_preview().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&after_add, &after_move));
    assert_eq!(
        project
            .compiler_structure()
            .unwrap()
            .sources
            .iter()
            .map(|source| source.id())
            .collect::<Vec<_>>(),
        vec![added, SourceId(0), SourceId(1)]
    );

    assert!(project.undo_sources(false).unwrap());
    let after_move_undo = project.compiled_preview().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&after_move, &after_move_undo));
    assert_eq!(
        project
            .compiler_structure()
            .unwrap()
            .sources
            .iter()
            .map(|source| source.id())
            .collect::<Vec<_>>(),
        vec![SourceId(0), SourceId(1), added]
    );

    assert!(project.undo_sources(false).unwrap());
    let after_add_undo = project.compiled_preview().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&after_move_undo, &after_add_undo));
    assert_eq!(
        project
            .compiler_structure()
            .unwrap()
            .sources
            .iter()
            .map(|source| source.id())
            .collect::<Vec<_>>(),
        original_order
    );
    assert_eq!(after_add_undo.bytes, initial.bytes);

    assert!(project.undo_sources(true).unwrap());
    let after_add_redo = project.compiled_preview().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&after_add_undo, &after_add_redo));
    assert!(project.undo_sources(true).unwrap());
    let after_move_redo = project.compiled_preview().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&after_add_redo, &after_move_redo));
    assert_eq!(
        project
            .compiler_structure()
            .unwrap()
            .sources
            .iter()
            .map(|source| source.id())
            .collect::<Vec<_>>(),
        vec![added, SourceId(0), SourceId(1)]
    );
}

#[test]
fn shared_feature_edits_and_variable_drafts_do_not_depend_on_selected_master() {
    let mut project = project();
    let other_features = project.sources()[1].font.features.clone();
    project.active = 1;
    let draft =
        "conditionset Heavy { wght 650.25 900; } Heavy; variation rvrn Heavy { sub A by V; } rvrn;";
    project.check_features(draft).unwrap();
    assert_eq!(
        project.sources()[0].font.features,
        other_features,
        "checking must not apply a draft"
    );
    assert!(project.set_feature_text(draft.into()));
    assert_eq!(project.document_feature_text(SourceId(0)), Some(draft));
    assert_eq!(
        project.sources()[1].font.features,
        other_features,
        "other UFO feature files are preserved"
    );
    let compiled = project.compile().unwrap();
    let shaper = ShapingFont::from_bytes((*compiled.bytes).clone())
        .unwrap()
        .at_normalized(vec![1.0]);
    let shaped = shaper.shape("A", false).unwrap();
    assert_eq!(shaper.glyph_name(shaped[0].glyph_id), Some("V"));
}

#[test]
fn mark_positioning_tracks_live_anchors_in_both_masters() {
    let mut project = project();
    for (index, source) in project.edit_sources().iter_mut().enumerate() {
        let base = source.font.default_layer_mut().get_glyph_mut("A").unwrap();
        base.anchors.push(norad::Anchor::new(
            250.0 + index as f64 * 150.0,
            700.0 + index as f64 * 200.0,
            Some(Name::new("top").unwrap()),
            None,
            None,
        ));
        let mut mark = Glyph::new("acutecomb");
        mark.codepoints.insert('\u{301}');
        mark.anchors.push(norad::Anchor::new(
            100.0,
            0.0,
            Some(Name::new("_top").unwrap()),
            None,
            None,
        ));
        source.font.default_layer_mut().insert_glyph(mark);
    }
    let compiled = project.compile().unwrap();
    for (location, y) in [(0.0, 700.0), (0.5, 800.0), (1.0, 900.0)] {
        let shaping = ShapingFont::from_bytes((*compiled.bytes).clone())
            .unwrap()
            .at_normalized(vec![location]);
        let shaped = shaping.shape("A\u{301}", false).unwrap();
        assert_eq!(shaped.len(), 2);
        assert_eq!(shaped[1].y_offset, y);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn background_preview_coalesces_edits_and_never_publishes_a_stale_revision() {
    let mut project = project();
    assert!(project.request_preview().unwrap().is_none());
    for width in [850.0, 900.0, 1050.0] {
        project
            .edit_source(SourceId(1))
            .unwrap()
            .font
            .default_layer_mut()
            .get_glyph_mut("A")
            .unwrap()
            .width = width;
        assert!(project.request_preview().unwrap().is_none());
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let compiled = loop {
        if let Some(compiled) = project.request_preview().unwrap() {
            break compiled;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "preview worker must complete"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let shaped = ShapingFont::from_bytes((*compiled.bytes).clone())
        .unwrap()
        .at_normalized(vec![1.0])
        .shape("A", false)
        .unwrap();
    assert_eq!(shaped[0].x_advance, 1050.0);
    assert!(project.preview_job().is_none());
}
