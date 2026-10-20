// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The bars around the canvas: the titlebar, the header tools, the status bar.

use crate::application::design::controls::icon_button::icon_button;
use crate::application::design::recipes;
use crate::application::design::recipes::button;
use crate::application::design::theme::Palette;
use crate::application::design::{
    ControlKind, ControlSize, ControlStyle, Ground, Region, Space, TextSize, row as xrow,
};
use crate::application::design::{MARK_SWATCH_GAP, STATUS_ICON_SIZE, TITLEBAR_HEIGHT};
use crate::application::editor::tools::graph;
use crate::application::view::label;
use crate::application::view::panels::tabs::{tab_chip, tab_strip};
use crate::application::widgets::drag_region::drag_region;
use crate::application::widgets::text_label::{self, Anchor};
use crate::application::widgets::tool_group::{ToolGroup, tool_group};
use crate::application::workspace::{Mode, Tool, Workspace};
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use masonry::properties::Padding;
use masonry::properties::types::CrossAxisAlignment;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::FlexExt as _;
use xilem::view::flex_row;
use xilem::view::{canvas, flex_col, portal, sized_box};

/// The title bar for editor tools and tabs.
///
/// Left to right: the window controls, the tools when a glyph is open,
/// and the tab strip. The tabs live here in both modes, so the strip
/// does not move when the mode changes. The document's name and save
/// state sit in the footer, where there is room for them.
pub(crate) fn titlebar(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let editing = matches!(app.mode, Mode::Editor(_));
    sized_box(
        flex_row((
            // Room for the traffic lights, which sit where AppKit puts
            // them: winit has no way to move them, so the header pads
            // for the default place. In full screen AppKit moves them out
            // of the content area, so the document uses the normal inset.
            (cfg!(target_os = "macos") && !app.fullscreen).then(|| {
                sized_box(label("")).dims(Dimensions::new(Dim::Fixed(Length::px(66.0)), Dim::Auto))
            }),
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
    let editor_focus = app.editor_focus.clone();
    let tile = move |icon: &'static str, tool: Tool| {
        let style = ControlStyle::new(pal, ControlKind::Tool)
            .on(pal, Ground::Header)
            .selected(pal, app.tool == tool);
        let button = icon_button(style, icon, icon, move |app: &mut Workspace| {
            app.select_tool(tool);
        })
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
    let count = marks.len();
    let swatches = marks
        .into_iter()
        .map(|(mark, color)| {
            let selected = mark == current;
            let clear = mark.is_none();
            recipes::mark_swatch(pal, color, selected, clear, move |app: &mut Workspace| {
                app.set_mark(mark.clone());
            })
            .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
            .flex(1.0)
        })
        .collect::<Vec<_>>();
    sized_box(
        flex_col((
            recipes::hairline(pal),
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

/// The document's name and save state, centred in the footer.
///
/// The path is the part that can shrink: the whole path when the bar
/// has room for it, the file name alone when it does not, and an elided
/// name below that. The save state always shows. A transient `note`
/// takes the readout's place while it has something to say.
fn document_readout(app: &Workspace, note: String) -> impl WidgetView<Workspace> + use<> {
    let source = app.font.source();
    // The path as opened may be relative to wherever the app was started,
    // and may climb through `..`; show it from the root with those folded.
    let path = {
        use std::path::Component;
        let mut folded = std::path::PathBuf::new();
        for part in std::path::absolute(source)
            .unwrap_or_else(|_| source.to_path_buf())
            .components()
        {
            match part {
                Component::CurDir => {}
                Component::ParentDir => {
                    folded.pop();
                }
                other => folded.push(other),
            }
        }
        folded.display().to_string()
    };
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let status = if app.modified { "Not saved" } else { "Saved" };
    let ink = app.palette.text_muted;
    // Saved is the resting state and stays as quiet as the name beside it;
    // only an unsaved document asks for attention.
    let status_ink = if app.modified {
        app.palette.save_status_ink(true)
    } else {
        ink
    };
    let description = if note.is_empty() {
        format!("{name}, {status}")
    } else {
        note.clone()
    };
    sized_box(
        portal(
            canvas(move |_: &mut Workspace, _, scene, size| {
                let mut painter: masonry::imaging::Painter<'_> =
                    masonry::imaging::Painter::new(scene);
                let text_size = TextSize::Body.px();
                let middle = kurbo::Point::new(size.width / 2.0, size.height / 2.0);
                if !note.is_empty() {
                    text_label::draw_elided(
                        &mut painter,
                        middle,
                        &note,
                        text_size,
                        ink,
                        Anchor::Middle,
                        size.width,
                    );
                    return;
                }
                let gap = Space::Md.px();
                let status_width = text_label::width(status, text_size);
                let room = (size.width - status_width - gap).max(0.0);
                let text = if text_label::width(&path, text_size) <= room {
                    &path
                } else {
                    &name
                };
                let text_width = text_label::width(text, text_size).min(room);
                // With no room for a name, the status centres on its own.
                let gap = if room > 0.0 { gap } else { 0.0 };
                let start = ((size.width - text_width - gap - status_width) / 2.0).max(0.0);
                text_label::draw_elided(
                    &mut painter,
                    kurbo::Point::new(start, middle.y),
                    text,
                    text_size,
                    ink,
                    Anchor::Start,
                    room,
                );
                let status_start = start + text_width + gap;
                text_label::draw_elided(
                    &mut painter,
                    kurbo::Point::new(status_start, middle.y),
                    status,
                    text_size,
                    status_ink,
                    Anchor::Start,
                    (size.width - status_start).max(0.0),
                );
            })
            .alt_text(description),
        )
        .constrain_horizontal(true)
        .constrain_vertical(true)
        .must_fill(true),
    )
    .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
}

/// The bar under the middle column: add and remove glyph at the
/// left, the document readout centred, the view boxes and the zoom at
/// the right.
pub(crate) fn status(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use xilem::core::one_of::Either;
    let pal = &app.palette;
    let text = match app.mode {
        // The readout shows the document at rest; transient notes still
        // replace this empty string below when an action has something to say.
        Mode::Overview | Mode::Editor(_) => String::new(),
        Mode::Graph => app
            .graph
            .open
            .as_ref()
            .map(|g| {
                format!(
                    "{} \u{00b7} {} nodes \u{00b7} {} links",
                    graph::file_label(&g.path),
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
                            None,
                            |app: &mut Workspace| app.new_glyph(),
                        ),
                        overview_status_button(
                            pal,
                            "Remove glyph",
                            "minus",
                            None,
                            |app: &mut Workspace| {
                                app.note = "Remove glyph: not built in this shell yet".into();
                            },
                        ),
                    ),
                )
            }),
            document_readout(app, text).flex(1.0),
            // The slider sits where the overview's cell-size slider does.
            matches!(app.mode, Mode::Graph).then(|| {
                use crate::application::view::canvas::graph::{ZOOM_MAX, ZOOM_MIN};
                xrow(
                    Region::Inline,
                    (
                        footer_button(pal, "Center Graph", |app: &mut Workspace| {
                            app.graph.fit_request = app.graph.fit_request.wrapping_add(1);
                        }),
                        // Zoom moves in equal steps per doubling, as the scroll wheel does.
                        recipes::neutral_slider(
                            pal,
                            ZOOM_MIN.log2(),
                            ZOOM_MAX.log2(),
                            app.graph
                                .zoom
                                .unwrap_or(1.0)
                                .clamp(ZOOM_MIN, ZOOM_MAX)
                                .log2(),
                            |app: &mut Workspace, v| app.graph.zoom = Some(v.exp2()),
                        )
                        .width(Length::px(crate::application::design::STATUS_SLIDER_WIDTH)),
                    ),
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
                            Some(!app.list),
                            |app: &mut Workspace| app.list = false,
                        ),
                        overview_status_button(
                            pal,
                            "List view",
                            "list",
                            Some(app.list),
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
                        .width(Length::px(crate::application::design::STATUS_SLIDER_WIDTH)),
                    ),
                )
            }),
            inspector_toggle(app),
        ),
    ))
}

/// One compact footer icon: an action when `on` is `None`, otherwise a
/// switch that is on or off.
fn overview_status_button<F>(
    pal: &Palette,
    label: &'static str,
    icon: &'static str,
    on: Option<bool>,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F>
where
    F: Fn(&mut Workspace) + Send + Sync + 'static,
{
    let style = match on {
        None => ControlStyle::new(pal, ControlKind::Bare),
        Some(on) => ControlStyle::new(pal, ControlKind::Tool).selected(pal, on),
    };
    icon_button(style, icon, label, on_click)
        .icon_size(STATUS_ICON_SIZE)
        .tile_size(STATUS_ICON_SIZE)
}

/// Compact editor footer: proof appearance controls surround the live status.
fn editor_status(app: &Workspace, text: String) -> impl WidgetView<Workspace> + use<> {
    use crate::application::design::STATUS_SLIDER_WIDTH;
    let pal = &app.palette;
    recipes::panel_footer(
        pal,
        (
            xrow(
                Region::Inline,
                (
                    sidebar_toggle(app),
                    icon_button(
                        ControlStyle::new(pal, ControlKind::Tool)
                            .selected(pal, app.preview_visible),
                        if app.preview_visible {
                            "eye-open"
                        } else {
                            "eye-closed"
                        },
                        "Show proof",
                        |app: &mut Workspace| app.preview_visible = !app.preview_visible,
                    )
                    .icon_size(STATUS_ICON_SIZE)
                    .tile_size(STATUS_ICON_SIZE),
                    icon_button(
                        ControlStyle::new(pal, ControlKind::Tool).selected(pal, app.preview_invert),
                        "invert",
                        "Invert proof",
                        |app: &mut Workspace| app.preview_invert = !app.preview_invert,
                    )
                    .icon_size(STATUS_ICON_SIZE)
                    .tile_size(STATUS_ICON_SIZE),
                    // A neural source can show its text set from the labeled pieces.
                    app.font.project.is_neural().then(|| {
                        use crate::application::pieces::PreviewView;
                        icon_button(
                            ControlStyle::new(pal, ControlKind::Tool)
                                .selected(pal, app.preview_view == PreviewView::Pieces),
                            "shapes",
                            "Pieces",
                            |app: &mut Workspace| {
                                app.preview_view = match app.preview_view {
                                    PreviewView::Pieces => PreviewView::Outline,
                                    _ => PreviewView::Pieces,
                                };
                            },
                        )
                        .icon_size(STATUS_ICON_SIZE)
                        .tile_size(STATUS_ICON_SIZE)
                    }),
                    // Or drawn by the latest trained font.
                    app.font.project.is_neural().then(|| {
                        use crate::application::pieces::PreviewView;
                        icon_button(
                            ControlStyle::new(pal, ControlKind::Tool)
                                .selected(pal, app.preview_view == PreviewView::Model),
                            "exclude",
                            "Model",
                            |app: &mut Workspace| {
                                app.preview_view = match app.preview_view {
                                    PreviewView::Model => PreviewView::Outline,
                                    _ => PreviewView::Model,
                                };
                            },
                        )
                        .icon_size(STATUS_ICON_SIZE)
                        .tile_size(STATUS_ICON_SIZE)
                    }),
                ),
            )
            .gap(Space::Sm),
            document_readout(app, text).flex(1.0),
            label("blur").color(pal.text_muted),
            recipes::neutral_slider(
                pal,
                0.0,
                8.0,
                app.preview_blur,
                |app: &mut Workspace, value| app.preview_blur = value,
            )
            .width(Length::px(STATUS_SLIDER_WIDTH)),
            inspector_toggle(app),
        ),
    )
}

/// A footer action named in words: a Secondary control at the footer's height.
fn footer_button<F>(
    pal: &Palette,
    text: &'static str,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F>
where
    F: Fn(&mut Workspace) + Send + Sync + 'static,
{
    use crate::application::design::areas::footer;
    let style = ControlStyle::new(pal, ControlKind::Secondary);
    sized_box(
        button(
            style,
            label(text)
                .text_size(TextSize::Body.px())
                .color(style.ink()),
            on_click,
        )
        .padding(Padding::horizontal(Length::px(footer::CONTROL_INSET))),
    )
    .dims(Dimensions::new(
        Dim::Auto,
        Dim::Fixed(Length::px(footer::CONTROL_HEIGHT)),
    ))
}

/// One stable footer control for hiding and restoring the left dock in every mode.
fn sidebar_toggle(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    icon_button(
        ControlStyle::new(pal, ControlKind::Bare),
        if app.left_collapsed {
            "sidebar-closed"
        } else {
            "sidebar-open"
        },
        "Toggle sidebar",
        |app: &mut Workspace| app.left_collapsed = !app.left_collapsed,
    )
    .icon_size(STATUS_ICON_SIZE)
    .tile_size(STATUS_ICON_SIZE)
}

/// The matching control at the footer's far right for hiding and restoring the inspector.
fn inspector_toggle(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    icon_button(
        ControlStyle::new(pal, ControlKind::Bare),
        if app.right_collapsed {
            "sidebar-right-closed"
        } else {
            "sidebar-right-open"
        },
        "Toggle inspector",
        |app: &mut Workspace| app.right_collapsed = !app.right_collapsed,
    )
    .icon_size(STATUS_ICON_SIZE)
    .tile_size(STATUS_ICON_SIZE)
}
