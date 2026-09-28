// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Single agent tool calls and their source-font adapters.

use super::*;

/// The UFO a font path stands for: the UFO itself, or the first
/// master of a designspace.
fn font_master(font: &Path, master: Option<usize>) -> Result<PathBuf, String> {
    let project = Project::load(font)?;
    let sources = project.document_sources().collect::<Vec<_>>();
    let index = match master {
        Some(index) => index,
        None if sources.len() == 1 => 0,
        None => return Err("master is required for a family; call project_info first".into()),
    };
    sources
        .get(index)
        .map(|source| source.path().to_path_buf())
        .ok_or_else(|| format!("no master at index {index}"))
}

fn project_info(font: &Path) -> serde_json::Value {
    match Project::load(font) {
        Ok(project) => {
            json!({"ok": true, "project": font, "masters": project.document_sources().enumerate()
            .map(|(index, source)| json!({"index": index, "name": source.name(), "source": source.path()}))
            .collect::<Vec<_>>()})
        }
        Err(e) => json!({"ok": false, "error": e}),
    }
}

/// Runs this same binary with `args` and returns the last JSON line
/// it printed. Every tool is a command the binary already has, so the
/// model's reach is exactly the command line's.
fn self_json(args: &[String]) -> serde_json::Value {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("runebender"));
    let output = std::process::Command::new(exe).args(args).output();
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout
                .lines()
                .rev()
                .find_map(|l| serde_json::from_str(l).ok())
                .unwrap_or_else(
                    || json!({ "ok": false, "error": String::from_utf8_lossy(&o.stderr).trim() }),
                )
        }
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
}

/// One glyph as the model reads it.
fn read_glyph(source: &Path, name: &str, layer: Option<&str>) -> serde_json::Value {
    let project = match Project::load(source) {
        Ok(project) => project,
        Err(error) => return json!({ "ok": false, "error": error }),
    };
    let Some(source_id) = project.document_sources().next().map(|source| source.id()) else {
        return json!({ "ok": false, "error": "the font has no source" });
    };
    runebender::analysis::glyph::read_project_glyph(&project, source_id, name, layer)
}

/// Searches the documentation folders for passages that match.
///
/// Roots: `$RUNEBENDER_DOCS` (colon-separated) and
/// `~/.runebender/docs`. Every `.md`, `.txt` and `.html` file is split
/// into paragraphs; a paragraph scores one per query word it holds,
/// and the top five come back with their file. Plain and offline: a
/// model that reads the spec beats one that remembers it.
fn docs_search(query: &str) -> serde_json::Value {
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        return json!({ "ok": false, "error": "give a few words to look for" });
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(extra) = std::env::var_os("RUNEBENDER_DOCS") {
        roots.extend(std::env::split_paths(&extra));
    }
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".runebender").join("docs"));
    }
    let mut hits: Vec<(usize, String, String)> = Vec::new();
    let mut stack: Vec<PathBuf> = roots.iter().filter(|r| r.is_dir()).cloned().collect();
    let mut files = 0;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "md" | "txt" | "html" | "mdx") {
                continue;
            }
            let Ok(mut text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if ext == "html" {
                text = strip_html(&text);
            }
            files += 1;
            for para in text.split("\n\n") {
                let lower = para.to_lowercase();
                let score = words.iter().filter(|w| lower.contains(w.as_str())).count();
                if score > 0 {
                    let snippet: String = para.chars().take(600).collect();
                    hits.push((
                        score,
                        path.display().to_string(),
                        snippet.trim().to_string(),
                    ));
                }
            }
        }
    }
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    hits.truncate(5);
    json!({
        "ok": true,
        "files_searched": files,
        "roots": roots,
        "passages": hits.iter().map(|(score, file, text)| json!({ "score": score, "file": file, "text": text })).collect::<Vec<_>>(),
        "note": if files == 0 { "No documentation found. Put .md or .txt files under ~/.runebender/docs or set RUNEBENDER_DOCS." } else { "" },
    })
}

