// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(test)]

//! Canonical layer and UFO preservation tests.

use super::*;

#[test]
fn layer_snapshot_rebind_requires_the_exact_old_address_and_layer() {
    let mut glyph = norad::Glyph::new("A");
    glyph.width = 500.125;
    glyph.note = Some("retain me".into());
    let layer_id = LayerId {
        source: super::super::variable::SourceId(7),
        name: "public.default".into(),
    };
    let old = super::super::variable::GlyphLayerAddress {
        glyph: "A".into(),
        layer: layer_id.clone(),
    };
    let (layer, preserved) = layer_from_ufo(&glyph, &layer_id, true);
    let mut snapshot = CanonicalLayerSnapshot::new(old.clone(), layer, preserved);
    let original = snapshot.clone();

    let stale = super::super::variable::GlyphLayerAddress {
        glyph: "B".into(),
        layer: layer_id.clone(),
    };
    let renamed = super::super::variable::GlyphLayerAddress {
        glyph: "A.alt".into(),
        layer: layer_id.clone(),
    };
    assert!(!snapshot.rebind_glyph(&stale, &renamed));
    assert_eq!(snapshot, original);

    let wrong_layer = super::super::variable::GlyphLayerAddress {
        glyph: "A.alt".into(),
        layer: LayerId {
            source: layer_id.source,
            name: "background".into(),
        },
    };
    assert!(!snapshot.rebind_glyph(&old, &wrong_layer));
    assert_eq!(snapshot, original);

    assert!(snapshot.rebind_glyph(&old, &renamed));
    assert_eq!(snapshot.address(), &renamed);
    let (layer, preserved) = snapshot.into_parts();
    let projected = project_layer(&layer, &preserved);
    assert_eq!(projected.name().as_str(), "A.alt");
    assert_eq!(projected.width, 500.125);
    assert_eq!(projected.note.as_deref(), Some("retain me"));
}

fn identifier(value: &str) -> norad::Identifier {
    norad::Identifier::new(value).unwrap()
}

fn object_lib(key: &str, value: &str) -> plist::Dictionary {
    let mut lib = plist::Dictionary::new();
    lib.insert(key.into(), value.into());
    lib
}

#[test]
fn projection_follows_object_identity_after_reorder_and_insert() {
    let mut glyph = norad::Glyph::new("A");
    let mut first = norad::ContourPoint::new(
        10.0,
        20.0,
        norad::PointType::Line,
        false,
        Some(norad::Name::new("first").unwrap()),
        Some(identifier("point.first")),
    );
    first.replace_lib(object_lib("owner", "first"));
    let second = norad::ContourPoint::new(
        30.0,
        40.0,
        norad::PointType::Line,
        false,
        Some(norad::Name::new("second").unwrap()),
        Some(identifier("point.second")),
    );
    let mut contour =
        norad::Contour::new(vec![first, second], Some(identifier("contour.original")));
    contour.replace_lib(object_lib("contour", "metadata"));
    glyph.contours.push(contour);
    let mut top = norad::Anchor::new(
        10.0,
        20.0,
        Some(norad::Name::new("top").unwrap()),
        None,
        Some(identifier("anchor.top")),
    );
    top.replace_lib(object_lib("anchor", "metadata"));
    glyph.anchors.push(top);
    glyph.anchors.push(norad::Anchor::new(
        30.0,
        40.0,
        Some(norad::Name::new("bottom").unwrap()),
        None,
        Some(identifier("anchor.bottom")),
    ));
    let first_transform = norad::AffineTransform {
        x_scale: 1.000_000_000_000_1,
        xy_scale: 0.125,
        yx_scale: -0.25,
        y_scale: 0.999_999_999_999_9,
        x_offset: 12.345_678_901_234,
        y_offset: -98.765_432_109_876,
    };
    let mut first_component = norad::Component::new(
        norad::Name::new("base.first").unwrap(),
        first_transform,
        Some(identifier("component.first")),
    );
    first_component.replace_lib(object_lib("component", "first"));
    glyph.components.push(first_component);
    glyph.components.push(norad::Component::new(
        norad::Name::new("base.second").unwrap(),
        norad::AffineTransform {
            x_offset: 50.0,
            ..norad::AffineTransform::default()
        },
        Some(identifier("component.second")),
    ));
    let id = LayerId {
        source: super::super::variable::SourceId(0),
        name: "public.default".into(),
    };
    let (mut layer, preserved) = layer_from_ufo(&glyph, &id, true);
    let Shape::Path(path) = &mut layer.shapes[0] else {
        panic!("first shape is a path");
    };
    path.nodes.swap(0, 1);
    path.nodes.insert(
        0,
        Node {
            x: 5.0,
            y: 5.0,
            nodetype: NodeType::Line,
            ..Node::default()
        },
    );
    layer.shapes.swap(1, 2);
    layer.anchors.swap(0, 1);

    let output = project_layer(&layer, &preserved);
    assert_eq!(
        output.contours[0].identifier(),
        Some(&identifier("contour.original"))
    );
    assert_eq!(output.contours[0].points[0].identifier(), None);
    assert_eq!(
        output.contours[0].points[1].identifier(),
        Some(&identifier("point.second"))
    );
    assert_eq!(
        output.contours[0].points[2].identifier(),
        Some(&identifier("point.first"))
    );
    assert_eq!(
        output.contours[0].points[2].lib().unwrap()["owner"],
        "first".into()
    );
    assert_eq!(
        output.anchors[0].identifier(),
        Some(&identifier("anchor.bottom"))
    );
    assert_eq!(
        output.anchors[1].identifier(),
        Some(&identifier("anchor.top"))
    );
    assert_eq!(
        output.anchors[1].lib().unwrap()["anchor"],
        "metadata".into()
    );
    assert_eq!(output.components[0].base.as_str(), "base.second");
    assert_eq!(
        output.components[0].identifier(),
        Some(&identifier("component.second"))
    );
    assert_eq!(output.components[1].base.as_str(), "base.first");
    assert_eq!(output.components[1].transform, first_transform);
    assert_eq!(
        output.components[1].identifier(),
        Some(&identifier("component.first"))
    );
    assert_eq!(
        output.components[1].lib().unwrap()["component"],
        "first".into()
    );
}

