// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical glyph-layer metadata preserves exact UFO payloads until a typed edit.

use std::path::PathBuf;

use runebender::document::DocumentEditError;
use runebender::document::history::HistoryDirection;
use runebender::document::model::glyph_metadata::{
    COMPOSITION_RECIPE_KEY, LEFT_METRICS_KEY, MARK_COLOR_KEY, METABALLS_KEY, Metaball,
    MetaballGroup, Metaballs, RIGHT_METRICS_KEY,
};
use runebender::document::project::{DocumentEditOutcome, Master, Project};
use runebender::document::variable::{GlyphLayerAddress, SourceId};

fn source_metaballs() -> Metaballs {
    Metaballs {
        version: 1,
        groups: vec![MetaballGroup {
            id: 7,
            threshold: 1.25,
            balls: vec![Metaball {
                id: 11,
                x: 12.5,
                y: -30.25,
                radius: 80.125,
                stiffness: -0.75,
            }],
        }],
    }
}

fn fixture() -> (Project, GlyphLayerAddress) {
    let mut font = norad::Font::new();
    let mut glyph = norad::Glyph::new("A");
    glyph.lib.insert(
        MARK_COLOR_KEY.into(),
        plist::Value::String(" 0.1, 0.20, 0.3, 1 ".into()),
    );
    glyph.lib.insert(
        LEFT_METRICS_KEY.into(),
        plist::Value::String(" =n+10 ".into()),
    );
    glyph
        .lib
        .insert(RIGHT_METRICS_KEY.into(), plist::Value::Integer(7.into()));
    glyph.lib.insert(
        METABALLS_KEY.into(),
        plist::to_value(&source_metaballs()).unwrap(),
    );
    glyph.lib.insert(
        COMPOSITION_RECIPE_KEY.into(),
        plist::Value::String(" A + acutecomb ".into()),
    );
    glyph
        .lib
        .insert("future.key".into(), plist::Value::String("exact".into()));
    font.default_layer_mut().insert_glyph(glyph);
    let project = Project::from_source(Master::from_font(font, PathBuf::from("LayerMetadata.ufo")));
    let address = GlyphLayerAddress {
        glyph: "A".into(),
        layer: project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer(),
    };
    (project, address)
}

#[test]
fn layer_metadata_reads_writes_and_replays_atomically() {
    let (mut project, address) = fixture();
    let view = project
        .document_layer(&address.glyph, &address.layer)
        .unwrap();
    assert_eq!(view.mark_color().unwrap().unwrap().green, 0.2);
    assert_eq!(view.metrics_key(true).unwrap(), Some(" =n+10 "));
    assert_eq!(
        view.metrics_formula(true)
            .unwrap()
            .unwrap()
            .referenced_glyph(),
        Some("n")
    );
    assert_eq!(
        view.metrics_key(false),
        Err(DocumentEditError::InvalidLayerMetadata)
    );
    assert_eq!(view.metaballs().unwrap(), source_metaballs());
    assert_eq!(
        view.composition_recipe_source().unwrap(),
        Some(" A + acutecomb ")
    );

    let exact = project.source_snapshot(SourceId(0)).unwrap();
    let exact_lib = exact.get_glyph("A").unwrap().lib.clone();
    assert_eq!(
        exact_lib.get(MARK_COLOR_KEY),
        Some(&plist::Value::String(" 0.1, 0.20, 0.3, 1 ".into()))
    );
    assert_eq!(
        exact_lib.get("future.key"),
        Some(&plist::Value::String("exact".into()))
    );
    assert_eq!(
        exact_lib.get(COMPOSITION_RECIPE_KEY),
        Some(&plist::Value::String(" A + acutecomb ".into()))
    );

    let mut no_op = project.begin_document_layer_transaction(&address).unwrap();
    let draft = no_op.draft_mut();
    let color = draft.view().mark_color().unwrap();
    assert!(!draft.set_mark_color(color).unwrap());
    assert!(!draft.set_metrics_key(true, Some(" =n+10 ".into())).unwrap());
    assert!(!draft.set_metaballs(source_metaballs()).unwrap());
    let revision = project.document_revision();
    assert_eq!(
        project.commit_document_layer_transaction(no_op).unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(
        project
            .source_snapshot(SourceId(0))
            .unwrap()
            .get_glyph("A")
            .unwrap()
            .lib,
        exact_lib
    );

    let mut edited_metaballs = source_metaballs();
    edited_metaballs.groups[0].balls[0].x = 99.75;
    let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
    let draft = transaction.draft_mut();
    assert!(
        draft
            .set_mark_color(Some(
                runebender::document::model::glyph_metadata::MarkColor {
                    red: 0.4,
                    green: 0.5,
                    blue: 0.6,
                    alpha: 1.0,
                },
            ))
            .unwrap()
    );
    assert!(
        draft
            .set_metrics_key(false, Some("=|o*1.25".into()))
            .unwrap()
    );
    assert!(draft.set_metaballs(edited_metaballs.clone()).unwrap());
    let DocumentEditOutcome::Changed { change, .. } = project
        .commit_document_layer_transaction(transaction)
        .unwrap()
    else {
        panic!("typed metadata edit did not commit")
    };
    assert!(change.metadata_changed());
    assert!(!change.geometry_changed());
    let view = project
        .document_layer(&address.glyph, &address.layer)
        .unwrap();
    assert_eq!(view.metrics_key(false).unwrap(), Some("=|o*1.25"));
    assert_eq!(view.metaballs().unwrap(), edited_metaballs);

    let document = project.document_snapshot();
    let revision = project.document_revision();
    let mut rejected = project.begin_document_layer_transaction(&address).unwrap();
    assert_eq!(
        rejected.draft_mut().set_mark_color(Some(
            runebender::document::model::glyph_metadata::MarkColor {
                red: f64::NAN,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0,
            },
        )),
        Err(DocumentEditError::InvalidLayerMetadata)
    );
    assert_eq!(project.document_snapshot(), document);
    assert_eq!(project.document_revision(), revision);

    project
        .replay_document_layer_history(&address, HistoryDirection::Undo)
        .unwrap();
    let restored = project.source_snapshot(SourceId(0)).unwrap();
    assert_eq!(restored.get_glyph("A").unwrap().lib, exact_lib);
}
