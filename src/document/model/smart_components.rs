// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lossless UFO codecs for smart-component metadata.
//!
//! UFO stores smart-component values in an array aligned with component order.
//! The document model instead binds every entry to a stable component identifier, so reordering
//! components cannot move values to a different component.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::document::ComponentId;

/// UFO glyph-lib key for smart axes declared by a component source glyph.
pub const SMART_COMPONENT_AXES_KEY: &str = "com.schriftgestaltung.Glyphs.smartComponentAxes";

/// UFO glyph-lib key for values aligned with the using glyph's component order.
pub const SMART_COMPONENT_VALUES_KEY: &str =
    "com.schriftgestaltung.Glyphs.componentsSmartComponentValues";

/// UFO glyph-lib key selecting the smart-axis poles represented by a layer.
pub const SMART_COMPONENT_POLE_KEY: &str = "com.runebender.partSelection";

const AXIS_NAME_KEY: &str = "name";
const AXIS_BOTTOM_KEY: &str = "bottomValue";
const AXIS_TOP_KEY: &str = "topValue";

/// A structural or numeric error at the smart-component metadata boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SmartComponentMetadataError {
    /// The value under the named glyph-lib key has the wrong container type.
    InvalidContainer(&'static str),
    /// An array entry at the given index is not a dictionary.
    InvalidEntry {
        /// The glyph-lib key containing the array.
        key: &'static str,
        /// The invalid array index.
        index: usize,
    },
    /// An axis declaration has no string name.
    InvalidAxisName {
        /// The invalid axis declaration index.
        index: usize,
    },
    /// A recognized numeric value is not finite.
    NonFiniteNumber {
        /// The glyph-lib key containing the value.
        key: &'static str,
        /// The dictionary field containing the value.
        field: String,
    },
    /// The component order contains a repeated stable identifier.
    DuplicateComponentId,
    /// The source array has an entry that cannot be bound to a component.
    UnboundComponentEntry {
        /// The first array index beyond the component order.
        index: usize,
    },
    /// A stored entry has no component in the order used for export.
    MissingComponent,
}

impl fmt::Display for SmartComponentMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidContainer(key) => write!(f, "invalid container under {key}"),
            Self::InvalidEntry { key, index } => {
                write!(f, "invalid dictionary entry {index} under {key}")
            }
            Self::InvalidAxisName { index } => {
                write!(f, "invalid smart-component axis name at index {index}")
            }
            Self::NonFiniteNumber { key, field } => {
                write!(f, "non-finite number in {field} under {key}")
            }
            Self::DuplicateComponentId => f.write_str("duplicate component identifier"),
            Self::UnboundComponentEntry { index } => {
                write!(f, "smart-component value entry {index} has no component")
            }
            Self::MissingComponent => {
                f.write_str("stored smart-component values have no component to export")
            }
        }
    }
}

impl std::error::Error for SmartComponentMetadataError {}

/// One smart axis with exact source metadata and typed interpolation bounds.
#[derive(Clone, Debug, PartialEq)]
pub struct SmartComponentAxis {
    source: plist::Dictionary,
}

impl SmartComponentAxis {
    /// The axis name used by component values and pole selections.
    pub fn name(&self) -> &str {
        self.source
            .get(AXIS_NAME_KEY)
            .and_then(plist::Value::as_string)
            .expect("axis names are validated when imported")
    }

    /// The bottom interpolation value, defaulting to zero like the legacy renderer.
    pub fn bottom_value(&self) -> f64 {
        self.source
            .get(AXIS_BOTTOM_KEY)
            .and_then(number)
            .unwrap_or(0.0)
    }

    /// The top interpolation value, defaulting to one hundred like the legacy renderer.
    pub fn top_value(&self) -> f64 {
        self.source
            .get(AXIS_TOP_KEY)
            .and_then(number)
            .unwrap_or(100.0)
    }

    /// The exact source dictionary, including fields Runebender does not interpret.
    pub fn source(&self) -> &plist::Dictionary {
        &self.source
    }

    /// Set the bottom interpolation value while retaining an equal source representation.
    pub fn set_bottom_value(&mut self, value: f64) -> Result<bool, SmartComponentMetadataError> {
        set_finite_value(
            &mut self.source,
            SMART_COMPONENT_AXES_KEY,
            AXIS_BOTTOM_KEY,
            value,
        )
    }

