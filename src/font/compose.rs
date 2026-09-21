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
#[cfg(test)]
use norad::{AffineTransform, Anchor, Component, Font, Glyph, Name};
use serde::{Deserialize, Serialize};

use crate::font::composites::{AlignInput, realign_component_offsets};
use crate::font::project::Project;
#[cfg(test)]
use crate::font::proposal;
use crate::font::proposal::ProposalSummary;
use crate::font::variable::SourceId;
use crate::font::{DocumentEditError, LayerView};

/// The task name, and so the proposal layer's suffix.
pub const TASK: &str = "compose";

/// The glyph lib key that spells a recipe out: `"base + mark + mark"`.
pub use crate::font::model::glyph_metadata::COMPOSITION_RECIPE_KEY as LIB_KEY;

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

/// One canonical proposal payload produced by a composition plan.
///
/// Component and anchor identities are allocated only when a guarded document transaction installs
/// this payload; the plan itself is immutable and does not create a parallel font model.
#[derive(Debug, Clone, PartialEq)]
pub struct CompositionGlyph {
    /// Derived recipe, component placement, advance and foreground comparison.
    pub derived: Derived,
    /// Unicode scalar values retained from the current foreground layer.
    pub codepoints: Vec<char>,
    /// Outgoing anchors offered by the completed component stack.
    pub anchors: Vec<(String, f64, f64)>,
    /// Opaque foreground revision captured while this payload was planned.
    pub expected_revision: String,
}

/// A read-only canonical composition pass ready for a guarded proposal-layer transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct CompositionPlan {
    /// Existing report schema used by GUI, CLI and node callers.
    pub report: Report,
    /// Payloads whose foregrounds are not already current.
    pub replacements: Vec<CompositionGlyph>,
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
#[cfg(test)]
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
#[cfg(test)]
pub fn recipe_for(font: &Font, glyph: &Glyph) -> Option<Recipe> {
    let map = by_codepoint(font);
    recipe_with_map(font, &map, glyph)
}

#[cfg(test)]
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

#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
pub fn derive(font: &Font, name: &str) -> Result<(Glyph, Derived), String> {
    let map = by_codepoint(font);
    derive_with_map(font, &map, name)
}

#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
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

