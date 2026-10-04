// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Metrics, point moves, and contour topology edits.

use super::*;

impl LayerEditDraft {
    /// Set the exact horizontal advance and refresh Babelfont's derived width.
    ///
    /// Returns whether the value changed.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the exact f64 value remains authoritative in the document extension"
    )]
    pub fn set_width(&mut self, width: f64) -> Result<bool, DocumentEditError> {
        ensure_finite(&[width])?;
        if self.preserved.width == width {
            return Ok(false);
        }
        self.preserved.width = width;
        self.layer.width = width as f32;
        Ok(true)
    }

    /// Set the exact vertical advance.
    ///
    /// Returns whether the value changed.
    pub fn set_height(&mut self, height: f64) -> Result<bool, DocumentEditError> {
        ensure_finite(&[height])?;
        if self.preserved.height == height {
            return Ok(false);
        }
        self.preserved.height = height;
        Ok(true)
    }

    /// Set one point's position by stable identity.
    ///
    /// Returns whether the value changed.
    pub fn set_point_position(
        &mut self,
        id: PointId,
        position: kurbo::Point,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&[position.x, position.y])?;
        let node = self
            .node_mut(id)
            .ok_or(DocumentEditError::MissingPoint(id))?;
        if node.x == position.x && node.y == position.y {
            return Ok(false);
        }
        node.x = position.x;
        node.y = position.y;
        Ok(true)
    }

    /// Move selected points with the editor's snapping and smooth-handle rules.
    ///
    /// `originals` supplies every position returned by [`LayerView::point_drag_origins`], allowing
    /// repeated pointer events to apply their total delta without accumulating intermediate
    /// snapping. An empty origins slice performs a one-step nudge. An empty selection is unchanged.
    /// Returns whether any point moved.
    pub fn translate_points(
        &mut self,
        selected: &[PointId],
        originals: &[(PointId, kurbo::Point)],
        delta: kurbo::Vec2,
        independent: bool,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&[delta.x, delta.y])?;
        if selected.is_empty() {
            return Ok(false);
        }
        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        for (id, position) in originals {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
            ensure_finite(&[position.x, position.y])?;
        }

        let selected: HashSet<_> = selected.iter().copied().collect();
        let originals: HashMap<_, _> = originals.iter().copied().collect();
        let mut replacements = HashMap::new();
        for path in self.layer.paths() {
            let ids: Vec<_> = path
                .nodes
                .iter()
                .map(|node| {
                    PointId(read_id(&node.format_specific).expect("canonical point identity"))
                })
                .collect();
            let selected_indices: HashSet<_> = ids
                .iter()
                .enumerate()
                .filter_map(|(index, id)| selected.contains(id).then_some(index))
                .collect();
            if selected_indices.is_empty() {
                continue;
            }
            let states: Vec<_> = path
                .nodes
                .iter()
                .map(|node| crate::outline::point_ops::PointState {
                    position: kurbo::Point::new(node.x, node.y),
                    off_curve: node.nodetype == NodeType::OffCurve,
                    smooth: node.smooth,
                })
                .collect();
            let path_originals: HashMap<usize, kurbo::Point> = ids
                .iter()
                .enumerate()
                .filter_map(|(index, id)| originals.get(id).copied().map(|point| (index, point)))
                .collect();
            if !originals.is_empty() {
                for index in crate::outline::point_ops::affected_indices(
                    &states,
                    &selected_indices,
                    path.closed,
                    independent,
                ) {
                    if !path_originals.contains_key(&index) {
                        return Err(DocumentEditError::MissingDragOrigin(ids[index]));
                    }
                }
            }
            for (index, position) in crate::outline::point_ops::translated_positions(
                &states,
                &selected_indices,
                &path_originals,
                (delta.x, delta.y),
                path.closed,
                independent,
            ) {
                ensure_finite(&[position.x, position.y])?;
                replacements.insert(ids[index].0, position);
            }
        }
        if replacements.is_empty() {
            return Ok(false);
        }
        for node in self
            .layer
            .shapes
            .iter_mut()
            .filter_map(|shape| match shape {
                Shape::Path(path) => Some(path),
                Shape::Component(_) => None,
            })
            .flat_map(|path| &mut path.nodes)
        {
            let id = read_id(&node.format_specific).expect("canonical point identity");
            if let Some(position) = replacements.get(&id) {
                node.x = position.x;
                node.y = position.y;
            }
        }
        Ok(true)
    }

    /// Transform selected points about the center of their bounding box.
    ///
    /// An empty selection transforms every point.
    /// Returns whether any point moved.
    pub fn transform_points(
        &mut self,
        selected: &[PointId],
        transform: kurbo::Affine,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&transform.as_coeffs())?;
        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let targeted = |node: &Node| {
            selected.is_empty()
                || read_id(&node.format_specific)
                    .is_some_and(|id| selected.iter().any(|selected| selected.0 == id))
        };
        let mut min = kurbo::Point::new(f64::INFINITY, f64::INFINITY);
        let mut max = kurbo::Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for node in self.layer.paths().flat_map(|path| &path.nodes) {
            if targeted(node) {
                min.x = min.x.min(node.x);
                min.y = min.y.min(node.y);
                max.x = max.x.max(node.x);
                max.y = max.y.max(node.y);
            }
        }
        if !min.x.is_finite() {
            return Ok(false);
        }
        let center = (min.x * 0.5 + max.x * 0.5, min.y * 0.5 + max.y * 0.5);
        ensure_finite(&[center.0, center.1])?;
        let transform = kurbo::Affine::translate(center)
            * transform
            * kurbo::Affine::translate((-center.0, -center.1));
        ensure_finite(&transform.as_coeffs())?;
        let mut replacements = HashMap::new();
        for node in self.layer.paths().flat_map(|path| &path.nodes) {
            if !targeted(node) {
                continue;
            }
            let position = transform * kurbo::Point::new(node.x, node.y);
            ensure_finite(&[position.x, position.y])?;
            if node.x != position.x || node.y != position.y {
                replacements.insert(
                    read_id(&node.format_specific).expect("canonical point identity"),
                    position,
                );
            }
        }
        if replacements.is_empty() {
            return Ok(false);
        }
        for node in self
            .layer
            .shapes
            .iter_mut()
            .filter_map(|shape| match shape {
                Shape::Path(path) => Some(path),
                Shape::Component(_) => None,
            })
            .flat_map(|path| &mut path.nodes)
        {
            let id = read_id(&node.format_specific).expect("canonical point identity");
            if let Some(position) = replacements.get(&id) {
                node.x = position.x;
                node.y = position.y;
            }
        }
        Ok(true)
    }

    /// Set one point's segment role by stable identity.
    ///
    /// Returns whether the value changed.
    pub fn set_point_type(
        &mut self,
        id: PointId,
        point_type: LayerPointType,
    ) -> Result<bool, DocumentEditError> {
        let (path, index) = self
            .path_and_node_index_mut(id)
            .ok_or(DocumentEditError::MissingPoint(id))?;
        if point_type == LayerPointType::Move && index != 0 {
            return Err(DocumentEditError::NonInitialMove(id));
        }
        let node_type = match point_type {
            LayerPointType::Move => NodeType::Move,
            LayerPointType::Line => NodeType::Line,
            LayerPointType::OffCurve => NodeType::OffCurve,
            LayerPointType::Curve => NodeType::Curve,
            LayerPointType::QCurve => NodeType::QCurve,
        };
        let closed = point_type != LayerPointType::Move;
        let changed =
            path.nodes[index].nodetype != node_type || (index == 0 && path.closed != closed);
        if !changed {
            return Ok(false);
        }
        path.nodes[index].nodetype = node_type;
        if index == 0 {
            path.closed = closed;
        }
        Ok(true)
    }

    /// Set one point's smooth state by stable identity.
    ///
    /// Returns whether the value changed.
    pub fn set_point_smooth(
        &mut self,
        id: PointId,
        smooth: bool,
    ) -> Result<bool, DocumentEditError> {
        let node = self
            .node_mut(id)
            .ok_or(DocumentEditError::MissingPoint(id))?;
        if node.smooth == smooth {
            return Ok(false);
        }
        node.smooth = smooth;
        Ok(true)
    }

    /// Toggle smooth/corner state on selected on-curve points.
    ///
    /// Selected off-curve points are left unchanged.
    /// Returns whether any point changed.
    pub fn toggle_smooth_points(
        &mut self,
        selected: &[PointId],
    ) -> Result<bool, DocumentEditError> {
        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let selected: HashSet<_> = selected.iter().map(|id| id.0).collect();
        let mut changed = false;
        for node in self
            .layer
            .shapes
            .iter_mut()
            .filter_map(|shape| match shape {
                Shape::Path(path) => Some(path),
                Shape::Component(_) => None,
            })
            .flat_map(|path| &mut path.nodes)
        {
            let id = read_id(&node.format_specific).expect("canonical point identity");
            if selected.contains(&id) && node.nodetype != NodeType::OffCurve {
                node.smooth = !node.smooth;
                changed = true;
            }
        }
        Ok(changed)
    }

    /// Delete selected points while preserving surviving canonical identities and metadata.
    ///
    /// Deleting an on-curve point also removes its incoming controls. Deleting a cubic control
    /// removes both controls from that segment. Deleting a quadratic control materializes its
    /// implied endpoints and replaces only its segment with a line. Contours without a surviving
    /// segment are removed. Returns whether any topology changed.
    pub fn delete_points(&mut self, selected: &[PointId]) -> Result<bool, DocumentEditError> {
        let mut staged = self.clone();
        let changed = staged.delete_points_in_place(selected)?;
        if changed {
            *self = staged;
        }
        Ok(changed)
    }

    pub(super) fn delete_points_in_place(
        &mut self,
        selected: &[PointId],
    ) -> Result<bool, DocumentEditError> {
        if selected.is_empty() {
            return Ok(false);
        }
        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let selected: HashSet<_> = selected.iter().map(|id| id.0).collect();
        let mut changed = false;
        let mut shape_index = 0_usize;
        while shape_index < self.layer.shapes.len() {
            let Shape::Path(path) = &self.layer.shapes[shape_index] else {
                shape_index += 1;
                continue;
            };
            let contour_id =
                ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
            let point_ids: Vec<_> = path
                .nodes
                .iter()
                .map(|node| read_id(&node.format_specific).expect("canonical point identity"))
                .collect();
            if !point_ids.iter().any(|id| selected.contains(id)) {
                shape_index += 1;
                continue;
            }
            if point_ids.iter().all(|id| selected.contains(id)) {
                changed = true;
                self.layer.shapes.remove(shape_index);
                self.preserved
                    .contours
                    .retain(|candidate| candidate.id != contour_id);
                continue;
            }
            {
                let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
                    unreachable!("selected contour is a path");
                };
                let preserved = self
                    .preserved
                    .contours
                    .iter_mut()
                    .find(|candidate| candidate.id == contour_id)
                    .expect("canonical contour preservation");
                changed |= materialize_deleted_quadratic_controls(path, preserved, &selected)?;
            }
            let Shape::Path(path) = &self.layer.shapes[shape_index] else {
                unreachable!("selected contour is a path");
            };
            let point_ids: Vec<_> = path
                .nodes
                .iter()
                .map(|node| read_id(&node.format_specific).expect("canonical point identity"))
                .collect();
            if !point_ids.iter().any(|id| selected.contains(id)) {
                shape_index += 1;
                continue;
            }
            changed = true;
            let closed = path.closed;
            let on_indices: Vec<_> = path
                .nodes
                .iter()
                .enumerate()
                .filter_map(|(index, node)| (node.nodetype != NodeType::OffCurve).then_some(index))
                .collect();
            if on_indices.is_empty() {
                self.layer.shapes.remove(shape_index);
                self.preserved
                    .contours
                    .retain(|candidate| candidate.id != contour_id);
                continue;
            }
            struct SegmentRecord {
                on_index: usize,
                controls: Vec<usize>,
            }
            let controls_between = |start: usize, end: usize| {
                let mut controls = Vec::new();
                let mut index = start + 1;
                if index == path.nodes.len() {
                    index = 0;
                }
                while index != end {
                    controls.push(index);
                    index += 1;
                    if index == path.nodes.len() {
                        index = 0;
                    }
                }
                controls
            };
            let mut records = Vec::with_capacity(on_indices.len());
            for (position, on_index) in on_indices.iter().copied().enumerate() {
                let controls = if !closed && position == 0 {
                    Vec::new()
                } else {
                    let previous = if position == 0 {
                        *on_indices.last().expect("on-curve point exists")
                    } else {
                        on_indices[position - 1]
                    };
                    controls_between(previous, on_index)
                };
                records.push(SegmentRecord { on_index, controls });
            }
            for record in &mut records {
                if record
                    .controls
                    .iter()
                    .any(|index| selected.contains(&point_ids[*index]))
                {
                    record.controls.clear();
                }
            }
            // A deleted on-curve point joins the segments on either side
            // into one. The joined segment keeps the handles that survive
            // instead of collapsing to a line.
            let deleted: Vec<bool> = records
                .iter()
                .map(|record| selected.contains(&point_ids[record.on_index]))
                .collect();
            let mut moved_controls = HashMap::new();
            let mut joined = Vec::new();
            for (end, record) in records.iter().enumerate() {
                if deleted[end] {
                    continue;
                }
                // Walk back over the deleted points to the previous survivor.
                let mut chain = vec![end];
                let mut start = end;
                loop {
                    start = if start == 0 {
                        if !closed {
                            break;
                        }
                        records.len() - 1
                    } else {
                        start - 1
                    };
                    if !deleted[start] || start == end {
                        break;
                    }
                    chain.push(start);
                }
                if chain.len() == 1 || start == end || deleted[start] {
                    continue;
                }
                chain.reverse();
                let segments: Vec<&[usize]> = chain
                    .iter()
                    .map(|index| records[*index].controls.as_slice())
                    .collect();
                let point =
                    |index: usize| kurbo::Point::new(path.nodes[index].x, path.nodes[index].y);
                if let Some(controls) = join_segments(
                    point(records[start].on_index),
                    &chain
                        .iter()
                        .map(|index| point(records[*index].on_index))
                        .collect::<Vec<_>>(),
                    &segments,
                    &point,
                    &mut moved_controls,
                ) {
                    joined.push((end, controls));
                } else {
                    joined.push((end, record.controls.clone()));
                }
            }
            for (end, controls) in joined {
                records[end].controls = controls;
            }
            let mut position = 0;
            records.retain(|_| {
                position += 1;
                !deleted[position - 1]
            });
            if records.is_empty() {
                self.layer.shapes.remove(shape_index);
                self.preserved
                    .contours
                    .retain(|candidate| candidate.id != contour_id);
                continue;
            }
            if !closed {
                records[0].controls.clear();
            }
            let preserved = self
                .preserved
                .contours
                .iter_mut()
                .find(|candidate| candidate.id == contour_id)
                .expect("canonical contour preservation");
            let old_nodes = path.nodes.clone();
            let old_points = preserved.points.clone();
            let mut nodes = Vec::new();
            let mut points = Vec::new();
            let append = |index: usize, nodes: &mut Vec<Node>, points: &mut Vec<PreservedPoint>| {
                let mut node = old_nodes[index].clone();
                if let Some(position) = moved_controls.get(&index) {
                    node.x = position.x;
                    node.y = position.y;
                }
                nodes.push(node);
                points.push(old_points[index].clone());
            };
            for (position, record) in records.iter().enumerate() {
                if !(closed && position == 0) {
                    for control in &record.controls {
                        append(*control, &mut nodes, &mut points);
                    }
                }
                append(record.on_index, &mut nodes, &mut points);
                let endpoint = nodes.last_mut().expect("on-curve point was appended");
                if !closed && position == 0 {
                    endpoint.nodetype = NodeType::Move;
                } else if record.controls.is_empty() {
                    endpoint.nodetype = NodeType::Line;
                } else if endpoint.nodetype == NodeType::Line {
                    endpoint.nodetype = NodeType::Curve;
                }
            }
            if closed {
                for control in &records[0].controls {
                    append(*control, &mut nodes, &mut points);
                }
                if records[0].controls.is_empty() {
                    nodes[0].nodetype = NodeType::Line;
                } else if nodes[0].nodetype == NodeType::Line {
                    nodes[0].nodetype = NodeType::Curve;
                }
            }
            let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
                unreachable!("edited contour remains a path");
            };
            path.nodes = nodes;
            preserved.points = points;
            shape_index += 1;
        }
        Ok(changed)
    }

    /// Reverse every contour containing a selected point while retaining object identities.
    ///
    /// An empty selection reverses every nonempty contour. Closed contours retain their first
    /// stored point so two reversals restore the exact canonical storage order. Returns whether
    /// any topology changed.
    pub fn reverse_contours(&mut self, selected: &[PointId]) -> Result<bool, DocumentEditError> {
        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let selected: HashSet<_> = selected.iter().map(|id| id.0).collect();
        let reverse_all = selected.is_empty();
        let mut changed = false;
        for shape in &mut self.layer.shapes {
            let Shape::Path(path) = shape else {
                continue;
            };
            if path.nodes.is_empty()
                || (!reverse_all
                    && !path.nodes.iter().any(|node| {
                        read_id(&node.format_specific).is_some_and(|id| selected.contains(&id))
                    }))
            {
                continue;
            }
            let contour_id =
                ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
            let preserved = self
                .preserved
                .contours
                .iter_mut()
                .find(|candidate| candidate.id == contour_id)
                .expect("canonical contour preservation");
            changed |= reverse_contour(path, preserved);
        }
        Ok(changed)
    }

    /// Make an on-curve point the first stored point of its closed contour.
    ///
    /// The contour and every point retain their stable identities and source metadata. Returns
    /// whether the canonical storage order changed.
    pub fn set_contour_start(&mut self, point: PointId) -> Result<bool, DocumentEditError> {
        let (shape_index, point_index) = self
            .layer
            .shapes
            .iter()
            .enumerate()
            .find_map(|(shape_index, shape)| {
                let Shape::Path(path) = shape else {
                    return None;
                };
                let point_index = path
                    .nodes
                    .iter()
                    .position(|node| read_id(&node.format_specific) == Some(point.0))?;
                Some((shape_index, point_index))
            })
            .ok_or(DocumentEditError::MissingPoint(point))?;
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        if !path.closed
            || point_index == 0
            || path.nodes[point_index].nodetype == NodeType::OffCurve
        {
            return Ok(false);
        }
        let contour_id =
            ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located contour remains a path");
        };
        path.nodes.rotate_left(point_index);
        self.preserved
            .contours
            .iter_mut()
            .find(|candidate| candidate.id == contour_id)
            .expect("canonical contour preservation")
            .points
            .rotate_left(point_index);
        Ok(true)
    }

    /// Open a closed contour at an on-curve point, or close its open contour.
    ///
    /// Closing changes the initial move point to a line. Opening removes the selected endpoint's
    /// incoming controls, rotates that point to the start and changes it to a move. Returns whether
    /// the contour changed.
    pub fn toggle_contour_open(&mut self, point: PointId) -> Result<bool, DocumentEditError> {
        let (shape_index, point_index) = self
            .layer
            .shapes
            .iter()
            .enumerate()
            .find_map(|(shape_index, shape)| {
                let Shape::Path(path) = shape else {
                    return None;
                };
                let point_index = path
                    .nodes
                    .iter()
                    .position(|node| read_id(&node.format_specific) == Some(point.0))?;
                Some((shape_index, point_index))
            })
            .ok_or(DocumentEditError::MissingPoint(point))?;
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        if path.nodes.len() < 2
            || (path.closed && path.nodes[point_index].nodetype == NodeType::OffCurve)
        {
            return Ok(false);
        }
        let incoming_controls = if path.closed {
            let mut count = 0_usize;
            let mut index = if point_index == 0 {
                path.nodes.len() - 1
            } else {
                point_index - 1
            };
            while path.nodes[index].nodetype == NodeType::OffCurve {
                count += 1;
                index = if index == 0 {
                    path.nodes.len() - 1
                } else {
                    index - 1
                };
            }
            if path.nodes.len() - count < 2 {
                return Ok(false);
            }
            count
        } else {
            0
        };
        let contour_id =
            ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located contour remains a path");
        };
        if path.closed {
            path.nodes.rotate_left(point_index);
            let preserved = self
                .preserved
                .contours
                .iter_mut()
                .find(|candidate| candidate.id == contour_id)
                .expect("canonical contour preservation");
            preserved.points.rotate_left(point_index);
            path.nodes.truncate(path.nodes.len() - incoming_controls);
            preserved
                .points
                .truncate(preserved.points.len() - incoming_controls);
            path.nodes[0].nodetype = NodeType::Move;
            path.closed = false;
        } else {
            path.nodes[0].nodetype = NodeType::Line;
            path.closed = true;
        }
        Ok(true)
    }

    /// Shift every contour point and anchor horizontally.
    ///
    /// Component transforms and the advance remain unchanged, matching a left-sidebearing edit.
    /// Returns whether any geometry moved.
    pub fn shift_points_and_anchors_x(&mut self, delta: f64) -> Result<bool, DocumentEditError> {
        ensure_finite(&[delta])?;
        if delta == 0.0 {
            return Ok(false);
        }
        let mut has_geometry = false;
        for node in self.layer.paths().flat_map(|path| &path.nodes) {
            ensure_finite(&[node.x + delta])?;
            has_geometry = true;
        }
        for anchor in &self.layer.anchors {
            ensure_finite(&[anchor.x + delta])?;
            has_geometry = true;
        }
        if !has_geometry {
            return Ok(false);
        }
        for node in self
            .layer
            .shapes
            .iter_mut()
            .filter_map(|shape| match shape {
                Shape::Path(path) => Some(path),
                Shape::Component(_) => None,
            })
            .flat_map(|path| &mut path.nodes)
        {
            node.x += delta;
        }
        for anchor in &mut self.layer.anchors {
            anchor.x += delta;
        }
        Ok(true)
    }
}

