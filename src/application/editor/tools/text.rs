// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The text tool: type glyphs into a line, and edit them in context.
//!
//! Spacing and kerning are judged in words, not on one glyph at a time,
//! so a font editor needs a place to type. The engine is
//! `runebender::text::buffer`, shared with other Runebender interfaces:
//! it owns the buffer, the shaping, the bidi runs, the kerning, the
//! caret, and the hit testing. What lives here is the part that is
//! specific to this editor: keeping the buffer fed with the current
//! master's metrics, drawing the laid-out sorts, and turning a click
//! into either a caret position or a glyph to edit.

use std::sync::Arc;

use masonry::kurbo::{Affine, BezPath, Point};
use runebender::text::buffer::{TextBuffer, TextDirection, TextGlyphInventory, TextKerningModel};

use crate::application::font_model::FontModel;

/// What the view can carry.
///
/// `TextBuffer` holds `Rc` and `RefCell` (a shaping-font cache and a run
/// cache), so it is neither `Send` nor `Sync`, and a Xilem view has to be
/// both. So the view passes this, which is plain data, and the widget
/// builds the buffer on the other side. The buffer then lives where the
/// editing happens, which is where it wanted to live anyway.
#[derive(Clone, PartialEq)]
pub(crate) struct TextInputs {
    /// Document and tab identity. A change means the widget must restore a
    /// different parked buffer instead of refreshing the current one.
    context_id: (u64, u64),
    inventory: TextGlyphInventory,
    kerning: TextKerningModel,
    outlines: Arc<Vec<(String, Arc<BezPath>)>>,
    compiled: Option<Arc<Vec<u8>>>,
    normalized: Vec<f64>,
    line_height: f64,
    ascender: f64,
    descender: f64,
    /// Text to start with. Only used when the buffer is created, so it
    /// is a starting state and not a binding.
    initial: String,
    /// The glyph whose editor tab owns this text line. It remains the active
    /// sort when a text buffer is first opened.
    active_glyph: Option<(String, Option<char>, f64)>,
    /// Initial logical range for deterministic visual evidence.
    initial_selection: Option<(usize, usize)>,
    /// Writing direction, or `None` for automatic. This one *is* a
    /// binding: the direction chips live in the title bar, which is
    /// view-land, so the setting has to travel in with the inputs.
    direction: Option<TextDirection>,
    feature_overrides: Vec<(String, bool)>,
    script: Option<String>,
    language: Option<String>,
}

impl TextInputs {
    /// Read a master: glyph advances, kerning, outlines, metrics.
    pub(crate) fn new(font: &FontModel) -> Self {
        let source = font
            .project
            .document_sources()
            .nth(font.project.active)
            .expect("the active source exists")
            .id();
        Self {
            context_id: (0, 0),
            inventory: TextGlyphInventory::from_project(&font.project, source)
                .expect("the active source has canonical text inputs"),
            kerning: TextKerningModel::from_project(&font.project, source)
                .expect("the active source has canonical kerning inputs"),
            outlines: Arc::new(
                font.glyphs
                    .iter()
                    .map(|glyph| (glyph.name.clone(), glyph.outline.clone()))
                    .collect(),
            ),
            compiled: None,
            normalized: Vec::new(),
            line_height: (font.units_per_em().max(font.ascender()) - font.descender()).max(1.0),
            ascender: font.ascender(),
            descender: font.descender(),
            initial: String::new(),
            active_glyph: None,
            initial_selection: None,
            direction: None,
            feature_overrides: Vec::new(),
            script: None,
            language: None,
        }
    }

    /// Read a master and remember the glyph whose editor tab is open.
    pub(crate) fn for_glyph(font: &FontModel, glyph_name: &str) -> Self {
        let mut inputs = Self::new(font);
        inputs.active_glyph = font
            .glyphs
            .iter()
            .find(|glyph| glyph.name == glyph_name)
            .map(|glyph| (glyph.name.clone(), glyph.codepoint, glyph.advance));
        inputs
    }

