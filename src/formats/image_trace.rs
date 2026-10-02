// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Image tracing through img2bez, the deterministic autotracer the
//! web editor uses.
//!
//! The legacy [`trace_image`] path uses the web editor's tracing defaults and
//! returns canonical contours ready to install into a document layer.
//! [`trace_image_calibrated`] retains the tracer's structured points and maps
//! the full source image into font coordinates without fitting the ink box.

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::outline::drawing::{DrawingContour, DrawingPoint, DrawingPointType};

const MAX_CALIBRATED_IMAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_CALIBRATED_IMAGE_EDGE: u32 = 1_024;
const MAX_CALIBRATED_IMAGE_PIXELS: u64 = 262_144;
const TRACE_EM_HEIGHT: f64 = 1_088.0;

/// Explicit mapping from top-left image pixel boundaries into font coordinates.
///
/// Pixel x increases rightward and pixel y increases downward. Pixel (0, 0) is
/// the upper-left image boundary, including any white padding. The font x of
/// that boundary is `font_x_at_left`; `pixel_baseline_y` maps to `font_baseline_y`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceCalibration {
    /// Positive number of font units represented by one image pixel.
    pub font_units_per_pixel: f64,
    /// Pixel y coordinate of the intended baseline, measured from the top boundary.
    pub pixel_baseline_y: f64,
    /// Font x coordinate of the image's left boundary.
    pub font_x_at_left: f64,
    /// Font y coordinate of the intended baseline.
    pub font_baseline_y: f64,
}

impl TraceCalibration {
    fn validate(self, image_height: u32) -> Result<(), String> {
        if !self.font_units_per_pixel.is_finite()
            || !(0.01..=1_024.0).contains(&self.font_units_per_pixel)
        {
            return Err("font units per image pixel must be finite and within 0.01..=1024".into());
        }
        if !self.pixel_baseline_y.is_finite()
            || !(0.0..=f64::from(image_height)).contains(&self.pixel_baseline_y)
        {
            return Err("pixel baseline must be finite and inside the image".into());
        }
        if !self.font_x_at_left.is_finite()
            || !self.font_baseline_y.is_finite()
            || self.font_x_at_left.abs() > 1_000_000.0
            || self.font_baseline_y.abs() > 1_000_000.0
        {
            return Err("font placement must be finite and within one million units".into());
        }
        Ok(())
    }
}

/// Detached traced geometry and the exact image placement used to create it.
///
/// The SHA-256 digest identifies the original encoded image bytes. The contour
/// points retain img2bez's order, segment roles, extrema, and smooth flags.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalibratedTrace {
    /// SHA-256 of the encoded image passed to the tracer.
    pub image_sha256: String,
    /// Original image width in pixels, including padding.
    pub image_width_px: u32,
    /// Original image height in pixels, including padding.
    pub image_height_px: u32,
    /// Exact placement of original pixel boundaries in font coordinates.
    pub calibration: TraceCalibration,
    /// Whether light pixels were treated as ink.
    pub invert: bool,
    /// Selected deterministic img2bez tracing profile.
    pub tracer: String,
    /// SHA-256 of the pinned dependency lockfile used by the tracer build.
    pub cargo_lock_sha256: String,
    /// Detached ordinary contours in font coordinates.
    pub contours: Vec<DrawingContour>,
}

