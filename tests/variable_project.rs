// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! End-to-end contracts for canonical glyph layers and UFO/Designspace projections.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use norad::{Anchor, Component, Contour, ContourPoint, Font, Glyph, Name, PointType};
use runebender::document::LayerPointType;
use runebender::document::canonical_metadata::{KerningParticipant, KerningSide};
use runebender::document::font_memory::designspace_from_str;
use runebender::document::history::{HistoryDirection, HistoryReplayError};
use runebender::document::model::glyph_metadata::OpenTypeGlyphCategory;
use runebender::document::project::{
    DocumentEditOutcome, DocumentHistoryError, DocumentHistoryReplayOutcome,
    DocumentSourceMetadataHistoryError, Master, Project,
};
use runebender::document::var_model::Location;
use runebender::document::variable::{GlyphLayerAddress, LayerId, SourceId};

const DESIGNSPACE: &str = include_str!("fixtures/variable/TwoAxes.designspace");

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "runebender-variable-{}-{}",
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

fn glyph(name: &str, x: f64) -> Glyph {
    let mut glyph = Glyph::new(name);
    glyph.width = 600.123_456_789 + x;
    glyph.height = 1000.0 + x;
    glyph.note = Some(format!("source {x}"));
    glyph.image = Some(
        norad::Image::new(
            "reference.png".into(),
            None,
            norad::AffineTransform::default(),
        )
        .unwrap(),
    );
    glyph.contours.push(Contour::new(
        vec![
            ContourPoint::new(
                x,
                0.0,
                PointType::Move,
                false,
                None,
                Some(norad::Identifier::new("start").unwrap()),
            ),
            ContourPoint::new(x + 50.0, 100.0, PointType::Line, false, None, None),
        ],
        None,
    ));
    glyph.anchors.push(Anchor::new(
        x + 100.0,
        800.0 + x,
        Some(Name::new("top").unwrap()),
        None,
        None,
    ));
    glyph
}

fn fixture() -> (Scratch, Project) {
    let scratch = Scratch::new();
    let doc = designspace_from_str(DESIGNSPACE).unwrap();
    let mut project = Project::from_designspace(doc, |filename| {
        let x = match filename {
            "Regular.ufo" => 0.0,
            "Heavy.ufo" => 100.0,
            "Wide.ufo" => 50.0,
            _ => 200.0,
        };
        let mut font = Font::new();
        font.font_info.family_name = Some("Variable Fixture".into());
        font.font_info.note = Some("preserve font metadata".into());
        font.lib
            .insert("vendor.private".into(), plist::Value::Data(vec![0, 1, 255]));
        font.features = "feature liga { sub A B by C; } liga;\n".into();
        font.groups.insert(
            Name::new("public.kern1.A").unwrap(),
            vec![Name::new("A").unwrap()],
        );
        font.kerning
            .entry(Name::new("public.kern1.A").unwrap())
            .or_default()
            .insert(Name::new("B").unwrap(), -80.5 - x);
        font.data
            .insert("private.bin".into(), vec![1, 7, 19])
            .unwrap();
        font.images
            .insert(
                "reference.png".into(),
                include_bytes!("fixtures/variable/reference.png").to_vec(),
            )
            .unwrap();
        for name in ["A", "B"] {
            font.default_layer_mut().insert_glyph(glyph(name, x));
        }
        let mut composite = glyph("C", x);
        composite.contours.clear();
        composite.components.push(Component::new(
            Name::new("B").unwrap(),
            norad::AffineTransform {
                x_offset: x,
                y_offset: x * 2.0,
                ..Default::default()
            },
            None,
        ));
        font.default_layer_mut().insert_glyph(composite);
        if filename == "Regular.ufo" {
            let layer = font.layers.new_layer("intermediate").unwrap();
            layer.lib.insert(
                "vendor.layer".into(),
                plist::Value::String("preserve".into()),
            );
            layer.insert_glyph(glyph("A", 80.0));
            font.layers
                .new_layer("sketch")
                .unwrap()
                .insert_glyph(glyph("onlySketch", 999.0));
        }
        Ok(Master::from_font(font, scratch.0.join(filename)))
    })
    .unwrap();
    project.export_source = Some(scratch.0.join("Font.designspace"));
    project.ds_dirty = true;
    (scratch, project)
}

fn object_lib(owner: &str) -> plist::Dictionary {
    let mut lib = plist::Dictionary::new();
    lib.insert("com.example.unknown".into(), owner.into());
    lib
}

fn adversarial_glyph(name: &str, width: f64, offset: f64) -> Glyph {
    let identifier = |kind: &str, index: usize| {
        norad::Identifier::new(&format!("{name}.{kind}.{index}")).unwrap()
    };
    let mut glyph = Glyph::new(name);
    glyph.width = width;
    glyph.height = 1_000.123_456_789 + offset;
    glyph.note = Some(format!("exact payload {name}"));
    glyph.lib.insert(
        "com.example.unknownGlyphData".into(),
        plist::Value::Array(vec![1_i64.into(), 2_i64.into(), 3_i64.into()]),
    );
    glyph.image = Some(
        norad::Image::new(
            "reference.png".into(),
            Some(norad::Color::new(0.1, 0.2, 0.3, 0.4).unwrap()),
            norad::AffineTransform {
                x_scale: 0.987_654_321,
                xy_scale: 0.123_456_789,
                yx_scale: -0.234_567_891,
                y_scale: 1.012_345_678,
                x_offset: 12.345_678_9 + offset,
                y_offset: -98.765_432_1,
            },
        )
        .unwrap(),
    );
    let mut guideline = norad::Guideline::new(
        norad::Line::Angle {
            x: 20.123_456_789 + offset,
            y: 30.987_654_321,
            degrees: 12.345_678_9,
        },
        Some(Name::new("slant reference").unwrap()),
        Some(norad::Color::new(0.8, 0.1, 0.2, 0.7).unwrap()),
        Some(identifier("guideline", 0)),
    );
    guideline.replace_lib(object_lib("guideline"));
    glyph.guidelines.push(guideline);

    for contour_index in 0..2 {
        let mut points = Vec::new();
        for point_index in 0..2 {
            let mut point = ContourPoint::new(
                offset + contour_index as f64 * 100.0 + point_index as f64 * 40.0,
                50.0 + point_index as f64 * 80.0,
                PointType::Line,
                point_index == 1,
                Some(Name::new(&format!("point {contour_index} {point_index}")).unwrap()),
                Some(identifier("point", contour_index * 2 + point_index)),
            );
            point.replace_lib(object_lib(&format!("point {contour_index} {point_index}")));
            points.push(point);
        }
        let mut contour = Contour::new(points, Some(identifier("contour", contour_index)));
        contour.replace_lib(object_lib(&format!("contour {contour_index}")));
        glyph.contours.push(contour);
    }
    for component_index in 0..2 {
        let reference = if name == "B" && component_index == 1 {
            "A"
        } else {
            "base"
        };
        let mut component = Component::new(
            Name::new(reference).unwrap(),
            norad::AffineTransform {
                x_scale: 1.0 + component_index as f64 * 0.125,
                xy_scale: 0.125 + component_index as f64 * 0.25,
                yx_scale: -0.25 - component_index as f64 * 0.125,
                y_scale: 0.875 + component_index as f64 * 0.0625,
                x_offset: offset + component_index as f64 * 25.123_456_789,
                y_offset: -12.987_654_321 - component_index as f64,
            },
            Some(identifier("component", component_index)),
        );
        component.replace_lib(object_lib(&format!("component {component_index}")));
        glyph.components.push(component);
    }
    for anchor_index in 0..2 {
        let mut anchor = Anchor::new(
            offset + anchor_index as f64 * 50.123_456_789,
            700.987_654_321 - anchor_index as f64,
            Some(Name::new(&format!("anchor {anchor_index}")).unwrap()),
            Some(norad::Color::new(0.1, 0.2, 0.3, 0.4).unwrap()),
            Some(identifier("anchor", anchor_index)),
        );
        anchor.replace_lib(object_lib(&format!("anchor {anchor_index}")));
        glyph.anchors.push(anchor);
    }
    glyph
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "the fixture intentionally constructs distinct f64 widths with one f32 value"
)]
fn adversarial_fixture() -> (Scratch, Project, BTreeMap<String, Font>) {
    let scratch = Scratch::new();
    let doc = designspace_from_str(DESIGNSPACE).unwrap();
    let exact_width = 600.123_456_789_f64;
    let colliding_width = exact_width + 0.000_000_001;
    assert_ne!(
        exact_width, colliding_width,
        "source widths must be distinct"
    );
    assert_eq!(
        exact_width as f32, colliding_width as f32,
        "source widths must collide after Babelfont narrowing"
    );
    let mut fonts = BTreeMap::new();
    for (filename, offset) in [
        ("Regular.ufo", 0.0),
        ("Heavy.ufo", 100.0),
        ("Wide.ufo", 50.0),
        ("HeavyWide.ufo", 200.0),
    ] {
        let mut font = Font::new();
        font.font_info.family_name = Some("Adversarial Fixture".into());
        font.lib.insert(
            "com.example.unknownFontData".into(),
            plist::Value::Array(vec![0_i64.into(), 1_i64.into(), 255_i64.into()]),
        );
        font.kerning
            .entry(Name::new("A").unwrap())
            .or_default()
            .insert(Name::new("B").unwrap(), -80.5 - offset / 10.0);
        font.images
            .insert(
                "reference.png".into(),
                include_bytes!("fixtures/variable/reference.png").to_vec(),
            )
            .unwrap();
        font.default_layer_mut()
            .insert_glyph(adversarial_glyph("A", exact_width + offset, offset));
        font.default_layer_mut().insert_glyph(adversarial_glyph(
            "B",
            colliding_width + offset,
            offset,
        ));
        font.default_layer_mut().insert_glyph(Glyph::new("base"));
        if filename == "Regular.ufo" {
            font.layers
                .new_layer("intermediate")
                .unwrap()
                .insert_glyph(adversarial_glyph("A", exact_width + 75.0, offset + 75.0));
        }
        fonts.insert(filename.into(), font);
    }
    let project = Project::from_designspace(doc, |filename| {
        Ok(Master::from_font(
            fonts.get(filename).unwrap().clone(),
            scratch.0.join(filename),
        ))
    })
    .unwrap();
    (scratch, project, fonts)
}

fn assert_adversarial_source(actual: &Font, expected: &Font) {
    assert_eq!(actual.font_info, expected.font_info, "font info changed");
    assert_eq!(actual.lib, expected.lib, "font lib changed");
    assert_eq!(actual.kerning, expected.kerning, "kerning changed");
    assert_eq!(actual.images, expected.images, "image data changed");
    for name in ["A", "B", "base"] {
        assert_eq!(
            actual.get_glyph(name),
            expected.get_glyph(name),
            "glyph {name} changed"
        );
    }
}

fn location(weight: f64, width: f64) -> Location {
    [("Weight".into(), weight), ("Width".into(), width)].into()
}

fn assert_single_source_metadata(label: &str, project: Project) {
    let source_ids = project.document_snapshot().source_ids().to_vec();
    assert_eq!(source_ids.len(), 1, "{label} lost its source identity");
    let source = project
        .document_source(source_ids[0])
        .unwrap_or_else(|| panic!("{label} omitted canonical source metadata"));
    assert!(
        source.location().is_empty(),
        "{label} has a nonempty single-source location"
    );
    assert_eq!(
        project.document_sources().count(),
        1,
        "{label} was omitted from the source iterator"
    );
}

#[test]
fn mapped_axes_sources_and_instances_use_the_same_coordinates() {
    let (_scratch, project) = fixture();
    assert_eq!(project.active, 0);
    assert_eq!(project.master_locations[0], location(0.0, 0.0));
    assert_eq!(project.master_locations[1], location(1.0, 0.0));
    assert_eq!(project.instances[0].1, location(0.5, 0.0));
    assert_eq!(project.axes[0].user.normalized_to_user(0.5), 650.0);
}

#[test]
fn single_source_constructors_expose_canonical_source_metadata() {
    let scratch = Scratch::new();
    let new_path = scratch.0.join("New.ufo");
    let new_project = Project::new_font(new_path);

    let mut direct_font = Font::new();
    direct_font.font_info.style_name = Some("Direct".into());
    direct_font
        .default_layer_mut()
        .insert_glyph(glyph("A", 0.0));
    let direct_project = Project::from_source(Master::from_font(
        direct_font.clone(),
        scratch.0.join("Direct.ufo"),
    ));

    let loaded_path = scratch.0.join("Loaded.ufo");
    direct_font.save(&loaded_path).unwrap();
    let loaded_project = Project::load(&loaded_path).unwrap();

    assert_single_source_metadata("new font", new_project);
    assert_single_source_metadata("direct source", direct_project);
    assert_single_source_metadata("loaded UFO", loaded_project);
}

#[test]
fn document_views_read_exact_canonical_layers_and_stable_source_identity() {
    let (_scratch, mut project, _fonts) = adversarial_fixture();
    let sources: Vec<_> = project.document_sources().collect();
    assert_eq!(sources.len(), 4, "all document sources must be visible");
    assert_eq!(sources[0].id(), SourceId(0), "source identity changed");
    assert_eq!(sources[0].name(), "Regular", "source name changed");
    assert!(
        sources[0].path().ends_with("Regular.ufo"),
        "source path changed"
    );
    let layer_id = sources[0].default_layer();
    assert_eq!(
        layer_id.name, "public.default",
        "default layer address changed"
    );

    let glyph = project.document_glyph("A").unwrap();
    assert_eq!(glyph.name(), "A", "glyph view returned the wrong name");
    assert_eq!(
        glyph.layer_ids().count(),
        5,
        "four source layers and the intermediate layer must be visible"
    );
    let layer = glyph.layer(&layer_id).unwrap();
    assert_eq!(
        layer.width(),
        600.123_456_789,
        "layer view must use the exact advance"
    );
    assert_eq!(
        layer.height(),
        1_000.123_456_789,
        "layer view must use the exact vertical advance"
    );
    assert_eq!(layer.note(), Some("exact payload A"), "layer note changed");
    let contours: Vec<_> = layer.contours().collect();
    assert_eq!(contours.len(), 2, "canonical contours are missing");
    assert_ne!(
        contours[0].id(),
        contours[1].id(),
        "contours must have distinct identities"
    );
    let points: Vec<_> = contours[0].points().collect();
    assert_eq!(points.len(), 2, "canonical points are missing");
    assert_eq!(
        points[0].point_type(),
        LayerPointType::Line,
        "point type changed"
    );
    assert_eq!(points[0].name(), Some("point 0 0"), "point name changed");
    assert_eq!(
        points[0].position(),
        kurbo::Point::new(0.0, 50.0),
        "point position changed"
    );
    let components: Vec<_> = layer.components().collect();
    assert_eq!(components.len(), 2, "canonical components are missing");
    let shape_kinds: Vec<_> = layer
        .shapes()
        .map(|shape| match shape {
            runebender::document::LayerShapeView::Contour(_) => "contour",
            runebender::document::LayerShapeView::Component(_) => "component",
        })
        .collect();
    assert_eq!(
        shape_kinds,
        ["contour", "contour", "component", "component"],
        "canonical shape order changed"
    );
    assert_eq!(
        components[0].reference(),
        "base",
        "component reference changed"
    );
    assert_eq!(
        components[0].transform().as_coeffs(),
        [1.0, 0.125, -0.25, 0.875, 0.0, -12.987_654_321],
        "component transform must remain exact"
    );
    let anchors: Vec<_> = layer.anchors().collect();
    assert_eq!(anchors.len(), 2, "canonical anchors are missing");
    assert_eq!(anchors[0].name(), "anchor 0", "anchor name changed");
    assert_eq!(
        anchors[0].position(),
        kurbo::Point::new(0.0, 700.987_654_321),
        "anchor position changed"
    );

    assert!(
        project.edit_layer("A", &layer_id, |glyph| {
            glyph.width += 0.000_000_001;
        }),
        "compatibility edit must change the layer"
    );
    assert_eq!(
        project.document_layer("A", &layer_id).unwrap().width(),
        600.123_456_79,
        "document reader must immediately reflect an unsaved edit"
    );
}

#[test]
fn canonical_contour_paths_match_legacy_conversion_and_keep_implied_quadratics() {
    let scratch = Scratch::new();
    let point = |x, y, typ| ContourPoint::new(x, y, typ, false, None, None);
    let mut glyph = Glyph::new("paths");
    glyph.contours.push(Contour::new(
        vec![
            point(0.0, 0.0, PointType::Line),
            point(100.0, 0.0, PointType::Line),
            point(100.0, 100.0, PointType::Line),
            point(0.0, 100.0, PointType::Line),
        ],
        None,
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(150.0, 0.0, PointType::Move),
            point(250.0, 100.0, PointType::Line),
        ],
        None,
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(300.0, 0.0, PointType::Line),
            point(350.0, 100.0, PointType::OffCurve),
            point(425.0, 125.0, PointType::OffCurve),
            point(475.0, 100.0, PointType::OffCurve),
            point(500.0, 0.0, PointType::QCurve),
        ],
        None,
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(600.0, 0.0, PointType::OffCurve),
            point(700.0, 100.0, PointType::OffCurve),
            point(800.0, 0.0, PointType::OffCurve),
        ],
        None,
    ));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph.clone());
    font.default_layer_mut().insert_glyph(Glyph::new("empty"));
    let project = Project::from_source(Master::from_font(font, scratch.0.join("Paths.ufo")));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();

    let legacy = runebender::outline::glyph_paths::contours_to_bezpath(&glyph);
    let canonical = runebender::outline::glyph_paths::ordinary_layer_contours_to_bezpath(
        project.document_layer("paths", &layer_id).unwrap(),
    );
    assert_eq!(canonical, legacy);
    assert_eq!(
        runebender::analysis::curve::ordinary_cubics_from_layer(
            project.document_layer("paths", &layer_id).unwrap(),
        ),
        runebender::analysis::curve::cubics_from_norad(&glyph),
        "canonical curve-analysis input changed cubic segments"
    );
    assert_eq!(
        canonical
            .elements()
            .iter()
            .filter(|element| matches!(element, kurbo::PathEl::QuadTo(_, _)))
            .count(),
        6,
        "explicit and implied quadratic segments were not preserved"
    );
    assert!(
        runebender::outline::glyph_paths::ordinary_layer_contours_to_bezpath(
            project.document_layer("empty", &layer_id).unwrap(),
        )
        .is_empty(),
        "empty canonical glyph produced a path"
    );

    let document_layer = project.document_layer("paths", &layer_id).unwrap();
    let canonical_segments =
        runebender::outline::segment_ops::ordinary_layer_segments(document_layer);
    let drawn_segments: Vec<_> = canonical.segments().collect();
    assert_eq!(
        canonical_segments
            .iter()
            .map(|segment| segment.seg)
            .collect::<Vec<_>>(),
        drawn_segments,
        "canonical hit-test segments diverged from the drawn path"
    );
    assert!(canonical_segments.iter().any(|segment| matches!(
        segment.end,
        runebender::outline::segment_ops::DocumentSegmentEndpoint::Implied { .. }
    )));
    let first_contour_ids: Vec<_> = document_layer
        .contours()
        .next()
        .unwrap()
        .points()
        .map(|point| point.id())
        .collect();
    let (canonical_hit, canonical_t) =
        runebender::outline::segment_ops::nearest_ordinary_layer_segment_with_t(
            document_layer,
            kurbo::Point::new(50.0, -2.0),
            5.0,
        )
        .unwrap();
    let (legacy_hit, legacy_t) = runebender::outline::segment_ops::nearest_segment_with_t(
        &glyph,
        kurbo::Point::new(50.0, -2.0),
        5.0,
    )
    .unwrap();
    assert_eq!(canonical_hit.seg, legacy_hit.seg);
    assert_eq!(canonical_hit.point_ids(), first_contour_ids[..2]);
    assert!((canonical_t - legacy_t).abs() < f64::EPSILON);
    assert!(
        runebender::outline::segment_ops::ordinary_layer_segments(
            project.document_layer("empty", &layer_id).unwrap(),
        )
        .is_empty(),
        "empty canonical glyph produced hit-test segments"
    );
}

#[test]
fn canonical_hit_testing_matches_implied_quadratic_geometry_and_identities() {
    use runebender::outline::segment_ops::DocumentSegmentEndpoint;

    let scratch = Scratch::new();
    let point = |x, y, typ| ContourPoint::new(x, y, typ, false, None, None);
    let mut glyph = Glyph::new("quadratic-hits");
    glyph.contours.push(Contour::new(
        vec![
            point(0.0, 0.0, PointType::Move),
            point(0.0, 100.0, PointType::OffCurve),
            point(100.0, 100.0, PointType::OffCurve),
            point(100.0, 0.0, PointType::QCurve),
        ],
        None,
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(200.0, 0.0, PointType::OffCurve),
            point(300.0, 100.0, PointType::OffCurve),
            point(400.0, 0.0, PointType::OffCurve),
        ],
        None,
    ));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let project =
        Project::from_source(Master::from_font(font, scratch.0.join("QuadraticHits.ufo")));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project.document_layer("quadratic-hits", &layer_id).unwrap();
    let contour_ids: Vec<Vec<_>> = layer
        .contours()
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .collect();

    let drawn = runebender::outline::glyph_paths::ordinary_layer_contours_to_bezpath(layer);
    let hits = runebender::outline::segment_ops::ordinary_layer_segments(layer);
    assert_eq!(
        hits.iter().map(|hit| hit.seg).collect::<Vec<_>>(),
        drawn.segments().collect::<Vec<_>>(),
        "hit-test geometry must exactly match the drawn quadratic path"
    );
    assert_eq!(hits.len(), 5, "quadratic segments were omitted");

    let chain_join = DocumentSegmentEndpoint::Implied {
        first_control: contour_ids[0][1],
        second_control: contour_ids[0][2],
    };
    assert_eq!(hits[0].end, chain_join);
    assert_eq!(hits[1].start, chain_join);
    assert_eq!(hits[0].controls, [contour_ids[0][1]]);
    assert_eq!(hits[1].controls, [contour_ids[0][2]]);
    let (join_hit, _) = runebender::outline::segment_ops::nearest_ordinary_layer_segment_with_t(
        layer,
        kurbo::Point::new(50.0, 100.0),
        1.0,
    )
    .expect("the implied quadratic join must be hit-testable");
    assert!(join_hit.start == chain_join || join_hit.end == chain_join);

    let all_off_curve = &hits[2..];
    assert_eq!(all_off_curve.len(), 3);
    assert!(all_off_curve.iter().all(|hit| {
        matches!(hit.start, DocumentSegmentEndpoint::Implied { .. })
            && matches!(hit.end, DocumentSegmentEndpoint::Implied { .. })
    }));
    assert_eq!(all_off_curve[0].controls, [contour_ids[1][0]]);
    assert_eq!(
        all_off_curve[0].start,
        DocumentSegmentEndpoint::Implied {
            first_control: contour_ids[1][2],
            second_control: contour_ids[1][0],
        }
    );
    assert_eq!(
        all_off_curve[0].end,
        DocumentSegmentEndpoint::Implied {
            first_control: contour_ids[1][0],
            second_control: contour_ids[1][1],
        }
    );
}

#[test]
fn canonical_component_resolution_matches_legacy_and_reports_broken_graphs() {
    let (_scratch, project) = fixture();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let source = project.source_snapshot(SourceId(0)).unwrap();
    let expected =
        runebender::outline::glyph_paths::glyph_to_bezpath(source.get_glyph("C").unwrap(), &source);
    let resolved = runebender::outline::glyph_paths::ordinary_layer_to_bezpath(
        project.document_layer("C", &layer_id).unwrap(),
        |name| project.document_layer(name, &layer_id),
    )
    .unwrap();
    assert_eq!(resolved, expected, "canonical component outline changed");

    let scratch = Scratch::new();
    let component = |name: &str| {
        Component::new(
            Name::new(name).unwrap(),
            norad::AffineTransform::default(),
            None,
        )
    };
    let mut missing = Glyph::new("missing-user");
    missing.components.push(component("absent"));
    let mut cycle_a = Glyph::new("cycle-a");
    cycle_a.components.push(component("cycle-b"));
    let mut cycle_b = Glyph::new("cycle-b");
    cycle_b.components.push(component("cycle-a"));
    let mut font = Font::new();
    for glyph in [missing, cycle_a, cycle_b] {
        font.default_layer_mut().insert_glyph(glyph);
    }
    let broken = Project::from_source(Master::from_font(font, scratch.0.join("Broken.ufo")));
    let broken_layer = broken.document_source(SourceId(0)).unwrap().default_layer();
    assert_eq!(
        runebender::outline::glyph_paths::ordinary_layer_to_bezpath(
            broken
                .document_layer("missing-user", &broken_layer)
                .unwrap(),
            |name| broken.document_layer(name, &broken_layer),
        ),
        Err(runebender::outline::glyph_paths::ComponentResolveError::Missing("absent".into()))
    );
    assert_eq!(
        runebender::outline::glyph_paths::ordinary_layer_to_bezpath(
            broken.document_layer("cycle-a", &broken_layer).unwrap(),
            |name| broken.document_layer(name, &broken_layer),
        ),
        Err(
            runebender::outline::glyph_paths::ComponentResolveError::Cycle(vec![
                "cycle-a".into(),
                "cycle-b".into(),
                "cycle-a".into(),
            ])
        )
    );
}

