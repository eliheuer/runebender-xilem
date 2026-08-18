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
    steps: Vec<u32>,
}

#[derive(Deserialize)]
struct ThemeDef {
    surfaces: HashMap<String, String>,
    text: HashMap<String, String>,
    roles: HashMap<String, String>,
}

#[derive(Deserialize)]
struct TokenFile {
    hues: HashMap<String, HueDef>,
    steps: HashMap<String, StepDef>,
    neutral: NeutralDef,
    themes: HashMap<String, ThemeDef>,
}

/// One resolved theme: every surface, text, and role token as sRGB.
pub struct Theme {
    pub surfaces: HashMap<String, ColorRgba>,
    pub text: HashMap<String, ColorRgba>,
    pub roles: HashMap<String, ColorRgba>,
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
    Some(Theme {
        surfaces: resolve_map(&def.surfaces),
        text: resolve_map(&def.text),
        roles: resolve_map(&def.roles),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(c: ColorRgba) -> String {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    }

    /// Values must match runebender-web's generated tokens exactly
    /// (src/themeTokens.generated.ts).
    #[test]
    fn matches_web_generated_tokens() {
        let dark = load_theme("dark").expect("dark theme");
        assert_eq!(hex(dark.surface("app")), "#0b0b0b");
        assert_eq!(hex(dark.surface("panel")), "#121212");
        assert_eq!(hex(dark.surface("outline")), "#404040");
        assert_eq!(hex(dark.text("primary")), "#8f8f8f");
        assert_eq!(hex(dark.role("accent")), "#4fb772");
        assert_eq!(hex(dark.role("warning")), "#e8c944");
        assert_eq!(hex(dark.role("selection")), "#ec7433");
        assert_eq!(hex(dark.role("pointSmooth")), "#4494db");
        assert_eq!(hex(dark.role("pointOffcurve")), "#8a6fe1");
        assert_eq!(hex(dark.role("pointSelected")), "#ffe88a");
        assert_eq!(hex(dark.role("pathStroke")), "#b1b1b1");
        assert_eq!(hex(dark.role("gridSelected")), "#c1c1c1");
        assert_eq!(hex(dark.role("continuityG2")), "#41b7ab");
    }
}
