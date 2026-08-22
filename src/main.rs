// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Runebender on xix. A font editor: glyph grid, glyph editor, sidebar.
//! See PORT.md for what each slice forced into the framework.

mod editor;
mod grid;
mod model;
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
    FlexExt as _, flex_col, flex_row, label, text_button,
};
use xilem::{EventLoop, EventLoopBuilder, WidgetView, WindowOptions, Xilem};

use editor::{Session, editor};
use grid::{Cell, CellMetrics, GridEvent, cells_of, grid};
use model::FontModel;
use theme::Palette;

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
    // Editor session, when a glyph is open.
    session: Arc<Session>,
    selected_points: usize,
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
        Ok(Self {
            font,
            palette,
            cells,
            mode: Mode::Overview,
            selected: Some(first),
            filter: String::new(),
            session,
            selected_points: 0,
        })
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

    fn back_to_overview(&mut self) {
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
    let back = matches!(app.mode, Mode::Editor(_));
    flex_row((
        back.then(|| {
            text_button("‹ Overview", |app: &mut App| app.back_to_overview())
                .background_color(pal.button)
        }),
        label(title).color(pal.text),
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
            app.session.glyph_name(),
            app.session.advance(),
            app.session.point_count(),
            app.selected_points,
        ),
    };
    flex_row((label(text).color(pal.text_muted),))
        .padding(Length::px(8.0))
        .background_color(pal.panel)
}

fn overview(app: &App) -> impl WidgetView<App> + use<> {
    let metrics = app.cell_metrics();
    grid(
        app.cells.clone(),
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
    editor(app.session.clone(), app.palette.clone(), |app: &mut App, ev| {
        app.selected_points = ev.selected;
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
    let app = App::open(FsPath::new(&path)).unwrap_or_else(|e| {
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
