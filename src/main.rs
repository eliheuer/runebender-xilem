// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Runebender on xix. A font editor: glyph grid, glyph editor, sidebar.
//! See PORT.md for what each slice forced into the framework.

mod editor;
mod grid;
mod icon_button;
mod model;
mod session;
mod text_label;
mod theme;

use std::path::Path as FsPath;
use std::sync::Arc;

use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use masonry::properties::types::CrossAxisAlignment;
use masonry::theme::default_property_set;
use winit::dpi::LogicalSize;
use winit::error::EventLoopError;
use xilem::style::Style;
use xilem::view::{
    FlexExt as _, canvas, flex_col, flex_row, label, portal, sized_box, text_button, text_input,
};
use xilem::{EventLoop, EventLoopBuilder, WidgetView, WindowOptions, Xilem};

use editor::editor;
use grid::{Cell, CellMetrics, GridEvent, cells_of, grid};
use icon_button::icon_button;
use model::FontModel;
use runebender_core::category::GlyphCategory;
use session::Session;
use theme::Palette;

/// The active editor tool.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Pen,
    Rect,
    Ellipse,
    HyperPen,
    Knife,
    Measure,
}

/// Which surface is showing.
enum Mode {
    /// The glyph grid.
    Overview,
    /// The editor, on the glyph at this index.
    Editor(usize),
}

pub struct App {
    font: FontModel,
    palette: Arc<Palette>,
    cells: Arc<Vec<Cell>>,
    mode: Mode,
    selected: Option<usize>,
    filter: String,
    category: GlyphCategory,
    // Editor session, when a glyph is open.
    session: Arc<Session>,
    selected_points: usize,
    tool: Tool,
    modified: bool,
    note: String,
    advance_buf: String,
    name_buf: String,
    unicode_buf: String,
}

impl App {
    fn open(path: &FsPath) -> Result<Self, String> {
        let font = FontModel::open(path)?;
        let palette = Arc::new(Palette::load("dark"));
        let cells = Arc::new(cells_of(&font, &palette));
        let first = font
            .index_of("A")
            .or_else(|| font.index_of("a"))
            .or(if font.glyphs.is_empty() { None } else { Some(0) })
            .ok_or_else(|| "font has no glyphs".to_string())?;
        let session = Arc::new(
            Session::new(&font.font, &font.glyphs[first].name).ok_or("glyph missing")?,
        );
        // For headless screenshots: optionally select all points.
        // (set later, after session is final)
        
        let start_cat = std::env::var("RUNEBENDER_CAT").ok();
        let (mode, open) = match std::env::var("RUNEBENDER_OPEN").ok().and_then(|n| font.index_of(&n)) {
            Some(i) => (Mode::Editor(i), Some(i)),
            None => (Mode::Overview, None),
        };
        let session = match open {
            Some(i) => Arc::new(
                Session::new(&font.font, &font.glyphs[i].name).unwrap_or_else(|| (*session).clone()),
            ),
            None => session,
        };
        let first_name = font.glyphs[first].name.clone();
        let first_uni = font.glyphs[first]
            .codepoint
            .map(|c| format!("{:04X}", c as u32))
            .unwrap_or_default();
        Ok(Self {
            font,
            palette,
            cells,
            mode,
            selected: Some(open.unwrap_or(first)),
            filter: String::new(),
            category: match start_cat.as_deref() {
                Some("Number") => GlyphCategory::Number,
                Some("Symbol") => GlyphCategory::Symbol,
                Some("Mark") => GlyphCategory::Mark,
                _ => GlyphCategory::All,
            },
            advance_buf: format!("{}", session.advance() as i64),
            name_buf: first_name,
            unicode_buf: first_uni,
            session,
            selected_points: 0,
            tool: match std::env::var("RUNEBENDER_TOOL").as_deref() {
                Ok("measure") => Tool::Measure,
                _ => Tool::Select,
            },
            modified: false,
            note: String::new(),
        })
    }

