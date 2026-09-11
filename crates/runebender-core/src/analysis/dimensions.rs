// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Stem and bar widths read off a glyph: the numbers a Dimensions
//! panel shows for H, O, n, o, t and v.
//!
//! A stem is the narrowest horizontal span through ink; a bar the
//! narrowest vertical one. Both come from the measurement engine's
//! spans, kept to the ones whose midpoint is inside the filled
//! outline, so a gap between two strokes does not read as a stem.

use kurbo::Shape as _;

use crate::analysis::measure::{self, MeasureKind};
use crate::outline::glyph_paths;
use crate::outline::path::Path;
use crate::outline::path::hyper_model::Contour as WContour;

/// The glyphs a Dimensions panel reads, in the order it lists them.
pub const REFERENCE_GLYPHS: &[&str] = &["H", "O", "n", "o", "t", "v"];

/// The narrowest stem and the narrowest bar of a glyph, in font
/// units, rounded. `None` for either when the glyph has no contour or
/// no span of that kind through ink.
pub fn stem_and_bar(font: &norad::Font, name: &str) -> (Option<i64>, Option<i64>) {
    let Some(glyph) = font.get_glyph(name) else {
        return (None, None);
    };
    if glyph.contours.is_empty() {
        return (None, None);
    }
    let paths: Vec<Path> = glyph
        .contours
        .iter()
        .map(|c| Path::from_contour(&WContour::from_norad(c)))
        .collect();
    let filled = glyph_paths::glyph_to_bezpath(glyph, font);
    let black = |m: &measure::Measurement| {
        let mid = kurbo::Point::new((m.a.x + m.b.x) / 2.0, (m.a.y + m.b.y) / 2.0);
        filled.contains(mid)
    };
    let measurements = measure::glyph_measurements(&paths);
    let narrowest = |kind: MeasureKind| {
        measurements
            .iter()
            .filter(|m| m.kind == kind)
            .filter(|m| black(m))
            .map(|m| m.length)
            .min()
    };
    (
        narrowest(MeasureKind::Horizontal),
        narrowest(MeasureKind::Vertical),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(name: &str, w: f64, h: f64) -> norad::Glyph {
        let mut glyph = norad::Glyph::new(name);
        glyph.width = w + 100.0;
        let mut contour = norad::Contour::default();
        for (x, y) in [(50.0, 0.0), (50.0 + w, 0.0), (50.0 + w, h), (50.0, h)] {
            contour.points.push(norad::ContourPoint::new(
                x,
                y,
                norad::PointType::Line,
                false,
                None,
                None,
            ));
        }
        glyph.contours.push(contour);
        glyph
    }

    #[test]
    fn a_rectangle_reads_its_width_as_the_stem_and_its_height_as_the_bar() {
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(rect("I", 96.0, 700.0));
        let (stem, bar) = stem_and_bar(&font, "I");
        assert_eq!(stem, Some(96));
        assert_eq!(bar, Some(700));
    }

    #[test]
    fn a_missing_or_empty_glyph_has_no_dimensions() {
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("space"));
        assert_eq!(stem_and_bar(&font, "space"), (None, None));
        assert_eq!(stem_and_bar(&font, "nothere"), (None, None));
    }
}
