// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Direct segment insertion and line-to-curve conversion.

use super::*;

impl LayerEditDraft {
    /// Insert one on-curve point on a direct segment between two stored endpoints.
    ///
    /// Existing controls retain their identities and metadata while moving to their subdivided
    /// positions. Newly required controls and the inserted point receive fresh identities. A
    /// point that splits a curve is smooth, since its two handles are collinear.
    /// Segments ending at implied quadratic points are handled by a later topology operation.
    pub fn insert_point_on_segment(
        &mut self,
        start: PointId,
        end: PointId,
        parameter: f64,
    ) -> Result<PointId, DocumentEditError> {
        ensure_finite(&[parameter])?;
        let parameter = parameter.clamp(0.0, 1.0);
        let locate = |id: PointId| {
            self.layer
                .shapes
                .iter()
                .enumerate()
                .find_map(|(shape_index, shape)| {
                    let Shape::Path(path) = shape else {
                        return None;
                    };
                    path.nodes
                        .iter()
                        .position(|node| read_id(&node.format_specific) == Some(id.0))
                        .map(|node_index| (shape_index, node_index))
                })
        };
        let (shape_index, start_index) =
            locate(start).ok_or(DocumentEditError::MissingPoint(start))?;
        let (end_shape, end_index) = locate(end).ok_or(DocumentEditError::MissingPoint(end))?;
        if shape_index != end_shape {
            return Err(DocumentEditError::NotDirectSegment(start, end));
        }
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        if path.nodes.len() < 2
            || path.nodes[start_index].nodetype == NodeType::OffCurve
            || path.nodes[end_index].nodetype == NodeType::OffCurve
            || (!path.closed && end_index <= start_index)
        {
            return Err(DocumentEditError::NotDirectSegment(start, end));
        }
        let mut control_indices = Vec::new();
        let mut index = (start_index + 1) % path.nodes.len();
        while index != end_index {
            if path.nodes[index].nodetype != NodeType::OffCurve {
                return Err(DocumentEditError::NotDirectSegment(start, end));
            }
            control_indices.push(index);
            index = (index + 1) % path.nodes.len();
            if !path.closed && index == 0 {
                return Err(DocumentEditError::NotDirectSegment(start, end));
            }
        }
        let start_position =
            kurbo::Point::new(path.nodes[start_index].x, path.nodes[start_index].y);
        let end_position = kurbo::Point::new(path.nodes[end_index].x, path.nodes[end_index].y);
        let endpoint_type = path.nodes[end_index].nodetype;
        let snap = |point: kurbo::Point| {
            kurbo::Point::new(
                crate::outline::point_ops::snap_coord(point.x),
                crate::outline::point_ops::snap_coord(point.y),
            )
        };
        enum Split {
            Line(kurbo::Point),
            Quadratic {
                control: usize,
                left_control: kurbo::Point,
                split: kurbo::Point,
                right_control: kurbo::Point,
            },
            Cubic {
                first_control: usize,
                second_control: usize,
                left_first: kurbo::Point,
                left_second: kurbo::Point,
                split: kurbo::Point,
                right_first: kurbo::Point,
                right_second: kurbo::Point,
            },
        }
        let split = match (control_indices.as_slice(), endpoint_type) {
            ([], _) => Split::Line(snap(start_position.lerp(end_position, parameter))),
            ([control], NodeType::Curve | NodeType::QCurve) => {
                let control_position =
                    kurbo::Point::new(path.nodes[*control].x, path.nodes[*control].y);
                let quad = kurbo::QuadBez::new(start_position, control_position, end_position);
                let left = quad.subsegment(0.0..parameter);
                let right = quad.subsegment(parameter..1.0);
                Split::Quadratic {
                    control: *control,
                    left_control: snap(left.p1),
                    split: snap(left.p2),
                    right_control: snap(right.p1),
                }
            }
            ([first, second], NodeType::Curve) => {
                let first_position = kurbo::Point::new(path.nodes[*first].x, path.nodes[*first].y);
                let second_position =
                    kurbo::Point::new(path.nodes[*second].x, path.nodes[*second].y);
                let cubic = kurbo::CubicBez::new(
                    start_position,
                    first_position,
                    second_position,
                    end_position,
                );
                let left = cubic.subsegment(0.0..parameter);
                let right = cubic.subsegment(parameter..1.0);
                Split::Cubic {
                    first_control: *first,
                    second_control: *second,
                    left_first: snap(left.p1),
                    left_second: snap(left.p2),
                    split: snap(left.p3),
                    right_first: snap(right.p1),
                    right_second: snap(right.p2),
                }
            }
            _ => return Err(DocumentEditError::NotDirectSegment(start, end)),
        };
        match &split {
            Split::Line(point) => ensure_finite(&[point.x, point.y])?,
            Split::Quadratic {
                left_control,
                split,
                right_control,
                ..
            } => ensure_finite(&[
                left_control.x,
                left_control.y,
                split.x,
                split.y,
                right_control.x,
                right_control.y,
            ])?,
            Split::Cubic {
                left_first,
                left_second,
                split,
                right_first,
                right_second,
                ..
            } => ensure_finite(&[
                left_first.x,
                left_first.y,
                left_second.x,
                left_second.y,
                split.x,
                split.y,
                right_first.x,
                right_first.y,
                right_second.x,
                right_second.y,
            ])?,
        }
        let contour_id =
            ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        let preserved = self
            .preserved
            .contours
            .iter_mut()
            .find(|candidate| candidate.id == contour_id)
            .expect("canonical contour preservation");
        let inserted = match split {
            Split::Line(position) => {
                let created = new_document_point(position, NodeType::Line, false);
                let insert_index = if end_index == 0 {
                    path.nodes.len()
                } else {
                    end_index
                };
                path.nodes.insert(insert_index, created.1);
                preserved.points.insert(insert_index, created.2);
                created.0
            }
            Split::Quadratic {
                control,
                left_control,
                split,
                right_control,
            } => {
                path.nodes[control].x = left_control.x;
                path.nodes[control].y = left_control.y;
                let split = new_document_point(split, NodeType::QCurve, true);
                let right = new_document_point(right_control, NodeType::OffCurve, false);
                let insert_index = control + 1;
                path.nodes.insert(insert_index, split.1);
                path.nodes.insert(insert_index + 1, right.1);
                preserved.points.insert(insert_index, split.2);
                preserved.points.insert(insert_index + 1, right.2);
                split.0
            }
            Split::Cubic {
                first_control,
                second_control,
                left_first,
                left_second,
                split,
                right_first,
                right_second,
            } => {
                path.nodes[first_control].x = left_first.x;
                path.nodes[first_control].y = left_first.y;
                path.nodes[second_control].x = right_second.x;
                path.nodes[second_control].y = right_second.y;
                let left = new_document_point(left_second, NodeType::OffCurve, false);
                let split = new_document_point(split, NodeType::Curve, true);
                let right = new_document_point(right_first, NodeType::OffCurve, false);
                path.nodes.insert(second_control, left.1);
                path.nodes.insert(second_control + 1, split.1);
                path.nodes.insert(second_control + 2, right.1);
                preserved.points.insert(second_control, left.2);
                preserved.points.insert(second_control + 1, split.2);
                preserved.points.insert(second_control + 2, right.2);
                split.0
            }
        };
        Ok(inserted)
    }

