// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical source-wide group and kerning values.
//!
//! This module deliberately has no UFO or Babelfont types.
//! Format adapters translate their boundary maps once, while the live document retains exact
//! `f64` values and typed kerning participants.

use std::collections::BTreeMap;
use std::fmt;

/// Prefix used by UFO groups on the first side of a kerning pair.
pub const FIRST_KERN_GROUP_PREFIX: &str = "public.kern1.";

/// Prefix used by UFO groups on the second side of a kerning pair.
pub const SECOND_KERN_GROUP_PREFIX: &str = "public.kern2.";

/// One side of a kerning pair or kerning-group membership.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KerningSide {
    /// The first, or left in left-to-right text, side.
    First,
    /// The second, or right in left-to-right text, side.
    Second,
}

impl KerningSide {
    /// The UFO group prefix for this side.
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::First => FIRST_KERN_GROUP_PREFIX,
            Self::Second => SECOND_KERN_GROUP_PREFIX,
        }
    }

    const fn opposite_prefix(self) -> &'static str {
        match self {
            Self::First => SECOND_KERN_GROUP_PREFIX,
            Self::Second => FIRST_KERN_GROUP_PREFIX,
        }
    }
}

/// A glyph or side-specific group used in a kerning pair.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KerningParticipant {
    /// A glyph name.
    Glyph(String),
    /// A full UFO kerning-group name and its side.
    Group {
        /// The side on which the group is valid.
        side: KerningSide,
        /// The full `public.kern1.*` or `public.kern2.*` name.
        name: String,
    },
}

impl KerningParticipant {
    /// Construct a glyph participant after validating its UFO name.
    pub fn glyph(name: impl Into<String>) -> Result<Self, CanonicalMetadataError> {
        let name = name.into();
        validate_name(&name)?;
        if side_from_group_name(&name).is_some() {
            return Err(CanonicalMetadataError::ReservedGlyphName(name));
        }
        Ok(Self::Glyph(name))
    }

    /// Construct a group participant from a bare or fully prefixed group name.
    pub fn group(side: KerningSide, name: impl AsRef<str>) -> Result<Self, CanonicalMetadataError> {
        let name = canonical_group_name(side, name.as_ref())?;
        Ok(Self::Group { side, name })
    }

    /// The raw UFO glyph or group name.
    pub fn as_raw_name(&self) -> &str {
        match self {
            Self::Glyph(name) | Self::Group { name, .. } => name,
        }
    }

    fn parse(raw: &str, expected_side: KerningSide) -> Result<Self, CanonicalMetadataError> {
        validate_name(raw)?;
        if raw.starts_with(expected_side.prefix()) {
            return Self::group(expected_side, raw);
        }
        if raw.starts_with(expected_side.opposite_prefix()) {
            return Err(CanonicalMetadataError::WrongGroupSide {
                name: raw.to_owned(),
                expected: expected_side,
            });
        }
        Self::glyph(raw)
    }

    fn renamed_glyph(&self, old: &str, new: &str) -> Self {
        match self {
            Self::Glyph(name) if name == old => Self::Glyph(new.to_owned()),
            _ => self.clone(),
        }
    }

    fn renamed_group(&self, old: &str, new: &str) -> Self {
        match self {
            Self::Group { side, name } if name == old => Self::Group {
                side: *side,
                name: new.to_owned(),
            },
            _ => self.clone(),
        }
    }

    fn is_glyph(&self, glyph: &str) -> bool {
        matches!(self, Self::Glyph(name) if name == glyph)
    }
}

/// A rejected canonical metadata operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonicalMetadataError {
    /// A name was empty or contained a control character.
    InvalidName(String),
    /// A glyph name used UFO's reserved kerning-group namespace.
    ReservedGlyphName(String),
    /// A group was used on the wrong side of a pair.
    WrongGroupSide {
        /// The full group name.
        name: String,
        /// The side required at this position.
        expected: KerningSide,
    },
    /// A rename would change an arbitrary group into a kerning group or vice versa.
    IncompatibleGroupRename {
        /// The current group name.
        old: String,
        /// The requested group name.
        new: String,
    },
    /// A kerning value was NaN or infinite.
    NonFiniteKerning {
        /// The first participant's raw name.
        left: String,
        /// The second participant's raw name.
        right: String,
    },
    /// A rename would overwrite an existing group, participant or glyph reference.
    RenameCollision(String),
}

