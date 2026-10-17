// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The panel language: what a section is made of.
//!
//! A panel section is built from [`Group`]s, and a group from [`Row`]s.
//! There is no other way to put content in a section. A panel module
//! never reaches for a bare label, column, or button; it describes its
//! section in this vocabulary and the builder owns every measurement,
//! color, and divider. `tests/panel_vocabulary.rs` fails the build when
//! a panel file reaches past this module.
//!
//! The four rules, which the builder enforces:
//!
//! 1. A section is one question. Its title is a short noun phrase in
//!    title case. It holds groups, and each group answers one part of
//!    the question.
//! 2. A group is a caption and its rows. Rows sit [`Space::Sm`] apart.
//!    Groups are separated by a hairline that stops at the section
//!    margin, with [`Space::Lg`] above and below. Sections are
//!    separated by the full-width rule that `panel_group` owns. Three
//!    spacings, two dividers, no others.
//! 3. Prose is state, not instruction. A section never explains itself
//!    at the top. [`Row::Meta`] is one muted line under the thing it
//!    describes. Instructions live where the action is.
//! 4. One [`Row::Primary`] per section. Everything else is
//!    [`Row::Secondary`] or [`Row::Quiet`]. Facts go behind
//!    [`Row::Details`].
//!
//! Captions and titles are title case. Nothing here uppercases text,
//! and nothing here may start.
//!
//! If a feature needs a row that is not here, add it to [`Row`] once,
//! with a reason in its doc comment. Every panel then has it.

use std::sync::Arc;

use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use xilem::style::Style;
use xilem::view::FlexExt as _;
use xilem::view::{FlexSpacer, canvas, sized_box};
use xilem::{AnyWidgetView, WidgetView};

use super::{Marker, button, kv, marker, prose, prose_muted};
use crate::application::view::design::{
    ControlSize, DOCK_WIDTH, Region, Space, Stroke, TextSize, column, row,
};
use crate::application::view::label;
use crate::application::view::theme::Palette;
use crate::application::workspace::Workspace;

/// A click handler a row carries. Boxed so rows of different kinds can
/// share one list.
pub(crate) type Click = Arc<dyn Fn(&mut Workspace) + Send + Sync>;
/// A handler that receives the index the person picked.
pub(crate) type Pick = Arc<dyn Fn(&mut Workspace, usize) + Send + Sync>;
/// A handler that receives a slider's value.
pub(crate) type Slide = Arc<dyn Fn(&mut Workspace, f64) + Send + Sync>;
/// A handler that receives a field's text.
pub(crate) type Typed = Arc<dyn Fn(&mut Workspace, String) + Send + Sync>;

/// Wrap a closure as a [`Click`].
pub(crate) fn click(f: impl Fn(&mut Workspace) + Send + Sync + 'static) -> Click {
    Arc::new(f)
}

/// A square preview with a caption, for [`Row::Tiles`].
pub(crate) struct Tile {
    /// The word under the square.
    pub(crate) caption: String,
    /// The drawing, already fitted to [`TILE_PX`].
    pub(crate) path: kurbo::BezPath,
}

/// The side of a tile, in logical pixels.
pub(crate) const TILE_PX: f64 = 56.0;

