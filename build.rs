// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Embeds the icon UFO glyphs and configures the native Windows executable.
//! Cargo runs this script before compiling the package.

use std::fs;
use std::path::{Path, PathBuf};

// Generate the list of icon GLIFs included in the binary.
fn embed_icons() {
    let glyph_dir = Path::new("assets/icons/icons.ufo/glyphs");
    println!("cargo::rerun-if-changed={}", glyph_dir.display());
    let mut glifs: Vec<PathBuf> = fs::read_dir(glyph_dir)
        .expect("icon UFO glyph directory")
        .map(|entry| entry.expect("icon directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "glif")
        })
        .collect();
    glifs.sort();
    assert!(!glifs.is_empty(), "icon UFO has no glyphs");

    let mut source = String::from("pub(super) const GLIFS: &[&[u8]] = &[\n");
    for path in glifs {
        let relative = format!("/{}", path.display());
        source.push_str(&format!(
            "    include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), {relative:?})),\n"
        ));
    }
    source.push_str("];\n");
    let output =
        PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR")).join("icon_glifs.rs");
    fs::write(output, source).expect("write embedded icon list");
}

fn main() {
    // Rerun this script only when it or the icon glyph directory changes.
    println!("cargo::rerun-if-changed=build.rs");
    embed_icons();

    // Xilem needs more than Windows' default 1 MiB stack in debug builds.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
            Ok("msvc") => println!("cargo::rustc-link-arg-bin=runebender=/STACK:16777216"),
            Ok("gnu") => println!("cargo::rustc-link-arg-bin=runebender=-Wl,--stack,16777216"),
            _ => {}
        }
    }
}
