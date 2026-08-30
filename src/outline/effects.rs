// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Outline effects: operations that produce a new shape from a contour.
//!
//! Expand a stroke, offset, extrude, roughen, apply a corner component,
//! and bolden by a learned per-point offset. Every function takes a
//! `norad::Glyph`, edits it in place, and reports whether anything
//! changed.

use std::collections::{HashMap, HashSet};

use kurbo::{Affine, BezPath, PathEl};

use crate::outline::glyph_ops::bezpath_to_contour;
use crate::outline::glyph_paths::contour_to_bezpath;

/// Replace targeted contours with the outline of a stroke of the
/// given width (round joins and caps), the Make Stroke half of
/// Glyphs' Offset Curve. An empty `selected` set targets every
/// contour. Returns false when nothing changed.
pub fn expand_stroke_contours(
    glyph: &mut norad::Glyph,
    selected: &HashSet<usize>,
    width: f64,
) -> bool {
    let style = kurbo::Stroke::new(width);
    let opts = kurbo::StrokeOpts::default();
    let empty = HashMap::new();
    let mut out: Vec<norad::Contour> = Vec::new();
    let mut any = false;
    for (ci, contour) in glyph.contours.iter().enumerate() {
        let targeted = selected.is_empty() || selected.contains(&ci);
        if !targeted {
            out.push(contour.clone());
            continue;
        }
        let path = contour_to_bezpath(contour);
        let stroked = kurbo::stroke(path.elements().iter().copied(), &style, &opts, 0.25);
        // One stroked outline can be several subpaths (a closed
        // skeleton keeps its counter).
        let mut sub = BezPath::new();
        let mut made = false;
        for el in stroked.elements() {
            if matches!(el, PathEl::MoveTo(_)) && !sub.elements().is_empty() {
                if let Some(c) = bezpath_to_contour(&sub, &empty) {
                    out.push(c);
                    made = true;
                }
                sub = BezPath::new();
            }
            sub.push(*el);
        }
        if !sub.elements().is_empty()
            && let Some(c) = bezpath_to_contour(&sub, &empty)
        {
            out.push(c);
            made = true;
        }
        if made {
            any = true;
        } else {
            out.push(contour.clone());
        }
    }
    if any {
        glyph.contours = out;
    }
    any
}

/// Offset every contour outward (positive `delta`, bolder) or inward
/// (negative, lighter): the whole glyph is unioned with — or cut by —
/// a stroke band of width 2·delta around its own outline, which moves
/// counters the opposite way automatically. The bolder/lighter half
/// of Glyphs' Offset Curve. Returns false when nothing changed.
pub fn offset_glyph_contours(glyph: &mut norad::Glyph, delta: f64) -> bool {
    if delta == 0.0 || glyph.contours.is_empty() {
        return false;
    }
    let mut combined = BezPath::new();
    let mut band = BezPath::new();
    let style = kurbo::Stroke::new(delta.abs() * 2.0);
    let opts = kurbo::StrokeOpts::default();
    for contour in &glyph.contours {
        let path = contour_to_bezpath(contour);
        band.extend(
            kurbo::stroke(path.elements().iter().copied(), &style, &opts, 0.25)
                .elements()
                .iter()
                .copied(),
        );
        combined.extend(path.elements().iter().copied());
    }
    let op = if delta > 0.0 {
        linesweeper::BinaryOp::Union
    } else {
        linesweeper::BinaryOp::Difference
    };
    let Ok(result) = linesweeper::binary_op(&combined, &band, linesweeper::FillRule::NonZero, op)
    else {
        return false;
    };
    let smooth_at: HashMap<(i64, i64), bool> = glyph
        .contours
        .iter()
        .flat_map(|c| c.points.iter())
        .filter(|p| p.typ != norad::PointType::OffCurve)
        .map(|p| ((p.x.round() as i64, p.y.round() as i64), p.smooth))
        .collect();
    let mut contours: Vec<norad::Contour> = Vec::new();
    for contour in result.contours() {
        if let Some(c) = bezpath_to_contour(&contour.path, &smooth_at) {
            contours.push(c);
        }
    }
    if contours.is_empty() {
        return false;
    }
    glyph.contours = contours;
    true
}

