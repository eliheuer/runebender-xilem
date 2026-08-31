// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! What every command shares: exit codes, output, and loading.
//!
//! The exit codes are the interface a script branches on, so they
//! live in one place and are pinned by a test.

use std::path::Path;

use norad::Font;
use serde_json::{Value, json};

/// Exit codes, matching font-ml so a caller can branch on them.
pub(crate) mod exit {
    /// Ran, and the answer is yes or the work is done.
    pub const OK: i32 = 0;
    /// Ran, and the answer is no. A check found something, or a
    /// dry run has changes waiting.
    pub const FINDINGS: i32 = 1;
    /// The command was wrong: bad path, unknown glyph, missing flag.
    pub const USAGE: i32 = 2;
    /// The command was right and the work failed.
    pub const FAILED: i32 = 4;
}

/// Reports an error on stderr, or as JSON on stdout, and returns the
/// code to exit with.
pub(crate) fn fail(json: bool, code: i32, message: &str) -> i32 {
    if json {
        println!("{}", json!({ "ok": false, "error": message }));
    } else {
        eprintln!("{message}");
    }
    code
}

/// Prints the JSON form, or runs `plain` for the human form.
pub(crate) fn emit(json: bool, value: Value, plain: impl FnOnce()) -> i32 {
    if json {
        println!("{value}");
    } else {
        plain();
    }
    exit::OK
}

/// Loads one UFO, reporting a bad path as a usage error.
pub(crate) fn open(path: &Path, json: bool) -> Result<Font, i32> {
    Font::load(path).map_err(|e| fail(json, exit::USAGE, &format!("{}: {e}", path.display())))
}
