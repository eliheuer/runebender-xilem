// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Files: opening a project, reloading it when the sources change, saving, and a new font.

use crate::*;
use runebender_core::outline::glyph_paths::round_units;

impl Workspace {
    pub(crate) fn open(path: &FsPath) -> Result<Self, String> {
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
            .or(if font.glyphs.is_empty() {
                None
            } else {
                Some(0)
            })
            .ok_or_else(|| "font has no glyphs".to_string())?;
        let session =
            Arc::new(Session::new(&font.font, &font.glyphs[first].name).ok_or("glyph missing")?);
        // For headless screenshots: optionally select all points.
        // (set later, after session is final)

        let start_cat = std::env::var("RUNEBENDER_CAT").ok();
        let (mode, open) = match std::env::var("RUNEBENDER_OPEN")
            .ok()
            .and_then(|n| font.index_of(&n))
        {
            Some(i) => (Mode::Editor(i), Some(i)),
            None => (Mode::Overview, None),
        };
        let session = match open {
            Some(i) => Arc::new(
                Session::new(&font.font, &font.glyphs[i].name)
                    .unwrap_or_else(|| (*session).clone()),
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
        // Headless overrides, so a render can show a state that normally
        // takes clicks to reach. The GPUI build has the same idea.
        let reference_buf = std::env::var("RUNEBENDER_REFERENCE").unwrap_or_default();
        let show_background = std::env::var("RUNEBENDER_BACKGROUND").is_ok();
        // RUNEBENDER_VIEW=comb,continuity,colorize,handles,segments,bearings
        let mut view = canvas::editor::ViewOptions::default();
        if let Ok(spec) = std::env::var("RUNEBENDER_VIEW") {
            for name in spec.split(',').map(str::trim) {
                match name {
                    "comb" => view.comb = true,
                    "continuity" => view.continuity = true,
                    "colorize" => view.colorize = true,
                    "handles" => view.handles = true,
                    "segments" => view.segments = true,
                    "bearings" => view.bearings = true,
                    "popcount" => view.popcount = true,
                    _ => {}
                }
            }
        }
        // Headless override: RUNEBENDER_AXIS="wght=500,wdth=80".
        if let Ok(spec) = std::env::var("RUNEBENDER_AXIS") {
            for pair in spec.split(',') {
                if let Some((tag, val)) = pair.split_once('=')
                    && let Ok(v) = val.trim().parse::<f64>()
                    && let Some(i) = font
                        .axes
                        .iter()
                        .position(|a| a.tag == tag.trim() || a.name == tag.trim())
                {
                    axis_values[i] = v.clamp(font.axes[i].min, font.axes[i].max);
                }
            }
        }
        // Seed the Name/Unicode fields from the glyph actually shown
        // (the opened one in editor mode, else the first).
        let shown = open.unwrap_or(first);
        let first_name = font.glyphs[shown].name.clone();
        let (kern1, kern2) = (
            font.kern_group(&first_name, true),
            font.kern_group(&first_name, false),
        );
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
            multi_selected: Arc::new(std::collections::HashSet::new()),
            filter: String::new(),
            detail: false,
            rail: Rail::Glyphs,
            text_dir: None,
            left_collapsed: false,
            collapsed: std::collections::HashSet::new(),
            sel: Sel::Category(match start_cat.as_deref() {
                Some("Number") => GlyphCategory::Number,
                Some("Symbol") => GlyphCategory::Symbol,
                Some("Mark") => GlyphCategory::Mark,
                _ => GlyphCategory::All,
            }),
            sort: Sort::Name,
            advance_buf: format!("{}", round_units(session.advance())),
            lsb_buf: metric_bufs(&session).0,
            rsb_buf: metric_bufs(&session).1,
            kern1_buf: kern1,
            kern2_buf: kern2,
            clipboard: Vec::new(),
            show_background,
            reference_buf,
            name_buf: first_name,
            unicode_buf: first_uni,
            tabs: vec![Tab {
                session: session.clone(),
                tool: Tool::Select,
            }],
            active_tab: 0,
            session,
            selected_points: 0,
            tool: match std::env::var("RUNEBENDER_TOOL").as_deref() {
                Ok("measure") => Tool::Measure,
                Ok("text") => Tool::Text,
                _ => Tool::Select,
            },
            modified: false,
            note: String::new(),
            view,
            initial_text: std::env::var("RUNEBENDER_TEXT").unwrap_or_default(),
            cell_size: 88.0,
            axis_values,
            theme_id,
            coord_quadrant: runebender_core::outline::path::Quadrant::Center,
            coord_x_buf: String::new(),
            coord_y_buf: String::new(),
            search_mode: 0,
            search_case: false,
            reference_layers: std::collections::HashSet::new(),
        })
    }

    pub(crate) fn save(&mut self) {
        self.refresh_open_glyph();
        match self.font.save() {
            Ok(()) => {
                self.modified = false;
                self.note = format!("Saved {}", self.font.source.display());
            }
            Err(e) => self.note = format!("Save failed: {e}"),
        }
    }

    /// Reload the font from disk, when something else has written it.
    ///
    /// Unsaved work wins: if this editor has edits that are not on disk,
    /// the reload is skipped rather than throwing them away.
    pub(crate) fn reload_from_disk(&mut self) {
        if self.modified {
            self.note = "sources changed on disk; save or discard first".into();
            return;
        }
        let source = self.font.source.clone();
        let open = self.session.glyph_name.clone();
        match Self::open(&source) {
            Ok(mut fresh) => {
                fresh.theme_id = self.theme_id;
                fresh.palette = self.palette.clone();
                fresh.sel = self.sel;
                fresh.sort = self.sort;
                fresh.filter = self.filter.clone();
                let reopen = matches!(self.mode, Mode::Editor(_))
                    .then(|| fresh.font.index_of(&open))
                    .flatten();
                *self = fresh;
                if let Some(index) = reopen {
                    self.open_glyph(index);
                }
                self.note = "reloaded".into();
            }
            Err(e) => self.note = e,
        }
    }

    /// A new font from the template: GF metrics and the GF Latin Core
    /// set as empty encoded glyphs, saved beside the font in hand.
    ///
    /// The GPUI build asks where to put it with a save dialog. There is
    /// no file dialog here, so it lands next to the current source under
    /// the first Untitled name that is free.
    pub(crate) fn new_font(&mut self) {
        let font = runebender_core::document::new_font::new_font("Untitled", "Regular", 400);
        let dir = self
            .font
            .source
            .parent()
            .unwrap_or(FsPath::new("."))
            .to_path_buf();
        let mut path = dir.join("Untitled.ufo");
        let mut n = 1;
        while path.exists() {
            path = dir.join(format!("Untitled-{n}.ufo"));
            n += 1;
        }
        if let Err(e) = font.save(&path) {
            self.note = format!("could not write {}: {e}", path.display());
            return;
        }
        match Self::open(&path) {
            Ok(mut fresh) => {
                fresh.theme_id = self.theme_id;
                fresh.palette = self.palette.clone();
                fresh.note = format!("new font at {}", path.display());
                *self = fresh;
            }
            Err(e) => self.note = e,
        }
    }
}
