// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Train a neural font from the open `.nufo` source, in the editor.
//!
//! The run is post-opentype's `scripts/train-nufo.sh`, as a subprocess. It writes a numbered
//! version beside the source, in `<Name>.models/<NNN>/`, with a manifest, the log and the
//! font. Training happens on the configured host when it answers, else on this machine. The
//! script's lines stream into the panel while it runs.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::application::workspace::Workspace;

/// One training run in progress.
#[derive(Clone)]
pub(crate) struct TrainJob {
    /// Lines the script printed, in order; the pump drains them.
    pub(crate) lines: Arc<Mutex<Vec<String>>>,
    /// Filled once when the script exits.
    pub(crate) finished: Arc<Mutex<Option<Result<String, String>>>>,
    /// The script's process, to cancel.
    child: Arc<Mutex<Option<std::process::Child>>>,
    /// Where the run happens: the host, or "this machine".
    pub(crate) place: Arc<Mutex<String>>,
}

/// Message sent while the worker runs and when it has finished.
#[derive(Debug)]
pub(crate) struct TrainProgress;

/// One trained version beside the source.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ModelVersion {
    /// The folder name, such as `003`.
    pub name: String,
    /// The folder.
    pub path: PathBuf,
    /// The last epoch line of its log, if any.
    pub last_epoch: Option<String>,
    /// Its font, when exported.
    pub font: Option<PathBuf>,
}

impl ModelVersion {
    /// The labeled `IoU` of the last epoch line, such as `0.93`, if present.
    pub(crate) fn score(&self) -> Option<String> {
        let line = self.last_epoch.as_deref()?;
        let at = line.find("labeled IoU ")?;
        let rest = &line[at + "labeled IoU ".len()..];
        Some(rest.split_whitespace().next()?.to_string())
    }

    /// The epoch count from the last epoch line.
    pub(crate) fn epochs(&self) -> Option<String> {
        let line = self.last_epoch.as_deref()?;
        Some(line.split_whitespace().nth(1)?.to_string())
    }
}

/// The state of training for the open source.
#[derive(Default)]
pub(crate) struct TrainState {
    /// A run in progress.
    pub(crate) job: Option<TrainJob>,
    /// The last line worth showing: the latest epoch, or where the run is.
    pub(crate) status: String,
    /// The versions beside the source, oldest first.
    pub(crate) versions: Vec<ModelVersion>,
}

/// The folder beside `source` that holds its versions: `Name.models` next to `Name.nufo`.
pub(crate) fn models_dir(source: &Path) -> PathBuf {
    let stem = source.file_stem().unwrap_or_default().to_string_lossy();
    source
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{stem}.models"))
}

/// The versions in `dir`, oldest first: the numbered runs on the whole source, and under
/// each `sample-…` folder the numbered runs on that sample alone, named `sample-…/NNN`.
pub(crate) fn versions(dir: &Path) -> Vec<ModelVersion> {
    let folders = |dir: &Path| -> Vec<PathBuf> {
        let mut names: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        names.sort();
        names
    };
    let folder_name = |path: &Path| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    };
    let mut versions: Vec<ModelVersion> = Vec::new();
    for path in folders(dir) {
        let folder = folder_name(&path);
        if folder.starts_with("sample-") {
            for run in folders(&path) {
                let name = format!("{folder}/{}", folder_name(&run));
                versions.push(version_at(run, name));
            }
        } else {
            versions.push(version_at(path, folder));
        }
    }
    // Oldest first by when the run began, so the newest run of any kind is last.
    versions.sort_by_key(|version| {
        std::fs::metadata(&version.path)
            .and_then(|meta| meta.created().or_else(|_| meta.modified()))
            .ok()
    });
    versions
}

/// One version folder, read from its log.
fn version_at(path: PathBuf, name: String) -> ModelVersion {
    let log = std::fs::read_to_string(path.join("train.log")).unwrap_or_default();
    let last_epoch = log
        .lines()
        .rfind(|line| line.starts_with("epoch"))
        .map(str::to_string);
    let font = path.join("font.ntf");
    ModelVersion {
        name,
        font: font.exists().then_some(font),
        path,
        last_epoch,
    }
}

