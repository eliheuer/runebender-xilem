// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Kurbo-free glyph metadata shared by Runebender frontends.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

const SKIP_EXPORT_GLYPHS: &str = "public.skipExportGlyphs";
const OPEN_TYPE_CATEGORIES: &str = "public.openTypeCategories";
pub(crate) const COMPONENT_ALIGNMENT_KEY: &str = "com.glyphsapp.component.alignment";
/// UFO glyph-lib key for the exact public mark color.
pub const MARK_COLOR_KEY: &str = "public.markColor";
/// UFO glyph-lib key for the semantic Runebender mark label.
pub const MARK_LABEL_KEY: &str = "com.runebender.markLabel";
/// UFO glyph-lib key for the left sidebearing formula.
pub const LEFT_METRICS_KEY: &str = "com.schriftgestaltung.Glyphs.glyph.leftMetricsKey";
/// UFO glyph-lib key for the right sidebearing formula.
pub const RIGHT_METRICS_KEY: &str = "com.schriftgestaltung.Glyphs.glyph.rightMetricsKey";
/// UFO glyph-lib key for editable Runebender metaball groups.
pub const METABALLS_KEY: &str = "com.runebender.metaballs";
/// UFO glyph-lib key for an explicit canonical composition recipe.
pub const COMPOSITION_RECIPE_KEY: &str = "com.runebender.compose";

#[cfg(test)]
pub(crate) fn skipped_exports(font: &norad::Font) -> impl Iterator<Item = &str> {
    font.lib
        .get(SKIP_EXPORT_GLYPHS)
        .and_then(plist::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(plist::Value::as_string)
}

pub(crate) fn set_skipped_exports(font: &mut norad::Font, names: Vec<String>) {
    if names.is_empty() {
        font.lib.remove(SKIP_EXPORT_GLYPHS);
    } else {
        font.lib.insert(
            SKIP_EXPORT_GLYPHS.into(),
            plist::Value::Array(names.into_iter().map(plist::Value::String).collect()),
        );
    }
}

/// A typed value from the UFO `public.openTypeCategories` dictionary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenTypeGlyphCategory {
    /// No category is assigned explicitly.
    Unassigned,
    /// A base glyph.
    Base,
    /// A combining or spacing mark.
    Mark,
    /// A ligature glyph.
    Ligature,
    /// A glyph intended only as a component source.
    Component,
    /// A source value this Runebender version does not interpret.
    Other(String),
}

impl OpenTypeGlyphCategory {
    /// Preserve a source category as a typed known value or an exact unknown string.
    pub fn from_source(value: impl Into<String>) -> Self {
        let value = value.into();
        match value.as_str() {
            "unassigned" => Self::Unassigned,
            "base" => Self::Base,
            "mark" => Self::Mark,
            "ligature" => Self::Ligature,
            "component" => Self::Component,
            _ => Self::Other(value),
        }
    }

    /// The exact source string written at the UFO boundary.
    pub fn as_source(&self) -> &str {
        match self {
            Self::Unassigned => "unassigned",
            Self::Base => "base",
            Self::Mark => "mark",
            Self::Ligature => "ligature",
            Self::Component => "component",
            Self::Other(value) => value,
        }
    }
}

/// A typed `public.markColor` value independent of its UFO string encoding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkColor {
    /// Red channel, from zero through one.
    pub red: f64,
    /// Green channel, from zero through one.
    pub green: f64,
    /// Blue channel, from zero through one.
    pub blue: f64,
    /// Alpha channel, from zero through one.
    pub alpha: f64,
}

impl MarkColor {
    /// Parse exactly four finite comma-separated channels from zero through one.
    pub fn parse(value: &str) -> Option<Self> {
        let mut values = value.split(',').map(str::trim).map(str::parse::<f64>);
        let red = values.next()?.ok()?;
        let green = values.next()?.ok()?;
        let blue = values.next()?.ok()?;
        let alpha = values.next()?.ok()?;
        if values.next().is_some() {
            return None;
        }
        let color = Self {
            red,
            green,
            blue,
            alpha,
        };
        color.is_valid().then_some(color)
    }

    /// Whether every channel is finite and from zero through one.
    pub fn is_valid(self) -> bool {
        [self.red, self.green, self.blue, self.alpha]
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
    }
}

