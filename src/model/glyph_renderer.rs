// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Glyph rendering - converts glyph contours to Kurbo paths

use super::workspace::{Contour, ContourPoint, Glyph, PointType, Workspace};
use kurbo::{Affine, BezPath, Point, Shape};

/// Convert a Norad Glyph to a Kurbo BezPath (contours only)
pub fn glyph_to_bezpath(glyph: &Glyph) -> BezPath {
    let mut path = BezPath::new();

    // Iterate through all contours in the glyph
    for contour in &glyph.contours {
        append_contour_to_path(&mut path, contour);
    }
    path
}

/// Convert a Glyph to a BezPath including components
///
/// This recursively resolves component references and applies their
/// transforms to build a complete path including all nested components.
pub fn glyph_to_bezpath_with_components(glyph: &Glyph, workspace: &Workspace) -> BezPath {
    let mut path = BezPath::new();

    // First, add the glyph's own contours
    for contour in &glyph.contours {
        append_contour_to_path(&mut path, contour);
    }

    // Then recursively add component paths
    append_components_to_path(&mut path, glyph, workspace, Affine::IDENTITY);

    path
}

/// Recursively append component paths to a BezPath
fn append_components_to_path(
    path: &mut BezPath,
    glyph: &Glyph,
    workspace: &Workspace,
    parent_transform: Affine,
) {
    for component in &glyph.components {
        // Look up the base glyph
        let base_glyph = match workspace.glyphs.get(&component.base) {
            Some(g) => g,
            None => {
                tracing::warn!(
                    "Component base glyph '{}' not found in workspace",
                    component.base
                );
                continue;
            }
        };

        // Combine transforms: parent * component
        let combined_transform = parent_transform * component.transform;

        // Build path from base glyph's contours and apply transform
        for contour in &base_glyph.contours {
            let mut contour_path = BezPath::new();
            append_contour_to_path(&mut contour_path, contour);
            // Apply the combined transform and add to main path
            let transformed = combined_transform * &contour_path;
            path.extend(transformed.elements().iter().cloned());
        }

        // Recursively process nested components
        append_components_to_path(path, base_glyph, workspace, combined_transform);
    }
}

/// Append a single contour to a BezPath
fn append_contour_to_path(path: &mut BezPath, contour: &Contour) {
    let points = &contour.points;
    if points.is_empty() {
        return;
    }

    // Check if this is a hyperbezier contour
    let is_hyperbezier = points
        .iter()
        .any(|pt| matches!(pt.point_type, PointType::Hyper | PointType::HyperCorner));

    if is_hyperbezier {
        append_hyperbezier_contour(path, contour);
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
                let off_curve_points = collect_preceding_off_curve_points(&rotated, i);
                add_curve_segment(path, &off_curve_points, pt);
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
            // Hyperbezier points should not appear in glyph_renderer
            // They should be converted to Path::Hyper which has its own rendering
            PointType::Hyper | PointType::HyperCorner => {
                tracing::warn!(
                    "Hyperbezier point in glyph_renderer - should use Path::Hyper instead"
                );
                path.line_to(point_to_kurbo(pt));
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
fn handle_trailing_off_curve_points(path: &mut BezPath, rotated: &[&ContourPoint]) {
    let trailing_off_curve = collect_trailing_off_curve_points(rotated);

    if trailing_off_curve.is_empty() {
        path.close_path();
        return;
    }

    let first_pt = rotated[0];
    add_closing_curve(path, &trailing_off_curve, first_pt);
}

/// Collect trailing off-curve points at the end of the path
fn collect_trailing_off_curve_points<'a>(rotated: &'a [&'a ContourPoint]) -> Vec<&'a ContourPoint> {
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

/// Append a hyperbezier contour to a BezPath using the spline solver
fn append_hyperbezier_contour(path: &mut BezPath, contour: &Contour) {
    use super::entity_id::EntityId;
    use crate::path::HyperPath;
    use crate::path::PathPoints;
    use crate::path::{PathPoint, PointType as PathPointType};

    // Convert workspace contour points to PathPoints
    let path_points: Vec<PathPoint> = contour
        .points
        .iter()
        .map(|pt| PathPoint {
            id: EntityId::next(),
            point: Point::new(pt.x, pt.y),
            typ: match pt.point_type {
                PointType::Hyper => PathPointType::OnCurve { smooth: true },
                PointType::HyperCorner => PathPointType::OnCurve { smooth: false },
                _ => PathPointType::OnCurve { smooth: true },
            },
        })
        .collect();

    // Determine if closed (first point is not Move type)
    let closed = !matches!(
        contour.points.first().map(|p| p.point_type),
        Some(PointType::Move)
    );

    // Create a HyperPath and get its bezier representation
    let hyper_path = HyperPath::from_points(PathPoints::from_vec(path_points), closed);
    let bezier = hyper_path.to_bezpath();

    // Append the solved bezier to the main path
    for el in bezier.elements() {
        match el {
            kurbo::PathEl::MoveTo(p) => path.move_to(*p),
            kurbo::PathEl::LineTo(p) => path.line_to(*p),
            kurbo::PathEl::QuadTo(p1, p2) => path.quad_to(*p1, *p2),
            kurbo::PathEl::CurveTo(p1, p2, p3) => path.curve_to(*p1, *p2, *p3),
            kurbo::PathEl::ClosePath => path.close_path(),
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
