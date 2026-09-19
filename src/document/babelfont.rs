// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Babelfont geometry with lossless UFO persistence projections.
//!
//! Babelfont owns paths, anchors and components. UFO payloads retain metadata its
//! model cannot express, plus exact advances and affine coefficients. A projection
//! takes geometry from Babelfont, restoring exact numbers when their corresponding
//! Babelfont value is unchanged. Compilation is the only quantizing boundary.

use babelfont::{Anchor, Component, Layer, Node, NodeType, Shape};

use super::variable::LayerId;

pub(super) fn layer_key(id: &LayerId) -> String {
    format!("{}:{}", id.source.0, id.name)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "the UFO projection retains the exact advance"
)]
pub(super) fn layer_from_ufo(glyph: &norad::Glyph, id: &LayerId, default: bool) -> Layer {
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
    for contour in &glyph.contours {
        layer.shapes.push(Shape::Path(babelfont::Path {
            closed: contour.is_closed(),
            nodes: contour
                .points
                .iter()
                .map(|point| Node {
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
                })
                .collect(),
            ..babelfont::Path::default()
        }));
    }
    for component in &glyph.components {
        layer.shapes.push(Shape::Component(Component {
            reference: component.base.as_str().into(),
            transform: affine(component.transform).into(),
            location: std::iter::empty().collect(),
            format_specific: babelfont::FormatSpecific::default(),
        }));
    }
    layer.anchors = glyph
        .anchors
        .iter()
        .map(|anchor| Anchor {
            x: anchor.x,
            y: anchor.y,
            name: anchor
                .name
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            ..Anchor::default()
        })
        .collect();
    layer
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
pub(super) fn project_layer(layer: &Layer, preserved: &norad::Glyph) -> norad::Glyph {
    let mut glyph = preserved.clone();
    if layer.width != preserved.width as f32 {
        glyph.width = f64::from(layer.width);
    }
    glyph.contours = layer
        .paths()
        .enumerate()
        .map(|(index, path)| {
            let mut contour = preserved.contours.get(index).cloned().unwrap_or_default();
            contour.points = path
                .nodes
                .iter()
                .enumerate()
                .map(|(index, node)| {
                    let mut point = contour.points.get(index).cloned().unwrap_or_else(|| {
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
        .enumerate()
        .map(|(index, component)| {
            let base = norad::Name::new(&component.reference).expect("validated glyph name");
            let mut output = preserved.components.get(index).cloned().unwrap_or_else(|| {
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
        .enumerate()
        .map(|(index, anchor)| {
            let mut output = preserved
                .anchors
                .get(index)
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
