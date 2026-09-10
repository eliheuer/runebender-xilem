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
use crate::{Tool, Workspace};
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
        matches!(
            self.action,
            A::Save
                | A::NodesRun
                | A::Copy
                | A::SelectAll
                | A::Decompose
                | A::RemoveOverlap
                | A::FlipHorizontal
                | A::Harmonize
                | A::SortByName
                | A::NextMaster
                | A::Theme(_)
                | A::MeasurePopcount
                | A::MeasureAllOn
        )
    }

    /// Nested menu shared by native and in-window renderers.
    pub(crate) fn submenu(&self) -> Option<&'static str> {
        use AppAction as A;
        match self.action {
            A::Theme(_) => Some("Theme"),
            A::MeasureColorize
            | A::MeasureHandles
            | A::MeasureSegments
            | A::MeasureSideBearings
            | A::MeasurePopcount
            | A::MeasureAllOn
            | A::MeasureAllOff => Some("Measure"),
            _ => None,
        }
    }

    /// Whether the command can change the current workspace.
    pub(crate) fn enabled(&self, app: &Workspace) -> bool {
        use AppAction as A;
        let editor = matches!(app.mode, crate::Mode::Editor(_));
        match self.action {
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
            | A::BooleanSubtract
            | A::BooleanIntersect
            | A::BooleanExclude
            | A::Decompose
            | A::Duplicate
            | A::ReverseContours
            | A::ZoomToFit => editor,
            A::GenerateMissing => matches!(app.sel, crate::Sel::Filter(_)),
            A::NextMaster | A::PreviousMaster => app.font.master_count() > 1,
            A::NodesSave => app.nodes.graph.is_some(),
            _ => true,
        }
    }

    /// Checked state for choice and toggle commands; `None` for ordinary rows.
    pub(crate) fn checked(&self, app: &Workspace) -> Option<bool> {
        use AppAction as A;
        match self.action {
            A::SortByName => Some(app.sort == crate::Sort::Name),
            A::SortByUnicode => Some(app.sort == crate::Sort::Unicode),
            A::Theme(id) => Some(app.theme_id == id),
            A::MeasureColorize => Some(app.view.colorize),
            A::MeasureHandles => Some(app.view.handles),
            A::MeasureSegments => Some(app.view.segments),
            A::MeasureSideBearings => Some(app.view.bearings),
            A::MeasurePopcount => Some(app.view.popcount),
            A::Tool(tool) => Some(app.tool == tool),
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
        menu: "File",
        title: "New Font",
        accelerator: Some("CmdOrCtrl+N"),
        action: AppAction::NewFont,
    },
    Entry {
        menu: "File",
        title: "Save",
        accelerator: Some("CmdOrCtrl+S"),
        action: AppAction::Save,
    },
    Entry {
        menu: "Nodes",
        title: "New Nodes",
        accelerator: None,
        action: AppAction::NodesNew,
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
        menu: "Filter",
        title: "Round Corners",
        accelerator: None,
        action: AppAction::RoundCorners,
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
pub(crate) const MENUS: &[&str] = &["File", "Nodes", "Edit", "Glyph", "Path", "Filter", "View"];

/// Resolve a key press from the same accelerator metadata used by both menu bars.
pub(crate) fn action_for_key(key: &Key, modifiers: Modifiers) -> Option<AppAction> {
    ACTIONS.iter().find_map(|entry| {
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
pub(crate) fn action_enabled(action: AppAction, app: &Workspace) -> bool {
    ACTIONS
        .iter()
        .find(|entry| entry.action == action)
        .is_some_and(|entry| entry.enabled(app))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Builds the menu bar and attaches it to the application.
    ///
    /// Must run on the main thread, which is where the view function runs.
    /// Later calls update enabled and checked state without rebuilding the bar.
    pub(crate) fn install(app: &crate::Workspace) {
        if IDS.get().is_some() {
            MENU.with(|slot| {
                if let Some((_, items)) = slot.borrow().as_ref() {
                    for (entry, item) in ACTIONS
                        .iter()
                        .filter(|entry| MENUS.contains(&entry.menu))
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

        for name in MENUS {
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
                        let accelerator = entry
                            .accelerator
                            .and_then(|text| text.parse::<Accelerator>().ok());
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
                let accelerator = entry
                    .accelerator
                    .and_then(|text| text.parse::<Accelerator>().ok());
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
        for name in MENUS {
            for entry in ACTIONS.iter().filter(|e| e.menu == *name) {
                if ids.get(index) == Some(id) {
                    return Some(entry.action);
                }
                index += 1;
            }
        }
        None
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use crate::widgets::shortcuts::AppAction;

    /// The native menu is macOS-only; [`crate::widgets::menu_shell`] owns the
    /// in-window menu on these platforms.
    pub(crate) fn install(_app: &crate::Workspace) {}

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
pub(crate) fn with_menu_events<V: xilem::WidgetView<Workspace>>(
    view: V,
) -> impl xilem::WidgetView<Workspace> + use<V> {
    use xilem::core::fork;
    use xilem::view::task;

    fork(
        view,
        task(
            |proxy: xilem::core::MessageProxy<muda::MenuId>, _: &mut Workspace| async move {
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
            |app: &mut Workspace, id: muda::MenuId| {
                if let Some(action) = platform::action_for(&id) {
                    app.dispatch(action);
                }
            },
        ),
    )
}
