// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! SVG in and out: one glyph, or a proof sheet of many.

use kurbo::{Affine, BezPath, PathEl};

use crate::document::experiments::Experiment;
use crate::document::project::Project;
use crate::document::variable::{GlyphLayerAddress, LayerId, SourceId};
use crate::outline::glyph_ops::bezpath_to_contour;
#[cfg(test)]
use crate::outline::glyph_paths;

/// A standalone SVG document for one glyph.
///
/// The outline is in font units with y flipped into SVG space. The
/// viewBox spans the em, ascender down to descender, across the
/// advance.
pub fn glyph_svg(path: &BezPath, advance: f64, ascender: f64, descender: f64) -> String {
    let height = ascender - descender;
    format!(
        concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" ",
            "viewBox=\"0 0 {w} {h}\">\n",
            "  <path transform=\"translate(0,{asc}) scale(1,-1)\" ",
            "d=\"{d}\"/>\n",
            "</svg>\n"
        ),
        w = advance,
        h = height,
        asc = ascender,
        d = path.to_svg(),
    )
}

/// Pull every path's `d` attribute out of an SVG document into
/// contours.
///
/// Paths parse with kurbo, flip to font coordinates because SVG
/// runs y-down, and fit between `descender` and `ascender` as one
/// drawing. Fills, strokes, groups, and transforms are ignored:
/// this is the Illustrator-outline paste, not a renderer.
pub fn svg_to_contours(
    svg_text: &str,
    ascender: f64,
    descender: f64,
) -> Result<crate::document::ImportedContours, String> {
    let contours = svg_to_ufo_contours(svg_text, ascender, descender)?;
    crate::formats::ufo::decode_contours(&contours)
}

fn svg_to_ufo_contours(
    svg_text: &str,
    ascender: f64,
    descender: f64,
) -> Result<Vec<norad::Contour>, String> {
    let mut combined = BezPath::new();
    let mut rest = svg_text;
    while let Some(at) = rest.find(" d=") {
        let after = &rest[at + 3..];
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            rest = after;
            continue;
        };
        let body = &after[1..];
        let Some(end) = body.find(quote) else {
            break;
        };
        let data = &body[..end];
        let path = BezPath::from_svg(data).map_err(|e| format!("SVG path: {e}"))?;
        combined.extend(path.elements().iter().copied());
        rest = &body[end..];
    }
    if combined.elements().is_empty() {
        return Err("no <path d=\"…\"> outlines in the SVG".into());
    }
    use kurbo::Shape as _;
    let bbox = combined.bounding_box();
    if bbox.height() < 1e-6 {
        return Err("SVG outlines have no height".into());
    }
    let scale = (ascender - descender) / bbox.height();
    // Flip and fit: SVG top lands on the ascender.
    let fitted = Affine::translate((0.0, ascender))
        * Affine::scale_non_uniform(scale, -scale)
        * Affine::translate((-bbox.x0, -bbox.y0))
        * combined;
    let empty = std::collections::HashMap::new();
    let mut contours = Vec::new();
    let mut sub = BezPath::new();
    for el in fitted.elements() {
        if matches!(el, PathEl::MoveTo(_)) && !sub.elements().is_empty() {
            if let Some(c) = bezpath_to_contour(&sub, &empty) {
                contours.push(c);
            }
            sub = BezPath::new();
        }
        sub.push(*el);
    }
    if !sub.elements().is_empty()
        && let Some(c) = bezpath_to_contour(&sub, &empty)
    {
        contours.push(c);
    }
    (!contours.is_empty())
        .then_some(contours)
        .ok_or_else(|| "SVG outlines did not convert".into())
}

/// A proof sheet and what it measured.
#[derive(Debug, Clone)]
pub struct ProofSheet {
    /// The SVG document.
    pub svg: String,
    /// One row per glyph: name, advance, sidebearings, bounds, point
    /// and contour counts.
    pub metrics: Vec<serde_json::Value>,
}

