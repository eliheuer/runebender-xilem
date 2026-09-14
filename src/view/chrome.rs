// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The bars around the canvas: the titlebar, the header tools, the status bar.

use crate::view::design::{
    MARK_CLEAR_CROSS_HALF, MARK_SELECTED_RING_INSET, MARK_SWATCH_DIAMETER, MARK_SWATCH_GAP,
    TITLEBAR_HEIGHT,
};
use crate::widgets::icon_button::{IconMark, mark_button};
use crate::*;
use masonry::properties::Padding;
use masonry::properties::types::CrossAxisAlignment;
use xilem::Color;
use xilem::view::flex_row;

/// The title bar, laid out like the GPUI build's header.
///
/// Left to right: the file name, the save state, the tools when a
/// glyph is open, and the tab
/// strip. The tabs live here in both modes, so the strip does not move
/// when the mode changes.
pub(crate) fn titlebar(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let editing = matches!(app.mode, Mode::Editor(_));
    // The GPUI title bar keeps the document identity visible in every
    // mode. An editor tab says which glyph is open; it is not a substitute
    // for knowing which font the edits belong to.
    let title = app
        .font
        .source()
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let status = if app.modified { "Not saved" } else { "Saved" };
    sized_box(
        flex_row((
            // Room for the traffic lights, which sit where AppKit puts
            // them: winit has no way to move them, so the header pads
            // for the default place, the same 78px Zed pads on Tahoe.
            cfg!(target_os = "macos").then(|| {
                sized_box(label("")).dims(Dimensions::new(Dim::Fixed(Length::px(66.0)), Dim::Auto))
            }),
            // The name and the save state take whatever is left and
            // clip. GPUI writes `flex_1` and `overflow_hidden` on this
            // group for the same reason: when the window is narrow, the
            // file name is the part that can go.
            sized_box(xrow(
                Region::Inline,
                (
                    label(title)
                        .text_size(TextSize::Body.px())
                        .color(pal.header_ink),
                    // Saved is the mark palette's green, not saved its
                    // red: the same two colours the glyph grid uses. The
                    // GPUI build draws this as a keylined tag; here that
                    // was five style wrappers on a label in the root
                    // tuple, and rustc never finished the build. Until
                    // the tag is a painted widget, it is coloured text.
                    label(status.to_string())
                        .text_size(TextSize::Body.px())
                        .color(if app.modified {
                            pal.mark("red").unwrap_or_else(|| pal.role("warning"))
                        } else {
                            pal.mark("green").unwrap_or(pal.text_muted)
                        }),
                ),
            ))
            // In the overview this takes the leftover space. In the
            // editor it is sized to its content, and the content is
            // truncated above, because nothing here will clip: a
            // `Dim::Stretch` child with a flex factor still refuses to
            // go under the intrinsic width of its text, and an
            // over-wide label pushes the tab strip off the window
            // instead of being cut.
            .dims(Dimensions::new(Dim::Auto, Dim::Auto)),
            // The empty middle of the bar moves the window and zooms
            // it on a double click, as the system title bar would.
            drag_region().flex(1.0),
            editing.then(|| header_tools(app)),
            tab_strip(app),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Space::Md)
        // AppKit owns the traffic-light position. Keeping the title row free
        // of vertical padding lets its fixed height center those controls;
        // horizontal padding retains the established leading/trailing inset.
        .padding(Padding::horizontal(Space::Md.length()))
        .background_color(pal.header),
    )
    .dims(Dimensions::new(
        Dim::Stretch,
        Dim::Fixed(Length::px(TITLEBAR_HEIGHT)),
    ))
    // The shared rule in app_logic separates the header from all three docks.
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
    // Tool state is carried by icon contrast, not an inverted tile. This keeps
    // the header as quiet as the adjacent outlined tabs while making the
    // selected tool the brightest mark on the bar.
    let fg = pal.header_ink.with_alpha(0.5);
    let fg_active = pal.header_ink;
    let active_bg = Color::TRANSPARENT;
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
        .tile_size(ControlSize::Icon.px())
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

/// The marks bar at the foot of the sidebar: round swatches, the
/// clear mark last, as the GPUI build draws it under its sidebar.
pub(crate) fn marks_bar(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let current = app
        .selected
        .and_then(|i| app.font.glyphs.get(i))
        .and_then(|g| g.mark.clone());
    let mut marks = pal
        .mark_list()
        .into_iter()
        .map(|(name, color)| (Some(name), color))
        .collect::<Vec<_>>();
    marks.push((None, pal.panel));
    let outline = pal.mark_outline.unwrap_or(pal.outline);
    let selected_ring = pal.outline;
    let swatches = marks
        .into_iter()
        .map(|(mark, color)| {
            let selected = mark == current;
            let clear = mark.is_none();
            let face = sized_box(canvas(move |_: &mut Workspace, _, scene, _| {
                use masonry::imaging::Painter;
                use masonry::kurbo::{Circle, Line, Stroke as Pen};
                let mut painter = Painter::new(scene);
                let half = ControlSize::Swatch.px() / 2.0;
                let center = (half, half);
                painter
                    .fill(Circle::new(center, MARK_SWATCH_DIAMETER / 2.0), color)
                    .draw();
                painter
                    .stroke(
                        Circle::new(center, (MARK_SWATCH_DIAMETER - Stroke::Hairline.px()) / 2.0),
                        &Pen::new(Stroke::Hairline.px()),
                        outline,
                    )
                    .draw();
                if selected {
                    painter
                        .stroke(
                            Circle::new(
                                center,
                                half - MARK_SELECTED_RING_INSET - Stroke::Hairline.px() / 2.0,
                            ),
                            &Pen::new(Stroke::Hairline.px()),
                            selected_ring,
                        )
                        .draw();
                }
                if clear {
                    let d = MARK_CLEAR_CROSS_HALF;
                    for (a, b) in [
                        ((half - d, half - d), (half + d, half + d)),
                        ((half + d, half - d), (half - d, half + d)),
                    ] {
                        painter
                            .stroke(Line::new(a, b), &Pen::new(Stroke::Hairline.px()), outline)
                            .draw();
                    }
                }
            }))
            .dims(Dimensions::fixed(
                ControlSize::Swatch.length(),
                ControlSize::Swatch.length(),
            ));
            button(face, move |app: &mut Workspace| app.set_mark(mark.clone()))
                .padding(Space::None)
                .border_width(Space::None.length())
                .background_color(Color::TRANSPARENT)
        })
        .collect::<Vec<_>>();
    sized_box(
        flex_col((
            sized_box(label(""))
                .dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::Fixed(Stroke::Hairline.length()),
                ))
                .background_color(outline),
            flex_row(swatches)
                .gap(Length::px(MARK_SWATCH_GAP))
                .padding(Padding::horizontal(Length::px(MARK_SWATCH_GAP)))
                .flex(1.0),
        ))
        .gap(Space::None)
        .background_color(pal.panel),
    )
    .dims(Dimensions::new(
        Dim::Stretch,
        Dim::from(ControlSize::Control),
    ))
}

