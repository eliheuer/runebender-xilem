// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! How every pressable control looks in every state.
//!
//! A control names what it is for with a [`ControlKind`], and this table
//! answers what it looks like at rest, under the pointer, pressed,
//! selected, disabled, and with keyboard focus. Nothing else in the view
//! layer chooses a control's colors, so a new control gets every state
//! by naming a kind.
//!
//! The theme sets hover and pressed once, on the plain button: the step
//! from `surfaces.button` to `surfaces.buttonHover`, and to
//! `surfaces.buttonPressed`. Every kind takes the same lightness step
//! from its own rest face, so an inverted Primary button lightens on
//! hover by as much as an outlined one, and a bare control in the dark
//! header by as much as one on a light panel.

use masonry::peniko::color::Oklab;
use xilem::Color;

use crate::application::design::theme::Palette;
use crate::application::design::{Alpha, ButtonShape, ControlSize};

/// What a pressable control is for. Closed: when a feature needs a kind
/// that is not here, add it once, with a reason, and every view has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ControlKind {
    /// The one thing a section is for. Inverted face.
    Primary,
    /// The default button: the button face inside an outline.
    Secondary,
    /// Rare or reversible actions: muted text with no face until hovered.
    Quiet,
    /// One of a few choices, or an on/off switch: outlined at rest,
    /// inverted when selected.
    Toggle,
    /// A word in a set of tags: a [`ControlKind::Toggle`] shaped as a
    /// pill at swatch height.
    Chip,
    /// A row in a list: no face until hovered, inverted when selected.
    Row,
    /// A section header that opens and closes: muted, and never a face.
    /// It sits inside the group's inset, so a fill would stop short of the
    /// rules and edges around it; hover and press strengthen the ink instead.
    Disclosure,
    /// A control whose content is its face, such as a color swatch or a
    /// reference dot: the content covers the fill, so hover and pressed
    /// show as a ring, and the shape is a circle.
    Swatch,
    /// A bare icon or word in a bar that does something: full ink, no
    /// face until hovered.
    Bare,
    /// A bare icon in a bar that is on or off, such as an editing tool or
    /// a view switch: quiet ink when off, full ink when on, no face until
    /// hovered. It strengthens instead of inverting so a bar of tools
    /// stays quiet.
    Tool,
    /// A sidebar rail tab: the tab shape is its face, so hover and press
    /// show in the ink, and the open tab is the panel showing through.
    Tab,
}

/// The surface a control sits on. Kinds without a fill of their own
/// take their hover and pressed faces from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum Ground {
    /// Panels, sidebars, and the center column's bars.
    #[default]
    Panel,
    /// The dark window header with the document tabs and tools.
    Header,
}

/// The colors of one state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Face {
    /// The face behind the content; transparent when the ground shows.
    pub(crate) fill: Color,
    /// The outline around the face; transparent for none.
    pub(crate) outline: Color,
    /// The color of the control's text and icons.
    pub(crate) ink: Color,
}

/// One control's resolved look: a face per state, the focus ring, and
/// its shape. Build it with [`ControlStyle::new`] and the builders, then
/// hand it to the control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ControlStyle {
    kind: ControlKind,
    ground: Ground,
    selected: bool,
    disabled: bool,
    rest: Face,
    hover: Face,
    pressed: Face,
    focus_ring: Color,
    radius: f64,
    stroke: f64,
}

impl ControlStyle {
    /// The look of an unselected, enabled control of `kind` on a panel.
    pub(crate) fn new(pal: &Palette, kind: ControlKind) -> Self {
        Self::resolve(pal, kind, Ground::Panel, false, false)
    }

    /// The same control drawn selected, or not.
    #[must_use]
    pub(crate) fn selected(self, pal: &Palette, selected: bool) -> Self {
        Self::resolve(pal, self.kind, self.ground, selected, self.disabled)
    }

