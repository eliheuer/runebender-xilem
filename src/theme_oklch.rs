// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The OKLCH theme system, shared by every Runebender editor.
//!
//! Colors are authored once in `themes/runebender.theme.json` (moved
//! here from runebender-web, which now reads the same file) and
//! resolved to sRGB with the exact conversion the web generator uses:
//! Björn Ottosson's Oklab matrices plus chroma-reducing gamut mapping
//! (a color outside sRGB keeps lightness and hue and loses chroma).

use std::collections::HashMap;

use serde::Deserialize;

use crate::theme::ColorRgba;

/// OKLCH → linear sRGB, unclamped (Ottosson reference matrices).
fn oklch_to_linear(l: f64, c: f64, h_deg: f64) -> [f64; 3] {
    let h = h_deg.to_radians();
    let a = c * h.cos();
    let b = c * h.sin();

    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.291485548 * b;

    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    [
        4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3,
        -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3,
        -0.0041960863 * l3 - 0.7034186147 * m3 + 1.707614701 * s3,
    ]
}

fn srgb_to_linear(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(v: f64) -> f64 {
    if v <= 0.0031308 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

fn in_gamut(rgb: [f64; 3]) -> bool {
    rgb.iter().all(|v| *v >= -1e-6 && *v <= 1.0 + 1e-6)
}

/// OKLCH → sRGB with the web generator's gamut mapping.
pub fn oklch_to_rgb(l: f64, c: f64, h: f64) -> ColorRgba {
    let mut chroma = c;
    if !in_gamut(oklch_to_linear(l, c, h)) {
        let (mut low, mut high) = (0.0, c);
        for _ in 0..24 {
            chroma = (low + high) / 2.0;
            if in_gamut(oklch_to_linear(l, chroma, h)) {
                low = chroma;
            } else {
                high = chroma;
            }
        }
        chroma = low;
    }
    let rgb = oklch_to_linear(l, chroma, h);
    let to_byte =
        |v: f64| (linear_to_srgb(v).clamp(0.0, 1.0) * 255.0).round() as u8;
    ColorRgba::rgb(to_byte(rgb[0]), to_byte(rgb[1]), to_byte(rgb[2]))
}

// ---- token file structures ----

#[derive(Deserialize)]
struct HueDef {
    hue: f64,
    lightness: f64,
    chroma: f64,
}

#[derive(Deserialize)]
struct StepDef {
    lightness: f64,
    chroma: f64,
}

#[derive(Deserialize)]
struct NeutralDef {
    hue: f64,
    chroma: f64,
}

#[derive(Deserialize)]
struct ThemeDef {
    surfaces: HashMap<String, String>,
    text: HashMap<String, String>,
    roles: HashMap<String, String>,
    #[serde(rename = "markStep")]
    mark_step: Option<String>,
    #[serde(default)]
    geometry: Option<GeometryDef>,
}

#[derive(Deserialize)]
struct MarkColorDef {
    name: String,
}

/// Shape tokens. Every field is optional in the file: a theme names
/// only what it changes, and the rest comes from `geometry.default`.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default)]
struct GeometryDef {
    #[serde(rename = "radiusSmall")]
    radius_small: Option<f32>,
    #[serde(rename = "radiusMedium")]
    radius_medium: Option<f32>,
    stroke: Option<f32>,
}

#[derive(Deserialize)]
struct TokenFile {
    hues: HashMap<String, HueDef>,
    steps: HashMap<String, StepDef>,
    neutral: NeutralDef,
    themes: HashMap<String, ThemeDef>,
    #[serde(default)]
    geometry: HashMap<String, GeometryDef>,
    #[serde(rename = "markColors", default)]
    mark_colors: Vec<MarkColorDef>,
    /// Frozen `public.markColor` strings, keyed by label. File
    /// contents, so they do not follow display tuning.
    #[serde(rename = "ufoMarkColors", default)]
    ufo_mark_colors: HashMap<String, String>,
}

/// Shape tokens, resolved: a theme's own value where it names one,
/// otherwise the file's default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    /// Corner radius for small chrome: tiles, tabs, swatches.
    pub radius_small: f32,
    /// Corner radius for larger surfaces: panels, popovers, buttons.
    pub radius_medium: f32,
    /// Border width for every themed rule.
    pub stroke: f32,
}

impl Default for Geometry {
    fn default() -> Self {
        Self { radius_small: 3.0, radius_medium: 6.0, stroke: 1.0 }
    }
}

/// One resolved theme: every surface, text, and role token as sRGB.
pub struct Theme {
    pub surfaces: HashMap<String, ColorRgba>,
    pub text: HashMap<String, ColorRgba>,
    pub roles: HashMap<String, ColorRgba>,
    /// Glyph mark colours in palette order, drawn at this theme's
    /// `markStep` (matches the web's `--rb-mark-{name}` variables).
    pub marks: Vec<(String, ColorRgba)>,
    /// Corner radii and stroke width for this theme.
    pub geometry: Geometry,
}

impl Theme {
    pub fn surface(&self, name: &str) -> ColorRgba {
        self.surfaces.get(name).copied().unwrap_or(FALLBACK)
    }
    pub fn text(&self, name: &str) -> ColorRgba {
        self.text.get(name).copied().unwrap_or(FALLBACK)
    }
    pub fn role(&self, name: &str) -> ColorRgba {
        self.roles.get(name).copied().unwrap_or(FALLBACK)
    }
    /// The display colour for a mark label, if the palette names it.
    pub fn mark(&self, label: &str) -> Option<ColorRgba> {
        self.marks
            .iter()
            .find(|(name, _)| name == label)
            .map(|(_, color)| *color)
    }
}

/// The `public.markColor` value written for a label, as "r,g,b,a" 0–1
/// floats. Frozen in the token file rather than derived, so neither a
/// theme switch nor a change to the palette rewrites every mark in a
/// font — matches the web's `ufoRgba` byte for byte.
pub fn ufo_rgba_for_label(label: &str) -> Option<String> {
    let file: TokenFile =
        serde_json::from_str(include_str!("../themes/runebender.theme.json")).ok()?;
    if !file.mark_colors.iter().any(|m| m.name == label) {
        return None;
    }
    if let Some(frozen) = file.ufo_mark_colors.get(label) {
        return Some(frozen.clone());
    }
    let color = resolve_token(&file, &format!("{label}.base"))?;
    let fmt = |byte: u8| {
        let v = (byte as f64 / 255.0 * 100.0).round() / 100.0;
        let s = format!("{v}");
        if s == "0" { "0".to_string() } else { s }
    };
    Some(format!("{},{},{},1", fmt(color.r), fmt(color.g), fmt(color.b)))
}

const FALLBACK: ColorRgba = ColorRgba::rgb(0xff, 0x00, 0xff);

fn resolve_token(file: &TokenFile, token: &str) -> Option<ColorRgba> {
    let (family, step) = token.split_once('.')?;
    if family == "neutral" {
        let percent: f64 = step.parse().ok()?;
        return Some(oklch_to_rgb(
            percent / 100.0,
            file.neutral.chroma,
            file.neutral.hue,
        ));
    }
    let hue = file.hues.get(family)?;
    let offsets = file.steps.get(step)?;
    // Lightness stops at 0.93: past that sRGB has almost no chroma
    // left at any hue (matches the web generator's stepColor).
    Some(oklch_to_rgb(
        (hue.lightness + offsets.lightness).clamp(0.08, 0.93),
        (hue.chroma + offsets.chroma).max(0.0),
        hue.hue,
    ))
}

/// Load and resolve one theme from the shared token file.
pub fn load_theme(theme_id: &str) -> Option<Theme> {
    let file: TokenFile =
        serde_json::from_str(include_str!("../themes/runebender.theme.json")).ok()?;
    let def = file.themes.get(theme_id)?;
    let resolve_map = |map: &HashMap<String, String>| {
        map.iter()
            .filter_map(|(k, v)| resolve_token(&file, v).map(|c| (k.clone(), c)))
            .collect()
    };
    let mark_step = def.mark_step.as_deref().unwrap_or("base");
    let marks = file
        .mark_colors
        .iter()
        .filter_map(|m| {
            resolve_token(&file, &format!("{}.{mark_step}", m.name))
                .map(|c| (m.name.clone(), c))
        })
        .collect();
    let base = file.geometry.get("default").copied().unwrap_or_default();
    let own = def.geometry.unwrap_or_default();
    let fallback = Geometry::default();
    let geometry = Geometry {
        radius_small: own
            .radius_small
            .or(base.radius_small)
            .unwrap_or(fallback.radius_small),
        radius_medium: own
            .radius_medium
            .or(base.radius_medium)
            .unwrap_or(fallback.radius_medium),
        stroke: own.stroke.or(base.stroke).unwrap_or(fallback.stroke),
    };
    Some(Theme {
        geometry,
        surfaces: resolve_map(&def.surfaces),
        text: resolve_map(&def.text),
        roles: resolve_map(&def.roles),
        marks,
    })
}

// ---- glyph mark labels ----

/// Set or clear a glyph's mark: writes `public.markColor` (the fixed
/// palette colour other editors need) and `com.runebender.markLabel`
/// (what the mark means) together, or removes both.
pub fn set_glyph_mark(glyph: &mut norad::Glyph, label: Option<&str>) {
    match label.and_then(ufo_rgba_for_label) {
        Some(rgba) => {
            glyph
                .lib
                .insert("public.markColor".into(), plist::Value::String(rgba));
            glyph.lib.insert(
                MARK_LABEL_KEY.into(),
                plist::Value::String(label.unwrap().into()),
            );
        }
        None => {
            glyph.lib.remove("public.markColor");
            glyph.lib.remove(MARK_LABEL_KEY);
        }
    }
}

/// The mark-label lib key written beside `public.markColor` (see
/// runebender-web's markColors.ts for the rationale: the colour is
/// what other editors need, the label is what the mark means).
pub const MARK_LABEL_KEY: &str = "com.runebender.markLabel";

/// OKLCH hue angle of an sRGB colour, or `None` for near-grey.
fn hue_of(r: f64, g: f64, b: f64) -> Option<f64> {
    let (r, g, b) = (srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b));
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    let a = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
    let bb = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
    if a.hypot(bb) < 0.03 {
        return None;
    }
    let hue = bb.atan2(a).to_degrees();
    Some(if hue < 0.0 { hue + 360.0 } else { hue })
}

