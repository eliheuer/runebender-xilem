// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Composition from anchors: a precomposed glyph derived from its base
//! and marks.
//!
//! Draw `alef-ar` once, draw `hamzaabove-ar` once, put a `top` anchor
//! on the letter and a `_top` anchor on the mark, and `alefHamzaabove-ar`
//! follows: the base as a component, the mark as a component placed
//! by anchor arithmetic, the base's advance. Edit either and the
//! composite re-derives. That is what Glyphs and Counterpunch call
//! composition-first, and here it is a proposal like everything else:
//! the derived glyphs land in the `com.runebender.proposal.compose`
//! layer and the designer installs or discards them.
//!
//! A recipe (which base, which marks) comes from three places, in
//! order: the glyph's Unicode canonical decomposition, an explicit
//! `com.runebender.compose` key in the glyph's lib, and the glyph's
//! name when it is a positional form of a glyph that decomposes
//! (`alefHamzaabove-ar.fina` is `alef-ar.fina` plus `hamzaabove-ar`).
//! Nothing is guessed from a name alone.

use std::collections::HashMap;

use kurbo::{Point, Vec2};
use norad::{AffineTransform, Anchor, Component, Font, Glyph, Name};
use serde::{Deserialize, Serialize};

use crate::document::composites::{AlignInput, realign_component_offsets};
use crate::document::proposal::{self, ProposalSummary};

/// The task name, and so the proposal layer's suffix.
pub const TASK: &str = "compose";

/// The glyph lib key that spells a recipe out: `"base + mark + mark"`.
pub const LIB_KEY: &str = "com.runebender.compose";

/// Where a recipe came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecipeSource {
    /// The glyph's codepoint decomposes canonically.
    Unicode,
    /// The glyph's lib says so.
    Lib,
    /// The glyph is a positional form of one that decomposes.
    Name,
}

/// What a glyph is made of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Recipe {
    /// The base glyph.
    pub base: String,
    /// The marks, in stacking order.
    pub marks: Vec<String>,
    /// Where it came from.
    pub source: RecipeSource,
}

/// One derived glyph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Derived {
    /// The glyph.
    pub glyph: String,
    /// Its recipe.
    pub recipe: Recipe,
    /// Each component and its offset, base first.
    pub components: Vec<(String, f64, f64)>,
    /// The advance, which is the base's.
    pub advance: f64,
    /// True when the foreground already has these components at these
    /// offsets, so nothing was proposed.
    pub up_to_date: bool,
}

/// What a compose pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Report {
    /// Glyphs derived, whether proposed or already current.
    pub derived: Vec<Derived>,
    /// Glyphs asked for that could not be derived, with why.
    pub skipped: Vec<(String, String)>,
    /// The proposal written, when `write` was on and anything changed.
    pub proposal: Option<ProposalSummary>,
}

impl Report {
    /// The glyphs that were proposed, not already current.
    pub fn proposed(&self) -> Vec<&str> {
        self.derived
            .iter()
            .filter(|d| !d.up_to_date)
            .map(|d| d.glyph.as_str())
            .collect()
    }
}

/// Combining marks that fonts usually draw as their spacing cousins.
/// When a font has no glyph for the combining codepoint, the spacing
/// one stands in, which is what every Latin font with `acute` and no
/// `acutecomb` expects.
const SPACING_FALLBACK: &[(u32, u32)] = &[
    (0x0300, 0x0060), // grave
    (0x0301, 0x00B4), // acute
    (0x0302, 0x02C6), // circumflex
    (0x0303, 0x02DC), // tilde
    (0x0304, 0x00AF), // macron
    (0x0306, 0x02D8), // breve
    (0x0307, 0x02D9), // dotaccent
    (0x0308, 0x00A8), // dieresis
    (0x030A, 0x02DA), // ring
    (0x030B, 0x02DD), // hungarumlaut
    (0x030C, 0x02C7), // caron
    (0x0327, 0x00B8), // cedilla
    (0x0328, 0x02DB), // ogonek
];

