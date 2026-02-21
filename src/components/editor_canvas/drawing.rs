// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Standalone drawing helper functions for paths, points, and metrics

use crate::editing::EditSession;
use crate::path::PointType;
use crate::theme;
use kurbo::{Affine, Circle, Point, Rect as KurboRect, Stroke};
use masonry::kurbo::Size;
use masonry::util::fill_color;
use masonry::vello::Scene;
use masonry::vello::peniko::Brush;

/// Draw a design-space unit grid when zoomed in past the threshold.
///
/// Two detail levels activate at different zoom thresholds:
/// - Mid zoom: coarser grid (fine=8, coarse=32)
/// - Close zoom: finer grid (fine=2, coarse=8)
///
/// Lines outside the visible canvas are culled to keep drawing cheap.
pub(crate) fn draw_design_grid(scene: &mut Scene, session: &EditSession, canvas_size: Size) {
    use crate::settings;

    let zoom = session.viewport.zoom;

    // Determine which grid levels to draw
    let draw_mid = zoom >= settings::design_grid::mid::MIN_ZOOM;
    let draw_close = zoom >= settings::design_grid::close::MIN_ZOOM;

    if !draw_mid {
        return;
    }

    // Convert canvas corners to design space to find visible range
    let top_left = session.viewport.screen_to_design(Point::ZERO);
    let bottom_right = session
        .viewport
        .screen_to_design(Point::new(canvas_size.width, canvas_size.height));

    // design y is flipped: top_left.y > bottom_right.y
    let min_x = top_left.x.min(bottom_right.x);
    let max_x = top_left.x.max(bottom_right.x);
    let min_y = top_left.y.min(bottom_right.y);
    let max_y = top_left.y.max(bottom_right.y);

    let transform = session.viewport.affine();
    let fine_stroke = Stroke::new(0.5);
    let coarse_stroke = Stroke::new(1.0);
    let fine_brush = Brush::Solid(theme::design_grid::FINE);
    let coarse_brush = Brush::Solid(theme::design_grid::COARSE);

    // Draw mid-level grid (fine=8, coarse=32)
    draw_grid_level(
        scene,
        &transform,
        settings::design_grid::mid::FINE,
        settings::design_grid::mid::COARSE_N,
        min_x,
        max_x,
        min_y,
        max_y,
        &fine_stroke,
        &coarse_stroke,
        &fine_brush,
        &coarse_brush,
    );

    // Draw close-level grid (fine=2, coarse=8)
    if draw_close {
        draw_grid_level(
            scene,
            &transform,
            settings::design_grid::close::FINE,
            settings::design_grid::close::COARSE_N,
            min_x,
            max_x,
            min_y,
            max_y,
            &fine_stroke,
            &coarse_stroke,
            &fine_brush,
            &coarse_brush,
        );
    }
}

/// Draw a single grid level with the given spacing and coarse interval.
///
/// Lines that coincide with a coarser grid (multiples of
/// `coarse_n * spacing`) are skipped when drawing fine lines, since
/// the coarse stroke covers them.
fn draw_grid_level(
    scene: &mut Scene,
    transform: &Affine,
    spacing: f64,
    coarse_n: u32,
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
    fine_stroke: &Stroke,
    coarse_stroke: &Stroke,
    fine_brush: &Brush,
    coarse_brush: &Brush,
) {
    let start_x = (min_x / spacing).floor() as i64;
    let end_x = (max_x / spacing).ceil() as i64;
    let start_y = (min_y / spacing).floor() as i64;
    let end_y = (max_y / spacing).ceil() as i64;

    // Vertical lines (constant x)
    for ix in start_x..=end_x {
        let x = ix as f64 * spacing;
        let is_coarse = coarse_n > 0 && (ix.unsigned_abs() % coarse_n as u64 == 0);
        let (stroke, brush) = if is_coarse {
            (coarse_stroke, coarse_brush)
        } else {
            (fine_stroke, fine_brush)
        };
        let p0 = *transform * Point::new(x, min_y);
        let p1 = *transform * Point::new(x, max_y);
        scene.stroke(
            stroke,
            Affine::IDENTITY,
            brush,
            None,
            &kurbo::Line::new(p0, p1),
        );
    }

    // Horizontal lines (constant y)
    for iy in start_y..=end_y {
        let y = iy as f64 * spacing;
        let is_coarse = coarse_n > 0 && (iy.unsigned_abs() % coarse_n as u64 == 0);
        let (stroke, brush) = if is_coarse {
            (coarse_stroke, coarse_brush)
        } else {
            (fine_stroke, fine_brush)
        };
        let p0 = *transform * Point::new(min_x, y);
        let p1 = *transform * Point::new(max_x, y);
        scene.stroke(
            stroke,
            Affine::IDENTITY,
            brush,
            None,
            &kurbo::Line::new(p0, p1),
        );
    }
}

