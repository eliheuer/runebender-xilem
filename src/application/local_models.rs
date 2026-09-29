// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Discover data-only local model packages separately from the installed inference runtime.
//! Model manifests cannot supply commands; the host config selects the trusted Python runtime.

pub(crate) struct ModelEntry {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) ready: bool,
}

#[cfg(unix)]
pub(crate) use native::{discover, open_folder, resolve};

#[cfg(not(unix))]
pub(crate) fn discover() -> Vec<ModelEntry> {
    Vec::new()
}

#[cfg(not(unix))]
pub(crate) fn open_folder() -> Result<(), String> {
    Err("Local models require the native Unix application".into())
}

#[cfg(unix)]
mod native {
    use super::ModelEntry;
    use std::path::{Path, PathBuf};

    use runebender::workflows::local_sketch::SketchRuntime;
    use serde::Deserialize;

    const DEFAULT_MODEL: &str = "virtua-12m-v1";
    const FILES: [&str; 3] = ["config.json", "vocab.txt", "weights.safetensors"];

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Manifest {
        name: String,
        format: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RuntimeConfig {
        repository: PathBuf,
    }

    fn root() -> Result<PathBuf, String> {
        if let Some(path) = std::env::var_os("RUNEBENDER_MODELS_DIR").filter(|v| !v.is_empty()) {
            return Ok(PathBuf::from(path));
        }
        let home = std::env::var_os("HOME").ok_or("HOME unavailable; set RUNEBENDER_MODELS_DIR")?;
        Ok(PathBuf::from(home).join("runebender/models"))
    }

    fn valid_id(id: &str) -> bool {
        !id.is_empty()
            && id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    }

    fn manifest(root: &Path, id: &str) -> Result<Manifest, String> {
        if !valid_id(id) {
            return Err(
                "Invalid model folder name; use letters, numbers, hyphens or underscores".into(),
            );
        }
        let path = root.join(id);
        let bytes = std::fs::read(path.join("manifest.json"))
            .map_err(|_| "Missing manifest.json".to_owned())?;
        let manifest: Manifest =
            serde_json::from_slice(&bytes).map_err(|error| format!("Invalid manifest: {error}"))?;
        if manifest.format != "virtua-sketch-v1" {
            return Err(format!("Unsupported model format: {}", manifest.format));
        }
        for file in FILES {
            if !path.join(file).is_file() {
                return Err(format!("Missing {file}"));
            }
        }
        Ok(manifest)
    }

    fn repository(root: &Path) -> Result<PathBuf, String> {
        let repository = if let Some(path) = std::env::var_os("RUNEBENDER_SKETCH_REPOSITORY") {
            PathBuf::from(path)
        } else {
            let bytes = std::fs::read(root.join("runtime.json")).map_err(|_| {
                "Missing Virtua runtime: configure runtime.json in Models Folder".to_owned()
            })?;
            serde_json::from_slice::<RuntimeConfig>(&bytes)
                .map_err(|error| format!("Invalid runtime.json: {error}"))?
                .repository
        };
        if !repository.is_absolute() {
            return Err("Virtua runtime repository must be an absolute path".into());
        }
        if !repository.join(".venv/bin/python").is_file()
            || !repository.join("glyphlab/sketch2glyph.py").is_file()
        {
            return Err("Virtua Python runtime is unavailable".into());
        }
        Ok(repository)
    }

    pub(crate) fn discover() -> Vec<ModelEntry> {
        let Ok(root) = root() else {
            return Vec::new();
        };
        discover_at(&root)
    }

    fn discover_at(root: &Path) -> Vec<ModelEntry> {
        let Ok(entries) = std::fs::read_dir(root) else {
            return Vec::new();
        };
        let mut result = Vec::new();
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            let info = manifest(root, &id);
            let name = info
                .as_ref()
                .map_or_else(|_| id.clone(), |m| m.name.clone());
            let status = info
                .and_then(|_| repository(root).map(|_| ()))
                .map_or_else(|error| error, |()| "Ready · Virtua sketch".into());
            let ready = status.starts_with("Ready");
            result.push(ModelEntry {
                id,
                name,
                status,
                ready,
            });
        }
        result.sort_by(|a, b| a.id.cmp(&b.id));
        result
    }

    pub(crate) fn resolve(model: Option<&str>) -> Result<SketchRuntime, String> {
        let root = root()?;
        let repository = repository(&root)?;
        let checkpoint = if let Some(model) = model {
            manifest(&root, model)?;
            root.join(model)
        } else if let Some(path) = std::env::var_os("RUNEBENDER_SKETCH_CHECKPOINT") {
            PathBuf::from(path)
        } else {
            manifest(&root, DEFAULT_MODEL)?;
            root.join(DEFAULT_MODEL)
        };
        Ok(SketchRuntime {
            python: repository.join(".venv/bin/python"),
            repository,
            checkpoint,
            script_home: PathBuf::from(std::env::var_os("HOME").ok_or("HOME unavailable")?),
        })
    }

    pub(crate) fn open_folder() -> Result<(), String> {
        let root = root()?;
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        #[cfg(target_os = "macos")]
        let command = "open";
        #[cfg(not(target_os = "macos"))]
        let command = "xdg-open";
        let status = std::process::Command::new(command)
            .arg(root)
            .status()
            .map_err(|error| format!("Could not open Models Folder: {error}"))?;
        if !status.success() {
            return Err(format!("Could not open Models Folder: {status}"));
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn discovery_reports_runtime_and_package_errors_without_executing_code() {
            let root =
                std::env::temp_dir().join(format!("runebender-library-{}", std::process::id()));
            let model = root.join("virtua-test");
            std::fs::create_dir_all(&model).unwrap();
            std::fs::write(
                model.join("manifest.json"),
                br#"{"name":"Test Virtua","format":"virtua-sketch-v1"}"#,
            )
            .unwrap();
            for file in FILES {
                std::fs::write(model.join(file), b"fixture").unwrap();
            }
            assert_eq!(manifest(&root, "virtua-test").unwrap().name, "Test Virtua");
            std::fs::remove_file(model.join("vocab.txt")).unwrap();
            let entries = discover_at(&root);
            assert_eq!(entries.len(), 1);
            assert!(!entries[0].ready);
            assert_eq!(entries[0].status, "Missing vocab.txt");
            std::fs::write(
                model.join("manifest.json"),
                br#"{"name":"Other","format":"unknown"}"#,
            )
            .unwrap();
            assert!(
                manifest(&root, "virtua-test")
                    .err()
                    .unwrap()
                    .contains("Unsupported")
            );
            std::fs::remove_dir_all(root).unwrap();
        }

        #[test]
        fn model_ids_cannot_escape_library() {
            for id in ["", "../clean1", "/tmp/model", "a/b", "."] {
                assert!(!valid_id(id));
            }
            assert!(valid_id("virtua-12m-v1"));
        }

        #[test]
        fn missing_library_and_invalid_package_are_actionable() {
            let root = PathBuf::from("/nonexistent/runebender-model-test");
            assert!(discover_at(&root).is_empty());
            assert_eq!(
                manifest(&root, "test").err().unwrap(),
                "Missing manifest.json"
            );
        }
    }
}
