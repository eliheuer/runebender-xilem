// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! UI-free editing operations on norad glyphs, shared by all
//! Runebender editors. Everything here takes plain norad types; the
//! UI shells own caching, selection state, and rendering.
//!
//! Point addressing: `(contour_index, point_index)` pairs into
//! `glyph.contours[c].points[p]`.

use std::collections::{HashMap, HashSet};

use kurbo::BezPath;
use norad::{Contour, ContourPoint, Font, Glyph, PointType};

use crate::glyph_paths;

/// A point address: (contour index, point index).
pub type PointId = (usize, usize);
/// A batch of point moves: (address, new position) pairs.
pub type PointUpdates = [(PointId, (f64, f64))];

/// One undo step: a glyph's full editable state.
#[derive(Clone)]
pub struct GlyphSnapshot {
    pub contours: Vec<Contour>,
    pub components: Vec<norad::Component>,
    pub anchors: Vec<norad::Anchor>,
    pub width: f64,
}

/// Clone a glyph's editable state for undo snapshots.
pub fn snapshot(glyph: &Glyph) -> GlyphSnapshot {
    GlyphSnapshot {
        contours: glyph.contours.clone(),
        components: glyph.components.clone(),
        anchors: glyph.anchors.clone(),
        width: glyph.width,
    }
}

/// Replace a glyph's editable state (undo/redo).
pub fn restore(glyph: &mut Glyph, snapshot: GlyphSnapshot) {
    glyph.contours = snapshot.contours;
    glyph.components = snapshot.components;
    glyph.anchors = snapshot.anchors;
    glyph.width = snapshot.width;
}

/// Set several points at once (multi-point drag).
pub fn set_points(glyph: &mut Glyph, updates: &PointUpdates) {
    for ((contour, index), (x, y)) in updates {
        if let Some(point) = glyph
            .contours
            .get_mut(*contour)
            .and_then(|c| c.points.get_mut(*index))
        {
            point.x = *x;
            point.y = *y;
        }
    }
}

