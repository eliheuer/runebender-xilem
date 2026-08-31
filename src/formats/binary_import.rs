// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Import a compiled TTF or OTF as a UFO, through skrifa.

use std::collections::HashSet;
use std::path::Path;

/// A pen that collects skrifa outline callbacks into UFO contours.
/// Quadratics stay quadratic (offcurve + qcurve points), cubics stay
/// cubic; every binary contour is closed.
#[derive(Default)]
pub struct BinaryImportPen {
    contours: Vec<norad::Contour>,
    current: Vec<norad::ContourPoint>,
}

impl BinaryImportPen {
    fn point(x: f32, y: f32, typ: norad::PointType) -> norad::ContourPoint {
        norad::ContourPoint::new(
            (x as f64).round(),
            (y as f64).round(),
            typ,
            false,
            None,
            None,
        )
    }

    fn finish_contour(&mut self) {
        if self.current.is_empty() {
            return;
        }
        let points = std::mem::take(&mut self.current);
        // Closed contour: the leading Move either duplicates the
        // final on-point (drop it) or becomes an ordinary point.
        let mut points = points;
        if points.len() >= 2 && points[0].typ == norad::PointType::Move {
            let (fx, fy) = (points[0].x, points[0].y);
            let last_matches = points
                .last()
                .is_some_and(|l| l.typ != norad::PointType::OffCurve && l.x == fx && l.y == fy);
            if last_matches {
                points.remove(0);
            } else {
                points[0].typ = norad::PointType::Line;
            }
        }
        if points.len() >= 2 {
            self.contours.push(norad::Contour::new(points, None));
        }
    }
}

impl skrifa::outline::OutlinePen for BinaryImportPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish_contour();
        self.current.push(Self::point(x, y, norad::PointType::Move));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.current.push(Self::point(x, y, norad::PointType::Line));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.current
            .push(Self::point(cx0, cy0, norad::PointType::OffCurve));
        self.current
            .push(Self::point(x, y, norad::PointType::QCurve));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.current
            .push(Self::point(cx0, cy0, norad::PointType::OffCurve));
        self.current
            .push(Self::point(cx1, cy1, norad::PointType::OffCurve));
        self.current
            .push(Self::point(x, y, norad::PointType::Curve));
    }
    fn close(&mut self) {
        self.finish_contour();
    }
}

/// Open a compiled TTF or OTF as an editable in-memory UFO: names,
/// metrics, encodings, and outlines (glyf quadratics kept as UFO
/// qcurves, CFF cubics kept cubic). Kerning and features are not
/// decompiled in this slice.
pub fn import_binary_font(path: &Path) -> Result<norad::Font, String> {
    use skrifa::MetadataProvider as _;
    use skrifa::raw::TableProvider as _;
    let bytes = std::fs::read(path).map_err(|e| format!("{e}"))?;
    let font_ref = skrifa::FontRef::new(&bytes).map_err(|e| format!("{e}"))?;
    let size = skrifa::instance::Size::unscaled();
    let location = skrifa::instance::LocationRef::default();
    let metrics = font_ref.metrics(size, location);
    let glyph_metrics = font_ref.glyph_metrics(size, location);
    let english = |id: skrifa::string::StringId| {
        font_ref
            .localized_strings(id)
            .english_or_first()
            .map(|s| s.chars().collect::<String>())
    };
    let mut font = norad::Font::default();
    let info = &mut font.font_info;
    info.family_name = english(skrifa::string::StringId::FAMILY_NAME);
    info.style_name = english(skrifa::string::StringId::SUBFAMILY_NAME);
    info.units_per_em =
        norad::fontinfo::NonNegativeIntegerOrFloat::try_from(metrics.units_per_em as f64).ok();
    info.ascender = Some(metrics.ascent as f64);
    // skrifa's descent is signed; UFO wants it below zero.
    let descent = metrics.descent as f64;
    info.descender = Some(if descent > 0.0 { -descent } else { descent });
    info.x_height = metrics.x_height.map(|v| v as f64);
    info.cap_height = metrics.cap_height.map(|v| v as f64);
    // gid → codepoints.
    let mut encodings: std::collections::HashMap<u32, Vec<char>> = std::collections::HashMap::new();
    for (codepoint, gid) in font_ref.charmap().mappings() {
        if let Some(c) = char::from_u32(codepoint) {
            encodings.entry(gid.to_u32()).or_default().push(c);
        }
    }
    let names = font_ref.glyph_names();
    let outlines = font_ref.outline_glyphs();
    let count = font_ref
        .maxp()
        .map(|maxp| maxp.num_glyphs() as u32)
        .map_err(|e| format!("{e}"))?;
    let mut seen = HashSet::new();
    for raw_gid in 0..count {
        let gid = skrifa::GlyphId::new(raw_gid);
        let mut name = names
            .get(gid)
            .map(|n| n.to_string())
            .unwrap_or_else(|| format!("glyph{raw_gid:05}"));
        if !seen.insert(name.clone()) {
            name = format!("{name}.gid{raw_gid}");
            seen.insert(name.clone());
        }
        let mut pen = BinaryImportPen::default();
        if let Some(outline) = outlines.get(gid) {
            let _ = outline.draw(
                skrifa::outline::DrawSettings::unhinted(size, location),
                &mut pen,
            );
            pen.finish_contour();
        }
        let Ok(glyph_name) = norad::Name::new(&name) else {
            continue;
        };
        let mut glyph = norad::Glyph::new(glyph_name.as_str());
        glyph.contours = pen.contours;
        glyph.width = glyph_metrics.advance_width(gid).unwrap_or(0.0) as f64;
        if let Some(codepoints) = encodings.get(&raw_gid) {
            glyph.codepoints = norad::Codepoints::new(codepoints.iter().copied());
        }
        font.default_layer_mut().insert_glyph(glyph);
    }
    Ok(font)
}