    /// Preview the live variable font at the sliders' user coordinates.
    /// Advances, kerning, substitutions and outlines come from the same binary.
    pub(crate) fn with_location(mut self, font: &FontModel, values: &[f64]) -> Self {
        if font.glyphs.is_empty() {
            return self;
        }
        self.normalized = font
            .axes
            .iter()
            .enumerate()
            .map(|(index, axis)| {
                axis.user_to_normalized(values.get(index).copied().unwrap_or(axis.default))
            })
            .collect();
        if let Ok(Some(compiled)) = font.preview_font() {
            self.normalized = compiled
                .axis_tags
                .iter()
                .map(|tag| {
                    font.axes
                        .iter()
                        .position(|axis| axis.tag == *tag)
                        .map(|index| self.normalized[index])
                        .unwrap_or(0.0)
                })
                .collect();
            match compiled.outlines(&self.normalized) {
                Ok(outlines) => self.outlines = Arc::new(outlines),
                Err(error) => {
                    runebender::text::shape::log_shaping_failure(&error);
                    return self;
                }
            }
            self.compiled = Some(compiled.bytes.clone());
            if let Ok(advances) = compiled.advances(&self.normalized) {
                for (name, advance) in advances {
                    if let Some((active, _, width)) = &mut self.active_glyph
                        && *active == name
                    {
                        *width = advance;
                    }
                    self.inventory.set_advance(name, advance);
                }
            }
        }
        self
    }

    /// Substitute the editor's live draft without recompiling the shaping font.
    pub(crate) fn with_live_outline(mut self, name: &str, outline: Arc<BezPath>) -> Self {
        if let Some((_, path)) = Arc::make_mut(&mut self.outlines)
            .iter_mut()
            .find(|(glyph, _)| glyph == name)
        {
            *path = outline;
        }
        self
    }

    /// Associate these inputs with one document tab's parked text buffer.
    pub(crate) fn with_context(mut self, context_id: (u64, u64)) -> Self {
        self.context_id = context_id;
        self
    }

    pub(crate) fn same_context(&self, other: &Self) -> bool {
        self.context_id == other.context_id
    }

    /// Set the writing direction, or clear it back to automatic.
    pub(crate) fn with_direction(mut self, direction: Option<TextDirection>) -> Self {
        self.direction = direction;
        self
    }

    /// Start the buffer with some text. `RUNEBENDER_TEXT` uses this, so
    /// a headless render can show a shaped line without typing.
    pub(crate) fn with_text(mut self, text: &str) -> Self {
        self.initial = text.to_string();
        self
    }

    /// Start with a logical text range selected.
    pub(crate) fn with_selection(mut self, selection: Option<(usize, usize)>) -> Self {
        self.initial_selection = selection;
        self
    }

    /// Apply preview-only OpenType feature and locale choices.
    pub(crate) fn with_shaping_options(
        mut self,
        disabled: &std::collections::HashSet<String>,
        script: Option<&str>,
        language: Option<&str>,
    ) -> Self {
        self.feature_overrides = disabled.iter().map(|tag| (tag.clone(), false)).collect();
        self.feature_overrides.sort();
        self.script = script.map(str::to_string);
        self.language = language.map(str::to_string);
        self
    }
}

/// The buffer plus everything the editor needs to draw it.
pub(crate) struct TextState {
    pub buffer: TextBuffer,
    /// Native IME composition shown in the line but not committed to the
    /// document text until the platform sends `Ime::Commit`.
    pub preedit: String,
    /// Line height in design units, from the master's metrics.
    pub line_height: f64,
    /// The master's ascender and descender, which the engine needs to
    /// work out which line a click landed on.
    ascender: f64,
    descender: f64,
    /// Outlines by glyph name, so painting does not touch the font.
    outlines: Arc<Vec<(String, Arc<BezPath>)>>,
}

impl TextState {
    #[cfg(test)]
    pub(crate) fn test_buffer() -> Self {
        let mut font = norad::Font::new();
        for (name, codepoint, width) in
            [("A", 'A', 500.0), ("B", 'B', 500.0), ("space", ' ', 250.0)]
        {
            let mut glyph = norad::Glyph::new(name);
            glyph.width = width;
            glyph.codepoints.insert(codepoint);
            font.default_layer_mut().insert_glyph(glyph);
        }
        let project = runebender::document::project::Project::from_source(
            runebender::document::project::SourceInput::from_font(font, "text-test.ufo".into()),
        );
        let source = project.source_id(0).unwrap();
        Self::new(&TextInputs {
            context_id: (0, 0),
            inventory: TextGlyphInventory::from_project(&project, source).unwrap(),
            kerning: TextKerningModel::from_project(&project, source).unwrap(),
            outlines: Arc::new(Vec::new()),
            compiled: None,
            normalized: Vec::new(),
            line_height: 1000.0,
            ascender: 800.0,
            descender: -200.0,
            initial: "A".into(),
            active_glyph: Some(("A".into(), Some('A'), 500.0)),
            initial_selection: None,
            direction: None,
            feature_overrides: Vec::new(),
            script: None,
            language: None,
        })
    }

