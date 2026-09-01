// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Text buffer behind the editor's Text tool.
//!
//! A `TextBuffer` holds a line of sorts, one `TextSort` per typed glyph
//! or line break, plus a cursor and an optional active sort that the
//! glyph editor opens.
//!
//! Each line reads left to right or right to left, either pinned by
//! the toolbar or detected from its first strong character. The
//! Unicode Bidirectional Algorithm splits a line into visual runs so
//! Latin inside Arabic keeps its own order. Sorts are shaped through
//! the font's own `features.fea` when it compiles, and through the
//! built-in Arabic joining rules in the `shaping` module otherwise.
//!
//! Kerning comes from a `TextKerningModel` that resolves pairs
//! through UFO `kern1` and `kern2` groups, and a manual kerning drag
//! writes a direct pair value back into that model. The `layout` and
//! `hit_test` methods place every sort in font units and map a point
//! back to a cursor position or a sort, which is what the
//! `runebender-core` CLI and runebender-gpui draw and click on.

use crate::{document::model::kerning::lookup_kerning as lookup_xilem_kerning, text::joining};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use unicode_bidi::{BidiInfo, Level};

use crate::text::shape::{ShapingFont, ShapingGlyph, ShapingSource, log_shaping_failure};

mod bidi;
mod kerning;
mod layout;
mod shaping;

/// The direction a character forces on its line, if any.
///
/// Neutrals, such as digits, punctuation, and spaces, return `None`
/// so they never decide a line's direction on their own.
pub fn strong_direction(char: char) -> Option<TextDirection> {
    let code = char as u32;
    let rtl = matches!(code,
        0x0590..=0x05FF   // Hebrew
        | 0x0600..=0x06FF // Arabic
        | 0x0700..=0x074F // Syriac
        | 0x0750..=0x077F // Arabic Supplement
        | 0x0780..=0x07BF // Thaana
        | 0x08A0..=0x08FF // Arabic Extended-A
        | 0xFB1D..=0xFDFF // Hebrew / Arabic presentation forms
        | 0xFE70..=0xFEFF // Arabic presentation forms-B
    );
    if rtl {
        return Some(TextDirection::RightToLeft);
    }
    if char.is_alphabetic() {
        return Some(TextDirection::LeftToRight);
    }
    None
}

/// Reading direction of a line or a bidi run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDirection {
    /// Reads left to right, the default.
    #[default]
    LeftToRight,
    /// Reads right to left, as Arabic and Hebrew do.
    RightToLeft,
}

/// What a sort holds: a glyph with its metrics, or a line break.
#[derive(Debug, Clone, PartialEq)]
pub enum TextSortKind {
    /// A glyph slot. `name` is the glyph drawn, `codepoint` the character typed, if any, and `advance_width` its width in font units.
    Glyph {
        /// Name of the glyph drawn.
        name: String,
        /// The character typed to produce this sort, if any.
        codepoint: Option<char>,
        /// The glyph's advance width in font units.
        advance_width: f64,
    },
    /// A hard line break. It draws nothing and starts a new line.
    LineBreak,
}

/// One slot in the buffer: a glyph or a line break, with its editing state.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSort {
    /// What this sort holds.
    pub kind: TextSortKind,
    /// True when this is the sort open in the glyph editor. Only one sort is active at a time.
    pub active: bool,
    /// Set by shaping when this character was folded into a ligature
    /// drawn by an earlier sort. See `TextSort::is_absorbed`.
    pub absorbed: bool,
}

/// Result of `TextBuffer::layout`: every drawn sort placed in font units, plus the caret position.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLayout {
    /// One entry per drawn sort. Line breaks and absorbed sorts have no item.
    pub items: Vec<TextLayoutItem>,
    /// Caret x in font units, at the edge the caret's line reads from when the line is empty.
    pub cursor_x: f64,
    /// Caret baseline y. Lines go down, so line `n` sits at `-n * line_height`.
    pub cursor_y: f64,
}

/// Placement of one sort in a layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLayoutItem {
    /// Index of the sort in the buffer.
    pub index: usize,
    /// Left edge of the glyph in font units, kerning already applied.
    pub x: f64,
    /// Baseline y of the glyph's line.
    pub y: f64,
    /// Advance width of the glyph as placed, after shaping.
    pub advance_width: f64,
}

