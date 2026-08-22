// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Runebender on xix. A font editor: glyph grid, glyph editor, sidebar.
//! See PORT.md for what each slice forced into the framework.

mod editor;
mod grid;
mod model;
mod session;
mod text_label;
mod theme;

use std::path::Path as FsPath;
use std::sync::Arc;

use masonry::layout::Length;
use masonry::properties::types::CrossAxisAlignment;
use masonry::theme::default_property_set;
use winit::dpi::LogicalSize;
use winit::error::EventLoopError;
use xilem::style::Style;
use xilem::view::{
    FlexExt as _, flex_col, flex_row, label, portal, sized_box, text_button, text_input,
};
use xilem::{EventLoop, EventLoopBuilder, WidgetView, WindowOptions, Xilem};

use editor::editor;
use grid::{Cell, CellMetrics, GridEvent, cells_of, grid};
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
        // For headless screenshots: open a named glyph at startup.
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
        // Headless pen check: draw a triangle into the open glyph's session.
        let session = if std::env::var("RUNEBENDER_DEMO_PEN").is_ok() {
            use masonry::kurbo::Point;
            let mut s = (*session).clone();
            s.pen_corner(150.0, 0.0);
            // A smooth point with handles: down at (350,500), drag out to (500,500).
            s.pen_smooth_begin(Point::new(350.0, 500.0), Point::new(500.0, 500.0));
            s.pen_smooth_drag(Point::new(350.0, 500.0), Point::new(500.0, 500.0));
            s.pen_corner(550.0, 0.0);
            s.pen_close();
            Arc::new(s)
        } else {
            session
        };
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
                self.session = Arc::new(session);
                self.selected = Some(index);
                self.selected_points = 0;
                self.mode = Mode::Editor(index);
            }
        }
    }

    /// After an edit, pull the glyph back out of the session and refresh
    /// the model + grid cache so the overview preview matches.
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

    fn back_to_overview(&mut self) {
        self.refresh_open_glyph();
        self.mode = Mode::Overview;
    }
}

fn toolbar(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let title = match app.mode {
        Mode::Overview => "Overview".to_string(),
        Mode::Editor(i) => app
            .font
            .glyphs
            .get(i)
            .map(|g| g.name.clone())
            .unwrap_or_default(),
    };
    let editing = matches!(app.mode, Mode::Editor(_));
    let tool_btn = |name: &'static str, tool: Tool, active: bool| {
        text_button(name, move |app: &mut App| app.tool = tool)
            .background_color(if active { pal.role("accent") } else { pal.button })
    };
    flex_row((
        editing.then(|| {
            text_button("‹ Overview", |app: &mut App| app.back_to_overview())
                .background_color(pal.button)
        }),
        label(title).color(pal.text),
        editing.then(|| tool_btn("Select", Tool::Select, app.tool == Tool::Select)),
        editing.then(|| tool_btn("Pen", Tool::Pen, app.tool == Tool::Pen)),
        editing.then(|| tool_btn("Rect", Tool::Rect, app.tool == Tool::Rect)),
        editing.then(|| tool_btn("Ellipse", Tool::Ellipse, app.tool == Tool::Ellipse)),
        editing.then(|| tool_btn("Knife", Tool::Knife, app.tool == Tool::Knife)),
        editing.then(|| tool_btn("Measure", Tool::Measure, app.tool == Tool::Measure)),
        Some(
            text_button(if app.modified { "Save •" } else { "Save" }, |app: &mut App| {
                app.save()
            })
            .background_color(pal.button),
        ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(12.0))
    .padding(Length::px(8.0))
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
    let grid_view = grid(
        app.filtered_cells(),
        metrics,
        app.palette.clone(),
        app.selected,
        |app: &mut App, ev| match ev {
            GridEvent::Selected(i) => app.selected = Some(i),
            GridEvent::Open(i) => app.open_glyph(i),
        },
    );
    flex_row((
        sized_box(sidebar(app)).fixed_width(Length::px(180.0)),
        grid_view.flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
}

fn editor_pane(app: &App) -> impl WidgetView<App> + use<> {
    editor(app.session.clone(), app.palette.clone(), app.tool, |app: &mut App, ev| match ev {
        editor::EditorEvent::Selection(n) => app.selected_points = n,
        editor::EditorEvent::Edited => app.refresh_open_glyph(),
        editor::EditorEvent::Save => app.save(),
        editor::EditorEvent::Exit => app.back_to_overview(),
    })
}

fn app_logic(app: &mut App) -> impl WidgetView<App> + use<> {
    use xilem::core::one_of::Either;
    let _ = &app.filter;
    let body = match app.mode {
        Mode::Overview => Either::A(overview(app)),
        Mode::Editor(_) => Either::B(editor_pane(app)),
    };
    flex_col((toolbar(app), body.flex(1.0), status(app)))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(0.0))
}

fn run(event_loop: EventLoopBuilder) -> Result<(), EventLoopError> {
    let path = std::env::args()
        .nth(1)
        .expect("usage: runebender-xix <Font.ufo|Font.designspace>");
    let mut app = App::open(FsPath::new(&path)).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    if std::env::var("RUNEBENDER_SAVE").is_ok() {
        app.save();
        println!("SAVE_RESULT: {}", app.note);
        return Ok(());
    }
    if std::env::var("RUNEBENDER_DEMO_SHAPE").is_ok() {
        if let Mode::Editor(i) = app.mode {
            let mut sess = (*app.session).clone();
            sess.add_rect(100.0, 0.0, 400.0, 300.0);
            sess.add_ellipse(450.0, 0.0, 750.0, 300.0);
            app.session = Arc::new(sess);
            let g = app.session.glyph.clone();
            app.font.replace_glyph(i, g);
        }
    }
    if std::env::var("RUNEBENDER_DEMO_BOOL").is_ok() {
        if let Mode::Editor(i) = app.mode {
            let mut sess = (*app.session).clone();
            sess.add_rect(100.0, 0.0, 400.0, 300.0);
            sess.add_ellipse(250.0, 150.0, 600.0, 500.0);
            sess.remove_overlap();
            app.session = Arc::new(sess);
            let g = app.session.glyph.clone();
            app.font.replace_glyph(i, g);
        }
    }
    if std::env::var("RUNEBENDER_DEMO_DECOMP").is_ok() {
        if let Mode::Editor(i) = app.mode {
            let mut sess = (*app.session).clone();
            let had = sess.glyph.components.len();
            let ok = sess.decompose();
            eprintln!("DECOMP: components={had} decomposed={ok} contours={}", sess.glyph.contours.len());
            app.session = Arc::new(sess);
            let g = app.session.glyph.clone();
            app.font.replace_glyph(i, g);
        }
    }
    if std::env::var("RUNEBENDER_DEMO_KNIFE").is_ok() {
        if let Mode::Editor(i) = app.mode {
            use masonry::kurbo::Point;
            let mut sess = (*app.session).clone();
            let before = sess.point_count();
            let ok = sess.knife_cut(Point::new(-100.0, 350.0), Point::new(1600.0, 350.0));
            eprintln!("KNIFE: cut={ok} points {before} -> {}", sess.point_count());
            app.session = Arc::new(sess);
            let g = app.session.glyph.clone();
            app.font.replace_glyph(i, g);
        }
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
