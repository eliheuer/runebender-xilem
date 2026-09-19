// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical layer construction and identity-preserving glyph lifecycle helpers.

use babelfont::{Layer, LayerType, Shape};

use super::{LayerPreservation, ObjectMetadata, layer_key};
use crate::document::model::glyph_metadata::parse_metrics_key;
use crate::document::variable::LayerId;

/// Explicit semantic differences between layer cloning workflows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::document) struct LayerCloneOptions {
    /// Whether the new layer is the default layer for its source.
    pub default: bool,
    /// Whether to clear Unicode values for an ordinary duplicate-glyph command.
    pub clear_codepoints: bool,
}

/// Construct an empty canonical glyph layer without a UFO editing object.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the exact f64 advance remains authoritative in LayerPreservation"
)]
pub(in crate::document) fn empty_layer(
    name: &str,
    id: &LayerId,
    default: bool,
    width: f64,
    codepoint: Option<char>,
) -> (Layer, LayerPreservation) {
    let layer = Layer {
        id: Some(layer_key(id)),
        name: Some(id.name.clone()),
        width: width as f32,
        master: if default {
            LayerType::DefaultForMaster(id.source.0.to_string())
        } else {
            LayerType::AssociatedWithMaster(id.source.0.to_string())
        },
        ..Layer::default()
    };
    let codepoints = codepoint.into_iter().collect::<Vec<_>>();
    (
        layer,
        LayerPreservation {
            name: name.to_owned(),
            width,
            height: 0.0,
            codepoints: norad::Codepoints::new(codepoints),
            note: None,
            guidelines: Vec::new(),
            image: None,
            lib: plist::Dictionary::new(),
            mark_color: None,
            left_metrics_key: None,
            right_metrics_key: None,
            metaballs: None,
            composition_recipe: None,
            smart_component_axes: None,
            smart_component_values: None,
            smart_component_pole: None,
            contours: Vec::new(),
            components: Vec::new(),
            anchors: Vec::new(),
        },
    )
}

/// Duplicate a canonical layer for a new glyph and assign fresh object identities.
///
/// Exact layer metadata is retained, while Unicode values are cleared for the existing duplicate
/// command and every document and source identifier owned by a copied object is refreshed.
pub(in crate::document) fn duplicate_layer(
    layer: &Layer,
    preserved: &LayerPreservation,
    id: &LayerId,
    name: &str,
) -> (Layer, LayerPreservation) {
    clone_layer(
        layer,
        preserved,
        id,
        name,
        LayerCloneOptions {
            default: matches!(layer.master, LayerType::DefaultForMaster(_)),
            clear_codepoints: true,
        },
    )
}

/// Clone a layer with fresh object identities and explicit default/Unicode semantics.
///
/// Ordinary glyph duplication clears Unicode values.
/// New-source construction retains them and selects a new default source layer.
pub(in crate::document) fn clone_layer(
    layer: &Layer,
    preserved: &LayerPreservation,
    id: &LayerId,
    name: &str,
    options: LayerCloneOptions,
) -> (Layer, LayerPreservation) {
    let (mut layer, mut preserved) = super::copy_layer(layer, preserved, id);
    layer.master = if options.default {
        LayerType::DefaultForMaster(id.source.0.to_string())
    } else {
        LayerType::AssociatedWithMaster(id.source.0.to_string())
    };
    preserved.name = name.to_owned();
    if options.clear_codepoints {
        preserved.codepoints = norad::Codepoints::new([]);
    }

    for (path, contour) in layer
        .shapes
        .iter_mut()
        .filter_map(|shape| match shape {
            Shape::Path(path) => Some(path),
            Shape::Component(_) => None,
        })
        .zip(&mut preserved.contours)
    {
        contour.metadata = copied_metadata(&contour.metadata, contour.hyper);
        for (_node, point) in path.nodes.iter_mut().zip(&mut contour.points) {
            point.metadata = copied_metadata(&point.metadata, false);
        }
    }
    for (_component, preserved) in layer
        .shapes
        .iter_mut()
        .filter_map(|shape| match shape {
            Shape::Component(component) => Some(component),
            Shape::Path(_) => None,
        })
        .zip(&mut preserved.components)
    {
        preserved.metadata = copied_metadata(&preserved.metadata, false);
    }
    for (_anchor, preserved) in layer.anchors.iter_mut().zip(&mut preserved.anchors) {
        preserved.metadata = copied_metadata(&preserved.metadata, false);
    }
    (layer, preserved)
}

/// Change only the glyph name carried by one layer's preservation payload.
pub(in crate::document) fn rename_layer(preserved: &mut LayerPreservation, new_name: &str) -> bool {
    if preserved.name == new_name {
        return false;
    }
    preserved.name = new_name.to_owned();
    true
}

