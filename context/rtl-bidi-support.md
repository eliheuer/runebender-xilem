# RTL and Bidirectional Text Support Implementation Plan

## Implementation Checklist

- [x] **Phase 1**: Create shaping module foundation (`src/shaping/mod.rs`, `unicode_data.rs`) ✅
- [x] **Phase 2**: Implement Arabic shaping engine (`src/shaping/arabic.rs`) ✅
- [x] **Phase 3**: Add text direction toolbar (LTR/RTL toggle) ✅
- [x] **Phase 4**: Integrate shaping with sort creation ✅
- [x] **Phase 5**: RTL rendering in `render_text_buffer` ✅
- [x] **Phase 6**: Visual cursor navigation for RTL ✅

---

## Overview

This document outlines the implementation of right-to-left (RTL) text support for the Runebender Xilem text editor, focusing on Arabic script. The goal is to enable font designers to edit Arabic fonts with proper contextual shaping and right-to-left text flow.

## Design Philosophy

### Why Build Our Own Shaping Engine?

1. **Real-time performance**: Font compilation is too slow for interactive typing
2. **Source-format agnostic**: Works with UFO, Glyphs, or any future format
3. **Direct sort integration**: Shapes as-you-type with our gap buffer architecture
4. **Incremental updates**: Only reshape affected neighbors, not entire buffer
5. **Focused scope**: Arabic first, extend to other scripts as needed

### What We're NOT Building