fn document_by_codepoint(layers: &[(String, LayerView<'_>)]) -> HashMap<u32, String> {
    let mut map = HashMap::new();
    for (name, layer) in layers {
        for codepoint in layer.codepoints() {
            map.entry(codepoint as u32).or_insert_with(|| name.clone());
        }
    }
    map
}

fn explicit_recipe(text: Option<&str>, layers: &HashMap<String, LayerView<'_>>) -> Option<Recipe> {
    let parts: Vec<String> = text?
        .split(['+', ' '])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(String::from)
        .collect();
    if parts.len() < 2 || parts.iter().any(|part| !layers.contains_key(part)) {
        return None;
    }
    Some(Recipe {
        base: parts[0].clone(),
        marks: parts[1..].to_vec(),
        source: RecipeSource::Lib,
    })
}

fn document_recipe(
    layers: &HashMap<String, LayerView<'_>>,
    codepoints: &HashMap<u32, String>,
    name: &str,
    explicit: Option<&str>,
) -> Option<Recipe> {
    let layer = layers.get(name)?;
    if let Some(codepoint) = layer.codepoints().next()
        && let Some(recipe) = recipe_from_codepoint(codepoints, codepoint)
        && recipe.base != name
    {
        return Some(recipe);
    }
    if let Some(recipe) = explicit_recipe(explicit, layers) {
        return Some(recipe);
    }
    if let Some((stem, suffix)) = name.split_once('.')
        && let Some(stem_layer) = layers.get(stem)
        && let Some(codepoint) = stem_layer.codepoints().next()
        && let Some(recipe) = recipe_from_codepoint(codepoints, codepoint)
    {
        let base = format!("{}.{suffix}", recipe.base);
        if layers.contains_key(&base) {
            return Some(Recipe {
                base,
                marks: recipe.marks,
                source: RecipeSource::Name,
            });
        }
    }
    None
}

fn document_anchors(layer: LayerView<'_>) -> Vec<(String, Point)> {
    layer
        .anchors()
        .map(|anchor| (anchor.name().to_owned(), anchor.position()))
        .collect()
}

fn place_document(
    layers: &HashMap<String, LayerView<'_>>,
    recipe: &Recipe,
) -> Result<Placement, String> {
    let base = layers
        .get(&recipe.base)
        .ok_or_else(|| format!("no glyph named {}", recipe.base))?;
    let mut inputs = vec![AlignInput {
        anchors: document_anchors(*base),
        offset: Vec2::ZERO,
        aligned: true,
    }];
    for mark in &recipe.marks {
        let layer = layers
            .get(mark)
            .ok_or_else(|| format!("no glyph named {mark}"))?;
        let anchors = document_anchors(*layer);
        if !anchors.iter().any(|(name, _)| name.starts_with('_')) {
            return Err(format!("{mark} has no _anchor to attach by"));
        }
        inputs.push(AlignInput {
            anchors,
            offset: Vec2::ZERO,
            aligned: true,
        });
    }
    let mut offered: Vec<String> = inputs[0]
        .anchors
        .iter()
        .filter(|(name, _)| !name.starts_with('_'))
        .map(|(name, _)| name.clone())
        .collect();
    for (input, mark) in inputs[1..].iter().zip(&recipe.marks) {
        let attaches = input
            .anchors
            .iter()
            .filter_map(|(name, _)| name.strip_prefix('_'))
            .any(|target| offered.iter().any(|name| name == target));
        if !attaches {
            let wants: Vec<_> = input
                .anchors
                .iter()
                .filter_map(|(name, _)| name.strip_prefix('_'))
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
                .filter(|(name, _)| !name.starts_with('_'))
                .map(|(name, _)| name.clone()),
        );
    }
    let offsets = realign_component_offsets(&inputs, &[]);
    let names = std::iter::once(&recipe.base).chain(&recipe.marks);
    let mut placed = Vec::new();
    let mut anchors = Vec::new();
    for ((input, offset), name) in inputs.iter().zip(offsets).zip(names) {
        placed.push((name.clone(), offset));
        for (anchor, point) in &input.anchors {
            if !anchor.starts_with('_') {
                anchors.retain(|(candidate, _)| candidate != anchor);
                anchors.push((anchor.clone(), *point + offset));
            }
        }
    }
    Ok((placed, anchors))
}

fn derive_document_with_map(
    layers: &HashMap<String, LayerView<'_>>,
    codepoints: &HashMap<u32, String>,
    name: &str,
    explicit: Option<&str>,
) -> Result<CompositionGlyph, String> {
    let current = layers
        .get(name)
        .ok_or_else(|| format!("no glyph named {name}"))?;
    let expected_revision = crate::font::edit_batch::canonical_glyph_revision(*current)?;
    let recipe = document_recipe(layers, codepoints, name, explicit)
        .ok_or_else(|| "no recipe: no decomposition, lib key, or positional stem".to_string())?;
    if recipe.base == name || recipe.marks.iter().any(|mark| mark == name) {
        return Err("the recipe names the glyph itself".into());
    }
    let (placed, anchors) = place_document(layers, &recipe)?;
    let advance = layers
        .get(&recipe.base)
        .ok_or_else(|| format!("no glyph named {}", recipe.base))?
        .width();
    let components: Vec<_> = current.components().collect();
    let same_glyph =
        |a: &str, b: &str| a == b || a == format!("{b}comb") || b == format!("{a}comb");
    let up_to_date = components.len() == placed.len()
        && (current.width() - advance).abs() < 0.5
        && components
            .iter()
            .zip(&placed)
            .all(|(component, (name, offset))| {
                let coefficients = component.transform().as_coeffs();
                same_glyph(component.reference(), name)
                    && (coefficients[4] - offset.x).abs() < 0.5
                    && (coefficients[5] - offset.y).abs() < 0.5
            });
    let derived = Derived {
        glyph: name.to_owned(),
        recipe,
        components: placed
            .iter()
            .map(|(name, offset)| (name.clone(), offset.x, offset.y))
            .collect(),
        advance,
        up_to_date,
    };
    Ok(CompositionGlyph {
        derived,
        codepoints: current.codepoints().collect(),
        anchors: anchors
            .into_iter()
            .map(|(name, point)| (name, point.x, point.y))
            .collect(),
        expected_revision,
    })
}

/// Plan composition directly from canonical document layers without constructing a UFO font.
///
/// `explicit_recipe` supplies the already decoded `com.runebender.compose` value for a glyph.
/// The returned payloads are immutable and must be installed through a guarded Project proposal
/// transaction; this function never mutates the document.
pub fn plan_document<'a, 'recipe>(
    layers: impl IntoIterator<Item = LayerView<'a>>,
    names: Option<&[String]>,
    mut explicit_recipe: impl FnMut(&str) -> Option<&'recipe str>,
) -> CompositionPlan {
    let ordered_layers: Vec<_> = layers
        .into_iter()
        .map(|layer| (layer.glyph_name().to_owned(), layer))
        .collect();
    let codepoints = document_by_codepoint(&ordered_layers);
    let layers: HashMap<_, _> = ordered_layers.iter().cloned().collect();
    let explicit_recipes: HashMap<_, _> = ordered_layers
        .iter()
        .map(|(name, _)| (name.clone(), explicit_recipe(name).map(ToOwned::to_owned)))
        .collect();
    let wanted = names.map_or_else(
        || {
            ordered_layers
                .iter()
                .filter(|(name, _)| {
                    document_recipe(
                        &layers,
                        &codepoints,
                        name,
                        explicit_recipes
                            .get(name)
                            .and_then(|recipe| recipe.as_deref()),
                    )
                    .is_some_and(|recipe| recipe.base != name.as_str())
                })
                .map(|(name, _)| name.clone())
                .collect()
        },
        <[String]>::to_vec,
    );
    let mut derived = Vec::new();
    let mut skipped = Vec::new();
    let mut replacements = Vec::new();
    for name in wanted {
        match derive_document_with_map(
            &layers,
            &codepoints,
            &name,
            explicit_recipes
                .get(&name)
                .and_then(|recipe| recipe.as_deref()),
        ) {
            Ok(glyph) => {
                if !glyph.derived.up_to_date {
                    replacements.push(glyph.clone());
                }
                derived.push(glyph.derived);
            }
            Err(why) => skipped.push((name, why)),
        }
    }
    CompositionPlan {
        report: Report {
            derived,
            skipped,
            proposal: None,
        },
        replacements,
    }
}

/// Plan composition for one canonical Project source.
///
/// Explicit recipes are read from their typed layer owner before planning, so malformed source
/// metadata rejects the complete read-only pass instead of being silently ignored.
pub fn plan_project(
    project: &Project,
    source: SourceId,
    names: Option<&[String]>,
) -> Result<CompositionPlan, DocumentEditError> {
    let layer = project
        .document_source(source)
        .ok_or(DocumentEditError::MissingSource)?
        .default_layer();
    let recipes = project
        .glyph_names()
        .filter_map(|name| {
            project
                .document_layer(name, &layer)
                .map(|layer| (name, layer))
        })
        .map(|(name, layer)| {
            Ok((
                name.to_owned(),
                layer.composition_recipe_source()?.map(ToOwned::to_owned),
            ))
        })
        .collect::<Result<HashMap<_, _>, DocumentEditError>>()?;
    Ok(plan_document(
        project
            .glyph_names()
            .filter_map(|name| project.document_layer(name, &layer)),
        names,
        |name| recipes.get(name).and_then(|recipe| recipe.as_deref()),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::*;

    use crate::font::project::{Project, SourceInput};
    use crate::font::variable::SourceId;

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
    fn canonical_plan_matches_the_legacy_derived_payload() {
        let font = latin();
        let (glyph, expected) = derive(&font, "Aacute").unwrap();
        let project = Project::from_source(SourceInput::from_font(
            font,
            PathBuf::from("CanonicalCompose.ufo"),
        ));
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        let plan = plan_document(
            project
                .glyph_names()
                .filter_map(|name| project.document_layer(name, &layer)),
            Some(&["Aacute".into()]),
            |_| None,
        );
        assert_eq!(
            plan_project(&project, SourceId(0), Some(&["Aacute".into()])).unwrap(),
            plan
        );
        assert!(plan.report.skipped.is_empty());
        assert_eq!(
            plan.report.derived.as_slice(),
            std::slice::from_ref(&expected)
        );
        assert_eq!(plan.replacements.len(), 1);
        assert_eq!(plan.replacements[0].derived, expected);
        assert_eq!(
            plan.replacements[0].anchors,
            glyph
                .anchors
                .iter()
                .map(|anchor| {
                    (
                        anchor.name.as_ref().unwrap().to_string(),
                        anchor.x,
                        anchor.y,
                    )
                })
                .collect::<Vec<_>>()
        );
        assert_eq!(
            plan.replacements[0].codepoints,
            glyph.codepoints.iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn canonical_duplicate_codepoint_resolution_preserves_glyph_order() {
        let mut font = latin();
        font.default_layer_mut().insert_glyph(glyph(
            "A.alternate",
            900.0,
            Some('A'),
            &[("top", 450.0, 1_000.0)],
        ));
        let (_, expected) = derive(&font, "Aacute").unwrap();
        assert_eq!(expected.recipe.base, "A");
        assert_eq!(expected.advance, 700.0);
        let project = Project::from_source(SourceInput::from_font(
            font,
            PathBuf::from("DuplicateUnicodeCompose.ufo"),
        ));
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        for _ in 0..32 {
            let plan = plan_document(
                project
                    .glyph_names()
                    .filter_map(|name| project.document_layer(name, &layer)),
                Some(&["Aacute".into()]),
                |_| None,
            );
            assert_eq!(
                plan.report.derived.as_slice(),
                std::slice::from_ref(&expected)
            );
        }
    }

    #[test]
    fn project_plan_rejects_invalid_typed_recipe_metadata() {
        let mut font = latin();
        font.default_layer_mut()
            .get_glyph_mut("Aacute")
            .unwrap()
            .lib
            .insert(LIB_KEY.into(), plist::Value::Integer(7.into()));
        let project = Project::from_source(SourceInput::from_font(
            font,
            PathBuf::from("InvalidCanonicalCompose.ufo"),
        ));
        assert_eq!(
            plan_project(&project, SourceId(0), Some(&["Aacute".into()])),
            Err(DocumentEditError::InvalidLayerMetadata)
        );
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

        let mut recipes = HashMap::new();
        recipes.insert("Aacute.alt", "A + acute");
        let project = Project::from_source(SourceInput::from_font(
            font,
            PathBuf::from("CanonicalExplicitCompose.ufo"),
        ));
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        let plan = plan_document(
            project
                .glyph_names()
                .filter_map(|name| project.document_layer(name, &layer)),
            Some(&["Aacute.alt".into()]),
            |name| recipes.get(name).copied(),
        );
        assert_eq!(plan.report.derived[0].recipe.source, RecipeSource::Lib);
        assert_eq!(plan.report.derived[0].recipe.base, "A");
        assert_eq!(plan.report.derived[0].recipe.marks, ["acute"]);
        assert_eq!(
            plan_project(&project, SourceId(0), Some(&["Aacute.alt".into()]),).unwrap(),
            plan
        );
    }
}
