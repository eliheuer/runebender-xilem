// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The text tool: type glyphs into a line, and edit them in context.
//!
//! Spacing and kerning are judged in words, not on one glyph at a time,
//! so a font editor needs a place to type. The engine is
//! `runebender_core::text`, the same one the web and GPUI builds use:
//! it owns the buffer, the shaping, the bidi runs, the kerning, the
//! caret, and the hit testing. What lives here is the part that is
//! specific to this editor: keeping the buffer fed with the current
//! master's metrics, drawing the laid-out sorts, and turning a click
//! into either a caret position or a glyph to edit.

use std::sync::Arc;

use masonry::kurbo::{Affine, BezPath, Point};
use runebender_core::text::{TextBuffer, TextGlyphInventory, TextKerningModel};

use crate::model::FontModel;

/// What the view can carry.
///
/// `TextBuffer` holds `Rc` and `RefCell` (a shaping-font cache and a run
/// cache), so it is neither `Send` nor `Sync`, and a Xilem view has to be
/// both. So the view passes this, which is plain data, and the widget
/// builds the buffer on the other side. The buffer then lives where the
/// editing happens, which is where it wanted to live anyway.
#[derive(Clone, PartialEq)]
pub struct TextInputs {
    inventory: TextGlyphInventory,
    kerning: TextKerningModel,
    outlines: Arc<Vec<(String, Arc<BezPath>)>>,
    line_height: f64,
    ascender: f64,
    descender: f64,
    /// Text to start with. Only used when the buffer is created, so it
    /// is a starting state and not a binding.
    initial: String,
}

impl TextInputs {
    /// Read a master: glyph advances, kerning, outlines, metrics.
    pub fn new(font: &FontModel) -> Self {
        Self {
            inventory: TextGlyphInventory::from_font(&font.font),
            kerning: TextKerningModel::from_font(&font.font),
            outlines: Arc::new(
                font.glyphs
                    .iter()
                    .map(|glyph| (glyph.name.clone(), glyph.outline.clone()))
                    .collect(),
            ),
            line_height: (font.units_per_em.max(font.ascender) - font.descender).max(1.0),
            ascender: font.ascender,
            descender: font.descender,
            initial: String::new(),
        }
    }

    /// Start the buffer with some text. `RUNEBENDER_TEXT` uses this, so
    /// a headless render can show a shaped line without typing.
    pub fn with_text(mut self, text: &str) -> Self {
        self.initial = text.to_string();
        self
    }
}

/// The buffer plus everything the editor needs to draw it.
pub struct TextState {
    pub buffer: TextBuffer,
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
    /// A buffer wired to a master.
    pub fn new(inputs: &TextInputs) -> Self {
        let mut buffer = TextBuffer::new();
        buffer.set_glyph_inventory(inputs.inventory.clone());
        buffer.set_kerning_model(inputs.kerning.clone());
        for character in inputs.initial.chars() {
            buffer.insert_character(character);
        }
        buffer.shape_arabic_if_rtl();
        Self {
            buffer,
            line_height: inputs.line_height,
            ascender: inputs.ascender,
            descender: inputs.descender,
            outlines: inputs.outlines.clone(),
        }
    }

    /// Re-read the master, keeping what has been typed. Switching master
    /// or editing a glyph changes advances and outlines, and a text line
    /// that does not follow is showing yesterday's spacing.
    pub fn refresh(&mut self, inputs: &TextInputs) {
        self.buffer.set_glyph_inventory(inputs.inventory.clone());
        self.buffer.set_kerning_model(inputs.kerning.clone());
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
    pub fn insert(&mut self, character: char) -> bool {
        let inserted = self.buffer.insert_character(character);
        if inserted {
            self.buffer.shape_arabic_if_rtl();
        }
        inserted
    }

    /// Insert a named glyph, for typing something with no codepoint.
    pub fn insert_glyph(&mut self, name: &str, advance: f64, codepoint: Option<char>) {
        self.buffer.insert_glyph(name, codepoint, advance);
        self.buffer.shape_arabic_if_rtl();
    }

    /// Every sort to draw, as a path already placed on the line.
    ///
    /// Absorbed sorts (a character folded into a ligature drawn by an
    /// earlier sort) contribute nothing, which is what `is_absorbed` is
    /// for.
    pub fn placed(&self) -> Vec<PlacedSort> {
        let layout = self.buffer.layout(self.line_height);
        let active = self.buffer.active_sort();
        layout
            .items
            .iter()
            .filter_map(|item| {
                let sort = self.buffer.sort(item.index)?;
                if sort.is_absorbed() {
                    return None;
                }
                let name = sort.glyph_name()?;
                let outline = self.outline(name)?;
                let placed = Affine::translate((item.x, item.y)) * (**outline).clone();
                Some(PlacedSort {
                    index: item.index,
                    path: placed,
                    origin: Point::new(item.x, item.y),
                    advance: item.advance_width,
                    active: active == Some(item.index),
                })
            })
            .collect()
    }

    /// Where the caret sits, in design space.
    pub fn caret(&self) -> Point {
        let layout = self.buffer.layout(self.line_height);
        Point::new(layout.cursor_x, layout.cursor_y)
    }

    /// A click: put the caret there, and report the sort under it.
    pub fn click(&mut self, at: Point) -> Option<usize> {
        let hit = self
            .buffer
            .hit_test(at.x, at.y, self.line_height, self.ascender, self.descender);
        self.buffer.place_cursor_at(
            at.x,
            at.y,
            self.line_height,
            self.ascender,
            self.descender,
        );
        hit.active_sort
    }

    /// Make a sort the one being edited, and report its glyph.
    pub fn activate(&mut self, index: usize) -> Option<String> {
        self.buffer.activate_sort(index).then(|| {
            self.buffer
                .sort(index)
                .and_then(|sort| sort.glyph_name())
                .map(str::to_string)
        })?
    }
}

/// One sort, ready to draw.
pub struct PlacedSort {
    pub index: usize,
    pub path: BezPath,
    pub origin: Point,
    pub advance: f64,
    pub active: bool,
}
