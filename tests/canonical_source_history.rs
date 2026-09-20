// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical history behavior across source and layer structural transactions.

use std::path::PathBuf;

use norad::{Contour, ContourPoint, Font, Glyph, PointType};
use runebender::document::font_memory::designspace_from_str;
use runebender::document::history::HistoryDirection;
use runebender::document::model::designspace::SourceOrderEntry;
use runebender::document::project::{
    DocumentEditOutcome, DocumentHistoryReplayOutcome, Project, SourceInput,
};
use runebender::document::variable::{GlyphLayerAddress, LayerId, SourceId};

const DESIGNSPACE: &str = include_str!("fixtures/variable/TwoAxes.designspace");

fn glyph(name: &str, x: f64) -> Glyph {
    let mut glyph = Glyph::new(name);
    glyph.width = 500.125 + x;
    glyph.contours.push(Contour::new(
        vec![ContourPoint::new(
            x,
            20.0,
            PointType::Line,
            false,
            None,
            None,
        )],
        None,
    ));
    glyph
}

fn fixture() -> Project {
    let doc = designspace_from_str(DESIGNSPACE).unwrap();
    project_from_designspace(doc)
}

fn interleaved_fixture() -> Project {
    let mut doc = designspace_from_str(DESIGNSPACE).unwrap();
    let sparse = doc
        .sources
        .iter()
        .position(|source| source.layer.is_some())
        .map(|index| doc.sources.remove(index))
        .unwrap();
    doc.sources.insert(1, sparse);
    project_from_designspace(doc)
}

fn project_from_designspace(doc: norad::designspace::DesignSpaceDocument) -> Project {
    Project::from_designspace(doc, |filename| {
        let x = match filename {
            "Regular.ufo" => 0.0,
            "Heavy.ufo" => 100.0,
            "Wide.ufo" => 50.0,
            _ => 200.0,
        };
        let mut font = Font::new();
        font.font_info.style_name = Some(filename.trim_end_matches(".ufo").into());
        font.default_layer_mut().insert_glyph(glyph("A", x));
        if filename == "Regular.ufo" {
            font.layers
                .new_layer("intermediate")
                .unwrap()
                .insert_glyph(glyph("A", 80.0));
        }
        Ok(SourceInput::from_font(font, PathBuf::from(filename)))
    })
    .unwrap()
}

fn address(project: &Project) -> GlyphLayerAddress {
    GlyphLayerAddress {
        glyph: "A".into(),
        layer: project
            .document_source(SourceId(1))
            .unwrap()
            .default_layer(),
    }
}

fn set_width(project: &mut Project, address: &GlyphLayerAddress, width: f64) {
    let mut transaction = project.begin_document_layer_transaction(address).unwrap();
    transaction.draft_mut().set_width(width).unwrap();
    project
        .commit_document_layer_transaction(transaction)
        .unwrap();
}

#[test]
fn removed_source_roundtrip_preserves_both_existing_layer_history_piles() {
    let mut project = fixture();
    let address = address(&project);
    let initial = project.capture_document_layer(&address).unwrap();
    set_width(&mut project, &address, 600.375);
    let first = project.capture_document_layer(&address).unwrap();
    set_width(&mut project, &address, 700.875);
    let second = project.capture_document_layer(&address).unwrap();
    project
        .replay_document_layer_history(&address, HistoryDirection::Undo)
        .unwrap();

    project.remove_source(SourceId(1)).unwrap();
    assert!(project.undo_sources(false).unwrap());
    assert_eq!(
        project.capture_document_layer(&address),
        Some(first.clone())
    );
    project
        .replay_document_layer_history(&address, HistoryDirection::Redo)
        .unwrap();
    assert_eq!(
        project.capture_document_layer(&address),
        Some(second.clone())
    );

    assert!(project.undo_sources(true).is_err());
    assert!(project.has_source_history(true));
    project
        .replay_document_layer_history(&address, HistoryDirection::Undo)
        .unwrap();
    assert!(project.undo_sources(true).unwrap());
    assert!(project.document_source(SourceId(1)).is_none());
    assert!(project.undo_sources(false).unwrap());

    project
        .replay_document_layer_history(&address, HistoryDirection::Undo)
        .unwrap();
    assert_eq!(project.capture_document_layer(&address), Some(initial));
    project
        .replay_document_layer_history(&address, HistoryDirection::Redo)
        .unwrap();
    assert_eq!(project.capture_document_layer(&address), Some(first));
    project
        .replay_document_layer_history(&address, HistoryDirection::Redo)
        .unwrap();
    assert_eq!(project.capture_document_layer(&address), Some(second));
}