/// One line of a group. Each variant is a complete row with its own
/// typography, height, and color; a caller supplies only content.
pub(crate) enum Row {
    /// One muted line that says what the row above is. State, never
    /// instruction.
    Meta(String),
    /// One line in body ink: a note or a result.
    Prose(String),
    /// A text field: a caption over a control-height input. Enter
    /// commits.
    #[expect(
        dead_code,
        reason = "the vocabulary is closed: a row nothing uses yet is still part of it"
    )]
    Field {
        caption: &'static str,
        placeholder: &'static str,
        value: String,
        on_change: Typed,
        on_enter: Typed,
    },
    /// One of several, as a list of rows with the chosen one inverted.
    /// A list, not a popup or a field, so what is available and what
    /// is chosen are both visible at once.
    Picker {
        choices: Vec<String>,
        chosen: Option<usize>,
        on_pick: Pick,
    },
    /// A set of words or tags as pills, in as many lines as the dock
    /// needs. Each pill is a control: clicking picks it, and the chosen
    /// one is inverted.
    Chips {
        items: Vec<String>,
        chosen: Option<usize>,
        on_pick: Pick,
    },
    /// Two or three exclusive options as one segmented control.
    Choice {
        options: Vec<String>,
        chosen: usize,
        on_pick: Pick,
    },
    /// The one thing the section is for. Filled, full width. One per
    /// section.
    Primary { text: String, on_click: Click },
    /// The other way to do it. Outlined, full width.
    Secondary { text: String, on_click: Click },
    /// Rare or reversible: Stop, Cancel, Undo. Text only, muted.
    Quiet { text: String, on_click: Click },
    /// Something in progress: one sentence and, when known, how far.
    Status { text: String, fraction: Option<f64> },
    /// Square previews with a caption each.
    Tiles(Vec<Tile>),
    /// Facts: name left, value right, one row each.
    KeyValue(Vec<(String, String)>),
    /// Rows nobody needs every day, behind a disclosure. The key is the
    /// collapse key; details start closed.
    #[expect(
        dead_code,
        reason = "the vocabulary is closed: a row nothing uses yet is still part of it"
    )]
    Details { key: &'static str, rows: Vec<Self> },
    /// A value on a range, with its caption and current reading.
    Slider {
        caption: String,
        min: f64,
        max: f64,
        value: f64,
        on_change: Slide,
    },
    /// A view from a module that predates this vocabulary. It is here
    /// so those modules can be moved one at a time; a new panel adds a
    /// row instead.
    Legacy(Box<AnyWidgetView<Workspace>>),
}

/// A caption and its rows.
pub(crate) struct Group {
    /// The name of the group, in title case. Empty when the section
    /// has one group: the section title is then the caption, and a
    /// second line saying the same thing is noise.
    pub(crate) caption: &'static str,
    pub(crate) rows: Vec<Row>,
}

impl Group {
    pub(crate) fn new(caption: &'static str, rows: Vec<Row>) -> Self {
        Self { caption, rows }
    }
}

/// A complete folding section built from groups: the shared header,
/// collapse state, inset, group dividers, and the full-width rule.
pub(crate) fn section_of(
    app: &Workspace,
    key: &'static str,
    title: &'static str,
    groups: Vec<Group>,
) -> impl WidgetView<Workspace> + use<> {
    super::panel_section(app, key, title, body_of(app, groups))
}

/// A section and, under its full-width rule, a Details strip: the facts
/// nobody needs every day, visibly outside the section they describe.
pub(crate) fn section_with_details(
    app: &Workspace,
    key: &'static str,
    title: &'static str,
    groups: Vec<Group>,
    details: Vec<Row>,
) -> impl WidgetView<Workspace> + use<> {
    let strip = details_view(app, key, details);
    super::panel_stack((
        section_of(app, key, title, groups),
        super::panel_group(&app.palette, strip),
    ))
}

/// The groups of a section without the header, for a caller that owns
/// its own frame.
pub(crate) fn body_of(app: &Workspace, groups: Vec<Group>) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let mut children: Vec<Box<AnyWidgetView<Workspace>>> = Vec::new();
    for (index, group) in groups.into_iter().enumerate() {
        if index > 0 {
            children.push(group_divider(pal).boxed());
        }
        children.push(group_view(app, group).boxed());
    }
    column(Region::List, children).gap(Space::Lg)
}

/// The hairline between two groups. It is a child of the section body,
/// so it stops at the section margin instead of the panel edge.
fn group_divider(pal: &Palette) -> impl WidgetView<Workspace> + use<> {
    sized_box(label(""))
        .dims(Dimensions::new(
            Dim::Stretch,
            Dim::Fixed(Stroke::Hairline.length()),
        ))
        .background_color(pal.outline)
}

fn group_view(app: &Workspace, group: Group) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let mut children: Vec<Box<AnyWidgetView<Workspace>>> = Vec::new();
    if !group.caption.is_empty() {
        children.push(caption(pal, group.caption).boxed());
    }
    for row in group.rows {
        children.push(row_view(app, row));
    }
    column(Region::List, children).gap(Space::Sm)
}

