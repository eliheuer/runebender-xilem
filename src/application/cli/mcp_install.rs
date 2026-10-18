// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `runebender mcp --install`: write the live MCP server into the
//! configuration of the agent clients on this machine.
//!
//! Each client reads one file. The entry names this binary by absolute
//! path, so a stale `runebender` on PATH can never answer for the
//! editor the person installed. Existing entries are replaced; every
//! other key in the file is kept.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// A client that reads an MCP server list from a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum McpClient {
    /// `~/.claude.json`, the user scope of Claude Code.
    ClaudeCode,
    /// `~/.codex/config.toml`.
    Codex,
    /// Claude Desktop's `claude_desktop_config.json`.
    ClaudeDesktop,
    /// `~/.cursor/mcp.json`.
    Cursor,
}

impl McpClient {
    const ALL: [Self; 4] = [
        Self::ClaudeCode,
        Self::Codex,
        Self::ClaudeDesktop,
        Self::Cursor,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::ClaudeDesktop => "claude-desktop",
            Self::Cursor => "cursor",
        }
    }

    /// The configuration file, under the given home directory.
    fn config_path(self, home: &Path) -> PathBuf {
        match self {
            Self::ClaudeCode => home.join(".claude.json"),
            Self::Codex => home.join(".codex").join("config.toml"),
            Self::ClaudeDesktop => claude_desktop_config(home),
            Self::Cursor => home.join(".cursor").join("mcp.json"),
        }
    }
}

fn claude_desktop_config(home: &Path) -> PathBuf {
    let dir = if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("Claude")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"))
            .join("Claude")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("Claude")
    };
    dir.join("claude_desktop_config.json")
}

/// The server entry every client gets: this binary, by absolute path,
/// in live mode.
fn server_entry(binary: &Path) -> Value {
    json!({
        "command": binary.to_string_lossy(),
        "args": ["mcp", "--live"],
    })
}

/// Writes the entry into each client's file. Returns one report per
/// client, in order, and the first error.
pub(crate) fn install(
    clients: &[McpClient],
    dry_run: bool,
    home: &Path,
    binary: &Path,
) -> Result<Vec<Value>, String> {
    let clients = if clients.is_empty() {
        &McpClient::ALL[..]
    } else {
        clients
    };
    let entry = server_entry(binary);
    let mut reports = Vec::new();
    for client in clients {
        let path = client.config_path(home);
        let existed = path.is_file();
        let before = if existed {
            std::fs::read_to_string(&path)
                .map_err(|error| format!("read {}: {error}", path.display()))?
        } else {
            String::new()
        };
        let after = match client {
            McpClient::Codex => with_toml_entry(&before, &entry),
            _ => with_json_entry(&before, &entry)
                .map_err(|error| format!("{}: {error}", path.display()))?,
        };
        let changed = after != before;
        if changed && !dry_run {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {}: {error}", parent.display()))?;
            }
            std::fs::write(&path, &after)
                .map_err(|error| format!("write {}: {error}", path.display()))?;
        }
        reports.push(json!({
            "client": client.name(),
            "path": path.to_string_lossy(),
            "existed": existed,
            "changed": changed,
            "written": changed && !dry_run,
        }));
    }
    Ok(reports)
}

/// The file with `mcpServers.runebender` set, every other key kept.
fn with_json_entry(before: &str, entry: &Value) -> Result<String, String> {
    let mut root = if before.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str::<Value>(before).map_err(|error| format!("not JSON: {error}"))?
    };
    let Some(object) = root.as_object_mut() else {
        return Err("top level is not an object".into());
    };
    let servers = object.entry("mcpServers").or_insert_with(|| json!({}));
    if !servers.is_object() {
        *servers = json!({});
    }
    servers["runebender"] = entry.clone();
    let mut text = serde_json::to_string_pretty(&root).map_err(|error| error.to_string())?;
    text.push('\n');
    Ok(text)
}

