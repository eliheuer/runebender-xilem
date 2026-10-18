// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The panel language gate.
//!
//! A panel module describes its section in `recipes::rows` and nothing
//! else. This test reads the panel sources and fails when one reaches
//! past the vocabulary to a bare label, column, or button, so a wrong
//! panel does not build. Modules written before the vocabulary are
//! listed below and leave the list as they move.

use std::fs;
use std::path::Path;

/// Panel modules that predate `recipes::rows`. Shrink, never grow.
const LEGACY_PANELS: &[&str] = &[
    "chat.rs",
    "editor.rs",
    "editor_info.rs",
    "graph.rs",
    "image.rs",
    "info.rs",
    "label.rs",
    "metaballs.rs",
    "neural.rs",
    "preview.rs",
    "scripts.rs",
    "sections.rs",
    "sketch.rs",
    "tabs.rs",
];

/// What a panel module may not call. Each is owned by a row.
const RAW_VIEWS: &[&str] = &[
    "label(",
    "column(",
    "row(",
    "flex_col(",
    "flex_row(",
    "sized_box(",
    "canvas(",
    "FlexSpacer",
    "text_input(",
    "button(",
    "toggle(",
    "prose(",
    "prose_muted(",
    ".boxed()",
];

/// Whether `code` calls `raw` as its own name, not as the tail of a
/// longer one such as `file_label(`.
fn calls(code: &str, raw: &str) -> bool {
    code.match_indices(raw).any(|(at, _)| {
        code[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
    })
}

fn panels_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/application/view/panels")
}

#[test]
fn panels_are_built_from_the_row_vocabulary() {
    let mut failures = Vec::new();
    for entry in fs::read_dir(panels_dir()).expect("panels directory") {
        let path = entry.expect("entry").path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.ends_with(".rs") || name == "mod.rs" || LEGACY_PANELS.contains(&name) {
            continue;
        }
        let source = fs::read_to_string(&path).expect("panel source");
        for (number, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            for raw in RAW_VIEWS {
                if calls(code, raw) {
                    failures.push(format!(
                        "{name}:{}: `{raw}` — build this from recipes::Row; see src/application/design/recipes/rows.rs",
                        number + 1
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}

#[test]
fn legacy_panels_still_exist() {
    for name in LEGACY_PANELS {
        assert!(
            panels_dir().join(name).exists(),
            "{name} is listed as a legacy panel but is gone; remove it from the list"
        );
    }
}

#[test]
fn nothing_in_the_view_uppercases_text() {
    let view = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/application/view");
    let mut stack = vec![view];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("view directory") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let source = fs::read_to_string(&path).expect("view source");
                assert!(
                    !source.contains("to_uppercase") && !source.contains("to_ascii_uppercase"),
                    "{}: captions and titles are title case, never all caps",
                    path.display()
                );
            }
        }
    }
}
