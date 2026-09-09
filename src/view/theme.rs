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
    /// The recessed surface behind editor-rail tabs.
    pub tab_rail: Color,
    /// The face of an inactive editor-rail tab.
    pub inactive_tab: Color,
    /// The quiet dark band behind the title and tools.
    pub header: Color,
    /// The high-contrast ink used exclusively on the header band.
    pub header_ink: Color,
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
    /// Points are hue-filled with a keyline, not dark with a hue ring.
    pub points_filled: bool,
    /// The keyline around a filled point.
    pub point_outline: Option<Color>,
    /// Points and anchors get a halo of the ground under them.
    pub point_halo: bool,
}

impl Palette {
    pub(crate) fn load(theme_id: &str) -> Self {
        let t = load_theme(theme_id).expect("theme file present");
        Self::from_theme(&t, theme_id)
    }

    fn from_theme(t: &CoreTheme, theme_id: &str) -> Self {
        let titlebar = color(t.surface("titlebar"));
        let panel = color(t.surface("panel"));
        let selected = color(t.role("controlSelected"));
        let text = color(t.text("primary"));
        Self {
            app: color(t.surface("app")),
            panel,
            // The GPUI rail steps from titlebar to inactive tab to panel.
            // Derive its middle step from the shared surfaces so every
            // shipped theme keeps the same relationship without an
            // application-owned colour literal.
            tab_rail: titlebar,
            inactive_tab: Color::new([
                (titlebar.components[0] + panel.components[0]) * 0.5,
                (titlebar.components[1] + panel.components[1]) * 0.5,
                (titlebar.components[2] + panel.components[2]) * 0.5,
                1.0,
            ]),
            header: Color::new([
                selected.components[0] * 0.5,
                selected.components[1] * 0.5,
                selected.components[2] * 0.5,
                1.0,
            ]),
            // Gray and Light invert the header controls; Dark's selected
            // ink is intentionally dark, so it keeps ordinary text ink.
            header_ink: if theme_id == "dark" {
                text
            } else {
                color(t.role("controlSelectedInk"))
            },
            control: color(t.surface("control")),
            button: color(t.surface("button")),
            canvas: color(t.surface("canvas")),
            field: color(t.surface("field")),
            text,
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
            points_filled: t.point_style == runebender_core::ui::theme::PointStyle::Fill,
            point_outline: t.point_outline.map(color),
            point_halo: t.point_halo,
        }
    }

    pub(crate) fn field(&self) -> Color {
        self.field
    }

    /// Selection is inversion, never a hue: the fill of anything
    /// selected or active is the ink.
    pub(crate) fn selected_bg(&self) -> Color {
        self.role("controlSelected")
    }

    /// The ink on a selected fill: the panel colour.
    pub(crate) fn selected_ink(&self) -> Color {
        self.role("controlSelectedInk")
    }

    /// The selected glyph or sidebar label, matching GPUI's yellow mark ink.
    pub(crate) fn selected_content_ink(&self) -> Color {
        self.mark("yellow").unwrap_or_else(|| self.selected_ink())
    }

    /// Recessed tile shadow, derived from the grid ground and selected surface.
    pub(crate) fn cell_shadow(&self) -> Color {
        let selected = self.selected_bg();
        Color::new([
            (self.app.components[0] + selected.components[0]) * 0.5,
            (self.app.components[1] + selected.components[1]) * 0.5,
            (self.app.components[2] + selected.components[2]) * 0.5,
            1.0,
        ])
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

    /// The ring around a selected point: its own role where the theme
    /// names one, else the selection colour.
    pub(crate) fn point_selected_ring(&self) -> Color {
        self.roles
            .get("pointSelectedRing")
            .copied()
            .unwrap_or_else(|| self.role("selection"))
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
