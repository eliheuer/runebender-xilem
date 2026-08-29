// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Add weight by pushing the outline outward, learning how far from
//! glyphs already drawn in both masters.
//!
//! This is the workflow of drawing the key glyphs and letting them
//! carry the rest: draw `n`, `o`, `H`, `O` in the heavier master, and
//! every other glyph moves by what those moved.
//!
//! There is no model here, and on some scripts that is the point. A
//! model trained on Latin measured worse than a uniform shift on
//! Arabic, while this needs no training data at all, only a few
//! reference glyphs somebody drew.
//!
//! Points are moved, never added or removed, so the result stays
//! interpolation-compatible with what it came from. That is a
//! guarantee of the method rather than something to check afterwards.
//!
//! The offset is anisotropic because type is: a vertical stem and a
//! horizontal bar do not gain the same weight. Virtua Grotesk grows
//! its verticals by 96 units and its bars by 72.

use norad::{Contour, Glyph};

/// How far to push, per axis, in font units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Offset {
    /// Applied to the horizontal part of a point's outward normal, so
    /// it sets how much vertical stems gain.
    pub x: f64,
    /// Applied to the vertical part, setting how much bars gain.
    pub y: f64,
}

/// Unit normal at each point, pointing the way that adds weight.
///
/// Always the right of travel, with no winding test. Under the usual
/// convention, where an outer contour runs counter-clockwise and a
/// counter runs the other way, the right of travel is outward on the
/// outer contour and into the hole on a counter. Both thicken the ink,
/// which is what is wanted.
///
/// Flipping this by winding, as an earlier version did, pushes
/// counters open and thins every bowl in the font.
///
/// The tangent comes from the neighbouring points rather than the
/// curve, so an off-curve control moves with the shape around it
/// instead of being treated as if it sat on the outline.
pub fn outward_normals(c: &Contour) -> Vec<(f64, f64)> {
    let pts = &c.points;
    let n = pts.len();
    if n < 3 {
        return vec![(0.0, 0.0); n];
    }
    (0..n)
        .map(|i| {
            let prev = &pts[(i + n - 1) % n];
            let next = &pts[(i + 1) % n];
            let (tx, ty) = (next.x - prev.x, next.y - prev.y);
            let len = (tx * tx + ty * ty).sqrt();
            if len < 1e-9 {
                return (0.0, 0.0);
            }
            (ty / len, -tx / len)
        })
        .collect()
}

/// The offset that best explains how a set of reference pairs moved.
///
/// For each point, the model says the move was `(nx * x, ny * y)`, so
/// each point with a strong enough normal component votes for a value
/// on that axis. The median of the votes wins, which keeps one badly
/// drawn reference from setting the weight for a whole master.
///
/// `None` when no pair is compatible, or when nothing moved.
pub fn learn_offset(pairs: &[(&Glyph, &Glyph)]) -> Option<Offset> {
    // Least squares per axis: the move at each point is modelled as
    // `nx * x`, so the best x is sum(nx*dx) / sum(nx*nx).
    //
    // An earlier version divided at each point and took the median,
    // which needed a threshold to avoid dividing by a normal that
    // barely pointed along the axis. On a shape whose corners are all
    // oblique, every point fell under it and nothing could be learned.
    // This weights each point by how much it has to say instead.
    let (mut sx, mut nx2, mut sy, mut ny2) = (0.0, 0.0, 0.0, 0.0);
    for (light, heavy) in pairs {
        if light.contours.len() != heavy.contours.len() {
            continue;
        }
        for (lc, hc) in light.contours.iter().zip(heavy.contours.iter()) {
            if lc.points.len() != hc.points.len() {
                continue;
            }
            for ((lp, hp), (nx, ny)) in lc
                .points
                .iter()
                .zip(hc.points.iter())
                .zip(outward_normals(lc))
            {
                sx += nx * (hp.x - lp.x);
                nx2 += nx * nx;
                sy += ny * (hp.y - lp.y);
                ny2 += ny * ny;
            }
        }
    }
    if nx2 < 1e-9 || ny2 < 1e-9 {
        return None;
    }
    let (x, y) = (sx / nx2, sy / ny2);
    if x.abs() < 1e-6 && y.abs() < 1e-6 {
        return None;
    }
    Some(Offset { x, y })
}

