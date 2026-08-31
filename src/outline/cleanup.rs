// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Outline cleanup: operations that keep the shape and fix its points.
//!
//! Tidy duplicate points, correct contour direction, round coordinates,
//! add extremes, refit handles, open or close a contour.

use std::collections::HashSet;

use kurbo::BezPath;

use crate::outline::glyph_paths::contour_to_bezpath;

/// Scale each curve segment's handles to a fraction of their
/// maximum length.
///
/// A handle's maximum is the distance from its on-curve point to
/// the intersection of the segment's two end tangents. At 100%,
/// both handles reach that intersection, which is the longest the
/// curve can be without a kink. This is the scale that Fit Curve
/// uses in Glyphs.
///
/// Handle directions do not change; only lengths do. If the
/// selection is empty, every segment is in scope. Otherwise, only
/// segments with a selected point are. Returns true when any
/// handle moved.
pub fn fit_curve_handles(
    glyph: &mut norad::Glyph,
    selected: &HashSet<(usize, usize)>,
    fraction: f64,
) -> bool {
    use kurbo::{Point, Vec2};
    if !(0.01..=1.5).contains(&fraction) {
        return false;
    }
    let all = selected.is_empty();
    let mut changed = false;
    for (ci, contour) in glyph.contours.iter_mut().enumerate() {
        let pts = &mut contour.points;
        let n = pts.len();
        if n < 4 {
            continue;
        }
        // Walk cubic segments: offcurve, offcurve, curve on-point,
        // with the previous on-point before them.
        for i in 0..n {
            if pts[i].typ != norad::PointType::Curve {
                continue;
            }
            let c2i = (i + n - 1) % n;
            let c1i = (i + n - 2) % n;
            let p0i = (i + n - 3) % n;
            if pts[c1i].typ != norad::PointType::OffCurve
                || pts[c2i].typ != norad::PointType::OffCurve
                || pts[p0i].typ == norad::PointType::OffCurve
            {
                continue;
            }
            let in_scope = all
                || [p0i, c1i, c2i, i]
                    .iter()
                    .any(|&k| selected.contains(&(ci, k)));
            if !in_scope {
                continue;
            }
            let p0 = Point::new(pts[p0i].x, pts[p0i].y);
            let c1 = Point::new(pts[c1i].x, pts[c1i].y);
            let c2 = Point::new(pts[c2i].x, pts[c2i].y);
            let p3 = Point::new(pts[i].x, pts[i].y);
            let d0 = c1 - p0;
            let d3 = c2 - p3;
            if d0.hypot() < 1e-9 || d3.hypot() < 1e-9 {
                continue;
            }
            let (d0, d3) = (d0 / d0.hypot(), d3 / d3.hypot());
            // Ray intersection p0 + s·d0 = p3 + u·d3.
            let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
            let denom = cross(d0, d3);
            if denom.abs() < 1e-9 {
                continue; // parallel tangents: no finite maximum
            }
            let w = p3 - p0;
            let s_max = cross(w, d3) / denom;
            let u_max = cross(w, d0) / denom;
            if s_max <= 0.0 || u_max <= 0.0 {
                continue; // tangents meet behind the points
            }
            let nc1 = p0 + d0 * (s_max * fraction);
            let nc2 = p3 + d3 * (u_max * fraction);
            let write = |pt: &mut norad::ContourPoint, p: Point| {
                let (nx, ny) = (p.x.round(), p.y.round());
                let moved = pt.x != nx || pt.y != ny;
                pt.x = nx;
                pt.y = ny;
                moved
            };
            changed |= write(&mut pts[c1i], nc1);
            changed |= write(&mut pts[c2i], nc2);
        }
    }
    changed
}

