// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Regression coverage for exact canonical font-info values and their UFO boundary.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use runebender::document::history::HistoryDirection;
use runebender::document::model::font_info::{
    CanonicalFontInfo, CanonicalFontInfoError, OpenTypeWidthClass, clear_canonical_font_info_fields,
};
use runebender::document::project::{
    DocumentEditOutcome, DocumentHistoryReplayOutcome, DocumentSourceMetadataHistoryError, Master,
    Project,
};
use runebender::document::variable::SourceId;

static SCRATCH_ID: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let id = SCRATCH_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "runebender-canonical-font-info-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn populated_font_info() -> norad::FontInfo {
    let mut info = norad::FontInfo {
        family_name: Some("Canonical Family".into()),
        style_name: Some("Optical Regular".into()),
        copyright: Some("Copyright exact".into()),
        trademark: Some("Trademark exact".into()),
        open_type_name_designer: Some("Designer".into()),
        open_type_name_designer_url: Some("https://designer.invalid".into()),
        open_type_name_manufacturer: Some("Manufacturer".into()),
        open_type_name_manufacturer_url: Some("https://manufacturer.invalid".into()),
        open_type_name_description: Some("Description".into()),
        open_type_name_license: Some("License".into()),
        open_type_name_license_url: Some("https://license.invalid".into()),
        open_type_name_version: Some("Version 1.234".into()),
        open_type_name_unique_id: Some("CanonicalFamily-Regular-1.234".into()),
        open_type_name_sample_text: Some("Hamburgefonts".into()),
        postscript_full_name: Some("Canonical Family Optical Regular".into()),
        postscript_font_name: Some("CanonicalFamily-OpticalRegular".into()),
        open_type_name_preferred_family_name: Some("Canonical Family Optical".into()),
        open_type_name_preferred_subfamily_name: Some("Regular".into()),
        open_type_name_wws_family_name: Some("Canonical Family".into()),
        open_type_name_wws_subfamily_name: Some("Regular".into()),
        units_per_em: Some(norad::fontinfo::NonNegativeIntegerOrFloat::try_from(1000.125).unwrap()),
        ascender: Some(812.75),
        descender: Some(-213.125),
        x_height: Some(523.625),
        cap_height: Some(712.875),
        italic_angle: Some(-11.25),
        open_type_hhea_ascender: Some(813),
        open_type_hhea_descender: Some(-213),
        open_type_hhea_line_gap: Some(17),
        open_type_hhea_caret_slope_rise: Some(1000),
        open_type_hhea_caret_slope_run: Some(195),
        open_type_hhea_caret_offset: Some(2),
        open_type_os2_typo_ascender: Some(800),
        open_type_os2_typo_descender: Some(-200),
        open_type_os2_typo_line_gap: Some(20),
        open_type_os2_subscript_x_size: Some(650),
        open_type_os2_subscript_y_size: Some(600),
        open_type_os2_subscript_x_offset: Some(10),
        open_type_os2_subscript_y_offset: Some(75),
        open_type_os2_superscript_x_size: Some(651),
        open_type_os2_superscript_y_size: Some(601),
        open_type_os2_superscript_x_offset: Some(11),
        open_type_os2_superscript_y_offset: Some(76),
        open_type_os2_strikeout_size: Some(51),
        open_type_os2_strikeout_position: Some(251),
        open_type_os2_win_ascent: Some(1024),
        open_type_os2_win_descent: Some(256),
        open_type_head_flags: Some(vec![0, 3, 15, 20]),
        open_type_os2_type: Some(vec![2, 3, 8]),
        open_type_os2_selection: Some(vec![7, 8, 9]),
        open_type_os2_weight_class: Some(450),
        open_type_os2_width_class: Some(norad::fontinfo::Os2WidthClass::SemiCondensed),
        open_type_os2_vendor_id: Some("TEST".into()),
        note: Some(String::new()),
        version_major: Some(1),
        version_minor: Some(234),
        ..Default::default()
    };
    info.open_type_name_compatible_full_name = Some("unowned compatible name".into());
    info.postscript_blue_scale = Some(0.039_625);
    info
}