/// The name of a group: caption size, muted, title case as written.
fn caption(pal: &Palette, text: &'static str) -> impl WidgetView<Workspace> + use<> {
    label(text)
        .text_size(TextSize::Caption.px())
        .color(pal.text_muted)
}

fn row_view(app: &Workspace, row: Row) -> Box<AnyWidgetView<Workspace>> {
    let pal = &app.palette;
    match row {
        Row::Meta(text) => prose_muted(pal, text).boxed(),
        Row::Prose(text) => prose(pal, text).boxed(),
        Row::Field {
            caption,
            placeholder,
            value,
            on_change,
            on_enter,
        } => super::labeled_control(
            pal,
            caption,
            super::field_bare(
                pal,
                placeholder,
                value,
                move |app, v| on_change(app, v),
                move |app, v| on_enter(app, v),
            ),
        )
        .boxed(),
        Row::Picker {
            choices,
            chosen,
            on_pick,
        } => picker(pal, choices, chosen, on_pick).boxed(),
        Row::Chips {
            items,
            chosen,
            on_pick,
        } => chips(pal, items, chosen, on_pick).boxed(),
        Row::Choice {
            options,
            chosen,
            on_pick,
        } => choice(pal, options, chosen, on_pick).boxed(),
        Row::Primary { text, on_click } => primary(pal, text, on_click).boxed(),
        Row::Secondary { text, on_click } => secondary(pal, text, on_click).boxed(),
        Row::Quiet { text, on_click } => quiet(pal, text, on_click).boxed(),
        Row::Status { text, fraction } => status(pal, text, fraction).boxed(),
        Row::Tiles(tiles) => tiles_view(pal, tiles).boxed(),
        Row::KeyValue(pairs) => column(
            Region::List,
            pairs
                .into_iter()
                .map(|(k, v)| kv(pal, k, v))
                .collect::<Vec<_>>(),
        )
        .gap(Space::None)
        .boxed(),
        Row::Details { key, rows } => details(app, key, rows).boxed(),
        Row::Slider {
            caption,
            min,
            max,
            value,
            on_change,
        } => slider(pal, caption, min, max, value, on_change).boxed(),
        Row::Legacy(view) => view,
    }
}

/// The ink and face of a button by its weight in the section.
fn primary(pal: &Palette, text: String, on_click: Click) -> impl WidgetView<Workspace> + use<> {
    let (fg, bg, border) = (
        pal.selected_ink(),
        pal.selected_bg(),
        pal.selected_outline(pal.outline),
    );
    sized_box(
        button(
            pal,
            label(text).text_size(TextSize::Body.px()).color(fg),
            move |app: &mut Workspace| on_click(app),
        )
        .background_color(bg)
        .border_color(border)
        .border_width(Stroke::Hairline.length()),
    )
    .dims(Dimensions::new(
        Dim::Stretch,
        Dim::from(ControlSize::Control),
    ))
}

fn secondary(pal: &Palette, text: String, on_click: Click) -> impl WidgetView<Workspace> + use<> {
    sized_box(
        button(
            pal,
            label(text).text_size(TextSize::Body.px()).color(pal.text),
            move |app: &mut Workspace| on_click(app),
        )
        .background_color(pal.button)
        .border_color(pal.outline)
        .border_width(Stroke::Hairline.length()),
    )
    .dims(Dimensions::new(
        Dim::Stretch,
        Dim::from(ControlSize::Control),
    ))
}

fn quiet(pal: &Palette, text: String, on_click: Click) -> impl WidgetView<Workspace> + use<> {
    sized_box(
        button(
            pal,
            label(text)
                .text_size(TextSize::Body.px())
                .color(pal.text_muted),
            move |app: &mut Workspace| on_click(app),
        )
        .background_color(Palette::FLAT)
        .border_width(Stroke::None.length())
        .padding(Space::None),
    )
    .dims(Dimensions::new(Dim::Auto, Dim::from(ControlSize::Row)))
}