    /// The cells that pass the current search + category filter.
    fn filtered_cells(&self) -> Arc<Vec<Cell>> {
        let q = self.filter.to_lowercase();
        let cat = self.category;
        let out: Vec<Cell> = self
            .cells
            .iter()
            .filter(|c| {
                let cat_ok = cat == GlyphCategory::All || {
                    let entry = &self.font.glyphs[c.index];
                    entry.category == cat
                };
                let q_ok = q.is_empty()
                    || c.name.to_lowercase().contains(&q)
                    || c
                        .codepoint
                        .map(|cp| format!("{:04x}", cp as u32).contains(q.trim_start_matches("u+").trim_start_matches("0x")))
                        .unwrap_or(false);
                cat_ok && q_ok
            })
            .cloned()
            .collect();
        Arc::new(out)
    }

    fn category_count(&self, cat: GlyphCategory) -> usize {
        if cat == GlyphCategory::All {
            self.font.glyphs.len()
        } else {
            self.font.glyphs.iter().filter(|g| g.category == cat).count()
        }
    }

    fn cell_metrics(&self) -> CellMetrics {
        CellMetrics {
            ascender: self.font.ascender,
            descender: self.font.descender,
            upm: self.font.units_per_em,
        }
    }

    fn open_glyph(&mut self, index: usize) {
        if let Some(entry) = self.font.glyphs.get(index) {
            if let Some(session) = Session::new(&self.font.font, &entry.name) {
                self.advance_buf = format!("{}", session.advance() as i64);
                self.name_buf = entry.name.clone();
                self.unicode_buf = entry.codepoint.map(|c| format!("{:04X}", c as u32)).unwrap_or_default();
                self.session = Arc::new(session);
                self.selected = Some(index);
                self.selected_points = 0;
                self.mode = Mode::Editor(index);
            }
        }
    }

    /// After an edit, pull the glyph back out of the session and refresh
    /// the model + grid cache so the overview preview matches.
    /// Replace the app's session with the island's live one (called on every
    /// editor event so save/preview see interactive edits).
    pub fn sync_session_from(&mut self, session: &Session) {
        self.session = Arc::new(session.clone());
    }

    fn refresh_open_glyph(&mut self) {
        if let Mode::Editor(index) = self.mode {
            let glyph = self.session.glyph.clone();
            self.font.replace_glyph(index, glyph);
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            self.modified = true;
            self.note.clear();
        }
    }

    fn save(&mut self) {
        self.refresh_open_glyph();
        match self.font.save() {
            Ok(()) => {
                self.modified = false;
                self.note = format!("Saved {}", self.font.source.display());
            }
            Err(e) => self.note = format!("Save failed: {e}"),
        }
    }

    fn apply_op(&mut self, f: impl FnOnce(&mut Session) -> bool) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let mut sess = (*self.session).clone();
        if f(&mut sess) {
            self.session = Arc::new(sess);
            self.refresh_open_glyph();
        }
    }

    fn set_unicode_from_buf(&mut self, v: String) {
        self.unicode_buf = v;
        let mut sess = (*self.session).clone();
        if sess.set_unicode(self.unicode_buf.trim()) {
            self.session = Arc::new(sess);
            self.refresh_open_glyph();
        }
    }

    fn commit_rename(&mut self) {
        let new = self.name_buf.trim().to_string();
        if new.is_empty() || new == self.session.glyph_name {
            return;
        }
        let old = self.session.glyph_name.clone();
        if self.font.rename_glyph(&old, &new) {
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            if let Some(i) = self.font.index_of(&new) {
                self.mode = Mode::Editor(i);
                self.selected = Some(i);
                if let Some(sess) = Session::new(&self.font.font, &new) {
                    self.session = Arc::new(sess);
                }
            }
            self.modified = true;
        }
    }

    fn set_advance_from_buf(&mut self, v: String) {
        self.advance_buf = v;
        if let Ok(w) = self.advance_buf.trim().parse::<f64>() {
            let mut sess = (*self.session).clone();
            sess.set_advance(w);
            self.session = Arc::new(sess);
            self.refresh_open_glyph();
        }
    }

    fn back_to_overview(&mut self) {
        self.refresh_open_glyph();
        self.mode = Mode::Overview;
    }
}

