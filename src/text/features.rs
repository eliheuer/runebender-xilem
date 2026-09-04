// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Mark positioning features written from the font's anchors.
//!
//! A font with `top` anchors on its bases and `_top` anchors on its
//! marks has said everything a `mark` lookup needs; fontmake and
//! fontc write that lookup for it at compile time. The editor shapes
//! a font compiled on the fly from `features.fea` alone, so a mark
//! typed after a base sat on the baseline. This writes the same
//! features, in the same shape as fontc's mark feature writer, so the
//! shaped preview matches the built font:
//!
//! - one `markClass` per mark and anchor name (`_top` puts the mark in
//!   `@MC_top`);
//! - `feature mark`, one lookup per anchor name, positioning every
//!   base that carries the anchor;
//! - `feature mkmk`, one lookup per anchor name, positioning every
//!   mark that carries the anchor as well as an `_anchor`, with a
//!   mark filtering set so a stack attaches through its own class.
//!
//! A composite with no anchors of its own takes its components'
//! anchors, translated, the way fontc propagates them; so `beh-ar`,
//! built from `behDotless-ar` and a dot, still offers `top`.
//!
//! Not written: `abvm` and `blwm` for Indic scripts, and ligature
//! component anchors (`top_1`, `top_2`). Both are gaps to close when a
//! font needs them. A `features.fea` that already defines `mark` or
//! `mkmk` wins, and nothing is added.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use norad::{Font, Glyph};

use crate::outline::glyph_paths::round_units;

/// The line `features.fea` gets so a compiled font positions marks
/// the same way the editor does.
pub const INCLUDE_LINE: &str = "include(features.generated.fea);";

/// The file the generated features are written to, beside
/// `features.fea` in the UFO.
pub const GENERATED_FILE: &str = "features.generated.fea";

/// What was written, and how much.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generated {
    /// The feature text. Empty when the font has no anchors that pair.
    pub fea: String,
    /// Anchor names that got a lookup.
    pub classes: Vec<String>,
    /// Marks in any class.
    pub marks: usize,
    /// Bases positioned in `mark`.
    pub bases: usize,
    /// Marks positioned in `mkmk`.
    pub stacked: usize,
}

impl Generated {
    /// True when there is nothing to add.
    pub fn is_empty(&self) -> bool {
        self.fea.is_empty()
    }
}

/// A glyph's anchors as the features see them: its own, or its
/// components' when it has none of its own, translated by the
/// component offsets. A later component's anchor replaces an earlier
/// one of the same name, so a dot placed on a base hands on its own
/// `bottom`. Attaching anchors (`_top`) never propagate: a composite
/// is not a mark because it holds one.
pub fn anchors(font: &Font, glyph: &Glyph) -> Vec<(String, f64, f64)> {
    effective_anchors(font, glyph, 0)
}

fn effective_anchors(font: &Font, glyph: &Glyph, depth: usize) -> Vec<(String, f64, f64)> {
    let own: Vec<(String, f64, f64)> = glyph
        .anchors
        .iter()
        .filter_map(|a| a.name.as_ref().map(|n| (n.to_string(), a.x, a.y)))
        .collect();
    if !own.is_empty() || depth > 8 {
        return own;
    }
    let mut out: Vec<(String, f64, f64)> = Vec::new();
    for component in &glyph.components {
        let Some(base) = font.get_glyph(&component.base) else {
            continue;
        };
        let t = component.transform;
        for (name, x, y) in effective_anchors(font, base, depth + 1) {
            if name.starts_with('_') {
                continue;
            }
            let (px, py) = (
                t.x_scale * x + t.yx_scale * y + t.x_offset,
                t.xy_scale * x + t.y_scale * y + t.y_offset,
            );
            out.retain(|(n, _, _)| *n != name);
            out.push((name, px, py));
        }
    }
    out
}