/// Trace one bounded image using an explicit pixel-to-font calibration.
///
/// This leaves the canonical document unchanged. It traces at img2bez's normal
/// em scale, then transforms every stored point using the full image dimensions.
/// It never fits ink bounds to a target height or discards source padding.
pub fn trace_image_calibrated(
    image_bytes: &[u8],
    calibration: TraceCalibration,
    invert: bool,
) -> Result<CalibratedTrace, String> {
    use img2bez::PointKind;
    use img2bez::image::{ImageReader, guess_format};

    if image_bytes.is_empty() || image_bytes.len() > MAX_CALIBRATED_IMAGE_BYTES {
        return Err("calibrated trace needs 1 to 4194304 encoded image bytes".into());
    }
    let format = guess_format(image_bytes).map_err(|error| format!("image format: {error}"))?;
    let (width, height) = ImageReader::with_format(std::io::Cursor::new(image_bytes), format)
        .into_dimensions()
        .map_err(|error| format!("image dimensions: {error}"))?;
    if width == 0
        || height == 0
        || width > MAX_CALIBRATED_IMAGE_EDGE
        || height > MAX_CALIBRATED_IMAGE_EDGE
        || u64::from(width) * u64::from(height) > MAX_CALIBRATED_IMAGE_PIXELS
    {
        return Err("calibrated trace image exceeds 1024-pixel edge or 262144 pixels".into());
    }
    calibration.validate(height)?;

    let mut options = img2bez::TraceOptions::for_profile(img2bez::Profile::Wild);
    options.verbose = false;
    options.invert = invert;
    options.em_height = TRACE_EM_HEIGHT;
    let outline = img2bez::trace(image_bytes, &options)
        .map_err(|error| format!("img2bez trace failed: {error}"))?;
    let factor = calibration.font_units_per_pixel * f64::from(height) / TRACE_EM_HEIGHT;
    let vertical_offset = calibration.font_baseline_y
        + calibration.font_units_per_pixel * (calibration.pixel_baseline_y - f64::from(height));
    let placed = outline
        .scaled(factor)
        .translated(calibration.font_x_at_left, vertical_offset);
    let contours = placed
        .contours
        .into_iter()
        .map(|contour| DrawingContour {
            points: contour
                .points
                .into_iter()
                .map(|point| DrawingPoint {
                    x: point.x,
                    y: point.y,
                    kind: match point.kind {
                        PointKind::Move => DrawingPointType::Move,
                        PointKind::Line => DrawingPointType::Line,
                        PointKind::Curve => DrawingPointType::Curve,
                        PointKind::QCurve => DrawingPointType::Qcurve,
                        PointKind::OffCurve => DrawingPointType::Offcurve,
                    },
                    smooth: point.smooth,
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let generated = contours.iter().map(Into::into).collect::<Vec<_>>();
    crate::font::generated::validate_contours(&generated)
        .map_err(|error| format!("calibrated trace geometry: {error}"))?;
    Ok(CalibratedTrace {
        image_sha256: format!("sha256:{:x}", Sha256::digest(image_bytes)),
        image_width_px: width,
        image_height_px: height,
        calibration,
        invert,
        tracer: "img2bez/wild".into(),
        cargo_lock_sha256: format!(
            "sha256:{:x}",
            Sha256::digest(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/Cargo.lock"
            )))
        ),
        contours,
    })
}

/// Where the traced outline lands in the em.
#[derive(Clone, Copy, Debug)]
pub struct TraceConfig {
    /// Height of the band the outline is fitted into, in font units
    /// (normally ascender − descender).
    pub target_height: f64,
    /// Bottom of that band (normally the descender, negative).
    pub y_offset: f64,
    /// Advance width for the traced glyph.
    pub advance: f64,
    /// x of the leftmost ink (the trace's LSB).
    pub lsb: f64,
    /// Trace dark ink on light ground (`false`) or inverted (`true`).
    pub invert: bool,
}

impl Default for TraceConfig {
    fn default() -> Self {
        // The web host's defaults (image_trace.rs there).
        Self {
            target_height: 1088.0,
            y_offset: -256.0,
            advance: 600.0,
            lsb: 64.0,
            invert: false,
        }
    }
}

/// Trace an image into a glyph outline.
///
/// Uses img2bez's `wild` profile, which auto-detects clean renders
/// vs soft scans, with library defaults. This is what the web
/// editor's Autotrace runs.
pub fn trace_image(
    image_bytes: &[u8],
    config: &TraceConfig,
) -> Result<crate::font::ImportedContours, String> {
    if image_bytes.is_empty() {
        return Err("image bytes are empty".to_string());
    }
    let mut opts = img2bez::TraceOptions::for_profile(img2bez::Profile::Wild);
    opts.verbose = false;
    opts.em_height = config.target_height.max(1.0);
    opts.invert = config.invert;

    let mut metrics =
        img2bez::FontMetrics::from_target_height(config.target_height.max(1.0), config.y_offset);
    metrics.advance_width = Some(config.advance.max(1.0));
    metrics.lsb = config.lsb;

    let glyph = img2bez::trace_glyph(image_bytes, "traced", &[], &opts, &metrics)
        .map_err(|e| format!("img2bez trace failed: {e}"))?;
    let glyph = norad::Glyph::parse_raw(glyph.to_glif().as_bytes())
        .map_err(|e| format!("parse traced glif: {e}"))?;
    crate::formats::ufo::decode_contours(&glyph.contours)
}

#[cfg(test)]
mod tests {
    use super::*;
    use img2bez::image;

    fn square_at(width: u32, height: u32, left: u32, top: u32, size: u32) -> Vec<u8> {
        let mut img = image::GrayImage::from_pixel(width, height, image::Luma([255_u8]));
        for y in top..top + size {
            for x in left..left + size {
                img.put_pixel(x, y, image::Luma([0_u8]));
            }
        }
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .expect("encode png");
        out
    }

    fn point_bounds(contours: &[DrawingContour]) -> (f64, f64, f64, f64) {
        let points = contours.iter().flat_map(|contour| &contour.points);
        points.fold(
            (
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ),
            |(min_x, min_y, max_x, max_y), point| {
                (
                    min_x.min(point.x),
                    min_y.min(point.y),
                    max_x.max(point.x),
                    max_y.max(point.y),
                )
            },
        )
    }

    /// A tiny black square on white, as an uncompressed 8x8 PNG made
    /// by the image crate img2bez already links.
    fn square_png() -> Vec<u8> {
        let mut img = image::GrayImage::from_pixel(8, 8, image::Luma([255_u8]));
        for y in 2..6 {
            for x in 2..6 {
                img.put_pixel(x, y, image::Luma([0_u8]));
            }
        }
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .expect("encode png");
        out
    }

    #[test]
    fn traces_a_square_into_contours() {
        let glyph = trace_image(&square_png(), &TraceConfig::default()).expect("trace succeeds");
        assert!(!glyph.is_empty());
    }

    #[test]
    fn empty_bytes_error() {
        assert!(trace_image(&[], &TraceConfig::default()).is_err());
    }

    #[test]
    fn calibrated_trace_preserves_padding_scale_translation_and_y_direction() {
        let calibration = TraceCalibration {
            font_units_per_pixel: 2.0,
            pixel_baseline_y: 80.0,
            font_x_at_left: 100.0,
            font_baseline_y: 0.0,
        };
        let first =
            trace_image_calibrated(&square_at(96, 96, 20, 18, 40), calibration, false).unwrap();
        let padded =
            trace_image_calibrated(&square_at(128, 128, 36, 34, 40), calibration, false).unwrap();
        assert_eq!((first.image_width_px, first.image_height_px), (96, 96));
        assert_eq!((padded.image_width_px, padded.image_height_px), (128, 128));
        assert!(first.image_sha256.starts_with("sha256:"));
        assert!(first.cargo_lock_sha256.starts_with("sha256:"));
        assert_eq!(first.tracer, "img2bez/wild");
        assert_ne!(first.image_sha256, padded.image_sha256);
        let (x0, y0, x1, y1) = point_bounds(&first.contours);
        let (px0, py0, px1, py1) = point_bounds(&padded.contours);
        assert!((x0 - 140.0).abs() < 6.0);
        assert!((y0 - 44.0).abs() < 6.0);
        assert!((x1 - 220.0).abs() < 6.0);
        assert!((y1 - 124.0).abs() < 6.0);
        assert!((px0 - x0 - 32.0).abs() < 6.0);
        assert!((px1 - x1 - 32.0).abs() < 6.0);
        assert!((py0 - y0 + 32.0).abs() < 6.0);
        assert!((py1 - y1 + 32.0).abs() < 6.0);
        assert!(((x1 - x0) - (px1 - px0)).abs() < 6.0);
        assert!(((y1 - y0) - (py1 - py0)).abs() < 6.0);
    }

    #[test]
    fn calibrated_trace_rejects_bad_images_and_placements() {
        let image = square_at(96, 96, 20, 18, 40);
        let calibration = TraceCalibration {
            font_units_per_pixel: 2.0,
            pixel_baseline_y: 80.0,
            font_x_at_left: 100.0,
            font_baseline_y: 0.0,
        };
        assert!(trace_image_calibrated(&[], calibration, false).is_err());
        assert!(trace_image_calibrated(b"not an image", calibration, false).is_err());
        assert!(
            trace_image_calibrated(&vec![0; MAX_CALIBRATED_IMAGE_BYTES + 1], calibration, false)
                .is_err()
        );
        assert!(
            trace_image_calibrated(&square_at(513, 513, 20, 18, 40), calibration, false).is_err()
        );
        assert!(
            trace_image_calibrated(
                &image,
                TraceCalibration {
                    font_units_per_pixel: 0.0,
                    ..calibration
                },
                false
            )
            .is_err()
        );
        assert!(
            trace_image_calibrated(
                &image,
                TraceCalibration {
                    font_units_per_pixel: f64::NAN,
                    ..calibration
                },
                false
            )
            .is_err()
        );
        assert!(
            trace_image_calibrated(
                &image,
                TraceCalibration {
                    pixel_baseline_y: 100.0,
                    ..calibration
                },
                false
            )
            .is_err()
        );
        assert!(
            trace_image_calibrated(
                &image,
                TraceCalibration {
                    font_x_at_left: f64::INFINITY,
                    ..calibration
                },
                false
            )
            .is_err()
        );
    }

    #[test]
    fn calibrated_trace_rejects_too_many_generated_contours() {
        let mut image = image::GrayImage::from_pixel(512, 512, image::Luma([255_u8]));
        for row in 0..17 {
            for column in 0..17 {
                for y in row * 28..row * 28 + 12 {
                    for x in column * 28..column * 28 + 12 {
                        image.put_pixel(x, y, image::Luma([0_u8]));
                    }
                }
            }
        }
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let error = trace_image_calibrated(
            &bytes,
            TraceCalibration {
                font_units_per_pixel: 1.0,
                pixel_baseline_y: 256.0,
                font_x_at_left: 0.0,
                font_baseline_y: 0.0,
            },
            false,
        )
        .unwrap_err();
        assert!(error.contains("contour count"), "{error}");
    }
}

/// Which kind of source image a trace is tuned for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TraceProfile {
    /// An unknown raster; a looser fit that detects clean renders and soft scans itself.
    #[default]
    Wild,
    /// A clean, high-resolution source; a tighter fit.
    Clean,
    /// A soft scan or photograph; blurs away the texture of the edge first.
    Photo,
}

impl TraceProfile {
    /// The next profile, in the order a control cycles through them.
    pub fn next(self) -> Self {
        match self {
            Self::Wild => Self::Clean,
            Self::Clean => Self::Photo,
            Self::Photo => Self::Wild,
        }
    }

    /// The profile's short display name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Wild => "Wild",
            Self::Clean => "Clean",
            Self::Photo => "Photo",
        }
    }
}

