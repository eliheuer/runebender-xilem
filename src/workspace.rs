// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The editor's state: the `Workspace` struct and the types it is made of.

use crate::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sort {
    Name,
    Unicode,
}

/// The active sidebar selection: a category chip, a language group, or a
/// builtin/GF-coverage filter (mirrors runebender-gpui's `SidebarFilter`).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Sel {
    Category(GlyphCategory),
    /// A row under a category: the subfilter's id.
    Subfilter(GlyphCategory, &'static str),
    Language(usize),
    /// A row under a language group: the group and its filter index.
    LanguageFilter(usize, usize),
    Filter(usize),
}

/// The active editor tool.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tool {
    Select,
    Pen,
    Rect,
    Ellipse,
    HyperPen,
    Knife,
    Measure,
    /// Type glyphs into a line and edit them in context: the web
    /// editor's text tool, on runebender-core's text engine.
    Text,
}

/// The Core history entries which make up one overview action. Glyph names
/// remain stable across per-master sort orders, and `master` keeps an Undo
/// after a master switch attached to the source the user actually changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OverviewEditBatch {
    pub(crate) master: usize,
    pub(crate) glyphs: Vec<String>,
}

/// The font-wide values restored together by a metadata history step.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FontDataSnapshot {
    pub(crate) groups: norad::Groups,
    pub(crate) kerning: norad::Kerning,
    pub(crate) features: String,
}

/// A cross-master metadata edit and the active glyph-history depth immediately
/// before it. The depth keeps metadata Undo ordered with outline edits.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MetadataEdit {
    Rename {
        before: String,
        after: String,
        undo_depth: usize,
    },
    Unicode {
        glyph: String,
        before: Vec<Vec<char>>,
        after: Vec<Vec<char>>,
        undo_depth: usize,
    },
    FontData {
        glyph: String,
        before: Vec<FontDataSnapshot>,
        after: Vec<FontDataSnapshot>,
        label: String,
        undo_depth: usize,
    },
}

pub(crate) struct Workspace {
    /// Identity of this particular in-memory document session. Reopening or
    /// reloading the same path creates a new session, so background work can
    /// never apply a result to a replacement document by accident.
    pub(crate) document_id: u64,
    /// The live document's private agent endpoint, serviced on the UI thread.
    #[cfg(unix)]
    pub(crate) live: Option<runebender_core::document::live_socket::Server>,

