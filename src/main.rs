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
mod ui;

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
    FlexExt as _, FlexSpacer, button, canvas, flex_col, flex_row, label, portal, sized_box,
    slider, text_button, text_input,
};
use xilem::{EventLoop, EventLoopBuilder, WidgetView, WindowOptions, Xilem};

use editor::editor;
use grid::{Cell, CellMetrics, GridEvent, cells_of, grid};
use icon_button::icon_button;
use model::FontModel;
use runebender_core::category::GlyphCategory;
use session::Session;
use theme::Palette;
use ui::{CONTROL_H, ROW_H, Space, Type, section_header};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Name,
    Unicode,
}

/// The active sidebar selection: a category chip, a language group, or a
/// builtin/GF-coverage filter (mirrors runebender-gpui's SidebarFilter).
#[derive(Clone, Copy, PartialEq)]
enum Sel {
    Category(GlyphCategory),
    Language(usize),
    Filter(usize),
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
    sel: Sel,
    sort: Sort,
    // Editor session, when a glyph is open.
    session: Arc<Session>,
    selected_points: usize,
    tool: Tool,
    modified: bool,
    note: String,
    show_comb: bool,
    advance_buf: String,
    lsb_buf: String,
    rsb_buf: String,
    name_buf: String,
    unicode_buf: String,
    /// Current axis location in user units, one per designspace axis.
    axis_values: Vec<f64>,
    /// Active OKLCH theme id (dark | midnight | gray | light).
    theme_id: &'static str,
    /// Reference corner for the Coordinates fields (the 9-point picker).
    coord_quadrant: runebender_core::path::Quadrant,
    coord_x_buf: String,
    coord_y_buf: String,
    /// Search scope: 0 name and unicode, 1 name only, 2 unicode only.
    search_mode: u8,
    /// Case-sensitive search.
    search_case: bool,
    /// Masters drawn as ghost outlines under the active one. The Layers
    /// section toggles these, one per thumbnail click (gpui's eye).
    reference_layers: std::collections::HashSet<usize>,
}

