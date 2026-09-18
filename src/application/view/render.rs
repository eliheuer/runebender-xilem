// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The render tree: how the workspace's state becomes a frame.

use crate::application::actions;
use crate::application::editor::tools::{chat, local_ai, nodes};
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
use crate::application::widgets::menu_shell;
use crate::application::widgets::scroll_viewport::portal;
#[cfg(target_os = "macos")]
use crate::application::widgets::shortcuts;
use crate::application::workspace::{AppState, Mode, Workspace};
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
    Left,
    Right,
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
        KeylineEdge::Left => (
            Dimensions::new(Dim::Fixed(Stroke::Hairline.length()), Dim::Stretch),
            UnitPoint::LEFT,
        ),
        KeylineEdge::Right => (
            Dimensions::new(Dim::Fixed(Stroke::Hairline.length()), Dim::Stretch),
            UnitPoint::RIGHT,
        ),
    };
    xilem::view::zstack((
        content,
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

/// Native splitters retain dragged sizes across view rebuilds and window resizes.
fn workspace_columns<State, A, B, C>(
    left: A,
    middle: B,
    right: C,
    collapsed: bool,
    outline: Color,
) -> impl WidgetView<State, Widget: Sized> + use<State, A, B, C>
where
    State: 'static,
    A: WidgetView<State>,
    B: WidgetView<State>,
    C: WidgetView<State>,
{
    use crate::application::view::design::{CENTER_MIN_WIDTH, DOCK_MIN_WIDTH, SPLITTER_HIT_WIDTH};
    let left = edge_keyline(left, KeylineEdge::Right, outline);
    let columns = xilem::view::split(left, middle)
        .split_point_from_start(Length::px(if collapsed { 0.0 } else { DOCK_WIDTH }))
        .min_lengths(
            Length::px(if collapsed { 0.0 } else { DOCK_MIN_WIDTH }),
            Length::px(CENTER_MIN_WIDTH),
        )
        // Split keeps the generous hit target and all native resize behavior;
        // the visible rule is our palette keyline above, not its hard-coded
        // bluish-gray bar.
        .bar_thickness(Length::ZERO)
        .min_bar_area(Length::px(SPLITTER_HIT_WIDTH))
        .solid_bar(false)
        .draggable(!collapsed);
    let right = edge_keyline(right, KeylineEdge::Left, outline);
    let columns = xilem::view::split(columns, right)
        .split_point_from_end(Length::px(DOCK_WIDTH))
        .min_lengths(
            Length::px(CENTER_MIN_WIDTH + if collapsed { 0.0 } else { DOCK_MIN_WIDTH } + 1.0),
            Length::px(DOCK_MIN_WIDTH),
        )
        .bar_thickness(Length::ZERO)
        .min_bar_area(Length::px(SPLITTER_HIT_WIDTH))
        .solid_bar(false);
    clip_split(columns)
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
    let split = xilem::view::split(editor, top_keyline(proof, outline))
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
/// compress it instead of continuing underneath it.
fn overview_inspector_stack<State, A, B>(
    sections: A,
    preview: B,
) -> impl WidgetView<State, Widget: Sized> + use<State, A, B>
where
    State: 'static,
    A: WidgetView<State>,
    B: WidgetView<State>,
{
    flex_col((sections, preview.flex(1.0)))
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

    // One title bar spans the whole width, followed by three columns and a bottom bar
    // that runs under the sidebar and the middle but not under the
    // inspector, which is full height.
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
        portal(sized_box(info_panel(app)).dims(Dimensions::new(Dim::Stretch, Dim::Auto)))
            .constrain_horizontal(true)
            .background_color(pal.panel)
    };
    // Font and Nodes both use the document inspector: compact sections above
    // the selected glyph's outline preview. Nodes is a workflow over the same
    // font, so changing modes must not replace that useful document context.
    let inspector = if matches!(app.mode, Mode::Overview | Mode::Nodes) {
        Either::A(overview_inspector_stack(
            inspector_sections(),
            glyph_preview(app),
        ))
    } else {
        Either::B(inspector_sections())
    }
    // Erase the inspector split before adding the two horizontal dock splits;
    // the section tree is already near rustc's recursive trait limit.
    .boxed();
    let columns = workspace_columns(
        left.background_color(pal.panel),
        middle,
        inspector,
        app.left_collapsed,
        pal.outline,
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
        // One shared top rule keeps all three columns on the same boundary.
        // The navigation rail must not paint another top edge of its own.
        sized_box(label(""))
            .dims(Dimensions::new(
                Dim::Stretch,
                Dim::Fixed(Stroke::Hairline.length()),
            ))
            .background_color(pal.outline),
        columns.flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Space::None)
    .background_color(pal.app);
    // Erase the chrome before the async pumps add their own generic layers;
    // otherwise the macOS linker receives multi-megabyte symbol names.
    let content = content.boxed();
    #[cfg(unix)]
    let content = live::with_live(content);
    #[cfg(target_arch = "wasm32")]
    return content;
    #[cfg(not(target_arch = "wasm32"))]
    watch::with_watch(
        ai_pump(
            chat_pump(
                export_pump(
                    nodes_pump(content, app.nodes.job.clone()),
                    app.export_job.clone(),
                ),
                app.chat.job.clone(),
            ),
            app.ai.job.clone(),
        ),
        app.font.master_paths().clone(),
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
                .color(palette.text),
            label(detail).color(palette.text_muted),
        ))
        .main_axis_alignment(MainAxisAlignment::Center)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Space::Md),
    )
    .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
    .background_color(palette.app)
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

