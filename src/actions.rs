// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A native menu bar, and one action list behind both it and the keymap.
//!
//! This is the piece Xilem cannot supply. Xilem is built on winit, which
//! has no menu API of any kind, so Masonry never inherited menus, and a
//! font editor with no File menu is not a font editor anyone will use.
//!
//! The shape here is GPUI's: an action is a plain value, a table binds a
//! key and a menu position to it, and both paths run the same code. What
//! this file adds on top is the plumbing Xilem does not own:
//!
//! - The menu bar is built with `muda`, on the main thread, the first
//!   time the view function runs. On macOS `init_for_nsapp` attaches to
//!   the application rather than a window, so no window handle is needed,
//!   which matters because Xilem does not hand one out.
//! - Menu clicks do not arrive through winit's event loop. They land on
//!   muda's own global channel, so a `task` view drains that channel on
//!   the runtime and posts each one back into the application.
//!
//! The Linux and Windows halves are not here. muda's menu bar wants a GTK
//! window on Linux and an HWND on Windows, and winit gives out neither
//! through Xilem, so those platforms need an in-window menu bar drawn
//! with Masonry's layer system. That split is most of why this is not
//! simply a pull request against Xilem.

use crate::widgets::shortcuts::AppAction;
use crate::{AppState, Tool};
use masonry::core::keyboard::{Key, Modifiers, NamedKey};

/// One row of the application's action table.
///
/// The same row supplies the menu item's title, its accelerator label,
/// and the action both the menu and the keymap fire. They cannot drift,
/// because there is one of them.
// Only the native menu bar reads the table, and that exists on macOS.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "the menu table is read by the native menu bar, which is macOS only"
    )
)]
pub(crate) struct Entry {
    /// Which menu it belongs under.
    pub menu: &'static str,
    /// The item's title.
    pub title: &'static str,
    /// An accelerator in muda's syntax, if it has one.
    pub accelerator: Option<&'static str>,
    /// What it does.
    pub action: AppAction,
}

impl Entry {
    /// Whether a visual group begins before this row.
    pub(crate) fn separator_before(&self) -> bool {
        use AppAction as A;
        (self.menu == "Filter" && self.action == A::AddExtremes)
            || matches!(
                self.action,
                A::Save
                    | A::ExportFont
                    | A::NodesRun
                    | A::Copy
                    | A::SelectAll
                    | A::UpdateMetrics
                    | A::CheckJoining
                    | A::TraceImage
                    | A::Decompose
                    | A::CorrectPathDirection
                    | A::RemoveOverlap
                    | A::FlipHorizontal
                    | A::Harmonize
                    | A::HyperToCubic
                    | A::SortByName
                    | A::NextMaster
                    | A::ShowAllMasters
                    | A::NextSampleString
                    | A::GridDots
                    | A::MeasureColorize
                    | A::Theme(_)
                    | A::MeasurePopcount
                    | A::MeasureAllOn
            )
    }