/// The feature text for a font, deterministic: glyphs and classes in
/// name order, coordinates rounded to units.
pub fn generate(font: &Font) -> Generated {
    // Per anchor name: the marks that attach by it, with their
    // `_name` anchor, and the bases and marks that offer it.
    let mut attach: BTreeMap<String, BTreeMap<String, (i64, i64)>> = BTreeMap::new();
    let mut offer_base: BTreeMap<String, BTreeMap<String, (i64, i64)>> = BTreeMap::new();
    let mut offer_mark: BTreeMap<String, BTreeMap<String, (i64, i64)>> = BTreeMap::new();
    let mut is_mark: BTreeSet<String> = BTreeSet::new();
    let round = |v: f64| round_units(v);

    let mut glyphs: Vec<&Glyph> = font.default_layer().iter().collect();
    glyphs.sort_by(|a, b| a.name().cmp(b.name()));
    for glyph in &glyphs {
        let anchors = effective_anchors(font, glyph, 0);
        if anchors.iter().any(|(n, _, _)| n.starts_with('_')) {
            is_mark.insert(glyph.name().to_string());
        }
    }
    for glyph in &glyphs {
        let name = glyph.name().to_string();
        for (anchor, x, y) in effective_anchors(font, glyph, 0) {
            let at = (round(x), round(y));
            if let Some(class) = anchor.strip_prefix('_') {
                if class.is_empty() {
                    continue;
                }
                attach
                    .entry(class.to_string())
                    .or_default()
                    .insert(name.clone(), at);
            } else if is_mark.contains(&name) {
                offer_mark
                    .entry(anchor)
                    .or_default()
                    .insert(name.clone(), at);
            } else {
                offer_base
                    .entry(anchor)
                    .or_default()
                    .insert(name.clone(), at);
            }
        }
    }

    let mut classes = Vec::new();
    let mut mark_lines = String::new();
    let mut base_lookups = String::new();
    let mut mark_lookups = String::new();
    let mut filter_sets = String::new();
    let mut marks_seen: BTreeSet<String> = BTreeSet::new();
    let mut bases = 0;
    let mut stacked = 0;
    for (class, marks) in &attach {
        let base_side = offer_base.get(class);
        let mark_side = offer_mark.get(class);
        if base_side.is_none() && mark_side.is_none() {
            continue;
        }
        classes.push(class.clone());
        for (mark, (x, y)) in marks {
            mark_lines.push_str(&format!("markClass {mark} <anchor {x} {y}> @MC_{class};\n"));
            marks_seen.insert(mark.clone());
        }
        if let Some(bases_here) = base_side {
            base_lookups.push_str(&format!("    lookup mark2base_{class} {{\n"));
            for (base, (x, y)) in bases_here {
                base_lookups.push_str(&format!(
                    "        pos base {base} <anchor {x} {y}> mark @MC_{class};\n"
                ));
                bases += 1;
            }
            base_lookups.push_str(&format!("    }} mark2base_{class};\n"));
        }
        if let Some(marks_here) = mark_side {
            // The filtering set is every mark in play for this class:
            // the ones that attach and the ones that carry the anchor,
            // so a stack skips nothing of its own and everything else.
            let mut set: BTreeSet<&str> = marks.keys().map(String::as_str).collect();
            set.extend(marks_here.keys().map(String::as_str));
            let names: Vec<&str> = set.into_iter().collect();
            filter_sets.push_str(&format!(
                "@MFS_mark2mark_{class} = [{}];\n",
                names.join(" ")
            ));
            mark_lookups.push_str(&format!("    lookup mark2mark_{class} {{\n"));
            mark_lookups.push_str(&format!(
                "        lookupflag UseMarkFilteringSet @MFS_mark2mark_{class};\n"
            ));
            for (mark, (x, y)) in marks_here {
                mark_lookups.push_str(&format!(
                    "        pos mark {mark} <anchor {x} {y}> mark @MC_{class};\n"
                ));
                stacked += 1;
            }
            mark_lookups.push_str(&format!("    }} mark2mark_{class};\n"));
        }
    }

    if classes.is_empty() {
        return Generated {
            fea: String::new(),
            classes,
            marks: 0,
            bases: 0,
            stacked: 0,
        };
    }
    let mut fea = String::new();
    fea.push_str("# Written by `runebender-core features` from the font's anchors.\n");
    fea.push_str("# Edit the anchors, not this file; it is written again on save.\n\n");
    fea.push_str(&mark_lines);
    if !filter_sets.is_empty() {
        fea.push('\n');
        fea.push_str(&filter_sets);
    }
    if !base_lookups.is_empty() {
        fea.push_str("\nfeature mark {\n");
        fea.push_str(&base_lookups);
        fea.push_str("} mark;\n");
    }
    if !mark_lookups.is_empty() {
        fea.push_str("\nfeature mkmk {\n");
        fea.push_str(&mark_lookups);
        fea.push_str("} mkmk;\n");
    }
    Generated {
        fea,
        classes,
        marks: marks_seen.len(),
        bases,
        stacked,
    }
}

/// Whether feature text already defines a `mark` or `mkmk` feature,
/// outside comments.
pub fn defines_mark_features(fea: &str) -> bool {
    fea.lines().any(|line| {
        let code = line.split('#').next().unwrap_or("");
        let mut words = code.split_whitespace();
        words.next() == Some("feature") && matches!(words.next(), Some("mark") | Some("mkmk"))
    })
}

/// The feature text the editor shapes with: `features.fea` plus the
/// generated features, unless the file defines its own. The include
/// line `--write` adds is dropped, since the text is inlined here and
/// the in-memory compiler follows no includes.
pub fn with_generated(font: &Font) -> String {
    let own: String = font
        .features
        .lines()
        .filter(|l| l.trim() != INCLUDE_LINE)
        .map(|l| format!("{l}\n"))
        .collect();
    if defines_mark_features(&own) {
        return own;
    }
    let generated = generate(font);
    if generated.is_empty() {
        return own;
    }
    format!("{own}\n{}", generated.fea)
}

