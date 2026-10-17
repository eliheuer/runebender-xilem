// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The local AI shapes tool: drag a box on the canvas, type a word,
//! and a model trained on your own drawings fills the box.
//!
//! The canvas owns the box and the typed word; this module owns the
//! models, the drawing request to `font-ml`, the ghost that comes
//! back, and training. A drawing is a proposal until Enter keeps it,
//! and then it is one ordinary undoable edit.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use masonry::kurbo::{self, BezPath, Point, Rect, Shape as _};
use runebender::font::LayerPointType;
use runebender::font::generated::{GeneratedContour, GeneratedPoint};
use runebender::workflows::nodes_run::{self, ShapesModel};
use runebender::workflows::process::{
    self, OutputStream, ProcessCancellation, ProcessLimits, ProcessOutcome,
};

use crate::application::workspace::Workspace;

/// The outcome of a drawing request, once the worker has one.
type DrawOutcome = Arc<Mutex<Option<Result<Vec<GeneratedContour>, String>>>>;

/// What a model drew for a box and a word, waiting to be kept.
#[derive(Debug, Clone)]
pub(crate) struct ShapeGhost {
    pub(crate) word: String,
    pub(crate) seed: u64,
    pub(crate) contours: Vec<GeneratedContour>,
    pub(crate) path: BezPath,
}

/// A drawing request in flight.
#[derive(Debug, Clone)]
pub(crate) struct DrawJob {
    pub(crate) word: String,
    pub(crate) seed: u64,
    finished: DrawOutcome,
    cancellation: ProcessCancellation,
}

/// A training run in flight.
#[derive(Debug, Clone)]
pub(crate) struct TrainJob {
    pub(crate) name: String,
    pub(crate) out: PathBuf,
    pub(crate) started: Instant,
    /// Step done, steps in all, the last note.
    pub(crate) progress: Arc<Mutex<(usize, usize, String)>>,
    /// The latest sample drawing per word, as the trainer reports them.
    pub(crate) samples: Arc<Mutex<Vec<(String, BezPath)>>>,
    finished: Arc<Mutex<Option<Result<serde_json::Value, String>>>>,
    cancellation: ProcessCancellation,
}

/// Which drawings a training run learns from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrainSource {
    /// The open font's own drawn glyphs, added to the chosen model.
    ThisFont,
    /// The sample shapes font that ships with the editor, from scratch.
    SampleFont,
}

/// The pump's message: something arrived from a worker thread.
#[derive(Debug)]
pub(crate) struct ShapesProgress;

/// Everything the tool holds between frames.
#[derive(Debug, Default)]
pub(crate) struct AiShapesState {
    /// The shapes models found on disk, the font's own first.
    pub(crate) installed: Vec<ShapesModel>,
    /// The chosen model directory.
    pub(crate) model: Option<PathBuf>,
    /// The words the chosen model knows.
    pub(crate) words: Vec<String>,
    /// A drawing request in flight.
    pub(crate) job: Option<DrawJob>,
    /// What came back, drawn over the glyph until kept or dropped.
    pub(crate) ghost: Option<Arc<ShapeGhost>>,
    /// One line under the box: what is happening or what went wrong.
    pub(crate) note: String,
    /// A training run in flight.
    pub(crate) training: Option<TrainJob>,
    /// Thorough training (cleaner shapes, longer) rather than quick.
    pub(crate) thorough: bool,
    /// Whether the disk has been looked at since the font opened.
    pub(crate) scanned: bool,
    /// Set while a worker runs, so the view polls; cleared when idle.
    pub(crate) pump: Option<Arc<AtomicBool>>,
    /// The box on the canvas, while one is open.
    pub(crate) rect: Option<Rect>,
    /// The shape picked in the panel, which stays picked from box to
    /// box, with a counter so re-picking still reaches the canvas.
    pub(crate) pick: (String, u64),
}

impl AiShapesState {
    /// The chosen model, as scanned.
    pub(crate) fn chosen(&self) -> Option<&ShapesModel> {
        let model = self.model.as_ref()?;
        self.installed.iter().find(|m| &m.path == model)
    }
}

