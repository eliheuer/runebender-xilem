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
    points: Vec<PointId>,
}

/// Exact UFO payload plus stable identities for one transitional glyph layer.
///
/// The payload remains until M01 moves each field into typed extensions, but
/// projection already follows object identity instead of array position.
#[derive(Clone, Debug)]
pub(super) struct LayerPreservation {
    glyph: norad::Glyph,
    contours: Vec<PreservedContour>,
    components: Vec<ComponentId>,
    anchors: Vec<AnchorId>,
}

impl LayerPreservation {
    pub(super) fn glyph(&self) -> &norad::Glyph {
        &self.glyph
    }
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
        let mut point_ids = Vec::with_capacity(contour.points.len());
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
                point_ids.push(point_id);
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
            points: point_ids,
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
        components.push(component_id);
    }
    let mut anchors = Vec::with_capacity(glyph.anchors.len());
    layer.anchors = glyph
        .anchors
        .iter()
        .map(|anchor| {
            let anchor_id = AnchorId::next();
            anchors.push(anchor_id);
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
            glyph: glyph.clone(),
            contours,
            components,
            anchors,
        },
    )
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
    let mut glyph = preserved.glyph.clone();
    if layer.width != preserved.glyph.width as f32 {
        glyph.width = f64::from(layer.width);
    }
    glyph.contours = layer
        .paths()
        .map(|path| {
            let preserved_contour = read_id(&path.format_specific)
                .and_then(|id| preserved.contours.iter().find(|item| item.id.0 == id));
            let mut contour = preserved_contour
                .and_then(|item| {
                    let index = preserved
                        .contours
                        .iter()
                        .position(|other| other.id == item.id)?;
                    preserved.glyph.contours.get(index).cloned()
                })
                .unwrap_or_default();
            contour.points = path
                .nodes
                .iter()
                .map(|node| {
                    let original = read_id(&node.format_specific).and_then(|id| {
                        let item = preserved_contour?;
                        let index = item.points.iter().position(|point| point.0 == id)?;
                        let contour_index = preserved
                            .contours
                            .iter()
                            .position(|other| other.id == item.id)?;
                        preserved
                            .glyph
                            .contours
                            .get(contour_index)?
                            .points
                            .get(index)
                    });
                    let mut point = original.cloned().unwrap_or_else(|| {
                        norad::ContourPoint::new(
                            0.0,
                            0.0,
                            norad::PointType::Line,
                            false,
                            None,
                            None,
                        )
                    });
                    point.x = node.x;
                    point.y = node.y;
                    point.smooth = node.smooth;
                    point.typ = match node.nodetype {
                        NodeType::Move => norad::PointType::Move,
                        NodeType::Line => norad::PointType::Line,
                        NodeType::OffCurve => norad::PointType::OffCurve,
                        NodeType::Curve => norad::PointType::Curve,
                        NodeType::QCurve => norad::PointType::QCurve,
                    };
                    point
                })
                .collect();
            contour
        })
        .collect();
    glyph.components = layer
        .components()
        .map(|component| {
            let base = norad::Name::new(&component.reference).expect("validated glyph name");
            let original = read_id(&component.format_specific).and_then(|id| {
                let index = preserved.components.iter().position(|item| item.0 == id)?;
                preserved.glyph.components.get(index)
            });
            let mut output = original.cloned().unwrap_or_else(|| {
                norad::Component::new(base.clone(), norad::AffineTransform::default(), None)
            });
            output.base = base;
            let original: babelfont::DecomposedAffine = affine(output.transform).into();
            if original != component.transform {
                let [x_scale, xy_scale, yx_scale, y_scale, x_offset, y_offset] =
                    component.transform.as_affine().as_coeffs();
                output.transform = norad::AffineTransform {
                    x_scale,
                    xy_scale,
                    yx_scale,
                    y_scale,
                    x_offset,
                    y_offset,
                };
            }
            output
        })
        .collect();
    glyph.anchors = layer
        .anchors
        .iter()
        .map(|anchor| {
            let original = read_id(&anchor.format_specific).and_then(|id| {
                let index = preserved.anchors.iter().position(|item| item.0 == id)?;
                preserved.glyph.anchors.get(index)
            });
            let mut output = original
                .cloned()
                .unwrap_or_else(|| norad::Anchor::new(0.0, 0.0, None, None, None));
            output.x = anchor.x;
            output.y = anchor.y;
            output.name = (!anchor.name.is_empty())
                .then(|| norad::Name::new(&anchor.name).expect("validated anchor name"));
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
}
