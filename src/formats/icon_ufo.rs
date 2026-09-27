// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Read the bundled icon UFO into paths that application controls can paint.

use kurbo::{Affine, BezPath, Point, Rect};
use norad::{Contour, Font, Glyph, PointType};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/icon_glifs.rs"));
}

/// One editable icon glyph resolved into paint geometry.
#[derive(Debug)]
pub struct IconGlyph {
    /// Glyph name used by the control.
    pub name: String,
    /// Outline in Y-down paint coordinates.
    pub path: BezPath,
    /// Frame from the glyph's advance width and height.
    pub frame: Rect,
    /// Whether every contour is open and should be stroked.
    pub stroke: bool,
}

/// Parse the GLIF files embedded from `assets/icons/icons.ufo`.
pub fn embedded_icons() -> Vec<IconGlyph> {
    let mut font = Font::new();
    for xml in embedded::GLIFS {
        let glyph = Glyph::parse_raw(xml).expect("bundled icon GLIF is valid");
        font.default_layer_mut().insert_glyph(glyph);
    }
    font.default_layer()
        .iter()
        .map(|glyph| {
            let path = Affine::scale_non_uniform(1.0, -1.0) * glyph_path(glyph, &font, 0);
            let stroke = !glyph.contours.is_empty()
                && glyph.contours.iter().all(|contour| {
                    contour
                        .points
                        .first()
                        .is_some_and(|point| point.typ == PointType::Move)
                });
            IconGlyph {
                name: glyph.name().to_string(),
                path,
                frame: Rect::new(0.0, -glyph.height, glyph.width, 0.0),
                stroke,
            }
        })
        .collect()
}

fn glyph_path(glyph: &Glyph, font: &Font, depth: u8) -> BezPath {
    if depth > 8 {
        return BezPath::new();
    }
    let mut path = BezPath::new();
    for contour in &glyph.contours {
        path.extend(contour_path(contour));
    }
    for component in &glyph.components {
        let Some(base) = font.get_glyph(&component.base) else {
            continue;
        };
        let transform = component.transform;
        let affine = Affine::new([
            transform.x_scale,
            transform.xy_scale,
            transform.yx_scale,
            transform.y_scale,
            transform.x_offset,
            transform.y_offset,
        ]);
        path.extend(
            (affine * glyph_path(base, font, depth + 1))
                .elements()
                .iter()
                .copied(),
        );
    }
    path
}

fn contour_path(contour: &Contour) -> BezPath {
    let mut path = BezPath::new();
    if contour.points.is_empty() {
        return path;
    }
    if contour
        .identifier()
        .is_some_and(|id| id.as_ref().contains("hyper"))
    {
        let model = crate::outline::path::hyper_model::Contour {
            points: contour
                .points
                .iter()
                .map(|point| {
                    use crate::outline::path::hyper_model::{ContourPoint, PointType as HyperType};
                    let point_type = match point.typ {
                        PointType::Move | PointType::Curve => HyperType::Hyper,
                        PointType::Line => HyperType::HyperCorner,
                        PointType::OffCurve => HyperType::OffCurve,
                        PointType::QCurve => HyperType::QCurve,
                    };
                    ContourPoint {
                        x: point.x,
                        y: point.y,
                        point_type,
                        smooth: point.smooth,
                    }
                })
                .collect(),
        };
        crate::outline::path::Path::from_contour(&model).append_to_bezpath(&mut path);
        return path;
    }

    let points = &contour.points;
    let closed = points[0].typ != PointType::Move;
    let Some(start) = points
        .iter()
        .position(|point| point.typ != PointType::OffCurve)
    else {
        return path;
    };
    let rotated: Vec<_> = points[start..]
        .iter()
        .chain(points[..start].iter())
        .collect();
    path.move_to(Point::new(rotated[0].x, rotated[0].y));
    let mut handles = Vec::with_capacity(2);
    let end = if closed {
        rotated.len() + 1
    } else {
        rotated.len()
    };
    for index in 1..end {
        let point = rotated[index % rotated.len()];
        let position = Point::new(point.x, point.y);
        match point.typ {
            PointType::OffCurve => handles.push(position),
            PointType::Move | PointType::Line => {
                handles.clear();
                path.line_to(position);
            }
            PointType::Curve => {
                match handles.as_slice() {
                    [first, second] => path.curve_to(*first, *second, position),
                    [first] => path.quad_to(*first, position),
                    _ => path.line_to(position),
                }
                handles.clear();
            }
            PointType::QCurve => {
                for pair in handles.windows(2) {
                    path.quad_to(pair[0], pair[0].midpoint(pair[1]));
                }
                if let Some(last) = handles.last() {
                    path.quad_to(*last, position);
                } else {
                    path.line_to(position);
                }
                handles.clear();
            }
        }
    }
    if closed {
        path.close_path();
    }
    path
}
