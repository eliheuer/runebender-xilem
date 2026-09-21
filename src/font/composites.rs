// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Component anchor alignment: marks placed by `_top`/`top` anchor
//! pairs follow their base. This is how Glyphs aligns components. A
//! port of the alignment code in runebender-web's editor.rs and
//! `wasm_api.rs`.
//!
//! A composite stores its components as fixed offsets, so alignment
//! is not re-derived at render time: it is baked into the file, and
//! this module is what has to run over every glyph that places a base
//! whose anchors just moved.

use kurbo::{Point, Vec2};

#[cfg(test)]
use norad::{Component, Font, Glyph};

#[cfg(test)]
use super::model::glyph_metadata::ComponentAlignment;
use super::{ComponentId, DocumentEditError, LayerEditDraft, LayerView};

#[derive(Debug)]
/// One component's contribution to anchor alignment: the anchors its
/// base glyph carries (in the base's own coordinates), where the
/// component currently sits, and whether it is still anchor-locked.
pub struct AlignInput {
    /// Anchors the base glyph carries, as `(name, position)` in the base's own coordinates.
    pub anchors: Vec<(String, Point)>,
    /// The component's current translation in the composite's coordinates.
    pub offset: Vec2,
    /// True while the component still follows its anchor; false once it is cut loose.
    pub aligned: bool,
}

/// Report whether a component is cut loose from its anchor.
///
/// The check reads the Glyphs alignment lib key. Returns true when
/// the key marks the component as not aligned.
#[cfg(test)]
pub fn component_alignment_disabled(component: &Component) -> bool {
    let mut lib = component.lib().cloned().unwrap_or_default();
    ComponentAlignment::take_from_lib(&mut lib).is_disabled()
}

/// Lock a component to its anchor or cut it loose.
///
/// Cutting loose writes the Glyphs key and leaves the component
/// where it sits. Locking removes the key; the caller realigns
/// afterwards to snap it home.
#[cfg(test)]
pub fn set_component_alignment_disabled(component: &mut Component, disabled: bool) {
    let mut lib = component.lib().cloned().unwrap_or_default();
    let mut alignment = ComponentAlignment::take_from_lib(&mut lib);
    if !alignment.set_disabled(disabled) {
        return;
    }
    alignment.write_to_lib(&mut lib);
    if lib.is_empty() {
        component.take_lib();
    } else {
        component.replace_lib(lib);
    }
}

/// Re-place anchor-locked components against the anchors in front
/// of them.
///
/// `seed` is the glyph's own anchors. The open-glyph editor offers
/// them; the file-level pass over composites does not.
///
/// Anchors accumulate as the walk goes: a component's outgoing
/// anchors are offered to the components after it, which is how a
/// second mark stacks on the first rather than landing back on the
/// letter. Returns each component's corrected offset.
pub fn realign_component_offsets(components: &[AlignInput], seed: &[(String, Point)]) -> Vec<Vec2> {
    let mut available: Vec<(&str, Point)> = seed
        .iter()
        .map(|(name, point)| (name.as_str(), *point))
        .collect();
    let mut out = Vec::with_capacity(components.len());

    for component in components {
        let mut offset = component.offset;
        if component.aligned {
            // Every anchor on the mark is a candidate, not just the
            // first: marks routinely carry outgoing anchors (`top`,
            // for stacking) beside the incoming `_top`, and source
            // order must not decide whether it aligns at all.
            let delta = component.anchors.iter().find_map(|(name, point)| {
                let target_name = name.strip_prefix('_')?;
                let (_, target) = available
                    .iter()
                    .rev()
                    .find(|(available, _)| *available == target_name)?;
                Some(*target - (*point + offset))
            });
            if let Some(delta) = delta {
                offset += delta;
            }
        }
        out.push(offset);
        for (name, point) in &component.anchors {
            if !name.starts_with('_') {
                available.push((name.as_str(), *point + offset));
            }
        }
    }
    out
}

fn document_anchors(layer: LayerView<'_>) -> Vec<(String, Point)> {
    layer
        .anchors()
        .map(|anchor| (anchor.name().to_owned(), anchor.position()))
        .collect()
}

