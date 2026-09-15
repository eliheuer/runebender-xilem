// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Leaves source-tree watching off on platforms without the native watcher.

/// Returns `view` unchanged because this platform does not watch font files.
pub(crate) fn with_watch<V: xilem::WidgetView<crate::Workspace>>(
    view: V,
    _paths: Vec<std::path::PathBuf>,
) -> V {
    view
}