/// Where the sample shapes font is: `$RUNEBENDER_SAMPLE_SHAPES`, the
/// app bundle's resources, or the source checkout's assets.
pub(crate) fn sample_shapes_ufo() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("RUNEBENDER_SAMPLE_SHAPES").filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        return p.is_dir().then_some(p);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let bundled = dir.join("../Resources/shapes/Shapes.ufo");
        if bundled.is_dir() {
            return Some(bundled);
        }
    }
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/shapes/Shapes.ufo");
    checkout.is_dir().then_some(checkout)
}

/// The contours as one path, for painting a ghost or a sample.
pub(crate) fn contours_path(contours: &[GeneratedContour]) -> BezPath {
    let mut path = BezPath::new();
    for contour in contours {
        let pts = &contour.points;
        if pts.is_empty() {
            continue;
        }
        let open = pts[0].point_type == LayerPointType::Move;
        let Some(start) = pts
            .iter()
            .position(|p| p.point_type != LayerPointType::OffCurve)
        else {
            continue;
        };
        let n = pts.len();
        let at = |i: usize| &pts[(start + i) % n];
        path.move_to(at(0).position);
        let mut pending: Vec<Point> = Vec::new();
        let last = if open { n - 1 } else { n };
        for i in 1..=last {
            let p = at(i);
            // The closing segment back to the start: a line is what
            // close_path draws anyway, so only a curve needs writing.
            let closing = !open && i == n;
            match p.point_type {
                LayerPointType::OffCurve => pending.push(p.position),
                LayerPointType::Line if closing => pending.clear(),
                LayerPointType::Curve if pending.len() == 2 => {
                    path.curve_to(pending[0], pending[1], p.position);
                    pending.clear();
                }
                LayerPointType::QCurve if !pending.is_empty() => {
                    path.quad_to(pending[0], p.position);
                    pending.clear();
                }
                _ => {
                    path.line_to(p.position);
                    pending.clear();
                }
            }
        }
        if !open {
            path.close_path();
        }
    }
    path
}

/// `sample <step> <word> <svg path>` from the trainer.
fn parse_sample(line: &str) -> Option<(usize, String, BezPath)> {
    let rest = line.strip_prefix("sample ")?;
    let (step, rest) = rest.split_once(' ')?;
    let (word, svg) = rest.split_once(' ')?;
    let path = BezPath::from_svg(svg).ok()?;
    Some((step.parse().ok()?, word.to_string(), path))
}

/// `progress <done>/<total> <note>` from the trainer.
fn parse_progress(line: &str) -> Option<(usize, usize, &str)> {
    let rest = line.strip_prefix("progress ")?;
    let (count, note) = rest.split_once(' ')?;
    let (done, total) = count.split_once('/')?;
    Some((done.parse().ok()?, total.parse().ok()?, note.trim()))
}

/// The contours in a `draw` report.
fn contours_from(report: &serde_json::Value) -> Result<Vec<GeneratedContour>, String> {
    let list = report
        .get("contours")
        .and_then(|c| c.as_array())
        .ok_or("the report has no contours")?;
    let mut out = Vec::new();
    for contour in list {
        let pts = contour.as_array().ok_or("a contour is not a list")?;
        let mut points = Vec::new();
        for p in pts {
            let x = p
                .get("x")
                .and_then(|v| v.as_f64())
                .ok_or("a point has no x")?;
            let y = p
                .get("y")
                .and_then(|v| v.as_f64())
                .ok_or("a point has no y")?;
            let point_type = match p.get("type").and_then(|v| v.as_str()) {
                Some("move") => LayerPointType::Move,
                Some("line") => LayerPointType::Line,
                Some("offcurve") => LayerPointType::OffCurve,
                Some("curve") => LayerPointType::Curve,
                Some("qcurve") => LayerPointType::QCurve,
                other => return Err(format!("unknown point type {other:?}")),
            };
            let smooth = p.get("smooth").and_then(|v| v.as_bool()).unwrap_or(false);
            points.push(GeneratedPoint {
                position: Point::new(x, y),
                point_type,
                smooth,
            });
        }
        if points.len() >= 2 {
            out.push(GeneratedContour { points });
        }
    }
    if out.is_empty() {
        return Err("the model drew nothing".into());
    }
    Ok(out)
}

impl Workspace {
    /// `<Font>.models` beside the open font: where its own shapes models live.
    pub(crate) fn font_models_dir(&self) -> Option<PathBuf> {
        let source = self.font.source();
        let stem = source.file_stem()?.to_str()?;
        Some(source.with_file_name(format!("{stem}.models")))
    }

