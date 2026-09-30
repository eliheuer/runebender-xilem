// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Native window state used to reserve space for the macOS window controls.

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSWindowStyleMask};

/// Read `AppKit`'s full-screen flag, rather than infer it from window dimensions.
pub(crate) fn is_fullscreen() -> bool {
    let Some(main_thread) = MainThreadMarker::new() else {
        return false;
    };
    NSApplication::sharedApplication(main_thread)
        .mainWindow()
        .is_some_and(|window| window.styleMask().contains(NSWindowStyleMask::FullScreen))
}