impl fmt::Display for CanonicalMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName(name) => write!(f, "invalid UFO name {name:?}"),
            Self::ReservedGlyphName(name) => {
                write!(
                    f,
                    "glyph name {name:?} uses the reserved kerning-group namespace"
                )
            }
            Self::WrongGroupSide { name, expected } => {
                write!(
                    f,
                    "kerning group {name:?} is not valid on the {expected:?} side"
                )
            }
            Self::IncompatibleGroupRename { old, new } => {
                write!(
                    f,
                    "cannot rename group {old:?} to incompatible name {new:?}"
                )
            }
            Self::NonFiniteKerning { left, right } => {
                write!(f, "kerning pair {left:?} {right:?} has a non-finite value")
            }
            Self::RenameCollision(name) => {
                write!(f, "renaming to {name:?} would overwrite metadata")
            }
        }
    }
}

impl std::error::Error for CanonicalMetadataError {}

/// Exact editable source metadata for groups and kerning.
///
/// All groups are retained, including non-kerning groups.
/// Kerning pairs use typed participants so a group cannot accidentally be placed on the wrong
/// side, and values remain `f64` until an immutable compiler snapshot quantizes them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CanonicalFontMetadata {
    groups: BTreeMap<String, Vec<String>>,
    kerning: BTreeMap<(KerningParticipant, KerningParticipant), f64>,
}

impl CanonicalFontMetadata {
    /// Import raw UFO-shaped maps after validating names, pair sides and finite values.
    ///
    /// Group member order and duplicates are preserved exactly.
    /// UFO permits duplicates in arbitrary groups and requires authoring tools to ignore later
    /// duplicate members in kerning groups.
    pub fn from_raw(
        groups: BTreeMap<String, Vec<String>>,
        kerning: BTreeMap<String, BTreeMap<String, f64>>,
    ) -> Result<Self, CanonicalMetadataError> {
        validate_groups(&groups)?;
        let mut typed = BTreeMap::new();
        for (left, row) in kerning {
            let left_participant = KerningParticipant::parse(&left, KerningSide::First)?;
            for (right, value) in row {
                let right_participant = KerningParticipant::parse(&right, KerningSide::Second)?;
                validate_pair(&left_participant, &right_participant, value)?;
                typed.insert((left_participant.clone(), right_participant), value);
            }
        }
        Ok(Self {
            groups,
            kerning: typed,
        })
    }

    /// Read every group exactly as it will be written at the format boundary.
    pub fn groups(&self) -> &BTreeMap<String, Vec<String>> {
        &self.groups
    }

    /// Materialize the nested raw kerning map required by a UFO writer.
    pub fn raw_kerning(&self) -> BTreeMap<String, BTreeMap<String, f64>> {
        let mut output: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
        for ((left, right), value) in &self.kerning {
            output
                .entry(left.as_raw_name().to_owned())
                .or_default()
                .insert(right.as_raw_name().to_owned(), *value);
        }
        output
    }

    /// Iterate over typed pairs and exact editable values.
    pub fn kerning_pairs(
        &self,
    ) -> impl Iterator<Item = (&KerningParticipant, &KerningParticipant, f64)> {
        self.kerning
            .iter()
            .map(|((left, right), value)| (left, right, *value))
    }

    /// Return the first deterministic group containing `glyph` on `side`.
    ///
    /// Canonical edits enforce at most one membership per side.
    /// Imported invalid data can contain more than one; B-tree order makes the read deterministic
    /// until the user edits that membership.
    pub fn kerning_group(&self, glyph: &str, side: KerningSide) -> Option<&str> {
        self.groups.iter().find_map(|(name, members)| {
            (name.starts_with(side.prefix()) && members.iter().any(|member| member == glyph))
                .then_some(name.as_str())
        })
    }

