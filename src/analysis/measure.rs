// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Live grid measurements for the on-canvas HUD: handle lengths, and the
//! horizontal/vertical spans between facing straight edges.
//!
//! The span pass yields stem widths and counters from the same logic,
//! because a counter is just the gap between two facing near-vertical inner
//! edges. The designer sees every measurement that matters while drawing,
//! on Virtua Grotesk's power-of-two grid.
//!
//! Everything here is design-space geometry (font units). The renderer maps
//! it to the screen and draws ticks and labels; popcount/tier styling reads
//! off the length. Kept ungated and free of render deps so it unit-tests on
//! native `cargo test`.

use kurbo::{BezPath, Line, ParamCurve, PathEl, PathSeg, Point, Shape};

use crate::outline::path::Path;

/// What a single measurement describes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MeasureKind {
    /// On-curve point to its off-curve control (a Bézier handle).
    Handle,
    /// A straight outline segment: two consecutive on-curve points, its own
    /// drawn length (any orientation, including diagonals).
    Segment,
    /// Horizontal span between two facing near-vertical edges (stem/counter).
    Horizontal,
    /// Vertical span between two facing near-horizontal edges (bar/height).
    Vertical,
}

/// One measurement in design space.
///
/// `a` and `b` are the endpoints of the span; for a `Handle`, `a` is the
/// on-curve anchor. `length` is the rounded design-unit distance to label.
#[derive(Clone, Copy, Debug)]
pub struct Measurement {
    /// First endpoint of the span, in design units.
    pub a: Point,
    /// Second endpoint of the span, in design units.
    pub b: Point,
    /// Rounded distance from `a` to `b`, in design units.
    pub length: i64,
    /// What the span describes.
    pub kind: MeasureKind,
}

/// Ignore spans, segments, and handles shorter than this: noise, coincident
/// points, or near-tangent scan crossings.
const MIN_LEN: f64 = 8.0;
/// A straight segment counts as axis-aligned when its off-axis drift is
/// within this many units. Virtua's stems and bars are dead straight.
const AXIS_TOL: f64 = 3.0;
/// Two facing edges must overlap on the perpendicular axis by at least this
/// to be measuring the same span: a real stem or counter, not a glancing
/// pair.
const MIN_OVERLAP: f64 = 24.0;

/// A straight axis-aligned outline edge, captured for facing-span detection.
#[derive(Clone, Copy)]
struct Edge {
    /// Position on the measured axis: x for vertical edges, y for horizontal.
    pos: f64,
    /// Extent on the perpendicular axis.
    lo: f64,
    hi: f64,
}

/// Compute every live measurement for a glyph's contours: handle lengths,
/// straight segment lengths, and stem/counter/thickness spans.
///
/// Spans come from two general passes. The facing-edge pass measures each
/// straight edge to its nearest facing edge that overlaps it: stems,
/// counters, bars, including split walls like the H's. The center scan line
/// is kept only for curve-bounded gaps, like the `o`.
pub fn glyph_measurements(paths: &[Path]) -> Vec<Measurement> {
    let mut out = Vec::new();
    let mut verticals: Vec<Edge> = Vec::new();
    let mut horizontals: Vec<Edge> = Vec::new();

    for path in paths {
        let pts = path.points().as_slice();
        let n = pts.len();
        if n < 2 {
            continue;
        }
        for i in 0..n {
            let cur = &pts[i];
            let nxt = &pts[(i + 1) % n];

            // Handles: an off-curve point paired with its adjacent on-curve
            // anchor. Each off-curve has exactly one on-curve neighbor.
            if !cur.is_on_curve() {
                let prev = &pts[(i + n - 1) % n];
                let anchor = if prev.is_on_curve() {
                    Some(prev)
                } else if nxt.is_on_curve() {
                    Some(nxt)
                } else {
                    None
                };
                if let Some(anchor) = anchor {
                    let len = (cur.point - anchor.point).hypot();
                    if len >= MIN_LEN {
                        out.push(Measurement {
                            a: anchor.point,
                            b: cur.point,
                            length: len.round() as i64,
                            kind: MeasureKind::Handle,
                        });
                    }
                }
            }

            // Straight segment: its own length, plus an axis-aligned edge for
            // the facing-span pass.
            if cur.is_on_curve() && nxt.is_on_curve() {
                let (a, b) = (cur.point, nxt.point);
                let seg_len = (b - a).hypot();
                if seg_len >= MIN_LEN {
                    out.push(Measurement {
                        a,
                        b,
                        length: seg_len.round() as i64,
                        kind: MeasureKind::Segment,
                    });
                }
                let (dx, dy) = ((a.x - b.x).abs(), (a.y - b.y).abs());
                if dx <= AXIS_TOL && dy > MIN_LEN {
                    verticals.push(Edge {
                        pos: (a.x + b.x) / 2.0,
                        lo: a.y.min(b.y),
                        hi: a.y.max(b.y),
                    });
                } else if dy <= AXIS_TOL && dx > MIN_LEN {
                    horizontals.push(Edge {
                        pos: (a.y + b.y) / 2.0,
                        lo: a.x.min(b.x),
                        hi: a.x.max(b.x),
                    });
                }
            }
        }
    }

    facing_gaps(&verticals, MeasureKind::Horizontal, &mut out);
    facing_gaps(&horizontals, MeasureKind::Vertical, &mut out);
    scan_spans(paths, &mut out);
    out
}

