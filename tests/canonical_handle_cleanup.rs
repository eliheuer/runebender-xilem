// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Regression contracts for canonical corner and handle cleanup.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use norad::{Anchor, Component, Contour, ContourPoint, Font, Glyph, Name, PointType};
use runebender::document::LayerPointType;
use runebender::document::history::HistoryDirection;
use runebender::document::project::{
    DocumentEditOutcome, DocumentHistoryReplayOutcome, Master, Project,
};
use runebender::document::variable::{GlyphLayerAddress, LayerId, SourceId};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "runebender-handle-cleanup-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn object_lib(label: &str) -> plist::Dictionary {
    let mut lib = plist::Dictionary::new();
    lib.insert("com.example.owner".into(), label.into());
    lib
}

fn point(x: f64, y: f64, typ: PointType, label: &str) -> ContourPoint {
    let mut point = ContourPoint::new(
        x,
        y,
        typ,
        false,
        Some(Name::new(label).unwrap()),
        Some(norad::Identifier::new(&format!("point-{label}")).unwrap()),
    );
    point.replace_lib(object_lib(label));
    point
}

fn contour(points: Vec<ContourPoint>, label: &str) -> Contour {
    let mut contour = Contour::new(
        points,
        Some(norad::Identifier::new(&format!("contour-{label}")).unwrap()),
    );
    contour.replace_lib(object_lib(label));
    contour
}

fn hyper_contour(points: Vec<ContourPoint>, label: &str) -> Contour {
    let mut contour = Contour::new(
        points,
        Some(norad::Identifier::new(&format!("hyperbezier-{label}")).unwrap()),
    );
    contour.replace_lib(object_lib(label));
    contour
}

fn fixture() -> (Scratch, Project, LayerId, GlyphLayerAddress) {
    let scratch = Scratch::new();
    let source_path = scratch.0.join("Corners.ufo");
    let mut glyph = Glyph::new("corners");
    glyph.width = 640.125;
    glyph.height = 900.875;
    glyph.note = Some("preserve cleanup metadata".into());
    glyph.lib.insert("com.example.glyph".into(), "keep".into());
    glyph.anchors.push(Anchor::new(
        100.25,
        700.75,
        Some(Name::new("top").unwrap()),
        None,
        Some(norad::Identifier::new("anchor-top").unwrap()),
    ));
    glyph.components.push(Component::new(
        Name::new("base").unwrap(),
        norad::AffineTransform {
            x_offset: 12.25,
            y_offset: -7.75,
            ..Default::default()
        },
        Some(norad::Identifier::new("component-base").unwrap()),
    ));
    glyph.contours.push(contour(
        vec![
            point(0.125, 0.375, PointType::Line, "closed-a"),
            point(200.125, 0.375, PointType::Line, "closed-b"),
            point(200.125, 200.375, PointType::Line, "closed-c"),
            point(0.125, 200.375, PointType::Line, "closed-d"),
        ],
        "closed",
    ));
    glyph.contours.push(contour(
        vec![
            point(300.25, 0.5, PointType::Move, "open-a"),
            point(400.25, 0.5, PointType::Line, "open-b"),
            point(400.25, 100.5, PointType::Line, "open-c"),
            point(500.25, 100.5, PointType::Line, "open-d"),
        ],
        "open",
    ));
    let mut base = Glyph::new("base");
    base.contours.push(contour(
        vec![
            point(0.0, 0.0, PointType::Move, "base-a"),
            point(10.0, 0.0, PointType::Line, "base-b"),
        ],
        "base",
    ));
    let mut other = Glyph::new("other");
    other.contours.push(contour(
        vec![
            point(0.0, 0.0, PointType::Move, "other-a"),
            point(20.0, 0.0, PointType::Line, "other-b"),
        ],
        "other",
    ));
    let mut font = Font::new();
    for glyph in [glyph, base, other] {
        font.default_layer_mut().insert_glyph(glyph);
    }
    let project = Project::from_source(Master::from_font(font, source_path));
    let layer = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let address = GlyphLayerAddress {
        glyph: "corners".into(),
        layer: layer.clone(),
    };
    (scratch, project, layer, address)
}

