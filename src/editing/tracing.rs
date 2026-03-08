// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! img2bez integration — trace and refit background images into
//! editable cubic bezier contours.
//!
//! Public entry points:
//! - `trace_background_image()` — fresh trace from raster image
//! - `refit_background_image()` — refit existing outlines onto a
//!   raster image, preserving point count/types for variable font
//!   interpolation compatibility

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

/// Refit existing outlines onto a background image.
///
/// Instead of tracing from scratch, this projects existing path
/// points onto the traced target shape. The number, type, and
/// winding direction of points are preserved — only positions
/// change. This produces interpolation-compatible outlines for
/// variable font workflows.
///
/// Algorithm:
/// 1. Trace the background image (same as Cmd+T) to get target
///    contours in design space.
/// 2. Match each existing contour to the closest target contour
///    by centroid proximity.
/// 3. For each on-curve point, compute its outward normal and
///    ray-cast onto the target boundary.
/// 4. Off-curve control points are adjusted via similarity
///    transform to preserve their relative position.
///
/// Both existing and target paths are in design space — no
/// coordinate conversions needed.
pub fn refit_background_image(
    bg: &BackgroundImage,
    existing_paths: &[crate::path::Path],
) -> Result<TraceOutput, String> {
    // Trace the image to get target paths in design space.
    // This uses the same pipeline as Cmd+T (which works).
    let traced = trace_background_image(bg)?;

    // Convert target paths to kurbo BezPaths for projection.
    let target_bezpaths: Vec<kurbo::BezPath> = traced
        .paths
        .iter()
        .map(|p| p.to_bezpath())
        .collect();

    // Convert existing paths to kurbo BezPaths (already in
    // design space — same coordinate space as targets).
    let existing_bezpaths: Vec<kurbo::BezPath> = existing_paths
        .iter()
        .map(|p| p.to_bezpath())
        .collect();

    // Match each existing contour to the closest target.
    let matches =
        match_contours(&existing_bezpaths, &target_bezpaths);

    // Log contour matching info.
    for (i, existing) in existing_bezpaths.iter().enumerate() {
        use kurbo::Shape;
        let eb = existing.bounding_box();
        tracing::info!(
            "Refit: contour {} bbox=[{:.1},{:.1}]-[{:.1},{:.1}] \
             → target {:?}",
            i,
            eb.x0, eb.y0, eb.x1, eb.y1,
            matches[i],
        );
    }

    // Scale each existing path to match its target's bbox.
    let refitted_bezpaths: Vec<kurbo::BezPath> = existing_bezpaths
        .iter()
        .enumerate()
        .map(|(i, existing)| {
            if let Some(target_idx) = matches[i] {
                refit_path_minimal(
                    existing,
                    &target_bezpaths[target_idx],
                    )
            } else {
                existing.clone()
            }
        })
        .collect();

    // Rebuild CubicPaths preserving EntityIds, point types, etc.
    let paths: Vec<crate::path::Path> =
        rebuild_paths_with_new_positions(
            existing_paths,
            &refitted_bezpaths,
        );

    Ok(TraceOutput {
        paths,
        advance_width: traced.advance_width,
    })
}

/// Rebuild runebender paths using new point positions from refitted
/// BezPaths, preserving EntityIds, point types, smooth flags, and
/// closed state from the originals.
fn rebuild_paths_with_new_positions(
    originals: &[crate::path::Path],
    refitted: &[kurbo::BezPath],
) -> Vec<crate::path::Path> {
    originals
        .iter()
        .zip(refitted.iter())
        .map(|(orig, new_bp)| {
            match orig {
                crate::path::Path::Cubic(cubic) => {
                    let new_points = extract_points_from_bezpath(new_bp);
                    let orig_points: Vec<&PathPoint> =
                        cubic.points.iter().collect();

                    // The refitted BezPath should have the same number
                    // of on-curve + off-curve points. Map new positions
                    // onto the original point metadata.
                    let updated: Vec<PathPoint> = if orig_points.len()
                        == new_points.len()
                    {
                        orig_points
                            .iter()
                            .zip(new_points.iter())
                            .map(|(orig_pt, new_pos)| PathPoint {
                                id: orig_pt.id,
                                point: *new_pos,
                                typ: orig_pt.typ,
                            })
                            .collect()
                    } else {
                        // Fallback: point counts don't match, use
                        // bezpath_to_cubic conversion.
                        tracing::warn!(
                            "Point count mismatch in refit: \
                             orig={}, refitted={}",
                            orig_points.len(),
                            new_points.len()
                        );
                        return crate::path::Path::Cubic(
                            bezpath_to_cubic(new_bp),
                        );
                    };

                    crate::path::Path::Cubic(CubicPath::new(
                        PathPoints::from_vec(updated),
                        cubic.closed,
                    ))
                }
                // For non-cubic paths, fall back to fresh conversion.
                _ => crate::path::Path::Cubic(bezpath_to_cubic(new_bp)),
            }
        })
        .collect()
}