/// Exact component auto-alignment metadata with typed Runebender semantics.
///
/// The private source value retains a recognized legacy spelling or an unknown future value until
/// an explicit edit replaces it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComponentAlignment {
    source: Option<plist::Value>,
}

impl ComponentAlignment {
    /// Move the alignment key out of a component's otherwise opaque lib dictionary.
    pub fn take_from_lib(lib: &mut plist::Dictionary) -> Self {
        Self {
            source: lib.remove(COMPONENT_ALIGNMENT_KEY),
        }
    }

    /// Whether this component is explicitly cut loose from anchor alignment.
    ///
    /// Glyphs-compatible negative integers and boolean false disable alignment.
    /// Unknown values retain their exact representation and keep the default aligned behavior.
    pub fn is_disabled(&self) -> bool {
        self.source.as_ref().is_some_and(|value| {
            value.as_signed_integer().is_some_and(|value| value < 0)
                || value.as_boolean() == Some(false)
        })
    }

    /// Change whether the component follows anchor alignment.
    ///
    /// A semantic no-op retains the exact source representation.
    /// Disabling a previously aligned component uses the established negative-integer encoding,
    /// while enabling removes the key.
    pub fn set_disabled(&mut self, disabled: bool) -> bool {
        if self.is_disabled() == disabled {
            return false;
        }
        self.source = disabled.then(|| plist::Value::Integer((-1).into()));
        true
    }

    /// Write the owned source value into an otherwise opaque component lib dictionary.
    pub fn write_to_lib(&self, lib: &mut plist::Dictionary) -> bool {
        match &self.source {
            Some(value) if lib.get(COMPONENT_ALIGNMENT_KEY) == Some(value) => false,
            Some(value) => {
                lib.insert(COMPONENT_ALIGNMENT_KEY.into(), value.clone());
                true
            }
            None => lib.remove(COMPONENT_ALIGNMENT_KEY).is_some(),
        }
    }
}

/// A center and its compact radial influence field in font coordinates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metaball {
    /// Stable element identifier, unique within its group.
    pub id: u32,
    /// Horizontal position in font units.
    pub x: f64,
    /// Vertical position in font units, increasing upwards.
    pub y: f64,
    /// Support radius in font units; the field is zero beyond this radius.
    pub radius: f64,
    /// Field strength at the center.
    /// Positive values add ink and negative values subtract it.
    pub stiffness: f64,
}

/// Elements whose fields blend together.
/// Separate groups never influence each other.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetaballGroup {
    /// Stable group identifier, unique in one glyph.
    pub id: u32,
    /// Positive field level at the visible boundary.
    pub threshold: f64,
    /// Editable centers, retained until explicit conversion.
    pub balls: Vec<Metaball>,
}

/// Editable metaball source data for one glyph layer.
/// Preview contours are derived and never stored here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metaballs {
    /// Schema version; currently only version one is supported.
    pub version: u32,
    /// Independent blending groups.
    pub groups: Vec<MetaballGroup>,
}

impl Default for Metaballs {
    fn default() -> Self {
        Self {
            version: 1,
            groups: Vec::new(),
        }
    }
}

impl Metaballs {
    /// Validate schema, identifiers, finite coordinates and bounded field parameters.
    ///
    /// At most 128 groups and 256 centers are accepted.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!("unsupported metaball version {}", self.version));
        }
        if self.groups.len() > 128 || self.groups.iter().map(|g| g.balls.len()).sum::<usize>() > 256
        {
            return Err("too many metaball groups or centers".into());
        }
        let mut groups = HashSet::new();
        for group in &self.groups {
            if !groups.insert(group.id)
                || !group.threshold.is_finite()
                || !(0.01..=100.0).contains(&group.threshold)
            {
                return Err("invalid metaball group identifier or threshold".into());
            }
            let mut ids = HashSet::new();
            for ball in &group.balls {
                if !ids.insert(ball.id)
                    || !ball.x.is_finite()
                    || !ball.y.is_finite()
                    || ball.x.abs().max(ball.y.abs()) > 1_000_000.0
                    || !ball.radius.is_finite()
                    || !(1.0..=100_000.0).contains(&ball.radius)
                    || !ball.stiffness.is_finite()
                    || ball.stiffness.abs() > 100.0
                {
                    return Err("invalid metaball identifier, position, radius or stiffness".into());
                }
            }
        }
        Ok(())
    }
}

