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

use crate::application::widgets::input_typography;

use crate::application::view::design::{
    ButtonShape, ControlSize, ROW_MARKER_BULLET_RADIUS, ROW_MARKER_CHEVRON_LONG,
    ROW_MARKER_CHEVRON_SHORT, ROW_MARKER_CHEVRON_TIP, ROW_MARKER_SIZE, Radius, Region, Space,
    Stroke, TextSize,
};
use crate::application::view::design::{column, row};
use crate::application::view::{label, text_input};
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::{FlexSpacer, button as xilem_button, canvas, sized_box};

use crate::application::view::theme::Palette;
use crate::application::workspace::Workspace;

/// The application's base button.
///
/// Xilem's stock button is rounded. Runebender panel controls are square so
/// their keylines can join neighboring rows and controls. View modules import
/// this factory directly; deliberately circular controls override the radius
/// with [`ButtonShape::Circular`].
pub(crate) fn button<V, F>(
    child: V,
    on_click: F,
) -> impl WidgetView<Workspace, Widget = masonry::widgets::Button> + use<V, F>
where
    V: WidgetView<Workspace>,
    F: Fn(&mut Workspace) + Send + Sync + 'static,
{
    xilem_button(child, on_click).corner_radius(ButtonShape::Square.radius())
}

/// A section header that collapses its section.
///
/// Sidebar groups fold because four filter groups and a language list would
/// otherwise become one undifferentiated scroll of rows.
pub(crate) fn section_toggle<F>(
    pal: &Palette,
    text: &'static str,
    open: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F>
where
    F: Fn(&mut Workspace) + Send + Sync + 'static,
{
    section_toggle_height(pal, text, open, ControlSize::Row.px(), on_click)
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

/// An inspector group with a full-width dividing rule and a shared inset.
pub(crate) fn inspector_group<V>(pal: &Palette, body: V) -> impl WidgetView<Workspace> + use<V>
where
    V: WidgetView<Workspace> + 'static,
{
    column(
        Region::List,
        (
            sized_box(body).padding(masonry::properties::Padding::from_vh(
                Length::px(crate::application::view::design::INSPECTOR_VERTICAL_INSET),
                Space::Md.length(),
            )),
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
            .corner_radius(Radius::None.length()),
    ))
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
            sized_box(input_typography::input_typography(
                text_input(value, move |app: &mut Workspace, v| on_change(app, v))
                    .text_color(pal.text)
                    .placeholder_color(pal.text_muted)
                    .background_color(pal.field())
                    .border_color(pal.field_outline)
                    .border_width(Stroke::Hairline.length())
                    .corner_radius(Radius::None.length()),
            ))
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
            sized_box(input_typography::input_typography(
                text_input(value, move |app: &mut Workspace, v| on_change(app, v))
                    .on_enter(move |app: &mut Workspace, v| on_enter(app, v))
                    .text_color(pal.text)
                    .placeholder_color(pal.text_muted)
                    .background_color(pal.field())
                    .border_color(pal.field_outline)
                    .border_width(Stroke::Hairline.length())
                    .corner_radius(Radius::None.length()),
            ))
            .dims(Dimensions::new(
                Dim::Stretch,
                Dim::from(ControlSize::Control),
            )),
        ),
    )
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

impl Marker {
    fn is_open(self) -> bool {
        matches!(self, Self::Open)
    }
}

/// A painted row marker. The bundled UI font deliberately does not carry
/// disclosure characters, so geometry is both deterministic and identical
/// to the GPUI reference at every theme and scale.
pub(crate) fn marker(marker: Marker, color: xilem::Color) -> impl WidgetView<Workspace> + use<> {
    sized_box(canvas(move |_: &mut Workspace, _, scene, size| {
        use masonry::imaging::Painter;
        use masonry::kurbo::{BezPath, Circle};

        if marker == Marker::None {
            return;
        }
        let mut painter = Painter::new(scene);
        let center = (size.width / 2.0, size.height / 2.0);
        if marker == Marker::Bullet {
            painter
                .fill(Circle::new(center, ROW_MARKER_BULLET_RADIUS), color)
                .draw();
            return;
        }

        let mut path = BezPath::new();
        if marker.is_open() {
            path.move_to((
                center.0 - ROW_MARKER_CHEVRON_LONG,
                center.1 - ROW_MARKER_CHEVRON_SHORT,
            ));
            path.line_to((
                center.0 + ROW_MARKER_CHEVRON_LONG,
                center.1 - ROW_MARKER_CHEVRON_SHORT,
            ));
            path.line_to((center.0, center.1 + ROW_MARKER_CHEVRON_TIP));
        } else {
            path.move_to((
                center.0 - ROW_MARKER_CHEVRON_SHORT,
                center.1 - ROW_MARKER_CHEVRON_LONG,
            ));
            path.line_to((center.0 + ROW_MARKER_CHEVRON_TIP, center.1));
            path.line_to((
                center.0 - ROW_MARKER_CHEVRON_SHORT,
                center.1 + ROW_MARKER_CHEVRON_LONG,
            ));
        }
        path.close_path();
        painter.fill(&path, color).draw();
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
        .corner_radius(Radius::None.length()),
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
            label(text).text_size(TextSize::Body.px()).color(fg),
            move |app: &mut Workspace| on_click(app),
        )
        .padding(Space::Md)
        .background_color(bg)
        .border_color(border)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Radius::None.length()),
    )
    .dims(Dimensions::new(Dim::Auto, Dim::from(ControlSize::Control)))
}

/// A labeled push button at control height.
pub(crate) fn action<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    action_enabled(pal, text, true, on_click)
}

/// A standard action whose availability is reflected in keyboard and pointer interaction.
pub(crate) fn action_enabled<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    enabled: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    sized_box(
        xilem_button(
            label(text)
                .text_size(TextSize::Body.px())
                .color(if enabled { pal.text } else { pal.text_muted }),
            move |app: &mut Workspace| on_click(app),
        )
        .disabled(!enabled)
        .background_color(pal.button)
        .border_color(pal.outline)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Radius::None.length()),
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
    sized_box(
        button(
            label(text).text_size(TextSize::Body.px()).color(pal.text),
            move |app: &mut Workspace| on_click(app),
        )
        .padding(Space::Sm)
        .background_color(pal.button)
        .border_color(pal.outline)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Radius::None.length()),
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
) -> impl WidgetView<Workspace, Widget = masonry::widgets::Slider> + use<F>
where
    F: Fn(&mut Workspace, f64) + Send + Sync + 'static,
{
    use masonry::properties::{ThumbColor, TrackColor};
    xilem::view::slider(min, max, value, on_change)
        .prop(TrackColor {
            active: pal.text_muted,
            inactive: pal.text_muted,
        })
        .prop(ThumbColor(pal.button))
        // Masonry's stock slider paints a white rounded capsule around the
        // whole control on hover or focus. Runebender keeps that state in the
        // slightly lighter thumb instead, so the footer remains one flat row.
        .border_color(xilem::Color::TRANSPARENT)
}
