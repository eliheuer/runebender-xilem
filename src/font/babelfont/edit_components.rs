// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Component, anchor, image, mark, and layer metadata edits.

use super::*;

impl LayerEditDraft {
    /// Replace one component's referenced glyph by stable identity.
    pub fn set_component_reference(
        &mut self,
        id: ComponentId,
        reference: &str,
    ) -> Result<bool, DocumentEditError> {
        norad::Name::new(reference).map_err(|_| DocumentEditError::InvalidLayerMetadata)?;
        let component = self
            .layer
            .shapes
            .iter_mut()
            .find_map(|shape| match shape {
                Shape::Component(component)
                    if read_id(&component.format_specific) == Some(id.0) =>
                {
                    Some(component)
                }
                Shape::Path(_) | Shape::Component(_) => None,
            })
            .ok_or(DocumentEditError::MissingComponent(id))?;
        if component.reference.as_str() == reference {
            return Ok(false);
        }
        component.reference = reference.into();
        Ok(true)
    }

    /// Set one component's exact affine transform by stable identity.
    ///
    /// Returns whether the value changed.
    pub fn set_component_transform(
        &mut self,
        id: ComponentId,
        transform: kurbo::Affine,
    ) -> Result<bool, DocumentEditError> {
        let coefficients = transform.as_coeffs();
        ensure_finite(&coefficients)?;
        let preserved = self
            .preserved
            .components
            .iter_mut()
            .find(|candidate| candidate.id == id)
            .ok_or(DocumentEditError::MissingComponent(id))?;
        let exact = norad::AffineTransform {
            x_scale: coefficients[0],
            xy_scale: coefficients[1],
            yx_scale: coefficients[2],
            y_scale: coefficients[3],
            x_offset: coefficients[4],
            y_offset: coefficients[5],
        };
        if preserved.transform == exact {
            return Ok(false);
        }
        let component = self
            .layer
            .shapes
            .iter_mut()
            .find_map(|shape| match shape {
                Shape::Component(component)
                    if read_id(&component.format_specific) == Some(id.0) =>
                {
                    Some(component)
                }
                Shape::Path(_) | Shape::Component(_) => None,
            })
            .expect("preserved component has canonical geometry");
        preserved.transform = exact;
        component.transform = transform.into();
        Ok(true)
    }

    /// Append a component with a fresh stable identity and exact affine transform.
    ///
    /// The Project or application boundary is responsible for validating that `reference` names
    /// a glyph in the current document.
    pub fn add_component(
        &mut self,
        reference: String,
        transform: kurbo::Affine,
    ) -> Result<ComponentId, DocumentEditError> {
        let coefficients = transform.as_coeffs();
        ensure_finite(&coefficients)?;
        let id = ComponentId::next();
        let mut component = Component {
            reference: reference.into(),
            transform: transform.into(),
            location: std::iter::empty().collect(),
            format_specific: babelfont::FormatSpecific::default(),
        };
        write_id(&mut component.format_specific, id.0);
        self.layer.shapes.push(Shape::Component(component));
        self.preserved.components.push(PreservedComponent {
            id,
            transform: norad::AffineTransform {
                x_scale: coefficients[0],
                xy_scale: coefficients[1],
                yx_scale: coefficients[2],
                y_scale: coefficients[3],
                x_offset: coefficients[4],
                y_offset: coefficients[5],
            },
            alignment: ComponentAlignment::default(),
            metadata: ObjectMetadata {
                identifier: None,
                lib: None,
            },
        });
        Ok(id)
    }

    /// Read whether one stable component is cut loose from automatic anchor alignment.
    pub fn component_alignment_disabled(&self, id: ComponentId) -> Result<bool, DocumentEditError> {
        self.preserved
            .components
            .iter()
            .find(|component| component.id == id)
            .map(|component| component.alignment.is_disabled())
            .ok_or(DocumentEditError::MissingComponent(id))
    }

    /// Change automatic anchor alignment for one stable component.
    pub fn set_component_alignment_disabled(
        &mut self,
        id: ComponentId,
        disabled: bool,
    ) -> Result<bool, DocumentEditError> {
        let component = self
            .preserved
            .components
            .iter_mut()
            .find(|component| component.id == id)
            .ok_or(DocumentEditError::MissingComponent(id))?;
        let changed = component.alignment.set_disabled(disabled);
        if changed && component.metadata.identifier.is_none() {
            component.metadata.identifier = Some(norad::Identifier::from_uuidv4());
        }
        Ok(changed)
    }

