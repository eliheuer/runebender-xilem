// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The pieces of a labeled sample's ink: its contours cut by the sample's cuts.
//!
//! A seed names the piece around it. Its polygon for training is that piece's outline,
//! grown across each cut by an overlap, so joined letters share a band of ink and neither
//! ends in a flat face. A lasso loop becomes a polygon with few corners.

use kurbo::{BezPath, Line, PathEl, Point, Shape as _, Vec2};

use crate::outline::knife::slice_paths;
use crate::outline::path::Path;

/// How far a seed's polygon reaches past each of its cuts, in font units.
pub const OVERLAP: f64 = 24.0;

/// `contours` cut along every line in `cuts`, as closed paths.
pub fn pieces(contours: &[Path], cuts: &[Line]) -> Vec<BezPath> {
    let mut paths: Vec<Path> = contours.to_vec();
    for cut in cuts {
        paths = slice_paths(&paths, *cut);
    }
    paths.iter().map(Path::to_bezpath).collect()
}

/// The piece of `pieces` that holds `seed`: the smallest, so a hole's own outline wins
/// over the ring around it.
pub fn piece_at(pieces: &[BezPath], seed: Point) -> Option<usize> {
    pieces
        .iter()
        .enumerate()
        .filter(|(_, piece)| piece.contains(seed))
        .min_by(|a, b| a.1.area().abs().total_cmp(&b.1.area().abs()))
        .map(|(index, _)| index)
}

/// The training polygon of the piece around `seed`: its outline, with every cut pushed
/// `overlap` away from the seed first, so the polygon reaches into the neighbor's ink.
///
/// None when no piece holds the seed.
pub fn seed_polygon(
    contours: &[Path],
    cuts: &[Line],
    seed: Point,
    overlap: f64,
) -> Option<Vec<[f64; 2]>> {
    let grown: Vec<Line> = cuts
        .iter()
        .map(|cut| {
            let along = cut.p1 - cut.p0;
            if along.hypot() < 1e-9 {
                return *cut;
            }
            let normal = Vec2::new(-along.y, along.x).normalize();
            // Push the cut toward the side the seed is not on.
            let side = (seed - cut.p0).dot(normal);
            let shift = if side >= 0.0 { -normal } else { normal } * overlap;
            Line::new(cut.p0 + shift, cut.p1 + shift)
        })
        .collect();
    let pieces = pieces(contours, &grown);
    let index = piece_at(&pieces, seed)?;
    Some(flatten(&pieces[index], 1.0))
}

/// A closed path as a polygon, flattened to within `tolerance` font units.
pub fn flatten(path: &BezPath, tolerance: f64) -> Vec<[f64; 2]> {
    let mut corners: Vec<[f64; 2]> = Vec::new();
    kurbo::flatten(
        path.elements().iter().copied(),
        tolerance,
        |element| match element {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => corners.push([p.x, p.y]),
            _ => {}
        },
    );
    if corners.len() > 1 && corners.first() == corners.last() {
        corners.pop();
    }
    corners
}

/// A freehand loop as a polygon with few corners (Ramer-Douglas-Peucker).
///
/// The tolerance grows until the polygon has at most `max_corners`, and the result keeps
/// at least three corners.
pub fn simplify_loop(points: &[Point], tolerance: f64, max_corners: usize) -> Vec<Point> {
    if points.len() <= 3 {
        return points.to_vec();
    }
    let mut tolerance = tolerance.max(1e-3);
    loop {
        let mut keep = vec![false; points.len()];
        keep[0] = true;
        keep[points.len() - 1] = true;
        douglas_peucker(points, 0, points.len() - 1, tolerance, &mut keep);
        let mut corners: Vec<Point> = points
            .iter()
            .zip(&keep)
            .filter(|(_, keep)| **keep)
            .map(|(p, _)| *p)
            .collect();
        // The loop closes itself: a last corner on top of the first is noise.
        if corners.len() > 3 && corners[0].distance(corners[corners.len() - 1]) <= tolerance {
            corners.pop();
        }
        if corners.len() <= max_corners.max(3) || tolerance > 1e6 {
            return corners;
        }
        tolerance *= 1.5;
    }
}

