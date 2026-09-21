// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Visible size and influence controls for the unchanged compact radial field.

use crate::formats::metaballs::Metaball;

/// Returns isolated visible radius and remaining influence reach for a positive element.
/// Returns `None` for a zero, negative, subthreshold or invalid field.
/// This is a read-only change of coordinates, not a change to the stored geometry.
pub fn visible_size(ball: &Metaball, threshold: f64) -> Option<(f64, f64)> {
    if !threshold.is_finite()
        || threshold <= 0.0
        || !ball.stiffness.is_finite()
        || ball.stiffness <= threshold
        || !ball.radius.is_finite()
        || ball.radius <= 0.0
    {
        return None;
    }
    let size = ball.radius * (1.0 - (threshold / ball.stiffness).cbrt()).sqrt();
    let reach = ball.radius - size;
    (size > 0.0 && reach > 0.0).then_some((size, reach))
}

/// Sets isolated radius and reach without changing the kernel or group threshold.
/// Rejects unrepresentable values without mutating the element; no values are clamped.
pub fn set_size_and_reach(
    ball: &mut Metaball,
    threshold: f64,
    size: f64,
    reach: f64,
) -> Result<(), String> {
    let Some(current) = visible_size(ball, threshold) else {
        return Err("This element requires the raw field controls".into());
    };
    if current == (size, reach) {
        return Ok(());
    }
    let radius = size + reach;
    let stiffness = threshold / (1.0 - (size / radius).powi(2)).powi(3);
    if !size.is_finite()
        || size <= 0.0
        || !reach.is_finite()
        || reach <= 0.0
        || !radius.is_finite()
        || !(1.0..=100_000.0).contains(&radius)
        || !stiffness.is_finite()
        || stiffness > 100.0
        || stiffness <= threshold
    {
        return Err("Size and blend reach exceed the supported field range".into());
    }
    ball.radius = radius;
    ball.stiffness = stiffness;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_and_reach_roundtrip_the_legacy_field() {
        for (radius, stiffness, threshold) in
            [(180.0, 2.0, 0.5), (800.0, 0.3, 0.1), (1.0, 100.0, 0.01)]
        {
            let mut ball = Metaball {
                id: 1,
                x: 0.0,
                y: 0.0,
                radius,
                stiffness,
            };
            let (size, reach) = visible_size(&ball, threshold).unwrap();
            set_size_and_reach(&mut ball, threshold, size, reach).unwrap();
            assert!((ball.radius - radius).abs() < 1e-10);
            assert!((ball.stiffness - stiffness).abs() < 1e-9);
            for fraction in [0.0, 0.3, 0.6, 0.9, 1.1] {
                let distance = fraction * radius;
                let before = stiffness * (1.0 - (distance / radius).powi(2)).max(0.0).powi(3);
                let after =
                    ball.stiffness * (1.0 - (distance / ball.radius).powi(2)).max(0.0).powi(3);
                assert!((before - after).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn reach_preserves_isolated_size_and_invalid_edits_preserve_source() {
        let mut ball = Metaball {
            id: 1,
            x: 0.0,
            y: 0.0,
            radius: 180.0,
            stiffness: 2.0,
        };
        let (size, reach) = visible_size(&ball, 0.5).unwrap();
        set_size_and_reach(&mut ball, 0.5, size, reach * 3.0).unwrap();
        assert!((visible_size(&ball, 0.5).unwrap().0 - size).abs() < 1e-10);
        let before = ball.clone();
        for reach in [0.0, -1.0, f64::NAN, 1e-10, 1e6] {
            assert!(set_size_and_reach(&mut ball, 0.5, size, reach).is_err());
            assert_eq!(ball, before);
        }
        for strength in [-2.0, 0.0, 0.25, 0.5] {
            ball.stiffness = strength;
            assert!(visible_size(&ball, 0.5).is_none());
        }
    }
}
