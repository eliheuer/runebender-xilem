// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Glyph rendering - converts glyph contours to Kurbo paths

use crate::workspace::{Contour, ContourPoint, Glyph, PointType};
use kurbo::{BezPath, Point, Shape};

/// Convert a Norad Glyph to a Kurbo BezPath
pub fn glyph_to_bezpath(glyph: &Glyph) -> BezPath {
    let mut path = BezPath::new();

    // Iterate through all contours in the glyph
    for contour in &glyph.contours {
        append_contour_to_path(&mut path, contour);
    }
    path
}

/// Append a single contour to a BezPath
fn append_contour_to_path(
    path: &mut BezPath,
    contour: &Contour,
) {
    let points = &contour.points;
    if points.is_empty() {
        return;
    }

    // Find the first on-curve point to start the path
    let start_idx = points
        .iter()
        .position(|p| {
            matches!(
                p.point_type,
                PointType::Move | PointType::Line | PointType::Curve
            )
        })
        .unwrap_or(0);

    // Rotate the points so we start at an on-curve point
    let rotated: Vec<_> = points[start_idx..]
        .iter()
        .chain(points[..start_idx].iter())
        .collect();

    if rotated.is_empty() {
        return;
    }

    // Start the path at the first point
    let first = rotated[0];
    path.move_to(point_to_kurbo(first));

    // Process remaining points
    let mut i = 1;
    while i < rotated.len() {
        let pt = rotated[i];

        match pt.point_type {
            PointType::Move => {
                path.move_to(point_to_kurbo(pt));
                i += 1;
            }
            PointType::Line => {
                path.line_to(point_to_kurbo(pt));
                i += 1;
            }
            PointType::Curve => {
                // Cubic bezier - need to look back for control points
                // In UFO, off-curve points (OffCurve) precede the
                // on-curve point (Curve)
                let off_curve_points =
                    collect_preceding_off_curve_points(
                        &rotated,
                        i,
                    );
                add_curve_segment(
                    path,
                    &off_curve_points,
                    pt,
                );
                i += 1;
            }
            PointType::OffCurve => {
                // Off-curve points are handled when we encounter the
                // following on-curve point
                i += 1;
            }
            PointType::QCurve => {
                // Quadratic curve point
                // Look back for off-curve point
                if i > 0 && rotated[i - 1].point_type == PointType::OffCurve {
                    let cp = point_to_kurbo(rotated[i - 1]);
                    let end = point_to_kurbo(pt);
                    path.quad_to(cp, end);
                } else {
                    path.line_to(point_to_kurbo(pt));
                }
                i += 1;
            }
        }
    }

    // Handle trailing off-curve points that curve back to the start
    handle_trailing_off_curve_points(path, &rotated);
}

/// Convert a ContourPoint to a Kurbo Point
fn point_to_kurbo(pt: &ContourPoint) -> Point {
    Point::new(pt.x, pt.y)
}

/// Collect preceding off-curve points before an index
fn collect_preceding_off_curve_points<'a>(
    rotated: &'a [&'a ContourPoint],
    current_idx: usize,
) -> Vec<&'a ContourPoint> {
    let mut off_curve_points = Vec::new();
    let mut j = current_idx.saturating_sub(1);

    while j > 0 && rotated[j].point_type == PointType::OffCurve {
        off_curve_points.insert(0, rotated[j]);
        j -= 1;
    }

    off_curve_points
}

/// Add a curve segment to the path based on control points
fn add_curve_segment(
    path: &mut BezPath,
    off_curve_points: &[&ContourPoint],
    end_point: &ContourPoint,
) {
    match off_curve_points.len() {
        0 => {
            // No control points - treat as line
            path.line_to(point_to_kurbo(end_point));
        }
        1 => {
            // Quadratic curve
            let cp = point_to_kurbo(off_curve_points[0]);
            let end = point_to_kurbo(end_point);
            path.quad_to(cp, end);
        }
        2 => {
            // Cubic curve
            let cp1 = point_to_kurbo(off_curve_points[0]);
            let cp2 = point_to_kurbo(off_curve_points[1]);
            let end = point_to_kurbo(end_point);
            path.curve_to(cp1, cp2, end);
        }
        _ => {
            // More than 2 control points - this shouldn't happen
            // in UFO. Just use the last two.
            let len = off_curve_points.len();
            let cp1 = point_to_kurbo(off_curve_points[len - 2]);
            let cp2 = point_to_kurbo(off_curve_points[len - 1]);
            let end = point_to_kurbo(end_point);
            path.curve_to(cp1, cp2, end);
        }
    }
}

/// Handle trailing off-curve points for closed paths
fn handle_trailing_off_curve_points(
    path: &mut BezPath,
    rotated: &[&ContourPoint],
) {
    let trailing_off_curve =
        collect_trailing_off_curve_points(rotated);

    if trailing_off_curve.is_empty() {
        path.close_path();
        return;
    }

    let first_pt = rotated[0];
    add_closing_curve(path, &trailing_off_curve, first_pt);
}

/// Collect trailing off-curve points at the end of the path
fn collect_trailing_off_curve_points<'a>(
    rotated: &'a [&'a ContourPoint],
) -> Vec<&'a ContourPoint> {
    let mut trailing_off_curve = Vec::new();
    let mut j = rotated.len().saturating_sub(1);

    while j > 0 && rotated[j].point_type == PointType::OffCurve {
        trailing_off_curve.insert(0, rotated[j]);
        j -= 1;
    }

    trailing_off_curve
}

/// Add closing curve segment for closed paths
fn add_closing_curve(
    path: &mut BezPath,
    trailing_off_curve: &[&ContourPoint],
    first_pt: &ContourPoint,
) {
    match first_pt.point_type {
        PointType::Curve => {
            add_curve_segment(path, trailing_off_curve, first_pt);
        }
        PointType::QCurve => {
            if !trailing_off_curve.is_empty() {
                let cp = point_to_kurbo(trailing_off_curve[0]);
                let end = point_to_kurbo(first_pt);
                path.quad_to(cp, end);
            } else {
                path.close_path();
            }
        }
        _ => {
            // First point is Line or Move - just close with
            // straight line
            path.close_path();
        }
    }
}

/// Get the bounding box of a glyph for scaling/centering
#[allow(dead_code)]
pub fn glyph_bounds(glyph: &Glyph) -> Option<kurbo::Rect> {
    let path = glyph_to_bezpath(glyph);
    if path.is_empty() {
        None
    } else {
        Some(path.bounding_box())
    }
}