/// Glyph names by codepoint, over the foreground. The first glyph
/// that carries a codepoint wins.
fn by_codepoint(font: &Font) -> HashMap<u32, String> {
    let mut map = HashMap::new();
    for glyph in font.default_layer().iter() {
        for cp in glyph.codepoints.iter() {
            map.entry(cp as u32)
                .or_insert_with(|| glyph.name().to_string());
        }
    }
    map
}

/// The glyph for a codepoint, or its spacing stand-in.
fn glyph_for(map: &HashMap<u32, String>, cp: u32) -> Option<String> {
    if let Some(name) = map.get(&cp) {
        return Some(name.clone());
    }
    let (_, spacing) = SPACING_FALLBACK.iter().find(|(c, _)| *c == cp)?;
    map.get(spacing).cloned()
}

/// The recipe a codepoint's canonical decomposition gives, when it
/// decomposes into a base and at least one mark the font has.
fn recipe_from_codepoint(map: &HashMap<u32, String>, cp: char) -> Option<Recipe> {
    let mut parts: Vec<char> = Vec::new();
    unicode_normalization::char::decompose_canonical(cp, |c| parts.push(c));
    if parts.len() < 2 {
        return None;
    }
    let base = glyph_for(map, parts[0] as u32)?;
    let marks = parts[1..]
        .iter()
        .map(|c| glyph_for(map, *c as u32))
        .collect::<Option<Vec<String>>>()?;
    Some(Recipe {
        base,
        marks,
        source: RecipeSource::Unicode,
    })
}

/// The recipe for a glyph, from whichever source has one.
pub fn recipe_for(font: &Font, glyph: &Glyph) -> Option<Recipe> {
    let map = by_codepoint(font);
    recipe_with_map(font, &map, glyph)
}

fn recipe_with_map(font: &Font, map: &HashMap<u32, String>, glyph: &Glyph) -> Option<Recipe> {
    let name = glyph.name().to_string();
    // 1. Unicode.
    if let Some(cp) = glyph.codepoints.iter().next()
        && let Some(r) = recipe_from_codepoint(map, cp)
        && r.base != name
    {
        return Some(r);
    }
    // 2. The lib key.
    if let Some(text) = glyph.lib.get(LIB_KEY).and_then(|v| v.as_string()) {
        let parts: Vec<String> = text
            .split(['+', ' '])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        if parts.len() >= 2 && parts.iter().all(|p| font.get_glyph(p.as_str()).is_some()) {
            return Some(Recipe {
                base: parts[0].clone(),
                marks: parts[1..].to_vec(),
                source: RecipeSource::Lib,
            });
        }
    }
    // 3. A positional form of a glyph that decomposes: the stem's
    // recipe with the same suffix on the base, when that base exists.
    if let Some((stem, suffix)) = name.split_once('.')
        && let Some(stem_glyph) = font.get_glyph(stem)
        && let Some(cp) = stem_glyph.codepoints.iter().next()
        && let Some(r) = recipe_from_codepoint(map, cp)
    {
        let base = format!("{}.{suffix}", r.base);
        if font.get_glyph(base.as_str()).is_some() {
            return Some(Recipe {
                base,
                marks: r.marks,
                source: RecipeSource::Name,
            });
        }
    }
    None
}

fn anchors_of(glyph: &Glyph) -> Vec<(String, Point)> {
    glyph
        .anchors
        .iter()
        .filter_map(|a| Some((a.name.as_ref()?.to_string(), Point::new(a.x, a.y))))
        .collect()
}

/// The placed components, base first, and the anchors the result
/// offers on.
type Placement = (Vec<(String, Vec2)>, Vec<(String, Point)>);