/// Extrude (Glyphs' filter): sweep the glyph along `angle` by
/// `offset` units — the union of the shape, its translated copy,
/// and a wall quad per segment — then cut the front face away
/// unless `keep_front`. Angle 0 extrudes right; 30 is the Glyphs
/// default's downward-right shadow.
pub fn extrude_glyph_contours(
    glyph: &mut norad::Glyph,
    offset: f64,
    angle_degrees: f64,
    keep_front: bool,
) -> bool {
    if offset <= 0.0 || glyph.contours.is_empty() {
        return false;
    }
    let (sin, cos) = (-angle_degrees).to_radians().sin_cos();
    let d = kurbo::Vec2::new(offset * cos, offset * sin);
    let mut combined = BezPath::new();
    let mut front = BezPath::new();
    for contour in &glyph.contours {
        let path = contour_to_bezpath(contour);
        front.extend(path.elements().iter().copied());
        combined.extend(path.elements().iter().copied());
        combined.extend((Affine::translate(d) * &path).elements().iter().copied());
        // Wall quads, each wound positive so the nonzero union eats
        // them all the same way.
        let mut walls = BezPath::new();
        for seg in path.segments() {
            use kurbo::ParamCurve as _;
            let (a, b) = (seg.eval(0.0), seg.eval(1.0));
            let (a2, b2) = (a + d, b + d);
            let area = (b.x - a.x) * (b2.y - a.y) - (b2.x - a.x) * (b.y - a.y);
            let quad = if area >= 0.0 {
                [a, b, b2, a2]
            } else {
                [a, a2, b2, b]
            };
            walls.move_to(quad[0]);
            walls.line_to(quad[1]);
            walls.line_to(quad[2]);
            walls.line_to(quad[3]);
            walls.close_path();
        }
        combined.extend(walls.elements().iter().copied());
    }
    let empty = BezPath::new();
    let Ok(silhouette) = linesweeper::binary_op(
        &combined,
        &empty,
        linesweeper::FillRule::NonZero,
        linesweeper::BinaryOp::Union,
    ) else {
        return false;
    };
    let mut merged = BezPath::new();
    for contour in silhouette.contours() {
        merged.extend(contour.path.elements().iter().copied());
    }
    let result = if keep_front {
        merged
    } else {
        let Ok(cut) = linesweeper::binary_op(
            &merged,
            &front,
            linesweeper::FillRule::NonZero,
            linesweeper::BinaryOp::Difference,
        ) else {
            return false;
        };
        let mut out = BezPath::new();
        for contour in cut.contours() {
            out.extend(contour.path.elements().iter().copied());
        }
        out
    };
    let empty_map = HashMap::new();
    let mut contours: Vec<norad::Contour> = Vec::new();
    let mut sub = BezPath::new();
    for el in result.elements() {
        if matches!(el, PathEl::MoveTo(_)) && !sub.elements().is_empty() {
            if let Some(c) = bezpath_to_contour(&sub, &empty_map) {
                contours.push(c);
            }
            sub = BezPath::new();
        }
        sub.push(*el);
    }
    if !sub.elements().is_empty()
        && let Some(c) = bezpath_to_contour(&sub, &empty_map)
    {
        contours.push(c);
    }
    if contours.is_empty() {
        return false;
    }
    glyph.contours = contours;
    true
}