/// Insert an on-curve point at every curve extremum.
///
/// An extremum is a point where the curve's tangent is exactly
/// horizontal or vertical. This is what Add Extremes does in
/// Glyphs. If the selection is empty, every segment is in scope.
/// Otherwise, only segments with a selected point are. Returns
/// true when any point was added.
pub fn add_extreme_points(glyph: &mut norad::Glyph, selected: &HashSet<(usize, usize)>) -> bool {
    use kurbo::ParamCurveExtrema as _;
    let mut changed = false;
    // One insertion per scan: a split invalidates the segment list.
    let mut guard = 0;
    'outer: loop {
        guard += 1;
        if guard > 300 {
            break;
        }
        for hit in crate::outline::segment_ops::segments(glyph) {
            let kurbo::PathSeg::Cubic(cubic) = hit.seg else {
                continue;
            };
            let in_scope =
                selected.is_empty() || hit.point_ids().iter().any(|id| selected.contains(id));
            if !in_scope {
                continue;
            }
            for t in cubic.extrema() {
                // Extrema at (or rounding onto) the endpoints are
                // already nodes; skipping them also terminates the
                // rescan loop, because subsegments keep their
                // extrema at the ends.
                if !(0.02..=0.98).contains(&t) {
                    continue;
                }
                if crate::outline::segment_ops::insert_point_on_segment(glyph, &hit, t).is_some() {
                    changed = true;
                    continue 'outer;
                }
            }
        }
        break;
    }
    changed
}

/// Open a closed contour at a point, or close an open contour.
///
/// If the contour is closed, the on-curve point at `(ci, pi)`
/// becomes the new start, typed `Move`. If the contour is open,
/// the `Move` start becomes a `Line` and `pi` is ignored. This is
/// how Glyphs opens and closes paths. Returns true when the
/// contour changed.
pub fn toggle_contour_open(glyph: &mut norad::Glyph, ci: usize, pi: usize) -> bool {
    use norad::PointType;
    let Some(contour) = glyph.contours.get_mut(ci) else {
        return false;
    };
    let n = contour.points.len();
    if n < 2 || pi >= n {
        return false;
    }
    let is_open = contour
        .points
        .first()
        .is_some_and(|p| p.typ == PointType::Move);
    if is_open {
        // Close: the Move start becomes an ordinary point. If the
        // start needs a curve type it stays Line; the designer
        // redraws the closing segment as needed.
        contour.points[0].typ = PointType::Line;
        return true;
    }
    if contour.points[pi].typ == PointType::OffCurve {
        return false;
    }
    contour.points.rotate_left(pi);
    contour.points[0].typ = PointType::Move;
    true
}

/// Remove zero-length line segments.
///
/// A zero-length segment is an on-curve point that duplicates the
/// on-curve point before it. The check includes the closing
/// segment of a closed contour. The operation is conservative on
/// purpose: simplifying curves is Simplify's job, not Tidy's. This
/// is Path > Tidy up Paths in Glyphs. Returns the number of points
/// removed.
pub fn tidy_contours(glyph: &mut norad::Glyph) -> usize {
    use norad::PointType;
    let mut removed = 0usize;
    for contour in glyph.contours.iter_mut() {
        let closed = contour
            .points
            .first()
            .is_none_or(|p| p.typ != PointType::Move);
        let mut i = 1;
        while i < contour.points.len() {
            let dup = {
                let prev = &contour.points[i - 1];
                let here = &contour.points[i];
                here.typ == PointType::Line
                    && prev.typ != PointType::OffCurve
                    && (here.x - prev.x).abs() < 0.01
                    && (here.y - prev.y).abs() < 0.01
            };
            if dup {
                contour.points.remove(i);
                removed += 1;
            } else {
                i += 1;
            }
        }
        // A closed contour's last Line landing on the first point is
        // the same zero-length segment, wrapped.
        if closed && contour.points.len() > 2 {
            let first = contour.points[0].clone();
            let last = contour.points.last().unwrap().clone();
            if last.typ == PointType::Line
                && first.typ != PointType::OffCurve
                && (last.x - first.x).abs() < 0.01
                && (last.y - first.y).abs() < 0.01
            {
                contour.points.pop();
                removed += 1;
            }
        }
    }
    removed
}

