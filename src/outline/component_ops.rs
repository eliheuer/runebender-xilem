// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Components inside a glyph: resolving them to contours, hit testing,
//! moving, adding, duplicating, deleting, and decomposing one.

#[cfg(test)]
use norad::{Contour, Font, Glyph};

use crate::outline::glyph_paths;

/// Canonical geometry resolved for one stable top-level component.
#[derive(Clone, Debug)]
pub struct ResolvedDocumentComponent {
    /// Stable identity of the top-level component.
    pub id: crate::font::ComponentId,
    /// Exact rendered path used for hit testing and selection feedback.
    ///
    /// This includes canonical smart-component interpolation, hyperbeziers and metaballs.
    pub path: kurbo::BezPath,
    /// Recursively resolved and rounded structural contours used by canonical decomposition.
    ///
    /// This intentionally preserves the existing decomposition contract rather than flattening
    /// rendered smart-component or metaball outlines.
    pub contours: Vec<crate::font::CopiedContour>,
}

fn collect_document_component_contours<'a>(
    layer: crate::font::LayerView<'a>,
    transform: kurbo::Affine,
    resolve: &mut impl FnMut(&str) -> Option<crate::font::LayerView<'a>>,
    stack: &mut Vec<String>,
    output: &mut Vec<crate::font::CopiedContour>,
) -> Result<(), glyph_paths::ComponentResolveError> {
    if stack.len() > 64 {
        return Err(glyph_paths::ComponentResolveError::TooDeep);
    }
    for shape in layer.shapes() {
        match shape {
            crate::font::LayerShapeView::Contour(contour) => {
                output.push(
                    contour
                        .copied()
                        .transformed_rounded(transform)
                        .ok_or(glyph_paths::ComponentResolveError::NonFinite)?,
                );
            }
            crate::font::LayerShapeView::Component(component) => {
                let name = component.reference();
                if let Some(start) = stack.iter().position(|entry| entry == name) {
                    let mut cycle = stack[start..].to_vec();
                    cycle.push(name.to_owned());
                    return Err(glyph_paths::ComponentResolveError::Cycle(cycle));
                }
                let base = resolve(name)
                    .ok_or_else(|| glyph_paths::ComponentResolveError::Missing(name.to_owned()))?;
                stack.push(name.to_owned());
                let result = collect_document_component_contours(
                    base,
                    transform * component.transform(),
                    resolve,
                    stack,
                    output,
                );
                stack.pop();
                result?;
            }
        }
    }
    Ok(())
}

/// Resolve every canonical component into transformed contour copies.
///
/// Nested components use the caller's layer resolver. Geometry is rounded to integer font units,
/// matching the existing decomposition command, while contour and point source metadata remains
/// attached for the canonical paste boundary to re-identify safely.
pub fn resolved_document_component_contours<'a>(
    layer: crate::font::LayerView<'a>,
    mut resolve: impl FnMut(&str) -> Option<crate::font::LayerView<'a>>,
) -> Result<Vec<crate::font::CopiedContour>, glyph_paths::ComponentResolveError> {
    let mut output = Vec::new();
    let mut stack = vec![layer.glyph_name().to_owned()];
    for component in layer.components() {
        let name = component.reference();
        if let Some(start) = stack.iter().position(|entry| entry == name) {
            let mut cycle = stack[start..].to_vec();
            cycle.push(name.to_owned());
            return Err(glyph_paths::ComponentResolveError::Cycle(cycle));
        }
        let base = resolve(name)
            .ok_or_else(|| glyph_paths::ComponentResolveError::Missing(name.to_owned()))?;
        stack.push(name.to_owned());
        let result = collect_document_component_contours(
            base,
            component.transform(),
            &mut resolve,
            &mut stack,
            &mut output,
        );
        stack.pop();
        result?;
    }
    Ok(output)
}