    /// Set the top interpolation value while retaining an equal source representation.
    pub fn set_top_value(&mut self, value: f64) -> Result<bool, SmartComponentMetadataError> {
        set_finite_value(
            &mut self.source,
            SMART_COMPONENT_AXES_KEY,
            AXIS_TOP_KEY,
            value,
        )
    }
}

/// Ordered smart-axis declarations owned by a component source layer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SmartComponentAxes {
    axes: Vec<SmartComponentAxis>,
}

impl SmartComponentAxes {
    /// Decode a source value without changing its owning lib dictionary.
    pub fn from_plist(value: &plist::Value) -> Result<Self, SmartComponentMetadataError> {
        let source = value
            .as_array()
            .ok_or(SmartComponentMetadataError::InvalidContainer(
                SMART_COMPONENT_AXES_KEY,
            ))?;
        let mut axes = Vec::with_capacity(source.len());
        for (index, value) in source.iter().enumerate() {
            let dictionary = value.as_dictionary().cloned().ok_or(
                SmartComponentMetadataError::InvalidEntry {
                    key: SMART_COMPONENT_AXES_KEY,
                    index,
                },
            )?;
            if dictionary
                .get(AXIS_NAME_KEY)
                .and_then(plist::Value::as_string)
                .is_none()
            {
                return Err(SmartComponentMetadataError::InvalidAxisName { index });
            }
            validate_optional_number(&dictionary, SMART_COMPONENT_AXES_KEY, AXIS_BOTTOM_KEY)?;
            validate_optional_number(&dictionary, SMART_COMPONENT_AXES_KEY, AXIS_TOP_KEY)?;
            axes.push(SmartComponentAxis { source: dictionary });
        }
        Ok(Self { axes })
    }

    /// Decode and remove the known key, leaving invalid input untouched.
    pub fn take_from_lib(
        lib: &mut plist::Dictionary,
    ) -> Result<Option<Self>, SmartComponentMetadataError> {
        let Some(value) = lib.get(SMART_COMPONENT_AXES_KEY) else {
            return Ok(None);
        };
        let decoded = Self::from_plist(value)?;
        lib.remove(SMART_COMPONENT_AXES_KEY);
        Ok(Some(decoded))
    }

    /// The declarations in their source order.
    pub fn axes(&self) -> &[SmartComponentAxis] {
        &self.axes
    }

    /// Encode the exact retained dictionaries in declaration order.
    pub fn to_plist(&self) -> plist::Value {
        plist::Value::Array(
            self.axes
                .iter()
                .map(|axis| plist::Value::Dictionary(axis.source.clone()))
                .collect(),
        )
    }

    /// Write the owned value into an otherwise opaque glyph-lib dictionary.
    pub fn write_to_lib(&self, lib: &mut plist::Dictionary) -> bool {
        write_value(lib, SMART_COMPONENT_AXES_KEY, self.to_plist())
    }
}

/// Exact source values for one component, with numeric values available by axis name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComponentSmartValues {
    source: plist::Dictionary,
}

impl ComponentSmartValues {
    /// A finite numeric value for an axis, if the source supplies one.
    pub fn value(&self, axis: &str) -> Option<f64> {
        self.source.get(axis).and_then(number)
    }

    /// The exact source dictionary, including fields Runebender does not interpret.
    pub fn source(&self) -> &plist::Dictionary {
        &self.source
    }

    /// Set an axis value while retaining an equal integer or real representation.
    pub fn set_value(
        &mut self,
        axis: &str,
        value: f64,
    ) -> Result<bool, SmartComponentMetadataError> {
        set_finite_value(&mut self.source, SMART_COMPONENT_VALUES_KEY, axis, value)
    }

    /// Remove a value for one axis.
    pub fn remove_value(&mut self, axis: &str) -> bool {
        self.source.remove(axis).is_some()
    }
}

/// Per-component smart values bound to stable component identifiers.
///
/// The default identifier is the document's [`ComponentId`].
/// A generic identifier keeps the boundary codec independently testable.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SmartComponentValues<Id = ComponentId> {
    entries: BTreeMap<Id, ComponentSmartValues>,
}

