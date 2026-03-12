// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Application configuration loaded from
//! `~/.config/runebender/config.toml`.
//!
//! Falls back to environment variables when the config file is
//! missing or a specific key is absent.
//!
//! Example config file:
//! ```toml
//! [quiver]
//! api_key = "sk-your-key-here"
//! ```

use serde::Deserialize;
use std::path::PathBuf;

// ================================================================
// CONFIG STRUCTS
// ================================================================

/// Top-level configuration.
#[derive(Debug, Default, Deserialize)]
pub struct Config {
    /// QuiverAI settings.
    #[serde(default)]
    pub quiver: QuiverConfig,
}

/// QuiverAI-specific configuration.
#[derive(Debug, Default, Deserialize)]
pub struct QuiverConfig {
    /// API key for QuiverAI.
    pub api_key: Option<String>,
}

// ================================================================
// LOADING
// ================================================================

/// Return the path to the config directory
/// (`~/.config/runebender/`).
pub fn config_dir() -> Option<PathBuf> {
    dirs_or_home().map(|base| base.join("runebender"))
}

/// Return the path to the config file
/// (`~/.config/runebender/config.toml`).
pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("config.toml"))
}

/// Load the configuration from disk, falling back to defaults.
pub fn load_config() -> Config {
    let path = match config_path() {
        Some(p) => p,
        None => return Config::default(),
    };

    if !path.exists() {
        tracing::debug!(
            "No config file at {}",
            path.display()
        );
        return Config::default();
    }

    let contents = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                "Failed to read {}: {e}",
                path.display()
            );
            return Config::default();
        }
    };

    match toml::from_str(&contents) {
        Ok(config) => {
            tracing::info!(
                "Loaded config from {}",
                path.display()
            );
            config
        }
        Err(e) => {
            tracing::warn!(
                "Failed to parse {}: {e}",
                path.display()
            );
            Config::default()
        }
    }
}

/// Get the QuiverAI API key. Checks:
/// 1. Config file (`~/.config/runebender/config.toml`)
/// 2. Environment variable (`QUIVERAI_API_KEY`)
///
/// Returns `None` if neither is set.
pub fn quiver_api_key() -> Option<String> {
    // Check config file first
    let config = load_config();
    if let Some(key) = config.quiver.api_key {
        if !key.is_empty() {
            return Some(key);
        }
    }

    // Fall back to environment variable
    std::env::var("QUIVERAI_API_KEY").ok()
}

/// Ensure the config directory exists and write a template
/// config file if none exists.
pub fn ensure_config_dir() {
    let dir = match config_dir() {
        Some(d) => d,
        None => return,
    };

    if !dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(
                "Failed to create config dir {}: {e}",
                dir.display()
            );
            return;
        }
        tracing::info!(
            "Created config directory: {}",
            dir.display()
        );
    }

    let config_file = dir.join("config.toml");
    if !config_file.exists() {
        let template = "\
# Runebender configuration
# See README.md for details.

# [quiver]
# api_key = \"your-quiverai-api-key-here\"
";
        if let Err(e) = std::fs::write(&config_file, template)
        {
            tracing::warn!(
                "Failed to write template config: {e}"
            );
        } else {
            tracing::info!(
                "Created template config at {}",
                config_file.display()
            );
        }
    }
}

// ================================================================
// HELPERS
// ================================================================

/// Get `~/.config` (XDG_CONFIG_HOME or fallback).
fn dirs_or_home() -> Option<PathBuf> {
    // Check XDG_CONFIG_HOME first
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        let path = PathBuf::from(xdg);
        if path.is_absolute() {
            return Some(path);
        }
    }

    // Fall back to ~/.config
    home_dir().map(|h| h.join(".config"))
}

/// Get the user's home directory.
fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}