/// Draw font metric guidelines
pub(crate) fn draw_metrics_guides(
    scene: &mut Scene,
    transform: &Affine,
    session: &EditSession,
    _canvas_size: Size,
) {
    let stroke = Stroke::new(theme::size::METRIC_LINE_WIDTH);
    let brush = Brush::Solid(theme::metrics::GUIDE);

    // Helper to draw a horizontal line at a given Y coordinate in
    // design space. Lines are contained within the metrics box
    // (from x=0 to x=advance_width)
    let draw_hline = |scene: &mut Scene, y: f64| {
        let start = Point::new(0.0, y);
        let end = Point::new(session.glyph.width, y);

        let start_screen = *transform * start;
        let end_screen = *transform * end;

        let line = kurbo::Line::new(start_screen, end_screen);
        scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &line);
    };

    // Helper to draw a vertical line at a given X coordinate in
    // design space. Lines are contained within the metrics box
    // (from y=descender to y=ascender)
    let draw_vline = |scene: &mut Scene, x: f64| {
        let start = Point::new(x, session.descender);
        let end = Point::new(x, session.ascender);

        let start_screen = *transform * start;
        let end_screen = *transform * end;

        let line = kurbo::Line::new(start_screen, end_screen);
        scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &line);
    };

    // Draw vertical lines (left and right edges of metrics box)
    draw_vline(scene, 0.0);
    draw_vline(scene, session.glyph.width);

    // Draw horizontal lines
    // Descender (bottom of metrics box)
    draw_hline(scene, session.descender);

    // Baseline (y=0)
    draw_hline(scene, 0.0);

    // X-height (if available)
    if let Some(x_height) = session.x_height {
        draw_hline(scene, x_height);
    }

    // Cap-height (if available)
    if let Some(cap_height) = session.cap_height {
        draw_hline(scene, cap_height);
    }

    // Ascender (top of metrics box)
    draw_hline(scene, session.ascender);
}

/// Draw paths with control point lines and styled points
pub(crate) fn draw_paths_with_points(scene: &mut Scene, session: &EditSession, transform: &Affine) {
    use crate::path::Path;

    // First pass: draw control point lines (handles)
    // In cubic bezier curves, handles connect on-curve points to
    // their adjacent off-curve control points
    for path in session.paths.iter() {
        match path {
            Path::Cubic(cubic) => {
                draw_control_handles(scene, cubic, transform);
            }
            Path::Quadratic(quadratic) => {
                draw_control_handles_quadratic(scene, quadratic, transform);
            }
            Path::Hyper(hyper) => {
                // Hyper paths use similar handle drawing to cubic
                draw_control_handles_hyper(scene, hyper, transform);
            }
        }
    }

    // Second pass: draw points
    for path in session.paths.iter() {
        match path {
            Path::Cubic(cubic) => {
                draw_points(scene, cubic, session, transform);
            }
            Path::Quadratic(quadratic) => {
                draw_points_quadratic(scene, quadratic, session, transform);
            }
            Path::Hyper(hyper) => {
                // Hyper paths use similar point drawing to cubic
                draw_points_hyper(scene, hyper, session, transform);
            }
        }
    }
}

/// Draw control handles for a cubic path
fn draw_control_handles(scene: &mut Scene, cubic: &crate::path::CubicPath, transform: &Affine) {
    let points: Vec<_> = cubic.points.iter().collect();
    if points.is_empty() {
        return;
    }

    // For each point, if it's on-curve, draw handles to adjacent
    // off-curve points
    for i in 0..points.len() {
        let pt = points[i];

        if !pt.is_on_curve() {
            continue;
        }

        // Look at the next point (with wrapping for closed paths)
        let next_i = if i + 1 < points.len() {
            i + 1
        } else if cubic.closed {
            0
        } else {
            continue;
        };

        // Look at the previous point (with wrapping for closed
        // paths)
        let prev_i = if i > 0 {
            i - 1
        } else if cubic.closed {
            points.len() - 1
        } else {
            continue;
        };

        // Draw handle to next point if it's off-curve
        if next_i < points.len() && points[next_i].is_off_curve() {
            let start = *transform * pt.point;
            let end = *transform * points[next_i].point;
            let line = kurbo::Line::new(start, end);
            let stroke = Stroke::new(theme::size::HANDLE_LINE_WIDTH);
            let brush = Brush::Solid(theme::handle::LINE);
            scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &line);
        }

        // Draw handle to previous point if it's off-curve
        if prev_i < points.len() && points[prev_i].is_off_curve() {
            let start = *transform * pt.point;
            let end = *transform * points[prev_i].point;
            let line = kurbo::Line::new(start, end);
            let stroke = Stroke::new(theme::size::HANDLE_LINE_WIDTH);
            let brush = Brush::Solid(theme::handle::LINE);
            scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &line);
        }
    }
}

