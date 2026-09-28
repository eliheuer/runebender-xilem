// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Cleanup, fitting, path effects, and contour replacement commands.

use super::*;

impl LayerEditDraft {
    /// Remove duplicate zero-length line endpoints while retaining every surviving object.
    ///
    /// Returns the number of removed points.
    pub fn tidy_contours(&mut self) -> usize {
        let mut removed = 0_usize;
        for shape in &mut self.layer.shapes {
            let Shape::Path(path) = shape else {
                continue;
            };
            let contour_id =
                ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
            let preserved = self
                .preserved
                .contours
                .iter_mut()
                .find(|candidate| candidate.id == contour_id)
                .expect("canonical contour preservation");
            let mut index = 1;
            while index < path.nodes.len() {
                let previous = &path.nodes[index - 1];
                let point = &path.nodes[index];
                let duplicate = point.nodetype == NodeType::Line
                    && previous.nodetype != NodeType::OffCurve
                    && (point.x - previous.x).abs() < 0.01
                    && (point.y - previous.y).abs() < 0.01;
                if duplicate {
                    path.nodes.remove(index);
                    preserved.points.remove(index);
                    removed += 1;
                } else {
                    index += 1;
                }
            }
            if path.closed && path.nodes.len() > 2 {
                let first = &path.nodes[0];
                let last = path.nodes.last().expect("closed contour has nodes");
                if last.nodetype == NodeType::Line
                    && first.nodetype != NodeType::OffCurve
                    && (last.x - first.x).abs() < 0.01
                    && (last.y - first.y).abs() < 0.01
                {
                    path.nodes.pop();
                    preserved.points.pop();
                    removed += 1;
                }
            }
        }
        removed
    }

