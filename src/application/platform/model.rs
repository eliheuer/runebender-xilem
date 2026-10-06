// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A trained font drawn in the proof strip, through the `NeuralType` engine.
//!
//! The engine's font holds caches that are not thread-safe, so a worker thread owns it. The
//! workspace sends it the text and the pulled nodes; the worker composes the words, traces
//! the field and hands back paths in font units, y up, with one node per caret index on the
//! chain of letters. Dragging a node pulls its letter and the rest of the word, and the join
//! before it stretches, as in the web demo.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use kurbo::{Affine, BezPath, Point};
use neuraltype_core::field_model::FieldFont;
use neuraltype_core::field_text;

/// What the worker is asked to draw.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ModelRequest {
    pub text: String,
    /// Node index to its pull, in field pixels, y down.
    pub offsets: Vec<(usize, (f64, f64))>,
}

/// The drawn text: paths and nodes in font units, y up, the first word's right edge at x = 0.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ModelRender {
    pub request: ModelRequest,
    pub paths: Vec<BezPath>,
    /// One point per caret index, 0 ..= the text's character count.
    pub nodes: Vec<Point>,
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
    while let Ok(mut request) = receiver.recv() {
        while let Ok(newer) = receiver.try_recv() {
            request = newer;
        }
        let render = draw(&font, request);
        *latest.lock().unwrap_or_else(|e| e.into_inner()) = Some(Ok(Arc::new(render)));
    }
}

/// One word placed on the line, in field pixels.
struct PlacedWord {
    field: field_text::WordField,
    /// Added to the word's own x to put it on the line.
    dx: f64,
    /// The index of the word's first character in the text.
    char_base: usize,
    n_chars: usize,
    /// The ink's edges and the heights where its ink enters and leaves, in the word's frame.
    ink_l: f64,
    ink_r: f64,
    exit_y: f64,
    entry_y: f64,
}

