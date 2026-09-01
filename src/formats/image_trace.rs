// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Image tracing through img2bez, the deterministic autotracer the
//! web editor uses.
//!
//! Same crate, same defaults, so a trace here is byte-identical to
//! the web editor, the CLI, and the blog demo. This adapter takes
//! plain arguments, where the web one takes host JSON, and returns
//! a parsed norad glyph ready to merge into the font.

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
pub fn trace_image(image_bytes: &[u8], config: &TraceConfig) -> Result<norad::Glyph, String> {
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
    norad::Glyph::parse_raw(glyph.to_glif().as_bytes())
        .map_err(|e| format!("parse traced glif: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use img2bez::image;

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
        assert!(!glyph.contours.is_empty());
    }

    #[test]
    fn empty_bytes_error() {
        assert!(trace_image(&[], &TraceConfig::default()).is_err());
    }
}
