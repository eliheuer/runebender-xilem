// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Edits at the font level rather than the outline: kerning pairs and
//! groups, glyph names and unicodes, and the structural signature
//! interpolation compatibility is judged by.

use norad::{Font, Glyph, PointType};

/// Kerning between two glyphs, resolving group fallbacks in UFO
/// precedence order: glyph-glyph, glyph-group, group-glyph,
/// group-group.
pub fn kern_value(font: &Font, left: &str, right: &str) -> f64 {
    let lookup =
        |a: &str, b: &str| -> Option<f64> { font.kerning.get(a).and_then(|m| m.get(b)).copied() };
    let lg = kern_group(font, left, true);
    let rg = kern_group(font, right, false);
    lookup(left, right)
        .or_else(|| rg.as_ref().and_then(|g| lookup(left, g.as_str())))
        .or_else(|| lg.as_ref().and_then(|g| lookup(g.as_str(), right)))
        .or_else(|| {
            lg.as_ref()
                .and_then(|l| rg.as_ref().and_then(|r| lookup(l.as_str(), r.as_str())))
        })
        .unwrap_or(0.0)
}

/// Set a glyph-to-glyph kern pair, the exception level.
pub fn set_kern_pair(font: &mut Font, left: &str, right: &str, value: f64) {
    let (Ok(l), Ok(r)) = (norad::Name::new(left), norad::Name::new(right)) else {
        return;
    };
    font.kerning.entry(l).or_default().insert(r, value);
}

/// The kern group containing a glyph, if any.
///
/// Group names carry the `public.kern1.` prefix on the first side
/// and `public.kern2.` on the second.
pub fn kern_group(font: &Font, glyph: &str, first_side: bool) -> Option<norad::Name> {
    let prefix = if first_side {
        "public.kern1."
    } else {
        "public.kern2."
    };
    font.groups
        .iter()
        .find(|(name, members)| {
            name.starts_with(prefix) && members.iter().any(|m| m.as_str() == glyph)
        })
        .map(|(name, _)| name.clone())
}

/// Put a glyph into a kerning group, replacing any membership on
/// that side.
///
/// Groups live in `groups.plist`. `group` is the bare name: `"A"`
/// becomes `public.kern1.A`. An empty name removes the membership.
/// Returns true when anything changed.
pub fn set_kern_group(font: &mut Font, glyph: &str, first_side: bool, group: &str) -> bool {
    let prefix = if first_side {
        "public.kern1."
    } else {
        "public.kern2."
    };
    let target = group.trim();
    let target_name = (!target.is_empty())
        .then(|| norad::Name::new(&format!("{prefix}{target}")).ok())
        .flatten();
    let mut changed = false;
    // Drop the glyph from every group on this side except the target.
    let mut empty: Vec<norad::Name> = Vec::new();
    for (name, members) in font.groups.iter_mut() {
        if !name.starts_with(prefix) {
            continue;
        }
        if Some(name) == target_name.as_ref() {
            continue;
        }
        let before = members.len();
        members.retain(|m| m.as_str() != glyph);
        if members.len() != before {
            changed = true;
        }
        if members.is_empty() {
            empty.push(name.clone());
        }
    }
    for name in empty {
        font.groups.remove(&name);
        changed = true;
    }
    if let Some(target_name) = target_name {
        let glyph_name = match norad::Name::new(glyph) {
            Ok(name) => name,
            Err(_) => return changed,
        };
        let members = font.groups.entry(target_name).or_default();
        if !members.iter().any(|m| m.as_str() == glyph) {
            members.push(glyph_name);
            changed = true;
        }
    }
    changed
}

/// Set a glyph's codepoint from text: `"0041"`, `"U+0041"`, or
/// `"0x41"`.
///
/// The parsed character replaces every codepoint the glyph had. An
/// empty string clears them all. Returns false when the text does
/// not parse.
pub fn set_glyph_unicode(glyph: &mut Glyph, unicode: &str) -> bool {
    let trimmed = unicode.trim();
    if trimmed.is_empty() {
        glyph.codepoints = norad::Codepoints::new([]);
        return true;
    }
    let hex = trimmed
        .strip_prefix("U+")
        .or_else(|| trimmed.strip_prefix("u+"))
        .or_else(|| trimmed.strip_prefix("0x"))
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    let Some(c) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) else {
        return false;
    };
    glyph.codepoints = norad::Codepoints::new([c]);
    true
}

/// Rename a glyph and every reference to it: components in other
/// glyphs, kerning group memberships, and direct kerning pair keys.
/// Refuses when the new name is taken or invalid.
pub fn rename_glyph(font: &mut Font, old: &str, new: &str) -> bool {
    let new = new.trim();
    if new.is_empty() || new == old {
        return false;
    }
    let Ok(new_name) = norad::Name::new(new) else {
        return false;
    };
    if font.get_glyph(new).is_some() {
        return false;
    }
    let layer = font.default_layer_mut();
    if layer.rename_glyph(old, new, false).is_err() {
        return false;
    }
    // Components in every glyph that places it.
    let renames: Vec<norad::Name> = layer
        .iter()
        .filter(|g| g.components.iter().any(|c| c.base.as_str() == old))
        .map(|g| g.name().clone())
        .collect();
    for user in renames {
        if let Some(user_glyph) = layer.get_glyph_mut(user.as_str()) {
            for component in user_glyph.components.iter_mut() {
                if component.base.as_str() == old {
                    component.base = new_name.clone();
                }
            }
        }
    }
    // Group memberships.
    for members in font.groups.values_mut() {
        for member in members.iter_mut() {
            if member.as_str() == old {
                *member = new_name.clone();
            }
        }
    }
    // Direct kerning keys on either side.
    let old_key = norad::Name::new(old).ok();
    if let Some(old_key) = old_key {
        if let Some(seconds) = font.kerning.remove(&old_key) {
            font.kerning.insert(new_name.clone(), seconds);
        }
        for seconds in font.kerning.values_mut() {
            if let Some(value) = seconds.remove(&old_key) {
                seconds.insert(new_name.clone(), value);
            }
        }
    }
    true
}

/// Structural signature used for interpolation compatibility: per
/// contour, the ordered list of point types.
pub fn glyph_signature(glyph: &Glyph) -> Vec<Vec<PointType>> {
    glyph
        .contours
        .iter()
        .map(|c| c.points.iter().map(|p| p.typ).collect())
        .collect()
}
