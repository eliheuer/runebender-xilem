// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The text tool: type glyphs into a line, and edit them in context.
//!
//! Spacing and kerning are judged in words, not on one glyph at a time,
//! so a font editor needs a place to type. The engine is
//! `runebender_core::text::buffer`, the same one the web and GPUI builds use:
//! it owns the buffer, the shaping, the bidi runs, the kerning, the
//! caret, and the hit testing. What lives here is the part that is
//! specific to this editor: keeping the buffer fed with the current
//! master's metrics, drawing the laid-out sorts, and turning a click
//! into either a caret position or a glyph to edit.

use std::sync::Arc;

use masonry::kurbo::{Affine, BezPath, Point};
use runebender_core::text::buffer::{
    TextBuffer, TextDirection, TextGlyphInventory, TextKerningModel,
};

use crate::model::FontModel;

/// What the view can carry.
///
/// `TextBuffer` holds `Rc` and `RefCell` (a shaping-font cache and a run
/// cache), so it is neither `Send` nor `Sync`, and a Xilem view has to be
/// both. So the view passes this, which is plain data, and the widget
/// builds the buffer on the other side. The buffer then lives where the
/// editing happens, which is where it wanted to live anyway.
#[derive(Clone, PartialEq)]
pub(crate) struct TextInputs {
    inventory: TextGlyphInventory,
    kerning: TextKerningModel,
    outlines: Arc<Vec<(String, Arc<BezPath>)>>,
    line_height: f64,
    ascender: f64,
    descender: f64,
    /// Text to start with. Only used when the buffer is created, so it
    /// is a starting state and not a binding.
    initial: String,
    /// Initial logical range for deterministic visual evidence.
    initial_selection: Option<(usize, usize)>,
    /// Writing direction, or `None` for automatic. This one *is* a
    /// binding: the direction chips live in the title bar, which is
    /// view-land, so the setting has to travel in with the inputs.
    direction: Option<TextDirection>,
}

impl TextInputs {
    /// Read a master: glyph advances, kerning, outlines, metrics.
    pub(crate) fn new(font: &FontModel) -> Self {
        Self {
            inventory: TextGlyphInventory::from_font(font.font()),
            kerning: TextKerningModel::from_font(font.font()),
            outlines: Arc::new(
                font.glyphs
                    .iter()
                    .map(|glyph| (glyph.name.clone(), glyph.outline.clone()))
                    .collect(),
            ),
            line_height: (font.units_per_em().max(font.ascender()) - font.descender()).max(1.0),
            ascender: font.ascender(),
            descender: font.descender(),
            initial: String::new(),
            initial_selection: None,
            direction: None,
        }
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
        for name in ["A", "B"] {
            let mut glyph = norad::Glyph::new(name);
            glyph.width = 500.0;
            glyph.codepoints.insert(name.chars().next().unwrap());
            font.default_layer_mut().insert_glyph(glyph);
        }
        Self::new(&TextInputs {
            inventory: TextGlyphInventory::from_font(&font),
            kerning: TextKerningModel::from_font(&font),
            outlines: Arc::new(Vec::new()),
            line_height: 1000.0,
            ascender: 800.0,
            descender: -200.0,
            initial: "A".into(),
            initial_selection: None,
            direction: None,
        })
    }

    /// A buffer wired to a master.
    pub(crate) fn new(inputs: &TextInputs) -> Self {
        let mut buffer = TextBuffer::new();
        buffer.set_glyph_inventory(inputs.inventory.clone());
        buffer.set_kerning_model(inputs.kerning.clone());
        match inputs.direction {
            Some(direction) => buffer.set_direction(direction),
            None => buffer.set_auto_direction(),
        }
        for character in inputs.initial.chars() {
            buffer.insert_character(character);
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
        self.buffer.set_kerning_model(inputs.kerning.clone());
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
        for character in committed.chars() {
            changed |= self.insert(character);
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

    /// A click: put the caret there, and report the sort under it.
    pub(crate) fn click(&mut self, at: Point) -> Option<usize> {
        let hit = self
            .buffer
            .hit_test(at.x, at.y, self.line_height, self.ascender, self.descender);
        self.buffer
            .place_cursor_at(at.x, at.y, self.line_height, self.ascender, self.descender);
        hit.active_sort
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
    #[ignore = "loads the adjacent full Virtua Grotesk designspace"]
    fn virtua_mixed_text_uses_real_arabic_forms_marks_and_bidi_layout() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../virtua-grotesk/sources/VirtuaGrotesk.designspace");
        assert!(
            source.is_file(),
            "clone Virtua Grotesk beside this repository"
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
        font.font_mut()
            .get_glyph_mut(&beh_name)
            .expect("the shaped beh remains in the live master")
            .width += 17.0;
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
    }
}
