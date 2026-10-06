// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The render tree: how the workspace's state becomes a frame.

use crate::application::actions;
use crate::application::editor::tools::{chat, local_ai, nodes, scripts};
use crate::application::platform::export;
#[cfg(unix)]
use crate::application::platform::live;
#[cfg(not(target_arch = "wasm32"))]
use crate::application::platform::watch;
use crate::application::view::chrome::{marks_bar, status, titlebar};
use crate::application::view::design::{DOCK_WIDTH, PROOF_STRIP_HEIGHT};
use crate::application::view::design::{Space, Stroke, TextSize};
use crate::application::view::panels::editor::{editor_pane, overview};
use crate::application::view::panels::info::info_panel;
use crate::application::view::panels::nodes::nodes_pane;
use crate::application::view::panels::preview::{glyph_preview, preview_strip};
use crate::application::view::panels::tabs::{editor_nav, sidebar};
use crate::application::view::{design, label};
use crate::application::widgets::ground::ground;
use crate::application::widgets::menu_shell;
use crate::application::widgets::scroll_viewport::portal;
#[cfg(target_os = "macos")]
use crate::application::widgets::shortcuts;
use crate::application::workspace::{AppState, Mode, Workspace};
use masonry::kurbo::Insets;
use masonry::layout::UnitPoint;
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use masonry::properties::types::CrossAxisAlignment;
use xilem::Color;
use xilem::WidgetView;
use xilem::core::lens;
use xilem::style::Style;
use xilem::view::FlexExt as _;
use xilem::view::ZStackExt as _;
use xilem::view::{canvas, flex_col, sized_box};

/// A kurbo value as the `f32` a Vello text size or stroke width
/// takes.
///
/// The editor's geometry is `f64`, because that is what kurbo and a
/// font's own coordinates are. The few places that hand a number to
/// a text layout want `f32`, so the conversion is here rather than
/// at each call: `f32` holds about seven digits, far below a pixel
/// at any size the interface uses.
pub(crate) fn px32(value: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a text size or stroke width, far inside f32"
    )]
    {
        value as f32
    }
}

#[derive(Clone, Copy)]
enum KeylineEdge {
    Top,
    Bottom,
}

/// Overlay a palette keyline at one edge without consuming layout space.
fn edge_keyline<State, V>(
    content: V,
    edge: KeylineEdge,
    color: Color,
) -> impl WidgetView<State, Widget: Sized> + use<State, V>
where
    State: 'static,
    V: WidgetView<State>,
{
    let (dimensions, alignment) = match edge {
        KeylineEdge::Top => (
            Dimensions::new(Dim::Stretch, Dim::Fixed(Stroke::Hairline.length())),
            UnitPoint::TOP,
        ),
        KeylineEdge::Bottom => (
            Dimensions::new(Dim::Stretch, Dim::Fixed(Stroke::Hairline.length())),
            UnitPoint::BOTTOM,
        ),
    };
    xilem::view::zstack((
        content.alignment(UnitPoint::TOP_LEFT),
        sized_box(canvas(move |_: &mut State, _, scene, size| {
            use masonry::imaging::Painter;
            let mut painter = Painter::new(scene);
            painter.fill(size.to_rect(), color).draw();
        }))
        .dims(dimensions)
        .alignment(alignment),
    ))
}

/// Put the shared outline token at the top of a panel.
pub(crate) fn top_keyline<State, V>(
    content: V,
    color: Color,
) -> impl WidgetView<State, Widget: Sized> + use<State, V>
where
    State: 'static,
    V: WidgetView<State>,
{
    edge_keyline(content, KeylineEdge::Top, color)
}

/// Put the shared outline token at the bottom of a panel.
pub(crate) fn bottom_keyline<State, V>(
    content: V,
    color: Color,
) -> impl WidgetView<State, Widget: Sized> + use<State, V>
where
    State: 'static,
    V: WidgetView<State>,
{
    edge_keyline(content, KeylineEdge::Bottom, color)
}

/// Frame a workspace panel without letting rectangular child backgrounds erase its corners.
fn floating_panel<State, V>(
    content: V,
    pal: &crate::application::view::theme::Palette,
    face: Color,
    ground_color: Color,
) -> impl WidgetView<State, Widget: Sized> + use<State, V>
where
    State: 'static,
    V: WidgetView<State>,
{
    let radius = pal.panel_radius;
    // The frame draws its outline inside the panel box. Clip the face to the
    // outline's inner edge with a concentric radius, so antialiased corner pixels
    // never mix the light face into the outline's outer edge.
    let outline = Stroke::Hairline.px();
    // The lower-left shadow extent is ground: it, and the corners outside the panel's
    // rounded shape, take the window ground; the panel's own face is never tinted beneath.
    let shadow = design::PANEL_SHADOW_OFFSET;
    ground(
        xilem::view::zstack((
            sized_box(crate::application::widgets::rounded_clip::rounded_clip(
                clip_split(sized_box(content).background_color(face)),
                (radius - outline).max(0.0),
            ))
            .padding(Length::px(outline))
            .alignment(UnitPoint::TOP_LEFT),
            crate::application::widgets::panel_frame::panel_frame(
                pal.outline,
                radius,
                pal.main_panel_shadow(),
            )
            .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
            .alignment(UnitPoint::TOP_LEFT),
        )),
        ground_color,
        Insets::new(shadow, 0.0, 0.0, shadow),
        Some(radius),
    )
}

/// Native splitters retain dragged sizes across view rebuilds and window resizes.
fn workspace_columns<State, A, B, C>(
    left: A,
    middle: B,
    right: C,
    collapsed: bool,
    ground_color: Color,
) -> impl WidgetView<State, Widget: Sized> + use<State, A, B, C>
where
    State: 'static,
    A: WidgetView<State>,
    B: WidgetView<State>,
    C: WidgetView<State>,
{
    use crate::application::view::design::{
        CENTER_MIN_WIDTH, SIDEBAR_SPLITTER_HIT_WIDTH, SPLITTER_HIT_WIDTH, WORKSPACE_GUTTER,
    };
    let shadow_extent = design::PANEL_SHADOW_OFFSET;
    let panel_width = DOCK_WIDTH + shadow_extent;
    let panel_gutter = WORKSPACE_GUTTER - shadow_extent;
    let left_gutter = if collapsed { 0.0 } else { panel_gutter };
    let left_width = if collapsed {
        0.0
    } else {
        panel_width + left_gutter
    };
    // The gutters between columns paint the window ground; each panel cuts its own corners.
    let left = ground(
        left,
        ground_color,
        Insets::new(0.0, 0.0, left_gutter, 0.0),
        None,
    );
    let middle = ground(
        middle,
        ground_color,
        Insets::new(0.0, 0.0, panel_gutter, 0.0),
        None,
    );
    let columns = crate::application::widgets::quiet_split::split(left, middle)
        .split_point_from_start(Length::px(left_width))
        .min_lengths(
            Length::px(left_width),
            Length::px(CENTER_MIN_WIDTH + shadow_extent + panel_gutter),
        )
        // Split keeps the generous hit target and all native resize behavior;
        // the visible gutter comes from padding, not its hard-coded bar.
        .bar_thickness(Length::ZERO)
        .min_bar_area(Length::px(SIDEBAR_SPLITTER_HIT_WIDTH))
        .solid_bar(false)
        .draggable(!collapsed);
    let columns = crate::application::widgets::quiet_split::split(columns, right)
        .split_point_from_end(Length::px(panel_width))
        .min_lengths(
            Length::px(CENTER_MIN_WIDTH + shadow_extent + panel_gutter + left_width),
            Length::px(panel_width),
        )
        .bar_thickness(Length::ZERO)
        .min_bar_area(Length::px(SPLITTER_HIT_WIDTH))
        .solid_bar(false);
    clip_split(columns)
}

/// The narrowest window that keeps both docks and the center at their minimum widths.
///
/// Window resizing stops here, at the same limits that dragging a splitter respects.
/// Below it, the splitters cannot meet their minimums and squeeze the docks.
pub(crate) fn workspace_min_width(collapsed: bool) -> f64 {
    use crate::application::view::design::{CENTER_MIN_WIDTH, WORKSPACE_GUTTER};
    let shadow_extent = design::PANEL_SHADOW_OFFSET;
    let panel_width = DOCK_WIDTH + shadow_extent;
    let panel_gutter = WORKSPACE_GUTTER - shadow_extent;
    let left_width = if collapsed {
        0.0
    } else {
        panel_width + panel_gutter
    };
    let center_width = CENTER_MIN_WIDTH + shadow_extent + panel_gutter;
    // The columns' outer padding: the shadow-adjusted left gutter and the full right gutter.
    panel_gutter + left_width + center_width + panel_width + WORKSPACE_GUTTER
}

