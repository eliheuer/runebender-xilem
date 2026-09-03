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
        title: "Show Nodes",
        accelerator: None,
        action: AppAction::NodesTab,
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
        title: "Duplicate",
        accelerator: Some("CmdOrCtrl+D"),
        action: AppAction::Duplicate,
    },
    Entry {
        menu: "Glyph",
        title: "Generate Missing Glyphs",
        accelerator: None,
        action: AppAction::GenerateMissing,
    },
    Entry {
        menu: "Glyph",
        title: "Flip Horizontal",
        accelerator: None,
        action: AppAction::FlipHorizontal,
    },
    Entry {
        menu: "Glyph",
        title: "Flip Vertical",
        accelerator: None,
        action: AppAction::FlipVertical,
    },
    Entry {
        menu: "Glyph",
        title: "Rotate 90",
        accelerator: None,
        action: AppAction::Rotate90,
    },
    Entry {
        menu: "Glyph",
        title: "Remove Overlap",
        accelerator: None,
        action: AppAction::RemoveOverlap,
    },
    Entry {
        menu: "Glyph",
        title: "Decompose",
        accelerator: None,
        action: AppAction::Decompose,
    },
    Entry {
        menu: "View",
        title: "Sort by Name",
        accelerator: None,
        action: AppAction::SortByName,
    },
    Entry {
        menu: "View",
        title: "Sort by Unicode",
        accelerator: None,
        action: AppAction::SortByUnicode,
    },
    Entry {
        menu: "View",
        title: "Cycle Theme",
        accelerator: Some("CmdOrCtrl+T"),
        action: AppAction::CycleTheme,
    },
    Entry {
        menu: "View",
        title: "Overview",
        accelerator: Some("Escape"),
        action: AppAction::Overview,
    },
    Entry {
        menu: "Tools",
        title: "Select",
        accelerator: Some("V"),
        action: AppAction::Tool(Tool::Select),
    },
    Entry {
        menu: "Tools",
        title: "Pen",
        accelerator: Some("P"),
        action: AppAction::Tool(Tool::Pen),
    },
    Entry {
        menu: "Tools",
        title: "Hyper Pen",
        accelerator: Some("B"),
        action: AppAction::Tool(Tool::HyperPen),
    },
    Entry {
        menu: "Tools",
        title: "Rectangle",
        accelerator: Some("U"),
        action: AppAction::Tool(Tool::Rect),
    },
    Entry {
        menu: "Tools",
        title: "Ellipse",
        accelerator: Some("O"),
        action: AppAction::Tool(Tool::Ellipse),
    },
    Entry {
        menu: "Tools",
        title: "Knife",
        accelerator: Some("E"),
        action: AppAction::Tool(Tool::Knife),
    },
    Entry {
        menu: "Tools",
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
const MENUS: &[&str] = &["File", "Nodes", "Edit", "Glyph", "View", "Tools"];

#[cfg(target_os = "macos")]
mod platform {
    use std::cell::RefCell;
    use std::sync::OnceLock;

    use muda::accelerator::Accelerator;
    use muda::{Menu, MenuId, MenuItem, Submenu};

    use super::{ACTIONS, MENUS};
    use crate::widgets::shortcuts::AppAction;

    thread_local! {
        /// The menu is built once and held for the life of the process:
        /// dropping it would take the menu bar with it. It is not `Send`,
        /// which is fine, because it is only ever touched from the main
        /// thread, and it is also why this cannot be a `static`.
        static MENU: RefCell<Option<Menu>> = const { RefCell::new(None) };
    }
    /// Menu item ids, in the same order as [`ACTIONS`]. These are plain
    /// strings, so the event pump on another thread can read them.
    static IDS: OnceLock<Vec<MenuId>> = OnceLock::new();

    /// Builds the menu bar and attaches it to the application.
    ///
    /// Must run on the main thread, which is where the view function
    /// runs, and does nothing on later calls.
    pub(crate) fn install() {
        if IDS.get().is_some() {
            return;
        }
        let bar = Menu::new();
        let mut ids = Vec::with_capacity(ACTIONS.len());
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
            for entry in ACTIONS.iter().filter(|e| e.menu == *name) {
                let accelerator = entry
                    .accelerator
                    .and_then(|text| text.parse::<Accelerator>().ok());
                let item = MenuItem::new(entry.title, true, accelerator);
                ids.push(item.id().clone());
                let _ = submenu.append(&item);
            }
            let _ = bar.append(&submenu);
        }
        // On macOS the bar belongs to the application, not to a window,
        // so this needs no window handle. Xilem does not hand one out.
        bar.init_for_nsapp();
        MENU.with(|slot| *slot.borrow_mut() = Some(bar));
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

    /// Not yet: muda's menu bar needs a GTK window on Linux and an HWND
    /// on Windows, and Xilem hands out neither. Those platforms want an
    /// in-window menu bar on Masonry's layer system instead.
    pub(crate) fn install() {}

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
