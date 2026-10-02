// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Theme file creation, validation, and listing.

use super::*;

pub(super) fn theme_command(action: &ThemeAction, json_output: bool) -> i32 {
    use runebender::ui::theme::{builtin_theme_source, parse_theme, validate_theme_id};

    match action {
        ThemeAction::List => {
            let catalog = crate::application::platform::themes::catalog();
            let themes: Vec<_> = catalog
                .ids()
                .iter()
                .filter_map(|id| catalog.get(id).map(|theme| (&theme.id, &theme.name)))
                .collect();
            if json_output {
                println!(
                    "{}",
                    json!({"ok": true, "themes": themes.iter().map(|(id, name)| json!({"id": id, "name": name})).collect::<Vec<_>>()})
                );
            } else {
                for (id, name) in themes {
                    println!("{id}\t{name}");
                }
            }
            exit::OK
        }
        ThemeAction::Validate { file } => {
            let result = (|| {
                let source = std::fs::read_to_string(file).map_err(|error| error.to_string())?;
                parse_theme(&source)
            })();
            match result {
                Ok(theme) => {
                    if json_output {
                        println!(
                            "{}",
                            json!({"ok": true, "id": theme.id, "name": theme.name})
                        );
                    } else {
                        println!("Valid theme: {} ({})", theme.name, theme.id);
                    }
                    exit::OK
                }
                Err(error) => fail(
                    json_output,
                    exit::USAGE,
                    &format!("{}: {error}", file.display()),
                ),
            }
        }
        ThemeAction::Init {
            from,
            id,
            name,
            out,
        } => {
            let result = (|| -> Result<(), String> {
                use std::io::Write as _;

                validate_theme_id(id)?;
                if name.trim().is_empty() {
                    return Err("theme name must not be empty".into());
                }
                if !out
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".theme.toml"))
                {
                    return Err("theme output must end in .theme.toml".into());
                }
                let source = builtin_theme_source(from)?;
                let base_name = parse_theme(source)?.name;
                let id_value = toml::Value::String(id.clone()).to_string();
                let name_value = toml::Value::String(name.clone()).to_string();
                let text = source
                    .replacen(&format!("id = \"{from}\""), &format!("id = {id_value}"), 1)
                    .replacen(
                        &format!("name = \"{base_name}\""),
                        &format!("name = {name_value}"),
                        1,
                    );
                let theme = parse_theme(&text)?;
                if theme.id != *id || theme.name != *name {
                    return Err("could not set the new theme id and name".into());
                }
                let mut file = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(out)
                    .map_err(|error| error.to_string())?;
                file.write_all(text.as_bytes())
                    .map_err(|error| error.to_string())
            })();
            match result {
                Ok(()) => {
                    if json_output {
                        println!(
                            "{}",
                            json!({"ok": true, "id": id, "name": name, "output": out})
                        );
                    } else {
                        println!("Created {}", out.display());
                    }
                    exit::OK
                }
                Err(error) => fail(json_output, exit::USAGE, &error),
            }
        }
    }
}
