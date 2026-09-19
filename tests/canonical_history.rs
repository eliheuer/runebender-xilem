// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical per-layer history behavior independent of source-format projections.

use std::collections::BTreeMap;
use std::path::PathBuf;

use norad::{Contour, ContourPoint, Font, Glyph, PointType};
use runebender::document::history::{
    CanonicalHistory, DocumentHistory, HistoryDirection, HistoryReplayError, HistoryReplayOutcome,
};
use runebender::document::project::{Master, Project};
use runebender::document::variable::{GlyphLayerAddress, LayerId, SourceId};

#[derive(Clone, Debug, PartialEq)]
struct LayerState {
    geometry: Vec<(f64, f64)>,
    width: f64,
    note: String,
    source_extension: BTreeMap<String, String>,
}

fn state(x: f64, width: f64, note: &str, extension: &str) -> LayerState {
    LayerState {
        geometry: vec![(x, 20.0)],
        width,
        note: note.into(),
        source_extension: BTreeMap::from([("opaque".into(), extension.into())]),
    }
}

fn address(glyph: &str, source: usize, layer: &str) -> GlyphLayerAddress {
    GlyphLayerAddress {
        glyph: glyph.into(),
        layer: LayerId {
            source: SourceId(source),
            name: layer.into(),
        },
    }
}

fn apply(
    live: &mut LayerState,
    expected: &LayerState,
    replacement: &LayerState,
) -> Result<(), &'static str> {
    if live != expected {
        return Err("stale document transaction");
    }
    live.clone_from(replacement);
    Ok(())
}

fn project_fixture() -> (Project, GlyphLayerAddress) {
    let mut glyph = Glyph::new("A");
    glyph.width = 500.125;
    glyph.height = 1_000.25;
    glyph.note = Some("before note".into());
    glyph.lib.insert(
        "vendor.private".into(),
        plist::Value::String("before extension".into()),
    );
    glyph.contours.push(Contour::new(
        vec![
            ContourPoint::new(10.0, 20.0, PointType::Move, false, None, None),
            ContourPoint::new(30.0, 40.0, PointType::Line, false, None, None),
        ],
        None,
    ));
    let mut font = Font::new();
    let layer_name = font.default_layer().name().to_string();
    font.default_layer_mut().insert_glyph(glyph);
    let project = Project::from_source(Master::from_font(
        font,
        PathBuf::from("canonical-history-fixture.ufo"),
    ));
    let address = GlyphLayerAddress {
        glyph: "A".into(),
        layer: LayerId {
            source: SourceId(0),
            name: layer_name,
        },
    };
    (project, address)
}

#[test]
fn default_and_auxiliary_layers_have_independent_exact_history() {
    let default = address("A", 7, "public.default");
    let auxiliary = address("A", 7, "background");
    let mut history = CanonicalHistory::default();
    let mut default_live = state(30.0, 500.25, "default after", "default-after");
    let mut auxiliary_live = state(40.0, 600.5, "aux after", "aux-after");

    assert!(history.record(
        default.clone(),
        state(10.0, 500.125, "default before", "default-before"),
        default_live.clone(),
    ));
    assert!(history.record(
        auxiliary.clone(),
        state(20.0, 600.25, "aux before", "aux-before"),
        auxiliary_live.clone(),
    ));

    let default_current = default_live.clone();
    assert_eq!(
        history.replay(
            &default,
            &default_current,
            HistoryDirection::Undo,
            |expected, replacement| apply(&mut default_live, expected, replacement),
        ),
        Ok(HistoryReplayOutcome::Applied)
    );
    assert_eq!(
        default_live,
        state(10.0, 500.125, "default before", "default-before")
    );
    assert_eq!(auxiliary_live, state(40.0, 600.5, "aux after", "aux-after"));
    assert!(history.can_replay(&auxiliary, HistoryDirection::Undo));
    assert!(history.can_replay(&default, HistoryDirection::Redo));

    let auxiliary_current = auxiliary_live.clone();
    assert_eq!(
        history.replay(
            &auxiliary,
            &auxiliary_current,
            HistoryDirection::Undo,
            |expected, replacement| apply(&mut auxiliary_live, expected, replacement),
        ),
        Ok(HistoryReplayOutcome::Applied)
    );
    assert_eq!(
        auxiliary_live,
        state(20.0, 600.25, "aux before", "aux-before")
    );
}

#[test]
fn drag_coalescing_keeps_the_origin_and_removes_a_returned_noop() {
    let address = address("A", 3, "public.default");
    let origin = state(0.0, 500.0, "origin", "same");
    let first = state(10.0, 500.0, "origin", "same");
    let last = state(30.0, 500.0, "origin", "same");
    let mut history = CanonicalHistory::default();

    assert!(history.record(address.clone(), origin.clone(), first.clone()));
    assert!(history.coalesce(&address, &first, last.clone()));
    assert_eq!(history.depth(&address, HistoryDirection::Undo), 1);

    let mut live = last.clone();
    let current = live.clone();
    assert_eq!(
        history.replay(
            &address,
            &current,
            HistoryDirection::Undo,
            |expected, replacement| apply(&mut live, expected, replacement),
        ),
        Ok(HistoryReplayOutcome::Applied)
    );
    assert_eq!(live, origin);

    let mut history = CanonicalHistory::default();
    assert!(history.record(address.clone(), origin.clone(), first.clone()));
    assert!(history.coalesce(&address, &first, origin));
    assert!(!history.can_replay(&address, HistoryDirection::Undo));

    assert!(history.record(address.clone(), first.clone(), last));
    assert!(history.discard_last(&address));
    assert!(!history.can_replay(&address, HistoryDirection::Undo));
}