    /// Resolve a pair in UFO precedence order.
    ///
    /// The return value distinguishes an explicit zero from a missing pair.
    pub fn resolved_kerning(&self, left: &str, right: &str) -> Option<f64> {
        let left = KerningParticipant::glyph(left.to_owned()).ok()?;
        let right = KerningParticipant::glyph(right.to_owned()).ok()?;
        if let Some(value) = self.kerning.get(&(left.clone(), right.clone())) {
            return Some(*value);
        }

        let left_groups = self.groups_containing(left.as_raw_name(), KerningSide::First);
        let right_groups = self.groups_containing(right.as_raw_name(), KerningSide::Second);
        for group in &right_groups {
            if let Some(value) = self.kerning.get(&(left.clone(), (*group).clone())) {
                return Some(*value);
            }
        }
        for group in &left_groups {
            if let Some(value) = self.kerning.get(&((*group).clone(), right.clone())) {
                return Some(*value);
            }
        }
        for left_group in &left_groups {
            for right_group in &right_groups {
                if let Some(value) = self
                    .kerning
                    .get(&((*left_group).clone(), (*right_group).clone()))
                {
                    return Some(*value);
                }
            }
        }
        None
    }

    /// Set or remove one exact pair.
    ///
    /// Invalid or non-finite input is rejected before mutation.
    pub fn set_kerning_pair(
        &mut self,
        left: KerningParticipant,
        right: KerningParticipant,
        value: Option<f64>,
    ) -> Result<bool, CanonicalMetadataError> {
        if let Some(value) = value {
            validate_pair(&left, &right, value)?;
            if self.kerning.get(&(left.clone(), right.clone())) == Some(&value) {
                return Ok(false);
            }
            self.kerning.insert((left, right), value);
            return Ok(true);
        }
        validate_pair_sides(&left, &right)?;
        Ok(self.kerning.remove(&(left, right)).is_some())
    }

    /// Put a glyph in one kerning group on a side, removing other memberships on that side.
    ///
    /// `group` can be bare or fully prefixed.
    /// `None` or an empty string removes the membership.
    pub fn set_kerning_group(
        &mut self,
        glyph: &str,
        side: KerningSide,
        group: Option<&str>,
    ) -> Result<bool, CanonicalMetadataError> {
        KerningParticipant::glyph(glyph.to_owned())?;
        let target = group
            .filter(|name| !name.trim().is_empty())
            .map(|name| canonical_group_name(side, name))
            .transpose()?;
        let mut groups = self.groups.clone();
        let side_names: Vec<_> = groups
            .keys()
            .filter(|name| name.starts_with(side.prefix()))
            .cloned()
            .collect();
        for name in side_names {
            let members = groups
                .get_mut(&name)
                .expect("collected group remains present");
            if target.as_deref() == Some(name.as_str()) {
                let mut found = false;
                members.retain(|member| {
                    if member != glyph {
                        return true;
                    }
                    let keep = !found;
                    found = true;
                    keep
                });
                if !found {
                    members.push(glyph.to_owned());
                }
            } else {
                members.retain(|member| member != glyph);
            }
            if members.is_empty() {
                groups.remove(&name);
            }
        }
        if let Some(target) = target {
            let members = groups.entry(target).or_default();
            if !members.iter().any(|member| member == glyph) {
                members.push(glyph.to_owned());
            }
        }
        if groups == self.groups {
            return Ok(false);
        }
        self.groups = groups;
        Ok(true)
    }

    /// Replace one complete group without changing unrelated groups or pairs.
    pub fn set_group(
        &mut self,
        name: impl Into<String>,
        members: Vec<String>,
    ) -> Result<bool, CanonicalMetadataError> {
        let name = name.into();
        validate_name(&name)?;
        if let Some(side) = side_from_group_name(&name) {
            canonical_group_name(side, &name)?;
        }
        validate_group_members(&members)?;
        if self.groups.get(&name) == Some(&members) {
            return Ok(false);
        }
        self.groups.insert(name, members);
        Ok(true)
    }

