//! Curve-smoothness analysis and editing geometry, shared by all
//! Runebender editors (web, xilem, gpui): curvature comb, per-node
//! continuity, harmonize/balance/optimize.
//!
//! The design system's power-of-two discipline is a means (model-friendly
//! data), not the goal: curve continuity outranks popcount (see virtua-grotesk
//! DESIGN.md, "Curve smoothness comes before popcount"). This module gives the
//! editor the tools to see and enforce that — a Speedpunk-style curvature comb,
//! per-node continuity (G0/G1/G2/G3), and Curvatura/SuperTool harmonize + Tunni
//! balance operations.
//!
//! Pure design-space geometry (font units), no render deps — unit-tested on
//! native `cargo test`. Formulas verified against Simon Cozens' SuperTool and
//! Linus Romer's Curvatura.

use kurbo::{Point, Vec2};

/// A cubic segment of an outline: on-curve `p0`/`p3`, off-curve handles
/// `p1`/`p2`. A straight line is stored as a cubic with handles on the chord
/// (`straight = true`, curvature 0).
#[derive(Clone, Copy, Debug)]
pub struct Cubic {
    /// Start on-curve point.
    pub p0: Point,
    /// First off-curve handle; lies on the chord when `straight` is set.
    pub p1: Point,
    /// Second off-curve handle; lies on the chord when `straight` is set.
    pub p2: Point,
    /// End on-curve point.
    pub p3: Point,
    /// Whether the source segment is a straight line stored in cubic form.
    pub straight: bool,
    /// Whether the on-curve point starting this segment (`p0`) is smooth.
    pub start_smooth: bool,
}

/// Build the per-contour cubic segment lists from a norad glyph, for
/// the comb/continuity analyses: lines become degenerate "straight"
/// cubics, quads elevate, hyper contours run through the solver.
pub fn cubics_from_norad(glyph: &norad::Glyph) -> Vec<Vec<Cubic>> {
    let mut out = Vec::new();
    for contour in &glyph.contours {
        let path = if crate::outline::path::hyper_model::norad_contour_is_hyper(contour) {
            let ws = crate::outline::path::hyper_model::Contour::from_norad(contour);
            let mut bez = kurbo::BezPath::new();
            crate::outline::path::Path::from_contour(&ws).append_to_bezpath(&mut bez);
            bez
        } else {
            crate::outline::glyph_paths::contour_to_bezpath(contour)
        };
        let mut segs: Vec<Cubic> = Vec::new();
        let mut current = Point::ZERO;
        let mut start = Point::ZERO;
        for el in path.elements() {
            match *el {
                kurbo::PathEl::MoveTo(p) => {
                    current = p;
                    start = p;
                }
                kurbo::PathEl::LineTo(p) => {
                    segs.push(Cubic {
                        p0: current,
                        p1: current.lerp(p, 1.0 / 3.0),
                        p2: current.lerp(p, 2.0 / 3.0),
                        p3: p,
                        straight: true,
                        start_smooth: false,
                    });
                    current = p;
                }
                kurbo::PathEl::QuadTo(c, p) => {
                    let c1 = current + (c - current) * (2.0 / 3.0);
                    let c2 = p + (c - p) * (2.0 / 3.0);
                    segs.push(Cubic {
                        p0: current,
                        p1: c1,
                        p2: c2,
                        p3: p,
                        straight: false,
                        start_smooth: false,
                    });
                    current = p;
                }
                kurbo::PathEl::CurveTo(c1, c2, p) => {
                    segs.push(Cubic {
                        p0: current,
                        p1: c1,
                        p2: c2,
                        p3: p,
                        straight: false,
                        start_smooth: false,
                    });
                    current = p;
                }
                kurbo::PathEl::ClosePath => {
                    if current.distance(start) > 1e-9 {
                        segs.push(Cubic {
                            p0: current,
                            p1: current.lerp(start, 1.0 / 3.0),
                            p2: current.lerp(start, 2.0 / 3.0),
                            p3: start,
                            straight: true,
                            start_smooth: false,
                        });
                    }
                    current = start;
                }
            }
        }
        // Smooth flags from the source points, matched by position.
        for seg in segs.iter_mut() {
            let p0 = seg.p0;
            seg.start_smooth = contour
                .points
                .iter()
                .any(|p| p.smooth && (p.x - p0.x).abs() < 0.01 && (p.y - p0.y).abs() < 0.01);
        }
        if !segs.is_empty() {
            out.push(segs);
        }
    }
    out
}