/// Choose the controls of one segment that replaces a chain of segments
/// joined by deleted on-curve points.
///
/// `segments` holds the control indices of each segment in the chain, and
/// `ends` the on-curve point that ends each one. Lines stay out of the way:
/// if one segment is a curve, its handles stay where they are. If several
/// are curves, the outer two handles keep their directions and get the
/// lengths that best fit the old chain; `moved` receives their new
/// positions. Returns `None` for a chain this join does not handle, such
/// as a quadratic segment.
fn join_segments(
    start: kurbo::Point,
    ends: &[kurbo::Point],
    segments: &[&[usize]],
    point: &impl Fn(usize) -> kurbo::Point,
    moved: &mut HashMap<usize, kurbo::Point>,
) -> Option<Vec<usize>> {
    use kurbo::{ParamCurve as _, ParamCurveDeriv as _};
    if segments
        .iter()
        .any(|controls| !matches!(controls.len(), 0 | 2))
    {
        return None;
    }
    let curved: Vec<&[usize]> = segments
        .iter()
        .copied()
        .filter(|controls| controls.len() == 2)
        .collect();
    let (first, last) = match curved.as_slice() {
        [] => return Some(Vec::new()),
        [only] => return Some(only.to_vec()),
        [first, .., last] => (first[0], last[1]),
    };
    let p0 = start;
    let p3 = *ends.last()?;
    let (t0, t1) = (point(first) - p0, p3 - point(last));
    if t0.hypot() < 1e-9 || t1.hypot() < 1e-9 {
        return Some(vec![first, last]);
    }
    let (t0, t1) = (t0.normalize(), t1.normalize());

    // Sample the old chain and give each sample a chord-length parameter.
    let mut samples = vec![p0];
    let mut from = p0;
    for (controls, &to) in segments.iter().zip(ends) {
        let cubic = match controls {
            [a, b] => kurbo::CubicBez::new(from, point(*a), point(*b), to),
            _ => kurbo::CubicBez::new(from, from.lerp(to, 1.0 / 3.0), from.lerp(to, 2.0 / 3.0), to),
        };
        samples.extend((1..=16).map(|step| cubic.eval(f64::from(step) / 16.0)));
        from = to;
    }
    let mut lengths = vec![0.0];
    for pair in samples.windows(2) {
        lengths.push(lengths.last().copied().unwrap_or(0.0) + pair[0].distance(pair[1]));
    }
    let total = lengths.last().copied().unwrap_or(0.0);
    if total < 1e-9 {
        return Some(vec![first, last]);
    }

    // Least squares for the two handle lengths (Schneider's curve fit),
    // with a few Newton steps that refine each sample's parameter.
    let mut params: Vec<f64> = lengths.iter().map(|length| length / total).collect();
    let (mut a, mut b) = (0.0, 0.0);
    for round in 0..6 {
        let (mut c11, mut c12, mut c22, mut x1, mut x2) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (sample, &u) in samples.iter().zip(&params) {
            let v = 1.0 - u;
            let (b0, b1, b2, b3) = (v * v * v, 3.0 * u * v * v, 3.0 * u * u * v, u * u * u);
            let a1 = t0 * b1;
            let a2 = -t1 * b2;
            let rest = sample.to_vec2() - (p0.to_vec2() * (b0 + b1) + p3.to_vec2() * (b2 + b3));
            c11 += a1.dot(a1);
            c12 += a1.dot(a2);
            c22 += a2.dot(a2);
            x1 += a1.dot(rest);
            x2 += a2.dot(rest);
        }
        let det = c11 * c22 - c12 * c12;
        if det.abs() < 1e-12 {
            return Some(vec![first, last]);
        }
        a = (x1 * c22 - c12 * x2) / det;
        b = (c11 * x2 - c12 * x1) / det;
        if round == 5 || a <= 0.0 || b <= 0.0 {
            break;
        }
        let fit = kurbo::CubicBez::new(p0, p0 + t0 * a, p3 - t1 * b, p3);
        let d1 = fit.deriv();
        let d2 = d1.deriv();
        for (sample, u) in samples.iter().zip(params.iter_mut()) {
            let offset = fit.eval(*u) - *sample;
            let first_derivative = d1.eval(*u).to_vec2();
            let denominator =
                first_derivative.dot(first_derivative) + offset.dot(d2.eval(*u).to_vec2());
            if denominator.abs() > 1e-12 {
                *u = (*u - offset.dot(first_derivative) / denominator).clamp(0.0, 1.0);
            }
        }
    }
    let chord = p0.distance(p3);
    if a <= chord * 1e-3 || b <= chord * 1e-3 {
        return Some(vec![first, last]);
    }
    let snap = |p: kurbo::Point| {
        use crate::outline::point_ops::snap_coord;
        kurbo::Point::new(snap_coord(p.x), snap_coord(p.y))
    };
    moved.insert(first, snap(p0 + t0 * a));
    moved.insert(last, snap(p3 - t1 * b));
    Some(vec![first, last])
}
