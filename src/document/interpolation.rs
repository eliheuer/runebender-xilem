// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Checked interpolation of glyph-local sources, retaining exact UFO payloads.
//!
//! Advances, vertical advances, points, anchors and affine component coefficients
//! interpolate as f64 values. Non-varying metadata comes from the default layer,
//! never the currently selected editor source.

use super::var_model::{Location, VariationModel};

pub(super) fn compatible(base: &norad::Glyph, other: &norad::Glyph) -> bool {
    base.contours.len() == other.contours.len()
        && base.contours.iter().zip(&other.contours).all(|(a, b)| {
            a.points.len() == b.points.len()
                && a.points.iter().zip(&b.points).all(|(a, b)| a.typ == b.typ)
        })
        && base.components.len() == other.components.len()
        && base
            .components
            .iter()
            .zip(&other.components)
            .all(|(a, b)| a.base == b.base)
        && base.anchors.len() == other.anchors.len()
        && base
            .anchors
            .iter()
            .all(|a| other.anchors.iter().filter(|b| a.name == b.name).count() == 1)
}

fn values(glyph: &norad::Glyph, base: &norad::Glyph) -> Vec<f64> {
    let mut values = vec![glyph.width, glyph.height];
    for contour in &glyph.contours {
        for point in &contour.points {
            values.extend([point.x, point.y]);
        }
    }
    for anchor in &base.anchors {
        let a = glyph
            .anchors
            .iter()
            .find(|a| a.name == anchor.name)
            .expect("validated anchors");
        values.extend([a.x, a.y]);
    }
    for component in &glyph.components {
        let t = component.transform;
        values.extend([
            t.x_scale, t.xy_scale, t.yx_scale, t.y_scale, t.x_offset, t.y_offset,
        ]);
    }
    values
}

pub(super) fn interpolate(
    glyphs: &[norad::Glyph],
    locations: &[Location],
    target: &Location,
) -> Result<norad::Glyph, String> {
    let default = locations
        .iter()
        .position(|l| l.values().all(|v| v.abs() < 1e-9))
        .ok_or("glyph has no layer at the default location")?;
    let base = glyphs.get(default).ok_or("missing default glyph")?;
    if glyphs.len() != locations.len() || glyphs.iter().any(|g| !compatible(base, g)) {
        return Err(format!(
            "{}: incompatible contours, point types, components or anchors",
            base.name()
        ));
    }
    if locations
        .iter()
        .chain(std::iter::once(target))
        .any(|l| l.values().any(|v| !v.is_finite()))
    {
        return Err("interpolation location must be finite".into());
    }
    for (index, location) in locations.iter().enumerate() {
        if locations[..index]
            .iter()
            .any(|other| same_location(location, other))
        {
            return Err(format!("{}: duplicate glyph-source location", base.name()));
        }
    }
    let values: Vec<_> = glyphs.iter().map(|g| values(g, base)).collect();
    if values.iter().flatten().any(|v| !v.is_finite()) {
        return Err(format!("{}: non-finite glyph geometry", base.name()));
    }
    let output = VariationModel::new(locations)?.interpolate(&values, target)?;
    if output.iter().any(|v| !v.is_finite()) {
        return Err("non-finite interpolation result".into());
    }
    let mut iter = output.into_iter();
    let mut next = || iter.next().expect("validated interpolation dimensions");
    let mut glyph = base.clone();
    glyph.width = next();
    glyph.height = next();
    for contour in &mut glyph.contours {
        for point in &mut contour.points {
            point.x = next();
            point.y = next();
        }
    }
    for anchor in &mut glyph.anchors {
        anchor.x = next();
        anchor.y = next();
    }
    for component in &mut glyph.components {
        component.transform = norad::AffineTransform {
            x_scale: next(),
            xy_scale: next(),
            yx_scale: next(),
            y_scale: next(),
            x_offset: next(),
            y_offset: next(),
        };
    }
    Ok(glyph)
}

fn same_location(a: &Location, b: &Location) -> bool {
    a.keys()
        .chain(b.keys())
        .all(|key| a.get(key).copied().unwrap_or(0.0) == b.get(key).copied().unwrap_or(0.0))
}
