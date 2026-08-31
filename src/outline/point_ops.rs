// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Moving selected points, ported from the web editor's select-tool
//! translate (`translate_and_snap_in_path_with_handles` and friends in
//! runebender-web's `core/src/editor.rs`).
//!
//! Three rules make a drag feel right, and all three live here:
//!
//! 1. A selected on-curve point carries its adjacent off-curve
//!    handles, so a curve keeps its shape instead of collapsing.
//! 2. A dragged handle keeps its smooth neighbour's tangent: the
//!    opposite handle is mirrored through the smooth point (its length
//!    is preserved), and when the opposite side is an on-curve point
//!    the dragged handle is projected onto that tangent line instead.
//! 3. Every moved point lands on the 2-unit design grid.
//!
//! Alt/Option editing passes `independent`, which turns rule 1 off:
//! the selected points move alone.

use std::collections::{HashMap, HashSet};

use norad::{ContourPoint, Glyph, PointType};

use crate::outline::glyph_ops::PointId;

/// The design grid every moved point snaps to.
///
/// This is `DESIGN_GRID_SPACING` in the web editor.
pub const DESIGN_GRID_SPACING: f64 = 2.0;

/// Snap one coordinate to the design grid.
pub fn snap_coord(value: f64) -> f64 {
    (value / DESIGN_GRID_SPACING).round() * DESIGN_GRID_SPACING
}

fn snap_pt(p: kurbo::Point) -> kurbo::Point {
    kurbo::Point::new(snap_coord(p.x), snap_coord(p.y))
}

fn is_off(p: &ContourPoint) -> bool {
    p.typ == PointType::OffCurve
}

fn pos(p: &ContourPoint) -> kurbo::Point {
    kurbo::Point::new(p.x, p.y)
}

/// Neighbour index in a contour, respecting open contours.
fn step(index: usize, len: usize, closed: bool, d: isize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let j = index as isize + d;
    if closed {
        Some(((j % len as isize + len as isize) % len as isize) as usize)
    } else if (0..len as isize).contains(&j) {
        Some(j as usize)
    } else {
        None
    }
}

/// A closed contour is one that does not open with a `move` point.
fn contour_is_closed(points: &[ContourPoint]) -> bool {
    points.first().is_none_or(|p| p.typ != PointType::Move)
}

/// The opposite handle mirrored through `anchor`, keeping its own
/// length.
///
/// This is `mirrored_smooth_handle` in the web editor.
fn mirrored_smooth_handle(
    moved: kurbo::Point,
    anchor: kurbo::Point,
    opposite: kurbo::Point,
) -> Option<kurbo::Point> {
    let v = moved - anchor;
    let len = v.hypot();
    if len < 1e-9 {
        return None;
    }
    let opposite_len = (opposite - anchor).hypot();
    if opposite_len < 1e-9 {
        return None;
    }
    Some(snap_pt(anchor - (v / len) * opposite_len))
}

/// The dragged handle projected onto the tangent through an
/// on-curve neighbour.
///
/// Only axis-aligned tangents snap: snapping a diagonal would pull
/// the handle off the line the projection just put it on. This is
/// `projected_smooth_handle` in the web editor.
fn projected_smooth_handle(
    moved: kurbo::Point,
    anchor: kurbo::Point,
    line_point: kurbo::Point,
) -> Option<kurbo::Point> {
    let tangent = anchor - line_point;
    let tangent_len = tangent.hypot();
    if tangent_len < 1e-9 {
        return None;
    }
    let unit = tangent / tangent_len;
    let d = moved - anchor;
    let distance = (d.x * unit.x + d.y * unit.y).abs();
    let projected = anchor + unit * distance;
    if tangent.x.abs() < 1e-9 || tangent.y.abs() < 1e-9 {
        Some(snap_pt(projected))
    } else {
        Some(projected)
    }
}

/// Handle updates a moved off-curve point forces on its smooth
/// neighbours.
///
/// This is `append_smooth_handle_updates` in the web editor.
fn smooth_handle_updates(
    points: &[ContourPoint],
    selected_here: &HashSet<usize>,
    closed: bool,
    index: usize,
    updates: &mut Vec<(usize, kurbo::Point)>,
) {
    let len = points.len();
    for d in [-1isize, 1] {
        let Some(on_index) = step(index, len, closed, d) else {
            continue;
        };
        if is_off(&points[on_index]) || !points[on_index].smooth {
            continue;
        }
        let Some(opposite) = step(on_index, len, closed, d) else {
            continue;
        };
        if selected_here.contains(&opposite) {
            continue;
        }
        if is_off(&points[opposite]) {
            if let Some(p) = mirrored_smooth_handle(
                pos(&points[index]),
                pos(&points[on_index]),
                pos(&points[opposite]),
            ) {
                updates.push((opposite, p));
            }
        } else if let Some(p) = projected_smooth_handle(
            pos(&points[index]),
            pos(&points[on_index]),
            pos(&points[opposite]),
        ) {
            updates.push((index, p));
        }
    }
}