fn douglas_peucker(points: &[Point], first: usize, last: usize, tolerance: f64, keep: &mut [bool]) {
    if last <= first + 1 {
        return;
    }
    let line = Line::new(points[first], points[last]);
    let (mut farthest, mut distance) = (first, 0.0);
    for (index, point) in points.iter().enumerate().take(last).skip(first + 1) {
        let d = distance_to_segment(*point, line);
        if d > distance {
            distance = d;
            farthest = index;
        }
    }
    if distance > tolerance {
        keep[farthest] = true;
        douglas_peucker(points, first, farthest, tolerance, keep);
        douglas_peucker(points, farthest, last, tolerance, keep);
    }
}

fn distance_to_segment(point: Point, line: Line) -> f64 {
    let along = line.p1 - line.p0;
    let length2 = along.dot(along);
    if length2 < 1e-12 {
        return point.distance(line.p0);
    }
    let t = ((point - line.p0).dot(along) / length2).clamp(0.0, 1.0);
    point.distance(line.p0 + along * t)
}

/// The nearest point on `line` to `point`, and its distance.
pub fn nearest_on_segment(point: Point, line: Line) -> (Point, f64) {
    let along = line.p1 - line.p0;
    let length2 = along.dot(along);
    let t = if length2 < 1e-12 {
        0.0
    } else {
        ((point - line.p0).dot(along) / length2).clamp(0.0, 1.0)
    };
    let at = line.p0 + along * t;
    (at, point.distance(at))
}

/// Whether a freehand drag is a cut: it starts and ends outside the ink and crosses it.
pub fn is_cut_gesture(points: &[Point], ink: &[BezPath]) -> bool {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return false;
    };
    let inside = |p: &Point| ink.iter().any(|contour| contour.contains(*p));
    !inside(first) && !inside(last) && points.iter().any(inside)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::path::Path;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Path {
        let mut contour = norad::Contour::default();
        for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] {
            contour.points.push(norad::ContourPoint::new(
                x,
                y,
                norad::PointType::Line,
                false,
                None,
                None,
            ));
        }
        Path::from_contour(&crate::outline::path::hyper_model::Contour::from_norad(
            &contour,
        ))
    }

    #[test]
    fn a_cut_splits_a_bar_and_a_seed_names_one_side() {
        let bar = rect(0.0, 0.0, 300.0, 100.0);
        let cut = Line::new(Point::new(100.0, -20.0), Point::new(100.0, 120.0));
        let pieces = pieces(std::slice::from_ref(&bar), &[cut]);
        assert_eq!(pieces.len(), 2);
        let left = piece_at(&pieces, Point::new(50.0, 50.0)).unwrap();
        let right = piece_at(&pieces, Point::new(200.0, 50.0)).unwrap();
        assert_ne!(left, right);
        assert!(piece_at(&pieces, Point::new(50.0, 500.0)).is_none());

        // the right piece grows 20 units past the cut into the left one
        let polygon = seed_polygon(&[bar], &[cut], Point::new(200.0, 50.0), 20.0).unwrap();
        let min_x = polygon.iter().map(|p| p[0]).fold(f64::MAX, f64::min);
        assert!((min_x - 80.0).abs() < 1e-6, "{min_x}");
    }

    #[test]
    fn a_loop_simplifies_to_few_corners_and_a_drag_across_ink_is_a_cut() {
        let circle: Vec<Point> = (0..200)
            .map(|i| {
                let t = f64::from(i) / 200.0 * std::f64::consts::TAU;
                Point::new(100.0 * t.cos(), 100.0 * t.sin())
            })
            .collect();
        let corners = simplify_loop(&circle, 2.0, 12);
        assert!(
            corners.len() >= 3 && corners.len() <= 12,
            "{}",
            corners.len()
        );

        let ink = vec![rect(0.0, 0.0, 300.0, 100.0).to_bezpath()];
        let across = [
            Point::new(150.0, -10.0),
            Point::new(150.0, 50.0),
            Point::new(150.0, 110.0),
        ];
        assert!(is_cut_gesture(&across, &ink));
        let inside = [Point::new(10.0, 50.0), Point::new(150.0, 50.0)];
        assert!(!is_cut_gesture(&inside, &ink));
    }
}
