// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reserves enough main-thread stack for native Windows debug builds.

// Xilem view construction exceeds Windows' default 1 MiB main-thread stack
// in debug builds. Reserve 16 MiB for the application; pages commit on demand.
fn main() {
    // Only this script determines the platform linker flags. Watching the whole
    // package would invalidate the font library after application-only edits.
    println!("cargo::rerun-if-changed=build.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
            Ok("msvc") => println!("cargo::rustc-link-arg-bin=runebender=/STACK:16777216"),
            Ok("gnu") => println!("cargo::rustc-link-arg-bin=runebender=-Wl,--stack,16777216"),
            _ => {}
        }
    }
}
