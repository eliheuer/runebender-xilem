// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical layer construction and identity-preserving glyph lifecycle helpers.

use babelfont::{Layer, LayerType, Shape};

use super::{
    LayerEditDraft, LayerPreservation, LayerView, ObjectMetadata, PreservedAnchor,
    PreservedComponent, PreservedContour, layer_key, write_id,
};
use crate::document::model::glyph_metadata::parse_metrics_key;
use crate::document::variable::{GlyphLayerAddress, LayerId};
use crate::formats::lib_keys::PROPOSAL_BASE_KEY;

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
            codepoints,
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
            hoi_intermediates: None,
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
        preserved.codepoints.clear();
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

/// Build one empty proposal layer from canonical foreground metadata.
///
/// The editable geometry payload is cleared without constructing a UFO glyph.
/// Unicode values and all non-proposal metadata remain available for previews, while installation
/// still copies only contours, components, anchors and advance width.
pub(in crate::document) fn composition_proposal_layer(
    foreground: LayerView<'_>,
    id: &LayerId,
    width: f64,
    components: &[(String, f64, f64)],
    anchors: &[(String, f64, f64)],
    revision: &str,
    reason: &str,
) -> Result<LayerEditDraft, String> {
    if !width.is_finite()
        || components
            .iter()
            .any(|(_, x, y)| !x.is_finite() || !y.is_finite())
        || anchors
            .iter()
            .any(|(_, x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err("composition geometry must be finite".into());
    }
    let (mut layer, mut preserved) = clone_layer(
        foreground.layer,
        foreground.preserved,
        id,
        foreground.glyph_name(),
        LayerCloneOptions {
            default: false,
            clear_codepoints: false,
        },
    );
    layer.shapes.clear();
    layer.anchors.clear();
    preserved.contours.clear();
    preserved.components.clear();
    preserved.anchors.clear();
    let mut draft = LayerEditDraft::new(layer, preserved);
    draft.set_width(width).map_err(|error| error.to_string())?;
    for (reference, x, y) in components {
        draft
            .add_component(
                reference.clone(),
                kurbo::Affine::translate(kurbo::Vec2::new(*x, *y)),
            )
            .map_err(|error| error.to_string())?;
    }
    for (name, x, y) in anchors {
        draft
            .add_anchor(name.clone(), kurbo::Point::new(*x, *y))
            .map_err(|error| error.to_string())?;
    }
    set_proposal_base(&mut draft, revision, reason);
    Ok(draft)
}

/// Read the external foreground revision record directly from canonical layer metadata.
///
/// A malformed record returns an empty revision so callers fail closed, matching the UFO codec.
pub(in crate::document) fn proposal_base(layer: LayerView<'_>) -> Option<&str> {
    layer.preserved.lib.get(PROPOSAL_BASE_KEY).map(|value| {
        value
            .as_dictionary()
            .and_then(|dictionary| dictionary.get("revision"))
            .and_then(plist::Value::as_string)
            .unwrap_or("")
    })
}

/// Record the external foreground revision and design intent in canonical layer metadata.
pub(in crate::document) fn set_proposal_base(
    draft: &mut LayerEditDraft,
    revision: &str,
    reason: &str,
) {
    let mut record = plist::Dictionary::new();
    record.insert("revision".into(), revision.into());
    record.insert("reason".into(), reason.into());
    draft
        .preserved
        .lib
        .insert(PROPOSAL_BASE_KEY.into(), record.into());
}

/// Whether an owned draft carries the exact requested canonical address.
pub(in crate::document) fn draft_matches_address(
    draft: &LayerEditDraft,
    address: &GlyphLayerAddress,
) -> bool {
    draft.preserved.name == address.glyph
        && draft.layer.id.as_deref() == Some(layer_key(&address.layer).as_str())
        && draft.layer.name.as_deref() == Some(address.layer.name.as_str())
        && matches!(
            &draft.layer.master,
            LayerType::AssociatedWithMaster(master) if master == &address.layer.source.0.to_string()
        )
}

/// Compare only the payload that proposal installation copies to the foreground.
pub(in crate::document) fn proposal_payload_eq(left: LayerView<'_>, right: LayerView<'_>) -> bool {
    left.width() == right.width()
        && left
            .contours()
            .map(|contour| {
                (
                    contour.is_closed(),
                    contour.is_hyper(),
                    contour
                        .points()
                        .map(|point| {
                            (
                                point.position(),
                                point.point_type(),
                                point.is_smooth(),
                                point.name().map(ToOwned::to_owned),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .eq(right.contours().map(|contour| {
                (
                    contour.is_closed(),
                    contour.is_hyper(),
                    contour
                        .points()
                        .map(|point| {
                            (
                                point.position(),
                                point.point_type(),
                                point.is_smooth(),
                                point.name().map(ToOwned::to_owned),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            }))
        && left
            .components()
            .map(|component| (component.reference(), component.transform()))
            .eq(right
                .components()
                .map(|component| (component.reference(), component.transform())))
        && left
            .anchors()
            .map(|anchor| (anchor.name(), anchor.position()))
            .eq(right
                .anchors()
                .map(|anchor| (anchor.name(), anchor.position())))
        && left
            .preserved
            .contours
            .iter()
            .map(|contour| {
                (
                    contour.hyper,
                    &contour.metadata,
                    contour
                        .points
                        .iter()
                        .map(|point| (&point.name, &point.metadata))
                        .collect::<Vec<_>>(),
                )
            })
            .eq(right.preserved.contours.iter().map(|contour| {
                (
                    contour.hyper,
                    &contour.metadata,
                    contour
                        .points
                        .iter()
                        .map(|point| (&point.name, &point.metadata))
                        .collect::<Vec<_>>(),
                )
            }))
        && left
            .preserved
            .components
            .iter()
            .map(|component| (&component.alignment, &component.metadata))
            .eq(right
                .preserved
                .components
                .iter()
                .map(|component| (&component.alignment, &component.metadata)))
        && left
            .preserved
            .anchors
            .iter()
            .map(|anchor| (anchor.color, &anchor.metadata))
            .eq(right
                .preserved
                .anchors
                .iter()
                .map(|anchor| (anchor.color, &anchor.metadata)))
}

/// Copy a proposal's editable payload onto a canonical foreground draft.
///
/// Foreground glyph metadata remains authoritative. Stable object identities are retained by
/// contour and point position when structure permits, by component reference, and by anchor name.
/// New or structurally replaced objects keep their already unique proposal identities.
pub(in crate::document) fn install_proposal_payload(
    foreground: LayerEditDraft,
    proposed: LayerView<'_>,
) -> LayerEditDraft {
    let (mut layer, mut preserved) = foreground.into_parts();
    let old_layer = layer.clone();
    let old_preserved = preserved.clone();

    let proposed_paths = proposed
        .layer
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Path(path) => Some(path.clone()),
            Shape::Component(_) => None,
        })
        .collect::<Vec<_>>();
    let proposed_components = proposed
        .layer
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Component(component) => Some(component.clone()),
            Shape::Path(_) => None,
        })
        .collect::<Vec<_>>();
    let old_paths = old_layer
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Path(path) => Some(path),
            Shape::Component(_) => None,
        })
        .collect::<Vec<_>>();
    let old_components = old_layer
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Component(component) => Some(component),
            Shape::Path(_) => None,
        })
        .collect::<Vec<_>>();

    let mut contours = proposed.preserved.contours.clone();
    let mut paths = proposed_paths;
    retain_contour_identities(
        &old_paths,
        &old_preserved.contours,
        &mut paths,
        &mut contours,
    );

    let mut components = proposed.preserved.components.clone();
    let mut component_shapes = proposed_components;
    retain_component_identities(
        &old_components,
        &old_preserved.components,
        &mut component_shapes,
        &mut components,
    );

    let mut anchors = proposed.layer.anchors.clone();
    let mut anchor_metadata = proposed.preserved.anchors.clone();
    retain_anchor_identities(
        &old_layer.anchors,
        &old_preserved.anchors,
        &mut anchors,
        &mut anchor_metadata,
    );

    layer.shapes = paths
        .into_iter()
        .map(Shape::Path)
        .chain(component_shapes.into_iter().map(Shape::Component))
        .collect();
    layer.anchors = anchors;
    layer.width = proposed.layer.width;
    preserved.width = proposed.preserved.width;
    preserved.contours = contours;
    preserved.components = components;
    preserved.anchors = anchor_metadata;
    LayerEditDraft::new(layer, preserved)
}

fn retain_contour_identities(
    old_paths: &[&babelfont::Path],
    old: &[PreservedContour],
    new_paths: &mut [babelfont::Path],
    new: &mut [PreservedContour],
) {
    let mut used_contours = vec![false; old.len()];
    for (new_index, (path, preserved)) in new_paths.iter_mut().zip(new).enumerate() {
        let by_identifier = preserved
            .metadata
            .identifier
            .as_ref()
            .and_then(|identifier| {
                unique_unused(old, &used_contours, |candidate| {
                    candidate.metadata.identifier.as_ref() == Some(identifier)
                })
            });
        let by_position = old_paths
            .get(new_index)
            .zip(old.get(new_index))
            .filter(|(old_path, _)| {
                !used_contours[new_index]
                    && old_path.closed == path.closed
                    && old_path.nodes.len() == path.nodes.len()
                    && old_path
                        .nodes
                        .iter()
                        .zip(&path.nodes)
                        .all(|(left, right)| left.nodetype == right.nodetype)
            })
            .map(|_| new_index);
        let Some(old_index) = by_identifier.or(by_position) else {
            continue;
        };
        used_contours[old_index] = true;
        let old_path = old_paths[old_index];
        let old_preserved = &old[old_index];
        preserved.id = old_preserved.id;
        write_id(&mut path.format_specific, preserved.id.0);
        let mut used_points = vec![false; old_preserved.points.len()];
        for (point_index, (node, point)) in
            path.nodes.iter_mut().zip(&mut preserved.points).enumerate()
        {
            let by_identifier = point.metadata.identifier.as_ref().and_then(|identifier| {
                unique_unused(&old_preserved.points, &used_points, |candidate| {
                    candidate.metadata.identifier.as_ref() == Some(identifier)
                })
            });
            let by_position = old_path
                .nodes
                .get(point_index)
                .zip(old_preserved.points.get(point_index))
                .filter(|(old_node, _)| {
                    !used_points[point_index] && old_node.nodetype == node.nodetype
                })
                .map(|_| point_index);
            let Some(old_point) = by_identifier.or(by_position) else {
                continue;
            };
            used_points[old_point] = true;
            point.id = old_preserved.points[old_point].id;
            write_id(&mut node.format_specific, point.id.0);
        }
    }
}

fn retain_component_identities(
    old_shapes: &[&babelfont::Component],
    old: &[PreservedComponent],
    new_shapes: &mut [babelfont::Component],
    new: &mut [PreservedComponent],
) {
    let mut used = vec![false; old_shapes.len()];
    for (shape, preserved) in new_shapes.iter_mut().zip(new) {
        let by_identifier = preserved
            .metadata
            .identifier
            .as_ref()
            .and_then(|identifier| {
                unique_unused(old, &used, |candidate| {
                    candidate.metadata.identifier.as_ref() == Some(identifier)
                })
            });
        let by_reference = unique_unused(old_shapes, &used, |candidate| {
            candidate.reference == shape.reference
        });
        let candidate = by_identifier.or(by_reference);
        let Some(index) = candidate else {
            continue;
        };
        used[index] = true;
        preserved.id = old[index].id;
        write_id(&mut shape.format_specific, preserved.id.0);
    }
}

fn retain_anchor_identities(
    old_anchors: &[babelfont::Anchor],
    old: &[PreservedAnchor],
    new_anchors: &mut [babelfont::Anchor],
    new: &mut [PreservedAnchor],
) {
    let mut used = vec![false; old_anchors.len()];
    for (anchor, preserved) in new_anchors.iter_mut().zip(new) {
        let by_identifier = preserved
            .metadata
            .identifier
            .as_ref()
            .and_then(|identifier| {
                unique_unused(old, &used, |candidate| {
                    candidate.metadata.identifier.as_ref() == Some(identifier)
                })
            });
        let by_name = unique_unused(old_anchors, &used, |candidate| {
            candidate.name == anchor.name
        });
        let candidate = by_identifier.or(by_name);
        let Some(index) = candidate else {
            continue;
        };
        used[index] = true;
        preserved.id = old[index].id;
        write_id(&mut anchor.format_specific, preserved.id.0);
    }
}

fn unique_unused<T>(items: &[T], used: &[bool], predicate: impl Fn(&T) -> bool) -> Option<usize> {
    let mut matches = items
        .iter()
        .enumerate()
        .filter(|(index, item)| !used[*index] && predicate(item))
        .map(|(index, _)| index);
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
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
                .then(super::fresh_object_identifier)
        },
        lib: metadata.lib.clone(),
    }
}

#[cfg(test)]
mod tests {
    use norad::{
        AffineTransform, Anchor, Component, Contour, ContourPoint, Glyph, Name, PointType,
    };

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
            Name::new("base").unwrap(),
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
            Name::new("A").unwrap(),
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

    #[test]
    fn proposal_install_follows_identifiers_across_reorder() {
        let identifier = |value: &str| norad::Identifier::new(value).unwrap();
        let mut glyph = Glyph::new("A");
        for (value, x) in [("contour.first", 10.0), ("contour.second", 20.0)] {
            glyph.contours.push(Contour::new(
                vec![ContourPoint::new(
                    x,
                    0.0,
                    PointType::Move,
                    false,
                    None,
                    Some(identifier(&format!("{value}.point"))),
                )],
                Some(identifier(value)),
            ));
        }
        for value in ["component.first", "component.second"] {
            glyph.components.push(Component::new(
                Name::new("base").unwrap(),
                AffineTransform::default(),
                Some(identifier(value)),
            ));
        }
        for (value, name) in [("anchor.first", "top"), ("anchor.second", "bottom")] {
            glyph.anchors.push(Anchor::new(
                0.0,
                0.0,
                Some(Name::new(name).unwrap()),
                None,
                Some(identifier(value)),
            ));
        }
        let foreground_id = layer_id();
        let proposal_id = LayerId {
            source: foreground_id.source,
            name: "com.runebender.proposal.reorder".into(),
        };
        let (foreground_layer, foreground_preserved) = layer_from_ufo(&glyph, &foreground_id, true);
        let foreground = LayerView::new(&foreground_layer, &foreground_preserved);
        let contour_ids = foreground
            .contours()
            .map(|contour| contour.id())
            .collect::<Vec<_>>();
        let component_ids = foreground
            .components()
            .map(|component| component.id())
            .collect::<Vec<_>>();
        let anchor_ids = foreground
            .anchors()
            .map(|anchor| anchor.id())
            .collect::<Vec<_>>();

        let mut reordered = glyph;
        reordered.contours.reverse();
        reordered.components.reverse();
        reordered.anchors.reverse();
        let (proposal_layer, proposal_preserved) = layer_from_ufo(&reordered, &proposal_id, false);
        let installed = install_proposal_payload(
            LayerEditDraft::new(foreground_layer, foreground_preserved),
            LayerView::new(&proposal_layer, &proposal_preserved),
        );
        assert_eq!(
            installed
                .view()
                .contours()
                .map(|contour| contour.id())
                .collect::<Vec<_>>(),
            [contour_ids[1], contour_ids[0]]
        );
        assert_eq!(
            installed
                .view()
                .components()
                .map(|component| component.id())
                .collect::<Vec<_>>(),
            [component_ids[1], component_ids[0]]
        );
        assert_eq!(
            installed
                .view()
                .anchors()
                .map(|anchor| anchor.id())
                .collect::<Vec<_>>(),
            [anchor_ids[1], anchor_ids[0]]
        );
    }
}