#[test]
fn stale_and_rejected_replay_leave_document_and_stacks_unchanged() {
    let address = address("A", 1, "public.default");
    let before = state(1.0, 500.0, "before", "before");
    let after = state(2.0, 500.0, "after", "after");
    let unrelated_later_edit = state(2.0, 510.0, "after", "after");
    let mut history = CanonicalHistory::default();
    assert!(history.record(address.clone(), before, after));

    let mut live = unrelated_later_edit.clone();
    assert_eq!(
        history.replay(
            &address,
            &live.clone(),
            HistoryDirection::Undo,
            |expected, replacement| apply(&mut live, expected, replacement),
        ),
        Err(HistoryReplayError::Stale)
    );
    assert_eq!(live, unrelated_later_edit);
    assert_eq!(history.depth(&address, HistoryDirection::Undo), 1);
    assert_eq!(history.depth(&address, HistoryDirection::Redo), 0);

    let after = state(2.0, 500.0, "after", "after");
    let error = history.replay(&address, &after, HistoryDirection::Undo, |_, _| {
        Err::<(), _>("restore rejected")
    });
    assert_eq!(error, Err(HistoryReplayError::Apply("restore rejected")));
    assert_eq!(history.depth(&address, HistoryDirection::Undo), 1);
    assert_eq!(history.depth(&address, HistoryDirection::Redo), 0);
}

#[test]
fn redo_is_invalidated_only_by_a_real_edit_on_the_same_layer() {
    let address = address("A", 1, "public.default");
    let before = state(1.0, 500.0, "before", "before");
    let after = state(2.0, 500.0, "after", "after");
    let replacement = state(3.0, 500.0, "replacement", "replacement");
    let mut history = CanonicalHistory::default();
    assert!(history.record(address.clone(), before.clone(), after.clone()));

    let mut live = after;
    let current = live.clone();
    assert_eq!(
        history.replay(
            &address,
            &current,
            HistoryDirection::Undo,
            |expected, replacement| apply(&mut live, expected, replacement),
        ),
        Ok(HistoryReplayOutcome::Applied)
    );
    assert!(!history.record(address.clone(), live.clone(), live.clone()));
    assert!(history.can_replay(&address, HistoryDirection::Redo));
    assert!(history.record(address.clone(), live, replacement));
    assert!(!history.can_replay(&address, HistoryDirection::Redo));
}

#[test]
fn rename_moves_every_layer_stack_without_changing_stable_layer_ids() {
    let old_default = address("A", 4, "public.default");
    let old_auxiliary = address("A", 4, "background");
    let new_default = address("A.alt", 4, "public.default");
    let new_auxiliary = address("A.alt", 4, "background");
    let mut history = CanonicalHistory::default();
    assert!(history.record(
        old_default.clone(),
        state(1.0, 500.0, "before", "before"),
        state(2.0, 500.0, "after", "after"),
    ));
    assert!(history.record(
        old_auxiliary.clone(),
        state(3.0, 500.0, "before", "before"),
        state(4.0, 500.0, "after", "after"),
    ));

    assert!(history.rename_glyph("A", "A.alt"));
    assert!(!history.can_replay(&old_default, HistoryDirection::Undo));
    assert!(!history.can_replay(&old_auxiliary, HistoryDirection::Undo));
    assert!(history.can_replay(&new_default, HistoryDirection::Undo));
    assert!(history.can_replay(&new_auxiliary, HistoryDirection::Undo));
    assert_eq!(new_default.layer, old_default.layer);
    assert_eq!(new_auxiliary.layer, old_auxiliary.layer);
}

#[test]
fn rename_collision_rejects_atomically() {
    let old = address("A", 4, "public.default");
    let occupied = address("B", 9, "background");
    let mut history = CanonicalHistory::default();
    assert!(history.record(
        old.clone(),
        state(1.0, 500.0, "before", "before"),
        state(2.0, 500.0, "after", "after"),
    ));
    assert!(history.record(
        occupied.clone(),
        state(3.0, 600.0, "before", "before"),
        state(4.0, 600.0, "after", "after"),
    ));

    assert!(!history.rename_glyph("A", "B"));
    assert!(history.can_replay(&old, HistoryDirection::Undo));
    assert!(history.can_replay(&occupied, HistoryDirection::Undo));
    assert!(!history.can_replay(&address("B", 4, "public.default"), HistoryDirection::Undo));
}