/// A parsed glyph spacing formula independent of its UFO lib encoding.
#[derive(Clone, Debug, PartialEq)]
pub enum MetricsFormula {
    /// A fixed sidebearing or width in font units, such as `=50`.
    Constant(f64),
    /// A metric copied from another glyph, with optional mirroring and arithmetic.
    Reference {
        /// Name of the glyph whose metric is copied.
        glyph: String,
        /// Read the opposite sidebearing of the referenced glyph.
        mirror: bool,
        /// Trailing arithmetic: `+`, `-` or `*` and a finite value.
        op: Option<(char, f64)>,
    },
}

impl MetricsFormula {
    /// The referenced glyph name, if this is not a constant formula.
    pub fn referenced_glyph(&self) -> Option<&str> {
        match self {
            Self::Constant(_) => None,
            Self::Reference { glyph, .. } => Some(glyph),
        }
    }

    /// Rename this formula's glyph reference.
    ///
    /// Invalid input is rejected before mutation.
    pub fn rename_reference(
        &mut self,
        old: &str,
        new: &str,
    ) -> Result<bool, super::super::canonical_metadata::CanonicalMetadataError> {
        let Self::Reference { glyph, .. } = self else {
            return Ok(false);
        };
        if glyph != old || old == new {
            return Ok(false);
        }
        super::super::canonical_metadata::validate_name(new)?;
        *glyph = new.to_owned();
        Ok(true)
    }

    /// Resolve this formula from a referenced metric.
    ///
    /// Constants ignore `reference`.
    /// A non-finite input or result is rejected.
    pub fn evaluate(&self, reference: f64) -> Option<f64> {
        let result = match self {
            Self::Constant(value) => *value,
            Self::Reference { op, .. } => match op {
                None => reference,
                Some(('+', value)) => reference + value,
                Some(('-', value)) => reference - value,
                Some(('*', value)) => reference * value,
                Some(_) => return None,
            },
        };
        result.is_finite().then_some(result)
    }
}

/// Parse a Glyphs-style metrics key such as `=n+10`.
///
/// The leading `=` is optional.
/// `=|o` references the opposite sidebearing.
/// Arithmetic is recognized only when the suffix is a finite number, so a hyphenated glyph name
/// such as `beh-ar` remains a reference rather than a malformed subtraction.
pub fn parse_metrics_key(text: &str) -> Option<MetricsFormula> {
    let body = text.trim().trim_start_matches('=').trim();
    if body.is_empty() {
        return None;
    }
    if let Ok(value) = body.parse::<f64>() {
        return value.is_finite().then_some(MetricsFormula::Constant(value));
    }
    let (mirror, body) = match body.strip_prefix('|') {
        Some(rest) => (true, rest.trim()),
        None => (false, body),
    };
    let mut arithmetic = None;
    for (index, operator) in body.char_indices().rev() {
        if index == 0 || !matches!(operator, '+' | '-' | '*') {
            continue;
        }
        let Ok(value) = body[index + operator.len_utf8()..].trim().parse::<f64>() else {
            continue;
        };
        if value.is_finite() {
            arithmetic = Some((index, operator, value));
            break;
        }
    }
    let (glyph, op) = arithmetic.map_or((body.trim(), None), |(index, operator, value)| {
        (body[..index].trim(), Some((operator, value)))
    });
    super::super::canonical_metadata::validate_name(glyph).ok()?;
    Some(MetricsFormula::Reference {
        glyph: glyph.to_owned(),
        mirror,
        op,
    })
}

/// Canonical metadata stored with one glyph layer.
///
/// Codepoints and notes are GLIF fields, so auxiliary layers and different sources can preserve
/// distinct values without a source-font mirror.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CanonicalLayerGlyphMetadata {
    codepoints: Vec<char>,
    note: Option<String>,
}

impl CanonicalLayerGlyphMetadata {
    /// Construct layer metadata, retaining codepoint order and the first occurrence of each scalar.
    pub fn new(codepoints: impl IntoIterator<Item = char>, note: Option<String>) -> Self {
        let mut unique = HashSet::new();
        let codepoints = codepoints
            .into_iter()
            .filter(|codepoint| unique.insert(*codepoint))
            .collect();
        Self { codepoints, note }
    }

