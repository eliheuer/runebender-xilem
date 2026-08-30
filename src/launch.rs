// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The launch path: the event loop, the window, and the first frame.

use crate::*;

pub(crate) fn run(event_loop: EventLoopBuilder) -> Result<(), EventLoopError> {
    let path = std::env::args()
        .nth(1)
        .expect("usage: runebender-xilem <Font.ufo|Font.designspace>");
    let mut app = Workspace::open(FsPath::new(&path)).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    if std::env::var("RUNEBENDER_SELECTALL").is_ok() {
        let mut sess = (*app.session).clone();
        sess.select_all();
        app.selected_points = sess.selection_bounds().map(|_| 999).unwrap_or(0);
        let n = {
            let mut c = 0;
            for co in &sess.glyph.contours {
                c += co.points.len();
            }
            c
        };
        app.selected_points = n;
        app.session = std::sync::Arc::new(sess);
        app.refresh_coord_bufs();
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
        screenshot::render_to(
            app,
            |app: &mut Workspace| sized_box(app_logic(app)),
            size,
            &path,
        );
        return Ok(());
    }
    let background = app.palette.app;
    let window_options =
        WindowOptions::new("Runebender").with_initial_inner_size(LogicalSize::new(1100., 720.));
    Xilem::new_simple(app, app_logic, window_options)
        .with_default_properties(default_property_set())
        .with_default_base_color(background)
        .run_in(event_loop)
}