#[test]
fn canonical_font_info_round_trips_exactly_and_clears_only_owned_fields() {
    let original = populated_font_info();
    let canonical = CanonicalFontInfo::from_ufo(&original).unwrap();
    assert_eq!(canonical.metrics.units_per_em, Some(1000.125));
    assert_eq!(canonical.metrics.ascender, Some(812.75));
    assert_eq!(canonical.metrics.descender, Some(-213.125));
    assert_eq!(
        canonical.open_type.width_class,
        Some(OpenTypeWidthClass::SemiCondensed)
    );
    assert_eq!(canonical.note.as_deref(), Some(""));

    let mut unchanged = original.clone();
    assert!(!canonical.write_to_ufo(&mut unchanged).unwrap());
    assert_eq!(unchanged, original);

    let mut template = original.clone();
    assert!(clear_canonical_font_info_fields(&mut template));
    assert_eq!(
        CanonicalFontInfo::from_ufo(&template).unwrap(),
        CanonicalFontInfo::default()
    );
    assert_eq!(
        template.open_type_name_compatible_full_name.as_deref(),
        Some("unowned compatible name")
    );
    assert_eq!(template.postscript_blue_scale, Some(0.039_625));

    assert!(canonical.write_to_ufo(&mut template).unwrap());
    assert_eq!(template, original);
}

#[test]
fn canonical_font_info_rejects_invalid_floats_before_mutation() {
    let mut canonical = CanonicalFontInfo::from_ufo(&populated_font_info()).unwrap();
    canonical.metrics.ascender = Some(f64::NAN);
    let mut target = populated_font_info();
    let before = target.clone();
    assert_eq!(
        canonical.write_to_ufo(&mut target),
        Err(CanonicalFontInfoError::NonFinite("ascender"))
    );
    assert_eq!(target, before);

    canonical.metrics.ascender = Some(800.0);
    canonical.metrics.units_per_em = Some(-1.0);
    assert_eq!(
        canonical.write_to_ufo(&mut target),
        Err(CanonicalFontInfoError::NegativeUnitsPerEm)
    );
    assert_eq!(target, before);
}

#[test]
fn editor_metric_defaults_are_resolved_without_becoming_stored_values() {
    let metrics = CanonicalFontInfo::default().metrics;
    assert_eq!(
        metrics.resolved(),
        runebender::document::model::font_info::ResolvedFontMetrics {
            units_per_em: 1000.0,
            ascender: 800.0,
            descender: -200.0,
            x_height: 500.0,
            cap_height: 700.0,
        }
    );
    assert_eq!(metrics.units_per_em, None);
    assert_eq!(metrics.ascender, None);
}

