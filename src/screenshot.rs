// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Render one frame to a PNG with no window and no event loop.
//!
//! Every visual decision in this editor is made by rendering a PNG and
//! looking at it, which is also the only way an agent can see what it
//! built. Upstream Xilem has no headless path for an application, so the
//! application carries one.
//!
//! It is about seventy lines and it uses only public API: build the root
//! view against a `ViewCtx`, hand the resulting widget to a
//! `TestHarness`, rebuild once so views that fill their scene during
//! rebuild (the canvas) are drawn, then rasterize with Vello CPU.
//!
//! The equivalent in runebender-gpui does not exist: screenshots there
//! are taken from a real window.

use std::sync::Arc;

use masonry::theme::default_property_set;
use masonry_testing::{TestHarness, TestHarnessParams};
use xilem::core::{ProxyError, RawProxy, SendMessage, View, ViewId};
use masonry::dpi::PhysicalSize;
use xilem::{ViewCtx, WidgetView};

use crate::App;

/// A proxy that drops messages: nothing can arrive in one frame.
#[derive(Debug)]
struct NoProxy;

impl RawProxy for NoProxy {
    fn send_message(&self, _path: Arc<[ViewId]>, _message: SendMessage) -> Result<(), ProxyError> {
        Ok(())
    }

    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

/// Renders `logic(app)` once at `size` and writes it to `path`.
///
/// `logic` has to return a view with a concrete widget type, because the
/// harness needs a sized root. Wrapping the application's root view in a
/// `sized_box` is enough, and that is what the caller does.
pub fn render_to<V, F>(mut app: App, logic: F, size: (u32, u32), path: &str)
where
    V: WidgetView<App>,
    V::Widget: Sized,
    F: Fn(&mut App) -> V,
{
    let background = app.palette.app;
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("screenshot: no tokio runtime"),
    );
    let mut ctx = ViewCtx::new(Arc::new(NoProxy), runtime);

    let view = logic(&mut app);
    let (pod, mut view_state) = view.build(&mut ctx, &mut app);

    let mut params = TestHarnessParams::default().with_size(PhysicalSize::new(size.0, size.1));
    params.background_color = background;
    let mut harness = TestHarness::create_with(default_property_set(), pod.new_widget, params);

    // One rebuild, so a view that fills its scene there is drawn. The
    // canvas view is the reason: it records nothing until rebuild.
    // `Mut<Pod<W>>` is a `WidgetMut<W>`, which the harness will hand out.
    let again = logic(&mut app);
    harness.edit_root_widget(|root| {
        again.rebuild(&view, &mut view_state, &mut ctx, root, &mut app);
    });

    let image = harness.render();
    image
        .save(path)
        .unwrap_or_else(|e| panic!("screenshot: could not write {path}: {e}"));
    eprintln!("wrote {path}");
}
