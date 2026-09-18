// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Browser fonts are held in memory; there is no filesystem watcher.

pub(crate) fn with_watch<V: xilem::WidgetView<crate::application::workspace::Workspace>>(
    view: V,
    _paths: Vec<std::path::PathBuf>,
) -> V {
    view
}
