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

/// Native chrome follows the opaque theme ground, independently of wallpaper and blur tint.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn window_theme_for_color(background: Color) -> winit::window::Theme {
    let [red, green, blue, _] = background.components;
    let brightness = 0.2126 * red + 0.7152 * green + 0.0722 * blue;
    if brightness < 0.5 {
        winit::window::Theme::Dark
    } else {
        winit::window::Theme::Light
    }
}

/// The theme's request for the native window backdrop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BackdropStyle {
    /// Whether the theme asks for the backdrop at all.
    pub enabled: bool,
    /// The window ground, which an automatic appearance follows.
    pub ground: Color,
    /// The material behind the blur.
    pub material: runebender::ui::theme::BlurMaterial,
    /// Light, dark, or automatic.
    pub appearance: runebender::ui::theme::BlurAppearance,
}

/// A resolved palette: named surfaces, text, roles, and mark colors.
pub(crate) struct Palette {
    pub app: Color,
    /// Independent gray overlay for the translucent macOS window ground.
    backdrop_tint: Color,
    /// Whether the theme requests the optional macOS backdrop.
    pub blur_background: bool,
    /// Opacity of the independent tint over the native backdrop.
    blur_tint_opacity: f32,
    /// Whether the side panels let the native backdrop show through.
    blur_panels: bool,
    /// Opacity of the side panels' face over the native backdrop.
    panel_tint_opacity: f32,
    /// The side panels' face color when they are frosted.
    panel_tint: Color,
    /// The macOS material and its appearance behind the blur.
    blur_material: runebender::ui::theme::BlurMaterial,
    blur_appearance: runebender::ui::theme::BlurAppearance,
    /// Text on the window ground; custom themes can supply the optional `appInk` token.
    pub app_ink: Color,
    pub panel: Color,
    /// The recessed surface behind editor-rail tabs.
    pub tab_rail: Color,
    /// The face of an inactive editor-rail tab.
    pub inactive_tab: Color,
    /// Solid inactive tab ink, with the legacy opacity for themes that omit it.
    pub inactive_tab_ink: Color,
    /// Icon ink on the active rail tab.
    pub active_tab_ink: Color,
    /// The background behind the title and tools.
    pub header: Color,
    /// The contrasting ink used on the header background.
    pub header_ink: Color,
    /// Optional solid ink for inactive header controls; older themes retain their opacity.
    header_muted_ink: Option<Color>,
    /// Corner radius for glyph tiles and small popups, supplied by the active theme.
    pub corner_radius: f64,
    /// Corner radius for the floating workspace panels.
    pub panel_radius: f64,
    /// A themed shadow color when panel shadows are enabled.
    pub panel_shadow: Option<Color>,
    shadow_panels_over_backdrop: bool,
    /// Corner radius for pressable controls and fields.
    pub control_radius: f64,
    pub control: Color,
    pub button: Color,
    /// Normal slider knob fill.
    pub slider_thumb: Color,
    /// Slider knob fill while focused or dragged.
    pub slider_thumb_active: Color,
    pub canvas: Color,
    /// Independent surface for the glyph outline preview in the inspector.
    pub glyph_preview: Color,
    /// The proof strip under the edit canvas, one step apart from the canvas.
    pub proof_strip: Color,
    pub field: Color,
    grid_background: Color,
    cell_shadow_color: Color,
    floating_pane_header: Color,
    slider_track_color: Color,
    pub text: Color,
    pub text_muted: Color,
    /// Glyph previews and their captions in the glyph grid.
    pub glyph_ink: Color,
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
    /// Whether glyph marks color the tile face rather than its outline and ink.
    pub marks_filled: bool,
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
    /// The fill of a flat control or container drawn on a panel: none, so the panel's face
    /// shows through, whether it is solid or frosted.
    pub(crate) const FLAT: Color = Color::TRANSPARENT;

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
            backdrop_tint: color(t.surface("backdropTint")),
            blur_background: t.window.blur_background,
            shadow_panels_over_backdrop: t.window.shadow_panels,
            blur_tint_opacity: t.window.blur_tint_opacity,
            blur_panels: t.window.blur_panels,
            panel_tint_opacity: t.window.panel_tint_opacity,
            panel_tint: color(t.surface("panelTint")),
            blur_material: t.window.blur_material,
            blur_appearance: t.window.blur_appearance,
            app_ink: color(t.text("appInk")),
            panel,
            tab_rail: color(t.surface("tabRail")),
            inactive_tab: color(t.surface("inactiveTab")),
            active_tab_ink: color(t.text("activeTabInk")),
            inactive_tab_ink: t
                .text
                .get("inactiveTabInk")
                .copied()
                .map(color)
                .unwrap_or_else(|| text.with_alpha(0.42)),
            header: color(t.surface("header")),
            header_ink: color(t.text("headerInk")),
            header_muted_ink: t.text.get("headerMutedInk").copied().map(color),
            corner_radius: f64::from(t.geometry.radius),
            panel_radius: f64::from(t.geometry.radius_panel),
            panel_shadow: t
                .geometry
                .shadow_panel
                .then(|| color(t.surface("panelShadow")).with_alpha(0.5)),
            control_radius: f64::from(t.geometry.radius_control),
            control: color(t.surface("control")),
            button: color(t.surface("button")),
            slider_thumb: color(t.surface("sliderThumb")),
            slider_thumb_active: color(t.surface("sliderThumbActive")),
            canvas: color(t.surface("canvas")),
            glyph_preview: color(t.surface("glyphPreview")),
            proof_strip: color(t.surface("proofStrip")),
            field: color(t.surface("field")),
            grid_background: color(t.surface("gridBackground")),
            cell_shadow_color: color(t.surface("cellShadow")),
            floating_pane_header: color(t.surface("floatingPaneHeader")),
            slider_track_color: color(t.surface("sliderTrack")),
            text,
            text_muted: color(t.text("muted")),
            glyph_ink: color(t.text("glyph")),
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
            marks_filled: t.mark_style == runebender::ui::theme::MarkStyle::Fill,
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

