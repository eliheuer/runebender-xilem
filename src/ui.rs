// Copyright 2026 the Runebender Xix Authors
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

pub use crate::design::{ControlSize, Radius, Region, Space, Stroke, TextSize};
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use crate::design::{column, row};
use xilem::view::{FlexExt as _, FlexSpacer, button, label, sized_box, text_input};

use crate::App;
use crate::theme::Palette;


/// A section header: caption type, muted. No disclosure caret: the
/// framework has no icon set and the default font has no coverage for
/// the geometric shapes block, so the caret rendered as tofu.
pub fn section_header(pal: &Palette, text: &'static str) -> impl WidgetView<App> + use<> {
    label(text.to_string())
        .text_size(TextSize::Caption.px())
        .color(pal.text_muted)
}

/// A section header that collapses its section.
///
/// The GPUI build's sidebar groups fold, which matters once a font has
/// four filter groups and a language list: without folding the sidebar
/// is a single scroll of rows with no shape.
pub fn section_toggle<F>(
    pal: &Palette,
    text: &'static str,
    open: bool,
    on_click: F,
) -> impl WidgetView<App> + use<F>
where
    F: Fn(&mut App) + Send + Sync + 'static,
{
    let muted = pal.text_muted;
    let mark = if open { "\u{25bc}" } else { "\u{25b6}" };
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
            move |app: &mut App| on_click(app),
        )
        .background_color(pal.panel)
        .border_width(Stroke::None.length())
        .padding(Space::None),
    )
    .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Row)))
}

/// A section: header over body, with the section gap under it.
pub fn section<V: WidgetView<App> + 'static>(
    pal: &Palette,
    title: &'static str,
    body: V,
) -> impl WidgetView<App> + use<V> {
    column(Region::Section, (section_header(pal, title), body))
}

/// A read-only label/value row: name left, value right, one row tall.
pub fn kv(pal: &Palette, name: String, value: String) -> impl WidgetView<App> + use<> {
    let (muted, text) = (pal.text_muted, pal.text);
    sized_box(row(
        Region::Inline,
        (
            label(name).text_size(TextSize::Body.px()).color(muted),
            FlexSpacer::Flex(1.0),
            label(value).text_size(TextSize::Body.px()).color(text),
        ),
    ))
    .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Row)))
}

/// A labeled text field: caption over a control-height input.
pub fn field<F>(
    pal: &Palette,
    name: &'static str,
    value: String,
    on_change: F,
) -> impl WidgetView<App> + use<F>
where
    F: Fn(&mut App, String) + Send + Sync + 'static,
{
    column(
        Region::List,
        (
            label(name).text_size(TextSize::Caption.px()).color(pal.text_muted),
            sized_box(
                text_input(value, move |app: &mut App, v| on_change(app, v))
                    .background_color(pal.field())
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
pub fn field_enter<F, G>(
    pal: &Palette,
    name: &'static str,
    value: String,
    on_change: F,
    on_enter: G,
) -> impl WidgetView<App> + use<F, G>
where
    F: Fn(&mut App, String) + Send + Sync + 'static,
    G: Fn(&mut App, String) + Send + Sync + 'static,
{
    column(
        Region::List,
        (
            label(name).text_size(TextSize::Caption.px()).color(pal.text_muted),
            sized_box(
                text_input(value, move |app: &mut App, v| on_change(app, v))
                    .on_enter(move |app: &mut App, v| on_enter(app, v))
                    .background_color(pal.field())
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
pub fn list_row<F: Fn(&mut App) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    trailing: String,
    active: bool,
    on_click: F,
) -> impl WidgetView<App> + use<F> {
    let (fg, border) = if active {
        (pal.role("accent"), pal.role("accent"))
    } else {
        (pal.text, xilem::Color::TRANSPARENT)
    };
    let trailing_color = if active { pal.role("accent") } else { pal.text_muted };
    sized_box(
        button(
            row(
                Region::Inline,
                (
                    label(text).text_size(TextSize::Body.px()).color(fg),
                    FlexSpacer::Flex(1.0),
                    label(trailing).text_size(TextSize::Body.px()).color(trailing_color),
                ),
            ),
            move |app: &mut App| on_click(app),
        )
        .background_color(pal.panel)
        .border_color(border)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Radius::Sm.length()),
    )
    .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Row)))
}

/// A list row with a leading icon.
///
/// The icon gets its own fixed-width column rather than being glued to
/// the label. It has to: a script icon like `\u{0636}` is right to left,
/// and inside one string the bidi algorithm moves it to the other end,
/// so "icon then name" renders as "name then icon". A separate box is
/// also how the GPUI build lays it out.
pub fn list_row_with_icon<F: Fn(&mut App) + Send + Sync + 'static>(
    pal: &Palette,
    icon: String,
    text: String,
    trailing: String,
    active: bool,
    on_click: F,
) -> impl WidgetView<App> + use<F> {
    let (fg, border) = if active {
        (pal.role("accent"), pal.role("accent"))
    } else {
        (pal.text, xilem::Color::TRANSPARENT)
    };
    let trailing_color = if active { pal.role("accent") } else { pal.text_muted };
    let icon_color = if active { pal.role("accent") } else { pal.text_muted };
    sized_box(
        button(
            row(
                Region::Inline,
                (
                    sized_box(label(icon).text_size(TextSize::Body.px()).color(icon_color))
                        .dims(Dimensions::fixed(
                            ControlSize::Swatch.length(),
                            ControlSize::Row.length(),
                        )),
                    label(text).text_size(TextSize::Body.px()).color(fg),
                    FlexSpacer::Flex(1.0),
                    label(trailing)
                        .text_size(TextSize::Body.px())
                        .color(trailing_color),
                ),
            ),
            move |app: &mut App| on_click(app),
        )
        .background_color(pal.panel)
        .border_color(border)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Radius::Sm.length()),
    )
    .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Row)))
}