    pub(crate) font: FontModel,
    pub(crate) palette: Arc<Palette>,
    pub(crate) cells: Arc<Vec<Cell>>,
    pub(crate) mode: Mode,
    pub(crate) selected: Option<usize>,
    pub(crate) multi_selected: Arc<std::collections::HashSet<usize>>,
    /// Atomic glyph batches edited from the overview, newest at the end.
    /// Core still owns each glyph snapshot; this records which snapshots make
    /// one user action so a multi-selection mark change undoes once.
    pub(crate) overview_undo: Vec<OverviewEditBatch>,
    pub(crate) overview_redo: Vec<OverviewEditBatch>,
    /// Cross-master metadata changes, ordered with the active glyph's Core pile.
    pub(crate) metadata_undo: Vec<MetadataEdit>,
    pub(crate) metadata_redo: Vec<MetadataEdit>,
    pub(crate) filter: String,
    /// The grid's Detail view: cells carry their category and advance.
    pub(crate) detail: bool,
    /// The List view in place of the grid, from the bottom bar's box.
    pub(crate) list: bool,
    /// Which tab the editor's left rail is showing.
    pub(crate) rail: Rail,
    /// Writing direction for the text tool, or `None` for automatic.
    /// The chips that set it are in the title bar, which is why this is
    /// application state and not the buffer's.
    pub(crate) text_dir: Option<runebender_core::text::buffer::TextDirection>,
    /// OpenType features disabled for text-tool and preview shaping.
    pub(crate) text_features_disabled: std::collections::HashSet<String>,
    /// Optional shaping script and language selected in the preview controls.
    pub(crate) text_script: Option<String>,
    pub(crate) text_language: Option<String>,
    /// Whether the left column is folded away, as the GPUI build's
    /// grid-icon button in the title bar does it.
    pub(crate) left_collapsed: bool,
    /// Sidebar groups that are folded shut, by title. The GPUI build's
    /// sidebar folds, and a font with four filter groups needs it.
    pub(crate) collapsed: std::collections::HashSet<&'static str>,
    pub(crate) sel: Sel,
    pub(crate) sort: Sort,
    /// Categories whose subfilter rows are open, by display name,
    /// since the category type carries no hash.
    pub(crate) expanded_categories: std::collections::HashSet<&'static str>,
    /// Language groups whose filter rows are open.
    pub(crate) expanded_scripts: std::collections::HashSet<usize>,
    /// Treat the search as a regular expression.
    pub(crate) search_regex: bool,
    /// The compiled search, when `search_regex` is on and it parses.
    pub(crate) search_re: Option<regex::Regex>,
    // Editor session, when a glyph is open. This is the live one: the
    // active tab's copy is only written back when tabs change.
    pub(crate) session: Arc<Session>,
    /// One parked session per open tab, in strip order. Each carries its
    /// own selection, viewport and undo stack, and it tracks its glyph by
    /// name, so a rename or a master switch does not lose it.
    pub(crate) tabs: Vec<Tab>,
    pub(crate) active_tab: usize,
    pub(crate) selected_points: usize,
    pub(crate) tool: Tool,
    pub(crate) modified: bool,
    pub(crate) note: String,
    /// Which analysis overlays the editor draws.
    pub(crate) view: canvas::editor::ViewOptions,
    /// What the text tool starts with, from `RUNEBENDER_TEXT`.
    pub(crate) initial_text: String,
    /// Grid cell size, driven by the bottom bar's zoom.
    pub(crate) cell_size: f64,
    pub(crate) advance_buf: String,
    pub(crate) lsb_buf: String,
    pub(crate) rsb_buf: String,
    pub(crate) name_buf: String,
    pub(crate) unicode_buf: String,
    /// Kerning group names for the open glyph, left side then right.
    pub(crate) kern1_buf: String,
    pub(crate) kern2_buf: String,
    /// Copied contours. An in-app clipboard, as in the GPUI build: the
    /// system clipboard carries text, not outlines.
    pub(crate) clipboard: Vec<norad::Contour>,
    /// Draw the UFO background layer under the outline.
    pub(crate) show_background: bool,
    /// A glyph name to show behind the drawing, empty for none.
    pub(crate) reference_buf: String,
    /// Base glyph typed in the Shapes panel when adding a component.
    pub(crate) component_base_buf: String,
    /// Current axis location in user units, one per designspace axis.
    pub(crate) axis_values: Vec<f64>,
    /// Active OKLCH theme id (dark | gray | light).
    pub(crate) theme_id: &'static str,
    /// Reference corner for the Coordinates fields (the 9-point picker).
    pub(crate) coord_quadrant: runebender_core::outline::path::Quadrant,
    pub(crate) coord_x_buf: String,
    pub(crate) coord_y_buf: String,
    pub(crate) coord_w_buf: String,
    pub(crate) coord_h_buf: String,
    /// Typed parameters shared by the Path Operations fields and Filter menu.
    pub(crate) slant_buf: String,
    pub(crate) offset_buf: String,
    pub(crate) extrude_buf: String,
    pub(crate) roughen_buf: String,
    pub(crate) roughen_seed: u64,
    /// Text rendered in the proof strip; empty shows the current glyph.
    pub(crate) preview_text: String,
    /// Gaussian blur radius for the proof strip in logical pixels.
    pub(crate) preview_blur: f64,
    /// Reverse the proof foreground and background contrast.
    pub(crate) preview_invert: bool,
    /// Search scope: 0 name and unicode, 1 name only, 2 unicode only.
    pub(crate) search_mode: u8,
    /// Case-sensitive search.
    pub(crate) search_case: bool,
    /// Masters drawn as ghost outlines under the active one. The Layers
    /// section toggles these, one per thumbnail click (gpui's eye).
    pub(crate) reference_layers: std::collections::HashSet<usize>,
    /// Whether every non-active master is shown as a reference outline.
    pub(crate) show_all_masters: bool,
    /// Current built-in proof string selected from the View menu.
    pub(crate) sample_index: usize,
    /// The nodes file, the files beside the font, and a run.
    pub(crate) nodes: nodes::NodesState,
    /// A font build running outside the UI thread.
    pub(crate) export_job: Option<export::ExportJob>,
    /// The Local AI panel: models, tasks, a run, proposals.
    pub(crate) ai: local_ai::LocalAiState,
    /// Local chat transcript, model choice, and current process.
    pub(crate) chat: chat::ChatState,
    /// The Kerning section's fields: filter, then the pair being
    /// edited.
    pub(crate) kern_filter_buf: String,
    pub(crate) kern_first_buf: String,
    pub(crate) kern_second_buf: String,
    pub(crate) kern_value_buf: String,
    /// The Groups section's name field.
    pub(crate) group_name_buf: String,
    /// The editable `features.fea` draft for the active master.
    pub(crate) features_buf: String,
    /// Whether `features_buf` differs from the active master's applied text.
    pub(crate) features_edited: bool,
    /// What the Features section last did.
    pub(crate) features_status: Option<String>,
}

