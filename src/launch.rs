// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The launch path: the event loop, the window, and the first frame.

use crate::*;

pub(crate) fn run(
    event_loop: EventLoopBuilder,
    path: Option<&FsPath>,
) -> Result<(), EventLoopError> {
    let mut app = AppState::open(path);
    // RUNEBENDER_GLYPH=<name> starts in the editor on that glyph, so
    // a headless screenshot reaches edit mode without clicks.
    if let Ok(name) = std::env::var("RUNEBENDER_GLYPH")
        && let Some(workspace) = app.workspace.as_mut()
        && let Some(index) = workspace.font.index_of(&name)
    {
        workspace.open_glyph(index);
    }
    if std::env::var("RUNEBENDER_SELECTALL").is_ok()
        && let Some(workspace) = app.workspace.as_mut()
    {
        let mut sess = (*workspace.session).clone();
        sess.select_all();
        workspace.selected_points = sess.selection_bounds().map(|_| 999).unwrap_or(0);
        let n = {
            let mut c = 0;
            for co in &sess.glyph.contours {
                c += co.points.len();
            }
            c
        };
        workspace.selected_points = n;
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
        if let Ok(section) = std::env::var("RUNEBENDER_EXPAND")
            && let Some(workspace) = app.workspace.as_mut()
        {
            workspace.collapsed.remove(section.as_str());
        }

        // The harness needs a root widget with a concrete type, so wrap
        // the app's root view in a sized box.
        // RUNEBENDER_SIZE=1000x680 renders at a chosen logical size, so a shot
        // can be matched against the GPUI build's window for comparison.
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
    let window_id = xilem::WindowId::next();
    Xilem::new(app, move |app| {
        let background = app.background();
        let view = xilem::window(window_id, "Runebender", root_logic(app))
            .with_options(|options| {
                let options = options
                    .with_initial_inner_size(LogicalSize::new(1100., 720.))
                    .on_close(AppState::request_quit);
                // On macOS the header row is the title bar, as in the GPUI build:
                // content runs under the transparent system bar and pads for traffic lights.
                #[cfg(target_os = "macos")]
                let options = {
                    use xilem::WindowOptionsExtMacOS as _;
                    options
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
    .with_default_properties(default_property_set())
    .with_default_base_color(initial_background)
    .run_in(event_loop)
}