/// The indices this contour moves for a selection.
///
/// These are the selected points, plus each selected on-curve
/// point's adjacent handles unless `independent` is set. This is
/// `selected_and_adjacent_handle_indices` in the web editor.
fn move_indices(
    points: &[ContourPoint],
    selected_here: &HashSet<usize>,
    closed: bool,
    independent: bool,
) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    let push = |out: &mut Vec<usize>, i: usize| {
        if !out.contains(&i) {
            out.push(i);
        }
    };
    for index in 0..points.len() {
        if !selected_here.contains(&index) {
            continue;
        }
        push(&mut out, index);
        if independent || is_off(&points[index]) {
            continue;
        }
        for d in [-1isize, 1] {
            if let Some(nb) = step(index, points.len(), closed, d)
                && is_off(&points[nb])
            {
                push(&mut out, nb);
            }
        }
    }
    out
}

/// Move the selected points by `delta`.
///
/// `originals` gives the positions a drag started from, keyed by point
/// address; addresses missing from it move from where they are now, so
/// a keyboard nudge can pass an empty map. Passing drag-start
/// positions is what keeps a long drag free of rounding drift.
///
/// Returns true when any coordinate changed.
pub fn translate_points(
    glyph: &mut Glyph,
    selected: &HashSet<PointId>,
    originals: &HashMap<PointId, (f64, f64)>,
    delta: (f64, f64),
    independent: bool,
) -> bool {
    if selected.is_empty() {
        return false;
    }
    let mut changed = false;
    for (ci, contour) in glyph.contours.iter_mut().enumerate() {
        let selected_here: HashSet<usize> = selected
            .iter()
            .filter(|(c, _)| *c == ci)
            .map(|(_, i)| *i)
            .filter(|i| *i < contour.points.len())
            .collect();
        if selected_here.is_empty() {
            continue;
        }
        let closed = contour_is_closed(&contour.points);
        let moved = move_indices(&contour.points, &selected_here, closed, independent);
        for &index in &moved {
            let base = originals
                .get(&(ci, index))
                .copied()
                .unwrap_or_else(|| (contour.points[index].x, contour.points[index].y));
            let target = snap_pt(kurbo::Point::new(base.0 + delta.0, base.1 + delta.1));
            let point = &mut contour.points[index];
            if point.x != target.x || point.y != target.y {
                point.x = target.x;
                point.y = target.y;
                changed = true;
            }
        }
        // Only handles the user actually grabbed re-aim their smooth
        // neighbours; handles that came along for the ride with an
        // on-curve point already moved rigidly.
        let mut updates: Vec<(usize, kurbo::Point)> = Vec::new();
        for &index in &moved {
            if selected_here.contains(&index) && is_off(&contour.points[index]) {
                smooth_handle_updates(&contour.points, &selected_here, closed, index, &mut updates);
            }
        }
        for (index, p) in updates {
            let point = &mut contour.points[index];
            if point.x != p.x || point.y != p.y {
                point.x = p.x;
                point.y = p.y;
                changed = true;
            }
        }
    }
    changed
}