/// One stretch of a line at a single bidi embedding level.
#[derive(Debug, Clone, PartialEq)]
struct VisualRun {
    /// The run reads right to left.
    rtl: bool,
    /// Its sorts, in logical order.
    sorts: Vec<usize>,
    /// The same sorts in the order they are drawn, left to right.
    drawn: Vec<usize>,
}

impl VisualRun {
    fn new(rtl: bool, sorts: Vec<usize>) -> Self {
        let mut drawn = sorts.clone();
        if rtl {
            drawn.reverse();
        }
        Self { rtl, sorts, drawn }
    }

    fn visual_order(&self) -> &[usize] {
        &self.drawn
    }
}

/// Kerning between two glyphs placed next to each other on screen.
/// Pairs are stored in logical order, so inside a right-to-left run the
/// glyph drawn first is the second half of the pair.
fn kern_between(
    buffer: &TextBuffer,
    previous: Option<&str>,
    current: Option<&str>,
    rtl: bool,
) -> f64 {
    let Some((previous, current)) = previous.zip(current) else {
        return 0.0;
    };
    if rtl {
        buffer.lookup_kerning(current, previous)
    } else {
        buffer.lookup_kerning(previous, current)
    }
}

/// Result of `TextBuffer::hit_test`: where a click lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextHit {
    /// Cursor position the click maps to, as a boundary index between sorts.
    pub cursor: usize,
    /// The sort whose box contains the point, if any.
    pub active_sort: Option<usize>,
}

/// Result of `TextBuffer::activate_sort_at`: the sort that was activated and where it sits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextSortActivation {
    /// Index of the activated sort in the buffer.
    pub index: usize,
    /// Left edge of the sort in the layout, in font units.
    pub x: f64,
    /// Baseline y of the sort's line.
    pub y: f64,
}

/// Kerning pairs and groups for one master, in UFO terms.
///
/// Pairs may name glyphs or `public.kern1` and `public.kern2` groups. Lookup tries the glyph pair first, then falls back through the groups.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct TextKerningModel {
    #[serde(default)]
    groups: HashMap<String, Vec<String>>,
    #[serde(default, rename = "leftGroups")]
    left_groups: HashMap<String, String>,
    #[serde(default, rename = "rightGroups")]
    right_groups: HashMap<String, String>,
    #[serde(default)]
    kerning: HashMap<String, HashMap<String, f64>>,
}

/// What the buffer knows about a master's glyphs: codepoint map, advance widths, optional SVG outlines, the feature file, and units per em.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct TextGlyphInventory {
    #[serde(default)]
    unicode: HashMap<u32, String>,
    #[serde(default)]
    widths: HashMap<String, f64>,
    #[serde(default)]
    outlines: HashMap<String, String>,
    /// The master's features.fea. Empty means shape with the built-in
    /// joining rules instead of the font's own.
    #[serde(default)]
    features: String,
    #[serde(default = "default_units_per_em")]
    units_per_em: f64,
}

fn default_units_per_em() -> f64 {
    1000.0
}

impl TextGlyphInventory {
    fn has_glyph(&self, name: &str) -> bool {
        self.widths.contains_key(name) || self.outlines.contains_key(name)
    }

    /// Build the inventory straight from a norad font (native hosts;
    /// the web host builds the same thing as JSON). Outlines stay
    /// empty: native hosts draw from their live paths, and shaping
    /// never looks at outlines.
    pub fn from_font(font: &norad::Font) -> Self {
        let mut unicode = HashMap::new();
        let mut widths = HashMap::new();
        for glyph in font.default_layer().iter() {
            let name = glyph.name().to_string();
            if let Some(c) = glyph.codepoints.iter().next() {
                unicode.entry(c as u32).or_insert_with(|| name.clone());
            }
            widths.insert(name, glyph.width);
        }
        Self {
            unicode,
            widths,
            outlines: HashMap::new(),
            features: font.features.clone(),
            units_per_em: font
                .font_info
                .units_per_em
                .map(|v| v.as_f64())
                .unwrap_or_else(default_units_per_em),
        }
    }
}

