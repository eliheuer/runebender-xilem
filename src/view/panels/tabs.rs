// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The left edge: the tab strip, the editor's rail, and the sidebar of
//! categories, languages, and filters.

use crate::view::design::{RAIL_TAB_ACTIVE_HEIGHT, RAIL_TAB_HEIGHT, RAIL_TAB_INACTIVE_HEIGHT};
use crate::*;
use xilem::Color;

/// The editor rail's implemented navigation panels.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rail {
    Glyphs,
    Shapes,
    Axes,
    /// Local models: the same panel the GPUI build keeps on this rail.
    LocalAi,
    /// Chat availability, until the conversation backend is connected.
    Chat,
}

/// Shared search controls keep filtering identical in overview and the editor rail.
fn glyph_search(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    // Search row: the field, then gpui's small scope and case toggles.
    let toggle = |text: String, active: bool, f: fn(&mut Workspace)| {
        recipes::toggle(pal, text, active, move |app: &mut Workspace| f(app))
    };
    xrow(
        Region::Inline,
        (
            // A field, not a well: one step darker than the
            // panel with a quiet outline, as every field is.
            input_typography::input_typography(
                text_input(app.filter.clone(), |app: &mut Workspace, v| {
                    app.filter = v;
                    app.rebuild_search_regex();
                })
                .placeholder("Search glyphs")
                .text_color(pal.text)
                .placeholder_color(pal.text_muted)
                .background_color(pal.field())
                .border_color(pal.field_outline)
                .border_width(Stroke::Hairline.length())
                .corner_radius(Radius::None.length())
                .dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::from(ControlSize::Control),
                )),
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

/// Contour rows select real points; components remain informational until
/// the editor supports component selection and transforms.
fn shapes_panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let contours = app
        .session
        .glyph
        .contours
        .iter()
        .enumerate()
        .map(|(ci, contour)| {
            let active = app.session.selection.iter().any(|&(index, _)| index == ci);
            recipes::toggle(
                pal,
                format!("Contour {} · {} nodes", ci + 1, contour.points.len()),
                active,
                move |app: &mut Workspace| {
                    let Some(contour) = app.session.glyph.contours.get(ci) else {
                        return;
                    };
                    let points = contour.points.len();
                    let session = Arc::make_mut(&mut app.session);
                    session.selection = (0..points).map(|pi| (ci, pi)).collect();
                    app.selected_points = points;
                    app.refresh_coord_bufs();
                },
            )
            .boxed()
        })
        .collect::<Vec<_>>();
    let components = app
        .session
        .glyph
        .components
        .iter()
        .map(|component| recipes::kv(pal, component.base.to_string(), "component".into()).boxed())
        .collect::<Vec<_>>();
    xcolumn(
        Region::Panel,
        (
            contours,
            components,
            (app.session.glyph.contours.is_empty() && app.session.glyph.components.is_empty())
                .then(|| label("No contours or components").color(pal.text_muted)),
        ),
    )
    .boxed()
}

/// One navigation strip for the font, node, and glyph workspaces.
fn rail_tabs(app: &Workspace, editing: bool) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let tab = |icon: &'static str, which: Rail| {
        let active =
            app.rail == which || (!editing && app.rail == Rail::Shapes && which == Rail::Glyphs);
        sized_box(
            icon_button(
                icon,
                active,
                pal.text.with_alpha(0.42),
                pal.text,
                Color::TRANSPARENT,
                Color::TRANSPARENT,
                move |app: &mut Workspace| app.rail = which,
            )
            .rail_tab(
                if active { pal.panel } else { pal.inactive_tab },
                pal.outline,
            ),
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
    let has_axes = !app.font.axes.is_empty();
    sized_box(xilem::view::zstack((
        sized_box(canvas({
            let background = pal.tab_rail;
            let outline = pal.outline;
            move |_: &mut Workspace, _, scene, size| {
                use masonry::imaging::Painter;
                use masonry::kurbo::Rect;
                let mut painter = Painter::new(scene);
                let stroke = Stroke::Hairline.px();
                painter
                    .fill(Rect::new(0.0, 0.0, size.width, size.height), background)
                    .draw();
                for edge in [
                    Rect::new(0.0, 0.0, stroke, size.height),
                    Rect::new(0.0, size.height - stroke, size.width, size.height),
                ] {
                    painter.fill(edge, outline).draw();
                }
            }
        }))
        .dims(Dimensions::new(Dim::Stretch, Dim::Stretch)),
        flex_row((
            tab("glyph-grid", Rail::Glyphs).flex(1.0),
            editing.then(|| tab("shapes", Rail::Shapes).flex(1.0)),
            has_axes.then(|| tab("measure", Rail::Axes).flex(1.0)),
            tab("preview", Rail::LocalAi).flex(1.0),
            tab("text", Rail::Chat).flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Space::Sm)
        .padding(masonry::properties::Padding {
            left: Space::Sm.length(),
            right: Space::Sm.length(),
            top: Space::Sm.length(),
            bottom: Space::None.length(),
        }),
    )))
    .dims(Dimensions::new(
        Dim::Stretch,
        Dim::Fixed(Length::px(RAIL_TAB_HEIGHT)),
    ))
}

pub(crate) fn editor_nav(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let current = match app.mode {
        Mode::Editor(i) => Some(i),
        _ => None,
    };
    flex_col((
        rail_tabs(app, true),
        xcolumn(
            Region::Panel,
            (
                (app.rail == Rail::Axes)
                    .then(|| axes_section(app))
                    .flatten(),
                (app.rail == Rail::LocalAi).then(|| local_ai_panel(app)),
                (app.rail == Rail::Shapes).then(|| shapes_panel(app)),
                (app.rail == Rail::Chat).then(|| chat_panel(app)),
                (app.rail == Rail::Glyphs).then(|| glyph_search(app)),
                // The grid scrolls itself, so no portal here: nesting the two
                // gave the rail a dead area below the third row.
                (app.rail == Rail::Glyphs).then(|| {
                    sized_box(grid(
                        app.filtered_cells(),
                        // GPUI's editor rail keeps a five-column thumbnail
                        // index in this width. The overview remains at the
                        // user-controlled 96px target; only this compact
                        // navigation grid uses smaller cards and insets.
                        app.rail_cell_metrics(),
                        app.palette.clone(),
                        current,
                        app.multi_selected.clone(),
                        |app: &mut Workspace, ev| match ev {
                            GridEvent::Selected { index, .. } => app.open_glyph(index),
                            GridEvent::Open(i) => app.open_glyph(i),
                        },
                    ))
                    .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
                    .flex(1.0)
                }),
            ),
        )
        .padding(Space::Md)
        .gap(Space::Md)
        .flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Space::None)
    .background_color(pal.panel)
}

/// Curves: the two analyses that are about shape quality rather than
/// measurement. Both read from runebender-core, so they say the same
/// thing here as in the other two editors.
/// The tab strip: one tab per open glyph, with a close box, and a plus
/// that opens a second view on the glyph in hand.
/// The tab strip, in the title bar, as the GPUI build has it.
///
/// A "Font" tab that is active in the overview, one tab per open glyph
/// with a close button, and a "+" that opens a second tab on the glyph
/// in hand. Tabs are outlined rather than filled: accent when active,
/// the grid border when not, which is the GPUI build's rule.
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
    // Selection is inversion: an active tab is a filled block of ink
    // with the panel colour for its label, the GPUI build's rule.
    let (fg, border, bg) = if active {
        (pal.selected_ink(), pal.selected_bg(), pal.selected_bg())
    } else {
        (pal.text_muted, pal.outline, pal.panel)
    };
    let width = if fixed_width {
        Dim::from(ControlSize::Row)
    } else {
        Dim::Auto
    };
    sized_box(
        button(
            label(text).text_size(TextSize::Body.px()).color(fg),
            move |app: &mut Workspace| on_click(app),
        )
        .background_color(bg)
        .border_color(border)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Radius::Sm.length())
        // The stock button's padding is sized for a form button, not
        // for a chip in a title bar. GPUI writes `px_2`.
        .padding(Space::Md),
    )
    .dims(Dimensions::new(width, Dim::from(ControlSize::Row)))
}

/// A title-bar tab: GPUI keeps the header quiet and marks the active
/// workspace with a brighter neutral keyline, matching the GPUI reference.
fn header_tab_chip<F>(
    pal: &Palette,
    text: String,
    active: bool,
    fixed_width: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F>
where
    F: Fn(&mut Workspace) + Send + Sync + 'static,
{
    let (fg, border) = if active {
        (pal.header_ink, pal.header_ink)
    } else {
        (
            pal.header_ink.with_alpha(0.7),
            pal.header_ink.with_alpha(0.5),
        )
    };
    let width = if fixed_width {
        Dim::from(ControlSize::Row)
    } else {
        Dim::Auto
    };
    sized_box(
        button(
            label(text).text_size(TextSize::Body.px()).color(fg),
            move |app: &mut Workspace| on_click(app),
        )
        .background_color(pal.header)
        .border_color(border)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Radius::Sm.length())
        .padding(Space::Md),
    )
    .dims(Dimensions::new(width, Dim::from(ControlSize::Row)))
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
            xrow(
                Region::Inline,
                (
                    header_tab_chip(
                        pal,
                        tab.session.glyph_name.clone(),
                        editing && index == active,
                        false,
                        move |app: &mut Workspace| app.activate_tab(index),
                    ),
                    closable.then(|| {
                        header_tab_chip(
                            pal,
                            "\u{00d7}".into(),
                            false,
                            true,
                            move |app: &mut Workspace| app.close_tab(index),
                        )
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
                false,
                |app: &mut Workspace| app.back_to_overview(),
            ),
            // Nodes sits beside Font: the workflow over the font, as
            // boxes and wires.
            header_tab_chip(
                pal,
                "Nodes".into(),
                matches!(app.mode, Mode::Nodes),
                false,
                |app: &mut Workspace| app.enter_nodes_mode(),
            ),
            xrow(Region::Inline, tabs),
            header_tab_chip(pal, "+".into(), false, true, |app: &mut Workspace| {
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
    let open = !app.collapsed.contains(title);
    let rule = app.palette.outline;
    flex_col((
        xcolumn(
            Region::Section,
            (
                recipes::section_toggle(&app.palette, title, open, move |app: &mut Workspace| {
                    if !app.collapsed.remove(title) {
                        app.collapsed.insert(title);
                    }
                }),
                open.then(|| xcolumn(Region::List, rows).gap(Space::None)),
            ),
        )
        .padding(Space::Md)
        .gap(Space::Sm),
        sized_box(label(""))
            .dims(Dimensions::new(
                Dim::Stretch,
                Dim::Fixed(Stroke::Hairline.length()),
            ))
            .background_color(rule),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Space::None)
}

pub(crate) fn sidebar(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let content = match app.rail {
        Rail::Axes if !app.font.axes.is_empty() => flex_col((axes_section(app),)).boxed(),
        Rail::LocalAi => local_ai_panel(app).boxed(),
        Rail::Chat => chat_panel(app).boxed(),
        _ => category_sidebar(app).boxed(),
    };
    flex_col((rail_tabs(app, false), content.flex(1.0)))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Space::None)
        .background_color(app.palette.panel)
}

fn category_sidebar(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use xilem::core::one_of::Either;
    let pal = &app.palette;

    let cats = [
        GlyphCategory::All,
        GlyphCategory::Letter,
        GlyphCategory::Number,
        GlyphCategory::Punctuation,
        GlyphCategory::Symbol,
        GlyphCategory::Mark,
        GlyphCategory::Other,
    ];
    // Categories: a chevron on one with subfilters, a bullet on one
    // without. Clicking a selected expandable row folds it, as the
    // GPUI sidebar does; the rows under it are indented leaves.
    let mut cat_rows: Vec<_> = Vec::new();
    for c in cats.into_iter().filter(|c| app.category_count(*c) > 0) {
        let subs = runebender_core::ui::sidebar::category_subfilters(c.display_name());
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
    for (i, g) in runebender_core::ui::sidebar::language_groups()
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

    let filter_rows: Vec<_> = runebender_core::ui::sidebar::builtin_filters()
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
            // No plus in the row: the GPUI build's Filters rows are
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

    let mut filters = vec![
        recipes::kv(
            pal,
            "Exporting glyphs".into(),
            format!("{}", app.font.exporting_count()),
        )
        .boxed(),
        recipes::kv(
            pal,
            "Incompatible masters".into(),
            format!("{}", app.font.incompatible_count()),
        )
        .boxed(),
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
        sized_box(label(""))
            .dims(Dimensions::new(
                Dim::Stretch,
                Dim::Fixed(Stroke::Hairline.length()),
            ))
            .background_color(pal.outline),
        portal(
            flex_col((
                sidebar_group(app, "Categories", cat_rows),
                (!lang_rows.is_empty()).then(|| sidebar_group(app, "Global Scripts", lang_rows)),
                sidebar_group(app, "Filters", filters),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(Space::None),
        )
        .constrain_horizontal(true)
        .flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Space::None)
    .background_color(pal.panel)
}
