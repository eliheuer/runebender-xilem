// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Staged canonical payloads for adding one interpolated source.

use crate::document::babelfont::glyph_transactions::{LayerCloneOptions, clone_layer};
use crate::document::interpolation::{InterpolatedLayer, InterpolatedShape};
use crate::document::model::designspace::CanonicalDesignspace;
use crate::document::model::font_info::CanonicalFontInfo;
use crate::document::{CanonicalLayerSnapshot, LayerEditDraft, LayerShapeView};

use super::{CanonicalSourceStructureSnapshot, GlyphLayerAddress, SourceId, SourceMetadata};

/// Clone one default layer into a new source and apply canonical interpolation values.
///
/// Layer metadata and Unicode values come from the default interpolation layer.
/// Every contour, point, component, anchor and UFO object identifier is fresh.
pub(in crate::document) fn interpolated_source_layer(
    base: CanonicalLayerSnapshot,
    output: &InterpolatedLayer,
    address: GlyphLayerAddress,
) -> Result<CanonicalLayerSnapshot, String> {
    if base.address().glyph != output.glyph_name || address.glyph != output.glyph_name {
        return Err("interpolated layer glyph identity does not match its address".into());
    }
    let (base_layer, base_preserved) = base.into_parts();
    let (layer, preserved) = clone_layer(
        &base_layer,
        &base_preserved,
        &address.layer,
        &address.glyph,
        LayerCloneOptions {
            default: true,
            clear_codepoints: false,
        },
    );
    let mut draft = LayerEditDraft::new(layer, preserved);
    if draft
        .view()
        .codepoints()
        .ne(output.codepoints.iter().copied())
        || draft.view().note() != output.note.as_deref()
    {
        return Err("interpolation changed non-varying glyph metadata".into());
    }
    draft
        .set_width(output.width)
        .map_err(|error| error.to_string())?;
    draft
        .set_height(output.height)
        .map_err(|error| error.to_string())?;

    let source_shapes = draft.view().shapes().collect::<Vec<_>>();
    if source_shapes.len() != output.shapes.len() {
        return Err("interpolation changed glyph shape count".into());
    }
    let mut point_updates = Vec::new();
    let mut component_updates = Vec::new();
    for (source, interpolated) in source_shapes.into_iter().zip(&output.shapes) {
        match (source, interpolated) {
            (LayerShapeView::Contour(source), InterpolatedShape::Contour(interpolated)) => {
                if source.is_closed() != interpolated.closed
                    || source.is_hyper() != interpolated.hyper
                {
                    return Err("interpolation changed contour structure".into());
                }
                let points = source.points().collect::<Vec<_>>();
                if points.len() != interpolated.points.len() {
                    return Err("interpolation changed contour point count".into());
                }
                for (source, interpolated) in points.into_iter().zip(&interpolated.points) {
                    if source.point_type() != interpolated.point_type
                        || source.is_smooth() != interpolated.smooth
                        || source.name() != interpolated.name.as_deref()
                    {
                        return Err("interpolation changed point structure".into());
                    }
                    point_updates.push((source.id(), interpolated.position));
                }
            }
            (LayerShapeView::Component(source), InterpolatedShape::Component(interpolated)) => {
                if source.reference() != interpolated.reference {
                    return Err("interpolation changed component structure".into());
                }
                component_updates.push((source.id(), interpolated.transform));
            }
            _ => return Err("interpolation changed contour/component paint order".into()),
        }
    }
    let source_anchors = draft.view().anchors().collect::<Vec<_>>();
    if source_anchors.len() != output.anchors.len() {
        return Err("interpolation changed anchor count".into());
    }
    let mut anchor_updates = Vec::new();
    for (source, interpolated) in source_anchors.into_iter().zip(&output.anchors) {
        if source.name() != interpolated.name {
            return Err("interpolation changed anchor structure".into());
        }
        anchor_updates.push((source.id(), interpolated.position));
    }
    for (point, position) in point_updates {
        draft
            .set_point_position(point, position)
            .map_err(|error| error.to_string())?;
    }
    for (component, transform) in component_updates {
        draft
            .set_component_transform(component, transform)
            .map_err(|error| error.to_string())?;
    }
    for (anchor, position) in anchor_updates {
        draft
            .set_anchor_position(anchor, position)
            .map_err(|error| error.to_string())?;
    }
    let (layer, preserved) = draft.into_parts();
    Ok(CanonicalLayerSnapshot::new(address, layer, preserved))
}

impl CanonicalSourceStructureSnapshot {
    /// Add one completely staged source to an owned structural replacement.
    pub(in crate::document) fn add_interpolated_source(
        &mut self,
        source: SourceId,
        based_on: SourceId,
        designspace: CanonicalDesignspace,
        feature_text: String,
        font_metadata: crate::document::canonical_metadata::CanonicalFontMetadata,
        font_info: CanonicalFontInfo,
        layers: Vec<CanonicalLayerSnapshot>,
    ) -> Result<(), String> {
        if self.source_ids.contains(&source)
            || self.source_formats.contains_key(&source)
            || self.source_metadata.contains_key(&source)
        {
            return Err("new source identity is already in use".into());
        }
        let expected_order = self
            .source_ids
            .iter()
            .copied()
            .chain(std::iter::once(source))
            .collect::<Vec<_>>();
        if designspace.full_source_order().collect::<Vec<_>>() != expected_order {
            return Err("new source order does not match the canonical Designspace".into());
        }
        font_info.validate().map_err(|error| error.to_string())?;
        let mut format = self
            .source_formats
            .get(&based_on)
            .cloned()
            .ok_or("missing default source format data")?;
        format.retain_default_layer();
        let default_layer_name = format.default_layer_name().to_owned();

        let mut seen = std::collections::BTreeSet::new();
        for snapshot in layers {
            let address = snapshot.address().clone();
            if address.layer.source != source || address.layer.name != default_layer_name {
                return Err("staged layer does not use the new source default identity".into());
            }
            if !seen.insert(address.glyph.clone()) {
                return Err(format!("duplicate staged layer for {}", address.glyph));
            }
            let glyph = self
                .glyphs
                .get_mut(&address.glyph)
                .ok_or_else(|| format!("missing logical glyph {}", address.glyph))?;
            if glyph.layers.contains_key(&address.layer) {
                return Err(format!(
                    "new source layer already exists for {}",
                    address.glyph
                ));
            }
            let source_metadata = glyph
                .source_metadata
                .get(&based_on)
                .cloned()
                .ok_or_else(|| format!("missing default source metadata for {}", address.glyph))?;
            let geometry = self
                .glyph_geometry
                .get_mut(&address.glyph)
                .ok_or_else(|| format!("missing canonical geometry for {}", address.glyph))?;
            let (layer, preserved) = snapshot.into_parts();
            glyph.layers.insert(address.layer.clone(), preserved);
            glyph.source_metadata.insert(source, source_metadata);
            geometry.layers.push(layer);
        }
        self.source_ids.push(source);
        self.source_formats.insert(source, format);
        self.source_metadata.insert(
            source,
            SourceMetadata {
                feature_text,
                font_metadata,
                font_info,
            },
        );
        self.designspace = Some(designspace);
        Ok(())
    }
}