impl TextKerningModel {
    /// Every stored pair as first key, second key, and value, group
    /// names included, for hosts syncing buffer kerning back into a
    /// font.
    pub fn pairs(&self) -> &HashMap<String, HashMap<String, f64>> {
        &self.kerning
    }

    /// Build the kerning model from a norad font's groups and
    /// kerning.plist (native hosts; the web host sends JSON).
    pub fn from_font(font: &norad::Font) -> Self {
        let mut groups = HashMap::new();
        let mut left_groups = HashMap::new();
        let mut right_groups = HashMap::new();
        for (name, members) in font.groups.iter() {
            let members: Vec<String> = members.iter().map(|m| m.to_string()).collect();
            // UFO names groups by pair position: kern1 is the first
            // glyph's right edge, kern2 the second glyph's left edge.
            if name.starts_with("public.kern1.") {
                for member in &members {
                    right_groups.insert(member.clone(), name.to_string());
                }
            } else if name.starts_with("public.kern2.") {
                for member in &members {
                    left_groups.insert(member.clone(), name.to_string());
                }
            }
            groups.insert(name.to_string(), members);
        }
        let kerning = font
            .kerning
            .iter()
            .map(|(first, pairs)| {
                (
                    first.to_string(),
                    pairs
                        .iter()
                        .map(|(second, value)| (second.to_string(), *value))
                        .collect(),
                )
            })
            .collect();
        Self {
            groups,
            left_groups,
            right_groups,
            kerning,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ManualKerningSession {
    sort_index: usize,
    start_x: f64,
    original_value: f64,
    current_offset: f64,
}

impl TextSort {
    /// Make an inactive glyph sort with the given name, typed character, and advance width.
    pub fn glyph(name: impl Into<String>, codepoint: Option<char>, advance_width: f64) -> Self {
        Self {
            kind: TextSortKind::Glyph {
                name: name.into(),
                codepoint,
                advance_width,
            },
            active: false,
            absorbed: false,
        }
    }

    /// Make an inactive line-break sort.
    pub fn line_break() -> Self {
        Self {
            kind: TextSortKind::LineBreak,
            active: false,
            absorbed: false,
        }
    }

    /// True when shaping folded this character into a ligature drawn
    /// by an earlier sort, like the alef of lam-alef. It keeps its
    /// place in the buffer so editing and the cursor still see the
    /// character, but it draws nothing and takes no width.
    pub fn is_absorbed(&self) -> bool {
        self.absorbed
    }

    /// The glyph this sort draws, or `None` for a line break.
    pub fn glyph_name(&self) -> Option<&str> {
        match &self.kind {
            TextSortKind::Glyph { name, .. } => Some(name),
            TextSortKind::LineBreak => None,
        }
    }
}

/// Cache slot for the compiled shaping font. Derived state: equality and
/// cloning ignore it.
#[derive(Debug, Default)]
struct ShapingFontCache(RefCell<Option<Option<Rc<ShapingFont>>>>);

impl Clone for ShapingFontCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PartialEq for ShapingFontCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl ShapingFontCache {
    fn get(&self) -> Option<Option<Rc<ShapingFont>>> {
        self.0.borrow().clone()
    }

    fn set(&self, value: Option<Rc<ShapingFont>>) {
        self.0.replace(Some(value));
    }

    fn clear(&self) {
        self.0.replace(None);
    }
}

/// Bidi runs already worked out for a stretch of the buffer, keyed by
/// what they were worked out from. Derived state: equality and cloning
/// ignore it, the same as the shaping font.
///
/// Without this the whole buffer is re-analysed on every frame, which at
/// a page of text costs more than drawing it.
#[derive(Debug, Default)]
struct BidiRunCache(RefCell<HashMap<BidiKey, Rc<Vec<VisualRun>>>>);

/// What a cached set of runs was computed from: the stretch of sorts,
/// the base direction, and a hash of the characters in it.
type BidiKey = (usize, usize, bool, u64);

/// Enough entries for the lines of a long text plus the preview's
/// groups; cleared wholesale rather than aged, since a buffer edit
/// invalidates nearly all of them anyway.
const BIDI_CACHE_LIMIT: usize = 256;

impl Clone for BidiRunCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PartialEq for BidiRunCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
/// A line of sorts under edit: the text, its cursor, direction, kerning, and cached shaping.
/// This is the model behind the text tool; it owns no rendering.
pub struct TextBuffer {
    sorts: Vec<TextSort>,
    cursor: usize,
    active_sort: Option<usize>,
    /// Base direction when the user has picked one explicitly.
    direction: TextDirection,
    /// True until the toolbar sets a direction: each line then follows
    /// its own first strong character, which is what lets a Latin line
    /// and an Arabic line share a buffer.
    auto_direction: bool,
    kerning: TextKerningModel,
    glyph_inventory: TextGlyphInventory,
    manual_kerning: Option<ManualKerningSession>,
    /// Preview feature overrides handed to the shaper.
    feature_overrides: Vec<(String, bool)>,
    /// Explicit shaping script (ISO 15924 tag) and language (BCP 47)
    /// for the whole session; None keeps direction-derived defaults.
    script_override: Option<String>,
    language_override: Option<String>,
    /// Font compiled from the inventory + features.fea, built on first
    /// use and dropped whenever the inventory changes. The inner `None`
    /// means the compile failed, which is the normal state mid-edit.
    ///
    /// Derived from the inventory, so it is skipped by `PartialEq` and
    /// `Clone` starts empty: two buffers with the same text are equal
    /// whether or not either has compiled its font yet.
    shaping_font: ShapingFontCache,
    /// Bidi runs per stretch of text. Derived from the sorts, so it is
    /// skipped by `PartialEq` and starts empty on clone.
    bidi_runs: BidiRunCache,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self {
            sorts: Vec::new(),
            cursor: 0,
            active_sort: None,
            direction: TextDirection::default(),
            // Detect per line until the toolbar pins a direction.
            auto_direction: true,
            kerning: TextKerningModel::default(),
            glyph_inventory: TextGlyphInventory::default(),
            manual_kerning: None,
            feature_overrides: Vec::new(),
            script_override: None,
            language_override: None,
            shaping_font: ShapingFontCache::default(),
            bidi_runs: BidiRunCache::default(),
        }
    }
}

impl TextBuffer {
    /// Make an empty buffer with no glyphs, no kerning, and per-line direction detection on.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of sorts in the buffer, line breaks included.
    pub fn len(&self) -> usize {
        self.sorts.len()
    }

    /// True when the buffer holds no sorts.
    pub fn is_empty(&self) -> bool {
        self.sorts.is_empty()
    }

    /// Caret position as a boundary index: `0` is before the first sort, `len()` is after the last.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Index of the sort open in the glyph editor, if any.
    pub fn active_sort(&self) -> Option<usize> {
        self.active_sort
    }

    /// The sort at `index`, or `None` when the index is out of range.
    pub fn sort(&self, index: usize) -> Option<&TextSort> {
        self.sorts.get(index)
    }

    /// The SVG path data stored for a glyph, or `None` when the inventory has no outline for it.
    pub fn glyph_outline_svg(&self, glyph_name: &str) -> Option<&str> {
        self.glyph_inventory
            .outlines
            .get(glyph_name)
            .map(String::as_str)
    }

    /// Replace the name, codepoint, and advance width of the glyph sort at `index`.
    /// Returns false when the index is out of range or the sort is a line break.
    pub fn update_glyph(
        &mut self,
        index: usize,
        name: impl Into<String>,
        codepoint: Option<char>,
        advance_width: f64,
    ) -> bool {
        let Some(sort) = self.sorts.get_mut(index) else {
            return false;
        };
        let TextSortKind::Glyph {
            name: glyph_name,
            codepoint: glyph_codepoint,
            advance_width: glyph_advance_width,
        } = &mut sort.kind
        else {
            return false;
        };
        *glyph_name = name.into();
        *glyph_codepoint = codepoint;
        *glyph_advance_width = advance_width;
        true
    }

    /// Per-feature shaping overrides: (tag, on). Unlisted tags keep
    /// the shaper's defaults. Hosts drive this from preview toggles.
    pub fn set_feature_overrides(&mut self, overrides: Vec<(String, bool)>) {
        if self.feature_overrides != overrides {
            self.feature_overrides = overrides;
        }
    }

    /// The per-feature shaping overrides set by `set_feature_overrides`, as `(tag, on)` pairs.
    pub fn feature_overrides(&self) -> &[(String, bool)] {
        &self.feature_overrides
    }

    /// Shaping script/language overrides ("arab"/"ur"): language is
    /// what makes languagesystem-specific rules (locl for Urdu or
    /// Sindhi) fire in the preview.
    pub fn set_shaping_locale(&mut self, script: Option<String>, language: Option<String>) {
        self.script_override = script;
        self.language_override = language;
    }

    /// The script and language overrides set by `set_shaping_locale`, each `None` when unset.
    pub fn shaping_locale(&self) -> (Option<&str>, Option<&str>) {
        (
            self.script_override.as_deref(),
            self.language_override.as_deref(),
        )
    }

    /// Replace one glyph's outline without rebuilding the whole
    /// inventory. Used when an edit to a base glyph changes every
    /// composite that places it.
    pub fn set_glyph_outline(&mut self, name: &str, outline: &str) {
        if outline.is_empty() {
            self.glyph_inventory.outlines.remove(name);
        } else {
            self.glyph_inventory
                .outlines
                .insert(name.to_string(), outline.to_string());
        }
    }

    /// Replace everything *except* the outlines, which are maintained one
    /// glyph at a time as edits land.
    ///
    /// The wholesale replace re-sends every outline in the font on
    /// every edit. That is slow, a few MB of JSON, and worse: it hands
    /// back the outlines as the sender last knew them, undoing
    /// anything updated since. Mid-nudge that reads as a flash: the
    /// composite snaps to its old shape and forward again.
    pub fn set_glyph_metrics(&mut self, mut inventory: TextGlyphInventory) {
        std::mem::swap(&mut inventory.outlines, &mut self.glyph_inventory.outlines);
        self.glyph_inventory = inventory;
        self.shaping_font.clear();
    }

    /// Replace the whole glyph inventory, outlines included, and drop the cached shaping font so it is rebuilt on next use.
    pub fn set_glyph_inventory(&mut self, glyph_inventory: TextGlyphInventory) {
        self.glyph_inventory = glyph_inventory;
        // Advances, codepoints and features all feed the shaping font.
        self.shaping_font.clear();
    }

    /// Iterate over the sorts in logical order.
    pub fn iter(&self) -> impl Iterator<Item = &TextSort> {
        self.sorts.iter()
    }

    /// Insert the glyph mapped to `char` at the cursor, using the inventory's advance width.
    /// Returns false when the inventory has no glyph for the character. See `insert_character_with_active_advance`.
    pub fn insert_character(&mut self, char: char) -> bool {
        self.insert_character_with_active_advance(char, None)
    }

    /// Insert the glyph mapped to `char` at the cursor as an inactive sort and advance the cursor.
    /// `active_advance_width` is the live width of the glyph being edited and wins over the inventory width, except for Arabic characters on an RTL line, which shaping will resize anyway.
    /// Reshapes around the insertion point. Returns false when the inventory has no glyph for the character.
    pub fn insert_character_with_active_advance(
        &mut self,
        char: char,
        active_advance_width: Option<f64>,
    ) -> bool {
        let Some(glyph_name) = self.glyph_inventory.unicode.get(&(char as u32)).cloned() else {
            return false;
        };
        let use_active_advance =
            self.cursor_direction() != TextDirection::RightToLeft || !joining::is_arabic(char);
        let advance_width = active_advance_width
            .filter(|_| use_active_advance)
            .or_else(|| self.glyph_inventory.widths.get(&glyph_name).copied())
            .unwrap_or(500.0);
        let position = self.cursor;
        self.insert_inactive_glyph(glyph_name, Some(char), advance_width);
        self.shape_arabic_around_if_rtl(position);
        true
    }

    /// Remove every sort, reset the cursor, active sort, and manual kerning session, and go back to auto direction. The inventory and kerning model are kept.
    pub fn clear(&mut self) {
        self.sorts.clear();
        self.cursor = 0;
        self.active_sort = None;
        self.manual_kerning = None;
        self.direction = TextDirection::default();
        self.auto_direction = true;
    }

    /// Insert a glyph sort at the cursor, make it the active sort, and advance the cursor past it.
    /// Deactivates the previous active sort and ends any manual kerning session.
    pub fn insert_glyph(
        &mut self,
        name: impl Into<String>,
        codepoint: Option<char>,
        advance_width: f64,
    ) {
        self.manual_kerning = None;
        if let Some(active) = self.active_sort
            && let Some(sort) = self.sorts.get_mut(active)
        {
            sort.active = false;
        }
        self.active_sort = None;
        let index = self.cursor;
        self.sorts
            .insert(index, TextSort::glyph(name, codepoint, advance_width));
        self.set_active_sort(Some(index));
        self.cursor += 1;
    }

    /// Insert a glyph sort at the cursor without activating it, and advance the cursor past it.
    /// The active sort index shifts right when it sits at or after the cursor. Ends any manual kerning session.
    pub fn insert_inactive_glyph(
        &mut self,
        name: impl Into<String>,
        codepoint: Option<char>,
        advance_width: f64,
    ) {
        self.insert_inactive_glyph_at_cursor(name, codepoint, advance_width);
    }

    /// Insert a line break at the cursor and advance the cursor past it.
    /// The active sort index shifts right when it sits at or after the cursor. Ends any manual kerning session.
    pub fn insert_line_break(&mut self) {
        self.manual_kerning = None;
        let index = self.cursor;
        self.sorts.insert(self.cursor, TextSort::line_break());
        self.cursor += 1;
        if let Some(active) = self.active_sort
            && active >= index
        {
            self.active_sort = Some(active + 1);
        }
    }

    /// Backspace: remove the sort before the cursor and move the cursor back.
    /// Returns the removed sort, or `None` at the start of the buffer. Clears the active sort if it was removed, and ends any manual kerning session.
    pub fn delete_before_cursor(&mut self) -> Option<TextSort> {
        if self.cursor == 0 {
            return None;
        }
        self.manual_kerning = None;
        let deleted_index = self.cursor - 1;
        let deleted = self.sorts.remove(deleted_index);
        self.cursor -= 1;
        self.adjust_active_after_delete(deleted_index);
        Some(deleted)
    }

    /// Forward delete: remove the sort at the cursor, leaving the cursor in place.
    /// Returns the removed sort, or `None` at the end of the buffer. Clears the active sort if it was removed, and ends any manual kerning session.
    pub fn delete_after_cursor(&mut self) -> Option<TextSort> {
        if self.cursor >= self.sorts.len() {
            return None;
        }
        self.manual_kerning = None;
        let deleted = self.sorts.remove(self.cursor);
        self.adjust_active_after_delete(self.cursor);
        Some(deleted)
    }

    /// Move the cursor one sort back in logical order, stopping at `0`.
    pub fn move_cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Move the cursor one sort forward in logical order, stopping at `len()`.
    pub fn move_cursor_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.sorts.len());
    }