/// Read the anchors a canonical layer offers to generated features and composition.
///
/// A layer's own anchors win.
/// A layer without anchors inherits outgoing anchors from its components recursively, using each
/// exact component transform; incoming anchors never propagate through the composite.
/// Missing component bases contribute no anchors, matching the established UFO workflow.
pub fn effective_document_anchors<'a>(
    layer: LayerView<'a>,
    mut resolve: impl FnMut(&str) -> Option<LayerView<'a>>,
) -> Vec<(String, Point)> {
    fn recurse<'a>(
        layer: LayerView<'a>,
        resolve: &mut impl FnMut(&str) -> Option<LayerView<'a>>,
        depth: usize,
    ) -> Vec<(String, Point)> {
        let own = document_anchors(layer);
        if !own.is_empty() || depth > 8 {
            return own;
        }
        let mut anchors = Vec::new();
        for component in layer.components() {
            let Some(base) = resolve(component.reference()) else {
                continue;
            };
            for (name, point) in recurse(base, resolve, depth + 1) {
                if name.starts_with('_') {
                    continue;
                }
                let point = component.transform() * point;
                anchors.retain(|(candidate, _)| candidate != &name);
                anchors.push((name, point));
            }
        }
        anchors
    }

    recurse(layer, &mut resolve, 0)
}

/// Build stable component identities and alignment inputs from one canonical layer.
///
/// `resolve` selects component bases in the same source/layer context as `layer`.
/// A missing base has no anchors and therefore leaves that component at its stored position.
pub fn document_align_inputs<'layer, 'base>(
    layer: LayerView<'layer>,
    mut resolve: impl FnMut(&str) -> Option<LayerView<'base>>,
) -> Vec<(ComponentId, AlignInput)> {
    layer
        .components()
        .map(|component| {
            let transform = component.transform();
            let coefficients = transform.as_coeffs();
            (
                component.id(),
                AlignInput {
                    anchors: resolve(component.reference())
                        .map(document_anchors)
                        .unwrap_or_default(),
                    offset: Vec2::new(coefficients[4], coefficients[5]),
                    aligned: !component.alignment_disabled(),
                },
            )
        })
        .collect()
}

