// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Regression coverage for canonical group, kerning and glyph metadata values.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use runebender::document::font_ops::{
    CanonicalFontMetadata, CanonicalMetadataError, KerningParticipant, KerningSide,
};
use runebender::document::model::glyph_metadata::{
    CanonicalGlyphMetadata, GlyphMetadataError, OpenTypeGlyphCategory, parse_codepoints,
};
use runebender::document::project::{DocumentEditOutcome, Project};
use runebender::document::variable::SourceId;

static SCRATCH_ID: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let id = SCRATCH_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "runebender-canonical-metadata-{}-{id}",
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

fn raw_metadata() -> CanonicalFontMetadata {
    CanonicalFontMetadata::from_raw(
        BTreeMap::from([
            (
                "com.example.arbitrary".into(),
                vec!["A".into(), "A".into(), "V".into()],
            ),
            ("public.kern1.A".into(), vec!["A".into()]),
            ("public.kern2.V".into(), vec!["V".into()]),
        ]),
        BTreeMap::from([
            (
                "A".into(),
                BTreeMap::from([("V".into(), -81.375), ("public.kern2.V".into(), -70.125)]),
            ),
            (
                "public.kern1.A".into(),
                BTreeMap::from([("V".into(), -60.625), ("public.kern2.V".into(), -50.5)]),
            ),
        ]),
    )
    .unwrap()
}

#[test]
fn fractional_kerning_and_all_groups_round_trip_exactly() {
    let metadata = raw_metadata();

    assert_eq!(metadata.resolved_kerning("A", "V"), Some(-81.375));
    assert_eq!(
        metadata.groups().get("com.example.arbitrary"),
        Some(&vec!["A".to_string(), "A".to_string(), "V".to_string()])
    );
    assert_eq!(
        metadata
            .raw_kerning()
            .get("public.kern1.A")
            .and_then(|row| row.get("public.kern2.V")),
        Some(&-50.5)
    );
}

#[test]
fn kerning_resolution_uses_ufo_precedence() {
    let mut metadata = raw_metadata();
    let glyph_a = KerningParticipant::glyph("A").unwrap();
    let glyph_v = KerningParticipant::glyph("V").unwrap();

    assert!(
        metadata
            .set_kerning_pair(glyph_a.clone(), glyph_v.clone(), None)
            .unwrap()
    );
    assert_eq!(metadata.resolved_kerning("A", "V"), Some(-70.125));
    assert!(
        metadata
            .set_kerning_pair(
                glyph_a,
                KerningParticipant::group(KerningSide::Second, "V").unwrap(),
                None,
            )
            .unwrap()
    );
    assert_eq!(metadata.resolved_kerning("A", "V"), Some(-60.625));
    assert!(
        metadata
            .set_kerning_pair(
                KerningParticipant::group(KerningSide::First, "A").unwrap(),
                glyph_v,
                None,
            )
            .unwrap()
    );
    assert_eq!(metadata.resolved_kerning("A", "V"), Some(-50.5));
}

#[test]
fn group_membership_is_side_scoped_and_accepts_full_names() {
    let mut metadata = raw_metadata();

    assert!(
        !metadata
            .set_kerning_group("A", KerningSide::First, Some("public.kern1.A"))
            .unwrap()
    );
    assert!(
        metadata
            .set_kerning_group("A", KerningSide::First, Some("Round"))
            .unwrap()
    );
    assert_eq!(
        metadata.kerning_group("A", KerningSide::First),
        Some("public.kern1.Round")
    );
    assert_eq!(
        metadata.groups().get("com.example.arbitrary"),
        Some(&vec!["A".to_string(), "A".to_string(), "V".to_string()])
    );
    assert_eq!(
        metadata.set_kerning_group("A", KerningSide::First, Some("public.kern2.wrong")),
        Err(CanonicalMetadataError::WrongGroupSide {
            name: "public.kern2.wrong".into(),
            expected: KerningSide::First,
        })
    );
    assert_eq!(
        metadata.kerning_group("A", KerningSide::First),
        Some("public.kern1.Round")
    );
}

