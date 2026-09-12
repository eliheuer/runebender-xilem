// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Files: opening a project, reloading it when the sources change, saving, and a new font.

use crate::*;
use runebender_core::outline::glyph_paths::round_units;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

/// The next identity for an in-memory document session.
///
/// This is deliberately process-local: background jobs only need to tell a
/// replacement workspace from the one that launched them.
static NEXT_DOCUMENT_ID: AtomicU64 = AtomicU64::new(1);
/// Stable identities for widget-owned text buffers parked in editor tabs.
pub(crate) static NEXT_TEXT_CONTEXT_ID: AtomicU64 = AtomicU64::new(1);

fn source_roots(font: &FontModel) -> Vec<std::path::PathBuf> {
    let mut roots = font.master_paths();
    roots.push(font.document_source().to_path_buf());
    roots.sort();
    roots.dedup();
    roots
}

fn source_fingerprint(roots: &[std::path::PathBuf]) -> u64 {
    fn hash_path(path: &FsPath, state: &mut std::collections::hash_map::DefaultHasher) {
        path.hash(state);
        if path.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(path)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .collect();
            entries.sort();
            for entry in entries {
                hash_path(&entry, state);
            }
        } else {
            match std::fs::read(path) {
                Ok(bytes) => bytes.hash(state),
                Err(error) => error.kind().hash(state),
            }
        }
    }

    let mut state = std::collections::hash_map::DefaultHasher::new();
    for root in roots {
        hash_path(root, &mut state);
    }
    state.finish()
}

