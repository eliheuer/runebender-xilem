// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! SVG in and out for a single glyph.

use kurbo::{Affine, BezPath, PathEl};

use crate::outline::glyph_ops::bezpath_to_contour;

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