/// 2D cross product `u × v = u.x·v.y − u.y·v.x`.
fn cross(u: Vec2, v: Vec2) -> f64 {
    u.x * v.y - u.y * v.x
}

impl Cubic {
    /// First derivative at `t` (power-basis form).
    fn deriv(&self, t: f64) -> Vec2 {
        let a = (self.p3 - self.p0) + (self.p1 - self.p2) * 3.0;
        let b = (self.p2 - self.p1) * 3.0 - (self.p1 - self.p0) * 3.0;
        let c = (self.p1 - self.p0) * 3.0;
        a * (3.0 * t * t) + b * (2.0 * t) + c
    }

    fn deriv2(&self, t: f64) -> Vec2 {
        let a = (self.p3 - self.p0) + (self.p1 - self.p2) * 3.0;
        let b = (self.p2 - self.p1) * 3.0 - (self.p1 - self.p0) * 3.0;
        a * (6.0 * t) + b * 2.0
    }

    /// Point at `t`.
    pub fn eval(&self, t: f64) -> Point {
        let mt = 1.0 - t;
        (self.p0.to_vec2() * (mt * mt * mt)
            + self.p1.to_vec2() * (3.0 * mt * mt * t)
            + self.p2.to_vec2() * (3.0 * mt * t * t)
            + self.p3.to_vec2() * (t * t * t))
            .to_point()
    }

    /// Signed curvature at `t` — κ = (r'×r'') / |r'|³.
    pub fn curvature(&self, t: f64) -> f64 {
        if self.straight {
            return 0.0;
        }
        let d1 = self.deriv(t);
        let d2 = self.deriv2(t);
        let speed = d1.hypot();
        if speed < 1e-9 {
            return 0.0;
        }
        cross(d1, d2) / (speed * speed * speed)
    }

    /// Signed curvature at the start (`t=0`), closed form.
    /// κ(0) = (2/3)·cross(P1−P0, P2−P0) / |P1−P0|³.
    fn curvature_start(&self) -> f64 {
        if self.straight {
            return 0.0;
        }
        let h = self.p1 - self.p0;
        let len = h.hypot();
        if len < 1e-9 {
            return 0.0;
        }
        (2.0 / 3.0) * cross(h, self.p2 - self.p0) / (len * len * len)
    }

    /// Signed curvature at the end (`t=1`), closed form.
    /// κ(1) = (2/3)·cross(P3−P2, P1−P2) / |P3−P2|³.
    fn curvature_end(&self) -> f64 {
        if self.straight {
            return 0.0;
        }
        let h = self.p3 - self.p2;
        let len = h.hypot();
        if len < 1e-9 {
            return 0.0;
        }
        (2.0 / 3.0) * cross(h, self.p1 - self.p2) / (len * len * len)
    }

    /// Incoming tangent direction at the end (`t=1`).
    fn tangent_end(&self) -> Vec2 {
        let t = self.p3 - self.p2;
        if t.hypot() > 1e-9 {
            t
        } else {
            self.p3 - self.p0
        }
    }

    /// Outgoing tangent direction at the start (`t=0`).
    fn tangent_start(&self) -> Vec2 {
        let t = self.p1 - self.p0;
        if t.hypot() > 1e-9 {
            t
        } else {
            self.p3 - self.p0
        }
    }
}

