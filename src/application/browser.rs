// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Live browser host for the same desktop Xilem/Masonry widget tree.
//! DOM input is delivered to Masonry; every frame comes from its retained scene.
use crate::application::font_model::FontModel;
use crate::application::view::render::root_logic;
use crate::application::view::{UI_FONT, default_property_set};
use crate::application::workspace::{AppState, Mode, Workspace};
use masonry::app::{
    RenderRoot, RenderRootOptions, RenderRootSignal, VisualLayerKind, WindowSizePolicy,
};
use masonry::core::{Ime, ScrollDelta};
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
use runebender::font::persistence::memory::{
    MemoryBookkeeping, memory_save_files, open_memory_project,
};
use runebender::font::project::Project;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
use wasm_bindgen::prelude::*;

thread_local! {
    /// The file command the page must carry out next: "open", "save" or "save-as".
    static HOST_REQUEST: Cell<Option<&'static str>> = const { Cell::new(None) };
}

#[wasm_bindgen(
    inline_js = "export function download_font(name, data) { const url = URL.createObjectURL(new Blob([data], {type:'font/ttf'})); const a = document.createElement('a'); a.href = url; a.download = name; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000); }"
)]
extern "C" {
    pub(crate) fn download_font(name: &str, data: &[u8]);
}
use xilem::WidgetView;
use xilem::core::ViewPathTracker;
use xilem::core::{DynMessage, MessageCtx, ProxyError, RawProxy, SendMessage, ViewId};
use xilem::view::sized_box;

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
    let project = runebender::font::persistence::memory::project_from_embedded_glif_json(
        "VirtuaGrotesk-Regular.ufo".into(),
        include_str!("../../web/demo-font.json"),
    )
    .expect("the embedded browser demo font is valid");
    let mut workspace = Workspace::from_model(FontModel::from_project(project)).unwrap();
    // Open a real, editable graph in memory so Graph is useful on first visit.
    workspace.new_graph_file();
    let graph = serde_json::from_str(include_str!("../../web/demo.graph.json")).unwrap();
    workspace.graph_changed(graph);
    workspace.graph.open.as_mut().unwrap().path = "example.graph.json".into();
    workspace.mode = Mode::Overview;
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
    fn resize(&mut self, width: u32, height: u32, scale: f64);
    fn frame(&mut self, elapsed_ms: f64) -> Vec<u8>;
    fn state(&self) -> String;
    fn feedback(&mut self) -> String;
    fn text(&mut self, kind: u8, text: String);
    fn focus(&mut self, focused: bool);
    fn open_bundle(&mut self, ufo_name: Option<String>, bundle: &[u8]) -> String;
    fn save_bundle(&mut self, into_new_folder: bool) -> Vec<u8>;
    fn host_result(&mut self, kind: &str, ok: bool, message: &str);
}

/// Split a host bundle: a little-endian length, a JSON manifest of `[path, offset, length]`
/// entries, then the file bytes.
fn parse_bundle(bundle: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let header = bundle.get(..4).ok_or("bundle is too short")?;
    let length = u32::from_le_bytes(header.try_into().expect("four bytes")) as usize;
    let manifest = bundle
        .get(4..4 + length)
        .ok_or("bundle manifest is truncated")?;
    let entries: Vec<(String, usize, usize)> =
        serde_json::from_slice(manifest).map_err(|error| format!("bundle manifest: {error}"))?;
    let data = &bundle[4 + length..];
    entries
        .into_iter()
        .map(|(path, offset, size)| {
            let bytes = data
                .get(offset..offset + size)
                .ok_or_else(|| format!("{path}: bytes lie outside the bundle"))?;
            Ok((path, bytes.to_vec()))
        })
        .collect()
}