/// The file with a `[mcp_servers.runebender]` table set. The text is
/// edited, not reserialized, so comments and ordering survive.
fn with_toml_entry(before: &str, entry: &Value) -> String {
    let command = entry["command"].as_str().unwrap_or_default();
    let args = entry["args"]
        .as_array()
        .map(|args| {
            args.iter()
                .filter_map(Value::as_str)
                .map(|arg| format!("{arg:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let table = format!("[mcp_servers.runebender]\ncommand = {command:?}\nargs = [{args}]\n");
    let mut out = String::new();
    let mut skipping = false;
    let mut replaced = false;
    for line in before.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if trimmed.trim_start_matches("[").trim_end_matches("]").trim()
                == "mcp_servers.runebender"
            {
                skipping = true;
                if !replaced {
                    out.push_str(&table);
                    replaced = true;
                }
                continue;
            }
            skipping = false;
        }
        if !skipping {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !replaced {
        if !out.is_empty() && !out.ends_with("\n\n") {
            out.push('\n');
        }
        out.push_str(&table);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_entry_keeps_other_keys_and_servers() {
        let before = r#"{"theme":"dark","mcpServers":{"other":{"command":"x"}}}"#;
        let after = with_json_entry(before, &server_entry(Path::new("/opt/rb"))).unwrap();
        let value: Value = serde_json::from_str(&after).unwrap();
        assert_eq!(value["theme"], "dark");
        assert_eq!(value["mcpServers"]["other"]["command"], "x");
        assert_eq!(value["mcpServers"]["runebender"]["command"], "/opt/rb");
        assert_eq!(value["mcpServers"]["runebender"]["args"][1], "--live");
        assert!(with_json_entry("[]", &json!({})).is_err());
        assert!(with_json_entry("not json", &json!({})).is_err());
    }

    #[test]
    fn toml_entry_replaces_its_table_and_keeps_comments() {
        let before = "# mine\nmodel = \"o3\"\n\n[mcp_servers.runebender]\ncommand = \"old\"\nargs = [\"mcp\"]\n\n[mcp_servers.other]\ncommand = \"y\"\n";
        let after = with_toml_entry(before, &server_entry(Path::new("/opt/rb")));
        assert!(after.starts_with("# mine\nmodel = \"o3\"\n"));
        assert!(after.contains(
            "[mcp_servers.runebender]\ncommand = \"/opt/rb\"\nargs = [\"mcp\", \"--live\"]\n"
        ));
        assert!(!after.contains("\"old\""));
        assert!(after.contains("[mcp_servers.other]\ncommand = \"y\"\n"));
        assert_eq!(after.matches("[mcp_servers.runebender]").count(), 1);
        let parsed: toml::Table = toml::from_str(&after).unwrap();
        assert_eq!(
            parsed["mcp_servers"]["runebender"]["args"][1].as_str(),
            Some("--live")
        );
        let fresh = with_toml_entry("", &server_entry(Path::new("/opt/rb")));
        assert!(fresh.starts_with("[mcp_servers.runebender]\n"));
    }

    #[test]
    fn install_is_idempotent_and_dry_run_writes_nothing() {
        let home = std::env::temp_dir().join(format!("rb-mcp-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let binary = Path::new("/opt/rb");
        let dry = install(&[McpClient::Cursor], true, &home, binary).unwrap();
        assert_eq!(dry[0]["changed"], true);
        assert_eq!(dry[0]["written"], false);
        assert!(!home.join(".cursor").exists());
        let first = install(&[], false, &home, binary).unwrap();
        assert_eq!(first.len(), 4);
        assert!(first.iter().all(|report| report["written"] == true));
        let second = install(&[], false, &home, binary).unwrap();
        assert!(second.iter().all(|report| report["changed"] == false));
        let codex = std::fs::read_to_string(home.join(".codex/config.toml")).unwrap();
        assert!(codex.contains("command = \"/opt/rb\""));
        let _ = std::fs::remove_dir_all(&home);
    }
}
