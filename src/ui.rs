// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The style layer this app should not have to write.
//!
//! xix note, and the reason this file exists: the port drifted to twenty
//! distinct spacing values and four text sizes because every xilem sizing
//! API takes a free `f64`. The gpui port does not drift, because its
//! vocabulary is a closed scale (`px_1`..`px_4`, `text_xs`/`text_sm`,
//! `rounded_sm`/`rounded_md`) that an agent cannot land between. Nothing
//! here is font-editor logic. It is `Space`, `Type`, `Radius`, and six
//! composed parts, and all of it belongs in the framework (DESIGN.md D8,
//! D11). Until it does, every app on xix rewrites this file, badly.

pub use masonry::kernel::{ControlSize, Radius, Space, Stroke, TextSize};
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use masonry::properties::types::CrossAxisAlignment;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::{
    FlexSpacer, button, flex_col, flex_row, label, sized_box, text_input,
};

use crate::App;
use crate::theme::Palette;


/// A horizontal group, vertically centered, on the scale.
pub fn row<S>(children: S, gap: Space) -> impl WidgetView<App> + use<S>
where
    S: xilem::view::FlexSequence<App, ()> + 'static,
    xilem::view::Flex<S, App, ()>: WidgetView<App, ()>,
    <xilem::view::Flex<S, App, ()> as WidgetView<App, ()>>::Widget:
        masonry::core::UsesProperty<masonry::properties::Gap>,
{
    flex_row(children)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(gap)
}

/// A vertical group, left aligned, on the scale.
pub fn col<S>(children: S, gap: Space) -> impl WidgetView<App> + use<S>
where
    S: xilem::view::FlexSequence<App, ()> + 'static,
    xilem::view::Flex<S, App, ()>: WidgetView<App, ()>,
    <xilem::view::Flex<S, App, ()> as WidgetView<App, ()>>::Widget:
        masonry::core::UsesProperty<masonry::properties::Gap>,
{
    flex_col(children)
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(gap)
}

/// A section header: caption type, muted. No disclosure caret: the
/// framework has no icon set and the default font has no coverage for
/// the geometric shapes block, so the caret rendered as tofu.
pub fn section_header(pal: &Palette, text: &'static str) -> impl WidgetView<App> + use<> {
    label(text.to_string())
        .text_size(TextSize::Caption)
        .color(pal.text_muted)
}

/// A section: header over body, with the section gap under it.
pub fn section<V: WidgetView<App> + 'static>(
    pal: &Palette,
    title: &'static str,
    body: V,
) -> impl WidgetView<App> + use<V> {
    col((section_header(pal, title), body), Space::Sm)
}

/// A read-only label/value row: name left, value right, one row tall.
pub fn kv(pal: &Palette, name: String, value: String) -> impl WidgetView<App> + use<> {
    let (muted, text) = (pal.text_muted, pal.text);
    sized_box(row(
        (
            label(name).text_size(TextSize::Body).color(muted),
            FlexSpacer::Flex(1.0),
            label(value).text_size(TextSize::Body).color(text),
        ),
        Space::Sm,
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
    col(
        (
            label(name).text_size(TextSize::Caption).color(pal.text_muted),
            sized_box(
                text_input(value, move |app: &mut App, v| on_change(app, v))
                    .background_color(pal.field())
                    .corner_radius(Radius::Sm),
            )
            .dims(Dimensions::new(
                Dim::Stretch,
                Dim::from(ControlSize::Control),
            )),
        ),
        Space::Xs,
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
                (
                    label(text).text_size(TextSize::Body).color(fg),
                    FlexSpacer::Flex(1.0),
                    label(trailing).text_size(TextSize::Body).color(trailing_color),
                ),
                Space::Sm,
            ),
            move |app: &mut App| on_click(app),
        )
        .background_color(pal.panel)
        .border_color(border)
        .border_width(Stroke::Hairline)
        .corner_radius(Radius::Sm),
    )
    .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Row)))
}

/// A square toggle: the small A / Aa / eye controls beside a field.
pub fn toggle<F: Fn(&mut App) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    active: bool,
    on_click: F,
) -> impl WidgetView<App> + use<F> {
    let (fg, border) = if active {
        (pal.role("accent"), pal.role("accent"))
    } else {
        (pal.text_muted, pal.role("gridBorder").with_alpha(0.6))
    };
    sized_box(
        button(
            label(text).text_size(TextSize::Caption).color(fg),
            move |app: &mut App| on_click(app),
        )
        .padding(Space::None)
        .background_color(pal.panel)
        .border_color(border)
        .border_width(Stroke::Hairline)
        .corner_radius(Radius::Sm),
    )
    .dims(Dimensions::fixed(ControlSize::Control, ControlSize::Control))
}

/// A labeled push button at control height.
pub fn action<F: Fn(&mut App) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    on_click: F,
) -> impl WidgetView<App> + use<F> {
    sized_box(
        button(
            label(text).text_size(TextSize::Body).color(pal.text),
            move |app: &mut App| on_click(app),
        )
        .background_color(pal.button)
        .corner_radius(Radius::Md),
    )
    .dims(Dimensions::new(Dim::Auto, Dim::from(ControlSize::Control)))
}
