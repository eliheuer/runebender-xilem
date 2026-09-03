// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The design system this application has to carry itself.
//!
//! Upstream Xilem takes a number wherever a measurement is needed, so an
//! application that wants a consistent interface has to bring its own
//! vocabulary. This module is that vocabulary: a closed spacing scale, a
//! control-size scale, radii, strokes, a type scale, and a table of what
//! each kind of container measures.
//!
//! It is about four hundred lines, none of it about fonts, and every
//! application on Xilem needs its own version. That is the point of the
//! comparison with runebender-gpui: gpui ships the scale (`px_1`..`px_4`,
//! `text_xs`, `rounded_md`) and `gpui-component` ships the styled parts,
//! so the equivalent file there does not exist.
//!
//! The types are the ones prototyped in the xix fork
//! (github.com/eliheuer/xix), kept here in application code so this
//! editor builds against upstream Xilem unmodified.

use masonry::layout::{Dim, Length};
use masonry::properties::types::CrossAxisAlignment;
use masonry::properties::{Gap, Padding};
use xilem::style::Style;
use xilem::view::{Flex, FlexSequence, Prop, flex_col, flex_row};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[expect(
    dead_code,
    reason = "the scale is a closed vocabulary: a size nothing uses yet is still part of it"
)]
pub(crate) enum Space {
    /// No space.
    None,
    /// 2 px. Inside a control, between an icon and its label.
    Xs,
    /// 4 px. Between rows of one list.
    Sm,
    /// 8 px. Between controls in a group. The default.
    #[default]
    Md,
    /// 12 px. Panel inset, and between groups.
    Lg,
    /// 16 px. Between sections.
    Xl,
    /// 24 px. Between regions of a window.
    Xxl,
}

impl Space {
    /// The step, in logical pixels.
    pub(crate) const fn px(self) -> f64 {
        match self {
            Self::None => 0.0,
            Self::Xs => 2.0,
            Self::Sm => 4.0,
            Self::Md => 8.0,
            Self::Lg => 12.0,
            Self::Xl => 16.0,
            Self::Xxl => 24.0,
        }
    }

    /// The step as a [`Length`].
    pub(crate) const fn length(self) -> Length {
        match Length::try_px(self.px()) {
            Some(length) => length,
            None => Length::ZERO,
        }
    }
}

/// The control-height scale: how tall an interactive thing is.
///
/// Named `ControlSize`, not `Size`, because [`masonry::kurbo::Size`]
/// is everywhere in this stack and a clash there is worse than a longer
/// name here.
///
/// Heights are what make a panel read as tidy or ragged, and they are the
/// first thing an author invents a number for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[expect(
    dead_code,
    reason = "the scale is a closed vocabulary: a size nothing uses yet is still part of it"
)]
pub(crate) enum ControlSize {
    /// 10 px. A dot in a picker, a status light.
    Dot,
    /// 16 px. A color swatch or a chip.
    Swatch,
    /// 20 px. An icon, a small thumbnail.
    Icon,
    /// 22 px. A row in a dense list, sidebar, or tree.
    Row,
    /// 28 px. A text field, a button, a toggle. The default.
    #[default]
    Control,
    /// 36 px. A primary control, or a touch target.
    Large,
    /// 44 px. The minimum comfortable touch target.
    Touch,
}

impl ControlSize {
    /// The height, in logical pixels.
    pub(crate) const fn px(self) -> f64 {
        match self {
            Self::Dot => 10.0,
            Self::Swatch => 16.0,
            Self::Icon => 20.0,
            Self::Row => 21.0,
            Self::Control => 28.0,
            Self::Large => 36.0,
            Self::Touch => 44.0,
        }
    }

    /// The height as a [`Length`].
    pub(crate) const fn length(self) -> Length {
        match Length::try_px(self.px()) {
            Some(length) => length,
            None => Length::ZERO,
        }
    }
}

/// The stroke scale: border and outline widths.
///
/// A border is either there or emphasized. Three values is already
/// generous, and it keeps 1.5 px hairlines from appearing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[expect(
    dead_code,
    reason = "the scale is a closed vocabulary: a size nothing uses yet is still part of it"
)]
pub(crate) enum Stroke {
    /// No border.
    None,
    /// 1 px. The default border.
    #[default]
    Hairline,
    /// 2 px. An emphasized or focused border.
    Thick,
}

impl Stroke {
    /// The width, in logical pixels.
    pub(crate) const fn px(self) -> f64 {
        match self {
            Self::None => 0.0,
            Self::Hairline => 1.0,
            Self::Thick => 2.0,
        }
    }

    /// The width as a [`Length`].
    pub(crate) const fn length(self) -> Length {
        match Length::try_px(self.px()) {
            Some(length) => length,
            None => Length::ZERO,
        }
    }
}

impl From<Stroke> for Length {
    fn from(stroke: Stroke) -> Self {
        stroke.length()
    }
}

/// The corner scale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[expect(
    dead_code,
    reason = "the scale is a closed vocabulary: a size nothing uses yet is still part of it"
)]
pub(crate) enum Radius {
    /// Square corners.
    None,
    /// 4 px. Rows, fields, small controls. The default.
    #[default]
    Sm,
    /// 6 px. Buttons and chips.
    Md,
    /// 10 px. Cards and grid cells.
    Lg,
    /// 16 px. Panels and dialogs.
    Xl,
    /// A pill: half of the shorter side, as far as a length can say it.
    Full,
}

