// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Files: opening a project, reloading it when the sources change, saving, and a new font.

use crate::*;
use runebender_core::outline::glyph_paths::round_units;
use std::sync::atomic::{AtomicU64, Ordering};

/// The next identity for an in-memory document session.
///
/// This is deliberately process-local: background jobs only need to tell a
/// replacement workspace from the one that launched them.
static NEXT_DOCUMENT_ID: AtomicU64 = AtomicU64::new(1);

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
            document_id: NEXT_DOCUMENT_ID.fetch_add(1, Ordering::Relaxed),
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
                // Start with the compact, scan-friendly inspector that the
                // GPUI shell presents: overview sections are headers until
                // requested, while the edit-mode coordinate and transform
                // sections remain immediately useful. This is state only;
                // every header still toggles its existing accessible panel.
                set.extend([
                    "Glyph",
                    "Font info",
                    "Dimensions",
                    "Advanced",
                    "Kerning",
                    "Groups",
                    "Compare",
                    "Features",
                    "Layers",
                    "Related",
                ]);
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

    /// Saves the live document and reports whether the disk now matches it.
    pub(crate) fn save(&mut self) -> bool {
        self.refresh_open_glyph();
        match self.font.save() {
            Ok(()) => {
                self.modified = false;
                self.note = format!("Saved {}", self.font.source().display());
                true
            }
            Err(e) => {
                self.note = format!("Save failed: {e}");
                false
            }
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
        // Sessions hold outlines and undo state from the old core project, so
        // re-create them from the fresh font. Viewport, fitted state, and the
        // selected tab are presentation state and remain meaningful after an
        // accepted reload.
        self.park();
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                (
                    tab.session.glyph_name.clone(),
                    tab.session.viewport.clone(),
                    tab.session.fitted,
                    tab.tool,
                    index == self.active_tab,
                )
            })
            .collect();
        let was_editor = matches!(self.mode, Mode::Editor(_));
        let source = self.font.document_source().to_path_buf();
        let active_master = self.font.active();
        let list = self.list;
        let detail = self.detail;
        let left_collapsed = self.left_collapsed;
        let search_mode = self.search_mode;
        let search_case = self.search_case;
        let search_regex = self.search_regex;
        match Self::open(&source) {
            Ok(mut fresh) => {
                fresh.font.set_active(active_master);
                fresh.axis_values = fresh.font.master_axis_values(fresh.font.active());
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
                fresh.tabs.clear();
                let mut active_tab = None;
                for (name, viewport, fitted, tool, was_active) in tabs {
                    let Some(mut session) = Session::new(fresh.font.font(), &name) else {
                        continue;
                    };
                    session.viewport = viewport;
                    session.fitted = fitted;
                    fresh.tabs.push(Tab {
                        session: Arc::new(session),
                        tool,
                    });
                    if was_active {
                        active_tab = Some(fresh.tabs.len() - 1);
                    }
                }
                *self = fresh;
                if was_editor {
                    if let Some(index) = active_tab {
                        self.active_tab = index;
                        let tab = &self.tabs[index];
                        self.session = tab.session.clone();
                        self.tool = tab.tool;
                        self.selected = self.font.index_of(&self.session.glyph_name);
                        if let Some(selected) = self.selected {
                            self.mode = Mode::Editor(selected);
                            self.refresh_metric_bufs();
                            self.refresh_coord_bufs();
                        }
                    } else {
                        self.mode = Mode::Overview;
                        self.selected = None;
                    }
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
        if self.modified {
            self.note = "Save or discard changes before creating a new font".into();
            return;
        }
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

    fn two_master_designspace(label: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "runebender-xilem-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        std::fs::create_dir_all(&dir).expect("the fixture directory is created");
        norad::Font::new()
            .save(dir.join("Regular.ufo"))
            .expect("the regular master saves");
        norad::Font::new()
            .save(dir.join("Bold.ufo"))
            .expect("the bold master saves");
        let designspace = dir.join("Test.designspace");
        std::fs::write(
            &designspace,
            r#"<?xml version='1.0' encoding='UTF-8'?>
<designspace format="4.0">
  <axes><axis name="Weight" tag="wght" minimum="400" default="400" maximum="700"/></axes>
  <sources>
    <source familyname="Test" stylename="Regular" filename="Regular.ufo">
      <location><dimension name="Weight" xvalue="400"/></location>
    </source>
    <source familyname="Test" stylename="Bold" filename="Bold.ufo">
      <location><dimension name="Weight" xvalue="700"/></location>
    </source>
  </sources>
</designspace>"#,
        )
        .expect("the designspace fixture saves");
        (dir, designspace)
    }

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
        let document_id = workspace.document_id;
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
        assert_ne!(workspace.document_id, document_id);
        assert!(workspace.list);
        assert!(workspace.detail);
        assert!(workspace.left_collapsed);
        assert_eq!(workspace.search_mode, 2);
        assert!(workspace.search_case);
        assert!(workspace.search_regex);
        assert!(workspace.search_re.is_some());

        std::fs::remove_dir_all(path).expect("the empty UFO fixture is removed");
    }

    #[test]
    fn reload_keeps_open_tabs_and_their_viewports() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-tabs-reload-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        norad::Font::new()
            .save(&path)
            .expect("the empty UFO fixture saves");
        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        workspace.filter = "A".into();
        workspace.new_glyph();
        workspace.new_tab();
        workspace.filter = "B".into();
        workspace.new_glyph();
        let mut session = (*workspace.session).clone();
        session.viewport.offset = kurbo::Vec2::new(40.0, 50.0);
        session.viewport.zoom = 2.0;
        workspace.session = Arc::new(session);
        workspace.park();
        workspace.activate_tab(0);
        let mut session = (*workspace.session).clone();
        session.viewport.offset = kurbo::Vec2::new(10.0, 20.0);
        session.viewport.zoom = 1.5;
        workspace.session = Arc::new(session);
        workspace.park();
        workspace.activate_tab(1);
        workspace.save();

        workspace.reload_from_disk();

        assert_eq!(workspace.tabs.len(), 2);
        assert_eq!(workspace.tabs[0].session.glyph_name, "A");
        assert_eq!(workspace.tabs[1].session.glyph_name, "B");
        assert_eq!(workspace.active_tab, 1);
        assert_eq!(workspace.session.glyph_name, "B");
        assert_eq!(workspace.tabs[0].session.viewport.offset.x, 10.0);
        assert_eq!(workspace.tabs[0].session.viewport.offset.y, 20.0);
        assert_eq!(workspace.tabs[0].session.viewport.zoom, 1.5);
        assert_eq!(workspace.session.viewport.offset.x, 40.0);
        assert_eq!(workspace.session.viewport.offset.y, 50.0);
        assert_eq!(workspace.session.viewport.zoom, 2.0);

        std::fs::remove_dir_all(path).expect("the empty UFO fixture is removed");
    }

    #[test]
    fn reload_keeps_the_active_master_of_a_designspace() {
        let (dir, designspace) = two_master_designspace("designspace-reload");
        let mut workspace = Workspace::open(&designspace).expect("the designspace opens");
        assert_eq!(workspace.font.master_names().len(), 2);
        workspace.set_master(1);

        workspace.reload_from_disk();

        assert_eq!(workspace.font.master_names().len(), 2);
        assert_eq!(workspace.font.active(), 1);
        assert_eq!(workspace.font.master_name(1), "Bold");
        std::fs::remove_dir_all(dir).expect("the designspace fixture is removed");
    }

    #[test]
    fn save_reopen_keeps_a_new_glyph_in_every_master() {
        let (dir, designspace) = two_master_designspace("designspace-save");
        let mut workspace = Workspace::open(&designspace).expect("the designspace opens");
        workspace.filter = "A".into();
        workspace.new_glyph();
        assert!(workspace.modified);
        assert!(workspace.save());

        let mut reopened = Workspace::open(&designspace).expect("the saved designspace reopens");
        assert!(reopened.font.index_of("A").is_some());
        reopened.set_master(1);
        assert!(reopened.font.index_of("A").is_some());
        std::fs::remove_dir_all(dir).expect("the designspace fixture is removed");
    }

    #[test]
    fn save_reopen_keeps_kerning_groups_in_every_master() {
        let (dir, designspace) = two_master_designspace("designspace-kerning");
        let mut workspace = Workspace::open(&designspace).expect("the designspace opens");
        workspace.filter = "A".into();
        workspace.new_glyph();
        workspace.set_kern_group(true, "A".into());
        assert!(workspace.save());

        let mut reopened = Workspace::open(&designspace).expect("the saved designspace reopens");
        assert_eq!(reopened.font.kern_group("A", true), "public.kern1.A");
        reopened.set_master(1);
        assert_eq!(reopened.font.kern_group("A", true), "public.kern1.A");
        std::fs::remove_dir_all(dir).expect("the designspace fixture is removed");
    }

    #[test]
    fn new_font_keeps_a_dirty_document_open() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-new-dirty-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        norad::Font::new()
            .save(&path)
            .expect("the empty UFO fixture saves");
        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        let source = workspace.font.source().to_path_buf();
        workspace.modified = true;

        workspace.new_font();

        assert_eq!(workspace.font.source(), source);
        assert!(workspace.modified);
        assert_eq!(
            workspace.note,
            "Save or discard changes before creating a new font"
        );
        std::fs::remove_dir_all(path).expect("the empty UFO fixture is removed");
    }

    #[test]
    fn save_reports_failure_for_an_unwritable_source() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-save-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        norad::Font::new()
            .save(&path)
            .expect("the empty UFO fixture saves");
        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        workspace.font.master_mut().source_path = "/dev/null/runebender-test.ufo".into();

        assert!(!workspace.save());
        assert!(workspace.note.starts_with("Save failed:"));

        std::fs::remove_dir_all(path).expect("the empty UFO fixture is removed");
    }
}
