// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The theme system, shared by every Runebender editor.
//!
//! Built-in colors are authored in `assets/themes/default/*.theme.toml` and
//! resolved to sRGB with the original web generator's conversion:
//! Björn Ottosson's Oklab matrices plus chroma-reducing gamut
//! mapping, where a color outside sRGB keeps lightness and hue and
//! loses chroma. See `https://runebender.org/docs/themes.html` for the portable theme format.

use std::collections::HashMap;

use crate::font::model::glyph_metadata::MarkColor;
#[cfg(test)]
use crate::font::model::glyph_metadata::{MARK_COLOR_KEY, MARK_LABEL_KEY};
use crate::ui::color::ColorRgba;

use serde::Deserialize;

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
    #[expect(
        clippy::cast_possible_truncation,
        reason = "clamped to 0..=255 before the cast"
    )]
    let to_byte = |v: f64| (linear_to_srgb(v).clamp(0.0, 1.0) * 255.0).round() as u8;
    ColorRgba::rgb(to_byte(rgb[0]), to_byte(rgb[1]), to_byte(rgb[2]))
}

// ---- token file structures ----

#[derive(Deserialize)]
#[serde(try_from = "OklchValue")]
struct HueDef {
    hue: f64,
    lightness: f64,
    chroma: f64,
}

#[derive(Deserialize)]
#[serde(try_from = "OklchValue")]
struct StepDef {
    lightness: f64,
    chroma: f64,
}

/// Read uniform OKLCH strings while retaining older component tables.
#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum OklchValue {
    Color(String),
    Components {
        lightness: f64,
        chroma: f64,
        hue: Option<f64>,
    },
}

impl OklchValue {
    fn components(self, default_hue: Option<f64>) -> Result<[f64; 3], String> {
        match self {
            Self::Color(value) => parse_oklch_components(&value)
                .ok_or_else(|| format!("expected oklch(lightness chroma hue), got '{value}'")),
            Self::Components {
                lightness,
                chroma,
                hue,
            } => Ok([
                lightness,
                chroma,
                hue.or(default_hue).ok_or("missing OKLCH hue")?,
            ]),
        }
    }
}

impl TryFrom<OklchValue> for HueDef {
    type Error = String;

    fn try_from(value: OklchValue) -> Result<Self, Self::Error> {
        let [lightness, chroma, hue] = value.components(None)?;
        Ok(Self {
            hue,
            lightness,
            chroma,
        })
    }
}

impl TryFrom<OklchValue> for StepDef {
    type Error = String;

    fn try_from(value: OklchValue) -> Result<Self, Self::Error> {
        let [lightness, chroma, hue] = value.components(Some(0.0))?;
        if hue != 0.0 {
            return Err("rainbow steps must use a zero hue offset".into());
        }
        Ok(Self { lightness, chroma })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RainbowDef {
    hues: HashMap<String, HueDef>,
    steps: HashMap<String, StepDef>,
    marks: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    #[serde(rename = "$comment", default)]
    _comment: Option<serde_json::Value>,
    #[serde(rename = "formatVersion")]
    format_version: u32,
    id: String,
    name: String,
    #[serde(rename = "baseUi")]
    base_ui: HashMap<String, String>,
    #[serde(alias = "glyphGrid")]
    rainbow: RainbowDef,
    surfaces: HashMap<String, String>,
    text: HashMap<String, String>,
    roles: HashMap<String, String>,
    #[serde(rename = "markStep")]
    mark_step: Option<String>,
    #[serde(rename = "markStyle")]
    mark_style: Option<String>,
    #[serde(rename = "markOutline")]
    mark_outline: Option<String>,
    #[serde(rename = "markInk")]
    mark_ink: Option<String>,
    #[serde(rename = "pointStyle")]
    point_style: Option<String>,
    #[serde(rename = "pointOutline")]
    point_outline: Option<String>,
    #[serde(rename = "pointHalo")]
    point_halo: Option<bool>,
    #[serde(default)]
    geometry: Option<GeometryDef>,
}

/// Shape tokens. Every field is optional in the file: a theme names
/// only what it changes, and the rest comes from `Geometry::default`.
/// Panel corners follow `radius` when no separate panel radius is supplied.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct GeometryDef {
    radius: Option<f32>,
    #[serde(rename = "radiusPanel")]
    radius_panel: Option<f32>,
    #[serde(rename = "shadowPanel")]
    shadow_panel: Option<bool>,
    #[serde(rename = "radiusControl")]
    radius_control: Option<f32>,
    stroke: Option<f32>,
    #[serde(rename = "strokeEmphasis")]
    stroke_emphasis: Option<f32>,
}

/// How a theme draws a point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointStyle {
    /// A dark interior with the kind's hue as a ring: the web
    /// editor's recipe, for grounds far from mid lightness.
    Ring,
    /// The kind's hue as the fill, keyed with `pointOutline`: the
    /// treatment the mark cells use, for a mid-grey ground.
    Fill,
}

/// How a theme draws a glyph mark.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkStyle {
    /// Tint the cell's rule and label, leave the fill alone. Works
    /// where the ground is far from mid lightness.
    Border,
    /// Fill the cell with the hue and key it with a rule. The only
    /// treatment that survives a mid-grey ground.
    Fill,
}

/// Shape tokens, resolved: a theme's own value where it names one,
/// otherwise the file's default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    /// The default corner, on small chrome.
    pub radius: f32,
    /// Large workspace panels; defaults to the general corner radius when omitted.
    pub radius_panel: f32,
    /// Whether the floating workspace panels cast a drop shadow; defaults to false.
    pub shadow_panel: bool,
    /// Pressable tiles: toolbar tiles, sidebar tabs, toggles.
    pub radius_control: f32,
    /// The ordinary rule, on panels and chrome.
    pub stroke: f32,
    /// Rings that mark a thing selected or grabbable. Its own token
    /// rather than `stroke` doubled: doubling works from a 1px base
    /// and breaks from a 2px one.
    pub stroke_emphasis: f32,
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            radius: 3.0,
            radius_panel: 3.0,
            shadow_panel: false,
            radius_control: 6.0,
            stroke: 1.0,
            stroke_emphasis: 2.0,
        }
    }
}