/// Resolve each top-level canonical component while retaining its stable identity.
///
/// The rendered path supports hit testing and selection feedback, including special outlines.
/// The contour copies separately retain the existing integer-rounded structural decomposition
/// contract and canonical source metadata.
pub fn resolved_document_components<'a>(
    layer: crate::font::LayerView<'a>,
    mut resolve: impl FnMut(&str) -> Option<crate::font::LayerView<'a>>,
    mut layers: impl FnMut(&str) -> Vec<crate::font::LayerView<'a>>,
) -> Result<Vec<ResolvedDocumentComponent>, glyph_paths::ComponentResolveError> {
    let mut output = Vec::new();
    for component in layer.components() {
        let path = glyph_paths::canonical_component_to_bezpath(
            layer,
            component,
            &mut resolve,
            &mut layers,
        )?;
        let name = component.reference();
        let base = resolve(name)
            .ok_or_else(|| glyph_paths::ComponentResolveError::Missing(name.to_owned()))?;
        let mut contours = Vec::new();
        let mut stack = vec![layer.glyph_name().to_owned(), name.to_owned()];
        collect_document_component_contours(
            base,
            component.transform(),
            &mut resolve,
            &mut stack,
            &mut contours,
        )?;
        output.push(ResolvedDocumentComponent {
            id: component.id(),
            path,
            contours,
        });
    }
    Ok(output)
}

/// Contours of a glyph's components, recursively resolved and
/// rounded to integer units.
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
pub fn translate_component(glyph: &mut Glyph, index: usize, dx: f64, dy: f64) -> bool {
    let Some(component) = glyph.components.get_mut(index) else {
        return false;
    };
    component.transform.x_offset += dx;
    component.transform.y_offset += dy;
    true
}

/// Remove a component.
#[cfg(test)]
pub fn delete_component(glyph: &mut Glyph, index: usize) -> bool {
    if index >= glyph.components.len() {
        return false;
    }
    glyph.components.remove(index);
    true
}

/// Replace one component with its resolved outline.
///
/// The outline is point-exact with what `resolved_component_contours`
/// produces, so decomposing one component matches decomposing all.
#[cfg(test)]
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

/// Add a component that places `base` in the glyph.
///
/// The placement is anchor-locked: a mark lands on its anchor
/// rather than at the origin. This is `addComponent` in the web
/// editor.
#[cfg(test)]
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
#[cfg(test)]
pub fn duplicate_component(glyph: &mut Glyph, index: usize) -> Option<usize> {
    let source = glyph.components.get(index)?;
    let mut transform = source.transform;
    transform.x_offset += 20.0;
    transform.y_offset += 20.0;
    let clone = norad::Component::new(source.base.clone(), transform, None);
    glyph.components.push(clone);
    Some(glyph.components.len() - 1)
}

#[cfg(test)]
mod canonical_tests {
    use super::*;
    use crate::font::model::glyph_metadata::{Metaball, MetaballGroup, Metaballs};
    use crate::font::project::Project;
    use crate::font::source::SourceInput;
    use crate::font::variable::{GlyphLayerAddress, SourceId};
    use crate::formats::metadata::metaballs::write_metaballs;
    use norad::{AffineTransform, Component, Name};

    #[test]
    fn rendered_component_path_keeps_structural_decomposition_separate() {
        let mut blob = Glyph::new("blob");
        write_metaballs(
            &mut blob,
            &Metaballs {
                version: 1,
                groups: vec![MetaballGroup {
                    id: 1,
                    threshold: 0.5,
                    balls: vec![Metaball {
                        id: 1,
                        x: 40.0,
                        y: 55.0,
                        radius: 45.0,
                        stiffness: 2.0,
                    }],
                }],
            },
        )
        .unwrap();
        let mut user = Glyph::new("blob.user");
        user.components.push(Component::new(
            Name::new("blob").unwrap(),
            AffineTransform {
                x_scale: 1.25,
                xy_scale: 0.125,
                yx_scale: -0.25,
                y_scale: 0.875,
                x_offset: 37.0,
                y_offset: -19.0,
            },
            None,
        ));
        let mut font = Font::default();
        font.default_layer_mut().insert_glyph(blob);
        font.default_layer_mut().insert_glyph(user);
        let project =
            Project::from_source(SourceInput::from_font(font, "MetaballComponent.ufo".into()));
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        let root = project.document_layer("blob.user", &layer).unwrap();

        let resolved = resolved_document_components(
            root,
            |name| project.document_layer(name, &layer),
            |name| {
                project
                    .document_glyph(name)
                    .map(|glyph| {
                        glyph
                            .layer_ids()
                            .filter(|candidate| candidate.source == layer.source)
                            .filter_map(|candidate| glyph.layer(candidate))
                            .collect()
                    })
                    .unwrap_or_default()
            },
        )
        .unwrap();
        let expected = project
            .document_layer_path(&GlyphLayerAddress {
                glyph: "blob.user".into(),
                layer,
            })
            .unwrap();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].path, expected);
        assert!(!resolved[0].path.is_empty());
        assert!(
            resolved[0].contours.is_empty(),
            "metaball rendering must not silently change structural decomposition"
        );
    }
}