/// How to read the ink out of a placed image.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlacedTraceOptions {
    /// The kind of source image.
    pub profile: TraceProfile,
    /// Trace light pixels as ink.
    pub invert: bool,
    /// Brightness that separates ink from ground, 0 to 255; automatic when `None`.
    pub threshold: Option<u8>,
}

/// The largest image a placed trace accepts, in pixels.
const MAX_PLACED_IMAGE_PIXELS: u64 = 64_000_000;

/// Trace a whole image where it is placed on the canvas.
///
/// `placement` is a layer image's transform: it maps image space, one unit per pixel with the
/// origin at the lower left corner and y up, to font units. The outline lands on the ink of the
/// image as it is shown, at any size; nothing is fitted into a glyph box. This is how a word or
/// a line of calligraphy is traced. The placement must scale both axes equally and not rotate.
pub fn trace_image_placed(
    image_bytes: &[u8],
    placement: kurbo::Affine,
    options: PlacedTraceOptions,
) -> Result<crate::font::ImportedContours, String> {
    crate::formats::ufo::decode_contours(&placed_contours(image_bytes, placement, options)?)
}

fn placed_contours(
    image_bytes: &[u8],
    placement: kurbo::Affine,
    options: PlacedTraceOptions,
) -> Result<Vec<norad::Contour>, String> {
    use img2bez::PointKind;
    use img2bez::image::{ImageReader, guess_format};

    if image_bytes.is_empty() {
        return Err("image bytes are empty".into());
    }
    let [scale, skew_y, skew_x, scale_y, dx, dy] = placement.as_coeffs();
    if !placement.as_coeffs().iter().all(|value| value.is_finite())
        || skew_x.abs() > 1e-9
        || skew_y.abs() > 1e-9
        || scale <= 0.0
        || (scale - scale_y).abs() > 1e-6 * scale
    {
        return Err("a placed trace needs an image that is scaled evenly and not rotated".into());
    }
    let format = guess_format(image_bytes).map_err(|error| format!("image format: {error}"))?;
    let (width, height) = ImageReader::with_format(std::io::Cursor::new(image_bytes), format)
        .into_dimensions()
        .map_err(|error| format!("image dimensions: {error}"))?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PLACED_IMAGE_PIXELS {
        return Err("a placed trace needs an image of 1 to 64 million pixels".into());
    }

    let mut trace = img2bez::TraceOptions::for_profile(match options.profile {
        TraceProfile::Wild => img2bez::Profile::Wild,
        TraceProfile::Clean => img2bez::Profile::Clean,
        TraceProfile::Photo => img2bez::Profile::Photo,
    });
    trace.verbose = false;
    trace.invert = options.invert;
    if let Some(threshold) = options.threshold {
        trace.threshold = img2bez::ThresholdMethod::Fixed(threshold);
    }
    // The tracer works in font units with the image as tall as it is placed, so its fitting
    // tolerances mean the same thing at any image size.
    trace.em_height = f64::from(height) * scale;
    let outline = img2bez::trace(image_bytes, &trace)
        .map_err(|error| format!("img2bez trace failed: {error}"))?
        .translated(dx, dy);
    let contours: Vec<norad::Contour> = outline
        .contours
        .into_iter()
        .map(|contour| {
            norad::Contour::new(
                contour
                    .points
                    .into_iter()
                    .map(|point| {
                        norad::ContourPoint::new(
                            point.x,
                            point.y,
                            match point.kind {
                                PointKind::Move => norad::PointType::Move,
                                PointKind::Line => norad::PointType::Line,
                                PointKind::Curve => norad::PointType::Curve,
                                PointKind::QCurve => norad::PointType::QCurve,
                                PointKind::OffCurve => norad::PointType::OffCurve,
                            },
                            point.smooth,
                            None,
                            None,
                        )
                    })
                    .collect(),
                None,
            )
        })
        .collect();
    Ok(contours)
}

