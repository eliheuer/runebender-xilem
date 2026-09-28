// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Proposal listing, installation, removal, and font-ml invocation.

use super::*;

pub(in crate::application::cli) fn proposal_list(source: &Path, json: bool) -> i32 {
    let project = match open_project(source, json) {
        Ok(project) => project,
        Err(code) => return code,
    };
    let source_id = project
        .source_id(0)
        .expect("one source has a stable identity");
    let list = proposal::list_project(&project, source_id);
    if json {
        println!("{}", json!({ "ok": true, "proposals": list }));
    } else if list.is_empty() {
        println!("no proposals");
    } else {
        for p in &list {
            println!(
                "{}: {} glyphs, {} compatible, {} change structure, {} missing",
                p.task,
                p.glyphs.len(),
                p.compatible.len(),
                p.incompatible.len(),
                p.missing.len()
            );
            for (name, why) in &p.incompatible {
                println!("  {name}: {why}");
            }
        }
    }
    exit::OK
}

pub(in crate::application::cli) fn proposal_install(
    source: &Path,
    task: &str,
    glyphs: Option<&[String]>,
    keep_structure: bool,
    json: bool,
) -> i32 {
    let mut project = match open_project(source, json) {
        Ok(project) => project,
        Err(code) => return code,
    };
    let source_id = project
        .source_id(0)
        .expect("one source has a stable identity");
    let done =
        match proposal::install_project(&mut project, source_id, task, glyphs, keep_structure) {
            Ok(done) => done.installed,
            Err(e) => {
                if json {
                    println!("{}", json!({ "ok": false, "error": e }));
                } else {
                    eprintln!("{e}");
                }
                return exit::USAGE;
            }
        };
    if let Err(code) = save_project(&mut project, json) {
        return code;
    }
    if json {
        println!("{}", json!({ "ok": true, "installed": done }));
    } else {
        println!(
            "{}: installed {} glyphs, skipped {}",
            done.task,
            done.installed.len(),
            done.skipped.len()
        );
        for (name, why) in &done.skipped {
            println!("  {name}: {why}");
        }
    }
    exit::OK
}

pub(in crate::application::cli) fn proposal_discard(source: &Path, task: &str, json: bool) -> i32 {
    let mut project = match open_project(source, json) {
        Ok(project) => project,
        Err(code) => return code,
    };
    let source_id = project
        .source_id(0)
        .expect("one source has a stable identity");
    let count = match proposal::discard_project(&mut project, source_id, task) {
        Ok(n) => n,
        Err(e) => {
            if json {
                println!("{}", json!({ "ok": false, "error": e }));
            } else {
                eprintln!("{e}");
            }
            return exit::USAGE;
        }
    };
    if let Err(code) = save_project(&mut project, json) {
        return code;
    }
    if json {
        println!(
            "{}",
            json!({ "ok": true, "task": task, "discarded": count })
        );
    } else {
        println!("{task}: dropped {count} proposed glyphs");
    }
    exit::OK
}

/// Find font-ml from the flag, environment, or PATH.
pub(in crate::application::cli) fn find_font_ml(tool: Option<&Path>) -> Option<PathBuf> {
    if let Some(t) = tool {
        return Some(t.to_path_buf());
    }
    if let Some(t) = std::env::var_os("RUNEBENDER_FONT_ML").filter(|t| !t.is_empty()) {
        return Some(PathBuf::from(t));
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("font-ml"))
        .find(|candidate| candidate.is_file())
}

/// What font-ml says it can do: each task name with whether it is
/// built. None when the tool does not answer, in which case the run
/// itself will say.
fn known_tasks(font_ml: &Path) -> Option<Vec<(String, bool)>> {
    let output = std::process::Command::new(font_ml)
        .arg("tasks")
        .arg("--json")
        .output()
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let tasks = value.get("tasks")?.as_array()?;
    Some(
        tasks
            .iter()
            .filter_map(|t| {
                Some((
                    t.get("name")?.as_str()?.to_string(),
                    t.get("implemented")?.as_bool().unwrap_or(false),
                ))
            })
            .collect(),
    )
}

/// Runs a font-ml task and reports the proposal it left behind.
///
/// font-ml is a separate program on purpose: it carries the model
/// runtime, and this crate does not. The seam is the UFO on disk and
/// the JSON font-ml prints: the task runs with `--write`, so what it
/// predicts lands in the UFO as a proposal layer and nothing touches
/// the foreground. Its exit codes are passed through, so a caller
/// that branches on them sees the same answers either way.
pub(in crate::application::cli) fn propose(
    task: &str,
    source: &Path,
    model: Option<&Path>,
    glyphs: Option<&[String]>,
    tool: Option<&Path>,
    rest: &[String],
    json: bool,
) -> i32 {
    if !source.is_dir() {
        return fail(
            json,
            exit::USAGE,
            &format!("{}: not a UFO directory", source.display()),
        );
    }
    let Some(font_ml) = find_font_ml(tool) else {
        return fail(
            json,
            exit::NOT_BUILT,
            "font-ml is not installed: set RUNEBENDER_FONT_ML, pass --tool, or put \
             font-ml on PATH (cargo install --git https://github.com/eliheuer/font-ml)",
        );
    };
    // The tool says what it can do; ask it before asking it to do
    // something, so an unknown task is a usage error with the list.
    if let Some(known) = known_tasks(&font_ml) {
        if !known.iter().any(|(name, _)| name == task) {
            let names: Vec<&str> = known.iter().map(|(n, _)| n.as_str()).collect();
            return fail(
                json,
                exit::USAGE,
                &format!("unknown task {task}; font-ml knows: {}", names.join(", ")),
            );
        }
        if known.iter().any(|(name, built)| name == task && !built) {
            return fail(
                json,
                exit::NOT_BUILT,
                &format!("{task} is a task font-ml names but has not built yet"),
            );
        }
    }
    let mut cmd = std::process::Command::new(&font_ml);
    cmd.arg("run").arg(task).arg("--source").arg(source);
    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }
    for g in glyphs.into_iter().flatten() {
        cmd.arg("--glyph").arg(g);
    }
    cmd.args(rest).arg("--write").arg("--json");
    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            return fail(
                json,
                exit::FAILED,
                &format!("could not run {}: {e}", font_ml.display()),
            );
        }
    };
    let code = output.status.code().unwrap_or(exit::FAILED);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let tool_report: serde_json::Value = stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str(line).ok())
        .unwrap_or_else(|| json!({ "raw": stdout.trim() }));
    let arrived = Project::load(source).ok().and_then(|project| {
        let source = project.source_id(0)?;
        proposal::find_project(&project, source, task).ok()
    });
    if json {
        println!(
            "{}",
            json!({
                "ok": code == exit::OK,
                "tool": font_ml,
                "exit": code,
                "report": tool_report,
                "proposal": arrived,
            })
        );
    } else {
        print!("{stdout}");
        if code != exit::OK {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }
        match arrived {
            Some(p) => println!(
                "proposal {}: {} glyphs waiting ({} compatible)",
                p.task,
                p.glyphs.len(),
                p.compatible.len()
            ),
            None => println!("no proposal layer written for {task}"),
        }
    }
    code
}