    /// Look at the disk again for shapes models, and keep a sensible choice.
    pub(crate) fn shapes_rescan(&mut self) {
        let beside = self.font_models_dir();
        self.shapes.installed =
            nodes_run::installed_shapes(Self::models_dir().as_deref(), beside.as_deref());
        self.shapes.scanned = true;
        let still_there = self
            .shapes
            .model
            .as_ref()
            .is_some_and(|m| self.shapes.installed.iter().any(|i| &i.path == m));
        if !still_there {
            self.shapes.model = self.shapes.installed.first().map(|m| m.path.clone());
        }
        self.shapes.words = self
            .shapes
            .chosen()
            .map(|m| m.words.clone())
            .unwrap_or_default();
        self.shapes_default_pick();
    }

    pub(crate) fn choose_shapes_model(&mut self, path: &Path) {
        self.shapes.model = Some(path.to_path_buf());
        self.shapes.words = self
            .shapes
            .chosen()
            .map(|m| m.words.clone())
            .unwrap_or_default();
        self.shapes.ghost = None;
        self.shapes.note.clear();
        self.shapes_default_pick();
    }

    /// Keep the picked shape one the model knows, so a dragged box
    /// always draws something.
    fn shapes_default_pick(&mut self) {
        if !self.shapes.words.contains(&self.shapes.pick.0)
            && let Some(first) = self.shapes.words.first()
        {
            self.shapes.pick = (first.clone(), self.shapes.pick.1 + 1);
        }
    }

    fn shapes_pump_token(&mut self) -> Arc<AtomicBool> {
        if let Some(t) = &self.shapes.pump
            && t.load(Ordering::Relaxed)
        {
            return t.clone();
        }
        let t = Arc::new(AtomicBool::new(true));
        self.shapes.pump = Some(t.clone());
        t
    }

    /// A new box was dragged: whatever was drawn before no longer applies.
    pub(crate) fn shapes_box_set(&mut self, rect: Rect) {
        if let Some(job) = self.shapes.job.take() {
            job.cancellation.cancel();
        }
        self.shapes.rect = Some(rect);
        self.shapes.ghost = None;
        self.shapes.note.clear();
        // The picked shape goes in at once; typing is optional detail.
        let shape = self.shapes.pick.0.clone();
        if shape.is_empty() {
            self.shapes.note = "Pick a shape in the panel".into();
        } else {
            self.shapes_draw(rect, &shape, 0);
        }
        if !self.shapes.scanned {
            self.shapes_rescan();
        }
        if self.shapes.model.is_none() {
            self.shapes.note = "No shapes model yet. Train one in the panel.".into();
        }
    }

    pub(crate) fn shapes_drop(&mut self) {
        if let Some(job) = self.shapes.job.take() {
            job.cancellation.cancel();
        }
        self.shapes.rect = None;
        self.shapes.ghost = None;
        self.shapes.note.clear();
    }

    /// A shape was clicked in the panel. It stays picked; with a box
    /// open, it goes in there now.
    pub(crate) fn shapes_pick_word(&mut self, word: &str) {
        self.shapes.pick = (word.to_owned(), self.shapes.pick.1 + 1);
        match self.shapes.rect {
            Some(rect) => self.shapes_draw(rect, word, 0),
            None => self.shapes.note = format!("Drag a box on the glyph to place a {word}"),
        }
    }

    /// Draw the picked shape in `rect` again, with another seed.
    pub(crate) fn shapes_redraw(&mut self, rect: Rect, seed: u64) {
        let shape = self.shapes.pick.0.clone();
        if shape.is_empty() {
            self.shapes.note = "Pick a shape in the panel".into();
            return;
        }
        self.shapes_draw(rect, &shape, seed);
    }

    pub(crate) fn shapes_kept(&mut self, word: &str) {
        self.shapes.rect = None;
        self.shapes.ghost = None;
        self.shapes.note = format!("Added {word}. Undo removes it.");
        self.finish_open_glyph_refresh();
    }