#[test]
fn selected_closed_and_open_corners_round_with_stable_metadata_and_history() {
    let (scratch, mut project, layer, address) = fixture();
    let before_projection = project.glyph_layer("corners", &layer).unwrap();
    let before = project.document_layer("corners", &layer).unwrap();
    let contours: Vec<_> = before.contours().collect();
    let contour_ids: Vec<_> = contours.iter().map(|contour| contour.id()).collect();
    let closed_corner = contours[0].points().next().unwrap().id();
    let open_corner = contours[1].points().nth(1).unwrap().id();
    let untouched: HashSet<_> = contours
        .iter()
        .flat_map(|contour| contour.points())
        .map(|point| point.id())
        .filter(|point| *point != closed_corner && *point != open_corner)
        .collect();

    let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
    let selection = transaction
        .draft_mut()
        .round_selected_corners(&[closed_corner, open_corner])
        .unwrap()
        .expect("selected line corners round");
    assert_eq!(
        selection.len(),
        4,
        "each rounded corner selects two endpoints"
    );
    assert!(
        selection.contains(&closed_corner),
        "closed corner identity survives"
    );
    assert!(
        selection.contains(&open_corner),
        "open corner identity survives"
    );
    assert!(matches!(
        project
            .commit_document_layer_transaction(transaction)
            .unwrap(),
        DocumentEditOutcome::Changed { .. }
    ));

    let after = project.document_layer("corners", &layer).unwrap();
    let contours: Vec<_> = after.contours().collect();
    assert_eq!(
        contours
            .iter()
            .map(|contour| contour.id())
            .collect::<Vec<_>>(),
        contour_ids,
        "rounding keeps stable contour identities"
    );
    assert!(contours[0].is_closed(), "closed contour remains closed");
    assert!(!contours[1].is_closed(), "open contour remains open");
    assert_eq!(
        contours[0].points().count(),
        7,
        "closed corner adds three points"
    );
    assert_eq!(
        contours[1].points().count(),
        7,
        "open corner adds three points"
    );
    let after_ids: HashSet<_> = contours
        .iter()
        .flat_map(|contour| contour.points())
        .map(|point| point.id())
        .collect();
    assert!(
        untouched.is_subset(&after_ids),
        "untouched point identities survive"
    );
    assert!(
        selection.iter().all(|point| after_ids.contains(point)),
        "replacement selection addresses live stable points"
    );
    let selected_points: Vec<_> = contours
        .iter()
        .flat_map(|contour| contour.points())
        .filter(|point| selection.contains(&point.id()))
        .collect();
    assert!(
        selected_points.iter().all(|point| point.is_smooth()),
        "fillet endpoints are smooth"
    );

    let after_projection = project.glyph_layer("corners", &layer).unwrap();
    assert_eq!(after_projection.width, before_projection.width);
    assert_eq!(after_projection.height, before_projection.height);
    assert_eq!(after_projection.note, before_projection.note);
    assert_eq!(after_projection.lib, before_projection.lib);
    assert_eq!(after_projection.anchors, before_projection.anchors);
    assert_eq!(after_projection.components, before_projection.components);
    let names: HashSet<_> = after_projection
        .contours
        .iter()
        .flat_map(|contour| &contour.points)
        .filter_map(|point| point.name.as_ref().map(Name::as_str))
        .collect();
    for expected in ["closed-a", "open-b", "closed-b", "open-c"] {
        assert!(
            names.contains(expected),
            "lost point metadata for {expected}"
        );
    }

    assert!(matches!(
        project
            .replay_document_layer_history(&address, HistoryDirection::Undo)
            .unwrap(),
        DocumentHistoryReplayOutcome::Changed { .. }
    ));
    assert_eq!(
        project.glyph_layer("corners", &layer),
        Some(before_projection),
        "undo restores the exact source projection"
    );
    assert!(matches!(
        project
            .replay_document_layer_history(&address, HistoryDirection::Redo)
            .unwrap(),
        DocumentHistoryReplayOutcome::Changed { .. }
    ));
    assert_eq!(
        project.glyph_layer("corners", &layer),
        Some(after_projection.clone()),
        "redo restores the rounded result"
    );

    project.save().unwrap();
    let reloaded = Project::load(&scratch.0.join("Corners.ufo")).unwrap();
    let reloaded_layer = reloaded
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    assert_eq!(
        reloaded.glyph_layer("corners", &reloaded_layer),
        Some(after_projection),
        "rounded topology and metadata survive UFO save/reopen"
    );
}

