// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical glyph-layer metadata preserves exact UFO payloads until a typed edit.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use runebender::font::DocumentEditError;
use runebender::font::history::HistoryDirection;
use runebender::font::model::glyph_metadata::{
    COMPOSITION_RECIPE_KEY, LEFT_METRICS_KEY, MARK_COLOR_KEY, MARK_LABEL_KEY, METABALLS_KEY,
    MarkColor, Metaball, MetaballGroup, Metaballs, RIGHT_METRICS_KEY,
};
use runebender::font::project::{DocumentEditOutcome, Project, SourceInput};
use runebender::font::variable::{GlyphLayerAddress, SourceId};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "runebender-layer-metadata-{}-{}",
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
    let project = Project::from_source(SourceInput::from_font(
        font,
        PathBuf::from("LayerMetadata.ufo"),
    ));
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

    let exact = project.encode_ufo_source(SourceId(0)).unwrap();
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
            .encode_ufo_source(SourceId(0))
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
            .set_mark_color(Some(MarkColor {
                red: 0.4,
                green: 0.5,
                blue: 0.6,
                alpha: 1.0,
            }))
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
        rejected.draft_mut().set_mark_color(Some(MarkColor {
            red: f64::NAN,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        })),
        Err(DocumentEditError::InvalidLayerMetadata)
    );
    assert_eq!(project.document_snapshot(), document);
    assert_eq!(project.document_revision(), revision);

    project
        .replay_document_layer_history(&address, HistoryDirection::Undo)
        .unwrap();
    let restored = project.encode_ufo_source(SourceId(0)).unwrap();
    assert_eq!(restored.get_glyph("A").unwrap().lib, exact_lib);
}

#[test]
fn semantic_mark_updates_both_keys_atomically_and_survives_save() {
    let scratch = Scratch::new();
    let path = scratch.0.join("SemanticMark.ufo");
    let mut font = norad::Font::new();
    let mut glyph = norad::Glyph::new("A");
    glyph.lib.insert(
        MARK_COLOR_KEY.into(),
        plist::Value::String(" 0.40, 0.50, 0.60, 1.0 ".into()),
    );
    glyph
        .lib
        .insert(MARK_LABEL_KEY.into(), plist::Value::String("blue".into()));
    glyph
        .lib
        .insert("future.key".into(), plist::Value::String("exact".into()));
    font.default_layer_mut().insert_glyph(glyph);
    font.save(&path).unwrap();

    let mut project = Project::load(&path).unwrap();
    let address = GlyphLayerAddress {
        glyph: "A".into(),
        layer: project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer(),
    };
    let blue = MarkColor {
        red: 0.4,
        green: 0.5,
        blue: 0.6,
        alpha: 1.0,
    };
    let exact_lib = project
        .encode_ufo_source(SourceId(0))
        .unwrap()
        .get_glyph("A")
        .unwrap()
        .lib
        .clone();
    let view = project
        .document_layer(&address.glyph, &address.layer)
        .unwrap();
    assert_eq!(view.mark_label().unwrap(), Some("blue"));
    assert_eq!(view.mark_color().unwrap(), Some(blue));

    let revision = project.document_revision();
    let mut no_op = project.begin_document_layer_transaction(&address).unwrap();
    assert!(
        !no_op
            .draft_mut()
            .set_mark(Some("blue"), Some(blue))
            .unwrap()
    );
    assert_eq!(
        project.commit_document_layer_transaction(no_op).unwrap(),
        DocumentEditOutcome::Unchanged { revision }
    );
    assert_eq!(
        project
            .encode_ufo_source(SourceId(0))
            .unwrap()
            .get_glyph("A")
            .unwrap()
            .lib,
        exact_lib
    );

    let document = project.document_snapshot();
    let mut rejected = project.begin_document_layer_transaction(&address).unwrap();
    assert_eq!(
        rejected.draft_mut().set_mark(
            Some("red"),
            Some(MarkColor {
                red: f64::NAN,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0,
            }),
        ),
        Err(DocumentEditError::InvalidLayerMetadata)
    );
    assert_eq!(project.document_snapshot(), document);
    assert_eq!(project.document_revision(), revision);

    let green = MarkColor {
        red: 0.1,
        green: 0.8,
        blue: 0.2,
        alpha: 1.0,
    };
    let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
    assert!(
        transaction
            .draft_mut()
            .set_mark(Some("green"), Some(green))
            .unwrap()
    );
    let DocumentEditOutcome::Changed { change, .. } = project
        .commit_document_layer_transaction(transaction)
        .unwrap()
    else {
        panic!("semantic mark edit did not commit")
    };
    assert!(change.metadata_changed());
    assert!(!change.geometry_changed());
    let projected = project.encode_ufo_source(SourceId(0)).unwrap();
    let projected = projected.get_glyph("A").unwrap();
    assert_eq!(
        projected.lib.get(MARK_COLOR_KEY),
        Some(&plist::Value::String("0.1,0.8,0.2,1".into()))
    );
    assert_eq!(
        projected.lib.get(MARK_LABEL_KEY),
        Some(&plist::Value::String("green".into()))
    );
    assert_eq!(
        projected.lib.get("future.key"),
        Some(&plist::Value::String("exact".into()))
    );

    project.save().unwrap();
    let mut reloaded = Project::load(&path).unwrap();
    let reloaded_address = GlyphLayerAddress {
        glyph: "A".into(),
        layer: reloaded
            .document_source(SourceId(0))
            .unwrap()
            .default_layer(),
    };
    let reloaded_view = reloaded
        .document_layer(&reloaded_address.glyph, &reloaded_address.layer)
        .unwrap();
    assert_eq!(reloaded_view.mark_label().unwrap(), Some("green"));
    assert_eq!(reloaded_view.mark_color().unwrap(), Some(green));

    let mut clear = reloaded
        .begin_document_layer_transaction(&reloaded_address)
        .unwrap();
    assert!(clear.draft_mut().set_mark(None, None).unwrap());
    reloaded.commit_document_layer_transaction(clear).unwrap();
    let cleared = reloaded.encode_ufo_source(SourceId(0)).unwrap();
    let cleared = cleared.get_glyph("A").unwrap();
    assert!(!cleared.lib.contains_key(MARK_COLOR_KEY));
    assert!(!cleared.lib.contains_key(MARK_LABEL_KEY));
}