#[derive(Debug)]
/// One resolved theme: every surface, text, and role token as sRGB.
pub struct Theme {
    /// Stable theme identifier used to select this theme.
    pub id: String,
    /// Name shown to people.
    pub name: String,
    /// Surface colors by token name, such as backgrounds and panels.
    pub surfaces: HashMap<String, ColorRgba>,
    /// Text colors by token name.
    pub text: HashMap<String, ColorRgba>,
    /// Role colors by token name: points, handles, selection, and other editor roles.
    pub roles: HashMap<String, ColorRgba>,
    /// Glyph mark colours in palette order, drawn at this theme's
    /// `markStep`. Matches the web's `--rb-mark-{name}` variables.
    pub marks: Vec<(String, ColorRgba)>,
    /// Corner radii and stroke width for this theme.
    pub geometry: Geometry,
    /// How marks are drawn.
    pub mark_style: MarkStyle,
    /// Keyline around a filled mark.
    pub mark_outline: Option<ColorRgba>,
    /// Label colour drawn on top of a filled mark.
    pub mark_ink: Option<ColorRgba>,
    /// How points are drawn.
    pub point_style: PointStyle,
    /// Keyline around a filled point.
    pub point_outline: Option<ColorRgba>,
    /// Whether points and anchors get a halo of the ground under
    /// them. A ring on a dark ground needs one to keep its edge over
    /// the outline; a keylined fill on a mid grey does not.
    pub point_halo: bool,
}

impl Theme {
    /// Looks up a surface color by name, failing on an unknown role.
    pub fn surface(&self, name: &str) -> ColorRgba {
        *self
            .surfaces
            .get(name)
            .unwrap_or_else(|| panic!("theme '{}' has no surface '{name}'", self.id))
    }
    /// Looks up a text color by name, failing on an unknown role.
    pub fn text(&self, name: &str) -> ColorRgba {
        *self
            .text
            .get(name)
            .unwrap_or_else(|| panic!("theme '{}' has no text color '{name}'", self.id))
    }
    /// Looks up a role color by name, failing on an unknown role.
    pub fn role(&self, name: &str) -> ColorRgba {
        *self
            .roles
            .get(name)
            .unwrap_or_else(|| panic!("theme '{}' has no role '{name}'", self.id))
    }
    /// The display colour for a mark label, if the palette names it.
    pub fn mark(&self, label: &str) -> Option<ColorRgba> {
        self.marks
            .iter()
            .find(|(name, _)| name == label)
            .map(|(_, color)| *color)
    }
}

/// The simple canonical `public.markColor` value written for a label.
///
/// These colors are saved for compatibility with other UFO editors.
/// The active theme supplies the colors Runebender displays.
pub fn ufo_rgba_for_label(label: &str) -> Option<String> {
    MARK_UFO_COLORS
        .iter()
        .find_map(|(name, color)| (*name == label).then(|| (*color).to_owned()))
}

// UFO mark values use normalized RGBA channels. Keep them simple and fixed;
// display colors belong to each theme's Rainbow palette.
const MARK_UFO_COLORS: &[(&str, &str)] = &[
    ("red", "1,0,0,1"),
    ("orange", "1,0.5,0,1"),
    ("yellow", "1,1,0,1"),
    ("green", "0,1,0,1"),
    ("blue", "0,0,1,1"),
    ("purple", "0.5,0,1,1"),
    ("pink", "1,0,0.5,1"),
];

// These are the names consumed by the application, not values in the base
// palette. Keep them here so a hand-edited theme fails at its source instead
// of painting an unexpected fallback color in one view.
const REQUIRED_SURFACES: &[&str] = &[
    "app",
    "panel",
    "titlebar",
    "control",
    "button",
    "buttonHover",
    "field",
    "outline",
    "fieldOutline",
    "divider",
    "canvas",
    "tabRail",
    "inactiveTab",
    "header",
    "gridBackground",
    "cellShadow",
    "floatingPaneHeader",
    "sliderTrack",
];
const REQUIRED_TEXT: &[&str] = &[
    "primary",
    "secondary",
    "muted",
    "subdued",
    "overlay",
    "glyph",
    "headerInk",
];
const REQUIRED_ROLES: &[&str] = &[
    "gridSelected",
    "cellSelectedFill",
    "cellSelectedInk",
    "controlSelected",
    "controlSelectedInk",
    "designGridFine",
    "designGridCoarse",
    "accent",
    "warning",
    "danger",
    "selection",
    "component",
    "componentSelected",
    "pointSmooth",
    "pointCorner",
    "pointOffcurve",
    "pointHyper",
    "pointSelected",
    "pointInner",
    "startNode",
    "textCursor",
    "kernActive",
    "kernPrevious",
    "pathStroke",
    "previewFill",
    "background",
    "reference",
    "halo",
    "metricQuiet",
    "outlineFill",
    "metricsLine",
    "readonlyPoint",
    "continuityG2",
    "continuityG1",
    "continuityLine",
    "continuityKink",
    "popcount1",
    "popcount2",
    "popcount3",
    "popcount4",
];
const MARK_LABELS: [&str; 7] = ["red", "orange", "yellow", "green", "blue", "purple", "pink"];