    /// Insert one on-curve point on a quadratic segment with stored or implied endpoints.
    ///
    /// An implied endpoint is materialized as a fresh on-curve point when subdivision would
    /// otherwise move either control that defines it. The source control retains its identity and
    /// metadata. All computed coordinates are validated before mutation.
    pub fn insert_point_on_quadratic_segment(
        &mut self,
        start: DocumentSegmentEndpoint,
        control: PointId,
        end: DocumentSegmentEndpoint,
        parameter: f64,
    ) -> Result<QuadraticSegmentInsertion, DocumentEditError> {
        ensure_finite(&[parameter])?;
        let parameter = parameter.clamp(0.0, 1.0);
        let representative = |endpoint| match endpoint {
            DocumentSegmentEndpoint::Point(id) => id,
            DocumentSegmentEndpoint::Implied { first_control, .. } => first_control,
        };
        let invalid =
            || DocumentEditError::NotDirectSegment(representative(start), representative(end));
        let locate = |id: PointId| {
            self.layer
                .shapes
                .iter()
                .enumerate()
                .find_map(|(shape_index, shape)| {
                    let Shape::Path(path) = shape else {
                        return None;
                    };
                    path.nodes
                        .iter()
                        .position(|node| read_id(&node.format_specific) == Some(id.0))
                        .map(|node_index| (shape_index, node_index))
                })
        };
        let (shape_index, control_index) =
            locate(control).ok_or(DocumentEditError::MissingPoint(control))?;
        let resolve = |endpoint: DocumentSegmentEndpoint| match endpoint {
            DocumentSegmentEndpoint::Point(id) => {
                let (shape, index) = locate(id).ok_or(DocumentEditError::MissingPoint(id))?;
                let Shape::Path(path) = &self.layer.shapes[shape] else {
                    unreachable!("located endpoint is in a path");
                };
                Ok((
                    shape,
                    kurbo::Point::new(path.nodes[index].x, path.nodes[index].y),
                    Some((index, index)),
                ))
            }
            DocumentSegmentEndpoint::Implied {
                first_control,
                second_control,
            } => {
                let (first_shape, first) =
                    locate(first_control).ok_or(DocumentEditError::MissingPoint(first_control))?;
                let (second_shape, second) = locate(second_control)
                    .ok_or(DocumentEditError::MissingPoint(second_control))?;
                if first_shape != second_shape {
                    return Err(invalid());
                }
                let Shape::Path(path) = &self.layer.shapes[first_shape] else {
                    unreachable!("located endpoint is in a path");
                };
                Ok((
                    first_shape,
                    kurbo::Point::new(path.nodes[first].x, path.nodes[first].y).midpoint(
                        kurbo::Point::new(path.nodes[second].x, path.nodes[second].y),
                    ),
                    Some((first, second)),
                ))
            }
        };
        let (start_shape, start_position, start_indices) = resolve(start)?;
        let (end_shape, end_position, end_indices) = resolve(end)?;
        if start_shape != shape_index || end_shape != shape_index {
            return Err(invalid());
        }
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located segment is in a path");
        };
        if path.nodes[control_index].nodetype != NodeType::OffCurve {
            return Err(invalid());
        }
        let next = |index| {
            if index + 1 < path.nodes.len() {
                Some(index + 1)
            } else if path.closed {
                Some(0)
            } else {
                None
            }
        };
        let is_quadratic_pair = |first: usize, second: usize| {
            if path.nodes[first].nodetype != NodeType::OffCurve
                || path.nodes[second].nodetype != NodeType::OffCurve
                || next(first) != Some(second)
            {
                return false;
            }
            let mut index = second;
            for _ in 0..path.nodes.len() {
                let Some(candidate) = next(index) else {
                    return false;
                };
                if path.nodes[candidate].nodetype != NodeType::OffCurve {
                    return path.nodes[candidate].nodetype == NodeType::QCurve;
                }
                index = candidate;
            }
            path.closed
        };
        let start_valid = match start {
            DocumentSegmentEndpoint::Point(_) => {
                let index = start_indices.expect("stored endpoint index").0;
                path.nodes[index].nodetype != NodeType::OffCurve
                    && next(index) == Some(control_index)
            }
            DocumentSegmentEndpoint::Implied { .. } => {
                let (first, second) = start_indices.expect("implied endpoint indices");
                is_quadratic_pair(first, second) && second == control_index
            }
        };
        let end_valid = match end {
            DocumentSegmentEndpoint::Point(_) => {
                let index = end_indices.expect("stored endpoint index").0;
                path.nodes[index].nodetype != NodeType::OffCurve
                    && next(control_index) == Some(index)
                    && matches!(
                        path.nodes[index].nodetype,
                        NodeType::Curve | NodeType::QCurve
                    )
            }
            DocumentSegmentEndpoint::Implied { .. } => {
                let (first, second) = end_indices.expect("implied endpoint indices");
                is_quadratic_pair(first, second) && first == control_index
            }
        };
        if !start_valid || !end_valid {
            return Err(invalid());
        }
        let control_position =
            kurbo::Point::new(path.nodes[control_index].x, path.nodes[control_index].y);
        let quad = kurbo::QuadBez::new(start_position, control_position, end_position);
        let left = quad.subsegment(0.0..parameter);
        let right = quad.subsegment(parameter..1.0);
        let snap = |point: kurbo::Point| {
            kurbo::Point::new(
                crate::outline::point_ops::snap_coord(point.x),
                crate::outline::point_ops::snap_coord(point.y),
            )
        };
        let left_control = snap(left.p1);
        let split_position = snap(left.p2);
        let right_control = snap(right.p1);
        ensure_finite(&[
            start_position.x,
            start_position.y,
            end_position.x,
            end_position.y,
            left_control.x,
            left_control.y,
            split_position.x,
            split_position.y,
            right_control.x,
            right_control.y,
        ])?;
        let contour_id =
            ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located segment is in a path");
        };
        let preserved = self
            .preserved
            .contours
            .iter_mut()
            .find(|candidate| candidate.id == contour_id)
            .expect("canonical contour preservation");

        let mut control_index = control_index;
        let explicitized_start =
            matches!(start, DocumentSegmentEndpoint::Implied { .. }).then(|| {
                let created = new_document_point(start_position, NodeType::QCurve, false);
                path.nodes.insert(control_index, created.1);
                preserved.points.insert(control_index, created.2);
                control_index += 1;
                created.0
            });
        path.nodes[control_index].x = left_control.x;
        path.nodes[control_index].y = left_control.y;
        let split = new_document_point(split_position, NodeType::QCurve, false);
        let right = new_document_point(right_control, NodeType::OffCurve, false);
        let insert_index = control_index + 1;
        path.nodes.insert(insert_index, split.1);
        path.nodes.insert(insert_index + 1, right.1);
        preserved.points.insert(insert_index, split.2);
        preserved.points.insert(insert_index + 1, right.2);
        let explicitized_end = matches!(end, DocumentSegmentEndpoint::Implied { .. }).then(|| {
            let created = new_document_point(end_position, NodeType::QCurve, false);
            path.nodes.insert(insert_index + 2, created.1);
            preserved.points.insert(insert_index + 2, created.2);
            created.0
        });
        Ok(QuadraticSegmentInsertion {
            point: split.0,
            explicitized_start,
            explicitized_end,
        })
    }

    /// Give a straight segment, or a curve with a single control, two cubic handles.
    ///
    /// This is Option-click on a segment in Glyphs. A straight segment gets handles at its
    /// thirds. A single-control curve is read as quadratic, whether stored as a quadratic or
    /// as a one-control cubic, and is degree-elevated so its shape does not change: the
    /// control keeps its identity as the first handle and a new handle follows it.
    /// A segment that already has two handles is rejected.
    /// Returns the two handle identities in contour order.
    pub fn add_segment_handles(
        &mut self,
        start: PointId,
        end: PointId,
    ) -> Result<[PointId; 2], DocumentEditError> {
        let (shape_index, start_index) = self
            .locate_point(start)
            .ok_or(DocumentEditError::MissingPoint(start))?;
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located shape is a path");
        };
        let len = path.nodes.len();
        let after = |index: usize, steps: usize| {
            let next = index + steps;
            if next < len {
                Some(next)
            } else if path.closed {
                Some(next % len)
            } else {
                None
            }
        };
        let is_end = |index: Option<usize>| {
            index.is_some_and(|index| {
                read_id(&path.nodes[index].format_specific) == Some(end.0)
                    && path.nodes[index].nodetype != NodeType::OffCurve
            })
        };
        if is_end(after(start_index, 1)) {
            return self.convert_line_to_curve(start, end);
        }
        let (Some(control_index), end_index) = (after(start_index, 1), after(start_index, 2))
        else {
            return Err(DocumentEditError::NotLineSegment(start, end));
        };
        if path.nodes[control_index].nodetype != NodeType::OffCurve || !is_end(end_index) {
            return Err(DocumentEditError::NotLineSegment(start, end));
        }
        let end_index = end_index.expect("checked end");
        let position = |index: usize| kurbo::Point::new(path.nodes[index].x, path.nodes[index].y);
        let (from, control, to) = (
            position(start_index),
            position(control_index),
            position(end_index),
        );
        let snapped = |point: kurbo::Point| {
            kurbo::Point::new(
                crate::outline::point_ops::snap_coord(point.x),
                crate::outline::point_ops::snap_coord(point.y),
            )
        };
        let first_position = snapped(from.lerp(control, 2.0 / 3.0));
        let second_position = snapped(to.lerp(control, 2.0 / 3.0));
        ensure_finite(&[
            first_position.x,
            first_position.y,
            second_position.x,
            second_position.y,
        ])?;
        let first = PointId(
            read_id(&path.nodes[control_index].format_specific).expect("canonical point identity"),
        );
        let second = PointId::next();
        let contour_id =
            ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located shape is a path");
        };
        path.nodes[control_index].x = first_position.x;
        path.nodes[control_index].y = first_position.y;
        let mut node = Node {
            x: second_position.x,
            y: second_position.y,
            nodetype: NodeType::OffCurve,
            ..Node::default()
        };
        write_id(&mut node.format_specific, second.0);
        let insert_index = control_index + 1;
        path.nodes.insert(insert_index, node);
        let end_index = if end_index >= insert_index {
            end_index + 1
        } else {
            end_index
        };
        path.nodes[end_index].nodetype = NodeType::Curve;
        let preserved = self
            .preserved
            .contours
            .iter_mut()
            .find(|candidate| candidate.id == contour_id)
            .expect("canonical contour preservation");
        preserved.points.insert(
            insert_index,
            PreservedPoint {
                id: second,
                name: None,
                metadata: ObjectMetadata {
                    identifier: None,
                    lib: None,
                },
            },
        );
        Ok([first, second])
    }

    /// The path shape and node index holding a point.
    fn locate_point(&self, id: PointId) -> Option<(usize, usize)> {
        self.layer
            .shapes
            .iter()
            .enumerate()
            .find_map(|(shape_index, shape)| {
                let Shape::Path(path) = shape else {
                    return None;
                };
                path.nodes
                    .iter()
                    .position(|node| read_id(&node.format_specific) == Some(id.0))
                    .map(|node_index| (shape_index, node_index))
            })
    }

    /// Convert one direct on-curve segment to a cubic with snapped thirds handles.
    ///
    /// The endpoints retain their stable identities and source metadata.
    /// Returns the new control-point identities in contour order.
    pub fn convert_line_to_curve(
        &mut self,
        start: PointId,
        end: PointId,
    ) -> Result<[PointId; 2], DocumentEditError> {
        let locate = |id: PointId| {
            self.layer
                .shapes
                .iter()
                .enumerate()
                .find_map(|(shape_index, shape)| {
                    let Shape::Path(path) = shape else {
                        return None;
                    };
                    path.nodes
                        .iter()
                        .position(|node| read_id(&node.format_specific) == Some(id.0))
                        .map(|node_index| (shape_index, node_index))
                })
        };
        let (shape_index, start_index) =
            locate(start).ok_or(DocumentEditError::MissingPoint(start))?;
        let (end_shape, end_index) = locate(end).ok_or(DocumentEditError::MissingPoint(end))?;
        if shape_index != end_shape {
            return Err(DocumentEditError::NotLineSegment(start, end));
        }
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located shape is a path");
        };
        let wraps = path.closed && start_index + 1 == path.nodes.len() && end_index == 0;
        if !(end_index == start_index + 1 || wraps)
            || path.nodes[start_index].nodetype == NodeType::OffCurve
            || path.nodes[end_index].nodetype == NodeType::OffCurve
        {
            return Err(DocumentEditError::NotLineSegment(start, end));
        }
        let start_position =
            kurbo::Point::new(path.nodes[start_index].x, path.nodes[start_index].y);
        let end_position = kurbo::Point::new(path.nodes[end_index].x, path.nodes[end_index].y);
        let snapped = |point: kurbo::Point| {
            kurbo::Point::new(
                crate::outline::point_ops::snap_coord(point.x),
                crate::outline::point_ops::snap_coord(point.y),
            )
        };
        let first_position = snapped(start_position.lerp(end_position, 1.0 / 3.0));
        let second_position = snapped(start_position.lerp(end_position, 2.0 / 3.0));
        ensure_finite(&[
            first_position.x,
            first_position.y,
            second_position.x,
            second_position.y,
        ])?;
        let point_ids = [PointId::next(), PointId::next()];
        let node = |id: PointId, position: kurbo::Point| {
            let mut node = Node {
                x: position.x,
                y: position.y,
                nodetype: NodeType::OffCurve,
                ..Node::default()
            };
            write_id(&mut node.format_specific, id.0);
            node
        };
        let insert_index = if wraps { start_index + 1 } else { end_index };
        let contour_id =
            ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located shape is a path");
        };
        path.nodes
            .insert(insert_index, node(point_ids[0], first_position));
        path.nodes
            .insert(insert_index + 1, node(point_ids[1], second_position));
        let shifted_end = if wraps { end_index } else { end_index + 2 };
        path.nodes[shifted_end].nodetype = NodeType::Curve;
        let preserved = self
            .preserved
            .contours
            .iter_mut()
            .find(|candidate| candidate.id == contour_id)
            .expect("canonical contour preservation");
        for (offset, id) in point_ids.iter().copied().enumerate() {
            preserved.points.insert(
                insert_index + offset,
                PreservedPoint {
                    id,
                    name: None,
                    metadata: ObjectMetadata {
                        identifier: None,
                        lib: None,
                    },
                },
            );
        }
        Ok(point_ids)
    }
}