/// A sheet of glyphs in cells, `columns` across, with baseline and
/// vertical metrics ruled in each cell. `layer` names a layer to draw
/// from; None draws the foreground. Errors name a glyph that is not
/// there.
#[cfg(test)]
pub fn proof_sheet(
    font: &norad::Font,
    layer: Option<&str>,
    names: &[String],
    columns: usize,
) -> Result<ProofSheet, String> {
    if names.is_empty() {
        return Err("no glyph to draw".into());
    }
    let preview = layer
        .map(|name| crate::document::proposal::preview_font(font, name))
        .transpose()?;
    let font = preview.as_ref().unwrap_or(font);
    let layer = match layer {
        Some(l) => Some(
            font.layers
                .get(l)
                .ok_or_else(|| format!("no layer named {l}"))?,
        ),
        None => None,
    };
    let columns = columns.clamp(1, names.len());
    let upm = font
        .font_info
        .units_per_em
        .map(|value| value.as_f64())
        .unwrap_or(1000.0);
    let ascender = font.font_info.ascender.unwrap_or(upm * 0.8);
    let descender = font.font_info.descender.unwrap_or(-(upm * 0.2));
    let cell_w = upm * 1.2;
    let cell_h = upm * 1.4;
    let rows = names.len().div_ceil(columns);
    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" \
         viewBox=\"0 0 {} {}\">\n<rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n",
        (cell_w * columns as f64 / 4.0).round(),
        (cell_h * rows as f64 / 4.0).round(),
        cell_w * columns as f64,
        cell_h * rows as f64
    ));
    let mut metrics = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let glyph = match layer {
            Some(l) => l
                .get_glyph(name.as_str())
                .or_else(|| font.get_glyph(name.as_str())),
            None => font.get_glyph(name.as_str()),
        };
        let Some(glyph) = glyph else {
            return Err(format!("no glyph named {name}"));
        };
        let path = glyph_paths::glyph_to_bezpath(glyph, font);
        let col = (i % columns) as f64;
        let row = (i / columns) as f64;
        let x0 = col * cell_w + upm * 0.1;
        let baseline = row * cell_h + upm * 1.05;
        let label = name
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        svg.push_str(&format!("<text x=\"{x0}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"40\">{label}</text>\n", row * cell_h + 60.0));
        let line = |y: f64, color: &str| {
            format!(
                "<line x1=\"{x0:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" \
                 stroke=\"{color}\" stroke-width=\"2\"/>\n",
                baseline - y,
                x0 + glyph.width,
                baseline - y
            )
        };
        svg.push_str(&line(0.0, "#999"));
        svg.push_str(&line(ascender, "#ccc"));
        svg.push_str(&line(descender, "#ccc"));
        if let Some(x) = font.font_info.x_height {
            svg.push_str(&line(x, "#bbb"));
        }
        if let Some(c) = font.font_info.cap_height {
            svg.push_str(&line(c, "#bbb"));
        }
        svg.push_str(&format!(
            "<path transform=\"translate({x0:.1} {baseline:.1}) scale(1 -1)\" d=\"{}\" fill=\"black\"/>\n",
            path.to_svg()
        ));
        use kurbo::Shape as _;
        let bounds = path.bounding_box();
        let drawn = !path.is_empty();
        metrics.push(serde_json::json!({
            "glyph": name,
            "advance": glyph.width,
            "lsb": if drawn { Some(bounds.x0.round()) } else { None },
            "rsb": if drawn { Some((glyph.width - bounds.x1).round()) } else { None },
            "bounds": if drawn { Some([bounds.x0, bounds.y0, bounds.x1, bounds.y1]) } else { None },
            "points": glyph.contours.iter().map(|c| c.points.len()).sum::<usize>(),
            "contours": glyph.contours.len(),
            "components": glyph.components.len(),
        }));
    }
    svg.push_str("</svg>\n");
    Ok(ProofSheet { svg, metrics })
}

