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
use crate::document::LayerView;
use crate::document::project::Project;
use crate::document::variable::SourceId;
use crate::outline::glyph_paths;
use crate::outline::path::Path;
use crate::outline::path::hyper_model::Contour as WContour;

/// The glyphs a Dimensions panel reads, in the order it lists them.
pub const REFERENCE_GLYPHS: &[&str] = &["H", "O", "n", "o", "t", "v"];

/// Read the narrowest stem and bar from a source's canonical default layer.
///
/// An unknown source, missing glyph, or glyph without direct contours has no dimensions.
pub fn stem_and_bar_project(
    project: &Project,
    source: SourceId,
    name: &str,
) -> (Option<i64>, Option<i64>) {
    let Some(source) = project.document_source(source) else {
        return (None, None);
    };
    let Some(layer) = project.document_layer(name, &source.default_layer()) else {
        return (None, None);
    };
    stem_and_bar_from_layer(layer)
}

/// Read the narrowest stem and bar directly from one canonical glyph layer.
///
/// Components are excluded, matching the established Dimensions-panel contract.
pub fn stem_and_bar_from_layer(layer: LayerView<'_>) -> (Option<i64>, Option<i64>) {
    let filled = glyph_paths::ordinary_layer_contours_to_bezpath(layer);
    if filled.is_empty() {
        return (None, None);
    }
    let black = |measurement: &measure::Measurement| {
        let midpoint = kurbo::Point::new(
            (measurement.a.x + measurement.b.x) / 2.0,
            (measurement.a.y + measurement.b.y) / 2.0,
        );
        filled.contains(midpoint)
    };
    let measurements = measure::ordinary_layer_measurements(layer);
    let narrowest = |kind: MeasureKind| {
        measurements
            .iter()
            .filter(|measurement| measurement.kind == kind)
            .filter(|measurement| black(measurement))
            .map(|measurement| measurement.length)
            .min()
    };
    (
        narrowest(MeasureKind::Horizontal),
        narrowest(MeasureKind::Vertical),
    )
}

/// The narrowest stem and the narrowest bar of a glyph, in font
/// units, rounded. `None` for either when the glyph has no contour or
/// no span of that kind through ink.
///
/// This UFO-boundary wrapper remains while application callers migrate to
/// [`stem_and_bar_from_layer`].
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
    use std::path::PathBuf;

    use crate::document::project::{Master, Project};
    use crate::document::variable::SourceId;

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

        let project = Project::from_source(Master::from_font(font, PathBuf::from("Test.ufo")));
        assert_eq!(
            stem_and_bar_project(&project, SourceId(0), "I"),
            (Some(96), Some(700))
        );
        let layer = project
            .document_source(SourceId(0))
            .and_then(|source| project.document_layer("I", &source.default_layer()))
            .unwrap();
        assert_eq!(stem_and_bar_from_layer(layer), (Some(96), Some(700)));
    }

    #[test]
    fn a_missing_or_empty_glyph_has_no_dimensions() {
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("space"));
        assert_eq!(stem_and_bar(&font, "space"), (None, None));
        assert_eq!(stem_and_bar(&font, "nothere"), (None, None));

        let project = Project::from_source(Master::from_font(font, PathBuf::from("Empty.ufo")));
        assert_eq!(
            stem_and_bar_project(&project, SourceId(0), "space"),
            (None, None)
        );
        assert_eq!(
            stem_and_bar_project(&project, SourceId(0), "nothere"),
            (None, None)
        );
        assert_eq!(
            stem_and_bar_project(&project, SourceId(99), "space"),
            (None, None)
        );
    }
}