#[test]
fn corner_rounding_noops_and_invalid_selection_are_atomic() {
    let (_scratch, mut project, layer, address) = fixture();
    let layer_view = project.document_layer("corners", &layer).unwrap();
    let open_endpoint = layer_view
        .contours()
        .nth(1)
        .unwrap()
        .points()
        .next()
        .unwrap()
        .id();
    let foreign = project
        .document_layer("other", &layer)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .next()
        .unwrap()
        .id();
    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer("corners", &layer, |draft| {
                assert!(
                    draft.round_selected_corners(&[])?.is_none(),
                    "empty selection is a no-op"
                );
                assert!(
                    draft.round_selected_corners(&[open_endpoint])?.is_none(),
                    "open endpoint is not an interior corner"
                );
                assert_eq!(
                    draft.round_selected_corners(&[foreign]),
                    Err(runebender::document::DocumentEditError::MissingPoint(
                        foreign
                    )),
                    "foreign stable identity is rejected"
                );
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision },
        "caught no-op and errors do not commit a partial draft"
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(
        project.document_layer_history_depth(&address, HistoryDirection::Undo),
        0,
        "no-op corner cleanup records no history"
    );
}

#[test]
fn corner_rounding_requires_adjacent_line_segments() {
    let (_scratch, mut project, layer, _address) = fixture();
    let original = project
        .document_layer("corners", &layer)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .next()
        .unwrap()
        .id();
    project
        .edit_document_layer("corners", &layer, |draft| {
            assert!(draft.round_selected_corners(&[original])?.is_some());
            Ok(())
        })
        .unwrap();
    let after = project.document_layer("corners", &layer).unwrap();
    let first = after.contours().next().unwrap().points().next().unwrap();
    assert_eq!(
        first.id(),
        original,
        "original corner identity moves to the incoming fillet endpoint"
    );
    assert_eq!(
        first.point_type(),
        LayerPointType::Line,
        "incoming line role survives on the original corner identity"
    );
}

fn handle_fixture() -> (Scratch, Project, LayerId, GlyphLayerAddress) {
    let scratch = Scratch::new();
    let source_path = scratch.0.join("Handles.ufo");
    let mut glyph = Glyph::new("handles");
    glyph.width = 700.125;
    glyph.contours.push(contour(
        vec![
            point(0.0, 0.0, PointType::Curve, "join"),
            point(0.0, 50.0, PointType::OffCurve, "out-adjacent"),
            point(-80.0, 80.0, PointType::OffCurve, "out-far"),
            point(-120.0, 120.0, PointType::Curve, "next"),
            point(-140.0, 70.0, PointType::OffCurve, "middle-a"),
            point(-130.0, -50.0, PointType::OffCurve, "middle-b"),
            point(-100.0, -90.0, PointType::Curve, "previous"),
            point(-70.0, -50.0, PointType::OffCurve, "in-far"),
            point(0.0, -30.0, PointType::OffCurve, "in-adjacent"),
        ],
        "handles",
    ));
    glyph.contours[0].points[0].smooth = true;
    glyph.contours.push(contour(
        vec![
            point(200.0, 0.0, PointType::Move, "open-start"),
            point(220.0, 40.0, PointType::OffCurve, "open-control-a"),
            point(280.0, 40.0, PointType::OffCurve, "open-control-b"),
            point(300.0, 0.0, PointType::Curve, "open-end"),
        ],
        "open-handles",
    ));
    glyph.contours.push(contour(
        vec![
            point(400.0, 0.0, PointType::QCurve, "mixed-start"),
            point(420.0, 80.0, PointType::OffCurve, "mixed-cubic-a"),
            point(480.0, 80.0, PointType::OffCurve, "mixed-cubic-b"),
            point(500.0, 0.0, PointType::Curve, "mixed-cubic-end"),
            point(450.0, -80.0, PointType::OffCurve, "mixed-quadratic"),
        ],
        "mixed-handles",
    ));
    glyph.contours.push(hyper_contour(
        vec![
            point(600.0, 0.0, PointType::Curve, "hyper-a"),
            point(680.0, 120.0, PointType::Curve, "hyper-b"),
            point(760.0, 0.0, PointType::Curve, "hyper-c"),
        ],
        "handles",
    ));
    glyph.contours.push(contour(
        vec![
            point(0.0, 0.0, PointType::QCurve, "quadratic-a"),
            point(0.0, 50.0, PointType::OffCurve, "quadratic-control-a"),
            point(-80.0, 80.0, PointType::OffCurve, "quadratic-control-b"),
            point(-120.0, 120.0, PointType::QCurve, "quadratic-b"),
            point(-140.0, 70.0, PointType::OffCurve, "quadratic-control-c"),
            point(-130.0, -50.0, PointType::OffCurve, "quadratic-control-d"),
            point(-100.0, -90.0, PointType::QCurve, "quadratic-c"),
            point(-70.0, -50.0, PointType::OffCurve, "quadratic-control-e"),
            point(0.0, -30.0, PointType::OffCurve, "quadratic-control-f"),
        ],
        "quadratic-handles",
    ));
    glyph.contours[4].points[0].smooth = true;
    let mut other = Glyph::new("other-handle");
    other.contours.push(contour(
        vec![
            point(0.0, 0.0, PointType::Move, "other-start"),
            point(10.0, 0.0, PointType::Line, "other-end"),
        ],
        "other-handle",
    ));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    font.default_layer_mut().insert_glyph(other);
    let project = Project::from_source(Master::from_font(font, source_path));
    let layer = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let address = GlyphLayerAddress {
        glyph: "handles".into(),
        layer: layer.clone(),
    };
    (scratch, project, layer, address)
}

#[test]
fn harmonize_moves_only_adjacent_handles_with_stable_identity() {
    let (_scratch, mut project, layer, address) = handle_fixture();
    let before = project.document_layer("handles", &layer).unwrap();
    let contour = before.contours().next().unwrap();
    let points: Vec<_> = contour.points().collect();
    let join = points[0].id();
    let incoming = points[8].id();
    let outgoing = points[1].id();
    let identities: Vec<_> = points.iter().map(|point| point.id()).collect();
    let positions: Vec<_> = points.iter().map(|point| point.position()).collect();
    let expected = crate_expected_harmonize(&positions).expect("fixture is harmonizable");

    let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
    assert!(
        transaction.draft_mut().harmonize_handles(&[join]).unwrap(),
        "selected smooth cubic join harmonizes"
    );
    project
        .commit_document_layer_transaction(transaction)
        .unwrap();

    let after = project.document_layer("handles", &layer).unwrap();
    let points: Vec<_> = after.contours().next().unwrap().points().collect();
    assert_eq!(
        points.iter().map(|point| point.id()).collect::<Vec<_>>(),
        identities,
        "harmonize preserves every point identity and storage position"
    );
    assert_eq!(
        points[8].position(),
        expected.0,
        "incoming adjacent handle uses the shared geometry primitive"
    );
    assert_eq!(
        points[1].position(),
        expected.1,
        "outgoing adjacent handle uses the shared geometry primitive"
    );
    assert_eq!(points[0].position(), positions[0]);
    assert_eq!(points[2].position(), positions[2]);
    assert_eq!(points[7].position(), positions[7]);
    assert!(points.iter().any(|point| point.id() == incoming));
    assert!(points.iter().any(|point| point.id() == outgoing));

    assert!(matches!(
        project
            .replay_document_layer_history(&address, HistoryDirection::Undo)
            .unwrap(),
        DocumentHistoryReplayOutcome::Changed { .. }
    ));
    let restored: Vec<_> = project
        .document_layer("handles", &layer)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .map(|point| point.position())
        .collect();
    assert_eq!(
        restored, positions,
        "undo restores exact fractional geometry"
    );
}

fn crate_expected_harmonize(points: &[kurbo::Point]) -> Option<(kurbo::Point, kurbo::Point)> {
    runebender::analysis::curve::harmonize(points[7], points[8], points[0], points[1], points[2])
        .map(|(incoming, outgoing)| (incoming.round(), outgoing.round()))
}

#[test]
fn harmonize_selection_scope_and_errors_are_atomic() {
    let (_scratch, mut project, layer, _address) = handle_fixture();
    let handle = project
        .document_layer("handles", &layer)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .nth(1)
        .unwrap()
        .id();
    let foreign = project
        .document_layer("other-handle", &layer)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .next()
        .unwrap()
        .id();
    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer("handles", &layer, |draft| {
                assert!(
                    !draft.harmonize_handles(&[handle])?,
                    "selecting only a handle does not select its smooth join"
                );
                assert_eq!(
                    draft.harmonize_handles(&[foreign]),
                    Err(runebender::document::DocumentEditError::MissingPoint(
                        foreign
                    )),
                    "foreign point identity is rejected"
                );
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision },
        "caught no-op and error leave the draft unchanged"
    );
    assert_eq!(project.document_snapshot(), snapshot);
}

#[test]
fn balance_scopes_a_cubic_by_any_stable_segment_point() {
    let (_scratch, mut project, layer, _address) = handle_fixture();
    let before = project.document_layer("handles", &layer).unwrap();
    let points: Vec<_> = before.contours().next().unwrap().points().collect();
    let identities: Vec<_> = points.iter().map(|point| point.id()).collect();
    let positions: Vec<_> = points.iter().map(|point| point.position()).collect();
    let selected_handle = points[4].id();
    let expected = runebender::analysis::curve::balance(
        positions[3],
        positions[4],
        positions[5],
        positions[6],
    )
    .map(|(first, second)| (first.round(), second.round()))
    .expect("fixture cubic is balanceable");

    assert!(matches!(
        project
            .edit_document_layer("handles", &layer, |draft| {
                assert!(draft.balance_handles(&[selected_handle])?);
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Changed { .. }
    ));

    let projected = project.glyph_layer("handles", &layer).unwrap();
    let after = project.document_layer("handles", &layer).unwrap();
    let points: Vec<_> = after.contours().next().unwrap().points().collect();
    assert_eq!(
        points.iter().map(|point| point.id()).collect::<Vec<_>>(),
        identities,
        "balance preserves every stable point identity"
    );
    assert_eq!(points[4].position(), expected.0);
    assert_eq!(points[5].position(), expected.1);
    assert_eq!(points[0].position(), positions[0]);
    assert_eq!(points[3].position(), positions[3]);
    assert_eq!(points[6].position(), positions[6]);
    assert_eq!(
        projected.contours[0].points[4].name.as_deref(),
        Some("middle-a")
    );
    let expected_lib = object_lib("middle-a");
    assert_eq!(
        projected.contours[0].points[4].lib(),
        Some(&expected_lib),
        "moved handle keeps its source metadata"
    );
}

#[test]
fn balance_ignores_open_and_unselected_segments_without_history() {
    let (_scratch, mut project, layer, address) = handle_fixture();
    let open_handle = project
        .document_layer("handles", &layer)
        .unwrap()
        .contours()
        .nth(1)
        .unwrap()
        .points()
        .nth(1)
        .unwrap()
        .id();
    let snapshot = project.document_snapshot();
    let revision = project.document_revision();

    assert_eq!(
        project
            .edit_document_layer("handles", &layer, |draft| {
                assert!(
                    !draft.balance_handles(&[open_handle])?,
                    "open cubic segments are not balance candidates"
                );
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(
        project.document_layer_history_depth(&address, HistoryDirection::Undo),
        0,
        "a no-op balance records no history"
    );
}

#[test]
fn harmonize_and_balance_leave_quadratic_chains_atomically_unchanged() {
    let (_scratch, mut project, layer, address) = handle_fixture();
    let quadratic = project
        .document_layer("handles", &layer)
        .unwrap()
        .contours()
        .nth(4)
        .unwrap();
    let smooth_join = quadratic.points().next().unwrap().id();
    let control = quadratic.points().nth(1).unwrap().id();
    let snapshot = project.document_snapshot();
    let revision = project.document_revision();

    assert_eq!(
        project
            .edit_document_layer("handles", &layer, |draft| {
                assert!(
                    !draft.harmonize_handles(&[smooth_join])?,
                    "harmonize must not reinterpret quadratic controls"
                );
                assert!(
                    !draft.balance_handles(&[control])?,
                    "balance must not reinterpret quadratic controls"
                );
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(
        project.document_layer_history_depth(&address, HistoryDirection::Undo),
        0,
        "unsupported quadratic operations record no history"
    );
}

#[test]
fn optimize_empty_selection_moves_cubic_handles_with_stable_identity_and_history() {
    let (_scratch, mut project, layer, address) = handle_fixture();
    let before_projection = project.glyph_layer("handles", &layer).unwrap();
    let before = project.document_layer("handles", &layer).unwrap();
    let points: Vec<_> = before.contours().next().unwrap().points().collect();
    let identities: Vec<_> = points.iter().map(|point| point.id()).collect();
    let positions: Vec<_> = points.iter().map(|point| point.position()).collect();
    let input: Vec<_> = points
        .iter()
        .map(|point| runebender::analysis::curve::OptPoint {
            p: point.position(),
            on: point.point_type() != LayerPointType::OffCurve,
            smooth: point.is_smooth(),
        })
        .collect();
    let expected = runebender::analysis::curve::optimize_contour(&input, 0.12);
    let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
    assert!(transaction.draft_mut().optimize_handles(&[], 0.12).unwrap());
    assert!(matches!(
        project
            .commit_document_layer_transaction(transaction)
            .unwrap(),
        DocumentEditOutcome::Changed { .. }
    ));

    let after = project.document_layer("handles", &layer).unwrap();
    let points: Vec<_> = after.contours().next().unwrap().points().collect();
    assert_eq!(
        points.iter().map(|point| point.id()).collect::<Vec<_>>(),
        identities,
        "optimize preserves every stable point identity"
    );
    for (index, point) in points.iter().enumerate() {
        let expected_position = if point.point_type() == LayerPointType::OffCurve {
            expected[index]
        } else {
            positions[index]
        };
        assert_eq!(
            point.position(),
            expected_position,
            "unexpected optimized position at point {index}"
        );
    }
    let projected = project.glyph_layer("handles", &layer).unwrap();
    assert_eq!(
        projected.contours[0].points[1].name.as_deref(),
        Some("out-adjacent")
    );
    let expected_lib = object_lib("out-adjacent");
    assert_eq!(projected.contours[0].points[1].lib(), Some(&expected_lib));

    assert!(matches!(
        project
            .replay_document_layer_history(&address, HistoryDirection::Undo)
            .unwrap(),
        DocumentHistoryReplayOutcome::Changed { .. }
    ));
    assert_eq!(
        project.glyph_layer("handles", &layer),
        Some(before_projection),
        "undo restores the exact source projection"
    );
}

#[test]
fn optimize_preserves_open_hyper_and_quadratic_geometry() {
    let (_scratch, mut project, layer, _address) = handle_fixture();
    let before = project.document_layer("handles", &layer).unwrap();
    let contours: Vec<_> = before.contours().collect();
    let before_positions: Vec<Vec<_>> = contours
        .iter()
        .map(|contour| contour.points().map(|point| point.position()).collect())
        .collect();
    let before_ids: Vec<Vec<_>> = contours
        .iter()
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .collect();
    let selected = [
        contours[1].points().nth(1).unwrap().id(),
        contours[2].points().nth(1).unwrap().id(),
        contours[3].points().next().unwrap().id(),
    ];

    assert!(matches!(
        project
            .edit_document_layer("handles", &layer, |draft| {
                assert!(draft.optimize_handles(&selected, 0.12)?);
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Changed { .. }
    ));

    let after = project.document_layer("handles", &layer).unwrap();
    let contours: Vec<_> = after.contours().collect();
    let after_ids: Vec<Vec<_>> = contours
        .iter()
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .collect();
    assert_eq!(
        after_ids, before_ids,
        "optimize preserves stable identities"
    );
    assert_eq!(
        contours[1]
            .points()
            .map(|point| point.position())
            .collect::<Vec<_>>(),
        before_positions[1],
        "open contour remains exact"
    );
    assert_eq!(
        contours[3]
            .points()
            .map(|point| point.position())
            .collect::<Vec<_>>(),
        before_positions[3],
        "hyperbezier contour remains exact"
    );
    assert_eq!(
        contours[2].points().nth(4).unwrap().position(),
        before_positions[2][4],
        "quadratic control on a mixed contour remains exact"
    );
    assert_ne!(
        contours[2].points().nth(1).unwrap().position(),
        before_positions[2][1],
        "the explicit cubic portion of a mixed contour still optimizes"
    );
}

#[test]
fn optimize_rejects_invalid_parameters_atomically() {
    let (_scratch, mut project, layer, address) = handle_fixture();
    let view = project.document_layer("handles", &layer).unwrap();
    let open_handle = view
        .contours()
        .nth(1)
        .unwrap()
        .points()
        .nth(1)
        .unwrap()
        .id();
    let foreign = project
        .document_layer("other-handle", &layer)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .next()
        .unwrap()
        .id();
    let snapshot = project.document_snapshot();
    let revision = project.document_revision();

    assert_eq!(
        project
            .edit_document_layer("handles", &layer, |draft| {
                assert_eq!(
                    draft.optimize_handles(&[open_handle], f64::NAN),
                    Err(runebender::document::DocumentEditError::NonFinite)
                );
                assert_eq!(
                    draft.optimize_handles(&[open_handle], f64::INFINITY),
                    Err(runebender::document::DocumentEditError::NonFinite)
                );
                assert_eq!(
                    draft.optimize_handles(&[open_handle], -0.01),
                    Err(runebender::document::DocumentEditError::Rejected)
                );
                assert_eq!(
                    draft.optimize_handles(&[foreign], 0.12),
                    Err(runebender::document::DocumentEditError::MissingPoint(
                        foreign
                    ))
                );
                assert!(
                    !draft.optimize_handles(&[open_handle], 0.0)?,
                    "zero tolerance is valid and an open contour remains a no-op"
                );
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(
        project.document_layer_history_depth(&address, HistoryDirection::Undo),
        0,
        "caught validation errors record no history"
    );
}