/// For each edge, measure the gap to the nearest facing edge at a larger
/// position whose perpendicular extent overlaps it.
///
/// Measuring per edge, rather than only x-adjacent pairs, means split walls
/// still pair up into their upper and lower counters. The H's inner stems,
/// cut by the crossbar, are the case in point.
fn facing_gaps(edges: &[Edge], kind: MeasureKind, out: &mut Vec<Measurement>) {
    for (i, e) in edges.iter().enumerate() {
        let mut best: Option<&Edge> = None;
        for (j, f) in edges.iter().enumerate() {
            if i == j || f.pos <= e.pos + MIN_LEN {
                continue;
            }
            let overlap = e.hi.min(f.hi) - e.lo.max(f.lo);
            if overlap < MIN_OVERLAP {
                continue;
            }
            if best.is_none_or(|b| f.pos < b.pos) {
                best = Some(f);
            }
        }
        if let Some(f) = best {
            let mid = (e.lo.max(f.lo) + e.hi.min(f.hi)) / 2.0;
            let gap = f.pos - e.pos;
            let (a, b) = match kind {
                MeasureKind::Horizontal => (Point::new(e.pos, mid), Point::new(f.pos, mid)),
                MeasureKind::Vertical => (Point::new(mid, e.pos), Point::new(mid, f.pos)),
                _ => unreachable!(),
            };
            push_span_dedup(out, a, b, gap.round() as i64, kind);
        }
    }
}

/// Cast a horizontal and a vertical line through the glyph's center and emit
/// a span for each gap between crossings.
///
/// Only gaps where a crossing is on a curve are emitted. Straight-bounded
/// gaps are already covered by `facing_gaps`, so this pass exists to measure
/// all-curve outlines like the `o`.
fn scan_spans(paths: &[Path], out: &mut Vec<Measurement>) {
    let mut bez = BezPath::new();
    for p in paths {
        p.append_to_bezpath(&mut bez);
    }
    if bez.elements().is_empty() {
        return;
    }
    let bbox = bez.bounding_box();
    if bbox.width() < MIN_LEN || bbox.height() < MIN_LEN {
        return;
    }
    let cx = (bbox.x0 + bbox.x1) / 2.0;
    let cy = (bbox.y0 + bbox.y1) / 2.0;

    let hline = Line::new((bbox.x0 - 1.0, cy), (bbox.x1 + 1.0, cy));
    let mut xs: Vec<(f64, bool)> = bez
        .segments()
        .flat_map(|seg| {
            let curved = !matches!(seg, PathSeg::Line(_));
            seg.intersect_line(hline)
                .into_iter()
                .map(move |hit| (hline.eval(hit.line_t).x, curved))
        })
        .collect();
    emit_scan(&mut xs, cy, MeasureKind::Horizontal, out);

    let vline = Line::new((cx, bbox.y0 - 1.0), (cx, bbox.y1 + 1.0));
    let mut ys: Vec<(f64, bool)> = bez
        .segments()
        .flat_map(|seg| {
            let curved = !matches!(seg, PathSeg::Line(_));
            seg.intersect_line(vline)
                .into_iter()
                .map(move |hit| (vline.eval(hit.line_t).y, curved))
        })
        .collect();
    emit_scan(&mut ys, cx, MeasureKind::Vertical, out);
}

/// Sort crossings, merge near-duplicates, and emit a span for each
/// consecutive gap that touches at least one curve.
///
/// Merged near-duplicates OR their curved flags.
fn emit_scan(
    coords: &mut [(f64, bool)],
    fixed: f64,
    kind: MeasureKind,
    out: &mut Vec<Measurement>,
) {
    coords.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut kept: Vec<(f64, bool)> = Vec::with_capacity(coords.len());
    for &(c, curved) in coords.iter() {
        match kept.last_mut() {
            Some(last) if c - last.0 < 2.0 => last.1 = last.1 || curved,
            _ => kept.push((c, curved)),
        }
    }
    for w in kept.windows(2) {
        let gap = w[1].0 - w[0].0;
        if gap < MIN_LEN || !(w[0].1 || w[1].1) {
            continue;
        }
        let (a, b) = match kind {
            MeasureKind::Horizontal => (Point::new(w[0].0, fixed), Point::new(w[1].0, fixed)),
            MeasureKind::Vertical => (Point::new(fixed, w[0].0), Point::new(fixed, w[1].0)),
            _ => unreachable!(),
        };
        push_span_dedup(out, a, b, gap.round() as i64, kind);
    }
}

