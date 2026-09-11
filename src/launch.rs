// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The launch path: the event loop, the window, and the first frame.

use crate::*;

pub(crate) fn run(event_loop: EventLoopBuilder) -> Result<(), EventLoopError> {
    let path = std::env::args().nth(1);
    let mut app = AppState::open(path.as_deref().map(FsPath::new));
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
        // The harness needs a root widget with a concrete type, so wrap
        // the app's root view in a sized box.
        // RUNEBENDER_SIZE=1000x680 renders at a chosen size, so a shot
        // can be matched against the GPUI build's window for comparison.
        let size = std::env::var("RUNEBENDER_SIZE")
            .ok()
            .and_then(|spec| {
                let (w, h) = spec.split_once('x')?;
                Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
            })
            .unwrap_or((1100, 720));
        let background = app.background();
        screenshot::render_to(
            app,
            background,
            |app: &mut AppState| sized_box(root_logic(app)),
            size,
            &path,
        );
        return Ok(());
    }
    let background = app.background();
    let window_options =
        WindowOptions::new("Runebender").with_initial_inner_size(LogicalSize::new(1100., 720.));
    // On macOS the header row is the title bar, as in the GPUI build:
    // the system bar goes transparent, the content runs up under the
    // traffic lights, and the header pads to clear them. Dragging is
    // the header's own drag region.
    #[cfg(target_os = "macos")]
    let window_options = {
        use xilem::WindowOptionsExtMacOS as _;
        window_options
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true)
            .with_title_hidden(true)
    };
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
    Xilem::new_simple(app, root_logic, window_options)
        .with_font(xilem::Blob::new(Arc::new(UI_FONT)))
        .with_default_properties(default_property_set())
        .with_default_base_color(background)
        .run_in(event_loop)
}
