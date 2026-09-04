// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The List view: one row per glyph, one column per property, behind
//! the bottom bar's List box. The same columns as the GPUI build's
//! `glyph_list_view`: a mark cell, the name, Unicode, width, LSB,
//! RSB, the two kerning groups, and the category.

use crate::*;

/// Column widths, the GPUI build's.
const W_UNI: f64 = 68.0;
const W_NUM: f64 = 52.0;
const W_GROUP: f64 = 84.0;
const W_CAT: f64 = 92.0;

pub(crate) fn glyph_list(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    fn col<V: WidgetView<Workspace> + 'static>(
        w: f64,
        v: V,
    ) -> impl WidgetView<Workspace> + use<V> {
        sized_box(v).dims(Dimensions::new(Dim::Fixed(Length::px(w)), Dim::Auto))
    }
    let head = |text: &'static str, w: f64| {
        col(
            w,
            label(text)
                .text_size(TextSize::Body.px())
                .color(pal.text_muted),
        )
    };
    let header = xrow(
        Region::Inline,
        (
            col(14.0, label("")),
            label("Name")
                .text_size(TextSize::Body.px())
                .color(pal.text_muted)
                .flex(1.0),
            head("Unicode", W_UNI),
            head("Width", W_NUM),
            head("LSB", W_NUM),
            head("RSB", W_NUM),
            head("Group L", W_GROUP),
            head("Group R", W_GROUP),
            head("Category", W_CAT),
        ),
    );
    let cells = app.filtered_cells();
    let rows: Vec<_> = cells
        .iter()
        .map(|cell| {
            let index = cell.index;
            let entry = &app.font.glyphs[index];
            let selected = app.selected == Some(index) || app.multi_selected.contains(&index);
            let (fg, bg) = if selected {
                (pal.role("cellSelectedInk"), pal.role("cellSelectedFill"))
            } else {
                (pal.text, pal.panel)
            };
            let muted = if selected { fg } else { pal.text_muted };
            let (lsb, rsb) = if entry.ink.is_zero_area() {
                (String::new(), String::new())
            } else {
                (
                    format!("{:.0}", entry.ink.x0),
                    format!("{:.0}", entry.advance - entry.ink.x1),
                )
            };
            let strip = |g: String| g.replace("public.kern1.", "").replace("public.kern2.", "");
            let value = |text: String, w: f64| {
                col(w, label(text).text_size(TextSize::Body.px()).color(muted))
            };
            // The mark, painted the way the grid paints it: the fill
            // and the keyline, on a small cell.
            let mark_bg = cell.mark.unwrap_or(pal.panel);
            let mark_border = if cell.mark.is_some() {
                pal.mark_outline.unwrap_or(pal.outline)
            } else {
                xilem::Color::TRANSPARENT
            };
            let mark = sized_box(label(""))
                .dims(Dimensions::new(
                    Dim::Fixed(Length::px(12.0)),
                    Dim::Fixed(Length::px(12.0)),
                ))
                .background_color(mark_bg)
                .border_color(mark_border)
                .border_width(Stroke::Hairline.length())
                .corner_radius(Radius::Sm.length());
            sized_box(
                button(
                    xrow(
                        Region::Inline,
                        (
                            col(14.0, mark),
                            label(entry.name.clone())
                                .text_size(TextSize::Body.px())
                                .color(fg)
                                .flex(1.0),
                            value(
                                entry
                                    .codepoint
                                    .map(|c| format!("U+{:04X}", c as u32))
                                    .unwrap_or_default(),
                                W_UNI,
                            ),
                            value(format!("{:.0}", entry.advance), W_NUM),
                            value(lsb, W_NUM),
                            value(rsb, W_NUM),
                            value(strip(app.font.kern_group(&entry.name, true)), W_GROUP),
                            value(strip(app.font.kern_group(&entry.name, false)), W_GROUP),
                            value(entry.category.display_name().to_string(), W_CAT),
                        ),
                    ),
                    move |app: &mut Workspace| {
                        app.note.clear();
                        app.grid_select(index, false, false);
                    },
                )
                .background_color(bg)
                .border_width(Stroke::None.length())
                .padding(Space::None)
                .corner_radius(Radius::Sm.length()),
            )
            .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(24.0))))
        })
        .collect();
    xcolumn(
        Region::List,
        (
            header,
            portal(xcolumn(Region::List, rows))
                .constrain_horizontal(true)
                .flex(1.0),
        ),
    )
}