/// The choices as rows at control height, the chosen one inverted,
/// the same silhouette as every other control in the section. With
/// nothing to choose from, one muted line says so.
fn picker(
    pal: &Palette,
    choices: Vec<String>,
    chosen: Option<usize>,
    on_pick: Pick,
) -> impl WidgetView<Workspace> + use<> {
    let empty = choices
        .is_empty()
        .then(|| prose_muted(pal, "None installed".to_owned()));
    let rows: Vec<_> = choices
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let on_pick = on_pick.clone();
            let active = chosen == Some(i);
            let (fg, border, bg) = if active {
                (
                    pal.selected_ink(),
                    pal.selected_outline(pal.outline),
                    pal.selected_bg(),
                )
            } else {
                (pal.text, pal.outline, Palette::FLAT)
            };
            sized_box(
                button(
                    pal,
                    label(text).text_size(TextSize::Body.px()).color(fg),
                    move |app: &mut Workspace| on_pick(app, i),
                )
                .background_color(bg)
                .border_color(border)
                .border_width(Stroke::Hairline.length()),
            )
            .dims(Dimensions::new(
                Dim::Stretch,
                Dim::from(ControlSize::Control),
            ))
        })
        .collect();
    column(Region::List, (empty, rows)).gap(Space::Sm)
}

/// Words as pills. Masonry's flex does not wrap, so the lines are cut
/// here from the dock width and an average glyph width.
fn chips(
    pal: &Palette,
    items: Vec<String>,
    chosen: Option<usize>,
    on_pick: Pick,
) -> impl WidgetView<Workspace> + use<> {
    const GLYPH_PX: f64 = 6.5;
    let budget = DOCK_WIDTH - 2.0 * Space::Lg.px() - 2.0 * Space::Md.px();
    let width = |w: &str| w.chars().count() as f64 * GLYPH_PX + 2.0 * Space::Md.px();
    let mut lines: Vec<Vec<(usize, String)>> = Vec::new();
    let mut used = 0.0;
    for (i, word) in items.into_iter().enumerate() {
        let w = width(&word);
        if lines.is_empty() || used + w + Space::Sm.px() > budget {
            lines.push(vec![(i, word)]);
            used = w;
        } else {
            used += w + Space::Sm.px();
            lines
                .last_mut()
                .expect("a line was just pushed")
                .push((i, word));
        }
    }
    let rows: Vec<_> = lines
        .into_iter()
        .map(|line| {
            let chips: Vec<_> = line
                .into_iter()
                .map(|(i, w)| {
                    let on_pick = on_pick.clone();
                    chip(pal, w, chosen == Some(i), move |app| on_pick(app, i))
                })
                .collect();
            row(Region::Inline, chips)
        })
        .collect();
    column(Region::List, rows).gap(Space::Sm)
}

/// A pill: a button, so its text is centered by the same code that
/// centers every other button, inverted when chosen.
fn chip<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    active: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    let (fg, border, bg) = if active {
        (
            pal.selected_ink(),
            pal.selected_outline(pal.outline),
            pal.selected_bg(),
        )
    } else {
        (pal.text, pal.outline, Palette::FLAT)
    };
    sized_box(
        button(
            pal,
            label(text).text_size(TextSize::Body.px()).color(fg),
            on_click,
        )
        .padding(masonry::properties::Padding::horizontal(Space::Md.length()))
        .background_color(bg)
        .border_color(border)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Length::px(ControlSize::Swatch.px() / 2.0)),
    )
    .dims(Dimensions::new(Dim::Auto, Dim::from(ControlSize::Swatch)))
}

/// A segmented control: equal cells, the chosen one inverted.
fn choice(
    pal: &Palette,
    options: Vec<String>,
    chosen: usize,
    on_pick: Pick,
) -> impl WidgetView<Workspace> + use<> {
    let cells: Vec<_> = options
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let on_pick = on_pick.clone();
            super::toggle(pal, text, i == chosen, move |app| on_pick(app, i)).flex(1.0)
        })
        .collect();
    row(Region::Inline, cells)
}