/// Geometric-continuity level achieved at an on-curve node.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GLevel {
    /// Intended corner (the node is not marked smooth).
    Corner,
    /// A node marked smooth whose tangents don't line up — a kink (defect).
    Kink,
    /// A line↔curve smooth join: G1 is the best achievable (curvature must
    /// jump 0→κ). Intended and acceptable (e.g. a stem meeting a bowl).
    G1Line,
    /// Tangent-continuous only, curve↔curve — a harmonize candidate.
    G1,
    /// Curvature-continuous.
    G2,
    /// Curvature-derivative-continuous.
    G3,
}

/// Continuity of one on-curve node joining an incoming and outgoing cubic.
#[derive(Clone, Copy, Debug)]
pub struct NodeContinuity {
    /// The on-curve point where the two segments meet.
    pub at: Point,
    /// The highest continuity level the join satisfies.
    pub level: GLevel,
    /// Relative curvature mismatch across the join (0 = perfectly G2).
    pub jump: f64,
}

/// Tangent-collinearity tolerance for G1 (~0.5°), tight enough to catch
/// integer-rounding kinks at points the designer marked smooth.
const G1_ANGLE_TOL: f64 = 0.009; // radians
/// Relative curvature tolerance for G2.
const G2_REL_TOL: f64 = 0.05;
/// Relative dκ tolerance for G3.
const G3_REL_TOL: f64 = 0.08;

/// Classify the continuity of every on-curve node of a glyph's contours.
pub fn node_continuity(contours: &[Vec<Cubic>]) -> Vec<NodeContinuity> {
    let mut out = Vec::new();
    for segs in contours {
        let n = segs.len();
        if n < 2 {
            continue;
        }
        // Node k joins seg[k-1] (incoming, ends at the node) and seg[k]
        // (outgoing, starts at the node). p0 of seg[k] is the node.
        for k in 0..n {
            let out_seg = &segs[k];
            let in_seg = &segs[(k + n - 1) % n];
            let node = out_seg.p0;
            let level = classify(in_seg, out_seg);
            let (ki, ko) = (in_seg.curvature_end(), out_seg.curvature_start());
            let jump = (ki - ko).abs() / ki.abs().max(ko.abs()).max(1e-6);
            out.push(NodeContinuity {
                at: node,
                level,
                jump,
            });
        }
    }
    out
}

fn classify(in_seg: &Cubic, out_seg: &Cubic) -> GLevel {
    if !out_seg.start_smooth {
        return GLevel::Corner;
    }
    let ti = in_seg.tangent_end();
    let to = out_seg.tangent_start();
    let (li, lo) = (ti.hypot(), to.hypot());
    if li < 1e-9 || lo < 1e-9 {
        return GLevel::Corner;
    }
    let dot = ti.dot(to);
    let angle = cross(ti, to).abs().atan2(dot);
    if dot <= 0.0 || angle > G1_ANGLE_TOL {
        return GLevel::Kink;
    }
    // Two straight segments meeting smoothly are trivially G-continuous.
    if in_seg.straight && out_seg.straight {
        return GLevel::G2;
    }
    // A line meeting a curve can only reach G1 (curvature jumps 0→κ); that is
    // the intended best, not a defect.
    if in_seg.straight || out_seg.straight {
        return GLevel::G1Line;
    }
    let ki = in_seg.curvature_end();
    let ko = out_seg.curvature_start();
    let denom = ki.abs().max(ko.abs()).max(1e-6);
    let rel = (ki - ko).abs() / denom;
    if rel > G2_REL_TOL {
        return GLevel::G1;
    }
    // G3: compare dκ/ds just inside each side.
    let dki = (in_seg.curvature(1.0) - in_seg.curvature(0.97)) / 0.03;
    let dko = (out_seg.curvature(0.03) - out_seg.curvature(0.0)) / 0.03;
    let ddenom = dki.abs().max(dko.abs()).max(1e-6);
    if (dki - dko).abs() / ddenom < G3_REL_TOL {
        GLevel::G3
    } else {
        GLevel::G2
    }
}

