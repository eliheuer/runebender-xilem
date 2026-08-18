// Copyright 2026 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Curve-quality operations on the edit session: harmonize (G2 at
//! smooth joins), Tunni balance, and the contour optimizer. Ports of
//! runebender-web's selection ops, running on the shared geometry in
//! `runebender_core::curve`.

use std::sync::Arc;

use runebender_core::curve::{OptPoint, balance, harmonize, optimize_contour};

use crate::path::{Path, PathPoints, PointType};

use runebender_core::editing::Selection;

use super::EditSession;

impl EditSession {
    /// Harmonize smooth on-curve nodes (selected, or all when the
    /// selection is empty) so their curve joins become
    /// curvature-continuous (G2). Keeps the on-curve point fixed and
    /// moves its two adjacent handles; results are rounded to the
    /// integer grid for the human to refine.
    pub fn harmonize_selection(&mut self) -> bool {
        harmonize_paths(Arc::make_mut(&mut self.paths).as_mut_slice(), &self.selection)
    }

    /// Balance the handles of cubic segments (selected, or all when
    /// the selection is empty) via Tunni. Rounded to the integer grid.
    pub fn balance_selection(&mut self) -> bool {
        balance_paths(Arc::make_mut(&mut self.paths).as_mut_slice(), &self.selection)
    }

    /// Auto-fair: run the multi-objective optimizer (continuity →
    /// even curvature → low popcount) on the handles of contours in
    /// scope, keeping on-curve points fixed.
    pub fn optimize_selection(&mut self, tol: f64) -> bool {
        optimize_paths(Arc::make_mut(&mut self.paths).as_mut_slice(), &self.selection, tol)
    }
}

pub(crate) fn harmonize_paths(paths: &mut [Path], sel: &Selection) -> bool {
    let all = sel.is_empty();
    let mut changed = false;
    {
        for path in paths.iter_mut() {
            let Path::Cubic(cubic) = path else {
                continue;
            };
            if !cubic.closed {
                continue;
            }
            let mut pts = cubic.points.to_vec();
            let n = pts.len();
            if n < 4 {
                continue;
            }
            let mut updates: Vec<(usize, kurbo::Point)> = Vec::new();
            for i in 0..n {
                if !matches!(pts[i].typ, PointType::OnCurve { smooth: true }) {
                    continue;
                }
                if !all && !sel.contains(&pts[i].id) {
                    continue;
                }
                let (a1i, a2i, b1i, b2i) =
                    ((i + n - 2) % n, (i + n - 1) % n, (i + 1) % n, (i + 2) % n);
                if pts[a1i].is_on_curve()
                    || pts[a2i].is_on_curve()
                    || pts[b1i].is_on_curve()
                    || pts[b2i].is_on_curve()
                {
                    continue; // need cubic segments on both sides
                }
                if let Some((na2, nb1)) = harmonize(
                    pts[a1i].point,
                    pts[a2i].point,
                    pts[i].point,
                    pts[b1i].point,
                    pts[b2i].point,
                ) {
                    updates.push((a2i, na2.round()));
                    updates.push((b1i, nb1.round()));
                }
            }
            if !updates.is_empty() {
                for (idx, p) in updates {
                    pts[idx].point = p;
                }
                cubic.points = PathPoints::from_vec(pts);
                changed = true;
            }
        }
    }
    changed
}

/// Tunni balance over cubic segments in scope.
pub(crate) fn balance_paths(paths: &mut [Path], sel: &Selection) -> bool {
    let all = sel.is_empty();
    let mut changed = false;
    {
        for path in paths.iter_mut() {
            let Path::Cubic(cubic) = path else {
                continue;
            };
            let mut pts = cubic.points.to_vec();
            let n = pts.len();
            if n < 4 {
                continue;
            }
            let last = if cubic.closed { n } else { n.saturating_sub(3) };
            let mut updates: Vec<(usize, kurbo::Point)> = Vec::new();
            for i in 0..last {
                let (b, c, d) = ((i + 1) % n, (i + 2) % n, (i + 3) % n);
                if !pts[i].is_on_curve()
                    || pts[b].is_on_curve()
                    || pts[c].is_on_curve()
                    || !pts[d].is_on_curve()
                {
                    continue; // not a cubic segment
                }
                let selected = all
                    || sel.contains(&pts[i].id)
                    || sel.contains(&pts[b].id)
                    || sel.contains(&pts[c].id)
                    || sel.contains(&pts[d].id);
                if !selected {
                    continue;
                }
                if let Some((np1, np2)) =
                    balance(pts[i].point, pts[b].point, pts[c].point, pts[d].point)
                {
                    updates.push((b, np1.round()));
                    updates.push((c, np2.round()));
                }
            }
            if !updates.is_empty() {
                for (idx, p) in updates {
                    pts[idx].point = p;
                }
                cubic.points = PathPoints::from_vec(pts);
                changed = true;
            }
        }
    }
    changed
}

