// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

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

/// Representation-neutral point input for canonical and compatibility drags.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PointState {
    pub(crate) position: kurbo::Point,
    pub(crate) off_curve: bool,
    pub(crate) smooth: bool,
}

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

/// The index `d` places from `index` around a contour of `len`
/// points.
///
/// A closed contour wraps. An open one stops at either end and
/// returns `None` past it.
pub(crate) fn step_index(index: usize, len: usize, closed: bool, d: isize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let offset = d.unsigned_abs();
    if d >= 0 {
        let j = index.checked_add(offset)?;
        if closed {
            Some(j % len)
        } else {
            (j < len).then_some(j)
        }
    } else if closed {
        Some((index + len - offset % len) % len)
    } else {
        index.checked_sub(offset)
    }
}

/// `step_index` on a contour known to be closed and non-empty.
pub(crate) fn wrap_index(index: usize, len: usize, d: isize) -> usize {
    step_index(index, len, true, d).unwrap_or(0)
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
    for d in [-1_isize, 1] {
        let Some(on_index) = step_index(index, len, closed, d) else {
            continue;
        };
        if is_off(&points[on_index]) || !points[on_index].smooth {
            continue;
        }
        let Some(opposite) = step_index(on_index, len, closed, d) else {
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

/// Point indices whose current positions a persistent drag must preserve.
pub(crate) fn affected_indices(
    points: &[PointState],
    selected: &HashSet<usize>,
    closed: bool,
    independent: bool,
) -> Vec<usize> {
    let mut affected = Vec::new();
    let push = |affected: &mut Vec<usize>, index| {
        if !affected.contains(&index) {
            affected.push(index);
        }
    };
    for index in 0..points.len() {
        if !selected.contains(&index) {
            continue;
        }
        push(&mut affected, index);
        if !independent && !points[index].off_curve {
            for direction in [-1_isize, 1] {
                if let Some(neighbor) = step_index(index, points.len(), closed, direction)
                    && points[neighbor].off_curve
                {
                    push(&mut affected, neighbor);
                }
            }
        }
        if !points[index].off_curve {
            continue;
        }
        for direction in [-1_isize, 1] {
            let Some(anchor) = step_index(index, points.len(), closed, direction) else {
                continue;
            };
            if points[anchor].off_curve || !points[anchor].smooth {
                continue;
            }
            let Some(opposite) = step_index(anchor, points.len(), closed, direction) else {
                continue;
            };
            if !selected.contains(&opposite) && points[opposite].off_curve {
                push(&mut affected, opposite);
            }
        }
    }
    affected
}

fn moved_indices(
    points: &[PointState],
    selected: &HashSet<usize>,
    closed: bool,
    independent: bool,
) -> Vec<usize> {
    let mut moved = Vec::new();
    for &index in &affected_indices(points, selected, closed, independent) {
        if selected.contains(&index)
            || (!independent
                && points[index].off_curve
                && [-1_isize, 1].into_iter().any(|direction| {
                    step_index(index, points.len(), closed, direction).is_some_and(|neighbor| {
                        selected.contains(&neighbor) && !points[neighbor].off_curve
                    })
                }))
        {
            moved.push(index);
        }
    }
    moved
}

/// Calculate snapped point positions for one contour during a selection drag.
pub(crate) fn translated_positions(
    points: &[PointState],
    selected: &HashSet<usize>,
    originals: &HashMap<usize, kurbo::Point>,
    delta: (f64, f64),
    closed: bool,
    independent: bool,
) -> Vec<(usize, kurbo::Point)> {
    let mut result = points.to_vec();
    let moved = moved_indices(points, selected, closed, independent);
    for &index in &moved {
        let base = originals
            .get(&index)
            .copied()
            .unwrap_or(points[index].position);
        result[index].position = snap_pt(base + kurbo::Vec2::new(delta.0, delta.1));
    }
    let mut smooth_updates = Vec::new();
    for &index in &moved {
        if !selected.contains(&index) || !result[index].off_curve {
            continue;
        }
        for direction in [-1_isize, 1] {
            let Some(anchor_index) = step_index(index, result.len(), closed, direction) else {
                continue;
            };
            if result[anchor_index].off_curve || !result[anchor_index].smooth {
                continue;
            }
            let Some(opposite) = step_index(anchor_index, result.len(), closed, direction) else {
                continue;
            };
            if selected.contains(&opposite) {
                continue;
            }
            let update = if result[opposite].off_curve {
                let opposite_position = if moved.contains(&opposite) {
                    result[opposite].position
                } else {
                    originals
                        .get(&opposite)
                        .copied()
                        .unwrap_or(result[opposite].position)
                };
                mirrored_smooth_handle(
                    result[index].position,
                    result[anchor_index].position,
                    opposite_position,
                )
                .map(|position| (opposite, position))
            } else {
                projected_smooth_handle(
                    result[index].position,
                    result[anchor_index].position,
                    result[opposite].position,
                )
                .map(|position| (index, position))
            };
            if let Some(update) = update {
                smooth_updates.push(update);
            }
        }
    }
    for (index, position) in smooth_updates {
        result[index].position = position;
    }
    result
        .iter()
        .zip(points)
        .enumerate()
        .filter(|(_, (after, before))| after.position != before.position)
        .map(|(index, (after, _))| (index, after.position))
        .collect()
}

/// Capture every point position needed to replay total-delta drag events.
pub fn drag_origins(
    glyph: &Glyph,
    selected: &HashSet<PointId>,
    independent: bool,
) -> HashMap<PointId, (f64, f64)> {
    let mut origins = HashMap::new();
    for (contour_index, contour) in glyph.contours.iter().enumerate() {
        let selected_here: HashSet<_> = selected
            .iter()
            .filter(|(candidate, _)| *candidate == contour_index)
            .map(|(_, point)| *point)
            .filter(|point| *point < contour.points.len())
            .collect();
        if selected_here.is_empty() {
            continue;
        }
        let states: Vec<_> = contour
            .points
            .iter()
            .map(|point| PointState {
                position: pos(point),
                off_curve: is_off(point),
                smooth: point.smooth,
            })
            .collect();
        for index in affected_indices(
            &states,
            &selected_here,
            contour_is_closed(&contour.points),
            independent,
        ) {
            origins.insert(
                (contour_index, index),
                (contour.points[index].x, contour.points[index].y),
            );
        }
    }
    origins
}

/// Move the selected points by `delta`.
///
/// `originals` gives every directly or smoothly affected position captured by
/// [`drag_origins`]. A nonempty incomplete map rejects the operation. A keyboard
/// nudge can pass an empty map.
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
    if !originals.is_empty() {
        for (contour_index, contour) in glyph.contours.iter().enumerate() {
            let selected_here: HashSet<_> = selected
                .iter()
                .filter(|(candidate, _)| *candidate == contour_index)
                .map(|(_, point)| *point)
                .filter(|point| *point < contour.points.len())
                .collect();
            if selected_here.is_empty() {
                continue;
            }
            let states: Vec<_> = contour
                .points
                .iter()
                .map(|point| PointState {
                    position: pos(point),
                    off_curve: is_off(point),
                    smooth: point.smooth,
                })
                .collect();
            if affected_indices(
                &states,
                &selected_here,
                contour_is_closed(&contour.points),
                independent,
            )
            .into_iter()
            .any(|index| !originals.contains_key(&(contour_index, index)))
            {
                return false;
            }
        }
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
        let states: Vec<_> = contour
            .points
            .iter()
            .map(|point| PointState {
                position: pos(point),
                off_curve: is_off(point),
                smooth: point.smooth,
            })
            .collect();
        let originals: HashMap<_, _> = originals
            .iter()
            .filter(|((contour, _), _)| *contour == ci)
            .map(|((_, index), &(x, y))| (*index, kurbo::Point::new(x, y)))
            .collect();
        for (index, p) in translated_positions(
            &states,
            &selected_here,
            &originals,
            delta,
            contour_is_closed(&contour.points),
            independent,
        ) {
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
    fn captured_origins_anchor_carried_handles_across_snapping_thresholds() {
        let mut original = curve_glyph();
        original.contours[0].points[2].x = 101.0;
        original.contours[0].points[4].x = 101.0;
        let selected: HashSet<PointId> = [(0, 3)].into_iter().collect();
        let originals = drag_origins(&original, &selected, false);
        assert_eq!(originals.len(), 3);
        let mut one_event = original.clone();
        translate_points(&mut one_event, &selected, &originals, (2.0, 0.0), false);
        let mut two_events = original;
        translate_points(&mut two_events, &selected, &originals, (1.0, 0.0), false);
        translate_points(&mut two_events, &selected, &originals, (2.0, 0.0), false);
        assert_eq!(two_events, one_event);
        assert_eq!(at(&two_events, 2), (104.0, 20.0));
        assert_eq!(at(&two_events, 3), (102.0, 100.0));
        assert_eq!(at(&two_events, 4), (104.0, 180.0));
    }

    #[test]
    fn persistent_drag_rejects_incomplete_origins() {
        let mut glyph = curve_glyph();
        let before = glyph.clone();
        let selected: HashSet<PointId> = [(0, 3)].into_iter().collect();
        let incomplete = [((0, 3), (100.0, 100.0))].into_iter().collect();
        assert!(!translate_points(
            &mut glyph,
            &selected,
            &incomplete,
            (2.0, 0.0),
            false,
        ));
        assert_eq!(glyph, before);
    }

    #[test]
    fn captured_origins_anchor_smooth_opposites_across_drag_events() {
        let mut original = curve_glyph();
        original.contours[0].points[2].x = 101.0;
        original.contours[0].points[2].y = 19.0;
        original.contours[0].points[4].x = 101.0;
        original.contours[0].points[4].y = 181.0;
        let selected: HashSet<PointId> = [(0, 4)].into_iter().collect();
        let originals = drag_origins(&original, &selected, false);
        assert_eq!(originals.len(), 2);
        let mut one_event = original.clone();
        translate_points(&mut one_event, &selected, &originals, (3.0, 1.0), false);
        let mut two_events = original;
        translate_points(&mut two_events, &selected, &originals, (1.0, 0.0), false);
        translate_points(&mut two_events, &selected, &originals, (3.0, 1.0), false);
        assert_eq!(two_events, one_event);
    }

    #[test]
    fn unrelated_selected_handle_does_not_carry_a_smooth_opposite() {
        let original = curve_glyph();
        let primary: HashSet<PointId> = [(0, 4)].into_iter().collect();
        let multiple: HashSet<PointId> = [(0, 1), (0, 4)].into_iter().collect();
        let mut primary_result = original.clone();
        let primary_origins = drag_origins(&primary_result, &primary, false);
        translate_points(
            &mut primary_result,
            &primary,
            &primary_origins,
            (20.0, 0.0),
            false,
        );
        let mut multiple_result = original;
        let multiple_origins = drag_origins(&multiple_result, &multiple, false);
        translate_points(
            &mut multiple_result,
            &multiple,
            &multiple_origins,
            (20.0, 0.0),
            false,
        );
        assert_eq!(at(&multiple_result, 2), at(&primary_result, 2));
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