#[test]
fn whole_kerning_group_edits_reject_ambiguous_membership_atomically() {
    let mut metadata = raw_metadata();
    let before = metadata.clone();

    assert_eq!(
        metadata.set_group("public.kern1.Other", vec!["A".into(), "A".into()]),
        Err(CanonicalMetadataError::AmbiguousKerningMembership {
            glyph: "A".into(),
            side: KerningSide::First,
            first_group: "public.kern1.A".into(),
            second_group: "public.kern1.Other".into(),
        })
    );
    assert_eq!(metadata, before);
    assert!(
        metadata
            .set_group("com.example.duplicates", vec!["A".into(), "A".into()])
            .unwrap()
    );
    assert_eq!(
        metadata.groups().get("com.example.duplicates"),
        Some(&vec!["A".to_string(), "A".to_string()])
    );
}

#[test]
fn nonfinite_pair_is_rejected_without_mutation() {
    let mut metadata = raw_metadata();
    let before = metadata.clone();
    let left = KerningParticipant::glyph("A").unwrap();
    let right = KerningParticipant::glyph("W").unwrap();

    assert!(matches!(
        metadata.set_kerning_pair(left, right, Some(f64::NAN)),
        Err(CanonicalMetadataError::NonFiniteKerning { .. })
    ));
    assert_eq!(metadata, before);
}

#[test]
fn rename_updates_groups_and_both_pair_sides_atomically() {
    let mut metadata = CanonicalFontMetadata::from_raw(
        BTreeMap::from([
            ("com.example.arbitrary".into(), vec!["A".into()]),
            ("public.kern1.A".into(), vec!["A".into()]),
        ]),
        BTreeMap::from([
            ("A".into(), BTreeMap::from([("V".into(), -20.25)])),
            ("T".into(), BTreeMap::from([("A".into(), -10.75)])),
        ]),
    )
    .unwrap();

    assert!(metadata.rename_glyph_references("A", "A.alt").unwrap());
    assert_eq!(
        metadata.groups().get("com.example.arbitrary"),
        Some(&vec!["A.alt".to_string()])
    );
    assert_eq!(
        metadata
            .raw_kerning()
            .get("A.alt")
            .and_then(|row| row.get("V")),
        Some(&-20.25)
    );
    assert_eq!(
        metadata
            .raw_kerning()
            .get("T")
            .and_then(|row| row.get("A.alt")),
        Some(&-10.75)
    );

    let before = metadata.clone();
    assert_eq!(
        metadata.rename_glyph_references("A.alt", "T"),
        Err(CanonicalMetadataError::RenameCollision("T".into()))
    );
    assert_eq!(metadata, before);
}

#[test]
fn removing_a_group_removes_only_its_pairs() {
    let mut metadata = raw_metadata();

    assert!(metadata.remove_group("public.kern2.V").unwrap());
    assert!(!metadata.groups().contains_key("public.kern2.V"));
    assert_eq!(metadata.resolved_kerning("A", "V"), Some(-81.375));
    assert_eq!(
        metadata.groups().get("com.example.arbitrary"),
        Some(&vec!["A".to_string(), "A".to_string(), "V".to_string()])
    );
    assert!(
        metadata
            .kerning_pairs()
            .all(|(_, right, _)| right.as_raw_name() != "public.kern2.V")
    );
}

#[test]
fn group_rename_updates_pair_references_and_rejects_kind_changes() {
    let mut metadata = raw_metadata();

    assert!(
        metadata
            .rename_group("public.kern1.A", "public.kern1.Round")
            .unwrap()
    );
    assert!(!metadata.groups().contains_key("public.kern1.A"));
    assert_eq!(
        metadata.groups().get("public.kern1.Round"),
        Some(&vec!["A".to_string()])
    );
    assert_eq!(
        metadata
            .raw_kerning()
            .get("public.kern1.Round")
            .and_then(|row| row.get("public.kern2.V")),
        Some(&-50.5)
    );

    let before = metadata.clone();
    assert_eq!(
        metadata.rename_group("public.kern1.Round", "com.example.renamed"),
        Err(CanonicalMetadataError::IncompatibleGroupRename {
            old: "public.kern1.Round".into(),
            new: "com.example.renamed".into(),
        })
    );
    assert_eq!(metadata, before);
}