/// Extract point positions from a BezPath in the same order as
/// CubicPath stores them (with the rotate_left(1) convention for
/// closed paths).
fn extract_points_from_bezpath(
    bezpath: &kurbo::BezPath,
) -> Vec<kurbo::Point> {
    let mut points = Vec::new();
    let has_close = bezpath
        .elements()
        .iter()
        .any(|el| matches!(el, kurbo::PathEl::ClosePath));

    for el in bezpath.elements() {
        match *el {
            kurbo::PathEl::MoveTo(p) => points.push(p),
            kurbo::PathEl::LineTo(p) => points.push(p),
            kurbo::PathEl::CurveTo(cp1, cp2, end) => {
                points.push(cp1);
                points.push(cp2);
                points.push(end);
            }
            kurbo::PathEl::QuadTo(cp, end) => {
                points.push(cp);
                points.push(end);
            }
            kurbo::PathEl::ClosePath => {}
        }
    }

    if has_close && !points.is_empty() {
        points.rotate_left(1);
    }

    points
}

// ============================================================================
// CONTOUR MATCHING & REFIT
// ============================================================================

/// Match each existing contour to the closest target contour by
/// centroid proximity.
fn match_contours(
    existing: &[kurbo::BezPath],
    targets: &[kurbo::BezPath],
) -> Vec<Option<usize>> {
    if targets.is_empty() {
        return vec![None; existing.len()];
    }

    let target_centroids: Vec<kurbo::Point> =
        targets.iter().map(path_centroid).collect();

    existing
        .iter()
        .map(|ep| {
            let ec = path_centroid(ep);
            target_centroids
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let da = (ec - **a).hypot2();
                    let db = (ec - **b).hypot2();
                    da.partial_cmp(&db)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(idx, _)| idx)
        })
        .collect()
}

/// Centroid of a BezPath (average of on-curve endpoint positions).
fn path_centroid(path: &kurbo::BezPath) -> kurbo::Point {
    let mut sum = kurbo::Vec2::ZERO;
    let mut count = 0;
    for el in path.elements() {
        let p = match *el {
            kurbo::PathEl::MoveTo(p)
            | kurbo::PathEl::LineTo(p)
            | kurbo::PathEl::CurveTo(_, _, p)
            | kurbo::PathEl::QuadTo(_, p) => p,
            kurbo::PathEl::ClosePath => continue,
        };
        sum += p.to_vec2();
        count += 1;
    }
    if count == 0 {
        kurbo::Point::ZERO
    } else {
        (sum / count as f64).to_point()
    }
}

