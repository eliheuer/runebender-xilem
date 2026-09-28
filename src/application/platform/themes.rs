// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Local theme discovery for the application.
//!
//! Parsing and color resolution live in the font engine's `ui::theme`.
//! This module owns filesystem paths and the process-lifetime catalog.

use std::collections::HashMap;
use std::sync::OnceLock;

use runebender::ui::theme::{Theme, load_theme_checked};

/// Built-in and installed themes, resolved once at application startup.
pub(crate) struct ThemeCatalog {
    themes: HashMap<String, Theme>,
    order: Vec<String>,
}

static CATALOG: OnceLock<ThemeCatalog> = OnceLock::new();

/// The process-lifetime catalog shared by the window and its workspaces.
pub(crate) fn catalog() -> &'static ThemeCatalog {
    CATALOG.get_or_init(ThemeCatalog::load)
}

impl ThemeCatalog {
    fn load() -> Self {
        let mut catalog = Self {
            themes: HashMap::new(),
            order: Vec::new(),
        };
        for id in ["dark", "gray", "light"] {
            let theme = load_theme_checked(id).unwrap_or_else(|error| panic!("{error}"));
            catalog.insert(theme).expect("unique built-in theme");
        }
        #[cfg(not(target_arch = "wasm32"))]
        catalog.discover_external();
        catalog
    }

    fn insert(&mut self, theme: Theme) -> Result<(), String> {
        let id = theme.id.clone();
        runebender::ui::theme::validate_theme_id(&id)?;
        if self.themes.contains_key(&id) {
            return Err(format!("theme id '{id}' is already installed"));
        }
        self.order.push(id.clone());
        self.themes.insert(id, theme);
        Ok(())
    }

    /// Resolve one installed theme by ID.
    pub(crate) fn get(&self, id: &str) -> Option<&Theme> {
        self.themes.get(id)
    }

    /// IDs in menu and cycle order: built-ins first, then local themes.
    pub(crate) fn ids(&self) -> &[String] {
        &self.order
    }

    /// Read `RUNEBENDER_THEME` once, falling back to Gray with a useful message.
    pub(crate) fn initial_id(&'static self) -> &'static str {
        if let Ok(requested) = std::env::var("RUNEBENDER_THEME") {
            if let Some((id, _)) = self.themes.get_key_value(&requested) {
                return id;
            }
            eprintln!("Unknown Runebender theme '{requested}'; using Gray");
        }
        "gray"
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn discover_external(&mut self) {
        use std::path::PathBuf;

        let config_root = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
        if let Some(root) = config_root {
            self.load_path(&root.join("runebender").join("themes"));
        }
        if let Some(paths) = std::env::var_os("RUNEBENDER_THEME_PATH") {
            for path in std::env::split_paths(&paths) {
                self.load_path(&path);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load_path(&mut self, path: &std::path::Path) {
        if path.is_file() {
            self.load_file(path);
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        let mut files: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.ends_with(".theme.toml") || name.ends_with(".theme.json")
                    })
            })
            .collect();
        files.sort();
        for file in files {
            self.load_file(&file);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load_file(&mut self, path: &std::path::Path) {
        let result = (|| {
            let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
            if metadata.len() > 1024 * 1024 {
                return Err("theme file exceeds 1 MiB".to_string());
            }
            let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
            let theme = runebender::ui::theme::parse_theme(&source)?;
            self.insert(theme)
        })();
        if let Err(error) = result {
            eprintln!("Theme {}: {error}", path.display());
        }
    }
}
