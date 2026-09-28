// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! UFO import and export projections for the canonical Babelfont layer.

use super::*;

#[expect(
    clippy::cast_possible_truncation,
    reason = "the UFO projection retains the exact advance"
)]
pub(in crate::font) fn layer_from_ufo(
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
            hyper: ufo_contour_is_hyper(contour),
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
        let mut lib = component.lib().cloned().unwrap_or_default();
        let alignment = ComponentAlignment::take_from_lib(&mut lib);
        components.push(PreservedComponent {
            id: component_id,
            transform: component.transform,
            alignment,
            metadata: ObjectMetadata {
                identifier: component.identifier().cloned(),
                lib: (!lib.is_empty()).then_some(lib),
            },
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
    let mut lib = glyph.lib.clone();
    let mark_color = lib.remove(MARK_COLOR_KEY);
    let left_metrics_key = lib.remove(LEFT_METRICS_KEY);
    let right_metrics_key = lib.remove(RIGHT_METRICS_KEY);
    let metaballs = lib.remove(METABALLS_KEY);
    let composition_recipe = lib.remove(COMPOSITION_RECIPE_KEY);
    let component_order = components
        .iter()
        .map(|component| component.id)
        .collect::<Vec<_>>();
    let smart_component_axes = SmartComponentAxes::take_from_lib(&mut lib)
        .expect("UFO smart-component axes must satisfy the canonical metadata contract");
    let smart_component_values = SmartComponentValues::take_from_lib(&mut lib, &component_order)
        .expect("UFO smart-component values must satisfy the canonical metadata contract");
    let smart_component_pole = SmartComponentPole::take_from_lib(&mut lib)
        .expect("UFO smart-component poles must satisfy the canonical metadata contract");
    let hoi_intermediates = HoiIntermediates::take_from_lib(&mut lib);
    (
        layer,
        LayerPreservation {
            name: glyph.name().to_string(),
            width: glyph.width,
            height: glyph.height,
            codepoints: glyph.codepoints.iter().collect(),
            note: glyph.note.clone(),
            guidelines: glyph.guidelines.clone(),
            image: glyph.image.as_ref().map(LayerImage::from_ufo),
            lib,
            mark_color,
            left_metrics_key,
            right_metrics_key,
            metaballs,
            composition_recipe,
            smart_component_axes,
            smart_component_values,
            smart_component_pole,
            hoi_intermediates,
            contours,
            components,
            anchors,
        },
    )
}

pub(in crate::font) fn copy_layer(
    layer: &Layer,
    preserved: &LayerPreservation,
    id: &LayerId,
) -> (Layer, LayerPreservation) {
    let mut layer = layer.clone();
    let mut preserved = preserved.clone();
    let old_component_order = preserved
        .components
        .iter()
        .map(|component| component.id)
        .collect::<Vec<_>>();
    let smart_component_values = preserved.smart_component_values.as_ref().map(|values| {
        values
            .to_plist(&old_component_order)
            .expect("canonical smart-component values retain their source components")
    });
    layer.id = Some(layer_key(id));
    layer.name = Some(id.name.clone());
    layer.master = babelfont::LayerType::AssociatedWithMaster(id.source.0.to_string());

    for (path, preserved) in layer
        .shapes
        .iter_mut()
        .filter_map(|shape| match shape {
            Shape::Path(path) => Some(path),
            Shape::Component(_) => None,
        })
        .zip(&mut preserved.contours)
    {
        preserved.id = ContourId::next();
        write_id(&mut path.format_specific, preserved.id.0);
        for (node, preserved) in path.nodes.iter_mut().zip(&mut preserved.points) {
            preserved.id = PointId::next();
            write_id(&mut node.format_specific, preserved.id.0);
        }
    }
    for (component, preserved) in layer
        .shapes
        .iter_mut()
        .filter_map(|shape| match shape {
            Shape::Component(component) => Some(component),
            Shape::Path(_) => None,
        })
        .zip(&mut preserved.components)
    {
        preserved.id = ComponentId::next();
        write_id(&mut component.format_specific, preserved.id.0);
    }
    for (anchor, preserved) in layer.anchors.iter_mut().zip(&mut preserved.anchors) {
        preserved.id = AnchorId::next();
        write_id(&mut anchor.format_specific, preserved.id.0);
    }
    if let Some(values) = smart_component_values {
        let new_component_order = preserved
            .components
            .iter()
            .map(|component| component.id)
            .collect::<Vec<_>>();
        preserved.smart_component_values = Some(
            SmartComponentValues::from_plist(&values, &new_component_order)
                .expect("copied smart-component values bind to copied components"),
        );
    }
    (layer, preserved)
}

pub(in crate::font) fn copy_contours_only(
    layer: &Layer,
    preserved: &LayerPreservation,
    id: &LayerId,
) -> (Layer, LayerPreservation) {
    let (mut layer, mut preserved) = copy_layer(layer, preserved, id);
    layer.shapes.retain(|shape| matches!(shape, Shape::Path(_)));
    layer.anchors.clear();
    preserved.height = 0.0;
    preserved.codepoints.clear();
    preserved.note = None;
    preserved.guidelines.clear();
    preserved.image = None;
    preserved.lib.clear();
    preserved.mark_color = None;
    preserved.left_metrics_key = None;
    preserved.right_metrics_key = None;
    preserved.metaballs = None;
    preserved.composition_recipe = None;
    preserved.smart_component_axes = None;
    preserved.smart_component_values = None;
    preserved.smart_component_pole = None;
    preserved.hoi_intermediates = None;
    preserved.components.clear();
    preserved.anchors.clear();
    (layer, preserved)
}

pub(super) fn affine(t: norad::AffineTransform) -> kurbo::Affine {
    kurbo::Affine::new([
        t.x_scale, t.xy_scale, t.yx_scale, t.y_scale, t.x_offset, t.y_offset,
    ])
}

fn project_contours(layer: &Layer, preserved: &LayerPreservation) -> Vec<norad::Contour> {
    layer
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
        .collect()
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "compare with the original narrowed Babelfont advance"
)]
pub(in crate::font) fn project_layer(layer: &Layer, preserved: &LayerPreservation) -> norad::Glyph {
    let mut glyph = norad::Glyph::new(&preserved.name);
    glyph.width = preserved.width;
    glyph.height = preserved.height;
    glyph.codepoints = norad::Codepoints::new(preserved.codepoints.iter().copied());
    glyph.note.clone_from(&preserved.note);
    glyph.guidelines.clone_from(&preserved.guidelines);
    glyph.image = preserved.image.as_ref().map(LayerImage::to_ufo);
    glyph.lib.clone_from(&preserved.lib);
    for (key, value) in [
        (MARK_COLOR_KEY, &preserved.mark_color),
        (LEFT_METRICS_KEY, &preserved.left_metrics_key),
        (RIGHT_METRICS_KEY, &preserved.right_metrics_key),
        (METABALLS_KEY, &preserved.metaballs),
        (COMPOSITION_RECIPE_KEY, &preserved.composition_recipe),
    ] {
        if let Some(value) = value {
            glyph.lib.insert(key.into(), value.clone());
        }
    }
    if let Some(axes) = &preserved.smart_component_axes {
        axes.write_to_lib(&mut glyph.lib);
    }
    let component_order = layer
        .components()
        .map(|component| {
            ComponentId(read_id(&component.format_specific).expect("canonical component identity"))
        })
        .collect::<Vec<_>>();
    if let Some(values) = &preserved.smart_component_values {
        values
            .write_to_lib(&mut glyph.lib, &component_order)
            .expect("canonical smart-component values retain current component identities");
    }
    if let Some(pole) = &preserved.smart_component_pole {
        pole.write_to_lib(&mut glyph.lib);
    }
    if let Some(points) = &preserved.hoi_intermediates {
        points.write_to_lib(&mut glyph.lib);
    }
    if layer.width != preserved.width as f32 {
        glyph.width = f64::from(layer.width);
    }
    glyph.contours = project_contours(layer, preserved);
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
            if let Some(original) = original {
                let mut lib = output.lib().cloned().unwrap_or_default();
                original.alignment.write_to_lib(&mut lib);
                if lib.is_empty() {
                    output.take_lib();
                } else {
                    output.replace_lib(lib);
                }
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
