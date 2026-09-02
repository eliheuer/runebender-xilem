// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The command line as a script sees it: JSON out, exit codes in.
//!
//! Needs the Virtua Grotesk fixture next to this checkout, or
//! `$RUNEBENDER_TEST_FONTS`. Each test copies the UFO into a fresh
//! temporary directory, so nothing edits the fixture.

use std::path::{Path, PathBuf};
use std::process::Command;

use runebender_core::document::proposal;

fn fixture() -> PathBuf {
    let dir = match std::env::var_os("RUNEBENDER_TEST_FONTS") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../virtua-grotesk/sources"),
    };
    let ufo = dir.join("VirtuaGrotesk-Regular.ufo");
    assert!(
        ufo.is_dir(),
        "fixture fonts not found at {}: clone eliheuer/virtua-grotesk next to this \
         repository, or set RUNEBENDER_TEST_FONTS",
        dir.display()
    );
    ufo
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create");
    for entry in std::fs::read_dir(src).expect("read") {
        let entry = entry.expect("entry");
        let target = dst.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

/// A directory under the system temp dir that goes away when the
/// test drops it.
struct Scratch(PathBuf);

impl Scratch {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A private copy of the fixture, in a directory that goes away
/// with the test.
fn scratch_ufo() -> (Scratch, PathBuf) {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("runebender-cli-test-{}-{n}", std::process::id()));
    let ufo = dir.join("Virtua.ufo");
    copy_dir(&fixture(), &ufo);
    (Scratch(dir), ufo)
}

fn run(args: &[&str]) -> (i32, serde_json::Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_runebender-core"))
        .arg("--json")
        .args(args)
        .output()
        .expect("the binary runs");
    let code = output.status.code().expect("an exit code");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value = stdout
        .lines()
        .last()
        .and_then(|line| serde_json::from_str(line).ok())
        .unwrap_or(serde_json::Value::Null);
    (code, value)
}

#[test]
fn info_reports_the_font_as_json() {
    let (_dir, ufo) = scratch_ufo();
    let (code, out) = run(&["info", ufo.to_str().expect("utf8")]);
    assert_eq!(code, 0);
    assert_eq!(out["ok"], true);
    assert_eq!(out["family"], "Virtua Grotesk");
    assert_eq!(out["unitsPerEm"], 1024.0);
    assert!(out["glyphs"].as_u64().expect("count") > 100);
    assert_eq!(out["proposals"].as_array().expect("list").len(), 0);
}

#[test]
fn a_bad_path_is_a_usage_error() {
    let (code, out) = run(&["info", "/nowhere/Nothing.ufo"]);
    assert_eq!(code, 2);
    assert_eq!(out["ok"], false);
}

#[test]
fn proof_writes_svg_and_metrics() {
    let (dir, ufo) = scratch_ufo();
    let svg = dir.path().join("proof.svg");
    let (code, out) = run(&[
        "proof",
        ufo.to_str().expect("utf8"),
        "--glyphs",
        "H,O",
        "--out",
        svg.to_str().expect("utf8"),
    ]);
    assert_eq!(code, 0, "{out}");
    let text = std::fs::read_to_string(&svg).expect("svg written");
    assert!(text.starts_with("<svg"));
    assert_eq!(text.matches("<path ").count(), 2);
    let glyphs = out["glyphs"].as_array().expect("metrics");
    assert_eq!(glyphs[0]["glyph"], "H");
    assert!(glyphs[0]["advance"].as_f64().expect("advance") > 0.0);
    assert!(glyphs[0]["lsb"].is_number());
}

#[test]
fn proposals_list_install_and_discard_through_the_binary() {
    let (_dir, ufo) = scratch_ufo();
    let path = ufo.to_str().expect("utf8");

    // A tool wrote a proposal: H moved right, O with a contour gone.
    let mut font = norad::Font::load(&ufo).expect("loads");
    let mut h = font.get_glyph("H").expect("H").clone();
    let width_before = h.width;
    for c in &mut h.contours {
        for p in &mut c.points {
            p.x += 20.0;
        }
    }
    h.width += 40.0;
    let mut o = font.get_glyph("O").expect("O").clone();
    o.contours.pop();
    proposal::write(&mut font, "bolden", [h, o]).expect("written");
    font.save(&ufo).expect("saved");

    let (code, out) = run(&["proposal", "list", path]);
    assert_eq!(code, 0);
    let list = out["proposals"].as_array().expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["task"], "bolden");
    assert_eq!(list[0]["compatible"], serde_json::json!(["H"]));
    assert_eq!(list[0]["incompatible"].as_array().expect("pairs").len(), 1);

    let (code, out) = run(&["proposal", "install", path, "--task", "bolden"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["installed"]["installed"], serde_json::json!(["H"]));
    assert_eq!(
        out["installed"]["skipped"].as_array().expect("skips").len(),
        1
    );
    assert_eq!(out["installed"]["layer_removed"], false);

    let font = norad::Font::load(&ufo).expect("reloads");
    assert_eq!(font.get_glyph("H").expect("H").width, width_before + 40.0);
    assert_eq!(font.get_glyph("O").expect("O").contours.len(), 2);

    let (code, out) = run(&["proposal", "discard", path, "--task", "bolden"]);
    assert_eq!(code, 0);
    assert_eq!(out["discarded"], 1);
    let (code, out) = run(&["proposal", "discard", path, "--task", "bolden"]);
    assert_eq!(code, 2);
    assert_eq!(out["error"]["kind"], "no_proposal");
}

#[test]
fn propose_without_font_ml_says_so_with_code_3() {
    let (_dir, ufo) = scratch_ufo();
    let output = Command::new(env!("CARGO_BIN_EXE_runebender-core"))
        .args(["--json", "propose", "bolden", ufo.to_str().expect("utf8")])
        .env("RUNEBENDER_FONT_ML", "")
        .env("PATH", "/nonexistent")
        .output()
        .expect("runs");
    assert_eq!(output.status.code(), Some(3));
    let out: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json on stdout");
    assert_eq!(out["ok"], false);
}

#[test]
fn propose_runs_the_tool_and_reports_what_it_left() {
    let (dir, ufo) = scratch_ufo();
    // A stand-in font-ml: prints JSON and writes a proposal layer by
    // hand, the way the real one will.
    let tool = dir.path().join("font-ml");
    std::fs::write(
        &tool,
        "#!/bin/sh\n\
         echo '{\"ok\":true,\"glyph\":\"H\"}'\n\
         exit 0\n",
    )
    .expect("script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let mut font = norad::Font::load(&ufo).expect("loads");
    let h = font.get_glyph("H").expect("H").clone();
    proposal::write(&mut font, "bolden", [h]).expect("written");
    font.save(&ufo).expect("saved");

    let (code, out) = run(&[
        "propose",
        "bolden",
        ufo.to_str().expect("utf8"),
        "--tool",
        tool.to_str().expect("utf8"),
        "--glyphs",
        "H",
    ]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["ok"], true);
    assert_eq!(out["report"]["glyph"], "H");
    assert_eq!(out["proposal"]["task"], "bolden");
    assert_eq!(out["proposal"]["glyphs"], serde_json::json!(["H"]));
}
