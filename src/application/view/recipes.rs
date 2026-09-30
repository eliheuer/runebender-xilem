// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The recipes this application repeats.
//!
//! Measurements come from `view::design`; colors and corner radii come from `view::theme`.
//! [`panel_section`] is the default for a new folding panel section: it owns the
//! header, collapse state, symmetric inset, body gap, and full-width divider.
//! [`panel_stack`] composes sections without duplicating their spacing.
//! Body layouts use `Region::Form`, `Region::Inline`, or `Region::List` without
//! adding another outer inset. See `UI-DESIGN.md` for a complete contributor example.

use crate::application::widgets::icon_paint;
use crate::application::widgets::input_typography;

use crate::application::view::design::{
    ControlSize, ROW_MARKER_SIZE, Region, Space, Stroke, TextSize,
};
use crate::application::view::design::{column, row};
use crate::application::view::{label, text_input};
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::FlexExt as _;
use xilem::view::{FlexSpacer, button as xilem_button, canvas, sized_box};

use crate::application::view::theme::Palette;
use crate::application::workspace::Workspace;

/// The application's base button.
///
/// Panel controls use the active theme's control radius.
/// Deliberately circular controls override it at their use site.
pub(crate) fn button<V, F>(
    pal: &Palette,
    child: V,
    on_click: F,
) -> impl WidgetView<Workspace, Widget = masonry::widgets::Button> + use<V, F>
where
    V: WidgetView<Workspace>,
    F: Fn(&mut Workspace) + Send + Sync + 'static,
{
    xilem_button(child, on_click).corner_radius(Length::px(pal.control_radius))
}

/// A section header with an explicit tokenized content height.
pub(crate) fn section_toggle_height<F>(
    pal: &Palette,
    text: &'static str,
    open: bool,
    height: f64,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F>
where
    F: Fn(&mut Workspace) + Send + Sync + 'static,
{
    let muted = pal.text_muted;
    // A stretched row inside the button, because the stock button
    // centres its child and a section header has to sit at the left edge
    // with the rows it heads.
    sized_box(
        button(
            pal,
            row(
                Region::Inline,
                (
                    marker(if open { Marker::Open } else { Marker::Closed }, muted),
                    label(text).text_size(TextSize::Caption.px()).color(muted),
                    FlexSpacer::Flex(1.0),
                ),
            ),
            move |app: &mut Workspace| on_click(app),
        )
        .background_color(pal.panel)
        .border_width(Stroke::None.length())
        .padding(Space::None),
    )
    .dims(Dimensions::new(
        Dim::Stretch,
        Dim::Fixed(Length::px(height)),
    ))
}

/// A center-panel footer with a shared height, balanced edge clearance and upper divider.
/// Supply compact controls and inline groups without their own outer padding.
/// The centered control row and its side inset use the same geometry in every mode.
pub(crate) fn panel_footer<State, Seq>(
    pal: &Palette,
    controls: Seq,
) -> impl WidgetView<State, Widget: Sized> + use<State, Seq>
where
    State: 'static,
    Seq: xilem::view::FlexSequence<State, ()> + Send + Sync,
{
    use crate::application::view::design::{FOOTER_HEIGHT, FOOTER_INSET};
    crate::application::view::render::top_keyline(
        sized_box(
            row(Region::Inline, controls).padding(masonry::properties::Padding::horizontal(
                Length::px(FOOTER_INSET),
            )),
        )
        .dims(Dimensions::new(
            Dim::Stretch,
            Dim::Fixed(Length::px(FOOTER_HEIGHT)),
        ))
        .background_color(pal.panel),
        pal.outline,
    )
}

/// A panel group with equal padding on every edge and a full-width dividing rule.
pub(crate) fn panel_group<State, V>(
    pal: &Palette,
    body: V,
) -> impl WidgetView<State> + use<State, V>
where
    State: 'static,
    V: WidgetView<State> + 'static,
{
    column(
        Region::List,
        (
            sized_box(body).padding(crate::application::view::design::PANEL_SECTION_INSET),
            sized_box(label(""))
                .dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::Fixed(Stroke::Hairline.length()),
                ))
                .background_color(pal.outline),
        ),
    )
    .gap(Space::None)
}

/// Stack complete panel sections without adding another inset or gap beside their dividers.
pub(crate) fn panel_stack<State, Seq>(
    sections: Seq,
) -> impl WidgetView<State, Widget = masonry::widgets::Flex> + use<State, Seq>
where
    State: 'static,
    Seq: xilem::view::FlexSequence<State, ()> + Send + Sync,
{
    column(Region::List, sections).gap(Space::None)
}