/// One rib of the curvature-comb envelope: the point on the curve and the
/// pushed-out point, plus the curvature magnitude for coloring.
#[derive(Clone, Copy, Debug)]
pub struct CombSample {
    /// The sampled point on the curve.
    pub on: Point,
    /// The point pushed out along the normal by the scaled curvature.
    pub outer: Point,
    /// Signed curvature magnitude at the sample, for coloring.
    pub kappa: f64,
}

/// Build the curvature comb for a glyph: per curved segment, a strip of
/// samples pushed out along the normal by `gain·|κ|·scale`. `scale` is a
/// design-space factor (so the comb zooms with the outline); `gain` is the
/// user multiplier. Straight segments are skipped (κ = 0). `signed` keeps the
/// curvature sign so the comb flips side at inflections.
pub fn curvature_comb(
    contours: &[Vec<Cubic>],
    gain: f64,
    scale: f64,
    signed: bool,
    samples: usize,
) -> Vec<Vec<CombSample>> {
    let mut strips = Vec::new();
    let n = samples.max(2);
    for contour in contours {
        for seg in contour {
            if seg.straight {
                continue;
            }
            let mut strip = Vec::with_capacity(n + 1);
            for i in 0..=n {
                let t = i as f64 / n as f64;
                let d1 = seg.deriv(t);
                let speed = d1.hypot();
                if speed < 1e-9 {
                    continue;
                }
                let k = seg.curvature(t);
                // Unit normal = tangent rotated −90°: (d1.y, −d1.x)/|d1|.
                let normal = Vec2::new(d1.y / speed, -d1.x / speed);
                let mag = if signed { k } else { k.abs() };
                let on = seg.eval(t);
                let outer = on + normal * (mag * gain * scale);
                strip.push(CombSample {
                    on,
                    outer,
                    kappa: k,
                });
            }
            if strip.len() >= 2 {
                strips.push(strip);
            }
        }
    }
    strips
}

/// Peak |κ| across all curved segments — for auto-scaling the comb so the
/// tallest rib is a readable height.
pub fn max_curvature(contours: &[Vec<Cubic>]) -> f64 {
    let mut m: f64 = 0.0;
    for contour in contours {
        for seg in contour {
            if seg.straight {
                continue;
            }
            for i in 0..=24 {
                m = m.max(seg.curvature(i as f64 / 24.0).abs());
            }
        }
    }
    m
}

/// Infinite-line intersection of line (a,b) with line (c,d). `None` if parallel.
fn line_intersect(a: Point, b: Point, c: Point, d: Point) -> Option<Point> {
    let r = b - a;
    let s = d - c;
    let denom = cross(r, s);
    if denom.abs() < 1e-9 {
        return None;
    }
    let t = cross(c - a, s) / denom;
    Some(a + r * t)
}

/// Harmonize a smooth on-curve `node`: given the incoming handles `a1`,`a2`
/// (`a2` adjacent to the node) and outgoing handles `b1`,`b2` (`b1` adjacent),
/// return the new positions of the two adjacent handles that make the join
/// curvature-continuous (G2) while keeping the on-curve point fixed
/// (SuperTool / Curvatura). `None` for degenerate configurations.
pub fn harmonize(
    a1: Point,
    a2: Point,
    node: Point,
    b1: Point,
    b2: Point,
) -> Option<(Point, Point)> {
    let d = line_intersect(a1, a2, b1, b2)?;
    let p0 = (a2 - a1).hypot() / (d - a2).hypot();
    let p1 = (b1 - d).hypot() / (b2 - b1).hypot();
    let r = (p0 * p1).sqrt();
    if !r.is_finite() {
        return None;
    }
    let t = r / (r + 1.0);
    let new_node = a2.lerp(b1, t);
    let fixup = node - new_node;
    Some((a2 + fixup, b1 + fixup))
}