#[test]
fn glyph_metadata_preserves_exact_values_and_rejects_bad_unicode() {
    let codepoints = parse_codepoints("U+0041, 0x0391 0041").unwrap();
    let mut metadata = CanonicalGlyphMetadata::new(
        codepoints,
        Some(String::new()),
        false,
        Some(OpenTypeGlyphCategory::from_source("future-category")),
    );

    assert_eq!(metadata.codepoints(), ['A', '\u{391}']);
    assert_eq!(metadata.note(), Some(""));
    assert!(!metadata.exported());
    assert_eq!(
        metadata.category().map(OpenTypeGlyphCategory::as_source),
        Some("future-category")
    );
    assert_eq!(
        parse_codepoints("0041 D800"),
        Err(GlyphMetadataError::InvalidCodepoint("D800".into()))
    );
    assert!(!metadata.set_codepoints(['A', '\u{391}']));
    assert!(metadata.set_note(None));
    assert!(!metadata.set_note(None));
}

#[test]
fn project_source_metadata_is_atomic_and_survives_save_reload() {
    let scratch = Scratch::new();
    let path = scratch.0.join("Metadata.ufo");
    let mut font = norad::Font::new();
    font.default_layer_mut()
        .insert_glyph(norad::Glyph::new("A"));
    font.default_layer_mut()
        .insert_glyph(norad::Glyph::new("V"));
    font.groups.insert(
        norad::Name::new("com.example.arbitrary").unwrap(),
        vec![
            norad::Name::new("A").unwrap(),
            norad::Name::new("A").unwrap(),
        ],
    );
    font.kerning.insert(
        norad::Name::new("A").unwrap(),
        BTreeMap::from([(norad::Name::new("V").unwrap(), -81.375)]),
    );
    font.save(&path).unwrap();

    let mut project = Project::load(&path).unwrap();
    let source = SourceId(0);
    let mut edited = project.document_font_metadata(source).unwrap().clone();
    assert_eq!(edited.resolved_kerning("A", "V"), Some(-81.375));
    assert_eq!(
        edited.groups().get("com.example.arbitrary"),
        Some(&vec!["A".to_string(), "A".to_string()])
    );
    assert!(
        edited
            .set_kerning_pair(
                KerningParticipant::glyph("A").unwrap(),
                KerningParticipant::glyph("V").unwrap(),
                Some(-63.625),
            )
            .unwrap()
    );
    assert!(
        edited
            .set_group("com.example.extra", vec!["V".into()])
            .unwrap()
    );

    let revision = project.document_revision();
    let outcome = project
        .edit_document_source_metadata(source, |draft| {
            assert!(draft.set_font_metadata(edited.clone()));
            Ok(())
        })
        .unwrap();
    let DocumentEditOutcome::Changed {
        revision: changed_revision,
        change,
    } = outcome
    else {
        panic!("canonical source edit reported no change");
    };
    assert_eq!(changed_revision, revision.wrapping_add(1));
    assert_eq!(change.source_metadata(), &[source]);
    assert!(change.metadata_changed());
    assert!(change.requires_compilation());
    assert!(!change.geometry_changed());
    assert!(!change.metrics_changed());
    assert_eq!(project.document_font_metadata(source), Some(&edited));

    let unchanged_revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_source_metadata(source, |draft| {
                assert!(!draft.set_font_metadata(edited.clone()));
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged {
            revision: unchanged_revision,
        }
    );
    let mut rejected = edited.clone();
    rejected
        .set_kerning_pair(
            KerningParticipant::glyph("A").unwrap(),
            KerningParticipant::glyph("V").unwrap(),
            Some(-20.25),
        )
        .unwrap();
    assert_eq!(
        project.edit_document_source_metadata(source, |draft| {
            assert!(draft.set_font_metadata(rejected));
            Err(runebender::document::DocumentEditError::Rejected)
        }),
        Err(runebender::document::DocumentEditError::Rejected)
    );
    assert_eq!(project.document_revision(), unchanged_revision);
    assert_eq!(project.document_font_metadata(source), Some(&edited));

    project.save().unwrap();
    let reloaded = Project::load(&path).unwrap();
    assert_eq!(reloaded.document_font_metadata(source), Some(&edited));
    let saved = reloaded.source_snapshot(source).unwrap();
    assert_eq!(saved.kerning["A"]["V"], -63.625);
    assert_eq!(
        saved.groups["com.example.arbitrary"],
        [
            norad::Name::new("A").unwrap(),
            norad::Name::new("A").unwrap()
        ]
    );
}
