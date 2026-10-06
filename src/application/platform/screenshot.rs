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

/// A headless application: the state, its view tree, and the widget tree they produce,
/// driven by hand instead of by a window.
///
/// `render_to` uses it for one frame; the frame benchmarks drive it through a whole
/// gesture so the work of each step can be timed on its own.
pub(crate) struct Headless<State: 'static, V: WidgetView<State>> {
    /// The application state the view reads.
    pub(crate) app: State,
    ctx: ViewCtx,
    view: V,
    view_state: V::ViewState,
    root: RenderRoot,
    signals: Rc<RefCell<Vec<RenderRootSignal>>>,
    scale: f64,
    physical_size: PhysicalSize<u32>,
}

impl<State: 'static, V> Headless<State, V>
where
    V: WidgetView<State>,
    V::Widget: Sized,
{
    /// Build `logic(app)` at `size` logical pixels and `scale` device pixels per logical one.
    ///
    /// `logic` has to return a view with a concrete widget type, because every rebuild
    /// downcasts the root back to it. Wrapping the application's root view in a `sized_box`
    /// is enough.
    pub(crate) fn new(
        mut app: State,
        logic: impl Fn(&mut State) -> V,
        size: (u32, u32),
        scale: f64,
    ) -> Self {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .build()
                .expect("screenshot: no tokio runtime"),
        );
        let mut ctx = ViewCtx::new(Arc::new(NoProxy), runtime);

        let view = logic(&mut app);
        let (pod, view_state) = view.build(&mut ctx, &mut app);

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
        Self {
            app,
            ctx,
            view,
            view_state,
            root,
            signals,
            scale,
            physical_size,
        }
    }

    /// Run the app logic again and rebuild the widget tree from the new view.
    pub(crate) fn rebuild(&mut self, logic: impl Fn(&mut State) -> V) {
        let again = logic(&mut self.app);
        let Self {
            ctx,
            view,
            view_state,
            root,
            app,
            ..
        } = self;
        root.edit_base_layer(|mut root_widget| {
            let root_widget = root_widget.downcast::<V::Widget>();
            again.rebuild(view, view_state, ctx, root_widget, app);
        });
        *view = again;
        process_layer_signals(root, &self.signals);
    }

    /// Deliver an animation frame `elapsed` after the previous one.
    pub(crate) fn anim_frame(&mut self, elapsed: Duration) {
        self.root
            .handle_window_event(WindowEvent::AnimFrame(elapsed));
        process_layer_signals(&mut self.root, &self.signals);
    }

    #[cfg(test)]
    /// Deliver a pointer event, in physical pixels, and run the passes it asks for.
    pub(crate) fn pointer(&mut self, event: masonry::core::PointerEvent) {
        self.root.handle_pointer_event(event);
        process_layer_signals(&mut self.root, &self.signals);
    }

    #[cfg(test)]
    /// Run the paint pass, recording the scene of every layer without rasterizing it.
    pub(crate) fn redraw(&mut self) -> masonry::app::VisualLayerPlan {
        self.root.redraw().0
    }

    #[cfg(test)]
    /// The first widget of type `W` in the base layer, depth first.
    pub(crate) fn find_widget<W: masonry::core::Widget + masonry::core::FromDynWidget + ?Sized>(
        &self,
    ) -> Option<masonry::core::WidgetRef<'_, W>> {
        fn walk<'w, W: masonry::core::Widget + masonry::core::FromDynWidget + ?Sized>(
            widget: masonry::core::WidgetRef<'w, dyn masonry::core::Widget>,
        ) -> Option<masonry::core::WidgetRef<'w, W>> {
            if let Some(found) = widget.downcast::<W>() {
                return Some(found);
            }
            widget.children().into_iter().find_map(walk::<W>)
        }
        walk::<W>(self.root.get_layer_root(0))
    }

    /// Rasterize the current frame over `background` and write it to `path`.
    pub(crate) fn write_png(&mut self, background: Color, path: &str) {
        let (layers, _tree) = self.root.redraw();
        let physical_size = self.physical_size;
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
                        Affine::scale(self.scale) * layer.transform,
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
    }
}

/// Renders `logic(app)` once at `size`, writes it to `path`, and returns the state.
///
/// `logic` has to return a view with a concrete widget type, because the
/// rebuild below downcasts the root back to it. Wrapping the
/// application's root view in a `sized_box` is enough, and that is what
/// the caller does.
pub(crate) fn render_to<State, V, F>(
    app: State,
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
    let mut headless = Headless::new(app, &logic, size, scale);
    // One rebuild, so a view that fills its scene there is drawn. The
    // canvas view is the reason: it records nothing until rebuild.
    headless.rebuild(&logic);
    // A real window has already received its first idle animation frame by
    // the time it is useful to inspect it. Drive that frame here too, so
    // auto-hiding portal scrollbars do not get frozen visible in every
    // screenshot solely because this renderer exits after its first paint.
    headless.anim_frame(Duration::from_millis(500));
    headless.write_png(background, path);
    headless.app
}
