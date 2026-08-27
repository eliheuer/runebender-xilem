// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reload when the sources change underneath us.
//!
//! A font project is edited by more than this program: a build script
//! writes a master, another editor saves, a git checkout moves the tree.
//! The GPUI build reloads on that, so this does too.
//!
//! The pump is the same shape as the menu one, and for the same reason:
//! the events come from somewhere that is not winit's event loop, and
//! Xilem exposes no hook to drain them, so a `task` view does it and
//! posts the result back into the application.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::App;

/// Sent when the sources on disk have changed and settled.
#[derive(Debug)]
pub struct SourcesChanged;

/// Runs `view`, and alongside it watches the font's sources.
pub fn with_watch<V: xilem::WidgetView<App>>(
    view: V,
    paths: Vec<PathBuf>,
) -> impl xilem::WidgetView<App> + use<V> {
    use xilem::core::fork;
    use xilem::view::task_raw;

    fork(
        view,
        task_raw(
            move |proxy: xilem::core::MessageProxy<SourcesChanged>, _: &mut App| {
                let paths = paths.clone();
                async move {
                    let (tx, rx) = mpsc::channel();
                    let mut watcher = match notify::recommended_watcher(
                        move |result: Result<notify::Event, notify::Error>| {
                            if result.is_ok() {
                                let _ = tx.send(());
                            }
                        },
                    ) {
                        Ok(watcher) => watcher,
                        Err(_) => return,
                    };
                    for path in &paths {
                        let _ = notify::Watcher::watch(
                            &mut watcher,
                            path,
                            notify::RecursiveMode::Recursive,
                        );
                    }
                    // A save is many file events. Wait for them to stop
                    // before saying anything, or one save reloads five
                    // times.
                    loop {
                        if rx.recv().is_err() {
                            return;
                        }
                        let settle = Instant::now();
                        while settle.elapsed() < Duration::from_millis(400) {
                            if rx.recv_timeout(Duration::from_millis(400)).is_err() {
                                break;
                            }
                        }
                        if proxy.message(SourcesChanged).is_err() {
                            return;
                        }
                    }
                }
            },
            |app: &mut App, _: SourcesChanged| app.reload_from_disk(),
        ),
    )
}
