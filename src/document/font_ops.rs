// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Edits at the font level rather than the outline: kerning pairs and
//! groups, glyph names and unicodes, and the structural signature
//! interpolation compatibility is judged by.

pub use super::canonical_metadata::{
    CanonicalFontMetadata, CanonicalMetadataError, KerningParticipant, KerningSide,
};

use norad::{Font, Glyph, PointType};

/// Decode the UFO boundary maps into one exact canonical value.
pub fn canonical_metadata_from_ufo(
    font: &Font,
) -> Result<CanonicalFontMetadata, CanonicalMetadataError> {
    let groups = font
        .groups
        .iter()
        .map(|(name, members)| {
            (
                name.to_string(),
                members.iter().map(ToString::to_string).collect(),
            )
        })
        .collect();
    let kerning = font
        .kerning
        .iter()
        .map(|(left, row)| {
            (
                left.to_string(),
                row.iter()
                    .map(|(right, value)| (right.to_string(), *value))
                    .collect(),
            )
        })
        .collect();
    CanonicalFontMetadata::from_raw(groups, kerning)
}

/// Encode one canonical value into UFO boundary maps atomically.
pub fn write_canonical_metadata_to_ufo(
    font: &mut Font,
    metadata: &CanonicalFontMetadata,
) -> Result<bool, CanonicalMetadataError> {
    let mut groups = norad::Groups::default();
    for (name, members) in metadata.groups() {
        let name = norad::Name::new(name)
            .map_err(|_| CanonicalMetadataError::InvalidName(name.clone()))?;
        let members = members
            .iter()
            .map(|member| {
                norad::Name::new(member)
                    .map_err(|_| CanonicalMetadataError::InvalidName(member.clone()))
            })
            .collect::<Result<_, _>>()?;
        groups.insert(name, members);
    }
    let mut kerning = norad::Kerning::default();
    for (left, row) in metadata.raw_kerning() {
        let left_name = norad::Name::new(&left)
            .map_err(|_| CanonicalMetadataError::InvalidName(left.clone()))?;
        let mut output_row = std::collections::BTreeMap::new();
        for (right, value) in row {
            let right_name = norad::Name::new(&right)
                .map_err(|_| CanonicalMetadataError::InvalidName(right.clone()))?;
            output_row.insert(right_name, value);
        }
        kerning.insert(left_name, output_row);
    }
    if font.groups == groups && font.kerning == kerning {
        return Ok(false);
    }
    font.groups = groups;
    font.kerning = kerning;
    Ok(true)
}

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

/// Set a glyph's codepoints from hexadecimal text such as `"0041"`, `"U+0041"`, or
/// `"0x41, U+0391"`.
///
/// Parsed characters replace every codepoint the glyph had.
/// An empty string clears them all.
/// Returns false when any token does not parse.
pub fn set_glyph_unicode(glyph: &mut Glyph, unicode: &str) -> bool {
    let Ok(codepoints) = super::model::glyph_metadata::parse_codepoints(unicode) else {
        return false;
    };
    glyph.codepoints = norad::Codepoints::new(codepoints);
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

#[cfg(test)]
mod canonical_tests {
    use super::*;

    #[test]
    fn canonical_ufo_boundary_preserves_fractional_kerning_and_unrelated_groups() {
        let mut source = Font::new();
        source.groups.insert(
            norad::Name::new("com.example.arbitrary").unwrap(),
            vec![norad::Name::new("A").unwrap()],
        );
        source.groups.insert(
            norad::Name::new("public.kern1.A").unwrap(),
            vec![norad::Name::new("A").unwrap()],
        );
        source.kerning.insert(
            norad::Name::new("A").unwrap(),
            std::collections::BTreeMap::from([(norad::Name::new("V").unwrap(), -81.375)]),
        );

        let canonical = canonical_metadata_from_ufo(&source).unwrap();
        let mut output = Font::new();
        assert!(write_canonical_metadata_to_ufo(&mut output, &canonical).unwrap());
        assert!(!write_canonical_metadata_to_ufo(&mut output, &canonical).unwrap());
        assert_eq!(output.groups, source.groups);
        assert_eq!(output.kerning, source.kerning);
    }
}
