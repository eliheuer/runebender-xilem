// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Runebender on xix. First slice: open a font, pick a glyph, edit its
//! outline in a canvas island. See PORT.md for what each slice forced.

use std::collections::HashSet;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PointerScrollEvent, PointerUpdate, PropertiesMut,
    PropertiesRef, RegisterCtx, ScrollDelta, Widget,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, BezPath, Circle, Line, Point, Rect, Size, Stroke};
use masonry::layout::{LenReq, Length};
use masonry::properties::types::CrossAxisAlignment;
use masonry::theme::default_property_set;
use runebender_core::editing::viewport::ViewPort;
use runebender_core::model::entity_id::EntityId;
use runebender_core::model::workspace::Contour;
use runebender_core::path::Path;
use runebender_core::theme::ColorRgba;
use runebender_core::theme_oklch::{Theme, load_theme};
use winit::dpi::LogicalSize;
use winit::error::EventLoopError;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::style::Style;
use xilem::view::{
    FlexExt as _, flex_col, flex_row, label, portal, sized_box, text_button, text_input,
};
use xilem::{
    Color, EventLoop, EventLoopBuilder, Pod, ViewCtx, WidgetView, WindowOptions, Xilem,
};

// ---------------------------------------------------------------------------
// Theme bridge: the shared OKLCH theme file, as peniko colors.

fn color(c: ColorRgba) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
}

#[derive(Clone, Copy)]
enum Role_ {
    PathStroke,
    PointSmooth,
    PointCorner,
    PointOffcurve,
    PointSelected,
    MetricQuiet,
    Accent,
}

struct Palette {
    app: Color,
    panel: Color,
    canvas: Color,
    text: Color,
    text_muted: Color,
    roles: [(Role_, Color); 7],
}

impl Palette {
    fn from_theme(t: &Theme) -> Self {
        let role = |r: Role_, name: &str| (r, color(t.role(name)));
        Self {
            app: color(t.surface("app")),
            panel: color(t.surface("panel")),
            canvas: color(t.surface("canvas")),
            text: color(t.text("primary")),
            text_muted: color(t.text("muted")),
            roles: [
                role(Role_::PathStroke, "pathStroke"),
                role(Role_::PointSmooth, "pointSmooth"),
                role(Role_::PointCorner, "pointCorner"),
                role(Role_::PointOffcurve, "pointOffcurve"),
                role(Role_::PointSelected, "pointSelected"),
                role(Role_::MetricQuiet, "metricQuiet"),
                role(Role_::Accent, "accent"),
            ],
        }
    }
    fn role(&self, r: Role_) -> Color {
        self.roles
            .iter()
            .find(|(k, _)| std::mem::discriminant(k) == std::mem::discriminant(&r))
            .map(|(_, c)| *c)
            .unwrap_or(Color::WHITE)
    }
}

// ---------------------------------------------------------------------------
// Session: the glyph being edited, owned by the canvas island.

#[derive(Clone)]
struct Metrics {
    ascender: f64,
    descender: f64,
    x_height: f64,
    cap_height: f64,
}

#[derive(Clone)]
struct Session {
    glyph_name: String,
    paths: Vec<Path>,
    advance: f64,
    metrics: Metrics,
    selection: HashSet<EntityId>,
    viewport: ViewPort,
    fitted: bool,
}

impl Session {
    fn new(font: &norad::Font, name: &str) -> Option<Self> {
        let glyph = font.get_glyph(name)?;
        let paths = glyph
            .contours
            .iter()
            .map(|c| Path::from_contour(&Contour::from_norad(c)))
            .collect();
        let info = &font.font_info;
        let upm = info.units_per_em.map(|u| u.as_f64()).unwrap_or(1000.0);
        Some(Self {
            glyph_name: name.to_string(),
            paths,
            advance: glyph.width,
            metrics: Metrics {
                ascender: info.ascender.unwrap_or(upm * 0.8),
                descender: info.descender.unwrap_or(-upm * 0.2),
                x_height: info.x_height.unwrap_or(upm * 0.5),
                cap_height: info.cap_height.unwrap_or(upm * 0.7),
            },
            selection: HashSet::new(),
            viewport: ViewPort::new(),
            fitted: false,
        })
    }

    fn bezpath(&self) -> BezPath {
        let mut out = BezPath::new();
        for path in &self.paths {
            path.append_to_bezpath(&mut out);
        }
        out
    }

    fn point_count(&self) -> usize {
        self.paths.iter().map(|p| p.points().len()).sum()
    }
}