/// A list row with a small action button after it.
///
/// The row and the button are separate targets on purpose: one selects,
/// one writes. The row's width is `Auto` plus `flex`, not `Stretch`,
/// because a stretched child in a flex row claims the whole width and
/// pushes the button off the edge.
pub fn list_row_with_action<F, G>(
    pal: &Palette,
    text: String,
    trailing: String,
    active: bool,
    on_click: F,
    action: String,
    on_action: G,
) -> impl WidgetView<App> + use<F, G>
where
    F: Fn(&mut App) + Send + Sync + 'static,
    G: Fn(&mut App) + Send + Sync + 'static,
{
    let (fg, border) = if active {
        (pal.role("accent"), pal.role("accent"))
    } else {
        (pal.text, xilem::Color::TRANSPARENT)
    };
    let trailing_color = if active { pal.role("accent") } else { pal.text_muted };
    row(
        Region::List,
        (
            button(
                row(
                    Region::Inline,
                    (
                        label(text).text_size(TextSize::Body.px()).color(fg),
                        FlexSpacer::Flex(1.0),
                        label(trailing)
                            .text_size(TextSize::Body.px())
                            .color(trailing_color),
                    ),
                ),
                move |app: &mut App| on_click(app),
            )
            .background_color(pal.panel)
            .border_color(border)
            .border_width(Stroke::Hairline.length())
            .corner_radius(Radius::Sm.length())
            .dims(Dimensions::new(Dim::Auto, Dim::from(ControlSize::Row)))
            .flex(1.0),
            // Icon-sized, not control-sized: a control-sized button here
            // puts the row's intrinsic width past the sidebar and clips
            // the button it was meant to add.
            toggle_sized(pal, action, false, ControlSize::Icon, on_action),
        ),
    )
}

/// A square toggle: the small A / Aa / eye controls beside a field.
pub fn toggle<F: Fn(&mut App) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    active: bool,
    on_click: F,
) -> impl WidgetView<App> + use<F> {
    toggle_sized(pal, text, active, ControlSize::Control, on_click)
}

/// A square toggle at a chosen size.
pub fn toggle_sized<F: Fn(&mut App) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    active: bool,
    size: ControlSize,
    on_click: F,
) -> impl WidgetView<App> + use<F> {
    let (fg, border) = if active {
        (pal.role("accent"), pal.role("accent"))
    } else {
        (pal.text_muted, pal.role("gridBorder").with_alpha(0.6))
    };
    sized_box(
        button(
            label(text).text_size(TextSize::Caption.px()).color(fg),
            move |app: &mut App| on_click(app),
        )
        .padding(Space::None)
        .background_color(pal.panel)
        .border_color(border)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Radius::Sm.length()),
    )
    .dims(Dimensions::fixed(size.length(), size.length()))
}

/// A labeled push button at control height.
pub fn action<F: Fn(&mut App) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    on_click: F,
) -> impl WidgetView<App> + use<F> {
    sized_box(
        button(
            label(text).text_size(TextSize::Body.px()).color(pal.text),
            move |app: &mut App| on_click(app),
        )
        .background_color(pal.button)
        .corner_radius(Radius::Md.length()),
    )
    .dims(Dimensions::new(Dim::Auto, Dim::from(ControlSize::Control)))
}
