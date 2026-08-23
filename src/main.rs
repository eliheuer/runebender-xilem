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
    FlexExt as _, flex_col, flex_row, label, portal, sized_box, text_button, text_input,
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

fn editor_pane(app: &App) -> impl WidgetView<App> + use<> {
    editor(app.session.clone(), app.palette.clone(), app.tool, |app: &mut App, ev| match ev {
        editor::EditorEvent::Selection(n) => app.selected_points = n,
        editor::EditorEvent::Edited => app.refresh_open_glyph(),
        editor::EditorEvent::Save => app.save(),
        editor::EditorEvent::Exit => app.back_to_overview(),
    })
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
    flex_col((
        label("Glyph").text_size(15.0).color(pal.text),
        row("Name".into(), name),
        (!cp.is_empty()).then(|| row("Unicode".into(), cp)),
        row("Advance".into(), adv),
        (!pts.is_empty()).then(|| row("Points".into(), pts)),
        matches!(app.mode, Mode::Editor(_)).then(|| row("Selected".into(), format!("{}", app.selected_points))),
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
    let center = flex_col((titlebar(app), body.flex(1.0), status(app)))
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
