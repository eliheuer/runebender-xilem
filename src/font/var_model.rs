// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Checked variation math behind Runebender-owned locations.
//!
//! The backend is fontdrasil, the same model used by pinned Babelfont.
//! Keep editable values as f64 and disable compiler rounding. The backend never
//! receives whole UFO glyphs, so it cannot narrow advances or discard metadata.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use fontdrasil::coords::{NormalizedCoord, NormalizedLocation};
use fontdrasil::types::Tag;
use fontdrasil::variations::RoundingBehaviour;

/// A normalized designspace location, axis name to value, with omitted axes at zero.
pub type Location = HashMap<String, f64>;

/// Runebender's checked adapter to Babelfont's variation-math dependency.
#[derive(Debug)]
pub struct VariationModel {
    backend: fontdrasil::variations::VariationModel,
    axes: BTreeMap<String, Tag>,
    locations: Vec<NormalizedLocation>,
}

impl VariationModel {
    /// Build a model with unique, finite source locations and a default source.
    /// Source coordinates must be normalized to -1..1.
    pub fn new(locations: &[Location]) -> Result<Self, String> {
        if locations.is_empty() {
            return Err("variation model needs at least one source".into());
        }
        let names: BTreeSet<_> = locations
            .iter()
            .flat_map(|loc| loc.keys().cloned())
            .collect();
        let mut axes = BTreeMap::new();
        for (index, name) in names.into_iter().enumerate() {
            // The backend needs tags, while the public API uses full axis names.
            // Private synthetic tags preserve deterministic name order without
            // truncating or colliding user names; they are never serialized.
            let index = u32::try_from(index).map_err(|_| "too many variation axes")?;
            axes.insert(name, Tag::new(&index.to_be_bytes()));
        }
        let locations: Vec<_> = locations
            .iter()
            .map(|loc| convert_location(&axes, loc))
            .collect::<Result<_, _>>()?;
        let unique: HashSet<_> = locations.iter().cloned().collect();
        if unique.len() != locations.len() {
            return Err("duplicate glyph-source location".into());
        }
        if !locations
            .iter()
            .any(|loc| loc.iter().all(|(_, value)| value.to_f64() == 0.0))
        {
            return Err("variation model needs a source at the default location".into());
        }
        let backend =
            fontdrasil::variations::VariationModel::new(unique, axes.values().copied().collect());
        Ok(Self {
            backend,
            axes,
            locations,
        })
    }

    /// Interpolate equally sized vectors in source order without rounding editable values.
    pub fn interpolate(
        &self,
        values: &[Vec<f64>],
        location: &Location,
    ) -> Result<Vec<f64>, String> {
        if values.len() != self.locations.len()
            || values.iter().flatten().any(|value| !value.is_finite())
        {
            return Err("interpolation requires finite values for every source".into());
        }
        let location = convert_location(&self.axes, location)?;
        let points = self
            .locations
            .iter()
            .cloned()
            .zip(values.iter().cloned())
            .collect();
        let deltas = self
            .backend
            .deltas_with_rounding(&points, RoundingBehaviour::None)
            .map_err(|error| error.to_string())?;
        let result = self.backend.interpolate_from_deltas(&location, &deltas);
        if result.iter().any(|value| !value.is_finite()) {
            return Err("non-finite interpolation result".into());
        }
        Ok(result)
    }
}

fn convert_location(
    axes: &BTreeMap<String, Tag>,
    location: &Location,
) -> Result<NormalizedLocation, String> {
    if location.keys().any(|name| !axes.contains_key(name))
        || location
            .values()
            .any(|value| !value.is_finite() || !(-1.0..=1.0).contains(value))
    {
        return Err(
            "interpolation requires known axes and finite normalized coordinates in -1..1".into(),
        );
    }
    Ok(axes
        .iter()
        .map(|(name, tag)| {
            (
                *tag,
                NormalizedCoord::new(location.get(name).copied().unwrap_or(0.0)),
            )
        })
        .collect())
}

/// Normalize a design-space value against `(min, default, max)`, the
/// same mapping fontTools' `normalizeValue` applies.
pub fn normalize_value(value: f64, min: f64, default: f64, max: f64) -> f64 {
    let value = value.clamp(min.min(max), max.max(min));
    if value == default {
        0.0
    } else if value < default {
        if default - min == 0.0 {
            0.0
        } else {
            -(default - value) / (default - min)
        }
    } else if max - default == 0.0 {
        0.0
    } else {
        (value - default) / (max - default)
    }
}

