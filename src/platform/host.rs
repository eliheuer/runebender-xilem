// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Files: opening a project, reloading it when the sources change, saving, and a new font.

use crate::*;
use runebender_core::outline::glyph_paths::round_units;

impl Workspace {
    pub(crate) fn open(path: &FsPath) -> Result<Self, String> {
        let font = FontModel::open(path)?;
        let theme_id: &'static str = match std::env::var("RUNEBENDER_THEME").ok().as_deref() {
            Some("dark") => "dark",
            Some("light") => "light",
            _ => "gray",
        };
        let palette = Arc::new(Palette::load(theme_id));
        let cells = Arc::new(cells_of(&font, &palette));
        let first =
            font.index_of("A")
                .or_else(|| font.index_of("a"))
                .or(if font.glyphs.is_empty() {
                    None
                } else {
                    Some(0)
                });
        // An empty UFO is a valid document. The overview does not use an
        // editor session, but the workspace keeps one ready for views that
        // share its type; `new_glyph` replaces this inactive session before
        // switching to editor mode.
        let session = match first {
            Some(index) => Arc::new(
                Session::new(font.font(), &font.glyphs[index].name).ok_or("glyph missing")?,
            ),
            None => Arc::new(Session::inactive(font.font())),
        };
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
                Session::new(font.font(), &font.glyphs[i].name)
                    .unwrap_or_else(|| (*session).clone()),
            ),
            None => session,
        };
        // Snap sliders to the active master's location (Glyphs behavior), so
        // opening a master shows no interpolation overlay until you move one.
        let mut axis_values: Vec<f64> = if font.axes.is_empty() {
            Vec::new()
        } else {
            font.master_axis_values(font.active())
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
        let shown = open.or(first);
        let first_name = shown
            .and_then(|index| font.glyphs.get(index))
            .map(|glyph| glyph.name.clone())
            .unwrap_or_default();
        let (kern1, kern2) = (
            font.kern_group(&first_name, true),
            font.kern_group(&first_name, false),
        );
        let first_uni = shown
            .and_then(|index| font.glyphs.get(index))
            .and_then(|glyph| glyph.codepoint)
            .map(|c| format!("{:04X}", c as u32))
            .unwrap_or_default();
        let mut app = Self {
            font,
            palette,
            cells,
            mode,
            selected: open.or(first),
            multi_selected: Arc::new(std::collections::HashSet::new()),
            filter: String::new(),
            detail: false,
            list: std::env::var("RUNEBENDER_VIEW_MODE").as_deref() == Ok("list"),
            rail: Rail::Glyphs,
            text_dir: None,
            left_collapsed: false,
            // Headless frames can start with sections folded:
            // `RUNEBENDER_COLLAPSED=Kerning,Groups`.
            collapsed: {
                let mut set: std::collections::HashSet<&'static str> =
                    std::env::var("RUNEBENDER_COLLAPSED")
                        .map(|s| {
                            s.split(',')
                                .filter(|t| !t.is_empty())
                                .map(|t| &*Box::leak(t.to_string().into_boxed_str()))
                                .collect()
                        })
                        .unwrap_or_default();
                // The export metrics start folded, as in the GPUI build.
                set.insert("Advanced");
                set
            },
            sel: Sel::Category(match start_cat.as_deref() {
                Some("Number") => GlyphCategory::Number,
                Some("Symbol") => GlyphCategory::Symbol,
                Some("Mark") => GlyphCategory::Mark,
                _ => GlyphCategory::All,
            }),
            sort: Sort::Name,
            expanded_categories: std::collections::HashSet::new(),
            expanded_scripts: std::collections::HashSet::new(),
            search_regex: false,
            search_re: None,
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
            tabs: first
                .map(|_| Tab {
                    session: session.clone(),
                    tool: Tool::Select,
                })
                .into_iter()
                .collect(),
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
            cell_size: 96.0,
            axis_values,
            theme_id,
            coord_quadrant: runebender_core::outline::path::Quadrant::Center,
            coord_x_buf: String::new(),
            coord_y_buf: String::new(),
            search_mode: 0,
            search_case: false,
            reference_layers: std::collections::HashSet::new(),
            nodes: nodes::NodesState::default(),
            ai: local_ai::LocalAiState::default(),
            kern_filter_buf: String::new(),
            kern_first_buf: String::new(),
            kern_second_buf: String::new(),
            kern_value_buf: String::new(),
            group_name_buf: String::new(),
            features_status: None,
            #[cfg(unix)]
            live: runebender_core::document::live_socket::Server::start()
                .map_err(|e| eprintln!("Live tools unavailable: {e}"))
                .ok(),
        };
        app.init_nodes();
        app.rescan_models();
        app.refresh_proposals();
        // Headless: RUNEBENDER_RAIL=ai starts the editor's rail on the
        // Local AI panel, and RUNEBENDER_MODEL=<dir> chooses a model.
        if std::env::var("RUNEBENDER_RAIL").as_deref() == Ok("ai") {
            app.rail = Rail::LocalAi;
        }
        if let Some(dir) = std::env::var_os("RUNEBENDER_MODEL").filter(|d| !d.is_empty()) {
            app.load_model(FsPath::new(&dir));
        }
        // Headless overrides: RUNEBENDER_NODES=<file> opens a nodes
        // file, and RUNEBENDER_MODE=nodes starts on the canvas.
        if let Some(file) = std::env::var_os("RUNEBENDER_NODES").filter(|f| !f.is_empty()) {
            app.open_nodes_file(FsPath::new(&file));
        }
        if std::env::var("RUNEBENDER_MODE").as_deref() == Ok("nodes") {
            app.enter_nodes_mode();
        }
        Ok(app)
    }

    pub(crate) fn save(&mut self) {
        self.refresh_open_glyph();
        match self.font.save() {
            Ok(()) => {
                self.modified = false;
                self.note = format!("Saved {}", self.font.source().display());
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
        let source = self.font.source().to_path_buf();
        let open = self.session.glyph_name.clone();
        let list = self.list;
        let detail = self.detail;
        let left_collapsed = self.left_collapsed;
        let search_mode = self.search_mode;
        let search_case = self.search_case;
        let search_regex = self.search_regex;
        match Self::open(&source) {
            Ok(mut fresh) => {
                fresh.theme_id = self.theme_id;
                fresh.palette = self.palette.clone();
                fresh.sel = self.sel;
                fresh.sort = self.sort;
                fresh.filter = self.filter.clone();
                fresh.list = list;
                fresh.detail = detail;
                fresh.left_collapsed = left_collapsed;
                fresh.search_mode = search_mode;
                fresh.search_case = search_case;
                fresh.search_regex = search_regex;
                fresh.rebuild_search_regex();
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
            .source()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_an_empty_ufo_and_creates_its_first_glyph() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-empty-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        norad::Font::new()
            .save(&path)
            .expect("the empty UFO fixture saves");

        let mut workspace = Workspace::open(&path).expect("an empty UFO opens");
        assert!(matches!(workspace.mode, Mode::Overview));
        assert!(workspace.font.glyphs.is_empty());
        assert_eq!(workspace.selected, None);
        assert!(workspace.tabs.is_empty());

        workspace.filter = "A".into();
        workspace.new_glyph();
        assert_eq!(workspace.font.glyphs.len(), 1);
        assert_eq!(workspace.font.glyphs[0].name, "A");
        assert_eq!(workspace.session.glyph_name, "A");
        assert!(matches!(workspace.mode, Mode::Editor(0)));
        assert_eq!(workspace.tabs.len(), 1);

        workspace.save();
        workspace.list = true;
        workspace.detail = true;
        workspace.left_collapsed = true;
        workspace.search_mode = 2;
        workspace.search_case = true;
        workspace.search_regex = true;
        workspace.filter = "A".into();
        workspace.rebuild_search_regex();
        workspace.reload_from_disk();
        assert!(workspace.list);
        assert!(workspace.detail);
        assert!(workspace.left_collapsed);
        assert_eq!(workspace.search_mode, 2);
        assert!(workspace.search_case);
        assert!(workspace.search_regex);
        assert!(workspace.search_re.is_some());

        std::fs::remove_dir_all(path).expect("the empty UFO fixture is removed");
    }
}