/// Places a recipe: the base at the origin, each mark by its `_name`
/// anchor onto the nearest `name` anchor offered so far (the base's,
/// or an earlier mark's, which is how marks stack). Returns the
/// placed components and the anchors the result offers on.
fn place(font: &Font, recipe: &Recipe) -> Result<Placement, String> {
    let base = font
        .get_glyph(recipe.base.as_str())
        .ok_or_else(|| format!("no glyph named {}", recipe.base))?;
    let mut inputs = vec![AlignInput {
        anchors: anchors_of(base),
        offset: Vec2::ZERO,
        aligned: true,
    }];
    for mark in &recipe.marks {
        let glyph = font
            .get_glyph(mark.as_str())
            .ok_or_else(|| format!("no glyph named {mark}"))?;
        let anchors = anchors_of(glyph);
        if !anchors.iter().any(|(n, _)| n.starts_with('_')) {
            return Err(format!("{mark} has no _anchor to attach by"));
        }
        inputs.push(AlignInput {
            anchors,
            offset: Vec2::ZERO,
            aligned: true,
        });
    }
    // Every mark must find a partner; the walk itself is silent about
    // a miss, so check as it would.
    let mut offered: Vec<String> = inputs[0]
        .anchors
        .iter()
        .filter(|(n, _)| !n.starts_with('_'))
        .map(|(n, _)| n.clone())
        .collect();
    for (input, mark) in inputs[1..].iter().zip(&recipe.marks) {
        let attaches = input
            .anchors
            .iter()
            .filter_map(|(n, _)| n.strip_prefix('_'))
            .any(|target| offered.iter().any(|o| o == target));
        if !attaches {
            let wants: Vec<&str> = input
                .anchors
                .iter()
                .filter_map(|(n, _)| n.strip_prefix('_'))
                .collect();
            return Err(format!(
                "{mark} attaches by {} and nothing before it offers that",
                wants.join(" or ")
            ));
        }
        offered.extend(
            input
                .anchors
                .iter()
                .filter(|(n, _)| !n.starts_with('_'))
                .map(|(n, _)| n.clone()),
        );
    }
    let offsets = realign_component_offsets(&inputs, &[]);
    let mut placed = Vec::new();
    let mut names = std::iter::once(&recipe.base).chain(&recipe.marks);
    let mut out_anchors: Vec<(String, Point)> = Vec::new();
    for (input, offset) in inputs.iter().zip(offsets) {
        let name = names.next().cloned().unwrap_or_default();
        placed.push((name, offset));
        for (n, p) in &input.anchors {
            if !n.starts_with('_') {
                // A later anchor of the same name replaces an earlier
                // one: the top of the stack is the new top.
                out_anchors.retain(|(o, _)| o != n);
                out_anchors.push((n.clone(), *p + offset));
            }
        }
    }
    Ok((placed, out_anchors))
}

/// Derives one glyph. Err names the reason it cannot be.
pub fn derive(font: &Font, name: &str) -> Result<(Glyph, Derived), String> {
    let map = by_codepoint(font);
    derive_with_map(font, &map, name)
}

fn derive_with_map(
    font: &Font,
    map: &HashMap<u32, String>,
    name: &str,
) -> Result<(Glyph, Derived), String> {
    let current = font
        .get_glyph(name)
        .ok_or_else(|| format!("no glyph named {name}"))?;
    let recipe = recipe_with_map(font, map, current)
        .ok_or_else(|| "no recipe: no decomposition, lib key, or positional stem".to_string())?;
    if recipe.base == name || recipe.marks.iter().any(|m| m == name) {
        return Err("the recipe names the glyph itself".into());
    }
    let (placed, anchors) = place(font, &recipe)?;
    let base = font
        .get_glyph(recipe.base.as_str())
        .ok_or_else(|| format!("no glyph named {}", recipe.base))?;
    let advance = base.width;

    // The foreground is current when it places the same glyphs at
    // the same offsets. A spacing accent stands for its combining
    // twin here (`acute` for `acutecomb`): fonts build composites
    // from either, and the position is what matters.
    let same_glyph =
        |a: &str, b: &str| a == b || a == format!("{b}comb") || b == format!("{a}comb");
    let up_to_date = current.components.len() == placed.len()
        && (current.width - advance).abs() < 0.5
        && current.components.iter().zip(&placed).all(|(c, (n, o))| {
            same_glyph(c.base.as_str(), n)
                && (c.transform.x_offset - o.x).abs() < 0.5
                && (c.transform.y_offset - o.y).abs() < 0.5
        });

    let mut glyph = Glyph::new(name);
    glyph.width = advance;
    for cp in current.codepoints.iter() {
        glyph.codepoints.insert(cp);
    }
    for (base_name, offset) in &placed {
        let base = Name::new(base_name).map_err(|e| format!("{base_name}: {e}"))?;
        let transform = AffineTransform {
            x_offset: offset.x,
            y_offset: offset.y,
            ..AffineTransform::default()
        };
        glyph.components.push(Component::new(base, transform, None));
    }
    for (anchor, p) in &anchors {
        let anchor = Name::new(anchor).map_err(|e| format!("{anchor}: {e}"))?;
        glyph
            .anchors
            .push(Anchor::new(p.x, p.y, Some(anchor), None, None));
    }
    let derived = Derived {
        glyph: name.to_string(),
        recipe,
        components: placed.iter().map(|(n, o)| (n.clone(), o.x, o.y)).collect(),
        advance,
        up_to_date,
    };
    Ok((glyph, derived))
}