/// The host answers ssh within a few seconds.
#[cfg(not(target_arch = "wasm32"))]
fn host_answers(host: &str) -> bool {
    std::process::Command::new("ssh")
        .args([
            "-o",
            "ConnectTimeout=5",
            "-o",
            "BatchMode=yes",
            host,
            "true",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

impl Workspace {
    /// Read the versions beside the open source.
    pub(crate) fn refresh_model_versions(&mut self) {
        let source = self.font.document_source().to_path_buf();
        self.train.versions = versions(&models_dir(&source));
    }

    /// Start a training run on the saved source. Unsaved edits are saved first, because the
    /// script reads the file on disk.
    pub(crate) fn command_train(&mut self) {
        let epochs = crate::application::platform::config::neural().epochs;
        self.start_training(epochs, None);
    }

    /// A short run on the open sample alone, into `<Name>.models/sample-<canvas>-<n>/`.
    pub(crate) fn command_train_sample(&mut self) {
        let Some((position, sample)) = self.session.selected_sample() else {
            self.note = "Open a sample to train on it alone".into();
            return;
        };
        if !sample.unlabeled().is_empty() {
            self.note = "Label every letter of the sample first".into();
            return;
        }
        let epochs = crate::application::platform::config::neural().sample_epochs;
        let only = format!("{} #{}", self.session.glyph_name, position + 1);
        self.start_training(epochs, Some(only));
    }

    /// Start a run of `epochs`, on `only` one sample ("canvas #n") or on every sample.
    fn start_training(&mut self, epochs: u32, only: Option<String>) {
        if self.train.job.is_some() {
            self.note = "a training run is already going".into();
            return;
        }
        if self.modified && !self.save() {
            return;
        }
        let source = self.font.document_source().to_path_buf();
        let name = source
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let settings = crate::application::platform::config::neural();
        let script = settings.post_opentype.join("scripts/train-nufo.sh");
        if !script.exists() {
            self.note = format!(
                "No training script at {}; set [neural] post_opentype in the config file",
                script.display()
            );
            return;
        }
        let mut out_dir = models_dir(&source);
        if let Some(only) = &only {
            // "ba-basic #2" runs go under sample-ba-basic-2.
            let folder: String = only
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect();
            out_dir = out_dir.join(format!("sample-{}", folder.trim_matches('-')));
        }
        let job = TrainJob {
            lines: Arc::new(Mutex::new(Vec::new())),
            finished: Arc::new(Mutex::new(None)),
            child: Arc::new(Mutex::new(None)),
            place: Arc::new(Mutex::new("…".into())),
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let worker = job.clone();
            std::thread::spawn(move || {
                let result = run(
                    &worker,
                    &settings,
                    epochs,
                    only.as_deref(),
                    &script,
                    &source,
                    &name,
                    &out_dir,
                );
                *worker.finished.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
            });
        }
        self.train.status = "Starting…".into();
        self.train.job = Some(job);
        self.note = "Training…".into();
    }

    /// Stop the run. A remote trainer that was started keeps going on its machine.
    pub(crate) fn cancel_train(&mut self) {
        let Some(job) = self.train.job.as_ref() else {
            return;
        };
        if let Some(child) = job.child.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            let _ = child.kill();
        }
        self.train.status = "Cancelled".into();
    }

    /// Drain the script's lines into the status, and finish the run when it has exited.
    pub(crate) fn train_pump(&mut self) {
        let Some(job) = self.train.job.as_ref() else {
            return;
        };
        let lines: Vec<String> =
            std::mem::take(&mut *job.lines.lock().unwrap_or_else(|e| e.into_inner()));
        let place = job.place.lock().unwrap_or_else(|e| e.into_inner()).clone();
        for line in lines {
            if let Some(status) = status_of(&line, &place) {
                self.train.status = status;
            }
        }
        let finished = job
            .finished
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(result) = finished {
            match result {
                Ok(message) => {
                    self.note = message;
                    self.train.status.clear();
                }
                Err(error) => {
                    self.note = format!("Training failed: {error}");
                    self.train.status = error;
                }
            }
            self.train.job = None;
            self.refresh_model_versions();
        }
    }
}

/// What a line of the script's output means for the status, if anything.
fn status_of(line: &str, place: &str) -> Option<String> {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix("epoch") {
        // "epoch  12  train loss 0.01  ...  labeled IoU 0.93  disp err 4.0u  (1s)"
        let epoch = rest.split_whitespace().next().unwrap_or("?");
        let score = line
            .find("labeled IoU ")
            .and_then(|at| line[at + 12..].split_whitespace().next())
            .map(|s| format!(", match {s}"))
            .unwrap_or_default();
        return Some(format!("Training on {place}, epoch {epoch}{score}"));
    }
    if line.starts_with("canvas:") || line.starts_with("wrote ") || line.starts_with("device:") {
        return Some(format!("{place}: {line}"));
    }
    if line.contains("panicked") || line.starts_with("error") {
        return Some(line.to_string());
    }
    None
}

/// Run the script and stream its lines. Returns the script's last line on success, or the
/// last few lines on failure.
#[cfg(not(target_arch = "wasm32"))]
#[expect(
    clippy::too_many_arguments,
    reason = "one call site, each a distinct setting"
)]
fn run(
    job: &TrainJob,
    settings: &crate::application::platform::config::Neural,
    epochs: u32,
    only: Option<&str>,
    script: &Path,
    source: &Path,
    name: &str,
    out_dir: &Path,
) -> Result<String, String> {
    use std::io::BufRead as _;
    let host = settings
        .train_host
        .as_deref()
        .filter(|host| host_answers(host))
        .map(str::to_string);
    *job.place.lock().unwrap_or_else(|e| e.into_inner()) =
        host.clone().unwrap_or_else(|| "this machine".into());
    let mut command = std::process::Command::new("sh");
    command
        .arg(script)
        .arg(source)
        .arg(name)
        .arg(epochs.to_string())
        .current_dir(&settings.post_opentype)
        .env("OUT_DIR", out_dir)
        .env("NTF_ONLY", only.unwrap_or_default())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    match &host {
        Some(host) => {
            command.env("TRAIN_HOST", host);
        }
        None => {
            command.env_remove("TRAIN_HOST");
        }
    }
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    *job.child.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
    let mut readers = Vec::new();
    for reader in [
        stdout.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        stderr.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let lines = job.lines.clone();
        readers.push(std::thread::spawn(move || {
            let mut tail: Vec<String> = Vec::new();
            for line in std::io::BufReader::new(reader)
                .lines()
                .map_while(Result::ok)
            {
                tail.push(line.clone());
                if tail.len() > 12 {
                    tail.remove(0);
                }
                lines.lock().unwrap_or_else(|e| e.into_inner()).push(line);
            }
            tail
        }));
    }
    let tails: Vec<Vec<String>> = readers
        .into_iter()
        .map(|reader| reader.join().unwrap_or_default())
        .collect();
    let status = {
        let mut guard = job.child.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_mut() {
            Some(child) => child.wait().map_err(|error| error.to_string())?,
            None => return Err("the run was lost".into()),
        }
    };
    *job.child.lock().unwrap_or_else(|e| e.into_inner()) = None;
    if status.success() {
        Ok(tails
            .first()
            .and_then(|tail| tail.last().cloned())
            .unwrap_or_else(|| "Trained".into()))
    } else {
        let mut lines: Vec<String> = tails.into_iter().flatten().collect();
        lines.retain(|line| !line.trim().is_empty());
        let last: Vec<String> = lines.into_iter().rev().take(4).collect();
        Err(last.into_iter().rev().collect::<Vec<_>>().join(" | "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_models_folder_sits_beside_the_source_and_versions_read_their_logs() {
        let dir = std::env::temp_dir().join(format!("rb-train-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let source = dir.join("Demo.nufo");
        std::fs::create_dir_all(&source).unwrap();
        let models = models_dir(&source);
        assert_eq!(models, dir.join("Demo.models"));
        std::fs::create_dir_all(models.join("001")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::create_dir_all(models.join("002")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::create_dir_all(models.join("sample-ba-basic-2/001")).unwrap();
        std::fs::write(
            models.join("001/train.log"),
            "device: Cpu\nepoch   1  train loss 0.5  val mse NaN  val IoU NaN  labeled IoU 0.1234  disp err 9.0u  (1s)\nepoch   2  train loss 0.4  val mse NaN  val IoU NaN  labeled IoU 0.9900  disp err 0.5u  (1s)\nsaved\n",
        )
        .unwrap();
        std::fs::write(models.join("001/font.ntf"), b"NTF0").unwrap();
        let versions = versions(&models);
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[2].name, "sample-ba-basic-2/001");
        assert_eq!(versions[0].name, "001");
        assert_eq!(versions[0].score().as_deref(), Some("0.9900"));
        assert_eq!(versions[0].epochs().as_deref(), Some("2"));
        assert!(versions[0].font.is_some());
        assert_eq!(versions[1].name, "002");
        assert!(versions[1].last_epoch.is_none() && versions[1].font.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn a_fake_script_streams_its_lines_and_finishes() {
        let dir = std::env::temp_dir().join(format!("rb-train-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        let script = dir.join("scripts/train-nufo.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\necho \"source $1 name $2 epochs $3 out $OUT_DIR host ${TRAIN_HOST:-none} only ${NTF_ONLY:-all}\"\necho 'epoch   1  train loss 0.5  val mse NaN  val IoU NaN  labeled IoU 0.5000  disp err 9.0u  (1s)'\necho 'wrote 001/font.ntf'\n",
        )
        .unwrap();
        let job = TrainJob {
            lines: Arc::new(Mutex::new(Vec::new())),
            finished: Arc::new(Mutex::new(None)),
            child: Arc::new(Mutex::new(None)),
            place: Arc::new(Mutex::new(String::new())),
        };
        let settings = crate::application::platform::config::Neural {
            post_opentype: dir.clone(),
            train_host: None,
            epochs: 7,
            sample_epochs: 3,
        };
        let out = dir.join("Demo.models");
        let result = run(
            &job,
            &settings,
            7,
            Some("ba #2"),
            &script,
            &dir.join("Demo.nufo"),
            "Demo",
            &out,
        );
        assert_eq!(result.as_deref(), Ok("wrote 001/font.ntf"));
        let lines = job.lines.lock().unwrap().clone();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].ends_with("name Demo epochs 7 out ") || lines[0].contains("epochs 7 out"));
        assert!(lines[0].ends_with("host none only ba #2"), "{}", lines[0]);
        assert_eq!(*job.place.lock().unwrap(), "this machine");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn epoch_lines_become_a_status() {
        assert_eq!(
            status_of(
                "epoch  12  train loss 0.01  val mse NaN  val IoU NaN  labeled IoU 0.93  disp err 4.0u  (1s)",
                "kiln"
            )
            .as_deref(),
            Some("Training on kiln, epoch 12, match 0.93")
        );
        assert_eq!(status_of("random", "kiln"), None);
    }
}