fn parse_color(value: &str) -> Option<ColorRgba> {
    if let Some(hex) = value.strip_prefix('#') {
        let byte = |start| u8::from_str_radix(&hex[start..start + 2], 16).ok();
        return match hex.len() {
            6 => Some(ColorRgba::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Some(ColorRgba::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        };
    }
    let [lightness, chroma, hue] = parse_oklch_components(value)?;
    if !(0.0..=1.0).contains(&lightness) || !chroma.is_finite() || chroma < 0.0 || !hue.is_finite()
    {
        return None;
    }
    Some(oklch_to_rgb(lightness, chroma, hue))
}

fn parse_oklch_components(value: &str) -> Option<[f64; 3]> {
    let contents = value.strip_prefix("oklch(")?.strip_suffix(')')?;
    let components: Vec<f64> = contents
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    components.try_into().ok()
}

fn resolve_token(file: &ThemeFile, token: &str) -> Option<ColorRgba> {
    if let Some(color) = parse_color(token) {
        return Some(color);
    }
    let mut parts = token.split('.');
    match (parts.next()?, parts.next(), parts.next(), parts.next()) {
        ("baseUi", Some(step), None, None) => parse_color(file.base_ui.get(step)?),
        ("rainbow" | "glyphGrid", Some(name), Some(step), None) => {
            let hue = file.rainbow.hues.get(name)?;
            let offsets = file.rainbow.steps.get(step)?;
            // Preserve the original palette's chroma-reducing gamut recipe.
            Some(oklch_to_rgb(
                (hue.lightness + offsets.lightness).clamp(0.08, 0.93),
                (hue.chroma + offsets.chroma).max(0.0),
                hue.hue,
            ))
        }
        _ => None,
    }
}

fn resolve_map(
    file: &ThemeFile,
    theme_id: &str,
    group: &str,
    map: &HashMap<String, String>,
    required: &[&str],
) -> Result<HashMap<String, ColorRgba>, String> {
    for name in required {
        if !map.contains_key(*name) {
            return Err(format!("theme '{theme_id}' is missing {group}.{name}"));
        }
    }
    for name in map.keys() {
        if !required.contains(&name.as_str()) {
            return Err(format!("theme '{theme_id}' has unknown {group}.{name}"));
        }
    }
    map.iter()
        .map(|(name, token)| {
            resolve_token(file, token)
                .map(|color| (name.clone(), color))
                .ok_or_else(|| {
                    format!("theme '{theme_id}' {group}.{name} has unknown color '{token}'")
                })
        })
        .collect()
}

fn resolve_optional(
    file: &ThemeFile,
    theme_id: &str,
    name: &str,
    token: Option<&str>,
) -> Result<Option<ColorRgba>, String> {
    token
        .map(|value| {
            resolve_token(file, value)
                .ok_or_else(|| format!("theme '{theme_id}' {name} has unknown color '{value}'"))
        })
        .transpose()
}

/// Parse and resolve a TOML theme or an existing JSON theme.
/// Reports invalid references by name.
pub fn parse_theme(source: &str) -> Result<Theme, String> {
    let file: ThemeFile = if source.trim_start().starts_with('{') {
        serde_json::from_str(source).map_err(|error| format!("invalid theme JSON: {error}"))?
    } else {
        toml::from_str(source).map_err(|error| format!("invalid theme TOML: {error}"))?
    };
    let theme_id = file.id.as_str();
    validate_theme_id(theme_id)?;
    if file.format_version != 1 {
        return Err(format!(
            "theme '{}' uses unsupported formatVersion {}",
            file.id, file.format_version
        ));
    }
    if file.id != theme_id {
        return Err(format!("expected theme '{theme_id}', found '{}'", file.id));
    }
    if file.name.trim().is_empty()
        || file.name.len() > 80
        || file.name.chars().any(char::is_control)
    {
        return Err(format!(
            "theme '{theme_id}' name must be 1–80 visible characters"
        ));
    }
    for (step, value) in &file.base_ui {
        if parse_color(value).is_none() {
            return Err(format!(
                "theme '{theme_id}' baseUi.{step} has invalid color '{value}'"
            ));
        }
    }
    for (name, hue) in &file.rainbow.hues {
        if !(0.0..=1.0).contains(&hue.lightness)
            || !hue.chroma.is_finite()
            || hue.chroma < 0.0
            || !hue.hue.is_finite()
        {
            return Err(format!(
                "theme '{theme_id}' rainbow.hues.{name} has invalid OKLCH"
            ));
        }
    }
    for (name, step) in &file.rainbow.steps {
        if !step.lightness.is_finite() || !step.chroma.is_finite() {
            return Err(format!(
                "theme '{theme_id}' rainbow.steps.{name} has invalid offsets"
            ));
        }
    }
    let mut surface_tokens = file.surfaces.clone();
    let panel_shadow = surface_tokens.remove("panelShadow");
    let slider_thumb = surface_tokens.remove("sliderThumb");
    let slider_thumb_active = surface_tokens.remove("sliderThumbActive");
    let mut surfaces = resolve_map(
        &file,
        theme_id,
        "surfaces",
        &surface_tokens,
        REQUIRED_SURFACES,
    )?;
    let panel_shadow = resolve_optional(
        &file,
        theme_id,
        "surfaces.panelShadow",
        panel_shadow.as_deref(),
    )?
    .unwrap_or_else(|| surfaces["outline"]);
    surfaces.insert("panelShadow".into(), panel_shadow);
    for (name, token, fallback) in [
        ("sliderThumb", slider_thumb, "button"),
        ("sliderThumbActive", slider_thumb_active, "buttonHover"),
    ] {
        let value = resolve_optional(&file, theme_id, name, token.as_deref())?
            .unwrap_or(surfaces[fallback]);
        surfaces.insert(name.into(), value);
    }
    let mut text_tokens = file.text.clone();
    let app_ink = text_tokens.remove("appInk");
    let header_muted_ink = text_tokens.remove("headerMutedInk");
    let mut text = resolve_map(&file, theme_id, "text", &text_tokens, REQUIRED_TEXT)?;
    let app_ink = resolve_optional(&file, theme_id, "text.appInk", app_ink.as_deref())?
        .unwrap_or_else(|| text["primary"]);
    text.insert("appInk".into(), app_ink);
    if let Some(ink) = resolve_optional(
        &file,
        theme_id,
        "text.headerMutedInk",
        header_muted_ink.as_deref(),
    )? {
        text.insert("headerMutedInk".into(), ink);
    }
    let roles = resolve_map(&file, theme_id, "roles", &file.roles, REQUIRED_ROLES)?;
    let mark_step = file.mark_step.as_deref().unwrap_or("base");
    let marks = file
        .rainbow
        .marks
        .iter()
        .map(|mark| {
            let token = format!("rainbow.{mark}.{mark_step}");
            resolve_token(&file, &token)
                .map(|color| (mark.clone(), color))
                .ok_or_else(|| {
                    format!("theme '{theme_id}' rainbow.marks.{mark} has unknown color '{token}'")
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    for label in MARK_LABELS {
        if marks.iter().filter(|(name, _)| name == label).count() != 1 {
            return Err(format!(
                "theme '{theme_id}' must name rainbow mark '{label}' once"
            ));
        }
    }
    if marks.len() != MARK_LABELS.len() {
        return Err(format!(
            "theme '{theme_id}' has an unknown or duplicate rainbow mark"
        ));
    }
    let own = file.geometry.unwrap_or_default();
    let fallback = Geometry::default();
    let geometry = Geometry {
        radius: own.radius.unwrap_or(fallback.radius),
        radius_panel: own
            .radius_panel
            .or(own.radius)
            .unwrap_or(fallback.radius_panel),
        shadow_panel: own.shadow_panel.unwrap_or(fallback.shadow_panel),
        radius_control: own.radius_control.unwrap_or(fallback.radius_control),
        stroke: own.stroke.unwrap_or(fallback.stroke),
        stroke_emphasis: own.stroke_emphasis.unwrap_or(fallback.stroke_emphasis),
    };
    for (name, value) in [
        ("radius", geometry.radius),
        ("radiusPanel", geometry.radius_panel),
        ("radiusControl", geometry.radius_control),
        ("stroke", geometry.stroke),
        ("strokeEmphasis", geometry.stroke_emphasis),
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err(format!(
                "theme '{theme_id}' geometry.{name} must be finite and nonnegative"
            ));
        }
    }
    let mark_style = match file.mark_style.as_deref() {
        Some("fill") => MarkStyle::Fill,
        Some("border") | None => MarkStyle::Border,
        Some(value) => {
            return Err(format!(
                "theme '{theme_id}' has unknown markStyle '{value}'"
            ));
        }
    };
    let mark_outline =
        resolve_optional(&file, theme_id, "markOutline", file.mark_outline.as_deref())?;
    let mark_ink = resolve_optional(&file, theme_id, "markInk", file.mark_ink.as_deref())?;
    let point_style = match file.point_style.as_deref() {
        Some("fill") => PointStyle::Fill,
        Some("ring") | None => PointStyle::Ring,
        Some(value) => {
            return Err(format!(
                "theme '{theme_id}' has unknown pointStyle '{value}'"
            ));
        }
    };
    let point_outline = resolve_optional(
        &file,
        theme_id,
        "pointOutline",
        file.point_outline.as_deref(),
    )?;
    Ok(Theme {
        id: file.id,
        name: file.name,
        geometry,
        mark_style,
        mark_outline,
        mark_ink,
        point_style,
        point_outline,
        point_halo: file.point_halo.unwrap_or(true),
        surfaces,
        text,
        roles,
        marks,
    })
}

/// Check a theme ID for use in filenames, environment variables, and menus.
pub fn validate_theme_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(format!(
            "theme id '{id}' must use 1–64 ASCII letters, digits, - or _"
        ));
    }
    Ok(())
}

/// Load and validate one of the built-in themes.
pub fn load_theme_checked(theme_id: &str) -> Result<Theme, String> {
    let source = builtin_theme_source(theme_id)?;
    let theme = parse_theme(source)?;
    if theme.id != theme_id {
        return Err(format!("built-in theme '{theme_id}' has id '{}'", theme.id));
    }
    Ok(theme)
}

/// The source text of a built-in theme, useful as a custom-theme starting point.
pub fn builtin_theme_source(theme_id: &str) -> Result<&'static str, String> {
    let source = match theme_id {
        "dark" => include_str!("../../assets/themes/default/dark.theme.toml"),
        "gray" => include_str!("../../assets/themes/default/gray.theme.toml"),
        "light" => include_str!("../../assets/themes/default/light.theme.toml"),
        _ => return Err(format!("unknown built-in theme '{theme_id}'")),
    };
    Ok(source)
}

/// Load one built-in theme, returning `None` if it is absent or invalid.
///
/// Prefer [`load_theme_checked`] when the caller can report a useful error.
pub fn load_theme(theme_id: &str) -> Option<Theme> {
    load_theme_checked(theme_id).ok()
}

// ---- glyph mark labels ----

/// Set or clear a glyph's mark.
///
/// Writes `public.markColor`, the fixed palette colour other editors
/// need, and `com.runebender.markLabel`, what the mark means,
/// together, or removes both.
#[cfg(test)]
pub fn set_glyph_mark(glyph: &mut norad::Glyph, label: Option<&str>) {
    match label.and_then(ufo_rgba_for_label) {
        Some(rgba) => {
            glyph
                .lib
                .insert(MARK_COLOR_KEY.into(), plist::Value::String(rgba));
            if let Some(label) = label {
                glyph
                    .lib
                    .insert(MARK_LABEL_KEY.into(), plist::Value::String(label.into()));
            }
        }
        None => {
            glyph.lib.remove(MARK_COLOR_KEY);
            glyph.lib.remove(MARK_LABEL_KEY);
        }
    }
}

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
/// saved mark hue. The snapped label is display only and never written
/// back.
#[cfg(test)]
pub fn mark_label_for_glyph(glyph: &norad::Glyph, theme: &Theme) -> Option<String> {
    if let Some(plist::Value::String(label)) = glyph.lib.get(MARK_LABEL_KEY)
        && theme.mark(label).is_some()
    {
        return Some(label.clone());
    }
    let plist::Value::String(rgba) = glyph.lib.get(MARK_COLOR_KEY)? else {
        return None;
    };
    label_for_rgba(rgba, theme)
}

/// Resolve canonical mark values against a display palette.
///
/// Unknown labels fall back to a recognized legacy color, if present.
pub fn mark_label_for_values(
    label: Option<&str>,
    color: Option<MarkColor>,
    theme: &Theme,
) -> Option<String> {
    label
        .filter(|label| theme.mark(label).is_some())
        .map(str::to_owned)
        .or_else(|| {
            let color = color?;
            label_for_channels(color.red, color.green, color.blue, theme)
        })
}

/// Resolve the display mark for one canonical glyph layer.
pub fn mark_label_for_layer(layer: crate::font::LayerView<'_>, theme: &Theme) -> Option<String> {
    mark_label_for_values(
        layer.mark_label().ok().flatten(),
        layer.mark_color().ok().flatten(),
        theme,
    )
}

/// Snap a UFO "r,g,b,a" colour (0–1 floats) to the nearest saved mark
/// label by hue. `None` for greys and colours far from every hue.
pub fn label_for_rgba(rgba: &str, theme: &Theme) -> Option<String> {
    let parts: Vec<f64> = rgba
        .split(',')
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    if parts.len() < 3 {
        return None;
    }
    label_for_channels(parts[0], parts[1], parts[2], theme)
}

fn label_for_channels(red: f64, green: f64, blue: f64, theme: &Theme) -> Option<String> {
    let hue = hue_of(red, green, blue)?;
    let mut best: Option<&str> = None;
    let mut best_distance = f64::INFINITY;
    for name in MARK_LABELS {
        if theme.mark(name).is_none() {
            continue;
        }
        let saved = MARK_UFO_COLORS
            .iter()
            .find_map(|(saved_name, color)| (*saved_name == name).then_some(*color))?;
        let channels: Vec<f64> = saved
            .split(',')
            .take(3)
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        let saved_hue = hue_of(channels[0], channels[1], channels[2])?;
        let raw = (hue - saved_hue).abs();
        let distance = raw.min(360.0 - raw);
        if distance < best_distance {
            best_distance = distance;
            best = Some(name);
        }
    }
    // Neighbouring palette hues are ~40° apart; anything further than
    // half that from every one has no name in this palette.
    best.filter(|_| best_distance <= 30.0).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_themes_have_complete_color_references() {
        for id in ["dark", "gray", "light"] {
            load_theme_checked(id).unwrap_or_else(|error| panic!("{error}"));
            let file: toml::Value = toml::from_str(builtin_theme_source(id).unwrap()).unwrap();
            let base = file["baseUi"].as_table().unwrap();
            assert_eq!(base.len(), 10, "{id} has ten Base UI stops");
            for step in 0..10 {
                assert!(base.contains_key(&format!("{step:02}")));
            }
            for section in ["surfaces", "text"] {
                for (role, value) in file[section].as_table().unwrap() {
                    assert!(
                        value.as_str().unwrap().starts_with("baseUi."),
                        "{id}.{section}.{role} must use the Base UI scale"
                    );
                }
            }
        }
    }

    #[test]
    fn custom_palette_changes_display_without_changing_saved_marks() {
        let source = builtin_theme_source("gray").expect("gray source");
        let mut file: toml::Value = toml::from_str(source).expect("built-in TOML");
        file["id"] = "custom".into();
        file["name"] = "Custom".into();
        file["baseUi"]["07"] = "#AABBCC".into();
        file["rainbow"]["hues"]["red"] = "oklch(0.61 0.167 200)".into();
        let theme =
            parse_theme(&toml::to_string(&file).expect("custom TOML")).expect("custom theme");

        assert_eq!(hex(theme.surface("panel")), "#aabbcc");
        assert_ne!(
            theme.mark("red"),
            load_theme("gray").and_then(|base| base.mark("red"))
        );
        assert_eq!(ufo_rgba_for_label("red").as_deref(), Some("1,0,0,1"));
        assert_eq!(label_for_rgba("1,0,0,1", &theme).as_deref(), Some("red"));
        assert_eq!(
            label_for_rgba("0.88,0.3,0.27,1", &theme).as_deref(),
            Some("red")
        );
    }

    #[test]
    fn window_ground_ink_is_optional_and_validated() {
        let source = include_str!("../../assets/themes/default/gray.theme.toml");
        let original = parse_theme(source).expect("Gray theme");
        assert_ne!(original.text("appInk"), original.text("primary"));

        let legacy = source.replace("appInk = \"baseUi.09\"\n", "");
        let legacy = parse_theme(&legacy).expect("older themes retain their window text");
        assert_eq!(legacy.text("appInk"), legacy.text("primary"));

        let invalid = source.replace("appInk = \"baseUi.09\"", "appInk = \"baseUi.missing\"");
        assert!(parse_theme(&invalid).unwrap_err().contains("text.appInk"));
    }

    #[test]
    fn inactive_header_ink_is_optional_and_validated() {
        let source = include_str!("../../assets/themes/default/gray.theme.toml");
        let original = parse_theme(source).expect("Gray theme");
        assert_eq!(original.text("headerMutedInk"), original.surface("field"));
        assert_eq!(original.text("headerMutedInk").a, 255);

        let legacy = source.replace("headerMutedInk = \"baseUi.08\"\n", "");
        let legacy = parse_theme(&legacy).expect("older themes keep their dimmed header ink");
        assert!(!legacy.text.contains_key("headerMutedInk"));

        let invalid = source.replace(
            "headerMutedInk = \"baseUi.08\"",
            "headerMutedInk = \"baseUi.missing\"",
        );
        assert!(
            parse_theme(&invalid)
                .unwrap_err()
                .contains("text.headerMutedInk")
        );
    }

    #[test]
    fn edited_theme_reports_missing_and_unknown_tokens() {
        let source = include_str!("../../assets/themes/default/gray.theme.toml");
        let mut file: toml::Value = toml::from_str(source).expect("built-in TOML");
        file["roles"]
            .as_table_mut()
            .expect("roles")
            .remove("pointSmooth");
        let edited = toml::to_string(&file).expect("edited TOML");
        assert_eq!(
            parse_theme(&edited).unwrap_err(),
            "theme 'gray' is missing roles.pointSmooth"
        );

        file["roles"]
            .as_table_mut()
            .expect("roles")
            .insert("pointSmooth".into(), "rainbow.blue.missing".into());
        let edited = toml::to_string(&file).expect("edited TOML");
        assert_eq!(
            parse_theme(&edited).unwrap_err(),
            "theme 'gray' roles.pointSmooth has unknown color 'rainbow.blue.missing'"
        );
    }

    #[test]
    fn edited_theme_reports_invalid_optional_color_and_style() {
        let source = include_str!("../../assets/themes/default/gray.theme.toml");
        let mut file: toml::Value = toml::from_str(source).expect("built-in TOML");
        file["pointOutline"] = "baseUi.nope".into();
        let edited = toml::to_string(&file).expect("edited TOML");
        assert_eq!(
            parse_theme(&edited).unwrap_err(),
            "theme 'gray' pointOutline has unknown color 'baseUi.nope'"
        );

        file["pointOutline"] = "baseUi.04".into();
        file["pointStyle"] = "circle".into();
        let edited = toml::to_string(&file).expect("edited TOML");
        assert_eq!(
            parse_theme(&edited).unwrap_err(),
            "theme 'gray' has unknown pointStyle 'circle'"
        );
    }

    #[test]
    fn edited_theme_rejects_unknown_role_and_format_version() {
        let source = builtin_theme_source("gray").expect("gray source");
        let mut file: toml::Value = toml::from_str(source).expect("built-in TOML");
        file["roles"]
            .as_table_mut()
            .expect("roles")
            .insert("pointSmoth".into(), "rainbow.blue.base".into());
        let edited = toml::to_string(&file).expect("edited TOML");
        assert_eq!(
            parse_theme(&edited).unwrap_err(),
            "theme 'gray' has unknown roles.pointSmoth"
        );

        file["roles"]
            .as_table_mut()
            .expect("roles")
            .remove("pointSmoth");
        file["formatVersion"] = 2.into();
        let edited = toml::to_string(&file).expect("edited TOML");
        assert_eq!(
            parse_theme(&edited).unwrap_err(),
            "theme 'gray' uses unsupported formatVersion 2"
        );
    }

    #[test]
    fn existing_json_themes_still_load() {
        let toml: toml::Value = toml::from_str(builtin_theme_source("gray").unwrap()).unwrap();
        let mut json = serde_json::to_value(&toml).unwrap();
        json["$comment"] = serde_json::json!(["Existing JSON comments remain valid."]);
        let json = serde_json::to_string(&json)
            .unwrap()
            .replace("rainbow", "glyphGrid");
        let theme = parse_theme(&json).expect("existing JSON theme");
        assert_eq!(theme.id, "gray");
        assert_eq!(theme.marks, load_theme("gray").unwrap().marks);
    }

    #[test]
    fn legacy_glyph_grid_tokens_preserve_rainbow_colors() {
        let source = builtin_theme_source("gray").unwrap();
        let mut file: toml::Value = toml::from_str(source).unwrap();
        for section in ["hues", "steps"] {
            for (_, value) in file["rainbow"][section].as_table_mut().unwrap().iter_mut() {
                let [lightness, chroma, hue] =
                    parse_oklch_components(value.as_str().unwrap()).unwrap();
                let mut components = toml::Table::new();
                components.insert("lightness".into(), lightness.into());
                components.insert("chroma".into(), chroma.into());
                if section == "hues" {
                    components.insert("hue".into(), hue.into());
                }
                *value = toml::Value::Table(components);
            }
        }
        let legacy_source = toml::to_string(&file)
            .unwrap()
            .replace("rainbow", "glyphGrid");
        let legacy = parse_theme(&legacy_source).unwrap();
        let current = parse_theme(source).unwrap();
        assert_eq!(legacy.marks, current.marks);
        assert_eq!(legacy.roles, current.roles);
    }

    #[test]
    fn rainbow_step_strings_reject_hue_changes_and_malformed_offsets() {
        let mut file: toml::Value = toml::from_str(builtin_theme_source("gray").unwrap()).unwrap();
        for value in ["oklch(0.13 -0.03 1)", "oklch(0.13 -0.03)"] {
            file["rainbow"]["steps"]["bright"] = value.into();
            assert!(parse_theme(&toml::to_string(&file).unwrap()).is_err());
        }
    }

    fn hex(c: ColorRgba) -> String {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    }

    /// The resolved dark palette uses its own display colors; changing
    /// them does not change the simple colors saved in a UFO.
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
        assert_eq!(hex(dark.role("pathStroke")), "#c1c1c1");
        assert_eq!(hex(dark.role("gridSelected")), "#c1c1c1");
        assert_eq!(hex(dark.role("continuityG2")), "#4db2a7");
    }

    /// The swatches drawn in the Colors panel.
    #[test]
    fn resolves_mark_swatches() {
        // Dark fills its marks at the bright step, so dark ink reads
        // on every one of them.
        let dark = load_theme("dark").expect("dark theme");
        assert_eq!(hex(dark.mark("red").unwrap()), "#f5867b");
        assert_eq!(hex(dark.mark("orange").unwrap()), "#ffa980");
        assert_eq!(hex(dark.mark("yellow").unwrap()), "#fde895");
        assert_eq!(hex(dark.mark("green").unwrap()), "#94d6a6");
        assert_eq!(dark.marks.len(), 7);
        assert!(dark.mark("chartreuse").is_none());
    }

    /// Unlabelled `public.markColor` values snap to the nearest
    /// palette hue; greys and far-off hues get no label.
    #[test]
    fn snaps_rgba_to_palette_label() {
        let dark = load_theme("dark").expect("dark theme");
        for (label, rgba) in MARK_UFO_COLORS {
            assert_eq!(label_for_rgba(rgba, &dark).as_deref(), Some(*label));
        }
        // Read legacy mark values already stored in UFOs.
        assert_eq!(
            label_for_rgba("0.88,0.3,0.27,1", &dark).as_deref(),
            Some("red")
        );
        assert_eq!(
            label_for_rgba("0.27,0.44,1,1", &dark).as_deref(),
            Some("blue")
        );
        assert_eq!(
            label_for_rgba("0.09,0.72,0.44,1", &dark).as_deref(),
            Some("green")
        );
        assert_eq!(label_for_rgba("0.5,0.5,0.5,1", &dark), None);
        assert_eq!(label_for_rgba("garbage", &dark), None);
    }

    /// UFO colors written for labels use simple normalized channels.
    #[test]
    fn ufo_rgba_uses_simple_colors() {
        assert_eq!(ufo_rgba_for_label("red").as_deref(), Some("1,0,0,1"));
        assert_eq!(ufo_rgba_for_label("orange").as_deref(), Some("1,0.5,0,1"));
        assert_eq!(ufo_rgba_for_label("yellow").as_deref(), Some("1,1,0,1"));
        assert_eq!(ufo_rgba_for_label("green").as_deref(), Some("0,1,0,1"));
        assert_eq!(ufo_rgba_for_label("blue").as_deref(), Some("0,0,1,1"));
        assert_eq!(ufo_rgba_for_label("purple").as_deref(), Some("0.5,0,1,1"));
        assert_eq!(ufo_rgba_for_label("pink").as_deref(), Some("1,0,0.5,1"));
        assert_eq!(ufo_rgba_for_label("mauve"), None);
    }

    #[test]
    fn set_mark_writes_both_keys_and_clears_both() {
        let dark = load_theme("dark").expect("dark theme");
        let mut glyph = norad::Glyph::new("A");
        set_glyph_mark(&mut glyph, Some("green"));
        assert_eq!(
            glyph.lib.get("public.markColor"),
            Some(&plist::Value::String("0,1,0,1".into()))
        );
        assert_eq!(
            mark_label_for_glyph(&glyph, &dark).as_deref(),
            Some("green")
        );
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
            plist::Value::String("1,0.5,0,1".into()),
        );
        assert_eq!(
            mark_label_for_glyph(&glyph, &dark).as_deref(),
            Some("orange")
        );
        glyph
            .lib
            .insert(MARK_LABEL_KEY.into(), plist::Value::String("blue".into()));
        assert_eq!(mark_label_for_glyph(&glyph, &dark).as_deref(), Some("blue"));
    }
}

