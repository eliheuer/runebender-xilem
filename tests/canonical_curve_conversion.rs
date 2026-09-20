// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Regression contracts for canonical curve conversion.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use kurbo::ParamCurve as _;
use norad::{Anchor, Component, Contour, ContourPoint, Font, Glyph, Name, PointType};
use runebender::document::history::HistoryDirection;
use runebender::document::project::{
    DocumentEditOutcome, DocumentHistoryReplayOutcome, Project, SourceInput,
};
use runebender::document::variable::{GlyphLayerAddress, LayerId, SourceId};
use runebender::document::{DocumentEditError, LayerPointType};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "runebender-curve-conversion-{}-{}",
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

fn project_from_font(font: Font, path: &Path) -> Project {
    Project::from_source(SourceInput::from_font(font, path.to_owned()))
}

fn default_layer(project: &Project) -> LayerId {
    project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer()
}

fn address(glyph: &str, layer: &LayerId) -> GlyphLayerAddress {
    GlyphLayerAddress {
        glyph: glyph.into(),
        layer: layer.clone(),
    }
}

fn ordinary_paths(project: &Project, glyph: &str, layer: &LayerId) -> Vec<kurbo::BezPath> {
    project
        .document_layer(glyph, layer)
        .unwrap()
        .contours()
        .map(runebender::outline::glyph_paths::ordinary_contour_to_bezpath)
        .collect()
}

fn assert_paths_equal(left: &[kurbo::BezPath], right: &[kurbo::BezPath], tolerance: f64) {
    assert_eq!(
        left.len(),
        right.len(),
        "curve conversion changed the contour count"
    );
    for (left, right) in left.iter().zip(right) {
        assert_eq!(
            left.segments().count(),
            right.segments().count(),
            "curve conversion changed the segment count"
        );
        for (left, right) in left.segments().zip(right.segments()) {
            for step in 0..=16 {
                let t = f64::from(step) / 16.0;
                assert!(
                    left.eval(t).distance(right.eval(t)) <= tolerance,
                    "curve conversion moved the outline at t={t}: {left:?} != {right:?}"
                );
            }
        }
    }
}