/// HTML as text: tags dropped, block ends as paragraph breaks, the
/// few entities a spec page uses decoded. Enough for a search hit to
/// read as prose.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut tag = String::new();
    let mut skip_depth = 0_usize;
    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let lower = tag.to_lowercase();
                let name = lower
                    .trim_start_matches('/')
                    .split(|c: char| c.is_whitespace() || c == '/')
                    .next()
                    .unwrap_or("");
                if matches!(name, "script" | "style" | "nav" | "header" | "footer") {
                    if lower.starts_with('/') {
                        skip_depth = skip_depth.saturating_sub(1);
                    } else {
                        skip_depth += 1;
                    }
                } else if matches!(
                    name,
                    "p" | "div" | "tr" | "li" | "h1" | "h2" | "h3" | "h4" | "pre" | "table" | "br"
                ) && !lower.starts_with('/')
                {
                    out.push_str("\n\n");
                } else if matches!(name, "td" | "th") && !lower.starts_with('/') {
                    out.push(' ');
                }
            }
            _ if in_tag => tag.push(ch),
            _ if skip_depth > 0 => {}
            _ => out.push(ch),
        }
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// `agent call`: one tool, mapped onto the command it already is.
pub(super) fn agent_call(
    name: &str,
    font: Option<&Path>,
    session: Option<&Path>,
    args: &str,
    tool: Option<&Path>,
) -> i32 {
    let args: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => {
            println!(
                "{}",
                json!({ "ok": false, "error": format!("arguments: {e}") })
            );
            return exit::USAGE;
        }
    };
    let result = dispatch_call(name, font, session, &args, tool);
    let ok = result.get("ok").and_then(|v| v.as_bool()).unwrap_or(true);
    println!(
        "{}",
        json!(agent::ToolResult {
            name: name.to_string(),
            ok,
            result
        })
    );
    if ok { exit::OK } else { exit::FAILED }
}

/// Selects the explicitly requested transport; a failed live connection never uses disk.
pub(super) fn dispatch_call(
    name: &str,
    font: Option<&Path>,
    session: Option<&Path>,
    args: &serde_json::Value,
    tool: Option<&Path>,
) -> serde_json::Value {
    let inherited = std::env::var_os("RUNEBENDER_LIVE_SESSION").map(PathBuf::from);
    if let Some(session) = session.or(inherited.as_deref()) {
        #[cfg(unix)]
        return runebender::automation::live_socket::call(
            session,
            &agent::ToolCall {
                name: name.into(),
                arguments: args.clone(),
            },
        )
        .unwrap_or_else(|e| json!({"ok": false, "error": e.to_string()}));
        #[cfg(not(unix))]
        return json!({"ok": false, "error": format!("live sockets unsupported: {}", session.display())});
    }
    match font {
        Some(font) => agent_call_value(name, font, args, tool),
        None => json!({"ok": false, "error": "font or session required"}),
    }
}

/// Runs one tool call and returns what it gave back, as JSON with an
/// `ok` field. Every tool is a command of this binary, run through it.
fn agent_call_value(
    name: &str,
    font: &Path,
    args: &serde_json::Value,
    tool: Option<&Path>,
) -> serde_json::Value {
    if !args.is_object() {
        return json!({"ok": false, "error": "arguments must be an object"});
    }
    if name == "project_info" {
        return project_info(font);
    }
    if name == "docs" {
        return docs_search(args.get("query").and_then(|v| v.as_str()).unwrap_or(""));
    }
    let master = match args.get("master") {
        None => None,
        Some(value) => match value.as_u64().and_then(|v| usize::try_from(v).ok()) {
            Some(index) => Some(index),
            None => return json!({"ok": false, "error": "master must be a nonnegative integer"}),
        },
    };
    if args.get("glyphs").is_some_and(|v| {
        !v.as_array()
            .is_some_and(|a| a.iter().all(|n| n.is_string()))
    }) {
        return json!({"ok": false, "error": "glyphs must be an array of names"});
    }
    if args.get("layer").is_some_and(|v| !v.is_string()) {
        return json!({"ok": false, "error": "layer must be a string"});
    }
    let source = match font_master(font, master) {
        Ok(s) => s,
        Err(e) => return json!({ "ok": false, "error": e }),
    };
    let mut result = agent_call_source(name, font, &source, args, tool);
    if let Some(object) = result.as_object_mut() {
        object.insert("source".into(), json!(source));
        object.insert("master".into(), json!(master.unwrap_or(0)));
    }
    result
}

