// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Babelfont geometry with lossless UFO persistence projections.
//!
//! Babelfont owns paths, anchors and components. UFO payloads retain metadata its
//! model cannot express, plus exact advances and affine coefficients. A projection
//! takes geometry from Babelfont, restoring exact numbers when their corresponding
//! Babelfont value is unchanged. Compilation is the only quantizing boundary.

use std::sync::atomic::{AtomicU64, Ordering};

use babelfont::{Anchor, Component, Layer, Node, NodeType, Shape};

use super::variable::LayerId;

pub(super) fn layer_key(id: &LayerId) -> String {
    format!("{}:{}", id.source.0, id.name)
}

const OBJECT_ID_KEY: &str = "com.runebender.documentObjectId";
static NEXT_OBJECT_ID: AtomicU64 = AtomicU64::new(1);

macro_rules! object_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        struct $name(u64);

        impl $name {
            fn next() -> Self {
                Self(NEXT_OBJECT_ID.fetch_add(1, Ordering::Relaxed))
            }
        }
    };
}

object_id!(ContourId);
object_id!(PointId);
object_id!(ComponentId);
object_id!(AnchorId);

#[derive(Clone, Debug)]
struct PreservedContour {
    id: ContourId,
    metadata: ObjectMetadata,
    points: Vec<PreservedPoint>,
}

#[derive(Clone, Debug)]
struct PreservedPoint {
    id: PointId,
    name: Option<norad::Name>,
    metadata: ObjectMetadata,
}

#[derive(Clone, Debug)]
struct PreservedComponent {
    id: ComponentId,
    transform: norad::AffineTransform,
    metadata: ObjectMetadata,
}

#[derive(Clone, Debug)]
struct PreservedAnchor {
    id: AnchorId,
    color: Option<norad::Color>,
    metadata: ObjectMetadata,
}

#[derive(Clone, Debug)]
struct ObjectMetadata {
    identifier: Option<norad::Identifier>,
    lib: Option<plist::Dictionary>,
}

impl ObjectMetadata {
    fn new(identifier: Option<&norad::Identifier>, lib: Option<&plist::Dictionary>) -> Self {
        Self {
            identifier: identifier.cloned(),
            lib: lib.cloned(),
        }
    }
}

/// Exact UFO values and object metadata that Babelfont cannot represent faithfully.
#[derive(Clone, Debug)]
pub(super) struct LayerPreservation {
    name: String,
    width: f64,
    height: f64,
    codepoints: norad::Codepoints,
    note: Option<String>,
    guidelines: Vec<norad::Guideline>,
    image: Option<norad::Image>,
    lib: plist::Dictionary,
    contours: Vec<PreservedContour>,
    components: Vec<PreservedComponent>,
    anchors: Vec<PreservedAnchor>,
}

fn write_id(format: &mut babelfont::FormatSpecific, id: u64) {
    format.insert(OBJECT_ID_KEY.into(), id.into());
}

