// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Runebender axis values; Babelfont owns coordinate conversion behind this boundary.

use fontdrasil::coords::{DesignCoord, UserCoord};

/// A continuous axis in user coordinates, with a map into design coordinates.
#[derive(Clone, Debug)]
pub struct Axis {
    /// Human-readable axis name.
    pub name: String,
    /// Four-byte OpenType tag.
    pub tag: String,
    /// Minimum user coordinate.
    pub min: f64,
    /// Default user coordinate.
    pub default: f64,
    /// Maximum user coordinate.
    pub max: f64,
    /// Ordered `(user, design)` mapping points; empty means identity.
    pub map: Vec<(f64, f64)>,
}

impl Axis {
    pub(super) fn backend(&self) -> Result<babelfont::Axis, String> {
        let tag: &[u8; 4] = self
            .tag
            .as_bytes()
            .try_into()
            .map_err(|_| "axis tag must contain four bytes")?;
        if !tag.iter().all(u8::is_ascii_graphic) {
            return Err("axis tag must contain printable ASCII bytes".into());
        }
        if ![self.min, self.default, self.max]
            .iter()
            .all(|v| v.is_finite())
            || self.min > self.default
            || self.default > self.max
            || self.min >= self.max
        {
            return Err(format!("{}: invalid continuous axis bounds", self.name));
        }
        if self
            .map
            .iter()
            .any(|(u, d)| !u.is_finite() || !d.is_finite())
            || self
                .map
                .windows(2)
                .any(|w| w[0].0 >= w[1].0 || w[0].1 >= w[1].1)
            || self.map.len() == 1
        {
            return Err(format!(
                "{}: axis map must be finite and strictly increasing",
                self.name
            ));
        }
        let mut axis = babelfont::Axis::new(self.name.clone(), babelfont::Tag::new(tag));
        axis.min = Some(UserCoord::new(self.min));
        axis.default = Some(UserCoord::new(self.default));
        axis.max = Some(UserCoord::new(self.max));
        if !self.map.is_empty() {
            let mut map = self.map.clone();
            // Babelfont requires the default explicitly in the map; Designspace does not.
            if !map.iter().any(|(u, _)| *u == self.default) {
                map.push((self.default, map_value(&map, self.default)));
                map.sort_by(|a, b| a.0.total_cmp(&b.0));
            }
            axis.map = Some(
                map.into_iter()
                    .map(|(u, d)| (UserCoord::new(u), DesignCoord::new(d)))
                    .collect(),
            );
        }
        Ok(axis)
    }

    /// Validate the axis before accepting a project.
    pub fn validate(&self) -> Result<(), String> {
        self.backend()?
            .normalize_userspace_value(UserCoord::new(self.default))
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Convert a user value to normalized design coordinates.
    pub fn user_to_normalized(&self, value: f64) -> f64 {
        self.backend()
            .expect("project axis was validated")
            .normalize_userspace_value(UserCoord::new(value))
            .expect("validated axis conversion")
            .to_f64()
    }

    /// Convert a design value to normalized design coordinates.
    pub fn design_to_normalized(&self, value: f64) -> f64 {
        self.backend()
            .expect("project axis was validated")
            .normalize_designspace_value(DesignCoord::new(value))
            .expect("validated axis conversion")
            .to_f64()
    }

    /// Convert a user coordinate to a design coordinate.
    pub fn user_to_design(&self, value: f64) -> f64 {
        self.backend()
            .expect("project axis was validated")
            .userspace_to_designspace(UserCoord::new(value))
            .expect("validated axis conversion")
            .to_f64()
    }

    /// Convert a design coordinate to a user coordinate.
    pub fn design_to_user(&self, value: f64) -> f64 {
        self.backend()
            .expect("project axis was validated")
            .designspace_to_userspace(DesignCoord::new(value))
            .expect("validated axis conversion")
            .to_f64()
    }

    /// Convert a normalized preview coordinate back to the user scale.
    pub fn normalized_to_user(&self, value: f64) -> f64 {
        let design = super::var_model::denormalize_value(
            value,
            self.user_to_design(self.min),
            self.user_to_design(self.default),
            self.user_to_design(self.max),
        );
        self.design_to_user(design)
    }
}

fn map_value(map: &[(f64, f64)], value: f64) -> f64 {
    if value <= map[0].0 {
        return value + map[0].1 - map[0].0;
    }
    if value >= map[map.len() - 1].0 {
        let last = map[map.len() - 1];
        return value + last.1 - last.0;
    }
    let pair = map
        .windows(2)
        .find(|w| value <= w[1].0)
        .expect("value inside map");
    pair[0].1 + (value - pair[0].0) * (pair[1].1 - pair[0].1) / (pair[1].0 - pair[0].0)
}
