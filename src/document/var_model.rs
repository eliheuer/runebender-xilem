// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Designspace interpolation: a port of fontTools' `VariationModel`
//! (the same algorithm Fontra's `var-model.js` implements), plus the
//! glyph-level glue that turns a set of master `.glif`s into an
//! interpolated one at an arbitrary location.
//!
//! Locations here are always **normalized** to −1..1 per axis; the
//! caller normalizes against the designspace axis min/default/max
//! before handing them over, exactly as fontTools does.
//!
//! The model is: for a set of master locations, derive one support
//! ("tent") per master, express each master's outline as a delta from
//! the ones before it, then evaluate at a target location by summing
//! the deltas scaled by their supports.

use std::collections::{HashMap, HashSet};

/// A per-axis tent: `(lower, peak, upper)` in normalized coordinates.
pub type Support = HashMap<String, (f64, f64, f64)>;
/// A normalized designspace location, axis tag → value.
pub type Location = HashMap<String, f64>;

/// How much a support contributes at `location`.
///
/// This is fontTools' `supportScalar` without the `extrapolate` and
/// `ot` special cases: the editor never evaluates outside the
/// designspace box.
pub fn support_scalar(location: &Location, support: &Support) -> f64 {
    let mut scalar = 1.0;
    for (axis, &(lower, peak, upper)) in support {
        if peak == 0.0 {
            continue;
        }
        if lower > peak || peak > upper {
            continue;
        }
        if lower < 0.0 && upper > 0.0 {
            continue;
        }
        let v = location.get(axis).copied().unwrap_or(0.0);
        if v == peak {
            continue;
        }
        if v <= lower || upper <= v {
            return 0.0;
        }
        if v < peak {
            scalar *= (v - lower) / (peak - lower);
        } else {
            scalar *= (upper - v) / (upper - peak);
        }
    }
    scalar
}

/// The interpolation model for one set of master locations.
pub struct VariationModel {
    /// Master order after sorting, as indices into the input list.
    pub sort_order: Vec<usize>,
    /// One support per master, in sorted order.
    pub supports: Vec<Support>,
    /// `delta_weights[i][j]` = how much delta `j` already contributes
    /// at master `i`, so master `i`'s own delta can subtract it out.
    pub delta_weights: Vec<Vec<(usize, f64)>>,
}

impl VariationModel {
    /// Build a model for the given master locations. Locations must be normalized to -1..1 per axis.
    /// The first location in sorted order becomes the base master.
    pub fn new(locations: &[Location]) -> Self {
        let sort_order = sort_locations(locations);
        let sorted: Vec<Location> = sort_order.iter().map(|&i| locations[i].clone()).collect();
        let supports = compute_supports(&sorted);
        let delta_weights = compute_delta_weights(&sorted, &supports);
        Self {
            sort_order,
            supports,
            delta_weights,
        }
    }

    /// Scalars to apply to each master's *delta* at `location`, in
    /// sorted order.
    pub fn support_scalars(&self, location: &Location) -> Vec<f64> {
        self.supports
            .iter()
            .map(|support| support_scalar(location, support))
            .collect()
    }

    /// Turn per-master values into deltas. `values` is in the
    /// caller's original order; the result is in sorted order.
    pub fn deltas(&self, values: &[Vec<f64>]) -> Vec<Vec<f64>> {
        let mut deltas: Vec<Vec<f64>> = Vec::with_capacity(values.len());
        for (i, &source) in self.sort_order.iter().enumerate() {
            let mut delta = values[source].clone();
            for &(j, weight) in &self.delta_weights[i] {
                for (d, prev) in delta.iter_mut().zip(&deltas[j]) {
                    *d -= prev * weight;
                }
            }
            deltas.push(delta);
        }
        deltas
    }

    /// Evaluate interpolated values at `location`.
    pub fn interpolate(&self, values: &[Vec<f64>], location: &Location) -> Vec<f64> {
        let deltas = self.deltas(values);
        let scalars = self.support_scalars(location);
        let width = values.first().map_or(0, Vec::len);
        let mut out = vec![0.0; width];
        for (delta, scalar) in deltas.iter().zip(scalars) {
            if scalar == 0.0 {
                continue;
            }
            for (o, d) in out.iter_mut().zip(delta) {
                *o += d * scalar;
            }
        }
        out
    }
}

/// Sort masters by how special they are: on-axis masters first,
/// then by number of axes involved, then by axis names and values.
///
/// The order decides which master narrows which support. This is
/// the order fontTools uses.
fn sort_locations(locations: &[Location]) -> Vec<usize> {
    let mut axis_points: HashMap<String, HashSet<u64>> = HashMap::new();
    for location in locations {
        let on_axis: Vec<&String> = location
            .iter()
            .filter(|&(_, &v)| v != 0.0)
            .map(|(axis, _)| axis)
            .collect();
        if on_axis.len() != 1 {
            continue;
        }
        let axis = on_axis[0];
        let value = location[axis];
        axis_points
            .entry(axis.clone())
            .or_default()
            .insert(value.to_bits());
    }

    let mut order: Vec<usize> = (0..locations.len()).collect();
    order.sort_by(|&a, &b| key(&locations[a], &axis_points).cmp(&key(&locations[b], &axis_points)));
    order
}

/// The master sort key, flattened into a tuple of orderable parts.
/// This is fontTools' `getMasterLocationsSortKeyFunc`.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct SortKey {
    rank: usize,
    on_point_count: usize,
    axis_names: Vec<String>,
    signs: Vec<i8>,
    magnitudes: Vec<u64>,
}

