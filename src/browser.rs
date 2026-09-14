// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Live browser host for the same desktop Xilem/Masonry widget tree.
//! DOM input is delivered to Masonry; every frame comes from its retained scene.
use crate::*;
use masonry::app::{
    RenderRoot, RenderRootOptions, RenderRootSignal, VisualLayerKind, WindowSizePolicy,
};
use masonry::core::ScrollDelta;
use masonry::core::{TextEvent, WindowEvent};
use masonry::dpi::{PhysicalPosition, PhysicalSize};
use masonry::imaging::{
    Painter,
    record::{Scene, replay_transformed},
    render::ImageRenderer,
};
use masonry::ui_events::{
    keyboard::{Code, Key, KeyState, KeyboardEvent, Modifiers, NamedKey},
    pointer::*,
};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::prelude::*;
use xilem::core::ViewPathTracker;
use xilem::core::{DynMessage, MessageCtx, ProxyError, RawProxy, SendMessage, ViewId};

#[derive(Debug)]
struct BrowserProxy;
impl RawProxy for BrowserProxy {
    fn send_message(&self, _: Arc<[ViewId]>, _: SendMessage) -> Result<(), ProxyError> {
        Ok(())
    }
    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

fn demo_state() -> AppState {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../web/demo-font.json")).unwrap();
    let mut font = norad::Font::new();
    font.font_info.family_name = Some("Virtua Grotesk".into());
    font.font_info.style_name = Some("Regular".into());
    font.font_info.units_per_em = Some(
        data["info"]["unitsPerEm"]
            .as_f64()
            .unwrap_or(1024.)
            .try_into()
            .unwrap(),
    );
    font.font_info.ascender = data["info"]["ascender"].as_f64();
    font.font_info.descender = data["info"]["descender"].as_f64();
    font.font_info.cap_height = data["info"]["capHeight"].as_f64();
    font.font_info.x_height = data["info"]["xHeight"].as_f64();
    for glif in data["glyphs"].as_array().unwrap() {
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::parse_raw(glif.as_str().unwrap().as_bytes()).unwrap());
    }
    let mut project =
        runebender_core::document::project::Project::new_font("VirtuaGrotesk-Regular.ufo".into());
    project.masters[0] = runebender_core::document::project::Master::from_font(
        font,
        "VirtuaGrotesk-Regular.ufo".into(),
    );
    let mut workspace = Workspace::from_model(FontModel::from_project(project)).unwrap();
    workspace.note = "Browser session — edits stay in this tab".into();
    AppState {
        palette: workspace.palette.clone(),
        theme_id: workspace.theme_id,
        workspace: Some(workspace),
        notice: None,
        running: true,
    }
}