/// Clip the native splitter's expanded focus outline at the panel boundary.
/// Both axes are constrained, so this Portal cannot scroll or show scrollbars.
fn clip_split<State, V>(content: V) -> xilem::view::Portal<V, State, ()>
where
    State: 'static,
    V: WidgetView<State>,
{
    xilem::view::portal(content)
        .constrain_horizontal(true)
        .constrain_vertical(true)
        .must_fill(true)
}

/// Keep proof height in pixels while allowing its top divider to be dragged.
fn proof_split<State, A, B>(
    editor: A,
    proof: B,
    outline: Color,
) -> impl WidgetView<State, Widget: Sized> + use<State, A, B>
where
    State: 'static,
    A: WidgetView<State>,
    B: WidgetView<State>,
{
    let split =
        crate::application::widgets::quiet_split::split(editor, top_keyline(proof, outline))
            .split_axis(kurbo::Axis::Vertical)
            .split_point_from_end(Length::px(PROOF_STRIP_HEIGHT))
            .min_lengths(
                Length::px(design::EDITOR_MIN_HEIGHT),
                Length::px(design::PROOF_MIN_HEIGHT),
            )
            .bar_thickness(Length::ZERO)
            .min_bar_area(Length::px(design::SPLITTER_HIT_WIDTH))
            .solid_bar(false);
    clip_split(split)
}

/// Let inspector sections take their natural height and give the glyph preview
/// every remaining pixel.
///
/// Unlike a stateful splitter, this recomputes the allocation when a section
/// opens or closes. Expanded sections therefore push the preview down and
/// eventually out of view.
fn inspector_stack<State, A, B>(
    sections: A,
    preview: B,
) -> impl WidgetView<State, Widget: Sized> + use<State, A, B>
where
    State: 'static,
    A: WidgetView<State>,
    B: WidgetView<State>,
{
    flex_col((sections, preview.flex(1.0)))
        .main_axis_alignment(xilem::view::MainAxisAlignment::Start)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Space::None)
}

pub(crate) fn app_logic(app: &mut Workspace) -> impl WidgetView<Workspace> + use<> {
    use xilem::core::one_of::{Either, OneOf3};
    let pal = &app.palette;

    // Left column: category sidebar in overview only. In the editor the
    // tools live in the header, so the left column collapses.
    let _editing_mode = matches!(app.mode, Mode::Editor(_));
    let _ = &app.multi_selected;

    // The header shares the window ground. Each workspace column is an inset
    // panel with its own content and footer.
    let body = match app.mode {
        Mode::Overview => OneOf3::A(overview(app)),
        Mode::Editor(_) => OneOf3::B(editor_pane(app)),
        Mode::Nodes => OneOf3::C(nodes_pane(app)),
    };
    let body = if matches!(app.mode, Mode::Editor(_)) && app.preview_visible {
        Either::A(proof_split(body, preview_strip(app), pal.outline))
    } else {
        Either::B(body)
    };
    // The bottom bar belongs to the middle column, so the sidebar
    // keeps the window's full height and its own marks bar.
    let middle = flex_col((body.flex(1.0), status(app)))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Space::None)
        .background_color(pal.app);

    let left = match app.mode {
        Mode::Overview | Mode::Nodes => Either::A(sidebar(app)),
        Mode::Editor(_) => Either::B(editor_nav(app)),
    };
    // The marks bar sits under the sidebar in both modes, not in the middle
    // column's bar.
    let left = flex_col((left.flex(1.0), marks_bar(app)))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Space::None);
    let inspector_sections = || {
        // The portal can clip the final section rule at its measured boundary.
        // Paint the same keyline inside the viewport so the preview separator
        // stays as solid as every other section divider.
        bottom_keyline(
            portal(sized_box(info_panel(app)).dims(Dimensions::new(Dim::Stretch, Dim::Auto)))
                .constrain_horizontal(true)
                .boxed(),
            pal.outline,
        )
    };
    // Every mode lets its sections take their natural height. The preview
    // fills only the space left below them and moves out of view as they grow.
    let inspector = inspector_stack(inspector_sections(), glyph_preview(app))
        // Erase the inspector tree before adding the two horizontal dock splits;
        // the section tree is already near rustc's recursive trait limit.
        .boxed();
    let columns = workspace_columns(
        floating_panel(
            left.boxed(),
            pal,
            pal.side_panel_face(),
            pal.app_background(),
        ),
        floating_panel(middle.boxed(), pal, pal.panel, pal.app_background()),
        floating_panel(inspector, pal, pal.side_panel_face(), pal.app_background()),
        app.left_collapsed,
        pal.app_background(),
    );
    // The native header already centers its controls between the window edge
    // and this panel edge. A second top gutter makes the space below them
    // larger than the space above, especially beside the macOS traffic lights.
    let gutter = design::WORKSPACE_GUTTER;
    // Each panel reserves its own shadow extent inside the clipped columns.
    let inner_gutter = design::WORKSPACE_GUTTER - design::PANEL_SHADOW_OFFSET;
    let top = if menu_shell::in_window() { gutter } else { 0.0 };
    let columns = ground(
        columns,
        pal.app_background(),
        Insets::new(inner_gutter, top, gutter, inner_gutter),
        None,
    );

    // Boxed on purpose, and not for tidiness. Every wrapper here adds a
    // layer to a monomorphized view type that is already enormous, and
    // with the watcher wrapped around the menu pump around the shortcut
    // host, the mangled symbol name grew past what the macOS linker
    // accepts: "ld: Assertion failed: (name.size() <= maxLength)". Not a
    // compile error, a link error, after a clean build of everything.
    // Erasing the type here cuts the chain.
    let content = flex_col((
        (!menu_shell::in_window()).then(|| titlebar(app)),
        columns.flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Space::None)
    // The ground is painted in the gutters and title bar, never beneath the panels.
    .background_color(pal.window_root_background());
    // Erase the chrome before the async pumps add their own generic layers;
    // otherwise the macOS linker receives multi-megabyte symbol names.
    let content = content.boxed();
    #[cfg(unix)]
    app.live_nodes_pending.store(
        app.live_nodes_need_pump(),
        std::sync::atomic::Ordering::Release,
    );
    let content = live::with_live(
        content,
        app.live
            .as_ref()
            .map(runebender::automation::live_socket::Server::pending_signal),
        app.live_nodes_pending.clone(),
    );
    #[cfg(target_arch = "wasm32")]
    return content;
    #[cfg(not(target_arch = "wasm32"))]
    watch::with_watch(
        ai_pump(
            script_pump(
                chat_pump(
                    export_pump(
                        nodes_pump(
                            preview_pump(content, app.font.project.preview_pending()).boxed(),
                            app.nodes.job.clone(),
                        ),
                        app.export_job.clone(),
                    ),
                    app.chat.job.clone(),
                ),
                app.scripts.running.is_some(),
            ),
            app.ai.job.clone(),
        ),
        app.font.master_paths().clone(),
    )
}

/// Rebuild periodically while the Scripts panel owns a retained background job.
fn script_pump<V: WidgetView<Workspace>>(
    view: V,
    running: bool,
) -> impl WidgetView<Workspace> + use<V> {
    use xilem::core::{MessageProxy, fork};
    use xilem::view::task_raw;
    fork(
        view,
        running.then(|| {
            task_raw(
                |proxy: MessageProxy<scripts::ScriptProgress>, _: &mut Workspace| async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                        if proxy.message(scripts::ScriptProgress).is_err() {
                            return;
                        }
                    }
                },
                |app: &mut Workspace, _: scripts::ScriptProgress| app.script_pump(),
            )
        }),
    )
}

/// Erase the large workspace tree before adapting it to `AppState`.
/// The named return type keeps the concrete tree out of the outer menu/lens
/// symbols, which otherwise exceed the macOS linker's symbol-name limit.
fn boxed_app_logic(app: &mut Workspace) -> Box<xilem::AnyWidgetView<Workspace>> {
    app_logic(app).boxed()
}