/// Snap the selected off-curve points onto the design grid, then
/// re-aim the smooth tangents they belong to. The web select tool runs
/// this when a drag ends, so handles never settle between gridlines.
pub fn snap_selected_offcurves(glyph: &mut Glyph, selected: &HashSet<PointId>) -> bool {
    if selected.is_empty() {
        return false;
    }
    let mut changed = false;
    for (ci, contour) in glyph.contours.iter_mut().enumerate() {
        let selected_here: HashSet<usize> = selected
            .iter()
            .filter(|(c, _)| *c == ci)
            .map(|(_, i)| *i)
            .filter(|i| *i < contour.points.len())
            .collect();
        if selected_here.is_empty() {
            continue;
        }
        let closed = contour_is_closed(&contour.points);
        let mut snapped_any = false;
        for &index in &selected_here {
            if !is_off(&contour.points[index]) {
                continue;
            }
            let p = snap_pt(pos(&contour.points[index]));
            let point = &mut contour.points[index];
            if point.x != p.x || point.y != p.y {
                point.x = p.x;
                point.y = p.y;
                changed = true;
                snapped_any = true;
            }
        }
        if !snapped_any {
            continue;
        }
        let mut updates: Vec<(usize, kurbo::Point)> = Vec::new();
        for &index in &selected_here {
            if is_off(&contour.points[index]) {
                smooth_handle_updates(&contour.points, &selected_here, closed, index, &mut updates);
            }
        }
        for (index, p) in updates {
            let point = &mut contour.points[index];
            if point.x != p.x || point.y != p.y {
                point.x = p.x;
                point.y = p.y;
                changed = true;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use norad::Contour;

    /// A closed contour: corner, handle, handle, smooth on-curve,
    /// handle, handle. Enough shape to test tangents.
    fn curve_glyph() -> Glyph {
        let p = |x: f64, y: f64, typ: PointType, smooth: bool| {
            ContourPoint::new(x, y, typ, smooth, None, None)
        };
        let points = vec![
            p(0.0, 0.0, PointType::Curve, false),
            p(20.0, 0.0, PointType::OffCurve, false),
            p(100.0, 20.0, PointType::OffCurve, false),
            p(100.0, 100.0, PointType::Curve, true),
            p(100.0, 180.0, PointType::OffCurve, false),
            p(20.0, 200.0, PointType::OffCurve, false),
            p(0.0, 200.0, PointType::Curve, false),
            p(-20.0, 100.0, PointType::OffCurve, false),
            p(-20.0, 50.0, PointType::OffCurve, false),
        ];
        let mut glyph = Glyph::new("test");
        glyph.contours.push(Contour::new(points, None));
        glyph
    }

    fn at(glyph: &Glyph, index: usize) -> (f64, f64) {
        let p = &glyph.contours[0].points[index];
        (p.x, p.y)
    }

    #[test]
    fn on_curve_point_carries_its_handles() {
        let mut glyph = curve_glyph();
        let selected: HashSet<PointId> = [(0, 3)].into_iter().collect();
        assert!(translate_points(
            &mut glyph,
            &selected,
            &HashMap::new(),
            (10.0, 0.0),
            false
        ));
        assert_eq!(at(&glyph, 3), (110.0, 100.0));
        // Both adjacent handles moved with it.
        assert_eq!(at(&glyph, 2), (110.0, 20.0));
        assert_eq!(at(&glyph, 4), (110.0, 180.0));
        // A point two steps away stayed put.
        assert_eq!(at(&glyph, 1), (20.0, 0.0));
    }

    #[test]
    fn independent_leaves_handles_behind() {
        let mut glyph = curve_glyph();
        let selected: HashSet<PointId> = [(0, 3)].into_iter().collect();
        translate_points(&mut glyph, &selected, &HashMap::new(), (10.0, 0.0), true);
        assert_eq!(at(&glyph, 3), (110.0, 100.0));
        assert_eq!(at(&glyph, 2), (100.0, 20.0));
        assert_eq!(at(&glyph, 4), (100.0, 180.0));
    }

    #[test]
    fn dragging_a_handle_mirrors_the_smooth_opposite() {
        let mut glyph = curve_glyph();
        // Point 4 is the handle after the smooth on-curve 3; its
        // opposite is point 2, 80 units away on the other side.
        let selected: HashSet<PointId> = [(0, 4)].into_iter().collect();
        translate_points(&mut glyph, &selected, &HashMap::new(), (0.0, 20.0), false);
        assert_eq!(at(&glyph, 4), (100.0, 200.0));
        // The opposite handle stays collinear through (100, 100) and
        // keeps its own length.
        assert_eq!(at(&glyph, 2), (100.0, 20.0));
    }

    #[test]
    fn positions_land_on_the_design_grid() {
        let mut glyph = curve_glyph();
        let selected: HashSet<PointId> = [(0, 0)].into_iter().collect();
        translate_points(&mut glyph, &selected, &HashMap::new(), (3.0, 3.0), true);
        assert_eq!(at(&glyph, 0), (4.0, 4.0));
    }

    #[test]
    fn originals_anchor_a_long_drag() {
        let mut glyph = curve_glyph();
        let selected: HashSet<PointId> = [(0, 0)].into_iter().collect();
        let originals: HashMap<PointId, (f64, f64)> = [((0, 0), (0.0, 0.0))].into_iter().collect();
        // Two drag events from the same start: the second wins outright
        // instead of stacking on the first.
        translate_points(&mut glyph, &selected, &originals, (10.0, 0.0), true);
        translate_points(&mut glyph, &selected, &originals, (20.0, 0.0), true);
        assert_eq!(at(&glyph, 0), (20.0, 0.0));
    }

    #[test]
    fn snapping_offcurves_moves_only_handles() {
        let mut glyph = curve_glyph();
        glyph.contours[0].points[1].x = 21.0;
        glyph.contours[0].points[0].x = 1.0;
        let selected: HashSet<PointId> = [(0, 0), (0, 1)].into_iter().collect();
        assert!(snap_selected_offcurves(&mut glyph, &selected));
        assert_eq!(at(&glyph, 1), (22.0, 0.0));
        assert_eq!(at(&glyph, 0), (1.0, 0.0));
    }
}
