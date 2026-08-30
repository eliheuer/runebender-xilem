// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Conversion between quadratic and cubic outlines.

/// Rewrite quadratic segments as exact cubics: each offcurve+qcurve
/// pair (P0, C, P1) becomes the identical cubic with controls at
/// P0 + 2/3(C-P0) and P1 + 2/3(C-P1). Lossless.
pub fn quads_to_cubics(glyph: &mut norad::Glyph) -> bool {
    use norad::PointType;
    let mut changed = false;
    for contour in glyph.contours.iter_mut() {
        let n = contour.points.len();
        if n < 3 {
            continue;
        }
        let has_quads = contour.points.iter().any(|p| p.typ == PointType::QCurve);
        if !has_quads {
            continue;
        }
        let old = contour.points.clone();
        let mut points: Vec<norad::ContourPoint> = Vec::with_capacity(n + 4);
        for (i, p) in old.iter().enumerate() {
            if p.typ != PointType::QCurve {
                points.push(p.clone());
                continue;
            }
            // The single offcurve before this qcurve, and the on-point
            // before that.
            let ci = (i + n - 1) % n;
            let oi = (i + n - 2) % n;
            let (c, p0) = (&old[ci], &old[oi]);
            if c.typ != PointType::OffCurve || p0.typ == PointType::OffCurve {
                points.push(p.clone());
                continue;
            }
            // Replace the emitted offcurve with the two cubic ones.
            let popped = points.pop();
            debug_assert!(popped.is_some_and(|q| q.typ == PointType::OffCurve));
            let c1 = (
                p0.x + (c.x - p0.x) * 2.0 / 3.0,
                p0.y + (c.y - p0.y) * 2.0 / 3.0,
            );
            let c2 = (p.x + (c.x - p.x) * 2.0 / 3.0, p.y + (c.y - p.y) * 2.0 / 3.0);
            let off = |x: f64, y: f64| {
                norad::ContourPoint::new(
                    x.round(),
                    y.round(),
                    PointType::OffCurve,
                    false,
                    None,
                    None,
                )
            };
            points.push(off(c1.0, c1.1));
            points.push(off(c2.0, c2.1));
            points.push(norad::ContourPoint::new(
                p.x,
                p.y,
                PointType::Curve,
                p.smooth,
                None,
                None,
            ));
            changed = true;
        }
        if changed {
            contour.points = points;
        }
    }
    changed
}

