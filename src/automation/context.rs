// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Design context: the knowledge an agent reads before it designs.
//!
//! Three layers, read in order, and a later layer wins where it disagrees
//! with an earlier one:
//!
//! 1. **Default** files ship with the editor: baseline type design and font
//!    engineering principles that hold for any font.
//! 2. **Personal** files live in `$XDG_CONFIG_HOME/runebender/context/`
//!    (or `~/.config/runebender/context/`) and in the directories named by
//!    `RUNEBENDER_CONTEXT_PATH`: one designer's habits across every font.
//! 3. **Font** files live beside the font: in the folder that holds the
//!    designspace or UFO and in its parents up to the repository root (the
//!    first parent with a `.git` entry). They state that font's own rules.
//!
//! A context file is any `*.context.md` file; the part before
//! `.context.md` is its name. This is the same arrangement as themes:
//! built-in files in `assets/`, the designer's own in the config folder.

use std::path::{Path, PathBuf};

/// Which layer a context file came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    /// Ships with the editor.
    Default,
    /// The designer's own, in the config folder.
    Personal,
    /// Beside the font.
    Font,
}

impl Layer {
    /// The layer's name on the wire and in headings.
    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Personal => "personal",
            Self::Font => "font",
        }
    }
}

/// One context file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextFile {
    /// Where it came from.
    pub layer: Layer,
    /// Its name: the file name before `.context.md`.
    pub name: String,
    /// Its path on disk; `None` for a built-in default.
    pub path: Option<PathBuf>,
    /// Its text.
    pub text: String,
}

/// The default files, built into the editor.
const DEFAULTS: &[(&str, &str)] = &[
    (
        "type-design",
        include_str!("../../assets/context/type-design.context.md"),
    ),
    (
        "font-engineering",
        include_str!("../../assets/context/font-engineering.context.md"),
    ),
];

/// A file larger than this is skipped with a note, not read: context is
/// for a model's prompt, not for a corpus.
const MAX_FILE_BYTES: u64 = 256 * 1024;

const SUFFIX: &str = ".context.md";

/// Every context file that applies to `font` (a designspace, UFO, or
/// folder; `None` for no font), defaults first and the font's own last.
pub fn load(font: Option<&Path>) -> Vec<ContextFile> {
    load_with(font, &personal_dirs())
}

/// [`load`] with the personal directories given, for callers and tests
/// that do not want the environment consulted.
pub fn load_with(font: Option<&Path>, personal: &[PathBuf]) -> Vec<ContextFile> {
    let mut files: Vec<ContextFile> = DEFAULTS
        .iter()
        .map(|(name, text)| ContextFile {
            layer: Layer::Default,
            name: (*name).to_owned(),
            path: None,
            text: (*text).to_owned(),
        })
        .collect();
    for dir in personal {
        files.extend(read_dir(dir, Layer::Personal));
    }
    if let Some(font) = font {
        for dir in font_dirs(font) {
            files.extend(read_dir(&dir, Layer::Font));
        }
    }
    files
}

/// The personal context directories: the config folder's, then any named
/// by `RUNEBENDER_CONTEXT_PATH`.
pub fn personal_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let config_root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(root) = config_root {
        dirs.push(root.join("runebender").join("context"));
    }
    if let Some(paths) = std::env::var_os("RUNEBENDER_CONTEXT_PATH") {
        dirs.extend(std::env::split_paths(&paths));
    }
    dirs
}

/// The folders searched for a font's own context files: the font's folder
/// and its parents up to and including the repository root. Outermost
/// first, so a file nearer the font is read later and wins. Without a
/// repository, the font's folder and its parent only, so a loose font in
/// a home folder never scans the whole home.
pub fn font_dirs(font: &Path) -> Vec<PathBuf> {
    let start = if font.is_dir() && font.extension().is_none_or(|e| e != "ufo") {
        font.to_path_buf()
    } else {
        font.parent().map_or_else(PathBuf::new, Path::to_path_buf)
    };
    let mut chain: Vec<PathBuf> = Vec::new();
    let mut here = Some(start.as_path());
    let mut found_root = false;
    while let Some(dir) = here {
        chain.push(dir.to_path_buf());
        if dir.join(".git").exists() {
            found_root = true;
            break;
        }
        here = dir.parent();
    }
    if !found_root {
        chain.truncate(2);
    }
    chain.reverse();
    chain
}

fn read_dir(dir: &Path, layer: Layer) -> Vec<ContextFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(SUFFIX) && name.len() > SUFFIX.len())
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.strip_suffix(SUFFIX)?.to_owned();
            let text = match std::fs::metadata(&path) {
                Ok(meta) if meta.len() > MAX_FILE_BYTES => format!(
                    "(skipped: {} is {} bytes, over the {} byte limit for context files)",
                    path.display(),
                    meta.len(),
                    MAX_FILE_BYTES
                ),
                _ => std::fs::read_to_string(&path).ok()?,
            };
            Some(ContextFile {
                layer,
                name,
                path: Some(path),
                text,
            })
        })
        .collect()
}

