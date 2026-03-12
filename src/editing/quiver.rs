// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! QuiverAI integration — cloud-based image vectorization as an
//! alternative to img2bez.
//!
//! Uses the QuiverAI API (`api.quiver.ai/v1/svgs/vectorizations`)
//! to convert a raster image into SVG, then parses SVG paths into
//! editable cubic bezier contours.
//!
//! API key is read from `~/.config/runebender/config.toml`
//! (under `[quiver] api_key`) or the `QUIVERAI_API_KEY`
//! environment variable.

use crate::editing::background_image::BackgroundImage;
use crate::editing::tracing::{TraceOutput, bezpath_to_cubic};

// ================================================================
// PUBLIC API
// ================================================================

/// Trace a background image using QuiverAI cloud vectorization.
///
/// Sends the image to QuiverAI's vectorization endpoint, parses
/// the returned SVG paths, aligns them with the background image
/// position, and converts to editable CubicPaths.
///
/// `advance_width` is the glyph's current advance width (used as
/// the output advance width since QuiverAI doesn't compute one).
pub fn trace_with_quiver(
    bg: &BackgroundImage,
    advance_width: f64,
) -> Result<TraceOutput, String> {
    let api_key =
        crate::config::quiver_api_key().ok_or_else(|| {
            "QuiverAI API key not found. Set it in \
             ~/.config/runebender/config.toml under \
             [quiver] api_key, or set the \
             QUIVERAI_API_KEY environment variable. \
             Get a key at https://quiver.ai/"
                .to_string()
        })?;

    // Read the source image and base64-encode it
    let image_bytes =
        std::fs::read(&bg.source_path).map_err(|e| {
            format!(
                "Failed to read image {}: {e}",
                bg.source_path.display()
            )
        })?;

    let b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &image_bytes,
    );

    // Build the request using the generation endpoint with the
    // image as a reference. This lets us prompt for a clean
    // single-contour outline rather than a multi-layer visual SVG.
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| {
            format!("Failed to create HTTP client: {e}")
        })?;
    let body = serde_json::json!({
        "model": "arrow-preview",
        "prompt": "Trace the exact outline of this letter as \
                   clean closed vector paths, like a \
                   professional type designer would draw. \
                   Include the outer contour and any inner \
                   counter shapes (holes). Output only the \
                   letter contours — no background, no fill, \
                   no decorations, no extra shapes, no \
                   bounding box. Black stroke, no fill.",
        "references": [{
            "base64": b64
        }],
        "n": 1,
        "temperature": 0.2
    });

    tracing::info!(
        "Sending image to QuiverAI generation ({} bytes)...",
        image_bytes.len()
    );

    let response = client
        .post("https://api.quiver.ai/v1/svgs/generations")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("QuiverAI request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .unwrap_or_else(|_| "unknown error".into());
        return Err(format!(
            "QuiverAI API error ({status}): {error_text}"
        ));
    }

    let resp_json: serde_json::Value =
        response.json().map_err(|e| {
            format!("Failed to parse QuiverAI response: {e}")
        })?;

    // Extract SVG string from response
    // Response format: { data: [{ svg: "<svg>...</svg>" }] }
    let svg_string = resp_json["data"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|item| item["svg"].as_str())
        .ok_or_else(|| {
            format!(
                "QuiverAI response missing SVG data: \
                 {resp_json}"
            )
        })?;

    tracing::info!(
        "QuiverAI returned {} bytes of SVG",
        svg_string.len()
    );

    // Save raw SVG for debugging
    let debug_path = crate::config::config_dir()
        .map(|d| d.join("last_quiver_response.svg"))
        .unwrap_or_else(|| {
            std::path::PathBuf::from(
                "/tmp/last_quiver_response.svg",
            )
        });
    match std::fs::write(&debug_path, svg_string) {
        Ok(()) => {
            tracing::info!(
                "Saved raw SVG to {}",
                debug_path.display()
            );
        }
        Err(e) => {
            tracing::warn!(
                "Failed to save debug SVG to {}: {e}",
                debug_path.display()
            );
        }
    }

    // Parse SVG paths into kurbo BezPaths
    let bezpaths = parse_svg_paths(svg_string)?;

    if bezpaths.is_empty() {
        return Err(
            "QuiverAI SVG contains no path elements".into()
        );
    }

    tracing::info!(
        "Parsed {} contours from QuiverAI SVG",
        bezpaths.len()
    );

    // Align traced contours with the background image position
    let image_bounds = bg.bounds();
    let mut local_paths = bezpaths;

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

    // Scale SVG paths to match design space.
    // QuiverAI outputs SVG coordinates (typically viewBox-sized),
    // so we need to scale to match the background image's
    // design-space dimensions.
    scale_to_design_space(&mut local_paths, bg);

    // Convert to editable CubicPaths
    let paths: Vec<crate::path::Path> = local_paths
        .iter()
        .map(|bp| {
            crate::path::Path::Cubic(bezpath_to_cubic(bp))
        })
        .collect();

    Ok(TraceOutput {
        paths,
        advance_width,
    })
}

// ================================================================
// SVG PARSING
// ================================================================

