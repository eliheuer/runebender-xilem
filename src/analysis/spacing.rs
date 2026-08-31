// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Sidebearings that sit off the grid the family is drawn on.
//!
//! Spacing in a systematic family is not a free number. It is drawn
//! from a small set of values: in Virtua Grotesk, 616 sidebearings use
//! 53 distinct values, 80% of them multiples of 8, and the commonest
//! twelve cover 82%.
//!
//! That is worth knowing because it says what tool this needs. A value
//! drawn from a ladder is not something to predict; it is something to
//! check. A sidebearing off the ladder is either a decision worth
//! recording or a slip, and only the designer can say which.
//!
//! The step is inferred from the font rather than assumed, so a family
//! drawn on a different grid is checked against its own.

use norad::Font;

/// One glyph's sidebearings.
#[derive(Clone, Debug, PartialEq)]
pub struct Sides {
    /// Name of the glyph.
    pub glyph: String,
    /// Left sidebearing in font units.
    pub left: f64,
    /// Right sidebearing in font units.
    pub right: f64,
}

/// A sidebearing that is not a multiple of the family's step.
#[derive(Clone, Debug, PartialEq)]
pub struct OffGrid {
    /// Name of the glyph.
    pub glyph: String,
    /// Which side, for the report: "left" or "right".
    pub side: &'static str,
    /// The sidebearing value in font units.
    pub value: f64,
    /// How far to the nearest multiple of the step.
    pub off_by: f64,
}

/// The sidebearings of every drawn, non-composite glyph with a width.
///
/// Composites are skipped: their spacing follows the base they are
/// built from, so reporting them is reporting the same fact twice.
pub fn sidebearings(font: &Font) -> Vec<Sides> {
    let mut out = Vec::new();
    for glyph in font.default_layer().iter() {
        if !glyph.components.is_empty() || glyph.contours.is_empty() || glyph.width <= 0.0 {
            continue;
        }
        let xs: Vec<f64> = glyph
            .contours
            .iter()
            .flat_map(|c| c.points.iter().map(|p| p.x))
            .collect();
        let (Some(min), Some(max)) = (
            xs.iter().cloned().reduce(f64::min),
            xs.iter().cloned().reduce(f64::max),
        ) else {
            continue;
        };
        out.push(Sides {
            glyph: glyph.name().to_string(),
            left: min,
            right: glyph.width - max,
        });
    }
    out
}

/// The largest step most of the family's spacing is a multiple of.
///
/// Tried from coarse to fine, taking the first that covers most of the
/// values, so a family drawn on 16s is not reported as being on 2s.
/// `None` when nothing fits, which is the honest answer for spacing
/// that was not drawn to a grid at all.
pub fn infer_step(sides: &[Sides]) -> Option<f64> {
    // No 1: every whole number is a multiple of it, so it would
    // always match and never mean anything.
    const CANDIDATES: [f64; 4] = [16.0, 8.0, 4.0, 2.0];
    // Virtua Grotesk, a family drawn to a grid, has 80% of its
    // sidebearings on 8s and only 55% on 16s. The threshold sits
    // between those so the coarser step is not claimed on the
    // strength of the half of the values that happen to fit it.
    const COVERAGE: f64 = 0.75;
    let values: Vec<f64> = sides.iter().flat_map(|s| [s.left, s.right]).collect();
    if values.len() < 8 {
        return None;
    }
    for step in CANDIDATES {
        let on = values
            .iter()
            .filter(|v| remainder(**v, step).abs() < EPSILON)
            .count();
        if on as f64 / values.len() as f64 >= COVERAGE {
            return Some(step);
        }
    }
    None
}

/// Tolerance for treating a value as on the grid.
///
/// Coordinates come from a file as decimals, so a value written as
/// 32 can arrive a hair off it. Anything inside this is on the
/// grid.
const EPSILON: f64 = 0.01;

/// Signed distance from `value` to the nearest multiple of `step`.
fn remainder(value: f64, step: f64) -> f64 {
    value - (value / step).round() * step
}