impl Radius {
    /// The radius, in logical pixels.
    pub(crate) const fn px(self) -> f64 {
        match self {
            Self::None => 0.0,
            Self::Sm => 4.0,
            Self::Md => 6.0,
            Self::Lg => 10.0,
            Self::Xl => 16.0,
            Self::Full => 9999.0,
        }
    }

    /// The radius as a [`Length`].
    pub(crate) const fn length(self) -> Length {
        match Length::try_px(self.px()) {
            Some(length) => length,
            None => Length::ZERO,
        }
    }
}

/// The type scale. Two sizes carry most of an interface.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[expect(
    dead_code,
    reason = "the scale is a closed vocabulary: a size nothing uses yet is still part of it"
)]
pub(crate) enum TextSize {
    /// 11 px. Section headers, counts, hints.
    Caption,
    /// 12 px. Rows, labels, fields. The default.
    #[default]
    Body,
    /// 14 px. A window or panel title.
    Title,
    /// 18 px. The one heading on a screen.
    Heading,
    /// 24 px. A display number or an empty-state line.
    Display,
}

impl TextSize {
    /// The size, in logical pixels.
    pub(crate) const fn px(self) -> f32 {
        match self {
            // One size, the GPUI build's 13px, whatever the role: the
            // scale is kept as names so a role can move later, but a
            // window with three sizes in its chrome reads as three
            // windows.
            Self::Caption | Self::Body | Self::Title => 13.0,
            Self::Heading => 18.0,
            Self::Display => 24.0,
        }
    }
}

impl From<Space> for Length {
    fn from(space: Space) -> Self {
        space.length()
    }
}

impl From<Space> for Padding {
    fn from(space: Space) -> Self {
        Self::from(space.length())
    }
}

impl From<ControlSize> for Length {
    fn from(size: ControlSize) -> Self {
        size.length()
    }
}

impl From<ControlSize> for Dim {
    fn from(size: ControlSize) -> Self {
        Self::from(size.length())
    }
}

impl From<Radius> for Length {
    fn from(radius: Radius) -> Self {
        radius.length()
    }
}

impl From<TextSize> for f32 {
    fn from(size: TextSize) -> Self {
        size.px()
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Region {
    /// A panel, sidebar, or inspector. The default for a pane of content.
    #[default]
    Panel,
    /// A titled group of related controls inside a panel.
    Section,
    /// A card in a collection.
    Card,
    /// A form: labeled fields down the page.
    Form,
    /// A toolbar, header, or status bar.
    Toolbar,
    /// A run of controls that belong to one another, like a button pair.
    Inline,
    /// A dense list, tree, or table: rows, not paragraphs.
    List,
}

impl Region {
    /// The space between children of this region.
    pub(crate) const fn gap(self) -> Space {
        match self {
            Self::Panel => Space::Lg,
            Self::Section => Space::Md,
            Self::Card => Space::Sm,
            Self::Form => Space::Md,
            Self::Toolbar => Space::Md,
            Self::Inline => Space::Sm,
            Self::List => Space::Xs,
        }
    }

    /// The space between this region's edge and its children.
    ///
    /// A region that groups without drawing anything (a form, a list, an
    /// inline run) has no inset: its parent already provided one, and a
    /// second one doubles the margin. This is the rule that removes the
    /// margin conversation.
    pub(crate) const fn inset(self) -> Space {
        match self {
            Self::Panel => Space::Lg,
            Self::Section => Space::None,
            Self::Card => Space::Md,
            Self::Toolbar => Space::Md,
            Self::Form | Self::Inline | Self::List => Space::None,
        }
    }
}
// --- conversions into the property types -------------------------------
//
// These are what let a call site say `.gap(Space::Md)` instead of a
// number. Upstream's `gap` and `padding` take `impl Into<_>`, so the
// conversions are enough; `corner_radius` and `border_width` take a
// `Length`, so those call sites say `Radius::Sm.length()`.

impl From<Space> for Gap {
    fn from(space: Space) -> Self {
        Self {
            gap: space.length(),
        }
    }
}

/// What [`column()`] and [`row()`] return: a flex container with its gap and
/// inset already set from its [`Region`]. Named rather than hidden behind
/// `impl WidgetView` so the result stays styleable.
pub(crate) type RegionStack<Seq, State, Action> =
    Prop<Padding, Prop<Gap, Flex<Seq, State, Action>, State, Action>, State, Action>;

/// A vertical container whose spacing comes from its [`Region`].
pub(crate) fn column<State, Action, Seq>(
    region: Region,
    children: Seq,
) -> RegionStack<Seq, State, Action>
where
    State: 'static,
    Action: 'static,
    Seq: FlexSequence<State, Action> + Send + Sync,
{
    stack(flex_col(children), region, CrossAxisAlignment::Stretch)
}

/// A horizontal container whose spacing comes from its [`Region`].
pub(crate) fn row<State, Action, Seq>(
    region: Region,
    children: Seq,
) -> RegionStack<Seq, State, Action>
where
    State: 'static,
    Action: 'static,
    Seq: FlexSequence<State, Action> + Send + Sync,
{
    stack(flex_row(children), region, CrossAxisAlignment::Center)
}

fn stack<State, Action, Seq>(
    flex: Flex<Seq, State, Action>,
    region: Region,
    alignment: CrossAxisAlignment,
) -> RegionStack<Seq, State, Action>
where
    State: 'static,
    Action: 'static,
    Seq: FlexSequence<State, Action> + Send + Sync,
{
    flex.cross_axis_alignment(alignment)
        .gap(region.gap())
        .padding(region.inset())
}
