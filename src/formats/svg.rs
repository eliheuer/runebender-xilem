// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! SVG in and out: one glyph, or a proof sheet of many.

use kurbo::{Affine, BezPath, PathEl};

use crate::document::project::Master;
use crate::outline::glyph_ops::bezpath_to_contour;
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
        let Some(end) = body.find(quote) else { break };
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
pub fn proof_sheet(
    master: &Master,
    layer: Option<&str>,
    names: &[String],
    columns: usize,
) -> Result<ProofSheet, String> {
    if names.is_empty() {
        return Err("no glyph to draw".into());
    }
    let preview = layer
        .map(|name| crate::document::proposal::preview_font(&master.font, name))
        .transpose()?;
    let font = preview.as_ref().unwrap_or(&master.font);
    let layer = match layer {
        Some(l) => Some(
            font.layers
                .get(l)
                .ok_or_else(|| format!("no layer named {l}"))?,
        ),
        None => None,
    };
    let columns = columns.clamp(1, names.len());
    let upm = master.units_per_em;
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
            Some(l) => l.get_glyph(name.as_str()),
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
        svg.push_str(&line(master.ascender, "#ccc"));
        svg.push_str(&line(master.descender, "#ccc"));
        if let Some(x) = master.x_height {
            svg.push_str(&line(x, "#bbb"));
        }
        if let Some(c) = master.cap_height {
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
        let contours = svg_to_contours(svg, 800.0, -200.0).expect("parses");
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
        let c = svg_to_contours(curvy, 800.0, -200.0).expect("parses curves");
        assert!(
            c[0].points
                .iter()
                .any(|p| p.typ == norad::PointType::OffCurve)
        );
        // No path data errors cleanly.
        assert!(svg_to_contours("<svg></svg>", 800.0, -200.0).is_err());
    }
}