/// Writes `features.generated.fea` into the UFO and, when `include`
/// is set and `features.fea` does not define mark features, adds the
/// include line to `features.fea` (creating it if absent). Returns the
/// file written and whether the include line is now present.
pub fn write(ufo: &Path, generated: &Generated, include: bool) -> Result<(PathBuf, bool), String> {
    let path = ufo.join(GENERATED_FILE);
    std::fs::write(&path, &generated.fea).map_err(|e| format!("{}: {e}", path.display()))?;
    let fea_path = ufo.join("features.fea");
    let existing = std::fs::read_to_string(&fea_path).unwrap_or_default();
    let present = existing.lines().any(|l| l.trim() == INCLUDE_LINE);
    if present {
        return Ok((path, true));
    }
    if !include || defines_mark_features(&existing) {
        return Ok((path, false));
    }
    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(INCLUDE_LINE);
    text.push('\n');
    std::fs::write(&fea_path, text).map_err(|e| format!("{}: {e}", fea_path.display()))?;
    Ok((path, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use norad::{Anchor, Component, Name};

    fn anchor(name: &str, x: f64, y: f64) -> Anchor {
        Anchor::new(x, y, Some(Name::new(name).unwrap()), None, None)
    }

    fn glyph(name: &str, width: f64, anchors: &[(&str, f64, f64)]) -> Glyph {
        let mut g = Glyph::new(name);
        g.width = width;
        for (n, x, y) in anchors {
            g.anchors.push(anchor(n, *x, *y));
        }
        g
    }

    fn font() -> Font {
        let mut font = Font::new();
        let layer = font.default_layer_mut();
        layer.insert_glyph(glyph(
            "a",
            600.0,
            &[("top", 312.0, 576.0), ("bottom", 312.0, 0.0)],
        ));
        layer.insert_glyph(glyph(
            "acutecomb",
            0.0,
            &[("_top", 164.0, 576.0), ("top", 164.0, 864.0)],
        ));
        layer.insert_glyph(glyph("dotbelowcomb", 0.0, &[("_bottom", 100.0, 0.0)]));
        // A composite with no anchors of its own: it takes the base's.
        let mut beh = glyph("beh", 900.0, &[]);
        beh.components.push(Component::new(
            Name::new("a").unwrap(),
            norad::AffineTransform {
                x_offset: 100.0,
                ..Default::default()
            },
            None,
        ));
        layer.insert_glyph(beh);
        // A glyph with no anchors at all.
        layer.insert_glyph(glyph("space", 256.0, &[]));
        font
    }

    #[test]
    fn classes_lookups_and_propagation() {
        let g = generate(&font());
        assert_eq!(g.classes, ["bottom", "top"]);
        assert_eq!(g.marks, 2);
        assert_eq!(g.bases, 4, "a and beh, each on top and bottom");
        assert_eq!(g.stacked, 1, "acutecomb carries top");
        let fea = &g.fea;
        assert!(fea.contains("markClass acutecomb <anchor 164 576> @MC_top;"));
        assert!(fea.contains("pos base a <anchor 312 576> mark @MC_top;"));
        assert!(
            fea.contains("pos base beh <anchor 412 576> mark @MC_top;"),
            "propagated and translated: {fea}"
        );
        assert!(fea.contains("pos mark acutecomb <anchor 164 864> mark @MC_top;"));
        assert!(fea.contains("@MFS_mark2mark_top = [acutecomb];"));
        assert!(!fea.contains("space"));
        // Deterministic: the same font twice is the same text.
        assert_eq!(generate(&font()).fea, g.fea);
    }

    #[test]
    fn a_file_with_its_own_mark_feature_wins() {
        let mut f = font();
        f.features = "languagesystem DFLT dflt;\nfeature mark {\n} mark;\n".into();
        assert!(defines_mark_features(&f.features));
        assert_eq!(with_generated(&f), f.features);
        f.features = "# feature mark in a comment only\n".into();
        assert!(!defines_mark_features(&f.features));
        assert!(with_generated(&f).contains("feature mark {"));
    }

    #[test]
    fn the_include_line_is_dropped_when_inlining() {
        let mut f = font();
        f.features = format!("languagesystem DFLT dflt;\n{INCLUDE_LINE}\n");
        let text = with_generated(&f);
        assert!(!text.contains(INCLUDE_LINE));
        assert!(text.contains("feature mark {"));
    }

    #[test]
    fn a_font_without_anchors_adds_nothing() {
        let mut f = Font::new();
        f.default_layer_mut().insert_glyph(glyph("a", 600.0, &[]));
        assert!(generate(&f).is_empty());
        assert_eq!(with_generated(&f), String::new());
    }
}
