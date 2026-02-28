// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Filesystem watcher for detecting external UFO changes.
//!
//! Uses the `notify` crate (OS-native events: FSEvents on macOS, inotify on
//! Linux) to watch UFO directories for modifications. A 1-second debounce
//! window batches rapid multi-file writes (e.g., an AI editing many glyphs).
//! Self-save suppression prevents reloading our own Cmd+S saves.

use notify::{Event, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use xilem::core::MessageProxy;
use xilem::tokio;

/// Message sent when external file changes are detected.
#[derive(Debug)]
pub struct FileChanged;

/// Watch one or more UFO directories for external changes and send
/// `FileChanged` messages via the Xilem proxy.
///
/// For a designspace, `ufo_paths` contains every master's UFO
/// directory. For a single UFO it contains just one entry.
pub async fn watch_ufo(
    proxy: MessageProxy<FileChanged>,
    ufo_paths: Vec<PathBuf>,
    save_flag: Arc<AtomicBool>,
) {
    if ufo_paths.is_empty() {
        return;
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Event>(64);

    // Create the watcher — it lives on this stack frame, so it stays
    // alive as long as the tokio task runs.
    let mut watcher = match notify::recommended_watcher(
        move |result: Result<Event, notify::Error>| {
            if let Ok(event) = result {
                use notify::EventKind::*;
                match event.kind {
                    Create(_) | Modify(_) | Remove(_) => {
                        let _ = tx.blocking_send(event);
                    }
                    _ => {}
                }
            }
        },
    ) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("Failed to create file watcher: {}", e);
            return;
        }
    };

    // Watch every UFO directory
    for path in &ufo_paths {
        if let Err(e) = watcher.watch(path, RecursiveMode::Recursive)
        {
            tracing::error!(
                "Failed to watch {}: {}",
                path.display(),
                e
            );
        } else {
            tracing::info!(
                "Watching for external changes: {}",
                path.display()
            );
        }
    }

    // Event loop: wait for events, debounce, then notify
    loop {
        // Wait for the first event
        let Some(_first) = rx.recv().await else {
            break;
        };

        // Check if this is a self-save (our own Cmd+S)
        if save_flag.load(Ordering::SeqCst) {
            drain_events(&mut rx).await;
            save_flag.store(false, Ordering::SeqCst);
            continue;
        }

        // Debounce: wait until 1 second of quiet
        loop {
            match tokio::time::timeout(
                Duration::from_secs(1),
                rx.recv(),
            )
            .await
            {
                Ok(Some(_)) => continue,
                Ok(None) => return,
                Err(_) => break,
            }
        }

        // Check save flag again after debounce
        if save_flag.load(Ordering::SeqCst) {
            save_flag.store(false, Ordering::SeqCst);
            continue;
        }

        tracing::info!("External changes detected, reloading");

        if proxy.message(FileChanged).is_err() {
            break;
        }
    }
}

/// Drain all pending events from the channel without blocking.
async fn drain_events(
    rx: &mut tokio::sync::mpsc::Receiver<Event>,
) {
    tokio::time::sleep(Duration::from_millis(500)).await;
    while rx.try_recv().is_ok() {}
}