- Full HarfBuzz clone (that's a multi-year project)
- OpenType feature compiler (complex, separate concern)
- General-purpose text layout engine (Parley does that)

### What We ARE Building

A **real-time Arabic shaping engine** that:
- Implements Unicode Arabic Joining Algorithm
- Maps codepoints to correct glyph forms (isolated/initial/medial/final)
- Handles Arabic ligatures (lam-alef)
- Works with any font source format
- Integrates with our sort/buffer system

## Key Concepts

### Arabic Script Characteristics

Arabic is a **cursive script** where letters connect to adjacent letters. Each letter has up to 4 positional forms:

| Position | Suffix | When Used |
|----------|--------|-----------|
| **Isolated** | (none) | Letter stands alone or next to non-joining letters |
| **Initial** | `.init` | First letter in a connected sequence |
| **Medial** | `.medi` | Middle of a connected sequence |
| **Final** | `.fina` | Last letter in a connected sequence |

### Joining Types (Unicode Property)

Each Arabic codepoint has a **Joining_Type** property (from [ArabicShaping.txt](https://www.unicode.org/Public/UCD/latest/ucd/ArabicShaping.txt)):

| Type | Code | Description | Forms Available |
|------|------|-------------|-----------------|
| **Dual-Joining** | D | Can connect on both sides | isol, init, medi, fina |
| **Right-Joining** | R | Connects only to previous (right) letter | isol, fina |
| **Non-Joining** | U | Cannot connect | isol only |
| **Join-Causing** | C | Causes adjacent letters to connect | (like TATWEEL) |
| **Transparent** | T | Ignored for joining (marks/diacritics) | N/A |

### Example: Dual-Joining Letters

Letters like ب (beh), س (seen), م (meem) are dual-joining:
- **ب** isolated: `beh-ar` (U+0628)
- **بـ** initial: `beh-ar.init`
- **ـبـ** medial: `beh-ar.medi`
- **ـب** final: `beh-ar.fina`

### Example: Right-Joining Letters

Letters like ا (alef), د (dal), ر (reh) are right-joining (cannot connect forward):
- **ا** isolated: `alef-ar` (U+0627)
- **ـا** final: `alef-ar.fina`
- No initial or medial forms (would be same as isolated/final)

## Architecture: Custom Shaping Engine

### Module Structure

```
src/shaping/
├── mod.rs              # Public API, ShapingEngine trait
├── arabic.rs           # Arabic-specific shaping logic
├── unicode_data.rs     # Unicode character property tables
└── ligatures.rs        # Ligature detection (lam-alef, etc.)
```

### Core Trait

```rust
/// Trait for script-specific shaping engines
pub trait ShapingEngine {
    /// Shape a sequence of characters, returning shaped glyph info
    fn shape(&self, text: &[char], font: &dyn GlyphProvider) -> Vec<ShapedGlyph>;

    /// Incrementally reshape around an edit position
    fn reshape_around(
        &self,
        buffer: &mut SortBuffer,
        position: usize,
        font: &dyn GlyphProvider,
    );
}

/// Provides glyph information from any font source format
pub trait GlyphProvider {
    /// Check if a glyph with this name exists
    fn has_glyph(&self, name: &str) -> bool;

    /// Get advance width for a glyph
    fn advance_width(&self, name: &str) -> Option<f64>;

    /// Get glyph names for a codepoint (may return multiple for variants)
    fn glyphs_for_codepoint(&self, c: char) -> Vec<String>;
}
```

## Implementation Phases

### Phase 1: Shaping Module Foundation

**Goal**: Create the shaping module with core traits and Arabic Unicode data.

**Files to Create**:
```
src/shaping/
├── mod.rs              # Module exports, ShapingEngine trait
├── arabic.rs           # Arabic shaping implementation
└── unicode_data.rs     # Unicode character property tables
```

**Core Types** (`src/shaping/mod.rs`):
```rust
// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Real-time text shaping engine for font editing.
//!
//! This module provides script-specific shaping that works directly with
//! font source files (UFO, Glyphs, etc.) without requiring font compilation.
//! The shaping is designed for interactive editing, with incremental updates
//! as the user types.

pub mod arabic;
pub mod unicode_data;

use crate::sort::SortBuffer;

/// Result of shaping a single character
#[derive(Clone, Debug)]
pub struct ShapedGlyph {
    /// Base glyph name (without positional suffix)
    pub base_name: String,
    /// Full glyph name (with positional suffix if applicable)
    pub glyph_name: String,
    /// Unicode codepoint
    pub codepoint: char,
    /// Positional form (for cursive scripts)
    pub form: PositionalForm,
    /// Advance width
    pub advance_width: f64,
}

/// Positional forms for cursive scripts (Arabic, etc.)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PositionalForm {
    #[default]
    Isolated,
    Initial,
    Medial,
    Final,
}

impl PositionalForm {
    /// Get the glyph name suffix for this form
    pub fn suffix(&self) -> &'static str {
        match self {
            Self::Isolated => "",
            Self::Initial => ".init",
            Self::Medial => ".medi",
            Self::Final => ".fina",
        }
    }
}

/// Trait for providing glyph information from any font source format
pub trait GlyphProvider {
    /// Check if a glyph with this name exists
    fn has_glyph(&self, name: &str) -> bool;

    /// Get advance width for a glyph
    fn advance_width(&self, name: &str) -> Option<f64>;

    /// Get base glyph name for a codepoint (without positional suffix)
    fn base_glyph_for_codepoint(&self, c: char) -> Option<String>;
}
```

**Unicode Data** (`src/shaping/unicode_data.rs`):
```rust
// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Unicode character property data for text shaping.
//!
//! Data sourced from Unicode Standard and ArabicShaping.txt.

/// Arabic joining type from Unicode ArabicShaping.txt
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JoiningType {
    /// Dual-joining: can connect on both sides (beh, seen, meem, etc.)
    Dual,
    /// Right-joining: connects only to previous letter (alef, dal, reh, waw)
    Right,
    /// Non-joining: cannot connect (hamza, Latin, numbers)
    #[default]
    NonJoining,
    /// Join-causing: causes neighbors to connect (tatweel/kashida)
    JoinCausing,
    /// Transparent: ignored for joining (marks, diacritics)
    Transparent,
}

impl JoiningType {
    /// Can this type connect forward (to the left in RTL)?
    pub fn joins_forward(&self) -> bool {
        matches!(self, Self::Dual | Self::JoinCausing)
    }

    /// Can this type connect backward (to the right in RTL)?
    pub fn joins_backward(&self) -> bool {
        matches!(self, Self::Dual | Self::Right | Self::JoinCausing)
    }
}

/// Get the joining type for a Unicode codepoint
pub fn joining_type(c: char) -> JoiningType {
    match c as u32 {
        // === Right-joining (R) ===
        // These letters can only connect to the previous (right-side) letter
        // Alef and variants
        0x0622 => JoiningType::Right, // ALEF WITH MADDA ABOVE
        0x0623 => JoiningType::Right, // ALEF WITH HAMZA ABOVE
        0x0625 => JoiningType::Right, // ALEF WITH HAMZA BELOW
        0x0627 => JoiningType::Right, // ALEF
        0x0629 => JoiningType::Right, // TEH MARBUTA
        // Dal group
        0x062F => JoiningType::Right, // DAL
        0x0630 => JoiningType::Right, // THAL
        // Reh group
        0x0631 => JoiningType::Right, // REH
        0x0632 => JoiningType::Right, // ZAIN
        // Waw group
        0x0648 => JoiningType::Right, // WAW
        0x0624 => JoiningType::Right, // WAW WITH HAMZA ABOVE

        // === Dual-joining (D) ===
        // These letters can connect on both sides
        // Beh group
        0x0628 => JoiningType::Dual, // BEH
        0x062A => JoiningType::Dual, // TEH
        0x062B => JoiningType::Dual, // THEH
        // Jeem group
        0x062C => JoiningType::Dual, // JEEM
        0x062D => JoiningType::Dual, // HAH
        0x062E => JoiningType::Dual, // KHAH
        // Seen group
        0x0633 => JoiningType::Dual, // SEEN
        0x0634 => JoiningType::Dual, // SHEEN
        // Sad group
        0x0635 => JoiningType::Dual, // SAD
        0x0636 => JoiningType::Dual, // DAD
        // Tah group
        0x0637 => JoiningType::Dual, // TAH
        0x0638 => JoiningType::Dual, // ZAH
        // Ain group
        0x0639 => JoiningType::Dual, // AIN
        0x063A => JoiningType::Dual, // GHAIN
        // Feh
        0x0641 => JoiningType::Dual, // FEH
        // Qaf
        0x0642 => JoiningType::Dual, // QAF
        // Kaf
        0x0643 => JoiningType::Dual, // KAF
        // Lam
        0x0644 => JoiningType::Dual, // LAM
        // Meem
        0x0645 => JoiningType::Dual, // MEEM
        // Noon
        0x0646 => JoiningType::Dual, // NOON
        // Heh
        0x0647 => JoiningType::Dual, // HEH
        // Yeh group
        0x064A => JoiningType::Dual, // YEH
        0x0626 => JoiningType::Dual, // YEH WITH HAMZA ABOVE
        0x0649 => JoiningType::Dual, // ALEF MAKSURA (behaves as dual-joining)

        // === Non-joining (U) ===
        0x0621 => JoiningType::NonJoining, // HAMZA (standalone)

        // === Join-causing (C) ===
        0x0640 => JoiningType::JoinCausing, // TATWEEL (kashida)

        // === Transparent (T) ===
        // Arabic marks and diacritics - ignored for joining
        0x064B..=0x0652 => JoiningType::Transparent, // Fathatan through Sukun
        0x0670 => JoiningType::Transparent, // SUPERSCRIPT ALEF
        0x0610..=0x061A => JoiningType::Transparent, // Various marks
        0x06D6..=0x06ED => JoiningType::Transparent, // Quranic marks

        // Default: non-joining (Latin, numbers, punctuation, etc.)
        _ => JoiningType::NonJoining,
    }
}

/// Check if a character is in the Arabic Unicode block
pub fn is_arabic(c: char) -> bool {
    let cp = c as u32;
    // Arabic: U+0600–U+06FF
    // Arabic Supplement: U+0750–U+077F
    // Arabic Extended-A: U+08A0–U+08FF
    (0x0600..=0x06FF).contains(&cp)
        || (0x0750..=0x077F).contains(&cp)
        || (0x08A0..=0x08FF).contains(&cp)
}
```

---

### Phase 2: Arabic Shaping Engine

**Goal**: Implement the Arabic shaping algorithm.

**File**: `src/shaping/arabic.rs`

```rust
// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Arabic shaping engine implementing Unicode Arabic Joining Algorithm.
//!
//! This provides real-time shaping for Arabic text, determining the correct
//! positional form (isolated, initial, medial, final) for each character
//! based on its neighbors.

use super::unicode_data::{JoiningType, is_arabic, joining_type};
use super::{GlyphProvider, PositionalForm, ShapedGlyph};

/// Arabic shaping engine
pub struct ArabicShaper;

impl ArabicShaper {
    /// Create a new Arabic shaper
    pub fn new() -> Self {
        Self
    }

    /// Shape a sequence of characters into glyphs with correct positional forms
    pub fn shape(
        &self,
        text: &[char],
        font: &dyn GlyphProvider,
    ) -> Vec<ShapedGlyph> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::with_capacity(text.len());

        for (i, &c) in text.iter().enumerate() {
            let shaped = self.shape_char_at(text, i, font);
            if let Some(glyph) = shaped {
                result.push(glyph);
            }
        }

        result
    }

    /// Shape a single character at the given index, considering neighbors
    pub fn shape_char_at(
        &self,
        text: &[char],
        index: usize,
        font: &dyn GlyphProvider,
    ) -> Option<ShapedGlyph> {
        let c = text.get(index)?;

        // Get base glyph name
        let base_name = font.base_glyph_for_codepoint(*c)?;

        // Determine positional form
        let form = self.determine_form(text, index);

        // Build full glyph name with suffix
        let glyph_name = self.resolve_glyph_name(&base_name, form, font);

        // Get advance width
        let advance_width = font.advance_width(&glyph_name).unwrap_or(500.0);

        Some(ShapedGlyph {
            base_name,
            glyph_name,
            codepoint: *c,
            form,
            advance_width,
        })
    }

    /// Determine the positional form for a character based on neighbors
    fn determine_form(&self, text: &[char], index: usize) -> PositionalForm {
        let c = text[index];

        // Non-Arabic characters are always isolated
        if !is_arabic(c) {
            return PositionalForm::Isolated;
        }

        let jt = joining_type(c);

        // Non-joining and transparent are always isolated
        if matches!(jt, JoiningType::NonJoining | JoiningType::Transparent) {
            return PositionalForm::Isolated;
        }

        // Check neighbors (skipping transparent characters)
        let prev_joins = self.check_prev_joins_forward(text, index);
        let next_joins = self.check_next_joins_backward(text, index);

        match jt {
            JoiningType::Dual => {
                match (prev_joins, next_joins) {
                    (false, false) => PositionalForm::Isolated,
                    (false, true) => PositionalForm::Initial,
                    (true, false) => PositionalForm::Final,
                    (true, true) => PositionalForm::Medial,
                }
            }
            JoiningType::Right => {
                // Right-joining only has isolated and final
                if prev_joins {
                    PositionalForm::Final
                } else {
                    PositionalForm::Isolated
                }
            }
            JoiningType::JoinCausing => PositionalForm::Isolated,
            _ => PositionalForm::Isolated,
        }
    }

    /// Check if previous non-transparent character joins forward
    fn check_prev_joins_forward(&self, text: &[char], index: usize) -> bool {
        // Walk backward, skipping transparent characters
        let mut i = index;
        while i > 0 {
            i -= 1;
            let jt = joining_type(text[i]);
            if jt != JoiningType::Transparent {
                return jt.joins_forward();
            }
        }
        false
    }

    /// Check if next non-transparent character joins backward
    fn check_next_joins_backward(&self, text: &[char], index: usize) -> bool {
        // Walk forward, skipping transparent characters
        let mut i = index + 1;
        while i < text.len() {
            let jt = joining_type(text[i]);
            if jt != JoiningType::Transparent {
                return jt.joins_backward();
            }
            i += 1;
        }
        false
    }

    /// Resolve glyph name with positional suffix, falling back if needed
    fn resolve_glyph_name(
        &self,
        base_name: &str,
        form: PositionalForm,
        font: &dyn GlyphProvider,
    ) -> String {
        // Try with suffix first
        let with_suffix = format!("{}{}", base_name, form.suffix());
        if font.has_glyph(&with_suffix) {
            return with_suffix;
        }

        // Fallback to base glyph
        base_name.to_string()
    }

    /// Reshape a range of characters (for incremental updates)
    ///
    /// Call this after inserting or deleting to update affected neighbors.
    /// Returns the range of indices that were reshaped.
    pub fn reshape_range(
        &self,
        text: &[char],
        start: usize,
        end: usize,
        font: &dyn GlyphProvider,
    ) -> Vec<ShapedGlyph> {
        // Expand range to include neighbors that might be affected
        let actual_start = start.saturating_sub(1);
        let actual_end = (end + 1).min(text.len());

        let mut result = Vec::with_capacity(actual_end - actual_start);
        for i in actual_start..actual_end {
            if let Some(shaped) = self.shape_char_at(text, i, font) {
                result.push(shaped);
            }
        }
        result
    }
}

impl Default for ArabicShaper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock font provider for testing
    struct MockFont;

    impl GlyphProvider for MockFont {
        fn has_glyph(&self, name: &str) -> bool {
            // Simulate having Arabic glyphs with suffixes
            name.contains("-ar")
        }

        fn advance_width(&self, _name: &str) -> Option<f64> {
            Some(500.0)
        }

        fn base_glyph_for_codepoint(&self, c: char) -> Option<String> {
            match c {
                'ا' => Some("alef-ar".to_string()),
                'ب' => Some("beh-ar".to_string()),
                'ت' => Some("teh-ar".to_string()),
                'م' => Some("meem-ar".to_string()),
                'ن' => Some("noon-ar".to_string()),
                _ => None,
            }
        }
    }

    #[test]
    fn test_isolated_single_char() {
        let shaper = ArabicShaper::new();
        let font = MockFont;
        let text: Vec<char> = "ب".chars().collect();

        let result = shaper.shape(&text, &font);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].form, PositionalForm::Isolated);
        assert_eq!(result[0].glyph_name, "beh-ar");
    }

    #[test]
    fn test_two_dual_joining() {
        let shaper = ArabicShaper::new();
        let font = MockFont;
        // بم - beh followed by meem
        let text: Vec<char> = "بم".chars().collect();

        let result = shaper.shape(&text, &font);
        assert_eq!(result.len(), 2);
        // First char (beh) should be initial
        assert_eq!(result[0].form, PositionalForm::Initial);
        // Second char (meem) should be final
        assert_eq!(result[1].form, PositionalForm::Final);
    }

    #[test]
    fn test_right_joining_alef() {
        let shaper = ArabicShaper::new();
        let font = MockFont;
        // با - beh followed by alef
        let text: Vec<char> = "با".chars().collect();

        let result = shaper.shape(&text, &font);
        assert_eq!(result.len(), 2);
        // Beh is initial (alef joins backward but beh can't connect forward to non-joining)
        // Actually: alef is right-joining, so beh IS followed by something that joins backward
        assert_eq!(result[0].form, PositionalForm::Initial);
        // Alef is final (beh joins forward)
        assert_eq!(result[1].form, PositionalForm::Final);
    }

    #[test]
    fn test_three_char_medial() {
        let shaper = ArabicShaper::new();
        let font = MockFont;
        // بمن - beh, meem, noon
        let text: Vec<char> = "بمن".chars().collect();

        let result = shaper.shape(&text, &font);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].form, PositionalForm::Initial); // beh
        assert_eq!(result[1].form, PositionalForm::Medial);  // meem
        assert_eq!(result[2].form, PositionalForm::Final);   // noon
    }
}
```

---

### Phase 3: Text Direction Toolbar

**Goal**: Add LTR/RTL toggle buttons to the text tool, similar to shapes toolbar.

**Files to Create**:
- `src/components/text_direction_toolbar.rs` - New toolbar widget

**Files to Modify**:
- `src/views/editor.rs` - Add toolbar to editor view layout
- `src/edit_session.rs` - Add `text_direction: LayoutMode` field
- `src/components/mod.rs` - Export new toolbar

**Implementation**: Follow the same pattern as `shapes_toolbar.rs`:
- Two buttons: LTR and RTL
- Icons showing text direction arrows
- Visible when Text tool is selected AND text_mode_active

---

### Phase 4: Integrate Shaping with Sort System

**Goal**: Connect the shaping engine to sort creation and buffer updates.

**Files to Modify**:
- `src/edit_session.rs` - Use shaper for `create_sort_from_char`
- `src/components/editor_canvas.rs` - Reshape neighbors after insert/delete

**Key Integration Points**:

1. **On character insert**: Shape the new character AND reshape neighbors
2. **On character delete**: Reshape neighbors
3. **On direction change**: Reshape entire buffer

---

### Phase 5: RTL Rendering

**Goal**: Render glyphs right-to-left when in RTL mode.

**File to Modify**:
- `src/components/editor_canvas.rs` - `render_text_buffer` function

**Changes**:
```rust
fn render_text_buffer(&self, scene: &mut Scene, transform: &Affine, is_preview_mode: bool) {
    let buffer = match &self.session.text_buffer {
        Some(buf) => buf,
        None => return,
    };

    // Determine text direction
    let is_rtl = self.session.text_direction == LayoutMode::RTL;

    // For RTL: start from right side, advance leftward
    let mut x_offset = if is_rtl {
        self.calculate_total_width(buffer)
    } else {
        0.0
    };

    for (index, sort) in buffer.iter().enumerate() {
        if let SortKind::Glyph { name, advance_width, .. } = &sort.kind {
            if is_rtl {
                x_offset -= advance_width;
            }

            let sort_position = Point::new(x_offset, 0.0);
            self.render_inactive_sort(scene, name, sort_position, transform);

            if !is_rtl {
                x_offset += advance_width;
            }
        }
    }
}
```

---

### Phase 6: RTL Cursor Navigation

**Goal**: Arrow keys move cursor visually (left=left, right=right) regardless of text direction.

**File to Modify**:
- `src/components/editor_canvas.rs` - `handle_text_mode_input`

**Changes**:
```rust
Key::Named(NamedKey::ArrowLeft) => {
    if let Some(buffer) = &mut self.session.text_buffer {
        if self.session.text_direction == LayoutMode::RTL {
            buffer.move_cursor_right(); // Visual left = logical right in RTL
        } else {
            buffer.move_cursor_left();
        }
    }
}
Key::Named(NamedKey::ArrowRight) => {
    if let Some(buffer) = &mut self.session.text_buffer {
        if self.session.text_direction == LayoutMode::RTL {
            buffer.move_cursor_left(); // Visual right = logical left in RTL
        } else {
            buffer.move_cursor_right();
        }
    }
}
```

---

## Testing Strategy

### Test UFO Setup
The existing `assets/untitled.ufo` has Arabic glyphs with contextual forms:
- `alef-ar`, `alef-ar.fina` (right-joining)
- `beh-ar`, `beh-ar.init`, `beh-ar.medi`, `beh-ar.fina` (dual-joining)
- `lam-ar`, `lam-ar.init`, `lam-ar.medi`, `lam-ar.fina` (dual-joining)
- Plus: seen, meem, noon, heh, ain, kaf, qaf, feh, and more

### Unit Tests (in `src/shaping/arabic.rs`)
Tests are embedded in the module - see Phase 2 code above.

### Manual Test Cases

1. **Direction toggle**: Switch between LTR/RTL, verify toolbar state
2. **Basic RTL rendering**: Type "ال" (alef-lam), verify renders right-to-left
3. **Joining forms**: Type "بسم" (beh-seen-meem), verify:
   - beh uses `.init` form
   - seen uses `.medi` form
   - meem uses `.fina` form
4. **Non-joining letters**: Type "اب" (alef-beh), verify:
   - beh uses `.init` form (followed by right-joining alef)
   - alef uses `.fina` form (preceded by dual-joining beh)
5. **Cursor navigation**: In RTL mode, verify left arrow moves cursor visually left
6. **Incremental reshape**: Type "بم", then insert "س" in middle, verify all three reshape correctly

---

## Future Enhancements

### Phase 7: Lam-Alef Ligatures
Arabic has mandatory ligatures when lam (ل) is followed by alef (ا):
- `lam_alef-ar` for لا
- Handle in shaping engine by detecting sequence and substituting

### Phase 8: Full Bidi Support
- Detect script direction automatically per-run
- Handle mixed LTR/RTL in same line (Unicode Bidi Algorithm UAX-9)
- Multiple direction runs within a line

### Phase 9: Mark Positioning
- Arabic diacritics (fatha, kasra, damma, etc.)
- Position marks relative to base glyphs
- May require anchor data from UFO

### Phase 10: Additional Scripts
- Hebrew (simpler - no contextual joining)
- Syriac, Thaana (similar to Arabic)
- Indic scripts (different shaping model)

---

## File Structure Summary

```
src/
├── shaping/                    # NEW: Shaping engine module
│   ├── mod.rs                  # Public API, traits, ShapedGlyph
│   ├── arabic.rs               # Arabic shaping implementation
│   └── unicode_data.rs         # Unicode character properties
├── sort/
│   ├── mod.rs                  # Add: use shaping
│   ├── buffer.rs               # Modify: add reshape_around_cursor
│   ├── cursor.rs               # Modify: RTL position calculation
│   └── data.rs                 # Already has LayoutMode::RTL
├── components/
│   ├── editor_canvas.rs        # Modify: RTL rendering, RTL cursor nav
│   ├── text_direction_toolbar.rs  # NEW: LTR/RTL toggle widget
│   └── mod.rs                  # Add: pub mod text_direction_toolbar;
├── views/
│   └── editor.rs               # Modify: add text direction toolbar
├── edit_session.rs             # Modify: text_direction field
└── main.rs                     # Add: mod shaping
```

---

## Dependencies

**No new dependencies required.** The implementation uses:
- Unicode character properties (hardcoded lookup table based on ArabicShaping.txt)
- Existing glyph naming conventions (`.init`, `.medi`, `.fina` suffixes)
- Existing Parley 0.6 (for future bidi, currently unused)

---

## References

- [Unicode Arabic Script](https://www.unicode.org/charts/PDF/U0600.pdf) - Character chart
- [Unicode ArabicShaping.txt](https://www.unicode.org/Public/UCD/latest/ucd/ArabicShaping.txt) - Joining type data
- [Unicode Chapter 9: Middle East-I](https://www.unicode.org/versions/Unicode15.0.0/ch09.pdf) - Arabic shaping algorithm
- [Unicode Bidirectional Algorithm (UAX-9)](https://www.unicode.org/reports/tr9/) - For future bidi support
- [Glyphs Arabic Tutorial](https://glyphsapp.com/learn/arabic) - Reference implementation
- [HarfRust GitHub](https://github.com/harfbuzz/harfrust) - Future reference
- [Parley GitHub](https://github.com/linebender/parley) - Future bidi integration