fn read_id(format: &babelfont::FormatSpecific) -> Option<u64> {
    format.get(OBJECT_ID_KEY)?.as_u64()
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "the UFO projection retains the exact advance"
)]
pub(super) fn layer_from_ufo(
    glyph: &norad::Glyph,
    id: &LayerId,
    default: bool,
) -> (Layer, LayerPreservation) {
    let mut layer = Layer {
        id: Some(layer_key(id)),
        name: Some(id.name.clone()),
        width: glyph.width as f32,
        master: if default {
            babelfont::LayerType::DefaultForMaster(id.source.0.to_string())
        } else {
            babelfont::LayerType::AssociatedWithMaster(id.source.0.to_string())
        },
        ..Layer::default()
    };
    let mut contours = Vec::with_capacity(glyph.contours.len());
    for contour in &glyph.contours {
        let contour_id = ContourId::next();
        let mut points = Vec::with_capacity(contour.points.len());
        let mut path = babelfont::Path {
            closed: contour.is_closed(),
            ..babelfont::Path::default()
        };
        write_id(&mut path.format_specific, contour_id.0);
        path.nodes = contour
            .points
            .iter()
            .map(|point| {
                let point_id = PointId::next();
                points.push(PreservedPoint {
                    id: point_id,
                    name: point.name.clone(),
                    metadata: ObjectMetadata::new(point.identifier(), point.lib()),
                });
                let mut node = Node {
                    x: point.x,
                    y: point.y,
                    nodetype: match point.typ {
                        norad::PointType::Move => NodeType::Move,
                        norad::PointType::Line => NodeType::Line,
                        norad::PointType::OffCurve => NodeType::OffCurve,
                        norad::PointType::Curve => NodeType::Curve,
                        norad::PointType::QCurve => NodeType::QCurve,
                    },
                    smooth: point.smooth,
                    ..Node::default()
                };
                write_id(&mut node.format_specific, point_id.0);
                node
            })
            .collect();
        layer.shapes.push(Shape::Path(path));
        contours.push(PreservedContour {
            id: contour_id,
            metadata: ObjectMetadata::new(contour.identifier(), contour.lib()),
            points,
        });
    }
    let mut components = Vec::with_capacity(glyph.components.len());
    for component in &glyph.components {
        let component_id = ComponentId::next();
        let mut output = Component {
            reference: component.base.as_str().into(),
            transform: affine(component.transform).into(),
            location: std::iter::empty().collect(),
            format_specific: babelfont::FormatSpecific::default(),
        };
        write_id(&mut output.format_specific, component_id.0);
        layer.shapes.push(Shape::Component(output));
        components.push(PreservedComponent {
            id: component_id,
            transform: component.transform,
            metadata: ObjectMetadata::new(component.identifier(), component.lib()),
        });
    }
    let mut anchors = Vec::with_capacity(glyph.anchors.len());
    layer.anchors = glyph
        .anchors
        .iter()
        .map(|anchor| {
            let anchor_id = AnchorId::next();
            anchors.push(PreservedAnchor {
                id: anchor_id,
                color: anchor.color,
                metadata: ObjectMetadata::new(anchor.identifier(), anchor.lib()),
            });
            let mut output = Anchor {
                x: anchor.x,
                y: anchor.y,
                name: anchor
                    .name
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                ..Anchor::default()
            };
            write_id(&mut output.format_specific, anchor_id.0);
            output
        })
        .collect();
    (
        layer,
        LayerPreservation {
            name: glyph.name().to_string(),
            width: glyph.width,
            height: glyph.height,
            codepoints: glyph.codepoints.clone(),
            note: glyph.note.clone(),
            guidelines: glyph.guidelines.clone(),
            image: glyph.image.clone(),
            lib: glyph.lib.clone(),
            contours,
            components,
            anchors,
        },
    )
}

