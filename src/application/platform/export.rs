// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Background font export through a repository build script or `fontc`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::application::workspace::Workspace;

/// One export in progress.
#[derive(Clone)]
pub(crate) struct ExportJob {
    /// Filled once by the worker and consumed by the Xilem pump.
    pub(crate) finished: Arc<Mutex<Option<Result<String, String>>>>,
}

/// Message sent when the worker has finished.
#[derive(Debug)]
pub(crate) struct ExportProgress;

fn build_script(source: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut directory = source.parent()?;
    for _ in 0..4 {
        for name in ["build-fontc.sh", "build.sh"] {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some((candidate, directory.to_path_buf()));
            }
        }
        if directory.join(".git").exists() {
            break;
        }
        directory = directory.parent()?;
    }
    None
}

fn command_path(workdir: Option<&Path>) -> std::ffi::OsString {
    let mut paths = Vec::new();
    if let Some(workdir) = workdir {
        paths.push(workdir.join(".venv/bin"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home).join(".cargo/bin"));
    }
    paths.push(PathBuf::from("/opt/homebrew/bin"));
    paths.push(PathBuf::from("/usr/local/bin"));
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path));
    }
    std::env::join_paths(paths.into_iter().filter(|path| path.exists()))
        .unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}

fn fontc() -> Option<PathBuf> {
    if std::process::Command::new("fontc")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return Some(PathBuf::from("fontc"));
    }
    let home = std::env::var_os("HOME")?;
    let binary = PathBuf::from(home).join(".cargo/bin/fontc");
    binary.is_file().then_some(binary)
}

fn run(source: PathBuf) -> Result<String, String> {
    if let Some((script, workdir)) = build_script(&source) {
        let label = script
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "build script".into());
        let output = std::process::Command::new("/bin/bash")
            .arg(script)
            .current_dir(&workdir)
            .env("PATH", command_path(Some(&workdir)))
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            return Ok(format!(
                "Exported through {label} → {}",
                workdir.join("fonts").display()
            ));
        }
        return Err(String::from_utf8_lossy(&output.stderr)
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("build script failed")
            .to_string());
    }

    let Some(fontc) = fontc() else {
        return Err("fontc not found: cargo install fontc".into());
    };
    let output_directory = source
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("exports");
    std::fs::create_dir_all(&output_directory).map_err(|error| error.to_string())?;
    let stem = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "font".into());
    let output_path = output_directory.join(format!("{stem}.ttf"));
    let build_directory = std::env::temp_dir().join("runebender-fontc");
    let output = std::process::Command::new(fontc)
        .arg(&source)
        .arg("--output-file")
        .arg(&output_path)
        .arg("--build-dir")
        .arg(build_directory)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("fontc failed")
            .to_string());
    }
    let fixed = std::process::Command::new("gftools-fix-font")
        .arg("-o")
        .arg(&output_path)
        .arg(&output_path)
        .env("PATH", command_path(source.parent()))
        .output()
        .is_ok_and(|output| output.status.success());
    Ok(if fixed {
        format!("Exported {} (gftools fixes applied)", output_path.display())
    } else {
        format!("Exported {}", output_path.display())
    })
}

impl Workspace {
    /// Save dirty sources, then export without blocking the UI thread.
    pub(crate) fn command_export(&mut self) {
        if self.export_job.is_some() {
            self.note = "an export is already running".into();
            return;
        }
        if self.modified && !self.save() {
            return;
        }
        let source = self.font.document_source().to_path_buf();
        if !source.exists() {
            self.note = "Save the font before exporting".into();
            return;
        }
        let finished = Arc::new(Mutex::new(None));
        let worker = finished.clone();
        std::thread::spawn(move || {
            *worker.lock().unwrap_or_else(|error| error.into_inner()) = Some(run(source));
        });
        self.export_job = Some(ExportJob { finished });
        self.note = "Exporting…".into();
    }

    /// Consume the worker result posted by the Xilem pump.
    pub(crate) fn export_pump(&mut self) {
        let Some(job) = self.export_job.as_ref() else {
            return;
        };
        let result = job
            .finished
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(result) = result {
            self.note = result.unwrap_or_else(|error| format!("Export failed: {error}"));
            self.export_job = None;
        }
    }
}
