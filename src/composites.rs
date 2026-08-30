// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Component anchor alignment: marks placed by `_top`/`top` anchor
//! pairs follow their base, the Glyphs model. A port of the alignment
//! code in runebender-web's editor.rs and wasm_api.rs.
//!
//! A composite stores its components as fixed offsets, so alignment
//! is not re-derived at render time — it is baked into the file, and
//! this module is what has to run over every glyph that places a base
//! whose anchors just moved.

use kurbo::{Point, Vec2};
use norad::{Component, Font, Glyph};

/// The Glyphs lib key that opts a component out of anchor alignment.
const ALIGNMENT_KEY: &str = "com.glyphsapp.component.alignment";

/// One component's contribution to anchor alignment: the anchors its
/// base glyph carries (in the base's own coordinates), where the
/// component currently sits, and whether it is still anchor-locked.
pub struct AlignInput {
    pub anchors: Vec<(String, Point)>,
    pub offset: Vec2,
    pub aligned: bool,
}

/// Is this component cut loose from its anchor (Glyphs lib key)?
pub fn component_alignment_disabled(component: &Component) -> bool {
    component
        .lib()
        .and_then(|lib| lib.get(ALIGNMENT_KEY))
        .is_some_and(|value| {
            value.as_signed_integer().is_some_and(|value| value < 0)
                || value.as_boolean() == Some(false)
        })
}

/// Lock a component to its anchor or cut it loose. Unlocking writes
/// the Glyphs key and leaves it where it sits; locking removes the
/// key (the caller realigns afterwards to snap it home).
pub fn set_component_alignment_disabled(component: &mut Component, disabled: bool) {
    if disabled {
        let mut lib = component.lib().cloned().unwrap_or_default();
        lib.insert(
            ALIGNMENT_KEY.to_string(),
            plist::Value::Integer((-1).into()),
        );
        component.replace_lib(lib);
    } else if let Some(lib) = component.lib_mut() {
        lib.remove(ALIGNMENT_KEY);
        if lib.is_empty() {
            component.take_lib();
        }
    }
}

/// Re-place anchor-locked components against the anchors in front of
/// them, returning each component's corrected offset. `seed` is the
/// glyph's own anchors (the open-glyph editor offers them; the
/// file-level pass over composites does not).
///
/// Anchors accumulate as we go: a component's outgoing anchors are
/// offered to the components after it, which is how a second mark
/// stacks on the first rather than landing back on the letter.
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

/// Realign one glyph's components in place. `seed_own_anchors` is
/// true for the glyph open in an editor (its own anchors are offered
/// to the components), false for the file-level pass. Returns true
/// when any component moved.
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
    use super::*;
    use norad::{AffineTransform, Anchor, Name};

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
        let font = norad::Font::load(crate::test_fonts::regular_ufo()).expect("fixture font");
        let mut agrave = font.get_glyph("Agrave").expect("Agrave").clone();
        // A well-formed source is already aligned: realigning must
        // not move anything.
        assert!(!realign_glyph(&font, &mut agrave, false));
    }
}