/// Render a proof sheet directly from one canonical Project source.
pub fn proof_sheet_project(
    project: &Project,
    source: SourceId,
    layer: Option<&str>,
    names: &[String],
    columns: usize,
) -> Result<ProofSheet, String> {
    if names.is_empty() {
        return Err("no glyph to draw".into());
    }
    let source_view = project
        .document_source(source)
        .ok_or("proof source does not exist")?;
    let default_layer = source_view.default_layer();
    let requested_layer = layer
        .map(|name| {
            let exists = project
                .document_source_layer_names(source)
                .is_some_and(|names| names.contains(&name));
            exists
                .then(|| LayerId {
                    source,
                    name: name.to_owned(),
                })
                .ok_or_else(|| format!("no layer named {name}"))
        })
        .transpose()?;
    let info = project
        .document_font_info(source)
        .ok_or("proof source has no canonical font information")?;
    let resolved = info.metrics.resolved();
    let columns = columns.clamp(1, names.len());
    let cell_w = resolved.units_per_em * 1.2;
    let cell_h = resolved.units_per_em * 1.4;
    let rows = names.len().div_ceil(columns);
    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" \
         viewBox=\"0 0 {} {}\">\n<rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n",
        (cell_w * columns as f64 / 4.0).round(),
        (cell_h * rows as f64 / 4.0).round(),
        cell_w * columns as f64,
        cell_h * rows as f64
    ));
    let mut proof_metrics = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let layer = requested_layer
            .as_ref()
            .filter(|layer| project.document_layer(name, layer).is_some())
            .unwrap_or(&default_layer);
        let view = project
            .document_layer(name, layer)
            .ok_or_else(|| format!("no glyph named {name}"))?;
        let path = project
            .document_layer_path(&GlyphLayerAddress {
                glyph: name.clone(),
                layer: layer.clone(),
            })
            .map_err(|error| error.to_string())?;
        let column = (index % columns) as f64;
        let row = (index / columns) as f64;
        let x0 = column * cell_w + resolved.units_per_em * 0.1;
        let baseline = row * cell_h + resolved.units_per_em * 1.05;
        let label = name
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        svg.push_str(&format!(
            "<text x=\"{x0}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"40\">{label}</text>\n",
            row * cell_h + 60.0
        ));
        let line = |y: f64, color: &str| {
            format!(
                "<line x1=\"{x0:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" \
                 stroke=\"{color}\" stroke-width=\"2\"/>\n",
                baseline - y,
                x0 + view.width(),
                baseline - y
            )
        };
        svg.push_str(&line(0.0, "#999"));
        svg.push_str(&line(resolved.ascender, "#ccc"));
        svg.push_str(&line(resolved.descender, "#ccc"));
        if let Some(x_height) = info.metrics.x_height {
            svg.push_str(&line(x_height, "#bbb"));
        }
        if let Some(cap_height) = info.metrics.cap_height {
            svg.push_str(&line(cap_height, "#bbb"));
        }
        svg.push_str(&format!(
            "<path transform=\"translate({x0:.1} {baseline:.1}) scale(1 -1)\" d=\"{}\" fill=\"black\"/>\n",
            path.to_svg()
        ));
        use kurbo::Shape as _;
        let bounds = path.bounding_box();
        let drawn = !path.is_empty();
        proof_metrics.push(serde_json::json!({
            "glyph": name,
            "advance": view.width(),
            "lsb": if drawn { Some(bounds.x0.round()) } else { None },
            "rsb": if drawn { Some((view.width() - bounds.x1).round()) } else { None },
            "bounds": if drawn { Some([bounds.x0, bounds.y0, bounds.x1, bounds.y1]) } else { None },
            "points": view.contours().map(|contour| contour.points().count()).sum::<usize>(),
            "contours": view.contours().count(),
            "components": view.components().count(),
        }));
    }
    svg.push_str("</svg>\n");
    Ok(ProofSheet {
        svg,
        metrics: proof_metrics,
    })
}