#[test]
fn cubic_metaballs_survive_canonical_conversion_save_and_undo() {
    use kurbo::{ParamCurve, Point};
    use runebender::outline::glyph_paths::ordinary_layer_contours_to_bezpath;
    use runebender::outline::metaballs::OutlineOptions;

    let (mut project, address) = fixture();
    let mut source = source_metaballs();
    source.groups[0].threshold = 0.5;
    source.groups[0].balls[0].stiffness = 2.0;
    let ball = source.groups[0].balls[0].clone();
    let mut setup = project.begin_document_layer_transaction(&address).unwrap();
    setup.draft_mut().set_metaballs(source.clone()).unwrap();
    project.commit_document_layer_transaction(setup).unwrap();
    let before = project.encode_ufo_source(SourceId(0)).unwrap();
    let before = before.get_glyph("A").unwrap().clone();

    let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
    assert_eq!(
        transaction
            .draft_mut()
            .collapse_metaballs(None, OutlineOptions::default())
            .unwrap(),
        1
    );
    project
        .commit_document_layer_transaction(transaction)
        .unwrap();
    let converted = project.encode_ufo_source(SourceId(0)).unwrap();
    let converted = converted.get_glyph("A").unwrap();
    assert!(!converted.lib.contains_key(METABALLS_KEY));
    assert_eq!(
        converted.lib.get("future.key"),
        before.lib.get("future.key")
    );
    // Exercise GLIF serialization too: control ordering must survive both format boundaries.
    let reloaded = norad::Glyph::parse_raw(&converted.encode_xml().unwrap()).unwrap();
    let mut font = norad::Font::new();
    font.default_layer_mut().insert_glyph(reloaded.clone());
    let loaded = Project::from_source(SourceInput::from_font(font, PathBuf::from("Roundtrip.ufo")));
    let layer = loaded.document_source(SourceId(0)).unwrap().default_layer();
    let path = ordinary_layer_contours_to_bezpath(loaded.document_layer("A", &layer).unwrap());
    assert_eq!(path.segments().count(), 4);
    assert_eq!(
        reloaded.contours[0]
            .points
            .iter()
            .filter(|p| p.smooth)
            .count(),
        4
    );
    let center = Point::new(ball.x, ball.y);
    let radius = ball.radius * (1.0 - 0.25_f64.cbrt()).sqrt();
    for segment in path.segments() {
        let c = segment.to_cubic();
        for handle in [c.p1 - c.p0, c.p3 - c.p2] {
            assert!(handle.x == 0.0 || handle.y == 0.0);
        }
        for i in 0..=100 {
            assert!((c.eval(f64::from(i) / 100.0).distance(center) - radius).abs() < 0.025);
        }
    }
    project
        .replay_document_layer_history(&address, HistoryDirection::Undo)
        .unwrap();
    let restored = project.encode_ufo_source(SourceId(0)).unwrap();
    assert_eq!(
        restored.get_glyph("A").unwrap().encode_xml().unwrap(),
        before.encode_xml().unwrap()
    );
    project
        .replay_document_layer_history(&address, HistoryDirection::Redo)
        .unwrap();
    let redone = project.encode_ufo_source(SourceId(0)).unwrap();
    assert_eq!(redone.get_glyph("A").unwrap().contours, converted.contours);
}