/// Approximate cubic segments with quadratics: each cubic splits in
/// halves until one quad (control from the 3/4 rule) sits within
/// `tolerance` of it, then the quads replace the cubic. The reverse
/// of quads_to_cubics, lossy by nature — the same trade every
/// cubic-to-TrueType compiler makes.
pub fn cubics_to_quads(glyph: &mut norad::Glyph, tolerance: f64) -> bool {
    use kurbo::{CubicBez, ParamCurve as _, Point};
    use norad::PointType;
    fn approx(cubic: CubicBez, tolerance: f64, out: &mut Vec<(Point, Point)>) {
        // One-quad candidate: Q = (3(c1+c2) − (p0+p3)) / 4.
        let q = Point::new(
            (3.0 * (cubic.p1.x + cubic.p2.x) - (cubic.p0.x + cubic.p3.x)) / 4.0,
            (3.0 * (cubic.p1.y + cubic.p2.y) - (cubic.p0.y + cubic.p3.y)) / 4.0,
        );
        let quad = kurbo::QuadBez::new(cubic.p0, q, cubic.p3);
        let err = [0.25, 0.5, 0.75]
            .iter()
            .map(|&t| cubic.eval(t).distance(quad.eval(t)))
            .fold(0.0_f64, f64::max);
        if err <= tolerance || out.len() > 64 {
            out.push((q, cubic.p3));
        } else {
            let (a, b) = cubic.subdivide();
            approx(a, tolerance, out);
            approx(b, tolerance, out);
        }
    }
    let mut changed = false;
    for contour in glyph.contours.iter_mut() {
        let n = contour.points.len();
        if n < 4 {
            continue;
        }
        let has_cubics = contour.points.iter().any(|p| p.typ == PointType::Curve);
        if !has_cubics {
            continue;
        }
        let old = contour.points.clone();
        let mut points: Vec<norad::ContourPoint> = Vec::new();
        for (i, p) in old.iter().enumerate() {
            if p.typ != PointType::Curve {
                points.push(p.clone());
                continue;
            }
            let c2i = (i + n - 1) % n;
            let c1i = (i + n - 2) % n;
            let p0i = (i + n - 3) % n;
            let (c2, c1, p0) = (&old[c2i], &old[c1i], &old[p0i]);
            if c1.typ != PointType::OffCurve
                || c2.typ != PointType::OffCurve
                || p0.typ == PointType::OffCurve
            {
                points.push(p.clone());
                continue;
            }
            // Drop the two emitted cubic offcurves.
            points.pop();
            points.pop();
            let cubic = CubicBez::new(
                Point::new(p0.x, p0.y),
                Point::new(c1.x, c1.y),
                Point::new(c2.x, c2.y),
                Point::new(p.x, p.y),
            );
            let mut quads = Vec::new();
            approx(cubic, tolerance, &mut quads);
            for (k, (control, end)) in quads.iter().enumerate() {
                points.push(norad::ContourPoint::new(
                    control.x.round(),
                    control.y.round(),
                    PointType::OffCurve,
                    false,
                    None,
                    None,
                ));
                let last = k + 1 == quads.len();
                points.push(norad::ContourPoint::new(
                    if last { p.x } else { end.x.round() },
                    if last { p.y } else { end.y.round() },
                    PointType::QCurve,
                    if last { p.smooth } else { true },
                    None,
                    None,
                ));
            }
            changed = true;
        }
        if changed {
            contour.points = points;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quad_cubic_conversions() {
        use norad::{Contour, ContourPoint, PointType};
        // A closed quad shape: line across the bottom, one quadratic
        // arc over the top through control (50, 50).
        let pt = |x, y, typ| ContourPoint::new(x, y, typ, false, None, None);
        let contour = Contour::new(
            vec![
                pt(0.0, 0.0, PointType::Line),
                pt(100.0, 0.0, PointType::Line),
                pt(75.0, 50.0, PointType::OffCurve),
                pt(50.0, 60.0, PointType::QCurve),
                pt(25.0, 50.0, PointType::OffCurve),
                pt(0.0, 0.0, PointType::QCurve),
            ],
            None,
        );
        let mut glyph = norad::Glyph::new("quads");
        glyph.contours = vec![contour];
        assert!(quads_to_cubics(&mut glyph));
        let types: Vec<PointType> = glyph.contours[0].points.iter().map(|p| p.typ).collect();
        assert!(!types.contains(&PointType::QCurve), "{types:?}");
        // Two quads became two cubics: 2 on + 2 line + 4 off.
        assert_eq!(
            types.iter().filter(|t| **t == PointType::OffCurve).count(),
            4
        );
        // Exactness at the quad midpoint: the cubic passes through
        // the same point the quad did. Quad (100,0)-(75,50)-(50,60)
        // at t=.5: (75, 40).
        let bez = crate::outline::glyph_paths::contour_to_bezpath(&glyph.contours[0]);
        use kurbo::ParamCurve as _;
        let close_to = |target: kurbo::Point| {
            bez.segments()
                .any(|seg| (0..=10).any(|i| seg.eval(i as f64 / 10.0).distance(target) < 1.5))
        };
        assert!(close_to(kurbo::Point::new(75.0, 40.0)));

        // And back: cubics to quads stays within tolerance.
        let mut back = glyph.clone();
        assert!(cubics_to_quads(&mut back, 1.0));
        let types: Vec<PointType> = back.contours[0].points.iter().map(|p| p.typ).collect();
        assert!(!types.contains(&PointType::Curve), "{types:?}");
        let bez2 = crate::outline::glyph_paths::contour_to_bezpath(&back.contours[0]);
        // Sample the round-tripped outline against the cubic one.
        for seg in bez.segments() {
            for i in 0..=4 {
                let p = seg.eval(i as f64 / 4.0);
                let nearest = bez2
                    .segments()
                    .flat_map(|s2| (0..=16).map(move |j| s2.eval(j as f64 / 16.0)))
                    .map(|q| p.distance(q))
                    .fold(f64::MAX, f64::min);
                assert!(nearest < 2.5, "outline drifted {nearest}");
            }
        }
    }
}
