// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Direct corner and handle cleanup for canonical document layers.

use std::collections::HashSet;

use babelfont::{Node, NodeType, Shape};
use kurbo::Point;

use super::{
    DocumentEditError, LayerEditDraft, PointId, PreservedContour, PreservedPoint,
    new_document_point, read_id,
};

const ROUND_GRID: f64 = 2.0;
const DEFAULT_ROUND_OFFSET: f64 = 32.0;
const DEFAULT_ROUND_HANDLE_RATIO: f64 = 0.552_284_749_830_793_6;
const MAX_ROUND_SIDE_FRACTION: f64 = 0.45;

#[derive(Clone)]
struct StoredPoint {
    node: Node,
    preserved: PreservedPoint,
}

impl StoredPoint {
    fn id(&self) -> PointId {
        self.preserved.id
    }

    fn position(&self) -> Point {
        Point::new(self.node.x, self.node.y)
    }

    fn is_on_curve(&self) -> bool {
        self.node.nodetype != NodeType::OffCurve
    }
}

impl LayerEditDraft {
    /// Replace selected line-line corners with cubic fillets.
    ///
    /// An empty selection is a no-op. Every supplied point identity is validated before any
    /// contour changes. The original corner identity and metadata move to the incoming fillet
    /// endpoint, while the second endpoint and both handles receive fresh identities. On success,
    /// the returned identities are the two on-curve fillet points for every rounded corner.
    pub fn round_selected_corners(
        &mut self,
        selected: &[PointId],
    ) -> Result<Option<Vec<PointId>>, DocumentEditError> {
        if selected.is_empty() {
            return Ok(None);
        }
        validate_selected_points(self, selected)?;
        let selected: HashSet<_> = selected.iter().copied().collect();
        let (offset_profile, handle_ratio) = infer_round_profile(self);
        let mut staged = self.clone();
        let mut next_selection = Vec::new();
        let mut changed = false;

        for shape_index in 0..staged.layer.shapes.len() {
            let Shape::Path(path) = &staged.layer.shapes[shape_index] else {
                continue;
            };
            let contour_id = read_id(&path.format_specific).expect("canonical contour identity");
            let preserved_index = staged
                .preserved
                .contours
                .iter()
                .position(|contour| contour.id.0 == contour_id)
                .expect("canonical contour preservation");
            if staged.preserved.contours[preserved_index].hyper {
                continue;
            }
            let (path, preserved) = match &mut staged.layer.shapes[shape_index] {
                Shape::Path(path) => (path, &mut staged.preserved.contours[preserved_index]),
                Shape::Component(_) => unreachable!("shape kind was checked above"),
            };
            let Some(replacement) =
                round_contour(path, preserved, &selected, offset_profile, handle_ratio)?
            else {
                continue;
            };
            path.nodes = replacement
                .points
                .iter()
                .map(|point| point.node.clone())
                .collect();
            preserved.points = replacement
                .points
                .into_iter()
                .map(|point| point.preserved)
                .collect();
            next_selection.extend(replacement.selection);
            changed = true;
        }
        if !changed {
            return Ok(None);
        }
        *self = staged;
        Ok(Some(next_selection))
    }
}

struct RoundedContour {
    points: Vec<StoredPoint>,
    selection: Vec<PointId>,
}

fn round_contour(
    path: &babelfont::Path,
    preserved: &PreservedContour,
    selected: &HashSet<PointId>,
    offset_profile: f64,
    handle_ratio: f64,
) -> Result<Option<RoundedContour>, DocumentEditError> {
    let points = stored_points(path, preserved);
    let length = points.len();
    if length < 3 {
        return Ok(None);
    }
    let mut replacement = Vec::with_capacity(length + selected.len() * 3);
    let mut selection = Vec::new();
    let mut changed = false;
    for (index, point) in points.iter().enumerate() {
        let neighbors = if path.closed {
            Some(((index + length - 1) % length, (index + 1) % length))
        } else if index > 0 && index + 1 < length {
            Some((index - 1, index + 1))
        } else {
            None
        };
        let rounded = selected
            .contains(&point.id())
            .then_some(neighbors)
            .flatten()
            .filter(|_| point.is_on_curve())
            .and_then(|(previous, next)| {
                points[previous]
                    .is_on_curve()
                    .then_some(())
                    .and(points[next].is_on_curve().then_some(()))?;
                rounded_corner(
                    points[previous].position(),
                    point.position(),
                    points[next].position(),
                    offset_profile,
                    handle_ratio,
                )
            });
        let Some([first_on, first_handle, second_handle, second_on]) = rounded else {
            replacement.push(point.clone());
            continue;
        };
        ensure_points_finite(&[first_on, first_handle, second_handle, second_on])?;
        let mut lead = point.clone();
        lead.node.x = first_on.x;
        lead.node.y = first_on.y;
        lead.node.smooth = true;
        selection.push(lead.id());
        replacement.push(lead);
        replacement.push(fresh_point(first_handle, NodeType::OffCurve, false));
        replacement.push(fresh_point(second_handle, NodeType::OffCurve, false));
        let tail = fresh_point(second_on, NodeType::Curve, true);
        selection.push(tail.id());
        replacement.push(tail);
        changed = true;
    }
    Ok(changed.then_some(RoundedContour {
        points: replacement,
        selection,
    }))
}