#[cfg(test)]
mod placed_tests {
    use super::*;
    use img2bez::image;

    /// A white PNG with one black rectangle, x 40..120 and y 20..60 from the top left.
    fn block_png() -> Vec<u8> {
        let mut image = image::GrayImage::from_pixel(200, 100, image::Luma([255]));
        for y in 20..60 {
            for x in 40..120 {
                image.put_pixel(x, y, image::Luma([0]));
            }
        }
        let mut bytes = Vec::new();
        image::DynamicImage::ImageLuma8(image)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn a_placed_trace_lands_on_the_ink_where_the_image_is_shown() {
        // 10 font units per pixel, lower left corner of the image at (500, -300)
        let placement = kurbo::Affine::new([10.0, 0.0, 0.0, 10.0, 500.0, -300.0]);
        let options = PlacedTraceOptions {
            profile: TraceProfile::Clean,
            ..Default::default()
        };
        let contours = placed_contours(&block_png(), placement, options).unwrap();
        assert_eq!(contours.len(), 1);
        assert_eq!(
            trace_image_placed(&block_png(), placement, options)
                .unwrap()
                .len(),
            1
        );
        // the block spans x 40..120 px and, from the bottom, y 40..80 px
        let on_curve = contours[0]
            .points
            .iter()
            .filter(|point| point.typ != norad::PointType::OffCurve);
        let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for point in on_curve {
            (x0, x1) = (x0.min(point.x), x1.max(point.x));
            (y0, y1) = (y0.min(point.y), y1.max(point.y));
        }
        let near = |a: f64, b: f64| (a - b).abs() <= 12.0;
        assert!(near(x0, 900.0) && near(x1, 1700.0), "x {x0}..{x1}");
        assert!(near(y0, 100.0) && near(y1, 500.0), "y {y0}..{y1}");
    }

    #[test]
    fn a_rotated_or_uneven_placement_is_refused() {
        let uneven = kurbo::Affine::new([10.0, 0.0, 0.0, 12.0, 0.0, 0.0]);
        assert!(trace_image_placed(&block_png(), uneven, PlacedTraceOptions::default()).is_err());
        let turned = kurbo::Affine::rotate(0.3);
        assert!(trace_image_placed(&block_png(), turned, PlacedTraceOptions::default()).is_err());
    }
}