#[test]
fn descriptor_replay_keeps_a_pending_layer_transaction_and_rejected_redo() {
    let mut project = fixture();
    let address = address(&project);
    let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
    transaction.draft_mut().set_width(750.625).unwrap();
    let source = SourceId(1);
    let original = project.document_source(source).unwrap();
    let original_name = original.name().to_owned();
    let target = [("Weight".into(), 0.75), ("Width".into(), 0.0)].into();

    project
        .update_source(source, &original_name, &target)
        .unwrap();
    let edited_revision = project.document_revision();
    assert!(project.undo_sources(false).unwrap());
    assert_eq!(project.document_revision(), edited_revision.wrapping_add(1));
    project
        .commit_document_layer_transaction(transaction)
        .unwrap();
    let committed = project.capture_document_layer(&address).unwrap();
    let revision = project.document_revision();

    assert!(project.undo_sources(true).is_err());
    assert_eq!(project.document_revision(), revision);
    assert_eq!(project.capture_document_layer(&address), Some(committed));
    assert!(project.has_source_history(true));
    project
        .replay_document_layer_history(&address, HistoryDirection::Undo)
        .unwrap();
    let revision = project.document_revision();
    assert!(project.undo_sources(true).unwrap());
    assert_eq!(project.document_revision(), revision.wrapping_add(1));
    assert_eq!(project.document_source(source).unwrap().location(), &target);
}

#[test]
fn removed_source_roundtrip_preserves_auxiliary_compatibility_history() {
    let mut project = fixture();
    let source = SourceId(1);
    let default = project.document_source(source).unwrap().default_layer();
    let auxiliary = project.add_glyph_layer("A", &default, "backup").unwrap();
    let initial_width = project.encode_ufo_layer("A", &auxiliary).unwrap().width;
    let address = GlyphLayerAddress {
        glyph: "A".into(),
        layer: auxiliary.clone(),
    };
    let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
    transaction.draft_mut().set_width(750.625).unwrap();
    assert!(matches!(
        project.commit_document_layer_transaction(transaction),
        Ok(DocumentEditOutcome::Changed { .. })
    ));

    project.remove_source(source).unwrap();
    assert!(project.undo_sources(false).unwrap());
    assert!(matches!(
        project.replay_document_layer_history(&address, HistoryDirection::Undo),
        Ok(DocumentHistoryReplayOutcome::Changed { .. })
    ));
    assert_eq!(
        project.encode_ufo_layer("A", &auxiliary).unwrap().width,
        initial_width
    );
    assert!(matches!(
        project.replay_document_layer_history(&address, HistoryDirection::Redo),
        Ok(DocumentHistoryReplayOutcome::Changed { .. })
    ));
    assert_eq!(
        project.encode_ufo_layer("A", &auxiliary).unwrap().width,
        750.625
    );
}

#[test]
fn source_move_and_history_preserve_full_sparse_serialized_interleaving() {
    let mut project = interleaved_fixture();
    let sparse = LayerId {
        source: SourceId(0),
        name: "intermediate".into(),
    };
    let original = vec![
        SourceOrderEntry::Full(SourceId(0)),
        SourceOrderEntry::Sparse(sparse.clone()),
        SourceOrderEntry::Full(SourceId(1)),
        SourceOrderEntry::Full(SourceId(2)),
        SourceOrderEntry::Full(SourceId(3)),
    ];
    let moved = vec![
        SourceOrderEntry::Full(SourceId(1)),
        SourceOrderEntry::Sparse(sparse),
        SourceOrderEntry::Full(SourceId(0)),
        SourceOrderEntry::Full(SourceId(2)),
        SourceOrderEntry::Full(SourceId(3)),
    ];
    assert_eq!(
        project.document_designspace().unwrap().source_order(),
        original
    );

    assert!(project.move_source(SourceId(1), 0).unwrap());
    assert_eq!(
        project.document_designspace().unwrap().source_order(),
        moved
    );
    let serialized_document = project.document_designspace().unwrap().to_norad().unwrap();
    let serialized = serialized_document
        .sources
        .iter()
        .map(|source| (source.filename.as_str(), source.layer.as_deref()))
        .collect::<Vec<_>>();
    assert_eq!(
        serialized,
        vec![
            ("Heavy.ufo", None),
            ("Regular.ufo", Some("intermediate")),
            ("Regular.ufo", None),
            ("Wide.ufo", None),
            ("HeavyWide.ufo", None),
        ]
    );

    assert!(project.undo_sources(false).unwrap());
    assert_eq!(
        project.document_designspace().unwrap().source_order(),
        original
    );
    assert!(project.undo_sources(true).unwrap());
    assert_eq!(
        project.document_designspace().unwrap().source_order(),
        moved
    );
}