/// Balance a cubic segment's handles (Tunni): move both handles to the same
/// fractional distance toward the Tunni point (handle-line intersection),
/// keeping their directions and the on-curve endpoints. Returns the new
/// `(p1, p2)`. `None` at inflections / degenerate segments.
pub fn balance(p0: Point, p1: Point, p2: Point, p3: Point) -> Option<(Point, Point)> {
    let s = line_intersect(p0, p1, p3, p2)?;
    let sd = (s - p0).hypot();
    let ed = (s - p3).hypot();
    if sd <= 1e-9 || ed <= 1e-9 {
        return None;
    }
    let x = (p1 - p0).hypot() / sd;
    let y = (p2 - p3).hypot() / ed;
    if (x > 1.0 && y > 1.0) || (x < 0.01 && y < 0.01) {
        return None;
    }
    let avg = (x + y) / 2.0;
    Some((p0.lerp(s, avg), p3.lerp(s, avg)))
}

/// Popcount (Hamming weight) — number of powers of two a length is the sum of.
pub fn popcount(v: i64) -> u32 {
    (v.max(0) as u64).count_ones()
}

/// Round a point to the nearest even integer on both axes — the 2-unit
/// design grid Virtua's coordinates all live on.
fn round_even(p: Point) -> Point {
    Point::new((p.x / 2.0).round() * 2.0, (p.y / 2.0).round() * 2.0)
}

fn round_even_scalar(v: f64) -> i64 {
    ((v / 2.0).round() * 2.0) as i64
}

/// Even integers within `w` of `center` (center should already be even).
fn even_range(center: i64, w: i64) -> Vec<i64> {
    let mut out = Vec::new();
    let mut d = -w;
    while d <= w {
        out.push(center + d);
        d += 2;
    }
    out
}

/// Signed curvature at a cubic's start (t=0), closed form. p3 unused.
pub fn curvature_start(p0: Point, p1: Point, p2: Point, _p3: Point) -> f64 {
    let h = p1 - p0;
    let len = h.hypot();
    if len < 1e-9 {
        return 0.0;
    }
    (2.0 / 3.0) * cross(h, p2 - p0) / (len * len * len)
}

/// Signed curvature at a cubic's end (t=1), closed form. p0 unused.
pub fn curvature_end(_p0: Point, p1: Point, p2: Point, p3: Point) -> f64 {
    let h = p3 - p2;
    let len = h.hypot();
    if len < 1e-9 {
        return 0.0;
    }
    (2.0 / 3.0) * cross(h, p1 - p2) / (len * len * len)
}

/// Variance of curvature sampled along a cubic — 0 for a circular arc.
fn curvature_variance(p0: Point, p1: Point, p2: Point, p3: Point) -> f64 {
    let c = Cubic {
        p0,
        p1,
        p2,
        p3,
        straight: false,
        start_smooth: false,
    };
    let n = 12;
    let ks: Vec<f64> = (0..=n).map(|i| c.curvature(i as f64 / n as f64)).collect();
    let m = ks.iter().sum::<f64>() / ks.len() as f64;
    ks.iter().map(|k| (k - m) * (k - m)).sum::<f64>() / ks.len() as f64
}

/// One point for the optimizer.
#[derive(Clone, Copy)]
pub struct OptPoint {
    /// Position in design space.
    pub p: Point,
    /// Whether the point is on-curve; off-curve handles may move.
    pub on: bool,
    /// Whether an on-curve point is smooth; the optimizer keeps its tangent continuous.
    pub smooth: bool,
}