fn rounded_corner(
    previous: Point,
    corner: Point,
    next: Point,
    offset_profile: f64,
    handle_ratio: f64,
) -> Option<[Point; 4]> {
    let previous_vector = previous - corner;
    let next_vector = next - corner;
    let previous_length = previous_vector.hypot();
    let next_length = next_vector.hypot();
    if previous_length < ROUND_GRID * 2.0 || next_length < ROUND_GRID * 2.0 {
        return None;
    }
    let offset = offset_profile
        .min(previous_length * MAX_ROUND_SIDE_FRACTION)
        .min(next_length * MAX_ROUND_SIDE_FRACTION);
    if offset < ROUND_GRID {
        return None;
    }
    let previous_unit = previous_vector / previous_length;
    let next_unit = next_vector / next_length;
    let handle_length = offset * handle_ratio;
    let first_on = round_snap(corner + previous_unit * offset);
    let second_on = round_snap(corner + next_unit * offset);
    let first_handle = round_snap(first_on - previous_unit * handle_length);
    let second_handle = round_snap(second_on - next_unit * handle_length);
    (first_on != corner && second_on != corner && first_on != second_on).then_some([
        first_on,
        first_handle,
        second_handle,
        second_on,
    ])
}

fn infer_round_profile(draft: &LayerEditDraft) -> (f64, f64) {
    let mut offsets = Vec::new();
    let mut ratios = Vec::new();
    for contour in draft
        .view()
        .contours()
        .filter(|contour| !contour.is_hyper())
    {
        let points: Vec<_> = contour
            .points()
            .map(|point| {
                (
                    point.position(),
                    point.point_type() != super::LayerPointType::OffCurve,
                )
            })
            .collect();
        let length = points.len();
        if length < 6 {
            continue;
        }
        let starts: Box<dyn Iterator<Item = usize>> = if contour.is_closed() {
            Box::new(0..length)
        } else {
            Box::new(1..length.saturating_sub(4))
        };
        for start in starts {
            let [previous, begin, first, second, end, next] = if contour.is_closed() {
                [
                    (start + length - 1) % length,
                    start,
                    (start + 1) % length,
                    (start + 2) % length,
                    (start + 3) % length,
                    (start + 4) % length,
                ]
            } else {
                [start - 1, start, start + 1, start + 2, start + 3, start + 4]
            };
            let mut unique = [previous, begin, first, second, end, next];
            unique.sort_unstable();
            if unique.windows(2).any(|pair| pair[0] == pair[1])
                || !points[previous].1
                || !points[begin].1
                || points[first].1
                || points[second].1
                || !points[end].1
                || !points[next].1
            {
                continue;
            }
            let Some(corner) = line_intersection(
                points[previous].0,
                points[begin].0,
                points[end].0,
                points[next].0,
            ) else {
                continue;
            };
            let start_offset = corner.distance(points[begin].0);
            let end_offset = corner.distance(points[end].0);
            if start_offset < ROUND_GRID || end_offset < ROUND_GRID {
                continue;
            }
            let first_handle = points[first].0.distance(points[begin].0);
            let second_handle = points[end].0.distance(points[second].0);
            offsets.push((start_offset + end_offset) * 0.5);
            if first_handle > 0.0 {
                ratios.push(first_handle / start_offset);
            }
            if second_handle > 0.0 {
                ratios.push(second_handle / end_offset);
            }
        }
    }
    (
        median_or_default(offsets, DEFAULT_ROUND_OFFSET),
        median_or_default(ratios, DEFAULT_ROUND_HANDLE_RATIO).clamp(0.1, 1.0),
    )
}

fn validate_selected_points(
    draft: &LayerEditDraft,
    selected: &[PointId],
) -> Result<(), DocumentEditError> {
    let available: HashSet<_> = draft
        .layer
        .paths()
        .flat_map(|path| &path.nodes)
        .map(|node| PointId(read_id(&node.format_specific).expect("canonical point identity")))
        .collect();
    for point in selected {
        if !available.contains(point) {
            return Err(DocumentEditError::MissingPoint(*point));
        }
    }
    Ok(())
}

fn stored_points(path: &babelfont::Path, preserved: &PreservedContour) -> Vec<StoredPoint> {
    path.nodes
        .iter()
        .map(|node| {
            let id = PointId(read_id(&node.format_specific).expect("canonical point identity"));
            StoredPoint {
                node: node.clone(),
                preserved: preserved
                    .points
                    .iter()
                    .find(|point| point.id == id)
                    .expect("canonical point preservation")
                    .clone(),
            }
        })
        .collect()
}

fn fresh_point(position: Point, point_type: NodeType, smooth: bool) -> StoredPoint {
    let (_, node, preserved) = new_document_point(position, point_type, smooth);
    StoredPoint { node, preserved }
}

fn round_snap(point: Point) -> Point {
    Point::new(
        (point.x / ROUND_GRID).round() * ROUND_GRID,
        (point.y / ROUND_GRID).round() * ROUND_GRID,
    )
}

fn line_intersection(a: Point, b: Point, c: Point, d: Point) -> Option<Point> {
    let first = b - a;
    let second = d - c;
    let cross = first.x * second.y - first.y * second.x;
    if cross.abs() < 1e-6 {
        return None;
    }
    let delta = c - a;
    let parameter = (delta.x * second.y - delta.y * second.x) / cross;
    Some(a + first * parameter)
}

fn median_or_default(mut values: Vec<f64>, default: f64) -> f64 {
    values.retain(|value| value.is_finite() && *value > 0.0);
    if values.is_empty() {
        return default;
    }
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn ensure_points_finite(points: &[Point]) -> Result<(), DocumentEditError> {
    points
        .iter()
        .all(|point| point.x.is_finite() && point.y.is_finite())
        .then_some(())
        .ok_or(DocumentEditError::NonFinite)
}