    /// The same control drawn disabled, or not.
    #[must_use]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "disabled is a state of every control, before any view disables one"
        )
    )]
    pub(crate) fn disabled(self, pal: &Palette, disabled: bool) -> Self {
        Self::resolve(pal, self.kind, self.ground, self.selected, disabled)
    }

    /// The same control on another ground.
    #[must_use]
    pub(crate) fn on(self, pal: &Palette, ground: Ground) -> Self {
        Self::resolve(pal, self.kind, ground, self.selected, self.disabled)
    }

    /// A bare control painted by hand on a surface the theme does not name,
    /// such as a graph node's colored title bar. It rests as `ink` on
    /// `fill`, and hover and press shade `fill` by the theme's button steps.
    pub(crate) fn on_surface(pal: &Palette, fill: Color, ink: Color) -> Self {
        let mut style = Self::resolve(pal, ControlKind::Bare, Ground::Panel, false, false);
        let face = |fill| Face {
            fill,
            outline: Color::TRANSPARENT,
            ink,
        };
        style.rest = face(Color::TRANSPARENT);
        style.hover = face(shift(
            fill,
            lightness(pal.button_hover) - lightness(pal.button),
        ));
        style.pressed = face(shift(
            fill,
            lightness(pal.button_pressed) - lightness(pal.button),
        ));
        style
    }

    fn resolve(
        pal: &Palette,
        kind: ControlKind,
        ground: Ground,
        selected: bool,
        disabled: bool,
    ) -> Self {
        let none = Color::TRANSPARENT;
        let (outlined, inverted) = match ground {
            Ground::Panel => (
                Face {
                    fill: pal.button,
                    outline: pal.outline,
                    ink: pal.text,
                },
                Face {
                    fill: pal.selected_bg(),
                    outline: pal.selected_outline(pal.outline),
                    ink: pal.selected_ink(),
                },
            ),
            // The header is dark in every theme: an outlined control is
            // keylined in the header's quiet ink, and the selected one
            // brightens its ink and keyline instead of inverting.
            Ground::Header => (
                Face {
                    fill: pal.header_background(),
                    outline: pal.header_inactive_ink(Alpha::HeaderKeyline.value()),
                    ink: pal.header_inactive_ink(Alpha::HeaderInk.value()),
                },
                Face {
                    fill: pal.header_background(),
                    outline: pal.header_active_ink(),
                    ink: pal.header_active_ink(),
                },
            ),
        };
        let (ground_fill, bare_ink, bare_strong) = match ground {
            Ground::Panel => (pal.panel, pal.text_subdued, pal.editor_control_ink()),
            Ground::Header => (pal.header, pal.tool_inactive_ink(), pal.tool_active_ink()),
        };
        let bare = |ink| Face {
            fill: none,
            outline: none,
            ink,
        };
        let rest = match (kind, selected) {
            (ControlKind::Primary, _) => inverted,
            (ControlKind::Secondary, _) => outlined,
            (ControlKind::Quiet, _) => bare(pal.text_muted),
            (ControlKind::Toggle | ControlKind::Chip, false) => outlined,
            (ControlKind::Toggle | ControlKind::Chip, true) => inverted,
            (ControlKind::Row, false) => bare(pal.text),
            (ControlKind::Row, true) => Face {
                fill: pal.selected_bg(),
                outline: pal.outline,
                ink: pal.selected_content_ink(),
            },
            (ControlKind::Disclosure, _) => bare(pal.text_muted),
            // An opaque ground, so lines drawn under a swatch stop at its edge.
            (ControlKind::Swatch, _) => Face {
                fill: ground_fill,
                outline: none,
                ink: pal.text,
            },
            (ControlKind::Bare, _) => bare(bare_strong),
            (ControlKind::Tool, false) => bare(bare_ink),
            (ControlKind::Tool, true) => bare(bare_strong),
            (ControlKind::Tab, false) => Face {
                fill: pal.inactive_tab,
                outline: pal.outline,
                ink: pal.inactive_tab_ink,
            },
            (ControlKind::Tab, true) => Face {
                fill: Palette::FLAT,
                outline: pal.outline,
                ink: pal.active_tab_ink,
            },
        };
        // A face with no fill shows the ground, so its states start there.
        let base = if rest.fill.components[3] == 0.0 {
            ground_fill
        } else {
            rest.fill
        };
        let hover_step = lightness(pal.button_hover) - lightness(pal.button);
        let pressed_step = lightness(pal.button_pressed) - lightness(pal.button);
        let (hover, pressed) = if kind == ControlKind::Tab {
            // The tab shape stays; the icon brightens to the open tab's ink.
            let lit = Face {
                ink: pal.active_tab_ink,
                ..rest
            };
            (
                lit,
                Face {
                    fill: shift(base, pressed_step),
                    ..lit
                },
            )
        } else if kind == ControlKind::Disclosure {
            // No fill: the header reads as part of the group, and the ink
            // answers the pointer.
            (
                Face {
                    ink: pal.text,
                    ..rest
                },
                Face {
                    ink: pal.text_subdued,
                    ..rest
                },
            )
        } else if kind == ControlKind::Swatch {
            // The content covers any fill, so the states draw on the edge.
            (
                Face {
                    outline: pal.text_muted,
                    ..rest
                },
                Face {
                    outline: pal.text,
                    ..rest
                },
            )
        } else {
            (
                Face {
                    fill: shift(base, hover_step),
                    ..rest
                },
                Face {
                    fill: shift(base, pressed_step),
                    ..rest
                },
            )
        };
        let (rest, hover, pressed) = if disabled {
            let quiet = |face: Face| Face {
                ink: face.ink.with_alpha(Alpha::Disabled.value()),
                outline: face.outline.with_alpha(Alpha::Disabled.value()),
                ..face
            };
            // A disabled control does not answer the pointer.
            (quiet(rest), quiet(rest), quiet(rest))
        } else {
            (rest, hover, pressed)
        };
        let radius = match kind {
            // Sidebar selection rows use half the control radius, as the theme documents.
            ControlKind::Row | ControlKind::Disclosure => pal.control_radius * 0.5,
            ControlKind::Chip => ControlSize::Swatch.px() / 2.0,
            ControlKind::Swatch => ButtonShape::Circular.radius().get(),
            _ => pal.control_radius,
        };
        Self {
            kind,
            ground,
            selected,
            disabled,
            rest,
            hover,
            pressed,
            focus_ring: pal.focus_ring,
            radius,
            stroke: pal.stroke,
        }
    }

    /// The face for the pointer's relation to the control.
    pub(crate) fn face(&self, hovered: bool, pressed: bool) -> Face {
        match (hovered, pressed) {
            (true, true) => self.pressed,
            (true, false) => self.hover,
            // A press dragged off the control shows it will not click.
            (false, _) => self.rest,
        }
    }

    /// The ink for the control's content: text, icons, and markers.
    pub(crate) fn ink(&self) -> Color {
        self.rest.ink
    }

    /// A quieter ink for secondary content in the same control, such as a
    /// row's trailing count.
    pub(crate) fn secondary_ink(&self, pal: &Palette) -> Color {
        if self.selected {
            self.rest.ink
        } else {
            pal.text_muted
        }
    }

    /// The color of the keyboard focus ring.
    pub(crate) fn focus_ring(&self) -> Color {
        self.focus_ring
    }

    /// The corner radius.
    pub(crate) fn radius(&self) -> f64 {
        self.radius
    }

    /// The outline width.
    pub(crate) fn stroke(&self) -> f64 {
        self.stroke
    }

    /// Whether the control is drawn disabled.
    pub(crate) fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Whether the control is drawn selected.
    pub(crate) fn is_selected(&self) -> bool {
        self.selected
    }

    /// Whether the kind is an on/off control, which assistive technology
    /// reports as toggled.
    pub(crate) fn is_selectable(&self) -> bool {
        matches!(
            self.kind,
            ControlKind::Toggle
                | ControlKind::Chip
                | ControlKind::Row
                | ControlKind::Tool
                | ControlKind::Tab
        )
    }
}

