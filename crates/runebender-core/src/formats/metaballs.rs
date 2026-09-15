// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Editable metaball groups stored independently of a glyph's ordinary contours.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Versioned glyph metadata for live metaballs; ordinary UFO anchors are unrelated.
pub const METABALLS_KEY: &str = "com.runebender.metaballs";

/// A center and its compact, radial influence field, in font coordinates.
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
    /// Field strength at the center. Positive values add ink; negative values subtract it.
    pub stiffness: f64,
}

/// Elements whose fields blend together. Separate groups never influence each other.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetaballGroup {
    /// Stable group identifier, unique in a glyph.
    pub id: u32,
    /// Positive field level at the visible boundary.
    pub threshold: f64,
    /// Editable centers, retained until explicit conversion.
    pub balls: Vec<Metaball>,
}

/// The editable metaball source in one glyph. No preview contours are stored here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metaballs {
    /// Schema version; currently only version 1 is supported.
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
    /// Checks schema, identifiers, finite coordinates and bounded field parameters.
    /// Rejects invalid data without changing it. At most 128 groups and 256 centers are allowed.
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

/// Reads and validates live metaballs. A missing key returns an empty source.
/// Malformed or newer metadata returns an error, so a caller can preserve it untouched.
pub fn read_metaballs(glyph: &norad::Glyph) -> Result<Metaballs, String> {
    let Some(value) = glyph.lib.get(METABALLS_KEY) else {
        return Ok(Metaballs::default());
    };
    let data: Metaballs = plist::from_value(value).map_err(|e| e.to_string())?;
    data.validate()?;
    Ok(data)
}

/// Validates and writes live metaballs, returning whether the lib changed.
/// Empty sources remove the key. An error leaves the entire glyph untouched.
pub fn write_metaballs(glyph: &mut norad::Glyph, data: &Metaballs) -> Result<bool, String> {
    data.validate()?;
    if data.groups.is_empty() {
        return Ok(glyph.lib.remove(METABALLS_KEY).is_some());
    }
    let value = plist::to_value(data).map_err(|e| e.to_string())?;
    if glyph.lib.get(METABALLS_KEY) == Some(&value) {
        return Ok(false);
    }
    glyph.lib.insert(METABALLS_KEY.into(), value);
    Ok(true)
}
