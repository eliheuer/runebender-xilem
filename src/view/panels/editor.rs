// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The overview and the editor pane.

use crate::*;

pub(crate) fn overview(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use xilem::core::one_of::Either;
    if app.list {
        return Either::A(canvas::list::glyph_list(app));
    }
    let metrics = app.cell_metrics(app.cell_size);
    Either::B(grid(
        app.filtered_cells(),
        metrics,
        app.palette.clone(),
        app.selected,
        app.multi_selected.clone(),
        |app: &mut Workspace, ev| match ev {
            GridEvent::Selected { index, cmd, shift } => app.grid_select(index, cmd, shift),
            GridEvent::Open(i) => app.open_glyph(i),
        },
    ))
}

pub(crate) fn editor_pane(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let ghosts = Arc::new(
        app.font
            .reference_outlines(&app.session.glyph_name, &app.reference_layers),
    );
    let interp = app.interp_preview();
    let groups = (
        app.font.kern_group(&app.session.glyph_name, true),
        app.font.kern_group(&app.session.glyph_name, false),
    );
    let mark = app
        .font
        .glyphs
        .iter()
        .find(|glyph| glyph.name == app.session.glyph_name)
        .and_then(|glyph| glyph.mark.as_deref())
        .and_then(|label| app.palette.mark(label));
    editor(
        app.session.clone(),
        app.palette.clone(),
        groups,
        mark,
        app.tool,
        app.tool_before_space_pan.is_some(),
        app.view,
        ghosts,
        interp,
        app.underlay(),
        app.has_text_session.then(|| {
            // Headless evidence can start with one logical range
            // selected; live selection remains widget-owned.
            let selection = std::env::var("RUNEBENDER_TEXT_SELECTION")
                .ok()
                .and_then(|value| {
                    let (start, end) = value.split_once(':')?;
                    Some((start.parse().ok()?, end.parse().ok()?))
                });
            text_tool::TextInputs::for_glyph(&app.font, &app.session.glyph_name)
                .with_context(app.text_context_id())
                .with_text(&app.initial_text)
                .with_direction(app.text_dir)
                .with_selection(selection)
                .with_shaping_options(
                    &app.text_features_disabled,
                    app.text_script.as_deref(),
                    app.text_language.as_deref(),
                )
        }),
        app.editor_focus.clone(),
        |app: &mut Workspace, ev| match ev {
            canvas::editor::EditorEvent::Selection(n) => {
                app.selected_points = n;
                app.refresh_coord_bufs();
            }
            canvas::editor::EditorEvent::Edited => app.refresh_open_glyph(),
            canvas::editor::EditorEvent::Undo => app.undo_open_glyph(false),
            canvas::editor::EditorEvent::Redo => app.undo_open_glyph(true),
            canvas::editor::EditorEvent::TextChanged(text) => app.set_editor_text(text),
            canvas::editor::EditorEvent::EditGlyph { name, tool } => {
                if let Some(index) = app.font.index_of(&name) {
                    app.edit_text_sort_glyph(index, tool);
                }
            }
        },
    )
}