    /// Nested menu shared by native and in-window renderers.
    pub(crate) fn submenu(&self) -> Option<&'static str> {
        use AppAction as A;
        match self.action {
            A::GridDots | A::GridLines => Some("Grid"),
            A::Theme(_) => Some("Theme"),
            A::MeasureColorize
            | A::MeasureHandles
            | A::MeasureSegments
            | A::MeasureSizes
            | A::MeasureSpans
            | A::MeasureSideBearings
            | A::MeasurePopcount
            | A::MeasureAllOn
            | A::MeasureAllOff => Some("Measure"),
            _ => None,
        }
    }

    /// Whether the command can change the current workspace.
    pub(crate) fn enabled(&self, app: &AppState) -> bool {
        use AppAction as A;
        let Some(app) = app.workspace.as_ref() else {
            return matches!(
                self.action,
                A::Quit | A::NewFont | A::OpenFont | A::Theme(_)
            );
        };
        let editor = matches!(app.mode, crate::Mode::Editor(_));
        match self.action {
            A::Save => app.modified && app.font.is_writable(),
            A::ExportFont => app.export_job.is_none(),
            A::Undo => match app.mode {
                crate::Mode::Editor(index) => app.font.master().can_undo(index),
                _ => false,
            },
            A::Redo => match app.mode {
                crate::Mode::Editor(index) => app.font.master().can_redo(index),
                _ => false,
            },
            A::Copy | A::SelectAll => editor,
            A::Paste => editor && !app.clipboard.is_empty(),
            A::CopySelectedGlyphs => app.selected.is_some() || !app.multi_selected.is_empty(),
            A::DeselectAll
            | A::InvertSelection
            | A::SetStartPoint
            | A::RoundCorners
            | A::Harmonize
            | A::Balance
            | A::Optimize => editor && app.selected_points > 0,
            A::FlipHorizontal
            | A::FlipVertical
            | A::Rotate90
            | A::RotateRight
            | A::Rotate180
            | A::RemoveOverlap
            | A::BooleanUnion
            | A::BooleanSubtract
            | A::BooleanIntersect
            | A::BooleanExclude
            | A::Decompose
            | A::Duplicate
            | A::DuplicateRepeat
            | A::ReverseContours
            | A::TidyPaths
            | A::AddExtremes
            | A::RoundCoordinates
            | A::CorrectPathDirection
            | A::HyperToCubic
            | A::QuadsToCubics
            | A::CubicsToQuads
            | A::ZoomToFit => editor,
            A::FilterOffset | A::FilterExtrude | A::FilterRoughen | A::FilterSlant => editor,
            A::GenerateMissing => matches!(app.sel, crate::Sel::Filter(_)),
            A::DuplicateGlyph | A::RemoveGlyph | A::BakeMasks | A::ExportGlyphSvg => {
                app.selected.is_some()
            }
            A::TraceImage | A::PlaceImage | A::ImportSvg => editor,
            A::BoldenWithModel => editor && app.ai.job.is_none(),
            A::RemoveImage => editor && app.session.glyph.image.is_some(),
            A::Reinterpolate => app.selected.is_some() && app.font.master_count() > 1,
            A::NextMaster | A::PreviousMaster => app.font.master_count() > 1,
            A::ShowAllMasters | A::NextSampleString | A::PreviousSampleString => editor,
            A::NodesSave => app.nodes.graph.is_some(),
            _ => true,
        }
    }

    /// Checked state for choice and toggle commands; `None` for ordinary rows.
    pub(crate) fn checked(&self, app: &AppState) -> Option<bool> {
        use AppAction as A;
        let workspace = app.workspace.as_ref();
        match self.action {
            A::SortByName => workspace.map(|app| app.sort == crate::Sort::Name),
            A::SortByUnicode => workspace.map(|app| app.sort == crate::Sort::Unicode),
            A::Theme(id) => Some(app.theme_id == id),
            A::ShowAllMasters => workspace.map(|app| app.show_all_masters),
            A::GridDots => workspace.map(|app| !app.view.grid_lines),
            A::GridLines => workspace.map(|app| app.view.grid_lines),
            A::MeasureColorize => workspace.map(|app| app.view.colorize),
            A::MeasureHandles => workspace.map(|app| app.view.handles),
            A::MeasureSegments => workspace.map(|app| app.view.segments),
            A::MeasureSizes => workspace.map(|app| app.view.sizes),
            A::MeasureSpans => workspace.map(|app| app.view.spans),
            A::MeasureSideBearings => workspace.map(|app| app.view.bearings),
            A::MeasurePopcount => workspace.map(|app| app.view.popcount),
            A::Tool(tool) => workspace.map(|app| app.tool == tool),
            _ => None,
        }
    }
}

