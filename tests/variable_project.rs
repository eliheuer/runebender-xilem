// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! End-to-end contracts for canonical glyph layers and UFO/Designspace projections.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use norad::{Anchor, Component, Contour, ContourPoint, Font, Glyph, Name, PointType};
use runebender::document::font_memory::designspace_from_str;
use runebender::document::project::{Master, Project};
use runebender::document::var_model::Location;
use runebender::document::variable::{LayerId, SourceId};

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

fn location(weight: f64, width: f64) -> Location {
    [("Weight".into(), weight), ("Width".into(), width)].into()
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
fn source_authoring_keeps_identity_and_round_trips_the_designspace() {
    let (scratch, mut project) = fixture();
    let original = project.source_snapshot(SourceId(1)).unwrap();
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