/// Realign the anchor-locked components in a canonical edit draft.
///
/// Only the translation coefficients change; the exact linear transform and component metadata
/// remain attached to their stable identities.
pub fn realign_document_layer<'a>(
    draft: &mut LayerEditDraft,
    resolve: impl FnMut(&str) -> Option<LayerView<'a>>,
    seed_own_anchors: bool,
) -> Result<bool, DocumentEditError> {
    let layer = draft.view();
    let components = document_align_inputs(layer, resolve);
    if components.is_empty() {
        return Ok(false);
    }
    let seed = if seed_own_anchors {
        document_anchors(layer)
    } else {
        Vec::new()
    };
    let inputs: Vec<_> = components
        .iter()
        .map(|(_, input)| AlignInput {
            anchors: input.anchors.clone(),
            offset: input.offset,
            aligned: input.aligned,
        })
        .collect();
    let placed = realign_component_offsets(&inputs, &seed);
    let updates = components
        .iter()
        .zip(placed)
        .map(|((id, _), offset)| {
            let component = layer
                .components()
                .find(|component| component.id() == *id)
                .expect("alignment input retains its canonical component");
            let mut coefficients = component.transform().as_coeffs();
            coefficients[4] = offset.x;
            coefficients[5] = offset.y;
            coefficients
                .iter()
                .all(|value| value.is_finite())
                .then_some((*id, kurbo::Affine::new(coefficients)))
                .ok_or(DocumentEditError::NonFinite)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut changed = false;
    for (id, transform) in updates {
        changed |= draft.set_component_transform(id, transform)?;
    }
    Ok(changed)
}

/// Return every canonical layer glyph that places `base` as a component.
pub fn document_composites_using<'a>(
    layers: impl IntoIterator<Item = LayerView<'a>>,
    base: &str,
) -> Vec<String> {
    layers
        .into_iter()
        .filter(|layer| {
            layer
                .components()
                .any(|component| component.reference() == base)
        })
        .map(|layer| layer.glyph_name().to_owned())
        .collect()
}

#[cfg(test)]
fn base_anchors(font: &Font, base: &str) -> Vec<(String, Point)> {
    font.get_glyph(base)
        .map(|glyph| {
            glyph
                .anchors
                .iter()
                .filter_map(|a| Some((a.name.as_ref()?.to_string(), Point::new(a.x, a.y))))
                .collect()
        })
        .unwrap_or_default()
}

/// The alignment inputs for a glyph's components, resolved against
/// the font.
#[cfg(test)]
pub fn align_inputs(font: &Font, glyph: &Glyph) -> Vec<AlignInput> {
    glyph
        .components
        .iter()
        .map(|component| AlignInput {
            anchors: base_anchors(font, component.base.as_str()),
            offset: Vec2::new(component.transform.x_offset, component.transform.y_offset),
            aligned: !component_alignment_disabled(component),
        })
        .collect()
}

/// Realign one glyph's components in place.
///
/// `seed_own_anchors` is true for the glyph open in an editor, where
/// its own anchors are offered to the components, and false for the
/// file-level pass. Returns true when any component moved.
#[cfg(test)]
pub fn realign_glyph(font: &Font, glyph: &mut Glyph, seed_own_anchors: bool) -> bool {
    if glyph.components.is_empty() {
        return false;
    }
    let inputs = align_inputs(font, glyph);
    let seed: Vec<(String, Point)> = if seed_own_anchors {
        glyph
            .anchors
            .iter()
            .filter_map(|a| Some((a.name.as_ref()?.to_string(), Point::new(a.x, a.y))))
            .collect()
    } else {
        Vec::new()
    };
    let placed = realign_component_offsets(&inputs, &seed);
    let mut moved = false;
    for (component, offset) in glyph.components.iter_mut().zip(placed) {
        if (component.transform.x_offset - offset.x).abs() > 1e-9
            || (component.transform.y_offset - offset.y).abs() > 1e-9
        {
            component.transform.x_offset = offset.x;
            component.transform.y_offset = offset.y;
            moved = true;
        }
    }
    moved
}

/// The names of every glyph that places `base` as a component.
#[cfg(test)]
pub fn composites_using(font: &Font, base: &str) -> Vec<String> {
    font.iter_layers()
        .next()
        .map(|layer| {
            layer
                .iter()
                .filter(|glyph| glyph.components.iter().any(|c| c.base.as_str() == base))
                .map(|glyph| glyph.name().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use norad::{AffineTransform, Anchor, Name};

    use crate::font::project::{Project, SourceInput};
    use crate::font::variable::{GlyphLayerAddress, SourceId};

    fn glyph_with_anchor(name: &str, anchor: &str, x: f64, y: f64) -> Glyph {
        let mut glyph = Glyph::new(name);
        glyph.anchors.push(Anchor::new(
            x,
            y,
            Some(Name::new(anchor).unwrap()),
            None,
            None,
        ));
        glyph
    }

    fn component(base: &str, x: f64, y: f64) -> Component {
        Component::new(
            Name::new(base).unwrap(),
            AffineTransform {
                x_offset: x,
                y_offset: y,
                ..Default::default()
            },
            None,
        )
    }

    fn mark_font() -> Font {
        let mut font = Font::new();
        let layer = font.default_layer_mut();
        // Base letter: outgoing `top` anchor.
        layer.insert_glyph(glyph_with_anchor("A", "top", 350.0, 700.0));
        // Mark: incoming `_top` plus outgoing `top` for stacking.
        let mut grave = glyph_with_anchor("gravecomb", "_top", 300.0, 720.0);
        grave.anchors.push(Anchor::new(
            300.0,
            920.0,
            Some(Name::new("top").unwrap()),
            None,
            None,
        ));
        layer.insert_glyph(grave);
        let mut agrave = Glyph::new("Agrave");
        agrave.components.push(component("A", 0.0, 0.0));
        agrave.components.push(component("gravecomb", 0.0, 0.0));
        layer.insert_glyph(agrave);
        font
    }

    #[test]
    fn mark_follows_base_anchor_and_stacks() {
        let font = mark_font();
        let mut agrave = font.get_glyph("Agrave").unwrap().clone();
        assert!(realign_glyph(&font, &mut agrave, false));
        // _top (300,720) lands on A's top (350,700): offset (50,-20).
        assert_eq!(agrave.components[1].transform.x_offset, 50.0);
        assert_eq!(agrave.components[1].transform.y_offset, -20.0);
        // Running again is a fixpoint.
        assert!(!realign_glyph(&font, &mut agrave, false));

        // A second mark stacks on the first's outgoing top, not the
        // letter's.
        let mut stacked = agrave.clone();
        stacked.components.push(component("gravecomb", 0.0, 0.0));
        assert!(realign_glyph(&font, &mut stacked, false));
        // First mark's top rides at (300,920) + (50,-20) = (350,900);
        // the second's _top (300,720) aligns there: offset (50,180).
        assert_eq!(stacked.components[2].transform.x_offset, 50.0);
        assert_eq!(stacked.components[2].transform.y_offset, 180.0);
    }

    #[test]
    fn canonical_alignment_matches_legacy_and_preserves_exact_linear_transform() {
        let mut font = mark_font();
        let mark = &mut font
            .default_layer_mut()
            .get_glyph_mut("Agrave")
            .unwrap()
            .components[1];
        mark.transform.x_scale = 1.25;
        mark.transform.xy_scale = 0.125;
        mark.transform.yx_scale = -0.25;
        mark.transform.y_scale = 0.875;
        let project = Project::from_source(SourceInput::from_font(
            font,
            PathBuf::from("CanonicalAlignment.ufo"),
        ));
        let source = SourceId(0);
        let layer = project.document_source(source).unwrap().default_layer();
        let address = GlyphLayerAddress {
            glyph: "Agrave".into(),
            layer: layer.clone(),
        };
        let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
        let moved = realign_document_layer(
            transaction.draft_mut(),
            |name| project.document_layer(name, &layer),
            false,
        )
        .unwrap();
        assert!(moved);
        let component = transaction.draft().view().components().nth(1).unwrap();
        assert_eq!(
            component.transform().as_coeffs(),
            [1.25, 0.125, -0.25, 0.875, 50.0, -20.0]
        );
        assert!(
            !realign_document_layer(
                transaction.draft_mut(),
                |name| project.document_layer(name, &layer),
                false,
            )
            .unwrap()
        );
    }

    #[test]
    fn canonical_effective_anchors_apply_complete_component_transform() {
        let mut font = mark_font();
        let mut inherited = Glyph::new("Inherited");
        inherited.components.push(Component::new(
            Name::new("A").unwrap(),
            AffineTransform {
                x_scale: 2.0,
                xy_scale: 0.25,
                yx_scale: -0.5,
                y_scale: 1.5,
                x_offset: 30.0,
                y_offset: -40.0,
            },
            None,
        ));
        font.default_layer_mut().insert_glyph(inherited);
        let project = Project::from_source(SourceInput::from_font(
            font,
            PathBuf::from("CanonicalAnchors.ufo"),
        ));
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        let inherited = project.document_layer("Inherited", &layer).unwrap();
        assert_eq!(
            effective_document_anchors(inherited, |name| project.document_layer(name, &layer)),
            [("top".into(), Point::new(380.0, 1_097.5))]
        );
    }

    #[test]
    fn canonical_alignment_rejects_nonfinite_results_before_mutation() {
        let mut font = mark_font();
        font.default_layer_mut().get_glyph_mut("A").unwrap().anchors[0].x = f64::NAN;
        let project = Project::from_source(SourceInput::from_font(
            font,
            PathBuf::from("InvalidCanonicalAlignment.ufo"),
        ));
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        let address = GlyphLayerAddress {
            glyph: "Agrave".into(),
            layer: layer.clone(),
        };
        let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
        let before: Vec<_> = transaction
            .draft()
            .view()
            .components()
            .map(|component| component.transform())
            .collect();
        assert_eq!(
            realign_document_layer(
                transaction.draft_mut(),
                |name| project.document_layer(name, &layer),
                false,
            ),
            Err(DocumentEditError::NonFinite)
        );
        assert_eq!(
            transaction
                .draft()
                .view()
                .components()
                .map(|component| component.transform())
                .collect::<Vec<_>>(),
            before
        );
    }

    #[test]
    fn disabled_components_stay_put() {
        let font = mark_font();
        let mut agrave = font.get_glyph("Agrave").unwrap().clone();
        set_component_alignment_disabled(&mut agrave.components[1], true);
        assert!(component_alignment_disabled(&agrave.components[1]));
        assert!(!realign_glyph(&font, &mut agrave, false));
        assert_eq!(agrave.components[1].transform.x_offset, 0.0);
        // Re-locking removes the key; realign snaps it home.
        set_component_alignment_disabled(&mut agrave.components[1], false);
        assert!(!component_alignment_disabled(&agrave.components[1]));
        assert!(realign_glyph(&font, &mut agrave, false));
        assert_eq!(agrave.components[1].transform.x_offset, 50.0);
    }

    #[test]
    fn component_alignment_helpers_preserve_exact_noop_spellings_and_unrelated_lib_data() {
        use super::super::model::glyph_metadata::COMPONENT_ALIGNMENT_KEY;

        let mut item = component("A", 0.0, 0.0);
        let mut lib = plist::Dictionary::from_iter([
            (
                String::from(COMPONENT_ALIGNMENT_KEY),
                plist::Value::Boolean(false),
            ),
            (
                String::from("future.key"),
                plist::Value::String("exact".into()),
            ),
        ]);
        item.replace_lib(lib.clone());
        assert!(component_alignment_disabled(&item));
        set_component_alignment_disabled(&mut item, true);
        assert_eq!(item.lib(), Some(&lib));

        set_component_alignment_disabled(&mut item, false);
        lib.remove(COMPONENT_ALIGNMENT_KEY);
        assert_eq!(item.lib(), Some(&lib));
        assert!(!component_alignment_disabled(&item));

        lib.insert(
            COMPONENT_ALIGNMENT_KEY.into(),
            plist::Value::String("future".into()),
        );
        item.replace_lib(lib.clone());
        set_component_alignment_disabled(&mut item, false);
        assert_eq!(item.lib(), Some(&lib));
        set_component_alignment_disabled(&mut item, true);
        assert_eq!(
            item.lib()
                .and_then(|value| value.get(COMPONENT_ALIGNMENT_KEY))
                .and_then(plist::Value::as_signed_integer),
            Some(-1)
        );
        assert_eq!(
            item.lib().and_then(|value| value.get("future.key")),
            Some(&plist::Value::String("exact".into()))
        );
    }

    #[test]
    fn own_anchors_seed_when_asked() {
        let font = mark_font();
        let mut glyph = glyph_with_anchor("dotless", "top", 100.0, 500.0);
        glyph.components.push(component("gravecomb", 0.0, 0.0));
        assert!(realign_glyph(&font, &mut glyph, true));
        assert_eq!(glyph.components[0].transform.x_offset, -200.0);
        assert_eq!(glyph.components[0].transform.y_offset, -220.0);
    }

    #[test]
    fn composites_using_finds_users() {
        let font = mark_font();
        assert_eq!(composites_using(&font, "A"), vec!["Agrave".to_string()]);
        assert_eq!(
            composites_using(&font, "gravecomb"),
            vec!["Agrave".to_string()]
        );
        assert!(composites_using(&font, "Agrave").is_empty());
    }

    #[test]
    fn fixture_agrave_realign_is_a_fixpoint() {
        let font = Font::load(crate::testing::fonts::regular_ufo()).expect("fixture font");
        let mut agrave = font.get_glyph("Agrave").expect("Agrave").clone();
        // A well-formed source is already aligned: realigning must
        // not move anything.
        assert!(!realign_glyph(&font, &mut agrave, false));
    }
}