/// Optimize a closed cubic contour's handles: balance → harmonize → balance
/// (continuity + even curvature), then snap each handle's length to the
/// lowest-popcount even value that doesn't worsen the local curvature/G2
/// beyond `tol`. On-curve points never move. Returns new positions.
pub fn optimize_contour(pts: &[OptPoint], tol: f64) -> Vec<Point> {
    let n = pts.len();
    let mut q: Vec<Point> = pts.iter().map(|x| x.p).collect();
    let on: Vec<bool> = pts.iter().map(|x| x.on).collect();
    if n < 4 {
        return q;
    }
    // Even the handle tension (direction-preserving, so continuity/G1 is
    // never disturbed), then snap onto the grid toward clean popcounts.
    balance_all(&mut q, &on, n);
    for _ in 0..2 {
        snap_all(&mut q, &on, n, tol);
    }
    for (i, p) in q.iter_mut().enumerate() {
        if !on[i] {
            *p = round_even(*p);
        }
    }
    q
}

fn balance_all(q: &mut [Point], on: &[bool], n: usize) {
    for i in 0..n {
        let (b, c, d) = ((i + 1) % n, (i + 2) % n, (i + 3) % n);
        if !on[i] || on[b] || on[c] || !on[d] {
            continue;
        }
        if let Some((np1, np2)) = balance(q[i], q[b], q[c], q[d]) {
            q[b] = np1;
            q[c] = np2;
        }
    }
}

fn snap_all(q: &mut [Point], on: &[bool], n: usize, tol: f64) {
    for h in 0..n {
        if on[h] {
            continue;
        }
        let anchor = if on[(h + n - 1) % n] {
            q[(h + n - 1) % n]
        } else if on[(h + 1) % n] {
            q[(h + 1) % n]
        } else {
            continue;
        };
        let smooth_pos = q[h];
        let d = smooth_pos - anchor;
        // A handle that runs horizontally/vertically from its node sits at an
        // extremum: pin the perpendicular coordinate to the node so the two
        // handles stay colinear (flat tangent) and on-grid. Only the along-axis
        // length is free to snap. Diagonal handles snap on both axes.
        let lock_x = d.x.abs() < 0.5; // vertical handle → x pinned
        let lock_y = d.y.abs() < 0.5; // horizontal handle → y pinned
        let ax = anchor.x.round() as i64;
        let ay = anchor.y.round() as i64;
        let base_x = if lock_x {
            ax
        } else {
            round_even_scalar(smooth_pos.x)
        };
        let base_y = if lock_y {
            ay
        } else {
            round_even_scalar(smooth_pos.y)
        };
        q[h] = Point::new(base_x as f64, base_y as f64);
        let base = local_cost(q, on, n, h);
        // Candidates: even values on the free axes only, ordered by the
        // popcount of the delta from the anchor (prefers 8s / powers of two),
        // then by closeness to the smooth position.
        let w = 8;
        let xs = if lock_x {
            vec![ax]
        } else {
            even_range(base_x, w)
        };
        let ys = if lock_y {
            vec![ay]
        } else {
            even_range(base_y, w)
        };
        let mut cands: Vec<(i64, i64)> = Vec::new();
        for &x in &xs {
            for &y in &ys {
                cands.push((x, y));
            }
        }
        cands.sort_by(|a, b| {
            let pa = popcount((a.0 - ax).abs()) + popcount((a.1 - ay).abs());
            let pb = popcount((b.0 - ax).abs()) + popcount((b.1 - ay).abs());
            let da = (a.0 as f64 - smooth_pos.x).hypot(a.1 as f64 - smooth_pos.y);
            let db = (b.0 as f64 - smooth_pos.x).hypot(b.1 as f64 - smooth_pos.y);
            pa.cmp(&pb).then(da.partial_cmp(&db).unwrap())
        });
        let mut best = Point::new(base_x as f64, base_y as f64);
        for (x, y) in cands {
            q[h] = Point::new(x as f64, y as f64);
            if local_cost(q, on, n, h) <= base * (1.0 + tol) + 1e-9 {
                best = q[h];
                break;
            }
        }
        q[h] = best;
    }
}