// ---------------------------------------------------------------------------
// The canvas island: a Masonry widget that owns the session and the gesture state.

const HIT_RADIUS_PX: f64 = 8.0;

#[derive(Debug)]
struct EditorEvent {
    selected: usize,
}

enum Drag {
    None,
    Points { last: Point },
    Pan { last: Point },
}

struct EditorWidget {
    session: Session,
    palette: Arc<Palette>,
    size: Size,
    drag: Drag,
}

impl EditorWidget {
    fn screen_points(&self) -> Vec<(EntityId, Point, bool, bool)> {
        let affine = self.session.viewport.affine();
        let mut out = Vec::new();
        for path in &self.session.paths {
            for p in path.points().iter() {
                let smooth = matches!(
                    p.typ,
                    runebender_core::path::point::PointType::OnCurve { smooth: true }
                );
                out.push((p.id, affine * p.point, p.is_on_curve(), smooth));
            }
        }
        out
    }

    fn hit_point(&self, at: Point) -> Option<EntityId> {
        self.screen_points()
            .into_iter()
            .filter(|(_, sp, _, _)| sp.distance(at) <= HIT_RADIUS_PX)
            .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
            .map(|(id, _, _, _)| id)
    }

    fn translate_selection(&mut self, delta_design: kurbo::Vec2) {
        for path in &mut self.session.paths {
            let points = match path {
                Path::Cubic(p) => p.points.make_mut(),
                Path::Quadratic(p) => p.points.make_mut(),
                Path::Hyper(p) => p.points.make_mut(),
            };
            for p in points.iter_mut() {
                if self.session.selection.contains(&p.id) {
                    p.point += delta_design;
                }
            }
        }
    }

    fn fit(&mut self) {
        let m = &self.session.metrics;
        self.session.viewport.fit_to_canvas(
            self.size.width,
            self.size.height,
            self.session.advance,
            m.ascender,
            m.descender,
            0.62,
        );
        self.session.fitted = true;
    }
}

impl Widget for EditorWidget {
    type Action = EditorEvent;

