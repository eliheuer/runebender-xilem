// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Runebender Xilem: A font editor built with Xilem

use winit::dpi::LogicalSize;
use winit::error::EventLoopError;
use xilem::core::one_of::Either;
use xilem::view::indexed_stack;
use xilem::{EventLoopBuilder, WidgetView, WindowView, Xilem, window};

mod components;
mod cubic_path;
mod data;
mod hyper_path;
mod quadratic_path;
mod edit_session;
mod edit_types;
mod entity_id;
mod glyph_renderer;
mod hit_test;
mod kerning;
mod mouse;
mod path;
mod point;
mod point_list;
mod quadrant;
mod path_segment;
mod selection;
mod settings;
mod sort;
mod theme;
mod tools;
mod undo;
mod viewport;
mod views;
mod workspace;

use data::AppState;
use views::{editor_tab, glyph_grid_tab, welcome};

/// Entry point for the Runebender Xilem application
pub fn run(event_loop: EventLoopBuilder) -> Result<(), EventLoopError> {
    // Initialize tracing subscriber (can be controlled via RUST_LOG env var)
    // Filter out noisy wgpu/naga shader compilation logs
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("wgpu=warn".parse().unwrap())
                .add_directive("naga=warn".parse().unwrap())
                .add_directive("wgpu_core=warn".parse().unwrap())
                .add_directive("wgpu_hal=warn".parse().unwrap())
        )
        .init();

    let mut initial_state = AppState::new();

    // Check for command-line argument (UFO path)
    handle_command_line_args(&mut initial_state);

    let app = Xilem::new(initial_state, app_logic);
    app.run_in(event_loop)?;
    Ok(())
}

/// Handle command-line arguments to load a UFO file
fn handle_command_line_args(initial_state: &mut AppState) {
    let args: Vec<String> = std::env::args().collect();
    if args.len() <= 1 {
        return;
    }

    let ufo_path = std::path::PathBuf::from(&args[1]);

    // Validate that the path exists
    if ufo_path.exists() {
        tracing::info!("Loading UFO from: {}", ufo_path.display());
        initial_state.load_ufo(ufo_path);
    } else {
        tracing::error!("Path does not exist: {}", ufo_path.display());
        tracing::error!("Usage: runebender [path/to/font.ufo]");
    }
}

/// Build the single-window UI (glyph grid tab + editor tab).
fn app_logic(
    state: &mut AppState,
) -> impl Iterator<Item = WindowView<AppState>> + use<> {
    let content = match state.workspace {
        Some(_) => Either::A(tabbed_view(state)),
        None => Either::B(welcome(state)),
    };

    let window_size = LogicalSize::new(1030.0, 800.0);
    let window_view = window(
        state.main_window_id,
        "Runebender Xilem",
        content,
    );
    let window_with_options = window_view.with_options(|options| {
        options
            .with_initial_inner_size(window_size)
            .on_close(|state: &mut AppState| state.running = false)
    });

    std::iter::once(window_with_options)
}

/// Tabbed interface with glyph grid view and editor view tabs
fn tabbed_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let tabs = indexed_stack((glyph_grid_tab(state), editor_tab(state)));
    tabs.active(state.active_tab as usize)
}