#[cfg(test)]
mod geometry_tests {
    use super::*;

    #[test]
    fn every_theme_resolves_geometry() {
        for id in ["dark", "light", "gray"] {
            let theme = load_theme(id).expect("theme in the token file");
            assert!(theme.geometry.stroke > 0.0, "{id} stroke");
            assert!(theme.geometry.radius >= 0.0, "{id} radius");
            assert!(
                theme.geometry.stroke_emphasis >= theme.geometry.stroke,
                "{id}: an emphasis ring must be at least as heavy as the \
                 ordinary rule, or selection reads as less than chrome"
            );
        }
    }

    #[test]
    fn a_theme_without_geometry_takes_the_default() {
        let dark = load_theme("dark").expect("dark");
        assert_eq!(dark.geometry, Geometry::default());
    }

    #[test]
    fn explicit_square_geometry_overrides_the_default() {
        let source = include_str!("../../assets/themes/default/gray.theme.toml");
        let colors = source.split("[geometry]").next().expect("theme colors");
        let fixture = format!("{colors}\n[geometry]\nradius = 0.0\nradiusControl = 0.0\n");
        let gray = parse_theme(&fixture).expect("square geometry fixture");
        assert_eq!(gray.geometry.radius, 0.0);
        assert_eq!(gray.geometry.radius_panel, 0.0);
        assert_eq!(gray.geometry.radius_control, 0.0);
        assert_ne!(gray.geometry, Geometry::default());
        // Gray changes the corners and not the rule weight, so the
        // two stroke widths come from geometry.default. This is the
        // partial-override case the optional fields exist for.
        let fallback = Geometry::default();
        assert_eq!(gray.geometry.stroke, fallback.stroke);
        assert_eq!(gray.geometry.stroke_emphasis, fallback.stroke_emphasis);
    }