/// Every action the application exposes, in menu order.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "the menu table is read by the native menu bar, which is macOS only"
    )
)]
pub(crate) const ACTIONS: &[Entry] = &[
    Entry {
        menu: "Runebender",
        title: "Quit Runebender",
        accelerator: Some("CmdOrCtrl+Q"),
        action: AppAction::Quit,
    },
    Entry {
        menu: "File",
        title: "New Font",
        accelerator: Some("CmdOrCtrl+N"),
        action: AppAction::NewFont,
    },
    Entry {
        menu: "File",
        title: "Open…",
        accelerator: Some("CmdOrCtrl+O"),
        action: AppAction::OpenFont,
    },
    Entry {
        menu: "File",
        title: "Save",
        accelerator: Some("CmdOrCtrl+S"),
        action: AppAction::Save,
    },
    Entry {
        menu: "File",
        title: "Save As…",
        accelerator: Some("CmdOrCtrl+Shift+S"),
        action: AppAction::SaveAs,
    },
    Entry {
        menu: "File",
        title: "Export…",
        accelerator: Some("CmdOrCtrl+Alt+E"),
        action: AppAction::ExportFont,
    },
    Entry {
        menu: "Nodes",
        title: "New Nodes",
        accelerator: None,
        action: AppAction::NodesNew,
    },
    Entry {
        menu: "Nodes",
        title: "Open Nodes…",
        accelerator: None,
        action: AppAction::NodesOpen,
    },
    Entry {
        menu: "Nodes",
        title: "Save Nodes",
        accelerator: None,
        action: AppAction::NodesSave,
    },
    Entry {
        menu: "Nodes",
        title: "Run Nodes",
        accelerator: None,
        action: AppAction::NodesRun,
    },
    Entry {
        menu: "Edit",
        title: "Undo",
        accelerator: Some("CmdOrCtrl+Z"),
        action: AppAction::Undo,
    },
    Entry {
        menu: "Edit",
        title: "Redo",
        accelerator: Some("CmdOrCtrl+Shift+Z"),
        action: AppAction::Redo,
    },
    Entry {
        menu: "Edit",
        title: "Copy",
        accelerator: Some("CmdOrCtrl+C"),
        action: AppAction::Copy,
    },
    Entry {
        menu: "Edit",
        title: "Paste",
        accelerator: Some("CmdOrCtrl+V"),
        action: AppAction::Paste,
    },
    Entry {
        menu: "Edit",
        title: "Copy Selected Glyphs as Text",
        accelerator: None,
        action: AppAction::CopySelectedGlyphs,
    },
    Entry {
        menu: "Edit",
        title: "Select All",
        accelerator: Some("CmdOrCtrl+A"),
        action: AppAction::SelectAll,
    },
    Entry {
        menu: "Edit",
        title: "Deselect All",
        accelerator: Some("CmdOrCtrl+Alt+A"),
        action: AppAction::DeselectAll,
    },
    Entry {
        menu: "Edit",
        title: "Invert Selection",
        accelerator: Some("CmdOrCtrl+Alt+Shift+I"),
        action: AppAction::InvertSelection,
    },
    Entry {
        menu: "Glyph",
        title: "New Glyph",
        accelerator: None,
        action: AppAction::NewGlyph,
    },
    Entry {
        menu: "Glyph",
        title: "Duplicate Glyph",
        accelerator: None,
        action: AppAction::DuplicateGlyph,
    },
    Entry {
        menu: "Glyph",
        title: "Remove Glyph",
        accelerator: None,
        action: AppAction::RemoveGlyph,
    },
    Entry {
        menu: "Glyph",
        title: "Update Metrics",
        accelerator: None,
        action: AppAction::UpdateMetrics,
    },
    Entry {
        menu: "Glyph",
        title: "Reinterpolate",
        accelerator: None,
        action: AppAction::Reinterpolate,
    },
    Entry {
        menu: "Glyph",
        title: "Generate Missing Glyphs",
        accelerator: None,
        action: AppAction::GenerateMissing,
    },
    Entry {
        menu: "Glyph",
        title: "Decompose Components",
        accelerator: None,
        action: AppAction::Decompose,
    },
    Entry {
        menu: "Glyph",
        title: "Check Joining",
        accelerator: None,
        action: AppAction::CheckJoining,
    },
    Entry {
        menu: "Glyph",
        title: "Compose from Anchors",
        accelerator: None,
        action: AppAction::ComposeFromAnchors,
    },
    Entry {
        menu: "Glyph",
        title: "Bake Masks",
        accelerator: None,
        action: AppAction::BakeMasks,
    },
    Entry {
        menu: "Glyph",
        title: "Export Glyph as SVG",
        accelerator: None,
        action: AppAction::ExportGlyphSvg,
    },
    Entry {
        menu: "Glyph",
        title: "Trace Image…",
        accelerator: None,
        action: AppAction::TraceImage,
    },
    Entry {
        menu: "Glyph",
        title: "Bolden With Model…",
        accelerator: None,
        action: AppAction::BoldenWithModel,
    },
    Entry {
        menu: "Glyph",
        title: "Place Image…",
        accelerator: None,
        action: AppAction::PlaceImage,
    },
    Entry {
        menu: "Glyph",
        title: "Import SVG…",
        accelerator: None,
        action: AppAction::ImportSvg,
    },
    Entry {
        menu: "Glyph",
        title: "Remove Image",
        accelerator: None,
        action: AppAction::RemoveImage,
    },
    Entry {
        menu: "Path",
        title: "Tidy Up Paths",
        accelerator: Some("CmdOrCtrl+Shift+T"),
        action: AppAction::TidyPaths,
    },
    Entry {
        menu: "Path",
        title: "Add Extremes",
        accelerator: None,
        action: AppAction::AddExtremes,
    },
    Entry {
        menu: "Path",
        title: "Round Coordinates",
        accelerator: None,
        action: AppAction::RoundCoordinates,
    },
    Entry {
        menu: "Path",
        title: "Correct Path Direction",
        accelerator: Some("CmdOrCtrl+Shift+R"),
        action: AppAction::CorrectPathDirection,
    },
    Entry {
        menu: "Path",
        title: "Reverse Contours",
        accelerator: Some("CmdOrCtrl+Alt+Shift+R"),
        action: AppAction::ReverseContours,
    },
    Entry {
        menu: "Path",
        title: "Set Start Point",
        accelerator: None,
        action: AppAction::SetStartPoint,
    },
    Entry {
        menu: "Path",
        title: "Remove Overlap",
        accelerator: Some("CmdOrCtrl+Shift+O"),
        action: AppAction::RemoveOverlap,
    },
    Entry {
        menu: "Path",
        title: "Union",
        accelerator: None,
        action: AppAction::BooleanUnion,
    },
    Entry {
        menu: "Path",
        title: "Subtract",
        accelerator: None,
        action: AppAction::BooleanSubtract,
    },
    Entry {
        menu: "Path",
        title: "Intersect",
        accelerator: None,
        action: AppAction::BooleanIntersect,
    },
    Entry {
        menu: "Path",
        title: "Exclude",
        accelerator: None,
        action: AppAction::BooleanExclude,
    },
    Entry {
        menu: "Path",
        title: "Flip Horizontal",
        accelerator: Some("CmdOrCtrl+Shift+H"),
        action: AppAction::FlipHorizontal,
    },
    Entry {
        menu: "Path",
        title: "Flip Vertical",
        accelerator: Some("CmdOrCtrl+Shift+V"),
        action: AppAction::FlipVertical,
    },
    Entry {
        menu: "Path",
        title: "Rotate 90° Left",
        accelerator: None,
        action: AppAction::Rotate90,
    },
    Entry {
        menu: "Path",
        title: "Rotate 90° Right",
        accelerator: None,
        action: AppAction::RotateRight,
    },
    Entry {
        menu: "Path",
        title: "Rotate 180°",
        accelerator: None,
        action: AppAction::Rotate180,
    },
    Entry {
        menu: "Path",
        title: "Duplicate Selection",
        accelerator: Some("CmdOrCtrl+D"),
        action: AppAction::Duplicate,
    },
    Entry {
        menu: "Path",
        title: "Duplicate + Repeat",
        accelerator: Some("CmdOrCtrl+Shift+D"),
        action: AppAction::DuplicateRepeat,
    },
    Entry {
        menu: "Path",
        title: "Harmonize",
        accelerator: None,
        action: AppAction::Harmonize,
    },
    Entry {
        menu: "Path",
        title: "Balance",
        accelerator: None,
        action: AppAction::Balance,
    },
    Entry {
        menu: "Path",
        title: "Optimize",
        accelerator: None,
        action: AppAction::Optimize,
    },
    Entry {
        menu: "Path",
        title: "Hyperbezier to Cubic",
        accelerator: None,
        action: AppAction::HyperToCubic,
    },
    Entry {
        menu: "Path",
        title: "Quadratic to Cubic",
        accelerator: None,
        action: AppAction::QuadsToCubics,
    },
    Entry {
        menu: "Path",
        title: "Cubic to Quadratic",
        accelerator: None,
        action: AppAction::CubicsToQuads,
    },
    Entry {
        menu: "Filter",
        title: "Offset Curve",
        accelerator: None,
        action: AppAction::FilterOffset,
    },
    Entry {
        menu: "Filter",
        title: "Extrude",
        accelerator: None,
        action: AppAction::FilterExtrude,
    },
    Entry {
        menu: "Filter",
        title: "Roughen",
        accelerator: None,
        action: AppAction::FilterRoughen,
    },
    Entry {
        menu: "Filter",
        title: "Slanter",
        accelerator: None,
        action: AppAction::FilterSlant,
    },
    Entry {
        menu: "Filter",
        title: "Round Corners",
        accelerator: None,
        action: AppAction::RoundCorners,
    },
    Entry {
        menu: "Filter",
        title: "Add Extremes",
        accelerator: None,
        action: AppAction::AddExtremes,
    },
    Entry {
        menu: "Filter",
        title: "Remove Overlap",
        accelerator: None,
        action: AppAction::RemoveOverlap,
    },
    Entry {
        menu: "View",
        title: "Zoom to Fit",
        accelerator: Some("CmdOrCtrl+0"),
        action: AppAction::ZoomToFit,
    },
    Entry {
        menu: "View",
        title: "Show All Masters",
        accelerator: None,
        action: AppAction::ShowAllMasters,
    },
    Entry {
        menu: "View",
        title: "Sort Glyphs by Name",
        accelerator: None,
        action: AppAction::SortByName,
    },
    Entry {
        menu: "View",
        title: "Sort Glyphs by Unicode",
        accelerator: None,
        action: AppAction::SortByUnicode,
    },
    Entry {
        menu: "View",
        title: "Next Master",
        accelerator: None,
        action: AppAction::NextMaster,
    },
    Entry {
        menu: "View",
        title: "Previous Master",
        accelerator: None,
        action: AppAction::PreviousMaster,
    },
    Entry {
        menu: "View",
        title: "Next Sample String",
        accelerator: None,
        action: AppAction::NextSampleString,
    },
    Entry {
        menu: "View",
        title: "Previous Sample String",
        accelerator: None,
        action: AppAction::PreviousSampleString,
    },
    Entry {
        menu: "View",
        title: "Dots",
        accelerator: None,
        action: AppAction::GridDots,
    },
    Entry {
        menu: "View",
        title: "Lines",
        accelerator: None,
        action: AppAction::GridLines,
    },
    Entry {
        menu: "View",
        title: "Colorize Outline",
        accelerator: None,
        action: AppAction::MeasureColorize,
    },
    Entry {
        menu: "View",
        title: "Handle Lengths",
        accelerator: None,
        action: AppAction::MeasureHandles,
    },
    Entry {
        menu: "View",
        title: "Segment Lengths",
        accelerator: None,
        action: AppAction::MeasureSegments,
    },
    Entry {
        menu: "View",
        title: "Segment Sizes",
        accelerator: None,
        action: AppAction::MeasureSizes,
    },
    Entry {
        menu: "View",
        title: "Stems & Counters",
        accelerator: None,
        action: AppAction::MeasureSpans,
    },
    Entry {
        menu: "View",
        title: "Side Bearings",
        accelerator: None,
        action: AppAction::MeasureSideBearings,
    },
    Entry {
        menu: "View",
        title: "Popcount Sums",
        accelerator: None,
        action: AppAction::MeasurePopcount,
    },
    Entry {
        menu: "View",
        title: "All On",
        accelerator: None,
        action: AppAction::MeasureAllOn,
    },
    Entry {
        menu: "View",
        title: "All Off",
        accelerator: None,
        action: AppAction::MeasureAllOff,
    },
    Entry {
        menu: "View",
        title: "Dark",
        accelerator: None,
        action: AppAction::Theme("dark"),
    },
    Entry {
        menu: "View",
        title: "Gray",
        accelerator: None,
        action: AppAction::Theme("gray"),
    },
    Entry {
        menu: "View",
        title: "Light",
        accelerator: None,
        action: AppAction::Theme("light"),
    },
    // Shortcut-only commands. GPUI exposes tools in the chrome rather than a
    // top-level Tools menu, and Escape returns to the overview without a row.
    Entry {
        menu: "",
        title: "Overview",
        accelerator: Some("Escape"),
        action: AppAction::Overview,
    },
    Entry {
        menu: "",
        title: "Cycle Theme",
        accelerator: Some("CmdOrCtrl+T"),
        action: AppAction::CycleTheme,
    },
    Entry {
        menu: "",
        title: "Show Nodes",
        accelerator: None,
        action: AppAction::NodesTab,
    },
    Entry {
        menu: "",
        title: "Select",
        accelerator: Some("V"),
        action: AppAction::Tool(Tool::Select),
    },
    Entry {
        menu: "",
        title: "Pen",
        accelerator: Some("P"),
        action: AppAction::Tool(Tool::Pen),
    },
    Entry {
        menu: "",
        title: "Hyper Pen",
        accelerator: Some("B"),
        action: AppAction::Tool(Tool::HyperPen),
    },
    Entry {
        menu: "",
        title: "Rectangle",
        accelerator: Some("U"),
        action: AppAction::Tool(Tool::Rect),
    },
    Entry {
        menu: "",
        title: "Ellipse",
        accelerator: Some("O"),
        action: AppAction::Tool(Tool::Ellipse),
    },
    Entry {
        menu: "",
        title: "Knife",
        accelerator: Some("E"),
        action: AppAction::Tool(Tool::Knife),
    },
    Entry {
        menu: "",
        title: "Measure",
        accelerator: Some("M"),
        action: AppAction::Tool(Tool::Measure),
    },
];