    /// Light or dark native title-bar styling, derived from this theme's window ground.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn window_theme(&self) -> winit::window::Theme {
        window_theme_for_color(self.app)
    }

    /// Tint the native backdrop once, beneath the opaque floating panels.
    pub(crate) fn app_background(&self) -> Color {
        if self.native_backdrop_active() {
            // Apply one independent tint over the native material, beneath all panels.
            self.backdrop_tint.with_alpha(self.blur_tint_opacity)
        } else {
            self.app
        }
    }

    /// The face of the left and right panels: the panel color, or the theme's `panelTint` at
    /// `panelTintOpacity` over the native backdrop when it sets `blurPanels`. The center
    /// panel stays solid.
    pub(crate) fn side_panel_face(&self) -> Color {
        let alpha = self.side_panel_alpha();
        if alpha < 1.0 {
            self.panel_tint.with_alpha(alpha)
        } else {
            self.panel
        }
    }

    /// How opaque side-panel surfaces are: 1 unless the theme frosts the side panels and
    /// the native backdrop is active. Surfaces drawn on a side panel scale by this too.
    pub(crate) fn side_panel_alpha(&self) -> f32 {
        self.side_panel_alpha_for_backdrop(self.native_backdrop_active())
    }

    fn side_panel_alpha_for_backdrop(&self, active: bool) -> f32 {
        if self.blur_panels && active {
            self.panel_tint_opacity
        } else {
            1.0
        }
    }

    /// Everything the native backdrop needs from the theme.
    pub(crate) fn backdrop_style(&self) -> BackdropStyle {
        BackdropStyle {
            enabled: self.blur_background,
            ground: self.app,
            material: self.blur_material,
            appearance: self.blur_appearance,
        }
    }

    /// The window root beneath everything: clear over the native backdrop, where the
    /// gutters and title bar paint the ground themselves so no panel sits on the tint.
    pub(crate) fn window_root_background(&self) -> Color {
        if self.native_backdrop_active() {
            Color::TRANSPARENT
        } else {
            self.app
        }
    }

    /// The title bar row: the window ground over the native backdrop, else the header.
    pub(crate) fn titlebar_background(&self) -> Color {
        if self.native_backdrop_active() {
            self.app_background()
        } else {
            self.header
        }
    }

    /// Controls on the title bar: clear over the native backdrop, where the bar already
    /// carries the ground, else the header.
    pub(crate) fn header_background(&self) -> Color {
        if self.native_backdrop_active() {
            Color::TRANSPARENT
        } else {
            self.header
        }
    }

    /// Suppress only the main-panel shadow when the actual native backdrop is active.
    pub(crate) fn main_panel_shadow(&self) -> Option<Color> {
        self.panel_shadow_for_backdrop(self.native_backdrop_active())
    }

    fn panel_shadow_for_backdrop(&self, active: bool) -> Option<Color> {
        if active && !self.shadow_panels_over_backdrop {
            None
        } else {
            self.panel_shadow
        }
    }

    fn native_backdrop_active(&self) -> bool {
        if !self.blur_background {
            return false;
        }
        #[cfg(target_os = "macos")]
        {
            crate::application::platform::window::backdrop_active()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// Use the theme's inactive header ink, preserving opacity in older themes.
    pub(crate) fn header_inactive_ink(&self, legacy_opacity: f32) -> Color {
        self.header_muted_ink
            .unwrap_or_else(|| self.header_ink.with_alpha(legacy_opacity))
    }

    /// The active editing tool in the header; defaults to the active header ink.
    pub(crate) fn tool_active_ink(&self) -> Color {
        self.roles
            .get("toolActiveInk")
            .copied()
            .unwrap_or_else(|| self.header_active_ink())
    }

    /// Inactive editing tools in the header; defaults to the inactive header ink.
    pub(crate) fn tool_inactive_ink(&self) -> Color {
        self.roles
            .get("toolInactiveInk")
            .copied()
            .unwrap_or_else(|| self.header_inactive_ink(0.5))
    }

    /// Active title-bar controls can use an accent independently of the document title.
    pub(crate) fn header_active_ink(&self) -> Color {
        self.roles
            .get("headerActiveInk")
            .copied()
            .unwrap_or(self.header_ink)
    }

    /// Save-state text can use contrasting ink independently of filled glyph marks.
    /// Themes without these roles retain their existing red and green status colors.
    pub(crate) fn save_status_ink(&self, modified: bool) -> Color {
        let role = if modified { "unsavedInk" } else { "savedInk" };
        self.roles.get(role).copied().unwrap_or_else(|| {
            if modified {
                self.mark("red").unwrap_or_else(|| self.role("warning"))
            } else {
                self.mark("green").unwrap_or(self.text_muted)
            }
        })
    }

    /// The themed background for selected rows, tiles, and controls.
    pub(crate) fn selected_bg(&self) -> Color {
        self.role("controlSelected")
    }

    /// Contrasting ink for selected controls, separate from glyph and sidebar labels.
    pub(crate) fn selected_ink(&self) -> Color {
        self.role("controlSelectedInk")
    }

    /// Selected control keylines retain their previous treatment in older custom themes.
    pub(crate) fn selected_outline(&self, fallback: Color) -> Color {
        self.roles
            .get("controlSelectedOutline")
            .copied()
            .unwrap_or(fallback)
    }

    /// The selected glyph or sidebar label: the theme's `selectedContentInk`, or else GPUI's
    /// yellow mark ink.
    pub(crate) fn selected_content_ink(&self) -> Color {
        self.roles
            .get("selectedContentInk")
            .copied()
            .or_else(|| self.mark("yellow"))
            .unwrap_or_else(|| self.selected_ink())
    }

    /// The ground behind both glyph grids, recessed from the application surface.
    pub(crate) fn grid_bg(&self) -> Color {
        self.grid_background
    }

    /// The themed hard shadow beneath glyph-grid tiles.
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

    /// Proof type can have its own ink without recoloring outlines in the editing canvas.
    pub(crate) fn proof_ink(&self) -> Color {
        self.roles
            .get("proofInk")
            .copied()
            .unwrap_or_else(|| self.editor_ink())
    }

    /// Follow the mark treatment on the small metrics card as well as on glyph tiles.
    pub(crate) fn floating_header_colors(&self, mark: Option<Color>) -> (Color, Color) {
        match mark {
            Some(mark) if self.marks_filled => (mark, self.mark_ink.unwrap_or(self.text)),
            Some(mark) => (self.floating_pane_header_bg(), mark),
            None => (self.floating_pane_header_bg(), self.text),
        }
    }

    /// The neutral for a compact editor control.
    ///
    /// Gray keeps controls quieter than structural outlines and darker than
    /// filled proof type.
    pub(crate) fn editor_control_ink(&self) -> Color {
        self.text_muted
    }

    /// The themed slider rail, separate from its thumb outline.
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
    #[cfg(not(target_arch = "wasm32"))]
    fn native_chrome_follows_the_light_and_dark_theme_families() {
        for id in ["light", "light-gray", "strawberry"] {
            assert_eq!(
                Palette::load(id).window_theme(),
                winit::window::Theme::Light
            );
        }
        for id in ["dark", "dark-gray", "gray", "campfire"] {
            assert_eq!(Palette::load(id).window_theme(), winit::window::Theme::Dark);
        }
    }

    #[test]
    fn legacy_themes_keep_their_optional_appearance_colors() {
        for &id in runebender::ui::theme::BUILTIN_THEME_IDS {
            let mut theme = runebender::ui::theme::load_theme(id).expect("theme");
            for role in [
                "proofInk",
                "headerActiveInk",
                "controlSelectedOutline",
                "savedInk",
                "unsavedInk",
            ] {
                theme.roles.remove(role);
            }
            let palette = Palette::from_theme(&theme);
            assert_eq!(palette.proof_ink(), palette.editor_ink());
            assert_eq!(palette.header_active_ink(), palette.header_ink);
            assert_eq!(palette.selected_outline(palette.outline), palette.outline);
            assert_eq!(
                palette.selected_outline(palette.selected_bg()),
                palette.selected_bg()
            );
            assert_eq!(
                palette.save_status_ink(false),
                palette.mark("green").expect("green mark")
            );
            assert_eq!(
                palette.save_status_ink(true),
                palette.mark("red").expect("red mark")
            );
        }
        let mut palette = Palette::load("gray");
        palette.marks.remove("green");
        palette.marks.remove("red");
        assert_eq!(palette.save_status_ink(false), palette.text_muted);
        assert_eq!(palette.save_status_ink(true), palette.role("warning"));
    }

    #[test]
    fn pale_theme_save_status_uses_its_own_ink_instead_of_tile_colors() {
        for id in ["light", "light-gray", "strawberry"] {
            let palette = Palette::load(id);
            assert_eq!(palette.save_status_ink(false), palette.role("savedInk"));
            assert_eq!(palette.save_status_ink(true), palette.role("unsavedInk"));
            assert_ne!(
                palette.save_status_ink(false),
                palette.mark("green").expect("green mark")
            );
            assert_ne!(
                palette.save_status_ink(true),
                palette.mark("red").expect("red mark")
            );
        }
    }

    #[test]
    fn metrics_headers_follow_the_mark_treatment() {
        for &id in runebender::ui::theme::BUILTIN_THEME_IDS {
            let palette = Palette::load(id);
            let mark = palette.mark("green").expect("green mark");
            let (background, ink) = palette.floating_header_colors(Some(mark));
            if palette.marks_filled {
                assert_eq!(background, mark);
                assert_eq!(Some(ink), palette.mark_ink);
            } else {
                assert_eq!(background, palette.floating_pane_header_bg());
                assert_eq!(ink, mark);
            }
        }
    }

    #[test]
    fn editable_outline_fill_preserves_role_color_at_gpui_opacity() {
        for &theme_id in runebender::ui::theme::BUILTIN_THEME_IDS {
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
    #[test]
    fn side_panels_frost_only_when_the_theme_asks_and_the_backdrop_is_active() {
        let mut palette = Palette::load("gray");
        palette.blur_panels = false;
        // Without the request, panels stay solid whether or not the backdrop is active.
        assert_eq!(palette.side_panel_alpha_for_backdrop(false), 1.0);
        assert_eq!(palette.side_panel_alpha_for_backdrop(true), 1.0);
        palette.blur_panels = true;
        palette.panel_tint_opacity = 0.6;
        // A theme's request takes effect only over the native backdrop.
        assert_eq!(palette.side_panel_alpha_for_backdrop(false), 1.0);
        assert_eq!(palette.side_panel_alpha_for_backdrop(true), 0.6);
    }

    #[test]
    fn translucent_panels_can_hide_shadows_without_affecting_glyph_shadows() {
        let mut palette = Palette::load("gray");
        let glyph_shadow = palette.cell_shadow_color;
        assert!(palette.panel_shadow_for_backdrop(false).is_some());
        assert!(palette.panel_shadow_for_backdrop(true).is_none());
        palette.shadow_panels_over_backdrop = true;
        assert!(palette.panel_shadow_for_backdrop(true).is_some());
        assert_eq!(palette.cell_shadow_color, glyph_shadow);
        palette.panel_shadow = None;
        assert!(palette.panel_shadow_for_backdrop(false).is_none());
        assert!(palette.panel_shadow_for_backdrop(true).is_none());
    }
}