    /// A buffer wired to a master.
    pub(crate) fn new(inputs: &TextInputs) -> Self {
        let mut buffer = TextBuffer::new();
        buffer.set_glyph_inventory(inputs.inventory.clone());
        buffer.set_compiled_font(inputs.compiled.clone(), inputs.normalized.clone());
        buffer.set_kerning_model(inputs.kerning.clone());
        buffer.set_feature_overrides(inputs.feature_overrides.clone());
        buffer.set_shaping_locale(inputs.script.clone(), inputs.language.clone());
        match inputs.direction {
            Some(direction) => buffer.set_direction(direction),
            None => buffer.set_auto_direction(),
        }
        for character in inputs.initial.chars() {
            buffer.insert_character(character);
        }
        if let Some((name, codepoint, advance)) = &inputs.active_glyph {
            let existing = {
                buffer
                    .iter()
                    .position(|sort| sort.glyph_name() == Some(name.as_str()))
            };
            match existing {
                Some(index) => {
                    buffer.activate_sort(index);
                }
                None => {
                    // A glyph need not have Unicode. Seed it explicitly, as
                    // GPUI does, so taking the Text tool never blanks the glyph
                    // that was already open. If a different line had been
                    // parked here, it cannot provide an active edit target.
                    buffer.clear();
                    buffer.insert_glyph(name.clone(), *codepoint, *advance);
                }
            }
        }
        buffer.shape_arabic_if_rtl();
        if let Some((start, end)) = inputs.initial_selection {
            buffer.select_range(start, end);
        }
        Self {
            buffer,
            preedit: String::new(),
            line_height: inputs.line_height,
            ascender: inputs.ascender,
            descender: inputs.descender,
            outlines: inputs.outlines.clone(),
        }
    }

    /// Re-read the master, keeping what has been typed. Switching master
    /// or editing a glyph changes advances and outlines, and a text line
    /// that does not follow is showing yesterday's spacing.
    pub(crate) fn refresh(&mut self, inputs: &TextInputs) {
        self.buffer.set_glyph_inventory(inputs.inventory.clone());
        self.buffer
            .set_compiled_font(inputs.compiled.clone(), inputs.normalized.clone());
        self.buffer.set_kerning_model(inputs.kerning.clone());
        self.buffer
            .set_feature_overrides(inputs.feature_overrides.clone());
        self.buffer
            .set_shaping_locale(inputs.script.clone(), inputs.language.clone());
        let direction_changed = match inputs.direction {
            Some(direction) => {
                let changed =
                    self.buffer.direction_is_auto() || self.buffer.direction() != direction;
                self.buffer.set_direction(direction);
                changed
            }
            None => {
                let changed = !self.buffer.direction_is_auto();
                self.buffer.set_auto_direction();
                changed
            }
        };
        if direction_changed {
            self.buffer.shape_arabic_if_rtl();
        }
        self.buffer.refresh_shaping();
        self.outlines = inputs.outlines.clone();
        self.line_height = inputs.line_height;
        self.ascender = inputs.ascender;
        self.descender = inputs.descender;
    }

