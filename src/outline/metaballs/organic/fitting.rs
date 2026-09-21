// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Fit smooth organic silhouettes through img2bez, preserving genuine Boolean corners.
//!
//! Cornered contours already consist of exact source cubics. Those pass through
//! img2bez's outline model and start normalization without smoothing the corner.
//! Smooth fitting is accepted only when economical and close to the true curves
//! in both directions; otherwise the extrema-split source cubics are retained.

use kurbo::{
    BezPath, CubicBez, ParamCurve, ParamCurveDeriv, ParamCurveExtrema, ParamCurveNearest, Point,
    Vec2,
};

fn extrema_split(path: &BezPath) -> Vec<CubicBez> {
    let mut output = Vec::new();
    for segment in path.segments() {
        let curve = segment.to_cubic();
        if curve
            .p0
            .distance(curve.p1)
            .max(curve.p0.distance(curve.p2))
            .max(curve.p0.distance(curve.p3))
            < 1e-9
        {
            continue;
        }
        let mut stops = vec![0.0];
        stops.extend(
            curve
                .extrema()
                .into_iter()
                .filter(|t| *t > 1e-8 && *t < 1.0 - 1e-8),
        );
        stops.push(1.0);
        stops.sort_by(f64::total_cmp);
        stops.dedup_by(|a, b| (*a - *b).abs() < 1e-8);
        for pair in stops.windows(2) {
            output.push(curve.subsegment(pair[0]..pair[1]));
        }
    }
    output
}

fn path_of(curves: &[CubicBez]) -> BezPath {
    let mut path = BezPath::new();
    if let Some(first) = curves.first() {
        path.move_to(first.p0);
        for curve in curves {
            path.curve_to(curve.p1, curve.p2, curve.p3);
        }
        path.close_path();
    }
    path
}

fn tangent(curve: &CubicBez, t: f64) -> Vec2 {
    curve.deriv().eval(t).to_vec2()
}

fn smooth(curves: &[CubicBez]) -> bool {
    !curves.is_empty()
        && curves.iter().enumerate().all(|(i, c)| {
            let a = tangent(c, 1.0);
            let b = tangent(&curves[(i + 1) % curves.len()], 0.0);
            a.hypot() > 1e-10 && b.hypot() > 1e-10 && a.normalize().dot(b.normalize()) > 1.0 - 1e-9
        })
}

fn samples(curves: &[CubicBez]) -> Vec<img2bez::BoundarySample> {
    let mut samples: Vec<img2bez::BoundarySample> = Vec::new();
    for curve in curves {
        // Exact cubic extrema begin each piece. Ordinary interior samples do
        // not become extrema merely because a whole span is a straight line.
        for i in 0..32 {
            let t = f64::from(i) / 32.0;
            let p = curve.eval(t);
            let direction = tangent(curve, t);
            let feature = if i == 0 && direction.x.abs() < 1e-9 * direction.hypot() {
                Some(img2bez::BoundaryFeature::ExtremumX)
            } else if i == 0 && direction.y.abs() < 1e-9 * direction.hypot() {
                Some(img2bez::BoundaryFeature::ExtremumY)
            } else {
                None
            };
            let sample = img2bez::BoundarySample {
                position: [p.x, p.y],
                tangent: [direction.x, direction.y],
                feature,
            };
            if let Some(last) = samples.last_mut()
                && Point::new(last.position[0], last.position[1]).distance(p) <= 1e-8
            {
                if feature.is_some() {
                    *last = sample;
                }
                continue;
            }
            samples.push(sample);
        }
    }
    if samples.len() > 1 {
        let first = samples[0];
        let last = *samples.last().expect("nonempty samples");
        if Point::new(first.position[0], first.position[1])
            .distance(Point::new(last.position[0], last.position[1]))
            <= 1e-8
        {
            samples.pop();
        }
    }
    samples
}

fn close_in_both_directions(a: &[CubicBez], b: &[CubicBez], tolerance: f64) -> bool {
    let close = |source: &[CubicBez], target: &[CubicBez]| {
        source.iter().all(|curve| {
            (0..=32).all(|i| {
                let point = curve.eval(f64::from(i) / 32.0);
                target
                    .iter()
                    .any(|other| other.nearest(point, 1e-5).distance_sq <= tolerance.powi(2))
            })
        })
    };
    close(a, b) && close(b, a)
}

pub(super) fn convert(paths: &[BezPath], accuracy: f64) -> Result<Vec<BezPath>, String> {
    let mut converted = Vec::new();
    for path in paths {
        let exact = extrema_split(path);
        if exact.iter().any(|c| !c.is_finite()) {
            return Err("nonfinite organic metaball boundary".into());
        }
        let mut result = path_of(&exact);
        // Bound optimized fitting work. A complex or cornered contour retains
        // its exact source cubics rather than receiving an inaccurate refit.
        if exact.len() <= 256
            && smooth(&exact)
            && let Ok(outline) = img2bez::fit_smooth_contours(&[samples(&exact)], accuracy)
            && let Some(fitted) = outline.to_bezpaths().into_iter().next()
        {
            let curves: Vec<_> = fitted.segments().map(|s| s.to_cubic()).collect();
            if curves.len() <= exact.len() && close_in_both_directions(&exact, &curves, accuracy) {
                result = fitted;
            }
        }
        if !result.is_empty() {
            converted.push(result);
        }
    }
    let mut output = img2bez::Outline::from_bezpaths(&converted);
    output.normalize_starts(false);
    Ok(output.to_bezpaths())
}