impl<Id: Copy + Ord> SmartComponentValues<Id> {
    /// Decode an array using the component order present at the import boundary.
    pub fn from_plist(
        value: &plist::Value,
        component_order: &[Id],
    ) -> Result<Self, SmartComponentMetadataError> {
        validate_component_order(component_order)?;
        let source = value
            .as_array()
            .ok_or(SmartComponentMetadataError::InvalidContainer(
                SMART_COMPONENT_VALUES_KEY,
            ))?;
        if source.len() > component_order.len() {
            return Err(SmartComponentMetadataError::UnboundComponentEntry {
                index: component_order.len(),
            });
        }
        let mut entries = BTreeMap::new();
        for (index, value) in source.iter().enumerate() {
            let dictionary = value.as_dictionary().cloned().ok_or(
                SmartComponentMetadataError::InvalidEntry {
                    key: SMART_COMPONENT_VALUES_KEY,
                    index,
                },
            )?;
            validate_dictionary_numbers(&dictionary, SMART_COMPONENT_VALUES_KEY)?;
            entries.insert(
                component_order[index],
                ComponentSmartValues { source: dictionary },
            );
        }
        Ok(Self { entries })
    }

    /// Decode and remove the known key, leaving invalid input untouched.
    pub fn take_from_lib(
        lib: &mut plist::Dictionary,
        component_order: &[Id],
    ) -> Result<Option<Self>, SmartComponentMetadataError> {
        let Some(value) = lib.get(SMART_COMPONENT_VALUES_KEY) else {
            return Ok(None);
        };
        let decoded = Self::from_plist(value, component_order)?;
        lib.remove(SMART_COMPONENT_VALUES_KEY);
        Ok(Some(decoded))
    }

    /// The exact entry bound to a stable component identifier.
    pub fn get(&self, component: Id) -> Option<&ComponentSmartValues> {
        self.entries.get(&component)
    }

    /// A mutable entry bound to a stable component identifier.
    pub fn get_mut(&mut self, component: Id) -> Option<&mut ComponentSmartValues> {
        self.entries.get_mut(&component)
    }

    /// A finite numeric value for one component and axis.
    pub fn value(&self, component: Id, axis: &str) -> Option<f64> {
        self.get(component).and_then(|entry| entry.value(axis))
    }

    /// Add an empty entry for a component that does not already have one.
    pub fn insert_component(&mut self, component: Id) -> bool {
        if self.entries.contains_key(&component) {
            return false;
        }
        self.entries
            .insert(component, ComponentSmartValues::default());
        true
    }

    /// Remove the values bound to a deleted component.
    pub fn remove_component(&mut self, component: Id) -> bool {
        self.entries.remove(&component).is_some()
    }

    /// Encode entries in the current component order.
    ///
    /// Empty dictionaries fill gaps before the last component with a retained entry.
    /// This preserves the UFO array's positional meaning after component reordering.
    pub fn to_plist(
        &self,
        component_order: &[Id],
    ) -> Result<plist::Value, SmartComponentMetadataError> {
        validate_component_order(component_order)?;
        if self
            .entries
            .keys()
            .any(|component| !component_order.contains(component))
        {
            return Err(SmartComponentMetadataError::MissingComponent);
        }
        let Some(last) = component_order
            .iter()
            .rposition(|component| self.entries.contains_key(component))
        else {
            return Ok(plist::Value::Array(Vec::new()));
        };
        Ok(plist::Value::Array(
            component_order[..=last]
                .iter()
                .map(|component| {
                    let source = self
                        .entries
                        .get(component)
                        .map(|entry| entry.source.clone())
                        .unwrap_or_default();
                    plist::Value::Dictionary(source)
                })
                .collect(),
        ))
    }

    /// Write the owned value using the current component order.
    pub fn write_to_lib(
        &self,
        lib: &mut plist::Dictionary,
        component_order: &[Id],
    ) -> Result<bool, SmartComponentMetadataError> {
        Ok(write_value(
            lib,
            SMART_COMPONENT_VALUES_KEY,
            self.to_plist(component_order)?,
        ))
    }
}

/// Exact pole-selection metadata for one smart-component source layer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SmartComponentPole {
    source: plist::Dictionary,
}