/// The order menus appear in the bar.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "the menu table is read by the native menu bar, which is macOS only"
    )
)]
pub(crate) const MENUS: &[&str] = &[
    "Runebender",
    "File",
    "Nodes",
    "Edit",
    "Glyph",
    "Path",
    "Filter",
    "View",
];

/// Resolve a key press from the same accelerator metadata used by both menu bars.
pub(crate) fn action_for_key(key: &Key, modifiers: Modifiers) -> Option<AppAction> {
    action_for_key_impl(key, modifiers, false)
}

/// Resolve a key for the in-window menu, including commands owned by the native
/// application menu when the same code is exercised on macOS.
pub(crate) fn action_for_key_in_window(key: &Key, modifiers: Modifiers) -> Option<AppAction> {
    action_for_key_impl(key, modifiers, true)
}

fn action_for_key_impl(
    key: &Key,
    modifiers: Modifiers,
    include_platform_commands: bool,
) -> Option<AppAction> {
    ACTIONS.iter().find_map(|entry| {
        #[cfg(target_os = "macos")]
        if entry.action == AppAction::Quit && !include_platform_commands {
            // The native application menu owns Cmd-Q on macOS.
            return None;
        }
        let accelerator = entry.accelerator?;
        let command = modifiers.meta() || modifiers.ctrl();
        let wants_command = accelerator.contains("CmdOrCtrl+");
        let wants_shift = accelerator.contains("Shift+");
        let wants_alt = accelerator.contains("Alt+");
        if command != wants_command
            || modifiers.shift() != wants_shift
            || modifiers.alt() != wants_alt
        {
            return None;
        }
        let expected = accelerator.rsplit('+').next()?;
        let matches = if expected == "Escape" {
            matches!(key, Key::Named(NamedKey::Escape))
        } else {
            matches!(key, Key::Character(c) if c.eq_ignore_ascii_case(expected))
        };
        matches.then_some(entry.action)
    })
}

