// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Norad glyph contours → kurbo `BezPath`, shared by all Runebender
//! editors. Components resolve recursively through the font.

use kurbo::{Affine, BezPath, Point};
use norad::{Contour, ContourPoint, Font, Glyph, PointType};

pub fn glyph_to_bezpath(glyph: &Glyph, font: &Font) -> BezPath {
    let mut path = BezPath::new();
    for contour in &glyph.contours {
        append_contour(&mut path, contour);
    }
    append_components(&mut path, glyph, font, Affine::IDENTITY, 0);
    path
}

/// Only the glyph's own contours (no components).
pub fn contours_to_bezpath(glyph: &Glyph) -> BezPath {
    let mut path = BezPath::new();
    for contour in &glyph.contours {
        append_contour(&mut path, contour);
    }
    path
}

/// Only the glyph's components, recursively resolved.
pub fn components_to_bezpath(glyph: &Glyph, font: &Font) -> BezPath {
    let mut path = BezPath::new();
    append_components(&mut path, glyph, font, Affine::IDENTITY, 0);
    path
}

/// The affine of a norad component transform.
pub fn component_affine(t: &norad::AffineTransform) -> Affine {
    Affine::new([t.x_scale, t.xy_scale, t.yx_scale, t.y_scale, t.x_offset, t.y_offset])
}

fn append_components(
    path: &mut BezPath,
    glyph: &Glyph,
    font: &Font,
    parent_transform: Affine,
    depth: u8,
) {
    // Guard against reference cycles in malformed UFOs.
    if depth > 8 {
        return;
    }
    for component in &glyph.components {
        let Some(base) = font.get_glyph(&component.base) else {
            continue;
        };
        let t = component.transform;
        let combined = parent_transform
            * Affine::new([t.x_scale, t.xy_scale, t.yx_scale, t.y_scale, t.x_offset, t.y_offset]);
        for contour in &base.contours {
            let mut contour_path = BezPath::new();
            append_contour(&mut contour_path, contour);
            path.extend((combined * &contour_path).elements().iter().cloned());
        }
        append_components(path, base, font, combined, depth + 1);
    }
}

fn pt(p: &ContourPoint) -> Point {
    Point::new(p.x, p.y)
}

fn is_on_curve(p: &ContourPoint) -> bool {
    matches!(
        p.typ,
        PointType::Move | PointType::Line | PointType::Curve | PointType::QCurve
    )
}

fn append_contour(path: &mut BezPath, contour: &Contour) {
    let points = &contour.points;
    if points.is_empty() {
        return;
    }
    let Some(start_idx) = points.iter().position(is_on_curve) else {
        // All-off-curve (TrueType implied on-curve) contour: skip for now.
        return;
    };
    let open = points[0].typ == PointType::Move;
    let rotated: Vec<&ContourPoint> = points[start_idx..]
        .iter()
        .chain(points[..start_idx].iter())
        .collect();

    path.move_to(pt(rotated[0]));

    let mut off_curves: Vec<Point> = Vec::with_capacity(2);
    // For a closed contour the segment list wraps around to the start
    // point; for an open one it ends at the last point.
    let n = rotated.len();
    let idx_range: Vec<usize> = if open {
        (1..n).collect()
    } else {
        (1..=n).map(|i| i % n).collect()
    };
    for i in idx_range {
        let p = rotated[i];
        match p.typ {
            PointType::OffCurve => off_curves.push(pt(p)),
            PointType::Line | PointType::Move => {
                off_curves.clear();
                path.line_to(pt(p));
            }
            PointType::Curve => {
                match off_curves.len() {
                    2 => path.curve_to(off_curves[0], off_curves[1], pt(p)),
                    1 => path.quad_to(off_curves[0], pt(p)),
                    _ => path.line_to(pt(p)),
                }
                off_curves.clear();
            }
            PointType::QCurve => {
                // Expand implied on-curves between consecutive quad
                // off-curves.
                let target = pt(p);
                match off_curves.len() {
                    0 => path.line_to(target),
                    1 => path.quad_to(off_curves[0], target),
                    _ => {
                        for w in 0..off_curves.len() - 1 {
                            let a = off_curves[w];
                            let b = off_curves[w + 1];
                            let mid = a.midpoint(b);
                            path.quad_to(a, mid);
                        }
                        path.quad_to(*off_curves.last().unwrap(), target);
                    }
                }
                off_curves.clear();
            }
        }
    }
    if !open {
        path.close_path();
    }
}