/// Every glyph in the foreground that has a recipe.
pub fn composable(font: &Font) -> Vec<String> {
    let map = by_codepoint(font);
    font.default_layer()
        .iter()
        .filter(|g| recipe_with_map(font, &map, g).is_some_and(|r| r.base != g.name().as_str()))
        .map(|g| g.name().to_string())
        .collect()
}

/// Every composable glyph whose recipe uses `glyph` as its base or
/// one of its marks: what has to re-derive when `glyph` changes.
pub fn dependents(font: &Font, glyph: &str) -> Vec<String> {
    let map = by_codepoint(font);
    font.default_layer()
        .iter()
        .filter(|g| {
            recipe_with_map(font, &map, g)
                .is_some_and(|r| r.base == glyph || r.marks.iter().any(|m| m == glyph))
        })
        .map(|g| g.name().to_string())
        .collect()
}

/// Derives `names` (or every composable glyph when None) and, with
/// `write`, puts the ones that differ from the foreground into the
/// proposal layer. Glyphs already current are reported and not
/// proposed.
pub fn compose(font: &mut Font, names: Option<&[String]>, write: bool) -> Report {
    let wanted: Vec<String> = match names {
        Some(list) => list.to_vec(),
        None => composable(font),
    };
    let map = by_codepoint(font);
    let mut derived = Vec::new();
    let mut skipped = Vec::new();
    let mut to_write = Vec::new();
    for name in wanted {
        match derive_with_map(font, &map, &name) {
            Ok((glyph, d)) => {
                if !d.up_to_date {
                    to_write.push(glyph);
                }
                derived.push(d);
            }
            Err(why) => skipped.push((name, why)),
        }
    }
    let proposal = if write && !to_write.is_empty() {
        proposal::write(font, TASK, to_write).ok()
    } else {
        None
    };
    Report {
        derived,
        skipped,
        proposal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(name: &str, x: f64, y: f64) -> Anchor {
        Anchor::new(x, y, Some(Name::new(name).unwrap()), None, None)
    }

    fn glyph(name: &str, width: f64, cp: Option<char>, anchors: &[(&str, f64, f64)]) -> Glyph {
        let mut g = Glyph::new(name);
        g.width = width;
        if let Some(c) = cp {
            g.codepoints.insert(c);
        }
        for (n, x, y) in anchors {
            g.anchors.push(anchor(n, *x, *y));
        }
        g
    }

    /// A, acute (spacing, standing in for U+0301), and Aacute drawn
    /// as an empty glyph to be derived.
    fn latin() -> Font {
        let mut font = Font::new();
        let layer = font.default_layer_mut();
        layer.insert_glyph(glyph("A", 700.0, Some('A'), &[("top", 350.0, 700.0)]));
        layer.insert_glyph(glyph(
            "acute",
            300.0,
            Some('\u{00B4}'),
            &[("_top", 150.0, 560.0), ("top", 150.0, 760.0)],
        ));
        layer.insert_glyph(glyph("Aacute", 0.0, Some('\u{00C1}'), &[]));
        font
    }

    #[test]
    fn a_latin_accent_derives_from_the_decomposition() {
        let font = latin();
        let (g, d) = derive(&font, "Aacute").unwrap();
        assert_eq!(d.recipe.source, RecipeSource::Unicode);
        assert_eq!(d.recipe.base, "A");
        assert_eq!(d.recipe.marks, ["acute"]);
        assert_eq!(
            d.components,
            vec![("A".into(), 0.0, 0.0), ("acute".into(), 200.0, 140.0)]
        );
        assert_eq!(d.advance, 700.0);
        assert!(!d.up_to_date);
        assert_eq!(g.components.len(), 2);
        // The stack's top is the accent's top, moved with it.
        let top = g
            .anchors
            .iter()
            .find(|a| a.name.as_deref() == Some("top"))
            .unwrap();
        assert_eq!((top.x, top.y), (350.0, 900.0));
    }

    #[test]
    fn a_positional_form_takes_the_stem_recipe_with_its_suffix() {
        let mut font = Font::new();
        let layer = font.default_layer_mut();
        layer.insert_glyph(glyph(
            "alef-ar",
            224.0,
            Some('\u{0627}'),
            &[("top", 112.0, 800.0)],
        ));
        layer.insert_glyph(glyph("alef-ar.fina", 256.0, None, &[("top", 128.0, 800.0)]));
        layer.insert_glyph(glyph(
            "hamzaabove-ar",
            0.0,
            Some('\u{0654}'),
            &[("_top", 112.0, 768.0)],
        ));
        layer.insert_glyph(glyph("alefHamzaabove-ar", 0.0, Some('\u{0623}'), &[]));
        layer.insert_glyph(glyph("alefHamzaabove-ar.fina", 0.0, None, &[]));
        let (_, d) = derive(&font, "alefHamzaabove-ar.fina").unwrap();
        assert_eq!(d.recipe.source, RecipeSource::Name);
        assert_eq!(d.recipe.base, "alef-ar.fina");
        assert_eq!(d.components[1], ("hamzaabove-ar".into(), 16.0, 32.0));
        assert_eq!(d.advance, 256.0);
        assert_eq!(dependents(&font, "hamzaabove-ar").len(), 2);
    }

    #[test]
    fn a_current_composite_is_up_to_date_and_a_missing_anchor_is_named() {
        let mut font = latin();
        let (g, _) = derive(&font, "Aacute").unwrap();
        font.default_layer_mut().insert_glyph(g);
        let (_, d) = derive(&font, "Aacute").unwrap();
        assert!(d.up_to_date);
        // Take the base's anchor away: the mark has nothing to hold.
        font.default_layer_mut()
            .get_glyph_mut("A")
            .unwrap()
            .anchors
            .clear();
        let err = derive(&font, "Aacute").unwrap_err();
        assert!(err.contains("attaches by top"), "{err}");
    }

    #[test]
    fn compose_writes_only_what_changed() {
        let mut font = latin();
        let report = compose(&mut font, None, true);
        assert_eq!(report.proposed(), ["Aacute"]);
        let p = report.proposal.unwrap();
        assert_eq!(p.task, "compose");
        assert_eq!(p.glyphs, ["Aacute"]);
        // Install it, and the next pass has nothing to say.
        let mut before = |_: &str, _: &Glyph| {};
        proposal::install(&mut font, TASK, None, false, &mut before).unwrap();
        let again = compose(&mut font, None, true);
        assert!(again.proposed().is_empty());
        assert!(again.proposal.is_none());
    }

    #[test]
    fn the_lib_key_spells_a_recipe() {
        let mut font = latin();
        let mut g = glyph("Aacute.alt", 0.0, None, &[]);
        g.lib
            .insert(LIB_KEY.into(), plist::Value::String("A + acute".into()));
        font.default_layer_mut().insert_glyph(g);
        let (_, d) = derive(&font, "Aacute.alt").unwrap();
        assert_eq!(d.recipe.source, RecipeSource::Lib);
    }
}
