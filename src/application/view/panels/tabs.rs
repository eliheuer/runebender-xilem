// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The left edge: the tab strip, the editor's rail, and the sidebar of
//! categories, languages, and filters.

use crate::application::design;
use crate::application::design::controls::icon_button::icon_button;
use crate::application::design::recipes;
use crate::application::design::recipes::button;
use crate::application::design::theme::Palette;
use crate::application::design::{
    ControlKind, ControlSize, ControlStyle, Ground, Region, Space, Stroke, TextSize,
    column as xcolumn, row as xrow,
};
use crate::application::design::{
    RAIL_TAB_ACTIVE_HEIGHT, RAIL_TAB_HEIGHT, RAIL_TAB_INACTIVE_HEIGHT,
};
use crate::application::view::canvas::grid::{GridEvent, grid};
use crate::application::view::panels::chat::chat_panel;
use crate::application::view::panels::local_ai::local_ai_panel;
use crate::application::view::panels::scripts::scripts_panel;
use crate::application::view::panels::sections::axes_section;
use crate::application::view::render::top_keyline;
use crate::application::view::{label, text_input};
use crate::application::widgets::even_row::even_row;
use crate::application::widgets::icon_paint;
use crate::application::widgets::input_typography;
use crate::application::widgets::scroll_viewport::portal;
use crate::application::workspace::{Mode, Sel, Workspace};
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use masonry::properties::types::CrossAxisAlignment;
use runebender::GlyphCategory;
use std::sync::Arc;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::FlexExt as _;
use xilem::view::{FlexSpacer, canvas, flex_col, sized_box};

/// The editor rail's implemented navigation panels.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rail {
    Glyphs,
    Shapes,
    Axes,
    /// Local models installed for the editor.
    LocalAi,
    /// Chat availability, until the conversation backend is connected.
    Chat,
    /// Saved Python scripts and their currently open draft.
    Scripts,
}

/// Shared search controls keep filtering identical in overview and the editor rail.
fn glyph_search(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    // Search row: the field, then compact scope and case toggles.
    let toggle = |text: String, active: bool, f: fn(&mut Workspace)| {
        let style = ControlStyle::new(pal, ControlKind::Toggle).selected(pal, active);
        sized_box(
            button(
                style,
                label(text)
                    .text_size(TextSize::Body.px())
                    .color(style.ink()),
                move |app: &mut Workspace| f(app),
            )
            .padding(Space::None),
        )
        .dims(Dimensions::fixed(
            Length::px(design::SEARCH_TOGGLE_WIDTH),
            ControlSize::Control.length(),
        ))
    };
    xrow(
        Region::Inline,
        (
            input_typography::input_typography(
                text_input(app.filter.clone(), |app: &mut Workspace, v| {
                    app.filter = v;
                    app.rebuild_search_regex();
                })
                .placeholder("Search glyphs")
                .text_color(pal.text)
                .placeholder_color(pal.placeholder_ink())
                .dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::from(ControlSize::Control),
                )),
                pal.text,
            )
            .flex(1.0),
            toggle(
                match app.search_mode {
                    1 => "N",
                    2 => "U",
                    _ => "A",
                }
                .to_string(),
                app.search_mode != 0,
                |app: &mut Workspace| app.search_mode = (app.search_mode + 1) % 3,
            ),
            toggle(".*".into(), app.search_regex, |app: &mut Workspace| {
                app.search_regex = !app.search_regex;
                app.rebuild_search_regex();
            }),
            toggle("Aa".into(), app.search_case, |app: &mut Workspace| {
                app.search_case = !app.search_case;
                app.rebuild_search_regex();
            }),
        ),
    )
}