pub(super) fn reconcile_layer_from_ufo(
    glyph: &norad::Glyph,
    id: &LayerId,
    default: bool,
    previous_layer: &Layer,
    previous: &LayerPreservation,
) -> (Layer, LayerPreservation) {
    let old = project_layer(previous_layer, previous);
    let (mut layer, mut preservation) = layer_from_ufo(glyph, id, default);

    let mut used_contours = vec![false; old.contours.len()];
    for (index, (contour, path)) in glyph
        .contours
        .iter()
        .zip(layer.shapes.iter_mut().filter_map(|shape| match shape {
            Shape::Path(path) => Some(path),
            Shape::Component(_) => None,
        }))
        .enumerate()
    {
        let old_index = match_index(&old.contours, &used_contours, |candidate| {
            contour.identifier().is_some() && contour.identifier() == candidate.identifier()
        })
        .or_else(|| {
            match_index(&old.contours, &used_contours, |candidate| {
                contour == candidate
            })
        })
        .or_else(|| {
            match_index(&old.contours, &used_contours, |candidate| {
                contour_signature_matches(contour, candidate)
            })
        });
        let Some(old_index) = old_index else {
            continue;
        };
        used_contours[old_index] = true;
        let old_preserved = &previous.contours[old_index];
        let new_preserved = &mut preservation.contours[index];
        new_preserved.id = old_preserved.id;
        write_id(&mut path.format_specific, old_preserved.id.0);

        let mut used_points = vec![false; old.contours[old_index].points.len()];
        for (point_index, (point, node)) in contour.points.iter().zip(&mut path.nodes).enumerate() {
            let old_point =
                match_index(&old.contours[old_index].points, &used_points, |candidate| {
                    point.identifier().is_some() && point.identifier() == candidate.identifier()
                })
                .or_else(|| {
                    match_index(&old.contours[old_index].points, &used_points, |candidate| {
                        point == candidate
                    })
                })
                .or_else(|| {
                    match_index(&old.contours[old_index].points, &used_points, |candidate| {
                        point_metadata_matches(point, candidate)
                    })
                });
            let Some(old_point) = old_point else {
                continue;
            };
            used_points[old_point] = true;
            let id = old_preserved.points[old_point].id;
            preservation.contours[index].points[point_index].id = id;
            write_id(&mut node.format_specific, id.0);
        }
    }

    let mut used_components = vec![false; old.components.len()];
    for (index, (component, shape)) in glyph
        .components
        .iter()
        .zip(layer.shapes.iter_mut().filter_map(|shape| match shape {
            Shape::Component(component) => Some(component),
            Shape::Path(_) => None,
        }))
        .enumerate()
    {
        let old_index = match_index(&old.components, &used_components, |candidate| {
            component.identifier().is_some() && component.identifier() == candidate.identifier()
        })
        .or_else(|| {
            match_index(&old.components, &used_components, |candidate| {
                component == candidate
            })
        })
        .or_else(|| {
            match_index(&old.components, &used_components, |candidate| {
                component_metadata_matches(component, candidate)
            })
        });
        let Some(old_index) = old_index else {
            continue;
        };
        used_components[old_index] = true;
        let id = previous.components[old_index].id;
        preservation.components[index].id = id;
        write_id(&mut shape.format_specific, id.0);
    }

    let mut used_anchors = vec![false; old.anchors.len()];
    for (index, (anchor, projected)) in glyph.anchors.iter().zip(&mut layer.anchors).enumerate() {
        let old_index = match_index(&old.anchors, &used_anchors, |candidate| {
            anchor.identifier().is_some() && anchor.identifier() == candidate.identifier()
        })
        .or_else(|| match_index(&old.anchors, &used_anchors, |candidate| anchor == candidate))
        .or_else(|| {
            match_index(&old.anchors, &used_anchors, |candidate| {
                anchor_metadata_matches(anchor, candidate)
            })
        });
        let Some(old_index) = old_index else {
            continue;
        };
        used_anchors[old_index] = true;
        let id = previous.anchors[old_index].id;
        preservation.anchors[index].id = id;
        write_id(&mut projected.format_specific, id.0);
    }

    (layer, preservation)
}