fn quadratic_fixture() -> Glyph {
    let mut glyph = Glyph::new("quadratics");
    glyph.width = 612.345_678_9;
    glyph.height = 987.654_321;
    glyph.note = Some("preserve layer metadata".into());
    glyph.lib.insert("com.example.glyph".into(), "keep".into());
    glyph.image = Some(
        norad::Image::new(
            "reference.png".into(),
            None,
            norad::AffineTransform {
                x_offset: 12.25,
                y_offset: -8.75,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    glyph.guidelines.push(norad::Guideline::new(
        norad::Line::Angle {
            x: 20.125,
            y: 30.875,
            degrees: 12.5,
        },
        Some(Name::new("slant").unwrap()),
        None,
        Some(norad::Identifier::new("guideline-slant").unwrap()),
    ));
    glyph.anchors.push(Anchor::new(
        50.125,
        700.875,
        Some(Name::new("top").unwrap()),
        None,
        Some(norad::Identifier::new("anchor-top").unwrap()),
    ));
    glyph.components.push(Component::new(
        Name::new("base").unwrap(),
        norad::AffineTransform {
            x_scale: 0.875,
            xy_scale: 0.125,
            yx_scale: -0.25,
            y_scale: 1.125,
            x_offset: 20.25,
            y_offset: -10.75,
        },
        Some(norad::Identifier::new("component-base").unwrap()),
    ));
    glyph.contours.push(contour(
        vec![
            point(0.125, 0.375, PointType::Move, "open-start"),
            point(40.25, 90.5, PointType::OffCurve, "open-control-a"),
            point(90.75, 110.125, PointType::OffCurve, "open-control-b"),
            point(150.875, 10.625, PointType::QCurve, "open-end"),
        ],
        "open",
    ));
    glyph.contours.push(contour(
        vec![
            point(250.125, 10.25, PointType::OffCurve, "all-off-a"),
            point(330.5, 140.75, PointType::OffCurve, "all-off-b"),
            point(410.875, 20.125, PointType::OffCurve, "all-off-c"),
        ],
        "all-off",
    ));
    glyph.contours.push(contour(
        vec![
            point(500.125, 0.25, PointType::Line, "mixed-start"),
            point(520.5, 70.75, PointType::OffCurve, "mixed-cubic-a"),
            point(580.25, 80.125, PointType::OffCurve, "mixed-cubic-b"),
            point(610.875, 10.5, PointType::Curve, "mixed-cubic-end"),
            point(590.625, -40.375, PointType::Line, "mixed-line"),
            point(530.75, -55.125, PointType::OffCurve, "mixed-quad-control"),
            point(500.125, 0.25, PointType::QCurve, "mixed-quad-end"),
        ],
        "mixed",
    ));
    glyph
}

#[test]
fn quadratic_to_cubic_is_exact_preserving_undoable_and_persistent() {
    let scratch = Scratch::new();
    let source_path = scratch.0.join("Quadratics.ufo");
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(Glyph::new("base"));
    font.default_layer_mut().insert_glyph(quadratic_fixture());
    let mut project = project_from_font(font, &source_path);
    let layer = default_layer(&project);
    let address = address("quadratics", &layer);
    let before_paths = ordinary_paths(&project, "quadratics", &layer);
    let before_projection = project.encode_ufo_layer("quadratics", &layer).unwrap();
    let before_layer = project.document_layer("quadratics", &layer).unwrap();
    let contour_ids: Vec<_> = before_layer
        .contours()
        .map(|contour| contour.id())
        .collect();
    let surviving_ids: HashSet<_> = before_layer
        .contours()
        .flat_map(|contour| contour.points())
        .filter(|point| point.point_type() != LayerPointType::OffCurve)
        .map(|point| point.id())
        .collect();
    let retired_ids: HashSet<_> = before_layer
        .contours()
        .flat_map(|contour| contour.points())
        .filter(|point| point.point_type() == LayerPointType::OffCurve)
        .map(|point| point.id())
        .collect();

    let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
    assert!(
        transaction
            .draft_mut()
            .convert_quadratics_to_cubics()
            .unwrap()
    );
    assert!(matches!(
        project
            .commit_document_layer_transaction(transaction)
            .unwrap(),
        DocumentEditOutcome::Changed { .. }
    ));

    let after_layer = project.document_layer("quadratics", &layer).unwrap();
    assert_eq!(
        after_layer
            .contours()
            .map(|contour| contour.id())
            .collect::<Vec<_>>(),
        contour_ids
    );
    let after_ids: HashSet<_> = after_layer
        .contours()
        .flat_map(|contour| contour.points())
        .map(|point| point.id())
        .collect();
    assert!(surviving_ids.is_subset(&after_ids));
    assert!(retired_ids.iter().any(|point| !after_ids.contains(point)));
    assert!(
        after_layer
            .contours()
            .flat_map(|contour| contour.points())
            .all(|point| point.point_type() != LayerPointType::QCurve)
    );
    assert!(
        after_layer
            .contours()
            .flat_map(|contour| contour.points())
            .any(|point| point.position().x.fract().abs() > 1e-9)
    );
    let after_paths = ordinary_paths(&project, "quadratics", &layer);
    assert_paths_equal(&before_paths, &after_paths, 1e-9);

    let after_projection = project.encode_ufo_layer("quadratics", &layer).unwrap();
    assert_eq!(after_projection.width, before_projection.width);
    assert_eq!(after_projection.height, before_projection.height);
    assert_eq!(after_projection.note, before_projection.note);
    assert_eq!(after_projection.lib, before_projection.lib);
    assert_eq!(after_projection.image, before_projection.image);
    assert_eq!(after_projection.guidelines, before_projection.guidelines);
    assert_eq!(after_projection.anchors, before_projection.anchors);
    assert_eq!(after_projection.components, before_projection.components);
    let retained_names: HashSet<_> = after_projection
        .contours
        .iter()
        .flat_map(|contour| &contour.points)
        .filter_map(|point| point.name.as_ref().map(Name::as_str))
        .collect();
    for expected in [
        "open-start",
        "open-end",
        "mixed-start",
        "mixed-cubic-a",
        "mixed-cubic-b",
        "mixed-cubic-end",
        "mixed-line",
        "mixed-quad-end",
    ] {
        assert!(
            retained_names.contains(expected),
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
        project.encode_ufo_layer("quadratics", &layer),
        Some(before_projection)
    );
    assert!(matches!(
        project
            .replay_document_layer_history(&address, HistoryDirection::Redo)
            .unwrap(),
        DocumentHistoryReplayOutcome::Changed { .. }
    ));
    assert_eq!(
        project.encode_ufo_layer("quadratics", &layer),
        Some(after_projection.clone())
    );

    project.save().unwrap();
    let reloaded = Project::load(&source_path).unwrap();
    let reloaded_layer = default_layer(&reloaded);
    assert_eq!(
        reloaded.encode_ufo_layer("quadratics", &reloaded_layer),
        Some(after_projection)
    );
}

fn cubic_fixture(name: &str, exact_quadratic: bool) -> Glyph {
    let start = kurbo::Point::new(0.125, 0.375);
    let end = kurbo::Point::new(180.875, 10.625);
    let (first, second) = if exact_quadratic {
        let control = kurbo::Point::new(75.25, 140.5);
        (
            start + (control - start) * (2.0 / 3.0),
            end + (control - end) * (2.0 / 3.0),
        )
    } else {
        (
            kurbo::Point::new(0.25, 500.75),
            kurbo::Point::new(180.625, -490.25),
        )
    };
    let mut glyph = Glyph::new(name);
    glyph.width = 500.625;
    glyph.contours.push(contour(
        vec![
            point(start.x, start.y, PointType::Move, "cubic-start"),
            point(first.x, first.y, PointType::OffCurve, "cubic-control-a"),
            point(second.x, second.y, PointType::OffCurve, "cubic-control-b"),
            point(end.x, end.y, PointType::Curve, "cubic-end"),
            point(240.25, 20.75, PointType::Line, "untouched-line"),
        ],
        "cubic",
    ));
    glyph
}

#[test]
fn cubic_to_quadratic_validates_bounds_and_preserves_surviving_points() {
    let scratch = Scratch::new();
    let source_path = scratch.0.join("Cubics.ufo");
    let mut font = Font::new();
    font.default_layer_mut()
        .insert_glyph(cubic_fixture("exact", true));
    font.default_layer_mut()
        .insert_glyph(cubic_fixture("bounded", false));
    let mut project = project_from_font(font, &source_path);
    let layer = default_layer(&project);
    let before_path = ordinary_paths(&project, "exact", &layer);
    let before = project.document_layer("exact", &layer).unwrap();
    let surviving_ids: HashSet<_> = before
        .contours()
        .flat_map(|contour| contour.points())
        .filter(|point| point.point_type() != LayerPointType::OffCurve)
        .map(|point| point.id())
        .collect();

    project
        .edit_document_layer("exact", &layer, |draft| {
            assert!(draft.convert_cubics_to_quadratics(0.001)?);
            Ok(())
        })
        .unwrap();
    let after = project.document_layer("exact", &layer).unwrap();
    let after_ids: HashSet<_> = after
        .contours()
        .flat_map(|contour| contour.points())
        .map(|point| point.id())
        .collect();
    assert!(surviving_ids.is_subset(&after_ids));
    assert!(
        after
            .contours()
            .flat_map(|contour| contour.points())
            .all(|point| point.point_type() != LayerPointType::Curve)
    );
    assert_paths_equal(
        &before_path,
        &ordinary_paths(&project, "exact", &layer),
        1e-9,
    );

    for tolerance in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let snapshot = project.document_snapshot();
        let revision = project.document_revision();
        assert_eq!(
            project
                .edit_document_layer("bounded", &layer, |draft| {
                    assert!(draft.convert_cubics_to_quadratics(tolerance).is_err());
                    Ok(())
                })
                .unwrap(),
            DocumentEditOutcome::Unchanged { revision }
        );
        assert_eq!(project.document_snapshot(), snapshot);
    }
    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer("bounded", &layer, |draft| {
                assert_eq!(
                    draft.convert_cubics_to_quadratics(f64::MIN_POSITIVE),
                    Err(DocumentEditError::Rejected)
                );
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);
}

fn hyper_contour(points: Vec<ContourPoint>, label: &str) -> Contour {
    let mut contour = Contour::new(
        points,
        Some(norad::Identifier::new(&format!("hyperbezier-{label}")).unwrap()),
    );
    contour.replace_lib(object_lib(label));
    contour
}

#[test]
fn hyper_conversion_targets_stable_selections_and_preserves_source_metadata() {
    let scratch = Scratch::new();
    let source_path = scratch.0.join("Hyper.ufo");
    let mut glyph = Glyph::new("hyper");
    glyph.width = 700.125;
    glyph.note = Some("keep hyper layer metadata".into());
    glyph.contours.push(hyper_contour(
        vec![
            point(0.125, 0.25, PointType::Move, "open-hyper-start"),
            point(80.5, 120.75, PointType::Curve, "open-hyper-middle"),
            point(180.875, 10.5, PointType::Curve, "open-hyper-end"),
        ],
        "open",
    ));
    glyph.contours.push(hyper_contour(
        vec![
            point(300.25, 0.5, PointType::Curve, "closed-hyper-a"),
            point(380.75, 120.125, PointType::Curve, "closed-hyper-b"),
            point(470.5, 10.875, PointType::Line, "closed-hyper-corner"),
        ],
        "closed",
    ));
    glyph.contours.push(contour(
        vec![
            point(550.0, 0.0, PointType::Move, "ordinary-a"),
            point(650.0, 0.0, PointType::Line, "ordinary-b"),
        ],
        "ordinary",
    ));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let mut project = project_from_font(font, &source_path);
    let layer = default_layer(&project);
    let before_projection = project.encode_ufo_layer("hyper", &layer).unwrap();
    let before = project.document_layer("hyper", &layer).unwrap();
    let contours: Vec<_> = before.contours().collect();
    let open_id = contours[0].id();
    let closed_id = contours[1].id();
    let ordinary_id = contours[2].id();
    let open_point = contours[0].points().nth(1).unwrap().id();
    let open_ids: HashSet<_> = contours[0].points().map(|point| point.id()).collect();
    let closed_ids: HashSet<_> = contours[1].points().map(|point| point.id()).collect();
    let open_shape =
        runebender::outline::path::Path::from_document_contour(contours[0]).to_bezpath();

    project
        .edit_document_layer("hyper", &layer, |draft| {
            assert!(draft.convert_hyperbeziers_to_cubics(&[open_point], &[])?);
            Ok(())
        })
        .unwrap();
    let after_open = project.document_layer("hyper", &layer).unwrap();
    let contours: Vec<_> = after_open.contours().collect();
    assert_eq!(
        contours
            .iter()
            .map(|contour| contour.id())
            .collect::<Vec<_>>(),
        [open_id, closed_id, ordinary_id]
    );
    assert!(!contours[0].is_hyper());
    assert!(contours[1].is_hyper());
    assert!(!contours[2].is_hyper());
    assert!(!contours[0].is_closed());
    let converted_open_ids: HashSet<_> = contours[0].points().map(|point| point.id()).collect();
    assert!(open_ids.is_subset(&converted_open_ids));
    let converted_open = runebender::outline::glyph_paths::ordinary_contour_to_bezpath(contours[0]);
    assert_paths_equal(&[open_shape], &[converted_open], 1e-9);

    project
        .edit_document_layer("hyper", &layer, |draft| {
            assert!(draft.convert_hyperbeziers_to_cubics(&[], &[closed_id])?);
            Ok(())
        })
        .unwrap();
    let after = project.document_layer("hyper", &layer).unwrap();
    let contours: Vec<_> = after.contours().collect();
    assert!(!contours[1].is_hyper());
    assert!(contours[1].is_closed());
    let converted_closed_ids: HashSet<_> = contours[1].points().map(|point| point.id()).collect();
    assert!(closed_ids.is_subset(&converted_closed_ids));
    let projected = project.encode_ufo_layer("hyper", &layer).unwrap();
    assert_eq!(projected.width, before_projection.width);
    assert_eq!(projected.note, before_projection.note);
    for index in 0..2 {
        assert_eq!(
            projected.contours[index].lib(),
            before_projection.contours[index].lib()
        );
        assert!(
            projected.contours[index]
                .identifier()
                .is_some_and(|identifier| !identifier.as_ref().contains("hyper"))
        );
    }
    assert_eq!(projected.contours[2], before_projection.contours[2]);
    let names: HashSet<_> = projected
        .contours
        .iter()
        .flat_map(|contour| &contour.points)
        .filter_map(|point| point.name.as_ref().map(Name::as_str))
        .collect();
    for expected in [
        "open-hyper-start",
        "open-hyper-middle",
        "open-hyper-end",
        "closed-hyper-a",
        "closed-hyper-b",
        "closed-hyper-corner",
    ] {
        assert!(names.contains(expected));
    }

    project.save().unwrap();
    let reloaded = Project::load(&source_path).unwrap();
    let reloaded_layer = default_layer(&reloaded);
    let reloaded = reloaded.document_layer("hyper", &reloaded_layer).unwrap();
    assert!(
        reloaded
            .contours()
            .take(2)
            .all(|contour| !contour.is_hyper())
    );
}

#[test]
fn conversion_noops_preserve_revision_and_redo() {
    let scratch = Scratch::new();
    let source_path = scratch.0.join("Lines.ufo");
    let mut glyph = Glyph::new("lines");
    glyph.contours.push(contour(
        vec![
            point(0.0, 0.0, PointType::Move, "line-a"),
            point(100.0, 0.0, PointType::Line, "line-b"),
        ],
        "line",
    ));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let mut project = project_from_font(font, &source_path);
    let layer = default_layer(&project);
    let address = address("lines", &layer);

    let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
    assert!(transaction.draft_mut().set_width(550.25).unwrap());
    assert!(matches!(
        project
            .commit_document_layer_transaction(transaction)
            .unwrap(),
        DocumentEditOutcome::Changed { .. }
    ));
    project
        .replay_document_layer_history(&address, HistoryDirection::Undo)
        .unwrap();
    assert_eq!(
        project.document_layer_history_depth(&address, HistoryDirection::Redo),
        1
    );
    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer("lines", &layer, |draft| {
                assert!(!draft.convert_quadratics_to_cubics()?);
                assert!(!draft.convert_cubics_to_quadratics(1.0)?);
                assert!(!draft.convert_hyperbeziers_to_cubics(&[], &[])?);
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_revision(), revision);
    assert_eq!(
        project.document_layer_history_depth(&address, HistoryDirection::Redo),
        1
    );
}
