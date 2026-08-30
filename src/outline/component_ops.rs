// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Components inside a glyph: resolving them to contours, hit testing,
//! moving, adding, duplicating, deleting, and decomposing one.

use norad::{Contour, Font, Glyph};

use crate::outline::glyph_paths;

/// Contours of a glyph's components, recursively resolved and
/// rounded to integer units.
pub fn resolved_component_contours(font: &Font, glyph: &Glyph) -> Vec<Contour> {
    fn collect(
        font: &Font,
        glyph: &Glyph,
        parent: kurbo::Affine,
        depth: u8,
        out: &mut Vec<Contour>,
    ) {
        if depth > 8 {
            return;
        }
        for component in &glyph.components {
            let Some(base) = font.get_glyph(&component.base) else {
                continue;
            };
            let affine = parent * glyph_paths::component_affine(&component.transform);
            for contour in &base.contours {
                let mut c = contour.clone();
                for p in c.points.iter_mut() {
                    let q = affine * kurbo::Point::new(p.x, p.y);
                    p.x = q.x.round();
                    p.y = q.y.round();
                }
                out.push(c);
            }
            collect(font, base, affine, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    collect(font, glyph, kurbo::Affine::IDENTITY, 0, &mut out);
    out
}

/// The topmost component whose resolved outline contains the point.
pub fn component_at(font: &Font, glyph: &Glyph, pt: kurbo::Point) -> Option<usize> {
    use kurbo::Shape as _;
    for (i, component) in glyph.components.iter().enumerate().rev() {
        let Some(base) = font.get_glyph(&component.base) else {
            continue;
        };
        let transform = glyph_paths::component_affine(&component.transform);
        let path = transform * &glyph_paths::glyph_to_bezpath(base, font);
        if path.contains(pt) {
            return Some(i);
        }
    }
    None
}

/// Move a component by adjusting its transform offset.
pub fn translate_component(glyph: &mut Glyph, index: usize, dx: f64, dy: f64) -> bool {
    let Some(component) = glyph.components.get_mut(index) else {
        return false;
    };
    component.transform.x_offset += dx;
    component.transform.y_offset += dy;
    true
}

/// Remove a component.
pub fn delete_component(glyph: &mut Glyph, index: usize) -> bool {
    if index >= glyph.components.len() {
        return false;
    }
    glyph.components.remove(index);
    true
}

/// Replace one component with its resolved outline (point-exact,
/// like decompose-all's resolved_component_contours).
pub fn decompose_single_component(font: &Font, glyph: &mut Glyph, index: usize) -> bool {
    let Some(component) = glyph.components.get(index) else {
        return false;
    };
    // A single-component wrapper glyph resolves through the shared
    // collector by pretending the glyph only has this component.
    let mut probe = Glyph::new("probe");
    probe.components.push(component.clone());
    let resolved = resolved_component_contours(font, &probe);
    if resolved.is_empty() {
        return false;
    }
    glyph.contours.extend(resolved);
    glyph.components.remove(index);
    true
}

/// Add a component placing `base`, anchor-locked so a mark lands on
/// its anchor rather than at the origin (web addComponent).
pub fn add_component(font: &Font, glyph: &mut Glyph, base: &str) -> bool {
    if base.is_empty() || base == glyph.name().as_str() {
        return false;
    }
    if font.get_glyph(base).is_none() {
        return false;
    }
    let Ok(base_name) = norad::Name::new(base) else {
        return false;
    };
    glyph.components.push(norad::Component::new(
        base_name,
        norad::AffineTransform::default(),
        None,
    ));
    true
}

/// Duplicate a component, offset by (20, 20). Returns the new index.
pub fn duplicate_component(glyph: &mut Glyph, index: usize) -> Option<usize> {
    let source = glyph.components.get(index)?;
    let mut transform = source.transform;
    transform.x_offset += 20.0;
    transform.y_offset += 20.0;
    let clone = norad::Component::new(source.base.clone(), transform, None);
    glyph.components.push(clone);
    Some(glyph.components.len() - 1)
}