fn match_index<T>(items: &[T], used: &[bool], predicate: impl Fn(&T) -> bool) -> Option<usize> {
    let mut matches = items
        .iter()
        .enumerate()
        .filter(|(index, item)| !used[*index] && predicate(item))
        .map(|(index, _)| index);
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn contour_signature_matches(a: &norad::Contour, b: &norad::Contour) -> bool {
    let object_metadata = a.identifier().is_some() || a.lib().is_some();
    let point_metadata = a
        .points
        .iter()
        .any(|point| point.identifier().is_some() || point.lib().is_some() || point.name.is_some());
    (object_metadata || point_metadata)
        && a.identifier() == b.identifier()
        && a.lib() == b.lib()
        && a.points.len() == b.points.len()
        && a.points.iter().all(|a| {
            b.points
                .iter()
                .filter(|b| {
                    a.identifier() == b.identifier()
                        && a.lib() == b.lib()
                        && a.name == b.name
                        && a.typ == b.typ
                })
                .count()
                == 1
        })
}

fn point_metadata_matches(a: &norad::ContourPoint, b: &norad::ContourPoint) -> bool {
    (a.identifier().is_some() || a.lib().is_some() || a.name.is_some())
        && a.identifier() == b.identifier()
        && a.lib() == b.lib()
        && a.name == b.name
        && a.typ == b.typ
}

fn component_metadata_matches(a: &norad::Component, b: &norad::Component) -> bool {
    a.identifier() == b.identifier() && a.lib() == b.lib() && a.base == b.base
}

fn anchor_metadata_matches(a: &norad::Anchor, b: &norad::Anchor) -> bool {
    (a.identifier().is_some() || a.lib().is_some() || a.name.is_some() || a.color.is_some())
        && a.identifier() == b.identifier()
        && a.lib() == b.lib()
        && a.name == b.name
        && a.color == b.color
}

fn affine(t: norad::AffineTransform) -> kurbo::Affine {
    kurbo::Affine::new([
        t.x_scale, t.xy_scale, t.yx_scale, t.y_scale, t.x_offset, t.y_offset,
    ])
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "compare with the original narrowed Babelfont advance"
)]
pub(super) fn project_layer(layer: &Layer, preserved: &LayerPreservation) -> norad::Glyph {
    let mut glyph = norad::Glyph::new(&preserved.name);
    glyph.width = preserved.width;
    glyph.height = preserved.height;
    glyph.codepoints.clone_from(&preserved.codepoints);
    glyph.note.clone_from(&preserved.note);
    glyph.guidelines.clone_from(&preserved.guidelines);
    glyph.image.clone_from(&preserved.image);
    glyph.lib.clone_from(&preserved.lib);
    if layer.width != preserved.width as f32 {
        glyph.width = f64::from(layer.width);
    }
    glyph.contours = layer
        .paths()
        .map(|path| {
            let preserved_contour = read_id(&path.format_specific)
                .and_then(|id| preserved.contours.iter().find(|item| item.id.0 == id));
            let points = path
                .nodes
                .iter()
                .map(|node| {
                    let original = read_id(&node.format_specific).and_then(|id| {
                        let item = preserved_contour?;
                        item.points.iter().find(|point| point.id.0 == id)
                    });
                    let typ = match node.nodetype {
                        NodeType::Move => norad::PointType::Move,
                        NodeType::Line => norad::PointType::Line,
                        NodeType::OffCurve => norad::PointType::OffCurve,
                        NodeType::Curve => norad::PointType::Curve,
                        NodeType::QCurve => norad::PointType::QCurve,
                    };
                    let mut point = norad::ContourPoint::new(
                        node.x,
                        node.y,
                        typ,
                        node.smooth,
                        original.and_then(|item| item.name.clone()),
                        original.and_then(|item| item.metadata.identifier.clone()),
                    );
                    if let Some(lib) = original.and_then(|item| item.metadata.lib.clone()) {
                        point.replace_lib(lib);
                    }
                    point
                })
                .collect();
            let mut contour = norad::Contour::new(
                points,
                preserved_contour.and_then(|item| item.metadata.identifier.clone()),
            );
            if let Some(lib) = preserved_contour.and_then(|item| item.metadata.lib.clone()) {
                contour.replace_lib(lib);
            }
            contour
        })
        .collect();
    glyph.components = layer
        .components()
        .map(|component| {
            let base = norad::Name::new(&component.reference).expect("validated glyph name");
            let original = read_id(&component.format_specific)
                .and_then(|id| preserved.components.iter().find(|item| item.id.0 == id));
            let exact =
                original.map_or_else(norad::AffineTransform::default, |item| item.transform);
            let decomposed: babelfont::DecomposedAffine = affine(exact).into();
            let transform = if decomposed == component.transform {
                exact
            } else {
                let [x_scale, xy_scale, yx_scale, y_scale, x_offset, y_offset] =
                    component.transform.as_affine().as_coeffs();
                norad::AffineTransform {
                    x_scale,
                    xy_scale,
                    yx_scale,
                    y_scale,
                    x_offset,
                    y_offset,
                }
            };
            let mut output = norad::Component::new(
                base,
                transform,
                original.and_then(|item| item.metadata.identifier.clone()),
            );
            if let Some(lib) = original.and_then(|item| item.metadata.lib.clone()) {
                output.replace_lib(lib);
            }
            output
        })
        .collect();
    glyph.anchors = layer
        .anchors
        .iter()
        .map(|anchor| {
            let original = read_id(&anchor.format_specific)
                .and_then(|id| preserved.anchors.iter().find(|item| item.id.0 == id));
            let mut output = norad::Anchor::new(
                anchor.x,
                anchor.y,
                (!anchor.name.is_empty())
                    .then(|| norad::Name::new(&anchor.name).expect("validated anchor name")),
                original.and_then(|item| item.color),
                original.and_then(|item| item.metadata.identifier.clone()),
            );
            if let Some(lib) = original.and_then(|item| item.metadata.lib.clone()) {
                output.replace_lib(lib);
            }
            output
        })
        .collect();
    glyph
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn reconciliation_retains_identity_across_legacy_edits_and_reorder() {
        let mut glyph = norad::Glyph::new("A");
        glyph.contours.push(norad::Contour::new(
            vec![
                norad::ContourPoint::new(
                    10.0,
                    20.0,
                    norad::PointType::Line,
                    false,
                    Some(norad::Name::new("first").unwrap()),
                    Some(identifier("point.first")),
                ),
                norad::ContourPoint::new(
                    30.0,
                    40.0,
                    norad::PointType::Line,
                    false,
                    Some(norad::Name::new("second").unwrap()),
                    Some(identifier("point.second")),
                ),
            ],
            Some(identifier("contour.original")),
        ));
        for (name, x) in [("base.first", 10.0), ("base.second", 20.0)] {
            glyph.components.push(norad::Component::new(
                norad::Name::new(name).unwrap(),
                norad::AffineTransform {
                    x_offset: x,
                    ..norad::AffineTransform::default()
                },
                Some(identifier(name)),
            ));
        }
        for (name, x) in [("top", 10.0), ("bottom", 20.0)] {
            glyph.anchors.push(norad::Anchor::new(
                x,
                100.0,
                Some(norad::Name::new(name).unwrap()),
                None,
                Some(identifier(name)),
            ));
        }
        let id = LayerId {
            source: super::super::variable::SourceId(0),
            name: "public.default".into(),
        };
        let (layer, preservation) = layer_from_ufo(&glyph, &id, true);
        let old_point_ids: Vec<_> = layer
            .paths()
            .next()
            .unwrap()
            .nodes
            .iter()
            .map(|node| read_id(&node.format_specific).unwrap())
            .collect();
        let old_component_ids: Vec<_> = layer
            .components()
            .map(|component| read_id(&component.format_specific).unwrap())
            .collect();
        let old_anchor_ids: Vec<_> = layer
            .anchors
            .iter()
            .map(|anchor| read_id(&anchor.format_specific).unwrap())
            .collect();

        let mut edited = project_layer(&layer, &preservation);
        edited.contours[0].points.swap(0, 1);
        edited.contours[0].points[0].x = 333.0;
        edited.components.swap(0, 1);
        edited.components[0].transform.x_offset = 222.0;
        edited.anchors.swap(0, 1);
        edited.anchors[0].x = 111.0;

        let (reconciled, preservation) =
            reconcile_layer_from_ufo(&edited, &id, true, &layer, &preservation);
        let point_ids: Vec<_> = reconciled
            .paths()
            .next()
            .unwrap()
            .nodes
            .iter()
            .map(|node| read_id(&node.format_specific).unwrap())
            .collect();
        let component_ids: Vec<_> = reconciled
            .components()
            .map(|component| read_id(&component.format_specific).unwrap())
            .collect();
        let anchor_ids: Vec<_> = reconciled
            .anchors
            .iter()
            .map(|anchor| read_id(&anchor.format_specific).unwrap())
            .collect();
        assert_eq!(point_ids, [old_point_ids[1], old_point_ids[0]]);
        assert_eq!(component_ids, [old_component_ids[1], old_component_ids[0]]);
        assert_eq!(anchor_ids, [old_anchor_ids[1], old_anchor_ids[0]]);
        assert_eq!(project_layer(&reconciled, &preservation), edited);
    }
}