#[test]
fn canonical_measurement_inputs_match_legacy_geometry() {
    let (_scratch, project, _fonts) = adversarial_fixture();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project.document_layer("A", &layer_id).unwrap();
    let glyph = project.glyph_layer("A", &layer_id).unwrap();
    let paths: Vec<_> = glyph
        .contours
        .iter()
        .map(|contour| {
            runebender::outline::path::Path::from_contour(
                &runebender::outline::path::hyper_model::Contour::from_norad(contour),
            )
        })
        .collect();

    assert_eq!(
        runebender::analysis::measure::ordinary_layer_measurements(layer),
        runebender::analysis::measure::glyph_measurements(&paths),
        "canonical measurement inputs changed results"
    );
    assert_eq!(
        runebender::analysis::measure::ordinary_layer_side_bearings(layer),
        runebender::analysis::measure::side_bearings(&paths, glyph.width),
        "canonical side-bearing inputs changed results"
    );
}

#[test]
fn canonical_measurements_do_not_close_open_contours() {
    let scratch = Scratch::new();
    let contour = |first_type| {
        Contour::new(
            vec![
                ContourPoint::new(0.0, 0.0, first_type, false, None, None),
                ContourPoint::new(100.0, 0.0, PointType::Line, false, None, None),
                ContourPoint::new(100.0, 100.0, PointType::Line, false, None, None),
            ],
            None,
        )
    };
    let mut open = Glyph::new("open");
    open.contours.push(contour(PointType::Move));
    let mut closed = Glyph::new("closed");
    closed.contours.push(contour(PointType::Line));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(open);
    font.default_layer_mut().insert_glyph(closed);
    let project = Project::from_source(Master::from_font(font, scratch.0.join("Measure.ufo")));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let segment_lengths = |name| {
        runebender::analysis::measure::ordinary_layer_measurements(
            project.document_layer(name, &layer_id).unwrap(),
        )
        .into_iter()
        .filter(|measurement| {
            measurement.kind == runebender::analysis::measure::MeasureKind::Segment
        })
        .map(|measurement| measurement.length)
        .collect::<Vec<_>>()
    };

    assert_eq!(segment_lengths("open"), [100, 100]);
    assert_eq!(segment_lengths("closed"), [100, 141, 100]);
}

#[test]
fn canonical_point_roles_keep_contour_closure_coherent() {
    let scratch = Scratch::new();
    let point = |x, y, typ| ContourPoint::new(x, y, typ, false, None, None);
    let mut glyph = Glyph::new("closure");
    glyph.contours.push(Contour::new(
        vec![
            point(0.0, 0.0, PointType::Move),
            point(100.0, 0.0, PointType::Line),
            point(100.0, 100.0, PointType::Line),
        ],
        None,
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(200.0, 0.0, PointType::Line),
            point(300.0, 0.0, PointType::Line),
            point(300.0, 100.0, PointType::Line),
        ],
        None,
    ));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let mut project = Project::from_source(Master::from_font(
        font,
        scratch.0.join("ContourClosure.ufo"),
    ));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project.document_layer("closure", &layer_id).unwrap();
    let contours: Vec<_> = layer.contours().collect();
    let open_first = contours[0].points().next().unwrap().id();
    let open_second = contours[0].points().nth(1).unwrap().id();
    let closed_first = contours[1].points().next().unwrap().id();

    project
        .edit_document_layer("closure", &layer_id, |draft| {
            assert!(draft.set_point_type(open_first, LayerPointType::Line)?);
            Ok(())
        })
        .unwrap();
    assert!(
        project
            .document_layer("closure", &layer_id)
            .unwrap()
            .contours()
            .next()
            .unwrap()
            .is_closed(),
        "removing the initial move did not close the canonical contour"
    );

    project
        .edit_document_layer("closure", &layer_id, |draft| {
            assert!(draft.set_point_type(closed_first, LayerPointType::Move)?);
            Ok(())
        })
        .unwrap();
    assert!(
        !project
            .document_layer("closure", &layer_id)
            .unwrap()
            .contours()
            .nth(1)
            .unwrap()
            .is_closed(),
        "setting the initial move did not open the canonical contour"
    );
    let projected = project.glyph_layer("closure", &layer_id).unwrap();
    assert_eq!(
        runebender::outline::glyph_paths::ordinary_layer_contours_to_bezpath(
            project.document_layer("closure", &layer_id).unwrap(),
        ),
        runebender::outline::glyph_paths::contours_to_bezpath(&projected),
        "canonical and projected closure semantics diverged"
    );

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project.edit_document_layer("closure", &layer_id, |draft| {
            draft.set_point_type(open_second, LayerPointType::Move)?;
            Ok(())
        }),
        Err(runebender::document::DocumentEditError::NonInitialMove(
            open_second
        ))
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);

    assert_eq!(
        project
            .edit_document_layer("closure", &layer_id, |draft| {
                assert!(draft.set_point_type(open_first, LayerPointType::Move)?);
                assert!(draft.set_point_type(open_first, LayerPointType::Line)?);
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision },
        "change-then-restore closure edit unexpectedly committed"
    );
}

#[test]
fn canonical_pen_builds_closed_contours_with_stable_new_identities() {
    let scratch = Scratch::new();
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(Glyph::new("pen"));
    let mut project = Project::from_source(Master::from_font(font, scratch.0.join("Pen.ufo")));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();

    let mut identities = None;
    let outcome = project
        .edit_document_layer("pen", &layer_id, |draft| {
            let (contour, start) = draft.start_contour(kurbo::Point::new(0.0, 0.0))?;
            let line = draft.append_contour_segment(
                contour,
                None,
                kurbo::Point::new(100.0, 0.0),
                false,
            )?;
            let curve = draft.append_contour_segment(
                contour,
                Some([
                    kurbo::Point::new(130.0, 40.0),
                    kurbo::Point::new(130.0, 80.0),
                ]),
                kurbo::Point::new(100.0, 120.0),
                true,
            )?;
            assert!(draft.close_contour(contour, None)?.is_empty());
            let (curved_close_contour, curved_close_start) =
                draft.start_contour(kurbo::Point::new(200.0, 0.0))?;
            let curved_close_line = draft.append_contour_segment(
                curved_close_contour,
                None,
                kurbo::Point::new(300.0, 0.0),
                false,
            )?;
            let closing_controls = draft.close_contour(
                curved_close_contour,
                Some([
                    kurbo::Point::new(300.0, 100.0),
                    kurbo::Point::new(200.0, 100.0),
                ]),
            )?;
            identities = Some((
                contour,
                start,
                line,
                curve,
                curved_close_contour,
                curved_close_start,
                curved_close_line,
                closing_controls,
            ));
            Ok(())
        })
        .unwrap();
    let DocumentEditOutcome::Changed { change, .. } = outcome else {
        panic!("canonical pen edit did not commit");
    };
    assert!(change.geometry_changed());

    let (
        contour_id,
        start_id,
        line_ids,
        curve_ids,
        curved_close_contour_id,
        curved_close_start_id,
        curved_close_line_ids,
        closing_control_ids,
    ) = identities.unwrap();
    assert_eq!(line_ids.len(), 1);
    assert_eq!(curve_ids.len(), 3);
    let layer = project.document_layer("pen", &layer_id).unwrap();
    let contour = layer.contours().next().unwrap();
    assert_eq!(contour.id(), contour_id);
    assert!(contour.is_closed());
    assert_eq!(
        contour.points().map(|point| point.id()).collect::<Vec<_>>(),
        [vec![start_id], line_ids.clone(), curve_ids.clone(),].concat()
    );
    assert!(contour.points().all(|point| point.name().is_none()));
    let curved_close = layer.contours().nth(1).unwrap();
    assert_eq!(curved_close.id(), curved_close_contour_id);
    assert!(curved_close.is_closed());
    assert_eq!(closing_control_ids.len(), 2);
    assert_eq!(
        curved_close
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        [
            vec![curved_close_start_id],
            curved_close_line_ids.clone(),
            closing_control_ids,
        ]
        .concat()
    );

    let mut expected = Glyph::new("pen");
    let legacy_contour = runebender::outline::glyph_ops::start_contour(&mut expected, 0.0, 0.0);
    runebender::outline::glyph_ops::append_segment(
        &mut expected,
        legacy_contour,
        None,
        100.0,
        0.0,
        false,
    );
    runebender::outline::glyph_ops::append_segment(
        &mut expected,
        legacy_contour,
        Some(((130.0, 40.0), (130.0, 80.0))),
        100.0,
        120.0,
        true,
    );
    runebender::outline::glyph_ops::close_contour(&mut expected, legacy_contour, None);
    let curved_close_contour =
        runebender::outline::glyph_ops::start_contour(&mut expected, 200.0, 0.0);
    runebender::outline::glyph_ops::append_segment(
        &mut expected,
        curved_close_contour,
        None,
        300.0,
        0.0,
        false,
    );
    runebender::outline::glyph_ops::close_contour(
        &mut expected,
        curved_close_contour,
        Some(((300.0, 100.0), (200.0, 100.0))),
    );
    assert_eq!(
        project.glyph_layer("pen", &layer_id).unwrap().contours,
        expected.contours,
        "canonical pen output changed the existing contour contract"
    );

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project.edit_document_layer("pen", &layer_id, |draft| {
            draft.close_contour(contour_id, None)?;
            Ok(())
        }),
        Err(runebender::document::DocumentEditError::NotOpenContour(
            contour_id
        ))
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);
}

#[test]
fn canonical_shape_creation_matches_existing_geometry_with_stable_identities() {
    let scratch = Scratch::new();
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(Glyph::new("shapes"));
    let mut project = Project::from_source(Master::from_font(font, scratch.0.join("Shapes.ufo")));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let rectangle = kurbo::Rect::new(10.25, 20.75, 110.75, 220.25);
    let ellipse = kurbo::Rect::new(200.25, 30.75, 400.75, 230.25);

    let mut identities = None;
    project
        .edit_document_layer("shapes", &layer_id, |draft| {
            let rectangle_ids = draft.add_shape_contour(rectangle, false)?;
            let ellipse_ids = draft.add_shape_contour(ellipse, true)?;
            identities = Some((rectangle_ids, ellipse_ids));
            Ok(())
        })
        .unwrap();

    let ((rectangle_id, rectangle_points), (ellipse_id, ellipse_points)) = identities.unwrap();
    assert_eq!(rectangle_points.len(), 4);
    assert_eq!(ellipse_points.len(), 12);
    let layer = project.document_layer("shapes", &layer_id).unwrap();
    let contours: Vec<_> = layer.contours().collect();
    assert_eq!(contours.len(), 2);
    assert_eq!(contours[0].id(), rectangle_id);
    assert_eq!(contours[1].id(), ellipse_id);
    assert_eq!(
        contours[0]
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        rectangle_points
    );
    assert_eq!(
        contours[1]
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        ellipse_points
    );

    let mut expected = Glyph::new("shapes");
    runebender::outline::glyph_ops::add_shape_contour(&mut expected, rectangle, false);
    runebender::outline::glyph_ops::add_shape_contour(&mut expected, ellipse, true);
    assert_eq!(
        project.glyph_layer("shapes", &layer_id).unwrap().contours,
        expected.contours,
        "canonical rectangle and ellipse creation changed existing geometry"
    );

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project.edit_document_layer("shapes", &layer_id, |draft| {
            draft.add_shape_contour(kurbo::Rect::new(0.0, 0.0, f64::INFINITY, 1.0), false)?;
            Ok(())
        }),
        Err(runebender::document::DocumentEditError::NonFinite)
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);
}