    /// Ask the model for `word` in `rect`; `seed` 0 is the most likely
    /// drawing, any other seed another option.
    pub(crate) fn shapes_draw(&mut self, rect: Rect, word: &str, seed: u64) {
        if !self.shapes.scanned {
            self.shapes_rescan();
        }
        let Some(model) = self.shapes.model.clone() else {
            self.shapes.note = "No shapes model yet. Train one in the panel.".into();
            return;
        };
        let Some(font_ml) = self.nodes.font_ml.clone() else {
            self.shapes.note = "font-ml not found. Install it, or set RUNEBENDER_FONT_ML.".into();
            return;
        };
        let word = word.trim().to_lowercase();
        if word.is_empty() {
            self.shapes.note = "Type a word first.".into();
            return;
        }
        let known = self.shapes.words.iter().any(|w| w == &word);
        if !known && !self.shapes.words.is_empty() {
            self.shapes.note = format!(
                "This model does not know “{word}”. Draw a glyph named {word} and train again."
            );
            return;
        }
        if let Some(job) = self.shapes.job.take() {
            job.cancellation.cancel();
        }
        self.shapes.ghost = None;
        self.shapes.note = if seed == 0 {
            format!("Drawing {word}…")
        } else {
            format!("Another {word}…")
        };
        let device = self.nodes.device.clone();
        let job = DrawJob {
            word: word.clone(),
            seed,
            finished: Arc::new(Mutex::new(None)),
            cancellation: ProcessCancellation::default(),
        };
        self.shapes.job = Some(job.clone());
        let token = self.shapes_pump_token();
        std::thread::spawn(move || {
            let mut cmd = std::process::Command::new(font_ml);
            cmd.arg("run")
                .arg("draw")
                .arg("--model")
                .arg(&model)
                .arg("--prompt")
                .arg(&word)
                .arg("--box")
                .arg(format!(
                    "{},{},{},{}",
                    rect.x0.round(),
                    rect.y0.round(),
                    rect.width().round(),
                    rect.height().round()
                ))
                .arg("--seed")
                .arg(seed.to_string())
                .arg("--json");
            cmd.arg("--device").arg(&device);
            let limits = ProcessLimits {
                deadline: std::time::Duration::from_secs(120),
                ..ProcessLimits::default()
            };
            let output = process::run(&mut cmd, &[], limits, &job.cancellation, |_, _| {});
            let report: serde_json::Value = String::from_utf8_lossy(&output.stdout)
                .lines()
                .rev()
                .find_map(|l| serde_json::from_str(l).ok())
                .unwrap_or(serde_json::Value::Null);
            let result = match output.outcome {
                _ if job.cancellation.is_cancelled() => Err("cancelled".into()),
                ProcessOutcome::Exited { success: true, .. } => contours_from(&report),
                _ => Err(report
                    .get("error")
                    .and_then(|e| e.as_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        String::from_utf8_lossy(&output.stderr)
                            .lines()
                            .next_back()
                            .unwrap_or("font-ml failed")
                            .to_string()
                    })),
            };
            *job.finished.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
            let _ = token;
        });
    }

    /// The pump: take what the worker threads have finished.
    pub(crate) fn shapes_pump(&mut self) {
        if let Some(job) = self.shapes.job.clone() {
            let finished = job
                .finished
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            if let Some(result) = finished {
                self.shapes.job = None;
                match result {
                    Ok(contours) => {
                        let points: usize = contours.iter().map(|c| c.points.len()).sum();
                        let path = contours_path(&contours);
                        self.shapes.ghost = Some(Arc::new(ShapeGhost {
                            word: job.word.clone(),
                            seed: job.seed,
                            contours,
                            path,
                        }));
                        self.shapes.note = format!(
                            "{}: {points} points. Enter keeps it, Tab draws another, Esc drops it.",
                            job.word
                        );
                    }
                    Err(e) if e == "cancelled" => {}
                    Err(e) => self.shapes.note = format!("Could not draw: {e}"),
                }
            }
        }
        if let Some(train) = self.shapes.training.clone() {
            let finished = train
                .finished
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            if let Some(result) = finished {
                self.shapes.training = None;
                match result {
                    Ok(_) => {
                        self.shapes_rescan();
                        if let Some(m) = self.shapes.installed.iter().find(|m| m.path == train.out)
                        {
                            let path = m.path.clone();
                            self.choose_shapes_model(&path);
                        }
                        let mins = train.started.elapsed().as_secs() / 60;
                        self.shapes.note = format!(
                            "{} is ready after {mins} min. Drag a box and type a word.",
                            train.name
                        );
                        self.note = self.shapes.note.clone();
                    }
                    Err(e) if e == "cancelled" => {
                        self.shapes.note = "Training stopped.".into();
                    }
                    Err(e) => {
                        self.shapes.note = format!("Training failed: {e}");
                        self.note = self.shapes.note.clone();
                    }
                }
            }
        }
        if self.shapes.job.is_none()
            && self.shapes.training.is_none()
            && let Some(t) = self.shapes.pump.take()
        {
            t.store(false, Ordering::Relaxed);
        }
    }

    /// Start a training run. From the sample font it makes a fresh base
    /// with spare word slots; from this font it adds the font's words to
    /// the chosen model, or makes a fresh one when there is none.
    pub(crate) fn shapes_train(&mut self, source: TrainSource) {
        if self.shapes.training.is_some() {
            self.shapes.note = "A training run is already going.".into();
            return;
        }
        let Some(font_ml) = self.nodes.font_ml.clone() else {
            self.shapes.note = "font-ml not found. Install it, or set RUNEBENDER_FONT_ML.".into();
            return;
        };
        let (ufo, name, init) = match source {
            TrainSource::SampleFont => {
                let Some(ufo) = sample_shapes_ufo() else {
                    self.shapes.note = "The sample shapes font was not found.".into();
                    return;
                };
                (ufo, "shapes-base".to_string(), None)
            }
            TrainSource::ThisFont => {
                let source = self.font.source().to_path_buf();
                if source.extension().is_none_or(|e| e != "ufo") {
                    self.shapes.note = "Open a UFO master to train from its drawings.".into();
                    return;
                }
                let stem = source
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("font")
                    .to_string();
                let init = self.shapes.model.clone().filter(|m| {
                    // Continuing from the font's own earlier model is the
                    // same as training it again with its spare slots.
                    nodes_run::shapes_model_at(m).is_some_and(|model| model.spare > 0)
                });
                (source, format!("{stem}-shapes"), init)
            }
        };
        let out_root = match source {
            TrainSource::SampleFont => nodes_run::models_dir_for("shapes"),
            TrainSource::ThisFont => self.font_models_dir().map(|d| d.join("shapes")),
        };
        let Some(out_root) = out_root else {
            self.shapes.note = "No place to write the model: no home directory.".into();
            return;
        };
        let mut out = out_root.join(&name);
        let mut n = 2;
        while out.exists() {
            out = out_root.join(format!("{name}-{n}"));
            n += 1;
        }
        let name = out
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("shapes")
            .to_string();
        let steps = match (source, self.shapes.thorough, init.is_some()) {
            (_, _, true) => {
                if self.shapes.thorough {
                    3000
                } else {
                    1000
                }
            }
            (_, true, false) => 15000,
            (_, false, false) => 6000,
        };
        let sample_words: Vec<String> = nodes_run::shape_words_in(&ufo)
            .into_iter()
            .take(3)
            .collect();
        // Continuing from a base: keep its own drawings in the mix, so
        // the old words are not forgotten while the new ones are learned.
        let replay: Vec<PathBuf> = init
            .as_ref()
            .and_then(|m| std::fs::read_to_string(m.join("manifest.json")).ok())
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|m| m.get("sources")?.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(PathBuf::from))
            .filter(|p| p.is_dir() && p != &ufo)
            .collect();
        let device = self.nodes.device.clone();
        let job = TrainJob {
            name: name.clone(),
            out: out.clone(),
            started: Instant::now(),
            progress: Arc::new(Mutex::new((0, steps, String::new()))),
            samples: Arc::new(Mutex::new(Vec::new())),
            finished: Arc::new(Mutex::new(None)),
            cancellation: ProcessCancellation::default(),
        };
        self.shapes.training = Some(job.clone());
        self.shapes.note = format!("Training {name}…");
        let token = self.shapes_pump_token();
        std::thread::spawn(move || {
            let mut cmd = std::process::Command::new(font_ml);
            cmd.arg("run")
                .arg("train-draw")
                .arg("--source")
                .arg(&ufo)
                .arg("--out")
                .arg(&out)
                .arg("--steps")
                .arg(steps.to_string())
                .arg("--dims")
                .arg("256")
                .arg("--layers")
                .arg("6")
                .arg("--heads")
                .arg("8")
                .arg("--batch")
                .arg("16")
                .arg("--spare")
                .arg("16")
                .arg("--json");
            if !sample_words.is_empty() {
                cmd.arg("--sample-words").arg(sample_words.join(","));
            }
            if let Some(init) = &init {
                cmd.arg("--init").arg(init);
            }
            for also in &replay {
                cmd.arg("--also").arg(also);
            }
            cmd.arg("--device").arg(&device);
            let limits = ProcessLimits {
                deadline: std::time::Duration::from_secs(4 * 3600),
                stderr_bytes: 64 * 1024 * 1024,
                ..ProcessLimits::default()
            };
            let progress = job.progress.clone();
            let samples = job.samples.clone();
            let output = process::run(&mut cmd, &[], limits, &job.cancellation, |stream, line| {
                if stream != OutputStream::Stderr {
                    return;
                }
                if let Some((done, total, note)) = parse_progress(line) {
                    *progress.lock().unwrap_or_else(|e| e.into_inner()) =
                        (done, total, note.to_string());
                } else if let Some((_, word, path)) = parse_sample(line) {
                    let mut list = samples.lock().unwrap_or_else(|e| e.into_inner());
                    match list.iter_mut().find(|(w, _)| *w == word) {
                        Some(slot) => slot.1 = path,
                        None => list.push((word, path)),
                    }
                }
            });
            let report: serde_json::Value = String::from_utf8_lossy(&output.stdout)
                .lines()
                .rev()
                .find_map(|l| serde_json::from_str(l).ok())
                .unwrap_or(serde_json::Value::Null);
            let result = match output.outcome {
                _ if job.cancellation.is_cancelled() => Err("cancelled".into()),
                ProcessOutcome::Exited { success: true, .. } => Ok(report),
                _ => Err(report
                    .get("error")
                    .and_then(|e| e.as_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        String::from_utf8_lossy(&output.stderr)
                            .lines()
                            .rfind(|l| !l.starts_with("progress ") && !l.starts_with("sample "))
                            .unwrap_or("font-ml failed")
                            .to_string()
                    })),
            };
            *job.finished.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
            let _ = token;
        });
    }

    pub(crate) fn shapes_cancel_training(&mut self) {
        if let Some(job) = &self.shapes.training {
            job.cancellation.cancel();
            self.shapes.note = "Stopping…".into();
        }
    }
}

