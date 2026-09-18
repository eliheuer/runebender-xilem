// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A font editor built on the Linebender ecosystem.

// The browser build reuses this desktop crate root, so some platform-only actions
// are compiled but unused on WASM.
#![cfg_attr(
    target_arch = "wasm32",
    allow(
        dead_code,
        reason = "the browser reuses desktop modules with platform-only actions"
    )
)]

// Load the program's interface, platform, and command-line modules from `application/`.
mod application;

// The browser builds this file as a library and starts through `application::browser`.
// Only the native executable needs the CLI, window launcher, event loop, and exit code.
#[cfg(not(target_arch = "wasm32"))]
use application::{cli, launch};
#[cfg(not(target_arch = "wasm32"))]
use std::process::ExitCode;
#[cfg(not(target_arch = "wasm32"))]
use xilem::EventLoop;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> ExitCode {
    // Headless commands finish in `cli::run`; the remaining path opens the graphical editor.
    let font_path = match cli::run() {
        cli::Startup::Exit(code) => return code,
        cli::Startup::Editor(font_path) => font_path,
    };

    // Borrow the optional `PathBuf` as an optional `Path`; the launcher does not need ownership.
    if let Err(error) = launch::run(EventLoop::with_user_event(), font_path.as_deref()) {
        eprintln!("{error}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