/// Folding section contents; the surrounding [`panel_group`] owns the outer inset and divider.
/// Use [`panel_section`] when the caller does not already provide that surrounding group.
pub(crate) fn section<V>(
    app: &Workspace,
    key: &'static str,
    title: &'static str,
    body: V,
) -> impl WidgetView<Workspace, Widget = masonry::widgets::Flex> + use<V>
where
    V: WidgetView<Workspace> + 'static,
{
    section_with_header_height(app, key, title, body, ControlSize::Row.px())
}

/// Folding contents with a tokenized header-height exception for compact node inspectors.
pub(crate) fn section_with_header_height<V>(
    app: &Workspace,
    key: &'static str,
    title: &'static str,
    body: V,
    height: f64,
) -> impl WidgetView<Workspace, Widget = masonry::widgets::Flex> + use<V>
where
    V: WidgetView<Workspace> + 'static,
{
    let open = !app.collapsed.contains(key);
    column(
        Region::Section,
        (
            section_toggle_height(
                &app.palette,
                title,
                open,
                height,
                move |app: &mut Workspace| {
                    if !app.collapsed.remove(key) {
                        app.collapsed.insert(key);
                    }
                },
            ),
            open.then_some(body),
        ),
    )
    .gap(crate::application::view::design::PANEL_SECTION_BODY_GAP)
}

/// A complete folding section with the shared padding, header, body spacing, and divider.
/// Supply only unpadded content; wrapping this in [`panel_group`] would double its inset.
pub(crate) fn panel_section<V>(
    app: &Workspace,
    key: &'static str,
    title: &'static str,
    body: V,
) -> impl WidgetView<Workspace> + use<V>
where
    V: WidgetView<Workspace> + 'static,
{
    panel_group(&app.palette, section(app, key, title, body))
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
        ),
    ))
    .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Row)))
}

/// A bare text field: no caption, a placeholder inside, control
/// height. Used by compact rows such as the kerning controls.
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
    sized_box(input_typography::input_typography(
        text_input(value, move |app: &mut Workspace, v| on_change(app, v))
            .on_enter(move |app: &mut Workspace, v| on_enter(app, v))
            .placeholder(placeholder)
            .text_color(pal.text)
            .placeholder_color(pal.text_muted)
            .background_color(pal.field())
            .border_color(pal.field_outline)
            .border_width(Stroke::Hairline.length())
            .corner_radius(Length::px(pal.control_radius)),
    ))
    .dims(Dimensions::new(
        Dim::Stretch,
        Dim::from(ControlSize::Control),
    ))
}

/// A caption and its control or control row, using the standard field typography and gap.
/// An empty caption adds no label or extra space.
pub(crate) fn labeled_control<V>(
    pal: &Palette,
    name: &'static str,
    control: V,
) -> impl WidgetView<Workspace> + use<V>
where
    V: WidgetView<Workspace> + 'static,
{
    column(
        Region::List,
        (
            (!name.is_empty()).then(|| {
                label(name)
                    .text_size(TextSize::Caption.px())
                    .color(pal.text_muted)
            }),
            control,
        ),
    )
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
    labeled_control(
        pal,
        name,
        sized_box(input_typography::input_typography(
            text_input(value, move |app: &mut Workspace, v| on_change(app, v))
                .text_color(pal.text)
                .placeholder_color(pal.text_muted)
                .background_color(pal.field())
                .border_color(pal.field_outline)
                .border_width(Stroke::Hairline.length())
                .corner_radius(Length::px(pal.control_radius)),
        ))
        .dims(Dimensions::new(
            Dim::Stretch,
            Dim::from(ControlSize::Control),
        )),
    )
}

/// An equal-width field column that fits its row and grows with the dock.
/// Pair with `row(Region::Form, ...)`; the row divides the available width after its gaps.
pub(crate) fn field_column<V>(body: V) -> impl xilem::view::FlexSequence<Workspace, ()> + use<V>
where
    V: WidgetView<Workspace> + 'static,
{
    sized_box(body)
        .dims(Dimensions::new(Dim::Stretch, Dim::Auto))
        .flex(1.0)
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
    labeled_control(pal, name, field_bare(pal, "", value, on_change, on_enter))
}

/// A plain filter row: label left, trailing text right, keylined when active.
pub(crate) fn list_row<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    trailing: String,
    active: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    list_row_marked(pal, Marker::None, false, text, trailing, active, on_click)
}

/// What a sidebar row shows before its label: a chevron on a row that
/// expands, a bullet on a leaf, as the GPUI sidebar has them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Marker {
    /// A plain filter row, without a disclosure or bullet.
    None,
    Bullet,
    Closed,
    Open,
}