/// Push a span unless a near-identical one is already present.
///
/// Near-identical means the same kind with endpoints within a few units.
fn push_span_dedup(out: &mut Vec<Measurement>, a: Point, b: Point, length: i64, kind: MeasureKind) {
    let dup = out
        .iter()
        .any(|m| m.kind == kind && (m.a - a).hypot() < 4.0 && (m.b - b).hypot() < 4.0);
    if !dup {
        out.push(Measurement { a, b, length, kind });
    }
}

/// A drawn outline piece tagged with the popcount that colors it.
///
/// A piece is a straight segment, a curve, or a handle line. The tag lets
/// the outline echo the label colors and link each number to its geometry.
#[derive(Clone)]
pub struct ColoredStroke {
    /// The piece in design space.
    pub path: BezPath,
    /// Popcount driving the tier color.
    pub popcount: u32,
    /// True for outline pieces (segments/curves, drawn at the path width),
    /// false for handle lines (drawn thinner).
    pub wide: bool,
}

/// Break each contour into colorable pieces: straight segments and curves at
/// the outline width, plus their handle lines thinner.
///
/// A segment is colored by its own length. A curve is colored by the worse
/// popcount of its two handles.
pub fn colored_strokes(paths: &[Path]) -> Vec<ColoredStroke> {
    let mut out = Vec::new();
    for path in paths {
        let pts = path.points().as_slice();
        let n = pts.len();
        if n < 2 {
            continue;
        }
        let on: Vec<usize> = (0..n).filter(|&i| pts[i].is_on_curve()).collect();
        if on.len() < 2 {
            continue;
        }
        for k in 0..on.len() {
            let a = on[k];
            let b = on[(k + 1) % on.len()];
            // Off-curve points strictly between the two on-curve anchors.
            let mut offs = Vec::new();
            let mut i = (a + 1) % n;
            while i != b {
                offs.push(i);
                i = (i + 1) % n;
            }
            let (pa, pb) = (pts[a].point, pts[b].point);

            match offs.as_slice() {
                [] => push_line(&mut out, pa, pb, true),
                [c] => {
                    let cp = pts[*c].point;
                    let mut bp = BezPath::new();
                    bp.move_to(pa);
                    bp.quad_to(cp, pb);
                    let pc = popcount((cp - pa).hypot().round() as i64)
                        .max(popcount((cp - pb).hypot().round() as i64));
                    out.push(ColoredStroke {
                        path: bp,
                        popcount: pc,
                        wide: true,
                    });
                    push_line(&mut out, pa, cp, false);
                    push_line(&mut out, pb, cp, false);
                }
                [c1, c2] => {
                    let (cp1, cp2) = (pts[*c1].point, pts[*c2].point);
                    let mut bp = BezPath::new();
                    bp.move_to(pa);
                    bp.curve_to(cp1, cp2, pb);
                    let pc = popcount((cp1 - pa).hypot().round() as i64)
                        .max(popcount((cp2 - pb).hypot().round() as i64));
                    out.push(ColoredStroke {
                        path: bp,
                        popcount: pc,
                        wide: true,
                    });
                    push_line(&mut out, pa, cp1, false);
                    push_line(&mut out, pb, cp2, false);
                }
                _ => {}
            }
        }
    }
    out
}

/// Side-bearing geometry, for drawing.
///
/// The gaps run between the advance margins, at x = 0 and x = advance, and
/// the glyph's leftmost and rightmost points. The extreme-point positions
/// come along so the renderer can point at them.
#[derive(Clone, Copy)]
pub struct SideBearings {
    /// The glyph's advance width in design units.
    pub advance: f64,
    /// Left side bearing: rounded gap from x=0 to the leftmost point, in design units.
    pub lsb: i64,
    /// Right side bearing: rounded gap from the rightmost point to the advance, in design units.
    pub rsb: i64,
    /// Extreme x positions of the outline (furthest-left / furthest-right).
    pub min_x: f64,
    /// Furthest-right x position of the outline.
    pub max_x: f64,
    /// y of the leftmost / rightmost point (so the SB line points at it).
    pub y_left: f64,
    /// y of the rightmost point.
    pub y_right: f64,
}