/// Builds either the document editor or the no-document welcome state.
///
/// `lens` is the pinned Xilem state adapter: the established editor view
/// continues to receive a `Workspace`, while the application root owns the
/// optional document boundary.
pub(crate) fn root_logic(app: &mut AppState) -> impl WidgetView<AppState> + use<> {
    use xilem::core::one_of::OneOf2;

    let content = match app.workspace.is_some() {
        true => OneOf2::A(lens(boxed_app_logic, |app: &mut AppState| {
            app.workspace
                .as_mut()
                .expect("the document branch has a workspace")
        })),
        false => OneOf2::B(welcome(app)),
    };

    // Native menu installation and the in-window fallback belong to the
    // application boundary so they remain present without an open document.
    actions::install(app);
    #[cfg(target_os = "macos")]
    let root = if std::env::var("RUNEBENDER_IN_WINDOW_MENU").is_ok() {
        OneOf2::A(menu_shell::menu_shell(content, app.palette.clone(), app))
    } else {
        OneOf2::B(shortcuts::shortcut_host(content))
    }
    .boxed();
    #[cfg(not(target_os = "macos"))]
    let root = menu_shell::menu_shell(content, app.palette.clone(), app).boxed();
    actions::with_menu_events(root)
}

/// A stable first frame for a window that has no document yet.
fn welcome(app: &mut AppState) -> impl WidgetView<AppState> + use<> {
    use xilem::view::MainAxisAlignment;

    let palette = &app.palette;
    let detail = app
        .notice
        .clone()
        .unwrap_or_else(|| "Open a font to begin.".into());
    sized_box(
        flex_col((
            label("No font open")
                .text_size(TextSize::Heading.px())
                .color(palette.app_ink),
            label(detail).color(palette.app_ink),
        ))
        .main_axis_alignment(MainAxisAlignment::Center)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Space::Md),
    )
    .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
    .background_color(palette.app_background())
}

/// Drain streamed local-chat events while its child process is running.
fn chat_pump<V: WidgetView<Workspace>>(
    view: V,
    job: Option<chat::ChatJob>,
) -> impl WidgetView<Workspace> + use<V> {
    use xilem::core::{MessageProxy, fork};
    use xilem::view::task_raw;
    fork(
        view,
        job.map(|job| {
            task_raw(
                move |proxy: MessageProxy<chat::ChatProgress>, _: &mut Workspace| {
                    let job = job.clone();
                    async move {
                        loop {
                            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                            let pending = !job
                                .events
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .is_empty();
                            let done = job
                                .finished
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .is_some();
                            if (pending || done) && proxy.message(chat::ChatProgress).is_err() {
                                return;
                            }
                            if done {
                                return;
                            }
                        }
                    }
                },
                |app: &mut Workspace, _: chat::ChatProgress| app.chat_pump(),
            )
        }),
    )
}

/// The same pump for a font-ml run from the Local AI panel: while one
/// is going, poll its progress and its result and post them back.
fn ai_pump<V: WidgetView<Workspace>>(
    view: V,
    job: Option<local_ai::AiJob>,
) -> impl WidgetView<Workspace> + use<V> {
    use xilem::core::{MessageProxy, fork};
    use xilem::view::task_raw;
    fork(
        view,
        job.map(|job| {
            task_raw(
                move |proxy: MessageProxy<local_ai::AiProgress>, _: &mut Workspace| {
                    let job = job.clone();
                    async move {
                        loop {
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                            let done = job
                                .finished
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .is_some();
                            if proxy.message(local_ai::AiProgress).is_err() || done {
                                return;
                            }
                        }
                    }
                },
                |app: &mut Workspace, _: local_ai::AiProgress| app.ai_pump(),
            )
        }),
    )
}

/// Runs `view`, and while a nodes run is going, a task that polls what
/// the run thread has said and posts it back into the application.
/// The same pump shape as the watcher and the menu, for the same
/// reason: Xilem has no hook to drain a channel from a thread.
fn nodes_pump<V: WidgetView<Workspace>>(
    view: V,
    job: Option<nodes::NodeJob>,
) -> impl WidgetView<Workspace> + use<V> {
    use xilem::core::{MessageProxy, fork};
    use xilem::view::task_raw;
    fork(
        view,
        job.map(|job| {
            task_raw(
                move |proxy: MessageProxy<nodes::NodesProgress>, _: &mut Workspace| {
                    let job = job.clone();
                    async move {
                        loop {
                            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                            let pending = !job
                                .events
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .is_empty();
                            let done = job
                                .finished
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .is_some();
                            if (pending || done) && proxy.message(nodes::NodesProgress).is_err() {
                                return;
                            }
                            if done {
                                return;
                            }
                        }
                    }
                },
                |app: &mut Workspace, _: nodes::NodesProgress| app.nodes_pump(),
            )
        }),
    )
}

/// Poll the preview queue until its latest revision has finished compiling.
#[cfg(not(target_arch = "wasm32"))]
fn preview_pump<V: WidgetView<Workspace>>(
    view: V,
    pending: bool,
) -> impl WidgetView<Workspace> + use<V> {
    use xilem::core::{MessageProxy, fork};
    use xilem::view::task;
    fork(
        view,
        pending.then(|| {
            task(
                |proxy: MessageProxy<()>, _: &mut Workspace| async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        if proxy.message(()).is_err() {
                            return;
                        }
                    }
                },
                |app: &mut Workspace, ()| {
                    let _ = app.font.project.request_preview();
                },
            )
        }),
    )
}

fn export_pump<V: WidgetView<Workspace>>(
    view: V,
    job: Option<export::ExportJob>,
) -> impl WidgetView<Workspace> + use<V> {
    use xilem::core::{MessageProxy, fork};
    use xilem::view::task_raw;
    fork(
        view,
        job.map(|job| {
            task_raw(
                move |proxy: MessageProxy<export::ExportProgress>, _: &mut Workspace| {
                    let job = job.clone();
                    async move {
                        loop {
                            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                            if job
                                .finished
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .is_some()
                            {
                                let _ = proxy.message(export::ExportProgress);
                                return;
                            }
                        }
                    }
                },
                |app: &mut Workspace, _: export::ExportProgress| app.export_pump(),
            )
        }),
    )
}

#[cfg(test)]
mod tab_tests {
    use super::*;
    use crate::application::workspace::Tool;
    use std::sync::Arc;