/// The window's document boundary.
///
/// A running editor has a [`Workspace`]; the window itself can also exist
/// before a document has been opened, or after a command-line load fails.
/// Keeping that distinction outside `Workspace` preserves the latter's
/// invariant that every font-facing operation has a live core project.
pub(crate) struct AppState {
    /// The active font workspace, absent on the welcome screen.
    pub(crate) workspace: Option<Workspace>,
    /// Palette used before a document has supplied its workspace palette.
    pub(crate) palette: Arc<Palette>,
    /// Active palette before and after a workspace is opened.
    pub(crate) theme_id: &'static str,
    /// A load failure to report on the welcome screen.
    pub(crate) notice: Option<String>,
}

impl AppState {
    /// Opens `path`, or makes the no-document state when no path was supplied.
    pub(crate) fn open(path: Option<&std::path::Path>) -> Self {
        let theme_id = match std::env::var("RUNEBENDER_THEME").ok().as_deref() {
            Some("dark") => "dark",
            Some("light") => "light",
            _ => "gray",
        };
        let palette = Arc::new(Palette::load(theme_id));
        let Some(path) = path else {
            return Self {
                workspace: None,
                palette,
                theme_id,
                notice: None,
            };
        };
        let mut app = Self {
            workspace: None,
            palette,
            theme_id,
            notice: None,
        };
        app.open_path(path);
        app
    }

    /// Opens `path` as the current document without discarding a document when
    /// core rejects the new source.
    ///
    /// Core owns format dispatch and conversion through [`Workspace::open`].
    /// This application boundary only changes which workspace the window shows.
    pub(crate) fn open_path(&mut self, path: &std::path::Path) -> bool {
        if self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.modified)
        {
            let error = "Save or discard changes before opening another font".to_string();
            if let Some(workspace) = self.workspace.as_mut() {
                workspace.note = error.clone();
            }
            self.notice = Some(error);
            return false;
        }
        match Workspace::open(path) {
            Ok(workspace) => {
                self.theme_id = workspace.theme_id;
                self.palette = workspace.palette.clone();
                self.workspace = Some(workspace);
                self.notice = None;
                true
            }
            Err(error) => {
                if let Some(workspace) = self.workspace.as_mut() {
                    workspace.note = error.clone();
                }
                self.notice = Some(error);
                false
            }
        }
    }

    /// The base colour of whichever window state is active.
    pub(crate) fn background(&self) -> xilem::Color {
        self.workspace
            .as_ref()
            .map(|workspace| workspace.palette.app)
            .unwrap_or(self.palette.app)
    }

    /// Dispatch an application command, routing document commands to the live
    /// workspace while keeping application-level theme state available on the
    /// welcome screen.
    pub(crate) fn dispatch(&mut self, action: shortcuts::AppAction) {
        if action == shortcuts::AppAction::Quit {
            // MenuShell exits through its driver context; macOS uses the
            // platform application menu. Quit is never a state mutation.
            return;
        }
        if let shortcuts::AppAction::Theme(id) = action {
            self.theme_id = id;
            self.palette = Arc::new(Palette::load(id));
        }
        if action == shortcuts::AppAction::OpenFont {
            let directory = self
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.font.document_source().parent())
                .unwrap_or_else(|| std::path::Path::new("."));
            if let Some(path) = dialogs::font(directory) {
                self.open_path(&path);
            }
            return;
        }
        if action == shortcuts::AppAction::NewFont && self.workspace.is_none() {
            let path = std::env::temp_dir().join(format!(
                "Runebender-Untitled-{}-{}.ufo",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |duration| duration.as_nanos()),
            ));
            let font = runebender_core::document::new_font::new_font("Untitled", "Regular", 400);
            match font.save(&path) {
                Ok(()) => {
                    self.open_path(&path);
                    if let Some(workspace) = self.workspace.as_mut() {
                        workspace.note = "new font · Save As picks where it lives".into();
                    }
                }
                Err(error) => self.notice = Some(format!("could not create new font: {error}")),
            }
            return;
        }
        if let Some(workspace) = self.workspace.as_mut() {
            workspace.dispatch(action);
            self.theme_id = workspace.theme_id;
            self.palette = workspace.palette.clone();
        }
    }
}

