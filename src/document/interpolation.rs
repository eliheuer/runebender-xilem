// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Checked interpolation of glyph-local sources, retaining exact UFO payloads.
//!
//! Advances, vertical advances, points, anchors and affine component coefficients
//! interpolate as f64 values. Non-varying metadata comes from the default layer,
//! never the currently selected editor source.

use super::babelfont::{
    AnchorId, ComponentId, ContourId, LayerPointType, LayerShapeView, LayerView, PointId,
};
use super::var_model::{Location, VariationModel};

/// One interpolated canonical layer, retaining the default layer's object identities.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct InterpolatedLayer {
    pub(super) glyph_name: String,
    pub(super) width: f64,
    pub(super) height: f64,
    pub(super) codepoints: Vec<char>,
    pub(super) note: Option<String>,
    pub(super) shapes: Vec<InterpolatedShape>,
    pub(super) anchors: Vec<InterpolatedAnchor>,
}

impl InterpolatedLayer {
    fn contours(&self) -> impl Iterator<Item = &InterpolatedContour> {
        self.shapes.iter().filter_map(|shape| match shape {
            InterpolatedShape::Contour(contour) => Some(contour),
            InterpolatedShape::Component(_) => None,
        })
    }

    fn components(&self) -> impl Iterator<Item = &InterpolatedComponent> {
        self.shapes.iter().filter_map(|shape| match shape {
            InterpolatedShape::Contour(_) => None,
            InterpolatedShape::Component(component) => Some(component),
        })
    }

    pub(super) fn point_at_mut(
        &mut self,
        contour_index: usize,
        point_index: usize,
    ) -> Option<&mut InterpolatedPoint> {
        self.shapes
            .iter_mut()
            .filter_map(|shape| match shape {
                InterpolatedShape::Contour(contour) => Some(contour),
                InterpolatedShape::Component(_) => None,
            })
            .nth(contour_index)?
            .points
            .get_mut(point_index)
    }
}

/// One contour or component in canonical paint order.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum InterpolatedShape {
    Contour(InterpolatedContour),
    Component(InterpolatedComponent),
}

/// One contour in an interpolated canonical layer.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct InterpolatedContour {
    pub(super) id: ContourId,
    pub(super) closed: bool,
    pub(super) points: Vec<InterpolatedPoint>,
}

/// One point in an interpolated canonical contour.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct InterpolatedPoint {
    pub(super) id: PointId,
    pub(super) position: kurbo::Point,
    pub(super) point_type: LayerPointType,
    pub(super) smooth: bool,
    pub(super) name: Option<String>,
}

/// One component in an interpolated canonical layer.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct InterpolatedComponent {
    pub(super) id: ComponentId,
    pub(super) reference: String,
    pub(super) transform: kurbo::Affine,
}

/// One named anchor in an interpolated canonical layer.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct InterpolatedAnchor {
    pub(super) id: AnchorId,
    pub(super) name: String,
    pub(super) position: kurbo::Point,
}

fn compatible_layers(base: LayerView<'_>, other: LayerView<'_>) -> bool {
    let base_shapes: Vec<_> = base.shapes().collect();
    let other_shapes: Vec<_> = other.shapes().collect();
    base_shapes.len() == other_shapes.len()
        && base_shapes
            .iter()
            .zip(&other_shapes)
            .all(|(a, b)| match (*a, *b) {
                (LayerShapeView::Contour(a), LayerShapeView::Contour(b)) => {
                    let a_types: Vec<_> = a.points().map(|point| point.point_type()).collect();
                    let b_types: Vec<_> = b.points().map(|point| point.point_type()).collect();
                    a.is_closed() == b.is_closed() && a_types == b_types
                }
                (LayerShapeView::Component(a), LayerShapeView::Component(b)) => {
                    a.reference() == b.reference()
                }
                _ => false,
            })
        && base.anchors().count() == other.anchors().count()
        && base
            .anchors()
            .all(|a| other.anchors().filter(|b| a.name() == b.name()).count() == 1)
}

