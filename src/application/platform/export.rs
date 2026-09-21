// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Export the live document through the same Rust compiler used by preview.

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
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

#[cfg(not(target_arch = "wasm32"))]
fn run(
    source: PathBuf,
    compiled: runebender::font::compile::CompiledFont,
) -> Result<String, String> {
    let directory = source
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("exports");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let stem = source.file_stem().unwrap_or_default().to_string_lossy();
    let path = directory.join(format!("{stem}.ttf"));
    std::fs::write(&path, compiled.bytes.as_slice()).map_err(|error| error.to_string())?;
    Ok(format!("Exported {}", path.display()))
}

impl Workspace {
    /// Compile an immutable snapshot of unsaved edits without saving source files.
    pub(crate) fn command_export(&mut self) {
        if self.export_job.is_some() {
            self.note = "an export is already running".into();
            return;
        }
        let snapshot = match self.font.project.babelfont_snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.note = format!("Export failed: {error}");
                return;
            }
        };
        let source = self.font.document_source().to_path_buf();
        let finished = Arc::new(Mutex::new(None));
        let worker = finished.clone();
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            let result = runebender::font::compile::CompiledFont::build(snapshot)
                .and_then(|compiled| run(source, compiled));
            *worker.lock().unwrap_or_else(|error| error.into_inner()) = Some(result);
        });
        #[cfg(target_arch = "wasm32")]
        {
            let result = runebender::font::compile::CompiledFont::build(snapshot).map(|compiled| {
                let name = format!(
                    "{}.ttf",
                    source.file_stem().unwrap_or_default().to_string_lossy()
                );
                crate::application::browser::download_font(&name, compiled.bytes.as_slice());
                format!("Exported {name}")
            });
            *worker.lock().unwrap_or_else(|error| error.into_inner()) = Some(result);
        }
        self.export_job = Some(ExportJob { finished });
        self.note = "Exporting…".into();
        #[cfg(target_arch = "wasm32")]
        self.export_pump();
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