/// Render a proof sheet directly from one canonical experiment snapshot.
pub fn proof_sheet_experiment(
    project: &Project,
    experiment: &Experiment,
    layer: Option<&str>,
    names: &[String],
    columns: usize,
) -> Result<ProofSheet, String> {
    if names.is_empty() {
        return Err("no glyph to draw".into());
    }
    if let Some(layer) = layer
        && !experiment
            .layer_drafts()
            .any(|(address, _)| address.layer.name == layer)
    {
        return Err(format!("no layer named {layer}"));
    }
    let info = project
        .document_font_info(experiment.root)
        .ok_or("proof source has no canonical font information")?;
    let resolved = info.metrics.resolved();
    let columns = columns.clamp(1, names.len());
    let cell_w = resolved.units_per_em * 1.2;
    let cell_h = resolved.units_per_em * 1.4;
    let rows = names.len().div_ceil(columns);
    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" \
         viewBox=\"0 0 {} {}\">\n<rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n",
        (cell_w * columns as f64 / 4.0).round(),
        (cell_h * rows as f64 / 4.0).round(),
        cell_w * columns as f64,
        cell_h * rows as f64
    ));
    let mut proof_metrics = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let (selected, view) = experiment
            .selected_layer(name, layer)
            .or_else(|| experiment.selected_layer(name, None))
            .ok_or_else(|| format!("no glyph named {name}"))?;
        let path = experiment
            .layer_path(name, &selected)
            .map_err(|error| error.to_string())?;
        let column = (index % columns) as f64;
        let row = (index / columns) as f64;
        let x0 = column * cell_w + resolved.units_per_em * 0.1;
        let baseline = row * cell_h + resolved.units_per_em * 1.05;
        let label = name
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        svg.push_str(&format!(
            "<text x=\"{x0}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"40\">{label}</text>\n",
            row * cell_h + 60.0
        ));
        let line = |y: f64, color: &str| {
            format!(
                "<line x1=\"{x0:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" \
                 stroke=\"{color}\" stroke-width=\"2\"/>\n",
                baseline - y,
                x0 + view.width(),
                baseline - y
            )
        };
        svg.push_str(&line(0.0, "#999"));
        svg.push_str(&line(resolved.ascender, "#ccc"));
        svg.push_str(&line(resolved.descender, "#ccc"));
        if let Some(x_height) = info.metrics.x_height {
            svg.push_str(&line(x_height, "#bbb"));
        }
        if let Some(cap_height) = info.metrics.cap_height {
            svg.push_str(&line(cap_height, "#bbb"));
        }
        svg.push_str(&format!(
            "<path transform=\"translate({x0:.1} {baseline:.1}) scale(1 -1)\" d=\"{}\" fill=\"black\"/>\n",
            path.to_svg()
        ));
        use kurbo::Shape as _;
        let bounds = path.bounding_box();
        let drawn = !path.is_empty();
        proof_metrics.push(serde_json::json!({
            "glyph": name,
            "advance": view.width(),
            "lsb": if drawn { Some(bounds.x0.round()) } else { None },
            "rsb": if drawn { Some((view.width() - bounds.x1).round()) } else { None },
            "bounds": if drawn { Some([bounds.x0, bounds.y0, bounds.x1, bounds.y1]) } else { None },
            "points": view.contours().map(|contour| contour.points().count()).sum::<usize>(),
            "contours": view.contours().count(),
            "components": view.components().count(),
        }));
    }
    svg.push_str("</svg>\n");
    Ok(ProofSheet {
        svg,
        metrics: proof_metrics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_svg_wraps_the_outline_in_font_units() {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((100.0, 0.0));
        path.line_to((100.0, 700.0));
        path.close_path();
        let svg = glyph_svg(&path, 600.0, 800.0, -200.0);
        assert!(svg.contains("viewBox=\"0 0 600 1000\""));
        assert!(svg.contains("translate(0,800) scale(1,-1)"));
        assert!(svg.contains("M0,0"));
        assert!(svg.ends_with("</svg>\n"));
    }
    #[test]
    fn svg_import_fits_and_flips() {
        // A 10x20 SVG rectangle path lands between descender and
        // ascender, y flipped, aspect kept.
        let svg = r#"<svg xmlns="x" viewBox="0 0 10 20">
            <g><path fill="red" d="M0,0 L10,0 L10,20 L0,20 Z"/></g>
        </svg>"#;
        let contours = svg_to_ufo_contours(svg, 800.0, -200.0).expect("parses");
        assert_eq!(contours.len(), 1);
        let ys: Vec<f64> = contours[0].points.iter().map(|p| p.y).collect();
        let xs: Vec<f64> = contours[0].points.iter().map(|p| p.x).collect();
        let (min_y, max_y) = ys
            .iter()
            .fold((f64::MAX, f64::MIN), |a, &v| (a.0.min(v), a.1.max(v)));
        let (min_x, max_x) = xs
            .iter()
            .fold((f64::MAX, f64::MIN), |a, &v| (a.0.min(v), a.1.max(v)));
        assert_eq!((min_y, max_y), (-200.0, 800.0), "fills the em");
        assert_eq!(min_x, 0.0);
        assert!((max_x - 500.0).abs() < 1.0, "aspect kept: {max_x}");
        // Curves survive.
        let curvy = r#"<path d="M0 0 C 10 0 20 10 20 20 L 0 20 Z"/>"#;
        let c = svg_to_ufo_contours(curvy, 800.0, -200.0).expect("parses curves");
        assert!(
            c[0].points
                .iter()
                .any(|p| p.typ == norad::PointType::OffCurve)
        );
        // No path data errors cleanly.
        assert!(svg_to_contours("<svg></svg>", 800.0, -200.0).is_err());
    }
}
