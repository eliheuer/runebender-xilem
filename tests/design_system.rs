// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The design system gate.
//!
//! Every color, shape, and interaction state in the window comes from the
//! design system: the theme files, then `src/application/design/`, then an
//! area override in `design/areas.rs`. This test reads all application
//! code outside `design/` and fails when it reaches past the system: a
//! face set by hand, a literal opacity, stroke, radius, or text size, a
//! stock button, or a theme role no theme defines.
//!
//! Older code that still does so is counted in [`BUDGET`]. A count may
//! only go down: when you remove a bypass, lower its number here; when
//! you would add one, use the design system instead. See
//! `src/application/design/mod.rs` for where each thing lives.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A way of styling past the design system, and what to use instead.
struct Rule {
    name: &'static str,
    /// Text that starts a bypass.
    needle: &'static str,
    /// Whether the text after the needle makes it a bypass.
    then: fn(&str) -> bool,
    /// Files, relative to `src/application/`, where this is the design
    /// system's own business.
    owners: &'static [&'static str],
    instead: &'static str,
}

fn always(_: &str) -> bool {
    true
}

/// A literal number follows: `0.5`, `12.0`, `3`.
fn number(rest: &str) -> bool {
    rest.trim_start().starts_with(|c: char| c.is_ascii_digit())
}

/// The design system itself, which owns every style.
const DESIGN: &[&str] = &["design/"];

const RULES: &[Rule] = &[
    Rule {
        name: "local face",
        needle: ".background_color(",
        then: always,
        owners: DESIGN,
        instead: "a ControlKind for a control, or a recipe for a surface",
    },
    Rule {
        name: "local face",
        needle: ".border_color(",
        then: always,
        owners: DESIGN,
        instead: "a ControlKind for a control, or a recipe for a surface",
    },
    Rule {
        name: "local face",
        needle: ".border_width(",
        then: always,
        owners: DESIGN,
        instead: "a ControlKind for a control, or a recipe for a surface",
    },
    Rule {
        name: "local face",
        needle: ".corner_radius(",
        then: always,
        owners: DESIGN,
        instead: "a ControlKind for a control, or a recipe for a surface",
    },
    Rule {
        name: "literal opacity",
        needle: ".with_alpha(",
        then: number,
        owners: DESIGN,
        instead: "a design::Alpha step",
    },
    Rule {
        name: "literal stroke",
        needle: "Stroke::new(",
        then: number,
        owners: DESIGN,
        instead: "pal.stroke or pal.stroke_emphasis",
    },
    Rule {
        name: "literal radius",
        needle: "to_rounded_rect(",
        then: number,
        owners: DESIGN,
        instead: "pal.control_radius, pal.corner_radius, or a ControlStyle's radius",
    },
    Rule {
        name: "literal text size",
        needle: "text_size(",
        then: number,
        owners: DESIGN,
        instead: "a design::TextSize step",
    },
    Rule {
        name: "literal color",
        needle: "Color::from_rgb",
        then: always,
        owners: DESIGN,
        instead: "a theme token through the Palette",
    },
    Rule {
        name: "stock button",
        needle: "xilem::view::button",
        then: always,
        owners: DESIGN,
        instead: "recipes::button with a ControlStyle",
    },
    Rule {
        name: "stock button",
        needle: "button as xilem_button",
        then: always,
        owners: DESIGN,
        instead: "recipes::button with a ControlStyle",
    },
];

/// Bypasses that predate this gate, by file and rule. Shrink, never grow.
const BUDGET: &[(&str, &str, usize)] = &[
    // The editor canvas draws many overlays (selection, warnings,
    // kerning, metrics), each faded for its own reason. Name them as
    // `areas::editor` overrides rather than raising these numbers.
    ("view/canvas/editor.rs", "literal opacity", 26),
    ("view/canvas/editor.rs", "literal stroke", 8),
    ("view/chrome.rs", "local face", 1),
    ("view/panels/chat.rs", "local face", 4),
    ("view/panels/graph.rs", "local face", 3),
    ("view/panels/preview.rs", "local face", 3),
    ("view/render.rs", "local face", 4),
];

fn application_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/application")
}

