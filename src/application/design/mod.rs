// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The design system: start here for any change to how the window looks.
//!
//! Every color, shape, and interaction state comes from three layers.
//! Code outside this folder never chooses a color or a size by itself.
//!
//! 1. The theme files, `assets/themes/default/*.theme.toml`, hold every
//!    color and shape value. The font library parses them in
//!    `src/ui/theme.rs`, which stays outside the application so that
//!    headless tools can read themes without Xilem.
//! 2. This folder turns the theme into a system:
//!    - [`theme`]: the `Palette`, the theme as the views read it.
//!    - `tokens`: the scales for spacing, control sizes, strokes,
//!      text sizes, and opacities ([`Alpha`]).
//!    - [`states`]: each pressable control's look at rest, under the
//!      pointer, pressed, selected, disabled, and with keyboard focus. A
//!      control names a [`ControlKind`], never a color.
//!    - [`controls`]: the widgets that draw those states.
//!    - [`stock`]: the stock Masonry widgets dressed in the theme, so a
//!      plain `label`, `text_input`, or `slider` matches without settings.
//!    - [`recipes`]: the shared building blocks, such as panel rows,
//!      fields, and swatches.
//! 3. [`areas`] holds each place where an area of the window departs from
//!    the base, with the reason. Code uses the value from there.
//!
//! To add a feature, build it from [`recipes`] and
//! [`controls::control::control`] with a [`ControlStyle`]. Set only size
//! and padding at the call site. If the base does not fit, add one
//! override to [`areas`] with its reason.
//!
//! `tests/design_system.rs` fails the build when code outside this folder
//! sets a face, a color, an opacity, a stroke, a radius, or a text size
//! by itself.

pub(crate) mod areas;
pub(crate) mod controls;
pub(crate) mod recipes;
pub(crate) mod states;
pub(crate) mod stock;
pub(crate) mod theme;
mod tokens;

pub(crate) use states::{ControlKind, ControlStyle, Ground};
pub(crate) use tokens::*;
