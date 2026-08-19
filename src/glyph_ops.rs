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