    /// Unicode scalar values in source order.
    pub fn codepoints(&self) -> &[char] {
        &self.codepoints
    }

    /// The optional glyph note, preserving the distinction between absent and empty.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// Replace the Unicode scalar values, retaining order and removing later duplicates.
    pub fn set_codepoints(&mut self, codepoints: impl IntoIterator<Item = char>) -> bool {
        let mut unique = HashSet::new();
        let codepoints: Vec<_> = codepoints
            .into_iter()
            .filter(|codepoint| unique.insert(*codepoint))
            .collect();
        if self.codepoints == codepoints {
            return false;
        }
        self.codepoints = codepoints;
        true
    }

    /// Replace the glyph note exactly.
    pub fn set_note(&mut self, note: Option<String>) -> bool {
        if self.note == note {
            return false;
        }
        self.note = note;
        true
    }
}

/// Canonical source-wide metadata for one glyph identity.
///
/// Export and category are font-lib values keyed by glyph name at the UFO boundary.
/// The live document attaches them to stable glyph identity instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalSourceGlyphMetadata {
    exported: bool,
    category: Option<OpenTypeGlyphCategory>,
}

impl Default for CanonicalSourceGlyphMetadata {
    fn default() -> Self {
        Self {
            exported: true,
            category: None,
        }
    }
}

impl CanonicalSourceGlyphMetadata {
    /// Construct source-wide metadata for one glyph identity.
    pub fn new(exported: bool, category: Option<OpenTypeGlyphCategory>) -> Self {
        Self { exported, category }
    }

    /// Whether the glyph participates in export.
    pub fn exported(&self) -> bool {
        self.exported
    }

    /// The explicit OpenType category, or `None` when it should be inferred.
    pub fn category(&self) -> Option<&OpenTypeGlyphCategory> {
        self.category.as_ref()
    }

    /// Change whether the glyph participates in export.
    pub fn set_exported(&mut self, exported: bool) -> bool {
        if self.exported == exported {
            return false;
        }
        self.exported = exported;
        true
    }

    /// Set or clear the explicit OpenType category.
    pub fn set_category(&mut self, category: Option<OpenTypeGlyphCategory>) -> bool {
        if self.category == category {
            return false;
        }
        self.category = category;
        true
    }
}

/// One default-layer UFO boundary value composed from canonical layer and source metadata.
///
/// This is a transfer value, not another live owner.
/// The glyph's mutable name remains in the document name index, and unknown glyph-lib entries
/// remain in the format-preservation payload.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CanonicalGlyphMetadata {
    layer: CanonicalLayerGlyphMetadata,
    source: CanonicalSourceGlyphMetadata,
}

impl CanonicalGlyphMetadata {
    /// Construct one boundary value from its layer-local and source-wide fields.
    pub fn new(
        codepoints: impl IntoIterator<Item = char>,
        note: Option<String>,
        exported: bool,
        category: Option<OpenTypeGlyphCategory>,
    ) -> Self {
        Self {
            layer: CanonicalLayerGlyphMetadata::new(codepoints, note),
            source: CanonicalSourceGlyphMetadata::new(exported, category),
        }
    }

    /// The layer-local canonical value.
    pub fn layer(&self) -> &CanonicalLayerGlyphMetadata {
        &self.layer
    }

    /// The source-wide canonical value for this glyph identity.
    pub fn source(&self) -> &CanonicalSourceGlyphMetadata {
        &self.source
    }

    /// Split this boundary value into its two canonical owners.
    pub fn into_parts(self) -> (CanonicalLayerGlyphMetadata, CanonicalSourceGlyphMetadata) {
        (self.layer, self.source)
    }

    /// Unicode scalar values in source order.
    pub fn codepoints(&self) -> &[char] {
        self.layer.codepoints()
    }

    /// The optional glyph note, preserving the distinction between absent and empty.
    pub fn note(&self) -> Option<&str> {
        self.layer.note()
    }

    /// Whether the glyph participates in export.
    pub fn exported(&self) -> bool {
        self.source.exported()
    }

    /// The explicit OpenType category, or `None` when it should be inferred.
    pub fn category(&self) -> Option<&OpenTypeGlyphCategory> {
        self.source.category()
    }

    /// Replace the Unicode scalar values, retaining order and removing later duplicates.
    pub fn set_codepoints(&mut self, codepoints: impl IntoIterator<Item = char>) -> bool {
        self.layer.set_codepoints(codepoints)
    }

