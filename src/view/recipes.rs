// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The recipes this application repeats.
//!
//! The scale and the containers both live in the framework now:
//! measurements are `xilem::kernel` steps, and a container states its
//! `Region` instead of its gap and inset. What is left here is the small
//! set of compositions a font editor uses over and over (a section, a
//! key/value row, a labeled field, a list row, a toggle, a button), which
//! are candidates for the framework's parts list. Each graduates when a
//! second application needs the same one.

pub(crate) use crate::view::design::{ControlSize, Radius, Region, Space, Stroke, TextSize};
use crate::view::design::{column, row};
use masonry::layout::Dim;
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::{FlexSpacer, button, label, sized_box, text_input};

use crate::Workspace;
use crate::view::theme::Palette;

/// A section header that collapses its section.
///
/// The GPUI build's sidebar groups fold, which matters once a font has
/// four filter groups and a language list: without folding the sidebar
/// is a single scroll of rows with no shape.
pub(crate) fn section_toggle<F>(
    pal: &Palette,
    text: &'static str,
    open: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F>
where
    F: Fn(&mut Workspace) + Send + Sync + 'static,
{
    let muted = pal.text_muted;
    // The small triangles, as the GPUI build paints them; the large
    // ones read as buttons.
    let mark = if open { "\u{25be}" } else { "\u{25b8}" };
    // A stretched row inside the button, because the stock button
    // centres its child and a section header has to sit at the left edge
    // with the rows it heads.
    sized_box(
        button(
            row(
                Region::Inline,
                (
                    label(format!("{mark} {text}"))
                        .text_size(TextSize::Caption.px())
                        .color(muted),
                    FlexSpacer::Flex(1.0),
                ),
            ),
            move |app: &mut Workspace| on_click(app),
        )
        .background_color(pal.panel)
        .border_width(Stroke::None.length())
        .padding(Space::None),
    )
    .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Row)))
}

/// A read-only label/value row: name left, value right, one row tall.
pub(crate) fn kv(pal: &Palette, name: String, value: String) -> impl WidgetView<Workspace> + use<> {
    let (muted, text) = (pal.text_muted, pal.text);
    sized_box(row(
        Region::Inline,
        (
            label(name).text_size(TextSize::Body.px()).color(muted),
            FlexSpacer::Flex(1.0),
            label(value).text_size(TextSize::Body.px()).color(text),
            // Clear of the scroll bar, as in the list rows above.
            FlexSpacer::Fixed(Space::Sm.length()),
        ),
    ))
    .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Row)))
}

/// A bare text field: no caption, a placeholder inside, control
/// height. The kerning row in the GPUI build.
pub(crate) fn field_bare<F, G>(
    pal: &Palette,
    placeholder: &'static str,
    value: String,
    on_change: F,
    on_enter: G,
) -> impl WidgetView<Workspace> + use<F, G>
where
    F: Fn(&mut Workspace, String) + Send + Sync + 'static,
    G: Fn(&mut Workspace, String) + Send + Sync + 'static,
{
    sized_box(
        text_input(value, move |app: &mut Workspace, v| on_change(app, v))
            .on_enter(move |app: &mut Workspace, v| on_enter(app, v))
            .placeholder(placeholder)
            .text_color(pal.text)
            .placeholder_color(pal.text_muted)
            .background_color(pal.field())
            .border_color(pal.field_outline)
            .border_width(Stroke::Hairline.length())
            .corner_radius(Radius::Sm.length()),
    )
    .dims(Dimensions::new(
        Dim::Stretch,
        Dim::from(ControlSize::Control),
    ))
}

/// A labeled text field: caption over a control-height input.
pub(crate) fn field<F>(
    pal: &Palette,
    name: &'static str,
    value: String,
    on_change: F,
) -> impl WidgetView<Workspace> + use<F>
where
    F: Fn(&mut Workspace, String) + Send + Sync + 'static,
{
    column(
        Region::List,
        (
            label(name)
                .text_size(TextSize::Caption.px())
                .color(pal.text_muted),
            sized_box(
                text_input(value, move |app: &mut Workspace, v| on_change(app, v))
                    .text_color(pal.text)
                    .placeholder_color(pal.text_muted)
                    .background_color(pal.field())
                    .border_color(pal.field_outline)
                    .border_width(Stroke::Hairline.length())
                    .corner_radius(Radius::Sm.length()),
            )
            .dims(Dimensions::new(
                Dim::Stretch,
                Dim::from(ControlSize::Control),
            )),
        ),
    )
}

