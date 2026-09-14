// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Native file and folder prompts used by application commands.

use std::path::{Path, PathBuf};

fn at(directory: &Path) -> rfd::FileDialog {
    rfd::FileDialog::new().set_directory(directory)
}

/// Pick a font source. macOS can select package directories such as UFOs.
pub(crate) fn font(directory: &Path) -> Option<PathBuf> {
    let dialog = at(directory).add_filter(
        "Font sources",
        &[
            "designspace",
            "glyphs",
            "glyphspackage",
            "babelfont",
            "ufo",
            "otf",
            "ttf",
        ],
    );
    #[cfg(target_os = "macos")]
    return dialog.pick_file_or_folder();
    #[cfg(not(target_os = "macos"))]
    dialog.pick_file()
}

/// Pick a destination directory.
pub(crate) fn folder(directory: &Path) -> Option<PathBuf> {
    at(directory).pick_folder()
}

/// Pick one nodes graph.
pub(crate) fn nodes(directory: &Path) -> Option<PathBuf> {
    at(directory)
        .add_filter("Runebender nodes", &["json"])
        .pick_file()
}

/// Pick one raster image.
pub(crate) fn image(directory: &Path) -> Option<PathBuf> {
    at(directory)
        .add_filter(
            "Images",
            &["png", "jpg", "jpeg", "gif", "webp", "tif", "tiff"],
        )
        .pick_file()
}

/// Pick one SVG file.
pub(crate) fn svg(directory: &Path) -> Option<PathBuf> {
    at(directory).add_filter("SVG", &["svg"]).pick_file()
}

/// Confirm replacing unsaved in-memory edits with the source tree on disk.
pub(crate) fn confirm_revert() -> bool {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Revert to Saved")
        .set_description("Discard all unsaved edits and reload the font sources from disk?")
        .set_buttons(rfd::MessageButtons::OkCancel)
        .show()
        == rfd::MessageDialogResult::Ok
}

/// Ask how to handle unsaved edits before a destructive application action.
pub(crate) fn dirty_decision(action: &str) -> crate::DirtyDecision {
    let result = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Unsaved Changes")
        .set_description(format!("Save changes before {action}?"))
        .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
            "Save".into(),
            "Discard".into(),
            "Cancel".into(),
        ))
        .show();
    match result {
        rfd::MessageDialogResult::Yes => crate::DirtyDecision::Save,
        rfd::MessageDialogResult::No => crate::DirtyDecision::Discard,
        rfd::MessageDialogResult::Custom(label) if label == "Save" => crate::DirtyDecision::Save,
        rfd::MessageDialogResult::Custom(label) if label == "Discard" => {
            crate::DirtyDecision::Discard
        }
        _ => crate::DirtyDecision::Cancel,
    }
}
