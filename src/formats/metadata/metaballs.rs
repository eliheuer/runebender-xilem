// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! UFO serialization for editable metaball groups.

pub use crate::font::model::glyph_metadata::{METABALLS_KEY, Metaball, MetaballGroup, Metaballs};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyMetaball {
    id: u32,
    x: f64,
    y: f64,
    radius: f64,
    stiffness: f64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyMetaballGroup {
    id: u32,
    threshold: f64,
    balls: Vec<LegacyMetaball>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyMetaballs {
    version: u32,
    groups: Vec<LegacyMetaballGroup>,
}

fn migrate_legacy(data: LegacyMetaballs) -> Result<Metaballs, String> {
    if data.version != 1 {
        return Err(format!("unsupported metaball version {}", data.version));
    }
    let groups = data
        .groups
        .into_iter()
        .map(|group| {
            let threshold = group.threshold;
            let balls = group
                .balls
                .into_iter()
                .map(|ball| {
                    let magnitude = ball.stiffness.abs();
                    if magnitude <= threshold {
                        return Err("cannot migrate an invisible legacy metaball".to_owned());
                    }
                    let edge = (threshold / magnitude).cbrt();
                    let radius = ball.radius * (1.0 - edge).sqrt();
                    Ok(Metaball {
                        id: ball.id,
                        x: ball.x,
                        y: ball.y,
                        radius,
                        reach: 1.0 / (1.0 - edge).sqrt(),
                        weight: threshold.copysign(ball.stiffness),
                    })
                })
                .collect::<Result<_, String>>()?;
            Ok(MetaballGroup {
                id: group.id,
                threshold,
                balls,
            })
        })
        .collect::<Result<_, String>>()?;
    let data = Metaballs { version: 2, groups };
    data.validate()?;
    Ok(data)
}

/// Reads and validates live metaballs. A missing key returns an empty source.
/// Malformed or newer metadata returns an error, so a caller can preserve it untouched.
pub fn read_metaballs(glyph: &norad::Glyph) -> Result<Metaballs, String> {
    let Some(value) = glyph.lib.get(METABALLS_KEY) else {
        return Ok(Metaballs::default());
    };
    let version = value
        .as_dictionary()
        .and_then(|dictionary| dictionary.get("version"))
        .and_then(plist::Value::as_unsigned_integer)
        .ok_or("metaball metadata has no unsigned version")?;
    let data = if version == 1 {
        migrate_legacy(plist::from_value(value).map_err(|e| e.to_string())?)?
    } else {
        plist::from_value(value).map_err(|e| e.to_string())?
    };
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

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Point;

    #[test]
    fn legacy_strength_migrates_without_changing_the_field() {
        let legacy = LegacyMetaballs {
            version: 1,
            groups: vec![LegacyMetaballGroup {
                id: 1,
                threshold: 0.5,
                balls: vec![LegacyMetaball {
                    id: 1,
                    x: 10.0,
                    y: 20.0,
                    radius: 100.0,
                    stiffness: 2.0,
                }],
            }],
        };
        let mut glyph = norad::Glyph::new("legacy");
        glyph
            .lib
            .insert(METABALLS_KEY.into(), plist::to_value(&legacy).unwrap());

        let data = read_metaballs(&glyph).unwrap();
        let ball = &data.groups[0].balls[0];
        let edge = 0.25_f64.cbrt();
        assert_eq!(data.version, 2);
        assert!((ball.radius - 100.0 * (1.0 - edge).sqrt()).abs() < 1e-12);
        assert!((ball.reach - 1.0 / (1.0 - edge).sqrt()).abs() < 1e-12);
        assert_eq!(ball.weight, 0.5);

        let point = Point::new(60.0, 20.0);
        let old = 2.0 * (1.0 - 50.0_f64.powi(2) / 100.0_f64.powi(2)).powi(3);
        assert!((crate::outline::metaballs::field(&data.groups[0], point) - old).abs() < 1e-12);
    }
}
