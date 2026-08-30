//! Text buffer state for the Text tool.
//!
//! This is the wasm-core counterpart to runebender-xilem's `sort`
//! buffer. It intentionally starts small: Vue still owns glyph lookup
//! and preview rendering today, but cursor movement, line breaks, and
//! active sort selection now have a Rust-side home we can migrate to.

use crate::{model::kerning::lookup_kerning as lookup_xilem_kerning, shaping};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use unicode_bidi::{BidiInfo, Level};

use crate::shape::{ShapingFont, ShapingGlyph, ShapingSource, log_shaping_failure};

/// The direction a character forces on its line, if any. Neutrals
/// (digits, punctuation, spaces) return `None` so they never decide a
/// line's direction on their own.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDirection {
    #[default]
    LeftToRight,
    RightToLeft,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextSortKind {
    Glyph {
        name: String,
        codepoint: Option<char>,
        advance_width: f64,
    },
    LineBreak,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextSort {
    pub kind: TextSortKind,
    pub active: bool,
    /// Set by shaping when this character was folded into a ligature
    /// drawn by an earlier sort. See `TextSort::is_absorbed`.
    pub absorbed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextLayout {
    pub items: Vec<TextLayoutItem>,
    pub cursor_x: f64,
    pub cursor_y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLayoutItem {
    pub index: usize,
    pub x: f64,
    pub y: f64,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextHit {
    pub cursor: usize,
    pub active_sort: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextSortActivation {
    pub index: usize,
    pub x: f64,
    pub y: f64,
}

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
    /// Every stored pair (first key, second key, value) — group names
    /// included — for hosts syncing buffer kerning back into a font.
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

    pub fn line_break() -> Self {
        Self {
            kind: TextSortKind::LineBreak,
            active: false,
            absorbed: false,
        }
    }

    /// True when shaping folded this character into a ligature drawn by
    /// an earlier sort — the alef of lam-alef. It keeps its place in the
    /// buffer so editing and the cursor still see the character, but it
    /// draws nothing and takes no width.
    pub fn is_absorbed(&self) -> bool {
        self.absorbed
    }

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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.sorts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sorts.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn active_sort(&self) -> Option<usize> {
        self.active_sort
    }

    pub fn manual_kerning_sort(&self) -> Option<usize> {
        self.manual_kerning.map(|session| session.sort_index)
    }

    pub fn sort(&self, index: usize) -> Option<&TextSort> {
        self.sorts.get(index)
    }

    pub fn glyph_outline_svg(&self, glyph_name: &str) -> Option<&str> {
        self.glyph_inventory
            .outlines
            .get(glyph_name)
            .map(String::as_str)
    }

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

    pub fn direction(&self) -> TextDirection {
        self.direction
    }

    /// Direction of the line the cursor is on: what the toolbar shows.
    pub fn cursor_direction(&self) -> TextDirection {
        self.resolved_line_direction(self.line_number_for_sort(self.cursor))
    }

    /// True while each line picks its own direction from its content.
    pub fn direction_is_auto(&self) -> bool {
        self.auto_direction
    }

    /// Pin every line to one direction (the toolbar's LTR / RTL).
    pub fn set_direction(&mut self, direction: TextDirection) {
        self.direction = direction;
        self.auto_direction = false;
    }

    /// Go back to per-line detection.
    pub fn set_auto_direction(&mut self) {
        self.auto_direction = true;
    }

    /// In auto mode a line reads right-to-left when its first strong
    /// character does; otherwise every line follows the pinned
    /// direction.
    pub fn resolved_line_direction(&self, line: usize) -> TextDirection {
        if !self.auto_direction {
            return self.direction;
        }
        let (start, end) = self.line_range_for_number(line);
        for index in start..end.min(self.sorts.len()) {
            if let TextSortKind::Glyph {
                codepoint: Some(char),
                ..
            } = &self.sorts[index].kind
                && let Some(direction) = strong_direction(*char)
            {
                return direction;
            }
        }
        self.direction
    }

    fn line_number_for_sort(&self, sort_index: usize) -> usize {
        self.sorts[..sort_index.min(self.sorts.len())]
            .iter()
            .filter(|sort| matches!(sort.kind, TextSortKind::LineBreak))
            .count()
    }

    pub fn line_count(&self) -> usize {
        1 + self
            .sorts
            .iter()
            .filter(|sort| matches!(sort.kind, TextSortKind::LineBreak))
            .count()
    }

    /// Per-feature shaping overrides: (tag, on). Unlisted tags keep
    /// the shaper's defaults. Hosts drive this from preview toggles.
    pub fn set_feature_overrides(&mut self, overrides: Vec<(String, bool)>) {
        if self.feature_overrides != overrides {
            self.feature_overrides = overrides;
        }
    }

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

    pub fn shaping_locale(&self) -> (Option<&str>, Option<&str>) {
        (
            self.script_override.as_deref(),
            self.language_override.as_deref(),
        )
    }

    pub fn set_kerning_model(&mut self, kerning: TextKerningModel) {
        self.kerning = kerning;
    }

    pub fn kerning_model(&self) -> &TextKerningModel {
        &self.kerning
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
    /// The wholesale replace re-sends every outline in the font on every
    /// edit. That is slow — a few MB of JSON — but worse, it hands back the
    /// outlines as the sender last knew them, undoing anything updated since.
    /// Mid-nudge that reads as a flash: the composite snaps to its old shape
    /// and forward again.
    pub fn set_glyph_metrics(&mut self, mut inventory: TextGlyphInventory) {
        std::mem::swap(&mut inventory.outlines, &mut self.glyph_inventory.outlines);
        self.glyph_inventory = inventory;
        self.shaping_font.clear();
    }

    pub fn set_glyph_inventory(&mut self, glyph_inventory: TextGlyphInventory) {
        self.glyph_inventory = glyph_inventory;
        // Advances, codepoints and features all feed the shaping font.
        self.shaping_font.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &TextSort> {
        self.sorts.iter()
    }

    pub fn insert_character(&mut self, char: char) -> bool {
        self.insert_character_with_active_advance(char, None)
    }

    pub fn insert_character_with_active_advance(
        &mut self,
        char: char,
        active_advance_width: Option<f64>,
    ) -> bool {
        let Some(glyph_name) = self.glyph_inventory.unicode.get(&(char as u32)).cloned() else {
            return false;
        };
        let use_active_advance =
            self.cursor_direction() != TextDirection::RightToLeft || !shaping::is_arabic(char);
        let advance_width = active_advance_width
            .filter(|_| use_active_advance)
            .or_else(|| self.glyph_inventory.widths.get(&glyph_name).copied())
            .unwrap_or(500.0);
        let position = self.cursor;
        self.insert_inactive_glyph(glyph_name, Some(char), advance_width);
        self.shape_arabic_around_if_rtl(position);
        true
    }

    // TODO(perf): every frame walks the whole buffer, and every sort is
    // drawn whether or not it is on screen. At a page of text this is the
    // bulk of a frame. Cache the layout against a buffer revision, and
    // cull sorts outside the viewport before handing them to the scene.
    pub fn layout(&self, line_height: f64) -> TextLayout {
        let mut items = Vec::with_capacity(self.sorts.len());
        let mut cursor_x = 0.0;
        let mut cursor_y = 0.0;
        let mut line_start = 0;
        let mut line_number = 0;
        // RTL lines share one right edge so a paragraph stays aligned,
        // the way xilem lines them up; LTR lines start at the origin.
        let rtl_line_start_x = self.rtl_line_start_x();

        while line_start <= self.sorts.len() {
            let line_end = self.next_line_end(line_start);
            let direction = self.resolved_line_direction(line_number);
            let y = -line_height * line_number as f64;

            // Everything is placed left to right, run by run. A line that
            // reads right to left is right-aligned rather than reversed
            // wholesale: the bidi runs carry the order, so Latin inside
            // an Arabic sentence still reads left to right.
            let runs = self.visual_runs_for_line(line_start, line_end);

            // Lay the line out from zero, then shift the finished line
            // into place: a right-to-left line is right-aligned on the
            // widest line. Measuring first would mean looking up every
            // kerning pair twice, which at a page of text is the most
            // expensive thing in the frame.
            let line_items_start = items.len();
            let mut x = 0.0;
            // Caret position within the line, filled in as the run
            // holding the cursor is placed.
            let mut caret_at: Option<f64> = if self.cursor == line_start {
                Some(0.0)
            } else {
                None
            };
            let mut caret_at_line_start = self.cursor == line_start;

            for run in runs.iter() {
                let mut previous: Option<&str> = None;
                for &index in run.visual_order() {
                    // A character folded into a ligature has no glyph of
                    // its own: no item, no width, no kerning pair.
                    if self.sorts[index].absorbed {
                        if self.cursor == index + 1 {
                            caret_at = Some(x);
                            caret_at_line_start = false;
                        }
                        continue;
                    }
                    let advance_width = self.sort_advance(index);
                    let glyph_name = self.sort_glyph_name(index);
                    // Kerning pairs are in logical order. Inside an RTL
                    // run the glyph placed just before this one is the
                    // logically *following* one, so the pair is reversed.
                    x += kern_between(self, previous, glyph_name, run.rtl);
                    items.push(TextLayoutItem {
                        index,
                        x,
                        y,
                        advance_width,
                    });
                    x += advance_width;
                    previous = glyph_name;

                    // The caret follows the text, not the screen: after a
                    // sort in an RTL run it sits at that sort's left edge.
                    if self.cursor == index + 1 {
                        caret_at = Some(if run.rtl { x - advance_width } else { x });
                        caret_at_line_start = false;
                    }
                }
            }

            let line_width = x;
            let line_left = match direction {
                TextDirection::LeftToRight => 0.0,
                TextDirection::RightToLeft => rtl_line_start_x - line_width,
            };
            if line_left != 0.0 {
                for item in &mut items[line_items_start..] {
                    item.x += line_left;
                }
            }
            // An empty caret slot, or one at the start of a line, sits at
            // the edge the line reads from.
            if caret_at_line_start && direction == TextDirection::RightToLeft {
                caret_at = Some(line_width);
            }
            let caret_at = caret_at.map(|caret| caret + line_left);

            let caret_at = match caret_at {
                Some(caret) => Some(caret),
                None if self.cursor >= line_start && self.cursor <= line_end => {
                    Some(match direction {
                        TextDirection::LeftToRight => line_left,
                        TextDirection::RightToLeft => line_left + line_width,
                    })
                }
                None => None,
            };
            if let Some(caret) = caret_at {
                cursor_x = caret;
                cursor_y = y;
            }

            if line_end >= self.sorts.len() {
                break;
            }

            // Skip the line-break sort.
            if self.cursor == line_end + 1 {
                cursor_x = match self.resolved_line_direction(line_number + 1) {
                    TextDirection::LeftToRight => 0.0,
                    TextDirection::RightToLeft => rtl_line_start_x,
                };
                cursor_y = -line_height * (line_number + 1) as f64;
            }
            line_start = line_end + 1;
            line_number += 1;
        }

        TextLayout {
            items,
            cursor_x,
            cursor_y,
        }
    }

    pub fn preview_layout(&self) -> Vec<TextLayoutItem> {
        // The strip is one visual line. Lines that read the same way run
        // on into each other as a single paragraph — an Arabic paragraph
        // keeps reading right to left across its line breaks — while the
        // bidi runs inside still order themselves, so a Latin word in an
        // Arabic sentence reads left to right here too.
        let mut items = Vec::with_capacity(self.sorts.len());
        let mut x = 0.0;

        for (start, end, rtl) in self.preview_groups() {
            for run in self.visual_runs_in(start, end, rtl).iter() {
                let mut previous: Option<(usize, &str)> = None;
                for &index in run.visual_order() {
                    if self.sorts[index].absorbed {
                        continue;
                    }
                    let advance_width = self.sort_advance(index);
                    let glyph_name = self.sort_glyph_name(index);
                    // Kerning stops at a run edge and at a line break:
                    // two lines are not a kerning context, however they
                    // are drawn.
                    let kern = match (previous, glyph_name) {
                        (Some((previous_index, previous_name)), Some(name))
                            if !self.line_break_between(previous_index, index) =>
                        {
                            if run.rtl {
                                self.lookup_kerning(name, previous_name)
                            } else {
                                self.lookup_kerning(previous_name, name)
                            }
                        }
                        _ => 0.0,
                    };
                    x += kern;
                    items.push(TextLayoutItem {
                        index,
                        x,
                        y: 0.0,
                        advance_width,
                    });
                    x += advance_width;
                    previous = glyph_name.map(|name| (index, name));
                }
            }
        }

        items
    }

    /// Stretches of consecutive lines that read the same way, as
    /// (start, end, rtl). The preview strip lays out one at a time.
    fn preview_groups(&self) -> Vec<(usize, usize, bool)> {
        let mut groups: Vec<(usize, usize, bool)> = Vec::new();
        let mut line_start = 0;
        let mut line_number = 0;
        while line_start <= self.sorts.len() {
            let line_end = self.next_line_end(line_start);
            let rtl = self.resolved_line_direction(line_number) == TextDirection::RightToLeft;
            match groups.last_mut() {
                Some(group) if group.2 == rtl => group.1 = line_end,
                _ => groups.push((line_start, line_end, rtl)),
            }
            if line_end >= self.sorts.len() {
                break;
            }
            line_start = line_end + 1;
            line_number += 1;
        }
        groups
    }

    /// Is there a line break between these two sorts?
    fn line_break_between(&self, a: usize, b: usize) -> bool {
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        self.sorts[low.min(self.sorts.len())..high.min(self.sorts.len())]
            .iter()
            .any(|sort| matches!(sort.kind, TextSortKind::LineBreak))
    }

    pub fn hit_test(
        &self,
        x: f64,
        y: f64,
        line_height: f64,
        ascender: f64,
        descender: f64,
    ) -> TextHit {
        let layout = self.layout(line_height);
        self.hit_test_with_layout(x, y, line_height, ascender, descender, &layout)
    }

    fn hit_test_with_layout(
        &self,
        x: f64,
        y: f64,
        line_height: f64,
        ascender: f64,
        descender: f64,
        layout: &TextLayout,
    ) -> TextHit {
        if self.sorts.is_empty() {
            return TextHit {
                cursor: 0,
                active_sort: None,
            };
        }

        let line_height = line_height.max(1.0);
        let target_line = self.line_number_for_y(y, line_height, ascender, descender);
        let (line_start, line_end) = self.line_range_for_number(target_line);
        let nearest_cursor = self.nearest_cursor_for_line(x, line_start, line_end, layout);

        for item in layout
            .items
            .iter()
            .filter(|item| (line_start..line_end).contains(&item.index))
        {
            // Match xilem's `kurbo::Rect::contains` sort hit-test:
            // min edges inclusive, max edges exclusive.
            let within_x = x >= item.x && x < item.x + item.advance_width;
            let within_y = y >= item.y + descender && y < item.y + ascender;
            if within_x && within_y {
                return TextHit {
                    cursor: item.index + 1,
                    active_sort: Some(item.index),
                };
            }
        }

        TextHit {
            cursor: nearest_cursor,
            active_sort: None,
        }
    }

    pub fn clear(&mut self) {
        self.sorts.clear();
        self.cursor = 0;
        self.active_sort = None;
        self.manual_kerning = None;
        self.direction = TextDirection::default();
        self.auto_direction = true;
    }

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

    pub fn insert_inactive_glyph(
        &mut self,
        name: impl Into<String>,
        codepoint: Option<char>,
        advance_width: f64,
    ) {
        self.insert_inactive_glyph_at_cursor(name, codepoint, advance_width);
    }

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

    pub fn delete_after_cursor(&mut self) -> Option<TextSort> {
        if self.cursor >= self.sorts.len() {
            return None;
        }
        self.manual_kerning = None;
        let deleted = self.sorts.remove(self.cursor);
        self.adjust_active_after_delete(self.cursor);
        Some(deleted)
    }

    pub fn move_cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_cursor_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.sorts.len());
    }

    pub fn move_cursor_visual_left(&mut self) {
        match self.cursor_direction() {
            TextDirection::LeftToRight => self.move_cursor_left(),
            TextDirection::RightToLeft => self.move_cursor_right(),
        }
    }

    pub fn move_cursor_visual_right(&mut self) {
        match self.cursor_direction() {
            TextDirection::LeftToRight => self.move_cursor_right(),
            TextDirection::RightToLeft => self.move_cursor_left(),
        }
    }

    /// Move the caret to the line above or below, keeping it as close as
    /// possible to the x it is at now — the way arrow keys work in any
    /// text editor. False when there is no line that way.
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

    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = cursor.min(self.sorts.len());
    }

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
        self.activate_sort(item.index).then(|| TextSortActivation {
            index: item.index,
            x: item.x,
            y: item.y,
        })
    }

    pub fn begin_manual_kerning(&mut self, sort_index: usize, start_x: f64) -> bool {
        if sort_index == 0
            || !matches!(
                self.sorts.get(sort_index).map(|sort| &sort.kind),
                Some(TextSortKind::Glyph { .. })
            )
        {
            return false;
        }
        let original_value = self
            .glyph_pair_names(sort_index)
            .map(|(left, right)| self.lookup_kerning(&left, &right))
            .unwrap_or(0.0)
            .round();
        self.manual_kerning = Some(ManualKerningSession {
            sort_index,
            start_x,
            original_value,
            current_offset: 0.0,
        });
        self.activate_sort(sort_index);
        true
    }

    pub fn drag_manual_kerning(&mut self, current_x: f64) -> Option<f64> {
        let session = self.manual_kerning?;
        let mut current_offset = (current_x - session.start_x).round();
        // In a right-to-left line the pair's visual gap sits on the
        // other side of the dragged sort — the logical-previous glyph
        // draws to the right — so a rightward drag closes the gap and
        // the offset flips sign.
        let line = self.line_number_for_sort(session.sort_index);
        if self.resolved_line_direction(line) == TextDirection::RightToLeft {
            current_offset = -current_offset;
        }
        if current_offset == session.current_offset {
            return None;
        }
        self.manual_kerning = Some(ManualKerningSession {
            current_offset,
            ..session
        });
        let (left, right) = self.glyph_pair_names(session.sort_index)?;
        let value = (session.original_value + current_offset).round();
        self.set_direct_kerning(&left, &right, value);
        Some(value)
    }

    pub fn end_manual_kerning(&mut self) -> bool {
        self.manual_kerning.take().is_some()
    }

    /// The compiled shaping font for the current inventory, or `None`
    /// when there is no features.fea or it does not compile. Built once
    /// and cached until the inventory changes.
    fn shaping_font(&self) -> Option<Rc<ShapingFont>> {
        if self.glyph_inventory.features.trim().is_empty() {
            return None;
        }
        if let Some(cached) = self.shaping_font.get() {
            return cached;
        }

        // Glyph order: every glyph the inventory knows, .notdef first so
        // it takes glyph id 0 the way a real font does.
        let mut names: Vec<&String> = self
            .glyph_inventory
            .widths
            .keys()
            .chain(self.glyph_inventory.outlines.keys())
            .collect();
        names.sort();
        names.dedup();

        let mut unicodes: HashMap<&str, Vec<u32>> = HashMap::new();
        for (codepoint, name) in &self.glyph_inventory.unicode {
            unicodes.entry(name.as_str()).or_default().push(*codepoint);
        }

        let glyphs: Vec<ShapingGlyph> = std::iter::once(".notdef")
            .chain(
                names
                    .iter()
                    .map(|name| name.as_str())
                    .filter(|name| *name != ".notdef"),
            )
            .map(|name| ShapingGlyph {
                name: name.to_string(),
                advance: self
                    .glyph_inventory
                    .widths
                    .get(name)
                    .copied()
                    .unwrap_or(0.0),
                unicodes: unicodes.get(name).cloned().unwrap_or_default(),
            })
            .collect();

        let built = ShapingFont::build(&ShapingSource {
            units_per_em: self.glyph_inventory.units_per_em,
            glyphs,
            features: self.glyph_inventory.features.clone(),
        })
        .map(Rc::new)
        .map_err(|e| {
            // Expected while the feature file is being edited; the old
            // joining rules carry on.
            log_shaping_failure(&e);
        })
        .ok();

        self.shaping_font.set(built.clone());
        built
    }

    /// Shape every line through the font's own rules. Returns false when
    /// there is no usable font, so the caller can fall back.
    ///
    /// Lines are split into runs first. A line mixing Latin and Arabic
    /// has to be shaped a run at a time: handed the whole line, the
    /// shaper takes its script from the first character, and the Arabic
    /// features — including the lam-alef ligature — never run.
    fn shape_with_font(&mut self) -> bool {
        let Some(font) = self.shaping_font() else {
            return false;
        };

        let mut updates: Vec<(usize, String, f64)> = Vec::new();
        let mut absorbed: Vec<bool> = vec![false; self.sorts.len()];

        for line in 0..self.line_count() {
            let (line_start, line_end) = self.line_range_for_number(line);

            // Shape run by run, the way the text is laid out: a run's
            // direction comes from its bidi level, not from the line
            // around it, and a run is one script so the shaper can pick
            // the right rules for it.
            for bidi_run in self.visual_runs_for_line(line_start, line_end).iter() {
                let chars: Vec<(char, usize)> = bidi_run
                    .sorts
                    .iter()
                    .filter_map(|&index| Some((self.sort_codepoint(index)?, index)))
                    .collect();
                let run_rtl = bidi_run.rtl;

                let mut run_start = 0;
                while run_start < chars.len() {
                    let arabic = shaping::is_arabic(chars[run_start].0);
                    let mut run_end = run_start;
                    while run_end < chars.len() && shaping::is_arabic(chars[run_end].0) == arabic {
                        run_end += 1;
                    }

                    let mut text = String::new();
                    let mut sort_for_offset: Vec<usize> = Vec::new();
                    for &(char, index) in &chars[run_start..run_end] {
                        for _ in 0..char.len_utf8() {
                            sort_for_offset.push(index);
                        }
                        text.push(char);
                    }

                    let Ok(shaped) = font.shape_with_options(
                        &text,
                        run_rtl,
                        &self.feature_overrides,
                        self.script_override.as_deref(),
                        self.language_override.as_deref(),
                    ) else {
                        return false;
                    };

                    // Clusters are byte offsets into the run. A ligature
                    // reports the offset of its first character and stands
                    // for every character up to the next cluster.
                    let mut covered = vec![false; sort_for_offset.len()];
                    for glyph in &shaped {
                        let Some(&sort_index) = sort_for_offset.get(glyph.cluster as usize) else {
                            continue;
                        };
                        let Some(name) = font.glyph_name(glyph.glyph_id) else {
                            continue;
                        };
                        updates.push((sort_index, name.to_string(), glyph.x_advance));
                        for (offset, covered) in covered.iter_mut().enumerate() {
                            if sort_for_offset[offset] == sort_index {
                                *covered = true;
                            }
                        }
                    }

                    let mut seen_sort: Option<usize> = None;
                    for (offset, &sort_index) in sort_for_offset.iter().enumerate() {
                        if seen_sort == Some(sort_index) {
                            continue;
                        }
                        seen_sort = Some(sort_index);
                        if !covered[offset] {
                            absorbed[sort_index] = true;
                            updates.push((sort_index, String::new(), 0.0));
                        }
                    }

                    run_start = run_end;
                }
            }
        }

        let changed = self.apply_shape_updates(updates);
        let mut absorbed_changed = false;
        for (index, sort) in self.sorts.iter_mut().enumerate() {
            let want = absorbed.get(index).copied().unwrap_or(false);
            if sort.absorbed != want {
                sort.absorbed = want;
                absorbed_changed = true;
            }
        }
        changed || absorbed_changed
    }

    pub fn shape_arabic(&mut self) -> bool {
        // The font's own GSUB first: it gives ligatures and contextual
        // rules the joining table below cannot express. Falls through
        // when there is no features.fea or it does not compile.
        if self.shape_with_font() {
            return true;
        }
        let chars = self.glyph_chars();
        let mut updates = Vec::new();

        for index in 0..self.sorts.len() {
            let Some(char) = self.sort_codepoint(index) else {
                continue;
            };
            let char_index = self.char_index_for_sort_index(index);
            let name = self.shaped_glyph_name_for_character(char, &chars, char_index, index);
            let advance_width = self
                .glyph_inventory
                .widths
                .get(&name)
                .copied()
                .unwrap_or_else(|| self.sort_advance(index));
            updates.push((index, name, advance_width));
        }

        self.apply_shape_updates(updates)
    }

    /// Shape when any line in the buffer reads RTL — a Latin line next
    /// to an Arabic one must not stop the Arabic from joining.
    ///
    /// With a shaping font the direction gate does not apply: the font's
    /// rules cover every script it supports, not just Arabic.
    pub fn shape_arabic_if_rtl(&mut self) -> bool {
        if self.shape_with_font() {
            return true;
        }
        let has_rtl_line = (0..self.line_count())
            .any(|line| self.resolved_line_direction(line) == TextDirection::RightToLeft);
        if !has_rtl_line {
            return false;
        }
        self.shape_arabic()
    }

    pub fn shape_arabic_around_if_rtl(&mut self, position: usize) -> bool {
        // Reshaping the whole buffer through the font is cheap at editor
        // sizes, and a ligature can appear or break several sorts away
        // from the one that changed.
        if self.shape_with_font() {
            return true;
        }
        let line = self.line_number_for_sort(position);
        if self.resolved_line_direction(line) != TextDirection::RightToLeft {
            return false;
        }
        self.shape_arabic_around(position)
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

    fn shape_arabic_around(&mut self, position: usize) -> bool {
        if self.sorts.is_empty() {
            return false;
        }

        let indices = self.arabic_shape_indices_around(position);
        if indices.is_empty() {
            return false;
        }

        let chars = self.glyph_chars();
        let mut updates = Vec::new();

        for index in indices {
            let Some(char) = self.sort_codepoint(index) else {
                continue;
            };
            if !shaping::is_arabic(char) {
                continue;
            }
            let char_index = self.char_index_for_sort_index(index);
            let name = self.shaped_glyph_name_for_character(char, &chars, char_index, index);
            let advance_width = self
                .glyph_inventory
                .widths
                .get(&name)
                .copied()
                .unwrap_or_else(|| self.sort_advance(index));
            updates.push((index, name, advance_width));
        }

        self.apply_shape_updates(updates)
    }

    fn arabic_shape_indices_around(&self, position: usize) -> Vec<usize> {
        let mut indices = Vec::new();

        if let Some(index) = self.previous_nontransparent_arabic_sort(position) {
            indices.push(index);
        }

        if let Some(index) = self.next_nontransparent_arabic_sort(position) {
            indices.push(index);
            if let Some(next_index) = self.next_nontransparent_arabic_sort(index + 1) {
                indices.push(next_index);
            }
        }

        indices.dedup();
        indices
    }

    fn previous_nontransparent_arabic_sort(&self, position: usize) -> Option<usize> {
        let end = position.min(self.sorts.len());
        (0..end)
            .rev()
            .find(|index| self.is_nontransparent_arabic_sort(*index))
    }

    fn next_nontransparent_arabic_sort(&self, position: usize) -> Option<usize> {
        (position..self.sorts.len()).find(|index| self.is_nontransparent_arabic_sort(*index))
    }

    fn is_nontransparent_arabic_sort(&self, index: usize) -> bool {
        self.sort_codepoint(index).is_some_and(|char| {
            shaping::is_arabic(char) && !shaping::arabic_joining_type(char).is_transparent()
        })
    }

    fn glyph_chars(&self) -> Vec<char> {
        self.sorts
            .iter()
            .filter_map(|sort| match sort.kind {
                TextSortKind::Glyph {
                    codepoint: Some(char),
                    ..
                } => Some(char),
                _ => None,
            })
            .collect()
    }

    fn char_index_for_sort_index(&self, sort_index: usize) -> usize {
        self.sorts[..sort_index]
            .iter()
            .filter(|sort| {
                matches!(
                    sort.kind,
                    TextSortKind::Glyph {
                        codepoint: Some(_),
                        ..
                    }
                )
            })
            .count()
    }

    fn apply_shape_updates(&mut self, updates: Vec<(usize, String, f64)>) -> bool {
        let mut changed = false;
        for (index, name, advance_width) in updates {
            let Some(sort) = self.sorts.get_mut(index) else {
                continue;
            };
            let TextSortKind::Glyph {
                name: glyph_name,
                advance_width: glyph_advance_width,
                ..
            } = &mut sort.kind
            else {
                continue;
            };
            if *glyph_name != name || *glyph_advance_width != advance_width {
                *glyph_name = name;
                *glyph_advance_width = advance_width;
                changed = true;
            }
        }

        changed
    }

    fn shaped_glyph_name_for_character(
        &self,
        char: char,
        line_chars: &[char],
        char_index: usize,
        sort_index: usize,
    ) -> String {
        let base_name = self
            .glyph_inventory
            .unicode
            .get(&(char as u32))
            .cloned()
            .or_else(|| self.sort_glyph_name(sort_index).map(ToOwned::to_owned))
            .unwrap_or_else(|| ".notdef".to_string());
        // Shape by the *line's* direction, not the buffer's: in Auto
        // mode an Arabic line joins even when the buffer default (or
        // another line) is left-to-right.
        let line = self.line_number_for_sort(sort_index);
        if self.resolved_line_direction(line) != TextDirection::RightToLeft
            || !shaping::is_arabic(char)
        {
            return base_name;
        }

        let suffix = shaping::arabic_positional_form(line_chars, char_index).suffix();
        let shaped_name = format!("{base_name}{suffix}");
        if !suffix.is_empty() && self.glyph_inventory.has_glyph(&shaped_name) {
            shaped_name
        } else {
            base_name
        }
    }

    // -----------------------------------------------------------------
    // Bidirectional order
    //
    // Shaping and bidi are separate jobs. HarfBuzz (and so harfrust)
    // shapes one run in one direction and says nothing about the order
    // runs go in — that is the Unicode Bidirectional Algorithm's job,
    // and the application's to run. Reversing a whole RTL line is what
    // made "this is a book" read "koob a si siht" inside an Arabic
    // sentence: the Latin is a level-2 run inside a level-1 line, and
    // only the run's *position* is mirrored, never its contents.
    // -----------------------------------------------------------------

    /// Whether the bidi run containing `index` reads right to left.
    ///
    /// This is the sort's own rendering direction, not the line's: a
    /// Latin glyph inside an Arabic sentence is in an LTR run and
    /// reports false. The metrics panel uses it to keep "left" and
    /// "right" meaning what they look like on screen.
    pub fn sort_rtl(&self, index: usize) -> bool {
        let line = self.line_number_for_sort(index);
        let (start, end) = self.line_range_for_number(line);
        self.visual_runs_for_line(start, end)
            .iter()
            .find(|run| run.sorts.contains(&index))
            .map(|run| run.rtl)
            .unwrap_or(false)
    }

    /// Sorts of one line in visual order, left to right, grouped into
    /// runs of one direction each.
    ///
    /// Returns one run per stretch of equal embedding level, in the
    /// order they appear on screen, with `rtl` set for odd levels.
    fn visual_runs_for_line(&self, line_start: usize, line_end: usize) -> Rc<Vec<VisualRun>> {
        let line_number = self.line_number_for_index(line_start);
        let base_rtl = self.resolved_line_direction(line_number) == TextDirection::RightToLeft;
        self.visual_runs_in(line_start, line_end, base_rtl)
    }

    /// The same, over any stretch of sorts with the base direction given.
    /// Line breaks inside the range are skipped, so the preview strip can
    /// treat a run of same-direction lines as one paragraph.
    fn visual_runs_in(&self, start: usize, end: usize, base_rtl: bool) -> Rc<Vec<VisualRun>> {
        let key = (start, end, base_rtl, self.bidi_fingerprint(start, end));
        if let Some(runs) = self.bidi_runs.0.borrow().get(&key) {
            return Rc::clone(runs);
        }
        let runs = Rc::new(self.compute_visual_runs(start, end, base_rtl));
        let mut cache = self.bidi_runs.0.borrow_mut();
        if cache.len() >= BIDI_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, Rc::clone(&runs));
        runs
    }

    /// What the runs for a stretch of sorts depend on: the characters
    /// themselves and where the line breaks fall.
    fn bidi_fingerprint(&self, start: usize, end: usize) -> u64 {
        let mut hasher = DefaultHasher::new();
        for index in start..end.min(self.sorts.len()) {
            match &self.sorts[index].kind {
                TextSortKind::Glyph { codepoint, .. } => codepoint.hash(&mut hasher),
                TextSortKind::LineBreak => u32::MAX.hash(&mut hasher),
            }
        }
        hasher.finish()
    }

    fn compute_visual_runs(&self, start: usize, end: usize, base_rtl: bool) -> Vec<VisualRun> {
        // Sorts with no codepoint (an unencoded glyph typed by name)
        // have no bidi class of their own. They take the paragraph's
        // direction, which is what an object replacement character does.
        const NO_CODEPOINT: char = '\u{fffc}';

        let mut text = String::new();
        let mut sort_for_offset: Vec<usize> = Vec::new();
        for index in start..end.min(self.sorts.len()) {
            if matches!(self.sorts[index].kind, TextSortKind::LineBreak) {
                continue;
            }
            let char = self.sort_codepoint(index).unwrap_or(NO_CODEPOINT);
            for _ in 0..char.len_utf8() {
                sort_for_offset.push(index);
            }
            text.push(char);
        }
        if text.is_empty() {
            return Vec::new();
        }

        let base_level = if base_rtl { Level::rtl() } else { Level::ltr() };

        let info = BidiInfo::new(&text, Some(base_level));
        let Some(paragraph) = info.paragraphs.first() else {
            return Vec::new();
        };
        let (levels, ranges) = info.visual_runs(paragraph, paragraph.range.clone());

        let mut runs: Vec<VisualRun> = Vec::new();
        for range in ranges {
            let rtl = levels[range.start].is_rtl();
            let mut sorts: Vec<usize> = Vec::new();
            for offset in range {
                let Some(&index) = sort_for_offset.get(offset) else {
                    continue;
                };
                if sorts.last() != Some(&index) {
                    sorts.push(index);
                }
            }
            // Within an RTL run the sorts are still in logical order;
            // the run as a whole reads right to left.
            if !sorts.is_empty() {
                runs.push(VisualRun::new(rtl, sorts));
            }
        }
        runs
    }

    fn line_number_for_index(&self, index: usize) -> usize {
        self.sorts[..index.min(self.sorts.len())]
            .iter()
            .filter(|sort| matches!(sort.kind, TextSortKind::LineBreak))
            .count()
    }

    fn next_line_end(&self, start: usize) -> usize {
        self.sorts[start..]
            .iter()
            .position(|sort| matches!(sort.kind, TextSortKind::LineBreak))
            .map(|offset| start + offset)
            .unwrap_or(self.sorts.len())
    }

    fn line_range_for_number(&self, line_number: usize) -> (usize, usize) {
        let mut start = 0;
        let mut current_line = 0;
        while start <= self.sorts.len() {
            let end = self.next_line_end(start);
            if current_line == line_number || end >= self.sorts.len() {
                return (start, end);
            }
            start = end + 1;
            current_line += 1;
        }
        (self.sorts.len(), self.sorts.len())
    }

    fn line_number_for_y(&self, y: f64, line_height: f64, ascender: f64, descender: f64) -> usize {
        let mut start = 0;
        let mut line_number = 0;
        let mut nearest_line = 0;
        let mut nearest_distance = f64::INFINITY;
        while start <= self.sorts.len() {
            let baseline = -line_height * line_number as f64;
            let top = baseline + ascender;
            let bottom = baseline + descender;
            if y >= bottom && y <= top {
                return line_number;
            }
            let distance = if y > top { y - top } else { bottom - y };
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest_line = line_number;
            }

            let end = self.next_line_end(start);
            if end >= self.sorts.len() {
                break;
            }
            start = end + 1;
            line_number += 1;
        }
        nearest_line
    }

    fn hit_sort_item_at(
        &self,
        x: f64,
        y: f64,
        line_height: f64,
        ascender: f64,
        descender: f64,
        layout: &TextLayout,
    ) -> Option<TextLayoutItem> {
        if self.sorts.is_empty() {
            return None;
        }

        let line_height = line_height.max(1.0);
        let target_line = self.line_number_for_y(y, line_height, ascender, descender);
        let (line_start, line_end) = self.line_range_for_number(target_line);
        for item in layout
            .items
            .iter()
            .filter(|item| (line_start..line_end).contains(&item.index))
        {
            let within_x = x >= item.x && x < item.x + item.advance_width;
            let within_y = y >= item.y + descender && y < item.y + ascender;
            if within_x && within_y {
                return Some(*item);
            }
        }
        None
    }

    fn line_width(&self, start: usize, end: usize) -> f64 {
        let mut width = 0.0;
        let mut previous_glyph_name: Option<&str> = None;
        for index in start..end {
            let glyph_name = self.sort_glyph_name(index);
            if let Some((left, right)) = previous_glyph_name.zip(glyph_name) {
                width += self.lookup_kerning(left, right);
            }
            width += self.sort_advance(index);
            previous_glyph_name = glyph_name;
        }
        width
    }

    fn nearest_cursor_for_line(
        &self,
        x: f64,
        line_start: usize,
        line_end: usize,
        layout: &TextLayout,
    ) -> usize {
        let mut nearest_cursor = line_start;
        let mut nearest_distance = f64::INFINITY;
        let line_start_x = match self.direction {
            TextDirection::LeftToRight => self.line_width(line_start, line_end),
            TextDirection::RightToLeft => self.rtl_line_start_x(),
        };

        for candidate in line_start..=line_end {
            let cursor_x = if candidate == line_start {
                match self.direction {
                    TextDirection::LeftToRight => 0.0,
                    TextDirection::RightToLeft => line_start_x,
                }
            } else {
                layout
                    .items
                    .iter()
                    .find(|item| item.index + 1 == candidate)
                    .map(|item| match self.direction {
                        TextDirection::LeftToRight => item.x + item.advance_width,
                        TextDirection::RightToLeft => item.x,
                    })
                    .unwrap_or(0.0)
            };
            let distance = (x - cursor_x).abs();
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest_cursor = candidate;
            }
        }

        nearest_cursor
    }

    /// Where an RTL line begins: the widest line in the buffer, so
    /// every RTL line ends up sharing the same right edge. (Xilem summed
    /// the whole buffer, which is the same number for the single-line
    /// case and too wide once lines stack.)
    fn rtl_line_start_x(&self) -> f64 {
        let mut widest: f64 = 0.0;
        let mut line_start = 0;
        while line_start <= self.sorts.len() {
            let line_end = self.next_line_end(line_start);
            let width: f64 = (line_start..line_end).map(|i| self.sort_advance(i)).sum();
            widest = widest.max(width);
            if line_end >= self.sorts.len() {
                break;
            }
            line_start = line_end + 1;
        }
        widest
    }

    fn sort_advance(&self, index: usize) -> f64 {
        match &self.sorts[index].kind {
            TextSortKind::Glyph { advance_width, .. } => *advance_width,
            TextSortKind::LineBreak => 0.0,
        }
    }

    fn sort_glyph_name(&self, index: usize) -> Option<&str> {
        match &self.sorts[index].kind {
            TextSortKind::Glyph { name, .. } => Some(name),
            TextSortKind::LineBreak => None,
        }
    }

    fn sort_codepoint(&self, index: usize) -> Option<char> {
        match &self.sorts[index].kind {
            TextSortKind::Glyph { codepoint, .. } => *codepoint,
            TextSortKind::LineBreak => None,
        }
    }

    fn glyph_pair_names(&self, sort_index: usize) -> Option<(String, String)> {
        let left = self.sort_glyph_name(sort_index.checked_sub(1)?)?;
        let right = self.sort_glyph_name(sort_index)?;
        Some((left.to_string(), right.to_string()))
    }

    fn lookup_kerning(&self, left: &str, right: &str) -> f64 {
        lookup_xilem_kerning(
            &self.kerning.kerning,
            &self.kerning.groups,
            left,
            self.kerning.right_groups.get(left).map(String::as_str),
            right,
            self.kerning.left_groups.get(right).map(String::as_str),
        )
    }

    fn set_direct_kerning(&mut self, left: &str, right: &str, value: f64) {
        if value == 0.0 {
            if let Some(pairs) = self.kerning.kerning.get_mut(left) {
                pairs.remove(right);
                if pairs.is_empty() {
                    self.kerning.kerning.remove(left);
                }
            }
            return;
        }
        self.kerning
            .kerning
            .entry(left.to_string())
            .or_default()
            .insert(right.to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Typing after a seeded active sort must lay out to the right
    /// of it, active index stable (regression for a GPUI overlap).
    #[test]
    fn typing_after_active_sort_lays_out_beside_it() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../runebender-web/assets/test-fonts/VirtuaGrotesk-Regular.ufo"
        );
        let font = norad::Font::load(path).expect("fixture UFO loads");
        let mut buffer = TextBuffer::new();
        buffer.set_glyph_inventory(TextGlyphInventory::from_font(&font));
        buffer.set_kerning_model(TextKerningModel::from_font(&font));

        let one = font.get_glyph("one").unwrap();
        buffer.insert_glyph("one", Some('1'), one.width);
        buffer.activate_sort(0);
        assert_eq!(buffer.cursor(), 1);

        assert!(buffer.insert_character('a'));
        assert!(buffer.insert_character('s'));

        assert_eq!(buffer.active_sort(), Some(0), "active sort must stay");
        let layout = buffer.layout(1200.0);
        assert_eq!(layout.items.len(), 3);
        let one_item = layout.items.iter().find(|i| i.index == 0).unwrap();
        let a_item = layout.items.iter().find(|i| i.index == 1).unwrap();
        let s_item = layout.items.iter().find(|i| i.index == 2).unwrap();
        assert_eq!(one_item.x, 0.0);
        let kern_one_a = crate::glyph_ops::kern_value(&font, "one", "a");
        assert!(
            (a_item.x - (one.width + kern_one_a)).abs() < 1e-6,
            "a at {} expected {}",
            a_item.x,
            one.width + kern_one_a
        );
        assert!(a_item.x >= one.width - 100.0, "a must not overlap one");
        assert!(s_item.x > a_item.x);
    }

    /// The native constructors must agree with the norad-level
    /// kerning resolution the editors already use (glyph_ops).
    #[test]
    fn from_font_builds_working_models() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../runebender-web/assets/test-fonts/VirtuaGrotesk-Regular.ufo"
        );
        let font = norad::Font::load(path).expect("fixture UFO loads");
        let inventory = TextGlyphInventory::from_font(&font);
        let kerning = TextKerningModel::from_font(&font);

        let mut buffer = TextBuffer::new();
        buffer.set_glyph_inventory(inventory);
        buffer.set_kerning_model(kerning);

        // Characters resolve to glyphs with real advances.
        assert!(buffer.insert_character('A'));
        assert!(buffer.insert_character('B'));
        let a_width = font.get_glyph("A").unwrap().width;
        let layout = buffer.layout(1200.0);
        assert_eq!(layout.items.len(), 2);
        assert_eq!(layout.items[0].advance_width, a_width);

        // Find an encoded pair with a nonzero kern and check the
        // buffer applies exactly what glyph_ops resolves.
        let mut checked = false;
        'outer: for glyph in font.default_layer().iter() {
            let Some(lc) = glyph.codepoints.iter().next() else {
                continue;
            };
            for other in font.default_layer().iter() {
                let Some(rc) = other.codepoints.iter().next() else {
                    continue;
                };
                let expected = crate::glyph_ops::kern_value(
                    &font,
                    glyph.name().as_str(),
                    other.name().as_str(),
                );
                if expected == 0.0 {
                    continue;
                }
                let mut b = TextBuffer::new();
                b.set_glyph_inventory(TextGlyphInventory::from_font(&font));
                b.set_kerning_model(TextKerningModel::from_font(&font));
                assert!(b.insert_character(lc));
                assert!(b.insert_character(rc));
                let l = b.layout(1200.0);
                let gap = l.items[1].x - l.items[0].x - l.items[0].advance_width;
                assert!(
                    (gap - expected).abs() < 1e-6,
                    "{}/{}: gap {gap} expected {expected}",
                    glyph.name(),
                    other.name()
                );
                checked = true;
                break 'outer;
            }
        }
        assert!(checked, "fixture font has no kerned encoded pair");
    }

    #[test]
    fn insert_glyph_moves_cursor_and_sets_active_sort() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 600.0);
        buffer.insert_glyph("B", Some('B'), 610.0);

        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.cursor(), 2);
        assert_eq!(buffer.active_sort(), Some(1));
        assert_eq!(
            buffer.iter().last().and_then(TextSort::glyph_name),
            Some("B")
        );
    }

    #[test]
    fn insert_inactive_glyph_preserves_active_sort() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 600.0);
        buffer.set_cursor(0);
        buffer.insert_inactive_glyph("B", Some('B'), 610.0);

        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.cursor(), 1);
        assert_eq!(buffer.active_sort(), Some(1));
        assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("B"));
        assert_eq!(buffer.sort(1).and_then(TextSort::glyph_name), Some("A"));
    }

    #[test]
    fn activate_sort_preserves_cursor_position() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 600.0);
        buffer.insert_glyph("B", Some('B'), 610.0);
        buffer.set_cursor(0);

        assert!(buffer.activate_sort(1));

        assert_eq!(buffer.active_sort(), Some(1));
        assert_eq!(buffer.cursor(), 0);
    }

    #[test]
    fn active_sort_flags_remain_unique_after_switch_and_insert() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 600.0);
        buffer.insert_glyph("B", Some('B'), 610.0);
        buffer.insert_glyph("C", Some('C'), 620.0);

        assert!(buffer.activate_sort(0));
        assert_eq!(
            buffer
                .iter()
                .enumerate()
                .filter_map(|(index, sort)| sort.active.then_some(index))
                .collect::<Vec<_>>(),
            vec![0]
        );

        assert!(buffer.activate_sort(2));
        assert_eq!(
            buffer
                .iter()
                .enumerate()
                .filter_map(|(index, sort)| sort.active.then_some(index))
                .collect::<Vec<_>>(),
            vec![2]
        );

        buffer.set_cursor(0);
        buffer.insert_glyph("D", Some('D'), 630.0);
        assert_eq!(
            buffer
                .iter()
                .enumerate()
                .filter_map(|(index, sort)| sort.active.then_some(index))
                .collect::<Vec<_>>(),
            vec![0]
        );
    }

    #[test]
    fn insert_character_uses_glyph_inventory() {
        let mut buffer = TextBuffer::new();
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": { "65": "A" },
                    "widths": { "A": 640 }
                }"#,
            )
            .expect("valid glyph inventory"),
        );

        assert!(buffer.insert_character('A'));
        assert!(!buffer.insert_character('Z'));

        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.cursor(), 1);
        assert_eq!(buffer.active_sort(), None);
        assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("A"));
        let TextSortKind::Glyph {
            codepoint,
            advance_width,
            ..
        } = &buffer.sort(0).expect("sort exists").kind
        else {
            panic!("expected glyph sort");
        };
        assert_eq!(*codepoint, Some('A'));
        assert_eq!(*advance_width, 640.0);
    }

    #[test]
    fn insert_character_missing_width_uses_xilem_shaper_fallback() {
        let mut buffer = TextBuffer::new();
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": { "65": "A" },
                    "outlines": { "A": "M0,0 L10,0" }
                }"#,
            )
            .expect("valid glyph inventory"),
        );

        assert!(buffer.insert_character('A'));

        let TextSortKind::Glyph { advance_width, .. } = &buffer.sort(0).expect("sort exists").kind
        else {
            panic!("expected glyph sort");
        };
        assert_eq!(*advance_width, 500.0);
    }

    #[test]
    fn clear_resets_direction_like_fresh_xilem_session() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.insert_glyph("A", Some('A'), 600.0);

        buffer.clear();

        assert_eq!(buffer.direction(), TextDirection::LeftToRight);
        assert_eq!(buffer.len(), 0);
        assert_eq!(buffer.cursor(), 0);
        assert_eq!(buffer.active_sort(), None);
    }

    #[test]
    fn auto_direction_shapes_arabic_without_pinning_rtl() {
        let mut buffer = TextBuffer::new();
        // No set_direction call: Auto mode must shape Arabic on its own.
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": {
                        "1576": "beh-ar",
                        "1605": "meem-ar"
                    },
                    "widths": {
                        "beh-ar": 500,
                        "beh-ar.init": 480,
                        "meem-ar": 520,
                        "meem-ar.fina": 500
                    }
                }"#,
            )
            .expect("valid glyph inventory"),
        );

        assert!(buffer.insert_character('\u{0628}'));
        assert!(buffer.insert_character('\u{0645}'));

        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar.init")
        );
        assert_eq!(
            buffer.sort(1).and_then(TextSort::glyph_name),
            Some("meem-ar.fina")
        );
    }

    #[test]
    fn insert_character_shapes_rtl_arabic_neighbors() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": {
                        "1576": "beh-ar",
                        "1605": "meem-ar"
                    },
                    "widths": {
                        "beh-ar": 500,
                        "beh-ar.init": 480,
                        "meem-ar": 520,
                        "meem-ar.fina": 500
                    }
                }"#,
            )
            .expect("valid glyph inventory"),
        );

        assert!(buffer.insert_character('\u{0628}'));
        assert!(buffer.insert_character('\u{0645}'));

        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar.init")
        );
        assert_eq!(
            buffer.sort(1).and_then(TextSort::glyph_name),
            Some("meem-ar.fina")
        );
    }

    #[test]
    fn rtl_arabic_shaping_context_crosses_line_breaks_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": {
                        "1576": "beh-ar",
                        "1607": "heh-ar"
                    },
                    "widths": {
                        "beh-ar": 500,
                        "heh-ar": 510,
                        "heh-ar.fina": 490
                    }
                }"#,
            )
            .expect("valid glyph inventory"),
        );

        assert!(buffer.insert_character('\u{0628}'));
        buffer.insert_line_break();
        assert!(buffer.insert_character('\u{0647}'));

        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar")
        );
        assert!(matches!(
            buffer.sort(1).map(|sort| &sort.kind),
            Some(TextSortKind::LineBreak)
        ));
        assert_eq!(
            buffer.sort(2).and_then(TextSort::glyph_name),
            Some("heh-ar.fina")
        );
    }

    #[test]
    fn rtl_arabic_insert_after_transparent_mark_reshapes_previous_letter() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": {
                        "1576": "beh-ar",
                        "1614": "fatha-ar",
                        "1607": "heh-ar"
                    },
                    "widths": {
                        "beh-ar": 500,
                        "beh-ar.init": 480,
                        "fatha-ar": 0,
                        "heh-ar": 510,
                        "heh-ar.fina": 490
                    }
                }"#,
            )
            .expect("valid glyph inventory"),
        );

        assert!(buffer.insert_character('\u{0628}'));
        assert!(buffer.insert_character('\u{064e}'));
        assert!(buffer.insert_character('\u{0647}'));

        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar.init")
        );
        assert_eq!(
            buffer.sort(1).and_then(TextSort::glyph_name),
            Some("fatha-ar")
        );
        assert_eq!(
            buffer.sort(2).and_then(TextSort::glyph_name),
            Some("heh-ar.fina")
        );
    }

    #[test]
    fn rtl_arabic_tatweel_joins_neighbors_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": {
                        "1576": "beh-ar",
                        "1600": "tatweel-ar",
                        "1607": "heh-ar"
                    },
                    "widths": {
                        "beh-ar": 500,
                        "beh-ar.init": 480,
                        "tatweel-ar": 250,
                        "heh-ar": 510,
                        "heh-ar.fina": 490
                    }
                }"#,
            )
            .expect("valid glyph inventory"),
        );

        assert!(buffer.insert_character('\u{0628}'));
        assert!(buffer.insert_character('\u{0640}'));
        assert!(buffer.insert_character('\u{0647}'));

        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar.init")
        );
        assert_eq!(
            buffer.sort(1).and_then(TextSort::glyph_name),
            Some("tatweel-ar")
        );
        assert_eq!(
            buffer.sort(2).and_then(TextSort::glyph_name),
            Some("heh-ar.fina")
        );
    }

    #[test]
    fn rtl_arabic_positional_glyph_can_exist_without_width_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": {
                        "1576": "beh-ar",
                        "1607": "heh-ar"
                    },
                    "widths": {
                        "beh-ar": 500,
                        "heh-ar": 510,
                        "heh-ar.fina": 490
                    },
                    "outlines": {
                        "beh-ar.init": "M0,0 L10,0"
                    }
                }"#,
            )
            .expect("valid glyph inventory"),
        );

        assert!(buffer.insert_character('\u{0628}'));
        assert!(buffer.insert_character('\u{0647}'));

        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar.init")
        );
        assert_eq!(
            buffer.sort(1).and_then(TextSort::glyph_name),
            Some("heh-ar.fina")
        );
    }

    #[test]
    fn rtl_arabic_delete_transparent_mark_repairs_joining_neighbors() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": {
                        "1576": "beh-ar",
                        "1614": "fatha-ar",
                        "1607": "heh-ar"
                    },
                    "widths": {
                        "beh-ar": 500,
                        "beh-ar.init": 480,
                        "fatha-ar": 0,
                        "heh-ar": 510,
                        "heh-ar.fina": 490
                    }
                }"#,
            )
            .expect("valid glyph inventory"),
        );

        buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
        buffer.insert_glyph("fatha-ar", Some('\u{064e}'), 0.0);
        buffer.insert_glyph("heh-ar", Some('\u{0647}'), 510.0);
        buffer.set_cursor(2);

        assert!(buffer.delete_before_cursor().is_some());
        assert!(buffer.shape_arabic_around_if_rtl(buffer.cursor()));

        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar.init")
        );
        assert_eq!(
            buffer.sort(1).and_then(TextSort::glyph_name),
            Some("heh-ar.fina")
        );
    }

    #[test]
    fn rtl_arabic_insert_right_joining_sort_reshapes_next_letter() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": {
                        "1576": "beh-ar",
                        "1575": "alef-ar",
                        "1607": "heh-ar"
                    },
                    "widths": {
                        "beh-ar": 500,
                        "beh-ar.init": 480,
                        "alef-ar": 450,
                        "alef-ar.fina": 430,
                        "heh-ar": 510,
                        "heh-ar.fina": 490
                    }
                }"#,
            )
            .expect("valid glyph inventory"),
        );

        assert!(buffer.insert_character('\u{0628}'));
        assert!(buffer.insert_character('\u{0647}'));
        buffer.set_cursor(1);
        assert!(buffer.insert_character('\u{0627}'));

        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar.init")
        );
        assert_eq!(
            buffer.sort(1).and_then(TextSort::glyph_name),
            Some("alef-ar.fina")
        );
        assert_eq!(
            buffer.sort(2).and_then(TextSort::glyph_name),
            Some("heh-ar")
        );
    }

    #[test]
    fn rtl_arabic_insert_latin_separator_breaks_joining_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": {
                        "65": "A",
                        "1576": "beh-ar",
                        "1605": "meem-ar"
                    },
                    "widths": {
                        "A": 700,
                        "beh-ar": 500,
                        "beh-ar.init": 480,
                        "meem-ar": 520,
                        "meem-ar.fina": 500
                    }
                }"#,
            )
            .expect("valid glyph inventory"),
        );

        assert!(buffer.insert_character('\u{0628}'));
        assert!(buffer.insert_character('\u{0645}'));
        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar.init")
        );
        assert_eq!(
            buffer.sort(1).and_then(TextSort::glyph_name),
            Some("meem-ar.fina")
        );

        buffer.set_cursor(1);
        assert!(buffer.insert_character('A'));

        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar")
        );
        assert_eq!(buffer.sort(1).and_then(TextSort::glyph_name), Some("A"));
        assert_eq!(
            buffer.sort(2).and_then(TextSort::glyph_name),
            Some("meem-ar")
        );
    }

    #[test]
    fn rtl_arabic_delete_latin_separator_repairs_joining_neighbors() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": {
                        "65": "A",
                        "1576": "beh-ar",
                        "1605": "meem-ar"
                    },
                    "widths": {
                        "A": 700,
                        "beh-ar": 500,
                        "beh-ar.init": 480,
                        "meem-ar": 520,
                        "meem-ar.fina": 500
                    }
                }"#,
            )
            .expect("valid glyph inventory"),
        );

        assert!(buffer.insert_character('\u{0628}'));
        assert!(buffer.insert_character('A'));
        assert!(buffer.insert_character('\u{0645}'));
        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar")
        );
        assert_eq!(
            buffer.sort(2).and_then(TextSort::glyph_name),
            Some("meem-ar")
        );

        buffer.set_cursor(2);
        assert!(buffer.delete_before_cursor().is_some());
        assert!(buffer.shape_arabic_around_if_rtl(buffer.cursor()));

        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar.init")
        );
        assert_eq!(
            buffer.sort(1).and_then(TextSort::glyph_name),
            Some("meem-ar.fina")
        );
    }

    #[test]
    fn insert_character_ltr_preserves_existing_shaped_forms() {
        let mut buffer = TextBuffer::new();
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": {
                        "65": "A",
                        "1576": "beh-ar",
                        "1607": "heh-ar"
                    },
                    "widths": {
                        "A": 700,
                        "beh-ar": 500,
                        "beh-ar.init": 480,
                        "heh-ar": 510,
                        "heh-ar.fina": 490
                    }
                }"#,
            )
            .expect("valid glyph inventory"),
        );
        buffer.set_direction(TextDirection::RightToLeft);
        assert!(buffer.insert_character('\u{0628}'));
        assert!(buffer.insert_character('\u{0647}'));
        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar.init")
        );
        assert_eq!(
            buffer.sort(1).and_then(TextSort::glyph_name),
            Some("heh-ar.fina")
        );

        buffer.set_direction(TextDirection::LeftToRight);
        assert!(buffer.insert_character('A'));

        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar.init")
        );
        assert_eq!(
            buffer.sort(1).and_then(TextSort::glyph_name),
            Some("heh-ar.fina")
        );
        assert_eq!(buffer.sort(2).and_then(TextSort::glyph_name), Some("A"));
    }

    #[test]
    fn delete_before_cursor_updates_active_sort() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 600.0);
        buffer.insert_glyph("B", Some('B'), 610.0);
        buffer.activate_sort(1);
        buffer.set_cursor(1);

        let deleted = buffer.delete_before_cursor();

        assert_eq!(deleted.as_ref().and_then(TextSort::glyph_name), Some("A"));
        assert_eq!(buffer.cursor(), 0);
        assert_eq!(buffer.active_sort(), Some(0));
    }

    #[test]
    fn delete_before_cursor_clears_deleted_active_sort_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 600.0);
        buffer.insert_glyph("B", Some('B'), 610.0);
        buffer.insert_glyph("C", Some('C'), 620.0);
        buffer.activate_sort(1);
        buffer.set_cursor(2);

        let deleted = buffer.delete_before_cursor();

        assert_eq!(deleted.as_ref().and_then(TextSort::glyph_name), Some("B"));
        assert_eq!(buffer.cursor(), 1);
        assert_eq!(buffer.active_sort(), None);
        assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("A"));
        assert_eq!(buffer.sort(1).and_then(TextSort::glyph_name), Some("C"));
        assert!(!buffer.iter().any(|sort| sort.active));
    }

    #[test]
    fn delete_after_cursor_clears_deleted_active_sort_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 600.0);
        buffer.insert_glyph("B", Some('B'), 610.0);
        buffer.insert_glyph("C", Some('C'), 620.0);
        buffer.activate_sort(1);
        buffer.set_cursor(1);

        let deleted = buffer.delete_after_cursor();

        assert_eq!(deleted.as_ref().and_then(TextSort::glyph_name), Some("B"));
        assert_eq!(buffer.cursor(), 1);
        assert_eq!(buffer.active_sort(), None);
        assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("A"));
        assert_eq!(buffer.sort(1).and_then(TextSort::glyph_name), Some("C"));
        assert!(!buffer.iter().any(|sort| sort.active));
    }

    #[test]
    fn line_break_preserves_active_sort() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 600.0);
        buffer.insert_line_break();

        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.cursor(), 2);
        assert_eq!(buffer.active_sort(), Some(0));
    }

    #[test]
    fn line_break_before_active_shifts_active_sort_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 600.0);
        buffer.insert_glyph("B", Some('B'), 610.0);
        buffer.activate_sort(1);
        buffer.set_cursor(1);

        buffer.insert_line_break();

        assert_eq!(buffer.cursor(), 2);
        assert_eq!(buffer.active_sort(), Some(2));
        assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("A"));
        assert!(matches!(
            buffer.sort(1).map(|sort| &sort.kind),
            Some(TextSortKind::LineBreak)
        ));
        assert_eq!(buffer.sort(2).and_then(TextSort::glyph_name), Some("B"));
        assert!(buffer.sort(2).is_some_and(|sort| sort.active));
    }

    #[test]
    fn typed_sort_before_active_shifts_active_sort() {
        let mut buffer = TextBuffer::new();
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": { "65": "A", "66": "B" },
                    "widths": { "A": 640, "B": 650 }
                }"#,
            )
            .expect("valid glyph inventory"),
        );
        buffer.insert_glyph("B", Some('B'), 650.0);
        buffer.set_cursor(0);

        assert!(buffer.insert_character('A'));

        assert_eq!(buffer.cursor(), 1);
        assert_eq!(buffer.active_sort(), Some(1));
        assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("A"));
        assert_eq!(buffer.sort(1).and_then(TextSort::glyph_name), Some("B"));
    }

    #[test]
    fn visual_cursor_movement_respects_direction() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 600.0);
        buffer.insert_glyph("B", Some('B'), 600.0);

        buffer.move_cursor_visual_left();
        assert_eq!(buffer.cursor(), 1);

        buffer.set_direction(TextDirection::RightToLeft);
        buffer.move_cursor_visual_left();
        assert_eq!(buffer.cursor(), 2);
        buffer.move_cursor_visual_right();
        assert_eq!(buffer.cursor(), 1);
    }

    #[test]
    fn hit_test_activates_clicked_ltr_sort() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("B", Some('B'), 500.0);

        let hit = buffer.hit_test(650.0, 200.0, 1000.0, 800.0, -200.0);

        assert_eq!(hit.active_sort, Some(1));
        assert_eq!(hit.cursor, 2);
    }

    #[test]
    fn hit_test_rejects_sort_above_ascender() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("B", Some('B'), 500.0);

        let hit = buffer.hit_test(650.0, 900.0, 1000.0, 800.0, -200.0);

        assert_eq!(hit.active_sort, None);
        assert_eq!(hit.cursor, 1);
    }

    /// Three lines of two glyphs each, 500 units wide, 1000-unit lines.
    fn three_line_buffer() -> TextBuffer {
        let mut buffer = TextBuffer::new();
        for line in 0..3 {
            if line > 0 {
                buffer.insert_line_break();
            }
            buffer.insert_glyph("A", Some('A'), 500.0);
            buffer.insert_glyph("B", Some('B'), 500.0);
        }
        buffer
    }

    #[test]
    fn cursor_moves_up_and_down_between_lines() {
        let mut buffer = three_line_buffer();
        // Caret sits after the last glyph of the last line.
        assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 2);

        assert!(buffer.move_cursor_vertically(-1, 1000.0));
        assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 1);
        assert!(buffer.move_cursor_vertically(-1, 1000.0));
        assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 0);

        assert!(buffer.move_cursor_vertically(1, 1000.0));
        assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 1);
    }

    #[test]
    fn cursor_keeps_its_column_when_changing_line() {
        let mut buffer = three_line_buffer();
        // Between the two glyphs of the bottom line.
        buffer.set_cursor(7);
        assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 2);

        assert!(buffer.move_cursor_vertically(-1, 1000.0));
        // Same offset into the line above, not its start or end.
        let (line_start, _) = buffer.line_range_for_number(1);
        assert_eq!(buffer.cursor(), line_start + 1);
    }

    #[test]
    fn cursor_stops_at_the_first_and_last_line() {
        let mut buffer = three_line_buffer();
        assert!(!buffer.move_cursor_vertically(1, 1000.0));
        buffer.set_cursor(0);
        assert!(!buffer.move_cursor_vertically(-1, 1000.0));
    }

    #[test]
    fn home_and_end_move_within_the_caret_line() {
        let mut buffer = three_line_buffer();
        // Sorts are [A B ↵ A B ↵ A B], so the middle line spans 3..5.
        buffer.set_cursor(4); // between the middle line's two glyphs
        assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 1);

        buffer.move_cursor_to_line_edge(true);
        assert_eq!(buffer.cursor(), 5);
        assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 1);

        buffer.move_cursor_to_line_edge(false);
        assert_eq!(buffer.cursor(), 3);
    }

    #[test]
    fn click_places_the_caret_between_sorts() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("B", Some('B'), 500.0);

        // Left half of the first glyph: before it.
        assert_eq!(buffer.place_cursor_at(100.0, 0.0, 1000.0, 800.0, -200.0), 0);
        // Right half of the first glyph: between the two.
        assert_eq!(buffer.place_cursor_at(400.0, 0.0, 1000.0, 800.0, -200.0), 1);
        // Past the end of the run: after the last glyph.
        assert_eq!(
            buffer.place_cursor_at(2000.0, 0.0, 1000.0, 800.0, -200.0),
            2
        );
    }

    #[test]
    fn click_places_the_caret_on_the_clicked_line() {
        let mut buffer = three_line_buffer();

        // Middle line sits one line-height below the first.
        let cursor = buffer.place_cursor_at(100.0, -1000.0, 1000.0, 800.0, -200.0);
        assert_eq!(buffer.line_number_for_sort(cursor), 1);
    }

    /// A buffer wired to the bundled font's inventory and features, the
    /// way the editor sets one up.
    fn buffer_with_shaping_font() -> TextBuffer {
        let ufo_dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../runebender-web/assets/test-fonts/VirtuaGrotesk-Regular.ufo"
        );
        let font = norad::Font::load(ufo_dir).expect("test UFO loads");
        let features =
            std::fs::read_to_string(format!("{ufo_dir}/features.fea")).expect("features.fea");

        let mut widths = HashMap::new();
        let mut unicode = HashMap::new();
        for glyph in font.layers.default_layer().iter() {
            widths.insert(glyph.name().to_string(), glyph.width);
            for codepoint in glyph.codepoints.iter() {
                unicode.insert(codepoint as u32, glyph.name().to_string());
            }
        }

        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.set_glyph_inventory(TextGlyphInventory {
            unicode,
            widths,
            outlines: HashMap::new(),
            features,
            units_per_em: 1000.0,
        });
        buffer
    }

    fn type_chars(buffer: &mut TextBuffer, text: &str) {
        for char in text.chars() {
            let name = buffer
                .glyph_inventory
                .unicode
                .get(&(char as u32))
                .cloned()
                .unwrap_or_else(|| ".notdef".to_string());
            let width = buffer
                .glyph_inventory
                .widths
                .get(&name)
                .copied()
                .unwrap_or(0.0);
            buffer.insert_glyph(name, Some(char), width);
        }
    }

    #[test]
    fn latin_keeps_one_glyph_per_character_when_shaped_by_the_font() {
        let mut buffer = buffer_with_shaping_font();
        buffer.set_direction(TextDirection::LeftToRight);
        type_chars(&mut buffer, "Runebender.org");

        buffer.shape_arabic();

        assert_eq!(buffer.len(), 14);
        let absorbed = (0..buffer.len())
            .filter(|i| buffer.sort(*i).expect("sort").is_absorbed())
            .count();
        assert_eq!(absorbed, 0, "no Latin character should be folded away");
        assert_eq!(buffer.layout(1000.0).items.len(), 14);
        assert_eq!(buffer.sort_glyph_name(0), Some("R"));
    }

    #[test]
    fn arabic_in_a_latin_line_still_ligates() {
        // A line whose first strong character is Latin still reads LTR,
        // but the Arabic inside it has to be shaped as its own run or
        // the script-specific features never run.
        let mut buffer = buffer_with_shaping_font();
        buffer.set_auto_direction();
        type_chars(&mut buffer, "hi \u{0644}\u{0627}");

        buffer.shape_arabic();

        assert_eq!(buffer.sort_glyph_name(3), Some("lam_alef-ar"));
        assert!(buffer.sort(4).expect("alef sort").is_absorbed());
    }

    #[test]
    fn a_glyph_opens_beside_the_one_being_edited() {
        // Double-clicking a component puts its base next to the glyph
        // that uses it, wherever the cursor happens to be.
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("B", Some('B'), 500.0);
        buffer.insert_glyph("C", Some('C'), 500.0);
        buffer.activate_sort(0);
        buffer.set_cursor(3); // cursor parked at the end

        let index = buffer.insert_glyph_after_active("acutecomb", None, 0.0);

        assert_eq!(index, 1);
        assert_eq!(buffer.sort_glyph_name(1), Some("acutecomb"));
        // ...and it is what gets edited.
        assert_eq!(buffer.active_sort(), Some(1));
        assert_eq!(buffer.cursor(), 2);
    }

    #[test]
    fn a_glyph_opens_at_the_cursor_when_nothing_is_active() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("B", Some('B'), 500.0);
        buffer.set_active_sort(None);
        buffer.set_cursor(1);

        assert_eq!(buffer.insert_glyph_after_active("C", Some('C'), 500.0), 1);
        assert_eq!(buffer.sort_glyph_name(1), Some("C"));
    }

    #[test]
    fn lam_alef_renders_as_one_ligature_glyph() {
        let mut buffer = buffer_with_shaping_font();
        type_chars(&mut buffer, "\u{0644}\u{0627}");

        assert!(buffer.shape_arabic(), "shaping changed the buffer");
        assert_eq!(buffer.sort_glyph_name(0), Some("lam_alef-ar"));
        // The alef keeps its place in the buffer — the cursor and editing
        // still see two characters — but draws nothing.
        assert_eq!(buffer.len(), 2);
        assert!(buffer.sort(1).expect("alef sort").is_absorbed());

        // One glyph on the line, and it is the ligature.
        let layout = buffer.layout(1000.0);
        assert_eq!(layout.items.len(), 1);
        assert_eq!(layout.items[0].index, 0);
    }

    #[test]
    fn deleting_the_lam_brings_the_alef_back() {
        let mut buffer = buffer_with_shaping_font();
        type_chars(&mut buffer, "\u{0644}\u{0627}");
        buffer.shape_arabic();

        buffer.set_cursor(1);
        buffer.delete_before_cursor();
        buffer.shape_arabic();

        assert_eq!(buffer.len(), 1);
        assert!(!buffer.sort(0).expect("alef sort").is_absorbed());
        assert_eq!(buffer.sort_glyph_name(0), Some("alef-ar"));
        assert_eq!(buffer.layout(1000.0).items.len(), 1);
    }

    #[test]
    fn shaping_falls_back_when_the_feature_file_is_broken() {
        let mut buffer = buffer_with_shaping_font();
        let mut inventory = buffer.glyph_inventory.clone();
        inventory.features = "feature liga { sub missing by alsoMissing; } liga;".into();
        buffer.set_glyph_inventory(inventory);
        type_chars(&mut buffer, "\u{0628}\u{0628}");

        // The built-in joining rules still run, so the text stays shaped.
        assert!(buffer.shape_arabic());
        assert_eq!(buffer.sort_glyph_name(0), Some("beh-ar.init"));
        assert_eq!(buffer.sort_glyph_name(1), Some("beh-ar.fina"));
    }

    #[test]
    fn hit_test_places_ltr_cursor_nearest_boundary() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("B", Some('B'), 500.0);

        let hit = buffer.hit_test(20.0, 1200.0, 1000.0, 800.0, -200.0);

        assert_eq!(hit.active_sort, None);
        assert_eq!(hit.cursor, 0);
    }

    #[test]
    fn hit_test_uses_xilem_exclusive_sort_max_edges() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("B", Some('B'), 300.0);

        let boundary = buffer.hit_test(500.0, 100.0, 1000.0, 800.0, -200.0);
        assert_eq!(boundary.active_sort, Some(1));
        assert_eq!(boundary.cursor, 2);

        let top_edge = buffer.hit_test(250.0, 800.0, 1000.0, 800.0, -200.0);
        assert_eq!(top_edge.active_sort, None);
        assert_eq!(top_edge.cursor, 0);
    }

    #[test]
    fn hit_test_uses_metric_box_for_ltr_line_selection() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_line_break();
        buffer.insert_glyph("B", Some('B'), 500.0);

        let hit = buffer.hit_test(250.0, -300.0, 1000.0, 800.0, -200.0);

        assert_eq!(hit.active_sort, Some(2));
        assert_eq!(hit.cursor, 3);
    }

    #[test]
    fn hit_test_uses_rtl_visual_cursor_positions() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("B", Some('B'), 500.0);

        let hit = buffer.hit_test(980.0, -1200.0, 1000.0, 800.0, -200.0);

        assert_eq!(hit.active_sort, None);
        assert_eq!(hit.cursor, 0);
    }

    #[test]
    fn hit_test_uses_metric_box_for_rtl_line_selection() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_line_break();
        buffer.insert_glyph("B", Some('B'), 500.0);

        let hit = buffer.hit_test(250.0, -300.0, 1000.0, 800.0, -200.0);

        assert_eq!(hit.active_sort, Some(2));
        assert_eq!(hit.cursor, 3);
    }

    #[test]
    fn activate_sort_at_returns_layout_origin_for_active_sort() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_line_break();
        buffer.insert_glyph("B", Some('B'), 300.0);
        buffer.set_cursor(0);

        let activation = buffer
            .activate_sort_at(300.0, -300.0, 1000.0, 800.0, -200.0)
            .expect("sort activates");

        assert_eq!(activation.index, 2);
        assert_eq!(activation.x, 200.0);
        assert_eq!(activation.y, -1000.0);
        assert_eq!(buffer.active_sort(), Some(2));
        assert_eq!(buffer.cursor(), 0);
    }

    #[test]
    fn update_glyph_changes_existing_sort_metadata() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);

        assert!(buffer.update_glyph(0, "beh-ar.init", Some('\u{0628}'), 480.0));
        let sort = buffer.sort(0).expect("sort exists");
        assert_eq!(sort.glyph_name(), Some("beh-ar.init"));
        let TextSortKind::Glyph { advance_width, .. } = sort.kind else {
            panic!("expected glyph sort");
        };
        assert_eq!(advance_width, 480.0);
    }

    #[test]
    fn shape_arabic_uses_positional_forms_when_available() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": {
                        "1576": "beh-ar",
                        "1607": "heh-ar"
                    },
                    "widths": {
                        "beh-ar": 500,
                        "beh-ar.init": 480,
                        "heh-ar": 510,
                        "heh-ar.fina": 490
                    }
                }"#,
            )
            .expect("valid glyph inventory"),
        );
        buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
        buffer.insert_glyph("heh-ar", Some('\u{0647}'), 510.0);

        assert!(buffer.shape_arabic());

        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar.init")
        );
        assert_eq!(
            buffer.sort(1).and_then(TextSort::glyph_name),
            Some("heh-ar.fina")
        );
    }

    #[test]
    fn shape_arabic_resets_to_base_forms_in_ltr() {
        let mut buffer = TextBuffer::new();
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": {
                        "1576": "beh-ar"
                    },
                    "widths": {
                        "beh-ar": 500,
                        "beh-ar.init": 480
                    }
                }"#,
            )
            .expect("valid glyph inventory"),
        );
        buffer.insert_glyph("beh-ar.init", Some('\u{0628}'), 480.0);

        assert!(buffer.shape_arabic());

        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar")
        );
    }

    #[test]
    fn set_direction_does_not_reshape_existing_sorts_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": {
                        "1576": "beh-ar",
                        "1607": "heh-ar"
                    },
                    "widths": {
                        "beh-ar": 500,
                        "beh-ar.init": 480,
                        "heh-ar": 510,
                        "heh-ar.fina": 490
                    }
                }"#,
            )
            .expect("valid glyph inventory"),
        );

        assert!(buffer.insert_character('\u{0628}'));
        assert!(buffer.insert_character('\u{0647}'));
        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar.init")
        );
        assert_eq!(
            buffer.sort(1).and_then(TextSort::glyph_name),
            Some("heh-ar.fina")
        );

        buffer.set_direction(TextDirection::LeftToRight);

        assert_eq!(
            buffer.sort(0).and_then(TextSort::glyph_name),
            Some("beh-ar.init")
        );
        assert_eq!(
            buffer.sort(1).and_then(TextSort::glyph_name),
            Some("heh-ar.fina")
        );
    }

    #[test]
    fn set_direction_only_changes_direction_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("V", Some('V'), 500.0);

        assert!(buffer.begin_manual_kerning(1, 500.0));
        buffer.set_direction(TextDirection::RightToLeft);

        assert_eq!(buffer.direction(), TextDirection::RightToLeft);
        assert_eq!(buffer.manual_kerning_sort(), Some(1));
    }

    #[test]
    fn set_kerning_model_keeps_manual_kerning_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("V", Some('V'), 500.0);

        assert!(buffer.begin_manual_kerning(1, 500.0));
        buffer.set_kerning_model(
            serde_json::from_str(
                r#"{
                    "kerning": {
                        "A": { "V": -80 }
                    }
                }"#,
            )
            .expect("valid kerning model"),
        );

        assert_eq!(buffer.manual_kerning_sort(), Some(1));
    }

    #[test]
    fn set_glyph_inventory_keeps_manual_kerning_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("V", Some('V'), 500.0);

        assert!(buffer.begin_manual_kerning(1, 500.0));
        buffer.set_glyph_inventory(
            serde_json::from_str(
                r#"{
                    "unicode": { "65": "A", "86": "V" },
                    "widths": { "A": 500, "V": 500 },
                    "outlines": {}
                }"#,
            )
            .expect("valid glyph inventory"),
        );

        assert_eq!(buffer.manual_kerning_sort(), Some(1));
    }

    #[test]
    fn update_glyph_keeps_manual_kerning_like_xilem_width_edit() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("V", Some('V'), 500.0);

        assert!(buffer.begin_manual_kerning(1, 500.0));
        assert!(buffer.update_glyph(1, "V", Some('V'), 520.0));

        assert_eq!(buffer.manual_kerning_sort(), Some(1));
        let TextSortKind::Glyph { advance_width, .. } = &buffer.sort(1).expect("sort exists").kind
        else {
            panic!("expected glyph sort");
        };
        assert_eq!(*advance_width, 520.0);
    }

    #[test]
    fn layout_positions_ltr_lines_and_cursor() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_line_break();
        buffer.insert_glyph("B", Some('B'), 300.0);

        let layout = buffer.layout(1000.0);

        assert_eq!(layout.items.len(), 2);
        assert_eq!(layout.items[0].x, 0.0);
        assert_eq!(layout.items[0].y, 0.0);
        assert_eq!(layout.items[1].x, 0.0);
        assert_eq!(layout.items[1].y, -1000.0);
        assert_eq!(layout.cursor_x, 300.0);
        assert_eq!(layout.cursor_y, -1000.0);
    }

    #[test]
    fn layout_places_cursor_on_empty_line_after_trailing_line_break_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 300.0);
        buffer.insert_line_break();

        let layout = buffer.layout(1000.0);

        assert_eq!(layout.items.len(), 1);
        assert_eq!(layout.cursor_x, 0.0);
        assert_eq!(layout.cursor_y, -1000.0);
    }

    #[test]
    fn layout_applies_direct_kerning_pairs() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("V", Some('V'), 500.0);
        buffer.set_kerning_model(
            serde_json::from_str(
                r#"{
                    "kerning": {
                        "A": { "V": -80 }
                    }
                }"#,
            )
            .expect("valid kerning model"),
        );

        let layout = buffer.layout(1000.0);

        assert_eq!(layout.items[0].x, 0.0);
        assert_eq!(layout.items[1].x, 420.0);
        assert_eq!(layout.cursor_x, 920.0);
    }

    #[test]
    fn manual_kerning_drag_updates_direct_pair() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("V", Some('V'), 500.0);
        buffer.set_kerning_model(
            serde_json::from_str(
                r#"{
                    "kerning": {
                        "A": { "V": -80 }
                    }
                }"#,
            )
            .expect("valid kerning model"),
        );

        assert!(buffer.begin_manual_kerning(1, 500.0));
        assert_eq!(buffer.manual_kerning_sort(), Some(1));
        assert_eq!(buffer.drag_manual_kerning(530.0), Some(-50.0));

        let layout = buffer.layout(1000.0);
        assert_eq!(layout.items[1].x, 450.0);
        assert_eq!(layout.cursor_x, 950.0);
        assert!(buffer.end_manual_kerning());
        assert_eq!(buffer.manual_kerning_sort(), None);
    }

    #[test]
    fn manual_kerning_drag_snaps_to_integer_units() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("V", Some('V'), 500.0);

        assert!(buffer.begin_manual_kerning(1, 0.0));
        assert_eq!(buffer.drag_manual_kerning(96.16), Some(96.0));
        assert_eq!(
            buffer
                .kerning_model()
                .kerning
                .get("A")
                .and_then(|pairs| pairs.get("V"))
                .copied(),
            Some(96.0)
        );
    }

    #[test]
    fn manual_kerning_enters_noop_session_after_line_break_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_line_break();
        buffer.insert_glyph("V", Some('V'), 500.0);

        assert!(!buffer.begin_manual_kerning(0, 0.0));
        assert!(buffer.begin_manual_kerning(2, 0.0));
        assert_eq!(buffer.manual_kerning_sort(), Some(2));
        assert_eq!(buffer.active_sort(), Some(2));
        assert_eq!(buffer.drag_manual_kerning(30.0), None);
        assert!(buffer.end_manual_kerning());
    }

    #[test]
    fn structural_text_edits_cancel_manual_kerning() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("V", Some('V'), 500.0);

        assert!(buffer.begin_manual_kerning(1, 500.0));
        assert_eq!(buffer.manual_kerning_sort(), Some(1));
        buffer.set_cursor(1);
        assert!(buffer.delete_after_cursor().is_some());
        assert_eq!(buffer.manual_kerning_sort(), None);

        buffer.insert_glyph("V", Some('V'), 500.0);
        assert!(buffer.begin_manual_kerning(1, 500.0));
        buffer.clear();
        assert_eq!(buffer.manual_kerning_sort(), None);
    }

    #[test]
    fn layout_applies_group_kerning_pairs() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("V", Some('V'), 500.0);
        buffer.set_kerning_model(
            serde_json::from_str(
                r#"{
                    "groups": {
                        "public.kern1.A": ["A"],
                        "public.kern2.V": ["V"]
                    },
                    "kerning": {
                        "public.kern1.A": { "public.kern2.V": -90 }
                    }
                }"#,
            )
            .expect("valid kerning model"),
        );

        let layout = buffer.layout(1000.0);

        assert_eq!(layout.items[1].x, 410.0);
        assert_eq!(layout.cursor_x, 910.0);
    }

    #[test]
    fn layout_applies_raw_xilem_group_names_without_public_prefix() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("V", Some('V'), 500.0);
        buffer.set_kerning_model(
            serde_json::from_str(
                r#"{
                    "groups": {
                        "leftRaw": ["A"],
                        "rightRaw": ["V"]
                    },
                    "kerning": {
                        "leftRaw": { "rightRaw": -80 }
                    }
                }"#,
            )
            .expect("valid kerning model"),
        );

        let layout = buffer.layout(1000.0);

        assert_eq!(layout.items[1].x, 420.0);
        assert_eq!(layout.cursor_x, 920.0);
    }

    #[test]
    fn layout_prioritizes_xilem_glyph_group_hints_before_other_memberships() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("V", Some('V'), 500.0);
        buffer.set_kerning_model(
            serde_json::from_str(
                r#"{
                    "groups": {
                        "firstLeft": ["A"],
                        "hintLeft": ["A"],
                        "firstRight": ["V"],
                        "hintRight": ["V"]
                    },
                    "leftGroups": { "V": "hintRight" },
                    "rightGroups": { "A": "hintLeft" },
                    "kerning": {
                        "firstLeft": { "firstRight": -20 },
                        "hintLeft": { "hintRight": -70 }
                    }
                }"#,
            )
            .expect("valid kerning model"),
        );

        let layout = buffer.layout(1000.0);

        assert_eq!(layout.items[1].x, 430.0);
        assert_eq!(layout.cursor_x, 930.0);
    }

    /// Where a given sort ended up in the preview strip.
    fn preview_x(items: &[TextLayoutItem], index: usize) -> f64 {
        items
            .iter()
            .find(|item| item.index == index)
            .unwrap_or_else(|| panic!("sort {index} is in the preview"))
            .x
    }

    /// Where a given sort ended up. Items come back in the order they
    /// are drawn, which for a right-to-left run is not the order the
    /// sorts were typed in.
    fn item_x(layout: &TextLayout, index: usize) -> f64 {
        layout
            .items
            .iter()
            .find(|item| item.index == index)
            .unwrap_or_else(|| panic!("sort {index} was laid out"))
            .x
    }

    #[test]
    fn latin_inside_an_arabic_line_still_reads_left_to_right() {
        // "ا this ب": an Arabic line with a Latin word in it. The word
        // sits where bidi puts it — to the left of the first Arabic
        // letter — but its own letters run left to right. Reversing the
        // whole line is what made "this is a book" read "koob a si siht".
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("alef-ar", Some('\u{0627}'), 100.0);
        for (name, char) in [("t", 't'), ("h", 'h'), ("i", 'i'), ("s", 's')] {
            buffer.insert_glyph(name, Some(char), 100.0);
        }
        buffer.insert_glyph("beh-ar", Some('\u{0628}'), 100.0);

        let layout = buffer.layout(1000.0);

        // The Arabic letters take the outer edges: alef (typed first) on
        // the right, beh on the left.
        assert_eq!(item_x(&layout, 0), 500.0);
        assert_eq!(item_x(&layout, 5), 0.0);
        // t-h-i-s, in that order, between them.
        assert_eq!(item_x(&layout, 1), 100.0);
        assert_eq!(item_x(&layout, 2), 200.0);
        assert_eq!(item_x(&layout, 3), 300.0);
        assert_eq!(item_x(&layout, 4), 400.0);
    }

    #[test]
    fn digits_in_an_arabic_line_read_left_to_right() {
        // Numbers are a weaker case than letters — bidi class EN, not L
        // — and they still run left to right inside an RTL line.
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("alef-ar", Some('\u{0627}'), 100.0);
        buffer.insert_glyph("one", Some('1'), 100.0);
        buffer.insert_glyph("two", Some('2'), 100.0);

        let layout = buffer.layout(1000.0);

        assert_eq!(item_x(&layout, 0), 200.0);
        assert_eq!(item_x(&layout, 1), 0.0);
        assert_eq!(item_x(&layout, 2), 100.0);
    }

    #[test]
    fn arabic_inside_a_latin_line_still_reads_right_to_left() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("a", Some('a'), 100.0);
        buffer.insert_glyph("alef-ar", Some('\u{0627}'), 100.0);
        buffer.insert_glyph("beh-ar", Some('\u{0628}'), 100.0);
        buffer.insert_glyph("b", Some('b'), 100.0);

        let layout = buffer.layout(1000.0);

        assert_eq!(item_x(&layout, 0), 0.0);
        // The Arabic pair is mirrored inside the Latin sentence.
        assert_eq!(item_x(&layout, 1), 200.0);
        assert_eq!(item_x(&layout, 2), 100.0);
        assert_eq!(item_x(&layout, 3), 300.0);
    }

    #[test]
    fn layout_positions_rtl_from_line_width() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
        buffer.insert_glyph("beh-ar", Some('\u{0628}'), 300.0);

        let layout = buffer.layout(1000.0);

        assert_eq!(layout.items.len(), 2);
        // First letter typed sits at the right edge.
        assert_eq!(item_x(&layout, 0), 300.0);
        assert_eq!(item_x(&layout, 1), 0.0);
        assert_eq!(layout.cursor_x, 0.0);
        assert_eq!(layout.cursor_y, 0.0);
    }

    #[test]
    fn manual_kerning_drag_flips_sign_in_rtl() {
        // LTR: dragging right (+40) widens the pair by +40.
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("a", Some('a'), 100.0);
        buffer.insert_glyph("b", Some('b'), 100.0);
        assert!(buffer.begin_manual_kerning(1, 500.0));
        assert_eq!(buffer.drag_manual_kerning(540.0), Some(40.0));
        buffer.end_manual_kerning();

        // RTL: the same rightward drag closes the visual gap, so the
        // logical pair's kern goes to −40.
        let mut rtl = TextBuffer::new();
        rtl.set_direction(TextDirection::RightToLeft);
        rtl.insert_glyph("alef-ar", Some('\u{0627}'), 100.0);
        rtl.insert_glyph("beh-ar", Some('\u{0628}'), 100.0);
        assert!(rtl.begin_manual_kerning(1, 500.0));
        assert_eq!(rtl.drag_manual_kerning(540.0), Some(-40.0));
    }

    #[test]
    fn activate_sort_at_uses_rtl_kerned_layout_origin_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
        buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
        buffer.set_kerning_model(
            serde_json::from_str(
                r#"{
                    "kerning": {
                        "alef-ar": { "beh-ar": -80 }
                    }
                }"#,
            )
            .expect("valid kerning model"),
        );

        let activation = buffer
            .activate_sort_at(100.0, 0.0, 1000.0, 800.0, -200.0)
            .expect("kerned RTL sort activates");

        assert_eq!(activation.index, 1);
        assert_eq!(activation.x, 80.0);
        assert_eq!(activation.y, 0.0);
        assert_eq!(buffer.active_sort(), Some(1));
    }

    #[test]
    fn rtl_layout_places_cursor_on_empty_line_after_trailing_line_break_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.insert_glyph("A", Some('A'), 300.0);
        buffer.insert_line_break();

        let layout = buffer.layout(1000.0);

        assert_eq!(layout.items.len(), 1);
        assert_eq!(layout.cursor_x, 300.0);
        assert_eq!(layout.cursor_y, -1000.0);
    }

    #[test]
    fn auto_direction_reads_each_line_from_its_own_script() {
        let mut buffer = TextBuffer::new();
        // Line 1 Latin, line 2 Arabic — the case a single buffer
        // direction could never get right.
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_line_break();
        buffer.insert_glyph("alef-ar", Some('\u{0627}'), 300.0);

        assert!(buffer.direction_is_auto());
        assert_eq!(
            buffer.resolved_line_direction(0),
            TextDirection::LeftToRight
        );
        assert_eq!(
            buffer.resolved_line_direction(1),
            TextDirection::RightToLeft
        );
    }

    #[test]
    fn auto_direction_ignores_neutral_characters() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("one", Some('1'), 500.0);
        buffer.insert_glyph("period", Some('.'), 200.0);
        buffer.insert_glyph("alef-ar", Some('\u{0627}'), 300.0);

        // Digits and punctuation don't decide; the Arabic letter does.
        assert_eq!(
            buffer.resolved_line_direction(0),
            TextDirection::RightToLeft
        );
    }

    #[test]
    fn pinning_a_direction_overrides_detection() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("alef-ar", Some('\u{0627}'), 300.0);
        assert_eq!(buffer.cursor_direction(), TextDirection::RightToLeft);

        buffer.set_direction(TextDirection::LeftToRight);
        assert!(!buffer.direction_is_auto());
        assert_eq!(buffer.cursor_direction(), TextDirection::LeftToRight);

        buffer.set_auto_direction();
        assert_eq!(buffer.cursor_direction(), TextDirection::RightToLeft);
    }

    #[test]
    fn mixed_lines_lay_out_in_their_own_directions() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_line_break();
        buffer.insert_glyph("alef-ar", Some('\u{0627}'), 300.0);
        buffer.insert_glyph("beh-ar", Some('\u{0628}'), 400.0);

        let layout = buffer.layout(1000.0);

        // Latin line runs rightwards from the origin.
        assert_eq!(item_x(&layout, 0), 0.0);
        // Arabic line right-aligns on the widest line (700) and reads
        // right to left: first letter nearest the right edge.
        assert_eq!(item_x(&layout, 2), 400.0);
        assert_eq!(item_x(&layout, 3), 0.0);
    }

    #[test]
    fn preview_orders_runs_left_to_right_but_fills_each_run_by_direction() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_line_break();
        buffer.insert_glyph("alef-ar", Some('\u{0627}'), 300.0);
        buffer.insert_glyph("beh-ar", Some('\u{0628}'), 400.0);

        let preview = buffer.preview_layout();

        // Latin run first, then the Arabic run occupying [500, 1200]
        // with its first letter on the right.
        assert_eq!(preview_x(&preview, 0), 0.0);
        assert_eq!(preview_x(&preview, 2), 900.0);
        assert_eq!(preview_x(&preview, 3), 500.0);
    }

    #[test]
    fn layout_right_aligns_rtl_lines_on_the_widest_line() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
        buffer.insert_line_break();
        buffer.insert_glyph("beh-ar", Some('\u{0628}'), 300.0);

        let layout = buffer.layout(1000.0);

        // Both lines share the right edge at x = 500 (the widest line),
        // so the 300-wide second line starts 200 units further left.
        assert_eq!(layout.items.len(), 2);
        assert_eq!(item_x(&layout, 0), 0.0);
        assert_eq!(layout.items[0].y, 0.0);
        assert_eq!(item_x(&layout, 2), 200.0);
        assert_eq!(layout.items[1].y, -1000.0);
        assert_eq!(layout.cursor_x, 200.0);
        assert_eq!(layout.cursor_y, -1000.0);
    }

    #[test]
    fn layout_applies_rtl_kerning_without_shifting_line_start() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
        buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
        buffer.set_kerning_model(
            serde_json::from_str(
                r#"{
                    "kerning": {
                        "alef-ar": { "beh-ar": -80 }
                    }
                }"#,
            )
            .expect("valid kerning model"),
        );

        let layout = buffer.layout(1000.0);

        assert_eq!(item_x(&layout, 0), 500.0);
        assert_eq!(item_x(&layout, 1), 80.0);
        assert_eq!(layout.cursor_x, 80.0);
    }

    #[test]
    fn rtl_multiline_layout_resets_kerning_between_lines() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
        buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
        buffer.insert_line_break();
        buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
        buffer.set_kerning_model(
            serde_json::from_str(
                r#"{
                    "kerning": {
                        "alef-ar": { "beh-ar": -80 }
                    }
                }"#,
            )
            .expect("valid kerning model"),
        );

        let layout = buffer.layout(1000.0);

        // Right edge is the widest line: 500 + 500 = 1000 advance
        // units (kerning does not shift the line's start).
        assert_eq!(layout.items.len(), 3);
        assert_eq!(item_x(&layout, 0), 500.0);
        assert_eq!(item_x(&layout, 1), 80.0);
        // The second line kerns from scratch and right-aligns.
        assert_eq!(item_x(&layout, 3), 500.0);
        assert_eq!(layout.cursor_x, 500.0);
        assert_eq!(layout.cursor_y, -1000.0);
    }

    #[test]
    fn preview_layout_keeps_line_breaks_in_one_strip() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_line_break();
        buffer.insert_glyph("V", Some('V'), 300.0);

        let preview = buffer.preview_layout();

        assert_eq!(preview.len(), 2);
        assert_eq!(preview[0].x, 0.0);
        assert_eq!(preview[0].y, 0.0);
        assert_eq!(preview[1].x, 500.0);
        assert_eq!(preview[1].y, 0.0);

        let canvas = buffer.layout(1000.0);
        assert_eq!(canvas.items[1].x, 0.0);
        assert_eq!(canvas.items[1].y, -1000.0);
    }

    #[test]
    fn preview_layout_breaks_kerning_across_line_breaks() {
        let mut buffer = TextBuffer::new();
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_line_break();
        buffer.insert_glyph("V", Some('V'), 500.0);
        buffer.set_kerning_model(
            serde_json::from_str(
                r#"{
                    "kerning": {
                        "A": { "V": -80 }
                    }
                }"#,
            )
            .expect("valid kerning model"),
        );

        let preview = buffer.preview_layout();

        assert_eq!(preview[1].x, 500.0);
    }

    #[test]
    fn rtl_preview_layout_keeps_line_breaks_in_one_strip() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
        buffer.insert_line_break();
        buffer.insert_glyph("beh-ar", Some('\u{0628}'), 300.0);

        let preview = buffer.preview_layout();

        // Both lines read the same way, so the strip carries on across
        // the break: 800 units reading right to left, first glyph on the
        // right.
        assert_eq!(preview.len(), 2);
        assert_eq!(preview_x(&preview, 0), 300.0);
        assert_eq!(preview[0].y, 0.0);
        assert_eq!(preview_x(&preview, 2), 0.0);
        assert_eq!(preview[1].y, 0.0);

        let canvas = buffer.layout(1000.0);
        assert_eq!(item_x(&canvas, 0), 0.0);
        assert_eq!(canvas.items[0].y, 0.0);
        assert_eq!(item_x(&canvas, 2), 200.0);
        assert_eq!(canvas.items[1].y, -1000.0);
    }

    #[test]
    fn rtl_preview_layout_breaks_kerning_across_line_breaks_like_xilem() {
        let mut buffer = TextBuffer::new();
        buffer.set_direction(TextDirection::RightToLeft);
        buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
        buffer.insert_line_break();
        buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
        buffer.set_kerning_model(
            serde_json::from_str(
                r#"{
                    "kerning": {
                        "alef-ar": { "beh-ar": -80 }
                    }
                }"#,
            )
            .expect("valid kerning model"),
        );

        let preview = buffer.preview_layout();

        // The pair straddles a line break, so it does not kern.
        assert_eq!(preview_x(&preview, 0), 500.0);
        assert_eq!(preview_x(&preview, 2), 0.0);
    }
}