/// Transform the selected points (all points when the selection is
/// empty) about the center of their bounding box. `transform` is
/// applied in a coordinate frame centered on that box, so flips and
/// rotations stay in place. Returns false if the glyph has no points.
pub fn transform_selection(
    glyph: &mut Glyph,
    selected: &HashSet<PointId>,
    transform: kurbo::Affine,
) -> bool {
    let targeted = |c: usize, p: usize| selected.is_empty() || selected.contains(&(c, p));
    let mut min = (f64::INFINITY, f64::INFINITY);
    let mut max = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for (c, contour) in glyph.contours.iter().enumerate() {
        for (p, point) in contour.points.iter().enumerate() {
            if targeted(c, p) {
                min = (min.0.min(point.x), min.1.min(point.y));
                max = (max.0.max(point.x), max.1.max(point.y));
            }
        }
    }
    if !min.0.is_finite() {
        return false;
    }
    let center = ((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0);
    let about_center = kurbo::Affine::translate(center)
        * transform
        * kurbo::Affine::translate((-center.0, -center.1));
    for (c, contour) in glyph.contours.iter_mut().enumerate() {
        for (p, point) in contour.points.iter_mut().enumerate() {
            if targeted(c, p) {
                let moved = about_center * kurbo::Point::new(point.x, point.y);
                point.x = moved.x;
                point.y = moved.y;
            }
        }
    }
    true
}

/// Reverse the direction of every contour that has a selected point
/// (all contours when the selection is empty). Round-trips through
/// kurbo's `reverse_subpaths`, the same conversion remove-overlap
/// uses, so coordinates round to integers.
pub fn reverse_contours(glyph: &mut Glyph, selected: &HashSet<PointId>) -> bool {
    let mut changed = false;
    for c in 0..glyph.contours.len() {
        let targeted = selected.is_empty()
            || (0..glyph.contours[c].points.len()).any(|p| selected.contains(&(c, p)));
        if !targeted || glyph.contours[c].points.is_empty() {
            continue;
        }
        let smooth_at: HashMap<(i64, i64), bool> = glyph.contours[c]
            .points
            .iter()
            .filter(|p| p.typ != PointType::OffCurve)
            .map(|p| ((p.x.round() as i64, p.y.round() as i64), p.smooth))
            .collect();
        let path = glyph_paths::contour_to_bezpath(&glyph.contours[c]).reverse_subpaths();
        if let Some(reversed) = bezpath_to_contour(&path, &smooth_at) {
            glyph.contours[c] = reversed;
            changed = true;
        }
    }
    changed
}

/// After moving one off-curve handle, keep its sibling handle
/// collinear through the shared smooth on-curve point (length
/// preserved). No-op when the shared point is a corner.
pub fn constrain_smooth_neighbor(glyph: &mut Glyph, contour: usize, index: usize) {
    let Some(c) = glyph.contours.get_mut(contour) else {
        return;
    };
    let n = c.points.len();
    if n < 4 {
        return;
    }
    let closed = c.points.first().is_none_or(|p| p.typ != PointType::Move);
    let step = |i: usize, d: isize| -> Option<usize> {
        let j = i as isize + d;
        if closed {
            Some(((j % n as isize + n as isize) % n as isize) as usize)
        } else if (0..n as isize).contains(&j) {
            Some(j as usize)
        } else {
            None
        }
    };
    let is_off = |p: &ContourPoint| p.typ == PointType::OffCurve;
    if !is_off(&c.points[index]) {
        return;
    }
    let arm = |d: isize| -> Option<(usize, usize)> {
        let (a, sib) = (step(index, d)?, step(index, 2 * d)?);
        (!is_off(&c.points[a]) && c.points[a].smooth && is_off(&c.points[sib]))
            .then_some((a, sib))
    };
    let Some((a, sib)) = arm(1).or_else(|| arm(-1)) else {
        return;
    };
    let anchor = kurbo::Point::new(c.points[a].x, c.points[a].y);
    let dragged = kurbo::Point::new(c.points[index].x, c.points[index].y);
    let sibling_pt = kurbo::Point::new(c.points[sib].x, c.points[sib].y);
    let dir = anchor - dragged;
    let len = dir.hypot();
    if len < 1e-6 {
        return;
    }
    let unit = dir / len;
    let sib_len = (sibling_pt - anchor).hypot();
    let new_sib = anchor + unit * sib_len;
    c.points[sib].x = new_sib.x.round();
    c.points[sib].y = new_sib.y.round();
}

/// Delete the given points. Selected on-curve points vanish with
/// their incoming controls (neighbors reconnect); selected off-curve
/// points turn their segment into a line. Contours left without
/// segments are removed. Returns true if anything changed.
pub fn delete_points(glyph: &mut Glyph, selected: &HashSet<PointId>) -> bool {
    if selected.is_empty() {
        return false;
    }
    let mut changed = false;
    let mut contour_index = 0usize;
    glyph.contours.retain_mut(|contour| {
        let ci = contour_index;
        contour_index += 1;
        let any_here = selected.iter().any(|(c, _)| *c == ci);
        if !any_here {
            return true;
        }
        changed = true;

        // Parse into segments anchored at on-curve points.
        struct Seg {
            x: f64,
            y: f64,
            smooth: bool,
            controls: Option<((f64, f64), (f64, f64))>,
            on_index: usize,
            control_indices: Vec<usize>,
        }
        let closed = contour
            .points
            .first()
            .is_none_or(|p| p.typ != PointType::Move);
        let mut segs: Vec<Seg> = Vec::new();
        let mut pending: Vec<(usize, (f64, f64))> = Vec::new();
        for (i, p) in contour.points.iter().enumerate() {
            match p.typ {
                PointType::OffCurve => pending.push((i, (p.x, p.y))),
                _ => {
                    let controls = if pending.len() == 2 {
                        Some((pending[0].1, pending[1].1))
                    } else {
                        None
                    };
                    segs.push(Seg {
                        x: p.x,
                        y: p.y,
                        smooth: p.smooth,
                        controls,
                        on_index: i,
                        control_indices: pending.iter().map(|(i, _)| *i).collect(),
                    });
                    pending.clear();
                }
            }
        }
        // Closed contours may carry trailing off-curves that wrap to
        // the first on-curve point.
        if closed && pending.len() == 2 && !segs.is_empty() {
            segs[0].controls = Some((pending[0].1, pending[1].1));
            segs[0].control_indices = pending.iter().map(|(i, _)| *i).collect();
        }

        // Apply the deletions.
        segs.retain(|seg| !selected.contains(&(ci, seg.on_index)));
        for seg in segs.iter_mut() {
            if seg
                .control_indices
                .iter()
                .any(|i| selected.contains(&(ci, *i)))
            {
                seg.controls = None;
            }
        }
        if segs.is_empty() {
            return false; // drop the contour
        }

        // Reserialize.
        let mut points: Vec<ContourPoint> = Vec::new();
        for (k, seg) in segs.iter().enumerate() {
            let is_first = k == 0;
            let controls = if !closed && is_first { None } else { seg.controls };
            let typ = if !closed && is_first {
                PointType::Move
            } else if controls.is_some() {
                PointType::Curve
            } else {
                PointType::Line
            };
            // For closed contours the wrap-around controls of the
            // first on-curve point go at the END of the list.
            if let (Some((c1, c2)), false) = (controls, closed && is_first) {
                points.push(off_point(c1));
                points.push(off_point(c2));
            }
            points.push(ContourPoint::new(seg.x, seg.y, typ, seg.smooth, None, None));
        }
        if closed {
            if let Some((c1, c2)) = segs[0].controls {
                points.push(off_point(c1));
                points.push(off_point(c2));
            }
            if let Some(first) = points.first_mut() {
                first.typ = if segs[0].controls.is_some() {
                    PointType::Curve
                } else {
                    PointType::Line
                };
            }
        }
        contour.points = points;
        true
    });
    changed
}

/// Toggle smooth/corner on the given on-curve points.
pub fn toggle_smooth(glyph: &mut Glyph, selected: &HashSet<PointId>) -> bool {
    let mut changed = false;
    for (ci, contour) in glyph.contours.iter_mut().enumerate() {
        for (pi, p) in contour.points.iter_mut().enumerate() {
            if p.typ != PointType::OffCurve && selected.contains(&(ci, pi)) {
                p.smooth = !p.smooth;
                changed = true;
            }
        }
    }
    changed
}

// ============================================================================
// PEN
// ============================================================================

fn off_point((x, y): (f64, f64)) -> ContourPoint {
    ContourPoint::new(x, y, PointType::OffCurve, false, None, None)
}

/// Start a new open contour at (x, y). Returns its index.
pub fn start_contour(glyph: &mut Glyph, x: f64, y: f64) -> usize {
    let point = ContourPoint::new(x, y, PointType::Move, false, None, None);
    glyph.contours.push(Contour::new(vec![point], None));
    glyph.contours.len() - 1
}

// ---- hyperbezier pen ----

/// Start a hyperbezier contour (identifier convention: the contour's
/// identifier contains "hyperbezier"; points are all on-curve, with
/// curve = smooth and line = corner; the spline solver draws it).
pub fn start_hyper_contour(glyph: &mut Glyph, x: f64, y: f64) -> usize {
    let point = ContourPoint::new(x, y, PointType::Move, false, None, None);
    glyph.contours.push(Contour::new(
        vec![point],
        Some(norad::Identifier::new("hyperbezier").expect("static id")),
    ));
    glyph.contours.len() - 1
}

/// Append an on-curve point to an open hyperbezier contour.
pub fn append_hyper_point(glyph: &mut Glyph, contour: usize, x: f64, y: f64, corner: bool) {
    let Some(c) = glyph.contours.get_mut(contour) else {
        return;
    };
    let typ = if corner {
        PointType::Line
    } else {
        PointType::Curve
    };
    c.points
        .push(ContourPoint::new(x, y, typ, !corner, None, None));
}

/// Close an open hyperbezier contour: the Move start becomes a
/// smooth hyper point.
pub fn close_hyper_contour(glyph: &mut Glyph, contour: usize) {
    let Some(c) = glyph.contours.get_mut(contour) else {
        return;
    };
    let Some(first) = c.points.first_mut() else {
        return;
    };
    if first.typ == PointType::Move {
        first.typ = PointType::Curve;
        first.smooth = true;
    }
}

/// Whether a glyph contour is a hyperbezier.
pub fn contour_is_hyper(glyph: &Glyph, contour: usize) -> bool {
    glyph
        .contours
        .get(contour)
        .map(crate::model::workspace::norad_contour_is_hyper)
        .unwrap_or(false)
}

/// Append a segment to an open contour (pen tool). Pass the two
/// off-curve controls for a curve segment, or none for a line.
pub fn append_segment(
    glyph: &mut Glyph,
    contour: usize,
    controls: Option<((f64, f64), (f64, f64))>,
    x: f64,
    y: f64,
    smooth: bool,
) {
    let Some(c) = glyph.contours.get_mut(contour) else {
        return;
    };
    let typ = if controls.is_some() {
        PointType::Curve
    } else {
        PointType::Line
    };
    if let Some((c1, c2)) = controls {
        c.points.push(off_point(c1));
        c.points.push(off_point(c2));
    }
    c.points.push(ContourPoint::new(x, y, typ, smooth, None, None));
}

/// Close an open contour: the Move start point becomes the final
/// segment's target. `controls` curves the closing segment.
pub fn close_contour(glyph: &mut Glyph, contour: usize, controls: Option<((f64, f64), (f64, f64))>) {
    let Some(c) = glyph.contours.get_mut(contour) else {
        return;
    };
    let Some(first) = c.points.first_mut() else {
        return;
    };
    if first.typ != PointType::Move {
        return;
    }
    // In UFO, a closed contour simply has no Move point: the final
    // segment wraps around to the first point.
    first.typ = if controls.is_some() {
        PointType::Curve
    } else {
        PointType::Line
    };
    if let Some((c1, c2)) = controls {
        c.points.push(off_point(c1));
        c.points.push(off_point(c2));
    }
}

/// Delete an unfinished pen contour (a single stray point).
pub fn remove_contour_if_degenerate(glyph: &mut Glyph, contour: usize) {
    if glyph
        .contours
        .get(contour)
        .is_some_and(|c| c.points.len() < 2)
    {
        glyph.contours.remove(contour);
    }
}

// ============================================================================
// SHAPES
// ============================================================================

/// Insert a rectangle or ellipse contour spanning `rect`.
pub fn add_shape_contour(glyph: &mut Glyph, rect: kurbo::Rect, ellipse: bool) {
    let on = |x: f64, y: f64, smooth: bool| {
        ContourPoint::new(x, y, PointType::Curve, smooth, None, None)
    };
    let line = |x: f64, y: f64| ContourPoint::new(x, y, PointType::Line, false, None, None);
    let points = if ellipse {
        let (cx, cy) = (rect.center().x, rect.center().y);
        let (rx, ry) = (rect.width() / 2.0, rect.height() / 2.0);
        let (kx, ky) = (rx * 0.5522847498, ry * 0.5522847498);
        let r = |v: f64| v.round();
        vec![
            on(r(cx + rx), r(cy), true), // right
            off_point((r(cx + rx), r(cy + ky))),
            off_point((r(cx + kx), r(cy + ry))),
            on(r(cx), r(cy + ry), true), // top
            off_point((r(cx - kx), r(cy + ry))),
            off_point((r(cx - rx), r(cy + ky))),
            on(r(cx - rx), r(cy), true), // left
            off_point((r(cx - rx), r(cy - ky))),
            off_point((r(cx - kx), r(cy - ry))),
            on(r(cx), r(cy - ry), true), // bottom
            off_point((r(cx + kx), r(cy - ry))),
            off_point((r(cx + rx), r(cy - ky))),
        ]
    } else {
        vec![
            line(rect.x0.round(), rect.y0.round()),
            line(rect.x1.round(), rect.y0.round()),
            line(rect.x1.round(), rect.y1.round()),
            line(rect.x0.round(), rect.y1.round()),
        ]
    };
    glyph.contours.push(Contour::new(points, None));
}

// ============================================================================
// COMPONENTS
// ============================================================================

/// Contours of a glyph's components, recursively resolved and
/// rounded to integer units.
pub fn resolved_component_contours(font: &Font, glyph: &Glyph) -> Vec<Contour> {
    fn collect(
        font: &Font,
        glyph: &Glyph,
        parent: kurbo::Affine,
        depth: u8,
        out: &mut Vec<Contour>,
    ) {
        if depth > 8 {
            return;
        }
        for component in &glyph.components {
            let Some(base) = font.get_glyph(&component.base) else {
                continue;
            };
            let affine = parent * glyph_paths::component_affine(&component.transform);
            for contour in &base.contours {
                let mut c = contour.clone();
                for p in c.points.iter_mut() {
                    let q = affine * kurbo::Point::new(p.x, p.y);
                    p.x = q.x.round();
                    p.y = q.y.round();
                }
                out.push(c);
            }
            collect(font, base, affine, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    collect(font, glyph, kurbo::Affine::IDENTITY, 0, &mut out);
    out
}

// ============================================================================
// REMOVE OVERLAP
// ============================================================================

/// Union all contours via linesweeper; smooth flags are restored on
/// points that kept their positions. Returns the new contours, or
/// None when the input is empty or the operation fails.
pub fn remove_overlap(glyph: &Glyph) -> Option<Vec<Contour>> {
    if glyph.contours.is_empty() {
        return None;
    }
    let combined = glyph_paths::contours_to_bezpath(glyph);
    let result = linesweeper::binary_op(
        &combined,
        &BezPath::new(),
        linesweeper::FillRule::NonZero,
        linesweeper::BinaryOp::Union,
    )
    .ok()?;
    let smooth_at: HashMap<(i64, i64), bool> = glyph
        .contours
        .iter()
        .flat_map(|c| c.points.iter())
        .filter(|p| p.typ != PointType::OffCurve)
        .map(|p| ((p.x.round() as i64, p.y.round() as i64), p.smooth))
        .collect();
    let mut new_contours: Vec<Contour> = Vec::new();
    for contour in result.contours() {
        if let Some(c) = bezpath_to_contour(&contour.path, &smooth_at) {
            new_contours.push(c);
        }
    }
    (!new_contours.is_empty()).then_some(new_contours)
}

/// Apply a boolean operation across the glyph's contours, like the
/// web editor: union merges everything; subtract/intersect/exclude
/// use the first contour as the left operand and the rest combined
/// as the right. Original smooth flags survive on points that keep
/// their positions.
pub fn boolean_contours(glyph: &Glyph, op: linesweeper::BinaryOp) -> Option<Vec<Contour>> {
    if glyph.contours.len() < 2 {
        return None;
    }
    let paths: Vec<BezPath> = glyph
        .contours
        .iter()
        .map(glyph_paths::contour_to_bezpath)
        .collect();
    let (set_a, set_b) = match op {
        linesweeper::BinaryOp::Union => {
            let mut combined = BezPath::new();
            for path in &paths {
                combined.extend(path.elements().iter().copied());
            }
            (combined, BezPath::new())
        }
        _ => {
            let mut iter = paths.into_iter();
            let set_a = iter.next()?;
            let mut rest = BezPath::new();
            for path in iter {
                rest.extend(path.elements().iter().copied());
            }
            (set_a, rest)
        }
    };
    let result =
        linesweeper::binary_op(&set_a, &set_b, linesweeper::FillRule::NonZero, op).ok()?;
    let smooth_at: HashMap<(i64, i64), bool> = glyph
        .contours
        .iter()
        .flat_map(|c| c.points.iter())
        .filter(|p| p.typ != PointType::OffCurve)
        .map(|p| ((p.x.round() as i64, p.y.round() as i64), p.smooth))
        .collect();
    let mut contours: Vec<Contour> = Vec::new();
    for contour in result.contours() {
        if let Some(c) = bezpath_to_contour(&contour.path, &smooth_at) {
            contours.push(c);
        }
    }
    (!contours.is_empty()).then_some(contours)
}

/// Make the given on-curve point a closed contour's start point (the
/// contour context menu's "set start point").
pub fn set_contour_start(glyph: &mut Glyph, contour: usize, point: usize) -> bool {
    let Some(c) = glyph.contours.get_mut(contour) else {
        return false;
    };
    let closed = c.points.first().is_none_or(|p| p.typ != PointType::Move);
    if !closed || point == 0 || point >= c.points.len() {
        return false;
    }
    if c.points[point].typ == PointType::OffCurve {
        return false;
    }
    c.points.rotate_left(point);
    true
}

/// The topmost component whose resolved outline contains the point.
pub fn component_at(font: &Font, glyph: &Glyph, pt: kurbo::Point) -> Option<usize> {
    use kurbo::Shape as _;
    for (i, component) in glyph.components.iter().enumerate().rev() {
        let Some(base) = font.get_glyph(&component.base) else {
            continue;
        };
        let transform = glyph_paths::component_affine(&component.transform);
        let path = transform * &glyph_paths::glyph_to_bezpath(base, font);
        if path.contains(pt) {
            return Some(i);
        }
    }
    None
}

/// Move a component by adjusting its transform offset.
pub fn translate_component(glyph: &mut Glyph, index: usize, dx: f64, dy: f64) -> bool {
    let Some(component) = glyph.components.get_mut(index) else {
        return false;
    };
    component.transform.x_offset += dx;
    component.transform.y_offset += dy;
    true
}

/// Remove a component.
pub fn delete_component(glyph: &mut Glyph, index: usize) -> bool {
    if index >= glyph.components.len() {
        return false;
    }
    glyph.components.remove(index);
    true
}

/// Convert one closed BezPath contour into a norad contour. Points
/// found in `smooth_at` keep their smooth flag.
pub fn bezpath_to_contour(
    path: &BezPath,
    smooth_at: &HashMap<(i64, i64), bool>,
) -> Option<Contour> {
    use kurbo::PathEl;
    let mut points: Vec<ContourPoint> = Vec::new();
    let mut start: Option<kurbo::Point> = None;
    let smooth = |x: f64, y: f64| {
        smooth_at
            .get(&(x.round() as i64, y.round() as i64))
            .copied()
            .unwrap_or(false)
    };
    let on = |x: f64, y: f64, curve: bool, smooth: bool| {
        ContourPoint::new(
            x.round(),
            y.round(),
            if curve {
                PointType::Curve
            } else {
                PointType::Line
            },
            smooth,
            None,
            None,
        )
    };
    for el in path.elements() {
        match el {
            PathEl::MoveTo(p) => start = Some(*p),
            PathEl::LineTo(p) => points.push(on(p.x, p.y, false, smooth(p.x, p.y))),
            PathEl::CurveTo(c1, c2, p) => {
                points.push(off_point((c1.x.round(), c1.y.round())));
                points.push(off_point((c2.x.round(), c2.y.round())));
                points.push(on(p.x, p.y, true, smooth(p.x, p.y)));
            }
            PathEl::QuadTo(c, p) => {
                let s = points
                    .iter()
                    .rev()
                    .find(|q| q.typ != PointType::OffCurve)
                    .map(|q| kurbo::Point::new(q.x, q.y))
                    .or(start)?;
                let c1 = s + (c.to_vec2() - s.to_vec2()) * (2.0 / 3.0);
                let c2 = *p + (c.to_vec2() - p.to_vec2()) * (2.0 / 3.0);
                points.push(off_point((c1.x.round(), c1.y.round())));
                points.push(off_point((c2.x.round(), c2.y.round())));
                points.push(on(p.x, p.y, true, smooth(p.x, p.y)));
            }
            PathEl::ClosePath => {}
        }
    }
    let start = start?;
    // If the last on-curve duplicates the start, its segment closes
    // the contour: rotate that on-curve (and controls) to the front
    // per the UFO closed convention (no Move point).
    if let Some(last_on) = points.iter().rposition(|p| p.typ != PointType::OffCurve) {
        let lp = &points[last_on];
        if (lp.x - start.x.round()).abs() < 0.51 && (lp.y - start.y.round()).abs() < 0.51 {
            let tail: Vec<ContourPoint> = points.drain(last_on..).collect();
            let (controls, on_pt) = tail.split_at(tail.len() - 1);
            let mut rotated = vec![on_pt[0].clone()];
            rotated.extend(points);
            rotated.extend(controls.iter().cloned());
            points = rotated;
        } else {
            let first = on(start.x, start.y, false, smooth(start.x, start.y));
            points.insert(0, first);
        }
    } else {
        return None;
    }
    if points
        .iter()
        .filter(|p| p.typ != PointType::OffCurve)
        .count()
        < 2
    {
        return None;
    }
    Some(Contour::new(points, None))
}

// ============================================================================
// CURVE OPS
// ============================================================================

/// A curve-quality operation from [`crate::curve`].
#[derive(Clone, Copy)]
pub enum CurveOp {
    Harmonize,
    Balance,
    Optimize(f64),
}

/// Apply a curve-quality operation to the selected points, or to the
/// whole glyph when the selection is empty. Only closed contours
/// participate. Returns true if anything moved.
pub fn curve_op(glyph: &mut Glyph, selected: &HashSet<PointId>, op: CurveOp) -> bool {
    use crate::curve::{OptPoint, balance, harmonize, optimize_contour};
    let all = selected.is_empty();
    let mut changed = false;
    for (ci, contour) in glyph.contours.iter_mut().enumerate() {
        if !contour.is_closed() {
            continue;
        }
        let pts = &mut contour.points;
        let n = pts.len();
        if n < 4 {
            continue;
        }
        let on = |p: &ContourPoint| p.typ != PointType::OffCurve;
        let in_scope = |i: usize| all || selected.contains(&(ci, i));
        match op {
            CurveOp::Harmonize => {
                let mut updates: Vec<(usize, kurbo::Point)> = Vec::new();
                for i in 0..n {
                    if !on(&pts[i]) || !pts[i].smooth || !in_scope(i) {
                        continue;
                    }
                    let (a1, a2, b1, b2) =
                        ((i + n - 2) % n, (i + n - 1) % n, (i + 1) % n, (i + 2) % n);
                    if on(&pts[a1]) || on(&pts[a2]) || on(&pts[b1]) || on(&pts[b2]) {
                        continue;
                    }
                    let point = |k: usize| kurbo::Point::new(pts[k].x, pts[k].y);
                    if let Some((na2, nb1)) =
                        harmonize(point(a1), point(a2), point(i), point(b1), point(b2))
                    {
                        updates.push((a2, na2.round()));
                        updates.push((b1, nb1.round()));
                    }
                }
                for (k, p) in updates {
                    pts[k].x = p.x;
                    pts[k].y = p.y;
                    changed = true;
                }
            }
            CurveOp::Balance => {
                let mut updates: Vec<(usize, kurbo::Point)> = Vec::new();
                for i in 0..n {
                    let (b, c, d) = ((i + 1) % n, (i + 2) % n, (i + 3) % n);
                    if !on(&pts[i]) || on(&pts[b]) || on(&pts[c]) || !on(&pts[d]) {
                        continue;
                    }
                    if !(in_scope(i) || in_scope(b) || in_scope(c) || in_scope(d)) {
                        continue;
                    }
                    let point = |k: usize| kurbo::Point::new(pts[k].x, pts[k].y);
                    if let Some((np1, np2)) = balance(point(i), point(b), point(c), point(d)) {
                        updates.push((b, np1.round()));
                        updates.push((c, np2.round()));
                    }
                }
                for (k, p) in updates {
                    pts[k].x = p.x;
                    pts[k].y = p.y;
                    changed = true;
                }
            }
            CurveOp::Optimize(tol) => {
                if !all && !(0..n).any(in_scope) {
                    continue;
                }
                let opts: Vec<OptPoint> = pts
                    .iter()
                    .map(|p| OptPoint {
                        p: kurbo::Point::new(p.x, p.y),
                        on: on(p),
                        smooth: p.smooth,
                    })
                    .collect();
                let newpos = optimize_contour(&opts, tol);
                for (i, p) in pts.iter_mut().enumerate() {
                    if p.typ == PointType::OffCurve
                        && (kurbo::Point::new(p.x, p.y) - newpos[i]).hypot() > 1e-6
                    {
                        p.x = newpos[i].x;
                        p.y = newpos[i].y;
                        changed = true;
                    }
                }
            }
        }
    }
    changed
}

// ============================================================================
// METRICS AND ANCHORS
// ============================================================================

/// Shift all of a glyph's ink horizontally (LSB edits). Component
/// references shift via their transform offset.
pub fn shift_ink(glyph: &mut Glyph, dx: f64) {
    for contour in glyph.contours.iter_mut() {
        for p in contour.points.iter_mut() {
            p.x += dx;
        }
    }
    for component in glyph.components.iter_mut() {
        component.transform.x_offset += dx;
    }
}

/// Structural signature used for interpolation compatibility: per
/// contour, the ordered list of point types.
pub fn glyph_signature(glyph: &Glyph) -> Vec<Vec<PointType>> {
    glyph
        .contours
        .iter()
        .map(|c| c.points.iter().map(|p| p.typ).collect())
        .collect()
}

// ============================================================================
// KERNING
// ============================================================================

/// The kern group ("public.kern1." / "public.kern2." prefix)
/// containing a glyph, if any.
pub fn kern_group(font: &Font, glyph: &str, first_side: bool) -> Option<norad::Name> {
    let prefix = if first_side {
        "public.kern1."
    } else {
        "public.kern2."
    };
    font.groups
        .iter()
        .find(|(name, members)| {
            name.starts_with(prefix) && members.iter().any(|m| m.as_str() == glyph)
        })
        .map(|(name, _)| name.clone())
}

/// Swap a contour with its neighbor in draw order.
pub fn move_contour(glyph: &mut Glyph, index: usize, up: bool) -> bool {
    let n = glyph.contours.len();
    if up {
        if index == 0 || index >= n {
            return false;
        }
        glyph.contours.swap(index, index - 1);
    } else {
        if index + 1 >= n {
            return false;
        }
        glyph.contours.swap(index, index + 1);
    }
    true
}

/// Replace one component with its resolved outline (point-exact,
/// like decompose-all's resolved_component_contours).
pub fn decompose_single_component(
    font: &Font,
    glyph: &mut Glyph,
    index: usize,
) -> bool {
    let Some(component) = glyph.components.get(index) else {
        return false;
    };
    // A single-component wrapper glyph resolves through the shared
    // collector by pretending the glyph only has this component.
    let mut probe = Glyph::new("probe");
    probe.components.push(component.clone());
    let resolved = resolved_component_contours(font, &probe);
    if resolved.is_empty() {
        return false;
    }
    glyph.contours.extend(resolved);
    glyph.components.remove(index);
    true
}

/// Add a component placing `base`, anchor-locked so a mark lands on
/// its anchor rather than at the origin (web addComponent).
pub fn add_component(font: &Font, glyph: &mut Glyph, base: &str) -> bool {
    if base.is_empty() || base == glyph.name().as_str() {
        return false;
    }
    if font.get_glyph(base).is_none() {
        return false;
    }
    let Ok(base_name) = norad::Name::new(base) else {
        return false;
    };
    glyph.components.push(norad::Component::new(
        base_name,
        norad::AffineTransform::default(),
        None,
    ));
    true
}

const ROUND_GRID: f64 = 2.0;
const DEFAULT_ROUND_OFFSET: f64 = 32.0;
const DEFAULT_ROUND_HANDLE_RATIO: f64 = 0.552_284_749_830_793_6;
const MAX_ROUND_SIDE_FRACTION: f64 = 0.45;

fn round_snap(p: kurbo::Point) -> kurbo::Point {
    kurbo::Point::new(
        (p.x / ROUND_GRID).round() * ROUND_GRID,
        (p.y / ROUND_GRID).round() * ROUND_GRID,
    )
}

fn round_line_intersection(
    a: kurbo::Point,
    b: kurbo::Point,
    c: kurbo::Point,
    d: kurbo::Point,
) -> Option<kurbo::Point> {
    let r = b - a;
    let s = d - c;
    let cross = r.x * s.y - r.y * s.x;
    if cross.abs() < 1e-6 {
        return None;
    }
    let delta = c - a;
    let t = (delta.x * s.y - delta.y * s.x) / cross;
    Some(a + r * t)
}

fn median_or_default(mut values: Vec<f64>, default: f64) -> f64 {
    values.retain(|v| v.is_finite() && *v > 0.0);
    if values.is_empty() {
        return default;
    }
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn contour_is_on(contour: &Contour, i: usize) -> bool {
    contour.points[i].typ != PointType::OffCurve
}

fn wrap(i: isize, len: usize) -> usize {
    i.rem_euclid(len as isize) as usize
}

/// The size and handle ratio existing rounded corners in this glyph
/// use (web infer_round_corner_profile): sampled from every
/// on-off-off-on run whose straight neighbors intersect, median with
/// the classic defaults as fallback.
fn infer_round_profile(glyph: &Glyph) -> (f64, f64) {
    let mut offsets = Vec::new();
    let mut ratios = Vec::new();
    for contour in &glyph.contours {
        if crate::model::workspace::norad_contour_is_hyper(contour) {
            continue;
        }
        let n = contour.points.len();
        if n < 6 {
            continue;
        }
        for start in 0..n {
            let idx = [
                wrap(start as isize - 1, n),
                start,
                wrap(start as isize + 1, n),
                wrap(start as isize + 2, n),
                wrap(start as isize + 3, n),
                wrap(start as isize + 4, n),
            ];
            let mut seen = idx;
            seen.sort_unstable();
            if seen.windows(2).any(|w| w[0] == w[1]) {
                continue;
            }
            let [prev, s, cp1, cp2, end, next] = idx;
            if !contour_is_on(contour, s)
                || contour_is_on(contour, cp1)
                || contour_is_on(contour, cp2)
                || !contour_is_on(contour, end)
                || !contour_is_on(contour, prev)
                || !contour_is_on(contour, next)
            {
                continue;
            }
            let pt = |i: usize| {
                kurbo::Point::new(contour.points[i].x, contour.points[i].y)
            };
            let Some(corner) =
                round_line_intersection(pt(prev), pt(s), pt(end), pt(next))
            else {
                continue;
            };
            let start_offset = corner.distance(pt(s));
            let end_offset = corner.distance(pt(end));
            if start_offset < ROUND_GRID || end_offset < ROUND_GRID {
                continue;
            }
            let handle_one = pt(cp1).distance(pt(s));
            let handle_two = pt(end).distance(pt(cp2));
            offsets.push((start_offset + end_offset) * 0.5);
            if handle_one > 0.0 {
                ratios.push(handle_one / start_offset);
            }
            if handle_two > 0.0 {
                ratios.push(handle_two / end_offset);
            }
        }
    }
    (
        median_or_default(offsets, DEFAULT_ROUND_OFFSET),
        median_or_default(ratios, DEFAULT_ROUND_HANDLE_RATIO).clamp(0.1, 1.0),
    )
}

/// Round the selected line-line corners into cubic fillets sized to
/// match the glyph's existing rounding (web round_selected_corners).
/// Returns the new selection: the fillets' on-curve points.
pub fn round_selected_corners(
    glyph: &mut Glyph,
    selected: &HashSet<(usize, usize)>,
) -> Option<HashSet<(usize, usize)>> {
    if selected.is_empty() {
        return None;
    }
    let (offset_profile, handle_ratio) = infer_round_profile(glyph);
    let mut next_selection = HashSet::new();
    let mut changed = false;

    for (ci, contour) in glyph.contours.iter_mut().enumerate() {
        if crate::model::workspace::norad_contour_is_hyper(contour) {
            continue;
        }
        let n = contour.points.len();
        let closed = contour
            .points
            .first()
            .map(|p| p.typ != PointType::Move)
            .unwrap_or(false);
        let mut replaced = false;
        let mut next_points: Vec<ContourPoint> = Vec::with_capacity(n + 8);
        for (pi, point) in contour.points.iter().enumerate() {
            let is_corner = selected.contains(&(ci, pi))
                && point.typ != PointType::OffCurve
                && n >= 3
                && (closed || (pi > 0 && pi + 1 < n));
            let rounded = is_corner
                .then(|| {
                    let prev = wrap(pi as isize - 1, n);
                    let next = wrap(pi as isize + 1, n);
                    if !contour_is_on(contour, prev)
                        || !contour_is_on(contour, next)
                    {
                        return None;
                    }
                    let corner = kurbo::Point::new(point.x, point.y);
                    let p_prev = kurbo::Point::new(
                        contour.points[prev].x,
                        contour.points[prev].y,
                    );
                    let p_next = kurbo::Point::new(
                        contour.points[next].x,
                        contour.points[next].y,
                    );
                    let prev_vec = p_prev - corner;
                    let next_vec = p_next - corner;
                    let prev_len = prev_vec.hypot();
                    let next_len = next_vec.hypot();
                    if prev_len < ROUND_GRID * 2.0 || next_len < ROUND_GRID * 2.0
                    {
                        return None;
                    }
                    let offset = offset_profile
                        .min(prev_len * MAX_ROUND_SIDE_FRACTION)
                        .min(next_len * MAX_ROUND_SIDE_FRACTION);
                    if offset < ROUND_GRID {
                        return None;
                    }
                    let prev_unit = prev_vec / prev_len;
                    let next_unit = next_vec / next_len;
                    let handle_len = offset * handle_ratio;
                    let first_on = round_snap(corner + prev_unit * offset);
                    let second_on = round_snap(corner + next_unit * offset);
                    let first_handle =
                        round_snap(first_on - prev_unit * handle_len);
                    let second_handle =
                        round_snap(second_on - next_unit * handle_len);
                    if first_on == corner
                        || second_on == corner
                        || first_on == second_on
                    {
                        return None;
                    }
                    Some((first_on, first_handle, second_handle, second_on))
                })
                .flatten();
            match rounded {
                Some((first_on, h1, h2, second_on)) => {
                    // Keep the incoming segment type for the first
                    // on-curve; the fillet ends in a Curve point.
                    let mut lead = point.clone();
                    lead.x = first_on.x;
                    lead.y = first_on.y;
                    lead.smooth = true;
                    next_selection.insert((ci, next_points.len()));
                    next_points.push(lead);
                    next_points.push(ContourPoint::new(
                        h1.x,
                        h1.y,
                        PointType::OffCurve,
                        false,
                        None,
                        None,
                    ));
                    next_points.push(ContourPoint::new(
                        h2.x,
                        h2.y,
                        PointType::OffCurve,
                        false,
                        None,
                        None,
                    ));
                    next_selection.insert((ci, next_points.len()));
                    next_points.push(ContourPoint::new(
                        second_on.x,
                        second_on.y,
                        PointType::Curve,
                        true,
                        None,
                        None,
                    ));
                    replaced = true;
                }
                None => next_points.push(point.clone()),
            }
        }
        if replaced {
            contour.points = next_points;
            changed = true;
        }
    }
    changed.then_some(next_selection)
}

/// Duplicate every contour containing a selected point, offset by
/// (20, 20) like the web editor, returning the new selection (every
/// point of the clones).
pub fn duplicate_selection(
    glyph: &mut Glyph,
    selected: &HashSet<(usize, usize)>,
) -> Option<HashSet<(usize, usize)>> {
    let sources: Vec<usize> = glyph
        .contours
        .iter()
        .enumerate()
        .filter(|(ci, contour)| {
            contour
                .points
                .iter()
                .enumerate()
                .any(|(pi, _)| selected.contains(&(*ci, pi)))
        })
        .map(|(ci, _)| ci)
        .collect();
    if sources.is_empty() {
        return None;
    }
    let mut new_selection = HashSet::new();
    for source in sources {
        // Fresh points and no identifiers: identifiers must stay
        // unique within a glif, so clones cannot carry them.
        let points: Vec<norad::ContourPoint> = glyph.contours[source]
            .points
            .iter()
            .map(|p| {
                norad::ContourPoint::new(
                    p.x + 20.0,
                    p.y + 20.0,
                    p.typ.clone(),
                    p.smooth,
                    p.name.clone(),
                    None,
                )
            })
            .collect();
        let new_index = glyph.contours.len();
        for pi in 0..points.len() {
            new_selection.insert((new_index, pi));
        }
        glyph.contours.push(norad::Contour::new(points, None));
    }
    Some(new_selection)
}

/// Duplicate a component, offset by (20, 20). Returns the new index.
pub fn duplicate_component(glyph: &mut Glyph, index: usize) -> Option<usize> {
    let source = glyph.components.get(index)?;
    let mut transform = source.transform;
    transform.x_offset += 20.0;
    transform.y_offset += 20.0;
    let clone = norad::Component::new(source.base.clone(), transform, None);
    glyph.components.push(clone);
    Some(glyph.components.len() - 1)
}

/// Duplicate an anchor, offset by (20, 20). Returns the new index.
pub fn duplicate_anchor(glyph: &mut Glyph, index: usize) -> Option<usize> {
    let source = glyph.anchors.get(index)?;
    let name = source
        .name
        .as_ref()
        .and_then(|n| norad::Name::new(&format!("{n}.copy")).ok());
    let anchor = norad::Anchor::new(
        source.x + 20.0,
        source.y + 20.0,
        name,
        None,
        None,
    );
    glyph.anchors.push(anchor);
    Some(glyph.anchors.len() - 1)
}

/// Put a glyph into a kerning group (groups.plist), replacing any
/// membership on that side. `group` is the bare name ("A" becomes
/// public.kern1.A); empty removes the membership. Returns true when
/// anything changed.
pub fn set_kern_group(
    font: &mut Font,
    glyph: &str,
    first_side: bool,
    group: &str,
) -> bool {
    let prefix = if first_side {
        "public.kern1."
    } else {
        "public.kern2."
    };
    let target = group.trim();
    let target_name = (!target.is_empty())
        .then(|| norad::Name::new(&format!("{prefix}{target}")).ok())
        .flatten();
    let mut changed = false;
    // Drop the glyph from every group on this side except the target.
    let mut empty: Vec<norad::Name> = Vec::new();
    for (name, members) in font.groups.iter_mut() {
        if !name.starts_with(prefix) {
            continue;
        }
        if Some(name) == target_name.as_ref() {
            continue;
        }
        let before = members.len();
        members.retain(|m| m.as_str() != glyph);
        if members.len() != before {
            changed = true;
        }
        if members.is_empty() {
            empty.push(name.clone());
        }
    }
    for name in empty {
        font.groups.remove(&name);
        changed = true;
    }
    if let Some(target_name) = target_name {
        let glyph_name = match norad::Name::new(glyph) {
            Ok(name) => name,
            Err(_) => return changed,
        };
        let members = font.groups.entry(target_name).or_default();
        if !members.iter().any(|m| m.as_str() == glyph) {
            members.push(glyph_name);
            changed = true;
        }
    }
    changed
}

/// Set a glyph's (first) codepoint from text: "0041", "U+0041", or
/// "0x41"; empty clears. Returns false when the text does not parse.
pub fn set_glyph_unicode(glyph: &mut Glyph, unicode: &str) -> bool {
    let trimmed = unicode.trim();
    if trimmed.is_empty() {
        glyph.codepoints = norad::Codepoints::new([]);
        return true;
    }
    let hex = trimmed
        .strip_prefix("U+")
        .or_else(|| trimmed.strip_prefix("u+"))
        .or_else(|| trimmed.strip_prefix("0x"))
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    let Some(c) = u32::from_str_radix(hex, 16)
        .ok()
        .and_then(char::from_u32)
    else {
        return false;
    };
    glyph.codepoints = norad::Codepoints::new([c]);
    true
}

/// Rename a glyph and every reference to it: components in other
/// glyphs, kerning group memberships, and direct kerning pair keys.
/// Refuses when the new name is taken or invalid.
pub fn rename_glyph(font: &mut Font, old: &str, new: &str) -> bool {
    let new = new.trim();
    if new.is_empty() || new == old {
        return false;
    }
    let Ok(new_name) = norad::Name::new(new) else {
        return false;
    };
    if font.get_glyph(new).is_some() {
        return false;
    }
    let layer = font.default_layer_mut();
    if layer.rename_glyph(old, new, false).is_err() {
        return false;
    }
    // Components in every glyph that places it.
    let renames: Vec<norad::Name> = layer
        .iter()
        .filter(|g| g.components.iter().any(|c| c.base.as_str() == old))
        .map(|g| g.name().clone())
        .collect();
    for user in renames {
        if let Some(user_glyph) = layer.get_glyph_mut(user.as_str()) {
            for component in user_glyph.components.iter_mut() {
                if component.base.as_str() == old {
                    component.base = new_name.clone();
                }
            }
        }
    }
    // Group memberships.
    for (_, members) in font.groups.iter_mut() {
        for member in members.iter_mut() {
            if member.as_str() == old {
                *member = new_name.clone();
            }
        }
    }
    // Direct kerning keys on either side.
    let old_key = norad::Name::new(old).ok();
    if let Some(old_key) = old_key {
        if let Some(seconds) = font.kerning.remove(&old_key) {
            font.kerning.insert(new_name.clone(), seconds);
        }
        for (_, seconds) in font.kerning.iter_mut() {
            if let Some(value) = seconds.remove(&old_key) {
                seconds.insert(new_name.clone(), value);
            }
        }
    }
    true
}

/// Kerning between two glyphs, resolving group fallbacks in UFO
/// precedence order: glyph-glyph, glyph-group, group-glyph,
/// group-group.
pub fn kern_value(font: &Font, left: &str, right: &str) -> f64 {
    let lookup =
        |a: &str, b: &str| -> Option<f64> { font.kerning.get(a).and_then(|m| m.get(b)).copied() };
    let lg = kern_group(font, left, true);
    let rg = kern_group(font, right, false);
    lookup(left, right)
        .or_else(|| rg.as_ref().and_then(|g| lookup(left, g.as_str())))
        .or_else(|| lg.as_ref().and_then(|g| lookup(g.as_str(), right)))
        .or_else(|| {
            lg.as_ref()
                .and_then(|l| rg.as_ref().and_then(|r| lookup(l.as_str(), r.as_str())))
        })
        .unwrap_or(0.0)
}

/// Set an exception-level (glyph-to-glyph) kern pair.
pub fn set_kern_pair(font: &mut Font, left: &str, right: &str, value: f64) {
    let (Ok(l), Ok(r)) = (norad::Name::new(left), norad::Name::new(right)) else {
        return;
    };
    font.kerning.entry(l).or_default().insert(r, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bare_glyph() -> Glyph {
        Glyph::new("test")
    }

    #[test]
    fn flip_and_rotate_preserve_bbox_center() {
        let mut g = bare_glyph();
        let c = start_contour(&mut g, 0.0, 0.0);
        append_segment(&mut g, c, None, 100.0, 0.0, false);
        append_segment(&mut g, c, None, 100.0, 60.0, false);
        append_segment(&mut g, c, None, 0.0, 60.0, false);
        close_contour(&mut g, c, None);

        // Flip horizontal about the bbox center: x -> 100 - x.
        assert!(transform_selection(
            &mut g,
            &HashSet::new(),
            kurbo::Affine::scale_non_uniform(-1.0, 1.0),
        ));
        let xs: Vec<f64> = g.contours[c].points.iter().map(|p| p.x).collect();
        assert!(xs.iter().all(|x| (0.0..=100.0).contains(x)));
        assert_eq!(g.contours[c].points[0].y, 0.0);

        // Rotate 90° twice = 180°: the rectangle maps onto itself.
        let before: Vec<(f64, f64)> =
            g.contours[c].points.iter().map(|p| (p.x, p.y)).collect();
        for _ in 0..2 {
            assert!(transform_selection(
                &mut g,
                &HashSet::new(),
                kurbo::Affine::rotate(std::f64::consts::FRAC_PI_2),
            ));
        }
        // 180° maps each corner to the opposite one; compare as sets.
        let round = |v: Vec<(f64, f64)>| {
            let mut v: Vec<(i64, i64)> =
                v.into_iter().map(|(x, y)| (x.round() as i64, y.round() as i64)).collect();
            v.sort();
            v
        };
        let after: Vec<(f64, f64)> =
            g.contours[c].points.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(round(before), round(after));

        // Empty glyph: nothing to transform.
        let mut empty = bare_glyph();
        assert!(!transform_selection(
            &mut empty,
            &HashSet::new(),
            kurbo::Affine::IDENTITY
        ));
    }

    #[test]
    fn transform_only_selected_points() {
        let mut g = bare_glyph();
        let c = start_contour(&mut g, 0.0, 0.0);
        append_segment(&mut g, c, None, 100.0, 0.0, false);
        append_segment(&mut g, c, None, 50.0, 80.0, false);
        close_contour(&mut g, c, None);
        let fixed: Vec<(f64, f64)> = g.contours[c]
            .points
            .iter()
            .take(2)
            .map(|p| (p.x, p.y))
            .collect();
        // Translate only point 2.
        let selected: HashSet<PointId> = [(c, 2)].into();
        assert!(transform_selection(
            &mut g,
            &selected,
            kurbo::Affine::translate((10.0, 5.0)),
        ));
        // A single point's bbox center is itself; translate still moves it.
        assert_eq!((g.contours[c].points[2].x, g.contours[c].points[2].y), (60.0, 85.0));
        for (point, (x, y)) in g.contours[c].points.iter().zip(fixed) {
            assert_eq!((point.x, point.y), (x, y));
        }
    }

    #[test]
    fn reverse_flips_winding_and_keeps_shape() {
        let mut g = bare_glyph();
        let c = start_contour(&mut g, 0.0, 0.0);
        append_segment(&mut g, c, None, 100.0, 0.0, false);
        append_segment(
            &mut g,
            c,
            Some(((130.0, 40.0), (130.0, 80.0))),
            100.0,
            120.0,
            true,
        );
        append_segment(&mut g, c, None, 0.0, 120.0, false);
        close_contour(&mut g, c, None);
        let area_before = signed_area(&glyph_paths::contour_to_bezpath(&g.contours[c]));
        let count_before = g.contours[c].points.len();
        assert!(reverse_contours(&mut g, &HashSet::new()));
        let area_after = signed_area(&glyph_paths::contour_to_bezpath(&g.contours[c]));
        assert_eq!(g.contours[c].points.len(), count_before);
        assert!((area_before + area_after).abs() < 1.0, "winding must flip");
        // The smooth flag survives the round-trip.
        assert!(g.contours[c].points.iter().any(|p| p.smooth));
    }

    fn signed_area(path: &BezPath) -> f64 {
        use kurbo::Shape;
        path.area()
    }

    #[test]
    fn boolean_subtract_cuts_a_hole_disjoint_union_merges() {
        let mut g = bare_glyph();
        // Outer square and a smaller inner square.
        for rect in [
            kurbo::Rect::new(0.0, 0.0, 100.0, 100.0),
            kurbo::Rect::new(25.0, 25.0, 75.0, 75.0),
        ] {
            add_shape_contour(&mut g, rect, false);
        }
        let result =
            boolean_contours(&g, linesweeper::BinaryOp::Difference).expect("subtract");
        // Outer minus inner: two contours (ring).
        assert_eq!(result.len(), 2);

        let mut g2 = bare_glyph();
        for rect in [
            kurbo::Rect::new(0.0, 0.0, 100.0, 100.0),
            kurbo::Rect::new(50.0, 0.0, 150.0, 100.0),
        ] {
            add_shape_contour(&mut g2, rect, false);
        }
        let merged =
            boolean_contours(&g2, linesweeper::BinaryOp::Union).expect("union");
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn set_contour_start_rotates_closed_contours() {
        let mut g = bare_glyph();
        add_shape_contour(&mut g, kurbo::Rect::new(0.0, 0.0, 100.0, 100.0), false);
        let first_before = (g.contours[0].points[0].x, g.contours[0].points[0].y);
        let target = (g.contours[0].points[2].x, g.contours[0].points[2].y);
        assert!(set_contour_start(&mut g, 0, 2));
        assert_eq!(
            (g.contours[0].points[0].x, g.contours[0].points[0].y),
            target
        );
        // Same point count, old start still present.
        assert_eq!(g.contours[0].points.len(), 4);
        assert!(g.contours[0]
            .points
            .iter()
            .any(|p| (p.x, p.y) == first_before));
        // Index 0 refuses (already the start), off-curves refuse.
        assert!(!set_contour_start(&mut g, 0, 0));
    }

    #[test]
    fn round_corner_replaces_with_fillet() {
        let mut g = bare_glyph();
        add_shape_contour(&mut g, kurbo::Rect::new(0.0, 0.0, 200.0, 200.0), false);
        // Round the (0,0) corner (index of that point in the rect).
        let corner = g.contours[0]
            .points
            .iter()
            .position(|p| p.x == 0.0 && p.y == 0.0)
            .unwrap();
        let selected: HashSet<(usize, usize)> = [(0, corner)].into();
        let new_sel = round_selected_corners(&mut g, &selected).expect("round");
        // One corner became 4 points: net +3.
        assert_eq!(g.contours[0].points.len(), 7);
        assert_eq!(new_sel.len(), 2);
        // The fillet's on-curves sit 32 units along each edge and are
        // smooth; the two handles between them are off-curve.
        let pts = &g.contours[0].points;
        let ons: Vec<&norad::ContourPoint> = pts
            .iter()
            .filter(|p| p.typ != PointType::OffCurve && p.smooth)
            .collect();
        assert_eq!(ons.len(), 2);
        for p in ons {
            let along = (p.x - 0.0).abs().max((p.y - 0.0).abs());
            assert!((along - 32.0).abs() < 1e-6, "offset {along}");
        }
        // Nothing rounds twice: the fillet points are smooth curves
        // with off-curve neighbors now.
        let again = round_selected_corners(&mut g, &new_sel);
        assert!(again.is_none());
    }

    #[test]
    fn duplicate_clones_selected_contours_offset() {
        let mut g = bare_glyph();
        add_shape_contour(&mut g, kurbo::Rect::new(0.0, 0.0, 100.0, 100.0), false);
        add_shape_contour(
            &mut g,
            kurbo::Rect::new(200.0, 0.0, 300.0, 100.0),
            false,
        );
        let selected: HashSet<(usize, usize)> = [(0, 0)].into();
        let new_sel = duplicate_selection(&mut g, &selected).expect("dup");
        assert_eq!(g.contours.len(), 3);
        // The clone is contour 2, offset by (20, 20), fully selected.
        assert_eq!(g.contours[2].points[0].x, g.contours[0].points[0].x + 20.0);
        assert!(new_sel.contains(&(2, 0)));
        assert_eq!(new_sel.len(), g.contours[2].points.len());
        assert!(g.contours[2].identifier().is_none());
        // Empty selection duplicates nothing.
        assert!(duplicate_selection(&mut g, &HashSet::new()).is_none());
    }

    #[test]
    fn kern_group_membership_moves_and_prunes() {
        let mut font = Font::new();
        font.default_layer_mut().insert_glyph(Glyph::new("A"));
        assert!(set_kern_group(&mut font, "A", true, "ROUND"));
        assert_eq!(
            kern_group(&font, "A", true).unwrap().as_str(),
            "public.kern1.ROUND"
        );
        // Moving to another group prunes the emptied one.
        assert!(set_kern_group(&mut font, "A", true, "FLAT"));
        assert_eq!(
            kern_group(&font, "A", true).unwrap().as_str(),
            "public.kern1.FLAT"
        );
        assert!(!font.groups.contains_key("public.kern1.ROUND"));
        // Clearing removes membership; second sides are independent.
        assert!(set_kern_group(&mut font, "A", false, "LEFTY"));
        assert!(set_kern_group(&mut font, "A", true, ""));
        assert!(kern_group(&font, "A", true).is_none());
        assert_eq!(
            kern_group(&font, "A", false).unwrap().as_str(),
            "public.kern2.LEFTY"
        );
    }

    #[test]
    fn unicode_parses_the_web_forms() {
        let mut glyph = Glyph::new("A");
        assert!(set_glyph_unicode(&mut glyph, "0041"));
        assert_eq!(glyph.codepoints.iter().next(), Some('A'));
        assert!(set_glyph_unicode(&mut glyph, "U+0042"));
        assert_eq!(glyph.codepoints.iter().next(), Some('B'));
        assert!(set_glyph_unicode(&mut glyph, "0x43"));
        assert_eq!(glyph.codepoints.iter().next(), Some('C'));
        assert!(set_glyph_unicode(&mut glyph, ""));
        assert_eq!(glyph.codepoints.iter().next(), None);
        assert!(!set_glyph_unicode(&mut glyph, "zzz"));
    }

    #[test]
    fn rename_updates_components_groups_and_kerning() {
        let mut font = Font::new();
        font.default_layer_mut().insert_glyph(Glyph::new("A"));
        font.default_layer_mut().insert_glyph(Glyph::new("B"));
        let mut agrave = Glyph::new("Agrave");
        agrave.components.push(norad::Component::new(
            norad::Name::new("A").unwrap(),
            Default::default(),
            None,
        ));
        font.default_layer_mut().insert_glyph(agrave);
        set_kern_group(&mut font, "A", true, "ROUND");
        set_kern_pair(&mut font, "A", "B", -30.0);

        assert!(rename_glyph(&mut font, "A", "A.new"));
        assert!(font.get_glyph("A").is_none());
        assert!(font.get_glyph("A.new").is_some());
        assert_eq!(
            font.get_glyph("Agrave").unwrap().components[0].base.as_str(),
            "A.new"
        );
        assert_eq!(
            kern_group(&font, "A.new", true).unwrap().as_str(),
            "public.kern1.ROUND"
        );
        assert_eq!(kern_value(&font, "A.new", "B"), -30.0);
        // Taken and invalid names are refused.
        assert!(!rename_glyph(&mut font, "A.new", "B"));
        assert!(!rename_glyph(&mut font, "A.new", ""));
    }

    #[test]
    fn components_hit_translate_delete() {
        let mut font = Font::new();
        let mut base = Glyph::new("base");
        add_shape_contour(&mut base, kurbo::Rect::new(0.0, 0.0, 100.0, 100.0), false);
        font.default_layer_mut().insert_glyph(base);

        let mut composite = Glyph::new("comp");
        composite.components.push(norad::Component::new(
            norad::Name::new("base").unwrap(),
            norad::AffineTransform {
                x_scale: 1.0,
                xy_scale: 0.0,
                yx_scale: 0.0,
                y_scale: 1.0,
                x_offset: 200.0,
                y_offset: 0.0,
            },
            None,
        ));
        assert_eq!(
            component_at(&font, &composite, kurbo::Point::new(250.0, 50.0)),
            Some(0)
        );
        assert_eq!(
            component_at(&font, &composite, kurbo::Point::new(50.0, 50.0)),
            None
        );
        assert!(translate_component(&mut composite, 0, 10.0, 5.0));
        assert_eq!(composite.components[0].transform.x_offset, 210.0);
        assert!(delete_component(&mut composite, 0));
        assert!(composite.components.is_empty());
    }

    #[test]
    fn hyper_pen_builds_closed_solver_contour() {
        let mut g = bare_glyph();
        let c = start_hyper_contour(&mut g, 0.0, 0.0);
        append_hyper_point(&mut g, c, 200.0, 0.0, false);
        append_hyper_point(&mut g, c, 200.0, 200.0, true);
        append_hyper_point(&mut g, c, 0.0, 200.0, false);
        close_hyper_contour(&mut g, c);

        assert!(contour_is_hyper(&g, c));
        assert!(g.contours[c].is_closed());
        assert_eq!(g.contours[c].points.len(), 4);
        // Corner point stored as line, smooth points as curve.
        assert_eq!(g.contours[c].points[2].typ, PointType::Line);
        assert_eq!(g.contours[c].points[1].typ, PointType::Curve);

        // The solver renders real curves: the bezpath must contain
        // curve elements even though the contour has no off-curves.
        let path = crate::glyph_paths::contours_to_bezpath(&g);
        let curves = path
            .elements()
            .iter()
            .filter(|e| matches!(e, kurbo::PathEl::CurveTo(..)))
            .count();
        assert!(curves >= 2, "solver should emit curves, got {curves}");

        // Round-trips through the workspace model keep the identifier.
        let ws = crate::model::workspace::Contour::from_norad(&g.contours[c]);
        let back = ws.to_norad();
        assert!(crate::model::workspace::norad_contour_is_hyper(&back));
    }

    #[test]
    fn pen_builds_closed_contour() {
        let mut g = bare_glyph();
        let c = start_contour(&mut g, 0.0, 0.0);
        append_segment(&mut g, c, None, 100.0, 0.0, false);
        append_segment(
            &mut g,
            c,
            Some(((130.0, 40.0), (130.0, 80.0))),
            100.0,
            120.0,
            true,
        );
        close_contour(&mut g, c, None);
        assert!(g.contours[c].is_closed());
        assert_eq!(g.contours[c].points.len(), 5);
        assert!(g.contours[c].points[4].smooth);
    }

    #[test]
    fn delete_offcurve_makes_line_delete_oncurve_drops_segment() {
        let mut g = bare_glyph();
        let c = start_contour(&mut g, 0.0, 0.0);
        append_segment(&mut g, c, None, 100.0, 0.0, false);
        append_segment(&mut g, c, None, 100.0, 100.0, false);
        append_segment(
            &mut g,
            c,
            Some(((80.0, 130.0), (20.0, 130.0))),
            0.0,
            100.0,
            true,
        );
        close_contour(&mut g, c, None);
        assert_eq!(g.contours[c].points.len(), 6);

        let off = g.contours[c]
            .points
            .iter()
            .position(|p| p.typ == PointType::OffCurve)
            .unwrap();
        assert!(delete_points(&mut g, &HashSet::from([(c, off)])));
        assert_eq!(g.contours[c].points.len(), 4);
        assert!(g.contours[c].is_closed());

        let corner = g.contours[c]
            .points
            .iter()
            .position(|p| p.x == 100.0 && p.y == 0.0)
            .unwrap();
        assert!(delete_points(&mut g, &HashSet::from([(c, corner)])));
        assert_eq!(g.contours[c].points.len(), 3);
    }

    #[test]
    fn overlap_union_two_squares() {
        let mut g = bare_glyph();
        add_shape_contour(&mut g, kurbo::Rect::new(0.0, 0.0, 100.0, 100.0), false);
        add_shape_contour(&mut g, kurbo::Rect::new(50.0, 50.0, 150.0, 150.0), false);
        let unioned = remove_overlap(&g).expect("union");
        assert_eq!(unioned.len(), 1);
        g.contours = unioned;
        use kurbo::Shape;
        let area = glyph_paths::contours_to_bezpath(&g).area().abs();
        assert!((area - 17500.0).abs() < 100.0, "area {area}");
    }

    #[test]
    fn smooth_constraint_rotates_sibling() {
        let mut g = bare_glyph();
        let c = start_contour(&mut g, 0.0, 0.0);
        append_segment(
            &mut g,
            c,
            Some(((40.0, 60.0), (60.0, 100.0))),
            100.0,
            100.0,
            true,
        );
        append_segment(
            &mut g,
            c,
            Some(((140.0, 100.0), (180.0, 60.0))),
            200.0,
            0.0,
            false,
        );
        close_contour(&mut g, c, None);
        let incoming = g.contours[c]
            .points
            .iter()
            .position(|p| p.x == 60.0 && p.y == 100.0)
            .unwrap();
        let outgoing = g.contours[c]
            .points
            .iter()
            .position(|p| p.x == 140.0 && p.y == 100.0)
            .unwrap();
        set_points(&mut g, &[((c, incoming), (60.0, 80.0))]);
        constrain_smooth_neighbor(&mut g, c, incoming);
        let out = &g.contours[c].points[outgoing];
        let cross = (100.0 - 60.0) * (out.y - 100.0) - (100.0 - 80.0) * (out.x - 100.0);
        assert!(cross.abs() <= 60.0, "not collinear: {cross}");
        let len = ((out.x - 100.0f64).powi(2) + (out.y - 100.0f64).powi(2)).sqrt();
        assert!((len - 40.0).abs() < 2.0, "length changed: {len}");
    }

    #[test]
    fn shapes_and_curve_ops() {
        let mut g = bare_glyph();
        add_shape_contour(&mut g, kurbo::Rect::new(10.0, 20.0, 110.0, 220.0), true);
        assert_eq!(g.contours[0].points.len(), 12);
        assert!(g.contours[0].is_closed());
        // Balance a perfect ellipse: on-curves must not move.
        let before: Vec<(f64, f64)> = g.contours[0].points.iter().map(|p| (p.x, p.y)).collect();
        curve_op(&mut g, &HashSet::new(), CurveOp::Balance);
        for (i, p) in g.contours[0].points.iter().enumerate() {
            if p.typ != PointType::OffCurve {
                assert_eq!(before[i], (p.x, p.y));
            }
        }
    }

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut g = bare_glyph();
        add_shape_contour(&mut g, kurbo::Rect::new(0.0, 0.0, 10.0, 10.0), false);
        g.width = 250.0;
        let snap = snapshot(&g);
        shift_ink(&mut g, 50.0);
        g.width = 999.0;
        restore(&mut g, snap);
        assert_eq!(g.width, 250.0);
        assert_eq!(g.contours[0].points[0].x, 0.0);
    }
}
