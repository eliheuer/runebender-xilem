// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Render one frame to a PNG with no window and no event loop.
//!
//! Every visual decision in this editor is made by looking at a PNG, and
//! an agent cannot see its own work any other way. Xilem has no headless
//! path for an application, so the application carries one.
//!
//! This used to be built on `masonry_testing::TestHarness`, which is less
//! code and was wrong in a way that took a while to notice. The harness
//! builds its `RenderRoot` with `use_system_fonts: false` and a fixed
//! test font, because a snapshot test wants to be deterministic. The real
//! window sets it to `true`. So the screenshots rendered different text
//! than the running application, silently: the sidebar's Arabic and
//! Hebrew script icons came out as nothing at all, which reads exactly
//! like a font-fallback bug in the framework rather than a setting in the
//! tool being used to look for bugs.
//!
//! So this drives a `RenderRoot` directly, with the options the winit
//! runner uses. It is a little more code, and it renders what the
//! application renders.

use std::sync::Arc;
use std::time::Duration;
use std::{cell::RefCell, rc::Rc};

use crate::application::view::default_property_set;
use masonry::app::{
    RenderRoot, RenderRootOptions, RenderRootSignal, VisualLayerKind, WindowSizePolicy,
};
use masonry::core::WindowEvent;
use masonry::dpi::PhysicalSize;
use masonry::imaging::Painter;
use masonry::imaging::record::{Scene, replay_transformed};
use masonry::imaging::render::ImageRenderer as _;
use masonry::kurbo::{Affine, Rect};
use xilem::core::{ProxyError, RawProxy, SendMessage, ViewId};
use xilem::{ViewCtx, WidgetView};

use xilem::Color;

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

/// Apply layer lifecycle signals synchronously, as the window runner and test
/// harness do. Other signals are irrelevant to a one-frame image.
fn process_layer_signals(root: &mut RenderRoot, signals: &Rc<RefCell<Vec<RenderRootSignal>>>) {
    loop {
        let pending = std::mem::take(&mut *signals.borrow_mut());
        if pending.is_empty() {
            return;
        }
        for signal in pending {
            match signal {
                RenderRootSignal::NewLayer(_, widget, position) => {
                    root.add_layer(widget, position);
                }
                RenderRootSignal::RemoveLayer(id) => root.remove_layer(id),
                RenderRootSignal::RepositionLayer(id, position) => {
                    root.reposition_layer(id, position);
                }
                _ => {}
            }
        }
    }
}

/// Renders `logic(app)` once at `size`, writes it to `path`, and returns the state.
///
/// `logic` has to return a view with a concrete widget type, because the
/// rebuild below downcasts the root back to it. Wrapping the
/// application's root view in a `sized_box` is enough, and that is what
/// the caller does.
pub(crate) fn render_to<State, V, F>(
    mut app: State,
    background: Color,
    logic: F,
    size: (u32, u32),
    scale: f64,
    path: &str,
) -> State
where
    State: 'static,
    V: WidgetView<State>,
    V::Widget: Sized,
    F: Fn(&mut State) -> V,
{
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("screenshot: no tokio runtime"),
    );
    let mut ctx = ViewCtx::new(Arc::new(NoProxy), runtime);

    let view = logic(&mut app);
    let (pod, mut view_state) = view.build(&mut ctx, &mut app);

    let signals = Rc::new(RefCell::new(Vec::new()));
    let signal_sink = signals.clone();
    let physical = |logical: u32| {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "rounded and clamped screenshot pixel extent"
        )]
        {
            (f64::from(logical) * scale)
                .round()
                .clamp(1.0, f64::from(u32::MAX)) as u32
        }
    };
    let physical_size = PhysicalSize::new(physical(size.0), physical(size.1));
    let mut root = RenderRoot::new(
        pod.new_widget.erased(),
        move |signal| signal_sink.borrow_mut().push(signal),
        RenderRootOptions {
            default_properties: Arc::new(default_property_set()),
            // The setting this file exists for.
            use_system_fonts: true,
            size_policy: WindowSizePolicy::User,
            size: physical_size,
            scale_factor: scale,
            test_font: None,
        },
    );
    process_layer_signals(&mut root, &signals);

    root.register_fonts(xilem::Blob::new(Arc::new(
        crate::application::view::UI_FONT,
    )));

    // One rebuild, so a view that fills its scene there is drawn. The
    // canvas view is the reason: it records nothing until rebuild.
    let again = logic(&mut app);
    root.edit_base_layer(|mut root_widget| {
        let root_widget = root_widget.downcast::<V::Widget>();
        again.rebuild(&view, &mut view_state, &mut ctx, root_widget, &mut app);
    });
    process_layer_signals(&mut root, &signals);

    // A real window has already received its first idle animation frame by
    // the time it is useful to inspect it. Drive that frame here too, so
    // auto-hiding portal scrollbars do not get frozen visible in every
    // screenshot solely because this renderer exits after its first paint.
    root.handle_window_event(WindowEvent::AnimFrame(Duration::from_millis(500)));
    process_layer_signals(&mut root, &signals);

    let (layers, _tree) = root.redraw();
    let mut scene = Scene::new();
    {
        let mut painter = Painter::new(&mut scene);
        painter.fill_rect(
            Rect::new(
                0.0,
                0.0,
                f64::from(physical_size.width),
                f64::from(physical_size.height),
            ),
            background,
        );
        for layer in &layers.layers {
            if let VisualLayerKind::Scene(layer_scene) = &layer.kind {
                replay_transformed(
                    layer_scene,
                    &mut scene,
                    Affine::scale(scale) * layer.transform,
                );
            }
        }
    }

    let mut renderer = imaging_vello_cpu::VelloCpuRenderer::new(1, 1);
    let rendered = renderer
        .render_source(&mut scene, physical_size.width, physical_size.height)
        .expect("screenshot: render failed");
    let image = image::RgbaImage::from_vec(rendered.width, rendered.height, rendered.data)
        .expect("screenshot: bad image buffer");
    image
        .save(path)
        .unwrap_or_else(|e| panic!("screenshot: could not write {path}: {e}"));
    eprintln!("wrote {path}");
    app
}
