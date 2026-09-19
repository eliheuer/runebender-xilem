// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical history behavior across source and layer structural transactions.

use std::path::PathBuf;

use norad::{Contour, ContourPoint, Font, Glyph, PointType};
use runebender::document::font_memory::designspace_from_str;
use runebender::document::history::HistoryDirection;
use runebender::document::project::{DocumentHistoryReplayOutcome, Master, Project};
use runebender::document::variable::SourceId;

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
        Ok(Master::from_font(font, PathBuf::from(filename)))
    })
    .unwrap()
}

#[test]
fn restored_source_keeps_an_older_canonical_layer_undo_valid() {
    let mut project = fixture();
    let source = SourceId(1);
    let layer = project.document_source(source).unwrap().default_layer();
    let address = runebender::document::variable::GlyphLayerAddress {
        glyph: "A".into(),
        layer,
    };
    let before = project.capture_document_layer(&address).unwrap();

    let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
    transaction.draft_mut().set_width(725.875).unwrap();
    project
        .commit_document_layer_transaction(transaction)
        .unwrap();

    project.remove_source(source).unwrap();
    assert!(project.document_source(source).is_none());
    assert!(project.undo_sources(false).unwrap());
    assert_eq!(project.document_source(source).unwrap().id(), source);
    assert!(matches!(
        project.replay_document_layer_history(&address, HistoryDirection::Undo),
        Ok(DocumentHistoryReplayOutcome::Changed { .. })
    ));
    assert_eq!(project.capture_document_layer(&address), Some(before));
}