    fn outline(&self, name: &str) -> Option<&Arc<BezPath>> {
        self.outlines
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, path)| path)
    }

    /// Type a character. Returns false when the font has no glyph for it,
    /// which is worth knowing rather than silently swallowing.
    pub(crate) fn insert(&mut self, character: char) -> bool {
        let inserted = self.buffer.insert_character(character);
        if inserted {
            self.buffer.shape_arabic_if_rtl();
        }
        inserted
    }

    /// Replace the native IME composition without changing the committed
    /// buffer. An empty preedit is cancellation.
    pub(crate) fn set_preedit(&mut self, text: String) {
        self.preedit = text;
    }

    /// Commit one native IME result exactly once and clear its preview.
    pub(crate) fn commit_preedit(&mut self, committed: &str) -> bool {
        self.preedit.clear();
        let mut changed = false;
        let normalized = committed.replace("\r\n", "\n").replace('\r', "\n");
        for character in normalized.chars() {
            if character == '\n' {
                self.buffer.insert_line_break();
                changed = true;
            } else {
                changed |= self.insert(character);
            }
        }
        changed
    }

    /// Every sort to draw, as a path already placed on the line.
    ///
    /// Absorbed sorts (a character folded into a ligature drawn by an
    /// earlier sort) contribute nothing, which is what `is_absorbed` is
    /// for.
    pub(crate) fn placed(&self) -> Vec<PlacedSort> {
        let display;
        let buffer = if self.preedit.is_empty() {
            &self.buffer
        } else {
            display = {
                let mut buffer = self.buffer.clone();
                for character in self.preedit.chars() {
                    buffer.insert_character(character);
                }
                buffer.shape_arabic_if_rtl();
                buffer
            };
            &display
        };
        let layout = buffer.layout(self.line_height);
        let active = buffer.active_sort();
        let selection = buffer.selection_range();
        layout
            .items
            .iter()
            .filter_map(|item| {
                let sort = buffer.sort(item.index)?;
                if sort.is_absorbed() {
                    return None;
                }
                let name = sort.glyph_name()?;
                let outline = self.outline(name)?;
                let placed = Affine::translate((item.x, item.y)) * (**outline).clone();
                let cluster_end = ((item.index + 1)..buffer.len())
                    .find(|index| !buffer.sort(*index).is_some_and(|sort| sort.is_absorbed()))
                    .unwrap_or(buffer.len());
                let selected = selection
                    .as_ref()
                    .is_some_and(|range| range.start < cluster_end && range.end > item.index);
                Some(PlacedSort {
                    path: placed,
                    origin: Point::new(item.x, item.y),
                    advance: item.advance_width,
                    active: active == Some(item.index),
                    selected,
                })
            })
            .collect()
    }

    /// Where the caret sits, in design space.
    pub(crate) fn caret(&self) -> Point {
        let display;
        let buffer = if self.preedit.is_empty() {
            &self.buffer
        } else {
            display = {
                let mut buffer = self.buffer.clone();
                for character in self.preedit.chars() {
                    buffer.insert_character(character);
                }
                buffer.shape_arabic_if_rtl();
                buffer
            };
            &display
        };
        let layout = buffer.layout(self.line_height);
        Point::new(layout.cursor_x, layout.cursor_y)
    }

    /// Layout origin of the sort whose glyph is open in the outline editor.
    pub(crate) fn active_origin(&self) -> Option<Point> {
        let active = self.buffer.active_sort()?;
        self.buffer
            .layout(self.line_height)
            .items
            .iter()
            .find(|item| item.index == active)
            .map(|item| Point::new(item.x, item.y))
    }

    /// A click: put the caret there, and report the sort under it.
    pub(crate) fn click(&mut self, at: Point) -> Option<usize> {
        let hit = self
            .buffer
            .hit_test(at.x, at.y, self.line_height, self.ascender, self.descender);
        self.buffer
            .place_cursor_at(at.x, at.y, self.line_height, self.ascender, self.descender);
        hit.active_sort
    }

    /// Activate the sort box under `at` without moving the text caret.
    pub(crate) fn activate_at(&mut self, at: Point) -> Option<String> {
        let activation = self.buffer.activate_sort_at(
            at.x,
            at.y,
            self.line_height,
            self.ascender,
            self.descender,
        )?;
        self.buffer
            .sort(activation.index)
            .and_then(|sort| sort.glyph_name())
            .map(str::to_string)
    }

    /// Make a sort the one being edited, and report its glyph.
    pub(crate) fn activate(&mut self, index: usize) -> Option<String> {
        self.buffer.activate_sort(index).then(|| {
            self.buffer
                .sort(index)
                .and_then(|sort| sort.glyph_name())
                .map(str::to_string)
        })?
    }
}

/// One sort, ready to draw.
pub(crate) struct PlacedSort {
    pub path: BezPath,
    pub origin: Point,
    pub advance: f64,
    pub active: bool,
    pub selected: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::kurbo::Shape;

    #[test]
    fn shaping_options_reach_the_widget_buffer() {
        let disabled = std::collections::HashSet::from(["liga".to_string(), "kern".to_string()]);
        let inputs = TextInputs {
            context_id: (0, 0),
            inventory: TextGlyphInventory::default(),
            kerning: TextKerningModel::default(),
            outlines: Arc::new(Vec::new()),
            compiled: None,
            normalized: Vec::new(),
            line_height: 1000.0,
            ascender: 800.0,
            descender: -200.0,
            initial: String::new(),
            active_glyph: None,
            initial_selection: None,
            direction: None,
            feature_overrides: Vec::new(),
            script: None,
            language: None,
        }
        .with_shaping_options(&disabled, Some("arab"), Some("ur"));

        let state = TextState::new(&inputs);

        assert_eq!(
            state.buffer.feature_overrides(),
            &[("kern".into(), false), ("liga".into(), false)]
        );
        assert_eq!(state.buffer.shaping_locale(), (Some("arab"), Some("ur")));
    }