#[test]
fn canonical_layer_transactions_commit_atomically_and_skip_noops() {
    let (_scratch, mut project, _fonts) = adversarial_fixture();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project.document_layer("A", &layer_id).unwrap();
    let original_width = layer.width();
    let original_height = layer.height();
    let point = layer.contours().next().unwrap().points().next().unwrap();
    let point_id = point.id();
    let original_point = point.position();
    let component_id = layer.components().next().unwrap().id();
    let anchor_id = layer.anchors().next().unwrap().id();
    let revision = project.document_revision();

    let unchanged = project
        .edit_document_layer("A", &layer_id, |draft| {
            assert!(!draft.set_width(original_width)?, "equal width changed");
            assert!(
                !draft.set_point_position(point_id, original_point)?,
                "equal point position changed"
            );
            Ok(())
        })
        .unwrap();
    assert_eq!(
        unchanged,
        DocumentEditOutcome::Unchanged { revision },
        "no-op draft must not commit"
    );
    assert_eq!(
        project.document_revision(),
        revision,
        "no-op draft advanced the revision"
    );

    let error = project
        .edit_document_layer("A", &layer_id, |draft| {
            draft.set_width(725.123_456_789)?;
            draft.set_point_position(point_id, kurbo::Point::new(f64::NAN, 10.0))?;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(
        error,
        runebender::document::DocumentEditError::NonFinite,
        "invalid draft returned the wrong error"
    );
    assert_eq!(
        project.document_revision(),
        revision,
        "failed draft advanced the revision"
    );
    assert_eq!(
        project.document_layer("A", &layer_id).unwrap().width(),
        original_width,
        "failed draft leaked an earlier mutation"
    );
    assert_eq!(
        project.document_layer("A", &layer_id).unwrap().height(),
        original_height,
        "failed draft changed an unrelated exact metric"
    );

    let transform = kurbo::Affine::new([1.25, 0.375, -0.125, 0.75, 15.5, -22.25]);
    let changed = project
        .edit_document_layer("A", &layer_id, |draft| {
            assert!(draft.set_width(725.123_456_789)?, "width did not change");
            assert!(draft.set_height(1_025.5)?, "height did not change");
            assert!(
                draft.set_point_position(point_id, kurbo::Point::new(12.5, 62.25))?,
                "point did not move"
            );
            assert!(
                draft.set_point_smooth(point_id, true)?,
                "point smooth state did not change"
            );
            assert!(
                draft.set_component_transform(component_id, transform)?,
                "component transform did not change"
            );
            assert!(
                draft.set_anchor_position(anchor_id, kurbo::Point::new(25.25, 725.75))?,
                "anchor did not move"
            );
            Ok(())
        })
        .unwrap();
    let DocumentEditOutcome::Changed {
        revision: changed_revision,
        change,
    } = changed
    else {
        panic!("changed draft reported no change");
    };
    assert_eq!(
        changed_revision,
        revision + 1,
        "changed draft reported the wrong revision"
    );
    assert_eq!(
        change.affected_layers(),
        &[GlyphLayerAddress {
            glyph: "A".into(),
            layer: layer_id.clone(),
        }],
        "transaction reported the wrong direct layer"
    );
    assert_eq!(
        change.dependent_layers().len(),
        4,
        "every B source layer referencing A must be invalidated"
    );
    assert!(
        change
            .dependent_layers()
            .iter()
            .all(|address| address.glyph == "B"),
        "dependency invalidation included an unrelated glyph"
    );
    assert!(
        change.geometry_changed(),
        "geometry change was not reported"
    );
    assert!(change.metrics_changed(), "metric change was not reported");
    assert!(
        !change.metadata_changed(),
        "geometry edit reported a metadata change"
    );
    assert!(
        change.source_metadata().is_empty(),
        "layer edit reported source metadata"
    );
    assert!(
        change.requires_compilation(),
        "compile invalidation was not reported"
    );
    let layer = project.document_layer("A", &layer_id).unwrap();
    assert_eq!(layer.width(), 725.123_456_789, "exact width changed");
    assert_eq!(layer.height(), 1_025.5, "exact height changed");
    let point = layer.contours().next().unwrap().points().next().unwrap();
    assert_eq!(
        point.position(),
        kurbo::Point::new(12.5, 62.25),
        "canonical point edit is missing"
    );
    assert!(point.is_smooth(), "canonical smooth edit is missing");
    assert_eq!(
        layer.components().next().unwrap().transform(),
        transform,
        "exact component edit is missing"
    );
    assert_eq!(
        layer.anchors().next().unwrap().position(),
        kurbo::Point::new(25.25, 725.75),
        "canonical anchor edit is missing"
    );
    let projected = project.source_snapshot(SourceId(0)).unwrap();
    let projected = projected.get_glyph("A").unwrap();
    assert_eq!(
        projected.width, 725.123_456_789,
        "compatibility projection missed the committed width"
    );
    assert_eq!(
        projected.components[0].transform,
        norad::AffineTransform {
            x_scale: 1.25,
            xy_scale: 0.375,
            yx_scale: -0.125,
            y_scale: 0.75,
            x_offset: 15.5,
            y_offset: -22.25,
        },
        "compatibility projection missed the exact transform"
    );
}

#[test]
fn canonical_selection_transform_matches_legacy_geometry_atomically() {
    let (_scratch, mut project, _fonts) = adversarial_fixture();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project.document_layer("A", &layer_id).unwrap();
    let point_ids: Vec<Vec<_>> = layer
        .contours()
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .collect();
    let selected_ids = [point_ids[0][0], point_ids[1][1]];
    let transform = kurbo::Affine::rotate(std::f64::consts::FRAC_PI_2)
        * kurbo::Affine::scale_non_uniform(-1.0, 0.5);
    let mut expected = project.glyph_layer("A", &layer_id).unwrap();
    let selected_indices = [(0, 0), (1, 1)].into_iter().collect();
    assert!(runebender::outline::glyph_ops::transform_selection(
        &mut expected,
        &selected_indices,
        transform,
    ));

    let changed = project
        .edit_document_layer("A", &layer_id, |draft| {
            assert!(draft.transform_points(&selected_ids, transform)?);
            Ok(())
        })
        .unwrap();
    assert!(matches!(changed, DocumentEditOutcome::Changed { .. }));
    assert_eq!(
        project.glyph_layer("A", &layer_id).unwrap(),
        expected,
        "canonical selection transform changed geometry"
    );

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    let missing = project
        .document_layer("B", &layer_id)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .next()
        .unwrap()
        .id();
    assert_eq!(
        project.edit_document_layer("A", &layer_id, |draft| {
            draft.transform_points(&[missing], kurbo::Affine::IDENTITY)?;
            Ok(())
        }),
        Err(runebender::document::DocumentEditError::MissingPoint(
            missing
        ))
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);
    assert_eq!(
        project.edit_document_layer("A", &layer_id, |draft| {
            draft.transform_points(
                &selected_ids,
                kurbo::Affine::new([1.0, 0.0, 0.0, 1.0, f64::NAN, 0.0]),
            )?;
            Ok(())
        }),
        Err(runebender::document::DocumentEditError::NonFinite)
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);
    assert_eq!(
        project.edit_document_layer("A", &layer_id, |draft| {
            draft.transform_points(&[], kurbo::Affine::scale(f64::MAX))?;
            Ok(())
        }),
        Err(runebender::document::DocumentEditError::NonFinite)
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);
    assert_eq!(
        project
            .edit_document_layer("A", &layer_id, |draft| {
                assert!(!draft.transform_points(&[], kurbo::Affine::IDENTITY)?);
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
}

#[test]
fn canonical_point_drag_matches_legacy_handle_behavior_atomically() {
    fn curve_glyph(name: &str) -> Glyph {
        let point = |x, y, typ, smooth| ContourPoint::new(x, y, typ, smooth, None, None);
        let mut glyph = Glyph::new(name);
        glyph.contours.push(Contour::new(
            vec![
                point(0.0, 0.0, PointType::Curve, false),
                point(20.0, 0.0, PointType::OffCurve, false),
                point(101.0, 20.0, PointType::OffCurve, false),
                point(100.0, 100.0, PointType::Curve, true),
                point(101.0, 180.0, PointType::OffCurve, false),
                point(20.0, 200.0, PointType::OffCurve, false),
                point(0.0, 200.0, PointType::Curve, false),
                point(-20.0, 100.0, PointType::OffCurve, false),
                point(-20.0, 50.0, PointType::OffCurve, false),
            ],
            None,
        ));
        glyph
    }

    let scratch = Scratch::new();
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(curve_glyph("drag"));
    font.default_layer_mut().insert_glyph(glyph("other", 0.0));
    let mut project =
        Project::from_source(Master::from_font(font, scratch.0.join("PointDrag.ufo")));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let point_ids: Vec<_> = project
        .document_layer("drag", &layer_id)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .map(|point| point.id())
        .collect();
    let selected = point_ids[3];
    let originals = project
        .document_layer("drag", &layer_id)
        .unwrap()
        .point_drag_origins(&[selected], false)
        .unwrap();
    assert_eq!(originals.len(), 3, "carried handle origins were omitted");
    let initial = project.glyph_layer("drag", &layer_id).unwrap();
    let selected_indices: HashSet<_> = [(0, 3)].into_iter().collect();
    let legacy_originals =
        runebender::outline::point_ops::drag_origins(&initial, &selected_indices, false);
    let mut expected = initial.clone();
    assert!(runebender::outline::point_ops::translate_points(
        &mut expected,
        &selected_indices,
        &legacy_originals,
        (1.0, 0.0),
        false,
    ));

    project
        .edit_document_layer("drag", &layer_id, |draft| {
            assert!(draft.translate_points(
                &[selected],
                &originals,
                kurbo::Vec2::new(1.0, 0.0),
                false,
            )?);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        project.glyph_layer("drag", &layer_id).unwrap(),
        expected,
        "canonical first drag event did not carry adjacent handles like the editor"
    );

    let mut expected = initial.clone();
    assert!(runebender::outline::point_ops::translate_points(
        &mut expected,
        &selected_indices,
        &legacy_originals,
        (2.0, 0.0),
        false,
    ));
    project
        .edit_document_layer("drag", &layer_id, |draft| {
            assert!(draft.translate_points(
                &[selected],
                &originals,
                kurbo::Vec2::new(2.0, 0.0),
                false,
            )?);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        project.glyph_layer("drag", &layer_id).unwrap(),
        expected,
        "successive canonical drag events accumulated instead of using drag-start positions"
    );
    let projected = project.glyph_layer("drag", &layer_id).unwrap();
    assert_eq!(projected.contours[0].points[2].x, 104.0);
    assert_eq!(projected.contours[0].points[3].x, 102.0);
    assert_eq!(projected.contours[0].points[4].x, 104.0);

    let selected_handle = point_ids[4];
    let handle_indices: HashSet<_> = [(0, 4)].into_iter().collect();
    let mut expected = project.glyph_layer("drag", &layer_id).unwrap();
    assert!(runebender::outline::point_ops::translate_points(
        &mut expected,
        &handle_indices,
        &HashMap::new(),
        (20.0, 20.0),
        false,
    ));
    project
        .edit_document_layer("drag", &layer_id, |draft| {
            assert!(draft.translate_points(
                &[selected_handle],
                &[],
                kurbo::Vec2::new(20.0, 20.0),
                false,
            )?);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        project.glyph_layer("drag", &layer_id).unwrap(),
        expected,
        "canonical handle drag did not preserve the smooth tangent like the editor"
    );
    assert_eq!(
        project
            .document_layer("drag", &layer_id)
            .unwrap()
            .contours()
            .next()
            .unwrap()
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        point_ids,
        "point drag replaced stable canonical identities"
    );

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    let missing = project
        .document_layer("other", &layer_id)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .next()
        .unwrap()
        .id();
    let mut overflow_origins = originals.clone();
    overflow_origins
        .iter_mut()
        .find(|(id, _)| *id == selected)
        .unwrap()
        .1 = kurbo::Point::new(f64::MAX, 0.0);
    assert_eq!(
        project
            .edit_document_layer("drag", &layer_id, |draft| {
                assert_eq!(
                    draft.translate_points(&[missing], &[], kurbo::Vec2::new(10.0, 0.0), false,),
                    Err(runebender::document::DocumentEditError::MissingPoint(
                        missing
                    ))
                );
                assert_eq!(
                    draft.translate_points(
                        &[selected],
                        &[(
                            selected,
                            kurbo::Point::new(
                                initial.contours[0].points[3].x,
                                initial.contours[0].points[3].y,
                            ),
                        )],
                        kurbo::Vec2::new(2.0, 0.0),
                        false,
                    ),
                    Err(runebender::document::DocumentEditError::MissingDragOrigin(
                        point_ids[2]
                    ))
                );
                assert_eq!(
                    draft.translate_points(
                        &[selected],
                        &overflow_origins,
                        kurbo::Vec2::new(f64::MAX, 0.0),
                        false,
                    ),
                    Err(runebender::document::DocumentEditError::NonFinite)
                );
                assert_eq!(
                    draft.translate_points(
                        &[selected],
                        &[],
                        kurbo::Vec2::new(f64::NAN, 0.0),
                        false,
                    ),
                    Err(runebender::document::DocumentEditError::NonFinite)
                );
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision },
        "caught point-drag errors leaked a partial draft mutation"
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);
}

#[test]
fn canonical_smoothing_and_sidebearing_shift_match_legacy_geometry_atomically() {
    let (_scratch, mut project, _fonts) = adversarial_fixture();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project.document_layer("A", &layer_id).unwrap();
    let point_ids: Vec<Vec<_>> = layer
        .contours()
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .collect();
    let selected = [point_ids[0][0], point_ids[1][1]];
    let selected_indices: HashSet<_> = [(0, 0), (1, 1)].into_iter().collect();
    let mut expected = project.glyph_layer("A", &layer_id).unwrap();
    assert!(runebender::outline::glyph_ops::toggle_smooth(
        &mut expected,
        &selected_indices,
    ));
    project
        .edit_document_layer("A", &layer_id, |draft| {
            assert!(draft.toggle_smooth_points(&selected)?);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        project.glyph_layer("A", &layer_id).unwrap(),
        expected,
        "canonical smooth toggles diverged from the existing editor operation"
    );

    let width = expected.width;
    let components = expected.components.clone();
    for contour in &mut expected.contours {
        for point in &mut contour.points {
            point.x += 17.25;
        }
    }
    for anchor in &mut expected.anchors {
        anchor.x += 17.25;
    }
    project
        .edit_document_layer("A", &layer_id, |draft| {
            assert!(draft.shift_points_and_anchors_x(17.25)?);
            Ok(())
        })
        .unwrap();
    let projected = project.glyph_layer("A", &layer_id).unwrap();
    assert_eq!(
        projected, expected,
        "canonical left-sidebearing shift changed the wrong geometry"
    );
    assert_eq!(projected.width, width, "sidebearing shift changed advance");
    assert_eq!(
        projected.components, components,
        "sidebearing shift changed component transforms"
    );

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    let missing = project
        .document_layer("B", &layer_id)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .next()
        .unwrap()
        .id();
    let last = point_ids[1][1];
    let last_position = project
        .document_layer("A", &layer_id)
        .unwrap()
        .contours()
        .nth(1)
        .unwrap()
        .points()
        .nth(1)
        .unwrap()
        .position();
    assert_eq!(
        project
            .edit_document_layer("A", &layer_id, |draft| {
                assert_eq!(
                    draft.toggle_smooth_points(&[selected[0], missing]),
                    Err(runebender::document::DocumentEditError::MissingPoint(
                        missing
                    ))
                );
                assert!(
                    draft.set_point_position(last, kurbo::Point::new(f64::MAX, last_position.y),)?
                );
                assert_eq!(
                    draft.shift_points_and_anchors_x(f64::MAX),
                    Err(runebender::document::DocumentEditError::NonFinite)
                );
                assert!(draft.set_point_position(last, last_position)?);
                assert_eq!(
                    draft.shift_points_and_anchors_x(f64::NAN),
                    Err(runebender::document::DocumentEditError::NonFinite)
                );
                assert!(!draft.toggle_smooth_points(&[])?);
                assert!(!draft.shift_points_and_anchors_x(0.0)?);
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision },
        "caught smoothing or sidebearing errors leaked a partial mutation"
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);
}

#[test]
fn canonical_line_segments_convert_with_stable_endpoint_identity() {
    let (_scratch, mut project, _fonts) = adversarial_fixture();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let points: Vec<_> = project
        .document_layer("A", &layer_id)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .map(|point| point.id())
        .collect();
    let [first, second] = [points[0], points[1]];
    let mut expected = project.glyph_layer("A", &layer_id).unwrap();
    let forward = runebender::outline::segment_ops::segments(&expected)
        .into_iter()
        .find(|hit| hit.contour == 0 && hit.start == 0 && hit.end == 1)
        .unwrap();
    runebender::outline::segment_ops::convert_line_to_curve(&mut expected, &forward).unwrap();

    let mut new_controls = None;
    let outcome = project
        .edit_document_layer("A", &layer_id, |draft| {
            new_controls = Some(draft.convert_line_to_curve(first, second)?);
            Ok(())
        })
        .unwrap();
    let DocumentEditOutcome::Changed { .. } = outcome else {
        panic!("line conversion reported no canonical change");
    };
    assert_eq!(
        project.glyph_layer("A", &layer_id).unwrap(),
        expected,
        "canonical forward line conversion diverged from the editor operation"
    );
    let converted_ids: Vec<_> = project
        .document_layer("A", &layer_id)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .map(|point| point.id())
        .collect();
    assert_eq!(converted_ids[0], first);
    assert_eq!(converted_ids[3], second);
    assert_eq!(new_controls.unwrap(), [converted_ids[1], converted_ids[2]]);

    let closing = runebender::outline::segment_ops::segments(&expected)
        .into_iter()
        .find(|hit| hit.contour == 0 && hit.start == 3 && hit.end == 0)
        .unwrap();
    runebender::outline::segment_ops::convert_line_to_curve(&mut expected, &closing).unwrap();
    project
        .edit_document_layer("A", &layer_id, |draft| {
            draft.convert_line_to_curve(second, first)?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        project.glyph_layer("A", &layer_id).unwrap(),
        expected,
        "canonical closing-line conversion changed wraparound ordering"
    );
    let final_ids: Vec<_> = project
        .document_layer("A", &layer_id)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .map(|point| point.id())
        .collect();
    assert_eq!(final_ids[0], first);
    assert_eq!(final_ids[3], second);
    assert_eq!(&final_ids[1..3], &converted_ids[1..3]);

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    let missing = project
        .document_layer("B", &layer_id)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .next()
        .unwrap()
        .id();
    assert_eq!(
        project
            .edit_document_layer("A", &layer_id, |draft| {
                assert_eq!(
                    draft.convert_line_to_curve(first, second),
                    Err(runebender::document::DocumentEditError::NotLineSegment(
                        first, second
                    ))
                );
                assert_eq!(
                    draft.convert_line_to_curve(first, missing),
                    Err(runebender::document::DocumentEditError::MissingPoint(
                        missing
                    ))
                );
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision },
        "caught segment-conversion errors changed the canonical document"
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);
}

#[test]
fn canonical_line_conversion_sets_quadratic_endpoints_to_cubic() {
    let scratch = Scratch::new();
    let point = |x, y, typ, name: Option<&str>| {
        ContourPoint::new(
            x,
            y,
            typ,
            false,
            name.map(|name| Name::new(name).unwrap()),
            None,
        )
    };
    let mut glyph = Glyph::new("line-kinds");
    glyph.contours.push(Contour::new(
        vec![
            point(0.0, 0.0, PointType::Move, None),
            point(90.0, 0.0, PointType::QCurve, Some("open endpoint")),
        ],
        None,
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(200.0, 0.0, PointType::QCurve, Some("closing endpoint")),
            point(290.0, 0.0, PointType::Line, None),
        ],
        None,
    ));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let mut project =
        Project::from_source(Master::from_font(font, scratch.0.join("LineKinds.ufo")));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let ids: Vec<Vec<_>> = project
        .document_layer("line-kinds", &layer_id)
        .unwrap()
        .contours()
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .collect();

    project
        .edit_document_layer("line-kinds", &layer_id, |draft| {
            draft.convert_line_to_curve(ids[0][0], ids[0][1])?;
            draft.convert_line_to_curve(ids[1][1], ids[1][0])?;
            Ok(())
        })
        .unwrap();

    let layer = project.document_layer("line-kinds", &layer_id).unwrap();
    let contours: Vec<_> = layer.contours().collect();
    let open_points: Vec<_> = contours[0].points().collect();
    assert_eq!(open_points[0].id(), ids[0][0]);
    assert_eq!(open_points[3].id(), ids[0][1]);
    assert_eq!(open_points[3].point_type(), LayerPointType::Curve);
    assert_eq!(open_points[3].name(), Some("open endpoint"));
    let open_segments: Vec<_> =
        runebender::outline::glyph_paths::ordinary_contour_to_bezpath(contours[0])
            .segments()
            .collect();
    assert_eq!(open_segments.len(), 1);
    assert!(matches!(open_segments[0], kurbo::PathSeg::Cubic(_)));

    let closing_points: Vec<_> = contours[1].points().collect();
    assert_eq!(closing_points[0].id(), ids[1][0]);
    assert_eq!(closing_points[1].id(), ids[1][1]);
    assert_eq!(closing_points[0].point_type(), LayerPointType::Curve);
    assert_eq!(closing_points[0].name(), Some("closing endpoint"));
    let closing_segments: Vec<_> =
        runebender::outline::glyph_paths::ordinary_contour_to_bezpath(contours[1])
            .segments()
            .collect();
    assert_eq!(closing_segments.len(), 2);
    assert!(matches!(closing_segments[0], kurbo::PathSeg::Line(_)));
    assert!(matches!(closing_segments[1], kurbo::PathSeg::Cubic(_)));
}

#[test]
fn canonical_segment_insertion_preserves_existing_control_identities() {
    let scratch = Scratch::new();
    let point = |x, y, typ, label: Option<&str>| {
        let mut point = ContourPoint::new(
            x,
            y,
            typ,
            false,
            label.map(|label| Name::new(label).unwrap()),
            label.map(|label| norad::Identifier::new(label).unwrap()),
        );
        if let Some(label) = label {
            point.replace_lib(object_lib(label));
        }
        point
    };
    let mut glyph = Glyph::new("insert-segments");
    glyph.contours.push(Contour::new(
        vec![
            point(0.0, 0.0, PointType::Move, None),
            point(120.0, 0.0, PointType::Line, None),
        ],
        None,
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(0.0, 100.0, PointType::Move, None),
            point(60.0, 220.0, PointType::OffCurve, Some("quadratic control")),
            point(120.0, 100.0, PointType::QCurve, None),
        ],
        None,
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(200.0, 0.0, PointType::Move, None),
            point(240.0, 120.0, PointType::OffCurve, Some("cubic first")),
            point(320.0, 120.0, PointType::OffCurve, Some("cubic second")),
            point(360.0, 0.0, PointType::Curve, None),
        ],
        None,
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(500.0, 0.0, PointType::Curve, Some("closing endpoint")),
            point(620.0, 0.0, PointType::Line, None),
            point(620.0, 120.0, PointType::OffCurve, Some("closing first")),
            point(500.0, 120.0, PointType::OffCurve, Some("closing second")),
        ],
        None,
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(-f64::MAX, 300.0, PointType::Move, None),
            point(f64::MAX, 400.0, PointType::Line, None),
        ],
        None,
    ));
    let original = glyph.clone();
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let mut project = Project::from_source(Master::from_font(
        font,
        scratch.0.join("InsertSegments.ufo"),
    ));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let ids: Vec<Vec<_>> = project
        .document_layer("insert-segments", &layer_id)
        .unwrap()
        .contours()
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .collect();

    let mut inserted = Vec::new();
    project
        .edit_document_layer("insert-segments", &layer_id, |draft| {
            inserted.push(draft.insert_point_on_segment(ids[0][0], ids[0][1], 0.5)?);
            inserted.push(draft.insert_point_on_segment(ids[1][0], ids[1][2], 0.5)?);
            inserted.push(draft.insert_point_on_segment(ids[2][0], ids[2][3], 0.5)?);
            inserted.push(draft.insert_point_on_segment(ids[3][1], ids[3][0], 0.5)?);
            Ok(())
        })
        .unwrap();

    let mut expected = original.clone();
    for (contour, start, end) in [(0, 0, 1), (1, 0, 2), (2, 0, 3), (3, 1, 0)] {
        let hit = runebender::outline::segment_ops::segments(&expected)
            .into_iter()
            .find(|hit| hit.contour == contour && hit.start == start && hit.end == end)
            .unwrap();
        runebender::outline::segment_ops::insert_point_on_segment(&mut expected, &hit, 0.5)
            .unwrap();
    }
    let projected = project.glyph_layer("insert-segments", &layer_id).unwrap();
    assert_eq!(projected.contours.len(), expected.contours.len());
    for (canonical, legacy) in projected.contours.iter().zip(&expected.contours) {
        assert_eq!(canonical.points.len(), legacy.points.len());
        for (canonical, legacy) in canonical.points.iter().zip(&legacy.points) {
            assert_eq!((canonical.x, canonical.y), (legacy.x, legacy.y));
            assert_eq!(canonical.typ, legacy.typ);
            assert_eq!(canonical.smooth, legacy.smooth);
        }
    }
    assert_eq!(
        runebender::outline::glyph_paths::contours_to_bezpath(&projected),
        runebender::outline::glyph_paths::contours_to_bezpath(&expected),
        "canonical segment subdivision changed the existing snapped geometry"
    );
    for (contour, before, after) in [
        (1, 1, 1),
        (2, 1, 1),
        (2, 2, 5),
        (3, 0, 0),
        (3, 2, 2),
        (3, 3, 6),
    ] {
        let source = &original.contours[contour].points[before];
        let split = &projected.contours[contour].points[after];
        assert_eq!(split.name, source.name);
        assert_eq!(split.identifier(), source.identifier());
        assert_eq!(split.lib(), source.lib());
    }

    let layer = project
        .document_layer("insert-segments", &layer_id)
        .unwrap();
    let points: Vec<Vec<_>> = layer
        .contours()
        .map(|contour| contour.points().collect())
        .collect();
    assert_eq!(points[1][1].id(), ids[1][1]);
    assert_eq!(points[1][1].name(), Some("quadratic control"));
    assert_eq!(points[2][1].id(), ids[2][1]);
    assert_eq!(points[2][5].id(), ids[2][2]);
    assert_eq!(points[2][1].name(), Some("cubic first"));
    assert_eq!(points[2][5].name(), Some("cubic second"));
    assert_eq!(points[3][0].id(), ids[3][0]);
    assert_eq!(points[3][2].id(), ids[3][2]);
    assert_eq!(points[3][6].id(), ids[3][3]);
    assert_eq!(points[3][0].name(), Some("closing endpoint"));
    assert_eq!(points[3][2].name(), Some("closing first"));
    assert_eq!(points[3][6].name(), Some("closing second"));
    for inserted in inserted {
        let point = points
            .iter()
            .flatten()
            .find(|point| point.id() == inserted)
            .unwrap();
        assert!(point.name().is_none());
    }
    let live_ids: Vec<_> = points.iter().flatten().map(|point| point.id()).collect();
    assert_eq!(
        live_ids.len(),
        live_ids.iter().collect::<HashSet<_>>().len(),
        "segment insertion duplicated a stable point identity"
    );

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project.edit_document_layer("insert-segments", &layer_id, |draft| {
            draft.insert_point_on_segment(ids[0][0], ids[1][0], 0.5)?;
            Ok(())
        }),
        Err(runebender::document::DocumentEditError::NotDirectSegment(
            ids[0][0], ids[1][0]
        ))
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);

    assert_eq!(
        project
            .edit_document_layer("insert-segments", &layer_id, |draft| {
                assert_eq!(
                    draft.insert_point_on_segment(ids[4][0], ids[4][1], 0.5),
                    Err(runebender::document::DocumentEditError::NonFinite)
                );
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision },
        "caught subdivision overflow committed a partial topology edit"
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);
}

#[test]
fn canonical_implied_quadratic_insertion_materializes_stable_endpoints() {
    use kurbo::ParamCurve;
    use runebender::document::DocumentSegmentEndpoint;

    let scratch = Scratch::new();
    let point = |x, y, typ, label: Option<&str>| {
        let mut point = ContourPoint::new(
            x,
            y,
            typ,
            false,
            label.map(|label| Name::new(label).unwrap()),
            label.map(|label| norad::Identifier::new(label).unwrap()),
        );
        if let Some(label) = label {
            point.replace_lib(object_lib(label));
        }
        point
    };
    let mut glyph = Glyph::new("implied-insertion");
    glyph.contours.push(Contour::new(
        vec![
            point(0.0, 0.0, PointType::Move, None),
            point(0.0, 80.0, PointType::OffCurve, Some("open first")),
            point(80.0, 80.0, PointType::OffCurve, Some("open second")),
            point(80.0, 0.0, PointType::QCurve, None),
        ],
        None,
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(200.0, 0.0, PointType::OffCurve, Some("closed first")),
            point(280.0, 80.0, PointType::OffCurve, Some("closed second")),
            point(360.0, 0.0, PointType::OffCurve, Some("closed third")),
        ],
        None,
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(-f64::MAX, 200.0, PointType::Move, None),
            point(f64::MAX, 200.0, PointType::OffCurve, None),
            point(f64::MAX, 300.0, PointType::OffCurve, None),
            point(0.0, 300.0, PointType::QCurve, None),
        ],
        None,
    ));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let mut project = Project::from_source(Master::from_font(
        font,
        scratch.0.join("ImpliedInsertion.ufo"),
    ));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project
        .document_layer("implied-insertion", &layer_id)
        .unwrap();
    let ids: Vec<Vec<_>> = layer
        .contours()
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .collect();
    let before = runebender::outline::segment_ops::ordinary_layer_segments(layer);
    let open_hit = before[0].clone();
    let closed_hit = before[2].clone();
    let overflow_hit = before[5].clone();
    assert!(matches!(
        open_hit.end,
        DocumentSegmentEndpoint::Implied { .. }
    ));
    assert!(matches!(
        closed_hit.start,
        DocumentSegmentEndpoint::Implied { .. }
    ));
    assert!(matches!(
        closed_hit.end,
        DocumentSegmentEndpoint::Implied { .. }
    ));

    let mut insertions = None;
    project
        .edit_document_layer("implied-insertion", &layer_id, |draft| {
            let open = draft.insert_point_on_quadratic_segment(
                open_hit.start,
                open_hit.controls[0],
                open_hit.end,
                0.5,
            )?;
            let closed = draft.insert_point_on_quadratic_segment(
                closed_hit.start,
                closed_hit.controls[0],
                closed_hit.end,
                0.5,
            )?;
            insertions = Some((open, closed));
            Ok(())
        })
        .unwrap();

    let (open_insertion, closed_insertion) = insertions.unwrap();
    assert_eq!(open_insertion.explicitized_start, None);
    assert!(open_insertion.explicitized_end.is_some());
    assert!(closed_insertion.explicitized_start.is_some());
    assert!(closed_insertion.explicitized_end.is_some());
    let after = runebender::outline::segment_ops::ordinary_layer_segments(
        project
            .document_layer("implied-insertion", &layer_id)
            .unwrap(),
    );
    let split = |segment: &runebender::outline::segment_ops::DocumentSegmentHit| {
        let kurbo::PathSeg::Quad(quad) = segment.seg else {
            panic!("fixture segment was not quadratic");
        };
        [
            kurbo::PathSeg::Quad(quad.subsegment(0.0..0.5)),
            kurbo::PathSeg::Quad(quad.subsegment(0.5..1.0)),
        ]
    };
    let mut expected = Vec::new();
    expected.extend(split(&open_hit));
    expected.push(before[1].seg);
    expected.extend(split(&closed_hit));
    expected.push(before[3].seg);
    expected.push(before[4].seg);
    expected.extend(before[5..].iter().map(|hit| hit.seg));
    assert_eq!(
        after.iter().map(|hit| hit.seg).collect::<Vec<_>>(),
        expected,
        "materializing implied endpoints changed quadratic geometry"
    );

    let layer = project
        .document_layer("implied-insertion", &layer_id)
        .unwrap();
    let points: Vec<Vec<_>> = layer
        .contours()
        .map(|contour| contour.points().collect())
        .collect();
    assert_eq!(points[0][1].id(), ids[0][1]);
    assert_eq!(points[0][1].name(), Some("open first"));
    assert_eq!(points[1][1].id(), ids[1][0]);
    assert_eq!(points[1][1].name(), Some("closed first"));
    let created = [
        open_insertion.point,
        open_insertion.explicitized_end.unwrap(),
        closed_insertion.point,
        closed_insertion.explicitized_start.unwrap(),
        closed_insertion.explicitized_end.unwrap(),
    ];
    for id in created {
        assert!(points.iter().flatten().any(|point| point.id() == id));
    }

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer("implied-insertion", &layer_id, |draft| {
                assert_eq!(
                    draft.insert_point_on_quadratic_segment(
                        overflow_hit.start,
                        overflow_hit.controls[0],
                        overflow_hit.end,
                        0.5,
                    ),
                    Err(runebender::document::DocumentEditError::NonFinite)
                );
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);
}

#[test]
fn canonical_implied_quadratic_insertion_rejects_stale_segment_identity() {
    let scratch = Scratch::new();
    let point = |x, y, typ| ContourPoint::new(x, y, typ, false, None, None);
    let mut glyph = Glyph::new("stale-implied-hit");
    glyph.contours.push(Contour::new(
        vec![
            point(0.0, 0.0, PointType::Move),
            point(0.0, 128.0, PointType::OffCurve),
            point(128.0, 128.0, PointType::OffCurve),
            point(128.0, 0.0, PointType::QCurve),
        ],
        None,
    ));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let mut project = Project::from_source(Master::from_font(
        font,
        scratch.0.join("StaleImpliedHit.ufo"),
    ));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project
        .document_layer("stale-implied-hit", &layer_id)
        .unwrap();
    let endpoint = layer
        .contours()
        .next()
        .unwrap()
        .points()
        .nth(3)
        .unwrap()
        .id();
    let stale = runebender::outline::segment_ops::ordinary_layer_segments(layer)
        .into_iter()
        .next()
        .unwrap();

    project
        .edit_document_layer("stale-implied-hit", &layer_id, |draft| {
            assert!(draft.set_point_type(endpoint, LayerPointType::Curve)?);
            Ok(())
        })
        .unwrap();
    let current = runebender::outline::segment_ops::ordinary_layer_segments(
        project
            .document_layer("stale-implied-hit", &layer_id)
            .unwrap(),
    );
    assert_eq!(current.len(), 1);
    assert!(matches!(current[0].seg, kurbo::PathSeg::Cubic(_)));

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer("stale-implied-hit", &layer_id, |draft| {
                assert_eq!(
                    draft.insert_point_on_quadratic_segment(
                        stale.start,
                        stale.controls[0],
                        stale.end,
                        0.5,
                    ),
                    Err(runebender::document::DocumentEditError::NotDirectSegment(
                        stale.point_ids()[0],
                        stale.point_ids()[1]
                    ))
                );
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);
}

#[test]
fn canonical_point_deletion_preserves_surviving_identities_and_metadata() {
    let scratch = Scratch::new();
    let point = |x, y, typ, label: &str| {
        let mut point = ContourPoint::new(
            x,
            y,
            typ,
            false,
            Some(Name::new(label).unwrap()),
            Some(norad::Identifier::new(label).unwrap()),
        );
        point.replace_lib(object_lib(label));
        point
    };
    let mut glyph = Glyph::new("delete-points");
    glyph.contours.push(Contour::new(
        vec![
            point(0.0, 0.0, PointType::Line, "corner a"),
            point(100.0, 0.0, PointType::Line, "corner b"),
            point(100.0, 100.0, PointType::Line, "corner c"),
            point(80.0, 130.0, PointType::OffCurve, "first control"),
            point(20.0, 130.0, PointType::OffCurve, "second control"),
            point(0.0, 100.0, PointType::Curve, "curve end"),
        ],
        Some(norad::Identifier::new("main contour").unwrap()),
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(200.0, 0.0, PointType::OffCurve, "all off a"),
            point(250.0, 100.0, PointType::OffCurve, "all off b"),
            point(300.0, 0.0, PointType::OffCurve, "all off c"),
        ],
        Some(norad::Identifier::new("all off contour").unwrap()),
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(400.0, 0.0, PointType::Move, "open start"),
            point(430.0, 80.0, PointType::OffCurve, "open first control"),
            point(470.0, 80.0, PointType::OffCurve, "open second control"),
            point(500.0, 0.0, PointType::Curve, "open curve end"),
            point(550.0, 0.0, PointType::Line, "open line end"),
        ],
        Some(norad::Identifier::new("open contour").unwrap()),
    ));
    let source = glyph.clone();
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    font.default_layer_mut().insert_glyph(Glyph::new("other"));
    let mut project =
        Project::from_source(Master::from_font(font, scratch.0.join("DeletePoints.ufo")));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project.document_layer("delete-points", &layer_id).unwrap();
    let contour_id = layer.contours().next().unwrap().id();
    let ids: Vec<Vec<_>> = layer
        .contours()
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .collect();
    let mut expected = source.clone();

    assert!(runebender::outline::glyph_ops::delete_points(
        &mut expected,
        &HashSet::from([(0, 3)])
    ));
    project
        .edit_document_layer("delete-points", &layer_id, |draft| {
            assert!(draft.delete_points(&[ids[0][3]])?);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        runebender::outline::glyph_paths::contours_to_bezpath(
            &project.glyph_layer("delete-points", &layer_id).unwrap(),
        ),
        runebender::outline::glyph_paths::contours_to_bezpath(&expected)
    );

    assert!(runebender::outline::glyph_ops::delete_points(
        &mut expected,
        &HashSet::from([(0, 1)])
    ));
    project
        .edit_document_layer("delete-points", &layer_id, |draft| {
            assert!(draft.delete_points(&[ids[0][1]])?);
            Ok(())
        })
        .unwrap();
    let projected = project.glyph_layer("delete-points", &layer_id).unwrap();
    assert_eq!(
        runebender::outline::glyph_paths::contours_to_bezpath(&projected),
        runebender::outline::glyph_paths::contours_to_bezpath(&expected)
    );
    let contour = project
        .document_layer("delete-points", &layer_id)
        .unwrap()
        .contours()
        .next()
        .unwrap();
    assert_eq!(contour.id(), contour_id);
    let surviving: Vec<_> = contour.points().collect();
    assert_eq!(
        surviving.iter().map(|point| point.id()).collect::<Vec<_>>(),
        [ids[0][0], ids[0][2], ids[0][5]]
    );
    assert_eq!(
        surviving
            .iter()
            .map(|point| point.name().unwrap())
            .collect::<Vec<_>>(),
        ["corner a", "corner c", "curve end"]
    );
    for (point, source_index) in projected.contours[0].points.iter().zip([0_usize, 2, 5]) {
        let source_point = &source.contours[0].points[source_index];
        assert_eq!(point.name, source_point.name);
        assert_eq!(point.identifier(), source_point.identifier());
        assert_eq!(point.lib(), source_point.lib());
    }

    assert!(runebender::outline::glyph_ops::delete_points(
        &mut expected,
        &HashSet::from([(1, 0), (1, 1), (1, 2)])
    ));
    project
        .edit_document_layer("delete-points", &layer_id, |draft| {
            assert!(draft.delete_points(&ids[1])?);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        project
            .document_layer("delete-points", &layer_id)
            .unwrap()
            .contours()
            .count(),
        2
    );
    assert_eq!(
        runebender::outline::glyph_paths::contours_to_bezpath(
            &project.glyph_layer("delete-points", &layer_id).unwrap(),
        ),
        runebender::outline::glyph_paths::contours_to_bezpath(&expected)
    );

    assert!(runebender::outline::glyph_ops::delete_points(
        &mut expected,
        &HashSet::from([(1, 0)])
    ));
    project
        .edit_document_layer("delete-points", &layer_id, |draft| {
            assert!(draft.delete_points(&[ids[2][0]])?);
            Ok(())
        })
        .unwrap();
    let open = project
        .document_layer("delete-points", &layer_id)
        .unwrap()
        .contours()
        .nth(1)
        .unwrap();
    assert!(!open.is_closed());
    assert_eq!(
        open.points().map(|point| point.id()).collect::<Vec<_>>(),
        [ids[2][3], ids[2][4]]
    );
    assert_eq!(
        runebender::outline::glyph_paths::contours_to_bezpath(
            &project.glyph_layer("delete-points", &layer_id).unwrap(),
        ),
        runebender::outline::glyph_paths::contours_to_bezpath(&expected)
    );

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer("delete-points", &layer_id, |draft| {
                assert_eq!(
                    draft.delete_points(&[ids[1][0]]),
                    Err(runebender::document::DocumentEditError::MissingPoint(
                        ids[1][0]
                    ))
                );
                assert!(!draft.delete_points(&[])?);
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);
}

#[test]
fn canonical_quadratic_control_deletion_preserves_neighbor_segments() {
    let scratch = Scratch::new();
    let point = |x, y, typ, label: &str| {
        let mut point = ContourPoint::new(
            x,
            y,
            typ,
            false,
            Some(Name::new(label).unwrap()),
            Some(norad::Identifier::new(label).unwrap()),
        );
        point.replace_lib(object_lib(label));
        point
    };
    let mut glyph = Glyph::new("quadratic-delete");
    glyph.contours.push(Contour::new(
        vec![
            point(0.0, 0.0, PointType::Move, "open start"),
            point(0.0, 80.0, PointType::OffCurve, "open a"),
            point(80.0, 80.0, PointType::OffCurve, "open b"),
            point(160.0, 80.0, PointType::OffCurve, "open c"),
            point(160.0, 0.0, PointType::QCurve, "open end"),
        ],
        None,
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(200.0, 0.0, PointType::OffCurve, "closed a"),
            point(280.0, 80.0, PointType::OffCurve, "closed b"),
            point(360.0, 0.0, PointType::OffCurve, "closed c"),
        ],
        None,
    ));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let mut project = Project::from_source(Master::from_font(
        font,
        scratch.0.join("QuadraticDelete.ufo"),
    ));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let ids: Vec<Vec<_>> = project
        .document_layer("quadratic-delete", &layer_id)
        .unwrap()
        .contours()
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .collect();

    project
        .edit_document_layer("quadratic-delete", &layer_id, |draft| {
            assert!(draft.delete_points(&[ids[0][2], ids[1][0]])?);
            Ok(())
        })
        .unwrap();

    let segments = runebender::outline::segment_ops::ordinary_layer_segments(
        project
            .document_layer("quadratic-delete", &layer_id)
            .unwrap(),
    );
    let expected = [
        kurbo::PathSeg::Quad(kurbo::QuadBez::new((0.0, 0.0), (0.0, 80.0), (40.0, 80.0))),
        kurbo::PathSeg::Line(kurbo::Line::new((40.0, 80.0), (120.0, 80.0))),
        kurbo::PathSeg::Quad(kurbo::QuadBez::new(
            (120.0, 80.0),
            (160.0, 80.0),
            (160.0, 0.0),
        )),
        kurbo::PathSeg::Line(kurbo::Line::new((280.0, 0.0), (240.0, 40.0))),
        kurbo::PathSeg::Quad(kurbo::QuadBez::new(
            (240.0, 40.0),
            (280.0, 80.0),
            (320.0, 40.0),
        )),
        kurbo::PathSeg::Quad(kurbo::QuadBez::new(
            (320.0, 40.0),
            (360.0, 0.0),
            (280.0, 0.0),
        )),
    ];
    assert_eq!(
        segments
            .iter()
            .map(|segment| segment.seg)
            .collect::<Vec<_>>(),
        expected,
        "quadratic control deletion changed neighboring segments"
    );

    let layer = project
        .document_layer("quadratic-delete", &layer_id)
        .unwrap();
    let points: Vec<Vec<_>> = layer
        .contours()
        .map(|contour| contour.points().collect())
        .collect();
    assert_eq!(points[0][1].id(), ids[0][1]);
    assert_eq!(points[0][4].id(), ids[0][3]);
    assert_eq!(points[0][1].name(), Some("open a"));
    assert_eq!(points[0][4].name(), Some("open c"));
    assert_eq!(points[1][2].id(), ids[1][1]);
    assert_eq!(points[1][3].id(), ids[1][2]);
    assert_eq!(points[1][2].name(), Some("closed b"));
    assert_eq!(points[1][3].name(), Some("closed c"));
    assert!(
        points
            .iter()
            .flatten()
            .filter(|point| !ids.iter().flatten().any(|id| *id == point.id()))
            .all(|point| point.name().is_none())
    );
    let live_ids: Vec<_> = points.iter().flatten().map(|point| point.id()).collect();
    assert_eq!(
        live_ids.len(),
        live_ids.iter().collect::<HashSet<_>>().len()
    );
}

#[test]
fn canonical_point_deletion_is_atomic_across_contours() {
    let point = |x, y, typ| ContourPoint::new(x, y, typ, false, None, None);
    let mut glyph = Glyph::new("atomic-delete");
    glyph.contours.push(Contour::new(
        vec![
            point(0.0, 0.0, PointType::Move),
            point(100.0, 0.0, PointType::Line),
        ],
        None,
    ));
    glyph.contours.push(Contour::new(
        vec![
            point(f64::MAX, 100.0, PointType::OffCurve),
            point(f64::MAX, 200.0, PointType::OffCurve),
            point(0.0, 100.0, PointType::OffCurve),
        ],
        None,
    ));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let mut project =
        Project::from_source(Master::from_font(font, PathBuf::from("AtomicDelete.ufo")));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let ids: Vec<Vec<_>> = project
        .document_layer("atomic-delete", &layer_id)
        .unwrap()
        .contours()
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .collect();
    let snapshot = project.document_snapshot();
    let revision = project.document_revision();

    assert_eq!(
        project
            .edit_document_layer("atomic-delete", &layer_id, |draft| {
                assert_eq!(
                    draft.delete_points(&[ids[0][1], ids[1][0]]),
                    Err(runebender::document::DocumentEditError::NonFinite)
                );
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);
}

#[test]
fn canonical_contour_reversal_preserves_identities_metadata_and_storage() {
    use kurbo::Shape as _;

    let scratch = Scratch::new();
    let point = |x, y, typ, label: &str| {
        let mut point = ContourPoint::new(
            x,
            y,
            typ,
            typ == PointType::Curve || typ == PointType::QCurve,
            Some(Name::new(label).unwrap()),
            Some(norad::Identifier::new(label).unwrap()),
        );
        point.replace_lib(object_lib(label));
        point
    };
    let mut first = Contour::new(
        vec![
            point(0.0, 0.0, PointType::Move, "open-start"),
            point(20.0, 60.0, PointType::OffCurve, "open-cubic-a"),
            point(60.0, 60.0, PointType::OffCurve, "open-cubic-b"),
            point(80.0, 0.0, PointType::Curve, "open-cubic-end"),
            point(100.0, -40.0, PointType::OffCurve, "open-quad"),
            point(120.0, 0.0, PointType::QCurve, "open-quad-end"),
            point(160.0, 0.0, PointType::Line, "open-line-end"),
        ],
        Some(norad::Identifier::new("open-contour").unwrap()),
    );
    first.replace_lib(object_lib("open-contour"));
    let mut second = Contour::new(
        vec![
            point(200.0, 0.0, PointType::Line, "closed-start"),
            point(240.0, 80.0, PointType::OffCurve, "closed-cubic-a"),
            point(300.0, 80.0, PointType::OffCurve, "closed-cubic-b"),
            point(340.0, 0.0, PointType::Curve, "closed-cubic-end"),
            point(300.0, -60.0, PointType::OffCurve, "closed-quad"),
            point(240.0, -60.0, PointType::QCurve, "closed-quad-end"),
        ],
        Some(norad::Identifier::new("closed-contour").unwrap()),
    );
    second.replace_lib(object_lib("closed-contour"));
    let mut third = Contour::new(
        vec![
            point(400.0, 0.0, PointType::OffCurve, "implied-a"),
            point(450.0, 100.0, PointType::OffCurve, "implied-b"),
            point(500.0, 0.0, PointType::OffCurve, "implied-c"),
        ],
        Some(norad::Identifier::new("implied-contour").unwrap()),
    );
    third.replace_lib(object_lib("implied-contour"));
    let mut glyph = Glyph::new("reverse-contours");
    glyph.contours = vec![first, second, third];
    let source = glyph.clone();
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let mut project = Project::from_source(Master::from_font(
        font,
        scratch.0.join("ReverseContours.ufo"),
    ));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let original = project.document_snapshot();
    let layer = project
        .document_layer("reverse-contours", &layer_id)
        .unwrap();
    let contours: Vec<_> = layer.contours().collect();
    let contour_ids: Vec<_> = contours.iter().map(|contour| contour.id()).collect();
    let point_ids: Vec<Vec<_>> = contours
        .iter()
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .collect();
    let before_paths: Vec<_> = contours
        .iter()
        .map(|contour| runebender::outline::glyph_paths::ordinary_contour_to_bezpath(*contour))
        .collect();

    project
        .edit_document_layer("reverse-contours", &layer_id, |draft| {
            assert!(draft.reverse_contours(&[point_ids[0][2], point_ids[1][1]])?);
            Ok(())
        })
        .unwrap();

    let layer = project
        .document_layer("reverse-contours", &layer_id)
        .unwrap();
    let reversed: Vec<_> = layer.contours().collect();
    assert_eq!(
        reversed
            .iter()
            .map(|contour| contour.id())
            .collect::<Vec<_>>(),
        contour_ids
    );
    assert_eq!(
        reversed[0]
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        point_ids[0].iter().copied().rev().collect::<Vec<_>>()
    );
    assert_eq!(
        reversed[1]
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        std::iter::once(point_ids[1][0])
            .chain(point_ids[1][1..].iter().copied().rev())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        reversed[2]
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        point_ids[2]
    );
    for index in 0..2 {
        assert_eq!(
            runebender::outline::glyph_paths::ordinary_contour_to_bezpath(reversed[index]),
            before_paths[index].reverse_subpaths()
        );
    }
    assert_eq!(
        runebender::outline::glyph_paths::ordinary_contour_to_bezpath(reversed[2]),
        before_paths[2]
    );
    let projected = project.glyph_layer("reverse-contours", &layer_id).unwrap();
    for contour in &projected.contours {
        let source_contour = source
            .contours
            .iter()
            .find(|candidate| candidate.identifier() == contour.identifier())
            .unwrap();
        assert_eq!(contour.lib(), source_contour.lib());
        for point in &contour.points {
            let source_point = source_contour
                .points
                .iter()
                .find(|candidate| candidate.name == point.name)
                .unwrap();
            assert_eq!(point.identifier(), source_point.identifier());
            assert_eq!(point.lib(), source_point.lib());
        }
    }

    project
        .edit_document_layer("reverse-contours", &layer_id, |draft| {
            assert!(draft.reverse_contours(&[point_ids[0][2], point_ids[1][1]])?);
            Ok(())
        })
        .unwrap();
    assert_eq!(project.document_snapshot(), original);

    project
        .edit_document_layer("reverse-contours", &layer_id, |draft| {
            assert!(draft.reverse_contours(&[])?);
            Ok(())
        })
        .unwrap();
    let reversed_all: Vec<_> = project
        .document_layer("reverse-contours", &layer_id)
        .unwrap()
        .contours()
        .collect();
    for index in 0..2 {
        assert_eq!(
            runebender::outline::glyph_paths::ordinary_contour_to_bezpath(reversed_all[index]),
            before_paths[index].reverse_subpaths()
        );
    }
    let implied = runebender::outline::glyph_paths::ordinary_contour_to_bezpath(reversed_all[2]);
    assert!((implied.area() + before_paths[2].area()).abs() < 1e-9);

    project
        .edit_document_layer("reverse-contours", &layer_id, |draft| {
            assert!(draft.delete_points(&point_ids[2])?);
            Ok(())
        })
        .unwrap();
    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer("reverse-contours", &layer_id, |draft| {
                assert_eq!(
                    draft.reverse_contours(&[point_ids[2][0]]),
                    Err(runebender::document::DocumentEditError::MissingPoint(
                        point_ids[2][0],
                    ))
                );
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);
}

#[test]
fn canonical_contour_reversal_reports_symmetric_noop() {
    let mut glyph = Glyph::new("symmetric-reversal");
    glyph.contours.push(Contour::new(
        vec![
            ContourPoint::new(0.0, 0.0, PointType::OffCurve, false, None, None),
            ContourPoint::new(100.0, 100.0, PointType::OffCurve, false, None, None),
        ],
        None,
    ));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let mut project = Project::from_source(Master::from_font(
        font,
        PathBuf::from("SymmetricReversal.ufo"),
    ));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let point_ids: Vec<_> = project
        .document_layer("symmetric-reversal", &layer_id)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .map(|point| point.id())
        .collect();
    let snapshot = project.document_snapshot();
    let revision = project.document_revision();

    assert_eq!(
        project
            .edit_document_layer("symmetric-reversal", &layer_id, |draft| {
                assert!(!draft.reverse_contours(&[point_ids[0]])?);
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);
}

#[test]
fn canonical_contour_start_reorders_without_replacing_points() {
    let scratch = Scratch::new();
    let point = |x, y, typ, label: &str| {
        let mut point = ContourPoint::new(
            x,
            y,
            typ,
            typ == PointType::Curve,
            Some(Name::new(label).unwrap()),
            Some(norad::Identifier::new(label).unwrap()),
        );
        point.replace_lib(object_lib(label));
        point
    };
    let mut closed = Contour::new(
        vec![
            point(0.0, 0.0, PointType::Line, "closed-a"),
            point(30.0, 80.0, PointType::OffCurve, "closed-control-a"),
            point(90.0, 80.0, PointType::OffCurve, "closed-control-b"),
            point(120.0, 0.0, PointType::Curve, "closed-b"),
            point(60.0, -60.0, PointType::Line, "closed-c"),
        ],
        Some(norad::Identifier::new("closed-source").unwrap()),
    );
    closed.replace_lib(object_lib("closed-source"));
    let open = Contour::new(
        vec![
            point(200.0, 0.0, PointType::Move, "open-a"),
            point(300.0, 0.0, PointType::Line, "open-b"),
        ],
        None,
    );
    let source = closed.clone();
    let mut glyph = Glyph::new("set-contour-start");
    glyph.contours = vec![closed, open];
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let mut project = Project::from_source(Master::from_font(
        font,
        scratch.0.join("SetContourStart.ufo"),
    ));
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project
        .document_layer("set-contour-start", &layer_id)
        .unwrap();
    let contours: Vec<_> = layer.contours().collect();
    let contour_id = contours[0].id();
    let ids: Vec<Vec<_>> = contours
        .iter()
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .collect();
    let before_segments: Vec<_> =
        runebender::outline::glyph_paths::ordinary_contour_to_bezpath(contours[0])
            .segments()
            .collect();

    project
        .edit_document_layer("set-contour-start", &layer_id, |draft| {
            assert!(draft.set_contour_start(ids[0][3])?);
            Ok(())
        })
        .unwrap();

    let layer = project
        .document_layer("set-contour-start", &layer_id)
        .unwrap();
    let reordered = layer.contours().next().unwrap();
    assert_eq!(reordered.id(), contour_id);
    assert_eq!(
        reordered
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        [ids[0][3], ids[0][4], ids[0][0], ids[0][1], ids[0][2]]
    );
    let mut after_segments: Vec<_> =
        runebender::outline::glyph_paths::ordinary_contour_to_bezpath(reordered)
            .segments()
            .collect();
    assert_eq!(after_segments.len(), before_segments.len());
    for segment in before_segments {
        let index = after_segments
            .iter()
            .position(|candidate| *candidate == segment)
            .expect("reordering retained every segment");
        after_segments.remove(index);
    }
    let projected = project.glyph_layer("set-contour-start", &layer_id).unwrap();
    for (point, source_index) in projected.contours[0]
        .points
        .iter()
        .zip([3_usize, 4, 0, 1, 2])
    {
        let source_point = &source.points[source_index];
        assert_eq!(point.name, source_point.name);
        assert_eq!(point.identifier(), source_point.identifier());
        assert_eq!(point.lib(), source_point.lib());
    }
    assert_eq!(projected.contours[0].identifier(), source.identifier());
    assert_eq!(projected.contours[0].lib(), source.lib());

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer("set-contour-start", &layer_id, |draft| {
                assert!(!draft.set_contour_start(ids[0][3])?);
                assert!(!draft.set_contour_start(ids[0][1])?);
                assert!(!draft.set_contour_start(ids[1][1])?);
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);
}

#[test]
fn canonical_contour_open_close_produces_persistable_topology() {
    let scratch = Scratch::new();
    let point = |x, y, typ, label: &str| {
        let mut point = ContourPoint::new(
            x,
            y,
            typ,
            typ == PointType::Curve || typ == PointType::QCurve,
            Some(Name::new(label).unwrap()),
            Some(norad::Identifier::new(label).unwrap()),
        );
        point.replace_lib(object_lib(label));
        point
    };
    let mut cubic = Contour::new(
        vec![
            point(0.0, 0.0, PointType::Line, "cubic-a"),
            point(30.0, 80.0, PointType::OffCurve, "cubic-control-a"),
            point(90.0, 80.0, PointType::OffCurve, "cubic-control-b"),
            point(120.0, 0.0, PointType::Curve, "cubic-b"),
            point(60.0, -60.0, PointType::Line, "cubic-c"),
        ],
        Some(norad::Identifier::new("cubic-toggle").unwrap()),
    );
    cubic.replace_lib(object_lib("cubic-toggle"));
    let mut open = Contour::new(
        vec![
            point(200.0, 0.0, PointType::Move, "open-a"),
            point(300.0, 0.0, PointType::Line, "open-b"),
        ],
        Some(norad::Identifier::new("open-toggle").unwrap()),
    );
    open.replace_lib(object_lib("open-toggle"));
    let mut quadratic = Contour::new(
        vec![
            point(400.0, 0.0, PointType::Line, "quadratic-a"),
            point(430.0, 80.0, PointType::OffCurve, "quadratic-control-a"),
            point(490.0, 80.0, PointType::OffCurve, "quadratic-control-b"),
            point(520.0, 0.0, PointType::QCurve, "quadratic-b"),
            point(460.0, -60.0, PointType::Line, "quadratic-c"),
        ],
        Some(norad::Identifier::new("quadratic-toggle").unwrap()),
    );
    quadratic.replace_lib(object_lib("quadratic-toggle"));
    let singleton = Contour::new(vec![point(600.0, 0.0, PointType::Line, "singleton")], None);
    let mut glyph = Glyph::new("toggle-contours");
    glyph.contours = vec![cubic, open, quadratic, singleton];
    let mut expected = glyph.clone();
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let source_path = scratch.0.join("ToggleContours.ufo");
    font.save(&source_path).unwrap();
    let mut project = Project::load(&source_path).unwrap();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project
        .document_layer("toggle-contours", &layer_id)
        .unwrap();
    let contours: Vec<_> = layer.contours().collect();
    let contour_ids: Vec<_> = contours.iter().map(|contour| contour.id()).collect();
    let ids: Vec<Vec<_>> = contours
        .iter()
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .collect();

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer("toggle-contours", &layer_id, |draft| {
                assert!(!draft.toggle_contour_open(ids[0][1])?);
                assert!(!draft.toggle_contour_open(ids[3][0])?);
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);

    for (contour, point_index) in [(0_usize, 3_usize), (2, 3)] {
        assert!(runebender::outline::cleanup::toggle_contour_open(
            &mut expected,
            contour,
            point_index
        ));
        while expected.contours[contour]
            .points
            .last()
            .is_some_and(|point| point.typ == PointType::OffCurve)
        {
            expected.contours[contour].points.pop();
        }
    }
    assert!(runebender::outline::cleanup::toggle_contour_open(
        &mut expected,
        1,
        1
    ));
    project
        .edit_document_layer("toggle-contours", &layer_id, |draft| {
            assert!(draft.toggle_contour_open(ids[0][3])?);
            assert!(draft.toggle_contour_open(ids[1][1])?);
            assert!(draft.toggle_contour_open(ids[2][3])?);
            Ok(())
        })
        .unwrap();
    let layer = project
        .document_layer("toggle-contours", &layer_id)
        .unwrap();
    let toggled: Vec<_> = layer.contours().collect();
    assert_eq!(
        toggled
            .iter()
            .map(|contour| contour.id())
            .collect::<Vec<_>>(),
        contour_ids
    );
    assert!(!toggled[0].is_closed());
    assert!(toggled[1].is_closed());
    assert!(!toggled[2].is_closed());
    for contour in [0_usize, 2] {
        assert_eq!(
            toggled[contour]
                .points()
                .map(|point| point.id())
                .collect::<Vec<_>>(),
            [ids[contour][3], ids[contour][4], ids[contour][0]]
        );
        assert_eq!(
            toggled[contour]
                .points()
                .map(|point| point.name().unwrap())
                .collect::<Vec<_>>(),
            if contour == 0 {
                vec!["cubic-b", "cubic-c", "cubic-a"]
            } else {
                vec!["quadratic-b", "quadratic-c", "quadratic-a"]
            }
        );
    }
    let projected = project.glyph_layer("toggle-contours", &layer_id).unwrap();
    assert_eq!(projected.contours, expected.contours);
    project.save().unwrap();
    let reloaded = Project::load(&source_path).unwrap();
    let reloaded_layer = reloaded
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    assert_eq!(
        reloaded
            .glyph_layer("toggle-contours", &reloaded_layer)
            .unwrap()
            .contours,
        projected.contours
    );

    for (contour, point_index) in [(0_usize, 1_usize), (1, 1), (2, 1)] {
        assert!(runebender::outline::cleanup::toggle_contour_open(
            &mut expected,
            contour,
            point_index
        ));
    }
    project
        .edit_document_layer("toggle-contours", &layer_id, |draft| {
            assert!(draft.toggle_contour_open(ids[0][0])?);
            assert!(draft.toggle_contour_open(ids[1][1])?);
            assert!(draft.toggle_contour_open(ids[2][0])?);
            Ok(())
        })
        .unwrap();
    let projected = project.glyph_layer("toggle-contours", &layer_id).unwrap();
    assert_eq!(projected.contours, expected.contours);
    project.save().unwrap();
    let reloaded = Project::load(&source_path).unwrap();
    let reloaded_layer = reloaded
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    assert_eq!(
        reloaded
            .glyph_layer("toggle-contours", &reloaded_layer)
            .unwrap()
            .contours,
        projected.contours
    );
}

#[test]
fn canonical_copy_paste_and_duplicate_assign_fresh_identities() {
    let scratch = Scratch::new();
    let point = |x, y, typ, label: &str| {
        let mut point = ContourPoint::new(
            x,
            y,
            typ,
            false,
            Some(Name::new(label).unwrap()),
            Some(norad::Identifier::new(label).unwrap()),
        );
        point.replace_lib(object_lib(label));
        point
    };
    let mut first = Contour::new(
        vec![
            point(0.0, 0.0, PointType::Line, "first-a"),
            point(100.0, 0.0, PointType::Line, "first-b"),
            point(50.0, 100.0, PointType::Line, "first-c"),
        ],
        Some(norad::Identifier::new("first-contour").unwrap()),
    );
    first.replace_lib(object_lib("first-contour"));
    let mut second = Contour::new(
        vec![
            point(200.0, 0.0, PointType::Move, "second-a"),
            point(300.0, 0.0, PointType::Line, "second-b"),
        ],
        Some(norad::Identifier::new("second-contour").unwrap()),
    );
    second.replace_lib(object_lib("second-contour"));
    let sources = [first.clone(), second.clone()];
    let mut glyph = Glyph::new("copy-contours");
    glyph.contours = vec![first, second];
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let source_path = scratch.0.join("CopyContours.ufo");
    font.save(&source_path).unwrap();
    let mut project = Project::load(&source_path).unwrap();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project.document_layer("copy-contours", &layer_id).unwrap();
    let original_contours: Vec<_> = layer.contours().collect();
    let original_contour_ids: Vec<_> = original_contours
        .iter()
        .map(|contour| contour.id())
        .collect();
    let original_point_ids: Vec<Vec<_>> = original_contours
        .iter()
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .collect();
    let copied = layer.copy_contours(&[original_point_ids[0][1]]).unwrap();
    assert_eq!(copied.len(), 1);
    assert_eq!(layer.copy_contours(&[]).unwrap().len(), 2);

    let mut pasted = None;
    project
        .edit_document_layer("copy-contours", &layer_id, |draft| {
            pasted = Some(draft.paste_contours(&copied)?);
            Ok(())
        })
        .unwrap();
    let pasted = pasted.unwrap();
    assert_eq!(pasted.contours.len(), 1);
    assert_eq!(pasted.points.len(), 3);

    let mut duplicated = None;
    project
        .edit_document_layer("copy-contours", &layer_id, |draft| {
            duplicated =
                Some(draft.duplicate_contours(
                    &[original_point_ids[1][0]],
                    kurbo::Vec2::new(20.0, 20.0),
                )?);
            Ok(())
        })
        .unwrap();
    let duplicated = duplicated.unwrap();
    assert_eq!(duplicated.contours.len(), 1);
    assert_eq!(duplicated.points.len(), 2);

    let layer = project.document_layer("copy-contours", &layer_id).unwrap();
    let contours: Vec<_> = layer.contours().collect();
    assert_eq!(contours.len(), 4);
    assert_eq!(contours[0].id(), original_contour_ids[0]);
    assert_eq!(contours[1].id(), original_contour_ids[1]);
    assert_eq!(contours[2].id(), pasted.contours[0]);
    assert_eq!(contours[3].id(), duplicated.contours[0]);
    assert_eq!(
        contours[2]
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        pasted.points
    );
    assert_eq!(
        contours[3]
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        duplicated.points
    );
    let all_ids: Vec<_> = contours
        .iter()
        .flat_map(|contour| contour.points().map(|point| point.id()))
        .collect();
    assert_eq!(all_ids.len(), all_ids.iter().collect::<HashSet<_>>().len());
    assert_eq!(
        contours[3]
            .points()
            .map(|point| point.position())
            .collect::<Vec<_>>(),
        [
            kurbo::Point::new(220.0, 20.0),
            kurbo::Point::new(320.0, 20.0)
        ]
    );

    let projected = project.glyph_layer("copy-contours", &layer_id).unwrap();
    for (output_index, source_index) in [(2_usize, 0_usize), (3, 1)] {
        let output = &projected.contours[output_index];
        let source = &sources[source_index];
        assert!(output.identifier().is_some());
        assert_ne!(output.identifier(), source.identifier());
        assert_eq!(output.lib(), source.lib());
        for (point, source_point) in output.points.iter().zip(&source.points) {
            assert_eq!(point.name, source_point.name);
            assert!(point.identifier().is_some());
            assert_ne!(point.identifier(), source_point.identifier());
            assert_eq!(point.lib(), source_point.lib());
        }
    }
    assert_ne!(
        projected.contours[2].identifier(),
        projected.contours[3].identifier()
    );
    for (point, source_point) in projected.contours[3].points.iter().zip(&sources[1].points) {
        assert_eq!(
            (point.x, point.y),
            (source_point.x + 20.0, source_point.y + 20.0)
        );
    }
    project.save().unwrap();
    let reloaded = Project::load(&source_path).unwrap();
    let reloaded_layer = reloaded
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    assert_eq!(
        reloaded
            .glyph_layer("copy-contours", &reloaded_layer)
            .unwrap()
            .contours,
        projected.contours
    );

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer("copy-contours", &layer_id, |draft| {
                assert_eq!(
                    draft.paste_contours(&[])?,
                    runebender::document::PastedContours::default()
                );
                assert_eq!(
                    draft.duplicate_contours(&[], kurbo::Vec2::new(20.0, 20.0))?,
                    runebender::document::PastedContours::default()
                );
                assert_eq!(
                    draft.duplicate_contours(
                        &[original_point_ids[0][0]],
                        kurbo::Vec2::new(f64::INFINITY, f64::INFINITY),
                    ),
                    Err(runebender::document::DocumentEditError::NonFinite)
                );
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);
}

#[test]
fn canonical_hyper_copy_duplicate_and_decomposition_retain_editable_kind() {
    let scratch = Scratch::new();
    let hyper_contour = Contour::new(
        [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
            .into_iter()
            .map(|(x, y)| ContourPoint::new(x, y, PointType::Curve, true, None, None))
            .collect(),
        Some(norad::Identifier::new("review-hyperbezier").unwrap()),
    );
    let mut copy_glyph = Glyph::new("hyper-copy");
    copy_glyph.contours.push(hyper_contour.clone());
    let mut base = Glyph::new("hyper-base");
    base.contours.push(hyper_contour);
    let mut target = Glyph::new("hyper-components");
    for x_offset in [0.0, 200.0] {
        target.components.push(Component::new(
            Name::new("hyper-base").unwrap(),
            norad::AffineTransform {
                x_offset,
                ..Default::default()
            },
            None,
        ));
    }
    let mut font = Font::new();
    for glyph in [copy_glyph, base, target] {
        font.default_layer_mut().insert_glyph(glyph);
    }
    let source_path = scratch.0.join("HyperCopies.ufo");
    font.save(&source_path).unwrap();
    let mut project = Project::load(&source_path).unwrap();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();

    let layer = project.document_layer("hyper-copy", &layer_id).unwrap();
    let original = layer.contours().next().unwrap();
    let original_point = original.points().next().unwrap().id();
    let copied = layer.copy_contours(&[]).unwrap();
    assert!(original.is_hyper());
    project
        .edit_document_layer("hyper-copy", &layer_id, |draft| {
            assert_eq!(draft.paste_contours(&copied)?.contours.len(), 1);
            assert_eq!(
                draft
                    .duplicate_contours(&[original_point], kurbo::Vec2::new(20.0, 20.0))?
                    .contours
                    .len(),
                1
            );
            Ok(())
        })
        .unwrap();
    let copied_layer = project.document_layer("hyper-copy", &layer_id).unwrap();
    assert_eq!(copied_layer.contours().count(), 3);
    for contour in copied_layer.contours() {
        assert!(contour.is_hyper());
        assert!(
            runebender::outline::path::Path::from_document_contour(contour)
                .to_bezpath()
                .elements()
                .iter()
                .any(|element| matches!(element, kurbo::PathEl::CurveTo(..)))
        );
    }
    let projected_copy = project.glyph_layer("hyper-copy", &layer_id).unwrap();
    let identifiers: Vec<_> = projected_copy
        .contours
        .iter()
        .map(|contour| contour.identifier().unwrap().as_ref().to_owned())
        .collect();
    assert_eq!(
        identifiers.len(),
        identifiers.iter().collect::<HashSet<_>>().len()
    );
    assert!(
        identifiers
            .iter()
            .all(|identifier| identifier.contains("hyper"))
    );

    let target_layer = project
        .document_layer("hyper-components", &layer_id)
        .unwrap();
    let resolved = runebender::outline::component_ops::resolved_document_component_contours(
        target_layer,
        |name| project.document_layer(name, &layer_id),
    )
    .unwrap();
    assert_eq!(resolved.len(), 2);
    project
        .edit_document_layer("hyper-components", &layer_id, |draft| {
            assert!(draft.decompose_components(&resolved)?);
            Ok(())
        })
        .unwrap();
    let target_layer = project
        .document_layer("hyper-components", &layer_id)
        .unwrap();
    assert_eq!(target_layer.contours().count(), 2);
    assert!(target_layer.contours().all(|contour| contour.is_hyper()));
    let projected_target = project.glyph_layer("hyper-components", &layer_id).unwrap();
    assert_eq!(projected_target.contours.len(), 2);
    assert_ne!(
        projected_target.contours[0].identifier(),
        projected_target.contours[1].identifier()
    );
    assert!(projected_target.contours.iter().all(|contour| {
        contour
            .identifier()
            .is_some_and(|identifier| identifier.as_ref().contains("hyper"))
    }));

    project.save().unwrap();
    let reloaded = Project::load(&source_path).unwrap();
    let reloaded_layer = reloaded
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    assert!(
        reloaded
            .document_layer("hyper-copy", &reloaded_layer)
            .unwrap()
            .contours()
            .all(|contour| contour.is_hyper())
    );
    assert!(
        reloaded
            .document_layer("hyper-components", &reloaded_layer)
            .unwrap()
            .contours()
            .all(|contour| contour.is_hyper())
    );
}

#[test]
fn canonical_filter_effects_replace_only_targeted_topology() {
    let scratch = Scratch::new();
    let cyclic_paths_equal = |first: &kurbo::BezPath, second: &kurbo::BezPath| {
        let first: Vec<_> = first.segments().collect();
        let second: Vec<_> = second.segments().collect();
        first.len() == second.len()
            && (0..first.len()).any(|offset| {
                first
                    .iter()
                    .enumerate()
                    .all(|(index, segment)| *segment == second[(index + offset) % second.len()])
            })
    };
    let point = |x, y, kind, label: &str| {
        let mut point = ContourPoint::new(
            x,
            y,
            kind,
            false,
            Some(Name::new(label).unwrap()),
            Some(norad::Identifier::new(label).unwrap()),
        );
        point.replace_lib(object_lib(label));
        point
    };
    let square = |x: f64, label: &str| {
        let mut contour = Contour::new(
            vec![
                point(x, 0.0, PointType::Line, &format!("{label}-a")),
                point(x + 100.0, 0.0, PointType::Line, &format!("{label}-b")),
                point(x + 100.0, 100.0, PointType::Line, &format!("{label}-c")),
                point(x, 100.0, PointType::Line, &format!("{label}-d")),
            ],
            Some(norad::Identifier::new(label).unwrap()),
        );
        contour.replace_lib(object_lib(label));
        contour
    };
    let mut stroke = Glyph::new("effect-stroke");
    let mut skeleton = Contour::new(
        vec![
            point(0.0, 0.0, PointType::Move, "stroke-a"),
            point(100.0, 0.0, PointType::Line, "stroke-b"),
        ],
        Some(norad::Identifier::new("stroke-source").unwrap()),
    );
    skeleton.replace_lib(object_lib("stroke-source"));
    stroke.contours = vec![skeleton, square(300.0, "stroke-untouched")];
    let mut component = Component::new(
        Name::new("effect-base").unwrap(),
        norad::AffineTransform::default(),
        Some(norad::Identifier::new("effect-component").unwrap()),
    );
    component.replace_lib(object_lib("effect-component"));
    stroke.components.push(component.clone());
    let mut anchor = Anchor::new(
        50.0,
        150.0,
        Some(Name::new("top").unwrap()),
        None,
        Some(norad::Identifier::new("effect-anchor").unwrap()),
    );
    anchor.replace_lib(object_lib("effect-anchor"));
    stroke.anchors.push(anchor.clone());
    let mut offset = Glyph::new("effect-offset");
    offset.contours.push(square(0.0, "offset-source"));
    let mut extrude = Glyph::new("effect-extrude");
    extrude.contours.push(square(0.0, "extrude-source"));
    let mut roughen = Glyph::new("effect-roughen");
    roughen.contours = vec![
        square(0.0, "roughen-source"),
        square(300.0, "roughen-untouched"),
    ];
    let mut hyper = Glyph::new("effect-hyper");
    hyper.contours.push(Contour::new(
        [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
            .into_iter()
            .map(|(x, y)| ContourPoint::new(x, y, PointType::Curve, true, None, None))
            .collect(),
        Some(norad::Identifier::new("effect-hyperbezier").unwrap()),
    ));
    let base = Glyph::new("effect-base");

    let mut expected_stroke = stroke.clone();
    assert!(runebender::outline::effects::expand_stroke_contours(
        &mut expected_stroke,
        &[0].into(),
        40.0
    ));
    let mut expected_offset = offset.clone();
    assert!(runebender::outline::effects::offset_glyph_contours(
        &mut expected_offset,
        10.0
    ));
    let mut expected_extrude = extrude.clone();
    assert!(runebender::outline::effects::extrude_glyph_contours(
        &mut expected_extrude,
        40.0,
        30.0,
        false
    ));
    let mut expected_roughen = roughen.clone();
    assert!(runebender::outline::effects::roughen_glyph_contours(
        &mut expected_roughen,
        &[0].into(),
        10.0,
        4.0,
        4.0,
        7
    ));
    let mut expected_hyper = hyper.clone();
    assert!(runebender::outline::effects::offset_glyph_contours(
        &mut expected_hyper,
        10.0
    ));

    let mut font = Font::new();
    for glyph in [stroke, offset, extrude, roughen, hyper, base] {
        font.default_layer_mut().insert_glyph(glyph);
    }
    let source_path = scratch.0.join("CanonicalEffects.ufo");
    font.save(&source_path).unwrap();
    let mut project = Project::load(&source_path).unwrap();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();

    let stroke_layer = project.document_layer("effect-stroke", &layer_id).unwrap();
    let stroke_contours: Vec<_> = stroke_layer.contours().collect();
    let stroke_target = stroke_contours[0].points().next().unwrap().id();
    let stroke_old_id = stroke_contours[0].id();
    let stroke_untouched_id = stroke_contours[1].id();
    let stroke_untouched_points: Vec<_> = stroke_contours[1]
        .points()
        .map(|point| point.id())
        .collect();
    let component_id = stroke_layer.components().next().unwrap().id();
    let anchor_id = stroke_layer.anchors().next().unwrap().id();
    project
        .edit_document_layer("effect-stroke", &layer_id, |draft| {
            assert!(draft.expand_stroke(&[stroke_target], 40.0)?);
            Ok(())
        })
        .unwrap();

    let rough_layer = project.document_layer("effect-roughen", &layer_id).unwrap();
    let rough_contours: Vec<_> = rough_layer.contours().collect();
    let rough_target = rough_contours[0].points().next().unwrap().id();
    let rough_old_id = rough_contours[0].id();
    let rough_untouched_id = rough_contours[1].id();
    project
        .edit_document_layer("effect-roughen", &layer_id, |draft| {
            assert!(draft.roughen_contours(&[rough_target], 10.0, 4.0, 4.0, 7)?);
            Ok(())
        })
        .unwrap();
    project
        .edit_document_layer("effect-offset", &layer_id, |draft| {
            assert!(draft.offset_contours(10.0)?);
            Ok(())
        })
        .unwrap();
    project
        .edit_document_layer("effect-extrude", &layer_id, |draft| {
            assert!(draft.extrude_contours(40.0, 30.0, false)?);
            Ok(())
        })
        .unwrap();
    project
        .edit_document_layer("effect-hyper", &layer_id, |draft| {
            assert!(draft.offset_contours(10.0)?);
            Ok(())
        })
        .unwrap();

    for (name, expected) in [
        ("effect-stroke", &expected_stroke),
        ("effect-offset", &expected_offset),
        ("effect-extrude", &expected_extrude),
        ("effect-roughen", &expected_roughen),
        ("effect-hyper", &expected_hyper),
    ] {
        let projected = project.glyph_layer(name, &layer_id).unwrap();
        assert_eq!(projected.contours.len(), expected.contours.len());
        for (actual, expected) in projected.contours.iter().zip(&expected.contours) {
            assert!(
                cyclic_paths_equal(
                    &runebender::outline::glyph_paths::contour_to_bezpath(actual),
                    &runebender::outline::glyph_paths::contour_to_bezpath(expected)
                ),
                "{name} geometry differs"
            );
        }
    }
    let stroke_layer = project.document_layer("effect-stroke", &layer_id).unwrap();
    let stroke_contours: Vec<_> = stroke_layer.contours().collect();
    assert!(
        stroke_contours[..stroke_contours.len() - 1]
            .iter()
            .all(|contour| contour.id() != stroke_old_id)
    );
    assert_eq!(stroke_contours.last().unwrap().id(), stroke_untouched_id);
    assert_eq!(
        stroke_contours
            .last()
            .unwrap()
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        stroke_untouched_points
    );
    assert_eq!(stroke_layer.components().next().unwrap().id(), component_id);
    assert_eq!(stroke_layer.anchors().next().unwrap().id(), anchor_id);
    let projected_stroke = project.glyph_layer("effect-stroke", &layer_id).unwrap();
    assert_eq!(projected_stroke.components, [component]);
    assert_eq!(projected_stroke.anchors, [anchor]);
    assert_eq!(
        projected_stroke.contours.last().unwrap(),
        expected_stroke.contours.last().unwrap()
    );
    assert!(
        projected_stroke.contours[..projected_stroke.contours.len() - 1]
            .iter()
            .all(|contour| {
                contour.identifier().is_none()
                    && contour.lib().is_none()
                    && contour.points.iter().all(|point| {
                        point.name.is_none()
                            && point.identifier().is_none()
                            && point.lib().is_none()
                    })
            })
    );

    let rough_layer = project.document_layer("effect-roughen", &layer_id).unwrap();
    let rough_contours: Vec<_> = rough_layer.contours().collect();
    assert_ne!(rough_contours[0].id(), rough_old_id);
    assert_eq!(rough_contours[1].id(), rough_untouched_id);
    assert!(
        project
            .document_layer("effect-hyper", &layer_id)
            .unwrap()
            .contours()
            .all(|contour| !contour.is_hyper())
    );
    for name in ["effect-offset", "effect-extrude"] {
        assert!(
            project
                .glyph_layer(name, &layer_id)
                .unwrap()
                .contours
                .iter()
                .all(|contour| contour.identifier().is_none() && contour.lib().is_none())
        );
    }

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer("effect-offset", &layer_id, |draft| {
                assert!(!draft.offset_contours(0.0)?);
                assert!(!draft.extrude_contours(0.0, 30.0, false)?);
                assert!(!draft.roughen_contours(&[], 0.5, 4.0, 4.0, 7)?);
                assert_eq!(
                    draft.expand_stroke(&[], f64::INFINITY),
                    Err(runebender::document::DocumentEditError::NonFinite)
                );
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);

    let projected: Vec<_> = [
        "effect-stroke",
        "effect-offset",
        "effect-extrude",
        "effect-roughen",
        "effect-hyper",
    ]
    .into_iter()
    .map(|name| (name, project.glyph_layer(name, &layer_id).unwrap()))
    .collect();
    project.save().unwrap();
    let reloaded = Project::load(&source_path).unwrap();
    let reloaded_layer = reloaded
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    for (name, glyph) in projected {
        assert_eq!(reloaded.glyph_layer(name, &reloaded_layer).unwrap(), glyph);
    }
}

#[test]
fn canonical_boolean_and_overlap_replacement_clear_old_topology_metadata() {
    let scratch = Scratch::new();
    let cyclic_paths_equal = |first: &kurbo::BezPath, second: &kurbo::BezPath| {
        let first: Vec<_> = first.segments().collect();
        let second: Vec<_> = second.segments().collect();
        first.len() == second.len()
            && (0..first.len()).any(|offset| {
                first
                    .iter()
                    .enumerate()
                    .all(|(index, segment)| *segment == second[(index + offset) % second.len()])
            })
    };
    let rectangle = |x0, x1, label: &str| {
        let mut contour = Contour::new(
            vec![
                ContourPoint::new(
                    x0,
                    0.0,
                    PointType::Line,
                    true,
                    Some(Name::new(&format!("{label}-a")).unwrap()),
                    Some(norad::Identifier::new(&format!("{label}-a")).unwrap()),
                ),
                ContourPoint::new(x1, 0.0, PointType::Line, false, None, None),
                ContourPoint::new(x1, 100.0, PointType::Line, false, None, None),
                ContourPoint::new(x0, 100.0, PointType::Line, false, None, None),
            ],
            Some(norad::Identifier::new(label).unwrap()),
        );
        contour.replace_lib(object_lib(label));
        contour
    };
    let mut glyph = Glyph::new("boolean-contours");
    glyph.contours = vec![
        rectangle(0.0, 100.0, "left"),
        rectangle(50.0, 150.0, "right"),
    ];
    let mut component = Component::new(
        Name::new("base").unwrap(),
        norad::AffineTransform {
            x_offset: 12.5,
            y_offset: 25.5,
            ..Default::default()
        },
        Some(norad::Identifier::new("component-source").unwrap()),
    );
    component.replace_lib(object_lib("component-source"));
    glyph.components.push(component.clone());
    let mut anchor = Anchor::new(
        75.25,
        125.75,
        Some(Name::new("top").unwrap()),
        None,
        Some(norad::Identifier::new("anchor-source").unwrap()),
    );
    anchor.replace_lib(object_lib("anchor-source"));
    glyph.anchors.push(anchor.clone());
    let mut expected = glyph.clone();
    expected.contours =
        runebender::outline::glyph_ops::boolean_contours(&glyph, linesweeper::BinaryOp::Union)
            .unwrap();
    let mut base = Glyph::new("base");
    base.contours.push(rectangle(0.0, 20.0, "base-contour"));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(base);
    font.default_layer_mut().insert_glyph(glyph);
    let source_path = scratch.0.join("BooleanContours.ufo");
    font.save(&source_path).unwrap();
    let mut project = Project::load(&source_path).unwrap();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project
        .document_layer("boolean-contours", &layer_id)
        .unwrap();
    let old_contours: Vec<_> = layer.contours().map(|contour| contour.id()).collect();
    let old_points: Vec<_> = layer
        .contours()
        .flat_map(|contour| contour.points().map(|point| point.id()))
        .collect();
    let component_id = layer.components().next().unwrap().id();
    let anchor_id = layer.anchors().next().unwrap().id();

    project
        .edit_document_layer("boolean-contours", &layer_id, |draft| {
            assert!(draft.boolean_contours(linesweeper::BinaryOp::Union)?);
            Ok(())
        })
        .unwrap();
    let layer = project
        .document_layer("boolean-contours", &layer_id)
        .unwrap();
    let new_contours: Vec<_> = layer.contours().collect();
    assert_eq!(new_contours.len(), 1);
    assert!(!old_contours.contains(&new_contours[0].id()));
    assert!(
        new_contours[0]
            .points()
            .all(|point| !old_points.contains(&point.id()))
    );
    assert_eq!(layer.components().next().unwrap().id(), component_id);
    assert_eq!(layer.anchors().next().unwrap().id(), anchor_id);
    assert!(
        new_contours[0]
            .points()
            .any(|point| point.position() == kurbo::Point::new(0.0, 0.0) && point.is_smooth())
    );
    let projected = project.glyph_layer("boolean-contours", &layer_id).unwrap();
    assert!(cyclic_paths_equal(
        &runebender::outline::glyph_paths::contours_to_bezpath(&projected),
        &runebender::outline::glyph_paths::contours_to_bezpath(&expected)
    ));
    assert_eq!(projected.components, [component.clone()]);
    assert_eq!(projected.anchors, [anchor.clone()]);
    assert!(projected.contours.iter().all(|contour| {
        contour.identifier().is_none()
            && contour.lib().is_none()
            && contour.points.iter().all(|point| {
                point.name.is_none() && point.identifier().is_none() && point.lib().is_none()
            })
    }));

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer("boolean-contours", &layer_id, |draft| {
                assert!(!draft.boolean_contours(linesweeper::BinaryOp::Difference)?);
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);

    let before_overlap = runebender::outline::glyph_paths::ordinary_layer_contours_to_bezpath(
        project
            .document_layer("boolean-contours", &layer_id)
            .unwrap(),
    );
    project
        .edit_document_layer("boolean-contours", &layer_id, |draft| {
            assert!(draft.remove_overlap()?);
            Ok(())
        })
        .unwrap();
    let layer = project
        .document_layer("boolean-contours", &layer_id)
        .unwrap();
    assert!(cyclic_paths_equal(
        &runebender::outline::glyph_paths::ordinary_layer_contours_to_bezpath(layer),
        &before_overlap
    ));
    assert_eq!(layer.components().next().unwrap().id(), component_id);
    assert_eq!(layer.anchors().next().unwrap().id(), anchor_id);
    project.save().unwrap();
    let reloaded = Project::load(&source_path).unwrap();
    let reloaded_layer = reloaded
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    assert_eq!(
        reloaded
            .glyph_layer("boolean-contours", &reloaded_layer)
            .unwrap(),
        project.glyph_layer("boolean-contours", &layer_id).unwrap()
    );
}

#[test]
fn canonical_boolean_successfully_clears_empty_results() {
    let scratch = Scratch::new();
    for (case, operation, right_x) in [
        ("intersection", linesweeper::BinaryOp::Intersection, 200.0),
        ("difference", linesweeper::BinaryOp::Difference, 0.0),
        ("xor", linesweeper::BinaryOp::Xor, 0.0),
    ] {
        let rectangle = |x: f64| {
            Contour::new(
                vec![
                    ContourPoint::new(x, 0.0, PointType::Line, false, None, None),
                    ContourPoint::new(x + 100.0, 0.0, PointType::Line, false, None, None),
                    ContourPoint::new(x + 100.0, 100.0, PointType::Line, false, None, None),
                    ContourPoint::new(x, 100.0, PointType::Line, false, None, None),
                ],
                None,
            )
        };
        let mut glyph = Glyph::new("empty-boolean");
        glyph.contours = vec![rectangle(0.0), rectangle(right_x)];
        let mut component = Component::new(
            Name::new("base").unwrap(),
            norad::AffineTransform {
                x_offset: 12.25,
                y_offset: 34.75,
                ..Default::default()
            },
            Some(norad::Identifier::new("empty-component").unwrap()),
        );
        component.replace_lib(object_lib("empty-component"));
        glyph.components.push(component.clone());
        let mut anchor = Anchor::new(
            50.5,
            150.25,
            Some(Name::new("top").unwrap()),
            None,
            Some(norad::Identifier::new("empty-anchor").unwrap()),
        );
        anchor.replace_lib(object_lib("empty-anchor"));
        glyph.anchors.push(anchor.clone());
        let mut base = Glyph::new("base");
        base.contours.push(rectangle(0.0));
        let mut font = Font::new();
        font.default_layer_mut().insert_glyph(base);
        font.default_layer_mut().insert_glyph(glyph);
        let source_path = scratch.0.join(format!("EmptyBoolean-{case}.ufo"));
        font.save(&source_path).unwrap();
        let mut project = Project::load(&source_path).unwrap();
        let layer_id = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        let layer = project.document_layer("empty-boolean", &layer_id).unwrap();
        let component_id = layer.components().next().unwrap().id();
        let anchor_id = layer.anchors().next().unwrap().id();
        let revision = project.document_revision();

        let outcome = project
            .edit_document_layer("empty-boolean", &layer_id, |draft| {
                assert!(draft.boolean_contours(operation)?);
                Ok(())
            })
            .unwrap();
        assert!(
            matches!(
                outcome,
                DocumentEditOutcome::Changed {
                    revision: next,
                    ..
                } if next == revision + 1
            ),
            "{case} did not commit its empty result"
        );
        let layer = project.document_layer("empty-boolean", &layer_id).unwrap();
        assert_eq!(layer.contours().count(), 0, "{case} retained contours");
        assert_eq!(layer.components().next().unwrap().id(), component_id);
        assert_eq!(layer.anchors().next().unwrap().id(), anchor_id);
        let projected = project.glyph_layer("empty-boolean", &layer_id).unwrap();
        assert!(projected.contours.is_empty());
        assert_eq!(projected.components, [component.clone()]);
        assert_eq!(projected.anchors, [anchor.clone()]);

        project.save().unwrap();
        let reloaded = Project::load(&source_path).unwrap();
        let reloaded_layer = reloaded
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        assert_eq!(
            reloaded
                .glyph_layer("empty-boolean", &reloaded_layer)
                .unwrap(),
            projected
        );
    }
}

#[test]
fn canonical_boolean_replacement_retains_single_cubic_loops() {
    use kurbo::Shape as _;

    let scratch = Scratch::new();
    let loop_contour = || {
        Contour::new(
            vec![
                ContourPoint::new(0.0, 0.0, PointType::Curve, false, None, None),
                ContourPoint::new(200.0, 400.0, PointType::OffCurve, false, None, None),
                ContourPoint::new(-200.0, 400.0, PointType::OffCurve, false, None, None),
            ],
            None,
        )
    };
    let rectangle = Contour::new(
        vec![
            ContourPoint::new(400.0, 0.0, PointType::Line, false, None, None),
            ContourPoint::new(500.0, 0.0, PointType::Line, false, None, None),
            ContourPoint::new(500.0, 100.0, PointType::Line, false, None, None),
            ContourPoint::new(400.0, 100.0, PointType::Line, false, None, None),
        ],
        None,
    );
    for (name, boolean) in [("loop-overlap", false), ("loop-boolean", true)] {
        let mut glyph = Glyph::new(name);
        glyph.contours.push(loop_contour());
        if boolean {
            glyph.contours.push(rectangle.clone());
        }
        let mut font = Font::new();
        font.default_layer_mut().insert_glyph(glyph);
        let source_path = scratch.0.join(format!("{name}.ufo"));
        font.save(&source_path).unwrap();
        let mut project = Project::load(&source_path).unwrap();
        let layer_id = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        let before = runebender::outline::glyph_paths::ordinary_layer_contours_to_bezpath(
            project.document_layer(name, &layer_id).unwrap(),
        );
        assert!(before.area().abs() > 100.0);
        project
            .edit_document_layer(name, &layer_id, |draft| {
                if boolean {
                    assert!(draft.boolean_contours(linesweeper::BinaryOp::Union)?);
                } else {
                    assert!(draft.remove_overlap()?);
                }
                Ok(())
            })
            .unwrap();
        let layer = project.document_layer(name, &layer_id).unwrap();
        assert_eq!(layer.contours().count(), if boolean { 2 } else { 1 });
        let after = runebender::outline::glyph_paths::ordinary_layer_contours_to_bezpath(layer);
        assert!((after.area().abs() - before.area().abs()).abs() < 1e-6);
        project.save().unwrap();
        let reloaded = Project::load(&source_path).unwrap();
        let reloaded_layer = reloaded
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        assert_eq!(
            reloaded.glyph_layer(name, &reloaded_layer).unwrap(),
            project.glyph_layer(name, &layer_id).unwrap()
        );
    }
}

#[test]
fn canonical_knife_replaces_only_cut_contours_and_preserves_quadratics() {
    let scratch = Scratch::new();
    let rectangle = |x0: f64, label: &str| {
        let mut contour = Contour::new(
            vec![
                ContourPoint::new(
                    x0,
                    0.0,
                    PointType::Line,
                    false,
                    Some(Name::new(&format!("{label}-a")).unwrap()),
                    Some(norad::Identifier::new(&format!("{label}-a")).unwrap()),
                ),
                ContourPoint::new(x0 + 100.0, 0.0, PointType::Line, false, None, None),
                ContourPoint::new(x0 + 100.0, 100.0, PointType::Line, false, None, None),
                ContourPoint::new(x0, 100.0, PointType::Line, false, None, None),
            ],
            Some(norad::Identifier::new(label).unwrap()),
        );
        contour.replace_lib(object_lib(label));
        contour
    };
    let cut = rectangle(0.0, "cut-source");
    let untouched = rectangle(300.0, "untouched-hyper");
    let mut glyph = Glyph::new("knife-contours");
    glyph.contours = vec![cut, untouched.clone()];
    let mut component = Component::new(
        Name::new("base").unwrap(),
        norad::AffineTransform {
            x_offset: 12.25,
            y_offset: 34.75,
            ..Default::default()
        },
        Some(norad::Identifier::new("knife-component").unwrap()),
    );
    component.replace_lib(object_lib("knife-component"));
    glyph.components.push(component.clone());
    let mut anchor = Anchor::new(
        50.5,
        150.25,
        Some(Name::new("top").unwrap()),
        None,
        Some(norad::Identifier::new("knife-anchor").unwrap()),
    );
    anchor.replace_lib(object_lib("knife-anchor"));
    glyph.anchors.push(anchor.clone());
    let mut base = Glyph::new("base");
    base.contours.push(rectangle(0.0, "base-contour"));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(base);
    font.default_layer_mut().insert_glyph(glyph);
    let source_path = scratch.0.join("KnifeContours.ufo");
    font.save(&source_path).unwrap();
    let mut project = Project::load(&source_path).unwrap();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project.document_layer("knife-contours", &layer_id).unwrap();
    let original: Vec<_> = layer.contours().collect();
    let cut_id = original[0].id();
    let cut_points: Vec<_> = original[0].points().map(|point| point.id()).collect();
    let untouched_id = original[1].id();
    let untouched_points: Vec<_> = original[1].points().map(|point| point.id()).collect();
    let component_id = layer.components().next().unwrap().id();
    let anchor_id = layer.anchors().next().unwrap().id();
    assert_eq!(
        runebender::outline::knife::knife_hit_points_in_layer(
            layer,
            kurbo::Point::new(-10.0, 50.0),
            kurbo::Point::new(110.0, 50.0)
        ),
        [kurbo::Point::new(0.0, 50.0), kurbo::Point::new(100.0, 50.0)]
    );

    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer("knife-contours", &layer_id, |draft| {
                assert!(!draft.knife_cut(
                    kurbo::Point::new(150.0, -50.0),
                    kurbo::Point::new(150.0, 150.0)
                )?);
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);

    project
        .edit_document_layer("knife-contours", &layer_id, |draft| {
            assert!(draft.knife_cut(
                kurbo::Point::new(-10.0, 50.0),
                kurbo::Point::new(110.0, 50.0)
            )?);
            Ok(())
        })
        .unwrap();
    let layer = project.document_layer("knife-contours", &layer_id).unwrap();
    let contours: Vec<_> = layer.contours().collect();
    assert_eq!(contours.len(), 3);
    assert!(contours[..2].iter().all(|contour| contour.id() != cut_id));
    assert!(contours[..2].iter().all(|contour| {
        contour
            .points()
            .all(|point| !cut_points.contains(&point.id()))
    }));
    assert_eq!(contours[2].id(), untouched_id);
    assert_eq!(
        contours[2]
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        untouched_points
    );
    assert!(contours[2].is_hyper());
    assert_eq!(layer.components().next().unwrap().id(), component_id);
    assert_eq!(layer.anchors().next().unwrap().id(), anchor_id);
    let projected = project.glyph_layer("knife-contours", &layer_id).unwrap();
    assert_eq!(projected.contours[2], untouched);
    assert_eq!(projected.components, [component.clone()]);
    assert_eq!(projected.anchors, [anchor.clone()]);
    assert!(projected.contours[..2].iter().all(|contour| {
        contour.identifier().is_none()
            && contour.lib().is_none()
            && contour.points.iter().all(|point| {
                point.name.is_none() && point.identifier().is_none() && point.lib().is_none()
            })
    }));
    project.save().unwrap();
    let reloaded = Project::load(&source_path).unwrap();
    let reloaded_layer = reloaded
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    assert_eq!(
        reloaded
            .glyph_layer("knife-contours", &reloaded_layer)
            .unwrap(),
        projected
    );

    let mut quadratic = Glyph::new("quadratic-knife");
    quadratic.contours.push(Contour::new(
        vec![
            ContourPoint::new(0.0, 0.0, PointType::QCurve, false, None, None),
            ContourPoint::new(50.0, -50.0, PointType::OffCurve, false, None, None),
            ContourPoint::new(100.0, 0.0, PointType::QCurve, false, None, None),
            ContourPoint::new(150.0, 50.0, PointType::OffCurve, false, None, None),
            ContourPoint::new(100.0, 100.0, PointType::QCurve, false, None, None),
            ContourPoint::new(50.0, 150.0, PointType::OffCurve, false, None, None),
            ContourPoint::new(0.0, 100.0, PointType::QCurve, false, None, None),
            ContourPoint::new(-50.0, 50.0, PointType::OffCurve, false, None, None),
        ],
        None,
    ));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(quadratic);
    let quadratic_path = scratch.0.join("QuadraticKnife.ufo");
    font.save(&quadratic_path).unwrap();
    let mut project = Project::load(&quadratic_path).unwrap();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    project
        .edit_document_layer("quadratic-knife", &layer_id, |draft| {
            assert!(draft.knife_cut(
                kurbo::Point::new(50.0, -100.0),
                kurbo::Point::new(50.0, 200.0)
            )?);
            Ok(())
        })
        .unwrap();
    let projected = project.glyph_layer("quadratic-knife", &layer_id).unwrap();
    assert_eq!(projected.contours.len(), 2);
    assert!(projected.contours.iter().all(|contour| {
        contour
            .points
            .iter()
            .all(|point| point.typ != PointType::Curve)
            && contour
                .points
                .iter()
                .any(|point| point.typ == PointType::QCurve)
    }));
}

#[test]
fn canonical_knife_preserves_all_off_curve_and_mixed_degree_geometry() {
    let scratch = Scratch::new();
    let point = |x, y, point_type| ContourPoint::new(x, y, point_type, false, None, None);
    let mut all_off_curve = Glyph::new("all-off-curve-knife");
    all_off_curve.contours.push(Contour::new(
        vec![
            point(0.0, 0.0, PointType::OffCurve),
            point(128.0, 256.0, PointType::OffCurve),
            point(256.0, 0.0, PointType::OffCurve),
        ],
        None,
    ));
    let mut mixed = Glyph::new("mixed-knife");
    mixed.contours.push(Contour::new(
        vec![
            point(0.0, 0.0, PointType::Move),
            point(0.0, 128.0, PointType::OffCurve),
            point(128.0, 128.0, PointType::OffCurve),
            point(128.0, 0.0, PointType::Curve),
            point(192.0, -128.0, PointType::OffCurve),
            point(256.0, 0.0, PointType::QCurve),
        ],
        None,
    ));
    let mut quadratic_chain = Glyph::new("quadratic-chain-knife");
    quadratic_chain.contours.push(Contour::new(
        vec![
            point(0.0, 0.0, PointType::Move),
            point(0.0, 128.0, PointType::OffCurve),
            point(128.0, 128.0, PointType::OffCurve),
            point(128.0, 0.0, PointType::QCurve),
        ],
        None,
    ));
    let mut three_control_chain = Glyph::new("three-control-chain-knife");
    three_control_chain.contours.push(Contour::new(
        vec![
            point(0.0, 0.0, PointType::Move),
            point(0.0, 128.0, PointType::OffCurve),
            point(64.0, 192.0, PointType::OffCurve),
            point(128.0, 128.0, PointType::OffCurve),
            point(128.0, 0.0, PointType::QCurve),
        ],
        None,
    ));
    let mut mixed_chain = Glyph::new("mixed-chain-knife");
    mixed_chain.contours.push(Contour::new(
        vec![
            point(0.0, 0.0, PointType::Move),
            point(0.0, 128.0, PointType::OffCurve),
            point(128.0, 128.0, PointType::OffCurve),
            point(128.0, 0.0, PointType::Curve),
            point(160.0, -128.0, PointType::OffCurve),
            point(224.0, -128.0, PointType::OffCurve),
            point(256.0, 0.0, PointType::QCurve),
        ],
        None,
    ));
    let closed_chain_points = vec![
        point(0.0, 0.0, PointType::QCurve),
        point(0.0, 128.0, PointType::OffCurve),
        point(128.0, 128.0, PointType::OffCurve),
        point(128.0, 0.0, PointType::QCurve),
        point(128.0, -128.0, PointType::OffCurve),
        point(0.0, -128.0, PointType::OffCurve),
    ];
    let mut closed_chain = Glyph::new("closed-chain-knife");
    closed_chain
        .contours
        .push(Contour::new(closed_chain_points.clone(), None));
    let mut rotated_closed_chain = Glyph::new("rotated-closed-chain-knife");
    let mut rotated_points = closed_chain_points;
    rotated_points.rotate_left(4);
    rotated_closed_chain
        .contours
        .push(Contour::new(rotated_points, None));
    let mut font = Font::new();
    for glyph in [
        all_off_curve,
        mixed,
        quadratic_chain,
        three_control_chain,
        mixed_chain,
        closed_chain,
        rotated_closed_chain,
    ] {
        font.default_layer_mut().insert_glyph(glyph);
    }
    let source_path = scratch.0.join("KnifePathKinds.ufo");
    font.save(&source_path).unwrap();
    let mut project = Project::load(&source_path).unwrap();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    for name in [
        "all-off-curve-knife",
        "mixed-knife",
        "quadratic-chain-knife",
        "three-control-chain-knife",
        "mixed-chain-knife",
        "closed-chain-knife",
        "rotated-closed-chain-knife",
    ] {
        let contour = project
            .document_layer(name, &layer_id)
            .unwrap()
            .contours()
            .next()
            .unwrap();
        assert_eq!(
            runebender::outline::path::Path::from_document_contour(contour)
                .to_bezpath()
                .segments()
                .collect::<Vec<_>>(),
            runebender::outline::glyph_paths::ordinary_contour_to_bezpath(contour)
                .segments()
                .collect::<Vec<_>>()
        );
    }
    let closed_segments = runebender::outline::path::Path::from_document_contour(
        project
            .document_layer("closed-chain-knife", &layer_id)
            .unwrap()
            .contours()
            .next()
            .unwrap(),
    )
    .to_bezpath()
    .segments()
    .collect::<Vec<_>>();
    let rotated_segments = runebender::outline::path::Path::from_document_contour(
        project
            .document_layer("rotated-closed-chain-knife", &layer_id)
            .unwrap()
            .contours()
            .next()
            .unwrap(),
    )
    .to_bezpath()
    .segments()
    .collect::<Vec<_>>();
    assert_eq!(closed_segments, rotated_segments);
    let hits = runebender::outline::knife::knife_hit_points_in_layer(
        project.document_layer("mixed-knife", &layer_id).unwrap(),
        kurbo::Point::new(-10.0, 80.0),
        kurbo::Point::new(300.0, 80.0),
    );
    assert_eq!(hits.len(), 2);
    let root = (1.0_f64 / 6.0).sqrt();
    for (hit, parameter) in hits.iter().zip([(1.0 - root) / 2.0, (1.0 + root) / 2.0]) {
        let expected_x = 128.0 * (3.0 * parameter * parameter - 2.0 * parameter.powi(3));
        assert!((hit.x - expected_x).abs() < 1e-6);
        assert!((hit.y - 80.0).abs() < 1e-6);
    }
    project
        .edit_document_layer("all-off-curve-knife", &layer_id, |draft| {
            assert!(draft.knife_cut(
                kurbo::Point::new(-10.0, 100.0),
                kurbo::Point::new(266.0, 100.0)
            )?);
            Ok(())
        })
        .unwrap();
    project
        .edit_document_layer("mixed-knife", &layer_id, |draft| {
            assert!(draft.knife_cut(
                kurbo::Point::new(-10.0, 80.0),
                kurbo::Point::new(300.0, 80.0)
            )?);
            Ok(())
        })
        .unwrap();
    for name in ["all-off-curve-knife", "mixed-knife"] {
        let projected = project.glyph_layer(name, &layer_id).unwrap();
        assert_eq!(projected.contours.len(), 2);
        assert!(
            projected.contours.iter().any(|contour| {
                contour
                    .points
                    .iter()
                    .any(|point| point.typ == PointType::QCurve)
            }),
            "{name} lost quadratic output: {:?}",
            projected.contours
        );
    }
    project.save().unwrap();
    let reloaded = Project::load(&source_path).unwrap();
    let reloaded_layer = reloaded
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    for name in ["all-off-curve-knife", "mixed-knife"] {
        assert_eq!(
            reloaded.glyph_layer(name, &reloaded_layer).unwrap(),
            project.glyph_layer(name, &layer_id).unwrap()
        );
    }
}

#[test]
fn canonical_cleanup_preserves_surviving_identities_and_metadata() {
    let scratch = Scratch::new();
    let point = |x, y, label: &str| {
        let mut point = ContourPoint::new(
            x,
            y,
            PointType::Line,
            false,
            Some(Name::new(label).unwrap()),
            Some(norad::Identifier::new(label).unwrap()),
        );
        point.replace_lib(object_lib(label));
        point
    };
    let mut outer = Contour::new(
        vec![
            point(0.4, 0.0, "outer-a"),
            point(0.0, 400.0, "outer-b"),
            point(0.0, 400.0, "duplicate"),
            point(400.0, 400.0, "outer-c"),
            point(400.0, 0.0, "outer-d"),
        ],
        Some(norad::Identifier::new("outer").unwrap()),
    );
    outer.replace_lib(object_lib("outer"));
    let mut hole = Contour::new(
        vec![
            point(100.0, 100.0, "hole-a"),
            point(300.0, 100.0, "hole-b"),
            point(300.0, 300.0, "hole-c"),
            point(100.0, 300.0, "hole-d"),
        ],
        Some(norad::Identifier::new("hole").unwrap()),
    );
    hole.replace_lib(object_lib("hole"));
    let mut glyph = Glyph::new("cleanup-contours");
    glyph.contours = vec![outer, hole];
    let mut expected = glyph.clone();
    assert_eq!(
        runebender::outline::cleanup::tidy_contours(&mut expected),
        1
    );
    assert_eq!(
        runebender::outline::cleanup::round_glyph_coordinates(&mut expected),
        1
    );
    assert_eq!(
        runebender::outline::cleanup::correct_path_directions(&mut expected),
        2
    );
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let source_path = scratch.0.join("CleanupContours.ufo");
    font.save(&source_path).unwrap();
    let mut project = Project::load(&source_path).unwrap();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project
        .document_layer("cleanup-contours", &layer_id)
        .unwrap();
    let original_ids: HashSet<_> = layer
        .contours()
        .flat_map(|contour| contour.points().map(|point| point.id()))
        .collect();
    let duplicate_id = layer
        .contours()
        .next()
        .unwrap()
        .points()
        .nth(2)
        .unwrap()
        .id();
    project
        .edit_document_layer("cleanup-contours", &layer_id, |draft| {
            assert_eq!(draft.tidy_contours(), 1);
            assert_eq!(draft.round_coordinates(), 1);
            assert_eq!(draft.correct_path_directions()?, 2);
            Ok(())
        })
        .unwrap();
    let layer = project
        .document_layer("cleanup-contours", &layer_id)
        .unwrap();
    let surviving_ids: HashSet<_> = layer
        .contours()
        .flat_map(|contour| contour.points().map(|point| point.id()))
        .collect();
    assert_eq!(surviving_ids.len(), original_ids.len() - 1);
    assert!(!surviving_ids.contains(&duplicate_id));
    assert!(surviving_ids.iter().all(|id| original_ids.contains(id)));
    let projected = project.glyph_layer("cleanup-contours", &layer_id).unwrap();
    assert_eq!(
        runebender::outline::glyph_paths::contours_to_bezpath(&projected),
        runebender::outline::glyph_paths::contours_to_bezpath(&expected)
    );
    assert!(projected.contours.iter().all(|contour| {
        contour.identifier().is_some()
            && contour.lib().is_some()
            && contour.points.iter().all(|point| {
                point
                    .name
                    .as_ref()
                    .is_some_and(|name| name.as_str() != "duplicate")
                    && point.identifier().is_some()
                    && point.lib().is_some()
            })
    }));
    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_layer("cleanup-contours", &layer_id, |draft| {
                assert_eq!(draft.tidy_contours(), 0);
                assert_eq!(draft.round_coordinates(), 0);
                assert_eq!(draft.correct_path_directions()?, 0);
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(project.document_snapshot(), snapshot);
    project.save().unwrap();
    let reloaded = Project::load(&source_path).unwrap();
    let reloaded_layer = reloaded
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    assert_eq!(
        reloaded
            .glyph_layer("cleanup-contours", &reloaded_layer)
            .unwrap(),
        projected
    );
}

#[test]
fn canonical_fit_and_extremes_match_existing_geometry_with_stable_objects() {
    let scratch = Scratch::new();
    let point = |x, y, point_type, label: &str| {
        let mut point = ContourPoint::new(
            x,
            y,
            point_type,
            false,
            Some(Name::new(label).unwrap()),
            Some(norad::Identifier::new(label).unwrap()),
        );
        point.replace_lib(object_lib(label));
        point
    };
    let mut contour = Contour::new(
        vec![
            point(0.0, 0.0, PointType::Move, "start"),
            point(0.0, -10.0, PointType::OffCurve, "first-control"),
            point(150.0, -50.0, PointType::OffCurve, "second-control"),
            point(200.0, 0.0, PointType::Curve, "end"),
        ],
        Some(norad::Identifier::new("fit-contour").unwrap()),
    );
    contour.replace_lib(object_lib("fit-contour"));
    let mut glyph = Glyph::new("fit-extremes");
    glyph.contours.push(contour);
    let mut expected = glyph.clone();
    let mut selection = HashSet::new();
    selection.insert((0, 0));
    assert!(runebender::outline::cleanup::fit_curve_handles(
        &mut expected,
        &selection,
        0.5
    ));
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(glyph);
    let source_path = scratch.0.join("FitExtremes.ufo");
    font.save(&source_path).unwrap();
    let mut project = Project::load(&source_path).unwrap();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project.document_layer("fit-extremes", &layer_id).unwrap();
    let original_ids: Vec<_> = layer
        .contours()
        .next()
        .unwrap()
        .points()
        .map(|point| point.id())
        .collect();
    project
        .edit_document_layer("fit-extremes", &layer_id, |draft| {
            assert!(draft.fit_curve_handles(&[original_ids[0]], 0.5)?);
            assert!(!draft.fit_curve_handles(&[original_ids[0]], 0.5)?);
            assert!(!draft.fit_curve_handles(&[], 0.0)?);
            Ok(())
        })
        .unwrap();
    let layer = project.document_layer("fit-extremes", &layer_id).unwrap();
    assert_eq!(
        layer
            .contours()
            .next()
            .unwrap()
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        original_ids
    );
    assert_eq!(
        project.glyph_layer("fit-extremes", &layer_id).unwrap(),
        expected
    );

    assert!(runebender::outline::cleanup::add_extreme_points(
        &mut expected,
        &HashSet::new()
    ));
    project
        .edit_document_layer("fit-extremes", &layer_id, |draft| {
            assert!(draft.add_extreme_points(&[])?);
            assert!(!draft.add_extreme_points(&[])?);
            Ok(())
        })
        .unwrap();
    let layer = project.document_layer("fit-extremes", &layer_id).unwrap();
    let final_ids: HashSet<_> = layer
        .contours()
        .next()
        .unwrap()
        .points()
        .map(|point| point.id())
        .collect();
    assert!(original_ids.iter().all(|id| final_ids.contains(id)));
    assert!(final_ids.len() > original_ids.len());
    let projected = project.glyph_layer("fit-extremes", &layer_id).unwrap();
    assert_eq!(
        runebender::outline::glyph_paths::contours_to_bezpath(&projected),
        runebender::outline::glyph_paths::contours_to_bezpath(&expected)
    );
    for label in ["start", "first-control", "second-control", "end"] {
        let point = projected.contours[0]
            .points
            .iter()
            .find(|point| {
                point
                    .name
                    .as_ref()
                    .is_some_and(|name| name.as_str() == label)
            })
            .unwrap();
        assert!(point.identifier().is_some());
        assert!(point.lib().is_some());
    }
    project.save().unwrap();
    let reloaded = Project::load(&source_path).unwrap();
    let reloaded_layer = reloaded
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    assert_eq!(
        reloaded
            .glyph_layer("fit-extremes", &reloaded_layer)
            .unwrap(),
        projected
    );
}

#[test]
fn canonical_embolden_preserves_structure_identities_and_metadata() {
    let scratch = Scratch::new();
    let square = |name: &str| {
        let mut glyph = Glyph::new(name);
        let mut contour = Contour::new(
            [
                (0.0, 0.0, "a"),
                (100.0, 0.0, "b"),
                (100.0, 100.0, "c"),
                (0.0, 100.0, "d"),
            ]
            .into_iter()
            .map(|(x, y, suffix)| {
                let label = format!("{name}-{suffix}");
                let mut point = ContourPoint::new(
                    x,
                    y,
                    PointType::Line,
                    false,
                    Some(Name::new(&label).unwrap()),
                    Some(norad::Identifier::new(&label).unwrap()),
                );
                point.replace_lib(object_lib(&label));
                point
            })
            .collect(),
            Some(norad::Identifier::new(&format!("{name}-contour")).unwrap()),
        );
        contour.replace_lib(object_lib(&format!("{name}-contour")));
        glyph.contours.push(contour);
        glyph
    };
    let light = square("light");
    let heavy = runebender::outline::embolden::embolden(
        &square("heavy"),
        runebender::outline::embolden::Offset { x: 12.0, y: 6.0 },
    );
    let target = square("target");
    let delta_target = square("delta-target");
    let mut font = Font::new();
    for glyph in [light.clone(), heavy, target.clone(), delta_target.clone()] {
        font.default_layer_mut().insert_glyph(glyph);
    }
    let source_path = scratch.0.join("Embolden.ufo");
    font.save(&source_path).unwrap();
    let mut project = Project::load(&source_path).unwrap();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let offset = runebender::outline::embolden::learn_layer_offset(&[(
        project.document_layer("light", &layer_id).unwrap(),
        project.document_layer("heavy", &layer_id).unwrap(),
    )])
    .unwrap();
    assert!((offset.x - 12.0).abs() < 1e-6);
    assert!((offset.y - 6.0).abs() < 1e-6);
    let original_ids: Vec<_> = project
        .document_layer("target", &layer_id)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .map(|point| point.id())
        .collect();
    project
        .edit_document_layer("target", &layer_id, |draft| {
            assert!(draft.embolden(offset)?);
            assert!(!draft.embolden(runebender::outline::embolden::Offset { x: 0.0, y: 0.0 })?);
            Ok(())
        })
        .unwrap();
    let layer = project.document_layer("target", &layer_id).unwrap();
    assert_eq!(
        layer
            .contours()
            .next()
            .unwrap()
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        original_ids
    );
    let expected = runebender::outline::embolden::embolden(&target, offset);
    let projected = project.glyph_layer("target", &layer_id).unwrap();
    assert_eq!(
        runebender::outline::glyph_paths::contours_to_bezpath(&projected),
        runebender::outline::glyph_paths::contours_to_bezpath(&expected)
    );
    assert_eq!(projected.contours[0].lib(), target.contours[0].lib());
    for (point, source) in projected.contours[0]
        .points
        .iter()
        .zip(&target.contours[0].points)
    {
        assert_eq!(point.name, source.name);
        assert_eq!(point.identifier(), source.identifier());
        assert_eq!(point.lib(), source.lib());
    }

    let deltas = [(1, 2), (3, 4), (5, 6), (7, 8), (999, 999)];
    let mut expected_delta = delta_target.clone();
    expected_delta.contours =
        runebender::outline::effects::bolden_contours(&delta_target, &deltas, (10, 20));
    let delta_ids: Vec<_> = project
        .document_layer("delta-target", &layer_id)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .map(|point| point.id())
        .collect();
    project
        .edit_document_layer("delta-target", &layer_id, |draft| {
            assert!(draft.apply_bolden_deltas(&deltas, (10, 20))?);
            Ok(())
        })
        .unwrap();
    let layer = project.document_layer("delta-target", &layer_id).unwrap();
    assert_eq!(
        layer
            .contours()
            .next()
            .unwrap()
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        delta_ids
    );
    let projected_delta = project.glyph_layer("delta-target", &layer_id).unwrap();
    assert_eq!(
        runebender::outline::glyph_paths::contours_to_bezpath(&projected_delta),
        runebender::outline::glyph_paths::contours_to_bezpath(&expected_delta)
    );
    assert_eq!(
        projected_delta.contours[0].lib(),
        delta_target.contours[0].lib()
    );
    project.save().unwrap();
    let reloaded = Project::load(&source_path).unwrap();
    let reloaded_layer = reloaded
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    assert_eq!(
        reloaded.glyph_layer("target", &reloaded_layer).unwrap(),
        projected
    );
    assert_eq!(
        reloaded
            .glyph_layer("delta-target", &reloaded_layer)
            .unwrap(),
        projected_delta
    );
}

#[test]
fn canonical_component_decomposition_resolves_nested_metadata_safely() {
    let scratch = Scratch::new();
    let point = |x, y, label: &str| {
        let mut point = ContourPoint::new(
            x,
            y,
            PointType::Line,
            false,
            Some(Name::new(label).unwrap()),
            Some(norad::Identifier::new(label).unwrap()),
        );
        point.replace_lib(object_lib(label));
        point
    };
    let mut base_contour = Contour::new(
        vec![
            point(0.0, 0.0, "base-a"),
            point(100.0, 0.0, "base-b"),
            point(100.0, 100.0, "base-c"),
            point(0.0, 100.0, "base-d"),
        ],
        Some(norad::Identifier::new("base-contour").unwrap()),
    );
    base_contour.replace_lib(object_lib("base-contour"));
    let mut base = Glyph::new("base");
    base.contours.push(base_contour.clone());
    let mut middle = Glyph::new("middle");
    middle.components.push(Component::new(
        Name::new("base").unwrap(),
        norad::AffineTransform {
            x_offset: 10.25,
            y_offset: 20.75,
            ..Default::default()
        },
        None,
    ));
    let existing = Contour::new(
        vec![
            point(300.0, 0.0, "existing-a"),
            point(400.0, 0.0, "existing-b"),
            point(400.0, 100.0, "existing-c"),
            point(300.0, 100.0, "existing-d"),
        ],
        Some(norad::Identifier::new("existing-contour").unwrap()),
    );
    let mut target = Glyph::new("decompose-target");
    target.contours.push(existing.clone());
    let mut component = Component::new(
        Name::new("middle").unwrap(),
        norad::AffineTransform {
            x_scale: 1.5,
            y_scale: 0.75,
            x_offset: 5.5,
            y_offset: -7.25,
            ..Default::default()
        },
        Some(norad::Identifier::new("target-component").unwrap()),
    );
    component.replace_lib(object_lib("target-component"));
    target.components.push(component);
    let mut anchor = Anchor::new(
        50.25,
        150.75,
        Some(Name::new("top").unwrap()),
        None,
        Some(norad::Identifier::new("decompose-anchor").unwrap()),
    );
    anchor.replace_lib(object_lib("decompose-anchor"));
    target.anchors.push(anchor.clone());
    let empty_base = Glyph::new("empty-base");
    let mut empty_target = Glyph::new("empty-decompose-target");
    empty_target.components.push(Component::new(
        Name::new("empty-base").unwrap(),
        norad::AffineTransform::default(),
        None,
    ));
    let mut expected = target.clone();
    let mut font = Font::new();
    for glyph in [base.clone(), middle, target, empty_base, empty_target] {
        font.default_layer_mut().insert_glyph(glyph);
    }
    expected
        .contours
        .extend(runebender::outline::component_ops::resolved_component_contours(&font, &expected));
    expected.components.clear();
    let source_path = scratch.0.join("DecomposeComponents.ufo");
    font.save(&source_path).unwrap();
    let mut project = Project::load(&source_path).unwrap();
    let layer_id = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let layer = project
        .document_layer("decompose-target", &layer_id)
        .unwrap();
    let existing_id = layer.contours().next().unwrap().id();
    let existing_points: Vec<_> = layer
        .contours()
        .next()
        .unwrap()
        .points()
        .map(|point| point.id())
        .collect();
    let anchor_id = layer.anchors().next().unwrap().id();
    let resolved =
        runebender::outline::component_ops::resolved_document_component_contours(layer, |name| {
            project.document_layer(name, &layer_id)
        })
        .unwrap();
    assert_eq!(resolved.len(), 1);
    project
        .edit_document_layer("decompose-target", &layer_id, |draft| {
            assert!(draft.decompose_components(&resolved)?);
            assert!(!draft.decompose_components(&resolved)?);
            Ok(())
        })
        .unwrap();
    let layer = project
        .document_layer("decompose-target", &layer_id)
        .unwrap();
    let contours: Vec<_> = layer.contours().collect();
    assert_eq!(contours.len(), 2);
    assert_eq!(contours[0].id(), existing_id);
    assert_eq!(
        contours[0]
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>(),
        existing_points
    );
    assert_ne!(contours[1].id(), existing_id);
    assert!(layer.components().next().is_none());
    assert_eq!(layer.anchors().next().unwrap().id(), anchor_id);
    let projected = project.glyph_layer("decompose-target", &layer_id).unwrap();
    assert_eq!(
        runebender::outline::glyph_paths::contours_to_bezpath(&projected),
        runebender::outline::glyph_paths::contours_to_bezpath(&expected)
    );
    assert_eq!(projected.contours[0], existing);
    assert_eq!(projected.anchors, [anchor.clone()]);
    assert_eq!(projected.contours[1].lib(), base_contour.lib());
    assert_ne!(
        projected.contours[1].identifier(),
        base_contour.identifier()
    );
    for (point, source) in projected.contours[1]
        .points
        .iter()
        .zip(&base_contour.points)
    {
        assert_eq!(point.name, source.name);
        assert_eq!(point.lib(), source.lib());
        assert_ne!(point.identifier(), source.identifier());
    }
    let empty_layer = project
        .document_layer("empty-decompose-target", &layer_id)
        .unwrap();
    let empty_resolved = runebender::outline::component_ops::resolved_document_component_contours(
        empty_layer,
        |name| project.document_layer(name, &layer_id),
    )
    .unwrap();
    assert!(empty_resolved.is_empty());
    project
        .edit_document_layer("empty-decompose-target", &layer_id, |draft| {
            assert!(draft.decompose_components(&empty_resolved)?);
            assert!(!draft.decompose_components(&empty_resolved)?);
            Ok(())
        })
        .unwrap();
    assert!(
        project
            .document_layer("empty-decompose-target", &layer_id)
            .unwrap()
            .components()
            .next()
            .is_none()
    );
    project.save().unwrap();
    let reloaded = Project::load(&source_path).unwrap();
    let reloaded_layer = reloaded
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    assert_eq!(
        reloaded
            .glyph_layer("decompose-target", &reloaded_layer)
            .unwrap(),
        projected
    );
    assert!(
        reloaded
            .glyph_layer("empty-decompose-target", &reloaded_layer)
            .unwrap()
            .components
            .is_empty()
    );
}

#[test]
fn canonical_snapshot_isolated_from_later_edits_and_format_projections() {
    let (_scratch, mut project, _fonts) = adversarial_fixture();
    let source = SourceId(0);
    let layer = project.document_source(source).unwrap().default_layer();
    let snapshot = project.document_snapshot();
    assert_eq!(
        snapshot.source_ids(),
        &[SourceId(0), SourceId(1), SourceId(2), SourceId(3)],
        "snapshot source order changed"
    );
    assert_eq!(
        snapshot.glyph_names().collect::<Vec<_>>(),
        vec!["A", "B", "base"],
        "snapshot glyph order changed"
    );
    let old_width = snapshot.layer("A", &layer).unwrap().width();
    let old_features = snapshot.feature_text(source).unwrap().to_owned();
    assert_eq!(
        snapshot.clone(),
        snapshot,
        "cloning changed canonical snapshot contents"
    );

    project
        .edit_document_layer("A", &layer, |draft| {
            draft.set_width(old_width + 25.0)?;
            Ok(())
        })
        .unwrap();
    project
        .edit_document_source_metadata(source, |draft| {
            draft.set_feature_text("feature liga { sub A B by base; } liga;".into());
            Ok(())
        })
        .unwrap();
    assert_eq!(
        snapshot.layer("A", &layer).unwrap().width(),
        old_width,
        "later geometry edit changed the snapshot"
    );
    assert_eq!(
        snapshot.feature_text(source),
        Some(old_features.as_str()),
        "later source metadata edit changed the snapshot"
    );
    assert_eq!(
        project.document_layer("A", &layer).unwrap().width(),
        old_width + 25.0,
        "live document did not retain its later geometry edit"
    );
    assert_ne!(
        project.document_feature_text(source),
        Some(old_features.as_str()),
        "live document did not retain its later metadata edit"
    );
}

#[test]
fn canonical_source_metadata_edits_are_atomic_and_round_trip_exactly() {
    let scratch = Scratch::new();
    let path = scratch.0.join("Metadata.ufo");
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(Glyph::new("A"));
    font.default_layer_mut().insert_glyph(Glyph::new("V"));
    font.groups.insert(
        Name::new("com.example.arbitrary").unwrap(),
        vec![Name::new("A").unwrap(), Name::new("A").unwrap()],
    );
    font.groups.insert(
        Name::new("public.kern1.A").unwrap(),
        vec![Name::new("A").unwrap()],
    );
    font.kerning
        .entry(Name::new("A").unwrap())
        .or_default()
        .insert(Name::new("V").unwrap(), -81.375);
    font.save(&path).unwrap();

    let mut project = Project::load(&path).unwrap();
    let source = SourceId(0);
    let imported = project.document_font_metadata(source).unwrap();
    assert_eq!(
        imported.groups().get("com.example.arbitrary"),
        Some(&vec!["A".to_string(), "A".to_string()])
    );
    assert_eq!(imported.resolved_kerning("A", "V"), Some(-81.375));
    assert_eq!(
        project.document_snapshot().font_metadata(source),
        Some(imported)
    );

    let mut edited = imported.clone();
    assert!(
        edited
            .set_kerning_pair(
                KerningParticipant::group(KerningSide::First, "A").unwrap(),
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
    assert!(matches!(outcome, DocumentEditOutcome::Changed { .. }));
    assert_eq!(project.document_revision(), revision.wrapping_add(1));
    assert_eq!(project.document_font_metadata(source), Some(&edited));
    assert!(project.sources()[0].dirty);
    assert!(project.sources()[0].kerning_dirty);
    let projected = project.source_snapshot(source).unwrap();
    assert_eq!(projected.kerning["public.kern1.A"]["V"], -63.625);
    assert_eq!(
        projected.groups["com.example.arbitrary"],
        [Name::new("A").unwrap(), Name::new("A").unwrap()]
    );

    let unchanged_revision = project.document_revision();
    assert_eq!(
        project
            .edit_document_source_metadata(source, |draft| {
                assert!(!draft.set_font_metadata(edited.clone()));
                Ok(())
            })
            .unwrap(),
        DocumentEditOutcome::Unchanged {
            revision: unchanged_revision
        }
    );
    let mut rejected = edited.clone();
    rejected
        .set_kerning_pair(
            KerningParticipant::glyph("A").unwrap(),
            KerningParticipant::glyph("V").unwrap(),
            Some(-100.25),
        )
        .unwrap();
    assert_eq!(
        project.edit_document_source_metadata(source, |draft| {
            draft.set_font_metadata(rejected);
            Err(runebender::document::DocumentEditError::Rejected)
        }),
        Err(runebender::document::DocumentEditError::Rejected)
    );
    assert_eq!(project.document_revision(), unchanged_revision);
    assert_eq!(project.document_font_metadata(source), Some(&edited));

    project.save().unwrap();
    let reloaded = Project::load(&path).unwrap();
    assert_eq!(reloaded.document_font_metadata(source), Some(&edited));
    assert_eq!(
        reloaded.source_snapshot(source).unwrap().groups,
        projected.groups
    );
    assert_eq!(
        reloaded.source_snapshot(source).unwrap().kerning,
        projected.kerning
    );
}

#[test]
fn source_glyph_export_and_category_have_canonical_project_queries() {
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(Glyph::new("A"));
    font.lib.insert(
        "public.skipExportGlyphs".into(),
        plist::Value::Array(vec![plist::Value::String("A".into())]),
    );
    font.lib.insert(
        "public.openTypeCategories".into(),
        plist::Value::Dictionary(plist::Dictionary::from_iter([(
            String::from("A"),
            plist::Value::String("mark".into()),
        )])),
    );
    let project = Project::from_source(Master::from_font(
        font.clone(),
        PathBuf::from("SourceGlyphMetadata.ufo"),
    ));

    let metadata = project
        .document_source_glyph_metadata(SourceId(0), "A")
        .unwrap();
    assert!(!metadata.exported());
    assert_eq!(metadata.category(), Some(&OpenTypeGlyphCategory::Mark));
    assert_eq!(project.source_snapshot(SourceId(0)).unwrap().lib, font.lib);
}

#[test]
fn invalid_font_info_edits_are_rejected_before_document_mutation() {
    let mut font = Font::new();
    font.default_layer_mut().insert_glyph(Glyph::new("A"));
    let mut project = Project::from_source(Master::from_font(
        font,
        PathBuf::from("InvalidFontInfo.ufo"),
    ));
    let source = SourceId(0);
    let before = project.document_snapshot();
    let revision = project.document_revision();
    let mut invalid = project.document_font_info(source).unwrap().clone();
    invalid.metrics.units_per_em = Some(-1.0);

    assert_eq!(
        project.edit_document_source_metadata(source, |draft| {
            draft.set_font_info(invalid);
            Ok(())
        }),
        Err(runebender::document::DocumentEditError::InvalidFontInfo)
    );
    assert_eq!(project.document_snapshot(), before);
    assert_eq!(project.document_revision(), revision);
}

#[test]
fn canonical_source_metadata_snapshot_restore_is_atomic_and_order_independent() {
    let (_scratch, mut project) = fixture();
    let source = SourceId(1);
    let before = project.capture_document_source_metadata();
    assert_eq!(
        before.source_ids().collect::<Vec<_>>(),
        [SourceId(0), SourceId(1), SourceId(2), SourceId(3)]
    );
    let old_feature_text = project.document_feature_text(source).unwrap().to_owned();
    project
        .edit_document_source_metadata(source, |draft| {
            draft.set_feature_text("feature kern { pos A V -123; } kern;".into());
            Ok(())
        })
        .unwrap();
    let after = project.capture_document_source_metadata();
    assert_ne!(after, before);
    assert!(project.move_source(source, 0).unwrap());

    let revision = project.document_revision();
    let outcome = project
        .restore_document_source_metadata_if_current(&after, before.clone())
        .unwrap();
    let DocumentEditOutcome::Changed {
        revision: restored_revision,
        change,
    } = outcome
    else {
        panic!("changed source metadata must restore")
    };
    assert_eq!(restored_revision, revision.wrapping_add(1));
    assert_eq!(change.source_metadata(), &[source]);
    assert!(change.metadata_changed());
    assert!(change.requires_compilation());
    assert_eq!(project.source_index(source), Some(0));
    assert_eq!(
        project.document_feature_text(source),
        Some(old_feature_text.as_str())
    );
    assert_eq!(
        project.source_snapshot(source).unwrap().features,
        old_feature_text
    );

    let unchanged_revision = project.document_revision();
    assert_eq!(
        project
            .restore_document_source_metadata_if_current(&before, before.clone())
            .unwrap(),
        DocumentEditOutcome::Unchanged {
            revision: unchanged_revision
        }
    );

    project
        .edit_document_source_metadata(SourceId(0), |draft| {
            draft.set_feature_text("feature liga { sub A B by C; } liga;".into());
            Ok(())
        })
        .unwrap();
    let stale_document = project.document_snapshot();
    let stale_revision = project.document_revision();
    assert_eq!(
        project.restore_document_source_metadata_if_current(&before, after.clone()),
        Err(DocumentSourceMetadataHistoryError::Stale)
    );
    assert_eq!(project.document_snapshot(), stale_document);
    assert_eq!(project.document_revision(), stale_revision);

    let four_sources = project.capture_document_source_metadata();
    project
        .add_interpolated_source("Medium", "Medium.ufo", &location(0.25, 0.0))
        .unwrap();
    let five_sources = project.capture_document_source_metadata();
    let mismatched_document = project.document_snapshot();
    let mismatched_revision = project.document_revision();
    assert_eq!(
        project.restore_document_source_metadata_if_current(&four_sources, four_sources.clone()),
        Err(DocumentSourceMetadataHistoryError::SourceSetMismatch)
    );
    assert_eq!(
        project.restore_document_source_metadata_if_current(&five_sources, four_sources),
        Err(DocumentSourceMetadataHistoryError::SourceSetMismatch)
    );
    assert_eq!(project.document_snapshot(), mismatched_document);
    assert_eq!(project.document_revision(), mismatched_revision);
}

#[test]
fn canonical_layer_snapshot_restore_is_atomic_and_stale_safe() {
    let (_scratch, mut project, _fonts) = adversarial_fixture();
    let layer = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let address = GlyphLayerAddress {
        glyph: "A".into(),
        layer: layer.clone(),
    };
    let before = project.capture_document_layer(&address).unwrap();
    assert_eq!(before.address(), &address);
    let before_projection = project.glyph_layer("A", &layer).unwrap();
    let point = project
        .document_layer("A", &layer)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .next()
        .unwrap();
    let point_id = point.id();
    let point_position = point.position();
    project
        .edit_document_layer("A", &layer, |draft| {
            draft.set_width(before_projection.width + 25.0)?;
            draft.set_point_position(point_id, point_position + kurbo::Vec2::new(7.0, -9.0))?;
            Ok(())
        })
        .unwrap();
    let after = project.capture_document_layer(&address).unwrap();
    let after_projection = project.glyph_layer("A", &layer).unwrap();
    assert_ne!(after, before);

    let revision = project.document_revision();
    let outcome = project
        .restore_document_layer_if_current(&address, &after, before.clone())
        .unwrap();
    let DocumentEditOutcome::Changed {
        revision: restored_revision,
        change,
    } = outcome
    else {
        panic!("restoring changed canonical state must commit")
    };
    assert_eq!(restored_revision, revision.wrapping_add(1));
    assert_eq!(change.affected_layers(), std::slice::from_ref(&address));
    assert!(change.geometry_changed());
    assert!(change.metrics_changed());
    assert!(change.requires_compilation());
    assert_eq!(project.glyph_layer("A", &layer).unwrap(), before_projection);
    assert_eq!(
        project.capture_document_layer(&address),
        Some(before.clone())
    );

    let unchanged_revision = project.document_revision();
    assert_eq!(
        project
            .restore_document_layer_if_current(&address, &before, before.clone())
            .unwrap(),
        DocumentEditOutcome::Unchanged {
            revision: unchanged_revision
        }
    );
    assert_eq!(project.document_revision(), unchanged_revision);

    let unchanged_document = project.document_snapshot();
    let unchanged_projection = project.source_snapshot(SourceId(0)).unwrap();
    assert_eq!(
        project.restore_document_layer_if_current(&address, &after, before.clone()),
        Err(DocumentHistoryError::StaleLayer(address.clone()))
    );
    assert_eq!(project.document_snapshot(), unchanged_document);
    assert_eq!(project.document_revision(), unchanged_revision);
    assert_eq!(
        project.source_snapshot(SourceId(0)).unwrap(),
        unchanged_projection
    );

    let missing = GlyphLayerAddress {
        glyph: "missing".into(),
        layer: layer.clone(),
    };
    assert_eq!(
        project.restore_document_layer_if_current(&missing, &before, before.clone()),
        Err(DocumentHistoryError::MissingLayer(missing))
    );
    let other = GlyphLayerAddress {
        glyph: "B".into(),
        layer: layer.clone(),
    };
    let other_snapshot = project.capture_document_layer(&other).unwrap();
    assert_eq!(
        project.restore_document_layer_if_current(&address, &before, other_snapshot),
        Err(DocumentHistoryError::AddressMismatch(address.clone()))
    );
    assert_eq!(project.document_snapshot(), unchanged_document);
    assert_eq!(project.document_revision(), unchanged_revision);

    let outcome = project
        .restore_document_layer_if_current(&address, &before, after.clone())
        .unwrap();
    assert!(matches!(outcome, DocumentEditOutcome::Changed { .. }));
    assert_eq!(project.glyph_layer("A", &layer).unwrap(), after_projection);
    assert_eq!(project.capture_document_layer(&address), Some(after));
}

#[test]
fn owned_layer_transactions_commit_guardedly_and_replay_project_history() {
    let (_scratch, mut project, _fonts) = adversarial_fixture();
    let layer = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let address = GlyphLayerAddress {
        glyph: "A".into(),
        layer,
    };
    let original_width = project.document_layer("A", &address.layer).unwrap().width();

    let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
    let draft = transaction.draft_mut();
    draft.set_width(original_width + 31.25).unwrap();
    let component = draft
        .add_component("B".into(), kurbo::Affine::translate((12.5, -3.25)))
        .unwrap();
    let anchor = draft
        .add_anchor("transaction-anchor".into(), kurbo::Point::new(25.5, 700.25))
        .unwrap();
    assert!(draft.set_image(None));
    let DocumentEditOutcome::Changed { change, .. } = project
        .commit_document_layer_transaction(transaction)
        .unwrap()
    else {
        panic!("changed transaction did not commit")
    };
    let committed = project.document_layer("A", &address.layer).unwrap();
    assert!(committed.components().any(|item| item.id() == component));
    assert!(committed.anchors().any(|item| item.id() == anchor));
    assert!(committed.image().is_none());
    assert!(change.metrics_changed());
    assert!(change.geometry_changed());
    assert!(change.metadata_changed());

    let mut removal = project.begin_document_layer_transaction(&address).unwrap();
    assert!(removal.draft_mut().remove_component(component).unwrap());
    assert!(removal.draft_mut().remove_anchor(anchor).unwrap());
    assert!(matches!(
        project.commit_document_layer_transaction(removal).unwrap(),
        DocumentEditOutcome::Changed { .. }
    ));
    let removed = project.document_layer("A", &address.layer).unwrap();
    assert!(removed.components().all(|item| item.id() != component));
    assert!(removed.anchors().all(|item| item.id() != anchor));
    assert_eq!(
        project.document_layer_history_depth(&address, HistoryDirection::Undo),
        2
    );

    let DocumentHistoryReplayOutcome::Changed { change, .. } = project
        .replay_document_layer_history(&address, HistoryDirection::Undo)
        .unwrap()
    else {
        panic!("undo did not replay")
    };
    assert!(change.geometry_changed());
    assert!(change.metadata_changed());
    let restored_objects = project.document_layer("A", &address.layer).unwrap();
    assert!(
        restored_objects
            .components()
            .any(|item| item.id() == component)
    );
    assert!(restored_objects.anchors().any(|item| item.id() == anchor));

    let DocumentHistoryReplayOutcome::Changed { change, .. } = project
        .replay_document_layer_history(&address, HistoryDirection::Undo)
        .unwrap()
    else {
        panic!("second undo did not replay")
    };
    assert!(change.metrics_changed());
    assert_eq!(
        project.document_layer("A", &address.layer).unwrap().width(),
        original_width
    );
    assert!(project.can_replay_document_layer_history(&address, HistoryDirection::Redo));

    let stale = project.begin_document_layer_transaction(&address).unwrap();
    project
        .edit_document_layer("A", &address.layer, |draft| {
            draft.set_height(1_234.5)?;
            Ok(())
        })
        .unwrap();
    let snapshot = project.document_snapshot();
    let revision = project.document_revision();
    assert_eq!(
        project.commit_document_layer_transaction(stale),
        Err(DocumentHistoryError::StaleLayer(address.clone()))
    );
    assert_eq!(project.document_snapshot(), snapshot);
    assert_eq!(project.document_revision(), revision);
    assert_eq!(
        project.replay_document_layer_history(&address, HistoryDirection::Redo),
        Err(HistoryReplayError::Stale)
    );
    assert_eq!(
        project.document_layer_history_depth(&address, HistoryDirection::Redo),
        2
    );
}

#[test]
fn source_authoring_keeps_identity_and_round_trips_the_designspace() {
    let (scratch, mut project) = fixture();
    let original = project.source_snapshot(SourceId(1)).unwrap();
    let original_metadata = project.document_font_metadata(SourceId(1)).unwrap().clone();
    let target = location(0.25, 0.0);
    let expected = project.try_interpolated_at("A", &target).unwrap();
    let added = project
        .add_interpolated_source("Medium", "Medium.ufo", &target)
        .unwrap();
    assert_eq!(
        project.source_snapshot(added).unwrap().get_glyph("A"),
        Some(&expected)
    );
    assert!(project.move_source(SourceId(1), 0).unwrap());
    assert_eq!(project.source_index(SourceId(1)), Some(0));
    assert_eq!(
        project.document_font_metadata(SourceId(1)),
        Some(&original_metadata)
    );
    assert_eq!(project.source_snapshot(SourceId(1)).unwrap(), original);
    assert_eq!(project.source_index(added), Some(4));
    project.save().unwrap();
    let reloaded = Project::load(&scratch.0.join("Font.designspace")).unwrap();
    assert_eq!(reloaded.master_names[0].as_ref(), "Heavy");
    assert_eq!(reloaded.sources()[4].font.get_glyph("A"), Some(&expected));
    project.remove_source(added).unwrap();
    assert!(
        scratch.0.join("Medium.ufo").exists(),
        "removing a source must retain its files"
    );
    assert!(project.source_snapshot(added).is_none());
    assert!(project.undo_sources(false).unwrap());
    assert_eq!(
        project.source_snapshot(added).unwrap().get_glyph("A"),
        Some(&expected)
    );
    assert!(project.undo_sources(true).unwrap());
    assert!(project.source_snapshot(added).is_none());
    assert!(
        project.remove_source(SourceId(0)).is_err(),
        "the default source must remain"
    );
}

#[test]
fn full_source_can_replace_intermediate_participation_without_losing_the_layer() {
    let (scratch, mut project) = fixture();
    let original = project.source_snapshot(SourceId(0)).unwrap();
    let target = location(0.5, 0.0);
    let expected = project.try_interpolated_at("A", &target).unwrap();
    let added = project
        .add_interpolated_source("Medium", "Medium.ufo", &target)
        .unwrap();
    assert!(project.brace.is_empty());
    assert_eq!(project.try_interpolated_at("A", &target).unwrap(), expected);
    assert_eq!(project.source_snapshot(SourceId(0)).unwrap(), original);
    assert_eq!(
        project.source_snapshot(added).unwrap().get_glyph("A"),
        Some(&expected)
    );
    project.save().unwrap();
    let reloaded = Project::load(&scratch.0.join("Font.designspace")).unwrap();
    assert!(reloaded.brace.is_empty());
    assert_eq!(
        reloaded.try_interpolated_at("A", &target).unwrap(),
        expected
    );
    assert!(project.undo_sources(false).unwrap());
    assert_eq!(project.brace.len(), 1);
}

#[test]
fn source_undo_refuses_to_overwrite_later_edits_and_layer_operations_preserve_other_glyphs() {
    let (_scratch, mut project) = fixture();
    let original = project.source_snapshot(SourceId(0)).unwrap();
    let from = LayerId {
        source: SourceId(0),
        name: original.default_layer().name().to_string(),
    };
    let layer = project.add_glyph_layer("A", &from, "backup").unwrap();
    assert!(project.glyph_layer("A", &layer).is_some());
    project.edit_layer("A", &from, |glyph| glyph.width += 10.0);
    assert!(project.undo_sources(false).is_err());
    assert!(project.undo_layer("A", &from, false));
    assert!(project.undo_sources(false).unwrap());
    assert_eq!(project.source_snapshot(SourceId(0)).unwrap(), original);
    assert!(project.undo_sources(true).unwrap());
    project.remove_glyph_layer("A", &layer).unwrap();
    assert!(project.glyph_layer("A", &layer).is_none());
    assert!(project.glyph_layer("A", &from).is_some());
    assert!(project.undo_sources(false).unwrap());
    assert!(project.glyph_layer("A", &layer).is_some());
}

#[test]
fn auxiliary_layer_structure_mutates_the_canonical_document_atomically() {
    let (_scratch, mut project, _fonts) = adversarial_fixture();
    let from = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let source = project.document_layer("A", &from).unwrap();
    let source_contours: Vec<_> = source.contours().map(|contour| contour.id()).collect();
    let source_points: Vec<_> = source
        .contours()
        .flat_map(|contour| contour.points().map(|point| point.id()))
        .collect();
    let source_components: Vec<_> = source
        .components()
        .map(|component| component.id())
        .collect();
    let source_anchors: Vec<_> = source.anchors().map(|anchor| anchor.id()).collect();
    let expected = project.glyph_layer("A", &from).unwrap();
    let revision = project.document_revision();

    let copied = project.add_glyph_layer("A", &from, "backup").unwrap();
    assert!(
        project.document_revision() > revision,
        "structural commit did not invalidate the document revision"
    );
    assert_eq!(
        project.glyph_layer("A", &copied).unwrap(),
        expected,
        "copied source data changed"
    );
    let copy = project.document_layer("A", &copied).unwrap();
    let copied_contours: Vec<_> = copy.contours().map(|contour| contour.id()).collect();
    let copied_points: Vec<_> = copy
        .contours()
        .flat_map(|contour| contour.points().map(|point| point.id()))
        .collect();
    let copied_components: Vec<_> = copy.components().map(|component| component.id()).collect();
    let copied_anchors: Vec<_> = copy.anchors().map(|anchor| anchor.id()).collect();
    assert_ne!(
        copied_contours, source_contours,
        "copied contours reused document identities"
    );
    assert_ne!(
        copied_points, source_points,
        "copied points reused document identities"
    );
    assert_ne!(
        copied_components, source_components,
        "copied components reused document identities"
    );
    assert_ne!(
        copied_anchors, source_anchors,
        "copied anchors reused document identities"
    );

    let copied_snapshot = project.document_snapshot();
    let copied_revision = project.document_revision();
    assert!(
        project.add_glyph_layer("A", &from, "backup").is_err(),
        "duplicate copy unexpectedly succeeded"
    );
    assert_eq!(project.document_snapshot(), copied_snapshot);
    assert_eq!(project.document_revision(), copied_revision);
    assert!(project.undo_sources(false).unwrap());
    assert!(project.document_layer("A", &copied).is_none());
    assert!(project.undo_sources(true).unwrap());
    assert_eq!(
        project
            .document_layer("A", &copied)
            .unwrap()
            .contours()
            .map(|contour| contour.id())
            .collect::<Vec<_>>(),
        copied_contours,
        "redo did not restore copied object identities"
    );

    project.remove_glyph_layer("A", &copied).unwrap();
    assert!(project.document_layer("A", &copied).is_none());
    let removed_snapshot = project.document_snapshot();
    let removed_revision = project.document_revision();
    assert!(
        project.remove_glyph_layer("A", &copied).is_err(),
        "missing-layer removal unexpectedly succeeded"
    );
    assert_eq!(project.document_snapshot(), removed_snapshot);
    assert_eq!(project.document_revision(), removed_revision);
    assert!(project.undo_sources(false).unwrap());
    assert_eq!(
        project
            .document_layer("A", &copied)
            .unwrap()
            .anchors()
            .map(|anchor| anchor.id())
            .collect::<Vec<_>>(),
        copied_anchors,
        "undo did not restore removed object identities"
    );
}

#[test]
fn structural_undo_and_redo_advance_the_live_document_revision() {
    let (_scratch, mut project) = fixture();
    let from = project
        .document_source(SourceId(0))
        .unwrap()
        .default_layer();
    let copied = project.add_glyph_layer("B", &from, "backup").unwrap();
    let after_first_copy = project.document_revision();
    assert_eq!(
        project.add_glyph_layer("A", &from, "backup").unwrap(),
        copied
    );
    let after_second_copy = project.document_revision();
    assert!(after_second_copy > after_first_copy);

    assert!(project.undo_sources(false).unwrap());
    let after_undo = project.document_revision();
    assert!(
        after_undo > after_second_copy,
        "structural undo reused an earlier document revision"
    );
    assert!(project.document_layer("A", &copied).is_none());
    assert!(project.document_layer("B", &copied).is_some());

    assert!(project.undo_sources(true).unwrap());
    let after_redo = project.document_revision();
    assert!(
        after_redo > after_undo,
        "structural redo reused an earlier document revision"
    );
    assert!(project.document_layer("A", &copied).is_some());
    assert!(project.document_layer("B", &copied).is_some());

    let width = project.document_layer("A", &copied).unwrap().width();
    let changed = project
        .edit_document_layer("A", &copied, |draft| {
            draft.set_width(width + 1.0)?;
            Ok(())
        })
        .unwrap();
    assert!(matches!(
        changed,
        DocumentEditOutcome::Changed { revision, .. } if revision == after_redo + 1
    ));
}

#[test]
fn interpolation_is_glyph_local_and_independent_of_selected_source() {
    let (_scratch, mut project) = fixture();
    let a = project
        .try_interpolated_at("A", &location(0.5, 0.0))
        .unwrap();
    assert_eq!(a.contours[0].points[0].x, 80.0);
    let b = project
        .try_interpolated_at("B", &location(0.5, 0.0))
        .unwrap();
    assert_eq!(b.contours[0].points[0].x, 50.0);
    let middle = project
        .try_interpolated_at("B", &location(0.5, 0.5))
        .unwrap();
    assert_eq!(middle.contours[0].points[0].x, 87.5);
    project.active = 3;
    assert_eq!(
        project
            .try_interpolated_at("B", &location(0.5, 0.5))
            .unwrap(),
        middle
    );
    assert_eq!(middle.note.as_deref(), Some("source 0"));
    assert!(project.glyph_names().any(|name| name == "onlySketch"));
    assert!(
        project
            .try_interpolated_at("onlySketch", &location(0.5, 0.0))
            .is_err()
    );
}

#[test]
fn interpolation_preserves_precision_and_varies_anchors_and_components() {
    let (_scratch, project) = fixture();
    let glyph = project
        .try_interpolated_at("C", &location(0.5, 0.0))
        .unwrap();
    assert_eq!(glyph.width, 650.123_456_789);
    assert_eq!(glyph.height, 1050.0);
    assert_eq!(glyph.anchors[0].x, 150.0);
    assert_eq!(glyph.components[0].transform.x_offset, 50.0);
    assert_eq!(glyph.components[0].transform.y_offset, 100.0);
    assert_eq!(
        project
            .interpolated_kerning_at("A", "B", &location(0.5, 0.0))
            .unwrap(),
        -130.5
    );
    let outline = project
        .interpolated_outline_at("C", &location(0.5, 0.0))
        .unwrap();
    assert_eq!(
        outline.elements()[0],
        kurbo::PathEl::MoveTo((100.0, 100.0).into())
    );
}

#[test]
fn sparse_glyphs_and_component_cycles_have_explicit_behavior() {
    let (_scratch, mut project) = fixture();
    project
        .edit_source(SourceId(1))
        .unwrap()
        .font
        .default_layer_mut()
        .remove_glyph("B");
    assert_eq!(
        project
            .try_interpolated_at("B", &location(1.0, 0.0))
            .unwrap()
            .width,
        600.123_456_789
    );
    for source in project.edit_sources().iter_mut() {
        source.font.get_glyph_mut("C").unwrap().components[0].base = Name::new("C").unwrap();
    }
    assert!(
        project
            .interpolated_outline_at("C", &location(0.5, 0.0))
            .unwrap_err()
            .contains("cyclic")
    );
    assert!(
        project
            .try_interpolated_at("A", &location(f64::NAN, 0.0))
            .unwrap_err()
            .contains("finite")
    );
}

#[test]
fn designspace_extensions_and_lossy_coordinates_are_rejected() {
    for xml in [
        DESIGNSPACE.replace("<location>", "<info copy=\"1\"/><location>"),
        DESIGNSPACE.replace("xvalue=\"70\"", "xvalue=\"70.123456789\""),
        DESIGNSPACE.replace("tag=\"wght\"", "tag=\"wght\" unknown=\"yes\""),
    ] {
        assert!(designspace_from_str(&xml).is_err());
    }
}

#[test]
fn layer_edits_and_history_round_trip_all_source_data() {
    let (scratch, mut project) = fixture();
    let layer = LayerId {
        source: SourceId(0),
        name: "intermediate".into(),
    };
    let original = project.glyph_layer("A", &layer).unwrap();
    assert!(project.edit_layer("A", &layer, |g| {
        g.width = 731.123_456_789;
        g.note = Some("edited".into());
    }));
    assert!(project.undo_layer("A", &layer, false));
    assert_eq!(project.glyph_layer("A", &layer), Some(original));
    assert!(project.undo_layer("A", &layer, true));
    let before: Vec<_> = (0..4)
        .map(|i| project.source_snapshot(SourceId(i)).unwrap())
        .collect();
    project.save().unwrap();
    let reloaded = Project::load(&scratch.0.join("Font.designspace")).unwrap();
    assert_eq!(reloaded.ds_doc, project.ds_doc);
    for (i, original) in before.iter().enumerate() {
        let current = reloaded.source_snapshot(SourceId(i)).unwrap();
        assert_eq!(current.font_info, original.font_info);
        assert_eq!(current.lib, original.lib);
        assert_eq!(current.features, original.features);
        assert_eq!(current.groups, original.groups);
        assert_eq!(current.kerning, original.kerning);
        assert_eq!(current.data, original.data);
        assert_eq!(current.images, original.images);
        for layer in original.layers.iter() {
            let read = current.layers.get(layer.name()).unwrap();
            assert_eq!(read.lib, layer.lib);
            for glyph in layer.iter() {
                assert_eq!(read.get_glyph(glyph.name()), Some(glyph));
            }
        }
    }
}

#[test]
fn exact_values_and_object_metadata_survive_import_edit_undo_and_save() {
    let (scratch, mut project, fonts) = adversarial_fixture();
    let original = fonts.get("Regular.ufo").unwrap();
    assert_adversarial_source(&project.source_snapshot(SourceId(0)).unwrap(), original);

    project.export_source = Some(scratch.0.join("Font.designspace"));
    project.ds_dirty = true;
    project.save().unwrap();
    let mut reloaded = Project::load(&scratch.0.join("Font.designspace")).unwrap();
    assert_adversarial_source(&reloaded.source_snapshot(SourceId(0)).unwrap(), original);

    let layer = LayerId {
        source: SourceId(0),
        name: "public.default".into(),
    };
    let original_a = original.get_glyph("A").unwrap().clone();
    let mut edited_a = original_a.clone();
    edited_a.width = original.get_glyph("B").unwrap().width;
    edited_a.contours.swap(0, 1);
    edited_a.contours[0].points.swap(0, 1);
    edited_a.contours[0].points[0].x += 0.123_456_789;
    edited_a.components.swap(0, 1);
    edited_a.components[0].transform.xy_scale += 0.000_000_001;
    edited_a.anchors.swap(0, 1);
    edited_a.anchors[0].y += 0.987_654_321;
    assert!(
        reloaded.edit_layer("A", &layer, |glyph| {
            *glyph = edited_a.clone();
        }),
        "adversarial edit must change the layer"
    );
    assert_eq!(
        reloaded.glyph_layer("A", &layer),
        Some(edited_a.clone()),
        "edit must retain every exact field"
    );
    assert!(
        reloaded.undo_layer("A", &layer, false),
        "edit must be undoable"
    );
    assert_eq!(
        reloaded.glyph_layer("A", &layer),
        Some(original_a),
        "undo must restore every exact field"
    );
    assert!(
        reloaded.undo_layer("A", &layer, true),
        "edit must be redoable"
    );
    assert_eq!(
        reloaded.glyph_layer("A", &layer),
        Some(edited_a.clone()),
        "redo must restore every exact edited field"
    );

    reloaded.save().unwrap();
    let saved = Project::load(&scratch.0.join("Font.designspace")).unwrap();
    let mut expected = original.clone();
    expected.default_layer_mut().insert_glyph(edited_a);
    assert_adversarial_source(&saved.source_snapshot(SourceId(0)).unwrap(), &expected);
}

#[test]
fn guarded_legacy_edits_commit_to_canonical_layers_before_save() {
    let (_scratch, mut project) = fixture();
    {
        let mut source = project.active_font_mut();
        let index = source.name_map["B"];
        source.record_undo(index);
        source.set_advance(index, 712.25);
    }
    let layer = LayerId {
        source: SourceId(0),
        name: "public.default".into(),
    };
    assert_eq!(project.glyph_layer("B", &layer).unwrap().width, 712.25);
    assert!(project.undo_layer("B", &layer, false));
    assert_eq!(
        project
            .source_snapshot(SourceId(0))
            .unwrap()
            .get_glyph("B")
            .unwrap()
            .width,
        600.123_456_789
    );
}

#[test]
fn equal_point_counts_do_not_hide_incompatible_types_or_components() {
    let (_scratch, mut project) = fixture();
    let layer = LayerId {
        source: SourceId(1),
        name: "public.default".into(),
    };
    project.edit_layer("B", &layer, |g| {
        g.contours[0].points[1].typ = PointType::Curve;
    });
    assert!(
        project
            .try_interpolated_at("B", &location(0.5, 0.0))
            .unwrap_err()
            .contains("incompatible")
    );
    project.edit_layer("C", &layer, |g| {
        g.components[0].base = Name::new("A").unwrap();
    });
    assert!(
        project
            .try_interpolated_at("C", &location(0.5, 0.0))
            .is_err()
    );
}

#[test]
fn invalid_maps_missing_layers_and_missing_sources_fail_explicitly() {
    let (_scratch, project) = fixture();
    for xml in [
        DESIGNSPACE.replace("output=\"100\"", "output=\"20\""),
        DESIGNSPACE.replace("layer=\"intermediate\"", "layer=\"missing\""),
        DESIGNSPACE.replace("xvalue=\"70\"", "yvalue=\"70\""),
    ] {
        let mut fonts = project.sources().iter();
        let result = Project::from_designspace(designspace_from_str(&xml).unwrap(), |_| {
            let source = fonts.next().unwrap();
            Ok(Master::from_font(
                source.font.clone(),
                source.source_path.clone(),
            ))
        });
        assert!(result.is_err());
    }
    assert!(
        Project::from_designspace(designspace_from_str(DESIGNSPACE).unwrap(), |_| Err(
            "source unavailable".into()
        ))
        .is_err()
    );
}
