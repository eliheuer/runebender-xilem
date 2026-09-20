// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The world outside the window: files, watching, and headless frames.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod dialogs;
#[cfg(target_arch = "wasm32")]
#[path = "browser_dialogs.rs"]
pub(crate) mod dialogs;
pub(crate) mod export;

pub(crate) mod host;
pub(crate) mod screenshot;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod watch;
#[cfg(target_arch = "wasm32")]
#[path = "browser_watch.rs"]
pub(crate) mod watch;

#[cfg(unix)]
pub(crate) mod live;
#[cfg(unix)]
pub(crate) mod live_edits;

#[cfg(unix)]
pub(crate) mod live_fixture;
