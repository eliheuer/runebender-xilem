// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Runebender Xilem: A font editor built with Xilem

use winit::dpi::LogicalSize;
use winit::error::EventLoopError;
use xilem::core::fork;
use xilem::core::one_of::Either;
use xilem::view::indexed_stack;
use xilem::{EventLoopBuilder, WidgetView, WindowView, Xilem, window};

mod components;
mod data;
mod editing;
mod file_watcher;
mod model;
mod path;
mod settings;
mod shaping;
mod sort;
mod theme;
mod tools;
mod views;

use data::AppState;
use views::{editor_tab, glyph_grid_tab, welcome};

/// Entry point for the Runebender Xilem application
pub fn run(event_loop: EventLoopBuilder) -> Result<(), EventLoopError> {
    // Initialize tracing subscriber (can be controlled via RUST_LOG env var)
    // Filter out noisy wgpu/naga shader compilation logs
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("runebender=info".parse().unwrap())
                .add_directive("wgpu=warn".parse().unwrap())
                .add_directive("naga=warn".parse().unwrap())
                .add_directive("wgpu_core=warn".parse().unwrap())
                .add_directive("wgpu_hal=warn".parse().unwrap()),
        )
        .init();

    let mut initial_state = AppState::new();

    // Check for command-line argument (UFO path)
    handle_command_line_args(&mut initial_state);

    let app = Xilem::new(initial_state, app_logic);
    app.run_in(event_loop)?;
    Ok(())
}

/// Handle command-line arguments to load a UFO or designspace file
fn handle_command_line_args(initial_state: &mut AppState) {
    let args: Vec<String> = std::env::args().collect();
    if args.len() <= 1 {
        return;
    }

    let font_path = std::path::PathBuf::from(&args[1]);

    // Validate that the path exists
    if font_path.exists() {
        tracing::info!("Loading font from: {}", font_path.display());
        initial_state.load_font(font_path);
    } else {
        tracing::error!("Path does not exist: {}", font_path.display());
        tracing::error!("Usage: runebender [path/to/font.ufo|designspace]");
    }
}

/// Build the single-window UI (glyph grid tab + editor tab).
fn app_logic(state: &mut AppState) -> impl Iterator<Item = WindowView<AppState>> + use<> {
    let content = if state.has_font_loaded() {
        Either::A(tabbed_view_with_watcher(state))
    } else {
        Either::B(welcome(state))
    };

    let window_size = LogicalSize::new(1280.0, 800.0);
    let window_view = window(state.main_window_id, "Runebender Xilem", content);
    let window_with_options = window_view.with_options(|options| {
        options
            .with_initial_inner_size(window_size)
            .on_close(|state: &mut AppState| state.running = false)
    });

    std::iter::once(window_with_options)
}

/// Tabbed interface with file watcher for auto-reloading external changes.
///
/// Wraps `tabbed_view` with a `fork` + `task_raw` that watches the UFO
/// directory for filesystem events. When external changes are detected
/// (after a 1-second debounce), the workspace is reloaded from disk.
fn tabbed_view_with_watcher(
    state: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
    let ufo_paths = state.watched_ufo_paths();
    let save_flag = state.save_in_progress.clone();
    let tabbed = tabbed_view(state);

    fork(
        tabbed,
        xilem::view::task_raw(
            move |proxy| {
                let paths = ufo_paths.clone();
                let flag = save_flag.clone();
                async move {
                    if !paths.is_empty() {
                        file_watcher::watch_ufo(proxy, paths, flag)
                            .await;
                    }
                }
            },
            |state: &mut AppState, _msg: file_watcher::FileChanged| {
                state.reload_workspace_from_disk();
            },
        ),
    )
}

/// Tabbed interface with glyph grid view and editor view tabs
fn tabbed_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let tabs = indexed_stack((glyph_grid_tab(state), editor_tab(state)));
    tabs.active(state.active_tab as usize)
}
