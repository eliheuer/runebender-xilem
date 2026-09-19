// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical variable-font structure and the explicit Designspace codec boundary.
//!
//! These values contain no Norad types. Import decodes a supported Designspace once;
//! export materializes a temporary codec document and checks every `f64` narrowing.

use std::collections::{HashMap, HashSet};

use super::super::axis::Axis;
use super::super::var_model::{Location, denormalize_value};
use super::super::variable::{LayerId, SourceId};

const DEFAULT_LAYER_NAME: &str = "public.default";

macro_rules! stable_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(usize);

        impl $name {
            /// The session-local numeric value, for diagnostics and stable snapshots.
            pub fn get(self) -> usize {
                self.0
            }

            pub(crate) const fn from_index(index: usize) -> Self {
                Self(index)
            }
        }
    };
}

stable_id!(AxisId, "Stable identity of one continuous design axis.");
stable_id!(
    InstanceId,
    "Stable identity of one named variable-font instance."
);
stable_id!(
    RuleId,
    "Stable identity of one ordered Designspace substitution rule."
);

/// Project-assigned identities used while decoding one Designspace.
#[derive(Clone, Debug)]
pub struct CanonicalDesignspaceIdentities {
    /// Axis identities in Designspace axis order.
    pub axes: Vec<AxisId>,
    /// Full source and default-layer identities in full-source order.
    pub sources: Vec<(SourceId, LayerId)>,
    /// Instance identities in Designspace instance order.
    pub instances: Vec<InstanceId>,
    /// Rule identities in Designspace rule order.
    pub rules: Vec<RuleId>,
}

/// One localized axis label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalizedName {
    /// BCP 47 language tag as stored by Designspace.
    pub language: String,
    /// Localized label text.
    pub value: String,
}

/// A continuous axis with one authoritative user-to-design mapping.
#[derive(Clone, Debug)]
pub struct CanonicalAxis {
    id: AxisId,
    /// Exact user-space bounds, OpenType tag and optional user-to-design map.
    pub coordinates: Axis,
    /// Whether user interfaces should hide the axis.
    pub hidden: bool,
    /// Ordered localized axis labels.
    pub labels: Vec<LocalizedName>,
    minimum_was_explicit: bool,
    maximum_was_explicit: bool,
    map_was_present: bool,
}

impl PartialEq for CanonicalAxis {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.coordinates.name == other.coordinates.name
            && self.coordinates.tag == other.coordinates.tag
            && self.coordinates.min == other.coordinates.min
            && self.coordinates.default == other.coordinates.default
            && self.coordinates.max == other.coordinates.max
            && self.coordinates.map == other.coordinates.map
            && self.hidden == other.hidden
            && self.labels == other.labels
            && self.minimum_was_explicit == other.minimum_was_explicit
            && self.maximum_was_explicit == other.maximum_was_explicit
            && self.map_was_present == other.map_was_present
    }
}

impl CanonicalAxis {
    /// Stable identity retained across reorder and edits.
    pub fn id(&self) -> AxisId {
        self.id
    }

    /// Minimum design-space value, derived through the pinned axis backend.
    pub fn design_minimum(&self) -> f64 {
        self.coordinates.user_to_design(self.coordinates.min)
    }

    /// Default design-space value, derived through the pinned axis backend.
    pub fn design_default(&self) -> f64 {
        self.coordinates.user_to_design(self.coordinates.default)
    }

    /// Maximum design-space value, derived through the pinned axis backend.
    pub fn design_maximum(&self) -> f64 {
        self.coordinates.user_to_design(self.coordinates.max)
    }
}

/// Which coordinate representation one Designspace dimension used.
///
/// Only one value is authoritative; user, design and normalized alternatives are derived.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CoordinateValue {
    /// User-space coordinate, mapped through its axis when needed.
    User(f64),
    /// Design-space coordinate.
    Design(f64),
}

/// An ordered sparse location keyed by stable axis identity.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CanonicalLocation {
    coordinates: Vec<(AxisId, CoordinateValue)>,
}

impl CanonicalLocation {
    /// Explicit dimensions in their source order.
    pub fn coordinates(&self) -> &[(AxisId, CoordinateValue)] {
        &self.coordinates
    }

    /// Normalized coordinate for one axis, defaulting an omitted dimension to zero.
    pub fn normalized(&self, axis: &CanonicalAxis) -> f64 {
        match self
            .coordinates
            .iter()
            .find(|(id, _)| *id == axis.id)
            .map(|(_, value)| *value)
        {
            Some(CoordinateValue::User(value)) => axis.coordinates.user_to_normalized(value),
            Some(CoordinateValue::Design(value)) => axis.coordinates.design_to_normalized(value),
            None => 0.0,
        }
    }

    /// Design coordinate for one axis, defaulting an omitted dimension to its mapped default.
    pub fn design(&self, axis: &CanonicalAxis) -> f64 {
        match self
            .coordinates
            .iter()
            .find(|(id, _)| *id == axis.id)
            .map(|(_, value)| *value)
        {
            Some(CoordinateValue::User(value)) => axis.coordinates.user_to_design(value),
            Some(CoordinateValue::Design(value)) => value,
            None => axis.design_default(),
        }
    }