/// Contour and component selection for the editor canvas.
fn shapes_panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let contours = app
        .session
        .contour_point_counts()
        .into_iter()
        .enumerate()
        .map(|(ci, point_count)| {
            let active = app.session.contour_selected(ci);
            recipes::toggle(
                pal,
                format!("Contour {} · {} nodes", ci + 1, point_count),
                active,
                move |app: &mut Workspace| {
                    let session = Arc::make_mut(&mut app.session);
                    let points = session.select_contour(ci);
                    app.selected_points = points;
                    app.refresh_coord_bufs();
                },
            )
            .boxed()
        })
        .collect::<Vec<_>>();
    let components = app
        .session
        .component_references()
        .into_iter()
        .enumerate()
        .map(|(index, reference)| {
            recipes::toggle(
                pal,
                format!("{reference} · component"),
                app.session.component_selected(index),
                move |app: &mut Workspace| {
                    Arc::make_mut(&mut app.session).select_component(index);
                    app.selected_points = 0;
                    app.refresh_coord_bufs();
                },
            )
            .boxed()
        })
        .collect::<Vec<_>>();
    let component_aligned = app.session.selected_component_aligned();
    xcolumn(
        Region::Panel,
        (
            contours,
            components,
            xrow(
                Region::Inline,
                (
                    input_typography::input_typography(
                        text_input(
                            app.component_base_buf.clone(),
                            |app: &mut Workspace, value| {
                                app.component_base_buf = value;
                            },
                        )
                        .placeholder("Base glyph")
                        .text_color(pal.text)
                        .placeholder_color(pal.placeholder_ink())
                        .dims(Dimensions::new(
                            Dim::Stretch,
                            Dim::from(ControlSize::Control),
                        )),
                        pal.text,
                    )
                    .flex(1.0),
                    recipes::action(pal, "Add".into(), |app: &mut Workspace| {
                        app.command_add_component();
                    }),
                ),
            ),
            component_aligned.map(|aligned| {
                xcolumn(
                    Region::List,
                    (
                        recipes::action(
                            pal,
                            if aligned { "Unlock" } else { "Lock" }.into(),
                            |app: &mut Workspace| app.command_toggle_component_alignment(),
                        ),
                        xrow(
                            Region::Inline,
                            (
                                recipes::action(pal, "Duplicate".into(), |app: &mut Workspace| {
                                    app.apply_op(|session| session.duplicate());
                                }),
                                recipes::action(pal, "Delete".into(), |app: &mut Workspace| {
                                    app.apply_op(|session| session.delete_selected());
                                }),
                            ),
                        ),
                    ),
                )
            }),
            app.session
                .outline_is_empty()
                .then(|| label("No contours or components").color(pal.text_muted)),
        ),
    )
    .boxed()
}

/// One navigation strip for the font, node, and glyph workspaces.
fn rail_tabs(app: &Workspace, editing: bool) -> impl WidgetView<Workspace> + use<> {
    use crate::application::design::{RAIL_TAB_ICON, RAIL_TAB_ICON_FULL_FRAME, RAIL_TAB_INSET};
    let pal = &app.palette;
    let radius = design::rail_tab_radius(pal.corner_radius);
    let is_active = |which: Rail| {
        app.rail == which || (!editing && app.rail == Rail::Shapes && which == Rail::Glyphs)
    };
    // Inset icons such as `invert` use the full rail size; icons that fill their
    // whole frame use a smaller one so all tab icons look the same size.
    let specs: Vec<(&'static str, Rail, f64)> = [
        Some(("hyperpen", Rail::Glyphs, RAIL_TAB_ICON)),
        editing.then_some(("shapes", Rail::Shapes, RAIL_TAB_ICON_FULL_FRAME)),
        (!app.font.axes.is_empty()).then_some(("measure", Rail::Axes, RAIL_TAB_ICON_FULL_FRAME)),
        Some(("invert", Rail::LocalAi, RAIL_TAB_ICON)),
        Some(("text", Rail::Chat, RAIL_TAB_ICON_FULL_FRAME)),
        Some(("save", Rail::Scripts, RAIL_TAB_ICON_FULL_FRAME)),
    ]
    .into_iter()
    .flatten()
    .collect();
    let count = specs.len();
    let active_slot = specs.iter().position(|&(_, which, _)| is_active(which));
    let tab = |(icon, which, size): (&'static str, Rail, f64)| {
        let active = is_active(which);
        sized_box(
            icon_button(
                ControlStyle::new(pal, ControlKind::Tab).selected(pal, active),
                icon,
                icon,
                move |app: &mut Workspace| app.rail = which,
            )
            .icon_size(size)
            .rail_tab(design::RAIL_TAB_ICON_RISE, radius),
        )
        .dims(Dimensions::new(
            Dim::Stretch,
            Dim::Fixed(Length::px(if active {
                RAIL_TAB_ACTIVE_HEIGHT
            } else {
                RAIL_TAB_INACTIVE_HEIGHT
            })),
        ))
    };
    sized_box(xilem::view::zstack((
        sized_box(canvas({
            let background = pal.tab_rail.with_alpha(pal.side_panel_alpha());
            let outline = pal.outline;
            move |_: &mut Workspace, _, scene, size| {
                use crate::application::design::controls::icon_button;
                use crate::application::widgets::even_row::slot_widths;
                use masonry::imaging::Painter;
                use masonry::kurbo::{Rect, Shape as _};
                let mut painter = Painter::new(scene);
                let stroke = Stroke::Hairline.px();
                let keyline =
                    |x0: f64, x1: f64| Rect::new(x0, size.height - stroke, x1, size.height);
                let mut ground = Rect::new(0.0, 0.0, size.width, size.height).to_path(0.1);
                // The same whole-pixel slots the tab row uses locate the active tab, whose
                // shape the rail leaves open; its keyline stops at the tab's flares.
                let opening = active_slot.map(|index| {
                    let widths = slot_widths(size.width, count, RAIL_TAB_INSET, RAIL_TAB_INSET);
                    let x = RAIL_TAB_INSET
                        + widths[..index].iter().sum::<f64>()
                        + RAIL_TAB_INSET * index as f64;
                    let tab = Rect::new(
                        x,
                        RAIL_TAB_INSET,
                        x + widths[index],
                        RAIL_TAB_INSET + RAIL_TAB_ACTIVE_HEIGHT,
                    );
                    icon_button::rail_tab_face(tab, radius).1
                });
                // The floating panel frame owns the side outlines; a strip here
                // would double the panel edge beside the rail.
                if let Some(hole) = &opening {
                    ground.extend(hole.reverse_subpaths().elements().iter().copied());
                }
                painter.fill(&ground, background).draw();
                match opening {
                    Some(hole) => {
                        let span = hole.bounding_box();
                        painter.fill(keyline(0.0, span.x0), outline).draw();
                        painter.fill(keyline(span.x1, size.width), outline).draw();
                    }
                    None => painter.fill(keyline(0.0, size.width), outline).draw(),
                }
            }
        }))
        .dims(Dimensions::new(Dim::Stretch, Dim::Stretch)),
        // Whole-pixel tab widths keep every gap and inset at exactly the rail inset.
        even_row(
            specs.into_iter().map(tab).collect(),
            RAIL_TAB_INSET,
            RAIL_TAB_INSET,
        ),
    )))
    .dims(Dimensions::new(
        Dim::Stretch,
        Dim::Fixed(Length::px(RAIL_TAB_HEIGHT)),
    ))
}