/// Parse all `<path d="...">` attributes from an SVG string into
/// kurbo BezPaths, filtering out background rectangles and
/// decorative elements.
fn parse_svg_paths(
    svg: &str,
) -> Result<Vec<kurbo::BezPath>, String> {
    let mut raw_paths = Vec::new();

    // Simple regex-free parser: find d="..." attributes in
    // <path> elements. This handles the common SVG output from
    // QuiverAI without pulling in a full XML parser.
    let mut search = svg;
    while let Some(path_start) = search.find("<path") {
        let remaining = &search[path_start..];

        // Find the end of this element
        let element_end =
            remaining.find("/>").or_else(|| remaining.find('>'));
        let element = match element_end {
            Some(end) => &remaining[..end],
            None => break,
        };

        // Extract d="..." attribute
        if let Some(d_value) = extract_d_attribute(element) {
            match kurbo::BezPath::from_svg(d_value) {
                Ok(bezpath) => {
                    // Split multi-contour paths into individual
                    // BezPaths (each MoveTo starts a new contour)
                    let sub_paths =
                        split_into_subpaths(&bezpath);
                    raw_paths.extend(sub_paths);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse SVG path: {e}"
                    );
                }
            }
        }

        // Advance past this element
        search = match element_end {
            Some(end) => &search[path_start + end + 1..],
            None => break,
        };
    }

    tracing::info!(
        "Raw SVG contained {} sub-paths before filtering",
        raw_paths.len()
    );

    // Filter out background rectangles and decorative junk
    let filtered: Vec<kurbo::BezPath> = raw_paths
        .into_iter()
        .filter(|p| !is_axis_aligned_rect(p))
        .collect();

    tracing::info!(
        "{} paths remain after filtering rectangles",
        filtered.len()
    );

    Ok(filtered)
}

/// Check if a BezPath is an axis-aligned rectangle (likely a
/// background or bounding box, not an actual glyph contour).
fn is_axis_aligned_rect(path: &kurbo::BezPath) -> bool {
    let els = path.elements();

    // A rectangle is: MoveTo + 3 LineTo + ClosePath (or 4
    // LineTo + ClosePath)
    let line_count = els
        .iter()
        .filter(|e| matches!(e, kurbo::PathEl::LineTo(_)))
        .count();
    let curve_count = els.iter().filter(|e| {
        matches!(
            e,
            kurbo::PathEl::CurveTo(..)
                | kurbo::PathEl::QuadTo(..)
        )
    }).count();
    let has_close = els
        .iter()
        .any(|e| matches!(e, kurbo::PathEl::ClosePath));

    // Must be lines only (no curves), 3-4 lines, and closed
    if curve_count > 0 || line_count < 3 || line_count > 4
        || !has_close
    {
        return false;
    }

    // Collect all points
    let points: Vec<kurbo::Point> = els
        .iter()
        .filter_map(|e| match e {
            kurbo::PathEl::MoveTo(p)
            | kurbo::PathEl::LineTo(p) => Some(*p),
            _ => None,
        })
        .collect();

    if points.len() < 4 {
        return false;
    }

    // Check if all segments are axis-aligned (horizontal or
    // vertical)
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let dx = (a.x - b.x).abs();
        let dy = (a.y - b.y).abs();
        // Neither horizontal nor vertical
        if dx > 0.5 && dy > 0.5 {
            return false;
        }
    }

    use kurbo::Shape;
    tracing::debug!(
        "Filtered axis-aligned rectangle: {:?}",
        path.bounding_box()
    );
    true
}

/// Extract the `d` attribute value from a `<path ...>` element
/// string.
fn extract_d_attribute(element: &str) -> Option<&str> {
    // Look for d=" or d='
    let d_pos = element.find(" d=\"").or_else(|| element.find(" d='"));
    let d_pos = d_pos?;
    let after_d = &element[d_pos + 3..]; // skip ' d='
    let quote_char = after_d.as_bytes()[0];
    let value_start = &after_d[1..]; // skip opening quote
    let end = value_start
        .find(|c: char| c as u8 == quote_char)?;
    Some(&value_start[..end])
}

/// Split a BezPath with multiple MoveTo commands into separate
/// sub-paths (one per contour).
fn split_into_subpaths(
    bezpath: &kurbo::BezPath,
) -> Vec<kurbo::BezPath> {
    let mut paths = Vec::new();
    let mut current = kurbo::BezPath::new();

    for el in bezpath.elements() {
        match el {
            kurbo::PathEl::MoveTo(_) => {
                if !current.elements().is_empty() {
                    paths.push(current);
                    current = kurbo::BezPath::new();
                }
                current.push(*el);
            }
            _ => {
                current.push(*el);
            }
        }
    }

    if !current.elements().is_empty() {
        paths.push(current);
    }

    paths
}

/// Scale SVG-coordinate paths to match the background image's
/// design-space size.
fn scale_to_design_space(
    paths: &mut [kurbo::BezPath],
    bg: &BackgroundImage,
) {
    use kurbo::Shape;

    if paths.is_empty() {
        return;
    }

    // Get the bounding box of all SVG paths
    let svg_bbox = match paths
        .iter()
        .map(|p| p.bounding_box())
        .reduce(|a, b| a.union(b))
    {
        Some(bbox) => bbox,
        None => return,
    };

    let svg_w = svg_bbox.width();
    let svg_h = svg_bbox.height();
    if svg_w < 0.001 || svg_h < 0.001 {
        return;
    }

    // Get the background image's design-space dimensions
    let image_bounds = bg.bounds();
    let design_w = image_bounds.width();
    let design_h = image_bounds.height();

    // Uniform scale to fit design space
    let scale = (design_w / svg_w).min(design_h / svg_h);
    if (scale - 1.0).abs() < 0.001 {
        return; // Already the right size
    }

    // Scale around the center of the SVG paths
    let center = svg_bbox.center();
    let transform = kurbo::Affine::translate(
        (center.x, center.y),
    ) * kurbo::Affine::scale(scale)
        * kurbo::Affine::translate((-center.x, -center.y));

    for p in paths.iter_mut() {
        p.apply_affine(transform);
    }
}
