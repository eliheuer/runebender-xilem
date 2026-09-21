// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Typed ownership and lossless UFO encoding for HOI intermediate points.

use std::collections::BTreeMap;
use std::fmt;

/// UFO glyph-lib key for per-node HOI intermediate points.
pub const HOI_INTERMEDIATE_KEY: &str = "com.runebender.hoiIntermediate";

/// A malformed HOI intermediate-point payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HoiMetadataError {
    /// The value under the HOI key is not a dictionary.
    InvalidContainer,
    /// One dictionary key is not a `contour,point` index pair.
    InvalidIndex(String),
    /// One dictionary value is not a two-coordinate real array.
    InvalidPoint(String),
    /// One coordinate is not finite.
    NonFinitePoint(String),
}

impl fmt::Display for HoiMetadataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidContainer => formatter.write_str("invalid HOI intermediate container"),
            Self::InvalidIndex(key) => write!(formatter, "invalid HOI point index {key:?}"),
            Self::InvalidPoint(key) => write!(formatter, "invalid HOI point value for {key:?}"),
            Self::NonFinitePoint(key) => {
                write!(formatter, "non-finite HOI point value for {key:?}")
            }
        }
    }
}

impl std::error::Error for HoiMetadataError {}

/// Exact source payload plus typed per-node HOI intermediate points.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HoiIntermediates {
    source: plist::Dictionary,
    points: BTreeMap<(usize, usize), (f64, f64)>,
}

impl HoiIntermediates {
    /// Construct a canonical payload from typed points.
    pub fn from_points(
        points: impl IntoIterator<Item = ((usize, usize), (f64, f64))>,
    ) -> Result<Self, HoiMetadataError> {
        let mut source = plist::Dictionary::new();
        let mut stored = BTreeMap::new();
        for (index @ (contour, point), value @ (x, y)) in points {
            let key = format!("{contour},{point}");
            if !x.is_finite() || !y.is_finite() {
                return Err(HoiMetadataError::NonFinitePoint(key));
            }
            source.insert(
                key,
                plist::Value::Array(vec![plist::Value::Real(x), plist::Value::Real(y)]),
            );
            stored.insert(index, value);
        }
        Ok(Self {
            source,
            points: stored,
        })
    }

    /// Decode a valid source value while retaining its exact dictionary representation.
    pub fn from_plist(value: &plist::Value) -> Result<Self, HoiMetadataError> {
        let source = value
            .as_dictionary()
            .cloned()
            .ok_or(HoiMetadataError::InvalidContainer)?;
        let mut points = BTreeMap::new();
        for (key, value) in &source {
            let (contour, point) = key
                .split_once(',')
                .and_then(|(contour, point)| Some((contour.parse().ok()?, point.parse().ok()?)))
                .ok_or_else(|| HoiMetadataError::InvalidIndex(key.clone()))?;
            let value = value
                .as_array()
                .and_then(|value| Some((value.first()?.as_real()?, value.get(1)?.as_real()?)))
                .ok_or_else(|| HoiMetadataError::InvalidPoint(key.clone()))?;
            if !value.0.is_finite() || !value.1.is_finite() {
                return Err(HoiMetadataError::NonFinitePoint(key.clone()));
            }
            points.insert((contour, point), value);
        }
        Ok(Self { source, points })
    }

    /// Decode and remove a valid known key, leaving malformed input opaque and untouched.
    pub fn take_from_lib(lib: &mut plist::Dictionary) -> Option<Self> {
        let decoded = Self::from_plist(lib.get(HOI_INTERMEDIATE_KEY)?).ok()?;
        lib.remove(HOI_INTERMEDIATE_KEY);
        Some(decoded)
    }

    /// Iterate typed points in stable contour-and-point order.
    pub fn points(&self) -> impl Iterator<Item = ((usize, usize), (f64, f64))> + '_ {
        self.points.iter().map(|(index, point)| (*index, *point))
    }

    /// Whether the payload contains no intermediate points.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Write the exact retained payload into an otherwise opaque glyph-lib dictionary.
    pub fn write_to_lib(&self, lib: &mut plist::Dictionary) -> bool {
        if self.is_empty() {
            return lib.remove(HOI_INTERMEDIATE_KEY).is_some();
        }
        let value = plist::Value::Dictionary(self.source.clone());
        if lib.get(HOI_INTERMEDIATE_KEY) == Some(&value) {
            return false;
        }
        lib.insert(HOI_INTERMEDIATE_KEY.into(), value);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_payload_is_owned_exactly_and_invalid_payload_stays_opaque() {
        let value = plist::Value::Dictionary(plist::Dictionary::from_iter([(
            String::from("2,7"),
            plist::Value::Array(vec![plist::Value::Real(12.25), plist::Value::Real(-4.5)]),
        )]));
        let mut lib =
            plist::Dictionary::from_iter([(String::from(HOI_INTERMEDIATE_KEY), value.clone())]);
        let points = HoiIntermediates::take_from_lib(&mut lib).unwrap();
        assert_eq!(
            points.points().collect::<Vec<_>>(),
            vec![((2, 7), (12.25, -4.5))]
        );
        assert!(!lib.contains_key(HOI_INTERMEDIATE_KEY));
        assert!(points.write_to_lib(&mut lib));
        assert_eq!(lib.get(HOI_INTERMEDIATE_KEY), Some(&value));

        let invalid = plist::Value::String("preserve me".into());
        let mut lib =
            plist::Dictionary::from_iter([(String::from(HOI_INTERMEDIATE_KEY), invalid.clone())]);
        assert!(HoiIntermediates::take_from_lib(&mut lib).is_none());
        assert_eq!(lib.get(HOI_INTERMEDIATE_KEY), Some(&invalid));
    }
}