    /// Replace the glyph note exactly.
    pub fn set_note(&mut self, note: Option<String>) -> bool {
        self.layer.set_note(note)
    }

    /// Change whether the glyph participates in export.
    pub fn set_exported(&mut self, exported: bool) -> bool {
        self.source.set_exported(exported)
    }

    /// Set or clear the explicit OpenType category.
    pub fn set_category(&mut self, category: Option<OpenTypeGlyphCategory>) -> bool {
        self.source.set_category(category)
    }
}

/// A rejected glyph-metadata input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlyphMetadataError {
    /// A token was not a valid Unicode scalar written in hexadecimal.
    InvalidCodepoint(String),
    /// The font-level skip-export value was not a unique list of glyph-name strings.
    InvalidSkipExportList,
    /// The font-level OpenType category value was not a string dictionary.
    InvalidCategoryMap,
    /// A requested glyph was absent from the UFO boundary object.
    MissingGlyph(String),
}

impl fmt::Display for GlyphMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCodepoint(value) => write!(f, "invalid Unicode scalar {value:?}"),
            Self::InvalidSkipExportList => {
                write!(f, "public.skipExportGlyphs is not a unique string list")
            }
            Self::InvalidCategoryMap => {
                write!(f, "public.openTypeCategories is not a string dictionary")
            }
            Self::MissingGlyph(name) => write!(f, "missing glyph {name:?}"),
        }
    }
}

impl std::error::Error for GlyphMetadataError {}

/// Parse one or more hexadecimal Unicode scalar values.
///
/// Values can be separated by commas or whitespace and may use `U+` or `0x` prefixes.
/// Empty input clears the encoding.
/// Duplicate values retain their first position.
pub fn parse_codepoints(input: &str) -> Result<Vec<char>, GlyphMetadataError> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let mut codepoints = Vec::new();
    for token in input.split(|character: char| character == ',' || character.is_whitespace()) {
        if token.is_empty() {
            continue;
        }
        let hex = token
            .strip_prefix("U+")
            .or_else(|| token.strip_prefix("u+"))
            .or_else(|| token.strip_prefix("0x"))
            .or_else(|| token.strip_prefix("0X"))
            .unwrap_or(token);
        let Some(codepoint) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) else {
            return Err(GlyphMetadataError::InvalidCodepoint(token.to_owned()));
        };
        if !codepoints.contains(&codepoint) {
            codepoints.push(codepoint);
        }
    }
    if codepoints.is_empty() {
        return Err(GlyphMetadataError::InvalidCodepoint(input.to_owned()));
    }
    Ok(codepoints)
}

/// Decode one glyph's canonical metadata from a UFO boundary object.
///
/// Font-level skip-export and category entries are interpreted without retaining the UFO font as
/// editable state.
/// Malformed standardized payloads are rejected instead of silently becoming opaque duplicates.
pub fn canonical_glyph_metadata_from_ufo(
    font: &norad::Font,
    glyph_name: &str,
) -> Result<CanonicalGlyphMetadata, GlyphMetadataError> {
    let glyph = font
        .get_glyph(glyph_name)
        .ok_or_else(|| GlyphMetadataError::MissingGlyph(glyph_name.to_owned()))?;
    let skipped = strict_skipped_exports(font)?;
    let categories = strict_categories(font)?;
    Ok(CanonicalGlyphMetadata::new(
        glyph.codepoints.iter(),
        glyph.note.clone(),
        !skipped.iter().any(|name| name == glyph_name),
        categories
            .and_then(|categories| categories.get(glyph_name))
            .and_then(plist::Value::as_string)
            .map(|value| OpenTypeGlyphCategory::from_source(value.to_owned())),
    ))
}