/// A row marker painted from the editable icon UFO.
pub(crate) fn marker(marker: Marker, color: xilem::Color) -> impl WidgetView<Workspace> + use<> {
    sized_box(canvas(move |_: &mut Workspace, _, scene, size| {
        use masonry::imaging::Painter;
        let name = match marker {
            Marker::None => return,
            Marker::Bullet => "disclosure-bullet",
            Marker::Closed => "disclosure-closed",
            Marker::Open => "disclosure-open",
        };
        icon_paint::paint(
            &mut Painter::new(scene).as_dyn(),
            name,
            size.to_rect(),
            color,
        );
    }))
    .dims(Dimensions::fixed(
        Length::px(ROW_MARKER_SIZE),
        Length::px(ROW_MARKER_SIZE),
    ))
}

/// A list row with a chosen marker, indented when it sits under
/// another row.
pub(crate) fn list_row_marked<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    row_marker: Marker,
    indent: bool,
    text: String,
    trailing: String,
    active: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    let (fg, border, bg) = if active {
        (pal.selected_content_ink(), pal.outline, pal.selected_bg())
    } else {
        (pal.text, xilem::Color::TRANSPARENT, pal.panel)
    };
    let trailing_color = if active {
        pal.selected_content_ink()
    } else {
        pal.text_muted
    };
    sized_box(
        button(
            pal,
            row(
                Region::Inline,
                (
                    indent.then_some(FlexSpacer::Fixed(Space::Lg.length())),
                    (row_marker != Marker::None)
                        .then(|| marker(row_marker, if active { fg } else { pal.text_muted })),
                    label(text).text_size(TextSize::Body.px()).color(fg),
                    FlexSpacer::Flex(1.0),
                    label(trailing)
                        .text_size(TextSize::Body.px())
                        .color(trailing_color),
                ),
            ),
            move |app: &mut Workspace| on_click(app),
        )
        .padding(masonry::properties::Padding::horizontal(Length::px(
            crate::application::view::design::SIDEBAR_ROW_INSET,
        )))
        .background_color(bg)
        .border_color(border)
        .border_width(
            if active {
                Stroke::Hairline
            } else {
                Stroke::None
            }
            .length(),
        )
        .corner_radius(Length::px(pal.control_radius * 0.5)),
    )
    .dims(Dimensions::new(
        Dim::Stretch,
        Dim::from(ControlSize::SidebarRow),
    ))
}

/// A toggle at control height that takes the width of its label and inverts
/// when active.
pub(crate) fn toggle<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    active: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    let (fg, border, bg) = if active {
        (pal.selected_ink(), pal.outline, pal.selected_bg())
    } else {
        (pal.text, pal.outline, pal.panel)
    };
    sized_box(
        button(
            pal,
            label(text).text_size(TextSize::Body.px()).color(fg),
            move |app: &mut Workspace| on_click(app),
        )
        .padding(Space::Md)
        .background_color(bg)
        .border_color(border)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Length::px(pal.control_radius)),
    )
    .dims(Dimensions::new(Dim::Auto, Dim::from(ControlSize::Control)))
}

/// A labeled push button at control height.
pub(crate) fn action<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    let action_name = text.clone();
    sized_box(
        button(
            pal,
            label(text).text_size(TextSize::Body.px()).color(pal.text),
            move |app: &mut Workspace| {
                let previous_message = app.note.clone();
                let revision = app.font.project.document_revision();
                on_click(app);
                app.record_ui_action(
                    "panel_button",
                    action_name.clone(),
                    &previous_message,
                    revision,
                );
            },
        )
        .background_color(pal.button)
        .border_color(pal.outline)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Length::px(pal.control_radius)),
    )
    .dims(Dimensions::new(Dim::Auto, Dim::from(ControlSize::Control)))
}

/// A labeled push button at a chosen compact height.
pub(crate) fn action_sized<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    size: ControlSize,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    let action_name = text.clone();
    sized_box(
        button(
            pal,
            label(text).text_size(TextSize::Body.px()).color(pal.text),
            move |app: &mut Workspace| {
                let previous_message = app.note.clone();
                let revision = app.font.project.document_revision();
                on_click(app);
                app.record_ui_action(
                    "panel_button",
                    action_name.clone(),
                    &previous_message,
                    revision,
                );
            },
        )
        .padding(Space::Sm)
        .background_color(pal.button)
        .border_color(pal.outline)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Length::px(pal.control_radius)),
    )
    .dims(Dimensions::new(Dim::Auto, Dim::from(size)))
}

/// The editor's neutral slider, retaining Masonry keyboard and pointer behavior.
pub(crate) fn neutral_slider<F>(
    pal: &Palette,
    min: f64,
    max: f64,
    value: f64,
    on_change: F,
) -> impl WidgetView<Workspace, Widget: Sized> + use<F>
where
    F: Fn(&mut Workspace, f64) + Send + Sync + 'static,
{
    crate::application::widgets::gesture_slider::gesture_slider(
        pal,
        min,
        max,
        value,
        move |app, value, _from_pointer| on_change(app, value),
        |_app, _cancelled| {},
    )
}
