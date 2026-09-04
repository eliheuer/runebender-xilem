// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The render tree: how the workspace's state becomes a frame.

use crate::*;

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

pub(crate) fn app_logic(app: &mut Workspace) -> impl WidgetView<Workspace> + use<> {
    use xilem::core::one_of::{Either, OneOf3};
    let pal = &app.palette;

    // Left column: category sidebar in overview only. In the editor the
    // tools live in the header (gpui-style), so the left column collapses.
    let _editing_mode = matches!(app.mode, Mode::Editor(_));
    let _ = &app.multi_selected;

    // The window, in the GPUI build's shape: one title bar across the
    // whole width, then the three columns under it, then a bottom bar
    // that runs under the sidebar and the middle but not under the
    // inspector, which is full height.
    let body = match app.mode {
        Mode::Overview => OneOf3::A(overview(app)),
        Mode::Editor(_) => OneOf3::B(editor_pane(app)),
        Mode::Nodes => OneOf3::C(nodes_pane(app)),
    };
    let preview = matches!(app.mode, Mode::Editor(_)).then(|| {
        sized_box(preview_strip(app))
            .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(120.0))))
            .background_color(pal.panel)
    });
    // The bottom bar belongs to the middle column, so the sidebar
    // keeps the window's full height and its own marks bar, as in the
    // GPUI build.
    let middle = flex_col((body.flex(1.0), preview, status(app)))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Space::None)
        .background_color(pal.app);

    let left = match app.mode {
        Mode::Overview | Mode::Nodes => Either::A(sidebar(app)),
        Mode::Editor(_) => Either::B(editor_nav(app)),
    };
    // The marks bar sits under the sidebar in both modes, as the GPUI
    // build has it, not in the middle column's bar.
    let left = flex_col((left.flex(1.0), marks_bar(app)))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Space::None);
    let left_width = if app.left_collapsed { 0.0 } else { 220.0 };

    let columns = flex_row((
        sized_box(left)
            .dims(Dimensions::new(
                Dim::Fixed(Length::px(left_width)),
                Dim::Stretch,
            ))
            .background_color(pal.panel),
        sized_box(middle)
            .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
            .background_color(pal.app)
            .flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Space::None);

    let left_and_middle = columns;

    // The menu bar is built on the main thread, which is here, and only
    // once. Xilem owns the event loop and offers no startup hook.
    actions::install();
    // Boxed on purpose, and not for tidiness. Every wrapper here adds a
    // layer to a monomorphized view type that is already enormous, and
    // with the watcher wrapped around the menu pump around the shortcut
    // host, the mangled symbol name grew past what the macOS linker
    // accepts: "ld: Assertion failed: (name.size() <= maxLength)". Not a
    // compile error, a link error, after a clean build of everything.
    // Erasing the type here cuts the chain.
    let root = shortcuts::shortcut_host(
        flex_col((
            titlebar(app),
            flex_row((
                sized_box(left_and_middle)
                    .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
                    .flex(1.0),
                sized_box(portal(info_panel(app)).constrain_horizontal(true))
                    .dims(Dimensions::new(Dim::Fixed(Length::px(256.0)), Dim::Stretch))
                    .background_color(pal.panel),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Space::None)
            .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Space::None)
        .background_color(pal.app),
    )
    .boxed();
    watch::with_watch(
        ai_pump(
            nodes_pump(actions::with_menu_events(root), app.nodes.job.clone()),
            app.ai.job.clone(),
        ),
        app.font.master_paths().clone(),
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

#[cfg(test)]
mod tab_tests {
    use super::*;

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
