// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Colors from the shared OKLCH theme file, as peniko colors.
//!
//! The framework does not currently resolve Runebender's application theme,
//! so this module maps the shared semantic tokens once for every view.

use runebender::ui::color::ColorRgba;
use runebender::ui::theme::Theme as CoreTheme;
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
    /// Corner radius for glyph tiles and small popups, supplied by the active theme.
    pub corner_radius: f64,
    /// Corner radius for pressable controls and fields.
    pub control_radius: f64,
    pub control: Color,
    pub button: Color,
    pub canvas: Color,
    pub field: Color,
    grid_background: Color,
    cell_shadow_color: Color,
    floating_pane_header: Color,
    slider_track_color: Color,
    pub text: Color,
    pub text_muted: Color,
    pub text_subdued: Color,
    /// Neutral control-handle lines from the shared secondary text token.
    pub handle_line: Color,
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
        let t = crate::application::platform::themes::catalog()
            .get(theme_id)
            .unwrap_or_else(|| panic!("unknown Runebender theme '{theme_id}'"));
        Self::from_theme(t)
    }

    fn from_theme(t: &CoreTheme) -> Self {
        let panel = color(t.surface("panel"));
        let text = color(t.text("primary"));
        Self {
            app: color(t.surface("app")),
            panel,
            tab_rail: color(t.surface("tabRail")),
            inactive_tab: color(t.surface("inactiveTab")),
            header: color(t.surface("header")),
            header_ink: color(t.text("headerInk")),
            corner_radius: f64::from(t.geometry.radius),
            control_radius: f64::from(t.geometry.radius_control),
            control: color(t.surface("control")),
            button: color(t.surface("button")),
            canvas: color(t.surface("canvas")),
            field: color(t.surface("field")),
            grid_background: color(t.surface("gridBackground")),
            cell_shadow_color: color(t.surface("cellShadow")),
            floating_pane_header: color(t.surface("floatingPaneHeader")),
            slider_track_color: color(t.surface("sliderTrack")),
            text,
            text_muted: color(t.text("muted")),
            text_subdued: color(t.text("subdued")),
            handle_line: color(t.text("secondary")),
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
            points_filled: t.point_style == runebender::ui::theme::PointStyle::Fill,
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

    /// The ground behind both glyph grids, recessed from the application surface.
    pub(crate) fn grid_bg(&self) -> Color {
        self.grid_background
    }

    /// Recessed tile shadow, halfway between the grid ground and selected surface.
    pub(crate) fn cell_shadow(&self) -> Color {
        self.cell_shadow_color
    }

    /// The quieter header surface used by an unmarked floating metrics card.
    pub(crate) fn floating_pane_header_bg(&self) -> Color {
        self.floating_pane_header
    }

    /// Whatever a tool draws while the pointer is down: the ink.
    pub(crate) fn tool_feedback(&self) -> Color {
        self.text
    }

    /// The ink for filled type and compact marks inside the editing workspace.
    ///
    /// Filled proof type uses the theme's preview-fill neutral, which remains
    /// quieter than Gray's shared dark ink for keylines and controls.
    pub(crate) fn editor_ink(&self) -> Color {
        self.role("previewFill")
    }

    /// The neutral for a compact editor control.
    ///
    /// Gray keeps controls quieter than structural outlines and darker than
    /// filled proof type.
    pub(crate) fn editor_control_ink(&self) -> Color {
        self.text_muted
    }

    /// A quiet slider rail halfway between its original ink and the control surface.
    pub(crate) fn slider_track(&self) -> Color {
        self.slider_track_color
    }

    /// The metrics lines: their own token, never the accent.
    pub(crate) fn metrics_line(&self) -> Color {
        self.role("metricsLine")
    }

    /// The translucent fill beneath an editable glyph outline.
    ///
    /// Keep this recipe beside the shared `outlineFill` role so every canvas
    /// uses GPUI's opacity without restating it at individual paint sites.
    pub(crate) fn outline_fill(&self) -> Color {
        const EDIT_FILL_ALPHA: f32 = 0.70;
        let fill = self.role("outlineFill");
        Color::new([
            fill.components[0],
            fill.components[1],
            fill.components[2],
            fill.components[3] * EDIT_FILL_ALPHA,
        ])
    }

    pub(crate) fn role(&self, name: &str) -> Color {
        *self
            .roles
            .get(name)
            .unwrap_or_else(|| panic!("view requested unknown theme role '{name}'"))
    }

    /// Theme mark labels with their colors, in theme order.
    pub(crate) fn mark_list(&self) -> Vec<(String, Color)> {
        self.mark_order
            .iter()
            .filter_map(|k| self.marks.get(k).map(|c| (k.clone(), *c)))
            .collect()
    }

    /// Popcount tier ramp, shared with the web editor: one power of two
    /// is structural (green), two an elegant
    /// sum (yellow), three acceptable (orange), four or more a flagged
    /// correction (red).
    pub(crate) fn popcount(&self, count: u32) -> Color {
        match count {
            0 | 1 => self.role("popcount1"),
            2 => self.role("popcount2"),
            3 => self.role("popcount3"),
            _ => self.role("popcount4"),
        }
    }

    /// GPUI's shared mark-color ramp for normalized curvature magnitude.
    pub(crate) fn comb_gradient(&self, t: f64) -> Color {
        let stops = ["green", "blue", "purple", "pink", "orange"];
        let u = t.clamp(0.0, 1.0) * 4.0;
        let (i, f) = if u < 1.0 {
            (0, u)
        } else if u < 2.0 {
            (1, u - 1.0)
        } else if u < 3.0 {
            (2, u - 2.0)
        } else {
            (3, u - 3.0)
        };
        let a = self.mark(stops[i]).unwrap_or(self.text).components;
        let b = self.mark(stops[i + 1]).unwrap_or(self.text).components;
        let f = crate::application::view::render::px32(f);
        Color::new([
            a[0] + (b[0] - a[0]) * f,
            a[1] + (b[1] - a[1]) * f,
            a[2] + (b[2] - a[2]) * f,
            1.0,
        ])
    }

    pub(crate) fn mark(&self, label: &str) -> Option<Color> {
        self.marks.get(label).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editable_outline_fill_preserves_role_color_at_gpui_opacity() {
        for theme_id in ["dark", "gray", "light"] {
            let palette = Palette::load(theme_id);
            let role = palette.role("outlineFill").components;
            let fill = palette.outline_fill().components;
            assert_eq!(&fill[..3], &role[..3]);
            assert!((fill[3] - role[3] * 0.70).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn gray_editor_ink_is_quieter_than_the_structural_outline() {
        let palette = Palette::load("gray");
        assert_eq!(palette.editor_ink(), palette.role("previewFill"));
        assert_eq!(palette.editor_control_ink(), palette.text_muted);
        assert!(palette.editor_control_ink().components[0] > palette.outline.components[0]);
        assert!(palette.editor_ink().components[0] > palette.outline.components[0]);
        assert!(palette.editor_ink().components[0] > palette.editor_control_ink().components[0]);
    }

    #[test]
    fn gray_slider_track_stays_between_ink_and_control_surface() {
        let palette = Palette::load("gray");
        let track = palette.slider_track().components;
        for (component, (ink, surface)) in track[..3].iter().zip(
            palette.text_muted.components[..3]
                .iter()
                .zip(palette.control.components[..3].iter()),
        ) {
            assert!(*component > (*ink).min(*surface));
            assert!(*component < (*ink).max(*surface));
        }
    }
}