impl SmartComponentPole {
    /// Decode a source value without changing its owning lib dictionary.
    pub fn from_plist(value: &plist::Value) -> Result<Self, SmartComponentMetadataError> {
        let source =
            value
                .as_dictionary()
                .cloned()
                .ok_or(SmartComponentMetadataError::InvalidContainer(
                    SMART_COMPONENT_POLE_KEY,
                ))?;
        Ok(Self { source })
    }

    /// Decode and remove the known key, leaving invalid input untouched.
    pub fn take_from_lib(
        lib: &mut plist::Dictionary,
    ) -> Result<Option<Self>, SmartComponentMetadataError> {
        let Some(value) = lib.get(SMART_COMPONENT_POLE_KEY) else {
            return Ok(None);
        };
        let decoded = Self::from_plist(value)?;
        lib.remove(SMART_COMPONENT_POLE_KEY);
        Ok(Some(decoded))
    }

    /// The exact signed-integer selection for an axis, if present.
    pub fn selection(&self, axis: &str) -> Option<i64> {
        self.source
            .get(axis)
            .and_then(plist::Value::as_signed_integer)
    }

    /// Whether this layer is a top pole for the named axis.
    pub fn is_top(&self, axis: &str) -> bool {
        self.selection(axis) == Some(2)
    }

    /// The exact source dictionary, including fields Runebender does not interpret.
    pub fn source(&self) -> &plist::Dictionary {
        &self.source
    }

    /// Write the owned value into an otherwise opaque glyph-lib dictionary.
    pub fn write_to_lib(&self, lib: &mut plist::Dictionary) -> bool {
        write_value(lib, SMART_COMPONENT_POLE_KEY, self.to_plist())
    }

    /// Encode the exact retained dictionary.
    pub fn to_plist(&self) -> plist::Value {
        plist::Value::Dictionary(self.source.clone())
    }
}

fn number(value: &plist::Value) -> Option<f64> {
    value
        .as_real()
        .or_else(|| value.as_signed_integer().map(|value| value as f64))
}

fn validate_optional_number(
    dictionary: &plist::Dictionary,
    key: &'static str,
    field: &str,
) -> Result<(), SmartComponentMetadataError> {
    if let Some(value) = dictionary.get(field).and_then(number)
        && !value.is_finite()
    {
        return Err(SmartComponentMetadataError::NonFiniteNumber {
            key,
            field: field.to_owned(),
        });
    }
    Ok(())
}

fn validate_dictionary_numbers(
    dictionary: &plist::Dictionary,
    key: &'static str,
) -> Result<(), SmartComponentMetadataError> {
    for (field, value) in dictionary {
        if let Some(value) = number(value)
            && !value.is_finite()
        {
            return Err(SmartComponentMetadataError::NonFiniteNumber {
                key,
                field: field.clone(),
            });
        }
    }
    Ok(())
}

fn validate_component_order<Id: Copy + Ord>(
    component_order: &[Id],
) -> Result<(), SmartComponentMetadataError> {
    let mut seen = BTreeSet::new();
    if component_order
        .iter()
        .copied()
        .any(|component| !seen.insert(component))
    {
        return Err(SmartComponentMetadataError::DuplicateComponentId);
    }
    Ok(())
}

fn set_finite_value(
    dictionary: &mut plist::Dictionary,
    key: &'static str,
    field: &str,
    value: f64,
) -> Result<bool, SmartComponentMetadataError> {
    if !value.is_finite() {
        return Err(SmartComponentMetadataError::NonFiniteNumber {
            key,
            field: field.to_owned(),
        });
    }
    if dictionary.get(field).and_then(number) == Some(value) {
        return Ok(false);
    }
    dictionary.insert(field.to_owned(), plist::Value::Real(value));
    Ok(true)
}