/// Rename component and metrics-formula references inside one canonical layer.
pub(in crate::document) fn rename_references(
    layer: &mut Layer,
    preserved: &mut LayerPreservation,
    old: &str,
    new: &str,
) -> bool {
    let mut changed = false;
    for shape in &mut layer.shapes {
        if let Shape::Component(component) = shape
            && component.reference == old
        {
            component.reference = new.into();
            changed = true;
        }
    }
    for source in [
        &mut preserved.left_metrics_key,
        &mut preserved.right_metrics_key,
    ] {
        let Some(plist::Value::String(value)) = source else {
            continue;
        };
        let Some(formula) = parse_metrics_key(value) else {
            continue;
        };
        if formula.referenced_glyph() != Some(old) {
            continue;
        }
        let Some(start) = value.find(old) else {
            continue;
        };
        value.replace_range(start..start + old.len(), new);
        changed = true;
    }
    changed
}

fn copied_metadata(metadata: &ObjectMetadata, hyper: bool) -> ObjectMetadata {
    ObjectMetadata {
        identifier: if hyper {
            Some(super::fresh_hyper_identifier())
        } else {
            (metadata.identifier.is_some() || metadata.lib.is_some())
                .then(norad::Identifier::from_uuidv4)
        },
        lib: metadata.lib.clone(),
    }
}

#[cfg(test)]
mod tests {
    use norad::{AffineTransform, Component, Contour, ContourPoint, Glyph, PointType};

    use super::*;
    use crate::document::babelfont::{LayerView, layer_from_ufo, project_layer};
    use crate::document::variable::SourceId;

    fn layer_id() -> LayerId {
        LayerId {
            source: SourceId(7),
            name: "public.default".into(),
        }
    }

    #[test]
    fn empty_layer_keeps_exact_advance_and_codepoint() {
        let id = layer_id();
        let (layer, preserved) = empty_layer("A", &id, true, 500.125, Some('A'));
        let view = LayerView::new(&layer, &preserved);

        assert_eq!(view.width(), 500.125);
        assert_eq!(view.codepoints().collect::<Vec<_>>(), vec!['A']);
        assert_eq!(project_layer(&layer, &preserved).name().as_str(), "A");
    }

    #[test]
    fn duplicate_refreshes_identities_and_preserves_exact_metadata() {
        let id = layer_id();
        let mut glyph = Glyph::new("A");
        glyph.width = 500.125;
        glyph.note = Some("exact note".into());
        glyph.codepoints = norad::Codepoints::new(['A']);
        glyph.contours.push(Contour::new(
            vec![ContourPoint::new(
                12.25,
                34.75,
                PointType::Move,
                false,
                None,
                Some(norad::Identifier::from_uuidv4()),
            )],
            Some(norad::Identifier::from_uuidv4()),
        ));
        glyph.components.push(Component::new(
            norad::Name::new("base").unwrap(),
            AffineTransform::default(),
            Some(norad::Identifier::from_uuidv4()),
        ));
        let (layer, preserved) = layer_from_ufo(&glyph, &id, true);
        let source = LayerView::new(&layer, &preserved);
        let source_contour = source.contours().next().unwrap().id();
        let source_point = source
            .contours()
            .next()
            .unwrap()
            .points()
            .next()
            .unwrap()
            .id();
        let source_component = source.components().next().unwrap().id();

        let (copy_layer, copy_preserved) = duplicate_layer(&layer, &preserved, &id, "A.001");
        let copy = LayerView::new(&copy_layer, &copy_preserved);
        assert_eq!(copy.width(), 500.125);
        assert_eq!(copy.note(), Some("exact note"));
        assert_eq!(copy.codepoints().count(), 0);
        assert_ne!(copy.contours().next().unwrap().id(), source_contour);
        assert_ne!(
            copy.contours()
                .next()
                .unwrap()
                .points()
                .next()
                .unwrap()
                .id(),
            source_point
        );
        assert_ne!(copy.components().next().unwrap().id(), source_component);
        assert_eq!(
            project_layer(&copy_layer, &copy_preserved).name().as_str(),
            "A.001"
        );
    }

    #[test]
    fn rename_updates_references_without_normalizing_metrics_source() {
        let id = layer_id();
        let mut glyph = Glyph::new("user");
        glyph.components.push(Component::new(
            norad::Name::new("A").unwrap(),
            AffineTransform::default(),
            None,
        ));
        glyph.lib.insert(
            crate::document::model::glyph_metadata::LEFT_METRICS_KEY.into(),
            plist::Value::String(" = |A + 1.50 ".into()),
        );
        let (mut layer, mut preserved) = layer_from_ufo(&glyph, &id, true);

        assert!(rename_references(&mut layer, &mut preserved, "A", "A.alt"));
        let projected = project_layer(&layer, &preserved);
        assert_eq!(projected.components[0].base.as_str(), "A.alt");
        assert_eq!(
            projected
                .lib
                .get(crate::document::model::glyph_metadata::LEFT_METRICS_KEY)
                .and_then(plist::Value::as_string),
            Some(" = |A.alt + 1.50 ")
        );
    }
}