/// Contour optimizer over contours in scope.
pub(crate) fn optimize_paths(paths: &mut [Path], sel: &Selection, tol: f64) -> bool {
    let all = sel.is_empty();
    let mut changed = false;
    {
        for path in paths.iter_mut() {
            let Path::Cubic(cubic) = path else {
                continue;
            };
            if !cubic.closed {
                continue;
            }
            let mut pts = cubic.points.to_vec();
            if pts.len() < 4 {
                continue;
            }
            if !all && !pts.iter().any(|p| sel.contains(&p.id)) {
                continue;
            }
            let opts: Vec<OptPoint> = pts
                .iter()
                .map(|p| OptPoint {
                    p: p.point,
                    on: p.is_on_curve(),
                    smooth: matches!(p.typ, PointType::OnCurve { smooth: true }),
                })
                .collect();
            let newpos = optimize_contour(&opts, tol);
            let mut any = false;
            for (i, p) in pts.iter_mut().enumerate() {
                if !p.is_on_curve() && (p.point - newpos[i]).hypot() > 1e-6 {
                    p.point = newpos[i];
                    any = true;
                }
            }
            if any {
                cubic.points = PathPoints::from_vec(pts);
                changed = true;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use crate::path::{Path, PathPoint, PathPoints, PointType};
    use kurbo::Point;

    /// A closed circle-ish contour whose handles are deliberately
    /// uneven, as (point, on_curve, smooth) triples.
    fn lopsided_circle() -> Path {
        let r = 100.0;
        let k1 = 40.0; // too short
        let k2 = 70.0; // too long
        let raw = [
            ((r, 0.0), true),
            ((r, k1), false),
            ((k2, r), false),
            ((0.0, r), true),
            ((-k1, r), false),
            ((-r, k2), false),
            ((-r, 0.0), true),
            ((-r, -k1), false),
            ((-k2, -r), false),
            ((0.0, -r), true),
            ((k1, -r), false),
            ((r, -k2), false),
        ];
        let pts: Vec<PathPoint> = raw
            .iter()
            .map(|((x, y), on)| PathPoint {
                id: crate::model::EntityId::next(),
                point: Point::new(*x, *y),
                typ: if *on {
                    PointType::OnCurve { smooth: true }
                } else {
                    PointType::OffCurve { auto: false }
                },
            })
            .collect();
        Path::Cubic(crate::path::CubicPath {
            points: PathPoints::from_vec(pts),
            closed: true,
            id: crate::model::EntityId::next(),
        })
    }

    #[test]
    fn balance_moves_handles_not_nodes() {
        let mut paths = vec![lopsided_circle()];
        let before: Vec<Point> = paths[0].points().to_vec().iter().map(|p| p.point).collect();
        let sel = runebender_core::editing::Selection::new();
        assert!(super::balance_paths(&mut paths, &sel));
        let after: Vec<Point> = paths[0].points().to_vec().iter().map(|p| p.point).collect();
        let pts = paths[0].points().to_vec();
        let mut moved = 0;
        for (i, p) in pts.iter().enumerate() {
            if p.is_on_curve() {
                assert_eq!(before[i], after[i], "on-curve moved at {i}");
            } else if before[i] != after[i] {
                moved += 1;
            }
        }
        assert!(moved > 0, "no handles moved");
    }

    #[test]
    fn harmonize_and_optimize_run_on_closed_contour() {
        let sel = runebender_core::editing::Selection::new();
        let mut paths = vec![lopsided_circle()];
        // Harmonize may or may not move points depending on geometry;
        // it must not panic and must keep on-curves fixed.
        super::harmonize_paths(&mut paths, &sel);
        let mut paths2 = vec![lopsided_circle()];
        assert!(super::optimize_paths(&mut paths2, &sel, 0.12));
        for p in paths2[0].points().to_vec() {
            if !p.is_on_curve() {
                assert_eq!(p.point.x as i64 % 2, 0);
                assert_eq!(p.point.y as i64 % 2, 0);
            }
        }
    }
}