/// Left/right side bearings and the extreme-point positions.
///
/// LSB is the leftmost x, measured from the origin. RSB is the advance minus
/// the rightmost x. Returns `None` for an empty glyph.
pub fn side_bearings(paths: &[Path], advance: f64) -> Option<SideBearings> {
    let mut bez = BezPath::new();
    for p in paths {
        p.append_to_bezpath(&mut bez);
    }
    if bez.elements().is_empty() {
        return None;
    }
    let bbox = bez.bounding_box();
    let (min_x, max_x) = (bbox.x0, bbox.x1);

    // y of the extreme on-curve points, so each SB line ends at the point.
    let mid_y = (bbox.y0 + bbox.y1) / 2.0;
    let (mut y_left, mut y_right) = (mid_y, mid_y);
    let (mut best_l, mut best_r) = (f64::MAX, f64::MAX);
    for p in paths {
        for pt in p.points().as_slice() {
            if !pt.is_on_curve() {
                continue;
            }
            let dl = (pt.point.x - min_x).abs();
            if dl < best_l {
                best_l = dl;
                y_left = pt.point.y;
            }
            let dr = (pt.point.x - max_x).abs();
            if dr < best_r {
                best_r = dr;
                y_right = pt.point.y;
            }
        }
    }

    Some(SideBearings {
        advance,
        lsb: min_x.round() as i64,
        rsb: (advance - max_x).round() as i64,
        min_x,
        max_x,
        y_left,
        y_right,
    })
}

/// Append a colored line piece if it is long enough to be meaningful.
fn push_line(out: &mut Vec<ColoredStroke>, p0: Point, p1: Point, wide: bool) {
    let len = (p1 - p0).hypot();
    if len < MIN_LEN {
        return;
    }
    let mut bp = BezPath::new();
    bp.move_to(p0);
    bp.line_to(p1);
    out.push(ColoredStroke {
        path: bp,
        popcount: popcount(len.round() as i64),
        wide,
    });
}

/// Popcount (Hamming weight) of a length: the number of powers of two it is
/// the sum of. Lower = more structural.
pub fn popcount(value: i64) -> u32 {
    (value.max(0) as u64).count_ones()
}

/// The label for a length: `"96 = 64+32"`, `"256 = 2^8"`.
///
/// Pure powers get an exponent. Sums list their powers high-to-low. Caret
/// notation is used rather than Unicode superscripts because the embedded
/// HUD font has no superscript glyphs; they render as tofu.
pub fn label(value: i64) -> String {
    if value <= 0 {
        return value.to_string();
    }
    let v = value as u64;
    if v.is_power_of_two() {
        return format!("{value} = 2^{}", v.trailing_zeros());
    }
    let mut parts = Vec::new();
    for bit in (0..64).rev() {
        if v & (1u64 << bit) != 0 {
            parts.push((1u64 << bit).to_string());
        }
    }
    format!("{value} = {}", parts.join("+"))
}

/// The y-extent of a glyph's ink at one joining edge.
///
/// The extent covers outline points, with components resolved, at
/// or past x = 0 going left, or at or past x = advance going right.
/// The test is one-sided because joining strokes overlap the edge
/// on purpose: the anti-seam tongue.
///
/// Returns `None` when nothing reaches the edge. For a form that
/// should join, that is itself the defect.
pub fn joining_band(
    outline: &BezPath,
    advance: f64,
    left: bool,
    tolerance: f64,
) -> Option<(f64, f64)> {
    let mut band: Option<(f64, f64)> = None;
    let mut visit = |p: kurbo::Point| {
        let reaches = if left {
            p.x <= tolerance
        } else {
            p.x >= advance - tolerance
        };
        if !reaches {
            return;
        }
        band = Some(match band {
            Some((lo, hi)) => (lo.min(p.y), hi.max(p.y)),
            None => (p.y, p.y),
        });
    };
    for el in outline.elements() {
        match el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => visit(*p),
            PathEl::QuadTo(c, p) => {
                visit(*c);
                visit(*p);
            }
            PathEl::CurveTo(c1, c2, p) => {
                visit(*c1);
                visit(*c2);
                visit(*p);
            }
            PathEl::ClosePath => {}
        }
    }
    band
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_popcount() {
        assert_eq!(label(256), "256 = 2^8");
        assert_eq!(label(96), "96 = 64+32");
        assert_eq!(label(272), "272 = 256+16");
        assert_eq!(popcount(256), 1);
        assert_eq!(popcount(96), 2);
        assert_eq!(popcount(116), 4); // 64+32+16+4
    }
}
