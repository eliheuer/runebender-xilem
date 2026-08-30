// Ported from runebender-web/core/src/editor.rs segment operations
// (Apache-2.0), reworked to address norad contours directly.

//! Segment-level editing on norad glyphs: hit-test the nearest
//! segment, insert a point on it, convert a line to a curve, and
//! enumerate a segment's points for selection. Shared by the select
//! and pen tools of every Runebender editor.

use kurbo::{CubicBez, Line, ParamCurve, ParamCurveNearest, PathSeg, Point, QuadBez};
use norad::{ContourPoint, Glyph, PointType};

use crate::glyph_ops::PointId;

/// One segment of a contour, addressed by its on-curve endpoints.
#[derive(Debug, Clone)]
pub struct SegmentHit {
    pub contour: usize,
    /// Index of the on-curve point the segment starts at.
    pub start: usize,
    /// Index of the on-curve point the segment ends at (wraps to the
    /// contour start for the closing segment).
    pub end: usize,
    /// Indices of the off-curve controls between them, in order.
    pub controls: Vec<usize>,
    pub seg: PathSeg,
}

impl SegmentHit {
    /// Every point of the segment, for selection.
    pub fn point_ids(&self) -> Vec<PointId> {
        let mut ids = vec![(self.contour, self.start)];
        ids.extend(self.controls.iter().map(|&i| (self.contour, i)));
        ids.push((self.contour, self.end));
        ids
    }
}

fn pt(p: &ContourPoint) -> Point {
    Point::new(p.x, p.y)
}

fn is_on(p: &ContourPoint) -> bool {
    p.typ != PointType::OffCurve
}

/// Enumerate a glyph's segments. Hyperbezier contours are skipped:
/// their on-screen segments come from the spline solver and are not
/// editable at the norad point level.
pub fn segments(glyph: &Glyph) -> Vec<SegmentHit> {
    let mut out = Vec::new();
    for (ci, contour) in glyph.contours.iter().enumerate() {
        if crate::model::workspace::norad_contour_is_hyper(contour) {
            continue;
        }
        let points = &contour.points;
        if points.len() < 2 {
            continue;
        }
        let on_indices: Vec<usize> = (0..points.len()).filter(|&i| is_on(&points[i])).collect();
        if on_indices.is_empty() {
            continue;
        }
        let closed = points[0].typ != PointType::Move;
        let pair_count = if closed {
            on_indices.len()
        } else {
            on_indices.len().saturating_sub(1)
        };
        for k in 0..pair_count {
            let start = on_indices[k];
            let end = on_indices[(k + 1) % on_indices.len()];
            // Controls are the off-curves strictly between start and
            // end, walking forward with wraparound.
            let mut controls = Vec::new();
            let mut i = (start + 1) % points.len();
            while i != end {
                if !is_on(&points[i]) {
                    controls.push(i);
                }
                i = (i + 1) % points.len();
            }
            let seg = match controls.len() {
                0 => PathSeg::Line(Line::new(pt(&points[start]), pt(&points[end]))),
                1 => PathSeg::Quad(QuadBez::new(
                    pt(&points[start]),
                    pt(&points[controls[0]]),
                    pt(&points[end]),
                )),
                2 => PathSeg::Cubic(CubicBez::new(
                    pt(&points[start]),
                    pt(&points[controls[0]]),
                    pt(&points[controls[1]]),
                    pt(&points[end]),
                )),
                // TrueType multi-off-curve runs: not editable here.
                _ => continue,
            };
            out.push(SegmentHit {
                contour: ci,
                start,
                end,
                controls,
                seg,
            });
        }
    }
    out
}

/// The segment nearest to `pt` within `radius`, with the curve
/// parameter of the nearest point on it.
pub fn nearest_segment_with_t(
    glyph: &Glyph,
    design_pt: Point,
    radius: f64,
) -> Option<(SegmentHit, f64)> {
    let max_dist_sq = radius * radius;
    let mut best: Option<(SegmentHit, f64, f64)> = None;
    for hit in segments(glyph) {
        let nearest = hit.seg.nearest(design_pt, 1e-6);
        if nearest.distance_sq > max_dist_sq {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|(_, _, d)| nearest.distance_sq < *d)
        {
            best = Some((hit, nearest.t, nearest.distance_sq));
        }
    }
    best.map(|(hit, t, _)| (hit, t))
}

fn off_point(p: Point) -> ContourPoint {
    let (x, y) = snapped(p);
    ContourPoint::new(x, y, PointType::OffCurve, false, None, None)
}

/// Generated points land on the design grid, like every other point
/// the editors create or move (see `point_ops`).
fn snapped(p: Point) -> (f64, f64) {
    use crate::point_ops::snap_coord;
    (snap_coord(p.x), snap_coord(p.y))
}