/// Encode one glyph's canonical metadata into a UFO boundary object atomically.
///
/// Unrelated skip-export and category entries remain unchanged and in their original order.
/// An unchanged value performs no mutation.
pub fn write_canonical_glyph_metadata_to_ufo(
    font: &mut norad::Font,
    glyph_name: &str,
    metadata: &CanonicalGlyphMetadata,
) -> Result<bool, GlyphMetadataError> {
    let glyph = font
        .get_glyph(glyph_name)
        .ok_or_else(|| GlyphMetadataError::MissingGlyph(glyph_name.to_owned()))?;
    let glyph_changed = glyph
        .codepoints
        .iter()
        .ne(metadata.codepoints().iter().copied())
        || glyph.note.as_deref() != metadata.note();

    let mut lib = font.lib.clone();
    let mut skipped = strict_skipped_exports(font)?;
    let is_skipped = skipped.iter().any(|name| name == glyph_name);
    if metadata.exported() {
        skipped.retain(|name| name != glyph_name);
    } else if !is_skipped {
        skipped.push(glyph_name.to_owned());
    }
    if skipped.is_empty() {
        lib.remove(SKIP_EXPORT_GLYPHS);
    } else {
        lib.insert(
            SKIP_EXPORT_GLYPHS.into(),
            plist::Value::Array(skipped.into_iter().map(plist::Value::String).collect()),
        );
    }

    let mut categories = strict_categories(font)?.cloned().unwrap_or_default();
    if let Some(category) = metadata.category() {
        categories.insert(
            glyph_name.into(),
            plist::Value::String(category.as_source().to_owned()),
        );
    } else {
        categories.remove(glyph_name);
    }
    if categories.is_empty() {
        lib.remove(OPEN_TYPE_CATEGORIES);
    } else {
        lib.insert(
            OPEN_TYPE_CATEGORIES.into(),
            plist::Value::Dictionary(categories),
        );
    }

    if !glyph_changed && font.lib == lib {
        return Ok(false);
    }
    font.lib = lib;
    let glyph = font
        .default_layer_mut()
        .get_glyph_mut(glyph_name)
        .expect("validated glyph remains present");
    glyph.codepoints = norad::Codepoints::new(metadata.codepoints().iter().copied());
    glyph.note = metadata.note().map(ToOwned::to_owned);
    Ok(true)
}

fn strict_skipped_exports(font: &norad::Font) -> Result<Vec<String>, GlyphMetadataError> {
    let Some(value) = font.lib.get(SKIP_EXPORT_GLYPHS) else {
        return Ok(Vec::new());
    };
    let Some(values) = value.as_array() else {
        return Err(GlyphMetadataError::InvalidSkipExportList);
    };
    let mut output = Vec::with_capacity(values.len());
    for value in values {
        let Some(name) = value.as_string() else {
            return Err(GlyphMetadataError::InvalidSkipExportList);
        };
        if output.iter().any(|candidate| candidate == name) {
            return Err(GlyphMetadataError::InvalidSkipExportList);
        }
        output.push(name.to_owned());
    }
    Ok(output)
}

