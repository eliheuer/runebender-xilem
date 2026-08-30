// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Colors from the shared OKLCH theme file, as peniko colors.
//!
//! xix note: this whole file is the design kernel's job. The app should
//! not hand-map named tokens into a palette; the framework's theme should
//! carry them and the widgets should read them.

use runebender_core::ui::theme::ColorRgba;
use runebender_core::ui::theme_oklch::{Theme as CoreTheme, load_theme};
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
    pub field: Color,
    pub divider: Color,
    pub text: Color,
    pub text_muted: Color,
    roles: HashMap<String, Color>,
    marks: HashMap<String, Color>,
    mark_order: Vec<String>,
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
            field: color(t.surface("field")),
            divider: color(t.surface("divider")),
            text: color(t.text("primary")),
            text_muted: color(t.text("muted")),
            roles: t
                .roles
                .iter()
                .map(|(k, v)| (k.clone(), color(*v)))
                .collect(),
            marks: t
                .marks
                .iter()
                .map(|(k, v)| (k.clone(), color(*v)))
                .collect(),
            mark_order: t.marks.iter().map(|(k, _)| k.clone()).collect(),
        }
    }

    pub fn field(&self) -> Color {
        self.field
    }

    pub fn role(&self, name: &str) -> Color {
        self.roles.get(name).copied().unwrap_or(Color::WHITE)
    }

    /// Theme mark labels with their colors, in theme order.
    pub fn mark_list(&self) -> Vec<(String, Color)> {
        self.mark_order
            .iter()
            .filter_map(|k| self.marks.get(k).map(|c| (k.clone(), *c)))
            .collect()
    }

    /// Popcount tier ramp, shared with the GPUI build and the web
    /// editor: one power of two is structural (green), two an elegant
    /// sum (yellow), three acceptable (orange), four or more a flagged
    /// correction (red).
    pub fn popcount(&self, count: u32) -> Color {
        match count {
            0 | 1 => Color::from_rgb8(0x17, 0xb8, 0x70),
            2 => Color::from_rgb8(0xff, 0xdb, 0x33),
            3 => Color::from_rgb8(0xff, 0x99, 0x0f),
            _ => Color::from_rgb8(0xff, 0x4a, 0x3d),
        }
    }

    pub fn mark(&self, label: &str) -> Option<Color> {
        self.marks.get(label).copied()
    }
}