/// The files as one document for a prompt, with a heading per file and a
/// preamble that states the precedence. With a `budget` in characters,
/// each layer gets an equal share and each file in a layer an equal share
/// of that; a file over its share is cut with a note naming where the
/// rest is.
pub fn merged(files: &[ContextFile], budget: Option<usize>) -> String {
    let mut out = String::from(
        "# Design context\n\nThree layers, and a later layer wins where it disagrees with an \
         earlier one: the font's own rules over the designer's personal rules over the \
         editor's defaults.\n",
    );
    let layers = [Layer::Default, Layer::Personal, Layer::Font];
    let present: Vec<Layer> = layers
        .into_iter()
        .filter(|layer| files.iter().any(|file| file.layer == *layer))
        .collect();
    for layer in present.iter().copied() {
        let in_layer: Vec<&ContextFile> = files.iter().filter(|file| file.layer == layer).collect();
        let share = budget.map(|budget| budget / present.len() / in_layer.len().max(1));
        for file in in_layer {
            out.push_str(&format!("\n## {} context: {}\n\n", layer.name(), file.name));
            match share {
                Some(share) if file.text.chars().count() > share => {
                    let cut: String = file.text.chars().take(share).collect();
                    out.push_str(cut.trim_end());
                    out.push_str(&format!(
                        "\n\n[cut for length; the rest is in {}]\n",
                        file.path.as_deref().map_or_else(
                            || "the editor's built-in file".to_owned(),
                            |p| p.display().to_string()
                        )
                    ));
                }
                _ => {
                    out.push_str(file.text.trim_end());
                    out.push('\n');
                }
            }
        }
    }
    out
}

/// The files as JSON for a tool result: layer, name, path and text each.
pub fn to_json(files: &[ContextFile]) -> serde_json::Value {
    serde_json::Value::Array(
        files
            .iter()
            .map(|file| {
                serde_json::json!({
                    "layer": file.layer.name(),
                    "name": file.name,
                    "path": file.path.as_deref().map(|p| p.display().to_string()),
                    "text": file.text,
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("runebender-context-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn defaults_are_always_present_and_first() {
        let files = load_with(None, &[]);
        let names: Vec<&str> = files.iter().map(|file| file.name.as_str()).collect();
        assert_eq!(names, ["type-design", "font-engineering"]);
        assert!(files.iter().all(|file| file.layer == Layer::Default));
        assert!(files[0].text.contains("# Type design"));
    }

    #[test]
    fn font_files_are_found_up_to_the_repository_root_and_win() {
        let repo = scratch("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("sources")).unwrap();
        std::fs::write(repo.join("type-design.context.md"), "root rules").unwrap();
        std::fs::write(repo.join("sources/spacing.context.md"), "near rules").unwrap();
        // A parent above the repository is never read.
        std::fs::write(repo.parent().unwrap().join("stray.context.md"), "stray").ok();
        let font = repo.join("sources/Family.designspace");
        std::fs::write(&font, "").unwrap();
        let files = load_with(Some(&font), &[]);
        let font_files: Vec<(&str, &str)> = files
            .iter()
            .filter(|file| file.layer == Layer::Font)
            .map(|file| (file.name.as_str(), file.text.as_str()))
            .collect();
        assert_eq!(
            font_files,
            [("type-design", "root rules"), ("spacing", "near rules")]
        );
        let text = merged(&files, None);
        let default_at = text.find("## default context: type-design").unwrap();
        let font_at = text.find("## font context: type-design").unwrap();
        assert!(
            default_at < font_at,
            "the font's file comes after the default"
        );
        let _ = std::fs::remove_file(repo.parent().unwrap().join("stray.context.md"));
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn a_loose_font_reads_only_its_folder_and_parent() {
        let dirs = font_dirs(Path::new("/a/b/c/d/Font.ufo"));
        assert_eq!(dirs, [PathBuf::from("/a/b/c"), PathBuf::from("/a/b/c/d")]);
    }

    #[test]
    fn personal_files_sit_between_defaults_and_the_font() {
        let personal = scratch("personal");
        std::fs::write(personal.join("habits.context.md"), "my habits").unwrap();
        let files = load_with(None, std::slice::from_ref(&personal));
        assert_eq!(files.last().unwrap().layer, Layer::Personal);
        assert_eq!(files.last().unwrap().name, "habits");
        let _ = std::fs::remove_dir_all(personal);
    }

    #[test]
    fn a_budget_cuts_each_file_to_its_share_and_says_where_the_rest_is() {
        let files = vec![
            ContextFile {
                layer: Layer::Default,
                name: "a".into(),
                path: None,
                text: "x".repeat(1000),
            },
            ContextFile {
                layer: Layer::Font,
                name: "b".into(),
                path: Some(PathBuf::from("/font/b.context.md")),
                text: "y".repeat(1000),
            },
        ];
        let text = merged(&files, Some(400));
        assert!(text.contains(&"x".repeat(200)));
        assert!(!text.contains(&"x".repeat(201)));
        assert!(text.contains("the rest is in /font/b.context.md"));
        assert!(text.contains("the rest is in the editor's built-in file"));
    }
}