/// Roughen (Glyphs' filter): flatten each targeted contour into
/// straight segments of roughly `segment_length`, then jitter every
/// point by up to ±h/±v. `seed` varies run to run so Apply twice
/// gives a different rough.
pub fn roughen_glyph_contours(
    glyph: &mut norad::Glyph,
    selected: &HashSet<usize>,
    segment_length: f64,
    h: f64,
    v: f64,
    seed: u64,
) -> bool {
    use kurbo::ParamCurve as _;
    use kurbo::ParamCurveArclen as _;
    if segment_length < 1.0 {
        return false;
    }
    // A tiny LCG: deterministic per seed, no clock, no dependency.
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    let mut jitter = |amount: f64| {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let unit = (state >> 11) as f64 / (1u64 << 53) as f64;
        (unit * 2.0 - 1.0) * amount
    };
    let mut changed = false;
    for (ci, contour) in glyph.contours.iter_mut().enumerate() {
        if !(selected.is_empty() || selected.contains(&ci)) {
            continue;
        }
        let path = contour_to_bezpath(&*contour);
        let mut points: Vec<norad::ContourPoint> = Vec::new();
        for seg in path.segments() {
            let len = seg.arclen(0.5);
            let steps = (len / segment_length).ceil().max(1.0) as usize;
            for step in 0..steps {
                let t = step as f64 / steps as f64;
                let p = seg.eval(t);
                points.push(norad::ContourPoint::new(
                    (p.x + jitter(h)).round(),
                    (p.y + jitter(v)).round(),
                    norad::PointType::Line,
                    false,
                    None,
                    None,
                ));
            }
        }
        if points.len() >= 3 {
            *contour = norad::Contour::new(points, None);
            changed = true;
        }
    }
    changed
}

/// Apply a corner glyph to one on-curve node: the corner's open
/// path, drawn around its origin, is mapped into the node's frame —
/// corner-space x runs back along the incoming segment, y forward
/// along the outgoing one (Glyphs' fit, which shears the corner to
/// unequal angles) — and spliced in place of the node. Both
/// neighbors must be on-curve (line corners) in this first slice.
/// The result is a plain outline: pipelines see baked points.
pub fn apply_corner_at(
    glyph: &mut norad::Glyph,
    corner: &norad::Glyph,
    ci: usize,
    pi: usize,
) -> bool {
    use norad::PointType;
    let Some(corner_contour) = corner.contours.first() else {
        return false;
    };
    if corner_contour.points.len() < 2 {
        return false;
    }
    let Some(contour) = glyph.contours.get(ci) else {
        return false;
    };
    let n = contour.points.len();
    if n < 3 || pi >= n {
        return false;
    }
    let point = &contour.points[pi];
    if point.typ == PointType::OffCurve {
        return false;
    }
    let prev = &contour.points[(pi + n - 1) % n];
    let next = &contour.points[(pi + 1) % n];
    if prev.typ == PointType::OffCurve || next.typ == PointType::OffCurve {
        return false; // curve corners come later
    }
    let node = (point.x, point.y);
    let len_in = ((node.0 - prev.x).powi(2) + (node.1 - prev.y).powi(2)).sqrt();
    let len_out = ((next.x - node.0).powi(2) + (next.y - node.1).powi(2)).sqrt();
    if len_in < 1e-6 || len_out < 1e-6 {
        return false;
    }
    let u = ((node.0 - prev.x) / len_in, (node.1 - prev.y) / len_in);
    let v = ((next.x - node.0) / len_out, (next.y - node.1) / len_out);
    let mapped: Vec<norad::ContourPoint> = corner_contour
        .points
        .iter()
        .map(|p| {
            let (x, y) = (
                node.0 + p.x * u.0 + p.y * v.0,
                node.1 + p.x * u.1 + p.y * v.1,
            );
            let typ = match p.typ {
                PointType::Move => PointType::Line,
                other => other,
            };
            norad::ContourPoint::new(x.round(), y.round(), typ, p.smooth, None, None)
        })
        .collect();
    let contour = glyph.contours.get_mut(ci).expect("checked");
    contour.points.splice(pi..=pi, mapped);
    true
}