/// Sidebearings that are not multiples of `step`, worst first.
pub fn off_grid(sides: &[Sides], step: f64) -> Vec<OffGrid> {
    if step <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for s in sides {
        for (side, value) in [("left", s.left), ("right", s.right)] {
            let off = remainder(value, step);
            if off.abs() >= EPSILON {
                out.push(OffGrid {
                    glyph: s.glyph.clone(),
                    side,
                    value,
                    off_by: off,
                });
            }
        }
    }
    out.sort_by(|a, b| {
        b.off_by
            .abs()
            .partial_cmp(&a.off_by.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use norad::{Contour, ContourPoint, Glyph, PointType};

    fn glyph(name: &str, left: f64, ink: f64, right: f64) -> Glyph {
        let mut g = Glyph::new(name);
        g.width = left + ink + right;
        let pt = |x: f64, y: f64| ContourPoint::new(x, y, PointType::Line, false, None, None);
        g.contours.push(Contour::new(
            vec![
                pt(left, 0.0),
                pt(left + ink, 0.0),
                pt(left + ink, 500.0),
                pt(left, 500.0),
            ],
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
    fn sidebearings_come_from_the_ink() {
        let font = font_with(vec![glyph("a", 32.0, 200.0, 64.0)]);
        let s = sidebearings(&font);
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].left, s[0].right), (32.0, 64.0));
    }

    #[test]
    fn a_family_on_eights_is_read_as_eights() {
        // Odd multiples of 8, so nothing here is also a multiple of
        // 16 and the coarser step cannot claim the family.
        let font = font_with(
            (0..6)
                .map(|i| glyph(&format!("g{i}"), 8.0, 200.0, 8.0 + 16.0 * i as f64))
                .collect(),
        );
        assert_eq!(infer_step(&sidebearings(&font)), Some(8.0));
    }

    /// A family drawn on 16s should be reported as 16s, not as 8s or
    /// 2s, which every multiple of 16 also satisfies.
    #[test]
    fn the_coarsest_step_that_fits_wins() {
        let font = font_with(
            (0..6)
                .map(|i| glyph(&format!("g{i}"), 16.0, 200.0, 16.0 * (i as f64 + 1.0)))
                .collect(),
        );
        assert_eq!(infer_step(&sidebearings(&font)), Some(16.0));
    }

    #[test]
    fn the_odd_one_out_is_reported() {
        let mut glyphs: Vec<Glyph> = (0..8)
            .map(|i| glyph(&format!("g{i}"), 32.0, 200.0, 64.0))
            .collect();
        glyphs.push(glyph("drifted", 32.0, 200.0, 61.0));
        let font = font_with(glyphs);
        let sides = sidebearings(&font);
        let step = infer_step(&sides).expect("a step");
        let found = off_grid(&sides, step);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].glyph, "drifted");
        assert_eq!(found[0].side, "right");
        assert_eq!(found[0].off_by, -3.0);
    }

    /// A composite's spacing follows its base, so reporting it says
    /// the same thing twice and points at the wrong glyph.
    #[test]
    fn composites_are_left_out() {
        let mut acute = Glyph::new("aacute");
        acute.width = 300.0;
        acute.components.push(norad::Component::new(
            norad::Name::new("a").unwrap(),
            Default::default(),
            None,
        ));
        let font = font_with(vec![glyph("a", 32.0, 200.0, 64.0), acute]);
        let names: Vec<_> = sidebearings(&font).into_iter().map(|s| s.glyph).collect();
        assert_eq!(names, vec!["a"]);
    }

    /// A coordinate that arrives a hair off a round number is on the
    /// grid. Without a tolerance a glyph spaced at exactly 32 gets
    /// reported as "off by +0", which reads as a bug in the check.
    #[test]
    fn float_noise_is_not_a_finding() {
        let mut glyphs: Vec<Glyph> = (0..8)
            .map(|i| glyph(&format!("g{i}"), 32.0, 200.0, 64.0))
            .collect();
        glyphs.push(glyph("hair", 32.0 + 1e-9, 200.0, 64.0));
        let font = font_with(glyphs);
        let sides = sidebearings(&font);
        assert!(off_grid(&sides, 8.0).is_empty());
    }

    #[test]
    fn spacing_with_no_grid_infers_nothing() {
        let font = font_with(
            (0..10)
                .map(|i| {
                    glyph(
                        &format!("g{i}"),
                        31.0 + i as f64,
                        200.0,
                        17.0 + i as f64 * 3.0,
                    )
                })
                .collect(),
        );
        assert_eq!(infer_step(&sidebearings(&font)), None);
    }
}
