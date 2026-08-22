// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Colors from the shared OKLCH theme file, as peniko colors.
//!
//! xix note: this whole file is the design kernel's job. The app should
//! not hand-map named tokens into a palette; the framework's theme should
//! carry them and the widgets should read them.

use runebender_core::theme::ColorRgba;
use runebender_core::theme_oklch::{Theme as CoreTheme, load_theme};
use std::collections::HashMap;
use xilem::Color;

fn color(c: ColorRgba) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
}

/// A resolved palette: named surfaces, text, roles, and mark colors.
pub struct Palette {
    pub app: Color,
    pub panel: Color,
    pub control: Color,
    pub button: Color,
    pub canvas: Color,
    pub divider: Color,
    pub text: Color,
    pub text_muted: Color,
    roles: HashMap<String, Color>,
    marks: HashMap<String, Color>,
}

impl Palette {
    pub fn load(theme_id: &str) -> Self {
        let t = load_theme(theme_id).expect("theme file present");
        Self::from_theme(&t)
    }

    fn from_theme(t: &CoreTheme) -> Self {
        Self {
            app: color(t.surface("app")),
            panel: color(t.surface("panel")),
            control: color(t.surface("control")),
            button: color(t.surface("button")),
            canvas: color(t.surface("canvas")),
            divider: color(t.surface("divider")),
            text: color(t.text("primary")),
            text_muted: color(t.text("muted")),
            roles: t.roles.iter().map(|(k, v)| (k.clone(), color(*v))).collect(),
            marks: t.marks.iter().map(|(k, v)| (k.clone(), color(*v))).collect(),
        }
    }

    pub fn role(&self, name: &str) -> Color {
        self.roles.get(name).copied().unwrap_or(Color::WHITE)
    }

    pub fn mark(&self, label: &str) -> Option<Color> {
        self.marks.get(label).copied()
    }
}
