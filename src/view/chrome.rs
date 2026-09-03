// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The bars around the canvas: the titlebar, the header tools, the status bar.

use crate::*;

/// The title bar, laid out like the GPUI build's header.
///
/// Left to right: a button that folds the left column away, the file
/// name, the save state, the tools when a glyph is open, and the tab
/// strip. The tabs live here in both modes, so the strip does not move
/// when the mode changes.
pub(crate) fn titlebar(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let editing = matches!(app.mode, Mode::Editor(_));
    // The file name is dropped once a glyph is open: with the direction
    // chips, the tools and the tab strip in the same row there is no
    // room for it, and nothing in the layout will clip, so an over-wide
    // label pushes the tabs off the end of the window rather than being
    // cut. The open font is named by the first tab either way.
    let title = if editing {
        String::new()
    } else {
        app.font
            .source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let status = if app.modified { "Not saved" } else { "Saved" };
    let bar = xrow(
        Region::Toolbar,
        (
            icon_button(
                "glyph-grid",
                !app.left_collapsed,
                pal.text_muted,
                pal.text,
                pal.control,
                pal.control,
                |app: &mut Workspace| app.left_collapsed = !app.left_collapsed,
            ),
            // The name and the save state take whatever is left and
            // clip. GPUI writes `flex_1` and `overflow_hidden` on this
            // group for the same reason: when the window is narrow, the
            // file name is the part that can go.
            sized_box(xrow(
                Region::Inline,
                (
                    label(title).text_size(TextSize::Body.px()).color(pal.text),
                    label(status.to_string())
                        .text_size(TextSize::Body.px())
                        .color(pal.role("warning")),
                ),
            ))
            // In the overview this takes the leftover space. In the
            // editor it is sized to its content, and the content is
            // truncated above, because nothing here will clip: a
            // `Dim::Stretch` child with a flex factor still refuses to
            // go under the intrinsic width of its text, and an
            // over-wide label pushes the tab strip off the window
            // instead of being cut.
            .dims(Dimensions::new(
                if editing { Dim::Auto } else { Dim::Stretch },
                Dim::Auto,
            ))
            .flex(1.0),
            editing.then(|| direction_chips(app)),
            editing.then(|| header_tools(app)),
            tab_strip(app),
        ),
    )
    .background_color(pal.panel);
    // A rule under the header, drawn as a one pixel box. Masonry's
    // border width is one value for all four sides, so there is no
    // bottom-only border to set; GPUI writes `border_b_1`.
    // No region here: this pair is one thing with no gap between its
    // halves, which is the one case a region cannot state.
    flex_col((
        bar,
        sized_box(label(""))
            .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(1.0))))
            .background_color(pal.role("gridBorder")),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Space::None)
}

/// LTR / RTL / Auto, as the GPUI build has them.
///
/// Up whenever a glyph is open, not only under the text tool: the
/// direction is a property of what is being reviewed, not of the tool
/// in hand.
pub(crate) fn direction_chips(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use runebender_core::text::buffer::TextDirection;
    let pal = &app.palette;
    let chip = |text: &'static str, want: Option<TextDirection>| {
        tab_chip(
            pal,
            text.into(),
            app.text_dir == want,
            false,
            move |app: &mut Workspace| {
                app.text_dir = want;
            },
        )
    };
    xrow(
        Region::Inline,
        (
            chip("LTR", Some(TextDirection::LeftToRight)),
            chip("RTL", Some(TextDirection::RightToLeft)),
            chip("Auto", None),
        ),
    )
}

/// The tools as a horizontal row for the header (gpui puts them there,
/// not in a left column).
pub(crate) fn header_tools(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let fg = pal.text_muted;
    let fg_active = pal.role("accent");
    let active_bg = pal.role("gridSelected").with_alpha(0.25);
    let hover_bg = pal.control;
    let tile = move |icon: &'static str, tool: Tool| {
        icon_button(
            icon,
            app.tool == tool,
            fg,
            fg_active,
            active_bg,
            hover_bg,
            move |app: &mut Workspace| {
                app.tool = tool;
            },
        )
    };
    xrow(
        Region::List,
        (
            tile("select", Tool::Select),
            tile("pen", Tool::Pen),
            tile("hyperpen", Tool::HyperPen),
            tile("shape-rectangle", Tool::Rect),
            tile("shape-ellipse", Tool::Ellipse),
            tile("knife", Tool::Knife),
            tile("measure", Tool::Measure),
            tile("text", Tool::Text),
        ),
    )
}