/// The ink's bounds in a field: left, right, top, bottom cells, or None for no ink.
fn ink_bounds(field: &field_text::WordField) -> Option<(usize, usize, usize, usize)> {
    let (mut x0, mut x1, mut y0, mut y1) = (usize::MAX, 0, usize::MAX, 0);
    for y in 0..field.h {
        for x in 0..field.w {
            if field.grid[y * field.w + x] >= 0.0 {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    (x0 != usize::MAX).then_some((x0, x1, y0, y1))
}

/// The mean row of the ink in columns `from..to`, in the word's frame.
fn ink_height(field: &field_text::WordField, from: usize, to: usize) -> f64 {
    let (mut sum, mut count) = (0.0, 0_usize);
    for y in 0..field.h {
        for x in from..to.min(field.w) {
            if field.grid[y * field.w + x] >= 0.0 {
                sum += y as f64;
                count += 1;
            }
        }
    }
    field.y0 + if count > 0 { sum / count as f64 } else { 0.0 }
}

/// Lay the words out right to left on one baseline, as the web demo does. Coordinates are
/// field pixels, y down, the pen starting at x = 0 and moving left.
fn place_words(
    font: &FieldFont,
    text: &str,
    offsets: &HashMap<usize, (f64, f64)>,
) -> Vec<PlacedWord> {
    let em = font.canvas.em_px;
    let space = 0.12 * em;
    let mut pen_right = 0.0;
    let mut words = Vec::new();
    let mut char_base = 0;
    for word in text.split(' ') {
        let n_chars = word.chars().count();
        if word.is_empty() {
            pen_right -= space;
            char_base += 1;
            continue;
        }
        let clusters = field_text::layout_word(font, word);
        let mut pulls = vec![(0.0, 0.0); clusters.len()];
        let mut pulled_x = 0.0;
        let mut has_pull = false;
        let mut ci = 0;
        for (k, cluster) in clusters.iter().enumerate() {
            ci += cluster.letters.chars().count();
            // A pull at node i belongs to the cluster that ends at i.
            if let Some(&pull) = offsets.get(&(char_base + ci)) {
                pulls[k] = pull;
                pulled_x += pull.0;
                has_pull = true;
            }
        }
        let base = field_text::compose_clusters(font, clusters.clone(), None);
        let Some((bx0, bx1, _, _)) = (base.w > 0).then(|| ink_bounds(&base)).flatten() else {
            char_base += n_chars + 1;
            continue;
        };
        // Placement anchors on the ink before any pull, so a drag moves the letter instead of
        // the pen cancelling it.
        let ink_r = base.x0 + bx1 as f64 + 1.0;
        let ink_l = base.x0 + bx0 as f64;
        let exit_y = ink_height(&base, bx0, bx0 + 4);
        let entry_y = ink_height(&base, bx1.saturating_sub(3), bx1 + 1);
        let field = if has_pull {
            field_text::compose_clusters_pulled(font, clusters, &pulls)
        } else {
            base
        };
        if field.w == 0 {
            char_base += n_chars + 1;
            continue;
        }
        let dx = pen_right - ink_r;
        pen_right -= (ink_r - ink_l) + space - pulled_x.min(0.0);
        words.push(PlacedWord {
            field,
            dx,
            char_base,
            n_chars,
            ink_l,
            ink_r,
            exit_y,
            entry_y,
        });
        char_base += n_chars + 1;
    }
    words
}

/// Where two clusters' ink meets: the deepest cell of their fields' overlap, or None when
/// they do not touch.
#[expect(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    reason = "field cells are a few hundred wide, and the overlap stays inside both canvases"
)]
fn join_point(font: &FieldFont, a: &field_text::Cluster, b: &field_text::Cluster) -> Option<Point> {
    let (cw, ch) = (font.canvas.w as i64, font.canvas.h as i64);
    let (cox, coy) = (font.canvas.origin_x, font.canvas.origin_y);
    let (ga, gb) = (font.glyph(a.feats), font.glyph(b.feats));
    let (ax0, ay0) = ((a.ox - cox).round() as i64, (a.oy - coy).round() as i64);
    let (bx0, by0) = ((b.ox - cox).round() as i64, (b.oy - coy).round() as i64);
    let mut best = f32::MIN;
    let mut at = None;
    for y in ay0.max(by0)..(ay0 + ch).min(by0 + ch) {
        for x in ax0.max(bx0)..(ax0 + cw).min(bx0 + cw) {
            let va = ga.field[((y - ay0) * cw + (x - ax0)) as usize];
            let vb = gb.field[((y - by0) * cw + (x - bx0)) as usize];
            let depth = va.min(vb);
            if depth > best {
                best = depth;
                at = Some(Point::new(x as f64 + 0.5, y as f64 + 0.5));
            }
        }
    }
    (best >= 0.0).then_some(at).flatten()
}

/// Draw one request: words traced and placed, nodes on the chain, all in font units, y up.
fn draw(font: &FieldFont, request: ModelRequest) -> ModelRender {
    let offsets: HashMap<usize, (f64, f64)> = request.offsets.iter().copied().collect();
    let words = place_words(font, &request.text, &offsets);
    let em = font.canvas.em_px;
    let space = 0.12 * em;
    let n = request.text.chars().count();
    let mut nodes: Vec<Option<Point>> = vec![None; n + 1];
    let mut paths = Vec::new();
    for word in &words {
        let field = &word.field;
        let traced = field_text::trace_field_smooth(&field.grid, field.w, field.h);
        // The engine's kurbo is older than the editor's: cross through SVG.
        if let Ok(path) = BezPath::from_svg(&traced.to_svg()) {
            paths.push(Affine::translate((field.x0 + word.dx, field.y0)) * path);
        }
        let clusters = &field.clusters;
        let left_ink = word.ink_l + word.dx;
        let mut ci = 0;
        for (k, cluster) in clusters.iter().enumerate() {
            let right = if k == 0 {
                word.ink_r + word.dx
            } else {
                clusters[k - 1].ox + word.dx
            };
            let left = if k + 1 < clusters.len() {
                clusters[k + 1].ox + word.dx
            } else {
                left_ink
            };
            let count = cluster.letters.chars().count();
            let cell = (right - left).max(1.0) / count as f64;
            for j in 0..count {
                let i = word.char_base + ci + j;
                if i > n {
                    continue;
                }
                nodes[i] = Some(if k == 0 && j == 0 {
                    // Before the word: on the first letter's entry stroke.
                    Point::new(word.ink_r + word.dx + space * 0.25, word.entry_y)
                } else if count == 1 {
                    // Between letters: where their ink meets, else the chain origin.
                    match join_point(font, &clusters[k - 1], cluster) {
                        Some(join) => join + kurbo::Vec2::new(word.dx, 0.0),
                        None => Point::new(cluster.ox + word.dx, cluster.oy),
                    }
                } else {
                    // A ligature: one node per character, spread across it.
                    Point::new(right - (j as f64 + 0.5) * cell, cluster.oy)
                });
            }
            ci += count;
        }
        let end = word.char_base + word.n_chars;
        if end <= n {
            nodes[end] = Some(Point::new(left_ink - space * 0.5, word.exit_y));
        }
    }
    // Gaps take the nearest known node, forward then backward.
    let mut last = None;
    for node in &mut nodes {
        match node {
            Some(point) => last = Some(*point),
            None => *node = last,
        }
    }
    let mut next = None;
    for node in nodes.iter_mut().rev() {
        match node {
            Some(point) => next = Some(*point),
            None => *node = next,
        }
    }
    // Field pixels, y down, to font units, y up.
    let units_per_px = font.canvas.upm / em;
    let to_units = Affine::scale_non_uniform(units_per_px, -units_per_px);
    ModelRender {
        request,
        paths: paths.into_iter().map(|path| to_units * path).collect(),
        nodes: nodes
            .into_iter()
            .map(|node| to_units * node.unwrap_or(Point::ZERO))
            .collect(),
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
        let request = ModelRequest {
            text: self.piece_preview_text(),
            offsets,
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
            },
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
            },
        );
        assert_ne!(pulled.paths, render.paths);
    }
}