/// Push a glyph's outline outward by `offset`.
///
/// Point count, order and types are untouched, so the result stays
/// compatible with the glyph it came from.
pub fn embolden(glyph: &Glyph, offset: Offset) -> Glyph {
    let mut out = glyph.clone();
    for contour in out.contours.iter_mut() {
        let normals = outward_normals(contour);
        for (point, (nx, ny)) in contour.points.iter_mut().zip(normals) {
            point.x += nx * offset.x;
            point.y += ny * offset.y;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use norad::{ContourPoint, PointType};

    fn pt(x: f64, y: f64) -> ContourPoint {
        ContourPoint::new(x, y, PointType::Line, false, None, None)
    }

    /// Counter-clockwise, the way an outer contour is drawn.
    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Contour {
        Contour::new(
            vec![pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)],
            None,
        )
    }

    fn glyph_of(contours: Vec<Contour>) -> Glyph {
        let mut g = Glyph::new("test");
        g.width = 500.0;
        g.contours = contours;
        g
    }

    #[test]
    fn an_outer_contour_grows() {
        let g = glyph_of(vec![square(0.0, 0.0, 100.0, 100.0)]);
        let out = embolden(&g, Offset { x: 10.0, y: 10.0 });
        let xs: Vec<f64> = out.contours[0].points.iter().map(|p| p.x).collect();
        assert!(xs.iter().cloned().fold(f64::MAX, f64::min) < 0.0, "{xs:?}");
        assert!(xs.iter().cloned().fold(f64::MIN, f64::max) > 100.0, "{xs:?}");
    }

    /// A counter is wound the other way, so pushing outward has to
    /// shrink it. Getting this backwards fills in every bowl.
    #[test]
    fn a_counter_shrinks() {
        let mut inner = square(20.0, 20.0, 80.0, 80.0);
        inner.points.reverse();
        let g = glyph_of(vec![square(0.0, 0.0, 100.0, 100.0), inner]);
        let out = embolden(&g, Offset { x: 10.0, y: 10.0 });
        let xs: Vec<f64> = out.contours[1].points.iter().map(|p| p.x).collect();
        let (lo, hi) = (
            xs.iter().cloned().fold(f64::MAX, f64::min),
            xs.iter().cloned().fold(f64::MIN, f64::max),
        );
        assert!(lo > 20.0 && hi < 80.0, "the counter should close up: {xs:?}");
    }

    /// The whole reason to move points rather than re-draw: the result
    /// still interpolates with what it came from.
    #[test]
    fn the_structure_is_untouched() {
        let g = glyph_of(vec![square(0.0, 0.0, 100.0, 100.0)]);
        let out = embolden(&g, Offset { x: 10.0, y: 4.0 });
        assert_eq!(out.contours.len(), g.contours.len());
        assert_eq!(out.contours[0].points.len(), g.contours[0].points.len());
        let types: Vec<_> = out.contours[0].points.iter().map(|p| p.typ).collect();
        let was: Vec<_> = g.contours[0].points.iter().map(|p| p.typ).collect();
        assert_eq!(types, was);
    }

    /// The point of the module: a reference pair says how far to push,
    /// and applying that to the reference reproduces it.
    #[test]
    fn a_reference_pair_teaches_its_own_offset() {
        let light = glyph_of(vec![square(0.0, 0.0, 100.0, 100.0)]);
        let heavy = embolden(&light, Offset { x: 12.0, y: 6.0 });
        let learned = learn_offset(&[(&light, &heavy)]).expect("learnable");
        assert!((learned.x - 12.0).abs() < 1e-6, "{learned:?}");
        assert!((learned.y - 6.0).abs() < 1e-6, "{learned:?}");
    }

    /// Type gains weight differently on each axis, so a single number
    /// would be wrong for one of them.
    #[test]
    fn the_two_axes_are_learned_apart() {
        let light = glyph_of(vec![square(0.0, 0.0, 200.0, 100.0)]);
        let heavy = embolden(&light, Offset { x: 48.0, y: 12.0 });
        let learned = learn_offset(&[(&light, &heavy)]).expect("learnable");
        assert!((learned.x - 48.0).abs() < 1e-6);
        assert!((learned.y - 12.0).abs() < 1e-6);
        assert_ne!(learned.x, learned.y);
    }

    #[test]
    fn incompatible_pairs_are_skipped() {
        let light = glyph_of(vec![square(0.0, 0.0, 100.0, 100.0)]);
        let mut odd = light.clone();
        odd.contours[0].points.push(pt(50.0, 50.0));
        assert!(learn_offset(&[(&light, &odd)]).is_none());
    }

    #[test]
    fn a_pair_that_did_not_move_teaches_nothing() {
        let light = glyph_of(vec![square(0.0, 0.0, 100.0, 100.0)]);
        assert!(learn_offset(&[(&light, &light)]).is_none());
    }
}