/// Wake the application once a background font export has finished.
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
    fn app() -> Workspace {
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
    use super::{overview_inspector_stack, proof_split, workspace_columns};
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
    fn both_docks_drag_and_retain_widths_after_rebuild_and_window_resize() {
        use xilem::core::View;
        use xilem::view::label;
        let outline = masonry::peniko::Color::from_rgb8(29, 29, 29);
        let logic = || {
            workspace_columns(
                label("Left"),
                label("Canvas"),
                label("Right"),
                false,
                outline,
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
                nested[0].ctx().border_box().width(),
                nested[1].ctx().border_box().width(),
                children[1].ctx().border_box().width(),
            )
        }
        assert_eq!(widths(&h), (246.0, 788.0, 246.0));
        // Start two pixels beside the visible line, within its wider hit target.
        h.mouse_move(Point::new(244.5, 100.0));
        h.mouse_button_press(None);
        h.mouse_move(Point::new(324.5, 100.0));
        h.mouse_button_release(None);
        assert_eq!(widths(&h), (326.0, 708.0, 246.0));
        h.mouse_move(Point::new(1033.5, 100.0));
        h.mouse_button_press(None);
        h.mouse_move(Point::new(953.5, 100.0));
        h.mouse_button_release(None);
        assert_eq!(widths(&h), (326.0, 628.0, 326.0));
        let again = logic();
        h.edit_root_widget(|root| again.rebuild(&view, &mut state, &mut ctx, root, &mut ()));
        assert_eq!(widths(&h), (326.0, 628.0, 326.0));
        h.process_window_event(WindowEvent::Resize(PhysicalSize::new(1100, 650)));
        assert_eq!(widths(&h), (326.0, 448.0, 326.0));
        h.mouse_move(Point::new(326.5, 100.0));
        h.mouse_button_press(None);
        h.mouse_move(Point::new(20.0, 100.0));
        h.mouse_button_release(None);
        assert_eq!(
            widths(&h).0,
            crate::application::view::design::DOCK_MIN_WIDTH
        );
    }

    #[test]
    fn collapsed_left_panel_reopens_without_disabling_the_inspector_splitter() {
        use xilem::core::View;
        use xilem::view::label;
        let outline = masonry::peniko::Color::from_rgb8(29, 29, 29);
        let logic = |collapsed| {
            workspace_columns(
                label("Left"),
                label("Canvas"),
                label("Right"),
                collapsed,
                outline,
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
            326.0
        );
        let again = logic(false);
        h.edit_root_widget(|root| again.rebuild(&view, &mut state, &mut ctx, root, &mut ()));
        assert_eq!(
            h.root_widget().children()[0].children()[0].children()[0]
                .ctx()
                .border_box()
                .width(),
            246.0
        );
        assert_eq!(
            h.root_widget().children()[0].children()[1]
                .ctx()
                .border_box()
                .width(),
            326.0
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
    fn overview_sections_take_their_height_and_preview_takes_the_remainder() {
        use masonry::layout::{Dim, Length};
        use masonry::properties::Dimensions;
        use xilem::core::View;
        use xilem::style::Style;
        use xilem::view::{label, sized_box};
        let logic = |sections_height| {
            overview_inspector_stack(
                sized_box(label("Sections")).dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::Fixed(Length::px(sections_height)),
                )),
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
    }

    #[test]
    fn splitter_focus_never_paints_end_caps_outside_the_panels() {
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
            h.mouse_button_press(None);
            h.mouse_move(end);
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
                assert!(
                    rendered
                        .enumerate_pixels()
                        .filter(|(x, y, _)| *x < 8
                            || *x >= size.0 - 8
                            || *y < 8
                            || *y >= size.1 - 8)
                        .all(|(x, y, pixel)| pixel == baseline.get_pixel(x, y)),
                    "{name} painted outside its panels while {phase}"
                );
            }
        }
        check(
            workspace_columns(pane(), pane(), pane(), false, fill),
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
