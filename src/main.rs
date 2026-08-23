// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Runebender on xix. A font editor: glyph grid, glyph editor, sidebar.
//! See PORT.md for what each slice forced into the framework.

mod editor;
mod grid;
mod icon_button;
mod model;
mod session;
mod shortcuts;
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
    FlexExt as _, FlexSpacer, canvas, flex_col, flex_row, label, portal, sized_box, slider,
    text_button, text_input,
};
use xilem::{EventLoop, EventLoopBuilder, WidgetView, WindowOptions, Xilem};

use editor::editor;
use grid::{Cell, CellMetrics, GridEvent, cells_of, grid};
use icon_button::icon_button;
use model::FontModel;
use runebender_core::category::GlyphCategory;
use session::Session;
use theme::Palette;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Name,
    Unicode,
}

/// The active editor tool.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
    multi_selected: std::sync::Arc<std::collections::HashSet<usize>>,
    filter: String,
    category: GlyphCategory,
    sort: Sort,
    // Editor session, when a glyph is open.
    session: Arc<Session>,
    selected_points: usize,
    tool: Tool,
    modified: bool,
    note: String,
    show_comb: bool,
    advance_buf: String,
    name_buf: String,
    unicode_buf: String,
    /// Current axis location in user units, one per designspace axis.
    axis_values: Vec<f64>,
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
        // Snap sliders to the active master's location (Glyphs behavior), so
        // opening a master shows no interpolation overlay until you move one.
        let mut axis_values: Vec<f64> = if font.axes.is_empty() {
            Vec::new()
        } else {
            font.master_axis_values(font.active)
        };
        // Headless override: RUNEBENDER_AXIS="wght=500,wdth=80".
        if let Ok(spec) = std::env::var("RUNEBENDER_AXIS") {
            for pair in spec.split(',') {
                if let Some((tag, val)) = pair.split_once('=') {
                    if let Ok(v) = val.trim().parse::<f64>() {
                        if let Some(i) = font.axes.iter().position(|a| a.tag == tag.trim() || a.name == tag.trim()) {
                            axis_values[i] = v.clamp(font.axes[i].min, font.axes[i].max);
                        }
                    }
                }
            }
        }
        // Seed the Name/Unicode fields from the glyph actually shown
        // (the opened one in editor mode, else the first).
        let shown = open.unwrap_or(first);
        let first_name = font.glyphs[shown].name.clone();
        let first_uni = font.glyphs[shown]
            .codepoint
            .map(|c| format!("{:04X}", c as u32))
            .unwrap_or_default();
        Ok(Self {
            font,
            palette,
            cells,
            mode,
            selected: Some(open.unwrap_or(first)),
            multi_selected: std::sync::Arc::new(std::collections::HashSet::new()),
            filter: String::new(),
            category: match start_cat.as_deref() {
                Some("Number") => GlyphCategory::Number,
                Some("Symbol") => GlyphCategory::Symbol,
                Some("Mark") => GlyphCategory::Mark,
                _ => GlyphCategory::All,
            },
            sort: Sort::Name,
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
            show_comb: false,
            axis_values,
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
        let mut out = out;
        match self.sort {
            Sort::Name => {}
            Sort::Unicode => out.sort_by_key(|c| c.codepoint.map(|cp| cp as u32).unwrap_or(u32::MAX)),
        }
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

    fn new_glyph(&mut self) {
        let name = self.filter.trim().to_string();
        let upm = self.font.units_per_em;
        if self.font.add_glyph(&name, (upm * 0.5).round()) {
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            self.filter.clear();
            if let Some(i) = self.font.index_of(&name) {
                self.open_glyph(i);
            }
            self.modified = true;
        }
    }

    fn grid_select(&mut self, index: usize, cmd: bool, shift: bool) {
        use std::collections::HashSet;
        if cmd {
            let mut m: HashSet<usize> = (*self.multi_selected).clone();
            if !m.remove(&index) {
                m.insert(index);
            }
            self.multi_selected = std::sync::Arc::new(m);
        } else if shift {
            // Range from the current single selection to this index, in cell order.
            let cells = self.filtered_cells();
            let order: Vec<usize> = cells.iter().map(|c| c.index).collect();
            let a = self.selected.and_then(|s| order.iter().position(|&i| i == s));
            let b = order.iter().position(|&i| i == index);
            if let (Some(a), Some(b)) = (a, b) {
                let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                let m: HashSet<usize> = order[lo..=hi].iter().copied().collect();
                self.multi_selected = std::sync::Arc::new(m);
            }
        } else {
            self.multi_selected = std::sync::Arc::new(HashSet::new());
        }
        self.selected = Some(index);
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
        // Keep the panel's advance field in step after canvas edits
        // (sidebearing/advance drags). This path is never hit by typing in
        // the field, so it does not clobber input.
        self.advance_buf = format!("{}", self.session.advance() as i64);
        self.selected_points = self.session.selection.len();
    }

    fn set_master(&mut self, index: usize) {
        if index == self.font.active {
            return;
        }
        self.font.set_active(index);
        self.axis_values = self.font.master_axis_values(index);
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        // Reopen the current glyph in the new master, keeping the viewport.
        if let Mode::Editor(i) = self.mode {
            if let Some(entry) = self.font.glyphs.get(i) {
                if let Some(sess) = Session::new(&self.font.font, &entry.name) {
                    self.session = Arc::new(sess);
                }
            } else if let Some(idx) = self
                .selected
                .and_then(|_| self.font.index_of(&self.session.glyph_name))
            {
                self.mode = Mode::Editor(idx);
                if let Some(sess) = Session::new(&self.font.font, &self.session.glyph_name.clone()) {
                    self.session = Arc::new(sess);
                }
            }
        }
    }

    /// The current axis location as a name->value map (user units).
    fn axis_location(&self) -> std::collections::HashMap<String, f64> {
        self.font
            .axes
            .iter()
            .zip(&self.axis_values)
            .map(|(a, v)| (a.name.clone(), *v))
            .collect()
    }

    /// True when the sliders sit exactly on the active master's location.
    fn on_active_master(&self) -> bool {
        match self.font.master_locations.get(self.font.active) {
            Some(m) => self.font.axes.iter().enumerate().all(|(i, a)| {
                // axis_values are user coords; master locations are design coords.
                let cur = a.user_to_design(self.axis_values.get(i).copied().unwrap_or(a.default));
                let mst = m.get(&a.name).copied().unwrap_or_else(|| a.user_to_design(a.default));
                (cur - mst).abs() < 1e-6
            }),
            None => true,
        }
    }

    /// The interpolated instance outline at the current axis location, shown as
    /// a read-only overlay. `None` on a master (the editable outline is enough)
    /// or when the glyph is not interpolatable.
    fn interp_preview(&self) -> Option<Arc<masonry::kurbo::BezPath>> {
        if self.on_active_master() {
            return None;
        }
        self.font
            .interpolate_outline(&self.session.glyph_name, &self.axis_location())
            .map(Arc::new)
    }

    fn set_axis(&mut self, index: usize, value: f64) {
        if let Some(v) = self.axis_values.get_mut(index) {
            *v = value;
        }
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

    pub fn dispatch(&mut self, action: shortcuts::AppAction) {
        use shortcuts::AppAction as A;
        match action {
            A::Save => self.save(),
            A::Overview => {
                if matches!(self.mode, Mode::Editor(_)) {
                    self.back_to_overview();
                }
            }
            A::Tool(t) => self.tool = t,
            A::FlipHorizontal => self.apply_op(|s| s.flip_horizontal()),
            A::FlipVertical => self.apply_op(|s| s.flip_vertical()),
            A::Rotate90 => self.apply_op(|s| s.rotate_90()),
            A::RemoveOverlap => self.apply_op(|s| s.remove_overlap()),
            A::Decompose => self.apply_op(|s| s.decompose()),
            A::Duplicate => self.apply_op(|s| s.duplicate()),
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

    fn set_mark(&mut self, label: Option<String>) {
        if !self.multi_selected.is_empty() {
            self.apply_mark_to_selection(label);
            return;
        }
        let mut sess = (*self.session).clone();
        sess.set_mark(label.as_deref());
        self.session = Arc::new(sess);
        self.refresh_open_glyph();
    }

    fn apply_mark_to_selection(&mut self, label: Option<String>) {
        let indices: Vec<usize> = self.multi_selected.iter().copied().collect();
        for i in indices {
            if let Some(entry) = self.font.glyphs.get(i) {
                if let Some(mut g) = self.font.font.get_glyph(&entry.name).cloned() {
                    runebender_core::theme_oklch::set_glyph_mark(&mut g, label.as_deref());
                    self.font.replace_glyph(i, g);
                }
            }
        }
        self.modified = true;
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

fn master_bar(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let buttons: Vec<_> = app
        .font
        .master_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let active = i == app.font.active;
            text_button(name.clone(), move |app: &mut App| app.set_master(i))
                .background_color(if active { pal.role("accent") } else { pal.button })
        })
        .collect();
    portal(flex_row(buttons).gap(Length::px(4.0)))
        .background_color(pal.panel)
}

fn axes_bar(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let on_master = app.on_active_master();
    let rows: Vec<_> = app
        .font
        .axes
        .iter()
        .enumerate()
        .map(|(i, ax)| {
            let value = app.axis_values.get(i).copied().unwrap_or(ax.default);
            flex_row((
                label(ax.tag.clone()).color(pal.text_muted),
                slider(ax.min, ax.max, value, move |app: &mut App, v| app.set_axis(i, v)).width(Length::px(160.0)),
                label(format!("{value:.0}")).color(pal.text),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(Length::px(8.0))
        })
        .collect();
    // A short hint when the location is interpolated (off any master).
    let hint = (!on_master).then(|| label("interpolated").color(pal.role("warning")));
    portal(
        flex_row((flex_row(rows).gap(Length::px(18.0)), hint))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(Length::px(18.0))
            .padding(Length::px(6.0)),
    )
    .background_color(pal.panel)
}

fn titlebar(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let editing = matches!(app.mode, Mode::Editor(_));
    let filename = app
        .font
        .source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = match app.mode {
        Mode::Overview => filename.clone(),
        Mode::Editor(i) => app.font.glyphs.get(i).map(|g| g.name.clone()).unwrap_or_default(),
    };
    // Save status, gpui-style: yellow when unsaved, muted otherwise.
    let (save_text, save_color) = if app.modified {
        ("Not saved", pal.role("warning"))
    } else {
        ("Saved", pal.text_muted)
    };
    flex_row((
        editing.then(|| {
            text_button("‹ Overview", |app: &mut App| app.back_to_overview())
                .background_color(pal.button)
        }),
        label(title).text_size(14.0).color(pal.text),
        editing.then(|| label(filename).color(pal.text_muted)),
        FlexSpacer::Flex(1.0),
        editing.then(|| header_tools(app)),
        editing.then(|| FlexSpacer::Fixed(Length::px(12.0))),
        label(save_text.to_string()).color(save_color),
        text_button("Save", |app: &mut App| app.save()).background_color(pal.button),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(12.0))
    .padding(Length::px(8.0))
    .background_color(pal.panel)
}

/// The tools as a horizontal row for the header (gpui puts them there,
/// not in a left column).
fn header_tools(app: &App) -> impl WidgetView<App> + use<> {
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
    flex_row((
        tile("select", Tool::Select),
        tile("pen", Tool::Pen),
        tile("hyperpen", Tool::HyperPen),
        tile("shape-rectangle", Tool::Rect),
        tile("shape-ellipse", Tool::Ellipse),
        tile("knife", Tool::Knife),
        tile("measure", Tool::Measure),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(2.0))
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
    let sort_label = match app.sort { Sort::Name => "Sort: name", Sort::Unicode => "Sort: unicode" };
    flex_col((
        text_input(app.filter.clone(), |app: &mut App, v| app.filter = v)
            .placeholder("Search"),
        text_button(sort_label, |app: &mut App| {
            app.sort = match app.sort { Sort::Name => Sort::Unicode, Sort::Unicode => Sort::Name };
        }).background_color(pal.button),
        {
            let fresh = !app.filter.trim().is_empty() && app.font.index_of(app.filter.trim()).is_none();
            fresh.then(|| text_button(format!("+ New {}", app.filter.trim()), |app: &mut App| app.new_glyph())
                .background_color(pal.role("accent")))
        },
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
        app.multi_selected.clone(),
        |app: &mut App, ev| match ev {
            GridEvent::Selected { index, cmd, shift } => app.grid_select(index, cmd, shift),
            GridEvent::Open(i) => app.open_glyph(i),
        },
    )
}

fn preview_strip(app: &App) -> impl WidgetView<App> + use<> {
    use masonry::imaging::Painter;
    use masonry::kurbo::{Affine, Point, Size};
    // When the axis sliders are off a master, preview the interpolated
    // instance (in warm amber) so the strip reflects the current location.
    let interp = app.interp_preview();
    let outline = match &interp {
        Some(o) => o.clone(),
        None => app.session.outline_arc(),
    };
    let components = app.session.components_arc();
    let has_components = !interp.is_some() && !components.elements().is_empty();
    let m = app.session.metrics;
    let advance = app.session.advance();
    let fill = if interp.is_some() {
        app.palette.role("warning")
    } else {
        app.palette.text
    };
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
        if has_components {
            p.fill(&(t * (*components).clone()), fill).draw();
        }
    })
}

fn editor_pane(app: &App) -> impl WidgetView<App> + use<> {
    let ghosts = Arc::new(app.font.ghost_outlines(&app.session.glyph_name));
    let interp = app.interp_preview();
    editor(app.session.clone(), app.palette.clone(), app.tool, app.show_comb, ghosts, interp, |app: &mut App, ev| match ev {
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

fn mark_section(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let swatch = |label: Option<String>, color: xilem::Color| {
        sized_box(
            text_button("", move |app: &mut App| app.set_mark(label.clone()))
                .background_color(color),
        )
        .dims(Dimensions::fixed(Length::px(22.0), Length::px(22.0)))
    };
    let marks: Vec<_> = app.palette.mark_list().into_iter().map(|(name, color)| swatch(Some(name), color)).collect();
    flex_col((
        label("Mark").text_size(15.0).color(pal.text),
        flex_row((
            swatch(None, pal.control),
        )).gap(Length::px(4.0)),
        flex_row(marks).gap(Length::px(4.0)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0))
}

fn info_panel(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let (muted, text) = (pal.text_muted, pal.text);
    // Compact read-only row: label left, value right (gpui's panel rows).
    let row = move |k: String, v: String| {
        sized_box(
            flex_row((
                label(k).color(muted),
                FlexSpacer::Flex(1.0),
                label(v).color(text),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center),
        )
        .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(18.0))))
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
    let show_multi_mark = !editing && !app.multi_selected.is_empty();
    let master_row = (app.font.master_names.len() > 1)
        .then(|| row("Master".into(), app.font.master_names[app.font.active].clone()));
    flex_col((
        label("Glyph").text_size(15.0).color(pal.text),
        master_row,
        show_multi_mark.then(|| row("Selected".into(), format!("{}", app.multi_selected.len()))),
        show_multi_mark.then(|| mark_section(app)),
        (!editing).then(|| row("Name".into(), name)),
        (!editing).then(|| row("Unicode".into(), cp.clone())),
        name_field,
        (!editing).then(|| row("Advance".into(), adv)),
        advance_field,
        (!pts.is_empty()).then(|| row("Points".into(), pts)),
        editing.then(|| row("Selected".into(), format!("{}", app.selected_points))),
        editing.then(|| selection_section(app)).flatten(),
        editing.then(|| path_section(app)),
        editing.then(|| {
            flex_col((
                label("Curves").text_size(15.0).color(pal.text),
                text_button(if app.show_comb { "Comb: on" } else { "Comb: off" }, |app: &mut App| app.show_comb = !app.show_comb)
                    .background_color(pal.button),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Length::px(4.0))
        }),
        editing.then(|| mark_section(app)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(6.0))
    .padding(Length::px(12.0))
    .background_color(pal.panel)
}

fn app_logic(app: &mut App) -> impl WidgetView<App> + use<> {
    use xilem::core::one_of::Either;
    let pal = &app.palette;

    // Left column: category sidebar in overview only. In the editor the
    // tools live in the header (gpui-style), so the left column collapses.
    let editing_mode = matches!(app.mode, Mode::Editor(_));
    let _ = &app.multi_selected;

    // Center: title bar + body + status bar.
    let body = match app.mode {
        Mode::Overview => Either::A(overview(app)),
        Mode::Editor(_) => Either::B(editor_pane(app)),
    };
    let preview = matches!(app.mode, Mode::Editor(_))
        .then(|| sized_box(preview_strip(app)).dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(120.0)))).background_color(pal.panel));
    let has_masters = app.font.master_names.len() > 1;
    let center = flex_col((
        titlebar(app),
        has_masters.then(|| sized_box(master_bar(app)).dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(30.0))))),
        (has_masters && !app.font.axes.is_empty() && matches!(app.mode, Mode::Editor(_)))
            .then(|| sized_box(axes_bar(app)).dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(34.0))))),
        body.flex(1.0),
        preview,
        status(app),
    ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(0.0))
        .background_color(pal.app);

    shortcuts::shortcut_host(flex_row((
        (!editing_mode).then(|| {
            sized_box(sidebar(app))
                .dims(Dimensions::new(Dim::Fixed(Length::px(200.0)), Dim::Stretch))
                .background_color(pal.panel)
        }),
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
    .background_color(pal.app))
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