    /// A two-glyph UFO on disk, because `Workspace::open` takes a path. Each
    /// test gets its own directory so they can run in parallel.
    pub(super) fn app() -> Workspace {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        let mut font = norad::Font::new();
        for name in ["A", "B"] {
            let mut glyph = norad::Glyph::new(name);
            glyph.width = 500.0;
            let mut contour = norad::Contour::default();
            for (x, y) in [(0.0, 0.0), (400.0, 0.0), (400.0, 700.0), (0.0, 700.0)] {
                contour.points.push(norad::ContourPoint::new(
                    x,
                    y,
                    norad::PointType::Line,
                    false,
                    None,
                    None,
                ));
            }
            glyph.contours.push(contour);
            font.default_layer_mut().insert_glyph(glyph);
        }
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("runebender-tabs-{n}.ufo"));
        let _ = std::fs::remove_dir_all(&path);
        font.save(&path).expect("save the test font");
        Workspace::open(&path).expect("open the test font")
    }

    #[test]
    fn opening_a_glyph_twice_reuses_its_tab() {
        let mut app = app();
        let a = app.font.index_of("A").expect("A");
        let b = app.font.index_of("B").expect("B");
        app.open_glyph(a);
        app.new_tab();
        app.open_glyph(b);
        let tabs = app.tabs.len();
        app.open_glyph(a);
        assert_eq!(app.tabs.len(), tabs, "no tab was added");
        assert_eq!(app.session.glyph_name, "A");
    }

    #[test]
    fn space_pan_restores_the_persistent_tool() {
        let mut app = app();
        let a = app.font.index_of("A").expect("A");
        app.open_glyph(a);
        app.select_tool(Tool::Pen);

        app.begin_space_pan();
        assert_eq!(app.tool, Tool::Hand);
        assert_eq!(app.tool_before_space_pan, Some(Tool::Pen));
        app.begin_space_pan();
        assert_eq!(
            app.tool_before_space_pan,
            Some(Tool::Pen),
            "repeat is harmless"
        );
        app.end_space_pan();
        assert_eq!(app.tool, Tool::Pen);
        assert_eq!(app.tool_before_space_pan, None);

        app.begin_space_pan();
        app.new_tab();
        assert_eq!(
            app.tool,
            Tool::Pen,
            "a tab switch restores the persistent tool"
        );
        assert_eq!(app.tabs[0].tool, Tool::Pen);
        assert_eq!(app.tabs[1].tool, Tool::Pen);

        app.select_tool(Tool::Hand);
        app.begin_space_pan();
        assert_eq!(app.tool_before_space_pan, Some(Tool::Hand));
        app.end_space_pan();
        assert_eq!(
            app.tool,
            Tool::Hand,
            "a selected Hand tool remains selected"
        );
    }

    #[test]
    fn a_tab_keeps_its_own_selection() {
        let mut app = app();
        let a = app.font.index_of("A").expect("A");
        app.open_glyph(a);
        let mut session = (*app.session).clone();
        session.select_all();
        app.session = Arc::new(session);
        let selected = app.session.selection.len();
        assert!(selected > 0, "the test glyph has points");

        app.new_tab();
        assert_eq!(app.session.selection.len(), 0, "the new tab starts clean");

        app.activate_tab(0);
        assert_eq!(
            app.session.selection.len(),
            selected,
            "the first tab kept it"
        );
    }

    #[test]
    fn tabs_keep_independent_text_and_preview_contexts() {
        use runebender::text::buffer::TextDirection;

        let mut app = app();
        let a = app.font.index_of("A").expect("A");
        app.open_glyph(a);
        app.set_editor_text("first editor".into());
        app.preview_text = "first preview".into();
        app.text_dir = Some(TextDirection::RightToLeft);
        app.text_features_disabled.insert("rlig".into());
        app.text_script = Some("arab".into());
        app.text_language = Some("ur".into());

        app.new_tab();
        assert_ne!(app.tabs[0].text_context_id, app.tabs[1].text_context_id);
        app.set_editor_text("second editor".into());
        app.preview_text = "second preview".into();
        app.text_dir = Some(TextDirection::LeftToRight);
        app.text_features_disabled.clear();
        app.text_script = None;
        app.text_language = None;

        app.activate_tab(0);
        assert_eq!(app.initial_text, "first editor");
        assert!(app.has_text_session);
        assert_eq!(app.preview_text, "first preview");
        assert_eq!(app.text_dir, Some(TextDirection::RightToLeft));
        assert!(app.text_features_disabled.contains("rlig"));
        assert_eq!(app.text_script.as_deref(), Some("arab"));
        assert_eq!(app.text_language.as_deref(), Some("ur"));

        app.activate_tab(1);
        assert_eq!(app.initial_text, "second editor");
        assert!(app.has_text_session);
        assert_eq!(app.preview_text, "second preview");
        assert_eq!(app.text_dir, Some(TextDirection::LeftToRight));
        assert!(app.text_features_disabled.is_empty());
        assert_eq!(app.text_script, None);
        assert_eq!(app.text_language, None);
    }

    #[test]
    fn activating_a_text_sort_keeps_the_word_in_its_current_tab() {
        let mut app = app();
        let a = app.font.index_of("A").expect("A");
        let b = app.font.index_of("B").expect("B");

        app.open_glyph(a);
        app.tool = Tool::Text;
        app.set_editor_text("AB".into());
        app.park();

        // Give B another existing tab. The ordinary open-glyph path would
        // follow this tab and replace the word with its parked context.
        app.new_tab();
        app.open_glyph(b);
        app.set_editor_text("B".into());
        app.park();

        app.activate_tab(0);
        let text_tab = app.active_tab;
        let context = app.text_context_id();
        app.edit_text_sort_glyph(b, Tool::Text);

        assert_eq!(
            app.active_tab, text_tab,
            "sort activation does not switch tabs"
        );
        assert_eq!(
            app.text_context_id(),
            context,
            "the buffer identity is stable"
        );
        assert_eq!(
            app.initial_text, "AB",
            "the surrounding word remains parked"
        );
        assert_eq!(app.tabs[text_tab].text_context.editor_text, "AB");
        assert_eq!(app.session.glyph_name, "B");
        assert!(matches!(app.mode, Mode::Editor(index) if index == b));
        assert_eq!(app.tool, Tool::Text);

        app.select_tool(Tool::Select);
        assert_eq!(app.initial_text, "AB");
        assert!(app.has_text_session, "Select keeps the composition open");
        assert_eq!(app.tabs[text_tab].text_context.editor_text, "AB");
        app.select_tool(Tool::Text);
        assert_eq!(
            app.initial_text, "AB",
            "returning to Text restores the word"
        );

        app.edit_text_sort_glyph(a, Tool::Select);
        assert_eq!(app.tool, Tool::Select);
        assert_eq!(app.initial_text, "AB");
        assert_eq!(app.tabs[text_tab].text_context.editor_text, "AB");
        assert_eq!(app.session.glyph_name, "A");
    }

    /// Renaming used to rebuild the model from the active master, which
    /// dropped the other masters, the axes and their locations. In a
    /// designspace that meant losing interpolation and saving one UFO.
    #[test]
    fn renaming_keeps_the_other_masters() {
        let mut app = app();
        let a = app.font.index_of("A").expect("A");
        app.open_glyph(a);
        let masters = app.font.master_names().len();
        app.name_buf = "Alpha".into();
        app.commit_rename();
        assert_eq!(app.font.master_names().len(), masters);
        assert!(app.font.index_of("Alpha").is_some());
        assert!(app.font.index_of("A").is_none());
    }

    /// A tab addresses its glyph by name, so a rename has to reach it.
    #[test]
    fn renaming_follows_the_open_tab() {
        let mut app = app();
        let a = app.font.index_of("A").expect("A");
        app.open_glyph(a);
        app.name_buf = "Alpha".into();
        app.commit_rename();
        assert_eq!(app.session.glyph_name, "Alpha");
        assert!(
            app.tabs.iter().all(|tab| tab.session.glyph_name != "A"),
            "no tab still points at the old name"
        );
        // And the tab still resolves: activating it finds the glyph.
        app.activate_tab(0);
        assert_eq!(app.session.glyph_name, "Alpha");
    }

    #[test]
    fn closing_the_last_tab_leaves_the_editor() {
        let mut app = app();
        let a = app.font.index_of("A").expect("A");
        app.open_glyph(a);
        app.close_tab(0);
        assert!(matches!(app.mode, Mode::Overview));
    }

    #[test]
    fn closing_a_tab_before_the_active_one_keeps_the_right_glyph() {
        let mut app = app();
        let a = app.font.index_of("A").expect("A");
        let b = app.font.index_of("B").expect("B");
        app.open_glyph(a);
        app.new_tab();
        app.open_glyph(b);
        assert_eq!(app.tabs.len(), 2);
        app.activate_tab(1);
        let active = app.session.glyph_name.clone();
        app.close_tab(0);
        assert_eq!(app.session.glyph_name, active);
    }
}

#[cfg(test)]
mod panel_resize_tests {
    use super::{floating_panel, inspector_stack, proof_split, workspace_columns};
    use masonry::core::keyboard::{Key, NamedKey};
    use masonry::core::{TextEvent, WindowEvent};
    use masonry::dpi::PhysicalSize;
    use masonry::kurbo::Point;
    use masonry_testing::TestHarness;
    use std::sync::Arc;
    use xilem::ViewCtx;
    use xilem::core::{ProxyError, RawProxy, SendMessage, ViewId};

    #[derive(Debug)]
    struct NoProxy;
    impl RawProxy for NoProxy {
        fn send_message(&self, _: Arc<[ViewId]>, _: SendMessage) -> Result<(), ProxyError> {
            Ok(())
        }
        fn dyn_debug(&self) -> &dyn std::fmt::Debug {
            self
        }
    }
    fn context() -> ViewCtx {
        ViewCtx::new(
            Arc::new(NoProxy),
            Arc::new(
                tokio::runtime::Builder::new_current_thread()
                    .build()
                    .unwrap(),
            ),
        )
    }

