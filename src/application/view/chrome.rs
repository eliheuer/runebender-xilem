// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The bars around the canvas: the titlebar, the header tools, the status bar.

use crate::application::editor::tools::nodes;
use crate::application::view::design::{
    ButtonShape, ControlSize, Region, Space, Stroke, TextSize, row as xrow,
};
use crate::application::view::design::{
    MARK_SELECTED_RING_INSET, MARK_SWATCH_GAP, STATUS_ICON_SIZE, TITLEBAR_HEIGHT,
};
use crate::application::view::panels::tabs::{tab_chip, tab_strip};
use crate::application::view::recipes::button;
use crate::application::view::theme::Palette;
use crate::application::view::{label, recipes};
use crate::application::widgets::drag_region::drag_region;
use crate::application::widgets::icon_button::icon_button;
use crate::application::widgets::icon_button::named_icon_button;
use crate::application::widgets::icon_paint;
use crate::application::widgets::tool_group::{ToolGroup, tool_group};
use crate::application::workspace::{Mode, Tool, Workspace};
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use masonry::properties::Padding;
use masonry::properties::types::CrossAxisAlignment;
use xilem::Color;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::FlexExt as _;
use xilem::view::flex_row;
use xilem::view::{canvas, flex_col, sized_box};

/// The title bar for document identity, editor tools, and tabs.
///
/// Left to right: the file name, the save state, the tools when a
/// glyph is open, and the tab
/// strip. The tabs live here in both modes, so the strip does not move
/// when the mode changes.
pub(crate) fn titlebar(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let editing = matches!(app.mode, Mode::Editor(_));
    // Keep the document identity visible in every mode. An editor tab says
    // which glyph is open; it is not a substitute
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
            // for the default place. In full screen AppKit moves them out
            // of the content area, so the document uses the normal inset.
            (cfg!(target_os = "macos") && !app.fullscreen).then(|| {
                sized_box(label("")).dims(Dimensions::new(Dim::Fixed(Length::px(66.0)), Dim::Auto))
            }),
            // The name and the save state take whatever is left and
            // clip. When the window is narrow, the file name is the part that
            // can go.
            sized_box(xrow(
                Region::Inline,
                (
                    label(title)
                        .text_size(TextSize::Body.px())
                        .color(pal.header_ink),
                    // Pale headers need darker status ink than filled glyph tiles.
                    label(status.to_string())
                        .text_size(TextSize::Body.px())
                        .color(pal.save_status_ink(app.modified)),
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
        .background_color(pal.titlebar_background()),
    )
    .dims(Dimensions::new(
        Dim::Stretch,
        Dim::Fixed(Length::px(TITLEBAR_HEIGHT)),
    ))
}

/// LTR, RTL, and automatic writing-direction controls.
///
/// Up whenever a glyph is open, not only under the text tool: the
/// direction is a property of what is being reviewed, not of the tool
/// in hand.
pub(crate) fn direction_chips(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use runebender::text::buffer::TextDirection;
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

/// The tools as a horizontal row in the header.
pub(crate) fn header_tools(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    // Tool state is carried by icon contrast, not an inverted tile. This keeps
    // the header as quiet as the adjacent outlined tabs while making the
    // selected tool the brightest mark on the bar.
    let fg = pal.tool_inactive_ink();
    let fg_active = pal.tool_active_ink();
    let active_bg = Color::TRANSPARENT;
    let hover_bg = pal.header_ink.with_alpha(0.1);
    let editor_focus = app.editor_focus.clone();
    let tile = move |icon: &'static str, tool: Tool| {
        let button = icon_button(
            icon,
            app.tool == tool,
            fg,
            fg_active,
            active_bg,
            hover_bg,
            move |app: &mut Workspace| {
                app.select_tool(tool);
            },
        )
        .corner_radius(pal.control_radius)
        .tile_size(ControlSize::Icon.px());
        if tool == Tool::Text {
            button.focus_target(editor_focus.clone())
        } else {
            button
        }
    };
    xrow(
        Region::Inline,
        (
            tool_group(ToolGroup::Select, app.tool, app.palette.clone()),
            // The shared icon set calls this asset `preview`; its drawing is
            // the hand used by the viewport-pan tool.
            tile("preview", Tool::Hand),
            tile("pen", Tool::Pen),
            tile("hyperpen", Tool::HyperPen),
            tile("invert", Tool::Sketch),
            tool_group(ToolGroup::Shapes, app.tool, app.palette.clone()),
            tile("knife", Tool::Knife),
            tile("measure", Tool::Measure),
            // No icon is drawn for it yet; two overlapping shapes are what a label is.
            tile("intersect", Tool::Label),
            tile("text", Tool::Text),
        ),
    )
}

/// The marks bar at the foot of the sidebar: round swatches, the
/// clear mark last.
pub(crate) fn marks_bar(app: &Workspace) -> impl WidgetView<Workspace, Widget: Sized> + use<> {
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
    let clear_ink = pal.editor_control_ink();
    let count = marks.len();
    let swatches = marks
        .into_iter()
        .map(|(mark, color)| {
            let selected = mark == current;
            let clear = mark.is_none();
            let face = sized_box(canvas(move |_: &mut Workspace, _, scene, size| {
                use masonry::imaging::Painter;
                use masonry::kurbo::{Circle, Rect, Stroke as Pen};
                let mut painter = Painter::new(scene);
                let half = size.width.min(size.height) / 2.0;
                let center = (half, half);
                painter.fill(Circle::new(center, half), color).draw();
                painter
                    .stroke(
                        Circle::new(center, half - Stroke::Hairline.px() / 2.0),
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
                    icon_paint::paint(
                        &mut painter.as_dyn(),
                        "close",
                        Rect::new(half - 5.0, half - 5.0, half + 5.0, half + 5.0),
                        clear_ink,
                    );
                }
            }))
            .dims(Dimensions::new(Dim::Stretch, Dim::Stretch));
            button(
                pal,
                crate::application::widgets::swatch_strip::swatch_face(face),
                move |app: &mut Workspace| {
                    app.set_mark(mark.clone());
                },
            )
            .padding(Space::None)
            .border_width(Space::None.length())
            .corner_radius(ButtonShape::Circular.radius())
            .background_color(Color::TRANSPARENT)
            .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
            .flex(1.0)
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
            crate::application::widgets::swatch_strip::swatch_strip(
                flex_row(swatches)
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .gap(Length::px(MARK_SWATCH_GAP))
                    .padding(Length::px(MARK_SWATCH_GAP)),
                count,
            ),
        ))
        .gap(Space::None),
    )
    .dims(Dimensions::new(Dim::Stretch, Dim::Auto))
}

/// The bar under the middle column: add and remove glyph at the
/// left, the count centred, the view boxes and the zoom at the right.
/// The workspace status bar.
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
        // Keep the resting edit footer quiet; transient notes still
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
    let text = if (app.has_text_session || !app.preview_text.is_empty())
        && let Some(status) = match app.font.preview_font() {
            Ok(Some(_)) => None,
            Ok(None) => Some("Preview compiling…".to_string()),
            Err(error) => Some(format!("Preview unavailable: {error}")),
        } {
        format!("{text}   {status}")
    } else if app.note.is_empty() {
        text
    } else {
        format!("{}   {}", text, app.note)
    };
    let editing = matches!(app.mode, Mode::Editor(_));
    if editing {
        return Either::A(editor_status(app, text));
    }
    Either::B(recipes::panel_footer(
        pal,
        (
            sidebar_toggle(app),
            matches!(app.mode, Mode::Overview).then(|| {
                xrow(
                    Region::Inline,
                    (
                        overview_status_button(
                            pal,
                            "Add glyph",
                            "plus",
                            false,
                            pal.editor_control_ink(),
                            |app: &mut Workspace| app.new_glyph(),
                        ),
                        overview_status_button(
                            pal,
                            "Remove glyph",
                            "minus",
                            false,
                            pal.editor_control_ink(),
                            |app: &mut Workspace| {
                                app.note = "Remove glyph: not built in this shell yet".into();
                            },
                        ),
                    ),
                )
            }),
            label(text)
                .text_size(TextSize::Body.px())
                .text_alignment(masonry::TextAlign::Center)
                .color(pal.text_muted)
                .prop(masonry::properties::LineBreaking::Clip)
                .dims(Dimensions::new(Dim::Fixed(Length::ZERO), Dim::Auto))
                .flex(1.0),
            matches!(app.mode, Mode::Nodes).then(|| {
                recipes::action_sized(
                    pal,
                    "Fit graph".into(),
                    ControlSize::Icon,
                    |app: &mut Workspace| {
                        app.nodes.fit_request = app.nodes.fit_request.wrapping_add(1);
                    },
                )
            }),
            matches!(app.mode, Mode::Overview).then(|| {
                xrow(
                    Region::Inline,
                    (
                        overview_status_button(
                            pal,
                            "Grid view",
                            "grid",
                            !app.list,
                            pal.text_subdued,
                            |app: &mut Workspace| app.list = false,
                        ),
                        overview_status_button(
                            pal,
                            "List view",
                            "list",
                            app.list,
                            pal.text_subdued,
                            |app: &mut Workspace| app.list = true,
                        ),
                        recipes::neutral_slider(
                            &app.palette,
                            48.0,
                            200.0,
                            app.cell_size,
                            |app: &mut Workspace, v| {
                                app.cell_size = v;
                            },
                        )
                        .width(Length::px(
                            crate::application::view::design::STATUS_SLIDER_WIDTH,
                        )),
                    ),
                )
            }),
        ),
    ))
}