/// The bar under the middle column: add and remove glyph at the
/// left, the count centred, the view boxes and the zoom at the right.
/// The GPUI build's bottom bar, box for box.
pub(crate) fn status(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use xilem::core::one_of::Either;
    let pal = &app.palette;
    let text = match app.mode {
        Mode::Overview => format!(
            "{} selected \u{00b7} {}/{} glyphs",
            app.multi_selected.len()
                + usize::from(
                    app.selected
                        .is_some_and(|index| !app.multi_selected.contains(&index))
                ),
            app.filtered_cells().len(),
            app.font.glyphs.len(),
        ),
        // GPUI keeps the resting edit footer quiet; transient notes still
        // replace this empty string below when an action has something to say.
        Mode::Editor(_) => String::new(),
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
    let editing = matches!(app.mode, Mode::Editor(_));
    if editing {
        return Either::A(top_keyline(editor_status(app, text), pal.outline));
    }
    // Small keylined boxes, the size of a swatch plus its padding.
    let bar_box = |text: String, active: bool, f: fn(&mut Workspace)| {
        recipes::toggle_sized(pal, text, active, ControlSize::Icon, f)
    };
    Either::B(top_keyline(
        xrow(
            Region::Toolbar,
            (
                matches!(app.mode, Mode::Overview).then(|| {
                    xrow(
                        Region::Inline,
                        (
                            bar_box("+".into(), false, |app: &mut Workspace| app.new_glyph()),
                            bar_box("\u{2212}".into(), false, |app: &mut Workspace| {
                                app.note = "Remove glyph: not built in this shell yet".into();
                            }),
                        ),
                    )
                }),
                FlexSpacer::Flex(1.0),
                label(text)
                    .text_size(TextSize::Body.px())
                    .color(pal.text_muted),
                FlexSpacer::Flex(1.0),
                matches!(app.mode, Mode::Nodes).then(|| {
                    recipes::toggle(pal, "Fit graph".into(), false, |app: &mut Workspace| {
                        app.nodes.fit_request = app.nodes.fit_request.wrapping_add(1);
                    })
                }),
                matches!(app.mode, Mode::Overview).then(|| {
                    xrow(
                        Region::Inline,
                        (
                            // GPUI paints these marks from geometry rather than
                            // asking the interface font for Unicode symbols.
                            mark_button(
                                "Grid view",
                                IconMark::Grid,
                                !app.list,
                                pal.text_muted,
                                pal.selected_ink(),
                                pal.selected_bg(),
                                pal.control,
                                |app: &mut Workspace| app.list = false,
                            )
                            .framed(pal.panel, pal.outline)
                            .tile_size(ControlSize::Icon.px()),
                            mark_button(
                                "List view",
                                IconMark::List,
                                app.list,
                                pal.text_muted,
                                pal.selected_ink(),
                                pal.selected_bg(),
                                pal.control,
                                |app: &mut Workspace| app.list = true,
                            )
                            .framed(pal.panel, pal.outline)
                            .tile_size(ControlSize::Icon.px()),
                            recipes::neutral_slider(
                                &app.palette,
                                48.0,
                                200.0,
                                app.cell_size,
                                |app: &mut Workspace, v| {
                                    app.cell_size = v;
                                },
                            )
                            .width(Length::px(96.0)),
                        ),
                    )
                }),
            ),
        )
        .padding(if matches!(app.mode, Mode::Overview) {
            Space::Sm
        } else {
            Space::Md
        })
        .background_color(pal.panel),
        pal.outline,
    ))
}

/// Compact editor footer: proof appearance controls surround the live status.
fn editor_status(app: &Workspace, text: String) -> impl WidgetView<Workspace> + use<> {
    use crate::view::design::STATUS_SLIDER_WIDTH;
    let pal = &app.palette;
    sized_box(
        xrow(
            Region::Inline,
            (
                xrow(
                    Region::Inline,
                    (
                        mark_button(
                            "Show proof",
                            if app.preview_visible {
                                IconMark::EyeOpen
                            } else {
                                IconMark::EyeClosed
                            },
                            app.preview_visible,
                            pal.text_muted,
                            pal.text,
                            Color::TRANSPARENT,
                            Color::TRANSPARENT,
                            |app: &mut Workspace| app.preview_visible = !app.preview_visible,
                        )
                        .icon_size(16.0)
                        .tile_size(16.0),
                        mark_button(
                            "Invert proof",
                            IconMark::Invert,
                            app.preview_invert,
                            pal.text_muted,
                            pal.text,
                            Color::TRANSPARENT,
                            Color::TRANSPARENT,
                            |app: &mut Workspace| app.preview_invert = !app.preview_invert,
                        )
                        .icon_size(16.0)
                        .tile_size(16.0),
                        mark_button(
                            "Toggle sidebar",
                            if app.left_collapsed {
                                IconMark::SidebarClosed
                            } else {
                                IconMark::SidebarOpen
                            },
                            !app.left_collapsed,
                            pal.text_muted,
                            pal.text,
                            Color::TRANSPARENT,
                            Color::TRANSPARENT,
                            |app: &mut Workspace| app.left_collapsed = !app.left_collapsed,
                        )
                        .icon_size(16.0)
                        .tile_size(16.0),
                    ),
                )
                .gap(Space::Sm),
                // The readout yields width to controls; Flex still gives it
                // the remaining space during layout. Its intrinsic text width
                // must not widen the whole center dock in a narrow window.
                label(text)
                    .color(pal.text_muted)
                    .prop(masonry::properties::LineBreaking::Clip)
                    .dims(Dimensions::new(Dim::Fixed(Length::ZERO), Dim::Auto))
                    .flex(1.0),
                label("blur").color(pal.text_muted),
                recipes::neutral_slider(
                    pal,
                    0.0,
                    8.0,
                    app.preview_blur,
                    |app: &mut Workspace, value| app.preview_blur = value,
                )
                .width(Length::px(STATUS_SLIDER_WIDTH)),
            ),
        )
        .padding(Padding::horizontal(Space::Md.length())),
    )
    .dims(Dimensions::new(
        Dim::Stretch,
        Dim::from(ControlSize::Control),
    ))
    .background_color(pal.panel)
}