/// Oklab lightness, 0 to 1.
fn lightness(color: Color) -> f32 {
    color.convert::<Oklab>().components[0]
}

/// `color` made lighter by `step` in Oklab lightness, or darker for a negative step.
fn shift(color: Color, step: f32) -> Color {
    color.map_lightness(|l| (l + step).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [ControlKind; 11] = [
        ControlKind::Primary,
        ControlKind::Secondary,
        ControlKind::Quiet,
        ControlKind::Toggle,
        ControlKind::Chip,
        ControlKind::Row,
        ControlKind::Disclosure,
        ControlKind::Swatch,
        ControlKind::Bare,
        ControlKind::Tool,
        ControlKind::Tab,
    ];

    #[test]
    fn every_kind_in_every_theme_shows_hover_and_pressed() {
        for id in crate::application::platform::themes::catalog().ids() {
            let pal = Palette::load(id);
            for kind in KINDS {
                for selected in [false, true] {
                    for ground in [Ground::Panel, Ground::Header] {
                        let style = ControlStyle::new(&pal, kind)
                            .on(&pal, ground)
                            .selected(&pal, selected);
                        let rest = style.face(false, false);
                        let hover = style.face(true, false);
                        let pressed = style.face(true, true);
                        // The open tab already shows the lit ink, so only its press shows.
                        if !(kind == ControlKind::Tab && selected) {
                            assert_ne!(rest, hover, "{id} {kind:?} selected={selected} hover");
                        }
                        assert_ne!(hover, pressed, "{id} {kind:?} selected={selected} pressed");
                        assert_ne!(rest, pressed, "{id} {kind:?} selected={selected} press");
                    }
                }
            }
        }
    }

    #[test]
    fn a_control_on_a_colored_surface_shades_that_surface() {
        for id in crate::application::platform::themes::catalog().ids() {
            let pal = Palette::load(id);
            let fill = pal.selected_bg();
            let style = ControlStyle::on_surface(&pal, fill, pal.text);
            assert_eq!(style.face(false, false).fill, Color::TRANSPARENT);
            let hover = style.face(true, false).fill;
            let pressed = style.face(true, true).fill;
            assert_ne!(hover, fill);
            assert_ne!(pressed, hover);
            assert_eq!(style.face(true, false).ink, pal.text);
        }
    }

    #[test]
    fn a_disabled_control_does_not_answer_the_pointer() {
        let pal = Palette::load("gray");
        let style = ControlStyle::new(&pal, ControlKind::Secondary).disabled(&pal, true);
        assert_eq!(style.face(false, false), style.face(true, false));
        assert_eq!(style.face(false, false), style.face(true, true));
        assert!(style.ink().components[3] < pal.text.components[3]);
    }

    #[test]
    fn selected_toggles_and_primaries_share_the_inverted_face() {
        let pal = Palette::load("gray");
        let primary = ControlStyle::new(&pal, ControlKind::Primary);
        let toggle = ControlStyle::new(&pal, ControlKind::Toggle).selected(&pal, true);
        assert_eq!(primary.face(false, false), toggle.face(false, false));
    }
}