    /// Remove a group and every kerning pair that names it.
    pub fn remove_group(&mut self, name: &str) -> Result<bool, CanonicalMetadataError> {
        validate_name(name)?;
        let side = side_from_group_name(name);
        let mut groups = self.groups.clone();
        let mut kerning = self.kerning.clone();
        let removed = groups.remove(name).is_some();
        if let Some(side) = side {
            let participant = KerningParticipant::group(side, name)?;
            kerning.retain(|(left, right), _| left != &participant && right != &participant);
        }
        if !removed && kerning == self.kerning {
            return Ok(false);
        }
        self.groups = groups;
        self.kerning = kerning;
        Ok(true)
    }

    /// Rename a group and every kerning pair that names it atomically.
    ///
    /// Arbitrary groups must remain arbitrary, and a kerning group must remain on the same side.
    pub fn rename_group(&mut self, old: &str, new: &str) -> Result<bool, CanonicalMetadataError> {
        validate_name(old)?;
        validate_name(new)?;
        let old_side = side_from_group_name(old);
        let new_side = side_from_group_name(new);
        if old_side != new_side {
            return Err(CanonicalMetadataError::IncompatibleGroupRename {
                old: old.to_owned(),
                new: new.to_owned(),
            });
        }
        if let Some(side) = old_side {
            canonical_group_name(side, old)?;
            canonical_group_name(side, new)?;
        }
        if old == new || !self.references_group(old) {
            return Ok(false);
        }
        if self.references_group(new) {
            return Err(CanonicalMetadataError::RenameCollision(new.to_owned()));
        }

        let mut groups = self.groups.clone();
        if let Some(members) = groups.remove(old) {
            groups.insert(new.to_owned(), members);
        }
        let mut kerning = BTreeMap::new();
        for ((left, right), value) in &self.kerning {
            let pair = (left.renamed_group(old, new), right.renamed_group(old, new));
            if kerning.insert(pair, *value).is_some() {
                return Err(CanonicalMetadataError::RenameCollision(new.to_owned()));
            }
        }
        self.groups = groups;
        self.kerning = kerning;
        Ok(true)
    }

    /// Rename a glyph in all groups and pair participants atomically.
    ///
    /// The caller owns the glyph table and component references.
    /// It should invoke this helper within the same document transaction after checking that the
    /// destination glyph name is free.
    pub fn rename_glyph_references(
        &mut self,
        old: &str,
        new: &str,
    ) -> Result<bool, CanonicalMetadataError> {
        KerningParticipant::glyph(old.to_owned())?;
        KerningParticipant::glyph(new.to_owned())?;
        if old == new || !self.references_glyph(old) {
            return Ok(false);
        }
        if self.references_glyph(new) {
            return Err(CanonicalMetadataError::RenameCollision(new.to_owned()));
        }

        let mut groups = self.groups.clone();
        for members in groups.values_mut() {
            for member in members {
                if member == old {
                    *member = new.to_owned();
                }
            }
        }
        let mut kerning = BTreeMap::new();
        for ((left, right), value) in &self.kerning {
            let pair = (left.renamed_glyph(old, new), right.renamed_glyph(old, new));
            if kerning.insert(pair, *value).is_some() {
                return Err(CanonicalMetadataError::RenameCollision(new.to_owned()));
            }
        }
        self.groups = groups;
        self.kerning = kerning;
        Ok(true)
    }

    /// Remove every group membership and direct pair reference for a deleted glyph.
    pub fn remove_glyph_references(&mut self, glyph: &str) -> Result<bool, CanonicalMetadataError> {
        KerningParticipant::glyph(glyph.to_owned())?;
        let mut groups = self.groups.clone();
        for members in groups.values_mut() {
            members.retain(|member| member != glyph);
        }
        let mut kerning = self.kerning.clone();
        kerning.retain(|(left, right), _| !left.is_glyph(glyph) && !right.is_glyph(glyph));
        if groups == self.groups && kerning == self.kerning {
            return Ok(false);
        }
        self.groups = groups;
        self.kerning = kerning;
        Ok(true)
    }

