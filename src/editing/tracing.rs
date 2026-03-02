// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! img2bez integration — trace a background image into editable
//! cubic bezier contours.
//!
//! The public entry point is `trace_background_image()`, which runs
//! the full pipeline: img2bez tracing → kurbo version conversion →
//! bbox alignment with the background image → CubicPath conversion.

use crate::editing::background_image::BackgroundImage;
use crate::model::EntityId;
use crate::path::{CubicPath, PathPoint, PathPoints, PointType};
use crate::settings;

/// Result of tracing a background image.
pub struct TraceOutput {
    /// Traced contours as runebender `Path`s.
    pub paths: Vec<crate::path::Path>,
    /// Advance width computed by img2bez.
    pub advance_width: f64,
}

/// Trace a background image into editable cubic bezier paths.
///
/// Runs img2bez on the image's source file, converts the resulting
/// paths from img2bez's kurbo (v0.13) to local kurbo (v0.12), aligns
/// them with the background image's position in design space, and
/// converts each contour to a `CubicPath`.
pub fn trace_background_image(
    bg: &BackgroundImage,
) -> Result<TraceOutput, String> {
    let image_bounds = bg.bounds();

    let config = img2bez::TracingConfig {
        target_height: bg.scaled_height(),
        y_offset: 0.0,
        alphamax: settings::tracing::ALPHAMAX,
        grid: settings::tracing::GRID,
        ..img2bez::TracingConfig::default()
    };

    let result = img2bez::trace(&bg.source_path, &config)
        .map_err(|e| format!("img2bez trace failed: {e}"))?;

    // Convert img2bez BezPaths (kurbo 0.13) to local kurbo (0.12)
    let mut local_paths: Vec<kurbo::BezPath> = result
        .paths
        .iter()
        .map(convert_img2bez_bezpath)
        .collect();

    // Align traced contours with the background image.
    // img2bez repositions paths to sit at y=0 with LSB padding,
    // but we need them to overlay the image at its current
    // design-space position.
    use kurbo::Shape;
    if let Some(traced_bbox) = local_paths
        .iter()
        .map(|p| p.bounding_box())
        .reduce(|a, b| a.union(b))
    {
        let tcx = (traced_bbox.x0 + traced_bbox.x1) / 2.0;
        let tcy = (traced_bbox.y0 + traced_bbox.y1) / 2.0;
        let icx = (image_bounds.x0 + image_bounds.x1) / 2.0;
        let icy = (image_bounds.y0 + image_bounds.y1) / 2.0;
        let shift =
            kurbo::Affine::translate((icx - tcx, icy - tcy));
        for p in &mut local_paths {
            p.apply_affine(shift);
        }
    }

    // Convert to runebender CubicPaths
    let paths: Vec<crate::path::Path> = local_paths
        .iter()
        .map(|bp| crate::path::Path::Cubic(bezpath_to_cubic(bp)))
        .collect();

    Ok(TraceOutput {
        paths,
        advance_width: result.advance_width,
    })
}

// ============================================================================
// CONVERSION HELPERS
// ============================================================================

/// Convert an img2bez kurbo::BezPath (v0.13) to local kurbo::BezPath
/// (v0.12).
///
/// img2bez uses a different version of kurbo than runebender
/// (0.13 vs 0.12). This function bridges the gap by extracting
/// raw coordinates from each path element.
fn convert_img2bez_bezpath(
    src: &img2bez::kurbo::BezPath,
) -> kurbo::BezPath {
    let mut dst = kurbo::BezPath::new();
    for el in src.elements() {
        match *el {
            img2bez::kurbo::PathEl::MoveTo(p) => {
                dst.move_to(kurbo::Point::new(p.x, p.y));
            }
            img2bez::kurbo::PathEl::LineTo(p) => {
                dst.line_to(kurbo::Point::new(p.x, p.y));
            }
            img2bez::kurbo::PathEl::QuadTo(p1, p2) => {
                dst.quad_to(
                    kurbo::Point::new(p1.x, p1.y),
                    kurbo::Point::new(p2.x, p2.y),
                );
            }
            img2bez::kurbo::PathEl::CurveTo(p1, p2, p3) => {
                dst.curve_to(
                    kurbo::Point::new(p1.x, p1.y),
                    kurbo::Point::new(p2.x, p2.y),
                    kurbo::Point::new(p3.x, p3.y),
                );
            }
            img2bez::kurbo::PathEl::ClosePath => {
                dst.close_path();
            }
        }
    }
    dst
}

/// Convert a kurbo::BezPath (single contour) to a CubicPath for
/// editing.
///
/// Walks the BezPath elements and creates PathPoints with
/// appropriate types: CurveTo endpoints are smooth on-curve,
/// LineTo/MoveTo endpoints are corner on-curve, and CurveTo
/// control points are off-curve handles.
fn bezpath_to_cubic(bezpath: &kurbo::BezPath) -> CubicPath {
    let mut points = Vec::new();
    let has_close = bezpath
        .elements()
        .iter()
        .any(|el| matches!(el, kurbo::PathEl::ClosePath));

    for el in bezpath.elements() {
        match *el {
            kurbo::PathEl::MoveTo(p) => {
                points.push(PathPoint {
                    id: EntityId::next(),
                    point: p,
                    typ: PointType::OnCurve { smooth: false },
                });
            }
            kurbo::PathEl::LineTo(p) => {
                points.push(PathPoint {
                    id: EntityId::next(),
                    point: p,
                    typ: PointType::OnCurve { smooth: false },
                });
            }
            kurbo::PathEl::CurveTo(cp1, cp2, end) => {
                points.push(PathPoint {
                    id: EntityId::next(),
                    point: cp1,
                    typ: PointType::OffCurve { auto: false },
                });
                points.push(PathPoint {
                    id: EntityId::next(),
                    point: cp2,
                    typ: PointType::OffCurve { auto: false },
                });
                points.push(PathPoint {
                    id: EntityId::next(),
                    point: end,
                    typ: PointType::OnCurve { smooth: true },
                });
            }
            kurbo::PathEl::QuadTo(cp, end) => {
                points.push(PathPoint {
                    id: EntityId::next(),
                    point: cp,
                    typ: PointType::OffCurve { auto: false },
                });
                points.push(PathPoint {
                    id: EntityId::next(),
                    point: end,
                    typ: PointType::OnCurve { smooth: true },
                });
            }
            kurbo::PathEl::ClosePath => {
                // CubicPath handles closing via the closed flag
            }
        }
    }

    // For closed paths, apply CubicPath's convention:
    // rotate_left(1) so the first point becomes last
    if has_close && !points.is_empty() {
        points.rotate_left(1);
    }

    CubicPath::new(PathPoints::from_vec(points), has_close)
}
