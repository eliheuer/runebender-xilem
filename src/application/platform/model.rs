// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A trained font drawn in the proof strip, through the `NeuralType` engine.
//!
//! The engine's font holds caches that are not thread-safe, so a worker thread owns it. The
//! workspace sends it the text and the pulled nodes; the worker composes the words, traces
//! the field and hands back paths in font units, y up, with one node per caret index on the
//! chain of letters. Dragging a node pulls its letter and the rest of the word, and the join
//! before it stretches, as in the web demo. `post-opentype/docs/VIEWER.md` is the contract
//! between the two viewers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use kurbo::{Affine, BezPath, Point};
use neuraltype_core::field_model::FieldFont;
use neuraltype_core::{field_line, field_text};

/// What the worker is asked to draw.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ModelRequest {
    pub text: String,
    /// Node index to its pull, in field pixels, y down.
    pub offsets: Vec<(usize, (f64, f64))>,
    /// The selected caret indices, `(anchor, caret)`; equal for a plain caret.
    pub selection: (usize, usize),
    /// Trace with img2bez, as the web demo's finished outlines are; false while a node is
    /// dragged, so the drag stays quick.
    pub quality: bool,
}

/// The drawn text: paths and nodes in font units, y up, the first word's right edge at x = 0.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ModelRender {
    pub request: ModelRequest,
    pub paths: Vec<BezPath>,
    /// One point per caret index, 0 ..= the text's character count.
    pub nodes: Vec<Point>,
    /// For each node, whether it touches a word boundary; it is drawn hollow.
    pub gaps: Vec<bool>,
    /// The strand through the nodes, densely sampled, each point with its parameter.
    pub strand: Vec<(f64, Point)>,
    /// Each node's parameter on the strand.
    pub node_t: Vec<f64>,
    /// The selection cloud; empty with a plain caret.
    pub outline: Vec<BezPath>,
    /// Field pixels per font unit, to turn a dragged distance into a pull.
    pub px_per_unit: f64,
}

/// The worker's newest drawing, or why it has none.
type Latest = Arc<Mutex<Option<Result<Arc<ModelRender>, String>>>>;

/// The worker that owns one loaded font.
pub(crate) struct ModelJob {
    /// The font file the worker loaded.
    pub font: PathBuf,
    requests: Sender<ModelRequest>,
    latest: Latest,
    /// The last request sent, so the same one is not sent twice.
    sent: Option<ModelRequest>,
}

/// Message sent while the worker draws.
#[derive(Debug)]
pub(crate) struct ModelProgress;

/// The model view's state.
#[derive(Default)]
pub(crate) struct ModelState {
    pub job: Option<ModelJob>,
    /// The last drawing that arrived.
    pub render: Option<Arc<ModelRender>>,
    /// Why there is no drawing, when there is none.
    pub error: Option<String>,
    /// The version chosen in the Neural section; None for the latest with a font.
    pub version: Option<String>,
    /// Pulled nodes: caret index to the pull in field pixels, y down.
    pub offsets: HashMap<usize, (f64, f64)>,
    /// A node drag in progress: the node, and its pull when the drag began.
    pub drag: Option<(usize, (f64, f64))>,
    /// The selected caret indices, `(anchor, caret)`.
    pub selection: (usize, usize),
    /// The text the pulls belong to; another text clears them.
    pub text: String,
}

impl ModelJob {
    /// Load `font` on a new thread.
    pub(crate) fn start(font: &Path) -> Self {
        let (requests, receiver) = channel::<ModelRequest>();
        let latest: Latest = Arc::new(Mutex::new(None));
        let path = font.to_path_buf();
        let store = latest.clone();
        std::thread::spawn(move || worker(&path, &receiver, &store));
        Self {
            font: font.to_path_buf(),
            requests,
            latest,
            sent: None,
        }
    }

    /// Ask for a drawing, unless the same one was already asked for.
    pub(crate) fn request(&mut self, request: ModelRequest) {
        if self.sent.as_ref() == Some(&request) {
            return;
        }
        self.sent = Some(request.clone());
        let _ = self.requests.send(request);
    }