trait BrowserApp {
    fn pointer(
        &mut self,
        kind: u8,
        x: f64,
        y: f64,
        button: i16,
        buttons: u16,
        count: u8,
        modifiers: u8,
        dx: f64,
        dy: f64,
    );
    fn key(&mut self, down: bool, key: &str, code: &str, modifiers: u8, repeat: bool);
    fn resize(&mut self, width: u32, height: u32);
    fn frame(&mut self) -> Vec<u8>;
    fn state(&self) -> String;
}
struct Host<V: WidgetView<AppState, Widget: Sized>, F> {
    app: AppState,
    view: V,
    view_state: V::ViewState,
    ctx: xilem::ViewCtx,
    logic: F,
    root: RenderRoot,
    signals: Rc<RefCell<Vec<RenderRootSignal>>>,
    renderer: imaging_vello_cpu::VelloCpuRenderer,
    width: u32,
    height: u32,
    painted_theme: &'static str,
}
impl<V, F> Host<V, F>
where
    V: WidgetView<AppState, Widget: Sized> + 'static,
    F: Fn(&mut AppState) -> V + 'static,
{
    fn rebuild(&mut self) {
        let next = (self.logic)(&mut self.app);
        self.root.edit_base_layer(|mut widget| {
            next.rebuild(
                &self.view,
                &mut self.view_state,
                &mut self.ctx,
                widget.downcast::<V::Widget>(),
                &mut self.app,
            );
        });
        self.view = next;
    }
    fn drain(&mut self) {
        for _ in 0..100 {
            let pending = std::mem::take(&mut *self.signals.borrow_mut());
            if pending.is_empty() {
                break;
            }
            for signal in pending {
                match signal {
                    RenderRootSignal::Action(action, widget_id) => {
                        if let Some(path) = self.ctx.get_id_path(widget_id).cloned() {
                            let mut cx = MessageCtx::new(
                                std::mem::take(self.ctx.environment()),
                                path,
                                DynMessage(action),
                            );
                            self.root.edit_base_layer(|mut widget| {
                                let _ = self.view.message(
                                    &mut self.view_state,
                                    &mut cx,
                                    widget.downcast::<V::Widget>(),
                                    &mut self.app,
                                );
                            });
                            let (env, _, _) = cx.finish();
                            *self.ctx.environment() = env;
                            self.rebuild();
                        }
                    }
                    RenderRootSignal::NewLayer(_, widget, position) => {
                        self.root.add_layer(widget, position)
                    }
                    RenderRootSignal::RemoveLayer(id) => self.root.remove_layer(id),
                    RenderRootSignal::RepositionLayer(id, position) => {
                        self.root.reposition_layer(id, position)
                    }
                    _ => {}
                }
            }
        }
    }
}
fn modifiers(bits: u8) -> Modifiers {
    let mut m = Modifiers::empty();
    if bits & 1 != 0 {
        m.insert(Modifiers::SHIFT);
    }
    if bits & 2 != 0 {
        m.insert(Modifiers::CONTROL);
    }
    if bits & 4 != 0 {
        m.insert(Modifiers::ALT);
    }
    if bits & 8 != 0 {
        m.insert(Modifiers::META);
    }
    m
}
const MOUSE: PointerInfo = PointerInfo {
    pointer_id: Some(PointerId::PRIMARY),
    persistent_device_id: None,
    pointer_type: PointerType::Mouse,
};
impl<V, F> BrowserApp for Host<V, F>
where
    V: WidgetView<AppState, Widget: Sized> + 'static,
    F: Fn(&mut AppState) -> V + 'static,
{
    fn pointer(
        &mut self,
        kind: u8,
        x: f64,
        y: f64,
        button: i16,
        buttons: u16,
        count: u8,
        mods: u8,
        dx: f64,
        dy: f64,
    ) {
        let mut state = PointerState {
            position: PhysicalPosition::new(x, y),
            modifiers: modifiers(mods),
            count,
            ..Default::default()
        };
        for (bit, b) in [
            (1, PointerButton::Primary),
            (2, PointerButton::Secondary),
            (4, PointerButton::Auxiliary),
        ] {
            if buttons & bit != 0 {
                state.buttons.insert(b);
            }
        }
        let button = Some(match button {
            1 => PointerButton::Auxiliary,
            2 => PointerButton::Secondary,
            _ => PointerButton::Primary,
        });
        let event = match kind {
            1 => PointerEvent::Down(PointerButtonEvent {
                pointer: MOUSE,
                button,
                state,
            }),
            2 => PointerEvent::Up(PointerButtonEvent {
                pointer: MOUSE,
                button,
                state,
            }),
            3 => PointerEvent::Scroll(PointerScrollEvent {
                pointer: MOUSE,
                delta: ScrollDelta::PixelDelta(PhysicalPosition::new(dx, dy)),
                state,
            }),
            _ => PointerEvent::Move(PointerUpdate {
                pointer: MOUSE,
                current: state,
                coalesced: vec![],
                predicted: vec![],
            }),
        };
        self.root.handle_pointer_event(event);
        self.drain();
    }
    fn key(&mut self, down: bool, key: &str, code: &str, mods: u8, repeat: bool) {
        use std::str::FromStr;
        self.root
            .handle_text_event(TextEvent::Keyboard(KeyboardEvent {
                state: if down { KeyState::Down } else { KeyState::Up },
                key: Key::from_str(key).unwrap_or(Key::Named(NamedKey::Unidentified)),
                code: Code::from_str(code).unwrap_or(Code::Unidentified),
                modifiers: modifiers(mods),
                repeat,
                ..Default::default()
            }));
        self.drain();
    }
    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.root
            .handle_window_event(WindowEvent::Resize(PhysicalSize::new(width, height)));
        self.drain();
    }
    fn frame(&mut self) -> Vec<u8> {
        if self.painted_theme != self.app.theme_id {
            // Theme changes must invalidate cached widget paint as well as view state.
            // Reapplying the unchanged logical scale requests a complete repaint.
            self.root.handle_window_event(WindowEvent::Rescale(1.));
            self.painted_theme = self.app.theme_id;
        }
        self.root
            .handle_window_event(WindowEvent::AnimFrame(std::time::Duration::from_millis(32)));
        self.drain();
        let (layers, _) = self.root.redraw();
        let mut scene = Scene::new();
        Painter::new(&mut scene).fill_rect(
            kurbo::Rect::new(0., 0., self.width as f64, self.height as f64),
            self.app.background(),
        );
        for layer in &layers.layers {
            if let VisualLayerKind::Scene(s) = &layer.kind {
                replay_transformed(s, &mut scene, layer.transform);
            }
        }
        self.renderer
            .render_source(&mut scene, self.width, self.height)
            .unwrap()
            .data
    }
    fn state(&self) -> String {
        let w = self.app.workspace.as_ref().unwrap();
        serde_json::json!({"mode":match w.mode { Mode::Overview=>"overview",Mode::Editor(_)=>"editor",Mode::Nodes=>"nodes" },"modified":w.modified,"glyph":w.session.glyph_name,"selected_points":w.selected_points,"glyph_count":w.font.glyphs.len(),"note":w.note,"points":w.session.glyph.contours.iter().flat_map(|c|c.points.iter().map(|p|(p.x,p.y))).collect::<Vec<_>>(),"zoom":w.session.viewport.zoom}).to_string()
    }
}
fn create<V, F>(mut app: AppState, logic: F, width: u32, height: u32) -> Box<dyn BrowserApp>
where
    V: WidgetView<AppState, Widget: Sized> + 'static,
    F: Fn(&mut AppState) -> V + 'static,
{
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap(),
    );
    let mut ctx = xilem::ViewCtx::new(Arc::new(BrowserProxy), runtime);
    let view = logic(&mut app);
    let (pod, view_state) = view.build(&mut ctx, &mut app);
    let signals = Rc::new(RefCell::new(Vec::new()));
    let sink = signals.clone();
    let mut root = RenderRoot::new(
        pod.new_widget.erased(),
        move |s| sink.borrow_mut().push(s),
        RenderRootOptions {
            default_properties: Arc::new(default_property_set()),
            use_system_fonts: false,
            size_policy: WindowSizePolicy::User,
            size: PhysicalSize::new(width, height),
            scale_factor: 1.,
            test_font: Some(xilem::Blob::new(Arc::new(UI_FONT))),
        },
    );
    root.register_fonts(xilem::Blob::new(Arc::new(UI_FONT)));
    let mut host = Host {
        painted_theme: app.theme_id,
        app,
        view,
        view_state,
        ctx,
        logic,
        root,
        signals,
        renderer: imaging_vello_cpu::VelloCpuRenderer::new(1, 1),
        width,
        height,
    };
    host.drain();
    host.rebuild();
    host.drain();
    Box::new(host)
}
#[wasm_bindgen]
pub struct BrowserEditor {
    host: Box<dyn BrowserApp>,
}
#[wasm_bindgen]
impl BrowserEditor {
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32) -> Self {
        console_error_panic_hook::set_once();
        Self {
            host: create(
                demo_state(),
                |app| sized_box(root_logic(app)),
                width,
                height,
            ),
        }
    }
    pub fn pointer(
        &mut self,
        kind: u8,
        x: f64,
        y: f64,
        button: i16,
        buttons: u16,
        count: u8,
        mods: u8,
        dx: f64,
        dy: f64,
    ) {
        self.host
            .pointer(kind, x, y, button, buttons, count, mods, dx, dy);
    }
    pub fn key(&mut self, down: bool, key: &str, code: &str, mods: u8, repeat: bool) {
        self.host.key(down, key, code, mods, repeat);
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        self.host.resize(width, height);
    }
    pub fn frame(&mut self) -> Vec<u8> {
        self.host.frame()
    }
    pub fn state(&self) -> String {
        self.host.state()
    }
}

/// Desktop-only actions give feedback without touching the browser's sample document.
pub(crate) fn desktop_action(action: crate::widgets::shortcuts::AppAction) -> bool {
    use crate::widgets::shortcuts::AppAction as A;
    matches!(
        action,
        A::Save | A::SaveAs | A::OpenFont | A::NewFont | A::RevertToSaved | A::ExportFont | A::Quit
    )
}