/// The glyph grid owns its inset; padding the whole rail would count it twice.
fn editor_glyph_rail(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use crate::application::design::{RAIL_CELL_MAX, RAIL_CELL_MIN, STATUS_SLIDER_WIDTH};
    let cells = app.filtered_cells();
    let count = cells.len();
    let current = match app.mode {
        Mode::Editor(i) => Some(i),
        _ => None,
    };
    flex_col((
        sized_box(glyph_search(app)).padding(Space::Md),
        recipes::hairline(&app.palette),
        grid(
            cells,
            app.rail_cell_metrics(),
            app.palette.clone(),
            current,
            app.multi_selected.clone(),
            |app: &mut Workspace, ev| match ev {
                GridEvent::Selected { index, .. } => app.open_glyph(index),
                GridEvent::Open(i) => app.open_glyph(i),
            },
        )
        .flex(1.0),
        // The marks bar immediately below already owns the shared divider.
        // Give this status row only its upper keyline; a full border painted
        // a second horizontal stroke directly above the swatches.
        top_keyline(
            sized_box(
                xrow(
                    Region::Inline,
                    (
                        label(format!("{count} glyphs"))
                            .color(app.palette.text_muted)
                            .flex(1.0),
                        recipes::neutral_slider(
                            &app.palette,
                            RAIL_CELL_MIN,
                            RAIL_CELL_MAX,
                            app.rail_cell_size,
                            |app: &mut Workspace, value| app.rail_cell_size = value,
                        )
                        .width(Length::px(STATUS_SLIDER_WIDTH)),
                    ),
                )
                .padding(masonry::properties::Padding::horizontal(Space::Md.length())),
            )
            .dims(Dimensions::new(
                Dim::Stretch,
                Dim::from(ControlSize::Control),
            )),
            app.palette.outline,
        ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Space::None)
}

/// What a rail panel is called, so the panels have names to talk about
/// in tests and docs. The name is not drawn; the tab icon is the label.
#[cfg(test)]
pub(crate) fn rail_name(rail: Rail, editing: bool) -> &'static str {
    match rail {
        Rail::Glyphs if editing => "Glyph grid",
        Rail::Glyphs => "Glyphs",
        Rail::Shapes => "Shapes",
        Rail::Axes => "Axes",
        Rail::LocalAi => "Local AI",
        Rail::Chat => "Chat",
        Rail::Scripts => "Scripts",
    }
}

/// The one rail frame both workspaces use: the tabs and the panel.
/// Anything that belongs to every rail panel goes here and nowhere else.
fn rail_frame<V>(app: &Workspace, editing: bool, content: V) -> impl WidgetView<Workspace> + use<V>
where
    V: WidgetView<Workspace> + 'static,
{
    flex_col((rail_tabs(app, editing), content.flex(1.0)))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Space::None)
}

