// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Desktop dialogs are unavailable in the bundled-font browser demo.

use std::path::{Path, PathBuf};
pub(crate) fn font(_: &Path) -> Option<PathBuf> {
    None
}
pub(crate) fn folder(_: &Path) -> Option<PathBuf> {
    None
}
pub(crate) fn nodes(_: &Path) -> Option<PathBuf> {
    None
}
pub(crate) fn image(_: &Path) -> Option<PathBuf> {
    None
}
pub(crate) fn svg(_: &Path) -> Option<PathBuf> {
    None
}
pub(crate) fn confirm_revert() -> bool {
    false
}
pub(crate) fn dirty_decision(_: &str) -> crate::DirtyDecision {
    crate::DirtyDecision::Cancel
}