fn strict_categories(font: &norad::Font) -> Result<Option<&plist::Dictionary>, GlyphMetadataError> {
    let Some(value) = font.lib.get(OPEN_TYPE_CATEGORIES) else {
        return Ok(None);
    };
    let Some(categories) = value.as_dictionary() else {
        return Err(GlyphMetadataError::InvalidCategoryMap);
    };
    if categories.values().any(|value| value.as_string().is_none()) {
        return Err(GlyphMetadataError::InvalidCategoryMap);
    }
    Ok(Some(categories))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Summary data for one glyph, enough to draw a glyph-grid cell without loading outlines.
pub struct GlyphMetadata {
    /// The glyph name, as in the UFO.
    pub name: String,
    /// Advance width in font units.
    pub width: f64,
    /// Number of contours in the glyph outline.
    pub contours: usize,
    /// The first codepoint as an uppercase hex string, or `None` when the glyph has no codepoint.
    pub unicode: Option<String>,
    #[serde(default)]
    /// All codepoints as uppercase hex strings; empty when the glyph is unencoded.
    pub unicodes: Vec<String>,
}

impl GlyphMetadata {
    /// Builds metadata from its parts and derives `unicode` from the first entry of `unicodes`.
    pub fn new(
        name: impl Into<String>,
        width: f64,
        contours: usize,
        unicodes: Vec<String>,
    ) -> Self {
        let unicode = unicodes.first().cloned();
        Self {
            name: name.into(),
            width,
            contours,
            unicode,
            unicodes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_unicode_is_compatibility_field() {
        let metadata =
            GlyphMetadata::new("A", 600.0, 2, vec!["0041".to_string(), "0391".to_string()]);

        assert_eq!(metadata.unicode.as_deref(), Some("0041"));
        assert_eq!(metadata.unicodes, ["0041", "0391"]);
    }

    #[test]
    fn glyph_without_codepoint_has_no_first_unicode() {
        let metadata = GlyphMetadata::new("glyph", 500.0, 0, Vec::new());

        assert_eq!(metadata.unicode, None);
        assert!(metadata.unicodes.is_empty());
    }

    #[test]
    fn canonical_metadata_retains_order_and_exact_unknown_category() {
        let mut metadata = CanonicalGlyphMetadata::new(
            ['A', '\u{391}', 'A'],
            Some(String::new()),
            false,
            Some(OpenTypeGlyphCategory::from_source("future-category")),
        );

        assert_eq!(metadata.codepoints(), ['A', '\u{391}']);
        assert_eq!(metadata.note(), Some(""));
        assert!(!metadata.exported());
        assert_eq!(
            metadata.category().map(OpenTypeGlyphCategory::as_source),
            Some("future-category")
        );
        let (layer, source) = metadata.clone().into_parts();
        assert_eq!(layer.codepoints(), ['A', '\u{391}']);
        assert_eq!(layer.note(), Some(""));
        assert!(!source.exported());
        assert_eq!(
            source.category().map(OpenTypeGlyphCategory::as_source),
            Some("future-category")
        );
        assert!(!metadata.set_codepoints(['A', '\u{391}']));
        assert!(metadata.set_exported(true));
        assert!(!metadata.set_exported(true));
    }

    #[test]
    fn parses_multiple_codepoint_spellings_atomically() {
        assert_eq!(
            parse_codepoints("U+0041, 0x0391 0041").unwrap(),
            ['A', '\u{391}']
        );
        assert_eq!(parse_codepoints("").unwrap(), Vec::<char>::new());
        assert_eq!(
            parse_codepoints("0041 D800"),
            Err(GlyphMetadataError::InvalidCodepoint("D800".into()))
        );
    }

    #[test]
    fn metrics_formulas_preserve_hyphenated_names_and_rename_atomically() {
        let mut formula = parse_metrics_key("=beh-ar*1.25").unwrap();
        assert_eq!(formula.referenced_glyph(), Some("beh-ar"));
        assert_eq!(formula.evaluate(80.0), Some(100.0));
        assert!(formula.rename_reference("beh-ar", "beh-ar.alt").unwrap());
        assert_eq!(formula.referenced_glyph(), Some("beh-ar.alt"));

        let before = formula.clone();
        assert!(formula.rename_reference("beh-ar.alt", "bad\0name").is_err());
        assert_eq!(formula, before);
        assert!(!formula.rename_reference("other", "bad\0name").unwrap());
        assert_eq!(formula, before);
        assert_eq!(
            MetricsFormula::Constant(50.0).evaluate(f64::NAN),
            Some(50.0)
        );
        assert_eq!(
            MetricsFormula::Reference {
                glyph: "n".into(),
                mirror: false,
                op: Some(('*', f64::MAX)),
            }
            .evaluate(f64::MAX),
            None
        );
    }

    #[test]
    fn mark_colors_and_metaballs_validate_as_canonical_values() {
        let color = MarkColor::parse("0.1234567890123456, 0.25, 0.5, 1").unwrap();
        assert_eq!(color.red, 0.123_456_789_012_345_6);
        assert!(color.is_valid());
        assert_eq!(MarkColor::parse("0,0,0,NaN"), None);

        let mut source = Metaballs {
            version: 1,
            groups: vec![MetaballGroup {
                id: 7,
                threshold: 1.0,
                balls: vec![Metaball {
                    id: 11,
                    x: 12.25,
                    y: -34.5,
                    radius: 80.125,
                    stiffness: -0.75,
                }],
            }],
        };
        assert_eq!(source.validate(), Ok(()));
        let duplicate = source.groups[0].balls[0].clone();
        source.groups[0].balls.push(duplicate);
        let invalid = source.clone();
        assert!(source.validate().is_err());
        assert_eq!(source, invalid);
    }

    #[test]
    fn component_alignment_preserves_exact_source_values_until_an_explicit_change() {
        for (value, disabled) in [
            (plist::Value::Boolean(false), true),
            (plist::Value::Integer((-7).into()), true),
            (plist::Value::Boolean(true), false),
            (plist::Value::String("future".into()), false),
        ] {
            let mut source = plist::Dictionary::from_iter([
                (String::from(COMPONENT_ALIGNMENT_KEY), value.clone()),
                (
                    String::from("future.key"),
                    plist::Value::String("exact".into()),
                ),
            ]);
            let original = source.clone();
            let mut alignment = ComponentAlignment::take_from_lib(&mut source);
            assert_eq!(alignment.is_disabled(), disabled);
            assert!(!source.contains_key(COMPONENT_ALIGNMENT_KEY));
            assert_eq!(
                source.get("future.key"),
                Some(&plist::Value::String("exact".into()))
            );

            assert!(!alignment.set_disabled(disabled));
            assert!(alignment.write_to_lib(&mut source));
            assert_eq!(source, original);
            assert!(!alignment.write_to_lib(&mut source));

            assert!(alignment.set_disabled(!disabled));
            assert_eq!(alignment.is_disabled(), !disabled);
            assert!(alignment.write_to_lib(&mut source));
            if disabled {
                assert!(!source.contains_key(COMPONENT_ALIGNMENT_KEY));
            } else {
                assert_eq!(
                    source
                        .get(COMPONENT_ALIGNMENT_KEY)
                        .and_then(plist::Value::as_signed_integer),
                    Some(-1)
                );
            }
            assert_eq!(
                source.get("future.key"),
                Some(&plist::Value::String("exact".into()))
            );
        }
    }

    #[test]
    fn canonical_ufo_boundary_preserves_unrelated_font_metadata() {
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("A");
        glyph.codepoints = norad::Codepoints::new(['A']);
        glyph.note = Some(String::new());
        font.default_layer_mut().insert_glyph(glyph);
        font.lib.insert(
            SKIP_EXPORT_GLYPHS.into(),
            plist::Value::Array(vec![
                plist::Value::String("future".into()),
                plist::Value::String("A".into()),
            ]),
        );
        let categories = plist::Dictionary::from_iter([
            (
                "future".to_string(),
                plist::Value::String("future-category".into()),
            ),
            ("A".to_string(), plist::Value::String("component".into())),
        ]);
        font.lib.insert(
            OPEN_TYPE_CATEGORIES.into(),
            plist::Value::Dictionary(categories),
        );

        let mut metadata = canonical_glyph_metadata_from_ufo(&font, "A").unwrap();
        assert!(!metadata.exported());
        assert_eq!(metadata.note(), Some(""));
        assert_eq!(metadata.category(), Some(&OpenTypeGlyphCategory::Component));
        let before = font.clone();
        assert!(!write_canonical_glyph_metadata_to_ufo(&mut font, "A", &metadata).unwrap());
        assert_eq!(font, before);
        assert!(metadata.set_codepoints(['A', '\u{391}']));
        assert!(metadata.set_note(Some("edited".into())));
        assert!(metadata.set_exported(true));
        assert!(metadata.set_category(Some(OpenTypeGlyphCategory::Mark)));

        assert!(write_canonical_glyph_metadata_to_ufo(&mut font, "A", &metadata).unwrap());
        assert!(!write_canonical_glyph_metadata_to_ufo(&mut font, "A", &metadata).unwrap());
        assert_eq!(
            strict_skipped_exports(&font).unwrap(),
            vec!["future".to_string()]
        );
        let categories = strict_categories(&font).unwrap().unwrap();
        assert_eq!(
            categories.get("future").and_then(plist::Value::as_string),
            Some("future-category")
        );
        assert_eq!(
            categories.get("A").and_then(plist::Value::as_string),
            Some("mark")
        );
        let glyph = font.get_glyph("A").unwrap();
        assert_eq!(
            glyph.codepoints.iter().collect::<Vec<_>>(),
            ['A', '\u{391}']
        );
        assert_eq!(glyph.note.as_deref(), Some("edited"));
    }

    #[test]
    fn malformed_standard_metadata_is_rejected_before_mutation() {
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("A"));
        font.lib
            .insert(SKIP_EXPORT_GLYPHS.into(), plist::Value::String("A".into()));
        let before = font.clone();

        assert_eq!(
            write_canonical_glyph_metadata_to_ufo(
                &mut font,
                "A",
                &CanonicalGlyphMetadata::default()
            ),
            Err(GlyphMetadataError::InvalidSkipExportList)
        );
        assert_eq!(font, before);
    }
}