    /// Convert to the normalized name-keyed location used by interpolation.
    pub fn to_normalized(&self, axes: &[CanonicalAxis]) -> Result<Location, String> {
        let known: HashSet<_> = axes.iter().map(|axis| axis.id).collect();
        validate_location(self, &known)?;
        Ok(axes
            .iter()
            .map(|axis| (axis.coordinates.name.clone(), self.normalized(axis)))
            .collect())
    }

    /// Capture a normalized interpolation location as exact design coordinates.
    pub fn from_normalized(location: &Location, axes: &[CanonicalAxis]) -> Result<Self, String> {
        if location
            .keys()
            .any(|name| !axes.iter().any(|axis| axis.coordinates.name == *name))
            || location
                .values()
                .any(|value| !value.is_finite() || !(-1.0..=1.0).contains(value))
        {
            return Err("location needs known finite normalized coordinates in -1..1".into());
        }
        Ok(Self {
            coordinates: axes
                .iter()
                .map(|axis| {
                    let normalized = location.get(&axis.coordinates.name).copied().unwrap_or(0.0);
                    (
                        axis.id,
                        CoordinateValue::Design(denormalize_value(
                            normalized,
                            axis.design_minimum(),
                            axis.design_default(),
                            axis.design_maximum(),
                        )),
                    )
                })
                .collect(),
        })
    }
}

/// A full editable source described by stable identity rather than display index.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceDescriptor {
    id: SourceId,
    /// Optional Designspace family name.
    pub family_name: Option<String>,
    /// Optional Designspace style name.
    pub style_name: Option<String>,
    /// Optional unique Designspace source name.
    pub name: Option<String>,
    /// Persistence destination relative to the Designspace.
    pub filename: String,
    /// Exact source location.
    pub location: CanonicalLocation,
    /// Stable default-layer address for glyph queries.
    pub default_layer: LayerId,
}

impl SourceDescriptor {
    /// Construct a full source using a Project-assigned identity.
    pub fn new(
        id: SourceId,
        filename: String,
        location: CanonicalLocation,
        default_layer: LayerId,
    ) -> Result<Self, String> {
        if filename.is_empty() || default_layer.source != id {
            return Err("source needs a filename and an owned default layer".into());
        }
        Ok(Self {
            id,
            family_name: None,
            style_name: None,
            name: None,
            filename,
            location,
            default_layer,
        })
    }

    /// Stable source identity.
    pub fn id(&self) -> SourceId {
        self.id
    }

    /// Display name without storing a second editable copy.
    pub fn display_name(&self) -> &str {
        self.style_name
            .as_deref()
            .or(self.name.as_deref())
            .unwrap_or(&self.filename)
    }
}

/// A glyph-specific intermediate source attached to a source layer.
#[derive(Clone, Debug, PartialEq)]
pub struct SparseSourceDescriptor {
    /// Stable owning source and layer address; never a master index.
    pub layer: LayerId,
    /// Optional Designspace family name.
    pub family_name: Option<String>,
    /// Optional Designspace style name.
    pub style_name: Option<String>,
    /// Optional unique Designspace source name.
    pub name: Option<String>,
    /// Exact intermediate location.
    pub location: CanonicalLocation,
}

impl SparseSourceDescriptor {
    /// Construct a sparse source using a Project-assigned layer identity.
    pub fn new(layer: LayerId, location: CanonicalLocation) -> Self {
        Self {
            layer,
            family_name: None,
            style_name: None,
            name: None,
            location,
        }
    }
}

/// Stable entry in Designspace source order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceOrderEntry {
    /// A full source.
    Full(SourceId),
    /// A sparse/intermediate layer source.
    Sparse(LayerId),
}

/// A named output instance with supported Designspace metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalInstance {
    id: InstanceId,
    /// Optional family name.
    pub family_name: Option<String>,
    /// Optional style name.
    pub style_name: Option<String>,
    /// Optional unique instance name.
    pub name: Option<String>,
    /// Optional output filename.
    pub filename: Option<String>,
    /// Optional PostScript name.
    pub postscript_name: Option<String>,
    /// Optional style-map family name.
    pub style_map_family_name: Option<String>,
    /// Optional style-map style name.
    pub style_map_style_name: Option<String>,
    /// Exact instance location.
    pub location: CanonicalLocation,
    /// Opaque instance lib retained at this stable identity.
    pub lib: plist::Dictionary,
}

impl CanonicalInstance {
    /// Stable identity retained across reorder and edits.
    pub fn id(&self) -> InstanceId {
        self.id
    }

    /// Preferred display name without a second editable value.
    pub fn display_name(&self) -> &str {
        self.style_name
            .as_deref()
            .or(self.name.as_deref())
            .unwrap_or("Instance")
    }
}

/// Whether Designspace substitutions run before or after ordinary features.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RuleProcessing {
    /// Apply rules before ordinary substitution features.
    #[default]
    First,
    /// Apply rules after ordinary substitution features.
    Last,
}

/// One bounded axis condition in design coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalCondition {
    /// Stable axis identity.
    pub axis: AxisId,
    /// Optional inclusive minimum in design coordinates.
    pub minimum: Option<f64>,
    /// Optional inclusive maximum in design coordinates.
    pub maximum: Option<f64>,
}