pub(crate) fn status(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let text = match app.mode {
        // No path here: it is in the title bar, and a long one ate the
        // whole bar, pushing the zoom control off the end.
        Mode::Overview => format!(
            "{} selected \u{00b7} {}/{} glyphs",
            app.multi_selected.len(),
            app.filtered_cells().len(),
            app.font.glyphs.len(),
        ),
        Mode::Editor(_) => format!(
            "{} \u{00b7} advance {} \u{00b7} {} points \u{00b7} {} selected",
            app.session.glyph_name.as_str(),
            app.session.advance(),
            app.session.point_count(),
            app.selected_points,
        ),
        Mode::Nodes => app
            .nodes
            .graph
            .as_ref()
            .map(|g| {
                format!(
                    "{} \u{00b7} {} nodes \u{00b7} {} links",
                    nodes::file_label(&g.path),
                    g.graph.nodes.len(),
                    g.graph.links.len()
                )
            })
            .unwrap_or_default(),
    };
    let text = if app.note.is_empty() {
        text
    } else {
        format!("{}   {}", text, app.note)
    };
    // Bottom bar: mark swatches on the left, the status centred, and the
    // zoom on the right, which is where the GPUI build puts them.
    // Circular, because a mark is a dot in every font editor.
    let swatch = |mark: Option<String>, color: xilem::Color| {
        sized_box(
            text_button("", move |app: &mut Workspace| app.set_mark(mark.clone()))
                .background_color(color)
                .padding(Space::None)
                .corner_radius(Radius::Full.length()),
        )
        .dims(Dimensions::fixed(
            ControlSize::Swatch.length(),
            ControlSize::Swatch.length(),
        ))
    };
    let marks: Vec<_> = app
        .palette
        .mark_list()
        .into_iter()
        .map(|(name, color)| swatch(Some(name), color))
        .collect();
    let editing = matches!(app.mode, Mode::Editor(_));
    xrow(
        Region::Toolbar,
        (
            swatch(None, pal.control),
            xrow(Region::List, marks),
            // Clears the mark, like the GPUI build's crossed swatch.
            recipes::toggle_sized(
                pal,
                "\u{00d7}".into(),
                false,
                ControlSize::Swatch,
                |app: &mut Workspace| app.set_mark(None),
            ),
            FlexSpacer::Flex(1.0),
            label(text)
                .text_size(TextSize::Caption.px())
                .color(pal.text_muted),
            FlexSpacer::Flex(1.0),
            // Grid or Detail, as the GPUI build has them. Its List and
            // Forms views are not built here yet, and a control that
            // does nothing is worse than one that is missing.
            (!editing).then(|| {
                xrow(
                    Region::Inline,
                    (
                        tab_chip(
                            pal,
                            "Grid".into(),
                            !app.detail,
                            false,
                            |app: &mut Workspace| {
                                app.detail = false;
                            },
                        ),
                        tab_chip(
                            pal,
                            "Detail".into(),
                            app.detail,
                            false,
                            |app: &mut Workspace| {
                                app.detail = true;
                            },
                        ),
                    ),
                )
            }),
            // Cell size in the grid, zoom in the editor: one control in
            // one place, whichever surface is showing. A slider, because
            // that is what the GPUI build puts here.
            editing.then(|| {
                let zoom = app.session.viewport.zoom.clamp(0.05, 8.0);
                xrow(
                    Region::Inline,
                    (
                        slider(0.05, 8.0, zoom, |app: &mut Workspace, v| app.zoom_to(v))
                            .width(Length::px(96.0)),
                        label(format!("{:.0}%", zoom * 100.0))
                            .text_size(TextSize::Caption.px())
                            .color(pal.text_muted),
                    ),
                )
            }),
            (!editing).then(|| {
                xrow(
                    Region::Inline,
                    (
                        slider(48.0, 200.0, app.cell_size, |app: &mut Workspace, v| {
                            app.cell_size = v;
                        })
                        .width(Length::px(96.0)),
                        label(format!("{:.0}", app.cell_size))
                            .text_size(TextSize::Caption.px())
                            .color(pal.text_muted),
                    ),
                )
            }),
        ),
    )
    .background_color(pal.panel)
}
