// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Optical weight: which glyphs read darker or lighter than the rest.
//!
//! The thing a designer catches in a print proof, where one letter
//! sits heavier than its neighbours, is measurable. It is the ink a
//! glyph puts down divided by the space it occupies, compared against
//! what the other glyphs of its kind do.
//!
//! This is measurement, not prediction, which matters for how far to
//! trust it. The number is exact and reproducible. What it cannot know
//! is intent: a glyph is allowed to be darker if that is the drawing.
//! It narrows a proof to a shortlist, and the eye still decides.
//!
//! Ink is counted only inside a horizontal band, from the baseline to
//! a reference height. That is what makes an accented glyph
//! comparable to a plain one: the accent sits above the band and does
//! not count, and neither does a descender below it. Measuring the
//! whole outline instead reports every composite as darker, because
//! its extra ink is divided by the same box.
//!
//! Inside the band the area is sampled on a grid rather than
//! integrated, because the band cuts the outline and the cut shape has
//! no closed form. The grid is fixed, so two glyphs are always
//! compared at the same resolution.

use kurbo::Shape as _;
use norad::{Font, Glyph};

use crate::{glyph_ops, glyph_paths};

/// One glyph's ink, and how much of its box that fills.
#[derive(Clone, Debug, PartialEq)]
pub struct Density {
    pub glyph: String,
    /// Outline area in square units, components resolved.
    pub ink: f64,
    /// Advance width times the reference height.
    pub box_area: f64,
    /// `ink / box_area`. Comparable between glyphs of the same kind.
    pub density: f64,
}

/// A glyph whose density is out of step with its group.
#[derive(Clone, Debug, PartialEq)]
pub struct Outlier {
    pub glyph: String,
    pub density: f64,
    /// The median density of the group it was compared against.
    pub median: f64,
    /// `density / median`. Above 1 reads darker, below 1 lighter.
    pub ratio: f64,
    /// Which group it was compared against, for the report.
    pub group: String,
}

/// Ink area and density for one glyph.
///
/// Returns `None` for a glyph with no outline, or a zero-width one:
/// a space has no density, and dividing by its box would invent one.
pub fn density(font: &Font, glyph: &Glyph, reference_height: f64) -> Option<Density> {
    if reference_height <= 0.0 || glyph.width <= 0.0 {
        return None;
    }
    // Components count: an accented letter's ink is the base plus the
    // mark, and comparing only its own contours would call every
    // composite empty.
    let mut resolved = glyph.clone();
    if !glyph.components.is_empty() {
        resolved.contours = glyph_ops::resolved_component_contours(font, glyph);
        resolved.components.clear();
    }
    if resolved.contours.is_empty() {
        return None;
    }
    let path = glyph_paths::contours_to_bezpath(&resolved);
    let ink = band_area(&path, glyph.width, reference_height);
    if ink <= 0.0 {
        return None;
    }
    let box_area = glyph.width * reference_height;
    Some(Density {
        glyph: glyph.name().to_string(),
        ink,
        box_area,
        density: ink / box_area,
    })
}