/// Conditions that must all hold for one rule alternative.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CanonicalConditionSet {
    /// Ordered axis conditions.
    pub conditions: Vec<CanonicalCondition>,
}

/// One glyph-name substitution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalSubstitution {
    /// Input glyph name.
    pub name: String,
    /// Replacement glyph name.
    pub replacement: String,
}

/// One ordered Designspace substitution rule.
#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalRule {
    id: RuleId,
    /// Optional descriptive name.
    pub name: Option<String>,
    /// Ordered alternatives; every condition within one set must hold.
    pub condition_sets: Vec<CanonicalConditionSet>,
    /// Ordered substitutions.
    pub substitutions: Vec<CanonicalSubstitution>,
}

impl CanonicalRule {
    /// Stable identity retained across reorder and edits.
    pub fn id(&self) -> RuleId {
        self.id
    }
}

/// Immutable structural input consumed by interpolation and compilation.
#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalCompilerStructure {
    /// Ordered continuous axes.
    pub axes: Vec<CanonicalAxis>,
    /// Ordered full source descriptors.
    pub sources: Vec<SourceDescriptor>,
    /// Exact full/sparse Designspace source order.
    pub source_order: Vec<SourceOrderEntry>,
    /// Ordered named instances.
    pub instances: Vec<CanonicalInstance>,
    /// Rule processing order.
    pub rule_processing: RuleProcessing,
    /// Ordered substitution rules.
    pub rules: Vec<CanonicalRule>,
    /// Ordered sparse source descriptors.
    pub sparse_sources: Vec<SparseSourceDescriptor>,
}

/// Complete supported Designspace structure with stable document identities.
#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalDesignspace {
    format: f64,
    axes: Vec<CanonicalAxis>,
    sources: Vec<SourceDescriptor>,
    sparse_sources: Vec<SparseSourceDescriptor>,
    source_order: Vec<SourceOrderEntry>,
    instances: Vec<CanonicalInstance>,
    rule_processing: RuleProcessing,
    rules: Vec<CanonicalRule>,
    lib: plist::Dictionary,
    preserved_empty_axis_mappings: bool,
}

impl CanonicalDesignspace {
    /// Decode a checked Norad Designspace value at the format boundary.
    pub fn from_norad(doc: &norad::designspace::DesignSpaceDocument) -> Result<Self, String> {
        let identities = doc
            .sources
            .iter()
            .filter(|source| source.layer.is_none())
            .enumerate()
            .map(|(index, _)| {
                let source = SourceId(index);
                (
                    source,
                    LayerId {
                        source,
                        name: DEFAULT_LAYER_NAME.into(),
                    },
                )
            });
        Self::from_norad_with_source_identities(doc, identities)
    }

    /// Decode using Project-assigned full-source and default-layer identities.
    ///
    /// Identities must be supplied in full-source order; sparse layers inherit the
    /// corresponding source identity without inferring it from a mutable master index.
    pub fn from_norad_with_source_identities(
        doc: &norad::designspace::DesignSpaceDocument,
        identities: impl IntoIterator<Item = (SourceId, LayerId)>,
    ) -> Result<Self, String> {
        Self::from_norad_with_identities(
            doc,
            CanonicalDesignspaceIdentities {
                axes: (0..doc.axes.len()).map(AxisId::from_index).collect(),
                sources: identities.into_iter().collect(),
                instances: (0..doc.instances.len())
                    .map(InstanceId::from_index)
                    .collect(),
                rules: (0..doc.rules.rules.len()).map(RuleId::from_index).collect(),
            },
        )
    }