fn layer_values(layer: LayerView<'_>, base: LayerView<'_>) -> Vec<f64> {
    let mut values = vec![layer.width(), layer.height()];
    for shape in layer.shapes() {
        match shape {
            LayerShapeView::Contour(contour) => {
                for point in contour.points() {
                    values.extend([point.position().x, point.position().y]);
                }
            }
            LayerShapeView::Component(component) => {
                values.extend(component.transform().as_coeffs());
            }
        }
    }
    for anchor in base.anchors() {
        let source = layer
            .anchors()
            .find(|candidate| candidate.name() == anchor.name())
            .expect("validated anchors");
        values.extend([source.position().x, source.position().y]);
    }
    values
}

/// Interpolate canonical geometry and exact values without constructing a UFO glyph.
///
/// Topology, point roles, names, smooth state and stable identities come from the default layer.
/// Other sources contribute only the checked numeric values that are allowed to vary.
pub(super) fn interpolate_layers(
    layers: &[LayerView<'_>],
    locations: &[Location],
    target: &Location,
) -> Result<InterpolatedLayer, String> {
    let default = locations
        .iter()
        .position(|location| location.values().all(|value| value.abs() < 1e-9))
        .ok_or("glyph has no layer at the default location")?;
    let base = layers
        .get(default)
        .copied()
        .ok_or("missing default layer")?;
    if layers.len() != locations.len()
        || layers
            .iter()
            .copied()
            .any(|layer| !compatible_layers(base, layer))
    {
        return Err(format!(
            "{}: incompatible contours, point types, components or anchors",
            base.glyph_name()
        ));
    }
    if locations
        .iter()
        .chain(std::iter::once(target))
        .any(|location| location.values().any(|value| !value.is_finite()))
    {
        return Err("interpolation location must be finite".into());
    }
    for (index, location) in locations.iter().enumerate() {
        if locations[..index]
            .iter()
            .any(|other| same_location(location, other))
        {
            return Err(format!(
                "{}: duplicate glyph-source location",
                base.glyph_name()
            ));
        }
    }
    let values: Vec<_> = layers
        .iter()
        .copied()
        .map(|layer| layer_values(layer, base))
        .collect();
    if values.iter().flatten().any(|value| !value.is_finite()) {
        return Err(format!("{}: non-finite glyph geometry", base.glyph_name()));
    }
    let output = VariationModel::new(locations)?.interpolate(&values, target)?;
    if output.iter().any(|value| !value.is_finite()) {
        return Err("non-finite interpolation result".into());
    }
    let mut values = output.into_iter();
    let mut next = || values.next().expect("validated interpolation dimensions");
    let width = next();
    let height = next();
    let shapes = base
        .shapes()
        .map(|shape| match shape {
            LayerShapeView::Contour(contour) => InterpolatedShape::Contour(InterpolatedContour {
                id: contour.id(),
                closed: contour.is_closed(),
                points: contour
                    .points()
                    .map(|point| InterpolatedPoint {
                        id: point.id(),
                        position: kurbo::Point::new(next(), next()),
                        point_type: point.point_type(),
                        smooth: point.is_smooth(),
                        name: point.name().map(str::to_owned),
                    })
                    .collect(),
            }),
            LayerShapeView::Component(component) => {
                InterpolatedShape::Component(InterpolatedComponent {
                    id: component.id(),
                    reference: component.reference().to_owned(),
                    transform: kurbo::Affine::new([next(), next(), next(), next(), next(), next()]),
                })
            }
        })
        .collect();
    let anchors = base
        .anchors()
        .map(|anchor| InterpolatedAnchor {
            id: anchor.id(),
            name: anchor.name().to_owned(),
            position: kurbo::Point::new(next(), next()),
        })
        .collect();
    if values.next().is_some() {
        return Err("interpolation produced unexpected output dimensions".into());
    }
    Ok(InterpolatedLayer {
        glyph_name: base.glyph_name().to_owned(),
        width,
        height,
        codepoints: base.codepoints().collect(),
        note: base.note().map(str::to_owned),
        shapes,
        anchors,
    })
}

/// Interpolate canonical layers and materialize one transitional UFO result.
///
/// The UFO value is created only after interpolation and retains the default layer's exact
/// preservation payload.
pub(super) fn interpolate_projected(
    layers: &[LayerView<'_>],
    locations: &[Location],
    target: &Location,
) -> Result<norad::Glyph, String> {
    let default = locations
        .iter()
        .position(|location| location.values().all(|value| value.abs() < 1e-9))
        .ok_or("glyph has no layer at the default location")?;
    let base = layers
        .get(default)
        .copied()
        .ok_or("missing default layer")?;
    let output = interpolate_layers(layers, locations, target)?;
    project_interpolated(&output, base)
}

pub(super) fn project_interpolated(
    output: &InterpolatedLayer,
    base: LayerView<'_>,
) -> Result<norad::Glyph, String> {
    let mut glyph = base.project();
    if output.glyph_name != glyph.name().as_str()
        || output
            .codepoints
            .iter()
            .copied()
            .collect::<norad::Codepoints>()
            != glyph.codepoints
        || output.note != glyph.note
    {
        return Err("canonical interpolation changed default-layer metadata".into());
    }
    glyph.width = output.width;
    glyph.height = output.height;
    for ((contour, output), source) in glyph
        .contours
        .iter_mut()
        .zip(output.contours())
        .zip(base.contours())
    {
        if output.id != source.id() || output.closed != source.is_closed() {
            return Err("canonical interpolation changed default contour structure".into());
        }
        for ((point, output), source) in contour
            .points
            .iter_mut()
            .zip(&output.points)
            .zip(source.points())
        {
            if output.id != source.id()
                || output.point_type != source.point_type()
                || output.smooth != source.is_smooth()
                || output.name.as_deref() != source.name()
            {
                return Err("canonical interpolation changed default point structure".into());
            }
            point.x = output.position.x;
            point.y = output.position.y;
        }
    }
    for ((anchor, output), source) in glyph
        .anchors
        .iter_mut()
        .zip(&output.anchors)
        .zip(base.anchors())
    {
        if output.id != source.id() || output.name != source.name() {
            return Err("canonical interpolation changed default anchor structure".into());
        }
        anchor.x = output.position.x;
        anchor.y = output.position.y;
    }
    for ((component, output), source) in glyph
        .components
        .iter_mut()
        .zip(output.components())
        .zip(base.components())
    {
        if output.id != source.id() || output.reference != source.reference() {
            return Err("canonical interpolation changed default component structure".into());
        }
        let [x_scale, xy_scale, yx_scale, y_scale, x_offset, y_offset] =
            output.transform.as_coeffs();
        component.transform = norad::AffineTransform {
            x_scale,
            xy_scale,
            yx_scale,
            y_scale,
            x_offset,
            y_offset,
        };
    }
    Ok(glyph)
}

fn same_location(a: &Location, b: &Location) -> bool {
    a.keys()
        .chain(b.keys())
        .all(|key| a.get(key).copied().unwrap_or(0.0) == b.get(key).copied().unwrap_or(0.0))
}

#[cfg(test)]
mod tests {
    use norad::{Anchor, Component, Contour, ContourPoint, Glyph, Name, PointType};

    use super::*;
    use crate::document::variable::{LayerId, SourceId};

    fn location(weight: f64) -> Location {
        [("Weight".to_owned(), weight)].into()
    }

    fn glyph(offset: f64) -> Glyph {
        let mut glyph = Glyph::new("interpolated");
        glyph.width = 500.123_456_789 + offset;
        glyph.height = 900.987_654_321 + offset * 2.0;
        glyph.note = Some(format!("source {offset}"));
        glyph.codepoints.insert('I');
        glyph.contours.push(Contour::new(
            vec![
                ContourPoint::new(
                    10.25 + offset,
                    20.5 + offset * 2.0,
                    PointType::Move,
                    true,
                    Some(Name::new("origin").unwrap()),
                    None,
                ),
                ContourPoint::new(
                    100.75 + offset,
                    200.125 + offset * 2.0,
                    PointType::Curve,
                    false,
                    None,
                    None,
                ),
            ],
            None,
        ));
        glyph.components.push(Component::new(
            Name::new("base").unwrap(),
            norad::AffineTransform {
                x_scale: 1.0 + offset / 1000.0,
                xy_scale: 0.125 + offset / 500.0,
                yx_scale: -0.25 + offset / 400.0,
                y_scale: 0.875 + offset / 300.0,
                x_offset: 12.5 + offset,
                y_offset: -25.25 + offset * 2.0,
            },
            None,
        ));
        glyph.anchors.push(Anchor::new(
            200.5 + offset,
            700.25 + offset * 2.0,
            Some(Name::new("top").unwrap()),
            None,
            None,
        ));
        glyph.anchors.push(Anchor::new(
            150.25 + offset,
            -20.5 + offset * 2.0,
            Some(Name::new("bottom").unwrap()),
            None,
            None,
        ));
        glyph
    }

    #[test]
    fn canonical_interpolation_retains_default_structure_and_exact_values() {
        let base_id = LayerId {
            source: SourceId(0),
            name: "public.default".into(),
        };
        let other_id = LayerId {
            source: SourceId(1),
            name: "public.default".into(),
        };
        let (base_layer, base_preserved) =
            crate::document::babelfont::layer_from_ufo(&glyph(0.0), &base_id, true);
        let mut other = glyph(100.0);
        other.anchors.reverse();
        let (other_layer, other_preserved) =
            crate::document::babelfont::layer_from_ufo(&other, &other_id, true);
        let base = LayerView::new(&base_layer, &base_preserved);
        let other = LayerView::new(&other_layer, &other_preserved);
        let mut result = interpolate_layers(
            &[base, other],
            &[location(0.0), location(1.0)],
            &location(0.5),
        )
        .unwrap();

        assert_eq!(result.glyph_name, "interpolated");
        assert_eq!(result.width, 550.123_456_789);
        assert_eq!(result.height, 1_000.987_654_321);
        assert_eq!(result.codepoints, ['I']);
        assert_eq!(result.note.as_deref(), Some("source 0"));
        assert!(matches!(
            result.shapes.first(),
            Some(InterpolatedShape::Contour(_))
        ));
        assert!(matches!(
            result.shapes.get(1),
            Some(InterpolatedShape::Component(_))
        ));
        let contours: Vec<_> = result.contours().collect();
        let components: Vec<_> = result.components().collect();
        assert_eq!(contours[0].id, base.contours().next().unwrap().id());
        assert_eq!(
            contours[0].points[0].id,
            base.contours()
                .next()
                .unwrap()
                .points()
                .next()
                .unwrap()
                .id()
        );
        assert_eq!(contours[0].points[0].point_type, LayerPointType::Move);
        assert!(contours[0].points[0].smooth);
        assert_eq!(contours[0].points[0].name.as_deref(), Some("origin"));
        assert_eq!(contours[0].points[0].position, (60.25, 120.5).into());
        assert_eq!(result.anchors[0].name, "top");
        assert_eq!(result.anchors[0].position, (250.5, 800.25).into());
        assert_eq!(result.anchors[1].name, "bottom");
        assert_eq!(result.anchors[1].position, (200.25, 79.5).into());
        assert_eq!(
            components[0].transform.as_coeffs(),
            [1.05, 0.225, -0.125, 1.041_666_666_666_666_5, 62.5, 74.75,]
        );
        assert_eq!(components[0].id, base.components().next().unwrap().id());
        assert_eq!(components[0].reference, "base");
        assert_eq!(result.anchors[0].id, base.anchors().next().unwrap().id());

        result.point_at_mut(0, 0).unwrap().position = (33.25, 44.75).into();
        let projected = project_interpolated(&result, base).unwrap();
        assert_eq!(
            (
                projected.contours[0].points[0].x,
                projected.contours[0].points[0].y
            ),
            (33.25, 44.75)
        );
    }

    #[test]
    fn canonical_interpolation_rejects_incompatible_roles_and_locations() {
        let base_id = LayerId {
            source: SourceId(0),
            name: "public.default".into(),
        };
        let other_id = LayerId {
            source: SourceId(1),
            name: "public.default".into(),
        };
        let base_glyph = glyph(0.0);
        let mut incompatible = glyph(100.0);
        incompatible.contours[0].points[1].typ = PointType::Line;
        let (base_layer, base_preserved) =
            crate::document::babelfont::layer_from_ufo(&base_glyph, &base_id, true);
        let (other_layer, other_preserved) =
            crate::document::babelfont::layer_from_ufo(&incompatible, &other_id, true);
        let layers = [
            LayerView::new(&base_layer, &base_preserved),
            LayerView::new(&other_layer, &other_preserved),
        ];

        assert!(
            interpolate_layers(&layers, &[location(0.0), location(1.0)], &location(0.5))
                .unwrap_err()
                .contains("incompatible")
        );
        assert!(
            interpolate_layers(&[layers[0]], &[location(0.0)], &location(f64::NAN))
                .unwrap_err()
                .contains("finite")
        );
    }
}