impl App {
    fn open(path: &FsPath) -> Result<Self, String> {
        let font = FontModel::open(path)?;
        let theme_id: &'static str = match std::env::var("RUNEBENDER_THEME").ok().as_deref() {
            Some("midnight") => "midnight",
            Some("gray") => "gray",
            Some("light") => "light",
            _ => "dark",
        };
        let palette = Arc::new(Palette::load(theme_id));
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
            sel: Sel::Category(match start_cat.as_deref() {
                Some("Number") => GlyphCategory::Number,
                Some("Symbol") => GlyphCategory::Symbol,
                Some("Mark") => GlyphCategory::Mark,
                _ => GlyphCategory::All,
            }),
            sort: Sort::Name,
            advance_buf: format!("{}", session.advance() as i64),
            lsb_buf: metric_bufs(&session).0,
            rsb_buf: metric_bufs(&session).1,
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
            theme_id,
            coord_quadrant: runebender_core::path::Quadrant::Center,
            coord_x_buf: String::new(),
            coord_y_buf: String::new(),
            search_mode: 0,
            search_case: false,
            reference_layers: std::collections::HashSet::new(),
        })
    }

    /// The cells that pass the current search + category filter. The two
    /// toggles beside the search box set the scope (name, unicode, both)
    /// and whether the match is case-sensitive.
    fn filtered_cells(&self) -> Arc<Vec<Cell>> {
        let q = if self.search_case {
            self.filter.clone()
        } else {
            self.filter.to_lowercase()
        };
        let by_name = self.search_mode != 2;
        let by_unicode = self.search_mode != 1;
        let out: Vec<Cell> = self
            .cells
            .iter()
            .filter(|c| {
                let cat_ok = self.cell_matches_sel(c.index);
                let name_hit = by_name
                    && if self.search_case {
                        c.name.contains(&q)
                    } else {
                        c.name.to_lowercase().contains(&q)
                    };
                let uni_hit = by_unicode
                    && c
                        .codepoint
                        .map(|cp| format!("{:04x}", cp as u32).contains(q.to_lowercase().trim_start_matches("u+").trim_start_matches("0x")))
                        .unwrap_or(false);
                let q_ok = q.is_empty() || name_hit || uni_hit;
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

    /// Codepoints of a glyph entry (the cache keeps only the first).
    fn entry_codepoints(entry: &model::GlyphEntry) -> Vec<u32> {
        entry.codepoint.map(|c| vec![c as u32]).unwrap_or_default()
    }

    /// Does the glyph at `index` pass the active sidebar selection?
    fn cell_matches_sel(&self, index: usize) -> bool {
        use runebender_core::sidebar as sb;
        let entry = &self.font.glyphs[index];
        match self.sel {
            Sel::Category(GlyphCategory::All) => true,
            Sel::Category(cat) => entry.category == cat,
            Sel::Language(i) => sb::language_groups()
                .get(i)
                .map(|g| sb::glyph_matches_language_group(&entry.name, &Self::entry_codepoints(entry), g))
                .unwrap_or(false),
            Sel::Filter(i) => sb::builtin_filters()
                .get(i)
                .and_then(|b| b.glyphset.as_ref())
                .map(|f| sb::glyph_matches_character_filter(&entry.name, &Self::entry_codepoints(entry), f))
                .unwrap_or(false),
        }
    }

    /// How many glyphs in the font match language group `i`.
    fn language_count(&self, i: usize) -> usize {
        use runebender_core::sidebar as sb;
        let Some(g) = sb::language_groups().get(i) else { return 0 };
        self.font
            .glyphs
            .iter()
            .filter(|e| sb::glyph_matches_language_group(&e.name, &Self::entry_codepoints(e), g))
            .count()
    }

    /// Present-count for GF-coverage filter `i` (glyphs the font has).
    fn filter_present(&self, i: usize) -> usize {
        use runebender_core::sidebar as sb;
        let Some(f) = sb::builtin_filters().get(i).and_then(|b| b.glyphset.as_ref()) else { return 0 };
        self.font
            .glyphs
            .iter()
            .filter(|e| sb::glyph_matches_character_filter(&e.name, &Self::entry_codepoints(e), f))
            .count()
    }

    fn category_count(&self, cat: GlyphCategory) -> usize {
        if cat == GlyphCategory::All {
            self.font.glyphs.len()
        } else {
            self.font.glyphs.iter().filter(|g| g.category == cat).count()
        }
    }

    fn cell_metrics(&self, cell: f64) -> CellMetrics {
        CellMetrics {
            cell,
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
                let (l, r) = metric_bufs(&session);
                self.lsb_buf = l;
                self.rsb_buf = r;
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
        self.refresh_metric_bufs();
        self.selected_points = self.session.selection.len();
    }

    /// The OKLCH themes in menu order (matches runebender-gpui).
    const THEMES: [&'static str; 4] = ["dark", "midnight", "gray", "light"];

    /// Advance to the next theme, reloading the palette and the baked cell
    /// colors. Exercises the design-token kernel: one id swaps every role.
    fn cycle_theme(&mut self) {
        let i = Self::THEMES.iter().position(|t| *t == self.theme_id).unwrap_or(0);
        self.theme_id = Self::THEMES[(i + 1) % Self::THEMES.len()];
        self.palette = Arc::new(Palette::load(self.theme_id));
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
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

    /// Recompute the LSB/RSB/advance text buffers from the current session.
    /// The selection's reference point, at the picked corner.
    fn coord_point(&self) -> Option<masonry::kurbo::Point> {
        let bounds = self.session.selection_bounds()?;
        Some(self.coord_quadrant.point_in_dspace_rect(bounds))
    }

    /// Refill the Coordinates fields from the selection.
    fn refresh_coord_bufs(&mut self) {
        match self.coord_point() {
            Some(p) => {
                self.coord_x_buf = format!("{}", p.x as i64);
                self.coord_y_buf = format!("{}", p.y as i64);
            }
            None => {
                self.coord_x_buf.clear();
                self.coord_y_buf.clear();
            }
        }
    }

    /// Move the selection so its reference point lands on the typed value.
    fn set_coord(&mut self, axis: usize, v: String) {
        if axis == 0 {
            self.coord_x_buf = v.clone();
        } else {
            self.coord_y_buf = v.clone();
        }
        let (Ok(target), Some(now)) = (v.trim().parse::<f64>(), self.coord_point()) else {
            return;
        };
        let (dx, dy) = if axis == 0 {
            (target - now.x, 0.0)
        } else {
            (0.0, target - now.y)
        };
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        self.apply_op(|s| s.nudge(dx, dy));
        self.refresh_coord_bufs();
    }

    fn refresh_metric_bufs(&mut self) {
        self.advance_buf = format!("{}", self.session.advance() as i64);
        let (l, r) = metric_bufs(&self.session);
        self.lsb_buf = l;
        self.rsb_buf = r;
    }

    /// Set the left sidebearing: shift the glyph so its ink left edge sits at
    /// `v` (advance unchanged, so the right sidebearing moves).
    fn set_lsb_from_buf(&mut self, v: String) {
        self.lsb_buf = v;
        if let Ok(t) = self.lsb_buf.trim().parse::<f64>() {
            let mut sess = (*self.session).clone();
            if let Some(sb) = sess.side_bearings() {
                sess.shift_glyph(t - sb.min_x);
                self.session = Arc::new(sess);
                self.refresh_open_glyph();
                self.advance_buf = format!("{}", self.session.advance() as i64);
                if let Some(sb2) = self.session.side_bearings() {
                    self.rsb_buf = format!("{}", sb2.rsb);
                }
            }
        }
    }

    /// Set the right sidebearing: change the advance so the gap past the ink
    /// right edge equals `v` (left sidebearing unchanged).
    fn set_rsb_from_buf(&mut self, v: String) {
        self.rsb_buf = v;
        if let Ok(t) = self.rsb_buf.trim().parse::<f64>() {
            let mut sess = (*self.session).clone();
            if let Some(sb) = sess.side_bearings() {
                sess.set_advance(sb.max_x + t);
                self.session = Arc::new(sess);
                self.refresh_open_glyph();
                self.advance_buf = format!("{}", self.session.advance() as i64);
            }
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

/// Layers: one row per master, with a thumbnail of the current glyph in
/// that master. Clicking a row switches the active master. This is the
/// gpui inspector's Layers section, and it replaces the old tab strip
/// that sat across the top of the canvas.
fn layers_section(app: &App) -> Option<impl WidgetView<App> + use<>> {
    use masonry::imaging::Painter;
    use masonry::kurbo::{Affine, Size};
    if app.font.master_names.len() < 2 {
        return None;
    }
    let pal = &app.palette;
    let glyph_name = match app.mode {
        Mode::Editor(_) => Some(app.session.glyph_name.clone()),
        Mode::Overview => app
            .selected
            .and_then(|i| app.font.glyphs.get(i))
            .map(|g| g.name.clone()),
    };
    let (asc, desc) = (app.font.ascender, app.font.descender);
    let rows: Vec<_> = app
        .font
        .short_master_names()
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            let active = i == app.font.active;
            let shown = app.reference_layers.contains(&i);
            let (bg, fg) = if active {
                (pal.role("gridSelected").with_alpha(0.22), pal.role("accent"))
            } else {
                (pal.panel, pal.text)
            };
            // A lit thumbnail means the master is drawn as a ghost under
            // the active outline; clicking the thumbnail toggles that.
            let ink = if active || shown { pal.text } else { pal.text_muted };
            let thumb_bg = if shown {
                pal.role("reference").with_alpha(0.28)
            } else {
                pal.control
            };
            let path_and_advance = glyph_name
                .as_ref()
                .and_then(|n| app.font.master_glyph(i, n));
            let thumb = path_and_advance.map(|(path, advance)| {
                sized_box(
                    button(
                        sized_box(canvas(move |_app: &mut App, _ctx, scene, size: Size| {
                            let mut p = Painter::new(scene);
                            let em = (asc - desc).max(1.0);
                            let scale =
                                (size.height / em).min(size.width / advance.max(1.0));
                            let ox = (size.width - advance * scale) / 2.0;
                            let baseline = size.height + desc * scale;
                            let t = Affine::new([scale, 0.0, 0.0, -scale, ox, baseline]);
                            p.fill(&(t * path.clone()), ink).draw();
                        }))
                        .dims(Dimensions::new(
                            Dim::Fixed(Length::px(20.0)),
                            Dim::Fixed(Length::px(20.0)),
                        )),
                        move |app: &mut App| {
                            if !app.reference_layers.remove(&i) {
                                app.reference_layers.insert(i);
                            }
                        },
                    )
                    .background_color(thumb_bg),
                )
                .dims(Dimensions::new(
                    Dim::Fixed(Length::px(26.0)),
                    Dim::Fixed(Length::px(26.0)),
                ))
            });
            flex_row((
                thumb,
                sized_box(
                    button(
                        label(name).text_size(Type::Body.px()).color(fg),
                        move |app: &mut App| app.set_master(i),
                    )
                    .background_color(bg),
                )
                .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(26.0))))
                .flex(1.0),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(Space::Sm.len())
        })
        .collect();
    Some(
        flex_col((
            section_header(pal, "Layers"),
            flex_col(rows)
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .gap(Space::Xs.len()),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Space::Sm.len()),
    )
}

/// Axes: one labeled slider per designspace axis, in the inspector.
fn axes_section(app: &App) -> Option<impl WidgetView<App> + use<>> {
    if app.font.axes.is_empty() {
        return None;
    }
    let pal = &app.palette;
    let (muted, text) = (pal.text_muted, pal.text);
    let rows: Vec<_> = app
        .font
        .axes
        .iter()
        .enumerate()
        .map(|(i, ax)| {
            let value = app.axis_values.get(i).copied().unwrap_or(ax.default);
            flex_col((
                flex_row((
                    label(ax.tag.clone()).text_size(Type::Body.px()).color(muted),
                    FlexSpacer::Flex(1.0),
                    label(format!("{value:.0}")).text_size(Type::Body.px()).color(text),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Center),
                slider(ax.min, ax.max, value, move |app: &mut App, v| {
                    app.set_axis(i, v)
                })
                .width(Length::px(214.0)),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Space::Xs.len())
        })
        .collect();
    // A short hint when the location sits off any master.
    let hint = (!app.on_active_master())
        .then(|| label("interpolated").text_size(Type::Caption.px()).color(pal.role("warning")));
    Some(
        flex_col((
            section_header(pal, "Axes"),
            flex_col(rows)
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .gap(Space::Md.len()),
            hint,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Space::Sm.len()),
    )
}

/// Display name for a theme id, e.g. "dark" -> "Dark".
fn theme_label(id: &str) -> String {
    let mut c = id.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
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
        (!editing).then(|| label(title).text_size(Type::Title.px()).color(pal.text)),
        FlexSpacer::Flex(1.0),
        editing.then(|| header_tools(app)),
        editing.then(|| FlexSpacer::Fixed(Length::px(12.0))),
        text_button(theme_label(app.theme_id), |app: &mut App| app.cycle_theme())
            .background_color(pal.button),
        label(save_text.to_string()).color(save_color),
        text_button("Save", |app: &mut App| app.save()).background_color(pal.button),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Space::Lg.len())
    .padding(Space::Md.len())
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
    .gap(Space::Xs.len())
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
    // Bottom bar: mark swatches on the left (set the current/selected glyphs'
    // mark), then the status text (gpui's bottom bar).
    let swatch = |mark: Option<String>, color: xilem::Color| {
        sized_box(
            text_button("", move |app: &mut App| app.set_mark(mark.clone()))
                .background_color(color),
        )
        .dims(Dimensions::fixed(Length::px(15.0), Length::px(15.0)))
    };
    let marks: Vec<_> = app
        .palette
        .mark_list()
        .into_iter()
        .map(|(name, color)| swatch(Some(name), color))
        .collect();
    flex_row((
        swatch(None, pal.control),
        flex_row(marks).gap(Space::Sm.len()),
        FlexSpacer::Fixed(Length::px(14.0)),
        label(text).color(pal.text_muted),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Space::Md.len())
    .padding(Space::Md.len())
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
    let cat_rows: Vec<_> = cats
        .into_iter()
        .filter(|c| app.category_count(*c) > 0)
        .map(|c| {
            ui::list_row(
                pal,
                c.display_name().to_string(),
                format!("{}", app.category_count(c)),
                app.sel == Sel::Category(c),
                move |app: &mut App| app.sel = Sel::Category(c),
            )
        })
        .collect();

    let lang_rows: Vec<_> = runebender_core::sidebar::language_groups()
        .iter()
        .enumerate()
        .filter(|(i, _)| app.language_count(*i) > 0)
        .map(|(i, g)| {
            ui::list_row(
                pal,
                g.label.clone(),
                format!("{}", app.language_count(i)),
                app.sel == Sel::Language(i),
                move |app: &mut App| app.sel = Sel::Language(i),
            )
        })
        .collect();

    let filter_rows: Vec<_> = runebender_core::sidebar::builtin_filters()
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            let gs = b.glyphset.as_ref()?;
            let expected = gs.expected_count.unwrap_or(gs.glyph_names.len().max(gs.targets.len()));
            Some(ui::list_row(
                pal,
                b.label.clone(),
                format!("{}/{}", app.filter_present(i), expected),
                app.sel == Sel::Filter(i),
                move |app: &mut App| app.sel = Sel::Filter(i),
            ))
        })
        .collect();

    // Search row: the field, then gpui's small scope and case toggles.
    let toggle = |text: String, active: bool, f: fn(&mut App)| {
        ui::toggle(pal, text, active, move |app: &mut App| f(app))
    };
    flex_col((
        flex_row((
            text_input(app.filter.clone(), |app: &mut App, v| app.filter = v)
                .placeholder("Search")
                .flex(1.0),
            toggle(
                match app.search_mode { 1 => "N", 2 => "U", _ => "A" }.to_string(),
                app.search_mode != 0,
                |app: &mut App| app.search_mode = (app.search_mode + 1) % 3,
            ),
            toggle("Aa".into(), app.search_case, |app: &mut App| {
                app.search_case = !app.search_case
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Space::Sm.len()),
        text_button(
            match app.sort { Sort::Name => "Sort: name", Sort::Unicode => "Sort: unicode" },
            |app: &mut App| {
                app.sort = match app.sort { Sort::Name => Sort::Unicode, Sort::Unicode => Sort::Name };
            },
        )
        .background_color(pal.button),
        {
            let fresh = !app.filter.trim().is_empty() && app.font.index_of(app.filter.trim()).is_none();
            fresh.then(|| {
                text_button(format!("+ New {}", app.filter.trim()), |app: &mut App| app.new_glyph())
                    .background_color(pal.role("accent"))
            })
        },
        portal(
            flex_col((
                section_header(pal, "Categories"),
                flex_col(cat_rows).gap(Space::Xs.len()),
                (!lang_rows.is_empty()).then(|| section_header(pal, "Languages")),
                flex_col(lang_rows).gap(Space::Xs.len()),
                (!filter_rows.is_empty()).then(|| section_header(pal, "Filters")),
                flex_col(filter_rows).gap(Space::Xs.len()),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Space::Md.len()),
        )
        .flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Space::Md.len())
    .padding(Space::Md.len())
    .background_color(pal.panel)
}

/// A flat, full-width sidebar row: label left, count right, subtle highlight
/// when selected (gpui's list rows, not pill buttons).

fn editor_nav(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let current = match app.mode { Mode::Editor(i) => Some(i), _ => None };
    flex_col((
        text_input(app.filter.clone(), |app: &mut App, v| app.filter = v)
            .placeholder("Search"),
        // The grid scrolls itself, so no portal here: nesting the two
        // gave the rail a dead area below the third row.
        grid(
            app.filtered_cells(),
            app.cell_metrics(84.0),
            app.palette.clone(),
            current,
            app.multi_selected.clone(),
            |app: &mut App, ev| match ev {
                GridEvent::Selected { index, .. } => app.open_glyph(index),
                GridEvent::Open(i) => app.open_glyph(i),
            },
        )
        .flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Space::Md.len())
    .padding(Space::Md.len())
    .background_color(pal.panel)
}

fn overview(app: &App) -> impl WidgetView<App> + use<> {
    let metrics = app.cell_metrics(104.0);
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

/// A large preview of the selected glyph, at the foot of the inspector in
/// overview mode (gpui's glyph preview panel). The grid cell is small; this
/// is where you look at the shape.
fn glyph_preview(app: &App) -> Option<impl WidgetView<App> + use<>> {
    use masonry::imaging::Painter;
    use masonry::kurbo::{Affine, Rect, Shape, Size, Stroke};
    let entry = app.selected.and_then(|i| app.font.glyphs.get(i))?;
    let outline = entry.outline.clone();
    let advance = entry.advance;
    let (asc, desc) = (app.font.ascender, app.font.descender);
    let fill = app.palette.text;
    let line = app.palette.role("gridBorder").with_alpha(0.5);
    Some(
        sized_box(canvas(move |_app: &mut App, _ctx, scene, size: Size| {
            let mut p = Painter::new(scene);
            let margin = 18.0;
            let em_w = advance.max(1.0);
            let em_h = (asc - desc).max(1.0);
            let scale = ((size.width - margin * 2.0) / em_w)
                .min((size.height - margin * 2.0) / em_h);
            let ox = (size.width - em_w * scale) / 2.0;
            let baseline = (size.height + em_h * scale) / 2.0 + desc * scale;
            let t = Affine::new([scale, 0.0, 0.0, -scale, ox, baseline]);
            // The advance box, so the preview reads as metrics, not art.
            let box_path = t * Rect::new(0.0, desc, em_w, asc).to_path(0.1);
            p.stroke(&box_path, &Stroke::new(1.0), line).draw();
            p.fill(&(t * (*outline).clone()), fill).draw();
        }))
        .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(170.0)))),
    )
}

fn editor_pane(app: &App) -> impl WidgetView<App> + use<> {
    let ghosts = Arc::new(
        app.font
            .reference_outlines(&app.session.glyph_name, &app.reference_layers),
    );
    let interp = app.interp_preview();
    editor(app.session.clone(), app.palette.clone(), app.tool, app.show_comb, ghosts, interp, |app: &mut App, ev| match ev {
        editor::EditorEvent::Selection(n) => {
            app.selected_points = n;
            app.refresh_coord_bufs();
        }
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
        section_header(pal, "Transformations"),
        // Two even rows of four, the way gpui lays its icon grid out. A
        // ragged 3 / 4 / 1 grid was the panel's most visible defect.
        flex_row((
            op("flip-h", |s| s.flip_horizontal()),
            op("flip-v", |s| s.flip_vertical()),
            op("rot-cw", |s| s.rotate_90()),
            op("close", |s| s.decompose()),
        )).gap(Space::Sm.len()),
        flex_row((
            op("union", |s| s.remove_overlap()),
            op("subtract", |s| s.boolean(BoolOp::Subtract)),
            op("intersect", |s| s.boolean(BoolOp::Intersect)),
            op("exclude", |s| s.boolean(BoolOp::Exclude)),
        )).gap(Space::Sm.len()),
        // Labeled transform buttons, matching gpui's Transformations block.
        flex_row((
            tbtn(pal, "Harmonize", |s| s.harmonize()),
            tbtn(pal, "Balance", |s| s.balance()),
        )).gap(Space::Sm.len()),
        flex_row((
            tbtn(pal, "Optimize", |s| s.optimize()),
            tbtn(pal, "Round", |s| s.round_corners()),
        )).gap(Space::Sm.len()),
        flex_row((
            tbtn(pal, "Reverse", |s| s.reverse()),
        )).gap(Space::Sm.len()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Space::Md.len())
}

/// The LSB/RSB text-buffer strings for a session.
fn metric_bufs(session: &Session) -> (String, String) {
    match session.side_bearings() {
        Some(sb) => (format!("{}", sb.lsb), format!("{}", sb.rsb)),
        None => (String::new(), String::new()),
    }
}

/// A right-panel section header with a disclosure caret (gpui style).

/// A labeled path-operation button.
fn tbtn(pal: &Palette, text: &'static str, f: fn(&mut Session) -> bool) -> impl WidgetView<App> + use<> {
    text_button(text, move |app: &mut App| app.apply_op(f)).background_color(pal.button)
}

/// Coordinates: the 9-point reference picker beside the X/Y fields, with
/// the selection's size on the right. gpui keeps this panel up whether or
/// not anything is selected, so the inspector does not jump.
fn coordinates_section(app: &App) -> impl WidgetView<App> + use<> {
    use runebender_core::path::Quadrant;
    const QUADRANTS: [Quadrant; 9] = [
        Quadrant::TopLeft,
        Quadrant::Top,
        Quadrant::TopRight,
        Quadrant::Left,
        Quadrant::Center,
        Quadrant::Right,
        Quadrant::BottomLeft,
        Quadrant::Bottom,
        Quadrant::BottomRight,
    ];
    let pal = &app.palette;
    let bounds = app.session.selection_bounds();
    // The picker: three rows of three dots, the active one filled accent.
    let dot = |q: Quadrant| {
        let active = app.coord_quadrant == q;
        let (bg, border) = if active {
            (pal.role("accent"), pal.role("accent"))
        } else {
            (pal.panel, pal.role("gridBorder").with_alpha(0.7))
        };
        sized_box(
            button(label(""), move |app: &mut App| {
                app.coord_quadrant = q;
                app.refresh_coord_bufs();
            })
            // Without this the button's own padding sets a minimum width
            // and the dot stretches into a pill.
            .padding(Length::px(0.0))
            .background_color(bg)
            .border_color(border)
            .border_width(Length::px(1.0))
            .corner_radius(Length::px(5.0)),
        )
        .dims(Dimensions::fixed(Length::px(10.0), Length::px(10.0)))
    };
    let row = |a: usize| {
        flex_row((dot(QUADRANTS[a]), dot(QUADRANTS[a + 1]), dot(QUADRANTS[a + 2])))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(Space::Md.len())
    };
    let picker = flex_col((row(0), row(3), row(6)))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Space::Md.len())
        .padding(Space::Sm.len());
    let field = |name: &'static str, value: String, axis: usize| {
        flex_row((
            sized_box(label(name).text_size(Type::Body.px()).color(pal.text_muted))
                .dims(Dimensions::fixed(Length::px(14.0), Length::px(18.0))),
            text_input(value, move |app: &mut App, v| app.set_coord(axis, v))
                .background_color(pal.field())
                .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Space::Md.len())
    };
    let size_row = bounds.map(|b| {
        flex_row((
            label("Size").text_size(Type::Body.px()).color(pal.text_muted),
            FlexSpacer::Flex(1.0),
            label(format!("{:.0} x {:.0}", b.width(), b.height()))
                .text_size(Type::Body.px())
                .color(pal.text),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
    });
    flex_col((
        section_header(pal, "Coordinates"),
        flex_row((
            picker,
            flex_col((
                field("X", app.coord_x_buf.clone(), 0),
                field("Y", app.coord_y_buf.clone(), 1),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Space::Sm.len())
            .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Space::Md.len()),
        size_row,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Space::Md.len())
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
        section_header(pal, "Mark"),
        flex_row((
            swatch(None, pal.control),
        )).gap(Space::Sm.len()),
        flex_row(marks).gap(Space::Sm.len()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Space::Sm.len())
}

fn info_panel(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let row = move |k: String, v: String| ui::kv(pal, k, v);
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
    // Width / LSB / RSB in one row (gpui's metrics row). Each field commits
    // live; LSB shifts the glyph, RSB changes the advance.
    let field_bg = pal.field();
    let _ = field_bg;
    let advance_field = editing.then(|| {
        ui::row(
            (
                ui::field(pal, "Width", app.advance_buf.clone(), |app: &mut App, v| {
                    app.set_advance_from_buf(v)
                })
                .flex(1.0),
                ui::field(pal, "LSB", app.lsb_buf.clone(), |app: &mut App, v| {
                    app.set_lsb_from_buf(v)
                })
                .flex(1.0),
                ui::field(pal, "RSB", app.rsb_buf.clone(), |app: &mut App, v| {
                    app.set_rsb_from_buf(v)
                })
                .flex(1.0),
            ),
            Space::Md,
        )
    });
    let name_field = editing.then(|| {
        ui::col(
            (
                ui::field(pal, "Name", app.name_buf.clone(), |app: &mut App, v| {
                    app.name_buf = v
                }),
                ui::field(pal, "Unicode", app.unicode_buf.clone(), |app: &mut App, v| {
                    app.set_unicode_from_buf(v)
                }),
            ),
            Space::Md,
        )
    });
    let show_multi_mark = !editing && !app.multi_selected.is_empty();
    flex_col((
        section_header(pal, "Glyph"),
        show_multi_mark.then(|| row("Selected".into(), format!("{}", app.multi_selected.len()))),
        show_multi_mark.then(|| mark_section(app)),
        (!editing).then(|| row("Name".into(), name)),
        (!editing).then(|| row("Unicode".into(), cp.clone())),
        name_field,
        (!editing).then(|| row("Advance".into(), adv)),
        advance_field,
        (!pts.is_empty()).then(|| row("Points".into(), pts)),
        editing.then(|| row("Selected".into(), format!("{}", app.selected_points))),
        editing.then(|| coordinates_section(app)),
        editing.then(|| path_section(app)),
        editing.then(|| {
            ui::section(
                pal,
                "Curves",
                ui::action(
                    pal,
                    if app.show_comb { "Comb: on" } else { "Comb: off" }.to_string(),
                    |app: &mut App| app.show_comb = !app.show_comb,
                ),
            )
        }),
        // Grouped: the flex tuple tops out at sixteen children.
        flex_col((
            editing.then(|| mark_section(app)),
            layers_section(app),
            editing.then(|| axes_section(app)).flatten(),
            (!editing).then(|| glyph_preview(app)).flatten(),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Space::Md.len()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Space::Md.len())
    .padding(Space::Lg.len())
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
    let center = flex_col((
        titlebar(app),
        body.flex(1.0),
        preview,
        status(app),
    ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(0.0))
        .background_color(pal.app);

    let left = match app.mode {
        Mode::Overview => Either::A(sidebar(app)),
        Mode::Editor(_) => Either::B(editor_nav(app)),
    };
    let left_width = if editing_mode { 232.0 } else { 224.0 };
    shortcuts::shortcut_host(flex_row((
        sized_box(left)
            .dims(Dimensions::new(Dim::Fixed(Length::px(left_width)), Dim::Stretch))
            .background_color(pal.panel),
        sized_box(center)
            .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
            .background_color(pal.app)
            .flex(1.0),
        sized_box(portal(info_panel(app)))
            .dims(Dimensions::new(Dim::Fixed(Length::px(250.0)), Dim::Stretch))
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
        app.refresh_coord_bufs();
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