/// Which surface is showing.
pub(crate) enum Mode {
    /// The glyph grid.
    Overview,
    /// The editor, on the glyph at this index.
    Editor(usize),
    /// The nodes canvas: the open `.nodes.json` as boxes and wires.
    Nodes,
}

/// One editing tab: a parked session and the tool it was left on.
pub(crate) struct Tab {
    /// Stable widget-state identity, distinct even for two tabs on one glyph.
    pub(crate) text_context_id: u64,
    pub(crate) session: Arc<Session>,
    pub(crate) tool: Tool,
    pub(crate) text_context: TextContext,
}

/// Plain-data text and preview state parked with an editor tab.
#[derive(Clone, Default)]
pub(crate) struct TextContext {
    pub(crate) editor_text: String,
    pub(crate) preview_text: String,
    pub(crate) direction: Option<runebender_core::text::buffer::TextDirection>,
    pub(crate) features_disabled: std::collections::HashSet<String>,
    pub(crate) script: Option<String>,
    pub(crate) language: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_path_starts_without_a_document() {
        let app = AppState::open(None);
        assert!(app.workspace.is_none());
        assert_eq!(app.notice, None);
    }

    #[test]
    fn failed_path_keeps_the_window_open_with_an_error() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-missing-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let app = AppState::open(Some(&path));
        assert!(app.workspace.is_none());
        assert!(app.notice.is_some());
    }

    #[test]
    fn failed_open_preserves_the_current_document() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-open-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        norad::Font::new()
            .save(&path)
            .expect("the empty UFO fixture saves");
        let mut app = AppState::open(Some(&path));
        let source = app
            .workspace
            .as_ref()
            .expect("the fixture opens")
            .font
            .source()
            .to_path_buf();

        let missing = path.with_file_name("runebender-xilem-does-not-exist.ufo");
        assert!(!app.open_path(&missing));
        assert_eq!(
            app.workspace
                .as_ref()
                .expect("the earlier document remains")
                .font
                .source(),
            source,
        );
        assert!(app.notice.is_some());
        assert_eq!(
            app.workspace
                .as_ref()
                .expect("the earlier document remains")
                .note,
            app.notice.as_deref().expect("the failure is retained"),
        );

        std::fs::remove_dir_all(path).expect("the empty UFO fixture is removed");
    }

    #[test]
    fn open_keeps_a_dirty_document_open() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-dirty-open-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let replacement = path.with_file_name("runebender-xilem-dirty-open-replacement.ufo");
        norad::Font::new()
            .save(&path)
            .expect("the source fixture saves");
        norad::Font::new()
            .save(&replacement)
            .expect("the replacement fixture saves");
        let mut app = AppState::open(Some(&path));
        app.workspace
            .as_mut()
            .expect("the source fixture opens")
            .modified = true;

        assert!(!app.open_path(&replacement));
        assert_eq!(
            app.workspace
                .as_ref()
                .expect("the source remains open")
                .font
                .source(),
            path
        );
        assert_eq!(
            app.notice.as_deref(),
            Some("Save or discard changes before opening another font")
        );

        std::fs::remove_dir_all(path).expect("the source fixture is removed");
        std::fs::remove_dir_all(replacement).expect("the replacement fixture is removed");
    }
}