/// Refit a single path using piecewise linear warping.
///
/// Detects vertical stem edges in both existing and target
/// contours, then applies a 3-zone horizontal warp:
///   - Left serif zone: scales to match left serif width
///   - Stem zone: scales to match stem width (typically
///     the biggest change between regular and bold)
///   - Right serif zone: scales to match right serif width
///
/// Y coordinates are scaled linearly to match bbox heights.
///
/// This preserves H/V handle alignment perfectly:
///   - Vertical handles have the same X, so they get the
///     same X warp → stay vertical.
///   - Horizontal handles have the same Y, so they get the
///     same Y scale → stay horizontal.
fn refit_path_minimal(
    existing: &kurbo::BezPath,
    target: &kurbo::BezPath,
) -> kurbo::BezPath {
    use kurbo::Shape;

    if existing.elements().is_empty() {
        return kurbo::BezPath::new();
    }

    let eb = existing.bounding_box();
    let tb = target.bounding_box();
    let ew = eb.x1 - eb.x0;
    let eh = eb.y1 - eb.y0;

    if ew < 1e-6 || eh < 1e-6 {
        return existing.clone();
    }

    // Detect vertical stem edges for piecewise X warp.
    let existing_stems = detect_vertical_stems(existing);
    let target_stems = detect_vertical_stems(target);

    let x_warp = if let (Some((el, er)), Some((tl, tr))) =
        (existing_stems, target_stems)
    {
        tracing::info!(
            "Refit: stems existing=[{:.1}, {:.1}] \
             target=[{:.1}, {:.1}]",
            el, er, tl, tr,
        );
        // 3-zone: left serif | stem | right serif
        PiecewiseWarp::new(vec![
            (eb.x0, tb.x0),
            (el, tl),
            (er, tr),
            (eb.x1, tb.x1),
        ])
    } else {
        tracing::info!("Refit: no stems detected, linear scale");
        PiecewiseWarp::new(vec![
            (eb.x0, tb.x0),
            (eb.x1, tb.x1),
        ])
    };

    // Detect horizontal edges for piecewise Y warp.
    // Only warp Y if there are INTERIOR horizontal stems
    // (like a crossbar in "H" or "E"). If the detected
    // horizontal lines are just the bbox edges (as in "I"),
    // keep Y unchanged — the baseline and cap height should
    // be preserved from the original outline.
    let existing_h = detect_horizontal_stems(existing);
    let target_h = detect_horizontal_stems(target);

    let y_warp = if let Some((eb_lo, eb_hi)) = existing_h {
        let at_bottom = (eb_lo - eb.y0).abs() < 10.0;
        let at_top = (eb_hi - eb.y1).abs() < 10.0;

        if at_bottom && at_top {
            // Stems are at bbox edges (no interior crossbar).
            // Keep Y unchanged to preserve baseline/cap height.
            tracing::info!(
                "Refit: h-stems at bbox edges, keeping Y",
            );
            PiecewiseWarp::new(vec![
                (eb.y0, eb.y0),
                (eb.y1, eb.y1),
            ])
        } else {
            // Interior horizontal stems found — warp Y.
            let (tb_lo, tb_hi) = target_h
                .unwrap_or((tb.y0, tb.y1));
            tracing::info!(
                "Refit: h-stems existing=[{:.1}, {:.1}] \
                 target=[{:.1}, {:.1}]",
                eb_lo, eb_hi, tb_lo, tb_hi,
            );
            PiecewiseWarp::new(vec![
                (eb.y0, tb.y0),
                (eb_lo, tb_lo),
                (eb_hi, tb_hi),
                (eb.y1, tb.y1),
            ])
        }
    } else {
        // No horizontal stems at all — keep Y unchanged.
        tracing::info!("Refit: no h-stems, keeping Y");
        PiecewiseWarp::new(vec![
            (eb.y0, eb.y0),
            (eb.y1, eb.y1),
        ])
    };

    tracing::info!(
        "Refit: [{:.1},{:.1}]-[{:.1},{:.1}] \
         → [{:.1},{:.1}]-[{:.1},{:.1}]",
        eb.x0, eb.y0, eb.x1, eb.y1,
        tb.x0, tb.y0, tb.x1, tb.y1,
    );

    // Apply the warp to every point in the BezPath.
    let mut result = kurbo::BezPath::new();
    for el in existing.elements() {
        let warp =
            |p: kurbo::Point| -> kurbo::Point {
                kurbo::Point::new(
                    x_warp.map(p.x),
                    y_warp.map(p.y),
                )
            };

        match *el {
            kurbo::PathEl::MoveTo(p) => {
                result.move_to(warp(p));
            }
            kurbo::PathEl::LineTo(p) => {
                result.line_to(warp(p));
            }
            kurbo::PathEl::CurveTo(c1, c2, p) => {
                result.curve_to(warp(c1), warp(c2), warp(p));
            }
            kurbo::PathEl::QuadTo(c, p) => {
                result.quad_to(warp(c), warp(p));
            }
            kurbo::PathEl::ClosePath => {
                result.close_path();
            }
        }
    }

    result
}

/// Smooth monotone warp using cubic Hermite interpolation.
///
/// Unlike a piecewise linear warp, this transitions gradually
/// between zones. Bracket curves that span the stem-to-serif
/// boundary are warped smoothly instead of kinked, preserving
/// collinearity of points and handle relationships.
///
/// Uses Fritsch-Carlson method for monotonicity.
struct PiecewiseWarp {
    /// (source, target) knots sorted by source value.
    knots: Vec<(f64, f64)>,
    /// Tangent slope at each knot.
    tangents: Vec<f64>,
}