/// Move a glyph's points by the model's offsets, in the order the
/// outline reader produced them.
///
/// Walks the same contours in the same rotation `font_ml::ufo` uses,
/// so offset *n* lands on the point it was predicted for. Point types
/// and smooth flags are left alone: this moves points and nothing
/// else.
pub fn bolden_contours(
    glyph: &norad::Glyph,
    deltas: &[(i32, i32)],
    center: (i32, i32),
) -> Vec<norad::Contour> {
    let mut next = deltas.iter();
    let mut out = Vec::with_capacity(glyph.contours.len());
    for contour in &glyph.contours {
        let points = &contour.points;
        let start = points
            .iter()
            .position(|p| p.typ != norad::PointType::OffCurve)
            .unwrap_or(0);
        let n = points.len();
        let mut moved = points.clone();
        // Visit in pen order, starting where the reader started.
        for step in 0..n {
            let i = (start + step) % n;
            let Some((dx, dy)) = next.next().copied() else {
                break;
            };
            moved[i].x += (dx + center.0) as f64;
            moved[i].y += (dy + center.1) as f64;
        }
        // The reader ends a closed contour by returning to its start,
        // so it yields one offset more than the contour has points.
        // Drop it, or every later contour is shifted by one point.
        next.next();
        out.push(norad::Contour::new(moved, contour.identifier().cloned()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_splices_the_chamfer() {
        use norad::{Contour, ContourPoint, PointType};
        // The ComponentDemo chamfer: open path (-60, 0) -> (0, 60)
        // around the origin.
        let corner_contour = Contour::new(
            vec![
                ContourPoint::new(-60.0, 0.0, PointType::Move, false, None, None),
                ContourPoint::new(0.0, 60.0, PointType::Line, false, None, None),
            ],
            None,
        );
        let mut corner = norad::Glyph::new("_corner.chamfer");
        corner.contours = vec![corner_contour];
        // A square; apply at (100, 0): incoming runs +x, outgoing +y.
        let square = Contour::new(
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
                .iter()
                .map(|&(x, y)| ContourPoint::new(x, y, PointType::Line, false, None, None))
                .collect(),
            None,
        );
        let mut glyph = norad::Glyph::new("square");
        glyph.contours = vec![square];
        assert!(apply_corner_at(&mut glyph, &corner, 0, 1));
        let pts: Vec<(f64, f64)> = glyph.contours[0]
            .points
            .iter()
            .map(|p| (p.x, p.y))
            .collect();
        // The node (100, 0) became two: 60 back along the incoming
        // (+x) segment, and 60 up along the outgoing (+y) one.
        assert_eq!(pts.len(), 5);
        assert!(pts.contains(&(40.0, 0.0)), "{pts:?}");
        assert!(pts.contains(&(100.0, 60.0)), "{pts:?}");
        assert!(!pts.contains(&(100.0, 0.0)), "original corner replaced");
        // Refuses off-curve neighbors and short segments untouched.
        let mut tiny = norad::Glyph::new("tiny");
        tiny.contours = vec![Contour::new(
            vec![
                ContourPoint::new(0.0, 0.0, PointType::Line, false, None, None),
                ContourPoint::new(0.0, 0.0, PointType::Line, false, None, None),
                ContourPoint::new(1.0, 1.0, PointType::Line, false, None, None),
            ],
            None,
        )];
        assert!(!apply_corner_at(&mut tiny, &corner, 0, 1));
    }
    #[test]
    fn extrude_and_roughen_transform_a_square() {
        use norad::{Contour, ContourPoint, PointType};
        let square = || {
            Contour::new(
                [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
                    .iter()
                    .map(|&(x, y)| ContourPoint::new(x, y, PointType::Line, false, None, None))
                    .collect(),
                None,
            )
        };
        let bbox = |g: &norad::Glyph| {
            let (mut min, mut max) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
            for p in g.contours.iter().flat_map(|c| c.points.iter()) {
                min = (min.0.min(p.x), min.1.min(p.y));
                max = (max.0.max(p.x), max.1.max(p.y));
            }
            (min, max)
        };
        // Extrude right-down at 30° by 40: the box grows +40·cos30 in
        // x and −40·sin30 in y, and the front face is cut away.
        let mut g = norad::Glyph::new("extrude-test");
        g.contours = vec![square()];
        assert!(extrude_glyph_contours(&mut g, 40.0, 30.0, false));
        let (min, max) = bbox(&g);
        assert!((max.0 - (100.0 + 40.0 * (30f64).to_radians().cos())).abs() <= 2.0);
        assert!((min.1 - (-40.0 * (30f64).to_radians().sin())).abs() <= 2.0);

        // Roughen: many short jittered segments replace the four.
        let mut r = norad::Glyph::new("roughen-test");
        r.contours = vec![square()];
        let all = std::collections::HashSet::new();
        assert!(roughen_glyph_contours(&mut r, &all, 10.0, 4.0, 4.0, 7));
        assert!(
            r.contours[0].points.len() >= 30,
            "flattened into short segments: {}",
            r.contours[0].points.len()
        );
        // Different seed, different rough.
        let mut r2 = norad::Glyph::new("roughen-test-2");
        r2.contours = vec![square()];
        assert!(roughen_glyph_contours(&mut r2, &all, 10.0, 4.0, 4.0, 8));
        assert_ne!(
            r.contours[0]
                .points
                .iter()
                .map(|p| (p.x, p.y))
                .collect::<Vec<_>>(),
            r2.contours[0]
                .points
                .iter()
                .map(|p| (p.x, p.y))
                .collect::<Vec<_>>(),
        );
    }
    #[test]
    fn offset_bolder_and_lighter() {
        use norad::{Contour, ContourPoint, PointType};
        // A closed 100x100 square, counter-clockwise (postscript
        // outer direction).
        let square = |pts: &[(f64, f64)]| {
            Contour::new(
                pts.iter()
                    .map(|&(x, y)| ContourPoint::new(x, y, PointType::Line, false, None, None))
                    .collect(),
                None,
            )
        };
        let outer = square(&[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]);
        let mut glyph = norad::Glyph::new("offset-test");
        glyph.contours = vec![outer.clone()];
        assert!(offset_glyph_contours(&mut glyph, 10.0));
        let bbox = |g: &norad::Glyph| {
            let (mut min, mut max) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
            for p in g.contours.iter().flat_map(|c| c.points.iter()) {
                min = (min.0.min(p.x), min.1.min(p.y));
                max = (max.0.max(p.x), max.1.max(p.y));
            }
            (max.0 - min.0, max.1 - min.1)
        };
        let (w, h) = bbox(&glyph);
        assert!(
            (w - 120.0).abs() <= 2.0 && (h - 120.0).abs() <= 2.0,
            "bolder grows: {w}x{h}"
        );
        let mut glyph2 = norad::Glyph::new("offset-test-2");
        glyph2.contours = vec![outer];
        assert!(offset_glyph_contours(&mut glyph2, -10.0));
        let (w2, h2) = bbox(&glyph2);
        assert!(
            (w2 - 80.0).abs() <= 2.0 && (h2 - 80.0).abs() <= 2.0,
            "lighter shrinks: {w2}x{h2}"
        );
    }
    #[test]
    fn expand_stroke_makes_outlines() {
        use norad::{Contour, ContourPoint, PointType};
        // An open two-point skeleton line from (0,0) to (100,0).
        let line = Contour::new(
            vec![
                ContourPoint::new(0.0, 0.0, PointType::Move, false, None, None),
                ContourPoint::new(100.0, 0.0, PointType::Line, false, None, None),
            ],
            None,
        );
        let mut glyph = norad::Glyph::new("stroke-test");
        glyph.contours = vec![line];
        let all = std::collections::HashSet::new();
        assert!(expand_stroke_contours(&mut glyph, &all, 40.0));
        // The skeleton became a closed outline that spans the stroke:
        // 100 long plus round caps of radius 20 each side, 40 tall.
        assert_eq!(glyph.contours.len(), 1);
        let ys: Vec<f64> = glyph.contours[0].points.iter().map(|p| p.y).collect();
        let xs: Vec<f64> = glyph.contours[0].points.iter().map(|p| p.x).collect();
        let (min_y, max_y) = ys
            .iter()
            .fold((f64::MAX, f64::MIN), |a, &v| (a.0.min(v), a.1.max(v)));
        let (min_x, max_x) = xs
            .iter()
            .fold((f64::MAX, f64::MIN), |a, &v| (a.0.min(v), a.1.max(v)));
        assert!((max_y - min_y - 40.0).abs() <= 2.0, "stroke height ~40");
        assert!(
            (max_x - min_x - 140.0).abs() <= 2.0,
            "length plus caps ~140"
        );
        // Width zero refuses.
        let mut untouched = norad::Glyph::new("no-op");
        assert!(!expand_stroke_contours(&mut untouched, &all, 40.0));
    }
}