#[test]
fn smart_component_values_follow_component_identity() {
    use super::super::model::smart_components::{
        SMART_COMPONENT_AXES_KEY, SMART_COMPONENT_POLE_KEY, SMART_COMPONENT_VALUES_KEY,
    };

    let mut glyph = norad::Glyph::new("smart-user");
    for name in ["part.first", "part.second"] {
        glyph.components.push(norad::Component::new(
            norad::Name::new(name).unwrap(),
            norad::AffineTransform::default(),
            None,
        ));
    }
    let axis = [
        ("name".to_string(), plist::Value::String("Width".into())),
        (
            "bottomValue".to_string(),
            plist::Value::Integer(0_i64.into()),
        ),
        ("topValue".to_string(), plist::Value::Real(100.0)),
    ]
    .into_iter()
    .collect();
    glyph.lib.insert(
        SMART_COMPONENT_AXES_KEY.into(),
        plist::Value::Array(vec![plist::Value::Dictionary(axis)]),
    );
    glyph.lib.insert(
        SMART_COMPONENT_VALUES_KEY.into(),
        plist::Value::Array(vec![
            plist::Value::Dictionary(
                [("Width".to_string(), plist::Value::Integer(25_i64.into()))]
                    .into_iter()
                    .collect(),
            ),
            plist::Value::Dictionary(
                [("Width".to_string(), plist::Value::Real(75.0))]
                    .into_iter()
                    .collect(),
            ),
        ]),
    );
    glyph.lib.insert(
        SMART_COMPONENT_POLE_KEY.into(),
        plist::Value::Dictionary(
            [("Width".to_string(), plist::Value::Integer(2_i64.into()))]
                .into_iter()
                .collect(),
        ),
    );
    let id = LayerId {
        source: super::super::variable::SourceId(0),
        name: "public.default".into(),
    };
    let (mut layer, preserved) = layer_from_ufo(&glyph, &id, true);
    assert!(!preserved.lib.contains_key(SMART_COMPONENT_AXES_KEY));
    assert!(!preserved.lib.contains_key(SMART_COMPONENT_VALUES_KEY));
    assert!(!preserved.lib.contains_key(SMART_COMPONENT_POLE_KEY));
    let view = LayerView::new(&layer, &preserved);
    let components = view.components().map(ComponentView::id).collect::<Vec<_>>();
    assert_eq!(
        view.smart_component_value(components[0], "Width"),
        Some(25.0)
    );
    assert_eq!(
        view.smart_component_value(components[1], "Width"),
        Some(75.0)
    );
    assert!(view.smart_component_pole().unwrap().is_top("Width"));

    layer.shapes.swap(0, 1);
    let output = project_layer(&layer, &preserved);
    assert_eq!(
        output.lib[SMART_COMPONENT_VALUES_KEY],
        plist::Value::Array(vec![
            plist::Value::Dictionary(
                [("Width".to_string(), plist::Value::Real(75.0))]
                    .into_iter()
                    .collect(),
            ),
            plist::Value::Dictionary(
                [("Width".to_string(), plist::Value::Integer(25_i64.into()))]
                    .into_iter()
                    .collect(),
            ),
        ])
    );
    assert_eq!(
        output.lib[SMART_COMPONENT_AXES_KEY],
        glyph.lib[SMART_COMPONENT_AXES_KEY]
    );
    assert_eq!(
        output.lib[SMART_COMPONENT_POLE_KEY],
        glyph.lib[SMART_COMPONENT_POLE_KEY]
    );
}

#[test]
fn component_alignment_edit_assigns_one_stable_boundary_identifier() {
    let mut glyph = norad::Glyph::new("component-user");
    glyph.components.push(norad::Component::new(
        norad::Name::new("base").unwrap(),
        norad::AffineTransform::default(),
        None,
    ));
    let layer_id = LayerId {
        source: super::super::variable::SourceId(0),
        name: "public.default".into(),
    };
    let (layer, preserved) = layer_from_ufo(&glyph, &layer_id, true);
    let component = LayerView::new(&layer, &preserved)
        .components()
        .next()
        .unwrap()
        .id();
    let mut draft = LayerEditDraft::new(layer, preserved);
    assert!(
        draft
            .set_component_alignment_disabled(component, true)
            .unwrap()
    );
    let (layer, preserved) = draft.into_parts();
    let first = project_layer(&layer, &preserved);
    let second = project_layer(&layer, &preserved);
    let identifier = first.components[0]
        .identifier()
        .expect("alignment metadata receives a stable identifier")
        .clone();
    assert_eq!(second.components[0].identifier(), Some(&identifier));

    let mut enabled = LayerEditDraft::new(layer, preserved);
    assert!(
        enabled
            .set_component_alignment_disabled(component, false)
            .unwrap()
    );
    let (enabled_layer, enabled_preserved) = enabled.into_parts();
    let enabled = project_layer(&enabled_layer, &enabled_preserved);
    assert_eq!(enabled.components[0].identifier(), Some(&identifier));
}