/// One compact footer control, with selection expressed through its ink.
fn overview_status_button<F>(
    pal: &Palette,
    label: &'static str,
    icon: &'static str,
    active: bool,
    inactive_ink: Color,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F>
where
    F: Fn(&mut Workspace) + Send + Sync + 'static,
{
    named_icon_button(
        label,
        icon,
        active,
        inactive_ink,
        pal.editor_control_ink(),
        Color::TRANSPARENT,
        Color::TRANSPARENT,
        on_click,
    )
    .corner_radius(pal.control_radius)
    .icon_size(STATUS_ICON_SIZE)
    .tile_size(STATUS_ICON_SIZE)
}

/// Compact editor footer: proof appearance controls surround the live status.
fn editor_status(app: &Workspace, text: String) -> impl WidgetView<Workspace> + use<> {
    use crate::application::view::design::STATUS_SLIDER_WIDTH;
    let pal = &app.palette;
    recipes::panel_footer(
        pal,
        (
            xrow(
                Region::Inline,
                (
                    sidebar_toggle(app),
                    named_icon_button(
                        "Show proof",
                        if app.preview_visible {
                            "eye-open"
                        } else {
                            "eye-closed"
                        },
                        app.preview_visible,
                        pal.editor_control_ink(),
                        pal.editor_control_ink(),
                        Color::TRANSPARENT,
                        Color::TRANSPARENT,
                        |app: &mut Workspace| app.preview_visible = !app.preview_visible,
                    )
                    .corner_radius(pal.control_radius)
                    .icon_size(STATUS_ICON_SIZE)
                    .tile_size(STATUS_ICON_SIZE),
                    named_icon_button(
                        "Invert proof",
                        "invert",
                        app.preview_invert,
                        pal.editor_control_ink(),
                        pal.editor_control_ink(),
                        Color::TRANSPARENT,
                        Color::TRANSPARENT,
                        |app: &mut Workspace| app.preview_invert = !app.preview_invert,
                    )
                    .corner_radius(pal.control_radius)
                    .icon_size(STATUS_ICON_SIZE)
                    .tile_size(STATUS_ICON_SIZE),
                    // A neural source can show its text set from the labeled pieces.
                    app.font.project.is_neural().then(|| {
                        use crate::application::pieces::PreviewView;
                        named_icon_button(
                            "Pieces",
                            "shapes",
                            app.preview_view == PreviewView::Pieces,
                            pal.editor_control_ink(),
                            pal.editor_control_ink(),
                            Color::TRANSPARENT,
                            Color::TRANSPARENT,
                            |app: &mut Workspace| {
                                app.preview_view = match app.preview_view {
                                    PreviewView::Pieces => PreviewView::Outline,
                                    _ => PreviewView::Pieces,
                                };
                            },
                        )
                        .corner_radius(pal.control_radius)
                        .icon_size(STATUS_ICON_SIZE)
                        .tile_size(STATUS_ICON_SIZE)
                    }),
                    // Or drawn by the latest trained font.
                    app.font.project.is_neural().then(|| {
                        use crate::application::pieces::PreviewView;
                        named_icon_button(
                            "Model",
                            "exclude",
                            app.preview_view == PreviewView::Model,
                            pal.editor_control_ink(),
                            pal.editor_control_ink(),
                            Color::TRANSPARENT,
                            Color::TRANSPARENT,
                            |app: &mut Workspace| {
                                app.preview_view = match app.preview_view {
                                    PreviewView::Model => PreviewView::Outline,
                                    _ => PreviewView::Model,
                                };
                            },
                        )
                        .corner_radius(pal.control_radius)
                        .icon_size(STATUS_ICON_SIZE)
                        .tile_size(STATUS_ICON_SIZE)
                    }),
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
}

/// One stable footer control for hiding and restoring the left dock in every mode.
fn sidebar_toggle(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    named_icon_button(
        "Toggle sidebar",
        if app.left_collapsed {
            "sidebar-closed"
        } else {
            "sidebar-open"
        },
        false,
        pal.editor_control_ink(),
        pal.editor_control_ink(),
        Color::TRANSPARENT,
        pal.control,
        |app: &mut Workspace| app.left_collapsed = !app.left_collapsed,
    )
    .corner_radius(pal.control_radius)
    .icon_size(STATUS_ICON_SIZE)
    .tile_size(STATUS_ICON_SIZE)
}