    /// The newest result, once.
    pub(crate) fn take(&self) -> Option<Result<Arc<ModelRender>, String>> {
        self.latest.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

/// Load the font, then draw every request that arrives, skipping to the newest when several
/// are waiting.
fn worker(
    path: &Path,
    receiver: &Receiver<ModelRequest>,
    latest: &Mutex<Option<Result<Arc<ModelRender>, String>>>,
) {
    let font = std::fs::read(path)
        .map_err(|error| format!("{}: {error}", path.display()))
        .and_then(|bytes| FieldFont::load(&bytes));
    let font = match font {
        Ok(font) => font,
        Err(error) => {
            *latest.lock().unwrap_or_else(|e| e.into_inner()) = Some(Err(error));
            return;
        }
    };
    // img2bez outlines of words with no pulls, by word: tracing is slow, retyping is not
    let mut traces: HashMap<String, BezPath> = HashMap::new();
    while let Ok(mut request) = receiver.recv() {
        while let Ok(newer) = receiver.try_recv() {
            request = newer;
        }
        let render = draw(&font, request, &mut traces);
        *latest.lock().unwrap_or_else(|e| e.into_inner()) = Some(Ok(Arc::new(render)));
    }
}

/// A word's outline traced with img2bez's type-quality fitter, as the web demo traces its
/// finished words: the field itself, sampled finely, fit tightly with smooth joins. In the
/// word's field pixels, y down. None when the tracer fails; the caller draws the fast trace.
fn trace_smooth(field: &field_text::WordField) -> Option<BezPath> {
    if field.w == 0 || field.h == 0 {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a word's field is a few hundred pixels tall"
    )]
    let supersample = ((640.0 / field.h as f64).ceil() as usize).clamp(3, 8);
    let mut opts = img2bez::TraceOptions::for_profile(img2bez::Profile::Clean);
    opts.rtl_start = true;
    opts.faithful = true;
    opts.fit_accuracy = 0.8;
    opts.smoothing = 1.5;
    opts.mode = img2bez::TraceMode::SmoothG2;
    let outline = img2bez::trace_sdf(field.w, field.h, &field.grid, supersample, &opts).ok()?;
    let path = BezPath::from_svg(&outline.to_svg_path()).ok()?;
    // y up and em_height units back to the field's pixels, y down
    #[expect(clippy::cast_precision_loss, reason = "a word's field is small")]
    let k = field.h as f64 / opts.em_height;
    Some(Affine::new([k, 0.0, 0.0, -k, 0.0, field.h as f64]) * path)
}

/// Draw one request: the line laid out by the engine's shared layout (the same code the web
/// demo runs), each word traced, all in font units, y up.
fn draw(
    font: &FieldFont,
    request: ModelRequest,
    traces: &mut HashMap<String, BezPath>,
) -> ModelRender {
    let offsets: HashMap<usize, (f64, f64)> = request.offsets.iter().copied().collect();
    let line = field_line::build_field_line(font, &request.text, &offsets);
    // The chain's own frame: the first word's right edge at x = 0, the baseline at y = 0.
    let marks = field_line::marks(font, &line, 0.0, 0.0);
    let chars: Vec<char> = request.text.chars().collect();
    let gaps = (0..marks.nodes.len())
        .map(|i| field_line::is_gap(&chars, i))
        .collect();
    // The strand as the pen moved, along the ink; each point keeps its distance along it.
    let (points, at_node) = field_line::strand(&line, &marks.nodes, 0.0, 0.0);
    let mut along = 0.0;
    let mut samples = Vec::with_capacity(points.len());
    for (k, point) in points.iter().enumerate() {
        if k > 0 {
            let prev = points[k - 1];
            along += (point.0 - prev.0).hypot(point.1 - prev.1);
        }
        samples.push((along, *point));
    }
    let node_t: Vec<f64> = at_node.iter().map(|&k| samples[k].0).collect();
    // The selection cloud, when letters are selected.
    let (anchor, caret) = request.selection;
    let (from, to) = (anchor.min(caret), anchor.max(caret));
    let outline: Vec<BezPath> = field_line::selection_paths(font, &line, from, to, 0.0, 0.0)
        .iter()
        .filter_map(|path| BezPath::from_svg(&path.to_svg()).ok())
        .collect();
    let mut paths = Vec::new();
    for word in &line.words {
        let field = &word.wf;
        let text: String = chars
            .iter()
            .skip(word.char_base)
            .take(word.n_chars)
            .collect();
        let pulled =
            (word.char_base..=word.char_base + word.n_chars).any(|i| offsets.contains_key(&i));
        let smooth = if !request.quality {
            None
        } else if pulled {
            trace_smooth(field)
        } else if let Some(path) = traces.get(&text) {
            Some(path.clone())
        } else {
            let path = trace_smooth(field);
            if let Some(path) = &path {
                traces.insert(text, path.clone());
            }
            path
        };
        let path = smooth.or_else(|| {
            let traced = field_text::trace_field_smooth(&field.grid, field.w, field.h);
            // The engine's kurbo is older than the editor's: cross through SVG.
            BezPath::from_svg(&traced.to_svg()).ok()
        });
        if let Some(path) = path {
            paths.push(Affine::translate((field.x0 + word.dx, field.y0)) * path);
        }
    }
    // Field pixels, y down, to font units, y up.
    let em = font.canvas.em_px;
    let units_per_px = font.canvas.upm / em;
    let to_units = Affine::scale_non_uniform(units_per_px, -units_per_px);
    ModelRender {
        request,
        paths: paths.into_iter().map(|path| to_units * path).collect(),
        nodes: marks
            .nodes
            .into_iter()
            .map(|(x, y)| to_units * Point::new(x, y))
            .collect(),
        gaps,
        strand: samples
            .into_iter()
            .map(|(u, (x, y))| (u, to_units * Point::new(x, y)))
            .collect(),
        node_t,
        outline: outline.into_iter().map(|path| to_units * path).collect(),
        px_per_unit: em / font.canvas.upm,
    }
}

impl crate::application::workspace::Workspace {
    /// The font the model view draws with: the chosen version's, else the newest with one.
    pub(crate) fn model_font(&self) -> Option<PathBuf> {
        let versions = &self.train.versions;
        match &self.model.version {
            Some(name) => versions
                .iter()
                .find(|version| &version.name == name)
                .and_then(|version| version.font.clone()),
            None => versions
                .iter()
                .rev()
                .find_map(|version| version.font.clone()),
        }
    }