impl Workspace {
    pub(crate) fn open(path: &FsPath) -> Result<Self, String> {
        let font = FontModel::open(path)?;
        let source_roots = source_roots(&font);
        let source_fingerprint = source_fingerprint(&source_roots);
        let features_buf = font.font().features.clone();
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
            overview_undo: Vec::new(),
            overview_redo: Vec::new(),
            metadata_undo: Vec::new(),
            metadata_redo: Vec::new(),
            filter: String::new(),
            detail: false,
            list: std::env::var("RUNEBENDER_VIEW_MODE").as_deref() == Ok("list"),
            rail: Rail::Glyphs,
            text_dir: None,
            text_features_disabled: std::env::var("RUNEBENDER_TEXT_FEATURES_DISABLED")
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|tag| !tag.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            text_script: std::env::var("RUNEBENDER_TEXT_SCRIPT")
                .ok()
                .filter(|value| !value.is_empty()),
            text_language: std::env::var("RUNEBENDER_TEXT_LANGUAGE")
                .ok()
                .filter(|value| !value.is_empty()),
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
                    "Path Operations",
                    "Background",
                    "Mark",
                    "Masters",
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
            show_mark_cloud: std::env::var("RUNEBENDER_MARK_CLOUD").as_deref() == Ok("1"),
            reference_buf,
            component_base_buf: String::new(),
            name_buf: first_name,
            unicode_buf: first_uni,
            tabs: first
                .map(|_| Tab {
                    text_context_id: NEXT_TEXT_CONTEXT_ID.fetch_add(1, Ordering::Relaxed),
                    session: session.clone(),
                    tool: Tool::Select,
                    text_context: TextContext::default(),
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
            source_roots,
            source_fingerprint,
            note: String::new(),
            view,
            initial_text: std::env::var("RUNEBENDER_TEXT").unwrap_or_default(),
            cell_size: 96.0,
            axis_values,
            theme_id,
            coord_quadrant: runebender_core::outline::path::Quadrant::Center,
            coord_x_buf: String::new(),
            coord_y_buf: String::new(),
            coord_w_buf: String::new(),
            coord_h_buf: String::new(),
            slant_buf: String::new(),
            offset_buf: String::new(),
            extrude_buf: String::new(),
            roughen_buf: String::new(),
            roughen_seed: 0,
            preview_text: std::env::var("RUNEBENDER_PREVIEW_TEXT")
                .unwrap_or_else(|_| "Runebender".into()),
            preview_blur: std::env::var("RUNEBENDER_PREVIEW_BLUR")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite())
                .unwrap_or(0.0)
                .clamp(0.0, 8.0),
            preview_invert: std::env::var("RUNEBENDER_PREVIEW_INVERT").as_deref() == Ok("1"),
            search_mode: 0,
            search_case: false,
            reference_layers: std::collections::HashSet::new(),
            show_all_masters: false,
            sample_index: 0,
            nodes: nodes::NodesState::default(),
            export_job: None,
            ai: local_ai::LocalAiState::default(),
            chat: chat::ChatState::default(),
            kern_filter_buf: String::new(),
            kern_first_buf: String::new(),
            kern_second_buf: String::new(),
            kern_value_buf: String::new(),
            group_name_buf: String::new(),
            features_buf,
            features_edited: false,
            features_status: None,
            #[cfg(unix)]
            live: runebender_core::document::live_socket::Server::start()
                .map_err(|e| eprintln!("Live tools unavailable: {e}"))
                .ok(),
        };
        app.park();
        app.init_nodes();
        app.rescan_models();
        app.scan_chat_models();
        app.refresh_proposals();
        // Headless: RUNEBENDER_RAIL=ai or chat starts the corresponding
        // local-model panel, RUNEBENDER_MODEL=<dir> chooses an outline model, and
        // RUNEBENDER_PROPOSAL_PREVIEW=<task> shows its review overlay.
        match std::env::var("RUNEBENDER_RAIL").as_deref() {
            Ok("ai") => app.rail = Rail::LocalAi,
            Ok("chat") => app.rail = Rail::Chat,
            Ok("shapes") => app.rail = Rail::Shapes,
            _ => {}
        }
        if let Some(index) = std::env::var("RUNEBENDER_COMPONENT")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
        {
            Arc::make_mut(&mut app.session).select_component(index);
        }
        if app.show_mark_cloud {
            app.collapsed.remove("Background");
        }
        if let Some(dir) = std::env::var_os("RUNEBENDER_MODEL").filter(|d| !d.is_empty()) {
            app.load_model(FsPath::new(&dir));
        }
        if let Ok(task) = std::env::var("RUNEBENDER_PROPOSAL_PREVIEW")
            && app
                .ai
                .proposals
                .iter()
                .any(|proposal| proposal.task == task)
        {
            app.ai.preview_task = Some(task);
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
        if self.features_edited {
            self.note = "Apply or Revert feature edits before saving".into();
            return false;
        }
        if source_fingerprint(&self.source_roots) != self.source_fingerprint {
            self.note = "Save blocked: sources changed on disk; use Save As to preserve your edits or Revert to Saved to accept disk changes".into();
            return false;
        }
        match self.font.save() {
            Ok(()) => {
                self.source_fingerprint = source_fingerprint(&self.source_roots);
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
        self.reload_from_disk_inner(false);
    }

    fn reload_from_disk_inner(&mut self, force: bool) {
        if !force && source_fingerprint(&self.source_roots) == self.source_fingerprint {
            return;
        }
        if self.modified && !force {
            self.note = "sources changed on disk; Save As preserves your edits, or Revert to Saved accepts disk changes".into();
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
                    tab.text_context_id,
                    tab.session.glyph_name.clone(),
                    tab.session.viewport.clone(),
                    tab.session.fitted,
                    tab.tool,
                    tab.text_context.clone(),
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
                for (text_context_id, name, viewport, fitted, tool, text_context, was_active) in
                    tabs
                {
                    let Some(mut session) = Session::new(fresh.font.font(), &name) else {
                        continue;
                    };
                    session.viewport = viewport;
                    session.fitted = fitted;
                    fresh.tabs.push(Tab {
                        text_context_id,
                        session: Arc::new(session),
                        tool,
                        text_context,
                    });
                    if was_active {
                        active_tab = Some(fresh.tabs.len() - 1);
                    }
                }
                *self = fresh;
                if was_editor {
                    if let Some(index) = active_tab {
                        self.active_tab = index;
                        let (session, tool, text_context) = {
                            let tab = &self.tabs[index];
                            (tab.session.clone(), tab.tool, tab.text_context.clone())
                        };
                        self.session = session;
                        self.tool = tool;
                        self.restore_text_context(text_context);
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

    /// Deliberately discard in-memory edits and accept the current source tree.
    pub(crate) fn revert_to_saved(&mut self) {
        self.reload_from_disk_inner(true);
    }

    /// Establish the non-existent destinations chosen by Save As as the new
    /// baseline before their first write.
    pub(crate) fn prepare_save_as(&mut self) {
        self.source_roots = source_roots(&self.font);
        self.source_fingerprint = source_fingerprint(&self.source_roots);
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

    fn copy_tree(source: &std::path::Path, destination: &std::path::Path) {
        std::fs::create_dir_all(destination).expect("the destination directory is created");
        for entry in std::fs::read_dir(source).expect("the source directory is readable") {
            let entry = entry.expect("the source entry is readable");
            let from = entry.path();
            let to = destination.join(entry.file_name());
            if entry
                .file_type()
                .expect("the source type is readable")
                .is_dir()
            {
                copy_tree(&from, &to);
            } else {
                std::fs::copy(&from, &to).expect("the source file is copied");
            }
        }
    }

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
        workspace.revert_to_saved();
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
    fn grid_modifiers_preserve_the_primary_and_extend_the_selection() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-grid-selection-{}-{}.ufo",
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
        for name in ["A", "B", "C"] {
            workspace.mode = Mode::Overview;
            workspace.filter = name.into();
            workspace.new_glyph();
        }
        workspace.mode = Mode::Overview;
        workspace.filter.clear();

        workspace.grid_select(0, false, false);
        workspace.grid_select(2, true, false);
        assert_eq!(workspace.selected, Some(2));
        assert_eq!(
            *workspace.multi_selected,
            std::collections::HashSet::from([0, 2])
        );

        workspace.grid_select(1, false, true);
        assert_eq!(workspace.selected, Some(1));
        assert_eq!(
            *workspace.multi_selected,
            std::collections::HashSet::from([0, 1, 2]),
            "shift-click extends rather than discards an existing multi-selection"
        );

        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    #[ignore = "copies and edits the adjacent 13 MB Virtua Grotesk sources"]
    fn disposable_virtua_edit_undo_save_reopen_preserves_unrelated_data() {
        let source =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../virtua-grotesk/sources");
        assert!(
            source.is_dir(),
            "clone Virtua Grotesk beside this repository"
        );
        let root = std::env::temp_dir().join(format!(
            "runebender-xilem-virtua-trial-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let copied = root.join("sources");
        copy_tree(&source, &copied);
        let designspace = copied.join("VirtuaGrotesk.designspace");
        let designspace_before = std::fs::read(&designspace).expect("the designspace is readable");

        let mut workspace =
            Workspace::open(&designspace).expect("the disposable designspace opens");
        assert_eq!(workspace.font.master_count(), 2);
        assert_eq!(workspace.font.master_name(0), "Regular");
        assert_eq!(workspace.font.master_name(1), "Bold");
        let arabic = runebender_core::ui::sidebar::language_groups()
            .iter()
            .position(|group| group.label == "Arabic")
            .expect("the Arabic script filter exists");
        assert!(workspace.language_count(arabic) >= 300);
        workspace.sel = Sel::Language(arabic);
        let arabic_cells = workspace.filtered_cells();
        assert!(
            arabic_cells
                .iter()
                .any(|cell| cell.name.as_ref() == "beh-ar")
        );
        assert!(
            arabic_cells
                .iter()
                .any(|cell| cell.name.as_ref() == "beh-ar.init")
        );
        assert!(!arabic_cells.iter().any(|cell| cell.name.as_ref() == "R"));
        workspace.sel = Sel::Category(GlyphCategory::All);

        for (name, unicode) in [
            ("R", Some('R')),
            ("beh-ar", Some('\u{0628}')),
            ("kasra-ar", Some('\u{0650}')),
            ("lam_alef-ar", None),
        ] {
            workspace.filter = name.into();
            let cells = workspace.filtered_cells();
            assert!(cells.iter().any(|cell| cell.name.as_ref() == name));
            let index = workspace.font.index_of(name).expect("the glyph is indexed");
            let source_glyph = workspace
                .font
                .font()
                .get_glyph(name)
                .expect("the source glyph exists")
                .clone();
            workspace.open_glyph(index);
            assert_eq!(workspace.session.glyph, source_glyph);
            assert_eq!(workspace.name_buf, name);
            assert_eq!(
                workspace.unicode_buf,
                unicode
                    .map(|character| format!("{:04X}", character as u32))
                    .unwrap_or_default()
            );
            assert_eq!(workspace.session.advance(), source_glyph.width);
        }
        workspace.filter.clear();
        workspace.set_master(1);
        assert_eq!(workspace.font.master_name(workspace.font.active()), "Bold");
        assert_eq!(workspace.session.glyph_name, "lam_alef-ar");
        workspace.set_master(0);
        assert_eq!(
            workspace.font.master_name(workspace.font.active()),
            "Regular"
        );
        let original_fonts: Vec<norad::Font> = workspace
            .font
            .project
            .masters
            .iter()
            .map(|master| master.font.clone())
            .collect();

        let index = workspace.font.index_of("R").expect("Virtua contains R");
        workspace.open_glyph(index);
        let original_x = workspace.session.glyph.contours[0].points[0].x;
        let original_width = workspace.session.advance();
        let original_anchor_count = workspace.session.glyph.anchors.len();

        workspace.apply_op(|session| {
            session.selection.insert((0, 0));
            session.nudge(2.0, 0.0)
        });
        workspace.set_advance_from_buf(format!("{}", original_width + 4.0));
        workspace.apply_op(|session| {
            session.add_anchor(123.0, 456.0);
            true
        });
        assert!(workspace.modified);

        for _ in 0..3 {
            workspace.undo_active_edit(false);
        }
        assert_eq!(workspace.session.glyph.contours[0].points[0].x, original_x);
        assert_eq!(workspace.session.advance(), original_width);
        assert_eq!(workspace.session.glyph.anchors.len(), original_anchor_count);
        for _ in 0..3 {
            workspace.undo_active_edit(true);
        }
        assert_eq!(
            workspace.session.glyph.contours[0].points[0].x,
            original_x + 2.0
        );
        assert_eq!(workspace.session.advance(), original_width + 4.0);
        assert_eq!(
            workspace.session.glyph.anchors.len(),
            original_anchor_count + 1
        );

        assert!(workspace.save());
        assert!(!workspace.modified);
        assert_eq!(
            std::fs::read(&designspace).expect("the designspace remains readable"),
            designspace_before,
            "saving masters must not rewrite their relative designspace paths"
        );

        let reopened = Workspace::open(&designspace).expect("the saved designspace reopens");
        let reopened_r = reopened
            .font
            .font()
            .get_glyph("R")
            .expect("R survives reopening");
        assert_eq!(reopened_r.contours[0].points[0].x, original_x + 2.0);
        assert_eq!(reopened_r.width, original_width + 4.0);
        assert_eq!(reopened_r.anchors.len(), original_anchor_count + 1);

        for (master_index, original) in original_fonts.into_iter().enumerate() {
            let mut normalized = reopened.font.project.masters[master_index].font.clone();
            if master_index == 0 {
                let original_r = original
                    .get_glyph("R")
                    .expect("the original Regular R exists")
                    .clone();
                normalized.default_layer_mut().insert_glyph(original_r);
            }
            assert_eq!(
                normalized, original,
                "only the edited Regular R may differ after the round trip"
            );
        }

        std::fs::remove_dir_all(root).expect("the disposable Virtua copy is removed");
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
        workspace.set_editor_text("B beside beh \u{0628}".into());
        workspace.preview_text = "B preview \u{0628}".into();
        workspace.text_dir = Some(runebender_core::text::buffer::TextDirection::RightToLeft);
        workspace.text_features_disabled.insert("rlig".into());
        workspace.text_script = Some("arab".into());
        workspace.text_language = Some("ur".into());
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
        assert_eq!(workspace.initial_text, "B beside beh \u{0628}");
        assert_eq!(workspace.preview_text, "B preview \u{0628}");
        assert_eq!(
            workspace.text_dir,
            Some(runebender_core::text::buffer::TextDirection::RightToLeft)
        );
        assert!(workspace.text_features_disabled.contains("rlig"));
        assert_eq!(workspace.text_script.as_deref(), Some("arab"));
        assert_eq!(workspace.text_language.as_deref(), Some("ur"));

        std::fs::remove_dir_all(path).expect("the empty UFO fixture is removed");
    }

    #[test]
    fn navigation_stays_clean_and_external_reload_never_overwrites_unsaved_work() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-external-reload-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("A"));
        font.save(&path).expect("the fixture saves");

        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        let index = workspace.font.index_of("A").expect("A exists");
        workspace.open_glyph(index);
        workspace.back_to_overview();
        assert!(!workspace.modified, "navigation alone is not an edit");

        workspace.open_glyph(index);
        workspace.set_advance_from_buf("604".into());
        assert!(workspace.modified);
        let unsaved_width = workspace.session.advance();

        let mut external = norad::Font::load(&path).expect("the fixture reloads externally");
        external
            .default_layer_mut()
            .get_glyph_mut("A")
            .expect("external A exists")
            .width = 712.0;
        external.save(&path).expect("the external change saves");
        workspace.reload_from_disk();

        assert_eq!(workspace.session.advance(), unsaved_width);
        assert_eq!(
            workspace.note,
            "sources changed on disk; Save As preserves your edits, or Revert to Saved accepts disk changes"
        );
        assert!(workspace.modified);
        assert!(
            !workspace.save(),
            "Save cannot overwrite the external change"
        );
        let still_external =
            norad::Font::load(&path).expect("the external source remains readable");
        assert_eq!(still_external.get_glyph("A").unwrap().width, 712.0);

        workspace.revert_to_saved();
        assert_eq!(workspace.session.advance(), 712.0);
        assert!(!workspace.modified);

        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn a_save_event_does_not_reload_away_undo_history() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-own-save-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("A");
        glyph.width = 500.0;
        font.default_layer_mut().insert_glyph(glyph);
        font.save(&path).expect("the fixture saves");

        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        workspace.open_glyph(0);
        workspace.set_advance_from_buf("620".into());
        assert!(workspace.save());
        assert!(workspace.font.master().can_undo(0));
        workspace.reload_from_disk();
        assert_eq!(workspace.session.advance(), 620.0);
        assert!(workspace.font.master().can_undo(0));
        workspace.undo_active_edit(false);
        assert_eq!(workspace.session.advance(), 500.0);
        workspace.revert_to_saved();
        assert_eq!(workspace.session.advance(), 620.0);
        assert!(!workspace.modified);
        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn glyph_metadata_validates_undoes_and_survives_save_reopen() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-metadata-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("A");
        glyph.width = 500.0;
        let mut contour = norad::Contour::default();
        for (x, y) in [(50.0, 0.0), (450.0, 0.0), (450.0, 700.0), (50.0, 700.0)] {
            contour.points.push(norad::ContourPoint::new(
                x,
                y,
                norad::PointType::Line,
                false,
                None,
                None,
            ));
        }
        glyph.contours.push(contour);
        font.default_layer_mut().insert_glyph(glyph);
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("B"));
        font.save(&path).expect("the metadata fixture saves");

        let mut workspace = Workspace::open(&path).expect("the metadata fixture opens");
        let index = workspace.font.index_of("A").expect("A exists");
        workspace.open_glyph(index);
        let original = workspace.session.glyph.clone();

        workspace.set_advance_from_buf("NaN".into());
        workspace.set_lsb_from_buf("inf".into());
        workspace.set_rsb_from_buf("-inf".into());
        workspace.set_unicode_from_buf("not hex".into());
        workspace.name_buf = "B".into();
        workspace.commit_rename();
        assert_eq!(workspace.session.glyph, original);
        assert!(!workspace.modified, "invalid fields do not dirty the font");
        assert_eq!(workspace.name_buf, "A");
        assert_eq!(workspace.note, "Cannot rename A to B");

        workspace.set_unicode_from_buf("U+0628".into());
        assert_eq!(
            workspace
                .session
                .glyph
                .codepoints
                .iter()
                .collect::<Vec<_>>(),
            ['\u{0628}']
        );
        workspace.undo_active_edit(false);
        assert!(workspace.session.glyph.codepoints.is_empty());
        workspace.undo_active_edit(true);
        assert_eq!(
            workspace
                .session
                .glyph
                .codepoints
                .iter()
                .collect::<Vec<_>>(),
            ['\u{0628}']
        );

        workspace.set_advance_from_buf("620".into());
        assert_eq!(workspace.session.advance(), 620.0);
        workspace.undo_active_edit(false);
        assert_eq!(workspace.session.advance(), 500.0);
        workspace.undo_active_edit(true);
        assert_eq!(workspace.session.advance(), 620.0);

        let before = workspace.session.side_bearings().expect("A has ink");
        workspace.set_lsb_from_buf("80".into());
        let shifted = workspace
            .session
            .side_bearings()
            .expect("shifted A has ink");
        assert_eq!(shifted.lsb, 80);
        assert_eq!(
            shifted.advance, before.advance,
            "LSB keeps the advance fixed"
        );
        assert_eq!(shifted.rsb, before.rsb - 30);
        workspace.undo_active_edit(false);
        let restored = workspace
            .session
            .side_bearings()
            .expect("restored A has ink");
        assert_eq!(restored.lsb, before.lsb);
        assert_eq!(restored.rsb, before.rsb);
        assert_eq!(restored.advance, before.advance);

        workspace.set_rsb_from_buf("200".into());
        assert_eq!(
            workspace.session.side_bearings().expect("A has ink").rsb,
            200
        );
        workspace.name_buf = "A.alt".into();
        workspace.commit_rename();
        assert_eq!(workspace.session.glyph_name, "A.alt");
        assert_eq!(workspace.note, "Renamed A to A.alt");
        workspace.undo_active_edit(false);
        assert_eq!(workspace.session.glyph_name, "A");
        assert_eq!(workspace.note, "Undid rename to A");
        workspace.undo_active_edit(false);
        assert_eq!(
            workspace.session.advance(),
            620.0,
            "the older RSB edit follows the rename"
        );
        workspace.undo_active_edit(true);
        assert_eq!(workspace.session.advance(), 650.0);
        workspace.undo_active_edit(true);
        assert_eq!(workspace.session.glyph_name, "A.alt");
        assert_eq!(workspace.note, "Redid rename to A.alt");
        assert!(workspace.modified);
        assert!(workspace.save());

        let reopened = Workspace::open(&path).expect("the saved metadata fixture reopens");
        assert!(reopened.font.font().get_glyph("A").is_none());
        let glyph = reopened
            .font
            .font()
            .get_glyph("A.alt")
            .expect("the renamed glyph survives reopening");
        assert_eq!(glyph.width, 650.0);
        assert_eq!(glyph.codepoints.iter().collect::<Vec<_>>(), ['\u{0628}']);
        std::fs::remove_dir_all(path).expect("the metadata fixture is removed");
    }

    #[test]
    fn unicode_and_rename_undo_atomically_across_masters() {
        let (dir, designspace) = two_master_designspace("metadata-masters");
        for source in [dir.join("Regular.ufo"), dir.join("Bold.ufo")] {
            let mut font = norad::Font::load(&source).expect("the master reloads");
            let mut glyph = norad::Glyph::new("A");
            glyph.width = 500.0;
            font.default_layer_mut().insert_glyph(glyph);
            font.save(&source).expect("the master with A saves");
        }
        let mut workspace = Workspace::open(&designspace).expect("the designspace opens");
        let index = workspace.font.index_of("A").expect("A exists");
        workspace.open_glyph(index);

        workspace.set_unicode_from_buf("U+0628".into());
        for master in &workspace.font.project.masters {
            assert_eq!(
                master
                    .font
                    .get_glyph("A")
                    .expect("A exists in every master")
                    .codepoints
                    .iter()
                    .collect::<Vec<_>>(),
                ['\u{0628}']
            );
        }
        workspace.undo_active_edit(false);
        assert!(workspace.font.project.masters.iter().all(|master| {
            master
                .font
                .get_glyph("A")
                .expect("A exists")
                .codepoints
                .is_empty()
        }));
        workspace.undo_active_edit(true);
        assert!(workspace.font.project.masters.iter().all(|master| {
            master
                .font
                .get_glyph("A")
                .expect("A exists")
                .codepoints
                .contains('\u{0628}')
        }));

        workspace.name_buf = "beh.test".into();
        workspace.commit_rename();
        assert!(
            workspace
                .font
                .project
                .masters
                .iter()
                .all(|master| master.font.get_glyph("beh.test").is_some())
        );
        workspace.undo_active_edit(false);
        assert!(
            workspace
                .font
                .project
                .masters
                .iter()
                .all(|master| master.font.get_glyph("A").is_some())
        );
        workspace.undo_active_edit(true);
        assert!(
            workspace
                .font
                .project
                .masters
                .iter()
                .all(|master| master.font.get_glyph("beh.test").is_some())
        );

        workspace.back_to_overview();
        workspace.overview_set_unicode("0041".into());
        assert!(workspace.font.project.masters.iter().all(|master| {
            master
                .font
                .get_glyph("beh.test")
                .expect("renamed glyph exists")
                .codepoints
                .contains('A')
        }));
        workspace.undo_active_edit(false);
        assert!(workspace.font.project.masters.iter().all(|master| {
            master
                .font
                .get_glyph("beh.test")
                .expect("renamed glyph exists")
                .codepoints
                .contains('\u{0628}')
        }));
        workspace.undo_active_edit(true);
        workspace.overview_set_advance("700".into());
        assert_eq!(
            workspace.font.font().get_glyph("beh.test").unwrap().width,
            700.0
        );
        workspace.undo_active_edit(false);
        assert_eq!(
            workspace.font.font().get_glyph("beh.test").unwrap().width,
            500.0
        );
        workspace.undo_active_edit(true);
        assert_eq!(
            workspace.font.font().get_glyph("beh.test").unwrap().width,
            700.0
        );

        assert!(workspace.save());
        let reopened = Workspace::open(&designspace).expect("saved masters reopen");
        for master in &reopened.font.project.masters {
            let glyph = master.font.get_glyph("beh.test").expect("rename persisted");
            assert!(glyph.codepoints.contains('A'));
        }
        std::fs::remove_dir_all(dir).expect("the fixture is removed");
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
    fn feature_draft_blocks_master_switch_reload_and_save() {
        let (dir, designspace) = two_master_designspace("feature-draft-safety");
        let mut workspace = Workspace::open(&designspace).expect("the designspace opens");
        let original = workspace.features_buf.clone();
        workspace.edit_features(format!("{original}\n# pending review\n"));

        workspace.set_master(1);
        assert_eq!(workspace.font.active(), 0);
        assert!(workspace.note.contains("Apply or Revert"));
        workspace.reload_from_disk();
        assert!(workspace.features_buf.ends_with("# pending review\n"));
        assert!(workspace.note.contains("Apply or Revert"));
        assert!(!workspace.save());
        assert!(workspace.note.contains("Apply or Revert"));

        workspace.revert_features();
        workspace.set_master(1);
        assert_eq!(workspace.font.active(), 1);
        assert_eq!(workspace.features_buf, workspace.font.font().features);
        std::fs::remove_dir_all(dir).expect("the designspace fixture is removed");
    }

    #[test]
    fn master_switch_rebuilds_all_tabs_and_keeps_undo_on_its_origin() {
        let (dir, designspace) = two_master_designspace("master-tab-undo");
        for (file, widths) in [
            ("Regular.ufo", [500.0, 600.0]),
            ("Bold.ufo", [700.0, 800.0]),
        ] {
            let path = dir.join(file);
            let mut font = norad::Font::load(&path).expect("the fixture master loads");
            for (name, width) in ["A", "B"].into_iter().zip(widths) {
                let mut glyph = norad::Glyph::new(name);
                glyph.width = width;
                font.default_layer_mut().insert_glyph(glyph);
            }
            font.save(&path)
                .expect("the populated fixture master saves");
        }

        let mut workspace = Workspace::open(&designspace).expect("the designspace opens");
        let a = workspace.font.index_of("A").unwrap();
        workspace.open_glyph(a);
        workspace.set_advance_from_buf("510".into());
        workspace.new_tab();
        let b = workspace.font.index_of("B").unwrap();
        workspace.open_glyph(b);
        workspace.set_advance_from_buf("610".into());
        let a_tab = workspace
            .tabs
            .iter()
            .position(|tab| tab.session.glyph_name == "A")
            .unwrap();
        workspace.activate_tab(a_tab);
        assert_eq!(workspace.session.advance(), 510.0);

        workspace.set_master(1);
        assert_eq!(workspace.session.glyph_name, "A");
        assert_eq!(workspace.session.advance(), 700.0);
        let b_tab = workspace
            .tabs
            .iter()
            .position(|tab| tab.session.glyph_name == "B")
            .unwrap();
        workspace.activate_tab(b_tab);
        assert_eq!(workspace.session.advance(), 800.0);
        workspace.set_advance_from_buf("820".into());
        workspace.undo_active_edit(false);
        assert_eq!(workspace.session.advance(), 800.0);

        workspace.set_master(0);
        assert_eq!(workspace.session.glyph_name, "B");
        assert_eq!(workspace.session.advance(), 610.0);
        workspace.undo_active_edit(false);
        assert_eq!(workspace.session.advance(), 600.0);
        workspace.set_master(1);
        assert_eq!(workspace.session.advance(), 800.0);

        workspace.set_master(0);
        let a_tab = workspace
            .tabs
            .iter()
            .position(|tab| tab.session.glyph_name == "A")
            .unwrap();
        workspace.activate_tab(a_tab);
        assert_eq!(workspace.session.advance(), 510.0);
        workspace.undo_active_edit(false);
        assert_eq!(workspace.session.advance(), 500.0);
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
    fn component_add_move_undo_and_save_reopen() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-components-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        let mut base = norad::Glyph::new("base");
        base.contours.push(norad::Contour::new(
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
                .into_iter()
                .map(|(x, y)| {
                    norad::ContourPoint::new(x, y, norad::PointType::Line, false, None, None)
                })
                .collect(),
            None,
        ));
        font.default_layer_mut().insert_glyph(base);
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("target"));
        font.save(&path).expect("the component fixture saves");

        let mut workspace = Workspace::open(&path).expect("the component fixture opens");
        let target = workspace.font.index_of("target").unwrap();
        workspace.open_glyph(target);
        workspace.component_base_buf = "base".into();
        workspace.command_add_component();
        assert_eq!(workspace.session.glyph.components.len(), 1);
        assert_eq!(workspace.session.selected_component_aligned(), Some(true));
        workspace.command_toggle_component_alignment();
        assert_eq!(workspace.session.selected_component_aligned(), Some(false));
        workspace.apply_op(|session| session.nudge(30.0, 40.0));
        assert_eq!(
            workspace.session.glyph.components[0].transform.x_offset,
            30.0
        );
        workspace.undo_active_edit(false);
        assert_eq!(
            workspace.session.glyph.components[0].transform.x_offset,
            0.0
        );
        workspace.undo_active_edit(false);
        assert_eq!(workspace.session.selected_component_aligned(), Some(true));
        workspace.undo_active_edit(false);
        assert!(workspace.session.glyph.components.is_empty());
        workspace.undo_active_edit(true);
        assert_eq!(workspace.session.glyph.components.len(), 1);
        workspace.undo_active_edit(true);
        assert!(
            runebender_core::document::composites::component_alignment_disabled(
                &workspace.session.glyph.components[0]
            )
        );
        workspace.undo_active_edit(true);
        assert_eq!(
            workspace.session.glyph.components[0].transform.y_offset,
            40.0
        );
        assert!(workspace.save());

        let reopened = Workspace::open(&path).expect("the edited component fixture reopens");
        let component = &reopened.font.font().get_glyph("target").unwrap().components[0];
        assert_eq!(component.base.as_str(), "base");
        assert_eq!(component.transform.x_offset, 30.0);
        assert_eq!(component.transform.y_offset, 40.0);
        std::fs::remove_dir_all(path).expect("the component fixture is removed");
    }

    #[test]
    fn moving_an_anchor_realigns_a_locked_mark_component() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-attachment-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        let mut mark = norad::Glyph::new("mark");
        mark.anchors.push(norad::Anchor::new(
            20.0,
            30.0,
            norad::Name::new("_top").ok(),
            None,
            None,
        ));
        font.default_layer_mut().insert_glyph(mark);
        let mut target = norad::Glyph::new("target");
        target.anchors.push(norad::Anchor::new(
            300.0,
            500.0,
            norad::Name::new("top").ok(),
            None,
            None,
        ));
        target.components.push(norad::Component::new(
            norad::Name::new("mark").unwrap(),
            norad::AffineTransform {
                x_offset: 280.0,
                y_offset: 470.0,
                ..Default::default()
            },
            None,
        ));
        font.default_layer_mut().insert_glyph(target);
        font.save(&path).expect("the attachment fixture saves");

        let mut workspace = Workspace::open(&path).expect("the attachment fixture opens");
        let target = workspace.font.index_of("target").unwrap();
        workspace.open_glyph(target);
        workspace.apply_op(|session| {
            session.move_anchor(0, 400.0, 600.0);
            session.end_metric_drag();
            true
        });
        let component = &workspace.session.glyph.components[0];
        assert_eq!(component.transform.x_offset, 380.0);
        assert_eq!(component.transform.y_offset, 570.0);
        workspace.undo_active_edit(false);
        let component = &workspace.session.glyph.components[0];
        assert_eq!(component.transform.x_offset, 280.0);
        assert_eq!(component.transform.y_offset, 470.0);
        std::fs::remove_dir_all(path).expect("the attachment fixture is removed");
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
        workspace.modified = true;
        workspace.font.master_mut().source_path = "/dev/null/runebender-test.ufo".into();

        assert!(!workspace.save());
        assert!(workspace.modified, "a failed save must remain dirty");
        assert!(workspace.note.starts_with("Save failed:"));

        std::fs::remove_dir_all(path).expect("the empty UFO fixture is removed");
    }

    #[test]
    fn view_menu_commands_update_reference_masters_and_sample_text() {
        let (dir, designspace) = two_master_designspace("view-menu");
        let mut workspace = Workspace::open(&designspace).expect("the designspace opens");
        workspace.filter = "A".into();
        workspace.new_glyph();

        workspace.dispatch(shortcuts::AppAction::ShowAllMasters);
        assert!(workspace.show_all_masters);
        assert_eq!(workspace.reference_layers.len(), 1);
        assert!(
            !workspace
                .reference_layers
                .contains(&workspace.font.active())
        );

        workspace.set_master(1);
        assert!(
            !workspace
                .reference_layers
                .contains(&workspace.font.active())
        );
        workspace.dispatch(shortcuts::AppAction::NextSampleString);
        assert_eq!(workspace.sample_index, 1);
        assert_eq!(workspace.preview_text, "nnonoonoo");
        workspace.dispatch(shortcuts::AppAction::PreviousSampleString);
        assert_eq!(workspace.sample_index, 0);
        assert_eq!(workspace.preview_text, "HHOHOHOO");

        workspace.dispatch(shortcuts::AppAction::GridLines);
        assert!(workspace.view.grid_lines);
        workspace.dispatch(shortcuts::AppAction::GridDots);
        assert!(!workspace.view.grid_lines);
        workspace.dispatch(shortcuts::AppAction::MeasureSizes);
        workspace.dispatch(shortcuts::AppAction::MeasureSpans);
        assert!(workspace.view.sizes && workspace.view.spans);
        workspace.dispatch(shortcuts::AppAction::MeasureAllOff);
        assert!(!workspace.view.sizes && !workspace.view.spans);

        std::fs::remove_dir_all(dir).expect("the designspace fixture is removed");
    }
}