/// The inverse of [`normalize_value`]: a normalized -1..1 coordinate
/// back to design space. Sliders live in design space (`wght 400`),
/// the model in normalized space, so a UI needs both directions.
pub fn denormalize_value(value: f64, min: f64, default: f64, max: f64) -> f64 {
    let value = value.clamp(-1.0, 1.0);
    if value == 0.0 {
        default
    } else if value < 0.0 {
        default + value * (default - min)
    } else {
        default + value * (max - default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_round_trips_through_denormalize() {
        let (min, default, max) = (100.0, 400.0, 900.0);
        for value in [100.0, 250.0, 400.0, 650.0, 900.0] {
            let n = normalize_value(value, min, default, max);
            let back = denormalize_value(n, min, default, max);
            assert!((back - value).abs() < 1e-9, "{value} -> {n} -> {back}");
        }
    }

    fn loc(pairs: &[(&str, f64)]) -> Location {
        pairs
            .iter()
            .map(|(axis, value)| ((*axis).to_string(), *value))
            .collect()
    }

    #[test]
    fn two_master_axis_interpolates_linearly() {
        let model = VariationModel::new(&[loc(&[("wght", 0.0)]), loc(&[("wght", 1.0)])]).unwrap();
        let values = vec![vec![100.0, 0.0], vec![300.0, 10.0]];
        let mid = model.interpolate(&values, &loc(&[("wght", 0.5)])).unwrap();
        assert!((mid[0] - 200.0).abs() < 1e-9);
        assert!((mid[1] - 5.0).abs() < 1e-9);
    }

    #[test]
    fn masters_reproduce_themselves() {
        let model = VariationModel::new(&[
            loc(&[("wght", 0.0)]),
            loc(&[("wght", 1.0)]),
            loc(&[("wght", 0.5)]),
        ])
        .unwrap();
        let values = vec![vec![100.0], vec![300.0], vec![150.0]];
        for (location, expected) in [(0.0, 100.0), (0.5, 150.0), (1.0, 300.0)] {
            let got = model
                .interpolate(&values, &loc(&[("wght", location)]))
                .unwrap();
            assert!(
                (got[0] - expected).abs() < 1e-9,
                "at {location}: got {got:?}, want {expected}"
            );
        }
    }

    #[test]
    fn intermediate_master_bends_the_curve() {
        // With a master at 0.5 pulled off the linear path, the value
        // between masters follows the bend rather than the straight
        // line between the extremes.
        let model = VariationModel::new(&[
            loc(&[("wght", 0.0)]),
            loc(&[("wght", 1.0)]),
            loc(&[("wght", 0.5)]),
        ])
        .unwrap();
        let values = vec![vec![0.0], vec![100.0], vec![80.0]];
        let quarter = model.interpolate(&values, &loc(&[("wght", 0.25)])).unwrap();
        assert!(quarter[0] > 25.0, "expected bend, got {quarter:?}");
    }

    #[test]
    fn two_axes_corner_master() {
        let model = VariationModel::new(&[
            loc(&[("wght", 0.0), ("wdth", 0.0)]),
            loc(&[("wght", 1.0), ("wdth", 0.0)]),
            loc(&[("wght", 0.0), ("wdth", 1.0)]),
            loc(&[("wght", 1.0), ("wdth", 1.0)]),
        ])
        .unwrap();
        let values = vec![vec![0.0], vec![10.0], vec![100.0], vec![110.0]];
        let mid = model
            .interpolate(&values, &loc(&[("wght", 0.5), ("wdth", 0.5)]))
            .unwrap();
        assert!((mid[0] - 55.0).abs() < 1e-9, "got {mid:?}");
    }

    #[test]
    fn normalize_matches_fonttools() {
        assert!((normalize_value(400.0, 100.0, 400.0, 900.0) - 0.0).abs() < 1e-9);
        assert!((normalize_value(900.0, 100.0, 400.0, 900.0) - 1.0).abs() < 1e-9);
        assert!((normalize_value(100.0, 100.0, 400.0, 900.0) + 1.0).abs() < 1e-9);
        assert!((normalize_value(650.0, 100.0, 400.0, 900.0) - 0.5).abs() < 1e-9);
    }
}