    fn accepts_focus(&self) -> bool {
        true
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        match len_req {
            LenReq::FitContent(space) => space,
            _ => Length::px(200.0),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        if self.size != size {
            self.size = size;
            if !self.session.fitted {
                self.fit();
            }
        }
        ctx.set_clip_path(size.to_rect());
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let pal = self.palette.clone();
        let affine = self.session.viewport.affine();
        let zoom = self.session.viewport.zoom;
        painter.fill_rect(self.size.to_rect(), pal.canvas);

        // Metrics: baseline, x-height, cap height, ascender, descender, sidebearings.
        let m = &self.session.metrics;
        let thin = Stroke::new(1.0);
        let quiet = pal.role(Role_::MetricQuiet);
        let x0 = (affine * Point::new(-10_000.0, 0.0)).x;
        let x1 = (affine * Point::new(10_000.0, 0.0)).x;
        for y in [0.0, m.x_height, m.cap_height, m.ascender, m.descender] {
            let sy = (affine * Point::new(0.0, y)).y;
            painter
                .stroke(Line::new((x0, sy), (x1, sy)), &thin, quiet)
                .draw();
        }
        for x in [0.0, self.session.advance] {
            let sx = (affine * Point::new(x, 0.0)).x;
            painter
                .stroke(Line::new((sx, -10_000.0), (sx, 10_000.0)), &thin, quiet)
                .draw();
        }

        // Outline.
        let outline = affine * self.session.bezpath();
        painter
            .fill(&outline, pal.role(Role_::PathStroke).with_alpha(0.08))
            .draw();
        painter
            .stroke(&outline, &Stroke::new(1.0), pal.role(Role_::PathStroke))
            .draw();

        // Handles: off-curve points connect to their neighboring on-curve points.
        let handle = Stroke::new(1.0);
        for path in &self.session.paths {
            let pts = path.points().as_slice();
            let n = pts.len();
            if n == 0 {
                continue;
            }
            for i in 0..n {
                let p = &pts[i];
                if !p.is_off_curve() {
                    continue;
                }
                for j in [(i + n - 1) % n, (i + 1) % n] {
                    if pts[j].is_on_curve() {
                        painter
                            .stroke(
                                Line::new(affine * p.point, affine * pts[j].point),
                                &handle,
                                pal.role(Role_::PointOffcurve).with_alpha(0.7),
                            )
                            .draw();
                    }
                }
            }
        }

        // Points. Sizes are screen-space; they do not scale with zoom.
        let _ = zoom;
        for (id, sp, on_curve, smooth) in self.screen_points() {
            let selected = self.session.selection.contains(&id);
            let fill = if selected {
                pal.role(Role_::PointSelected)
            } else if !on_curve {
                pal.role(Role_::PointOffcurve)
            } else if smooth {
                pal.role(Role_::PointSmooth)
            } else {
                pal.role(Role_::PointCorner)
            };
            if on_curve && !smooth {
                let r = 4.5;
                painter
                    .fill(Rect::new(sp.x - r, sp.y - r, sp.x + r, sp.y + r), fill)
                    .draw();
            } else {
                let r = if on_curve { 4.5 } else { 3.0 };
                painter.fill(Circle::new(sp, r), fill).draw();
            }
        }
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent { button, state, .. }) => {
                ctx.request_focus();
                ctx.capture_pointer();
                let at = ctx.local_position(state.position);
                match button {
                    Some(PointerButton::Primary) => {
                        let shift = state.modifiers.shift();
                        match self.hit_point(at) {
                            Some(id) => {
                                if shift {
                                    if !self.session.selection.remove(&id) {
                                        self.session.selection.insert(id);
                                    }
                                } else if !self.session.selection.contains(&id) {
                                    self.session.selection.clear();
                                    self.session.selection.insert(id);
                                }
                                self.drag = Drag::Points { last: at };
                            }
                            None => {
                                if !shift {
                                    self.session.selection.clear();
                                }
                                self.drag = Drag::Pan { last: at };
                            }
                        }
                    }
                    _ => self.drag = Drag::Pan { last: at },
                }
                ctx.request_render();
                ctx.set_handled();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let at = ctx.local_position(current.position);
                match &mut self.drag {
                    Drag::Points { last } => {
                        let zoom = self.session.viewport.zoom;
                        let d = at - *last;
                        *last = at;
                        // Screen y grows down; design y grows up.
                        self.translate_selection(kurbo::Vec2::new(d.x / zoom, -d.y / zoom));
                        ctx.request_render();
                    }
                    Drag::Pan { last } => {
                        let d = at - *last;
                        *last = at;
                        self.session.viewport.pan(d.x, d.y);
                        ctx.request_render();
                    }
                    Drag::None => {}
                }
            }
            PointerEvent::Up(_) | PointerEvent::Cancel(_) => {
                if !matches!(self.drag, Drag::None) {
                    self.drag = Drag::None;
                    ctx.submit_action::<EditorEvent>(EditorEvent {
                        selected: self.session.selection.len(),
                    });
                    ctx.request_render();
                }
            }
            PointerEvent::Scroll(PointerScrollEvent { delta, state, .. }) => {
                let at = ctx.local_position(state.position);
                let dy = match delta {
                    ScrollDelta::PixelDelta(p) => p.y,
                    ScrollDelta::LineDelta(_, y) => f64::from(*y) * 20.0,
                    _ => 0.0,
                };
                let factor = (dy / 300.0).exp();
                self.session.viewport.zoom_about(at, factor, 0.02, 64.0);
                ctx.request_render();
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Canvas
    }

    fn accessibility(&mut self, _ctx: &mut AccessCtx<'_>, _props: &PropertiesRef<'_>, node: &mut Node) {
        node.set_description(format!("Glyph editor: {}", self.session.glyph_name));
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

// ---------------------------------------------------------------------------
// The view that hosts the island. The app hands it a session; it reports events back.

struct EditorView<F> {
    session: Arc<Session>,
    palette: Arc<Palette>,
    on_event: F,
}

fn editor<F: Fn(&mut App, EditorEvent) + 'static>(
    session: Arc<Session>,
    palette: Arc<Palette>,
    on_event: F,
) -> EditorView<F> {
    EditorView {
        session,
        palette,
        on_event,
    }
}

impl<F> ViewMarker for EditorView<F> {}
impl<F: Fn(&mut App, EditorEvent) + 'static> View<App, (), ViewCtx> for EditorView<F> {
    type Element = Pod<EditorWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut App) -> (Self::Element, Self::ViewState) {
        let widget = EditorWidget {
            session: (*self.session).clone(),
            palette: self.palette.clone(),
            size: Size::ZERO,
            drag: Drag::None,
        };
        (ctx.with_action_widget(|ctx| ctx.create_pod(widget)), ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut App,
    ) {
        if !Arc::ptr_eq(&self.session, &prev.session) {
            // A new glyph. The island keeps its viewport; the session is replaced.
            let viewport = element.widget.session.viewport.clone();
            let fitted = element.widget.session.fitted;
            element.widget.session = (*self.session).clone();
            element.widget.session.viewport = viewport;
            element.widget.session.fitted = fitted;
            element.ctx.request_render();
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app: &mut App,
    ) -> MessageResult<()> {
        match message.take_message::<EditorEvent>() {
            Some(event) => {
                (self.on_event)(app, *event);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}

// ---------------------------------------------------------------------------
// App state and views.

struct App {
    font: Arc<norad::Font>,
    source: PathBuf,
    names: Vec<String>,
    filter: String,
    session: Arc<Session>,
    palette: Arc<Palette>,
    selected_points: usize,
}

impl App {
    fn open(path: &FsPath) -> Result<Self, String> {
        let ufo_path = if path.extension().is_some_and(|e| e == "designspace") {
            let doc = norad::designspace::DesignSpaceDocument::load(path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let first = doc
                .sources
                .first()
                .ok_or_else(|| "designspace has no sources".to_string())?;
            path.parent()
                .unwrap_or(FsPath::new("."))
                .join(&first.filename)
        } else {
            path.to_path_buf()
        };
        let font = norad::Font::load(&ufo_path).map_err(|e| format!("{}: {e}", ufo_path.display()))?;
        let mut names: Vec<String> = font.iter_names().map(|n| n.to_string()).collect();
        names.sort();
        let start = ["a", "A", "o", "n"]
            .iter()
            .find(|n| font.get_glyph(n).is_some())
            .map(|n| n.to_string())
            .or_else(|| names.first().cloned())
            .ok_or_else(|| "font has no glyphs".to_string())?;
        let session = Session::new(&font, &start).ok_or("glyph missing")?;
        let theme = load_theme("dark").ok_or("theme missing")?;
        Ok(Self {
            font: Arc::new(font),
            source: ufo_path,
            names,
            filter: String::new(),
            session: Arc::new(session),
            palette: Arc::new(Palette::from_theme(&theme)),
            selected_points: 0,
        })
    }

    fn select_glyph(&mut self, name: &str) {
        if let Some(session) = Session::new(&self.font, name) {
            self.session = Arc::new(session);
            self.selected_points = 0;
        }
    }
}

fn sidebar(app: &App) -> impl WidgetView<App> + use<> {
    let pal = app.palette.clone();
    let filter = app.filter.to_lowercase();
    let buttons: Vec<_> = app
        .names
        .iter()
        .filter(|n| filter.is_empty() || n.to_lowercase().contains(&filter))
        .take(400)
        .map(|n| {
            let name = n.clone();
            let current = *n == app.session.glyph_name;
            text_button(n.clone(), move |app: &mut App| app.select_glyph(&name))
                .background_color(if current { pal.role(Role_::Accent) } else { pal.panel })
        })
        .collect();
    flex_col((
        text_input(app.filter.clone(), |app: &mut App, v| app.filter = v)
            .placeholder("Find glyph"),
        portal(flex_col(buttons).gap(Length::px(2.0))).flex(1.0),
    ))
    .gap(Length::px(8.0))
    .padding(Length::px(8.0))
    .background_color(pal.panel)
}

fn status(app: &App) -> impl WidgetView<App> + use<> {
    let s = &app.session;
    let text = format!(
        "{}   advance {}   {} points   {} selected   {}",
        s.glyph_name,
        s.advance,
        s.point_count(),
        app.selected_points,
        app.source.display()
    );
    flex_row((label(text).color(app.palette.text_muted),))
        .padding(Length::px(8.0))
        .background_color(app.palette.panel)
}

fn app_logic(app: &mut App) -> impl WidgetView<App> + use<> {
    let editor = editor(app.session.clone(), app.palette.clone(), |app: &mut App, ev| {
        app.selected_points = ev.selected;
    });
    flex_col((
        flex_row((
            sized_box(sidebar(app)).fixed_width(Length::px(200.0)),
            editor.flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .flex(1.0),
        status(app),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(0.0))
}

fn run(event_loop: EventLoopBuilder) -> Result<(), EventLoopError> {
    let path = std::env::args().nth(1).expect("usage: runebender-xix <Font.ufo|Font.designspace>");
    let app = App::open(FsPath::new(&path)).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let background = app.palette.app;
    let window_options = WindowOptions::new("Runebender")
        .with_initial_inner_size(LogicalSize::new(1100., 720.));
    Xilem::new_simple(app, app_logic, window_options)
        .with_default_properties(default_property_set())
        .with_default_base_color(background)
        .run_in(event_loop)
}

fn main() -> Result<(), EventLoopError> {
    run(EventLoop::with_user_event())
}
