// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The info panel: which sections show for the grid and for a glyph.

use crate::application::view::design::{ControlSize, Region, column as xcolumn, row as xrow};
use crate::application::view::panels::editor_info::{
    compare_section, dimensions_section, features_section, groups_section, kerning_section,
    related_section,
};
use crate::application::view::panels::sections::{
    axes_section, background_section, coordinates_section, curves_section, font_advanced_section,
    font_info_section, layers_section, mark_section, masters_section, measure_section,
    path_operations_section, shaping_section, transformations_section,
};
use crate::application::view::{design, recipes};
use crate::application::workspace::{Mode, Tool, Workspace};
use runebender::outline::glyph_paths::round_units;
use xilem::WidgetView;

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
    // A neural item has no advance, code point or kerning group.
    let neural = app.font.project.is_neural();
    let advance_field = (editing && !neural).then(|| {
        xrow(
            Region::Form,
            (
                // Equal columns constrain field contents to the available dock width.
                recipes::field_column(recipes::field(
                    pal,
                    "Width",
                    app.advance_buf.clone(),
                    |app: &mut Workspace, v| app.set_advance_from_buf(v),
                )),
                recipes::field_column(recipes::field(
                    pal,
                    "LSB",
                    app.lsb_buf.clone(),
                    |app: &mut Workspace, v| app.set_lsb_from_buf(v),
                )),
                recipes::field_column(recipes::field(
                    pal,
                    "RSB",
                    app.rsb_buf.clone(),
                    |app: &mut Workspace, v| app.set_rsb_from_buf(v),
                )),
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
                (!neural).then(|| {
                    recipes::field(
                        pal,
                        "Unicode",
                        app.unicode_buf.clone(),
                        |app: &mut Workspace, v| app.set_unicode_from_buf(v),
                    )
                }),
                // Kerning groups, left side then right. The Glyph
                // panel has them. Empty takes the glyph out of the group,
                // and the write lands in every master, because a
                // designspace's masters have to agree about groups.
                (!neural).then(|| {
                    recipes::labeled_control(
                        pal,
                        "Kerning Groups (L \u{00b7} R)",
                        // Group names scroll inside equal columns rather than widening the dock.
                        xrow(
                            Region::Form,
                            (
                                recipes::field_column(recipes::field(
                                    pal,
                                    "",
                                    app.kern1_buf.clone(),
                                    |app: &mut Workspace, v| app.set_kern_group(true, v),
                                )),
                                recipes::field_column(recipes::field(
                                    pal,
                                    "",
                                    app.kern2_buf.clone(),
                                    |app: &mut Workspace, v| app.set_kern_group(false, v),
                                )),
                            ),
                        ),
                    )
                }),
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
                    "Glyph name",
                    app.name_buf.clone(),
                    |app: &mut Workspace, v| app.name_buf = v,
                    |app: &mut Workspace, v| app.overview_rename(v),
                ),
                xrow(
                    Region::Form,
                    (
                        recipes::field_column(recipes::field_enter(
                            pal,
                            "Width",
                            app.advance_buf.clone(),
                            |app: &mut Workspace, v| app.overview_set_advance(v),
                            |_: &mut Workspace, _| {},
                        )),
                        recipes::field_column(recipes::field_enter(
                            pal,
                            "Unicode",
                            app.unicode_buf.clone(),
                            |app: &mut Workspace, v| app.overview_set_unicode(v),
                            |_: &mut Workspace, _| {},
                        )),
                    ),
                ),
            ),
        )
    });
    let show_multi_mark = !editing && !app.multi_selected.is_empty();
    let glyph_body = xcolumn(
        Region::Form,
        (
            (show_multi_mark || !pts.is_empty() || editing).then(|| {
                xcolumn(
                    Region::List,
                    (
                        show_multi_mark.then(|| {
                            row("Selected".into(), format!("{}", app.multi_selected.len()))
                        }),
                        (!pts.is_empty()).then(|| row("Points".into(), pts)),
                        editing.then(|| row("Selected".into(), format!("{}", app.selected_points))),
                    ),
                )
            }),
            show_multi_mark.then(|| mark_section(app)),
            name_field,
            advance_field,
            overview_fields,
        ),
    );
    let glyph_section = recipes::section_with_header_height(
        app,
        "Glyph",
        "Glyph",
        glyph_body,
        if nodes && app.collapsed.contains("Glyph") {
            design::NODE_VIEW_SECTION_HEADER_HEIGHT
        } else {
            ControlSize::Row.px()
        },
    );
    recipes::panel_stack((
        // GPUI leads edit mode with selection geometry and its tools;
        // the overview still begins with glyph identity because these
        // optional edit groups disappear there.
        recipes::panel_stack((
            // The picture's properties belong to the picture: they show while it is selected.
            (editing && app.session.image_selected && app.image_frame().is_some())
                .then(|| recipes::panel_group(pal, super::image::panel(app))),
            (editing && app.tool == Tool::Label).then(|| {
                recipes::panel_group(
                    pal,
                    recipes::section(app, "Samples", "Samples", super::label::panel(app)),
                )
            }),
            (editing && app.font.project.is_neural()).then(|| {
                recipes::panel_group(
                    pal,
                    recipes::section(app, "Neural", "Neural", super::neural::panel(app)),
                )
            }),
            (editing && app.tool == Tool::Metaball)
                .then(|| recipes::panel_group(pal, super::metaballs::panel(app))),
            (editing && app.tool == Tool::Sketch)
                .then(|| recipes::panel_group(pal, super::sketch::panel(app))),
            editing.then(|| recipes::panel_group(pal, coordinates_section(app))),
        )),
        editing.then(|| recipes::panel_group(pal, transformations_section(app))),
        editing.then(|| recipes::panel_group(pal, curves_section(app))),
        editing.then(|| recipes::panel_group(pal, path_operations_section(app))),
        recipes::panel_group(pal, glyph_section),
        editing.then(|| recipes::panel_group(pal, background_section(app))),
        editing.then(|| {
            recipes::panel_stack((
                recipes::panel_group(pal, mark_section(app)),
                recipes::panel_group(pal, shaping_section(app)),
            ))
        }),
        editing.then(|| recipes::panel_group(pal, related_section(app))),
        // One column for three sections: the panel's tuple is at
        // Xilem's sixteen-child limit, so the overview's first
        // three sections share a slot. Same region as the panel,
        // so the gap between them is the panel's own.
        (!editing).then(|| {
            recipes::panel_stack((
                recipes::panel_group(pal, font_info_section(app)),
                recipes::panel_group(pal, dimensions_section(app)),
                recipes::panel_group(pal, font_advanced_section(app)),
            ))
        }),
        (!editing).then(|| recipes::panel_group(pal, kerning_section(app))),
        (!editing).then(|| recipes::panel_group(pal, groups_section(app))),
        (!editing).then(|| recipes::panel_group(pal, compare_section(app))),
        (!editing).then(|| recipes::panel_group(pal, features_section(app))),
        layers_section(app).map(|body| recipes::panel_group(pal, body)),
        recipes::panel_stack((
            masters_section(app).map(|body| recipes::panel_group(pal, body)),
            editing
                .then(|| axes_section(app))
                .flatten()
                .map(|body| recipes::panel_group(pal, body)),
        )),
        editing.then(|| recipes::panel_group(pal, measure_section(app))),
    ))
}