/// Area of the outline inside `0..width` by `0..height`, sampled.
///
/// Sample centres, not corners, so a shape narrower than one cell is
/// still either in or out rather than always missed.
fn band_area(path: &kurbo::BezPath, width: f64, height: f64) -> f64 {
    const COLS: usize = 64;
    const ROWS: usize = 64;
    let (dx, dy) = (width / COLS as f64, height / ROWS as f64);
    let mut inside = 0usize;
    for row in 0..ROWS {
        let y = (row as f64 + 0.5) * dy;
        for col in 0..COLS {
            let x = (col as f64 + 0.5) * dx;
            if path.contains(kurbo::Point::new(x, y)) {
                inside += 1;
            }
        }
    }
    inside as f64 * dx * dy
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// Which of `names` read darker or lighter than the group's median.
///
/// The median rather than the mean, because a proof usually has a few
/// genuine outliers and they would drag a mean toward themselves and
/// hide the rest.
///
/// `tolerance` is a fraction: 0.15 reports anything more than 15% off.
pub fn outliers(
    font: &Font,
    names: &[String],
    reference_height: f64,
    tolerance: f64,
    group: &str,
) -> Vec<Outlier> {
    let layer = font.default_layer();
    let measured: Vec<Density> = names
        .iter()
        .filter_map(|n| layer.get_glyph(n.as_str()))
        .filter_map(|g| density(font, g, reference_height))
        .collect();
    // Three glyphs cannot establish a norm worth comparing against.
    if measured.len() < 4 {
        return Vec::new();
    }
    let mut values: Vec<f64> = measured.iter().map(|d| d.density).collect();
    let mid = median(&mut values);
    if mid <= 0.0 {
        return Vec::new();
    }
    let mut found: Vec<Outlier> = measured
        .into_iter()
        .filter_map(|d| {
            let ratio = d.density / mid;
            ((ratio - 1.0).abs() > tolerance).then(|| Outlier {
                glyph: d.glyph,
                density: d.density,
                median: mid,
                ratio,
                group: group.to_string(),
            })
        })
        .collect();
    // Worst first: that is the order a proof gets reviewed in.
    found.sort_by(|a, b| {
        (b.ratio - 1.0)
            .abs()
            .partial_cmp(&(a.ratio - 1.0).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use norad::{Contour, ContourPoint, PointType};

    fn box_glyph(name: &str, w: f64, h: f64, advance: f64) -> Glyph {
        let mut g = Glyph::new(name);
        g.width = advance;
        let pt = |x: f64, y: f64| ContourPoint::new(x, y, PointType::Line, false, None, None);
        g.contours.push(Contour::new(
            vec![pt(0.0, 0.0), pt(w, 0.0), pt(w, h), pt(0.0, h)],
            None,
        ));
        g
    }

    fn font_with(glyphs: Vec<Glyph>) -> Font {
        let mut font = Font::new();
        for g in glyphs {
            font.default_layer_mut().insert_glyph(g);
        }
        font
    }

    #[test]
    fn density_is_ink_over_the_box() {
        let g = box_glyph("a", 100.0, 500.0, 200.0);
        let font = font_with(vec![g.clone()]);
        let d = density(&font, &g, 500.0).expect("has ink");
        assert_eq!(d.box_area, 100_000.0);
        // Half the box is inked; sampling puts it within a cell.
        assert!((d.density - 0.5).abs() < 0.02, "density {}", d.density);
    }

    /// The reason for the band: ink above the reference height is not
    /// counted, so an accent does not make a glyph read darker.
    #[test]
    fn ink_above_the_band_does_not_count() {
        let plain = box_glyph("n", 100.0, 500.0, 200.0);
        let mut accented = box_glyph("nacute", 100.0, 500.0, 200.0);
        let pt = |x: f64, y: f64| ContourPoint::new(x, y, PointType::Line, false, None, None);
        accented.contours.push(Contour::new(
            vec![pt(0.0, 600.0), pt(100.0, 600.0), pt(100.0, 700.0), pt(0.0, 700.0)],
            None,
        ));
        let font = font_with(vec![plain.clone(), accented.clone()]);
        let a = density(&font, &plain, 500.0).expect("ink");
        let b = density(&font, &accented, 500.0).expect("ink");
        assert!((a.density - b.density).abs() < 1e-9, "{} vs {}", a.density, b.density);
    }

    #[test]
    fn a_blank_glyph_has_no_density() {
        let mut space = Glyph::new("space");
        space.width = 200.0;
        let font = font_with(vec![space.clone()]);
        assert!(density(&font, &space, 500.0).is_none());
    }

    #[test]
    fn a_zero_width_glyph_has_no_density() {
        let g = box_glyph("mark", 100.0, 500.0, 0.0);
        let font = font_with(vec![g.clone()]);
        assert!(density(&font, &g, 500.0).is_none());
    }

    /// The point of the whole module: one heavier glyph among even
    /// ones is reported, and the even ones are not.
    #[test]
    fn the_odd_one_out_is_found() {
        let mut glyphs: Vec<Glyph> = "abcdef"
            .chars()
            .map(|c| box_glyph(&c.to_string(), 100.0, 500.0, 200.0))
            .collect();
        glyphs.push(box_glyph("heavy", 160.0, 500.0, 200.0));
        let names: Vec<String> = glyphs.iter().map(|g| g.name().to_string()).collect();
        let font = font_with(glyphs);
        let found = outliers(&font, &names, 500.0, 0.15, "lowercase");
        assert_eq!(found.len(), 1, "only the heavy one is off: {found:?}");
        assert_eq!(found[0].glyph, "heavy");
        assert!(found[0].ratio > 1.0, "it reads darker, not lighter");
    }

    /// A median, not a mean: two heavy glyphs must not drag the norm
    /// far enough to excuse themselves.
    #[test]
    fn outliers_do_not_move_the_norm() {
        let mut glyphs: Vec<Glyph> = "abcdefgh"
            .chars()
            .map(|c| box_glyph(&c.to_string(), 100.0, 500.0, 200.0))
            .collect();
        glyphs.push(box_glyph("h1", 200.0, 500.0, 200.0));
        glyphs.push(box_glyph("h2", 200.0, 500.0, 200.0));
        let names: Vec<String> = glyphs.iter().map(|g| g.name().to_string()).collect();
        let font = font_with(glyphs);
        let found = outliers(&font, &names, 500.0, 0.15, "lowercase");
        let flagged: Vec<&str> = found.iter().map(|o| o.glyph.as_str()).collect();
        assert!(flagged.contains(&"h1") && flagged.contains(&"h2"), "{flagged:?}");
        assert_eq!(found.len(), 2, "the even glyphs are not outliers");
    }

    #[test]
    fn too_few_glyphs_is_not_a_norm() {
        let glyphs: Vec<Glyph> = "abc"
            .chars()
            .map(|c| box_glyph(&c.to_string(), 100.0, 500.0, 200.0))
            .collect();
        let names: Vec<String> = glyphs.iter().map(|g| g.name().to_string()).collect();
        let font = font_with(glyphs);
        assert!(outliers(&font, &names, 500.0, 0.15, "lowercase").is_empty());
    }
}