    #[test]
    fn empty_text_starts_with_the_open_glyph_active() {
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("five");
        glyph.width = 612.0;
        glyph.codepoints.insert('5');
        font.default_layer_mut().insert_glyph(glyph);
        let mut other = norad::Glyph::new("A");
        other.width = 500.0;
        other.codepoints.insert('A');
        font.default_layer_mut().insert_glyph(other);
        let project = runebender::document::project::Project::from_source(
            runebender::document::project::SourceInput::from_font(font, "text-test.ufo".into()),
        );
        let source = project.source_id(0).unwrap();
        let mut inputs = TextInputs {
            context_id: (0, 0),
            inventory: TextGlyphInventory::from_project(&project, source).unwrap(),
            kerning: TextKerningModel::default(),
            outlines: Arc::new(Vec::new()),
            compiled: None,
            normalized: Vec::new(),
            line_height: 1000.0,
            ascender: 800.0,
            descender: -200.0,
            initial: String::new(),
            active_glyph: Some(("five".into(), Some('5'), 612.0)),
            initial_selection: None,
            direction: None,
            feature_overrides: Vec::new(),
            script: None,
            language: None,
        };
        let state = TextState::new(&inputs);

        assert_eq!(state.buffer.len(), 1);
        assert_eq!(state.buffer.active_sort(), Some(0));
        assert_eq!(state.buffer.sort(0).unwrap().glyph_name(), Some("five"));

        inputs.initial = "5".into();
        let state = TextState::new(&inputs);
        assert_eq!(state.buffer.len(), 1, "the open glyph is not duplicated");
        assert_eq!(state.buffer.active_sort(), Some(0));

        inputs.initial = "A".into();
        let state = TextState::new(&inputs);
        assert_eq!(
            state.buffer.sort(0).unwrap().glyph_name(),
            Some("five"),
            "a parked line without the open glyph resets to its edit target"
        );
        assert_eq!(state.buffer.active_sort(), Some(0));
    }

    #[test]
    fn non_unicode_open_glyph_is_still_seeded() {
        let inputs = TextInputs {
            context_id: (0, 0),
            inventory: TextGlyphInventory::default(),
            kerning: TextKerningModel::default(),
            outlines: Arc::new(Vec::new()),
            compiled: None,
            normalized: Vec::new(),
            line_height: 1000.0,
            ascender: 800.0,
            descender: -200.0,
            initial: String::new(),
            active_glyph: Some(("alternate.001".into(), None, 500.0)),
            initial_selection: None,
            direction: None,
            feature_overrides: Vec::new(),
            script: None,
            language: None,
        };
        let state = TextState::new(&inputs);

        assert_eq!(state.buffer.len(), 1);
        assert_eq!(
            state.buffer.sort(0).unwrap().glyph_name(),
            Some("alternate.001")
        );
        assert_eq!(state.buffer.active_sort(), Some(0));
    }