/// Draw points for a cubic path
fn draw_points(
    scene: &mut Scene,
    cubic: &crate::path::CubicPath,
    session: &EditSession,
    transform: &Affine,
) {
    for pt in cubic.points.iter() {
        let screen_pos = *transform * pt.point;
        let is_selected = session.selection.contains(&pt.id);

        match pt.typ {
            PointType::OnCurve { smooth } => {
                if smooth {
                    draw_smooth_point(scene, screen_pos, is_selected);
                } else {
                    draw_corner_point(scene, screen_pos, is_selected);
                }
            }
            PointType::OffCurve { .. } => {
                draw_offcurve_point(scene, screen_pos, is_selected);
            }
        }
    }
}

/// Draw a smooth on-curve point as a circle
fn draw_smooth_point(scene: &mut Scene, screen_pos: Point, is_selected: bool) {
    let radius = if is_selected {
        theme::size::SMOOTH_POINT_SELECTED_RADIUS
    } else {
        theme::size::SMOOTH_POINT_RADIUS
    };

    let (inner_color, outer_color) = if is_selected {
        (theme::point::SELECTED_INNER, theme::point::SELECTED_OUTER)
    } else {
        (theme::point::SMOOTH_INNER, theme::point::SMOOTH_OUTER)
    };

    // Outer circle (border)
    let outer_circle = Circle::new(screen_pos, radius + 1.0);
    fill_color(scene, &outer_circle, outer_color);

    // Inner circle
    let inner_circle = Circle::new(screen_pos, radius);
    fill_color(scene, &inner_circle, inner_color);
}

/// Draw a corner on-curve point as a square
fn draw_corner_point(scene: &mut Scene, screen_pos: Point, is_selected: bool) {
    let half_size = if is_selected {
        theme::size::CORNER_POINT_SELECTED_HALF_SIZE
    } else {
        theme::size::CORNER_POINT_HALF_SIZE
    };

    let (inner_color, outer_color) = if is_selected {
        (theme::point::SELECTED_INNER, theme::point::SELECTED_OUTER)
    } else {
        (theme::point::CORNER_INNER, theme::point::CORNER_OUTER)
    };

    // Outer square (border)
    let outer_rect = KurboRect::new(
        screen_pos.x - half_size - 1.0,
        screen_pos.y - half_size - 1.0,
        screen_pos.x + half_size + 1.0,
        screen_pos.y + half_size + 1.0,
    );
    fill_color(scene, &outer_rect, outer_color);

    // Inner square
    let inner_rect = KurboRect::new(
        screen_pos.x - half_size,
        screen_pos.y - half_size,
        screen_pos.x + half_size,
        screen_pos.y + half_size,
    );
    fill_color(scene, &inner_rect, inner_color);
}

/// Draw an off-curve point as a small circle
fn draw_offcurve_point(scene: &mut Scene, screen_pos: Point, is_selected: bool) {
    let radius = if is_selected {
        theme::size::OFFCURVE_POINT_SELECTED_RADIUS
    } else {
        theme::size::OFFCURVE_POINT_RADIUS
    };

    let (inner_color, outer_color) = if is_selected {
        (theme::point::SELECTED_INNER, theme::point::SELECTED_OUTER)
    } else {
        (theme::point::OFFCURVE_INNER, theme::point::OFFCURVE_OUTER)
    };

    // Outer circle (border)
    let outer_circle = Circle::new(screen_pos, radius + 1.0);
    fill_color(scene, &outer_circle, outer_color);

    // Inner circle
    let inner_circle = Circle::new(screen_pos, radius);
    fill_color(scene, &inner_circle, inner_color);
}

/// Draw a hyperbezier on-curve point as a circle (cyan/teal color)
fn draw_hyper_point(scene: &mut Scene, screen_pos: Point, is_selected: bool) {
    let radius = if is_selected {
        theme::size::HYPER_POINT_SELECTED_RADIUS
    } else {
        theme::size::HYPER_POINT_RADIUS
    };

    let (inner_color, outer_color) = if is_selected {
        (theme::point::SELECTED_INNER, theme::point::SELECTED_OUTER)
    } else {
        (theme::point::HYPER_INNER, theme::point::HYPER_OUTER)
    };

    // Outer circle (border)
    let outer_circle = Circle::new(screen_pos, radius + 1.0);
    fill_color(scene, &outer_circle, outer_color);

    // Inner circle
    let inner_circle = Circle::new(screen_pos, radius);
    fill_color(scene, &inner_circle, inner_color);
}