    /// Keep the worker on the right font and ask it for the current text and pulls.
    pub(crate) fn refresh_model(&mut self) {
        let Some(font) = self.model_font() else {
            self.model.job = None;
            self.model.render = None;
            self.model.error = Some("No trained version yet".into());
            return;
        };
        if self.model.job.as_ref().is_none_or(|job| job.font != font) {
            self.model.job = Some(ModelJob::start(&font));
            self.model.render = None;
            self.model.error = None;
        }
        let mut offsets: Vec<(usize, (f64, f64))> =
            self.model.offsets.iter().map(|(i, p)| (*i, *p)).collect();
        offsets.sort_by_key(|(i, _)| *i);
        let text = self.piece_preview_text();
        if text != self.model.text {
            // Pulls belong to one text. The caret opens one node in, after the first letter,
            // as in the web demo.
            self.model.text = text.clone();
            self.model.offsets.clear();
            let first = text.chars().count().min(1);
            self.model.selection = (first, first);
            offsets.clear();
        }
        let count = text.chars().count();
        let selection = (
            self.model.selection.0.min(count),
            self.model.selection.1.min(count),
        );
        let request = ModelRequest {
            text,
            offsets,
            selection,
            quality: self.model.drag.is_none(),
        };
        if let Some(job) = self.model.job.as_mut() {
            job.request(request);
        }
    }

    /// Take the worker's newest drawing.
    pub(crate) fn model_pump(&mut self) {
        let Some(result) = self.model.job.as_ref().and_then(ModelJob::take) else {
            return;
        };
        match result {
            Ok(render) => {
                self.model.render = Some(render);
                self.model.error = None;
            }
            Err(error) => {
                self.model.error = Some(error);
                self.model.job = None;
            }
        }
    }

    /// Edit the strip's text: `edit` changes the letters given the selection, smallest index
    /// first, and returns where the caret goes. Pulls belong to one text, so they go.
    fn edit_model_text(&mut self, edit: impl FnOnce(&mut Vec<char>, (usize, usize)) -> usize) {
        let mut chars: Vec<char> = self.piece_preview_text().chars().collect();
        let count = chars.len();
        let (a, b) = self.model.selection;
        let range = (a.min(b).min(count), a.max(b).min(count));
        let caret = edit(&mut chars, range).min(chars.len());
        let text: String = chars.into_iter().collect();
        self.preview_text = text.clone();
        self.model.text = text;
        self.model.offsets.clear();
        self.model.selection = (caret, caret);
    }