/// The mark label a glyph carries: `com.runebender.markLabel` when
/// present, otherwise its `public.markColor` snapped to the nearest
/// palette hue (display only — never written back).
pub fn mark_label_for_glyph(glyph: &norad::Glyph, theme: &Theme) -> Option<String> {
    if let Some(plist::Value::String(label)) = glyph.lib.get(MARK_LABEL_KEY) {
        if theme.mark(label).is_some() {
            return Some(label.clone());
        }
    }
    let plist::Value::String(rgba) = glyph.lib.get("public.markColor")? else {
        return None;
    };
    label_for_rgba(rgba, theme)
}

/// Snap a UFO "r,g,b,a" colour (0–1 floats) to the nearest palette
/// label by hue. `None` for greys and colours far from every hue.
pub fn label_for_rgba(rgba: &str, theme: &Theme) -> Option<String> {
    let parts: Vec<f64> = rgba.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    if parts.len() < 3 {
        return None;
    }
    let hue = hue_of(parts[0], parts[1], parts[2])?;
    let mut best: Option<&str> = None;
    let mut best_distance = f64::INFINITY;
    for (name, color) in &theme.marks {
        let palette_hue = hue_of(
            color.r as f64 / 255.0,
            color.g as f64 / 255.0,
            color.b as f64 / 255.0,
        )
        .unwrap_or(0.0);
        let raw = (hue - palette_hue).abs();
        let distance = raw.min(360.0 - raw);
        if distance < best_distance {
            best_distance = distance;
            best = Some(name);
        }
    }
    // Neighbouring palette hues are ~40° apart; anything further than
    // half that from every one has no name in this palette.
    (best_distance <= 30.0).then(|| best.unwrap().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(c: ColorRgba) -> String {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    }

    /// The resolved dark palette. These are display values: the UFO
    /// mark strings are frozen separately (see `ufo_rgba_matches_web`),
    /// so tuning the palette here never touches a font's contents.
    /// runebender-web generates its own tokens from its copy of the
    /// token file; copy this one over when the two should match.
    #[test]
    fn resolves_the_dark_palette() {
        let dark = load_theme("dark").expect("dark theme");
        assert_eq!(hex(dark.surface("app")), "#0b0b0b");
        assert_eq!(hex(dark.surface("panel")), "#121212");
        assert_eq!(hex(dark.surface("outline")), "#404040");
        assert_eq!(hex(dark.text("primary")), "#8f8f8f");
        assert_eq!(hex(dark.role("accent")), "#57b174");
        assert_eq!(hex(dark.role("warning")), "#dec352");
        assert_eq!(hex(dark.role("selection")), "#e2763f");
        assert_eq!(hex(dark.role("pointSmooth")), "#4b91d1");
        assert_eq!(hex(dark.role("pointOffcurve")), "#876fd4");
        assert_eq!(hex(dark.role("pointSelected")), "#fde895");
        assert_eq!(hex(dark.role("pathStroke")), "#b1b1b1");
        assert_eq!(hex(dark.role("gridSelected")), "#c1c1c1");
        assert_eq!(hex(dark.role("continuityG2")), "#4db2a7");
    }

    /// The swatches drawn in the Colors panel.
    #[test]
    fn resolves_mark_swatches() {
        let dark = load_theme("dark").expect("dark theme");
        assert_eq!(hex(dark.mark("red").unwrap()), "#d55249");
        assert_eq!(hex(dark.mark("orange").unwrap()), "#e2763f");
        assert_eq!(hex(dark.mark("yellow").unwrap()), "#dec352");
        assert_eq!(hex(dark.mark("green").unwrap()), "#57b174");
        assert_eq!(dark.marks.len(), 7);
        assert!(dark.mark("chartreuse").is_none());
    }

    /// Unlabelled `public.markColor` values snap to the nearest
    /// palette hue; greys and far-off hues get no label.
    #[test]
    fn snaps_rgba_to_palette_label() {
        let dark = load_theme("dark").expect("dark theme");
        // The exact UFO colours the web writes round-trip to their labels.
        assert_eq!(label_for_rgba("0.88,0.3,0.27,1", &dark).as_deref(), Some("red"));
        assert_eq!(label_for_rgba("0.27,0.44,1,1", &dark).as_deref(), Some("blue"));
        assert_eq!(label_for_rgba("0.09,0.72,0.44,1", &dark).as_deref(), Some("green"));
        assert_eq!(label_for_rgba("0.5,0.5,0.5,1", &dark), None);
        assert_eq!(label_for_rgba("garbage", &dark), None);
    }

    /// UFO colours written for labels match the web's fixed ufoRgba
    /// strings exactly (files must not churn between editors).
    #[test]
    fn ufo_rgba_matches_web() {
        assert_eq!(ufo_rgba_for_label("red").as_deref(), Some("0.88,0.3,0.27,1"));
        assert_eq!(ufo_rgba_for_label("orange").as_deref(), Some("0.93,0.45,0.2,1"));
        assert_eq!(ufo_rgba_for_label("yellow").as_deref(), Some("0.91,0.79,0.27,1"));
        assert_eq!(ufo_rgba_for_label("green").as_deref(), Some("0.31,0.72,0.45,1"));
        assert_eq!(ufo_rgba_for_label("mauve"), None);
    }

    #[test]
    fn set_mark_writes_both_keys_and_clears_both() {
        let dark = load_theme("dark").expect("dark theme");
        let mut glyph = norad::Glyph::new("A");
        set_glyph_mark(&mut glyph, Some("green"));
        assert_eq!(
            glyph.lib.get("public.markColor"),
            Some(&plist::Value::String("0.31,0.72,0.45,1".into()))
        );
        assert_eq!(mark_label_for_glyph(&glyph, &dark).as_deref(), Some("green"));
        set_glyph_mark(&mut glyph, None);
        assert!(glyph.lib.get("public.markColor").is_none());
        assert!(glyph.lib.get(MARK_LABEL_KEY).is_none());
        // An unknown label clears rather than writing garbage.
        set_glyph_mark(&mut glyph, Some("chartreuse"));
        assert!(glyph.lib.get(MARK_LABEL_KEY).is_none());
    }

    #[test]
    fn reads_glyph_mark_label_then_color() {
        let dark = load_theme("dark").expect("dark theme");
        let mut glyph = norad::Glyph::new("A");
        assert_eq!(mark_label_for_glyph(&glyph, &dark), None);
        glyph.lib.insert(
            "public.markColor".into(),
            plist::Value::String("0.93,0.45,0.2,1".into()),
        );
        assert_eq!(mark_label_for_glyph(&glyph, &dark).as_deref(), Some("orange"));
        glyph.lib.insert(
            MARK_LABEL_KEY.into(),
            plist::Value::String("blue".into()),
        );
        assert_eq!(mark_label_for_glyph(&glyph, &dark).as_deref(), Some("blue"));
    }
}

// ---- toolbar icons ----

/// One toolbar icon: outline geometry from the shared icon UFO
/// (assets/runebender-icons.ufo, via the web generator's JSON).
pub struct ToolbarIcon {
    /// Tight bounds of the outline in Y-down SVG space.
    pub view_box: kurbo::Rect,
    pub path: kurbo::BezPath,
    /// Stroke instead of fill (open path icons).
    pub stroke: bool,
}

/// The shared toolbar icon set, keyed by UFO glyph name ("select",
/// "pen", "knife", "flip-h", …). Parsed once.
pub fn toolbar_icons() -> &'static HashMap<String, ToolbarIcon> {
    use std::sync::OnceLock;
    static ICONS: OnceLock<HashMap<String, ToolbarIcon>> = OnceLock::new();
    ICONS.get_or_init(|| {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(rename = "viewBox")]
            view_box: String,
            d: String,
            mode: Option<String>,
        }
        let raw: HashMap<String, Raw> =
            serde_json::from_str(include_str!("../themes/toolbar-icons.json"))
                .unwrap_or_default();
        raw.into_iter()
            .filter_map(|(name, icon)| {
                let numbers: Vec<f64> = icon
                    .view_box
                    .split_whitespace()
                    .filter_map(|v| v.parse().ok())
                    .collect();
                let [x, y, w, h] = numbers.as_slice() else {
                    return None;
                };
                let path = kurbo::BezPath::from_svg(&icon.d).ok()?;
                Some((
                    name,
                    ToolbarIcon {
                        view_box: kurbo::Rect::new(*x, *y, x + w, y + h),
                        path,
                        stroke: icon.mode.as_deref() == Some("stroke"),
                    },
                ))
            })
            .collect()
    })
}