/// Draw control handles for a quadratic path
fn draw_control_handles_quadratic(
    scene: &mut Scene,
    quadratic: &crate::path::QuadraticPath,
    transform: &Affine,
) {
    let points: Vec<_> = quadratic.points.iter().collect();
    if points.is_empty() {
        return;
    }

    // For each point, if it's on-curve, draw handles to adjacent
    // off-curve points
    for i in 0..points.len() {
        let pt = points[i];

        if !pt.is_on_curve() {
            continue;
        }

        // Look at the next point (with wrapping for closed paths)
        let next_i = if i + 1 < points.len() {
            i + 1
        } else if quadratic.closed {
            0
        } else {
            continue;
        };

        // Look at the previous point (with wrapping for closed
        // paths)
        let prev_i = if i > 0 {
            i - 1
        } else if quadratic.closed {
            points.len() - 1
        } else {
            continue;
        };

        // Draw handle to next point if it's off-curve
        if next_i < points.len() && points[next_i].is_off_curve() {
            let start = *transform * pt.point;
            let end = *transform * points[next_i].point;
            let line = kurbo::Line::new(start, end);
            let stroke = Stroke::new(theme::size::HANDLE_LINE_WIDTH);
            let brush = Brush::Solid(theme::handle::LINE);
            scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &line);
        }

        // Draw handle to previous point if it's off-curve
        if prev_i < points.len() && points[prev_i].is_off_curve() {
            let start = *transform * pt.point;
            let end = *transform * points[prev_i].point;
            let line = kurbo::Line::new(start, end);
            let stroke = Stroke::new(theme::size::HANDLE_LINE_WIDTH);
            let brush = Brush::Solid(theme::handle::LINE);
            scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &line);
        }
    }
}

/// Draw points for a quadratic path
fn draw_points_quadratic(
    scene: &mut Scene,
    quadratic: &crate::path::QuadraticPath,
    session: &EditSession,
    transform: &Affine,
) {
    for pt in quadratic.points.iter() {
        let screen_pos = *transform * pt.point;
        let is_selected = session.selection.contains(&pt.id);

        match pt.typ {
            PointType::OnCurve { smooth } => {
                if smooth {
                    draw_smooth_point(scene, screen_pos, is_selected);
                } else {
                    draw_corner_point(scene, screen_pos, is_selected);
                }
            }
            PointType::OffCurve { .. } => {
                draw_offcurve_point(scene, screen_pos, is_selected);
            }
        }
    }
}

/// Draw control handles for a hyper path
fn draw_control_handles_hyper(
    scene: &mut Scene,
    hyper: &crate::path::HyperPath,
    transform: &Affine,
) {
    let points: Vec<_> = hyper.points.iter().collect();
    if points.is_empty() {
        return;
    }

    // For each point, if it's on-curve, draw handles to adjacent
    // off-curve points
    for i in 0..points.len() {
        let pt = points[i];

        if !pt.is_on_curve() {
            continue;
        }

        // Look at the next point (with wrapping for closed paths)
        let next_i = if i + 1 < points.len() {
            i + 1
        } else if hyper.closed {
            0
        } else {
            continue;
        };

        // Look at the previous point (with wrapping for closed paths)
        let prev_i = if i > 0 {
            i - 1
        } else if hyper.closed {
            points.len() - 1
        } else {
            continue;
        };

        // Draw handle to next point if it's off-curve
        if next_i < points.len() && points[next_i].is_off_curve() {
            let start = *transform * pt.point;
            let end = *transform * points[next_i].point;
            let line = kurbo::Line::new(start, end);
            let stroke = Stroke::new(theme::size::HANDLE_LINE_WIDTH);
            let brush = Brush::Solid(theme::handle::LINE);
            scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &line);
        }

        // Draw handle to previous point if it's off-curve
        if prev_i < points.len() && points[prev_i].is_off_curve() {
            let start = *transform * pt.point;
            let end = *transform * points[prev_i].point;
            let line = kurbo::Line::new(start, end);
            let stroke = Stroke::new(theme::size::HANDLE_LINE_WIDTH);
            let brush = Brush::Solid(theme::handle::LINE);
            scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &line);
        }
    }
}

/// Draw points for a hyper path
fn draw_points_hyper(
    scene: &mut Scene,
    hyper: &crate::path::HyperPath,
    session: &EditSession,
    transform: &Affine,
) {
    for pt in hyper.points.iter() {
        let screen_pos = *transform * pt.point;
        let is_selected = session.selection.contains(&pt.id);

        match pt.typ {
            PointType::OnCurve { .. } => {
                // All hyperbezier on-curve points use the hyper point style
                draw_hyper_point(scene, screen_pos, is_selected);
            }
            PointType::OffCurve { .. } => {
                draw_offcurve_point(scene, screen_pos, is_selected);
            }
        }
    }
}