    /// Move the cursor one sort toward the left of the screen: back on an LTR line, forward on an RTL line.
    pub fn move_cursor_visual_left(&mut self) {
        match self.cursor_direction() {
            TextDirection::LeftToRight => self.move_cursor_left(),
            TextDirection::RightToLeft => self.move_cursor_right(),
        }
    }

    /// Move the cursor one sort toward the right of the screen: forward on an LTR line, back on an RTL line.
    pub fn move_cursor_visual_right(&mut self) {
        match self.cursor_direction() {
            TextDirection::LeftToRight => self.move_cursor_right(),
            TextDirection::RightToLeft => self.move_cursor_left(),
        }
    }

    /// Move the caret to the line above or below, keeping it as close
    /// as possible to the x it is at now, the way arrow keys work in
    /// any text editor. Returns false when there is no line that way.
    pub fn move_cursor_vertically(&mut self, delta: i32, line_height: f64) -> bool {
        let current_line = self.line_number_for_sort(self.cursor);
        let target = current_line as i64 + delta as i64;
        if target < 0 || target as usize >= self.line_count() {
            return false;
        }
        let line_height = line_height.max(1.0);
        let layout = self.layout(line_height);
        let x = layout.cursor_x;
        let (line_start, line_end) = self.line_range_for_number(target as usize);
        self.cursor = self.nearest_cursor_for_line(x, line_start, line_end, &layout);
        true
    }