    fn groups_containing(&self, glyph: &str, side: KerningSide) -> Vec<KerningParticipant> {
        self.groups
            .iter()
            .filter(|(name, members)| {
                name.starts_with(side.prefix()) && members.iter().any(|member| member == glyph)
            })
            .filter_map(|(name, _)| KerningParticipant::group(side, name).ok())
            .collect()
    }

    fn references_glyph(&self, glyph: &str) -> bool {
        self.groups
            .values()
            .any(|members| members.iter().any(|member| member == glyph))
            || self
                .kerning
                .keys()
                .any(|(left, right)| left.is_glyph(glyph) || right.is_glyph(glyph))
    }

    fn references_group(&self, group: &str) -> bool {
        self.groups.contains_key(group)
            || self.kerning.keys().any(|(left, right)| {
                matches!(left, KerningParticipant::Group { name, .. } if name == group)
                    || matches!(right, KerningParticipant::Group { name, .. } if name == group)
            })
    }
}

/// Validate a glyph, layer or group name using the UFO name contract.
pub fn validate_name(name: &str) -> Result<(), CanonicalMetadataError> {
    if name.is_empty()
        || name
            .chars()
            .any(|character| matches!(character as u32, 0x00..=0x1f | 0x7f | 0x80..=0x9f))
    {
        return Err(CanonicalMetadataError::InvalidName(name.to_owned()));
    }
    Ok(())
}

fn canonical_group_name(side: KerningSide, name: &str) -> Result<String, CanonicalMetadataError> {
    let name = name.trim();
    let full = if name.starts_with(side.prefix()) {
        name.to_owned()
    } else if name.starts_with(side.opposite_prefix()) {
        return Err(CanonicalMetadataError::WrongGroupSide {
            name: name.to_owned(),
            expected: side,
        });
    } else {
        format!("{}{name}", side.prefix())
    };
    validate_name(&full)?;
    if full == side.prefix() {
        return Err(CanonicalMetadataError::InvalidName(full));
    }
    Ok(full)
}

fn side_from_group_name(name: &str) -> Option<KerningSide> {
    if name.starts_with(FIRST_KERN_GROUP_PREFIX) {
        Some(KerningSide::First)
    } else if name.starts_with(SECOND_KERN_GROUP_PREFIX) {
        Some(KerningSide::Second)
    } else {
        None
    }
}

fn validate_groups(groups: &BTreeMap<String, Vec<String>>) -> Result<(), CanonicalMetadataError> {
    for (name, members) in groups {
        validate_name(name)?;
        if let Some(side) = side_from_group_name(name) {
            canonical_group_name(side, name)?;
        }
        validate_group_members(members)?;
    }
    Ok(())
}

fn validate_group_members(members: &[String]) -> Result<(), CanonicalMetadataError> {
    for glyph in members {
        validate_name(glyph)?;
    }
    Ok(())
}

fn validate_pair(
    left: &KerningParticipant,
    right: &KerningParticipant,
    value: f64,
) -> Result<(), CanonicalMetadataError> {
    validate_pair_sides(left, right)?;
    if !value.is_finite() {
        return Err(CanonicalMetadataError::NonFiniteKerning {
            left: left.as_raw_name().to_owned(),
            right: right.as_raw_name().to_owned(),
        });
    }
    Ok(())
}

fn validate_pair_sides(
    left: &KerningParticipant,
    right: &KerningParticipant,
) -> Result<(), CanonicalMetadataError> {
    if let KerningParticipant::Group { side, name } = left
        && *side != KerningSide::First
    {
        return Err(CanonicalMetadataError::WrongGroupSide {
            name: name.clone(),
            expected: KerningSide::First,
        });
    }
    if let KerningParticipant::Group { side, name } = right
        && *side != KerningSide::Second
    {
        return Err(CanonicalMetadataError::WrongGroupSide {
            name: name.clone(),
            expected: KerningSide::Second,
        });
    }
    Ok(())
}
