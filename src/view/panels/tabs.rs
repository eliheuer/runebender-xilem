// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The left edge: the tab strip, the editor's rail, and the sidebar of
//! categories, languages, and filters.

use crate::*;

/// The editor rail's tabs. The GPUI build has four (Glyphs, Shapes,
/// Axes, Chat); these are the two this editor has something to put in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rail {
    Glyphs,
    Axes,
}

pub(crate) fn editor_nav(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let current = match app.mode {
        Mode::Editor(i) => Some(i),
        _ => None,
    };
    let tab = |text: &'static str, which: Rail| {
        tab_chip(
            pal,
            text.into(),
            app.rail == which,
            false,
            move |app: &mut Workspace| {
                app.rail = which;
            },
        )
    };
    xcolumn(
        Region::Panel,
        (
            xrow(
                Region::Inline,
                (tab("Glyphs", Rail::Glyphs), tab("Axes", Rail::Axes)),
            ),
            (app.rail == Rail::Axes)
                .then(|| axes_section(app))
                .flatten(),
            (app.rail == Rail::Glyphs).then(|| {
                text_input(app.filter.clone(), |app: &mut Workspace, v| app.filter = v)
                    .placeholder("Search")
            }),
            // The grid scrolls itself, so no portal here: nesting the two
            // gave the rail a dead area below the third row.
            (app.rail == Rail::Glyphs).then(|| {
                grid(
                    app.filtered_cells(),
                    app.cell_metrics(62.0),
                    app.palette.clone(),
                    current,
                    app.multi_selected.clone(),
                    |app: &mut Workspace, ev| match ev {
                        GridEvent::Selected { index, .. } => app.open_glyph(index),
                        GridEvent::Open(i) => app.open_glyph(i),
                    },
                )
                .flex(1.0)
            }),
        ),
    )
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
    let (fg, border) = if active {
        (pal.role("accent"), pal.role("accent"))
    } else {
        (pal.text_muted, pal.role("gridBorder"))
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
        .background_color(pal.panel)
        .border_color(border)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Radius::Sm.length())
        // The stock button's padding is sized for a form button, not
        // for a chip in a title bar. GPUI writes `px_2`.
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
                    tab_chip(
                        pal,
                        tab.session.glyph_name.clone(),
                        editing && index == active,
                        false,
                        move |app: &mut Workspace| app.activate_tab(index),
                    ),
                    closable.then(|| {
                        tab_chip(
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
            tab_chip(
                pal,
                "Font".into(),
                !editing,
                false,
                |app: &mut Workspace| app.back_to_overview(),
            ),
            xrow(Region::Inline, tabs),
            tab_chip(pal, "+".into(), false, true, |app: &mut Workspace| {
                app.new_tab()
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
    xcolumn(
        Region::Section,
        (
            recipes::section_toggle(&app.palette, title, open, move |app: &mut Workspace| {
                if !app.collapsed.remove(title) {
                    app.collapsed.insert(title);
                }
            }),
            open.then(|| xcolumn(Region::List, rows)),
        ),
    )
}

pub(crate) fn sidebar(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
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
    let cat_rows: Vec<_> = cats
        .into_iter()
        .filter(|c| app.category_count(*c) > 0)
        .map(|c| {
            recipes::list_row(
                pal,
                c.display_name().to_string(),
                format!("{}", app.category_count(c)),
                app.sel == Sel::Category(c),
                move |app: &mut Workspace| app.sel = Sel::Category(c),
            )
        })
        .collect();

    let lang_rows: Vec<_> = runebender_core::ui::sidebar::language_groups()
        .iter()
        .enumerate()
        // Every script, including the ones this font has nothing for.
        // A zero is information: it says the coverage is not there.
        .map(|(i, g)| {
            recipes::list_row_with_icon(
                pal,
                g.icon.clone(),
                g.label.clone(),
                format!("{}", app.language_count(i)),
                app.sel == Sel::Language(i),
                move |app: &mut Workspace| app.sel = Sel::Language(i),
            )
        })
        .collect();

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
            let missing = app.filter_missing(i);
            let count = format!("{}/{}", app.filter_present(i), expected);
            let selected = app.sel == Sel::Filter(i);
            Some(if missing > 0 {
                Either::A(recipes::list_row_with_action(
                    pal,
                    b.label.clone(),
                    count,
                    selected,
                    move |app: &mut Workspace| app.sel = Sel::Filter(i),
                    "+".into(),
                    move |app: &mut Workspace| app.generate_missing(i),
                ))
            } else {
                Either::B(recipes::list_row(
                    pal,
                    b.label.clone(),
                    count,
                    selected,
                    move |app: &mut Workspace| app.sel = Sel::Filter(i),
                ))
            })
        })
        .collect();

    // Search row: the field, then gpui's small scope and case toggles.
    let toggle = |text: String, active: bool, f: fn(&mut Workspace)| {
        recipes::toggle(pal, text, active, move |app: &mut Workspace| f(app))
    };
    xcolumn(
        Region::Panel,
        (
            xrow(
                Region::Inline,
                (
                    text_input(app.filter.clone(), |app: &mut Workspace, v| app.filter = v)
                        .placeholder("Search")
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
                    toggle("Aa".into(), app.search_case, |app: &mut Workspace| {
                        app.search_case = !app.search_case
                    }),
                ),
            ),
            text_button(
                match app.sort {
                    Sort::Name => "Sort: name",
                    Sort::Unicode => "Sort: unicode",
                },
                |app: &mut Workspace| {
                    app.sort = match app.sort {
                        Sort::Name => Sort::Unicode,
                        Sort::Unicode => Sort::Name,
                    };
                },
            )
            .background_color(pal.button),
            {
                let fresh =
                    !app.filter.trim().is_empty() && app.font.index_of(app.filter.trim()).is_none();
                fresh.then(|| {
                    text_button(
                        format!("+ New {}", app.filter.trim()),
                        |app: &mut Workspace| app.new_glyph(),
                    )
                    .background_color(pal.role("accent"))
                })
            },
            // Constrained horizontally, or the rows lay out at their
            // intrinsic width and the sidebar grows a horizontal scrollbar
            // with the counts cut off past the edge.
            // A card, not a panel: this column is already inside the
            // sidebar panel, and two panel insets in a row take 48px out of
            // a 200px column, which is where the counts went. The card's
            // smaller inset also keeps the counts clear of the scrollbar,
            // which the portal draws over its own right edge.
            portal(xcolumn(
                Region::Card,
                (
                    sidebar_group(app, "Categories", cat_rows),
                    (!lang_rows.is_empty()).then(|| sidebar_group(app, "Languages", lang_rows)),
                    (!filter_rows.is_empty()).then(|| sidebar_group(app, "Filters", filter_rows)),
                    // Two counts the GPUI build puts at the head of its filters:
                    // how much of the font exports, and how much of it the
                    // masters disagree about. Neither is a filter to click, so
                    // they are rows, not buttons.
                    xcolumn(
                        Region::List,
                        (
                            recipes::kv(
                                pal,
                                "Exporting glyphs".into(),
                                format!("{}", app.font.exporting_count()),
                            ),
                            recipes::kv(
                                pal,
                                "Incompatible masters".into(),
                                format!("{}", app.font.incompatible_count()),
                            ),
                        ),
                    ),
                ),
            ))
            .constrain_horizontal(true)
            .flex(1.0),
        ),
    )
    .background_color(pal.panel)
}
