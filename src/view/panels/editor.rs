// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The overview and the editor pane.

use crate::*;

pub(crate) fn overview(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let metrics = app.cell_metrics(app.cell_size);
    grid(
        app.filtered_cells(),
        metrics,
        app.palette.clone(),
        app.selected,
        app.multi_selected.clone(),
        |app: &mut Workspace, ev| match ev {
            GridEvent::Selected { index, cmd, shift } => app.grid_select(index, cmd, shift),
            GridEvent::Open(i) => app.open_glyph(i),
        },
    )
}

pub(crate) fn editor_pane(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let ghosts = Arc::new(
        app.font
            .reference_outlines(&app.session.glyph_name, &app.reference_layers),
    );
    let interp = app.interp_preview();
    editor(
        app.session.clone(),
        app.palette.clone(),
        app.tool,
        app.view,
        ghosts,
        interp,
        app.underlay(),
        (app.tool == Tool::Text).then(|| {
            text_tool::TextInputs::new(&app.font)
                .with_text(&app.initial_text)
                .with_direction(app.text_dir)
        }),
        |app: &mut Workspace, ev| match ev {
            canvas::editor::EditorEvent::Selection(n) => {
                app.selected_points = n;
                app.refresh_coord_bufs();
            }
            canvas::editor::EditorEvent::Edited => app.refresh_open_glyph(),
            canvas::editor::EditorEvent::EditGlyph(name) => {
                if let Some(index) = app.font.index_of(&name) {
                    app.open_glyph(index);
                    // Stay in the text tool: the point is to edit the glyph
                    // while the word around it is still on screen.
                    app.tool = Tool::Text;
                }
            }
        },
    )
}
