// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Where the tests find their fixture fonts.
//!
//! The Virtua Grotesk sources live in their own repository,
//! eliheuer/virtua-grotesk, not here, so this crate stays small and
//! the tests run against the current font. Tests look for them in
//! `$RUNEBENDER_TEST_FONTS`, or in `../virtua-grotesk/sources` next
//! to this checkout. CI checks that repository out alongside.

use std::path::{Path, PathBuf};

/// The fixture directory. Panics with the expected location when the
/// fonts are not there, so a missing checkout reads as such and not
/// as a broken test.
pub(crate) fn dir() -> PathBuf {
    let dir = match std::env::var_os("RUNEBENDER_TEST_FONTS") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../virtua-grotesk/sources"),
    };
    assert!(
        dir.join("VirtuaGrotesk.designspace").is_file(),
        "fixture fonts not found at {}: clone eliheuer/virtua-grotesk next to this \
         repository, or set RUNEBENDER_TEST_FONTS",
        dir.display()
    );
    dir
}

pub(crate) fn regular_ufo() -> PathBuf {
    dir().join("VirtuaGrotesk-Regular.ufo")
}

pub(crate) fn designspace() -> PathBuf {
    dir().join("VirtuaGrotesk.designspace")
}

/// Copy a UFO (a directory tree) so a test can edit and save it.
pub(crate) fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
