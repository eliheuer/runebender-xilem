// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical component alignment stays attached to stable component identity.

use std::path::PathBuf;

use norad::{Component, Font, Glyph, Name};
use runebender::font::history::HistoryDirection;
use runebender::font::project::{DocumentEditOutcome, Project, SourceInput};
use runebender::font::variable::{GlyphLayerAddress, SourceId};

const ALIGNMENT_KEY: &str = "com.glyphsapp.component.alignment";

fn fixture() -> (Project, GlyphLayerAddress) {
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(Glyph::new("base"));
    let mut composite = Glyph::new("composite");
    let mut component = Component::new(
        Name::new("base").unwrap(),
        norad::AffineTransform::default(),
        None,
    );
    component.replace_lib(plist::Dictionary::from_iter([
        (String::from(ALIGNMENT_KEY), plist::Value::Boolean(false)),
        (
            String::from("future.key"),
            plist::Value::String("exact".into()),
        ),
    ]));
    composite.components.push(component);
    font.default_layer_mut().insert_glyph(composite);
    let project = Project::from_source(SourceInput::from_font(
        font,
        PathBuf::from("ComponentAlignment.ufo"),
    ));
    let address = GlyphLayerAddress {
        glyph: "composite".into(),
        layer: project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer(),
    };
    (project, address)
}

#[test]
fn alignment_edits_are_typed_exact_and_undoable() {
    let (mut project, address) = fixture();
    let component = project
        .document_layer(&address.glyph, &address.layer)
        .unwrap()
        .components()
        .next()
        .unwrap()
        .id();
    assert!(
        project
            .document_layer(&address.glyph, &address.layer)
            .unwrap()
            .components()
            .next()
            .unwrap()
            .alignment_disabled()
    );

    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer(&address.glyph, &address.layer, |draft| {
                assert!(draft.component_alignment_disabled(component)?);
                assert!(!draft.set_component_alignment_disabled(component, true)?);
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    let exact = project.encode_ufo_source(SourceId(0)).unwrap();
    let lib = exact.get_glyph("composite").unwrap().components[0]
        .lib()
        .unwrap();
    assert_eq!(lib.get(ALIGNMENT_KEY), Some(&plist::Value::Boolean(false)));

    let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
    assert!(
        transaction
            .draft_mut()
            .set_component_alignment_disabled(component, false)
            .unwrap()
    );
    let DocumentEditOutcome::Changed { change, .. } = project
        .commit_document_layer_transaction(transaction)
        .unwrap()
    else {
        panic!("alignment edit did not commit")
    };
    assert!(change.metadata_changed());
    assert!(!change.geometry_changed());
    let edited = project.encode_ufo_source(SourceId(0)).unwrap();
    let lib = edited.get_glyph("composite").unwrap().components[0]
        .lib()
        .unwrap();
    assert!(!lib.contains_key(ALIGNMENT_KEY));
    assert_eq!(
        lib.get("future.key"),
        Some(&plist::Value::String("exact".into()))
    );

    project
        .replay_document_layer_history(&address, HistoryDirection::Undo)
        .unwrap();
    let restored = project.encode_ufo_source(SourceId(0)).unwrap();
    assert_eq!(
        restored.get_glyph("composite").unwrap().components[0]
            .lib()
            .unwrap()
            .get(ALIGNMENT_KEY),
        Some(&plist::Value::Boolean(false))
    );
}