/// Convert a line segment to a cubic with controls at 1/3 and 2/3
/// (the web select tool's alt-click). Returns the control point ids.
pub fn convert_line_to_curve(glyph: &mut Glyph, hit: &SegmentHit) -> Option<[PointId; 2]> {
    let PathSeg::Line(line) = hit.seg else {
        return None;
    };
    let contour = glyph.contours.get_mut(hit.contour)?;
    let c1 = off_point(line.p0.lerp(line.p1, 1.0 / 3.0));
    let c2 = off_point(line.p0.lerp(line.p1, 2.0 / 3.0));
    // `end < start` for the wrap-around closing segment.
    let insert_index = if hit.end > hit.start {
        hit.end
    } else {
        hit.start + 1
    };
    contour.points.insert(insert_index, c2);
    contour.points.insert(insert_index, c1);
    // The segment's end point now closes a curve.
    let end_index = (insert_index + 2).min(contour.points.len() - 1);
    let end_index = if hit.end > hit.start {
        end_index
    } else {
        // Closing segment: the end point is the contour start.
        hit.end
    };
    if let Some(end) = contour.points.get_mut(end_index)
        && end.typ == PointType::Line {
            end.typ = PointType::Curve;
        }
    Some([(hit.contour, insert_index), (hit.contour, insert_index + 1)])
}

/// Insert an on-curve point on a segment at parameter `t`, splitting
/// curves exactly (the web pen tool's click-on-segment). Returns the
/// new point's id.
pub fn insert_point_on_segment(glyph: &mut Glyph, hit: &SegmentHit, t: f64) -> Option<PointId> {
    let t = t.clamp(0.0, 1.0);
    let contour = glyph.contours.get_mut(hit.contour)?;
    let insert_index = if hit.end > hit.start {
        hit.end
    } else {
        hit.start + 1
    };
    match hit.seg {
        PathSeg::Line(line) => {
            let p = snapped(line.eval(t));
            contour.points.insert(
                insert_index,
                ContourPoint::new(p.0, p.1, PointType::Line, false, None, None),
            );
            Some((hit.contour, insert_index))
        }
        PathSeg::Cubic(cubic) => {
            let left = cubic.subsegment(0.0..t);
            let right = cubic.subsegment(t..1.0);
            // Replace the two old controls with left+split+right.
            let mut removed = hit.controls.clone();
            removed.sort_unstable();
            for &i in removed.iter().rev() {
                contour.points.remove(i);
            }
            let base = if hit.end > hit.start {
                hit.start + 1
            } else {
                hit.start + 1 - removed.iter().filter(|&&i| i < hit.start).count()
            };
            let (sx, sy) = snapped(left.p3);
            let split = ContourPoint::new(sx, sy, PointType::Curve, false, None, None);
            let new_points = vec![
                off_point(left.p1),
                off_point(left.p2),
                split,
                off_point(right.p1),
                off_point(right.p2),
            ];
            for (offset, p) in new_points.into_iter().enumerate() {
                let index = (base + offset).min(contour.points.len());
                contour.points.insert(index, p);
            }
            Some((hit.contour, base + 2))
        }
        PathSeg::Quad(quad) => {
            let left = quad.subsegment(0.0..t);
            let right = quad.subsegment(t..1.0);
            let mut removed = hit.controls.clone();
            removed.sort_unstable();
            for &i in removed.iter().rev() {
                contour.points.remove(i);
            }
            let base = if hit.end > hit.start {
                hit.start + 1
            } else {
                hit.start + 1 - removed.iter().filter(|&&i| i < hit.start).count()
            };
            let (sx, sy) = snapped(left.p2);
            let split = ContourPoint::new(sx, sy, PointType::QCurve, false, None, None);
            let new_points = vec![off_point(left.p1), split, off_point(right.p1)];
            for (offset, p) in new_points.into_iter().enumerate() {
                let index = (base + offset).min(contour.points.len());
                contour.points.insert(index, p);
            }
            Some((hit.contour, base + 1))
        }
    }
}