#[test]
fn project_owns_font_info_and_preserves_unowned_fields_through_save() {
    let scratch = Scratch::new();
    let path = scratch.0.join("FontInfo.ufo");
    let mut source = norad::Font::new();
    source.font_info = populated_font_info();
    source.save(&path).unwrap();

    let mut project = Project::load(&path).unwrap();
    let source_id = SourceId(0);
    let original = CanonicalFontInfo::from_ufo(&source.font_info).unwrap();
    assert_eq!(project.document_font_info(source_id), Some(&original));
    assert_eq!(
        project.source_snapshot(source_id).unwrap().font_info,
        source.font_info
    );

    let mut edited = original.clone();
    edited.names.family_name = Some("Canonical Edited Family".into());
    edited.metrics.units_per_em = Some(2_048.25);
    edited.open_type_metrics.win_ascent = Some(2_100);
    let revision = project.document_revision();
    let DocumentEditOutcome::Changed {
        revision: changed_revision,
        change,
    } = project
        .edit_document_source_metadata(source_id, |draft| {
            assert!(draft.set_font_info(edited.clone()));
            Ok(())
        })
        .unwrap()
    else {
        panic!("canonical font-info edit reported no change")
    };
    assert_eq!(changed_revision, revision.wrapping_add(1));
    assert_eq!(change.source_metadata(), &[source_id]);
    assert!(change.metadata_changed());
    assert!(change.metrics_changed());
    assert!(change.requires_compilation());
    assert_eq!(project.document_font_info(source_id), Some(&edited));

    let mut expected = source.font_info.clone();
    edited.write_to_ufo(&mut expected).unwrap();
    assert_eq!(
        project.source_snapshot(source_id).unwrap().font_info,
        expected
    );
    assert_eq!(
        expected.open_type_name_compatible_full_name.as_deref(),
        Some("unowned compatible name")
    );
    assert_eq!(expected.postscript_blue_scale, Some(0.039_625));

    let unchanged_revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_source_metadata(source_id, |draft| {
                assert!(!draft.set_font_info(edited.clone()));
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged {
            revision: unchanged_revision,
        }
    );

    project.save().unwrap();
    let reloaded = Project::load(&path).unwrap();
    assert_eq!(reloaded.document_font_info(source_id), Some(&edited));
    assert_eq!(
        reloaded.source_snapshot(source_id).unwrap().font_info,
        expected
    );
}

#[test]
fn project_rejects_invalid_font_info_without_changing_canonical_or_projected_state() {
    let mut source = norad::Font::new();
    source.font_info = populated_font_info();
    let mut project = Project::from_source(Master::from_font(
        source,
        PathBuf::from("InvalidFontInfo.ufo"),
    ));
    let source_id = SourceId(0);
    let canonical = project.document_font_info(source_id).unwrap().clone();
    let projection = project.source_snapshot(source_id).unwrap();
    let document = project.document_snapshot();
    let revision = project.document_revision();

    let mut negative_upm = canonical.clone();
    negative_upm.metrics.units_per_em = Some(-1.0);
    let mut infinite_ascender = canonical.clone();
    infinite_ascender.metrics.ascender = Some(f64::INFINITY);
    for invalid in [negative_upm, infinite_ascender] {
        assert_eq!(
            project.edit_document_source_metadata(source_id, |draft| {
                assert!(draft.set_font_info(invalid));
                Ok(())
            }),
            Err(runebender::document::DocumentEditError::InvalidFontInfo)
        );
        assert_eq!(project.document_font_info(source_id), Some(&canonical));
        assert_eq!(project.source_snapshot(source_id).unwrap(), projection);
        assert_eq!(project.document_snapshot(), document);
        assert_eq!(project.document_revision(), revision);
    }
}

#[test]
fn metric_metadata_restore_and_history_report_precise_invalidation() {
    let mut project = Project::from_source(Master::from_font(
        norad::Font::new(),
        PathBuf::from("MetricHistory.ufo"),
    ));
    let source = SourceId(0);
    let before = project.begin_document_source_metadata_history();
    let DocumentEditOutcome::Changed { change, .. } = project
        .edit_document_source_metadata(source, |draft| {
            let mut info = draft.font_info().clone();
            info.metrics.ascender = Some(913.625);
            assert!(draft.set_font_info(info));
            Ok(())
        })
        .unwrap()
    else {
        panic!("metric edit did not commit")
    };
    assert!(change.metrics_changed());
    assert!(project.record_document_source_metadata_history(before.clone()));
    let after = project.capture_document_source_metadata();

    let DocumentHistoryReplayOutcome::Changed { change, .. } = project
        .replay_document_source_metadata_history(HistoryDirection::Undo)
        .unwrap()
    else {
        panic!("metric undo did not replay")
    };
    assert!(change.metrics_changed());
    assert_eq!(
        project.document_font_info(source).unwrap().metrics.ascender,
        None
    );

    let DocumentHistoryReplayOutcome::Changed { change, .. } = project
        .replay_document_source_metadata_history(HistoryDirection::Redo)
        .unwrap()
    else {
        panic!("metric redo did not replay")
    };
    assert!(change.metrics_changed());
    assert_eq!(
        project.document_font_info(source).unwrap().metrics.ascender,
        Some(913.625)
    );

    let revision = project.document_revision();
    assert_eq!(
        project
            .restore_document_source_metadata_if_current(&after, after.clone())
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );

    let document = project.document_snapshot();
    assert_eq!(
        project.restore_document_source_metadata_if_current(&before, after),
        Err(DocumentSourceMetadataHistoryError::Stale)
    );
    assert_eq!(project.document_snapshot(), document);
    assert_eq!(project.document_revision(), revision);
}