    #[test]
    fn panel_corners_can_differ_from_tile_corners_and_reject_negative_radii() {
        let source = include_str!("../../assets/themes/default/gray.theme.toml");
        let colors = source.split("[geometry]").next().expect("theme colors");
        let fixture = format!("{colors}\n[geometry]\nradius = 1.0\nradiusPanel = 8.0\n");
        let theme = parse_theme(&fixture).expect("separate panel corners");
        assert_eq!(theme.geometry.radius, 1.0);
        assert_eq!(theme.geometry.radius_panel, 8.0);
        let invalid = fixture.replace("radiusPanel = 8.0", "radiusPanel = -1.0");
        assert!(
            parse_theme(&invalid)
                .unwrap_err()
                .contains("geometry.radiusPanel")
        );
    }

    #[test]
    fn panel_shadows_are_optional_and_use_a_validated_theme_color() {
        let source = include_str!("../../assets/themes/default/gray.theme.toml");
        let theme = parse_theme(source).expect("Gray panel shadows");
        assert!(theme.geometry.shadow_panel);
        assert_eq!(theme.surface("panelShadow"), theme.text("secondary"));

        let disabled = source.replace("shadowPanel = true", "shadowPanel = false");
        assert!(
            !parse_theme(&disabled)
                .expect("disabled shadows")
                .geometry
                .shadow_panel
        );

        let legacy = source
            .replace("shadowPanel = true\n", "")
            .replace("panelShadow = \"baseUi.01\"\n", "");
        let legacy = parse_theme(&legacy).expect("older themes need no shadow tokens");
        assert!(!legacy.geometry.shadow_panel);
        assert_eq!(legacy.surface("panelShadow"), legacy.surface("outline"));

        let invalid = source.replace(
            "panelShadow = \"baseUi.01\"",
            "panelShadow = \"baseUi.missing\"",
        );
        assert!(
            parse_theme(&invalid)
                .unwrap_err()
                .contains("surfaces.panelShadow")
        );
    }