#[test]
fn source_restore_with_the_same_identity_keeps_an_older_layer_undo_valid() {
    let edited = address("A", 11, "public.default");
    let unrelated = address("B", 12, "public.default");
    let before = state(1.0, 500.125, "before", "before");
    let after = state(2.0, 500.25, "after", "after");
    let unrelated_value = state(9.0, 700.0, "unrelated", "unrelated");
    let mut document = BTreeMap::from([
        (edited.clone(), after.clone()),
        (unrelated.clone(), unrelated_value.clone()),
    ]);
    let mut history = CanonicalHistory::default();
    assert!(history.record(edited.clone(), before.clone(), after.clone()));

    let removed = document.remove(&edited).expect("the source layer exists");
    assert_eq!(removed, after);
    document.insert(edited.clone(), removed);

    let current = document
        .get(&edited)
        .expect("the source identity was restored")
        .clone();
    assert_eq!(
        history.replay(
            &edited,
            &current,
            HistoryDirection::Undo,
            |expected, replacement| {
                let live = document.get_mut(&edited).ok_or("missing restored source")?;
                apply(live, expected, replacement)
            },
        ),
        Ok(HistoryReplayOutcome::Applied)
    );
    assert_eq!(document.get(&edited), Some(&before));
    assert_eq!(document.get(&unrelated), Some(&unrelated_value));

    let current = document.get(&edited).expect("the layer remains").clone();
    assert_eq!(
        history.replay(
            &edited,
            &current,
            HistoryDirection::Redo,
            |expected, replacement| {
                let live = document.get_mut(&edited).ok_or("missing restored source")?;
                apply(live, expected, replacement)
            },
        ),
        Ok(HistoryReplayOutcome::Applied)
    );
    assert_eq!(document.get(&edited), Some(&after));
    assert_eq!(document.get(&unrelated), Some(&unrelated_value));
}

#[test]
fn project_history_restores_exact_geometry_metadata_and_extensions() {
    let (mut project, address) = project_fixture();
    let before_projection = project
        .glyph_layer(&address.glyph, &address.layer)
        .expect("fixture layer exists");
    let before = DocumentHistory::capture(&project, &address).unwrap();
    let mut history = DocumentHistory::default();

    // Metadata draft operations land in M07. Use the transitional mutation boundary
    // here only to prove that canonical history captures and restores those values.
    assert!(project.edit_layer(&address.glyph, &address.layer, |glyph| {
        glyph.width = 600.875;
        glyph.height = 1_025.5;
        glyph.note = Some("after note".into());
        glyph.lib.insert(
            "vendor.private".into(),
            plist::Value::String("after extension".into()),
        );
        glyph.contours[0].points[1].x = 123.75;
        glyph.contours[0].points[1].y = -45.5;
    }));
    assert!(
        history
            .record_completed(&project, &address, before.clone())
            .unwrap()
    );
    let after = DocumentHistory::capture(&project, &address).unwrap();
    let after_projection = project
        .glyph_layer(&address.glyph, &address.layer)
        .expect("edited layer exists");
    assert_ne!(after, before);

    let revision = project.document_revision();
    assert_eq!(
        history.replay(&mut project, &address, HistoryDirection::Undo),
        Ok(HistoryReplayOutcome::Applied)
    );
    assert_eq!(project.document_revision(), revision.wrapping_add(1));
    assert_eq!(DocumentHistory::capture(&project, &address), Ok(before));
    assert_eq!(
        project.glyph_layer(&address.glyph, &address.layer),
        Some(before_projection)
    );

    let revision = project.document_revision();
    assert_eq!(
        history.replay(&mut project, &address, HistoryDirection::Redo),
        Ok(HistoryReplayOutcome::Applied)
    );
    assert_eq!(project.document_revision(), revision.wrapping_add(1));
    assert_eq!(DocumentHistory::capture(&project, &address), Ok(after));
    assert_eq!(
        project.glyph_layer(&address.glyph, &address.layer),
        Some(after_projection)
    );
}

#[test]
fn project_history_rejects_stale_replay_without_moving_the_stack() {
    let (mut project, address) = project_fixture();
    let before = DocumentHistory::capture(&project, &address).unwrap();
    let mut history = DocumentHistory::default();
    project
        .edit_document_layer(&address.glyph, &address.layer, |draft| {
            draft.set_width(600.25)?;
            Ok(())
        })
        .unwrap();
    assert!(
        history
            .record_completed(&project, &address, before)
            .unwrap()
    );
    project
        .edit_document_layer(&address.glyph, &address.layer, |draft| {
            draft.set_height(1_100.75)?;
            Ok(())
        })
        .unwrap();
    let later = project.document_snapshot();
    let revision = project.document_revision();

    assert_eq!(
        history.replay(&mut project, &address, HistoryDirection::Undo),
        Err(HistoryReplayError::Stale)
    );
    assert_eq!(project.document_snapshot(), later);
    assert_eq!(project.document_revision(), revision);
    assert_eq!(history.depth(&address, HistoryDirection::Undo), 1);
    assert_eq!(history.depth(&address, HistoryDirection::Redo), 0);
}