/// Rewind contours so outers run counterclockwise and holes run
/// clockwise.
///
/// This is the PostScript and UFO convention for cubic outlines,
/// and the winding that remove overlap expects. A contour counts
/// as a hole when an odd number of other contours contain its
/// first on-curve point. This is Path > Correct Path Direction in
/// Glyphs. Returns the number of contours reversed.
pub fn correct_path_directions(glyph: &mut norad::Glyph) -> usize {
    use kurbo::Shape as _;
    let paths: Vec<BezPath> = glyph.contours.iter().map(contour_to_bezpath).collect();
    let mut flip: HashSet<(usize, usize)> = HashSet::new();
    let mut flipped = 0usize;
    for (ci, contour) in glyph.contours.iter().enumerate() {
        let Some(probe) = contour
            .points
            .iter()
            .find(|p| p.typ != norad::PointType::OffCurve)
        else {
            continue;
        };
        let pt = kurbo::Point::new(probe.x, probe.y);
        let depth = paths
            .iter()
            .enumerate()
            .filter(|(oi, path)| *oi != ci && path.contains(pt))
            .count();
        let area = paths[ci].area();
        let want_ccw = depth % 2 == 0;
        if (want_ccw && area < 0.0) || (!want_ccw && area > 0.0) {
            flip.insert((ci, 0));
            flipped += 1;
        }
    }
    if !flip.is_empty() {
        crate::outline::glyph_ops::reverse_contours(glyph, &flip);
    }
    flipped
}