fn agent_call_source(
    name: &str,
    _font: &Path,
    source: &Path,
    args: &serde_json::Value,
    tool: Option<&Path>,
) -> serde_json::Value {
    let src = source.display().to_string();
    let glyphs = agent::glyph_list(args);
    let text = |key: &str| args.get(key).and_then(|v| v.as_str()).map(str::to_string);
    match name {
        "font_info" => self_json(&["--json".into(), "info".into(), src]),
        "read_glyph" => match text("glyph") {
            Some(g) => read_glyph(source, &g, args.get("layer").and_then(|v| v.as_str())),
            None => json!({ "ok": false, "error": "glyph is required" }),
        },
        "proof" => {
            let out = std::env::temp_dir().join(format!(
                "runebender-proof-{}-{}.svg",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let mut a = vec![
                "--json".to_string(),
                "proof".into(),
                src,
                "--out".into(),
                out.display().to_string(),
            ];
            if !glyphs.is_empty() {
                a.push("--glyphs".into());
                a.push(glyphs.join(","));
            }
            if let Some(layer) = text("layer") {
                a.extend(["--layer".into(), layer]);
            }
            let mut result = self_json(&a);
            if result["ok"] == true
                && let Some(path) = result["svg"].as_str()
                && let Ok(svg) = std::fs::read_to_string(path)
            {
                result["svg_content"] = json!(svg);
            }
            result
        }
        "propose_edits" => {
            let mut batch = args.clone();
            batch
                .as_object_mut()
                .expect("object validated")
                .remove("master");
            match serde_json::from_value::<runebender::font::edit_batch::EditBatch>(batch) {
                Ok(batch) => {
                    match runebender::formats::proposal_ufo::save_proposal(source, &batch) {
                        Ok(summary) => json!({"ok": true, "proposal": summary}),
                        Err(e) => json!({"ok": false, "error": e}),
                    }
                }
                Err(e) => json!({"ok": false, "error": e.to_string()}),
            }
        }
        "propose" => match (text("task"), text("model")) {
            (Some(task), Some(model)) => {
                let model_dir = nodes_run::installed(None, false)
                    .into_iter()
                    .find(|(n, _)| *n == model)
                    .map(|(_, p)| p)
                    .unwrap_or_else(|| PathBuf::from(&model));
                let mut a = vec![
                    "--json".to_string(),
                    "propose".into(),
                    task,
                    src,
                    "--model".into(),
                    model_dir.display().to_string(),
                ];
                if !glyphs.is_empty() {
                    a.push("--glyphs".into());
                    a.push(glyphs.join(","));
                }
                if let Some(t) = tool {
                    a.push("--tool".into());
                    a.push(t.display().to_string());
                }
                // The per-point deltas are for a tool, not a model
                // reading prose; without them the result is a few
                // hundred characters instead of thousands.
                let mut v = self_json(&a);
                if let Some(rows) = v
                    .get_mut("report")
                    .and_then(|r| r.get_mut("glyphs"))
                    .and_then(|g| g.as_array_mut())
                {
                    for row in rows {
                        if let Some(o) = row.as_object_mut() {
                            o.remove("deltas");
                        }
                    }
                }
                if let Some(r) = v.get_mut("report").and_then(|r| r.as_object_mut()) {
                    r.remove("deltas");
                }
                v
            }
            _ => json!({ "ok": false, "error": "task and model are required" }),
        },
        "nodes_run" => match text("file") {
            Some(file) => {
                let mut a = vec![
                    "--json".to_string(),
                    "nodes".into(),
                    "run".into(),
                    file,
                    "--font".into(),
                    source.display().to_string(),
                    "--proposal-only".into(),
                ];
                if !glyphs.is_empty() {
                    a.push("--glyphs".into());
                    a.push(glyphs.join(","));
                }
                if let Some(t) = tool {
                    a.push("--tool".into());
                    a.push(t.display().to_string());
                }
                self_json(&a)
            }
            None => json!({ "ok": false, "error": "file is required" }),
        },
        "proposal_list" => self_json(&["--json".into(), "proposal".into(), "list".into(), src]),
        "proposal_discard" => match text("task") {
            Some(task) => self_json(&[
                "--json".into(),
                "proposal".into(),
                "discard".into(),
                src,
                "--task".into(),
                task,
            ]),
            None => json!({ "ok": false, "error": "task is required" }),
        },
        "docs" => docs_search(&text("query").unwrap_or_default()),
        other => json!({ "ok": false, "error": format!("no tool named {other}") }),
    }
}