    /// Set or remove one smart-axis value bound to a stable component identity.
    pub fn set_smart_component_value(
        &mut self,
        id: ComponentId,
        axis: &str,
        value: Option<f64>,
    ) -> Result<bool, DocumentEditError> {
        if !self
            .preserved
            .components
            .iter()
            .any(|component| component.id == id)
        {
            return Err(DocumentEditError::MissingComponent(id));
        }
        let Some(value) = value else {
            return Ok(self
                .preserved
                .smart_component_values
                .as_mut()
                .and_then(|values| values.get_mut(id))
                .is_some_and(|entry| entry.remove_value(axis)));
        };
        let values = self
            .preserved
            .smart_component_values
            .get_or_insert_with(SmartComponentValues::default);
        values.insert_component(id);
        values
            .get_mut(id)
            .expect("inserted smart-component entry")
            .set_value(axis, value)
            .map_err(|_| DocumentEditError::InvalidLayerMetadata)
    }

    /// Remove one component by stable identity.
    pub fn remove_component(&mut self, id: ComponentId) -> Result<bool, DocumentEditError> {
        let shape_index = self
            .layer
            .shapes
            .iter()
            .position(|shape| match shape {
                Shape::Component(component) => read_id(&component.format_specific) == Some(id.0),
                Shape::Path(_) => false,
            })
            .ok_or(DocumentEditError::MissingComponent(id))?;
        let preserved_index = self
            .preserved
            .components
            .iter()
            .position(|component| component.id == id)
            .expect("canonical component has preservation metadata");
        self.layer.shapes.remove(shape_index);
        self.preserved.components.remove(preserved_index);
        if let Some(values) = &mut self.preserved.smart_component_values {
            values.remove_component(id);
        }
        Ok(true)
    }