    #[test]
    #[ignore = "loads the adjacent full Virtua Grotesk designspace"]
    fn virtua_mixed_text_uses_real_arabic_forms_marks_and_bidi_layout() {
        let source = std::env::var_os("RUNEBENDER_TEST_FONTS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../virtua-grotesk/sources")
            })
            .join("VirtuaGrotesk.designspace");
        assert!(
            source.is_file(),
            "clone Virtua Grotesk beside this repository or set RUNEBENDER_TEST_FONTS"
        );
        let mut font = FontModel::open(&source).expect("Virtua Grotesk opens");
        let sample = "R لا 123 بِ";
        let mut state = TextState::new(&TextInputs::new(&font).with_text(sample));

        assert_eq!(
            state.buffer.len(),
            sample.chars().count(),
            "the actual Regular master covers every sample character"
        );
        assert!(
            state
                .buffer
                .iter()
                .any(|sort| sort.glyph_name() == Some("lam_alef-ar")),
            "the real feature file substitutes lam-alef"
        );
        assert!(
            state
                .buffer
                .sort(10)
                .and_then(|sort| sort.glyph_name())
                .is_some_and(|name| name.contains("kasra")),
            "the kasra remains an addressable shaped sort"
        );
        let disabled = std::collections::HashSet::from(["rlig".to_string()]);
        let no_rlig = TextState::new(
            &TextInputs::new(&font)
                .with_text(sample)
                .with_shaping_options(&disabled, Some("arab"), Some("ur")),
        );
        assert!(
            no_rlig
                .buffer
                .iter()
                .all(|sort| sort.glyph_name() != Some("lam_alef-ar")),
            "the real required-ligature override changes existing shaping"
        );
        assert!(
            !no_rlig.buffer.sort(3).unwrap().is_absorbed(),
            "disabled rlig leaves the alef independently editable"
        );
        let placed = state.placed();
        assert!(
            placed.len() < state.buffer.len(),
            "the absorbed alef remains logical text but is not painted twice"
        );
        assert!(placed.iter().all(|sort| {
            let bounds = sort.path.bounding_box();
            bounds.x0.is_finite()
                && bounds.y0.is_finite()
                && bounds.x1.is_finite()
                && bounds.y1.is_finite()
        }));

        let layout = state.buffer.layout(state.line_height);
        let latin = layout
            .items
            .iter()
            .find(|item| item.index == 0)
            .expect("the Latin run is laid out");
        let arabic = layout
            .items
            .iter()
            .filter(|item| [2, 3, 9, 10].contains(&item.index))
            .collect::<Vec<_>>();
        assert!(!arabic.is_empty());
        assert!(
            arabic.iter().any(|item| item.x > latin.x),
            "the Arabic run occupies its bidi-resolved visual position"
        );

        let beh_name = state
            .buffer
            .sort(9)
            .and_then(|sort| sort.glyph_name())
            .expect("the shaped beh has a glyph name")
            .to_string();
        let before_advance = state
            .buffer
            .layout(state.line_height)
            .items
            .iter()
            .find(|item| item.index == 9)
            .expect("the beh remains visible")
            .advance_width;
        let beh_index = font.index_of(&beh_name).expect("the shaped beh is indexed");
        let address = font
            .active_layer_address(&beh_name)
            .expect("the shaped beh remains in the canonical source");
        let mut transaction = font
            .project
            .begin_document_layer_transaction(&address)
            .expect("the shaped beh remains editable");
        let width = transaction.draft().view().width() + 17.0;
        transaction
            .draft_mut()
            .set_width(width)
            .expect("the finite width is valid");
        font.project
            .commit_document_layer_transaction(transaction)
            .expect("the test edit commits");
        font.refresh_entry(beh_index);
        state.refresh(&TextInputs::new(&font));
        assert_eq!(
            state
                .buffer
                .layout(state.line_height)
                .items
                .iter()
                .find(|item| item.index == 9)
                .expect("the refreshed beh remains visible")
                .advance_width,
            before_advance + 17.0,
            "existing Arabic text reshapes immediately from a live glyph edit"
        );

        let lam = layout
            .items
            .iter()
            .find(|item| item.index == 2)
            .expect("the lam-alef ligature has one visible item");
        assert_eq!(
            state.click(Point::new(lam.x + lam.advance_width / 2.0, lam.y)),
            Some(2),
            "pointer hit mapping resolves the visible ligature to its logical lam"
        );
        state.buffer.select_range(2, 4);
        assert_eq!(
            state.placed().iter().filter(|sort| sort.selected).count(),
            1,
            "lam and absorbed alef share one visible selection box"
        );

        state.buffer.set_cursor(9);
        state.buffer.extend_selection_visual_right();
        assert_eq!(state.buffer.selection_range(), Some(9..10));
        assert!(
            state.placed().iter().any(|sort| sort.selected),
            "the real Arabic selection has visible geometry"
        );
        assert!(state.buffer.delete_after_cursor().is_some());
        state.buffer.shape_arabic_if_rtl();
        assert_eq!(state.buffer.len(), 10);
        assert_eq!(state.buffer.selection_range(), None);

        let mut pasted = TextState::new(&TextInputs::new(&font));
        assert!(pasted.commit_preedit("\u{0628}\u{0650}"));
        pasted.buffer.select_range(0, pasted.buffer.len());
        assert_eq!(
            pasted.buffer.selected_text().as_deref(),
            Some("\u{0628}\u{0650}"),
            "real Virtua combining-mark input round-trips as Unicode text"
        );
        assert!(
            pasted
                .buffer
                .iter()
                .any(|sort| sort.glyph_name().is_some_and(|name| name.contains("kasra"))),
            "the pasted combining mark remains a shaped sort"
        );
    }
}
