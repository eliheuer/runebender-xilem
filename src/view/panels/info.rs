// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The info panel: which sections show for the grid and for a glyph.

use crate::*;
use runebender_core::outline::glyph_paths::round_units;

pub(crate) fn info_panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let row = move |k: String, v: String| recipes::kv(pal, k, v);
    let (_name, _adv, pts, _cp) = match app.mode {
        Mode::Editor(_) => (
            app.session.glyph_name.clone(),
            format!("{}", round_units(app.session.advance())),
            format!("{}", app.session.point_count()),
            String::new(),
        ),
        Mode::Overview => {
            let g = app.selected.and_then(|i| app.font.glyphs.get(i));
            (
                g.map(|g| g.name.clone()).unwrap_or_default(),
                g.map(|g| format!("{}", round_units(g.advance)))
                    .unwrap_or_default(),
                String::new(),
                g.and_then(|g| g.codepoint)
                    .map(|c| format!("U+{:04X}", c as u32))
                    .unwrap_or_default(),
            )
        }
    };
    let editing = matches!(app.mode, Mode::Editor(_));
    // Width / LSB / RSB in one row (gpui's metrics row). Each field commits
    // live; LSB shifts the glyph, RSB changes the advance.
    let field_bg = pal.field();
    let _ = field_bg;
    let advance_field = editing.then(|| {
        xrow(
            Region::Form,
            (
                recipes::field(
                    pal,
                    "Width",
                    app.advance_buf.clone(),
                    |app: &mut Workspace, v| app.set_advance_from_buf(v),
                )
                .flex(1.0),
                recipes::field(pal, "LSB", app.lsb_buf.clone(), |app: &mut Workspace, v| {
                    app.set_lsb_from_buf(v);
                })
                .flex(1.0),
                recipes::field(pal, "RSB", app.rsb_buf.clone(), |app: &mut Workspace, v| {
                    app.set_rsb_from_buf(v);
                })
                .flex(1.0),
            ),
        )
    });
    let name_field = editing.then(|| {
        xcolumn(
            Region::Form,
            (
                // Renaming waits for Enter: it rewrites every master and
                // every component reference, which is not a per-keystroke
                // operation.
                recipes::field_enter(
                    pal,
                    "Name",
                    app.name_buf.clone(),
                    |app: &mut Workspace, v| app.name_buf = v,
                    |app: &mut Workspace, v| {
                        app.name_buf = v;
                        app.commit_rename();
                    },
                ),
                recipes::field(
                    pal,
                    "Unicode",
                    app.unicode_buf.clone(),
                    |app: &mut Workspace, v| app.set_unicode_from_buf(v),
                ),
                // Kerning groups, left side then right, as gpui's Glyph
                // panel has them. Empty takes the glyph out of the group,
                // and the write lands in every master, because a
                // designspace's masters have to agree about groups.
                label("Kerning Groups (L \u{00b7} R)")
                    .text_size(TextSize::Caption.px())
                    .color(pal.text_muted),
                // Fixed widths: a group name is long enough that letting
                // the inputs size to their content pushes the whole
                // inspector past its column.
                xrow(
                    Region::Form,
                    (
                        sized_box(recipes::field(
                            pal,
                            "",
                            app.kern1_buf.clone(),
                            |app: &mut Workspace, v| app.set_kern_group(true, v),
                        ))
                        .dims(Dimensions::new(Dim::Fixed(Length::px(105.0)), Dim::Auto)),
                        sized_box(recipes::field(
                            pal,
                            "",
                            app.kern2_buf.clone(),
                            |app: &mut Workspace, v| app.set_kern_group(false, v),
                        ))
                        .dims(Dimensions::new(Dim::Fixed(Length::px(105.0)), Dim::Auto)),
                    ),
                ),
            ),
        )
    });
    // The overview panel used to be three read-only rows, and the GPUI
    // build lets you rename a glyph, set its codepoint and set its width
    // without opening it. These write to the highlighted cell.
    let overview_fields = (!editing && app.selected.is_some()).then(|| {
        xcolumn(
            Region::Form,
            (
                recipes::field_enter(
                    pal,
                    "Name",
                    app.name_buf.clone(),
                    |app: &mut Workspace, v| app.name_buf = v,
                    |app: &mut Workspace, v| app.overview_rename(v),
                ),
                recipes::field(
                    pal,
                    "Unicode",
                    app.unicode_buf.clone(),
                    |app: &mut Workspace, v| app.overview_set_unicode(v),
                ),
                recipes::field(
                    pal,
                    "Advance",
                    app.advance_buf.clone(),
                    |app: &mut Workspace, v| app.overview_set_advance(v),
                ),
            ),
        )
    });
    let show_multi_mark = !editing && !app.multi_selected.is_empty();
    xcolumn(
        Region::Panel,
        (
            xcolumn(
                Region::Section,
                (
                    recipes::section_toggle(
                        pal,
                        "Glyph",
                        !app.collapsed.contains("Glyph"),
                        move |app: &mut Workspace| {
                            if !app.collapsed.remove("Glyph") {
                                app.collapsed.insert("Glyph");
                            }
                        },
                    ),
                    (!app.collapsed.contains("Glyph")).then(|| {
                        xcolumn(
                            Region::List,
                            (
                                show_multi_mark.then(|| {
                                    row("Selected".into(), format!("{}", app.multi_selected.len()))
                                }),
                                (!pts.is_empty()).then(|| row("Points".into(), pts)),
                                editing.then(|| {
                                    row("Selected".into(), format!("{}", app.selected_points))
                                }),
                            ),
                        )
                    }),
                    (!app.collapsed.contains("Glyph"))
                        .then(|| show_multi_mark.then(|| mark_section(app))),
                    (!app.collapsed.contains("Glyph")).then_some(name_field),
                    (!app.collapsed.contains("Glyph")).then_some(advance_field),
                    (!app.collapsed.contains("Glyph")).then_some(overview_fields),
                ),
            ),
            editing.then(|| coordinates_section(app)),
            editing.then(|| path_section(app)),
            editing.then(|| curves_section(app)),
            editing.then(|| measure_section(app)),
            editing.then(|| background_section(app)),
            editing.then(|| mark_section(app)),
            layers_section(app),
            (!editing).then(|| font_info_section(app)),
            (!editing).then(|| glyph_preview(app)).flatten(),
        ),
    )
    .background_color(pal.panel)
}