/// Local curvature cost around off-curve handle `h`: variance of its segment
/// plus a heavier G2-mismatch penalty at that segment's two joins.
fn local_cost(q: &[Point], on: &[bool], n: usize, h: usize) -> f64 {
    let s0 = if on[(h + n - 1) % n] {
        (h + n - 1) % n
    } else if on[(h + n - 2) % n] {
        (h + n - 2) % n
    } else {
        return 0.0;
    };
    let (s1, s2, s3) = ((s0 + 1) % n, (s0 + 2) % n, (s0 + 3) % n);
    if !on[s0] || on[s1] || on[s2] || !on[s3] {
        return 0.0;
    }
    let seg = [q[s0], q[s1], q[s2], q[s3]];
    let mut cost = curvature_variance(seg[0], seg[1], seg[2], seg[3]) * 1e6;
    // G2 with the previous segment (ending at s0).
    let p0 = (s0 + n - 3) % n;
    if on[p0] && !on[(p0 + 1) % n] && !on[(p0 + 2) % n] {
        let ke = curvature_end(q[p0], q[(p0 + 1) % n], q[(p0 + 2) % n], seg[0]);
        cost += (ke - curvature_start(seg[0], seg[1], seg[2], seg[3])).abs() * 1e5;
    }
    // G2 with the next segment (starting at s3).
    let ne = (s3 + 3) % n;
    if on[ne] && !on[(s3 + 1) % n] && !on[(s3 + 2) % n] {
        let ks = curvature_start(q[s3], q[(s3 + 1) % n], q[(s3 + 2) % n], q[ne]);
        cost += (curvature_end(seg[0], seg[1], seg[2], seg[3]) - ks).abs() * 1e5;
    }
    cost
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popcount_and_grid() {
        assert_eq!(popcount(128), 1);
        assert_eq!(popcount(192), 2); // 128 + 64
        assert_eq!(popcount(139), 4); // odd, off-grid, noisy
        // round_even keeps both axes on the 2-grid.
        let r = round_even(Point::new(103.2, 137.6));
        assert_eq!(r, Point::new(104.0, 138.0));
    }

    #[test]
    fn optimize_keeps_on_curve_fixed_and_smooth() {
        // Quarter-ish circle-ish closed contour (unit-ish), on/off/off/on...
        let r = 100.0;
        let k = 0.5522847 * r;
        let pts = vec![
            OptPoint {
                p: Point::new(r, 0.0),
                on: true,
                smooth: true,
            },
            OptPoint {
                p: Point::new(r, k),
                on: false,
                smooth: false,
            },
            OptPoint {
                p: Point::new(k, r),
                on: false,
                smooth: false,
            },
            OptPoint {
                p: Point::new(0.0, r),
                on: true,
                smooth: true,
            },
            OptPoint {
                p: Point::new(-k, r),
                on: false,
                smooth: false,
            },
            OptPoint {
                p: Point::new(-r, k),
                on: false,
                smooth: false,
            },
            OptPoint {
                p: Point::new(-r, 0.0),
                on: true,
                smooth: true,
            },
            OptPoint {
                p: Point::new(-r, -k),
                on: false,
                smooth: false,
            },
            OptPoint {
                p: Point::new(-k, -r),
                on: false,
                smooth: false,
            },
            OptPoint {
                p: Point::new(0.0, -r),
                on: true,
                smooth: true,
            },
            OptPoint {
                p: Point::new(k, -r),
                on: false,
                smooth: false,
            },
            OptPoint {
                p: Point::new(r, -k),
                on: false,
                smooth: false,
            },
        ];
        let out = optimize_contour(&pts, 0.12);
        // On-curve points are unchanged.
        for (i, op) in pts.iter().enumerate() {
            if op.on {
                assert!((out[i] - op.p).hypot() < 1e-6);
            }
        }
        // Every off-curve handle lands on the 2-unit grid (even coords).
        for (i, op) in pts.iter().enumerate() {
            if !op.on {
                assert_eq!(out[i].x as i64 % 2, 0, "x off-grid: {}", out[i].x);
                assert_eq!(out[i].y as i64 % 2, 0, "y off-grid: {}", out[i].y);
            }
        }
        // Handles stay near the circle's control length (didn't blow up).
        assert!((out[1] - Point::new(r, 0.0)).hypot() < r);
        // Extremum handles keep their axis: the two around the top node
        // (0, r) stay horizontal (y == r); the two around the right node
        // (r, 0) stay vertical (x == r). Colinearity preserved.
        assert_eq!(out[2].y, r, "top-right handle left its flat tangent");
        assert_eq!(out[4].y, r, "top-left handle left its flat tangent");
        assert_eq!(out[1].x, r, "right-top handle left its vertical tangent");
        assert_eq!(
            out[11].x, r,
            "right-bottom handle left its vertical tangent"
        );
    }

    // A cubic approximating a quarter circle of radius r: handle length
    // k·r with k = 4/3·(√2−1) ≈ 0.5523. Curvature ≈ 1/r.
    fn quarter_circle(r: f64) -> Cubic {
        let k = 0.5522847498 * r;
        Cubic {
            p0: Point::new(r, 0.0),
            p1: Point::new(r, k),
            p2: Point::new(k, r),
            p3: Point::new(0.0, r),
            straight: false,
            start_smooth: true,
        }
    }

    #[test]
    fn curvature_of_circle() {
        let c = quarter_circle(100.0);
        // Endpoint curvature magnitude ~ 1/100 (sign depends on winding).
        assert!((c.curvature_start().abs() - 0.01).abs() < 0.001);
        assert!((c.curvature_end().abs() - 0.01).abs() < 0.001);
    }

    #[test]
    fn straight_has_zero_curvature() {
        let s = Cubic {
            p0: Point::new(0.0, 0.0),
            p1: Point::new(33.0, 0.0),
            p2: Point::new(66.0, 0.0),
            p3: Point::new(100.0, 0.0),
            straight: true,
            start_smooth: false,
        };
        assert_eq!(s.curvature(0.5), 0.0);
    }

    #[test]
    fn harmonize_makes_join_g2() {
        // Two cubics meeting smoothly at the origin, shared vertical tangent,
        // both curving the same way (G1 but not G2 — different curvatures).
        let node = Point::new(0.0, 0.0);
        let a2 = Point::new(0.0, -30.0); // incoming handle adjacent to node
        let a1 = Point::new(-70.0, -50.0); // far incoming handle
        let b1 = Point::new(0.0, 50.0); // outgoing handle adjacent to node
        let b2 = Point::new(-80.0, 80.0); // far outgoing handle
        let inc = Cubic {
            p0: Point::new(-100.0, -90.0),
            p1: a1,
            p2: a2,
            p3: node,
            straight: false,
            start_smooth: true,
        };
        let out = Cubic {
            p0: node,
            p1: b1,
            p2: b2,
            p3: Point::new(-120.0, 120.0),
            straight: false,
            start_smooth: true,
        };
        let before = (inc.curvature_end() - out.curvature_start()).abs();
        let (na2, nb1) = harmonize(a1, a2, node, b1, b2).unwrap();
        let inc2 = Cubic { p2: na2, ..inc };
        let out2 = Cubic { p1: nb1, ..out };
        let after = (inc2.curvature_end() - out2.curvature_start()).abs();
        assert!(
            after < before * 0.3,
            "harmonize should nearly equalize curvature: {before} -> {after}"
        );
    }

    #[test]
    fn balance_keeps_endpoints() {
        let (p0, p3) = (Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        let (np1, np2) = balance(p0, Point::new(20.0, 50.0), Point::new(90.0, 60.0), p3).unwrap();
        // Handles stay on their original rays from the endpoints.
        assert!((np1 - p0).cross(Point::new(20.0, 50.0) - p0).abs() < 1e-6);
        assert!((np2 - p3).cross(Point::new(90.0, 60.0) - p3).abs() < 1e-6);
    }
}