    /// Set one anchor's position by stable identity.
    ///
    /// Returns whether the value changed.
    pub fn set_anchor_position(
        &mut self,
        id: AnchorId,
        position: kurbo::Point,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&[position.x, position.y])?;
        let anchor = self
            .layer
            .anchors
            .iter_mut()
            .find(|candidate| read_id(&candidate.format_specific) == Some(id.0))
            .ok_or(DocumentEditError::MissingAnchor(id))?;
        if anchor.x == position.x && anchor.y == position.y {
            return Ok(false);
        }
        anchor.x = position.x;
        anchor.y = position.y;
        Ok(true)
    }

    /// Append an anchor with a fresh stable identity.
    pub fn add_anchor(
        &mut self,
        name: String,
        position: kurbo::Point,
    ) -> Result<AnchorId, DocumentEditError> {
        ensure_finite(&[position.x, position.y])?;
        let id = AnchorId::next();
        let mut anchor = Anchor {
            x: position.x,
            y: position.y,
            name,
            ..Anchor::default()
        };
        write_id(&mut anchor.format_specific, id.0);
        self.layer.anchors.push(anchor);
        self.preserved.anchors.push(PreservedAnchor {
            id,
            color: None,
            metadata: ObjectMetadata {
                identifier: None,
                lib: None,
            },
        });
        Ok(id)
    }

    /// Remove one anchor by stable identity.
    pub fn remove_anchor(&mut self, id: AnchorId) -> Result<bool, DocumentEditError> {
        let anchor_index = self
            .layer
            .anchors
            .iter()
            .position(|anchor| read_id(&anchor.format_specific) == Some(id.0))
            .ok_or(DocumentEditError::MissingAnchor(id))?;
        let preserved_index = self
            .preserved
            .anchors
            .iter()
            .position(|anchor| anchor.id == id)
            .expect("canonical anchor has preservation metadata");
        self.layer.anchors.remove(anchor_index);
        self.preserved.anchors.remove(preserved_index);
        Ok(true)
    }

    /// Set or remove the source image attached to this layer.
    pub fn set_image(&mut self, image: Option<LayerImage>) -> bool {
        if self.preserved.image == image {
            return false;
        }
        self.preserved.image = image;
        true
    }

    /// Replace or remove the typed public mark color.
    pub fn set_mark_color(&mut self, color: Option<MarkColor>) -> Result<bool, DocumentEditError> {
        if color.is_some_and(|color| !color.is_valid()) {
            return Err(DocumentEditError::InvalidLayerMetadata);
        }
        if parse_mark_color(self.preserved.mark_color.as_ref()).ok() == Some(color) {
            return Ok(false);
        }
        self.preserved.mark_color = color.map(|color| {
            plist::Value::String(format!(
                "{},{},{},{}",
                color.red, color.green, color.blue, color.alpha
            ))
        });
        Ok(true)
    }

    /// Set or clear one semantic glyph mark atomically.
    ///
    /// A mark requires both its stable label and its typed public color.
    /// Clearing removes both UFO keys, while an equivalent edit retains the exact source spelling
    /// of the existing valid color.
    pub fn set_mark(
        &mut self,
        label: Option<&str>,
        color: Option<MarkColor>,
    ) -> Result<bool, DocumentEditError> {
        if label.is_some_and(str::is_empty)
            || color.is_some_and(|color| !color.is_valid())
            || label.is_some() != color.is_some()
        {
            return Err(DocumentEditError::InvalidLayerMetadata);
        }
        let Some((label, color)) = label.zip(color) else {
            let color_changed = self.preserved.mark_color.take().is_some();
            let label_changed = self.preserved.lib.remove(MARK_LABEL_KEY).is_some();
            return Ok(color_changed || label_changed);
        };
        let color_changed = parse_mark_color(self.preserved.mark_color.as_ref()) != Ok(Some(color));
        let label_value = plist::Value::String(label.to_owned());
        let label_changed = self.preserved.lib.get(MARK_LABEL_KEY) != Some(&label_value);
        if color_changed {
            self.preserved.mark_color = Some(plist::Value::String(format!(
                "{},{},{},{}",
                color.red, color.green, color.blue, color.alpha
            )));
        }
        if label_changed {
            self.preserved
                .lib
                .insert(MARK_LABEL_KEY.into(), label_value);
        }
        Ok(color_changed || label_changed)
    }

    /// Replace or remove one exact left or right metrics-key source string.
    pub fn set_metrics_key(
        &mut self,
        left: bool,
        source: Option<String>,
    ) -> Result<bool, DocumentEditError> {
        if source
            .as_deref()
            .is_some_and(|source| parse_metrics_key(source).is_none())
        {
            return Err(DocumentEditError::InvalidLayerMetadata);
        }
        let target = if left {
            &mut self.preserved.left_metrics_key
        } else {
            &mut self.preserved.right_metrics_key
        };
        let replacement = source.map(plist::Value::String);
        if *target == replacement {
            return Ok(false);
        }
        *target = replacement;
        Ok(true)
    }

    /// Replace validated editable metaball data, removing the key for an empty value.
    pub fn set_metaballs(&mut self, metaballs: Metaballs) -> Result<bool, DocumentEditError> {
        metaballs
            .validate()
            .map_err(|_| DocumentEditError::InvalidLayerMetadata)?;
        if parse_metaballs(self.preserved.metaballs.as_ref()).ok() == Some(metaballs.clone()) {
            return Ok(false);
        }
        self.preserved.metaballs = if metaballs.groups.is_empty() {
            None
        } else {
            Some(plist::to_value(&metaballs).map_err(|_| DocumentEditError::InvalidLayerMetadata)?)
        };
        Ok(true)
    }

    /// Convert selected live metaball groups to explicit cubic contours.
    ///
    /// `None` converts every group and an empty slice converts none. All sampling and curve
    /// fitting complete on a staged draft, so an invalid group or empty sampled outline leaves
    /// geometry and live source data unchanged. Generated topology receives fresh identities and
    /// empty source metadata while existing contours, components and anchors remain exact.
    pub fn collapse_metaballs(
        &mut self,
        groups: Option<&[u32]>,
        options: crate::outline::metaballs::OutlineOptions,
    ) -> Result<usize, String> {
        self.collapse_metaballs_with_kind(groups, options, MetaballCollapseKind::Cubic)
    }

    /// Convert selected live metaball groups to editable hyperbezier contours.
    ///
    /// The generated spline keeps the fitted metaball outline's on-curve points and derives its
    /// handles through the hyperbezier solver.
    pub fn collapse_metaballs_to_hyperbezier(
        &mut self,
        groups: Option<&[u32]>,
        options: crate::outline::metaballs::OutlineOptions,
    ) -> Result<usize, String> {
        self.collapse_metaballs_with_kind(groups, options, MetaballCollapseKind::Hyperbezier)
    }

    pub(super) fn collapse_metaballs_with_kind(
        &mut self,
        groups: Option<&[u32]>,
        options: crate::outline::metaballs::OutlineOptions,
        kind: MetaballCollapseKind,
    ) -> Result<usize, String> {
        let mut data = self.view().metaballs().map_err(|error| error.to_string())?;
        let selected: HashSet<_> = groups
            .map(|groups| groups.iter().copied().collect())
            .unwrap_or_else(|| data.groups.iter().map(|group| group.id).collect());
        if selected
            .iter()
            .any(|id| !data.groups.iter().any(|group| group.id == *id))
        {
            return Err("unknown metaball group".into());
        }
        if selected.is_empty() {
            return Ok(0);
        }
        let mut generated = Vec::new();
        for group in data
            .groups
            .iter()
            .filter(|group| selected.contains(&group.id))
        {
            let paths = crate::outline::metaballs::cubic_outline(group, options)?;
            if paths.is_empty() {
                return Err("metaball group has no sampled outline; source preserved".into());
            }
            for path in paths {
                let smooth_at = path
                    .segments()
                    .map(|segment| {
                        (
                            crate::outline::glyph_paths::point_key(
                                segment.end().x,
                                segment.end().y,
                            ),
                            true,
                        )
                    })
                    .collect();
                let mut converted = Self::replacement_contour_from_path(&path, &smooth_at)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "metaball outline did not produce a contour".to_owned())?;
                // Babelfont rotates closed cubics to begin with off-curves on import.
                // Restore img2bez's bottom start, keeping point metadata in the same order.
                if let (Some(start), Shape::Path(contour)) =
                    (path.segments().next().map(|s| s.start()), &mut converted.0)
                    && let Some(index) = contour.nodes.iter().position(|node| {
                        node.nodetype != NodeType::OffCurve
                            && node.x == start.x
                            && node.y == start.y
                    })
                {
                    contour.nodes.rotate_left(index);
                    converted.1.points.rotate_left(index);
                }
                if kind == MetaballCollapseKind::Hyperbezier {
                    Self::convert_metaball_contour_to_hyperbezier(&mut converted)?;
                }
                generated.push(converted);
            }
        }
        data.groups.retain(|group| !selected.contains(&group.id));
        let mut staged = self.clone();
        let insert_at = staged
            .layer
            .shapes
            .iter()
            .rposition(|shape| matches!(shape, Shape::Path(_)))
            .map_or_else(
                || {
                    staged
                        .layer
                        .shapes
                        .iter()
                        .position(|shape| matches!(shape, Shape::Component(_)))
                        .unwrap_or(staged.layer.shapes.len())
                },
                |index| index + 1,
            );
        staged.layer.shapes.splice(
            insert_at..insert_at,
            generated.iter().map(|item| item.0.clone()),
        );
        staged
            .preserved
            .contours
            .extend(generated.into_iter().map(|item| item.1));
        staged
            .set_metaballs(data)
            .map_err(|error| error.to_string())?;
        *self = staged;
        Ok(selected.len())
    }

    pub(super) fn convert_metaball_contour_to_hyperbezier(
        converted: &mut (Shape, PreservedContour),
    ) -> Result<(), String> {
        let Shape::Path(path) = &mut converted.0 else {
            return Err("metaball outline did not produce a path".into());
        };
        let mut nodes = Vec::new();
        let mut points = Vec::new();
        for (node, point) in path.nodes.drain(..).zip(converted.1.points.drain(..)) {
            if node.nodetype != NodeType::OffCurve {
                nodes.push(node);
                points.push(point);
            }
        }
        if nodes.len() < 3 {
            return Err("metaball outline needs at least three points for a hyperbezier".into());
        }
        for node in &mut nodes {
            node.nodetype = NodeType::Curve;
            node.smooth = true;
        }
        path.nodes = nodes;
        converted.1.points = points;
        converted.1.hyper = true;
        converted.1.metadata.identifier = Some(fresh_hyper_identifier());
        Ok(())
    }

    /// Replace typed HOI intermediate points without materializing a UFO glyph.
    pub fn set_hoi_intermediates(&mut self, points: HoiIntermediates) -> bool {
        let replacement = (!points.is_empty()).then_some(points);
        if self.preserved.hoi_intermediates == replacement {
            return false;
        }
        self.preserved.hoi_intermediates = replacement;
        true
    }

    pub(super) fn node_mut(&mut self, id: PointId) -> Option<&mut Node> {
        self.layer
            .shapes
            .iter_mut()
            .filter_map(|shape| match shape {
                Shape::Path(path) => Some(path),
                Shape::Component(_) => None,
            })
            .flat_map(|path| &mut path.nodes)
            .find(|node| read_id(&node.format_specific) == Some(id.0))
    }

    pub(super) fn node(&self, id: PointId) -> Option<&Node> {
        self.layer
            .paths()
            .flat_map(|path| &path.nodes)
            .find(|node| read_id(&node.format_specific) == Some(id.0))
    }

    pub(super) fn path_and_node_index_mut(
        &mut self,
        id: PointId,
    ) -> Option<(&mut babelfont::Path, usize)> {
        self.layer.shapes.iter_mut().find_map(|shape| {
            let Shape::Path(path) = shape else {
                return None;
            };
            let index = path
                .nodes
                .iter()
                .position(|node| read_id(&node.format_specific) == Some(id.0))?;
            Some((path, index))
        })
    }

    pub(super) fn contour_shape_index(&self, id: ContourId) -> Option<usize> {
        self.layer.shapes.iter().position(|shape| match shape {
            Shape::Path(path) => read_id(&path.format_specific) == Some(id.0),
            Shape::Component(_) => false,
        })
    }
}