    /// A node in the strip was pressed, moved or released.
    pub(crate) fn model_strip_event(
        &mut self,
        event: crate::application::widgets::model_strip::ModelStripEvent,
    ) {
        use crate::application::widgets::model_strip::ModelStripEvent;
        match event {
            ModelStripEvent::DragStart(node) => {
                let start = self.model.offsets.get(&node).copied().unwrap_or((0.0, 0.0));
                self.model.drag = Some((node, start));
            }
            ModelStripEvent::Drag { node, delta } => {
                let Some((_, start)) = self.model.drag else {
                    return;
                };
                let px = self.model.render.as_ref().map_or(1.0, |r| r.px_per_unit);
                // Font units, y up, to field pixels, y down.
                self.model
                    .offsets
                    .insert(node, (start.0 + delta.x * px, start.1 - delta.y * px));
            }
            ModelStripEvent::DragEnd => self.model.drag = None,
            ModelStripEvent::Insert(typed) => self.edit_model_text(|chars, (a, b)| {
                let typed: Vec<char> = typed.chars().collect();
                let at = a + typed.len();
                chars.splice(a..b, typed);
                at
            }),
            ModelStripEvent::Erase { forward } => self.edit_model_text(|chars, (a, b)| {
                if a != b {
                    chars.drain(a..b);
                    a
                } else if forward {
                    if a < chars.len() {
                        chars.remove(a);
                    }
                    a
                } else if a > 0 {
                    chars.remove(a - 1);
                    a - 1
                } else {
                    a
                }
            }),
            ModelStripEvent::Step { by, extend } => {
                let count = self.model.text.chars().count();
                let caret = self.model.selection.1.saturating_add_signed(by).min(count);
                let anchor = if extend {
                    self.model.selection.0
                } else {
                    caret
                };
                self.model.selection = (anchor, caret);
            }
            ModelStripEvent::Home { extend } => {
                let anchor = if extend { self.model.selection.0 } else { 0 };
                self.model.selection = (anchor, 0);
            }
            ModelStripEvent::End { extend } => {
                let count = self.model.text.chars().count();
                let anchor = if extend {
                    self.model.selection.0
                } else {
                    count
                };
                self.model.selection = (anchor, count);
            }
            ModelStripEvent::Caret { index, extend } => {
                let anchor = if extend {
                    self.model.selection.0
                } else {
                    index
                };
                self.model.selection = (anchor, index);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape as _;

    /// A trained font beside the repository, when one is there to test with.
    fn sample_font() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        let path = Path::new(&home).join("GH/repos/post-opentype/models/ba-basic/004/font.ntf");
        path.exists().then_some(path)
    }

    #[test]
    fn a_word_draws_with_one_node_per_caret() {
        let Some(path) = sample_font() else {
            return;
        };
        let font = FieldFont::load(&std::fs::read(path).unwrap()).unwrap();
        let render = draw(
            &font,
            ModelRequest {
                text: "با ب".into(),
                offsets: Vec::new(),
                selection: (2, 2),
                quality: true,
            },
            &mut HashMap::new(),
        );
        assert_eq!(render.paths.len(), 2);
        assert_eq!(render.nodes.len(), 5);
        // Right to left: the second word sits left of the first.
        let first = render.paths[0].bounding_box();
        let second = render.paths[1].bounding_box();
        assert!(second.x1 < first.x0, "{second:?} {first:?}");
        assert!(first.x1 <= 1.0 && first.x1 > -200.0, "{first:?}");
        // A pull moves the pulled letter.
        let pulled = draw(
            &font,
            ModelRequest {
                text: "با".into(),
                offsets: vec![(1, (-30.0, 0.0))],
                selection: (0, 0),
                quality: false,
            },
            &mut HashMap::new(),
        );
        assert_ne!(pulled.paths, render.paths);
        // The strand passes through every node; the gaps are the ends and the space's sides.
        assert_eq!(render.gaps, vec![true, false, true, true, true]);
        assert_eq!(render.node_t.len(), render.nodes.len());
        // A plain caret has no cloud; every node has its place on the strand.
        assert!(render.outline.is_empty());
        assert!(render.node_t.windows(2).all(|t| t[1] >= t[0]));
    }

    #[test]
    fn typing_in_the_strip_edits_its_text_like_the_web_demo() {
        use crate::application::widgets::model_strip::ModelStripEvent as E;
        let path = Path::new("assets/font-sources/neural-fonts/NastaliqDemo.nufo");
        let mut app = crate::application::workspace::Workspace::open(path).unwrap();
        app.preview_text = "بب".into();
        app.model.text = "بب".into();
        app.model.selection = (1, 1);
        app.model.offsets.insert(1, (3.0, 0.0));
        // Space between the two letters breaks the join, and the pulls go with the old text.
        app.model_strip_event(E::Insert(" ".into()));
        assert_eq!(app.preview_text, "ب ب");
        assert_eq!(app.model.selection, (2, 2));
        assert!(app.model.offsets.is_empty());
        app.model_strip_event(E::Erase { forward: false });
        assert_eq!(app.preview_text, "بب");
        assert_eq!(app.model.selection, (1, 1));
        // Left goes on through the text; Shift extends; typing replaces the selection.
        app.model_strip_event(E::Step {
            by: 1,
            extend: false,
        });
        assert_eq!(app.model.selection, (2, 2));
        app.model_strip_event(E::Home { extend: true });
        assert_eq!(app.model.selection, (2, 0));
        app.model_strip_event(E::Insert("ا".into()));
        assert_eq!(app.preview_text, "ا");
        app.model_strip_event(E::End { extend: false });
        assert_eq!(app.model.selection, (1, 1));
    }
}
