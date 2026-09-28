// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Rebuild canonical contours from replacement outline paths.

use super::*;

impl LayerEditDraft {
    pub(super) fn replacement_contour_from_outline_path(
        path: &crate::outline::path::Path,
    ) -> Result<(Shape, PreservedContour), DocumentEditError> {
        let contour = path.to_contour();
        ensure_finite(
            &contour
                .points
                .iter()
                .flat_map(|point| [point.x, point.y])
                .collect::<Vec<_>>(),
        )?;
        let contour_id = ContourId::next();
        let mut output = babelfont::Path {
            closed: path.is_closed(),
            ..babelfont::Path::default()
        };
        write_id(&mut output.format_specific, contour_id.0);
        let mut points = Vec::with_capacity(contour.points.len());
        output.nodes = contour
            .points
            .iter()
            .map(|point| {
                let point_id = PointId::next();
                points.push(PreservedPoint {
                    id: point_id,
                    name: None,
                    metadata: ObjectMetadata {
                        identifier: None,
                        lib: None,
                    },
                });
                let mut node = Node {
                    x: point.x,
                    y: point.y,
                    nodetype: match point.point_type {
                        crate::outline::path::hyper_model::PointType::Move => NodeType::Move,
                        crate::outline::path::hyper_model::PointType::Line
                        | crate::outline::path::hyper_model::PointType::HyperCorner => {
                            NodeType::Line
                        }
                        crate::outline::path::hyper_model::PointType::OffCurve => {
                            NodeType::OffCurve
                        }
                        crate::outline::path::hyper_model::PointType::Curve
                        | crate::outline::path::hyper_model::PointType::Hyper => NodeType::Curve,
                        crate::outline::path::hyper_model::PointType::QCurve => NodeType::QCurve,
                    },
                    smooth: point.smooth,
                    ..Node::default()
                };
                write_id(&mut node.format_specific, point_id.0);
                node
            })
            .collect();
        Ok((
            Shape::Path(output),
            PreservedContour {
                id: contour_id,
                hyper: false,
                metadata: ObjectMetadata {
                    identifier: None,
                    lib: None,
                },
                points,
            },
        ))
    }

    pub(super) fn replace_contours_after_knife(
        &mut self,
        paths: &[crate::outline::path::Path],
        originals: &[(
            crate::font::model::entity_id::EntityId,
            ContourId,
            crate::outline::path::Path,
        )],
    ) -> Result<bool, DocumentEditError> {
        let mut replacements = Vec::with_capacity(paths.len());
        let mut preserved = Vec::with_capacity(paths.len());
        for path in paths {
            if let Some((_, contour_id, _)) = originals
                .iter()
                .find(|(entity_id, _, _)| *entity_id == path.entity_id())
            {
                let source_index = self
                    .preserved
                    .contours
                    .iter()
                    .position(|candidate| candidate.id == *contour_id)
                    .expect("knife source contour preservation");
                let source_path = self
                    .layer
                    .shapes
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Path(path) => Some(path),
                        Shape::Component(_) => None,
                    })
                    .find(|path| read_id(&path.format_specific) == Some(contour_id.0))
                    .expect("knife source canonical contour");
                replacements.push(Shape::Path(source_path.clone()));
                preserved.push(self.preserved.contours[source_index].clone());
                continue;
            }