    #[test]
    fn editor_repaints_every_theme_without_reopening_the_glyph() {
        use xilem::core::View;
        use xilem::view::sized_box;

        let mut app = super::tab_tests::app();
        app.open_glyph(app.font.index_of("A").expect("A"));
        app.set_theme("gray");
        let session = app.session.clone();
        let revision = app.font.project.document_revision();
        let mut ctx = context();
        let mut view = sized_box(super::editor_pane(&app));
        let (pod, mut state) = view.build(&mut ctx, &mut app);
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (786, 510),
        );
        let initial = harness.render();

        let themes = runebender::ui::theme::BUILTIN_THEME_IDS;
        for theme in themes
            .iter()
            .copied()
            .chain(std::iter::once("gray"))
            .chain(themes.iter().rev().copied())
            .chain(std::iter::once("gray"))
        {
            app.set_theme(theme);
            let next = sized_box(super::editor_pane(&app));
            harness.edit_root_widget(|root| {
                next.rebuild(&view, &mut state, &mut ctx, root, &mut app);
            });
            view = next;
            let actual = harness.render();

            // Compare the retained canvas, including its metrics card and outline,
            // against a newly opened canvas in the requested theme.
            let fresh = sized_box(super::editor_pane(&app));
            let mut fresh_ctx = context();
            let (fresh_pod, _) = fresh.build(&mut fresh_ctx, &mut app);
            let mut fresh_harness = TestHarness::create_with_size(
                crate::application::view::default_property_set(),
                fresh_pod.new_widget,
                (786, 510),
            );
            assert!(
                actual == fresh_harness.render(),
                "stale editor colors in {theme}"
            );
            if theme == "gray" {
                assert!(
                    actual == initial,
                    "Gray must return to its original appearance"
                );
            }
            assert!(Arc::ptr_eq(&session, &app.session));
            assert_eq!(app.font.project.document_revision(), revision);
        }
    }

    #[test]
    fn center_footer_icons_and_slider_share_edge_clearance_in_both_modes() {
        use crate::application::view::design::{FOOTER_HEIGHT, FOOTER_INSET, STATUS_ICON_SIZE};
        use crate::application::widgets::gesture_slider::GestureSliderWidget;
        use crate::application::widgets::icon_button::IconWidget;
        use masonry::core::{Widget, WidgetRef};
        use masonry::kurbo::Rect;
        use xilem::core::View;
        use xilem::style::Style;
        fn controls(
            widget: WidgetRef<'_, dyn Widget>,
            icons: &mut Vec<Rect>,
            sliders: &mut Vec<Rect>,
        ) {
            let rect = widget
                .ctx()
                .window_transform()
                .transform_rect_bbox(widget.ctx().border_box());
            if widget.downcast::<IconWidget>().is_some() {
                icons.push(rect);
            }
            if widget.downcast::<GestureSliderWidget>().is_some() {
                sliders.push(rect);
            }
            for child in widget.children() {
                controls(child, icons, sliders);
            }
        }
        for mode in [super::Mode::Overview, super::Mode::Editor(0)] {
            let mut app = super::tab_tests::app();
            app.mode = mode;
            app.note = "A long status message that should yield its width to the controls".into();
            let view =
                xilem::view::flex_col((super::status(&app), xilem::view::FlexSpacer::Flex(1.0)))
                    .gap(super::Space::None);
            let mut ctx = context();
            let (pod, _) = view.build(&mut ctx, &mut app);
            let mut harness = TestHarness::create_with_size(
                crate::application::view::default_property_set(),
                pod.new_widget,
                (320, 100),
            );
            for width in [320, 997] {
                harness.process_window_event(WindowEvent::Resize(PhysicalSize::new(width, 100)));
                let footer = harness.root_widget().children()[0];
                assert_eq!(footer.ctx().border_box().height(), FOOTER_HEIGHT);
                let mut icons = Vec::new();
                let mut sliders = Vec::new();
                controls(footer, &mut icons, &mut sliders);
                assert!(!icons.is_empty());
                assert_eq!(icons[0].x0, FOOTER_INSET);
                for icon in icons {
                    assert_eq!(icon.height(), STATUS_ICON_SIZE);
                    assert_eq!(icon.y0, (FOOTER_HEIGHT - STATUS_ICON_SIZE) / 2.0);
                    assert_eq!(
                        FOOTER_HEIGHT - icon.y1,
                        (FOOTER_HEIGHT - STATUS_ICON_SIZE) / 2.0
                    );
                }
                assert_eq!(sliders.len(), 1);
                assert_eq!(f64::from(width) - sliders[0].x1, FOOTER_INSET);
                assert_eq!((sliders[0].y0 + sliders[0].y1) / 2.0, FOOTER_HEIGHT / 2.0);
            }
        }
    }

    #[test]
    fn palette_circles_have_equal_outer_and_inner_gaps_when_the_dock_grows() {
        use xilem::core::View;
        use xilem::style::Style;
        let mut app = super::tab_tests::app();
        let view =
            xilem::view::flex_col((super::marks_bar(&app), xilem::view::FlexSpacer::Flex(1.0)))
                .gap(super::Space::None);
        let mut ctx = context();
        let (pod, _) = view.build(&mut ctx, &mut app);
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (246, 100),
        );
        for width in [246, 384] {
            harness.process_window_event(WindowEvent::Resize(PhysicalSize::new(width, 100)));
            let root = harness.root_widget();
            let strip = root.children()[0].children()[0].children()[1];
            let row = strip.children()[0];
            let buttons = row.children();
            let gap = super::design::MARK_SWATCH_GAP;
            let diameter = (f64::from(width) - 9.0 * gap) / 8.0;
            assert_eq!(buttons.len(), 8);
            for (i, button) in buttons.iter().enumerate() {
                let rect = button.ctx().border_box();
                assert!(
                    (rect.width() - diameter).abs() <= 1.0,
                    "width {width}: {rect:?}, expected {diameter}"
                );
                assert!((rect.height() - diameter).abs() <= 1.0);
                let origin = button.ctx().window_transform() * Point::ORIGIN;
                assert!((origin.x - (gap + i as f64 * (diameter + gap))).abs() <= 1.0);
                assert!((origin.y - (1.0 + gap)).abs() <= 1.0);
            }
        }
    }

    #[test]
    fn panel_groups_keep_equal_side_and_bottom_insets_at_the_divider() {
        use crate::application::view::recipes;
        use crate::application::view::theme::Palette;
        use masonry::layout::{Dim, Length};
        use masonry::properties::Dimensions;
        use xilem::core::View;
        use xilem::style::Style;
        use xilem::view::{label, sized_box};

        let palette = Palette::load("gray");
        let group = || {
            recipes::panel_group(
                &palette,
                sized_box(label(""))
                    .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(20.0))))
                    .background_color(palette.field()),
            )
        };
        let view =
            sized_box(recipes::panel_stack((group(), group()))).background_color(palette.panel);
        let mut ctx = context();
        let (pod, _) = view.build(&mut ctx, &mut ());
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (100, 74),
        );
        let image = harness.render();
        let ground = image.get_pixel(0, 0);
        let face = image.get_pixel(8, 8);
        assert_ne!(face, ground);
        for section_y in [0, 37] {
            assert_eq!(image.get_pixel(8, section_y + 8), face);
            assert_eq!(image.get_pixel(91, section_y + 27), face);
            for inset in 0..8 {
                assert_eq!(image.get_pixel(inset, section_y + 18), ground, "left inset");
                assert_eq!(
                    image.get_pixel(99 - inset, section_y + 18),
                    ground,
                    "right inset"
                );
                assert_eq!(image.get_pixel(50, section_y + inset), ground, "top inset");
                assert_eq!(
                    image.get_pixel(50, section_y + 28 + inset),
                    ground,
                    "bottom inset"
                );
            }
            assert_ne!(
                image.get_pixel(50, section_y + 36),
                ground,
                "the divider follows the bottom inset"
            );
        }
    }

    #[test]
    fn panel_field_columns_fit_the_minimum_dock_and_grow_evenly() {
        use crate::application::view::{design, recipes};
        use xilem::core::View;
        use xilem::view::sized_box;

        for count in [2, 3] {
            let mut app = super::tab_tests::app();
            let fields = (0..count)
                .map(|_| {
                    recipes::field_column(recipes::field(
                        &app.palette,
                        "Width",
                        "A value longer than the available column".into(),
                        |_: &mut super::Workspace, _| {},
                    ))
                })
                .collect::<Vec<_>>();
            let view = sized_box(recipes::panel_group(
                &app.palette,
                design::row(design::Region::Form, fields),
            ));
            let mut ctx = context();
            let (pod, _) = view.build(&mut ctx, &mut app);
            let mut harness = TestHarness::create_with_size(
                crate::application::view::default_property_set(),
                pod.new_widget,
                (246, 100),
            );
            for width in [246, 384] {
                harness.process_window_event(WindowEvent::Resize(PhysicalSize::new(width, 100)));
                let row = harness.root_widget().children()[0].children()[0].children()[0];
                let content_width = f64::from(width) - 2.0 * design::PANEL_SECTION_INSET.px();
                let expected = (content_width
                    - f64::from(count - 1) * design::Region::Form.gap().px())
                    / f64::from(count);
                assert_eq!(row.ctx().border_box().width(), content_width);
                assert_eq!(row.children().len(), usize::try_from(count).unwrap());
                let mut used_width = f64::from(count - 1) * design::Region::Form.gap().px();
                for column in row.children() {
                    let actual = column.ctx().border_box().width();
                    assert!(
                        (actual - expected).abs() <= 1.0,
                        "column width {actual} should equal {expected} within pixel rounding"
                    );
                    used_width += actual;
                }
                assert!((used_width - content_width).abs() < 0.01);
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn fullscreen_header_removes_and_restores_window_controls_inset() {
        use crate::application::view::chrome::titlebar;
        use xilem::core::View;

        let mut app = super::tab_tests::app();
        let mut ctx = context();
        let normal = xilem::view::sized_box(titlebar(&app));
        let (pod, mut state) = normal.build(&mut ctx, &mut app);
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (1336, 30),
        );
        let normal_count = harness.root_widget().children()[0].children()[0]
            .children()
            .len();

        app.fullscreen = true;
        let fullscreen = xilem::view::sized_box(titlebar(&app));
        harness.edit_root_widget(|root| {
            fullscreen.rebuild(&normal, &mut state, &mut ctx, root, &mut app);
        });
        assert_eq!(
            harness.root_widget().children()[0].children()[0]
                .children()
                .len(),
            normal_count - 1,
            "full screen must remove the window-controls spacer"
        );

        app.fullscreen = false;
        let restored = xilem::view::sized_box(titlebar(&app));
        harness.edit_root_widget(|root| {
            restored.rebuild(&fullscreen, &mut state, &mut ctx, root, &mut app);
        });
        assert_eq!(
            harness.root_widget().children()[0].children()[0]
                .children()
                .len(),
            normal_count
        );
    }

    #[test]
    fn floating_panel_rounds_children_without_blocking_clicks() {
        use crate::application::view::theme::Palette;
        use masonry::layout::{Dim, Length};
        use masonry::properties::Dimensions;
        use masonry::widgets::ButtonPress;
        use xilem::core::View;
        use xilem::style::Style;
        use xilem::view::{button, label, sized_box};

        for radius in [0.0, 8.0] {
            let mut palette = Palette::load("gray");
            palette.panel_radius = radius;
            let content = button(label(""), |_: &mut ()| {})
                .background_color(palette.panel)
                .border_width(Length::ZERO)
                .dims(Dimensions::new(Dim::Stretch, Dim::Stretch));
            let view = sized_box(floating_panel(
                content,
                &palette,
                palette.panel,
                xilem::Color::TRANSPARENT,
            ))
            .padding(Length::px(8.0))
            .background_color(palette.app);
            let mut ctx = context();
            let (pod, _) = view.build(&mut ctx, &mut ());
            let mut harness = TestHarness::create_with_size(
                crate::application::view::default_property_set(),
                pod.new_widget,
                (96, 96),
            );
            let image = harness.render();
            let ground = image.get_pixel(0, 0);
            let face = image.get_pixel(20, 20);
            assert_ne!(ground, face);
            assert_eq!(
                image.get_pixel(11, 9),
                if radius == 0.0 { face } else { ground },
                "the panel must retain square and rounded theme geometry"
            );
            harness.mouse_move(Point::new(48.0, 48.0));
            harness.mouse_button_press(None);
            harness.mouse_button_release(None);
            assert!(
                harness.pop_action::<ButtonPress>().is_some(),
                "the frame must pass pointer input through to its content"
            );
        }
    }

    #[test]
    fn rounded_panel_face_stays_inside_its_outline() {
        use crate::application::view::theme::Palette;
        use masonry::layout::{Dim, Length};
        use masonry::properties::Dimensions;
        use xilem::core::View;
        use xilem::style::Style;
        use xilem::view::{label, sized_box};

        let mut palette = Palette::load("gray");
        palette.panel_radius = 8.0;
        let view = sized_box(floating_panel(
            sized_box(label("")).dims(Dimensions::new(Dim::Stretch, Dim::Stretch)),
            &palette,
            palette.panel,
            xilem::Color::TRANSPARENT,
        ))
        .padding(Length::px(8.0))
        .background_color(palette.app);
        let mut ctx = context();
        let (pod, _) = view.build(&mut ctx, &mut ());
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (96, 96),
        );
        let image = harness.render();
        let brightest = |x: u32, y: u32| image.get_pixel(x, y).0[..3].iter().copied().max();
        let ground = brightest(0, 0).expect("pixel has color channels");
        assert!(
            brightest(48, 8).expect("pixel has color channels") < ground,
            "the top edge must show the outline"
        );

        // The panel box spans x 10..88 and y 8..86, and the shadow falls down and left.
        // Scanning each top-right corner row inward, nothing may be brighter than the
        // ground until the outline first darkens a pixel; a brighter pixel is face leaking out.
        for y in 8..16 {
            for x in (78..96).rev() {
                let value = brightest(x, y).expect("pixel has color channels");
                if value + 3 < ground {
                    break;
                }
                assert!(
                    value <= ground + 2,
                    "the panel face showed outside its outline at ({x}, {y}): {value}"
                );
            }
        }
    }

    #[test]
    fn panel_shadows_toggle_without_moving_content_and_survive_resizing() {
        use crate::application::view::theme::Palette;
        use image::Rgba;
        use masonry::layout::Length;
        use xilem::core::View;
        use xilem::style::Style;
        use xilem::view::{label, sized_box};

        let mut palette = Palette::load("gray");
        let shadow = palette.panel_shadow;
        palette.panel_shadow = None;
        let logic = |pal: &Palette| {
            sized_box(floating_panel(
                label(""),
                pal,
                pal.panel,
                xilem::Color::TRANSPARENT,
            ))
            .padding(Length::px(8.0))
            .background_color(pal.app)
        };
        let mut ctx = context();
        let off = logic(&palette);
        let (pod, mut state) = off.build(&mut ctx, &mut ());
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (96, 96),
        );
        let baseline = harness.render();
        let ground = *baseline.get_pixel(0, 0);
        assert_eq!(*baseline.get_pixel(48, 87), ground);

        palette.panel_shadow = shadow;
        let on = logic(&palette);
        harness.edit_root_widget(|root| on.rebuild(&off, &mut state, &mut ctx, root, &mut ()));
        let rendered = harness.render();
        let color = runebender::ui::theme::load_theme("gray")
            .expect("Gray")
            .surface("panelShadow");
        let midpoint = Rgba([
            color.r.midpoint(ground[0]),
            color.g.midpoint(ground[1]),
            color.b.midpoint(ground[2]),
            255,
        ]);
        let expected = *rendered.get_pixel(48, 86);
        for channel in 0..4 {
            assert!(
                expected[channel].abs_diff(midpoint[channel]) <= 1,
                "the shadow must blend halfway into the ground"
            );
        }
        for y in 86..88 {
            assert_eq!(
                *rendered.get_pixel(48, y),
                expected,
                "the full shadow must be visible"
            );
        }
        assert_eq!(*rendered.get_pixel(48, 88), ground);
        for x in 8..10 {
            assert_eq!(
                *rendered.get_pixel(x, 48),
                expected,
                "the left shadow must be visible"
            );
        }
        assert_eq!(*rendered.get_pixel(7, 48), ground);
        assert_eq!(rendered.get_pixel(10, 48), baseline.get_pixel(10, 48));
        assert_eq!(
            rendered.get_pixel(48, 85),
            baseline.get_pixel(48, 85),
            "the panel edge must stay put"
        );
        assert_eq!(rendered.get_pixel(48, 48), baseline.get_pixel(48, 48));

        // The harness keeps its initial raster dimensions, so resize within that image.
        harness.process_window_event(WindowEvent::Resize(PhysicalSize::new(80, 80)));
        let resized = harness.render();
        assert_eq!(*resized.get_pixel(40, 71), expected);
        assert_eq!(*resized.get_pixel(40, 72), ground);

        palette.panel_shadow = None;
        let off_again = logic(&palette);
        harness
            .edit_root_widget(|root| off_again.rebuild(&on, &mut state, &mut ctx, root, &mut ()));
        assert_eq!(*harness.render().get_pixel(40, 71), ground);
    }

    #[test]
    fn rounded_panel_corners_reveal_the_actual_background() {
        use masonry::imaging::Painter;
        use masonry::kurbo::Rect;
        use masonry::layout::Length;
        use xilem::Color;
        use xilem::core::View;
        use xilem::style::Style;
        use xilem::view::{canvas, label, sized_box};

        let mut palette = crate::application::view::theme::Palette::load("gray");
        palette.panel_shadow = None;
        palette.panel_radius = 16.0;
        let logic = |pal: &crate::application::view::theme::Palette| {
            xilem::view::zstack((
                canvas(|_: &mut (), _, scene, size| {
                    let mut painter = Painter::new(scene);
                    painter
                        .fill(size.to_rect(), Color::from_rgb8(220, 0, 180))
                        .draw();
                    painter
                        .fill(
                            Rect::new(size.width / 2.0, 0.0, size.width, size.height),
                            Color::from_rgb8(0, 180, 220),
                        )
                        .draw();
                }),
                sized_box(floating_panel(
                    label(""),
                    pal,
                    pal.panel,
                    Color::TRANSPARENT,
                ))
                .padding(Length::px(8.0)),
            ))
        };
        let mut ctx = context();
        let rounded = logic(&palette);
        let (pod, mut state) = rounded.build(&mut ctx, &mut ());
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (96, 96),
        );
        let image = harness.render();
        for (corner_x, background_x) in [(10, 0), (87, 95)] {
            for y in [8, 85] {
                assert_eq!(
                    image.get_pixel(corner_x, y),
                    image.get_pixel(background_x, y),
                    "rounded corners must reveal either backdrop color without a flat mask"
                );
            }
        }
        assert_ne!(image.get_pixel(48, 48), image.get_pixel(95, 48));

        palette.panel_radius = 0.0;
        let square = logic(&palette);
        harness.edit_root_widget(|root| {
            square.rebuild(&rounded, &mut state, &mut ctx, root, &mut ());
        });
        let image = harness.render();
        assert_ne!(
            image.get_pixel(11, 9),
            image.get_pixel(0, 9),
            "changing the theme radius must update the panel clip"
        );
    }

    #[test]
    fn both_docks_drag_and_retain_widths_after_rebuild_and_window_resize() {
        use xilem::core::View;
        use xilem::view::label;
        let logic = || {
            workspace_columns(
                label("Left"),
                label("Canvas"),
                label("Right"),
                false,
                xilem::Color::TRANSPARENT,
            )
        };
        let mut ctx = context();
        let view = logic();
        let (pod, mut state) = view.build(&mut ctx, &mut ());
        let mut h = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (1280, 650),
        );
        fn widths<W: masonry::core::Widget>(h: &TestHarness<W>) -> (f64, f64, f64) {
            let root = h.root_widget();
            let clip_children = root.children();
            let children = clip_children[0].children();
            let nested = children[0].children();
            (
                nested[0].children()[0].ctx().border_box().width(),
                nested[1].children()[0].ctx().border_box().width(),
                children[1].ctx().border_box().width(),
            )
        }
        assert_eq!(widths(&h), (248.0, 772.0, 248.0));
        // Six pixels beside the visible line, outside the old hit target.
        h.mouse_move(Point::new(247.5, 100.0));
        h.mouse_button_press(None);
        h.mouse_move(Point::new(327.5, 100.0));
        h.mouse_button_release(None);
        assert_eq!(widths(&h), (328.0, 692.0, 248.0));
        h.mouse_move(Point::new(1033.5, 100.0));
        h.mouse_button_press(None);
        h.mouse_move(Point::new(953.5, 100.0));
        h.mouse_button_release(None);
        assert_eq!(widths(&h), (328.0, 612.0, 328.0));
        let again = logic();
        h.edit_root_widget(|root| again.rebuild(&view, &mut state, &mut ctx, root, &mut ()));
        assert_eq!(widths(&h), (328.0, 612.0, 328.0));
        h.process_window_event(WindowEvent::Resize(PhysicalSize::new(1100, 650)));
        assert_eq!(widths(&h), (328.0, 432.0, 328.0));
        h.mouse_move(Point::new(334.5, 100.0));
        h.mouse_button_press(None);
        h.mouse_move(Point::new(20.0, 100.0));
        h.mouse_button_release(None);
        assert_eq!(
            widths(&h).0,
            crate::application::view::design::DOCK_WIDTH + super::design::PANEL_SHADOW_OFFSET
        );
        let right_divider = 1100.0 - widths(&h).2;
        h.mouse_move(Point::new(right_divider, 100.0));
        h.mouse_button_press(None);
        h.mouse_move(Point::new(1090.0, 100.0));
        h.mouse_button_release(None);
        assert_eq!(
            widths(&h).2,
            crate::application::view::design::DOCK_WIDTH + super::design::PANEL_SHADOW_OFFSET
        );
    }

    #[test]
    fn a_translucent_ground_is_painted_exactly_once_in_every_gutter() {
        use masonry_testing::TestHarnessParams;
        use xilem::core::View;
        use xilem::view::label;
        let mut palette = crate::application::view::theme::Palette::load("gray");
        // Over the native backdrop, Gray draws no panel shadow; it would darken the gutter.
        palette.panel_shadow = None;
        let ground = xilem::Color::from_rgba8(255, 0, 0, 128);
        let panel = || floating_panel(label(""), &palette, palette.panel, ground);
        let view = workspace_columns(panel(), panel(), panel(), false, ground);
        let mut ctx = context();
        let (pod, _) = view.build(&mut ctx, &mut ());
        // An odd width splits the center into a fractional width, as dragging does.
        let mut h = TestHarness::create_with(
            crate::application::view::default_property_set(),
            pod.new_widget,
            TestHarnessParams::default()
                .with_size((1281, 400))
                .with_background(xilem::Color::TRANSPARENT),
        );
        let image = h.render();
        // Rows away from the rounded corners cross every vertical gutter; columns through
        // each panel cross the horizontal edges. Outline antialiasing only occurs at corners.
        let rows = [100, 200, 300]
            .into_iter()
            .flat_map(|y| (0..image.width()).map(move |x| (x, y)));
        let columns = [100, 640, 1200]
            .into_iter()
            .flat_map(|x| (0..image.height()).map(move |y| (x, y)));
        for (x, y) in rows.chain(columns) {
            let alpha = image.get_pixel(x, y).0[3];
            assert!(
                alpha == 128 || alpha == 255,
                "pixel ({x}, {y}) has alpha {alpha}: the ground overlaps itself or a panel"
            );
        }
    }

    #[test]
    fn minimum_window_width_keeps_dragged_docks_at_their_minimums() {
        use crate::application::view::design::{
            CENTER_MIN_WIDTH, DOCK_WIDTH, PANEL_SHADOW_OFFSET, WORKSPACE_GUTTER,
        };
        use xilem::core::View;
        use xilem::view::label;
        let view = workspace_columns(
            label("Left"),
            label("Canvas"),
            label("Right"),
            false,
            xilem::Color::TRANSPARENT,
        );
        let mut ctx = context();
        let (pod, _) = view.build(&mut ctx, &mut ());
        let mut h = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (1280, 650),
        );
        fn widths<W: masonry::core::Widget>(h: &TestHarness<W>) -> (f64, f64, f64) {
            let root = h.root_widget();
            let clip_children = root.children();
            let children = clip_children[0].children();
            let nested = children[0].children();
            (
                nested[0].children()[0].ctx().border_box().width(),
                nested[1].children()[0].ctx().border_box().width(),
                children[1].ctx().border_box().width(),
            )
        }
        // Widen both docks, then shrink to the window minimum less the outer padding.
        h.mouse_move(Point::new(247.5, 100.0));
        h.mouse_button_press(None);
        h.mouse_move(Point::new(327.5, 100.0));
        h.mouse_button_release(None);
        h.mouse_move(Point::new(1033.5, 100.0));
        h.mouse_button_press(None);
        h.mouse_move(Point::new(953.5, 100.0));
        h.mouse_button_release(None);
        let padding = (WORKSPACE_GUTTER - PANEL_SHADOW_OFFSET) + WORKSPACE_GUTTER;
        assert_eq!(super::workspace_min_width(false) - padding, 790.0);
        h.process_window_event(WindowEvent::Resize(PhysicalSize::new(790, 650)));
        let dock = DOCK_WIDTH + PANEL_SHADOW_OFFSET;
        assert_eq!(
            widths(&h),
            (dock, CENTER_MIN_WIDTH + PANEL_SHADOW_OFFSET, dock)
        );
    }

    #[test]
    fn collapsed_left_panel_reopens_without_disabling_the_inspector_splitter() {
        use xilem::core::View;
        use xilem::view::label;
        let logic = |collapsed| {
            workspace_columns(
                label("Left"),
                label("Canvas"),
                label("Right"),
                collapsed,
                xilem::Color::TRANSPARENT,
            )
        };
        let mut ctx = context();
        let view = logic(true);
        let (pod, mut state) = view.build(&mut ctx, &mut ());
        let mut h = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (1280, 650),
        );
        assert_eq!(
            h.root_widget().children()[0].children()[0].children()[0]
                .ctx()
                .border_box()
                .width(),
            0.0
        );
        h.mouse_move(Point::new(1033.5, 100.0));
        h.mouse_button_press(None);
        h.mouse_move(Point::new(953.5, 100.0));
        h.mouse_button_release(None);
        assert_eq!(
            h.root_widget().children()[0].children()[1]
                .ctx()
                .border_box()
                .width(),
            328.0
        );
        let again = logic(false);
        h.edit_root_widget(|root| again.rebuild(&view, &mut state, &mut ctx, root, &mut ()));
        assert_eq!(
            h.root_widget().children()[0].children()[0].children()[0]
                .ctx()
                .border_box()
                .width(),
            254.0
        );
        assert_eq!(
            h.root_widget().children()[0].children()[1]
                .ctx()
                .border_box()
                .width(),
            328.0
        );
    }

    #[test]
    fn proof_divider_drags_and_supports_keyboard_resizing() {
        use xilem::core::View;
        use xilem::view::label;
        let outline = masonry::peniko::Color::from_rgb8(29, 29, 29);
        let logic = || proof_split(label("Editor"), label("Proof"), outline);
        let mut ctx = context();
        let view = logic();
        let (pod, mut state) = view.build(&mut ctx, &mut ());
        let mut h = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (800, 651),
        );
        fn heights<W: masonry::core::Widget>(h: &TestHarness<W>) -> (f64, f64) {
            let root = h.root_widget();
            let clip_children = root.children();
            let children = clip_children[0].children();
            (
                children[0].ctx().border_box().height(),
                children[1].ctx().border_box().height(),
            )
        }
        assert_eq!(heights(&h), (511.0, 140.0));
        h.mouse_move(Point::new(400.0, 510.5));
        h.mouse_button_press(None);
        h.mouse_move(Point::new(400.0, 450.5));
        h.mouse_button_release(None);
        assert_eq!(heights(&h), (451.0, 200.0));
        let again = logic();
        h.edit_root_widget(|root| again.rebuild(&view, &mut state, &mut ctx, root, &mut ()));
        assert_eq!(heights(&h), (451.0, 200.0));
        h.process_window_event(WindowEvent::Resize(PhysicalSize::new(800, 751)));
        assert_eq!(heights(&h), (551.0, 200.0));
        let split_id = h.root_widget().children()[0].id();
        h.focus_on(Some(split_id));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        assert!(heights(&h).1 < 200.0);
        h.mouse_move(Point::new(400.0, heights(&h).0 + 0.5));
        h.mouse_button_press(None);
        h.mouse_move(Point::new(400.0, 748.0));
        h.mouse_button_release(None);
        assert_eq!(
            heights(&h).1,
            crate::application::view::design::PROOF_MIN_HEIGHT
        );
    }

    #[test]
    fn inspector_sections_take_their_height_and_preview_takes_the_remainder() {
        use crate::application::widgets::scroll_viewport::portal;
        use masonry::layout::{Dim, Length};
        use masonry::properties::Dimensions;
        use xilem::core::View;
        use xilem::style::Style;
        use xilem::view::{label, sized_box};
        let logic = |sections_height| {
            inspector_stack(
                portal(sized_box(label("Sections")).dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::Fixed(Length::px(sections_height)),
                )))
                .constrain_horizontal(true),
                label("Preview"),
            )
        };
        let mut ctx = context();
        let view = logic(320.0);
        let (pod, mut state) = view.build(&mut ctx, &mut ());
        let mut h = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (246, 682),
        );
        fn heights<W: masonry::core::Widget>(h: &TestHarness<W>) -> (f64, f64) {
            let root = h.root_widget();
            let children = root.children();
            (
                children[0].ctx().border_box().height(),
                children[1].ctx().border_box().height(),
            )
        }
        assert_eq!(heights(&h), (320.0, 362.0));

        let again = logic(440.0);
        h.edit_root_widget(|root| again.rebuild(&view, &mut state, &mut ctx, root, &mut ()));
        assert_eq!(heights(&h), (440.0, 242.0));

        let expanded = logic(760.0);
        h.edit_root_widget(|root| expanded.rebuild(&again, &mut state, &mut ctx, root, &mut ()));
        assert_eq!(heights(&h), (760.0, 0.0));
    }

    #[test]
    fn splitter_hover_drag_and_focus_never_paint_visual_indicators() {
        use masonry::layout::{Dim, Length};
        use masonry::properties::Dimensions;
        use xilem::core::View;
        use xilem::style::Style;
        use xilem::view::{label, sized_box};
        let fill = masonry::peniko::Color::from_rgb8(177, 177, 177);
        let pane = || {
            sized_box(label(""))
                .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
                .background_color(fill)
        };
        fn check<V: xilem::WidgetView<()>>(
            view: V,
            size: (u32, u32),
            start: Point,
            end: Point,
            name: &str,
        ) {
            let mut ctx = context();
            let view = sized_box(view)
                .padding(Length::px(8.0))
                .background_color(masonry::peniko::Color::from_rgb8(177, 177, 177));
            let (pod, _) = view.build(&mut ctx, &mut ());
            let mut h = TestHarness::create_with_size(
                crate::application::view::default_property_set(),
                pod.new_widget,
                size,
            );
            let baseline = h.render();
            h.mouse_move(start);
            assert_eq!(h.render(), baseline, "{name} changed its paint on hover");
            let resize_cursor = if name == "dock" {
                masonry::core::CursorIcon::EwResize
            } else {
                masonry::core::CursorIcon::NsResize
            };
            h.mouse_button_press(None);
            h.mouse_move(end);
            assert_eq!(h.cursor_icon(), resize_cursor);
            for phase in ["dragging", "released", "pointer-away", "blurred"] {
                match phase {
                    "released" => h.mouse_button_release(None),
                    "pointer-away" => h.mouse_move(Point::new(40.0, 40.0)),
                    "blurred" => h.focus_on(None),
                    _ => {}
                }
                assert_eq!(
                    h.focused_widget_id().is_some(),
                    phase != "blurred",
                    "the paint fix must preserve native splitter focus"
                );
                let rendered = h.render();
                if let Ok(dir) = std::env::var("RUNEBENDER_RESIZE_EVIDENCE") {
                    let dir = std::path::PathBuf::from(dir);
                    std::fs::create_dir_all(&dir).unwrap();
                    rendered
                        .save(dir.join(format!("{name}-{phase}.png")))
                        .unwrap();
                }
                assert_eq!(rendered, baseline, "{name} changed its paint while {phase}");
            }
        }
        check(
            workspace_columns(pane(), pane(), pane(), false, xilem::Color::TRANSPARENT),
            (1296, 666),
            Point::new(1041.5, 108.0),
            Point::new(961.5, 108.0),
            "dock",
        );
        check(
            proof_split(pane(), pane(), fill),
            (816, 667),
            Point::new(408.0, 518.5),
            Point::new(408.0, 458.5),
            "proof",
        );
    }
}