    /// Home / End: the logical start or end of the caret's own line.
    pub fn move_cursor_to_line_edge(&mut self, to_end: bool) {
        let line = self.line_number_for_sort(self.cursor);
        let (line_start, line_end) = self.line_range_for_number(line);
        self.cursor = if to_end { line_end } else { line_start };
    }

    /// Where a click puts the caret: the boundary between sorts nearest
    /// the point. Clicking a glyph's left half lands before it and its
    /// right half after it, rather than always landing after the glyph
    /// the way sort activation does.
    pub fn place_cursor_at(
        &mut self,
        x: f64,
        y: f64,
        line_height: f64,
        ascender: f64,
        descender: f64,
    ) -> usize {
        let line_height = line_height.max(1.0);
        let layout = self.layout(line_height);
        let line = self.line_number_for_y(y, line_height, ascender, descender);
        let (line_start, line_end) = self.line_range_for_number(line);
        self.cursor = self.nearest_cursor_for_line(x, line_start, line_end, &layout);
        self.cursor
    }

    /// Move the cursor to a boundary index, clamped to `len()`.
    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = cursor.min(self.sorts.len());
    }

    /// Make the sort at `index` the active one and deactivate the previous active sort.
    /// Returns false, changing nothing, when the index is out of range or the sort is a line break.
    pub fn activate_sort(&mut self, index: usize) -> bool {
        if !matches!(
            self.sorts.get(index).map(|sort| &sort.kind),
            Some(TextSortKind::Glyph { .. })
        ) {
            return false;
        }
        self.set_active_sort(Some(index));
        true
    }

    /// Activate the sort whose box contains the point, using the same hit rules as `hit_test`.
    /// Returns the activated sort and its layout position, or `None` when no sort is under the point.
    pub fn activate_sort_at(
        &mut self,
        x: f64,
        y: f64,
        line_height: f64,
        ascender: f64,
        descender: f64,
    ) -> Option<TextSortActivation> {
        let layout = self.layout(line_height);
        let item = self.hit_sort_item_at(x, y, line_height, ascender, descender, &layout)?;
        self.activate_sort(item.index)
            .then_some(TextSortActivation {
                index: item.index,
                x: item.x,
                y: item.y,
            })
    }

    fn set_active_sort(&mut self, active: Option<usize>) {
        if self.active_sort == active {
            return;
        }
        if let Some(previous) = self.active_sort
            && Some(previous) != active
            && let Some(sort) = self.sorts.get_mut(previous)
        {
            sort.active = false;
        }
        self.active_sort = None;
        if let Some(index) = active
            && let Some(sort) = self.sorts.get_mut(index)
        {
            sort.active = true;
            self.active_sort = Some(index);
        } else {
            self.active_sort = None;
        }
    }

    /// Open a glyph beside the one being edited: double-clicking a
    /// component should put its base glyph next to the current sort, not
    /// wherever the cursor happens to be sitting in the line.
    ///
    /// The new sort becomes the active one, so it is what gets edited.
    pub fn insert_glyph_after_active(
        &mut self,
        name: impl Into<String>,
        codepoint: Option<char>,
        advance_width: f64,
    ) -> usize {
        self.manual_kerning = None;
        let index = match self.active_sort {
            Some(active) => (active + 1).min(self.sorts.len()),
            None => self.cursor,
        };
        self.sorts
            .insert(index, TextSort::glyph(name, codepoint, advance_width));
        if self.cursor >= index {
            self.cursor += 1;
        }
        if let Some(active) = self.active_sort
            && active >= index
        {
            self.active_sort = Some(active + 1);
        }
        self.set_active_sort(Some(index));
        self.cursor = index + 1;
        index
    }

    fn insert_inactive_glyph_at_cursor(
        &mut self,
        name: impl Into<String>,
        codepoint: Option<char>,
        advance_width: f64,
    ) {
        self.manual_kerning = None;
        let index = self.cursor;
        self.sorts
            .insert(index, TextSort::glyph(name, codepoint, advance_width));
        self.cursor += 1;
        if let Some(active) = self.active_sort
            && active >= index
        {
            self.active_sort = Some(active + 1);
        }
    }

    fn adjust_active_after_delete(&mut self, deleted_index: usize) {
        let Some(active) = self.active_sort else {
            return;
        };
        if active == deleted_index {
            self.set_active_sort(None);
        } else if active > deleted_index {
            self.active_sort = Some(active - 1);
        }
    }
}

#[cfg(test)]
mod tests;