    /// Decode using Project-assigned identities for every structural entity.
    pub fn from_norad_with_identities(
        doc: &norad::designspace::DesignSpaceDocument,
        identities: CanonicalDesignspaceIdentities,
    ) -> Result<Self, String> {
        if doc
            .axis_mappings
            .as_ref()
            .is_some_and(|mappings| !mappings.is_empty())
        {
            return Err("cross-axis mappings are not supported".into());
        }
        let full_source_count = doc
            .sources
            .iter()
            .filter(|source| source.layer.is_none())
            .count();
        if identities.axes.len() != doc.axes.len()
            || identities.sources.len() != full_source_count
            || identities.instances.len() != doc.instances.len()
            || identities.rules.len() != doc.rules.rules.len()
        {
            return Err("identity counts do not match Designspace structure".into());
        }
        let mut axis_names = HashSet::new();
        let mut axis_tags = HashSet::new();
        let mut axes = Vec::with_capacity(doc.axes.len());
        for (axis, id) in doc.axes.iter().zip(identities.axes) {
            if !axis_names.insert(axis.name.clone()) || !axis_tags.insert(axis.tag.clone()) {
                return Err("designspace axes must have unique names and tags".into());
            }
            if axis
                .values
                .as_ref()
                .is_some_and(|values| !values.is_empty())
            {
                return Err(format!("{}: discrete axes are not supported", axis.name));
            }
            let coordinates = Axis {
                name: axis.name.clone(),
                tag: axis.tag.clone(),
                min: f64::from(axis.minimum.unwrap_or(axis.default)),
                default: f64::from(axis.default),
                max: f64::from(axis.maximum.unwrap_or(axis.default)),
                map: axis
                    .map
                    .iter()
                    .flatten()
                    .map(|mapping| (f64::from(mapping.input), f64::from(mapping.output)))
                    .collect(),
            };
            coordinates.validate()?;
            axes.push(CanonicalAxis {
                id,
                coordinates,
                hidden: axis.hidden,
                labels: axis
                    .label_names
                    .iter()
                    .map(|label| LocalizedName {
                        language: label.language.clone(),
                        value: label.string.clone(),
                    })
                    .collect(),
                minimum_was_explicit: axis.minimum.is_some(),
                maximum_was_explicit: axis.maximum.is_some(),
                map_was_present: axis.map.is_some(),
            });
        }
        let axis_ids: HashMap<_, _> = axes
            .iter()
            .map(|axis| (axis.coordinates.name.as_str(), axis.id))
            .collect();

        let mut source_ids = HashMap::new();
        let mut sources = Vec::new();
        for (source, (id, default_layer)) in doc
            .sources
            .iter()
            .filter(|source| source.layer.is_none())
            .zip(identities.sources)
        {
            if source_ids.contains_key(&source.filename) {
                return Err(format!("duplicate full source file {}", source.filename));
            }
            if default_layer.source != id
                || sources
                    .iter()
                    .any(|source: &SourceDescriptor| source.id == id)
            {
                return Err("source identities must be unique and own their default layers".into());
            }
            source_ids.insert(source.filename.clone(), id);
            sources.push(SourceDescriptor {
                id,
                family_name: source.familyname.clone(),
                style_name: source.stylename.clone(),
                name: source.name.clone(),
                filename: source.filename.clone(),
                location: import_location(&source.location, &axis_ids, "source")?,
                default_layer,
            });
        }
        if sources.is_empty() {
            return Err("designspace has no full sources".into());
        }

        let mut sparse_sources = Vec::new();
        let mut source_order = Vec::with_capacity(doc.sources.len());
        let mut sparse_layers = HashSet::new();
        for source in &doc.sources {
            if let Some(layer_name) = &source.layer {
                let source_id = source_ids.get(&source.filename).copied().ok_or_else(|| {
                    format!(
                        "layer source {} requires a full source from the same UFO",
                        source.filename
                    )
                })?;
                let layer = LayerId {
                    source: source_id,
                    name: layer_name.clone(),
                };
                if !sparse_layers.insert(layer.clone()) {
                    return Err(format!("duplicate sparse layer {}", layer.name));
                }
                sparse_sources.push(SparseSourceDescriptor {
                    layer: layer.clone(),
                    family_name: source.familyname.clone(),
                    style_name: source.stylename.clone(),
                    name: source.name.clone(),
                    location: import_location(&source.location, &axis_ids, "sparse source")?,
                });
                source_order.push(SourceOrderEntry::Sparse(layer));
            } else {
                source_order.push(SourceOrderEntry::Full(source_ids[&source.filename]));
            }
        }

        let instances = doc
            .instances
            .iter()
            .zip(identities.instances)
            .map(|(instance, id)| {
                Ok(CanonicalInstance {
                    id,
                    family_name: instance.familyname.clone(),
                    style_name: instance.stylename.clone(),
                    name: instance.name.clone(),
                    filename: instance.filename.clone(),
                    postscript_name: instance.postscriptfontname.clone(),
                    style_map_family_name: instance.stylemapfamilyname.clone(),
                    style_map_style_name: instance.stylemapstylename.clone(),
                    location: import_location(&instance.location, &axis_ids, "instance")?,
                    lib: instance.lib.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let rules = doc
            .rules
            .rules
            .iter()
            .zip(identities.rules)
            .map(|(rule, id)| import_rule(id, rule, &axis_ids))
            .collect::<Result<Vec<_>, _>>()?;
        let result = Self {
            format: f64::from(doc.format),
            axes,
            sources,
            sparse_sources,
            source_order,
            instances,
            rule_processing: match doc.rules.processing {
                norad::designspace::RuleProcessing::First => RuleProcessing::First,
                norad::designspace::RuleProcessing::Last => RuleProcessing::Last,
            },
            rules,
            lib: doc.lib.clone(),
            preserved_empty_axis_mappings: doc.axis_mappings.is_some(),
        };
        result.validate()?;
        Ok(result)
    }

    /// Materialize one temporary Norad value for Designspace serialization.
    pub fn to_norad(&self) -> Result<norad::designspace::DesignSpaceDocument, String> {
        self.validate()?;
        let axes = self
            .axes
            .iter()
            .map(export_axis)
            .collect::<Result<Vec<_>, _>>()?;
        let axis_names: HashMap<_, _> = self
            .axes
            .iter()
            .map(|axis| (axis.id, axis.coordinates.name.as_str()))
            .collect();
        let full_sources: HashMap<_, _> = self
            .sources
            .iter()
            .map(|source| (source.id, source))
            .collect();
        let sparse_sources: HashMap<_, _> = self
            .sparse_sources
            .iter()
            .map(|source| (source.layer.clone(), source))
            .collect();
        let mut sources = Vec::with_capacity(self.source_order.len());
        for entry in &self.source_order {
            match entry {
                SourceOrderEntry::Full(id) => {
                    let source = full_sources
                        .get(id)
                        .ok_or("source order refers to missing source")?;
                    sources.push(norad::designspace::Source {
                        familyname: source.family_name.clone(),
                        stylename: source.style_name.clone(),
                        name: source.name.clone(),
                        filename: source.filename.clone(),
                        layer: None,
                        location: export_location(&source.location, &axis_names)?,
                    });
                }
                SourceOrderEntry::Sparse(layer) => {
                    let source = sparse_sources
                        .get(layer)
                        .ok_or("source order refers to missing sparse source")?;
                    let owner = full_sources
                        .get(&layer.source)
                        .ok_or("sparse source owner is missing")?;
                    sources.push(norad::designspace::Source {
                        familyname: source.family_name.clone(),
                        stylename: source.style_name.clone(),
                        name: source.name.clone(),
                        filename: owner.filename.clone(),
                        layer: Some(layer.name.clone()),
                        location: export_location(&source.location, &axis_names)?,
                    });
                }
            }
        }
        let instances = self
            .instances
            .iter()
            .map(|instance| {
                Ok(norad::designspace::Instance {
                    familyname: instance.family_name.clone(),
                    stylename: instance.style_name.clone(),
                    name: instance.name.clone(),
                    filename: instance.filename.clone(),
                    postscriptfontname: instance.postscript_name.clone(),
                    stylemapfamilyname: instance.style_map_family_name.clone(),
                    stylemapstylename: instance.style_map_style_name.clone(),
                    location: export_location(&instance.location, &axis_names)?,
                    lib: instance.lib.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let rules = self
            .rules
            .iter()
            .map(|rule| export_rule(rule, &axis_names))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(norad::designspace::DesignSpaceDocument {
            format: checked_f32("format", self.format)?,
            axes,
            axis_mappings: self
                .preserved_empty_axis_mappings
                .then(norad::designspace::AxisMappings::default),
            rules: norad::designspace::Rules {
                processing: match self.rule_processing {
                    RuleProcessing::First => norad::designspace::RuleProcessing::First,
                    RuleProcessing::Last => norad::designspace::RuleProcessing::Last,
                },
                rules,
            },
            sources,
            instances,
            lib: self.lib.clone(),
        })
    }

    /// Ordered canonical axes.
    pub fn axes(&self) -> &[CanonicalAxis] {
        &self.axes
    }

    /// Ordered full source descriptors.
    pub fn sources(&self) -> &[SourceDescriptor] {
        &self.sources
    }

    /// Ordered sparse/intermediate source descriptors.
    pub fn sparse_sources(&self) -> &[SparseSourceDescriptor] {
        &self.sparse_sources
    }

    /// Exact Designspace source order.
    pub fn source_order(&self) -> &[SourceOrderEntry] {
        &self.source_order
    }

    /// Full-source display order, excluding interleaved sparse entries.
    pub fn full_source_order(&self) -> impl DoubleEndedIterator<Item = SourceId> + '_ {
        self.source_order.iter().filter_map(|entry| match entry {
            SourceOrderEntry::Full(source) => Some(*source),
            SourceOrderEntry::Sparse(_) => None,
        })
    }

    /// Ordered canonical instances.
    pub fn instances(&self) -> &[CanonicalInstance] {
        &self.instances
    }

    /// Ordered canonical rules.
    pub fn rules(&self) -> &[CanonicalRule] {
        &self.rules
    }

    /// Rule processing order.
    pub fn rule_processing(&self) -> RuleProcessing {
        self.rule_processing
    }

    /// Owned immutable values needed by interpolation and compilation.
    pub fn compiler_structure(&self) -> CanonicalCompilerStructure {
        CanonicalCompilerStructure {
            axes: self.axes.clone(),
            sources: self.sources.clone(),
            source_order: self.source_order.clone(),
            instances: self.instances.clone(),
            rule_processing: self.rule_processing,
            rules: self.rules.clone(),
            sparse_sources: self.sparse_sources.clone(),
        }
    }

    /// Apply an edit atomically after validating identities, coordinates and export precision.
    ///
    /// The closure edits an isolated clone. An error or invalid candidate leaves `self` unchanged.
    pub fn edit_checked(
        &mut self,
        edit: impl FnOnce(&mut Self) -> Result<(), String>,
    ) -> Result<bool, String> {
        let mut candidate = self.clone();
        edit(&mut candidate)?;
        candidate.validate()?;
        candidate.to_norad()?;
        if candidate == *self {
            return Ok(false);
        }
        *self = candidate;
        Ok(true)
    }

    /// Find one axis mutably within a checked edit draft.
    pub fn axis_mut(&mut self, id: AxisId) -> Option<&mut CanonicalAxis> {
        self.axes.iter_mut().find(|axis| axis.id == id)
    }

    /// Find one full source mutably within a checked edit draft.
    pub fn source_mut(&mut self, id: SourceId) -> Option<&mut SourceDescriptor> {
        self.sources.iter_mut().find(|source| source.id == id)
    }

    /// Find one sparse source mutably within a checked edit draft.
    pub fn sparse_source_mut(&mut self, layer: &LayerId) -> Option<&mut SparseSourceDescriptor> {
        self.sparse_sources
            .iter_mut()
            .find(|source| &source.layer == layer)
    }

    /// Find one instance mutably within a checked edit draft.
    pub fn instance_mut(&mut self, id: InstanceId) -> Option<&mut CanonicalInstance> {
        self.instances.iter_mut().find(|instance| instance.id == id)
    }

    /// Remove one instance by stable identity.
    pub fn remove_instance(&mut self, id: InstanceId) -> Option<CanonicalInstance> {
        let index = self
            .instances
            .iter()
            .position(|instance| instance.id == id)?;
        Some(self.instances.remove(index))
    }

    /// Find one rule mutably within a checked edit draft.
    pub fn rule_mut(&mut self, id: RuleId) -> Option<&mut CanonicalRule> {
        self.rules.iter_mut().find(|rule| rule.id == id)
    }

    /// Insert a Project-identified full source at one full-source display position.
    pub fn insert_source(
        &mut self,
        source: SourceDescriptor,
        display_index: usize,
    ) -> Result<(), String> {
        if self.sources.iter().any(|current| {
            current.id == source.id || current.filename.eq_ignore_ascii_case(&source.filename)
        }) {
            return Err("source identity or filename is already present".into());
        }
        let full_positions = self
            .source_order
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                matches!(entry, SourceOrderEntry::Full(_)).then_some(index)
            })
            .collect::<Vec<_>>();
        if display_index > full_positions.len() {
            return Err("source display index is out of bounds".into());
        }
        let order_index = full_positions
            .get(display_index)
            .copied()
            .unwrap_or(self.source_order.len());
        let id = source.id;
        self.sources.push(source);
        self.source_order
            .insert(order_index, SourceOrderEntry::Full(id));
        self.reorder_source_storage();
        Ok(())
    }

    /// Remove a full source and its attached sparse descriptors without touching files.
    pub fn remove_source(
        &mut self,
        id: SourceId,
    ) -> Option<(SourceDescriptor, Vec<SparseSourceDescriptor>)> {
        let source_index = self.sources.iter().position(|source| source.id == id)?;
        let source = self.sources.remove(source_index);
        let mut removed_sparse = Vec::new();
        self.sparse_sources.retain(|sparse| {
            if sparse.layer.source == id {
                removed_sparse.push(sparse.clone());
                false
            } else {
                true
            }
        });
        self.source_order.retain(|entry| match entry {
            SourceOrderEntry::Full(source) => *source != id,
            SourceOrderEntry::Sparse(layer) => layer.source != id,
        });
        Some((source, removed_sparse))
    }

    /// Move one full source while preserving stable identity and sparse-entry order.
    pub fn move_source(&mut self, id: SourceId, display_index: usize) -> Result<bool, String> {
        let current_order = self.full_source_order().collect::<Vec<_>>();
        let current = current_order
            .iter()
            .position(|source| *source == id)
            .ok_or("unknown source identity")?;
        if display_index >= current_order.len() {
            return Err("source display index is out of bounds".into());
        }
        if current == display_index {
            return Ok(false);
        }
        let mut reordered = current_order;
        let moved = reordered.remove(current);
        reordered.insert(display_index, moved);
        for (entry, source) in self
            .source_order
            .iter_mut()
            .filter(|entry| matches!(entry, SourceOrderEntry::Full(_)))
            .zip(reordered)
        {
            *entry = SourceOrderEntry::Full(source);
        }
        self.reorder_source_storage();
        Ok(true)
    }

    /// Insert one sparse descriptor at an exact Designspace source-order position.
    pub fn insert_sparse_source(
        &mut self,
        source: SparseSourceDescriptor,
        order_index: usize,
    ) -> Result<(), String> {
        if order_index > self.source_order.len()
            || !self
                .sources
                .iter()
                .any(|owner| owner.id == source.layer.source)
            || self
                .sparse_sources
                .iter()
                .any(|current| current.layer == source.layer)
        {
            return Err("invalid sparse source owner, identity or order index".into());
        }
        self.source_order
            .insert(order_index, SourceOrderEntry::Sparse(source.layer.clone()));
        self.sparse_sources.push(source);
        Ok(())
    }

    /// Remove one sparse descriptor by stable layer identity.
    pub fn remove_sparse_source(&mut self, layer: &LayerId) -> Option<SparseSourceDescriptor> {
        let index = self
            .sparse_sources
            .iter()
            .position(|source| &source.layer == layer)?;
        let source = self.sparse_sources.remove(index);
        self.source_order
            .retain(|entry| entry != &SourceOrderEntry::Sparse(layer.clone()));
        Some(source)
    }

    /// Remove sparse entries superseded by a full source at one normalized location.
    pub fn remove_sparse_at_normalized_location(
        &mut self,
        location: &Location,
    ) -> Result<Vec<SparseSourceDescriptor>, String> {
        let target =
            CanonicalLocation::from_normalized(location, &self.axes)?.to_normalized(&self.axes)?;
        let layers = self
            .sparse_sources
            .iter()
            .map(|source| {
                Ok((source.location.to_normalized(&self.axes)? == target)
                    .then_some(source.layer.clone()))
            })
            .collect::<Result<Vec<_>, String>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        Ok(layers
            .iter()
            .filter_map(|layer| self.remove_sparse_source(layer))
            .collect())
    }

    fn reorder_source_storage(&mut self) {
        let order = self.full_source_order().collect::<Vec<_>>();
        self.sources
            .sort_by_key(|source| order.iter().position(|id| *id == source.id));
    }

    fn validate(&self) -> Result<(), String> {
        checked_f32("format", self.format)?;
        let mut axis_ids = HashSet::new();
        let mut axis_names = HashSet::new();
        let mut axis_tags = HashSet::new();
        for axis in &self.axes {
            if !axis_ids.insert(axis.id)
                || !axis_names.insert(axis.coordinates.name.as_str())
                || !axis_tags.insert(axis.coordinates.tag.as_str())
            {
                return Err("axes must have unique identities, names and tags".into());
            }
            axis.coordinates.validate()?;
            export_axis(axis)?;
        }
        let known_axes: HashSet<_> = self.axes.iter().map(|axis| axis.id).collect();
        let mut source_ids = HashSet::new();
        let mut filenames = HashSet::new();
        let mut source_locations = Vec::new();
        for source in &self.sources {
            if !source_ids.insert(source.id) || !filenames.insert(source.filename.as_str()) {
                return Err("full sources must have unique identities and filenames".into());
            }
            if source.default_layer.source != source.id {
                return Err("default layer must belong to its source".into());
            }
            validate_location(&source.location, &known_axes)?;
            let normalized = source.location.to_normalized(&self.axes)?;
            if source_locations.contains(&normalized) {
                return Err("full sources must have unique locations".into());
            }
            source_locations.push(normalized);
        }
        if !source_locations
            .iter()
            .any(|location| location.values().all(|value| value.abs() < 1e-9))
        {
            return Err("designspace needs a full source at the mapped default".into());
        }
        let mut sparse_layers = HashSet::new();
        for source in &self.sparse_sources {
            if !source_ids.contains(&source.layer.source)
                || !sparse_layers.insert(source.layer.clone())
            {
                return Err("sparse sources need a unique layer and live owner".into());
            }
            validate_location(&source.location, &known_axes)?;
        }
        let full_order: HashSet<_> = self
            .source_order
            .iter()
            .filter_map(|entry| match entry {
                SourceOrderEntry::Full(id) => Some(*id),
                SourceOrderEntry::Sparse(_) => None,
            })
            .collect();
        let sparse_order: HashSet<_> = self
            .source_order
            .iter()
            .filter_map(|entry| match entry {
                SourceOrderEntry::Full(_) => None,
                SourceOrderEntry::Sparse(layer) => Some(layer.clone()),
            })
            .collect();
        if full_order != source_ids
            || sparse_order != sparse_layers
            || self.source_order.len() != self.sources.len() + self.sparse_sources.len()
        {
            return Err("source order must contain every source exactly once".into());
        }
        let mut instance_ids = HashSet::new();
        for instance in &self.instances {
            if !instance_ids.insert(instance.id) {
                return Err("instances must have unique identities".into());
            }
            validate_location(&instance.location, &known_axes)?;
        }
        let mut rule_ids = HashSet::new();
        for rule in &self.rules {
            if !rule_ids.insert(rule.id) {
                return Err("rules must have unique identities".into());
            }
            for set in &rule.condition_sets {
                let mut conditioned_axes = HashSet::new();
                for condition in &set.conditions {
                    if !known_axes.contains(&condition.axis)
                        || !conditioned_axes.insert(condition.axis)
                    {
                        return Err("rule condition uses an unknown or duplicate axis".into());
                    }
                    if condition
                        .minimum
                        .into_iter()
                        .chain(condition.maximum)
                        .any(|value| !value.is_finite())
                        || condition
                            .minimum
                            .zip(condition.maximum)
                            .is_some_and(|(minimum, maximum)| minimum > maximum)
                    {
                        return Err("invalid rule condition bounds".into());
                    }
                }
            }
        }
        Ok(())
    }
}

fn import_location(
    dimensions: &[norad::designspace::Dimension],
    axis_ids: &HashMap<&str, AxisId>,
    context: &str,
) -> Result<CanonicalLocation, String> {
    let mut seen = HashSet::new();
    let mut coordinates = Vec::with_capacity(dimensions.len());
    for dimension in dimensions {
        let axis = axis_ids
            .get(dimension.name.as_str())
            .copied()
            .ok_or_else(|| format!("{context}: unknown axis {}", dimension.name))?;
        if !seen.insert(axis) {
            return Err(format!("{context}: duplicate axis {}", dimension.name));
        }
        if dimension.yvalue.is_some()
            || (dimension.xvalue.is_some() && dimension.uservalue.is_some())
        {
            return Err(format!(
                "{context}: anisotropic or ambiguous coordinate {}",
                dimension.name
            ));
        }
        let value = match (dimension.uservalue, dimension.xvalue) {
            (Some(value), None) if value.is_finite() => CoordinateValue::User(f64::from(value)),
            (None, Some(value)) if value.is_finite() => CoordinateValue::Design(f64::from(value)),
            _ => return Err(format!("{context}: invalid coordinate {}", dimension.name)),
        };
        coordinates.push((axis, value));
    }
    Ok(CanonicalLocation { coordinates })
}

fn validate_location(
    location: &CanonicalLocation,
    known_axes: &HashSet<AxisId>,
) -> Result<(), String> {
    let mut seen = HashSet::new();
    for (axis, value) in &location.coordinates {
        let coordinate = match value {
            CoordinateValue::User(value) | CoordinateValue::Design(value) => *value,
        };
        if !known_axes.contains(axis) || !seen.insert(*axis) || !coordinate.is_finite() {
            return Err("location uses an unknown, duplicate or non-finite axis value".into());
        }
    }
    Ok(())
}

fn import_rule(
    id: RuleId,
    rule: &norad::designspace::Rule,
    axis_ids: &HashMap<&str, AxisId>,
) -> Result<CanonicalRule, String> {
    let condition_sets = rule
        .condition_sets
        .iter()
        .map(|set| {
            let conditions =
                set.conditions
                    .iter()
                    .map(|condition| {
                        Ok(CanonicalCondition {
                            axis: axis_ids.get(condition.name.as_str()).copied().ok_or_else(
                                || format!("rule refers to unknown axis {}", condition.name),
                            )?,
                            minimum: condition.minimum.map(f64::from),
                            maximum: condition.maximum.map(f64::from),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
            Ok(CanonicalConditionSet { conditions })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(CanonicalRule {
        id,
        name: rule.name.clone(),
        condition_sets,
        substitutions: rule
            .substitutions
            .iter()
            .map(|substitution| CanonicalSubstitution {
                name: substitution.name.to_string(),
                replacement: substitution.with.to_string(),
            })
            .collect(),
    })
}

fn export_axis(axis: &CanonicalAxis) -> Result<norad::designspace::Axis, String> {
    Ok(norad::designspace::Axis {
        name: axis.coordinates.name.clone(),
        tag: axis.coordinates.tag.clone(),
        default: checked_f32("axis default", axis.coordinates.default)?,
        hidden: axis.hidden,
        minimum: axis
            .minimum_was_explicit
            .then(|| checked_f32("axis minimum", axis.coordinates.min))
            .transpose()?,
        maximum: axis
            .maximum_was_explicit
            .then(|| checked_f32("axis maximum", axis.coordinates.max))
            .transpose()?,
        values: None,
        map: (axis.map_was_present || !axis.coordinates.map.is_empty())
            .then(|| {
                axis.coordinates
                    .map
                    .iter()
                    .map(|(input, output)| {
                        Ok(norad::designspace::AxisMapping {
                            input: checked_f32("axis map input", *input)?,
                            output: checked_f32("axis map output", *output)?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .transpose()?,
        label_names: axis
            .labels
            .iter()
            .map(|label| norad::designspace::LocalizedString {
                language: label.language.clone(),
                string: label.value.clone(),
            })
            .collect(),
    })
}

fn export_location(
    location: &CanonicalLocation,
    axis_names: &HashMap<AxisId, &str>,
) -> Result<Vec<norad::designspace::Dimension>, String> {
    location
        .coordinates
        .iter()
        .map(|(axis, value)| {
            let name = axis_names
                .get(axis)
                .ok_or("location refers to missing axis")?
                .to_string();
            let (uservalue, xvalue) = match value {
                CoordinateValue::User(value) => {
                    (Some(checked_f32("user coordinate", *value)?), None)
                }
                CoordinateValue::Design(value) => {
                    (None, Some(checked_f32("design coordinate", *value)?))
                }
            };
            Ok(norad::designspace::Dimension {
                name,
                uservalue,
                xvalue,
                yvalue: None,
            })
        })
        .collect()
}

fn export_rule(
    rule: &CanonicalRule,
    axis_names: &HashMap<AxisId, &str>,
) -> Result<norad::designspace::Rule, String> {
    Ok(norad::designspace::Rule {
        name: rule.name.clone(),
        condition_sets: rule
            .condition_sets
            .iter()
            .map(|set| {
                Ok(norad::designspace::ConditionSet {
                    conditions: set
                        .conditions
                        .iter()
                        .map(|condition| {
                            Ok(norad::designspace::Condition {
                                name: axis_names
                                    .get(&condition.axis)
                                    .ok_or("rule condition refers to missing axis")?
                                    .to_string(),
                                minimum: condition
                                    .minimum
                                    .map(|value| checked_f32("rule minimum", value))
                                    .transpose()?,
                                maximum: condition
                                    .maximum
                                    .map(|value| checked_f32("rule maximum", value))
                                    .transpose()?,
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        substitutions: rule
            .substitutions
            .iter()
            .map(|substitution| {
                Ok(norad::designspace::Substitution {
                    name: norad::Name::new(&substitution.name)
                        .map_err(|_| "invalid substitution glyph name")?,
                    with: norad::Name::new(&substitution.replacement)
                        .map_err(|_| "invalid replacement glyph name")?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    })
}

fn checked_f32(context: &str, value: f64) -> Result<f32, String> {
    if !value.is_finite() {
        return Err(format!("{context}: non-finite value"));
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the exact f32 round trip is checked before accepting the value"
    )]
    let stored = value as f32;
    if f64::from(stored) != value {
        return Err(format!(
            "{context}: value cannot round-trip through Designspace"
        ));
    }
    Ok(stored)
}
