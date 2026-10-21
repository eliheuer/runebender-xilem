// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The launch path: the event loop, the window, and the first frame.

use crate::application::platform::screenshot;
use crate::application::view::render::root_logic;
use crate::application::view::{UI_FONT, default_property_set};
use crate::application::workspace::AppState;
use masonry::core::{ErasedAction, WidgetId};
use masonry_winit::app::{AppDriver, DriverCtx, WgpuContext, WindowId};
use std::cell::Cell;
use std::path::Path as FsPath;
use std::rc::Rc;
use std::sync::Arc;
use winit::dpi::LogicalSize;
use winit::error::EventLoopError;
use winit::window::Theme;
use xilem::view::sized_box;
use xilem::{EventLoopBuilder, Xilem};

pub(crate) fn run(
    event_loop: EventLoopBuilder,
    path: Option<&FsPath>,
) -> Result<(), EventLoopError> {
    crate::application::platform::config::load().apply();
    let mut app = AppState::open(path);
    // RUNEBENDER_GLYPH=<name> starts in the editor on that glyph, so
    // a headless screenshot reaches edit mode without clicks.
    if let Ok(name) = std::env::var("RUNEBENDER_GLYPH")
        && let Some(workspace) = app.workspace.as_mut()
        && let Some(index) = workspace.font.index_of(&name)
    {
        workspace.open_glyph(index);
        // RUNEBENDER_PROOF_SAMPLE=1 proposes a wider open glyph through the
        // live tools, so a headless render of the proof view has a proposal
        // to compare.
        if std::env::var("RUNEBENDER_PROOF_SAMPLE").as_deref() == Ok("1") {
            proof_sample(workspace);
        }
        // RUNEBENDER_COMPONENT=<n> opens the glyph's nth component in place.
        if let Some(id) = std::env::var("RUNEBENDER_COMPONENT")
            .ok()
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| {
                let layer = workspace.session.current_layer()?;
                layer.components().nth(n).map(|component| component.id())
            })
        {
            workspace.enter_component(id);
        }
        // RUNEBENDER_LABEL_SAMPLE=<n> opens the label tool on that sample.
        if let Some(sample) = std::env::var("RUNEBENDER_LABEL_SAMPLE")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
        {
            Arc::make_mut(&mut workspace.session).select_sample(Some(sample));
        }
    }
    if std::env::var("RUNEBENDER_SELECTALL").is_ok()
        && let Some(workspace) = app.workspace.as_mut()
    {
        let mut sess = (*workspace.session).clone();
        sess.select_all();
        workspace.selected_points = sess.selection_bounds().map(|_| 999).unwrap_or(0);
        workspace.selected_points = sess.point_count();
        workspace.session = Arc::new(sess);
        workspace.refresh_coord_bufs();
    }
    // Headless: render one frame and exit. No window, no event loop.
    if let Ok(path) = std::env::var("RUNEBENDER_SCREENSHOT") {
        if let Ok(name) = std::env::var("RUNEBENDER_SELECTED")
            && let Some(workspace) = app.workspace.as_mut()
            && let Some(index) = workspace.font.index_of(&name)
        {
            workspace.grid_select(index, false, false);
        }
        // RUNEBENDER_PREVIEW_TEXT=<text> sets the proof strip's text.
        if let Ok(text) = std::env::var("RUNEBENDER_PREVIEW_TEXT")
            && let Some(workspace) = app.workspace.as_mut()
        {
            workspace.preview_text = text;
        }
        // RUNEBENDER_PREVIEW=pieces|model shows that view of the proof strip; the model view
        // waits for the worker's first drawing.
        if let Ok(view) = std::env::var("RUNEBENDER_PREVIEW")
            && let Some(workspace) = app.workspace.as_mut()
        {
            use crate::application::pieces::PreviewView;
            workspace.preview_view = match view.as_str() {
                "pieces" => PreviewView::Pieces,
                "model" => PreviewView::Model,
                _ => PreviewView::Outline,
            };
            if workspace.preview_view == PreviewView::Model {
                workspace.refresh_model();
                for _ in 0..400 {
                    workspace.model_pump();
                    if workspace.model.render.is_some() || workspace.model.error.is_some() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        // RUNEBENDER_NEURAL_STATUS=<text> shows the Neural section mid-run, for review captures.
        if let Ok(status) = std::env::var("RUNEBENDER_NEURAL_STATUS")
            && let Some(workspace) = app.workspace.as_mut()
        {
            workspace.train.status = status;
        }
        if let Ok(section) = std::env::var("RUNEBENDER_EXPAND")
            && let Some(workspace) = app.workspace.as_mut()
        {
            workspace.collapsed.remove(section.as_str());
        }
        // In the Graph view, wait for the specimen's first render.
        // RUNEBENDER_GRAPH_EXPANDED=1 then shows its first image expanded.
        #[cfg(unix)]
        if let Some(workspace) = app.workspace.as_mut()
            && matches!(workspace.mode, crate::application::workspace::Mode::Graph)
            && workspace.live_graph.is_some()
        {
            crate::application::editor::tools::graph::controls::live_autorun_tick(workspace);
            for _ in 0..1200 {
                workspace.live_graph_pump();
                workspace.sync_live_graph_presentation();
                if workspace.graph.proof_images.len()
                    >= runebender::workflows::graph_live::SPECIMEN_PAGES
                {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            // Let the finished run settle so the nodes show it as current.
            for _ in 0..20 {
                workspace.live_graph_pump();
                workspace.sync_live_graph_presentation();
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            eprintln!(
                "DEBUG graph wait: images={} note={}",
                workspace.graph.proof_images.len(),
                workspace.note
            );
            if std::env::var_os("RUNEBENDER_GRAPH_EXPANDED").is_some() {
                workspace.graph.expanded = workspace.live_graph.as_ref().and_then(|state| {
                    let graph = state.session.snapshot().graph;
                    graph
                        .nodes
                        .iter()
                        .find(|node| node.type_name == "live.proof")
                        .map(|node| node.id)
                });
            }
        }

        // The harness needs a root widget with a concrete type, so wrap
        // the app's root view in a sized box.
        // RUNEBENDER_SIZE=1000x680 renders at a chosen logical size for
        // repeatable review captures.
        let size = std::env::var("RUNEBENDER_SIZE")
            .ok()
            .and_then(|spec| {
                let (w, h) = spec.split_once('x')?;
                Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
            })
            .unwrap_or((1100, 720));
        // Keep the logical layout fixed while exercising device-pixel scaling.
        // This produces a true 2x proof rather than a larger, reflowed window.
        let scale = std::env::var("RUNEBENDER_SCALE")
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(1.0);
        let background = app.background();
        screenshot::render_to(
            app,
            background,
            |app: &mut AppState| sized_box(root_logic(app)),
            size,
            scale,
            &path,
        );
        return Ok(());
    }
    // Xilem builds our first view (and installs the muda menu) before winit
    // starts its AppKit launch callback. Winit's default application-only menu
    // would then replace ours. Runebender owns the complete native menu bar.
    #[cfg(target_os = "macos")]
    let event_loop = {
        use winit::platform::macos::EventLoopBuilderExtMacOS as _;
        let mut event_loop = event_loop;
        event_loop.with_default_menu(false);
        event_loop
    };
    let initial_background = app.background();
    let initial_theme = app.palette.window_theme();
    let window_theme = Rc::new(Cell::new(initial_theme));
    let theme_for_view = window_theme.clone();
    #[cfg(target_os = "macos")]
    let initial_transparency = app.palette.blur_background;
    let window_id = WindowId::next();
    let application = Xilem::new(app, move |app| {
        theme_for_view.set(app.palette.window_theme());
        #[cfg(target_os = "macos")]
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.fullscreen = crate::application::platform::window::is_fullscreen();
        }
        #[cfg(target_os = "macos")]
        crate::application::platform::window::set_backdrop(app.palette.backdrop_style());
        let background = app.background();
        #[cfg(target_os = "macos")]
        let background = if crate::application::platform::window::backdrop_active() {
            xilem::Color::TRANSPARENT
        } else {
            background
        };
        let content = root_logic(app);
        // RUNEBENDER_FRAME_STATS=1 renders every frame and reports the cadence on stderr.
        let content = match std::env::var_os(crate::application::widgets::frame_stats::ENV) {
            Some(_) => xilem::core::one_of::OneOf2::A(
                crate::application::widgets::frame_stats::frame_stats(content),
            ),
            None => xilem::core::one_of::OneOf2::B(content),
        };
        #[cfg(target_os = "macos")]
        let content = xilem::view::resize_observer(
            |app: &mut AppState, _| {
                if let Some(workspace) = app.workspace.as_mut() {
                    workspace.fullscreen = crate::application::platform::window::is_fullscreen();
                }
                crate::application::platform::window::set_backdrop(app.palette.backdrop_style());
            },
            content,
        );
        #[cfg(target_os = "macos")]
        let content = crate::application::platform::window::with_backdrop(
            content,
            app.palette.blur_background,
        );
        // An open document cannot shrink the window past its panels' minimum widths.
        // The option is reactive, so collapsing a dock lowers the limit.
        let min_width = app.workspace.as_ref().map(|workspace| {
            crate::application::view::render::workspace_min_width(
                workspace.left_collapsed,
                workspace.right_collapsed,
            )
        });
        let view = xilem::window(window_id, "Runebender", content)
            // Stock widgets follow the active theme; the window swaps them on a theme change.
            .with_default_properties(app.palette.widget_properties())
            .with_options(|options| {
                let options = options
                    .with_initial_inner_size(LogicalSize::new(
                        crate::application::design::DEFAULT_WINDOW_WIDTH,
                        crate::application::design::DEFAULT_WINDOW_HEIGHT,
                    ))
                    .on_close(AppState::request_quit)
                    .on_file_drop(AppState::file_dropped);
                let options = match min_width {
                    Some(width) => options.with_min_inner_size(LogicalSize::new(width, 0.0)),
                    None => options,
                };
                // On macOS the header row is the title bar: content runs under
                // the transparent system bar and pads for traffic lights.
                #[cfg(target_os = "macos")]
                let options = {
                    use xilem::WindowOptionsExtMacOS as _;
                    options
                        // Preserve the opaque native window while blur is disabled.
                        // The backdrop setup makes the window clear itself when the
                        // View menu turns blur on later.
                        .with_transparent(initial_transparency)
                        .with_titlebar_transparent(true)
                        .with_fullsize_content_view(true)
                        .with_title_hidden(true)
                };
                options
            })
            .with_base_color(background);
        std::iter::once(view)
    })
    .with_font(xilem::Blob::new(Arc::new(UI_FONT)))
    .with_default_base_color(initial_background);
    // The pinned Xilem WindowOptions lacks a native appearance setter.
    // Its public Masonry driver path supports both initial and live winit themes.
    let event_loop = {
        let mut event_loop = event_loop;
        event_loop.build()?
    };
    let proxy = event_loop.create_proxy();
    let (driver, mut windows) = application
        .into_driver_and_windows(move |event| proxy.send_event(event).map_err(|error| error.0));
    for window in &mut windows {
        window.attributes = window.attributes.clone().with_theme(Some(initial_theme));
    }
    let driver = WindowAppearanceDriver {
        inner: driver,
        theme: window_theme,
        applied: initial_theme,
    };
    masonry_winit::app::run_with(event_loop, windows, driver, default_property_set())
}

/// Forward Xilem events while keeping the application's single native window themed.
struct WindowAppearanceDriver<D> {
    inner: D,
    theme: Rc<Cell<Theme>>,
    applied: Theme,
}

impl<D> WindowAppearanceDriver<D> {
    fn sync_theme(&mut self, window_id: WindowId, ctx: &mut DriverCtx<'_>) {
        let theme = self.theme.get();
        if theme != self.applied {
            ctx.window(window_id).handle().set_theme(Some(theme));
            self.applied = theme;
        }
    }
}

impl<D: AppDriver> AppDriver for WindowAppearanceDriver<D> {
    fn on_action(
        &mut self,
        window_id: WindowId,
        ctx: &mut DriverCtx<'_>,
        widget_id: WidgetId,
        action: ErasedAction,
    ) {
        self.inner.on_action(window_id, ctx, widget_id, action);
        self.sync_theme(window_id, ctx);
    }

    fn on_async_action(
        &mut self,
        window_id: WindowId,
        ctx: &mut DriverCtx<'_>,
        action: ErasedAction,
    ) {
        self.inner.on_async_action(window_id, ctx, action);
        self.sync_theme(window_id, ctx);
    }

    fn on_start(&mut self, state: &mut masonry_winit::app::MasonryState) {
        self.inner.on_start(state);
    }

    fn on_close_requested(&mut self, window_id: WindowId, ctx: &mut DriverCtx<'_>) {
        // Closing cannot change the palette and may remove the window.
        self.inner.on_close_requested(window_id, ctx);
    }

    fn on_wgpu_ready(&mut self, wgpu: &WgpuContext<'_>) {
        self.inner.on_wgpu_ready(wgpu);
    }

    fn on_file_dropped(
        &mut self,
        window_id: WindowId,
        path: std::path::PathBuf,
        ctx: &mut DriverCtx<'_>,
    ) {
        self.inner.on_file_dropped(window_id, path, ctx);
        self.sync_theme(window_id, ctx);
    }
}

/// Propose the open glyph 64 units wider through the live tools, as a
/// model would, so the proof view has something to compare.
fn proof_sample(app: &mut crate::application::workspace::Workspace) {
    use runebender::automation::agent::ToolCall;
    let Some(source) = app.font.project.source_id(app.font.active()) else {
        return;
    };
    let glyph = app.session.glyph_name.clone();
    let read = app.call_live(&ToolCall {
        name: "read_glyph".into(),
        arguments: serde_json::json!({"glyph": glyph, "source": source.0}),
    });
    let x_min = read["advance"].as_f64().unwrap_or(0.0) / 2.0;
    let proposed = app.call_live(&ToolCall {
        name: "propose_edits".into(),
        arguments: serde_json::json!({
            "source": source.0, "task": "wider-sample",
            "reason": "A sample proposal for the proof view",
            "edits": [{"glyph": glyph, "expected_revision": read["revision"],
                "operations": [
                    {"op": "transform_points", "x_min": x_min, "dx": 64},
                    {"op": "adjust_width", "delta": 64}
                ]}]
        }),
    });
    eprintln!("proof sample proposal: {proposed}");
    app.refresh_proposals();
}