pub(crate) fn editor_nav(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    // Each pane owns its inset: the grid fills the rail, Axes uses the section
    // recipe, and standalone tool panes retain their own Region::Panel inset.
    let content = match app.rail {
        Rail::Glyphs => editor_glyph_rail(app).boxed(),
        Rail::Axes => recipes::panel_stack((
            axes_section(app).map(|body| recipes::panel_group(&app.palette, body)),
        ))
        .boxed(),
        Rail::LocalAi => local_ai_panel(app).boxed(),
        Rail::Shapes => shapes_panel(app).boxed(),
        Rail::Chat => chat_panel(app).boxed(),
        Rail::Scripts => scripts_panel(app).boxed(),
    };
    rail_frame(app, true, content)
}

/// Curves: the two analyses that are about shape quality rather than
/// measurement. Both read from the same analysis modules, so they say the same
/// thing here as in other tabbed document editors.
/// The tab strip: one tab per open glyph, with a close box, and a plus
/// that opens a second view on the glyph in hand.
/// The tab strip in the title bar.
///
/// A "Font" tab that is active in the overview, one tab per open glyph
/// with a close button, and a "+" that opens a second tab on the glyph
/// in hand. Tabs are outlined rather than filled: accent when active,
/// the grid border when not.
pub(crate) fn tab_chip<F>(
    pal: &Palette,
    text: String,
    active: bool,
    fixed_width: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F>
where
    F: Fn(&mut Workspace) + Send + Sync + 'static,
{
    let style = ControlStyle::new(pal, ControlKind::Toggle).selected(pal, active);
    let width = if fixed_width {
        Dim::from(ControlSize::Row)
    } else {
        Dim::Auto
    };
    sized_box(
        button(
            style,
            label(text)
                .text_size(TextSize::Body.px())
                .color(style.ink()),
            move |app: &mut Workspace| on_click(app),
        )
        .padding(Space::Md),
    )
    .dims(Dimensions::new(width, Dim::from(ControlSize::Row)))
}

/// A title-bar tab with the theme's active ink and keyline.
fn header_tab_chip<F>(
    pal: &Palette,
    text: String,
    active: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F>
where
    F: Fn(&mut Workspace) + Send + Sync + 'static,
{
    let style = ControlStyle::new(pal, ControlKind::Toggle)
        .on(pal, Ground::Header)
        .selected(pal, active);
    sized_box(
        button(
            style,
            label(text)
                .text_size(TextSize::Body.px())
                .color(style.ink()),
            move |app: &mut Workspace| on_click(app),
        )
        .padding(masonry::properties::Padding::horizontal(Length::px(
            design::HEADER_TAB_TEXT_INSET,
        ))),
    )
    .dims(Dimensions::new(
        Dim::Auto,
        Dim::Fixed(Length::px(design::HEADER_TAB_HEIGHT)),
    ))
}

/// A compact title-bar button whose visible symbol comes from `icons.ufo`.
fn header_icon_chip<F>(
    pal: &Palette,
    icon: &'static str,
    label: &'static str,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F>
where
    F: Fn(&mut Workspace) + Send + Sync + 'static,
{
    let style = ControlStyle::new(pal, ControlKind::Secondary).on(pal, Ground::Header);
    sized_box(
        button(
            style,
            icon_paint::view(icon, label, style.ink(), design::SMALL_ICON_SIZE),
            on_click,
        )
        .padding(Space::None),
    )
    .dims(Dimensions::new(
        Dim::Fixed(Length::px(design::HEADER_TAB_ICON_WIDTH)),
        Dim::Fixed(Length::px(design::HEADER_TAB_HEIGHT)),
    ))
}

pub(crate) fn tab_strip(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let editing = matches!(app.mode, Mode::Editor(_));
    let active = app.active_tab;
    let closable = app.tabs.len() > 1;
    let tabs: Vec<_> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(index, tab)| {
            let mut label = tab.text_context.editor_text.clone();
            if label.is_empty() {
                label = tab.session.glyph_name.clone();
            } else if label.chars().count() > 24 {
                label = label.chars().take(24).collect();
                label.push('…');
            }
            xrow(
                Region::Inline,
                (
                    header_tab_chip(
                        pal,
                        label,
                        editing && index == active,
                        move |app: &mut Workspace| app.activate_tab(index),
                    ),
                    closable.then(|| {
                        header_icon_chip(pal, "close", "Close tab", move |app: &mut Workspace| {
                            app.close_tab(index);
                        })
                    }),
                ),
            )
        })
        .collect();
    xrow(
        Region::Inline,
        (
            // The font itself is a tab, and it is the one that is active
            // in the overview. Going back to the grid is picking that
            // tab, not pressing a back button.
            header_tab_chip(
                pal,
                "Font".into(),
                matches!(app.mode, Mode::Overview),
                |app: &mut Workspace| app.back_to_overview(),
            ),
            // Graph sits beside Font: the workflow over the font, as
            // boxes and wires.
            header_tab_chip(
                pal,
                "Graph".into(),
                matches!(app.mode, Mode::Graph),
                |app: &mut Workspace| app.enter_graph_mode(),
            ),
            xrow(Region::Inline, tabs),
            header_icon_chip(pal, "plus", "New tab", |app: &mut Workspace| {
                app.new_tab();
            }),
        ),
    )
}

/// One folding sidebar group: a header that toggles, and its rows.
///
/// This cannot be a closure inside `sidebar`, because the three groups
/// hold three different row types and a closure cannot be generic. It is
/// a small thing, and it is the shape of most of the scaffolding in this
/// build: anything that takes children has to be a named generic
/// function with a `Send + Sync` bound on the sequence.
pub(crate) fn sidebar_group<V>(
    app: &Workspace,
    title: &'static str,
    rows: Vec<V>,
) -> impl WidgetView<Workspace> + use<V>
where
    V: WidgetView<Workspace> + 'static,
{
    recipes::panel_section(app, title, title, xcolumn(Region::List, rows))
}

pub(crate) fn sidebar(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let content = match app.rail {
        Rail::Axes if !app.font.axes.is_empty() => recipes::panel_stack((
            axes_section(app).map(|body| recipes::panel_group(&app.palette, body)),
        ))
        .boxed(),
        Rail::LocalAi => local_ai_panel(app).boxed(),
        Rail::Chat => chat_panel(app).boxed(),
        Rail::Scripts => scripts_panel(app).boxed(),
        _ => category_sidebar(app).boxed(),
    };
    rail_frame(app, false, content)
}

fn category_sidebar(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use xilem::core::one_of::Either;
    let pal = &app.palette;

    let cats = GlyphCategory::all_categories();
    // Categories: a chevron on one with subfilters, a bullet on one
    // without. Clicking a selected expandable row folds it, as the
    // GPUI sidebar does; the rows under it are indented leaves.
    let mut cat_rows: Vec<_> = Vec::new();
    for c in cats.iter().copied().filter(|c| app.category_count(*c) > 0) {
        let subs = runebender::ui::sidebar::category_subfilters(c.display_name());
        let key = c.display_name();
        let open = app.expanded_categories.contains(key);
        let selected =
            app.sel == Sel::Category(c) || matches!(app.sel, Sel::Subfilter(cat, _) if cat == c);
        let marker = if subs.is_empty() {
            recipes::Marker::Bullet
        } else if open {
            recipes::Marker::Open
        } else {
            recipes::Marker::Closed
        };
        cat_rows.push(Either::A(recipes::list_row_marked(
            pal,
            marker,
            false,
            c.display_name().to_string(),
            format!("{}", app.category_count(c)),
            app.sel == Sel::Category(c),
            move |app: &mut Workspace| {
                if selected && !subs.is_empty() && !app.expanded_categories.remove(key) {
                    app.expanded_categories.insert(key);
                }
                app.sel = Sel::Category(c);
            },
        )));
        if open {
            for (sub, sub_label) in subs {
                cat_rows.push(Either::B(recipes::list_row_marked(
                    pal,
                    recipes::Marker::Bullet,
                    true,
                    (*sub_label).to_string(),
                    format!("{}", app.subfilter_count(c, sub)),
                    app.sel == Sel::Subfilter(c, sub),
                    move |app: &mut Workspace| app.sel = Sel::Subfilter(c, sub),
                )));
            }
        }
    }

    // Languages: script groups with their coverage sets under them.
    let mut lang_rows: Vec<_> = Vec::new();
    for (i, g) in runebender::ui::sidebar::language_groups()
        .iter()
        .enumerate()
    {
        let open = app.expanded_scripts.contains(&i);
        let selected =
            app.sel == Sel::Language(i) || matches!(app.sel, Sel::LanguageFilter(gi, _) if gi == i);
        let marker = if g.filters.is_empty() {
            recipes::Marker::Bullet
        } else if open {
            recipes::Marker::Open
        } else {
            recipes::Marker::Closed
        };
        let expandable = !g.filters.is_empty();
        lang_rows.push(Either::A(recipes::list_row_marked(
            pal,
            marker,
            false,
            g.label.clone(),
            format!("{}", app.language_count(i)),
            app.sel == Sel::Language(i),
            move |app: &mut Workspace| {
                if expandable {
                    if selected {
                        if !app.expanded_scripts.remove(&i) {
                            app.expanded_scripts.insert(i);
                        }
                    } else {
                        app.expanded_scripts.insert(i);
                    }
                }
                app.sel = Sel::Language(i);
            },
        )));
        if open {
            for (fi, f) in g.filters.iter().enumerate() {
                let (present, expected) = app.language_filter_count(i, fi);
                let count = match expected {
                    Some(e) => format!("{present}/{e}"),
                    None => format!("{present}"),
                };
                lang_rows.push(Either::B(recipes::list_row_marked(
                    pal,
                    recipes::Marker::Bullet,
                    true,
                    f.label.clone(),
                    count,
                    app.sel == Sel::LanguageFilter(i, fi),
                    move |app: &mut Workspace| app.sel = Sel::LanguageFilter(i, fi),
                )));
            }
        }
    }

    let filter_rows: Vec<_> = runebender::ui::sidebar::builtin_filters()
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            let gs = b.glyphset.as_ref()?;
            let expected = gs
                .expected_count
                .unwrap_or(gs.glyph_names.len().max(gs.targets.len()));
            // A row that is short of its target gets a plus that adds
            // the glyphs it is missing, named and encoded, to every
            // master. Selecting the row and filling it are different
            // buttons on purpose: one is navigation, one writes.
            // No plus in the row: filter rows are
            // plain, and generating the missing glyphs is a command.
            let count = format!("{}/{}", app.filter_present(i), expected);
            let selected = app.sel == Sel::Filter(i);
            Some(recipes::list_row(
                pal,
                b.label.clone(),
                count,
                selected,
                move |app: &mut Workspace| app.sel = Sel::Filter(i),
            ))
        })
        .collect();

    let total = |name: &'static str, value: usize| {
        // Resolve the row's state here rather than through the boxed filter list.
        sized_box(xrow::<Workspace, (), _>(
            Region::Inline,
            (
                label(name).text_size(TextSize::Body.px()).color(pal.text),
                FlexSpacer::Flex(1.0),
                label(value.to_string())
                    .text_size(TextSize::Body.px())
                    .color(pal.text_muted),
            ),
        ))
        .padding(masonry::properties::Padding::horizontal(Length::px(
            design::SIDEBAR_ROW_INSET,
        )))
        .dims(Dimensions::new(
            Dim::Stretch,
            Dim::from(ControlSize::SidebarRow),
        ))
        .boxed()
    };
    let mut filters = vec![
        total("Exporting glyphs", app.font.exporting_count()),
        total("Incompatible masters", app.font.incompatible_count()),
    ];
    filters.extend(filter_rows.into_iter().map(|row| row.boxed()));
    flex_col((
        xcolumn(
            Region::List,
            (
                glyph_search(app),
                (!app.filter.trim().is_empty() && app.font.index_of(app.filter.trim()).is_none())
                    .then(|| {
                        recipes::action(
                            pal,
                            format!("+ New {}", app.filter.trim()),
                            |app: &mut Workspace| app.new_glyph(),
                        )
                    }),
            ),
        )
        .padding(Space::Md),
        recipes::hairline(pal),
        portal(recipes::panel_stack((
            sidebar_group(app, "Categories", cat_rows),
            (!lang_rows.is_empty()).then(|| sidebar_group(app, "Global Scripts", lang_rows)),
            sidebar_group(app, "Filters", filters),
        )))
        .trailing_rule_clearance(design::PANEL_SECTION_INSET.px())
        .constrain_horizontal(true)
        .flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Space::None)
}