/// A field that commits on Enter as well as reporting every keystroke.
///
/// Some edits cannot run per keystroke. Renaming a glyph rewrites every
/// master and every component reference, so it has to wait for the whole
/// name. Xilem's `text_input` has `on_enter` for exactly this, and the
/// plain [`field`] above does not use it, which is how the editor ended
/// up with a Name box that could be typed in but never applied.
pub(crate) fn field_enter<F, G>(
    pal: &Palette,
    name: &'static str,
    value: String,
    on_change: F,
    on_enter: G,
) -> impl WidgetView<Workspace> + use<F, G>
where
    F: Fn(&mut Workspace, String) + Send + Sync + 'static,
    G: Fn(&mut Workspace, String) + Send + Sync + 'static,
{
    column(
        Region::List,
        (
            label(name)
                .text_size(TextSize::Caption.px())
                .color(pal.text_muted),
            sized_box(
                text_input(value, move |app: &mut Workspace, v| on_change(app, v))
                    .on_enter(move |app: &mut Workspace, v| on_enter(app, v))
                    .text_color(pal.text)
                    .placeholder_color(pal.text_muted)
                    .background_color(pal.field())
                    .border_color(pal.field_outline)
                    .border_width(Stroke::Hairline.length())
                    .corner_radius(Radius::Sm.length()),
            )
            .dims(Dimensions::new(
                Dim::Stretch,
                Dim::from(ControlSize::Control),
            )),
        ),
    )
}

/// A list row: label left, trailing text right, accent outline when active.
/// This is the sidebar row, the layer row, and any future tree row.
pub(crate) fn list_row<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    trailing: String,
    active: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    list_row_marked(pal, Marker::Bullet, false, text, trailing, active, on_click)
}

/// What a sidebar row shows before its label: a chevron on a row that
/// expands, a bullet on a leaf, as the GPUI sidebar has them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Marker {
    Bullet,
    Closed,
    Open,
}

impl Marker {
    fn text(self) -> &'static str {
        match self {
            Self::Bullet => "\u{2022}",
            Self::Closed => "\u{25b8}",
            Self::Open => "\u{25be}",
        }
    }
}

/// A list row with a chosen marker, indented when it sits under
/// another row.
pub(crate) fn list_row_marked<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    marker: Marker,
    indent: bool,
    text: String,
    trailing: String,
    active: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    let text = if indent {
        format!("      {}  {text}", marker.text())
    } else {
        format!("{}  {text}", marker.text())
    };
    let (fg, border, bg) = if active {
        (pal.selected_ink(), pal.selected_bg(), pal.selected_bg())
    } else {
        (pal.text, xilem::Color::TRANSPARENT, pal.panel)
    };
    let trailing_color = if active {
        pal.selected_ink()
    } else {
        pal.text_muted
    };
    sized_box(
        button(
            row(
                Region::Inline,
                (
                    label(text).text_size(TextSize::Body.px()).color(fg),
                    FlexSpacer::Flex(1.0),
                    label(trailing)
                        .text_size(TextSize::Body.px())
                        .color(trailing_color),
                    // Clear of the scroll bar, which a portal draws over
                    // its own right edge rather than beside it. Without
                    // this the counts in the sidebar are cut in half.
                    FlexSpacer::Fixed(Space::Sm.length()),
                ),
            ),
            move |app: &mut Workspace| on_click(app),
        )
        .padding(Space::Sm)
        .background_color(bg)
        .border_color(border)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Radius::Sm.length()),
    )
    .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Row)))
}

/// A toggle at control height that takes the width of its label: the
/// GPUI build's `toggle`, inverted when active.
pub(crate) fn toggle<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    active: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    let (fg, border, bg) = if active {
        (pal.selected_ink(), pal.selected_bg(), pal.selected_bg())
    } else {
        (pal.text, pal.outline, pal.panel)
    };
    sized_box(
        button(
            label(text).text_size(TextSize::Body.px()).color(fg),
            move |app: &mut Workspace| on_click(app),
        )
        .padding(Space::Md)
        .background_color(bg)
        .border_color(border)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Radius::Sm.length()),
    )
    .dims(Dimensions::new(Dim::Auto, Dim::from(ControlSize::Control)))
}

/// A square toggle at a chosen size.
pub(crate) fn toggle_sized<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    active: bool,
    size: ControlSize,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    let (fg, border, bg) = if active {
        (pal.selected_ink(), pal.selected_bg(), pal.selected_bg())
    } else {
        (pal.text_muted, pal.outline, pal.panel)
    };
    sized_box(
        button(
            label(text).text_size(TextSize::Caption.px()).color(fg),
            move |app: &mut Workspace| on_click(app),
        )
        .padding(Space::None)
        .padding(Space::Sm)
        .background_color(bg)
        .border_color(border)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Radius::Sm.length()),
    )
    .dims(Dimensions::fixed(size.length(), size.length()))
}

/// A labeled push button at control height.
pub(crate) fn action<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    sized_box(
        button(
            label(text).text_size(TextSize::Body.px()).color(pal.text),
            move |app: &mut Workspace| on_click(app),
        )
        .background_color(pal.button)
        .corner_radius(Radius::Md.length()),
    )
    .dims(Dimensions::new(Dim::Auto, Dim::from(ControlSize::Control)))
}
