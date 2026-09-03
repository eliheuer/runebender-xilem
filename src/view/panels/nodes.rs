// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The nodes pane: one row of buttons over the canvas.
//!
//! The files beside the font as chips, then New, Open…, Save and Run,
//! the GPUI build's row. Node types to add are on the canvas's
//! right-click menu, a layer the canvas widget opens itself. A
//! selected Master, Model or Adapter node offers its choices in a
//! second row.

use crate::edit::nodes::file_label;
use crate::view::canvas::nodes::{NodesEvent, nodes_canvas};
use crate::*;

pub(crate) fn nodes_pane(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use xilem::core::one_of::Either;
    let pal = &app.palette;
    let Some(state) = app.nodes.graph.as_ref() else {
        return Either::A(
            label("No nodes file open")
                .text_size(TextSize::Body.px())
                .color(pal.text_muted),
        );
    };
    let open_path = state.path.clone();
    let files: Vec<_> = app
        .nodes
        .files
        .iter()
        .cloned()
        .map(|file| {
            let current = file == open_path;
            tab_chip(
                pal,
                file_label(&file),
                current,
                false,
                move |app: &mut Workspace| {
                    if app.nodes.graph.as_ref().is_none_or(|g| g.path != file) {
                        app.open_nodes_file(&file);
                    }
                },
            )
        })
        .collect();
    let unlisted = (!app.nodes.files.contains(&open_path)).then(|| {
        tab_chip(
            pal,
            file_label(&open_path),
            true,
            false,
            |_: &mut Workspace| {},
        )
    });
    let running = app.nodes.job.is_some();
    let strip = xrow(
        Region::Toolbar,
        (
            xrow(Region::Inline, files),
            unlisted,
            FlexSpacer::Flex(1.0),
            recipes::toggle(pal, "New".into(), false, |app: &mut Workspace| {
                app.new_nodes_file();
            }),
            recipes::toggle(pal, "Open\u{2026}".into(), false, |app: &mut Workspace| {
                app.note = "Open\u{2026}: this shell has no file dialog yet; nodes files beside \
                            the font are the tabs at the left"
                    .into();
            }),
            recipes::toggle(pal, "Save".into(), false, |app: &mut Workspace| {
                app.save_nodes_file();
            }),
            recipes::toggle(
                pal,
                if running { "Running\u{2026}" } else { "Run" }.into(),
                !running,
                |app: &mut Workspace| app.run_nodes(),
            ),
        ),
    )
    .background_color(pal.panel);
    let choices = nodes_choices(app);
    let problems: Vec<_> = state
        .problems
        .iter()
        .map(|p| {
            label(p.to_string())
                .text_size(TextSize::Caption.px())
                .color(pal.text)
        })
        .collect();
    let canvas = nodes_canvas(
        state.graph.clone(),
        state.registry.clone(),
        app.palette.clone(),
        state.rows.clone(),
        app.nodes.selected,
        |app: &mut Workspace, ev| match ev {
            NodesEvent::Changed(graph) => app.nodes_changed(graph),
            NodesEvent::Selected(id) => app.nodes.selected = id,
            NodesEvent::Note(note) => app.note = note,
        },
    );
    Either::B(
        flex_col((
            strip,
            choices,
            xcolumn(Region::List, problems),
            sized_box(canvas)
                .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
                .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Space::None),
    )
}

/// One toggle per choice for the selected Master, Model or Adapter
/// node; nothing for any other node.
fn nodes_choices(app: &Workspace) -> Option<impl WidgetView<Workspace> + use<>> {
    let pal = &app.palette;
    let state = app.nodes.graph.as_ref()?;
    let id = app.nodes.selected?;
    let node = state.graph.node(id)?;
    let current = node
        .values
        .get("name")
        .and_then(|v| v.as_str())
        .map(String::from);
    let options: Vec<String> = match node.type_name.as_str() {
        "core.master" => app.font.master_names.clone(),
        "core.model" => runebender_core::document::nodes_run::installed(None, false)
            .into_iter()
            .map(|(n, _)| n)
            .collect(),
        "core.adapter" => runebender_core::document::nodes_run::installed(None, true)
            .into_iter()
            .map(|(n, _)| n)
            .collect(),
        _ => return None,
    };
    let chips: Vec<_> = options
        .into_iter()
        .map(|option| {
            let on = current.as_deref() == Some(option.as_str());
            let value = option.clone();
            recipes::toggle(pal, option, on, move |app: &mut Workspace| {
                app.nodes_set_value(id, "name", serde_json::Value::String(value.clone()));
            })
        })
        .collect();
    Some(xrow(Region::List, chips).background_color(pal.panel))
}