/// A sentence and, when the fraction is known, a thin bar under it.
fn status(
    pal: &Palette,
    text: String,
    fraction: Option<f64>,
) -> impl WidgetView<Workspace> + use<> {
    let track = pal.slider_track();
    let fill = pal.text;
    column(
        Region::List,
        (
            prose(pal, text),
            fraction.map(|f| {
                let f = f.clamp(0.0, 1.0);
                sized_box(canvas(move |_: &mut Workspace, _, scene, size| {
                    use masonry::imaging::Painter;
                    use masonry::kurbo::{Rect, Shape as _};
                    let mut painter = Painter::new(scene);
                    let all = Rect::new(0.0, 0.0, size.width, size.height);
                    painter.fill(all.to_path(0.1), track).draw();
                    let done = Rect::new(0.0, 0.0, size.width * f, size.height);
                    painter.fill(done.to_path(0.1), fill).draw();
                }))
                .dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::Fixed(Space::Sm.length()),
                ))
            }),
        ),
    )
    .gap(Space::Sm)
}

fn tiles_view(pal: &Palette, tiles: Vec<Tile>) -> impl WidgetView<Workspace> + use<> {
    let (ink, ground, outline) = (pal.text, pal.field(), pal.outline);
    let views: Vec<_> = tiles
        .into_iter()
        .map(|tile| {
            let path = tile.path;
            column(
                Region::List,
                (
                    sized_box(canvas(move |_: &mut Workspace, _, scene, size| {
                        use masonry::imaging::Painter;
                        use masonry::kurbo::{Rect, Shape as _, Stroke};
                        let mut painter = Painter::new(scene);
                        let frame = Rect::new(0.0, 0.0, size.width, size.height);
                        painter.fill(frame.to_path(0.1), ground).draw();
                        painter.stroke(frame, &Stroke::new(1.0), outline).draw();
                        painter.fill(&path, ink).draw();
                    }))
                    .dims(Dimensions::new(
                        Dim::Fixed(Length::px(TILE_PX)),
                        Dim::Fixed(Length::px(TILE_PX)),
                    )),
                    label(tile.caption)
                        .text_size(TextSize::Caption.px())
                        .color(pal.text_muted),
                ),
            )
            .gap(Space::Xs)
        })
        .collect();
    row(Region::Inline, views)
}

fn details(
    app: &Workspace,
    key: &'static str,
    rows: Vec<Row>,
) -> impl WidgetView<Workspace> + use<> {
    details_view(app, key, rows)
}

/// The disclosure and its rows. The collapse key is the section's with
/// a suffix, so a section and its details fold independently.
fn details_view(
    app: &Workspace,
    key: &'static str,
    rows: Vec<Row>,
) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    // Details start closed, so the key's presence means open. One leak
    // per distinct section key, a fixed set.
    let key: &'static str = Box::leak(format!("{key} details").into_boxed_str());
    let open = app.collapsed.contains(key);
    let inner: Vec<Box<AnyWidgetView<Workspace>>> = if open {
        rows.into_iter().map(|r| row_view(app, r)).collect()
    } else {
        Vec::new()
    };
    column(
        Region::List,
        (
            sized_box(
                button(
                    pal,
                    row(
                        Region::Inline,
                        (
                            marker(
                                if open { Marker::Open } else { Marker::Closed },
                                pal.text_muted,
                            ),
                            label("Details")
                                .text_size(TextSize::Caption.px())
                                .color(pal.text_muted),
                            FlexSpacer::Flex(1.0),
                        ),
                    ),
                    move |app: &mut Workspace| {
                        if !app.collapsed.remove(key) {
                            app.collapsed.insert(key);
                        }
                    },
                )
                .background_color(Palette::FLAT)
                .border_width(Stroke::None.length())
                .padding(Space::None),
            )
            .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Row))),
            column(Region::List, inner).gap(Space::Sm),
        ),
    )
    .gap(Space::Sm)
}

fn slider(
    pal: &Palette,
    caption_text: String,
    min: f64,
    max: f64,
    value: f64,
    on_change: Slide,
) -> impl WidgetView<Workspace> + use<> {
    row(
        Region::Inline,
        (
            label(caption_text)
                .text_size(TextSize::Body.px())
                .color(pal.text_muted),
            super::neutral_slider(pal, min, max, value, move |app, v| on_change(app, v)).flex(1.0),
        ),
    )
}