/// Round every point to the integer grid.
///
/// This is Path > Round Coordinates in Glyphs. Returns the number
/// of points that moved.
pub fn round_glyph_coordinates(glyph: &mut norad::Glyph) -> usize {
    let mut moved = 0usize;
    for contour in glyph.contours.iter_mut() {
        for p in contour.points.iter_mut() {
            let (rx, ry) = (p.x.round(), p.y.round());
            if rx != p.x || ry != p.y {
                p.x = rx;
                p.y = ry;
                moved += 1;
            }
        }
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_extremes_inserts_the_dip() {
        use norad::{Contour, ContourPoint, PointType};
        // A symmetric dip: extremum (vertical tangent point of the
        // y-curve) at t=0.5, which is (100, -37.5).
        let pt = |x, y, typ| ContourPoint::new(x, y, typ, false, None, None);
        let contour = Contour::new(
            vec![
                pt(0.0, 0.0, PointType::Move),
                pt(50.0, -50.0, PointType::OffCurve),
                pt(150.0, -50.0, PointType::OffCurve),
                pt(200.0, 0.0, PointType::Curve),
            ],
            None,
        );
        let mut glyph = norad::Glyph::new("extremes-test");
        glyph.contours = vec![contour];
        let all = HashSet::new();
        assert!(add_extreme_points(&mut glyph, &all));
        let ons: Vec<(f64, f64)> = glyph.contours[0]
            .points
            .iter()
            .filter(|p| p.typ != PointType::OffCurve)
            .map(|p| (p.x, p.y))
            .collect();
        assert!(
            ons.iter()
                .any(|&(x, y)| (x - 100.0).abs() <= 1.0 && (y + 37.5).abs() <= 1.5),
            "extremum node added: {ons:?}"
        );
        // Second run finds nothing new.
        assert!(!add_extreme_points(&mut glyph, &all));
    }
    #[test]
    fn tidy_correct_and_round_fix_a_messy_glyph() {
        use norad::{Contour, ContourPoint, PointType};
        let pts = |coords: &[(f64, f64)]| -> Vec<ContourPoint> {
            coords
                .iter()
                .map(|&(x, y)| ContourPoint::new(x, y, PointType::Line, false, None, None))
                .collect()
        };
        let mut glyph = norad::Glyph::new("messy");
        // Outer square drawn clockwise (wrong), with a duplicated
        // point and an off-grid coordinate; inner hole drawn
        // counter-clockwise (wrong for a hole).
        glyph.contours = vec![
            Contour::new(
                pts(&[
                    (0.0, 0.0),
                    (0.0, 400.0),
                    (0.0, 400.0),
                    (400.0, 400.0),
                    (400.2, 0.0),
                ]),
                None,
            ),
            Contour::new(
                pts(&[
                    (100.0, 100.0),
                    (300.0, 100.0),
                    (300.0, 300.0),
                    (100.0, 300.0),
                ]),
                None,
            ),
        ];
        assert_eq!(tidy_contours(&mut glyph), 1);
        assert_eq!(glyph.contours[0].points.len(), 4);
        assert_eq!(round_glyph_coordinates(&mut glyph), 1);
        assert_eq!(correct_path_directions(&mut glyph), 2);
        use kurbo::Shape as _;
        let outer = crate::outline::glyph_paths::contour_to_bezpath(&glyph.contours[0]);
        let hole = crate::outline::glyph_paths::contour_to_bezpath(&glyph.contours[1]);
        assert!(outer.area() > 0.0, "outer counter-clockwise");
        assert!(hole.area() < 0.0, "hole clockwise");
        // Running again changes nothing.
        assert_eq!(correct_path_directions(&mut glyph), 0);
        assert_eq!(tidy_contours(&mut glyph), 0);
    }
    #[test]
    fn contours_open_and_close_again() {
        use norad::{Contour, ContourPoint, PointType};
        let square = Contour::new(
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
                .iter()
                .map(|&(x, y)| ContourPoint::new(x, y, PointType::Line, false, None, None))
                .collect(),
            None,
        );
        let mut glyph = norad::Glyph::new("openclose");
        glyph.contours = vec![square];
        // Open at point 2: it becomes the Move start.
        assert!(toggle_contour_open(&mut glyph, 0, 2));
        let pts = &glyph.contours[0].points;
        assert_eq!(pts[0].typ, PointType::Move);
        assert_eq!((pts[0].x, pts[0].y), (100.0, 100.0));
        // Close again: the Move becomes a Line, same point count.
        assert!(toggle_contour_open(&mut glyph, 0, 0));
        assert!(
            glyph.contours[0]
                .points
                .iter()
                .all(|p| p.typ != PointType::Move)
        );
        assert_eq!(glyph.contours[0].points.len(), 4);
        // Off-curve target refuses.
        assert!(!toggle_contour_open(&mut glyph, 0, 99));
    }
    #[test]
    fn fit_curve_sets_handle_fractions() {
        use norad::{Contour, ContourPoint, PointType};
        // A quarter arc: on-point (0,0) tangent up-ish, on-point
        // (100,100) tangent right-ish; tangents meet at (0,100).
        let pt = |x, y, typ, smooth| ContourPoint::new(x, y, typ, smooth, None, None);
        let contour = Contour::new(
            vec![
                pt(0.0, 0.0, PointType::Move, false),
                pt(0.0, 10.0, PointType::OffCurve, false),
                pt(50.0, 100.0, PointType::OffCurve, false),
                pt(100.0, 100.0, PointType::Curve, false),
            ],
            None,
        );
        let mut glyph = norad::Glyph::new("fit-test");
        glyph.contours = vec![contour];
        let all = HashSet::new();
        assert!(fit_curve_handles(&mut glyph, &all, 0.5));
        let pts = &glyph.contours[0].points;
        // First handle: from (0,0) toward (0,100), half way = (0,50).
        assert_eq!((pts[1].x, pts[1].y), (0.0, 50.0));
        // Second handle: from (100,100) toward (0,100), half = (50,100).
        assert_eq!((pts[2].x, pts[2].y), (50.0, 100.0));
    }
}
