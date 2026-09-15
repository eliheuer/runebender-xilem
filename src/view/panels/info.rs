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
        Mode::Overview | Mode::Nodes => {
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
    let nodes = matches!(app.mode, Mode::Nodes);
    // Width, LSB, and RSB share one row. Each field commits
    // live; LSB shifts the glyph, RSB changes the advance.
    let field_bg = pal.field();
    let _ = field_bg;
    let advance_field = editing.then(|| {
        xrow(
            Region::Form,
            (
                // Fixed widths: three flexed fields ask for more than the
                // column has and push the whole inspector wide.
                third(recipes::field(
                    pal,
                    "Width",
                    app.advance_buf.clone(),
                    |app: &mut Workspace, v| app.set_advance_from_buf(v),
                )),
                third(recipes::field(
                    pal,
                    "LSB",
                    app.lsb_buf.clone(),
                    |app: &mut Workspace, v| app.set_lsb_from_buf(v),
                )),
                third(recipes::field(
                    pal,
                    "RSB",
                    app.rsb_buf.clone(),
                    |app: &mut Workspace, v| app.set_rsb_from_buf(v),
                )),
            ),
        )
    });
    fn third<V: WidgetView<Workspace> + 'static>(v: V) -> impl WidgetView<Workspace> + use<V> {
        sized_box(v).dims(Dimensions::new(Dim::Fixed(Length::px(72.0)), Dim::Auto))
    }
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
                // Kerning groups, left side then right. The Glyph
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
        let master = app
            .font
            .master_names()
            .get(app.font.active())
            .cloned()
            .unwrap_or_default();
        xcolumn(
            Region::Form,
            (
                sized_box(xrow(
                    Region::Inline,
                    (
                        label("Master")
                            .text_size(TextSize::Body.px())
                            .color(pal.text_muted),
                        FlexSpacer::Flex(1.0),
                        label(master).text_size(TextSize::Body.px()).color(pal.text),
                    ),
                ))
                .dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::Fixed(Length::px(design::GLYPH_FACT_ROW_HEIGHT)),
                )),
                overview_identity_field(
                    pal,
                    "Glyph name",
                    app.name_buf.clone(),
                    |app: &mut Workspace, v| app.name_buf = v,
                    |app: &mut Workspace, v| app.overview_rename(v),
                ),
                overview_identity_field(
                    pal,
                    "Width",
                    app.advance_buf.clone(),
                    |app: &mut Workspace, v| app.overview_set_advance(v),
                    |_: &mut Workspace, _| {},
                ),
                overview_identity_field(
                    pal,
                    "Unicode",
                    app.unicode_buf.clone(),
                    |app: &mut Workspace, v| app.overview_set_unicode(v),
                    |_: &mut Workspace, _| {},
                ),
            ),
        )
    });
    let show_multi_mark = !editing && !app.multi_selected.is_empty();
    let glyph_section = xcolumn(
        Region::Section,
        (
            recipes::section_toggle_height(
                pal,
                "Glyph",
                !app.collapsed.contains("Glyph"),
                if nodes && app.collapsed.contains("Glyph") {
                    design::NODE_VIEW_SECTION_HEADER_HEIGHT
                } else {
                    ControlSize::Row.px()
                },
                move |app: &mut Workspace| {
                    if !app.collapsed.remove("Glyph") {
                        app.collapsed.insert("Glyph");
                    }
                },
            ),
            (!app.collapsed.contains("Glyph") && (show_multi_mark || !pts.is_empty() || editing))
                .then(|| {
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
            (!app.collapsed.contains("Glyph")).then(|| show_multi_mark.then(|| mark_section(app))),
            (!app.collapsed.contains("Glyph")).then_some(name_field),
            (!app.collapsed.contains("Glyph")).then_some(advance_field),
            (!app.collapsed.contains("Glyph")).then_some(overview_fields),
        ),
    )
    .gap(if editing { Space::Md } else { Space::Sm });
    xcolumn(
        Region::List,
        (
            // GPUI leads edit mode with selection geometry and its tools;
            // the overview still begins with glyph identity because these
            // optional edit groups disappear there.
            editing.then(|| recipes::inspector_group(pal, coordinates_section(app))),
            editing.then(|| recipes::inspector_group(pal, transformations_section(app))),
            editing.then(|| recipes::inspector_group(pal, curves_section(app))),
            editing.then(|| recipes::inspector_group(pal, path_operations_section(app))),
            recipes::inspector_group(pal, glyph_section),
            editing.then(|| recipes::inspector_group(pal, background_section(app))),
            editing.then(|| {
                xcolumn(
                    Region::List,
                    (
                        recipes::inspector_group(pal, mark_section(app)),
                        recipes::inspector_group(pal, shaping_section(app)),
                    ),
                )
                .gap(Space::None)
            }),
            editing.then(|| recipes::inspector_group(pal, related_section(app))),
            // One column for three sections: the panel's tuple is at
            // Xilem's sixteen-child limit, so the overview's first
            // three sections share a slot. Same region as the panel,
            // so the gap between them is the panel's own.
            (!editing).then(|| {
                xcolumn(
                    Region::List,
                    (
                        recipes::inspector_group(pal, font_info_section(app)),
                        recipes::inspector_group(pal, dimensions_section(app)),
                        recipes::inspector_group(pal, font_advanced_section(app)),
                    ),
                )
                .gap(Space::None)
            }),
            (!editing).then(|| recipes::inspector_group(pal, kerning_section(app))),
            (!editing).then(|| recipes::inspector_group(pal, groups_section(app))),
            (!editing).then(|| recipes::inspector_group(pal, compare_section(app))),
            (!editing).then(|| recipes::inspector_group(pal, features_section(app))),
            layers_section(app).map(|body| recipes::inspector_group(pal, body)),
            xcolumn(
                Region::List,
                (
                    masters_section(app).map(|body| recipes::inspector_group(pal, body)),
                    editing
                        .then(|| axes_section(app))
                        .flatten()
                        .map(|body| recipes::inspector_group(pal, body)),
                ),
            )
            .gap(Space::None),
            editing.then(|| recipes::inspector_group(pal, measure_section(app))),
        ),
    )
    .gap(Space::None)
    .padding(Space::None)
    .background_color(pal.panel)
}

/// Overview identity fields use the reference's fixed label line box.
/// Existing callbacks retain their commit rules: rename on Enter, other edits on change.
fn overview_identity_field<F, G>(
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
    xcolumn(
        Region::List,
        (
            label(name)
                .text_size(TextSize::Body.px())
                .color(pal.text_muted)
                .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Row))),
            sized_box(input_typography::input_typography(
                text_input(value, on_change)
                    .on_enter(on_enter)
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