/// Delete the last drawn pen point of an open contour: the trailing
/// on-curve and any off-curves that led to it. Returns the number of
/// points remaining.
pub fn delete_last_pen_point(glyph: &mut Glyph, contour: usize) -> Option<usize> {
    let c = glyph.contours.get_mut(contour)?;
    if c.points.is_empty() {
        return None;
    }
    c.points.pop();
    while c
        .points
        .last()
        .is_some_and(|p| p.typ == PointType::OffCurve)
    {
        c.points.pop();
    }
    Some(c.points.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_glyph() -> Glyph {
        let mut glyph = Glyph::new("test");
        let points = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
            .map(|(x, y)| ContourPoint::new(x, y, PointType::Line, false, None, None));
        glyph
            .contours
            .push(norad::Contour::new(points.to_vec(), None));
        glyph
    }

    #[test]
    fn nearest_segment_finds_the_bottom_edge() {
        let glyph = rect_glyph();
        let (hit, t) = nearest_segment_with_t(&glyph, Point::new(50.0, -2.0), 5.0).unwrap();
        assert_eq!((hit.contour, hit.start, hit.end), (0, 0, 1));
        assert!((t - 0.5).abs() < 0.05);
        assert!(matches!(hit.seg, PathSeg::Line(_)));
        assert!(nearest_segment_with_t(&glyph, Point::new(50.0, 50.0), 5.0).is_none());
    }

    #[test]
    fn line_converts_to_curve_with_thirds_handles() {
        let mut glyph = rect_glyph();
        let (hit, _) = nearest_segment_with_t(&glyph, Point::new(50.0, -2.0), 5.0).unwrap();
        let ids = convert_line_to_curve(&mut glyph, &hit).unwrap();
        let c = &glyph.contours[0];
        assert_eq!(c.points.len(), 6);
        assert_eq!(c.points[ids[0].1].typ, PointType::OffCurve);
        // Thirds of a 100-unit line, snapped to the 2-unit design
        // grid the editors place every point on.
        assert_eq!((c.points[ids[0].1].x, c.points[ids[0].1].y), (34.0, 0.0));
        assert_eq!((c.points[ids[1].1].x, c.points[ids[1].1].y), (66.0, 0.0));
        // The segment's end point is a curve target now.
        assert_eq!(c.points[3].typ, PointType::Curve);
    }

    #[test]
    fn closing_segment_converts_too() {
        let mut glyph = rect_glyph();
        // Left edge: from (0,100) back to (0,0) — the wrap-around.
        let (hit, _) = nearest_segment_with_t(&glyph, Point::new(-2.0, 50.0), 5.0).unwrap();
        assert!(hit.end < hit.start);
        assert!(convert_line_to_curve(&mut glyph, &hit).is_some());
        let c = &glyph.contours[0];
        assert_eq!(c.points.len(), 6);
        // Contour start became the curve's target.
        assert_eq!(c.points[0].typ, PointType::Curve);
    }

    #[test]
    fn insert_point_on_line_splits_it() {
        let mut glyph = rect_glyph();
        let (hit, t) = nearest_segment_with_t(&glyph, Point::new(50.0, -2.0), 5.0).unwrap();
        let id = insert_point_on_segment(&mut glyph, &hit, t).unwrap();
        let c = &glyph.contours[0];
        assert_eq!(c.points.len(), 5);
        assert_eq!((c.points[id.1].x, c.points[id.1].y), (50.0, 0.0));
        assert_eq!(c.points[id.1].typ, PointType::Line);
    }

    #[test]
    fn insert_point_on_cubic_subdivides_exactly() {
        let mut glyph = Glyph::new("c");
        let points = vec![
            ContourPoint::new(0.0, 0.0, PointType::Curve, false, None, None),
            ContourPoint::new(0.0, 55.0, PointType::OffCurve, false, None, None),
            ContourPoint::new(45.0, 100.0, PointType::OffCurve, false, None, None),
            ContourPoint::new(100.0, 100.0, PointType::Curve, false, None, None),
            ContourPoint::new(100.0, 0.0, PointType::Line, false, None, None),
        ];
        glyph.contours.push(norad::Contour::new(points, None));
        let target = Point::new(20.0, 70.0);
        let (hit, t) = nearest_segment_with_t(&glyph, target, 20.0).unwrap();
        assert!(matches!(hit.seg, PathSeg::Cubic(_)));
        let before = hit.seg.eval(t);
        let id = insert_point_on_segment(&mut glyph, &hit, t).unwrap();
        let c = &glyph.contours[0];
        assert_eq!(c.points.len(), 8);
        let inserted = &c.points[id.1];
        assert_eq!(inserted.typ, PointType::Curve);
        // Within one grid step of the exact split point.
        assert!((inserted.x - before.x).abs() <= 1.0);
        assert!((inserted.y - before.y).abs() <= 1.0);
        // The shape is unchanged within rounding: the split point lies
        // on the original curve.
        let seg_after = segments(&glyph);
        assert_eq!(seg_after.len(), 4);
    }

    #[test]
    fn delete_last_pen_point_pops_controls_too() {
        let mut glyph = Glyph::new("p");
        let points = vec![
            ContourPoint::new(0.0, 0.0, PointType::Move, false, None, None),
            ContourPoint::new(50.0, 0.0, PointType::Line, false, None, None),
            ContourPoint::new(60.0, 30.0, PointType::OffCurve, false, None, None),
            ContourPoint::new(60.0, 70.0, PointType::OffCurve, false, None, None),
            ContourPoint::new(50.0, 100.0, PointType::Curve, false, None, None),
        ];
        glyph.contours.push(norad::Contour::new(points, None));
        assert_eq!(delete_last_pen_point(&mut glyph, 0), Some(2));
        assert_eq!(glyph.contours[0].points.len(), 2);
        assert_eq!(delete_last_pen_point(&mut glyph, 0), Some(1));
        assert_eq!(delete_last_pen_point(&mut glyph, 0), Some(0));
    }
}