/// Whether `action` is currently available, using its shared menu predicate.
pub(crate) fn action_enabled(action: AppAction, app: &AppState) -> bool {
    ACTIONS
        .iter()
        .find(|entry| entry.action == action)
        .is_some_and(|entry| entry.enabled(app))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn accelerator_metadata_is_the_shortcut_map() {
        assert_eq!(
            action_for_key(&Key::Character("s".into()), Modifiers::META),
            Some(AppAction::Save)
        );
        assert_eq!(
            action_for_key(&Key::Character("p".into()), Modifiers::empty()),
            Some(AppAction::Tool(Tool::Pen))
        );
        assert_eq!(
            action_for_key(&Key::Named(NamedKey::Escape), Modifiers::empty()),
            Some(AppAction::Overview)
        );
    }

    #[test]
    fn accelerators_are_unique_and_every_row_has_a_known_home() {
        let mut accelerators = HashSet::new();
        for entry in ACTIONS {
            assert!(
                entry.menu.is_empty() || MENUS.contains(&entry.menu),
                "unknown menu for {}: {}",
                entry.title,
                entry.menu
            );
            if let Some(accelerator) = entry.accelerator {
                assert!(
                    accelerators.insert(accelerator),
                    "duplicate accelerator: {accelerator}"
                );
            }
        }
    }

    #[test]
    fn menu_and_submenu_order_matches_the_gpui_reference() {
        assert_eq!(
            MENUS,
            &[
                "Runebender",
                "File",
                "Nodes",
                "Edit",
                "Glyph",
                "Path",
                "Filter",
                "View"
            ]
        );
        let view: Vec<_> = ACTIONS
            .iter()
            .filter(|entry| entry.menu == "View")
            .map(|entry| (entry.title, entry.submenu()))
            .collect();
        assert_eq!(view[0], ("Zoom to Fit", None));
        assert_eq!(view[8], ("Dots", Some("Grid")));
        assert_eq!(view[10], ("Colorize Outline", Some("Measure")));
        assert_eq!(view[19], ("Dark", Some("Theme")));
        assert_eq!(view.last(), Some(&("Light", Some("Theme"))));

        let glyph: Vec<_> = ACTIONS
            .iter()
            .filter(|entry| entry.menu == "Glyph")
            .map(|entry| entry.title)
            .collect();
        assert_eq!(
            glyph,
            [
                "New Glyph",
                "Duplicate Glyph",
                "Remove Glyph",
                "Update Metrics",
                "Reinterpolate",
                "Generate Missing Glyphs",
                "Decompose Components",
                "Check Joining",
                "Compose from Anchors",
                "Bake Masks",
                "Export Glyph as SVG",
                "Trace Image…",
                "Bolden With Model…",
                "Place Image…",
                "Import SVG…",
                "Remove Image",
            ]
        );

        let file: Vec<_> = ACTIONS
            .iter()
            .filter(|entry| entry.menu == "File")
            .map(|entry| entry.title)
            .collect();
        assert_eq!(file, ["New Font", "Open…", "Save", "Save As…", "Export…"]);

        let edit: Vec<_> = ACTIONS
            .iter()
            .filter(|entry| entry.menu == "Edit")
            .map(|entry| entry.title)
            .collect();
        assert_eq!(
            edit,
            [
                "Undo",
                "Redo",
                "Copy",
                "Paste",
                "Copy Selected Glyphs as Text",
                "Select All",
                "Deselect All",
                "Invert Selection",
            ]
        );

        let nodes: Vec<_> = ACTIONS
            .iter()
            .filter(|entry| entry.menu == "Nodes")
            .map(|entry| entry.title)
            .collect();
        assert_eq!(
            nodes,
            ["New Nodes", "Open Nodes…", "Save Nodes", "Run Nodes"]
        );

        let filter: Vec<_> = ACTIONS
            .iter()
            .filter(|entry| entry.menu == "Filter")
            .map(|entry| entry.title)
            .collect();
        assert_eq!(
            filter,
            [
                "Offset Curve",
                "Extrude",
                "Roughen",
                "Slanter",
                "Round Corners",
                "Add Extremes",
                "Remove Overlap",
            ]
        );

        let path: Vec<_> = ACTIONS
            .iter()
            .filter(|entry| entry.menu == "Path")
            .map(|entry| entry.title)
            .collect();
        assert_eq!(
            path,
            [
                "Tidy Up Paths",
                "Add Extremes",
                "Round Coordinates",
                "Correct Path Direction",
                "Reverse Contours",
                "Set Start Point",
                "Remove Overlap",
                "Union",
                "Subtract",
                "Intersect",
                "Exclude",
                "Flip Horizontal",
                "Flip Vertical",
                "Rotate 90° Left",
                "Rotate 90° Right",
                "Rotate 180°",
                "Duplicate Selection",
                "Duplicate + Repeat",
                "Harmonize",
                "Balance",
                "Optimize",
                "Hyperbezier to Cubic",
                "Quadratic to Cubic",
                "Cubic to Quadratic",
            ]
        );
    }

    #[test]
    fn welcome_state_keeps_application_commands_and_disables_document_commands() {
        let app = AppState::open(None);
        let entry = |action| {
            ACTIONS
                .iter()
                .find(|entry| entry.action == action)
                .expect("the action is in the command table")
        };

        assert!(entry(AppAction::Quit).enabled(&app));
        assert!(entry(AppAction::NewFont).enabled(&app));
        assert!(entry(AppAction::OpenFont).enabled(&app));
        assert!(entry(AppAction::Theme("gray")).enabled(&app));
        assert!(entry(AppAction::Theme("gray")).checked(&app).unwrap());
        assert!(!entry(AppAction::Save).enabled(&app));
        assert!(!entry(AppAction::Undo).enabled(&app));
        assert!(!entry(AppAction::ZoomToFit).enabled(&app));
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::cell::RefCell;
    use std::sync::OnceLock;

    use muda::accelerator::Accelerator;
    use muda::{CheckMenuItem, Menu, MenuId, MenuItem, MenuItemKind, Submenu};

    use super::{ACTIONS, MENUS};
    use crate::widgets::shortcuts::AppAction;

    thread_local! {
        /// The menu is built once and held for the life of the process:
        /// dropping it would take the menu bar with it. It is not `Send`,
        /// which is fine, because it is only ever touched from the main
        /// thread, and it is also why this cannot be a `static`.
        static MENU: RefCell<Option<(Menu, Vec<MenuItemKind>)>> = const { RefCell::new(None) };
    }
    /// Menu item ids, in the same order as [`ACTIONS`]. These are plain
    /// strings, so the event pump on another thread can read them.
    static IDS: OnceLock<Vec<MenuId>> = OnceLock::new();

    /// Text-editing accelerators stay in Masonry's event path so the focused
    /// `TextArea` gets first refusal. Installing the same equivalents on the
    /// native items would make `AppKit` consume them before `winit` sees the key.
    fn accelerator(entry: &super::Entry) -> Option<Accelerator> {
        if matches!(
            entry.action,
            AppAction::Undo
                | AppAction::Redo
                | AppAction::Copy
                | AppAction::Paste
                | AppAction::SelectAll
        ) {
            None
        } else {
            entry
                .accelerator
                .and_then(|text| text.parse::<Accelerator>().ok())
        }
    }

    /// Builds the menu bar and attaches it to the application.
    ///
    /// Must run on the main thread, which is where the view function runs.
    /// Later calls update enabled and checked state without rebuilding the bar.
    pub(crate) fn install(app: &crate::AppState) {
        if IDS.get().is_some() {
            MENU.with(|slot| {
                if let Some((_, items)) = slot.borrow().as_ref() {
                    for (entry, item) in ACTIONS
                        .iter()
                        .filter(|entry| entry.menu != "Runebender" && MENUS.contains(&entry.menu))
                        .zip(items)
                    {
                        match item {
                            MenuItemKind::MenuItem(item) => item.set_enabled(entry.enabled(app)),
                            MenuItemKind::Check(item) => {
                                item.set_enabled(entry.enabled(app));
                                item.set_checked(entry.checked(app).unwrap_or(false));
                            }
                            _ => {}
                        }
                    }
                }
            });
            return;
        }
        let bar = Menu::new();
        let mut ids = Vec::with_capacity(ACTIONS.len());
        let mut items = Vec::with_capacity(ACTIONS.len());
        // The first submenu on macOS is the application menu, and it is
        // where the platform expects Quit to live.
        let app_menu = Submenu::new("Runebender", true);
        let _ = app_menu.append(&muda::PredefinedMenuItem::about(None, None));
        let _ = app_menu.append(&muda::PredefinedMenuItem::separator());
        let _ = app_menu.append(&muda::PredefinedMenuItem::hide(None));
        let _ = app_menu.append(&muda::PredefinedMenuItem::separator());
        let _ = app_menu.append(&muda::PredefinedMenuItem::quit(None));
        let _ = bar.append(&app_menu);

        for name in MENUS.iter().filter(|name| **name != "Runebender") {
            let submenu = Submenu::new(*name, true);
            let menu_entries: Vec<_> = ACTIONS.iter().filter(|entry| entry.menu == *name).collect();
            let mut index = 0;
            while index < menu_entries.len() {
                let entry = menu_entries[index];
                if let Some(group) = entry.submenu() {
                    if entry.separator_before() {
                        let _ = submenu.append(&muda::PredefinedMenuItem::separator());
                    }
                    let nested = Submenu::new(group, true);
                    let mut first = true;
                    while index < menu_entries.len() && menu_entries[index].submenu() == Some(group)
                    {
                        let entry = menu_entries[index];
                        if !first && entry.separator_before() {
                            let _ = nested.append(&muda::PredefinedMenuItem::separator());
                        }
                        first = false;
                        let accelerator = accelerator(entry);
                        let item = CheckMenuItem::new(
                            entry.title,
                            entry.enabled(app),
                            entry.checked(app).unwrap_or(false),
                            accelerator,
                        );
                        ids.push(item.id().clone());
                        let _ = nested.append(&item);
                        items.push(MenuItemKind::Check(item));
                        index += 1;
                    }
                    let _ = submenu.append(&nested);
                    continue;
                }
                if entry.separator_before() {
                    let _ = submenu.append(&muda::PredefinedMenuItem::separator());
                }
                let accelerator = accelerator(entry);
                let item = if let Some(checked) = entry.checked(app) {
                    MenuItemKind::Check(CheckMenuItem::new(
                        entry.title,
                        entry.enabled(app),
                        checked,
                        accelerator,
                    ))
                } else {
                    MenuItemKind::MenuItem(MenuItem::new(
                        entry.title,
                        entry.enabled(app),
                        accelerator,
                    ))
                };
                ids.push(item.id().clone());
                match &item {
                    MenuItemKind::MenuItem(item) => {
                        let _ = submenu.append(item);
                    }
                    MenuItemKind::Check(item) => {
                        let _ = submenu.append(item);
                    }
                    _ => unreachable!("Runebender creates only normal and check menu items"),
                }
                items.push(item);
                index += 1;
            }
            let _ = bar.append(&submenu);
        }
        // On macOS the bar belongs to the application, not to a window,
        // so this needs no window handle. Xilem does not hand one out.
        bar.init_for_nsapp();
        MENU.with(|slot| *slot.borrow_mut() = Some((bar, items)));
        let _ = IDS.set(ids);
    }

    /// The action a menu id fires, if it is one of ours.
    pub(super) fn action_for(id: &MenuId) -> Option<AppAction> {
        let ids = IDS.get()?;
        // ACTIONS and IDS are built together, in menu order.
        let mut index = 0;
        for name in MENUS.iter().filter(|name| **name != "Runebender") {
            for entry in ACTIONS.iter().filter(|e| e.menu == *name) {
                if ids.get(index) == Some(id) {
                    return Some(entry.action);
                }
                index += 1;
            }
        }
        None
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn focused_editing_shortcuts_stay_out_of_the_native_accelerator_scope() {
            let entry = |action| {
                ACTIONS
                    .iter()
                    .find(|entry| entry.action == action)
                    .expect("the action has a menu row")
            };
            for action in [
                AppAction::Undo,
                AppAction::Redo,
                AppAction::Copy,
                AppAction::Paste,
                AppAction::SelectAll,
            ] {
                assert!(accelerator(entry(action)).is_none());
            }
            assert!(accelerator(entry(AppAction::Save)).is_some());
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use crate::widgets::shortcuts::AppAction;

    /// The native menu is macOS-only; [`crate::widgets::menu_shell`] owns the
    /// in-window menu on these platforms.
    pub(crate) fn install(_app: &crate::AppState) {}

    /// No menu ids exist off macOS, so nothing matches.
    pub(super) fn action_for(_id: &muda::MenuId) -> Option<AppAction> {
        None
    }
}

pub(crate) use platform::install;

/// Runs `view`, and alongside it drains muda's global menu channel and
/// posts each click back into the application.
///
/// Menu events do not travel through winit's event loop, so without this
/// they never reach the widget tree at all. The pump produces no widget,
/// which is why it is forked alongside the tree rather than placed in it.
pub(crate) fn with_menu_events<V: xilem::WidgetView<AppState>>(
    view: V,
) -> impl xilem::WidgetView<AppState> + use<V> {
    use xilem::core::fork;
    use xilem::view::task;

    fork(
        view,
        task(
            |proxy: xilem::core::MessageProxy<muda::MenuId>, _: &mut AppState| async move {
                let channel = muda::MenuEvent::receiver();
                loop {
                    // Polled, never blocked. `recv()` is a synchronous call:
                    // inside an async task it parks a runtime worker and
                    // never gives it back, and then dropping the runtime on
                    // quit waits for a thread that cannot finish. The window
                    // closes and the process hangs.
                    match channel.try_recv() {
                        Ok(event) => {
                            if proxy.message(event.id).is_err() {
                                return;
                            }
                        }
                        // Empty, or (never, for a static channel)
                        // disconnected. Either way, wait a frame and look
                        // again.
                        Err(_) => {
                            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
                        }
                    }
                }
            },
            |app: &mut AppState, id: muda::MenuId| {
                if let Some(action) = platform::action_for(&id) {
                    app.dispatch(action);
                }
            },
        ),
    )
}