fn write_value(lib: &mut plist::Dictionary, key: &'static str, value: plist::Value) -> bool {
    if lib.get(key) == Some(&value) {
        return false;
    }
    lib.insert(key.into(), value);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dictionary(entries: impl IntoIterator<Item = (&'static str, plist::Value)>) -> plist::Value {
        plist::Value::Dictionary(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }

    #[test]
    fn axes_preserve_exact_source_and_legacy_defaults() {
        let source = plist::Value::Array(vec![dictionary([
            (AXIS_NAME_KEY, plist::Value::String("Width".into())),
            (AXIS_BOTTOM_KEY, plist::Value::Integer(0_i64.into())),
            (AXIS_TOP_KEY, plist::Value::String("legacy-default".into())),
            ("future", plist::Value::Boolean(true)),
        ])]);
        let axes = SmartComponentAxes::from_plist(&source).unwrap();
        assert_eq!(axes.axes()[0].name(), "Width");
        assert_eq!(axes.axes()[0].bottom_value(), 0.0);
        assert_eq!(axes.axes()[0].top_value(), 100.0);
        assert_eq!(axes.to_plist(), source);
    }

    #[test]
    fn component_values_follow_stable_ids_after_reordering() {
        let first = dictionary([
            ("Width", plist::Value::Integer(25_i64.into())),
            ("future", plist::Value::String("exact".into())),
        ]);
        let second = dictionary([("Width", plist::Value::Real(75.0))]);
        let source = plist::Value::Array(vec![first.clone(), second.clone()]);
        let values = SmartComponentValues::from_plist(&source, &[11_u8, 22]).unwrap();

        assert_eq!(values.value(11, "Width"), Some(25.0));
        assert_eq!(values.value(22, "Width"), Some(75.0));
        assert_eq!(
            values.to_plist(&[22, 11]).unwrap(),
            plist::Value::Array(vec![second, first])
        );
    }

    #[test]
    fn gaps_are_encoded_without_moving_a_later_entry() {
        let source = plist::Value::Array(vec![dictionary([("Width", plist::Value::Real(75.0))])]);
        let mut values = SmartComponentValues::from_plist(&source, &[22_u8]).unwrap();
        assert!(values.insert_component(11));
        assert_eq!(
            values.to_plist(&[11, 22]).unwrap(),
            plist::Value::Array(vec![
                plist::Value::Dictionary(plist::Dictionary::new()),
                source.as_array().unwrap()[0].clone(),
            ])
        );
    }

    #[test]
    fn invalid_component_input_is_not_removed() {
        let source = plist::Value::Array(vec![
            dictionary([("Width", plist::Value::Real(25.0))]),
            dictionary([("Width", plist::Value::Real(75.0))]),
        ]);
        let mut lib = plist::Dictionary::new();
        lib.insert(SMART_COMPONENT_VALUES_KEY.into(), source.clone());

        assert_eq!(
            SmartComponentValues::take_from_lib(&mut lib, &[11_u8]),
            Err(SmartComponentMetadataError::UnboundComponentEntry { index: 1 })
        );
        assert_eq!(lib.get(SMART_COMPONENT_VALUES_KEY), Some(&source));
    }

    #[test]
    fn non_finite_numbers_are_rejected_without_removal() {
        let source = plist::Value::Array(vec![dictionary([
            (AXIS_NAME_KEY, plist::Value::String("Width".into())),
            (AXIS_TOP_KEY, plist::Value::Real(f64::INFINITY)),
        ])]);
        let mut lib = plist::Dictionary::new();
        lib.insert(SMART_COMPONENT_AXES_KEY.into(), source.clone());

        assert!(matches!(
            SmartComponentAxes::take_from_lib(&mut lib),
            Err(SmartComponentMetadataError::NonFiniteNumber { .. })
        ));
        assert_eq!(lib.get(SMART_COMPONENT_AXES_KEY), Some(&source));
    }

    #[test]
    fn pole_selection_preserves_unknown_values() {
        let source = dictionary([
            ("Width", plist::Value::Integer(2_u64.into())),
            ("Height", plist::Value::Integer(1_u64.into())),
            ("future", plist::Value::String("exact".into())),
        ]);
        let pole = SmartComponentPole::from_plist(&source).unwrap();
        assert!(pole.is_top("Width"));
        assert!(!pole.is_top("Height"));
        assert_eq!(pole.to_plist(), source);
    }

    #[test]
    fn semantic_no_op_retains_integer_representation() {
        let source = plist::Value::Array(vec![dictionary([
            (AXIS_NAME_KEY, plist::Value::String("Width".into())),
            (AXIS_BOTTOM_KEY, plist::Value::Integer(0_i64.into())),
            (AXIS_TOP_KEY, plist::Value::Integer(100_i64.into())),
        ])]);
        let mut axes = SmartComponentAxes::from_plist(&source).unwrap();

        assert!(!axes.axes[0].set_top_value(100.0).unwrap());
        assert_eq!(axes.to_plist(), source);
    }
}