/// Every Rust file of the application, relative to `src/application/`.
fn view_files() -> Vec<(String, String)> {
    let root = application_dir();
    let mut files = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("view directory") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let name = path
                    .strip_prefix(&root)
                    .expect("under src/application")
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push((name, fs::read_to_string(&path).expect("source")));
            }
        }
    }
    files.sort();
    files
}

/// The production code of a file: comments and the test module dropped.
fn production_lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
    source
        .lines()
        .enumerate()
        .take_while(|(_, line)| line.trim() != "#[cfg(test)]")
        .map(|(number, line)| (number + 1, line.split("//").next().unwrap_or("")))
}

fn bypasses() -> BTreeMap<(String, &'static str), Vec<String>> {
    let mut found: BTreeMap<(String, &'static str), Vec<String>> = BTreeMap::new();
    for (name, source) in view_files() {
        for rule in RULES {
            if rule.owners.iter().any(|owner| name.starts_with(owner)) {
                continue;
            }
            for (number, code) in production_lines(&source) {
                for (at, _) in code.match_indices(rule.needle) {
                    if (rule.then)(&code[at + rule.needle.len()..]) {
                        found
                            .entry((name.clone(), rule.name))
                            .or_default()
                            .push(format!(
                                "{name}:{number}: {} `{}`: use {}",
                                rule.name,
                                code.trim(),
                                rule.instead
                            ));
                    }
                }
            }
        }
    }
    found
}

#[test]
fn the_view_layer_styles_only_through_the_design_system() {
    let found = bypasses();
    let budget: BTreeMap<(String, &str), usize> = BUDGET
        .iter()
        .map(|&(file, rule, count)| ((file.to_owned(), rule), count))
        .collect();
    let mut failures = Vec::new();
    for (key, lines) in &found {
        let allowed = budget.get(&(key.0.clone(), key.1)).copied().unwrap_or(0);
        if lines.len() > allowed {
            failures.push(format!(
                "{} {}: {} found, {allowed} allowed\n  {}",
                key.0,
                key.1,
                lines.len(),
                lines.join("\n  ")
            ));
        }
    }
    for (key, &allowed) in &budget {
        let count = found.get(&(key.0.clone(), key.1)).map_or(0, Vec::len);
        if count < allowed {
            failures.push(format!(
                "{} {}: down to {count} from {allowed}; lower its BUDGET entry to {count}",
                key.0, key.1
            ));
        }
    }
    let table: Vec<String> = found
        .iter()
        .map(|(key, lines)| format!("    (\"{}\", \"{}\", {}),", key.0, key.1, lines.len()))
        .collect();
    assert!(
        failures.is_empty(),
        "\n{}\n\nThe current counts, for reference:\n{}\n",
        failures.join("\n"),
        table.join("\n")
    );
}

/// Role names a theme file must or may define, read from the parser.
fn theme_roles() -> Vec<String> {
    let source = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/theme.rs"))
        .expect("theme parser");
    let mut roles = Vec::new();
    for list in ["const REQUIRED_ROLES", "const OPTIONAL_ROLES"] {
        let start = source.find(list).expect("role list");
        let body = &source[start..start + source[start..].find("];").expect("list end")];
        roles.extend(body.split('"').skip(1).step_by(2).map(str::to_owned));
    }
    roles
}

#[test]
fn every_role_the_view_asks_for_is_one_a_theme_defines() {
    let roles = theme_roles();
    let mut failures = Vec::new();
    for (name, source) in view_files() {
        for (number, code) in production_lines(&source) {
            for (at, _) in code.match_indices(".role(\"") {
                let rest = &code[at + ".role(\"".len()..];
                let role = rest.split('"').next().unwrap_or("");
                if !roles.iter().any(|known| known == role) {
                    failures.push(format!(
                        "{name}:{number}: no theme defines the role `{role}`; it would panic"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}

#[test]
fn budget_entries_name_real_files_and_rules() {
    let root = application_dir();
    for (file, rule, _) in BUDGET {
        assert!(root.join(file).exists(), "{file} in BUDGET is gone");
        assert!(
            RULES.iter().any(|r| r.name == *rule),
            "{rule} in BUDGET is not a rule"
        );
    }
}