            let (shape, metadata) = Self::replacement_contour_from_outline_path(path)?;
            replacements.push(shape);
            preserved.push(metadata);
        }

        let insert_at = self
            .layer
            .shapes
            .iter()
            .take_while(|shape| !matches!(shape, Shape::Path(_)))
            .filter(|shape| matches!(shape, Shape::Component(_)))
            .count();
        let mut shapes: Vec<_> = self
            .layer
            .shapes
            .iter()
            .filter(|shape| matches!(shape, Shape::Component(_)))
            .cloned()
            .collect();
        shapes.splice(insert_at..insert_at, replacements);
        self.layer.shapes = shapes;
        self.preserved.contours = preserved;
        Ok(true)
    }

    pub(super) fn replacement_contour_from_path(
        path: &kurbo::BezPath,
        smooth_at: &HashMap<(i64, i64), bool>,
    ) -> Result<Option<(Shape, PreservedContour)>, DocumentEditError> {
        let mut path = babelfont::Path::from(path.clone());
        ensure_finite(
            &path
                .nodes
                .iter()
                .flat_map(|node| [node.x, node.y])
                .collect::<Vec<_>>(),
        )?;
        let on_curve_count = path
            .nodes
            .iter()
            .filter(|node| node.nodetype != NodeType::OffCurve)
            .count();
        if on_curve_count < if path.closed { 1 } else { 2 } {
            return Ok(None);
        }
        let contour_id = ContourId::next();
        write_id(&mut path.format_specific, contour_id.0);
        let points = path
            .nodes
            .iter_mut()
            .map(|node| {
                if node.nodetype != NodeType::OffCurve {
                    node.smooth = smooth_at
                        .get(&crate::outline::glyph_paths::point_key(node.x, node.y))
                        .copied()
                        .unwrap_or(false);
                }
                let point_id = PointId::next();
                write_id(&mut node.format_specific, point_id.0);
                PreservedPoint {
                    id: point_id,
                    name: None,
                    metadata: ObjectMetadata {
                        identifier: None,
                        lib: None,
                    },
                }
            })
            .collect();
        Ok(Some((
            Shape::Path(path),
            PreservedContour {
                id: contour_id,
                hyper: false,
                metadata: ObjectMetadata {
                    identifier: None,
                    lib: None,
                },
                points,
            },
        )))
    }

    pub(super) fn replace_selected_contours_with_paths(
        &mut self,
        replacements: &HashMap<ContourId, Vec<kurbo::BezPath>>,
    ) -> Result<bool, DocumentEditError> {
        if replacements.is_empty() {
            return Ok(false);
        }
        let smooth_at: HashMap<_, _> = self
            .layer
            .paths()
            .flat_map(|path| &path.nodes)
            .filter(|node| node.nodetype != NodeType::OffCurve)
            .map(|node| {
                (
                    crate::outline::glyph_paths::point_key(node.x, node.y),
                    node.smooth,
                )
            })
            .collect();
        let mut shapes = Vec::new();
        let mut preserved = Vec::new();
        for shape in &self.layer.shapes {
            let Shape::Path(path) = shape else {
                shapes.push(shape.clone());
                continue;
            };
            let contour_id =
                ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
            let Some(paths) = replacements.get(&contour_id) else {
                shapes.push(shape.clone());
                preserved.push(
                    self.preserved
                        .contours
                        .iter()
                        .find(|candidate| candidate.id == contour_id)
                        .expect("canonical contour preservation")
                        .clone(),
                );
                continue;
            };
            for path in paths {
                if let Some((shape, metadata)) =
                    Self::replacement_contour_from_path(path, &smooth_at)?
                {
                    shapes.push(shape);
                    preserved.push(metadata);
                }
            }
        }
        self.layer.shapes = shapes;
        self.preserved.contours = preserved;
        Ok(true)
    }

    pub(super) fn replace_contours_with_paths(
        &mut self,
        paths: &[kurbo::BezPath],
    ) -> Result<bool, DocumentEditError> {
        let had_contours = self.layer.paths().next().is_some();
        let smooth_at: HashMap<_, _> = self
            .layer
            .paths()
            .flat_map(|path| &path.nodes)
            .filter(|node| node.nodetype != NodeType::OffCurve)
            .map(|node| {
                (
                    crate::outline::glyph_paths::point_key(node.x, node.y),
                    node.smooth,
                )
            })
            .collect();
        let mut replacements = Vec::with_capacity(paths.len());
        let mut preserved = Vec::with_capacity(paths.len());
        for path in paths {
            if let Some((shape, metadata)) = Self::replacement_contour_from_path(path, &smooth_at)?
            {
                replacements.push(shape);
                preserved.push(metadata);
            }
        }
        let changed = had_contours || !replacements.is_empty();
        let insert_at = self
            .layer
            .shapes
            .iter()
            .take_while(|shape| !matches!(shape, Shape::Path(_)))
            .filter(|shape| matches!(shape, Shape::Component(_)))
            .count();
        let mut shapes: Vec<_> = self
            .layer
            .shapes
            .iter()
            .filter(|shape| matches!(shape, Shape::Component(_)))
            .cloned()
            .collect();
        shapes.splice(insert_at..insert_at, replacements);
        self.layer.shapes = shapes;
        self.preserved.contours = preserved;
        Ok(changed)
    }

    /// Replace only the contours and exact width from another canonical layer.
    pub(in crate::font) fn replace_layer_contours(
        &mut self,
        source: LayerView<'_>,
    ) -> Result<bool, DocumentEditError> {
        let contours_changed = self.replace_layer_contours_only(source)?;
        let width_changed = self.set_width(source.width())?;
        Ok(contours_changed || width_changed)
    }

    /// Replace only the contours from another canonical layer, retaining this layer's width.
    pub(in crate::font) fn replace_layer_contours_only(
        &mut self,
        source: LayerView<'_>,
    ) -> Result<bool, DocumentEditError> {
        if source.glyph_name() != self.preserved.name {
            return Err(DocumentEditError::InvalidLayerMetadata);
        }
        let current = self.view();
        let same_contours = current.contours().count() == source.contours().count()
            && current.contours().zip(source.contours()).all(|(a, b)| {
                a.is_closed() == b.is_closed()
                    && a.is_hyper() == b.is_hyper()
                    && same_copied_object_metadata(&a.preserved.metadata, &b.preserved.metadata)
                    && a.points().count() == b.points().count()
                    && a.points().zip(b.points()).all(|(a, b)| {
                        a.position() == b.position()
                            && a.point_type() == b.point_type()
                            && a.is_smooth() == b.is_smooth()
                            && a.name() == b.name()
                            && same_copied_object_metadata(
                                &a.preserved.metadata,
                                &b.preserved.metadata,
                            )
                    })
            });
        if same_contours {
            return Ok(false);
        }
        let copied = source.copy_contours(&[])?;
        let old_shape_count = self.layer.shapes.len();
        let old_contour_count = self.preserved.contours.len();
        self.paste_contours(&copied)?;
        let replacements = self.layer.shapes.split_off(old_shape_count);
        replace_path_shapes_preserving_slots(&mut self.layer.shapes, replacements);
        let replacements = self.preserved.contours.split_off(old_contour_count);
        self.preserved.contours = replacements;
        Ok(true)
    }

    /// Replace only the contours with one checked canonical interpolation result.
    ///
    /// The replacement receives fresh contour and point identities because its topology comes
    /// from other source layers. Components, anchors and every non-contour layer field remain
    /// untouched.
    pub(in crate::font) fn replace_interpolated_contours(
        &mut self,
        output: &super::super::interpolation::InterpolatedLayer,
    ) -> Result<bool, DocumentEditError> {
        if output.glyph_name != self.preserved.name {
            return Err(DocumentEditError::InvalidLayerMetadata);
        }
        let current = self.view();
        let same_contours = current.contours().count() == output.contours().count()
            && current
                .contours()
                .zip(output.contours())
                .all(|(source, replacement)| {
                    source.is_closed() == replacement.closed
                        && source.is_hyper() == replacement.hyper
                        && source.points().count() == replacement.points.len()
                        && source.points().zip(&replacement.points).all(|(a, b)| {
                            a.position() == b.position
                                && a.point_type() == b.point_type
                                && a.is_smooth() == b.smooth
                                && a.name() == b.name.as_deref()
                        })
                });
        if same_contours && self.preserved.width == output.width {
            return Ok(false);
        }
        let mut replacements = Vec::with_capacity(output.contours().count());
        let mut preserved = Vec::with_capacity(output.contours().count());
        for contour in output.contours() {
            let contour_id = ContourId::next();
            let mut path = babelfont::Path {
                closed: contour.closed,
                ..babelfont::Path::default()
            };
            write_id(&mut path.format_specific, contour_id.0);
            let mut points = Vec::with_capacity(contour.points.len());
            for point in &contour.points {
                ensure_finite(&[point.position.x, point.position.y])?;
                let point_id = PointId::next();
                let mut node = Node {
                    x: point.position.x,
                    y: point.position.y,
                    nodetype: match point.point_type {
                        LayerPointType::Move => NodeType::Move,
                        LayerPointType::Line => NodeType::Line,
                        LayerPointType::OffCurve => NodeType::OffCurve,
                        LayerPointType::Curve => NodeType::Curve,
                        LayerPointType::QCurve => NodeType::QCurve,
                    },
                    smooth: point.smooth,
                    ..Node::default()
                };
                write_id(&mut node.format_specific, point_id.0);
                path.nodes.push(node);
                points.push(PreservedPoint {
                    id: point_id,
                    name: point
                        .name
                        .as_deref()
                        .and_then(|name| norad::Name::new(name).ok()),
                    metadata: ObjectMetadata {
                        identifier: None,
                        lib: None,
                    },
                });
            }
            replacements.push(Shape::Path(path));
            preserved.push(PreservedContour {
                id: contour_id,
                hyper: contour.hyper,
                metadata: ObjectMetadata {
                    identifier: contour.hyper.then(fresh_hyper_identifier),
                    lib: None,
                },
                points,
            });
        }
        let insert_at = self
            .layer
            .shapes
            .iter()
            .take_while(|shape| !matches!(shape, Shape::Path(_)))
            .filter(|shape| matches!(shape, Shape::Component(_)))
            .count();
        let mut shapes: Vec<_> = self
            .layer
            .shapes
            .iter()
            .filter(|shape| matches!(shape, Shape::Component(_)))
            .cloned()
            .collect();
        shapes.splice(insert_at..insert_at, replacements);
        self.layer.shapes = shapes;
        self.preserved.contours = preserved;
        self.set_width(output.width)?;
        Ok(true)
    }
}