#[cfg(test)]
mod icon_tests {
    use super::*;

    #[test]
    fn parses_all_toolbar_icons() {
        let icons = toolbar_icons();
        assert_eq!(icons.len(), 26);
        for name in ["select", "pen", "knife", "measure", "shapes", "flip-h", "rot-cw", "union", "save"] {
            let icon = icons.get(name).unwrap_or_else(|| panic!("missing {name}"));
            assert!(!icon.path.elements().is_empty());
            assert!(icon.view_box.width() > 0.0 && icon.view_box.height() > 0.0);
        }
    }
}

#[cfg(test)]
mod geometry_tests {
    use super::*;

    #[test]
    fn every_theme_resolves_geometry() {
        for id in ["dark", "midnight", "light", "gray", "paper"] {
            let theme = load_theme(id).expect("theme in the token file");
            assert!(theme.geometry.stroke > 0.0, "{id} stroke");
            assert!(theme.geometry.radius_small >= 0.0, "{id} radius");
        }
    }

    #[test]
    fn a_theme_without_geometry_takes_the_default() {
        let dark = load_theme("dark").expect("dark");
        assert_eq!(dark.geometry, Geometry::default());
    }

    #[test]
    fn paper_overrides_the_default() {
        let paper = load_theme("paper").expect("paper");
        assert_eq!(paper.geometry.stroke, 2.0);
        assert_eq!(paper.geometry.radius_small, 0.0);
        assert_eq!(paper.geometry.radius_medium, 0.0);
        assert_ne!(paper.geometry, Geometry::default());
    }

    #[test]
    fn paper_draws_its_rules_in_the_text_colour() {
        // The website's borders are the text colour, not a lighter
        // tint. That is the whole look; if these drift apart the theme
        // stops resembling it.
        let paper = load_theme("paper").expect("paper");
        assert_eq!(paper.surface("outline"), paper.text("primary"));
        assert_eq!(paper.surface("divider"), paper.text("primary"));
    }
}
