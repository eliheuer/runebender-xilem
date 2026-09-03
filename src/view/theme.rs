// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Colors from the shared OKLCH theme file, as peniko colors.
//!
//! xix note: this whole file is the design kernel's job. The app should
//! not hand-map named tokens into a palette; the framework's theme should
//! carry them and the widgets should read them.

use runebender_core::ui::color::ColorRgba;
use runebender_core::ui::theme::{Theme as CoreTheme, load_theme};
use std::collections::HashMap;
use xilem::Color;

fn color(c: ColorRgba) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
}

/// A resolved palette: named surfaces, text, roles, and mark colors.
pub(crate) struct Palette {
    pub app: Color,
    pub panel: Color,
    pub control: Color,
    pub button: Color,
    pub canvas: Color,
    pub field: Color,
    pub text: Color,
    pub text_muted: Color,
    /// The rule around a panel and a grid cell: the keyline.
    pub outline: Color,
    /// The rule around a text field, quieter than a panel's.
    pub field_outline: Color,
    roles: HashMap<String, Color>,
    marks: HashMap<String, Color>,
    mark_order: Vec<String>,
    /// The keyline a filled mark cell carries, when the theme names one.
    pub mark_outline: Option<Color>,
    /// The ink on a filled mark cell, when the theme names one.
    pub mark_ink: Option<Color>,
}

impl Palette {
    pub(crate) fn load(theme_id: &str) -> Self {
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
            text: color(t.text("primary")),
            text_muted: color(t.text("muted")),
            outline: color(t.surface("outline")),
            field_outline: color(t.surface("fieldOutline")),
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
            mark_outline: t.mark_outline.map(color),
            mark_ink: t.mark_ink.map(color),
        }
    }

    pub(crate) fn field(&self) -> Color {
        self.field
    }

    /// Selection is inversion, never a hue: the fill of anything
    /// selected or active is the ink.
    pub(crate) fn selected_bg(&self) -> Color {
        self.text
    }

    /// The ink on a selected fill: the panel colour.
    pub(crate) fn selected_ink(&self) -> Color {
        self.panel
    }

    /// Whatever a tool draws while the pointer is down: the ink.
    pub(crate) fn tool_feedback(&self) -> Color {
        self.text
    }

    /// The metrics lines: their own token, never the accent.
    pub(crate) fn metrics_line(&self) -> Color {
        self.role("metricsLine")
    }

    pub(crate) fn role(&self, name: &str) -> Color {
        self.roles.get(name).copied().unwrap_or(Color::WHITE)
    }

    /// Theme mark labels with their colors, in theme order.
    pub(crate) fn mark_list(&self) -> Vec<(String, Color)> {
        self.mark_order
            .iter()
            .filter_map(|k| self.marks.get(k).map(|c| (k.clone(), *c)))
            .collect()
    }

    /// Popcount tier ramp, shared with the GPUI build and the web
    /// editor: one power of two is structural (green), two an elegant
    /// sum (yellow), three acceptable (orange), four or more a flagged
    /// correction (red).
    pub(crate) fn popcount(&self, count: u32) -> Color {
        match count {
            0 | 1 => Color::from_rgb8(0x17, 0xb8, 0x70),
            2 => Color::from_rgb8(0xff, 0xdb, 0x33),
            3 => Color::from_rgb8(0xff, 0x99, 0x0f),
            _ => Color::from_rgb8(0xff, 0x4a, 0x3d),
        }
    }

    pub(crate) fn mark(&self, label: &str) -> Option<Color> {
        self.marks.get(label).copied()
    }
}