fn titlebar(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let title = match app.mode {
        Mode::Overview => "Overview".to_string(),
        Mode::Editor(i) => app.font.glyphs.get(i).map(|g| g.name.clone()).unwrap_or_default(),
    };
    let editing = matches!(app.mode, Mode::Editor(_));
    flex_row((
        editing.then(|| {
            text_button("‹ Overview", |app: &mut App| app.back_to_overview())
                .background_color(pal.button)
        }),
        label(title).color(pal.text),
        text_button(if app.modified { "Save •" } else { "Save" }, |app: &mut App| app.save())
            .background_color(pal.button),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(12.0))
    .padding(Length::px(8.0))
    .background_color(pal.panel)
}

fn tool_palette(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let fg = pal.text_muted;
    let fg_active = pal.role("accent");
    let active_bg = pal.role("gridSelected").with_alpha(0.25);
    let hover_bg = pal.control;
    let tile = move |icon: &'static str, tool: Tool| {
        icon_button(icon, app.tool == tool, fg, fg_active, active_bg, hover_bg, move |app: &mut App| {
            app.tool = tool;
        })
    };
    flex_col((
        tile("select", Tool::Select),
        tile("pen", Tool::Pen),
        tile("hyperpen", Tool::HyperPen),
        tile("shape-rectangle", Tool::Rect),
        tile("shape-ellipse", Tool::Ellipse),
        tile("knife", Tool::Knife),
        tile("measure", Tool::Measure),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(4.0))
    .padding(Length::px(6.0))
    .background_color(pal.panel)
}

fn status(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let text = match app.mode {
        Mode::Overview => format!(
            "{} glyphs   {}",
            app.font.glyphs.len(),
            app.font.source.display()
        ),
        Mode::Editor(_) => format!(
            "{}   advance {}   {} points   {} selected",
            app.session.glyph_name.as_str(),
            app.session.advance(),
            app.session.point_count(),
            app.selected_points,
        ),
    };
    let text = if app.note.is_empty() {
        text
    } else {
        format!("{}   {}", text, app.note)
    };
    flex_row((label(text).color(pal.text_muted),))
        .padding(Length::px(8.0))
        .background_color(pal.panel)
}

fn sidebar(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let cats = [
        GlyphCategory::All,
        GlyphCategory::Letter,
        GlyphCategory::Number,
        GlyphCategory::Punctuation,
        GlyphCategory::Symbol,
        GlyphCategory::Mark,
        GlyphCategory::Other,
    ];
    let rows: Vec<_> = cats
        .into_iter()
        .filter(|c| app.category_count(*c) > 0)
        .map(|c| {
            let count = app.category_count(c);
            let active = app.category == c;
            text_button(
                format!("{}  {}", c.display_name(), count),
                move |app: &mut App| app.category = c,
            )
            .background_color(if active { pal.role("accent") } else { pal.panel })
        })
        .collect();
    flex_col((
        text_input(app.filter.clone(), |app: &mut App, v| app.filter = v)
            .placeholder("Search"),
        portal(flex_col(rows).gap(Length::px(2.0))).flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(8.0))
    .padding(Length::px(8.0))
    .background_color(pal.panel)
}

fn overview(app: &App) -> impl WidgetView<App> + use<> {
    let metrics = app.cell_metrics();
    grid(
        app.filtered_cells(),
        metrics,
        app.palette.clone(),
        app.selected,
        |app: &mut App, ev| match ev {
            GridEvent::Selected(i) => app.selected = Some(i),
            GridEvent::Open(i) => app.open_glyph(i),
        },
    )
}

fn preview_strip(app: &App) -> impl WidgetView<App> + use<> {
    use masonry::imaging::Painter;
    use masonry::kurbo::{Affine, Point, Size};
    let outline = app.session.outline_arc();
    let components = app.session.components_arc();
    let m = app.session.metrics;
    let advance = app.session.advance();
    let fill = app.palette.text;
    canvas(move |_app: &mut App, _ctx, scene, size: Size| {
        let mut p = Painter::new(scene);
        // Fit the em box (advance wide, ascender..descender tall) into the strip.
        let margin = 16.0;
        let em_w = advance.max(m.upm * 0.5);
        let em_h = m.ascender - m.descender;
        let scale = ((size.width - margin * 2.0) / em_w).min((size.height - margin * 2.0) / em_h);
        let baseline_y = margin + (m.ascender / em_h) * (size.height - margin * 2.0);
        let x0 = (size.width - em_w * scale) / 2.0;
        let t = Affine::new([scale, 0.0, 0.0, -scale, x0, baseline_y]);
        let _ = Point::ORIGIN;
        p.fill(&(t * (*outline).clone()), fill).draw();
        if !components.elements().is_empty() {
            p.fill(&(t * (*components).clone()), fill).draw();
        }
    })
}

fn editor_pane(app: &App) -> impl WidgetView<App> + use<> {
    editor(app.session.clone(), app.palette.clone(), app.tool, |app: &mut App, ev| match ev {
        editor::EditorEvent::Selection(n) => app.selected_points = n,
        editor::EditorEvent::Edited => app.refresh_open_glyph(),
        editor::EditorEvent::Save => app.save(),
        editor::EditorEvent::Exit => app.back_to_overview(),
    })
}

fn path_section(app: &App) -> impl WidgetView<App> + use<> {
    use icon_button::icon_button;
    use session::BoolOp;
    let pal = &app.palette;
    let fg = pal.text_muted;
    let fga = pal.role("accent");
    let abg = pal.role("gridSelected").with_alpha(0.25);
    let hbg = pal.control;
    let op = move |icon: &'static str, f: fn(&mut Session) -> bool| {
        icon_button(icon, false, fg, fga, abg, hbg, move |app: &mut App| app.apply_op(f))
    };
    flex_col((
        label("Path").text_size(15.0).color(pal.text),
        flex_row((
            op("flip-h", |s| s.flip_horizontal()),
            op("flip-v", |s| s.flip_vertical()),
            op("rot-cw", |s| s.rotate_90()),
        )).gap(Length::px(4.0)),
        flex_row((
            op("union", |s| s.remove_overlap()),
            op("subtract", |s| s.boolean(BoolOp::Subtract)),
            op("intersect", |s| s.boolean(BoolOp::Intersect)),
            op("exclude", |s| s.boolean(BoolOp::Exclude)),
        )).gap(Length::px(4.0)),
        flex_row((
            op("close", |s| s.decompose()),
        )).gap(Length::px(4.0)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(6.0))
}

fn selection_section(app: &App) -> Option<impl WidgetView<App> + use<>> {
    let pal = &app.palette;
    let b = app.session.selection_bounds()?;
    let row = |k: &'static str, v: String| {
        flex_row((label(k).color(pal.text_muted), label(v).color(pal.text))).gap(Length::px(8.0))
    };
    Some(
        flex_col((
            label("Selection").text_size(15.0).color(pal.text),
            row("X", format!("{}", b.x0 as i64)),
            row("Y", format!("{}", b.y0 as i64)),
            row("W", format!("{}", b.width() as i64)),
            row("H", format!("{}", b.height() as i64)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(4.0)),
    )
}

fn info_panel(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let row = |k: String, v: String| {
        flex_row((
            label(k).color(pal.text_muted),
            label(v).color(pal.text),
        ))
        .gap(Length::px(8.0))
    };
    let (name, adv, pts, cp) = match app.mode {
        Mode::Editor(_) => (
            app.session.glyph_name.clone(),
            format!("{}", app.session.advance() as i64),
            format!("{}", app.session.point_count()),
            String::new(),
        ),
        Mode::Overview => {
            let g = app.selected.and_then(|i| app.font.glyphs.get(i));
            (
                g.map(|g| g.name.clone()).unwrap_or_default(),
                g.map(|g| format!("{}", g.advance as i64)).unwrap_or_default(),
                String::new(),
                g.and_then(|g| g.codepoint).map(|c| format!("U+{:04X}", c as u32)).unwrap_or_default(),
            )
        }
    };
    let editing = matches!(app.mode, Mode::Editor(_));
    let advance_field = editing.then(|| {
        flex_col((
            label("Advance").color(pal.text_muted),
            text_input(app.advance_buf.clone(), |app: &mut App, v| app.set_advance_from_buf(v))
                .background_color(pal.field()),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(4.0))
    });
    let name_field = editing.then(|| {
        flex_col((
            label("Name").color(pal.text_muted),
            text_input(app.name_buf.clone(), |app: &mut App, v| app.name_buf = v)
                .on_enter(|app: &mut App, _| app.commit_rename())
                .background_color(pal.field()),
            label("Unicode").color(pal.text_muted),
            text_input(app.unicode_buf.clone(), |app: &mut App, v| app.set_unicode_from_buf(v))
                .background_color(pal.field()),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(4.0))
    });
    flex_col((
        label("Glyph").text_size(15.0).color(pal.text),
        (!editing).then(|| row("Name".into(), name)),
        (!editing).then(|| row("Unicode".into(), cp.clone())),
        name_field,
        (!editing).then(|| row("Advance".into(), adv)),
        advance_field,
        (!pts.is_empty()).then(|| row("Points".into(), pts)),
        editing.then(|| row("Selected".into(), format!("{}", app.selected_points))),
        editing.then(|| selection_section(app)).flatten(),
        editing.then(|| path_section(app)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(6.0))
    .padding(Length::px(12.0))
    .background_color(pal.panel)
}

fn app_logic(app: &mut App) -> impl WidgetView<App> + use<> {
    use xilem::core::one_of::Either;
    let pal = &app.palette;

    // Left column: category sidebar in overview, tool palette in editor.
    let left = match app.mode {
        Mode::Overview => Either::A(sidebar(app)),
        Mode::Editor(_) => Either::B(tool_palette(app)),
    };
    let left_width = match app.mode {
        Mode::Overview => 200.0,
        Mode::Editor(_) => 44.0,
    };

    // Center: title bar + body + status bar.
    let body = match app.mode {
        Mode::Overview => Either::A(overview(app)),
        Mode::Editor(_) => Either::B(editor_pane(app)),
    };
    let preview = matches!(app.mode, Mode::Editor(_))
        .then(|| sized_box(preview_strip(app)).dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(120.0)))).background_color(pal.panel));
    let center = flex_col((titlebar(app), body.flex(1.0), preview, status(app)))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(0.0))
        .background_color(pal.app);

    flex_row((
        sized_box(left)
            .dims(Dimensions::new(Dim::Fixed(Length::px(left_width)), Dim::Stretch))
            .background_color(pal.panel),
        sized_box(center)
            .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
            .background_color(pal.app)
            .flex(1.0),
        sized_box(info_panel(app))
            .dims(Dimensions::new(Dim::Fixed(Length::px(220.0)), Dim::Stretch))
            .background_color(pal.panel),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(0.0))
    .background_color(pal.app)
}

fn run(event_loop: EventLoopBuilder) -> Result<(), EventLoopError> {
    let path = std::env::args()
        .nth(1)
        .expect("usage: runebender-xix <Font.ufo|Font.designspace>");
    let mut app = App::open(FsPath::new(&path)).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    if std::env::var("RUNEBENDER_SELECTALL").is_ok() {
        let mut sess = (*app.session).clone();
        sess.select_all();
        app.selected_points = sess.selection_bounds().map(|_| 999).unwrap_or(0);
        let n = { let mut c = 0; for co in &sess.glyph.contours { c += co.points.len(); } c };
        app.selected_points = n;
        app.session = std::sync::Arc::new(sess);
    }
    let background = app.palette.app;
    let window_options =
        WindowOptions::new("Runebender").with_initial_inner_size(LogicalSize::new(1100., 720.));
    Xilem::new_simple(app, app_logic, window_options)
        .with_default_properties(default_property_set())
        .with_default_base_color(background)
        .run_in(event_loop)
}


fn main() -> Result<(), EventLoopError> {
    run(EventLoop::with_user_event())
}