    /// Round every canonical contour point to integer coordinates.
    ///
    /// Stable identities and source metadata remain attached to their points. Returns the number
    /// of points that moved.
    pub fn round_coordinates(&mut self) -> usize {
        let mut moved = 0_usize;
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
            let rounded = (node.x.round(), node.y.round());
            if (node.x, node.y) != rounded {
                node.x = rounded.0;
                node.y = rounded.1;
                moved += 1;
            }
        }
        moved
    }

    /// Rewind canonical contours to counterclockwise outers and clockwise holes.
    ///
    /// Contours and points retain their stable identities and source metadata. Returns the number
    /// of contours reversed.
    pub fn correct_path_directions(&mut self) -> Result<usize, DocumentEditError> {
        use kurbo::Shape as _;

        let layer = self.view();
        let contours: Vec<_> = layer.contours().collect();
        let paths: Vec<_> = contours
            .iter()
            .map(|contour| crate::outline::glyph_paths::ordinary_contour_to_bezpath(*contour))
            .collect();
        let mut selected = Vec::new();
        for (index, contour) in contours.iter().enumerate() {
            let Some(probe) = contour
                .points()
                .find(|point| point.point_type() != LayerPointType::OffCurve)
            else {
                continue;
            };
            let depth = paths
                .iter()
                .enumerate()
                .filter(|(other, path)| *other != index && path.contains(probe.position()))
                .count();
            let area = paths[index].area();
            let wants_counterclockwise = depth % 2 == 0;
            if (wants_counterclockwise && area < 0.0) || (!wants_counterclockwise && area > 0.0) {
                selected.push(probe.id());
            }
        }
        if selected.is_empty() {
            return Ok(0);
        }
        self.reverse_contours(&selected)?;
        Ok(selected.len())
    }

    /// Scale selected cubic handles to a fraction of their tangent-intersection maximum.
    ///
    /// An empty selection fits every cubic segment. Existing point identities and source metadata
    /// remain attached to moved controls. Returns whether any control moved.
    pub fn fit_curve_handles(
        &mut self,
        selected: &[PointId],
        fraction: f64,
    ) -> Result<bool, DocumentEditError> {
        if !(0.01..=1.5).contains(&fraction) {
            return Ok(false);
        }
        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let selected: HashSet<_> = selected.iter().map(|id| id.0).collect();
        let fit_all = selected.is_empty();
        let cross =
            |first: kurbo::Vec2, second: kurbo::Vec2| first.x * second.y - first.y * second.x;
        let mut replacements = HashMap::new();
        for path in self.layer.paths() {
            let count = path.nodes.len();
            if count < 4 {
                continue;
            }
            for end in 0..count {
                if path.nodes[end].nodetype != NodeType::Curve {
                    continue;
                }
                let second_control = (end + count - 1) % count;
                let first_control = (end + count - 2) % count;
                let start = (end + count - 3) % count;
                if path.nodes[first_control].nodetype != NodeType::OffCurve
                    || path.nodes[second_control].nodetype != NodeType::OffCurve
                    || path.nodes[start].nodetype == NodeType::OffCurve
                {
                    continue;
                }
                if !fit_all
                    && ![start, first_control, second_control, end]
                        .iter()
                        .any(|index| {
                            read_id(&path.nodes[*index].format_specific)
                                .is_some_and(|id| selected.contains(&id))
                        })
                {
                    continue;
                }
                let start_point = kurbo::Point::new(path.nodes[start].x, path.nodes[start].y);
                let first_point =
                    kurbo::Point::new(path.nodes[first_control].x, path.nodes[first_control].y);
                let second_point =
                    kurbo::Point::new(path.nodes[second_control].x, path.nodes[second_control].y);
                let end_point = kurbo::Point::new(path.nodes[end].x, path.nodes[end].y);
                let first_direction = first_point - start_point;
                let second_direction = second_point - end_point;
                if first_direction.hypot() < 1e-9 || second_direction.hypot() < 1e-9 {
                    continue;
                }
                let first_direction = first_direction / first_direction.hypot();
                let second_direction = second_direction / second_direction.hypot();
                let denominator = cross(first_direction, second_direction);
                if denominator.abs() < 1e-9 {
                    continue;
                }
                let between = end_point - start_point;
                let first_maximum = cross(between, second_direction) / denominator;
                let second_maximum = cross(between, first_direction) / denominator;
                if first_maximum <= 0.0 || second_maximum <= 0.0 {
                    continue;
                }
                let first = start_point + first_direction * (first_maximum * fraction);
                let second = end_point + second_direction * (second_maximum * fraction);
                let first = kurbo::Point::new(first.x.round(), first.y.round());
                let second = kurbo::Point::new(second.x.round(), second.y.round());
                ensure_finite(&[first.x, first.y, second.x, second.y])?;
                if first != first_point {
                    replacements.insert(
                        read_id(&path.nodes[first_control].format_specific)
                            .expect("canonical point identity"),
                        first,
                    );
                }
                if second != second_point {
                    replacements.insert(
                        read_id(&path.nodes[second_control].format_specific)
                            .expect("canonical point identity"),
                        second,
                    );
                }
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

    /// Insert on-curve points at selected cubic extrema.
    ///
    /// An empty selection considers every ordinary cubic segment. Hyperbezier contours remain on
    /// their editable source path. Returns whether any point was inserted.
    pub fn add_extreme_points(&mut self, selected: &[PointId]) -> Result<bool, DocumentEditError> {
        use kurbo::ParamCurveExtrema as _;

        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let selected: HashSet<_> = selected.iter().copied().collect();
        let mut staged = self.clone();
        let mut changed = false;
        for _ in 0..300 {
            let candidate = crate::outline::segment_ops::ordinary_layer_segments(staged.view())
                .into_iter()
                .find_map(|segment| {
                    let kurbo::PathSeg::Cubic(cubic) = segment.seg else {
                        return None;
                    };
                    if !selected.is_empty()
                        && !segment.point_ids().iter().any(|id| selected.contains(id))
                    {
                        return None;
                    }
                    let parameter = cubic
                        .extrema()
                        .into_iter()
                        .find(|parameter| (0.02..=0.98).contains(parameter))?;
                    let (
                        DocumentSegmentEndpoint::Point(start),
                        DocumentSegmentEndpoint::Point(end),
                    ) = (segment.start, segment.end)
                    else {
                        return None;
                    };
                    Some((start, end, parameter))
                });
            let Some((start, end, parameter)) = candidate else {
                break;
            };
            staged.insert_point_on_segment(start, end, parameter)?;
            changed = true;
        }
        if changed {
            *self = staged;
        }
        Ok(changed)
    }

    /// Push every canonical contour point along its anisotropic outward normal.
    ///
    /// Point order, roles, stable identities and source metadata remain unchanged. Returns whether
    /// any point moved.
    pub fn embolden(
        &mut self,
        offset: crate::outline::embolden::Offset,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&[offset.x, offset.y])?;
        if offset.x == 0.0 && offset.y == 0.0 {
            return Ok(false);
        }
        let mut replacements = HashMap::new();
        for path in self.layer.paths() {
            let positions: Vec<_> = path
                .nodes
                .iter()
                .map(|node| kurbo::Point::new(node.x, node.y))
                .collect();
            for (node, (normal_x, normal_y)) in
                path.nodes
                    .iter()
                    .zip(crate::outline::embolden::outward_normals_for_points(
                        &positions,
                    ))
            {
                let position =
                    kurbo::Point::new(node.x + normal_x * offset.x, node.y + normal_y * offset.y);
                ensure_finite(&[position.x, position.y])?;
                if position != kurbo::Point::new(node.x, node.y) {
                    replacements.insert(
                        read_id(&node.format_specific).expect("canonical point identity"),
                        position,
                    );
                }
            }
        }
        self.apply_point_replacements(&replacements)
    }

    /// Apply model-predicted integer point deltas in outline-reader order.
    ///
    /// The extra closing delta after each contour is consumed to match the model's reader. Point
    /// order, roles, stable identities and source metadata remain unchanged. Returns whether any
    /// point moved.
    pub fn apply_bolden_deltas(
        &mut self,
        deltas: &[(i32, i32)],
        center: (i32, i32),
    ) -> Result<bool, DocumentEditError> {
        let mut next = deltas.iter();
        let mut replacements = HashMap::new();
        for path in self.layer.paths() {
            let count = path.nodes.len();
            let start = path
                .nodes
                .iter()
                .position(|node| node.nodetype != NodeType::OffCurve)
                .unwrap_or(0);
            for step in 0..count {
                let Some((delta_x, delta_y)) = next.next().copied() else {
                    break;
                };
                let index = (start + step) % count;
                let node = &path.nodes[index];
                let position = kurbo::Point::new(
                    node.x + f64::from(delta_x) + f64::from(center.0),
                    node.y + f64::from(delta_y) + f64::from(center.1),
                );
                ensure_finite(&[position.x, position.y])?;
                if position != kurbo::Point::new(node.x, node.y) {
                    replacements.insert(
                        read_id(&node.format_specific).expect("canonical point identity"),
                        position,
                    );
                }
            }
            next.next();
        }
        self.apply_point_replacements(&replacements)
    }

    /// Replace every component with pre-resolved canonical contours.
    ///
    /// Existing contours and anchors retain their identities and exact metadata. Decomposed
    /// contours preserve source names and libraries while receiving fresh document and UFO
    /// identities. Returns whether components were replaced.
    pub fn decompose_components(
        &mut self,
        resolved: &[CopiedContour],
    ) -> Result<bool, DocumentEditError> {
        if self.layer.components().next().is_none() {
            return Ok(false);
        }
        self.paste_contours(resolved)?;
        self.layer
            .shapes
            .retain(|shape| matches!(shape, Shape::Path(_)));
        self.preserved.components.clear();
        Ok(true)
    }

    pub(super) fn apply_point_replacements(
        &mut self,
        replacements: &HashMap<u64, kurbo::Point>,
    ) -> Result<bool, DocumentEditError> {
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

    pub(super) fn selected_contour_ids(
        &self,
        selected: &[PointId],
    ) -> Result<HashSet<ContourId>, DocumentEditError> {
        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        if selected.is_empty() {
            return Ok(self.view().contours().map(ContourView::id).collect());
        }
        let selected: HashSet<_> = selected.iter().copied().collect();
        Ok(self
            .view()
            .contours()
            .filter(|contour| contour.points().any(|point| selected.contains(&point.id())))
            .map(ContourView::id)
            .collect())
    }

    /// Replace selected contours with stroked outlines.
    ///
    /// An empty selection targets every contour. Replaced contours receive fresh identities and
    /// empty source metadata; untargeted contours, components and anchors remain unchanged.
    pub fn expand_stroke(
        &mut self,
        selected: &[PointId],
        width: f64,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&[width])?;
        if width <= 0.0 {
            return Ok(false);
        }
        let selected = self.selected_contour_ids(selected)?;
        let replacements: HashMap<_, _> = self
            .view()
            .contours()
            .filter(|contour| selected.contains(&contour.id()))
            .filter_map(|contour| {
                let path = crate::outline::path::Path::from_document_contour(contour).to_bezpath();
                let paths = crate::outline::effects::expanded_stroke_paths(&path, width);
                (!paths.is_empty()).then_some((contour.id(), paths))
            })
            .collect();
        self.replace_selected_contours_with_paths(&replacements)
    }

    /// Offset every canonical contour outward or inward.
    ///
    /// All output contours receive fresh identities and empty source metadata. Components and
    /// anchors remain unchanged. A successful empty result removes every contour.
    pub fn offset_contours(&mut self, delta: f64) -> Result<bool, DocumentEditError> {
        ensure_finite(&[delta])?;
        let paths: Vec<_> = self
            .view()
            .contours()
            .map(|contour| crate::outline::path::Path::from_document_contour(contour).to_bezpath())
            .collect();
        let Some(paths) = crate::outline::effects::offset_paths(&paths, delta) else {
            return Ok(false);
        };
        self.replace_contours_with_paths(&paths)
    }

    /// Extrude every canonical contour along an angle.
    ///
    /// All output contours receive fresh identities and empty source metadata. Components and
    /// anchors remain unchanged. A successful empty result removes every contour.
    pub fn extrude_contours(
        &mut self,
        offset: f64,
        angle_degrees: f64,
        keep_front: bool,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&[offset, angle_degrees])?;
        let paths: Vec<_> = self
            .view()
            .contours()
            .map(|contour| crate::outline::path::Path::from_document_contour(contour).to_bezpath())
            .collect();
        let Some(paths) =
            crate::outline::effects::extruded_paths(&paths, offset, angle_degrees, keep_front)
        else {
            return Ok(false);
        };
        self.replace_contours_with_paths(&paths)
    }

    /// Flatten and jitter selected canonical contours deterministically.
    ///
    /// An empty selection targets every contour. Replaced contours receive fresh identities and
    /// empty source metadata; untargeted contours, components and anchors remain unchanged.
    pub fn roughen_contours(
        &mut self,
        selected: &[PointId],
        segment_length: f64,
        horizontal: f64,
        vertical: f64,
        seed: u64,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&[segment_length, horizontal, vertical])?;
        if segment_length < 1.0 {
            return Ok(false);
        }
        let selected = self.selected_contour_ids(selected)?;
        let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut replacements = HashMap::new();
        for contour in self.view().contours() {
            if !selected.contains(&contour.id()) {
                continue;
            }
            let path = crate::outline::path::Path::from_document_contour(contour).to_bezpath();
            if let Some(path) = crate::outline::effects::roughened_path(
                &path,
                segment_length,
                horizontal,
                vertical,
                &mut state,
            ) {
                replacements.insert(contour.id(), vec![path]);
            }
        }
        self.replace_selected_contours_with_paths(&replacements)
    }

    /// Apply a boolean operation to canonical contours and replace their topology.
    ///
    /// Union combines every contour. Other operations use the first contour as the left operand
    /// and the remaining contours as the right operand. Replacement contours receive fresh stable
    /// identities and empty source metadata. Returns whether replacement succeeded.
    pub fn boolean_contours(
        &mut self,
        operation: linesweeper::BinaryOp,
    ) -> Result<bool, DocumentEditError> {
        let paths: Vec<_> = self
            .view()
            .contours()
            .map(crate::outline::glyph_paths::ordinary_contour_to_bezpath)
            .collect();
        if paths.len() < 2 {
            return Ok(false);
        }
        let (left, right) = if operation == linesweeper::BinaryOp::Union {
            let mut combined = kurbo::BezPath::new();
            for path in &paths {
                combined.extend(path.elements().iter().copied());
            }
            (combined, kurbo::BezPath::new())
        } else {
            let mut paths = paths.into_iter();
            let left = paths
                .next()
                .expect("boolean input has at least two contours");
            let mut right = kurbo::BezPath::new();
            for path in paths {
                right.extend(path.elements().iter().copied());
            }
            (left, right)
        };
        let Ok(result) =
            linesweeper::binary_op(&left, &right, linesweeper::FillRule::NonZero, operation)
        else {
            return Ok(false);
        };
        let paths: Vec<_> = result
            .contours()
            .map(|contour| contour.path.clone())
            .collect();
        self.replace_contours_with_paths(&paths)
    }

    /// Union every canonical contour and replace their topology.
    ///
    /// Replacement contours receive fresh stable identities and empty source metadata. Returns
    /// whether overlap removal succeeded.
    pub fn remove_overlap(&mut self) -> Result<bool, DocumentEditError> {
        let combined = crate::outline::glyph_paths::ordinary_layer_contours_to_bezpath(self.view());
        if combined.is_empty() {
            return Ok(false);
        }
        let Ok(result) = linesweeper::binary_op(
            &combined,
            &kurbo::BezPath::new(),
            linesweeper::FillRule::NonZero,
            linesweeper::BinaryOp::Union,
        ) else {
            return Ok(false);
        };
        let paths: Vec<_> = result
            .contours()
            .map(|contour| contour.path.clone())
            .collect();
        self.replace_contours_with_paths(&paths)
    }

    /// Permanently subtract contours marked as masks from the other canonical contours.
    ///
    /// Mask indices are decoded only at this explicit UFO-key boundary. Successful replacement
    /// clears the key and assigns fresh identities and empty source metadata to the result.
    pub fn bake_masks(&mut self) -> Result<bool, DocumentEditError> {
        let Some(values) = self
            .preserved
            .lib
            .get(crate::formats::metadata::lib_keys::MASKS_KEY)
            .and_then(plist::Value::as_array)
        else {
            return Ok(false);
        };
        let contour_count = self.view().contours().count();
        let masks: HashSet<_> = values
            .iter()
            .filter_map(plist::Value::as_unsigned_integer)
            .filter_map(|value| usize::try_from(value).ok())
            .filter(|index| *index < contour_count)
            .collect();
        if masks.is_empty() || masks.len() == contour_count {
            return Ok(false);
        }
        let mut keep = kurbo::BezPath::new();
        let mut cut = kurbo::BezPath::new();
        for (index, contour) in self.view().contours().enumerate() {
            let path = crate::outline::path::Path::from_document_contour(contour).to_bezpath();
            let destination = if masks.contains(&index) {
                &mut cut
            } else {
                &mut keep
            };
            destination.extend(path.elements().iter().copied());
        }
        let Ok(result) = linesweeper::binary_op(
            &keep,
            &cut,
            linesweeper::FillRule::NonZero,
            linesweeper::BinaryOp::Difference,
        ) else {
            return Ok(false);
        };
        let paths = result
            .contours()
            .map(|contour| contour.path.clone())
            .collect::<Vec<_>>();
        let changed = self.replace_contours_with_paths(&paths)?;
        if changed {
            self.preserved
                .lib
                .remove(crate::formats::metadata::lib_keys::MASKS_KEY);
        }
        Ok(changed)
    }

    /// Cut canonical contours along the line from `p0` to `p1`.
    ///
    /// Missed contours retain their stable identities and exact source metadata. Every contour
    /// whose topology changes receives fresh identities and empty source metadata. A sliced
    /// hyperbezier becomes explicit cubic geometry, matching the existing knife behavior.
    /// Returns whether any topology changed.
    pub fn knife_cut(
        &mut self,
        p0: kurbo::Point,
        p1: kurbo::Point,
    ) -> Result<bool, DocumentEditError> {
        let originals: Vec<_> = self
            .view()
            .contours()
            .map(|contour| {
                let path = crate::outline::path::Path::from_document_contour(contour);
                (path.entity_id(), contour.id(), path)
            })
            .collect();
        if originals.is_empty() {
            return Ok(false);
        }
        let input: Vec<_> = originals.iter().map(|(_, _, path)| path.clone()).collect();
        let sliced = crate::outline::knife::slice_paths(&input, kurbo::Line::new(p0, p1));
        if sliced.len() == input.len()
            && sliced.iter().all(|path| {
                originals
                    .iter()
                    .any(|(entity_id, _, _)| *entity_id == path.entity_id())
            })
        {
            return Ok(false);
        }
        self.replace_contours_after_knife(&sliced, &originals)
    }

    /// Convert selected editable hyperbezier contours to explicit cubic topology.
    ///
    /// An empty selection converts every hyperbezier contour. Converted topology receives fresh
    /// identities and empty source metadata, while ordinary and unselected contours remain exact.
    pub fn convert_hyper_to_cubic(
        &mut self,
        selected: &[PointId],
    ) -> Result<bool, DocumentEditError> {
        for id in selected {
            if !self.layer.paths().flat_map(|path| &path.nodes).any(|node| {
                read_id(&node.format_specific).is_some_and(|candidate| candidate == id.0)
            }) {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let selected: HashSet<_> = selected.iter().copied().collect();
        let convert_all = selected.is_empty();
        let replacement_paths = self
            .view()
            .contours()
            .filter(|contour| {
                contour.is_hyper()
                    && (convert_all || contour.points().any(|point| selected.contains(&point.id())))
            })
            .map(|contour| {
                let path = crate::outline::path::Path::from_document_contour(contour);
                let crate::outline::path::Path::Hyper(hyper) = path else {
                    unreachable!("canonical hyperbezier contour produces a hyper path")
                };
                (
                    contour.id(),
                    crate::outline::path::Path::Cubic(hyper.to_cubic()),
                )
            })
            .collect::<Vec<_>>();
        if replacement_paths.is_empty() {
            return Ok(false);
        }
        let replacements = replacement_paths
            .iter()
            .map(|(id, path)| Ok((*id, Self::replacement_contour_from_outline_path(path)?)))
            .collect::<Result<HashMap<_, _>, DocumentEditError>>()?;
        let mut shapes = Vec::with_capacity(self.layer.shapes.len());
        let mut preserved = Vec::with_capacity(self.preserved.contours.len());
        for shape in &self.layer.shapes {
            let Shape::Path(path) = shape else {
                shapes.push(shape.clone());
                continue;
            };
            let contour_id =
                ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
            if let Some((replacement, metadata)) = replacements.get(&contour_id) {
                shapes.push(replacement.clone());
                preserved.push(metadata.clone());
            } else {
                shapes.push(shape.clone());
                preserved.push(
                    self.preserved
                        .contours
                        .iter()
                        .find(|candidate| candidate.id == contour_id)
                        .expect("canonical contour preservation")
                        .clone(),
                );
            }
        }
        self.layer.shapes = shapes;
        self.preserved.contours = preserved;
        Ok(true)
    }
}