/// Pack files for the page: the manifest lists `files` as `[path, offset, length]` and
/// `prune`, directories whose unlisted `.glif` files are stale.
fn build_bundle(files: &[(String, Vec<u8>)], prune: &[String]) -> Vec<u8> {
    let mut offset = 0;
    let entries: Vec<serde_json::Value> = files
        .iter()
        .map(|(path, bytes)| {
            let entry = serde_json::json!([path, offset, bytes.len()]);
            offset += bytes.len();
            entry
        })
        .collect();
    let manifest = serde_json::json!({"files": entries, "prune": prune}).to_string();
    let mut bundle = Vec::with_capacity(4 + manifest.len() + offset);
    let header = u32::try_from(manifest.len()).expect("a save manifest is far below 4 GiB");
    bundle.extend_from_slice(&header.to_le_bytes());
    bundle.extend_from_slice(manifest.as_bytes());
    for (_, bytes) in files {
        bundle.extend_from_slice(bytes);
    }
    bundle
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
    scale: f64,
    dirty: bool,
    ime_active: bool,
    ime_position: [f64; 2],
    clipboard: Option<String>,
    painted_theme: &'static str,
    /// Save bookkeeping for a font opened from the page's folder; none for the demo font.
    bookkeeping: Option<MemoryBookkeeping>,
    /// The folder's own name when the user opened a UFO directory itself.
    ufo_root: Option<String>,
}
impl<V, F> Host<V, F>
where
    V: WidgetView<AppState, Widget: Sized> + 'static,
    F: Fn(&mut AppState) -> V + 'static,
{
    /// Replace the workspace with `project`, keeping the chosen theme.
    fn install_project(&mut self, project: Project, label: &str) -> Result<(), String> {
        let mut workspace = Workspace::from_model(FontModel::from_project(project))?;
        if workspace.theme_id != self.app.theme_id {
            workspace.set_theme(self.app.theme_id);
        }
        workspace.note = format!("Opened {label}");
        self.app.theme_id = workspace.theme_id;
        self.app.palette = workspace.palette.clone();
        self.app.workspace = Some(workspace);
        self.app.notice = None;
        self.rebuild();
        self.dirty = true;
        Ok(())
    }
    fn set_note(&mut self, note: String) {
        if let Some(workspace) = self.app.workspace.as_mut() {
            workspace.note = note;
        }
        self.rebuild();
        self.dirty = true;
    }
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
                        self.root.add_layer(widget, position);
                    }
                    RenderRootSignal::RemoveLayer(id) => {
                        self.root.remove_layer(id);
                    }
                    RenderRootSignal::RepositionLayer(id, position) => {
                        self.root.reposition_layer(id, position);
                    }
                    RenderRootSignal::RequestRedraw => self.dirty = true,
                    RenderRootSignal::StartIme => self.ime_active = true,
                    RenderRootSignal::EndIme => self.ime_active = false,
                    RenderRootSignal::ImeMoved(pos, _) => self.ime_position = [pos.x, pos.y],
                    RenderRootSignal::ClipboardStore(text) => self.clipboard = Some(text),
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
    fn resize(&mut self, width: u32, height: u32, scale: f64) {
        self.width = width;
        self.height = height;
        if self.scale != scale {
            self.scale = scale;
            self.root.handle_window_event(WindowEvent::Rescale(scale));
        }
        self.root
            .handle_window_event(WindowEvent::Resize(PhysicalSize::new(width, height)));
        self.drain();
    }
    fn frame(&mut self, elapsed_ms: f64) -> Vec<u8> {
        if self.painted_theme != self.app.theme_id {
            // Theme changes must invalidate cached widget paint as well as view state.
            // Reapplying the unchanged logical scale requests a complete repaint.
            self.root
                .handle_window_event(WindowEvent::Rescale(self.scale));
            self.painted_theme = self.app.theme_id;
        }
        if self.root.needs_anim() {
            self.root.handle_window_event(WindowEvent::AnimFrame(
                std::time::Duration::from_secs_f64(elapsed_ms.clamp(0., 100.) / 1000.),
            ));
        }
        self.drain();
        self.dirty = false;
        let (layers, _) = self.root.redraw();
        let mut scene = Scene::new();
        Painter::new(&mut scene).fill_rect(
            kurbo::Rect::new(0., 0., self.width as f64, self.height as f64),
            self.app.background(),
        );
        for layer in &layers.layers {
            if let VisualLayerKind::Scene(s) = &layer.kind {
                replay_transformed(
                    s,
                    &mut scene,
                    kurbo::Affine::scale(self.scale) * layer.transform,
                );
            }
        }
        self.renderer
            .render_source(&mut scene, self.width, self.height)
            .unwrap()
            .data
    }
    fn feedback(&mut self) -> String {
        serde_json::json!({"cursor": self.root.cursor_icon().to_string(),
            "dirty": self.dirty, "animate": self.root.needs_anim(),
            "ime": self.ime_active, "imePosition": self.ime_position,
            "clipboard": self.clipboard.take(),
            "request": HOST_REQUEST.take()})
        .to_string()
    }
    fn open_bundle(&mut self, ufo_name: Option<String>, bundle: &[u8]) -> String {
        let opened =
            parse_bundle(bundle).and_then(|files| open_memory_project(ufo_name.as_deref(), &files));
        let opened = match opened {
            Ok(opened) => opened,
            Err(error) => {
                self.set_note(format!("Open failed: {error}"));
                return error;
            }
        };
        let label = opened
            .project
            .export_source
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "font".into());
        match self.install_project(opened.project, &label) {
            Ok(()) => {
                self.bookkeeping = Some(opened.bookkeeping);
                self.ufo_root = ufo_name;
                String::new()
            }
            Err(error) => {
                self.set_note(format!("Open failed: {error}"));
                error
            }
        }
    }
    fn save_bundle(&mut self, into_new_folder: bool) -> Vec<u8> {
        let Some(workspace) = self.app.workspace.as_ref() else {
            return Vec::new();
        };
        if workspace.features_edited {
            self.set_note("Apply or Revert feature edits before saving".into());
            return Vec::new();
        }
        let bookkeeping = self
            .bookkeeping
            .get_or_insert_with(MemoryBookkeeping::default);
        let save = match memory_save_files(&workspace.font.project, bookkeeping) {
            Ok(save) => save,
            Err(error) => {
                self.set_note(format!("Save failed: {error}"));
                return Vec::new();
            }
        };
        if into_new_folder {
            // The page writes into a folder the user just chose: the UFO sits inside it.
            self.ufo_root = None;
        }
        let mut files = save.files;
        let mut prune = save.glif_directories;
        if let Some(root) = &self.ufo_root {
            let prefix = format!("{root}/");
            let strip = |path: &mut String| {
                if let Some(rest) = path.strip_prefix(&prefix) {
                    *path = rest.to_owned();
                }
            };
            files.iter_mut().for_each(|(path, _)| strip(path));
            prune.iter_mut().for_each(strip);
        }
        build_bundle(&files, &prune)
    }
    fn host_result(&mut self, kind: &str, ok: bool, message: &str) {
        let note = match (kind, ok) {
            ("save", true) => {
                let Some(workspace) = self.app.workspace.as_mut() else {
                    return;
                };
                workspace.font.project.mark_saved();
                workspace.modified = false;
                let source = workspace.font.document_source();
                let label = source
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| source.display().to_string());
                format!("Saved {label}")
            }
            ("save", false) if message.is_empty() => "Save cancelled".into(),
            ("save", false) => format!("Save failed: {message}"),
            (_, false) if message.is_empty() => "Open cancelled".into(),
            (_, false) => format!("Open failed: {message}"),
            _ => return,
        };
        self.set_note(note);
    }
    fn text(&mut self, kind: u8, text: String) {
        match kind {
            0 => {
                self.root.handle_text_event(TextEvent::ClipboardPaste(text));
            }
            1 => {
                let end = text.len();
                self.root
                    .handle_text_event(TextEvent::Ime(Ime::Preedit(text, Some((end, end)))));
            }
            2 => {
                self.root
                    .handle_text_event(TextEvent::Ime(Ime::Preedit(String::new(), None)));
                self.root
                    .handle_text_event(TextEvent::Ime(Ime::Commit(text)));
            }
            _ => {
                self.root.handle_text_event(TextEvent::Ime(Ime::Enabled));
            }
        }
        self.drain();
    }
    fn focus(&mut self, focused: bool) {
        self.root
            .handle_text_event(TextEvent::WindowFocusChange(focused));
        self.drain();
    }
    fn state(&self) -> String {
        let w = self.app.workspace.as_ref().unwrap();
        serde_json::json!({"source":w.font.document_source().display().to_string(),"graph":w.graph.open.as_ref().map(|g| &g.graph),"simd":cfg!(target_feature="simd128"),"scale":self.scale,"mode":match w.mode { Mode::Overview=>"overview",Mode::Editor(_)=>"editor",Mode::Graph=>"graph" },"modified":w.modified,"glyph":w.session.glyph_name,"selected_points":w.selected_points,"glyph_count":w.font.glyphs.len(),"note":w.note,"points":w.session.points().into_iter().map(|p|(p.point.x,p.point.y)).collect::<Vec<_>>(),"zoom":w.session.viewport.zoom}).to_string()
    }
}
fn create<V, F>(
    mut app: AppState,
    logic: F,
    width: u32,
    height: u32,
    scale: f64,
) -> Box<dyn BrowserApp>
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
            scale_factor: scale,
            test_font: Some(xilem::Blob::new(Arc::new(UI_FONT))),
        },
    );
    root.register_fonts(xilem::Blob::new(Arc::new(UI_FONT)));
    let mut host = Host {
        scale,
        dirty: true,
        ime_active: false,
        ime_position: [0., 0.],
        clipboard: None,
        painted_theme: app.theme_id,
        bookkeeping: None,
        ufo_root: None,
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
#[expect(
    unnameable_types,
    missing_debug_implementations,
    reason = "this opaque handle is exported to JavaScript rather than as a Rust API"
)]
pub struct BrowserEditor {
    host: Box<dyn BrowserApp>,
}
#[wasm_bindgen]
impl BrowserEditor {
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32, scale: f64) -> Self {
        console_error_panic_hook::set_once();
        Self {
            host: create(
                demo_state(),
                |app| sized_box(root_logic(app)),
                width,
                height,
                scale,
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
    pub fn resize(&mut self, width: u32, height: u32, scale: f64) {
        self.host.resize(width, height, scale);
    }
    pub fn frame(&mut self, elapsed_ms: f64) -> Vec<u8> {
        self.host.frame(elapsed_ms)
    }
    pub fn feedback(&mut self) -> String {
        self.host.feedback()
    }
    pub fn text(&mut self, kind: u8, text: String) {
        self.host.text(kind, text);
    }
    pub fn focus(&mut self, focused: bool) {
        self.host.focus(focused);
    }
    pub fn state(&self) -> String {
        self.host.state()
    }
    /// Open a UFO or designspace folder the page read: `ufo_name` when the folder is the UFO.
    /// Returns an empty string, or the error.
    pub fn open_bundle(&mut self, ufo_name: Option<String>, bundle: &[u8]) -> String {
        self.host.open_bundle(ufo_name, bundle)
    }
    /// The files the page must write to save the font; empty when nothing can be saved.
    pub fn save_bundle(&mut self, into_new_folder: bool) -> Vec<u8> {
        self.host.save_bundle(into_new_folder)
    }
    /// What became of an "open" or "save" request the page carried out.
    pub fn host_result(&mut self, kind: &str, ok: bool, message: &str) {
        self.host.host_result(kind, ok, message);
    }
}

/// Route a desktop file command to the page, or explain why the browser cannot run it.
///
/// Open and Save become host requests the page answers with the File System Access API.
/// Returns the status note to show, or `None` when the command runs in the widget tree.
pub(crate) fn intercept(
    action: crate::application::widgets::shortcuts::AppAction,
    modified: bool,
) -> Option<String> {
    use crate::application::widgets::shortcuts::AppAction as A;
    let (request, note) = match action {
        A::OpenFont if modified => {
            return Some("Save first (Ctrl+S), or reload the page to discard the changes".into());
        }
        A::OpenFont => (
            "open",
            "Choose a UFO folder, or a folder with a designspace",
        ),
        A::Save => ("save", "Saving…"),
        A::SaveAs => ("save-as", "Choose the folder to save into"),
        A::NewFont | A::RevertToSaved | A::Quit => {
            return Some("Use the desktop app for this command".into());
        }
        _ => return None,
    };
    HOST_REQUEST.set(Some(request));
    Some(note.into())
}