fn key(location: &Location, axis_points: &HashMap<String, HashSet<u64>>) -> SortKey {
    let mut used: Vec<(&String, f64)> = location
        .iter()
        .filter(|&(_, &v)| v != 0.0)
        .map(|(axis, &v)| (axis, v))
        .collect();
    used.sort_by(|a, b| a.0.cmp(b.0));

    let on_point_count = used
        .iter()
        .filter(|(axis, value)| {
            axis_points
                .get(*axis)
                .is_some_and(|points| points.contains(&value.to_bits()))
        })
        .count();

    SortKey {
        rank: used.len(),
        // More on-axis hits sort earlier, so negate by subtracting.
        on_point_count: used.len() - on_point_count,
        axis_names: used.iter().map(|(axis, _)| (*axis).clone()).collect(),
        signs: used
            .iter()
            .map(|(_, v)| if *v < 0.0 { -1i8 } else { 1i8 })
            .collect(),
        magnitudes: used.iter().map(|(_, v)| v.abs().to_bits()).collect(),
    }
}

/// Compute each master's support: its tent starts as its own peak
/// spanning the whole axis, then gets narrowed by every earlier
/// master whose region it overlaps. This is fontTools' support
/// computation.
fn compute_supports(locations: &[Location]) -> Vec<Support> {
    let mut supports: Vec<Support> = Vec::with_capacity(locations.len());

    for (i, location) in locations.iter().enumerate() {
        let mut support: Support = HashMap::new();
        for (axis, &value) in location {
            if value == 0.0 {
                continue;
            }
            let tent = if value > 0.0 {
                (0.0, value, 1.0)
            } else {
                (-1.0, value, 0.0)
            };
            support.insert(axis.clone(), tent);
        }

        for other in locations.iter().take(i) {
            // Only masters whose axes are a subset and which sit
            // inside this support can narrow it.
            if other
                .iter()
                .any(|(axis, &v)| v != 0.0 && !support.contains_key(axis))
            {
                continue;
            }
            if support.keys().any(|axis| {
                let v = other.get(axis).copied().unwrap_or(0.0);
                v == 0.0
            }) {
                continue;
            }
            let relevant = support.iter().all(|(axis, &(lower, peak, upper))| {
                let v = other.get(axis).copied().unwrap_or(0.0);
                v > lower && v < upper && v != peak
            });
            if !relevant {
                continue;
            }

            // Narrow on the axis where the other master is closest to
            // our peak — fontTools picks the smallest resulting tent.
            let mut best: Option<(String, (f64, f64, f64), f64)> = None;
            for (axis, &(lower, peak, upper)) in &support {
                let v = other[axis];
                let (new_lower, new_upper) = if v > peak { (lower, v) } else { (v, upper) };
                let width = new_upper - new_lower;
                if best.as_ref().is_none_or(|(_, _, w)| width < *w) {
                    best = Some((axis.clone(), (new_lower, peak, new_upper), width));
                }
            }
            if let Some((axis, tent, _)) = best {
                support.insert(axis, tent);
            }
        }

        supports.push(support);
    }

    supports
}

/// `delta_weights[i]` lists earlier deltas that already contribute at
/// master `i`, with how much.
fn compute_delta_weights(locations: &[Location], supports: &[Support]) -> Vec<Vec<(usize, f64)>> {
    let mut weights = Vec::with_capacity(locations.len());
    for (i, location) in locations.iter().enumerate() {
        let mut row = Vec::new();
        for (j, support) in supports.iter().enumerate().take(i) {
            let scalar = support_scalar(location, support);
            if scalar != 0.0 {
                row.push((j, scalar));
            }
        }
        weights.push(row);
    }
    weights
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
        let model = VariationModel::new(&[loc(&[("wght", 0.0)]), loc(&[("wght", 1.0)])]);
        let values = vec![vec![100.0, 0.0], vec![300.0, 10.0]];
        let mid = model.interpolate(&values, &loc(&[("wght", 0.5)]));
        assert!((mid[0] - 200.0).abs() < 1e-9);
        assert!((mid[1] - 5.0).abs() < 1e-9);
    }

    #[test]
    fn masters_reproduce_themselves() {
        let model = VariationModel::new(&[
            loc(&[("wght", 0.0)]),
            loc(&[("wght", 1.0)]),
            loc(&[("wght", 0.5)]),
        ]);
        let values = vec![vec![100.0], vec![300.0], vec![150.0]];
        for (location, expected) in [(0.0, 100.0), (0.5, 150.0), (1.0, 300.0)] {
            let got = model.interpolate(&values, &loc(&[("wght", location)]));
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
        ]);
        let values = vec![vec![0.0], vec![100.0], vec![80.0]];
        let quarter = model.interpolate(&values, &loc(&[("wght", 0.25)]));
        assert!(quarter[0] > 25.0, "expected bend, got {quarter:?}");
    }

    #[test]
    fn two_axes_corner_master() {
        let model = VariationModel::new(&[
            loc(&[("wght", 0.0), ("wdth", 0.0)]),
            loc(&[("wght", 1.0), ("wdth", 0.0)]),
            loc(&[("wght", 0.0), ("wdth", 1.0)]),
            loc(&[("wght", 1.0), ("wdth", 1.0)]),
        ]);
        let values = vec![vec![0.0], vec![10.0], vec![100.0], vec![110.0]];
        let mid = model.interpolate(&values, &loc(&[("wght", 0.5), ("wdth", 0.5)]));
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