/// A ghost or sample path fitted into a square of `size` pixels.
pub(crate) fn fit_path(path: &BezPath, size: f64, inset: f64) -> BezPath {
    let bounds = path.bounding_box();
    if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
        return path.clone();
    }
    let scale = ((size - 2.0 * inset) / bounds.width().max(bounds.height())).max(1e-6);
    let dx = inset + ((size - 2.0 * inset) - bounds.width() * scale) / 2.0;
    let dy = inset + ((size - 2.0 * inset) - bounds.height() * scale) / 2.0;
    let affine = kurbo::Affine::translate((dx, size - dy))
        * kurbo::Affine::scale_non_uniform(scale, -scale)
        * kurbo::Affine::translate((-bounds.x0, -bounds.y0));
    affine * path.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_square_report_becomes_one_closed_contour() {
        let report: serde_json::Value = serde_json::json!({
            "contours": [[
                {"x": 0.0, "y": 0.0, "type": "line", "smooth": false},
                {"x": 10.0, "y": 0.0, "type": "line", "smooth": false},
                {"x": 10.0, "y": 10.0, "type": "line", "smooth": false},
                {"x": 0.0, "y": 10.0, "type": "line", "smooth": false}
            ]]
        });
        let contours = contours_from(&report).unwrap();
        assert_eq!(contours.len(), 1);
        assert_eq!(contours[0].points.len(), 4);
        let path = contours_path(&contours);
        assert_eq!(path.elements().len(), 5);
        assert!(matches!(
            path.elements().last(),
            Some(kurbo::PathEl::ClosePath)
        ));
    }

    #[test]
    fn sample_and_progress_lines_parse() {
        let (step, word, path) = parse_sample("sample 500 star M0 0 L10 0 L10 10 Z").unwrap();
        assert_eq!((step, word.as_str()), (500, "star"));
        assert_eq!(path.elements().len(), 4);
        assert_eq!(
            parse_progress("progress 10/6000 loss 2.1"),
            Some((10, 6000, "loss 2.1"))
        );
    }
}