    #[test]
    fn gray_draws_its_rules_in_the_text_colour() {
        // The website's borders are the text colour, not a lighter
        // tint. That is the whole look; if these drift apart the theme
        // stops resembling it.
        let gray = load_theme("gray").expect("gray");
        assert_eq!(gray.surface("outline"), gray.text("primary"));
        assert_eq!(gray.surface("divider"), gray.text("primary"));
    }
}

#[cfg(test)]
mod mark_contrast {
    use super::*;

    fn relative_luminance(c: ColorRgba) -> f64 {
        let f = |v: u8| {
            let s = v as f64 / 255.0;
            if s <= 0.03928 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
    }

    /// WCAG contrast ratio, 1.0 (identical) to 21.0 (black on white).
    fn contrast(a: ColorRgba, b: ColorRgba) -> f64 {
        let (x, y) = (relative_luminance(a), relative_luminance(b));
        let (hi, lo) = if x > y { (x, y) } else { (y, x) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// A mark has to be legible against whatever it is actually drawn
    /// on, and that depends on the treatment. This is the test that was
    /// missing: Gray borrowed the Light theme's dim marks, whose
    /// lightness is tuned for a near-white canvas, and drew them as
    /// tinted rules on a mid-grey panel. Yellow came out at 1.00,
    /// the same luminance as the ground it sat on.
    #[test]
    fn every_mark_is_legible_on_every_theme() {
        const FLOOR: f64 = 3.0;
        for id in ["dark", "light", "gray"] {
            let theme = load_theme(id).expect("theme");
            for (name, mark) in &theme.marks {
                match theme.mark_style {
                    // A tinted rule is read against the surfaces behind it.
                    MarkStyle::Border => {
                        for surface in ["canvas", "panel"] {
                            let ratio = contrast(*mark, theme.surface(surface));
                            assert!(
                                ratio >= FLOOR,
                                "{id}: {name} on {surface} is {ratio:.2}, \
                                 under {FLOOR:.1}"
                            );
                        }
                    }
                    // A filled mark IS the ground, so what has to be
                    // legible is the label on top of it.
                    MarkStyle::Fill => {
                        let ink = theme
                            .mark_ink
                            .expect("a fill theme names the ink drawn on it");
                        let ratio = contrast(*mark, ink);
                        assert!(
                            ratio >= FLOOR,
                            "{id}: ink on {name} is {ratio:.2}, under {FLOOR:.1}"
                        );
                    }
                }
            }
        }
    }

    /// A filled mark is separated from its neighbours by the keyline,
    /// not by luminance, so the keyline itself has to read.
    #[test]
    fn a_filled_theme_keys_its_marks() {
        let gray = load_theme("gray").expect("gray");
        assert_eq!(gray.mark_style, MarkStyle::Fill);
        let outline = gray.mark_outline.expect("gray names a keyline");
        for (name, mark) in &gray.marks {
            let ratio = contrast(*mark, outline);
            assert!(ratio >= 3.0, "gray: keyline on {name} is {ratio:.2}");
        }
    }

    /// Every theme fills its marks the same way, so the grid does not
    /// change character with the theme.
    #[test]
    fn every_theme_fills_its_marks() {
        for id in ["dark", "light", "gray"] {
            let theme = load_theme(id).expect("theme");
            assert_eq!(theme.mark_style, MarkStyle::Fill, "{id}");
            assert!(theme.mark_ink.is_some(), "{id} names the ink on a mark");
            assert!(theme.mark_outline.is_some(), "{id} names the keyline");
        }
    }
}

#[cfg(test)]
mod ui_contrast {
    use super::*;

    fn relative_luminance(c: ColorRgba) -> f64 {
        let f = |v: u8| {
            let s = v as f64 / 255.0;
            if s <= 0.03928 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
    }

    fn contrast(a: ColorRgba, b: ColorRgba) -> f64 {
        let (x, y) = (relative_luminance(a), relative_luminance(b));
        let (hi, lo) = if x > y { (x, y) } else { (y, x) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// WCAG's floor for user interface components and graphical
    /// objects. Body-text prose wants 4.5, but almost nothing in an
    /// editor chrome is prose: it is labels, tiles and marks.
    const FLOOR: f64 = 3.0;

    const THEMES: [&str; 3] = ["dark", "gray", "light"];
    // Prose sits on workspace surfaces. The window ground uses its own ink.
    const SURFACES: [&str; 5] = ["panel", "control", "button", "field", "canvas"];
    /// The text tokens the editor actually draws with. `muted` and
    /// `subdued` are in the file for other front-ends and are not
    /// checked here, because a floor nothing has to meet is noise.
    const TEXT: [&str; 3] = ["primary", "secondary", "glyph"];

    #[test]
    fn text_reads_on_every_surface() {
        for id in THEMES {
            let theme = load_theme(id).expect("theme");
            assert!(
                contrast(theme.text("appInk"), theme.surface("app")) >= FLOOR,
                "{id}: application text must read on the window ground"
            );
            for surface in SURFACES {
                for text in TEXT {
                    let ratio = contrast(theme.text(text), theme.surface(surface));
                    assert!(
                        ratio >= FLOOR,
                        "{id}: {text} text on {surface} is {ratio:.2}, under {FLOOR:.1}"
                    );
                }
            }
        }
    }

    /// Point colours are drawn on the canvas, not the panel. Checking
    /// them against the wrong ground is how a real problem hides: it
    /// passes against one surface and fails on the one you look at.
    #[test]
    fn point_colours_read_on_the_canvas() {
        const POINTS: [&str; 5] = [
            "pointSmooth",
            "pointCorner",
            "pointOffcurve",
            "pointSelected",
            "startNode",
        ];
        for id in THEMES {
            let theme = load_theme(id).expect("theme");
            let canvas = theme.surface("canvas");
            // A filled point keeps its edge with the keyline, so the
            // keyline is what has to read; the hues carry meaning,
            // not the edge.
            if theme.point_style == PointStyle::Fill {
                let outline = theme.point_outline.expect("a filled point names a keyline");
                let ratio = contrast(outline, canvas);
                assert!(ratio >= FLOOR, "{id}: pointOutline on canvas is {ratio:.2}");
                continue;
            }
            for role in POINTS {
                let ratio = contrast(theme.role(role), canvas);
                assert!(ratio >= FLOOR, "{id}: {role} on canvas is {ratio:.2}");
            }
        }
    }

    /// The chrome roles are drawn over panels.
    ///
    /// This is what caught Gray: its accent measured 1.62 against a
    /// panel two steps darker than the app behind it, so the selected
    /// thing was the hardest thing to see.
    #[test]
    fn chrome_roles_read_on_the_panel() {
        for id in THEMES {
            let theme = load_theme(id).expect("theme");
            let panel = theme.surface("panel");
            for role in ["accent", "danger"] {
                let ratio = contrast(theme.role(role), panel);
                assert!(ratio >= FLOOR, "{id}: {role} on panel is {ratio:.2}");
            }
            // Warning text is the palette orange on a theme that
            // fills its points, the same hue as the orange mark
            // cells, by the same reasoning as selection below.
            let warning = theme.role("warning");
            if theme.point_style == PointStyle::Fill {
                assert_eq!(
                    Some(warning),
                    theme.mark("orange"),
                    "{id}: warning is the palette orange"
                );
            } else {
                let ratio = contrast(warning, panel);
                assert!(ratio >= FLOOR, "{id}: warning on panel is {ratio:.2}");
            }
            // Selection is drawn on the canvas: the marquee, the
            // transform box, the ring on a selected point. On a theme
            // that fills its points, selection is the palette orange
            // by design, the same hue as the mark cells, and a mid
            // hue on a mid grey cannot reach the floor; the check
            // there is that it is that hue and not a darkened one.
            let canvas = theme.surface("canvas");
            let selection = theme.role("selection");
            if theme.point_style == PointStyle::Fill {
                assert_eq!(
                    Some(selection),
                    theme.mark("orange"),
                    "{id}: selection is the palette orange"
                );
                continue;
            }
            let ratio = contrast(selection, canvas);
            assert!(ratio >= FLOOR, "{id}: selection on canvas is {ratio:.2}");
        }
    }

    /// Primary and secondary have to stay apart, or the hierarchy the
    /// two tokens exist for is not there.
    #[test]
    fn the_two_text_levels_stay_distinct() {
        for id in THEMES {
            let theme = load_theme(id).expect("theme");
            let ratio = contrast(theme.text("primary"), theme.text("secondary"));
            assert!(
                ratio > 1.05,
                "{id}: primary and secondary are the same colour ({ratio:.3})"
            );
        }
    }
}