impl PiecewiseWarp {
    fn new(knots: Vec<(f64, f64)>) -> Self {
        let n = knots.len();
        if n < 2 {
            return Self {
                knots,
                tangents: vec![1.0],
            };
        }

        // Compute segment slopes (deltas).
        let mut deltas = Vec::with_capacity(n - 1);
        for i in 0..n - 1 {
            let dx = knots[i + 1].0 - knots[i].0;
            let dy = knots[i + 1].1 - knots[i].1;
            deltas.push(if dx.abs() > 1e-10 {
                dy / dx
            } else {
                1.0
            });
        }

        // Initial tangents: average of adjacent slopes.
        let mut tangents = vec![0.0; n];
        tangents[0] = deltas[0];
        tangents[n - 1] = deltas[n - 2];
        for i in 1..n - 1 {
            if deltas[i - 1].signum() != deltas[i].signum() {
                tangents[i] = 0.0;
            } else {
                tangents[i] =
                    (deltas[i - 1] + deltas[i]) / 2.0;
            }
        }

        // Fritsch-Carlson monotonicity adjustment.
        for i in 0..n - 1 {
            if deltas[i].abs() < 1e-10 {
                tangents[i] = 0.0;
                tangents[i + 1] = 0.0;
            } else {
                let alpha = tangents[i] / deltas[i];
                let beta = tangents[i + 1] / deltas[i];
                let mag_sq = alpha * alpha + beta * beta;
                if mag_sq > 9.0 {
                    let tau = 3.0 / mag_sq.sqrt();
                    tangents[i] = tau * alpha * deltas[i];
                    tangents[i + 1] =
                        tau * beta * deltas[i];
                }
            }
        }

        Self { knots, tangents }
    }

    fn map(&self, x: f64) -> f64 {
        let n = self.knots.len();
        if n < 2 {
            return x;
        }
        let (s0, t0) = self.knots[0];
        let (sn, tn) = self.knots[n - 1];

        // Extrapolate beyond endpoints using edge tangent.
        if x <= s0 {
            return t0 + self.tangents[0] * (x - s0);
        }
        if x >= sn {
            return tn + self.tangents[n - 1] * (x - sn);
        }

        // Find the enclosing segment and evaluate
        // cubic Hermite.
        for i in 0..n - 1 {
            let (sa, ya) = self.knots[i];
            let (sb, yb) = self.knots[i + 1];
            if x <= sb {
                let h = sb - sa;
                if h.abs() < 1e-10 {
                    return ya;
                }
                let t = (x - sa) / h;
                let t2 = t * t;
                let t3 = t2 * t;

                // Hermite basis functions.
                let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
                let h10 = t3 - 2.0 * t2 + t;
                let h01 = -2.0 * t3 + 3.0 * t2;
                let h11 = t3 - t2;

                return h00 * ya
                    + h10 * h * self.tangents[i]
                    + h01 * yb
                    + h11 * h * self.tangents[i + 1];
            }
        }
        x
    }
}

/// Detect the two longest vertical line segments in a path.
///
/// Returns (left_x, right_x) of the stem edges, or None if
/// fewer than 2 vertical segments are found.
fn detect_vertical_stems(
    path: &kurbo::BezPath,
) -> Option<(f64, f64)> {
    // Collect all near-vertical line segments.
    let mut verticals: Vec<(f64, f64)> = Vec::new();

    for seg in path.segments() {
        if let kurbo::PathSeg::Line(line) = seg {
            let dx = (line.p1.x - line.p0.x).abs();
            let dy = (line.p1.y - line.p0.y).abs();
            if dx < 2.0 && dy > 50.0 {
                let x = (line.p0.x + line.p1.x) / 2.0;
                verticals.push((x, dy));
            }
        }
    }

    if verticals.len() < 2 {
        return None;
    }

    // Sort by length descending, take the two longest.
    verticals.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let x1 = verticals[0].0;
    let x2 = verticals[1].0;

    Some((x1.min(x2), x1.max(x2)))
}

/// Detect the two longest horizontal line segments in a path.
///
/// Returns (bottom_y, top_y) of the horizontal edges, or None
/// if fewer than 2 horizontal segments are found.
fn detect_horizontal_stems(
    path: &kurbo::BezPath,
) -> Option<(f64, f64)> {
    let mut horizontals: Vec<(f64, f64)> = Vec::new();

    for seg in path.segments() {
        if let kurbo::PathSeg::Line(line) = seg {
            let dx = (line.p1.x - line.p0.x).abs();
            let dy = (line.p1.y - line.p0.y).abs();
            if dy < 2.0 && dx > 50.0 {
                let y = (line.p0.y + line.p1.y) / 2.0;
                horizontals.push((y, dx));
            }
        }
    }

    if horizontals.len() < 2 {
        return None;
    }

    // Sort by length descending, take the two longest.
    horizontals.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let y1 = horizontals[0].0;
    let y2 = horizontals[1].0;

    Some((y1.min(y2), y1.max(y2)))
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
