# Text Input Placeholder Alignment Issue

## Problem Description

In Xilem 0.4.0, the `text_alignment()` method on `text_input` widgets does not affect the alignment of placeholder text. While the actual text content is centered when `.text_alignment(parley::Alignment::Center)` is applied, the placeholder text remains left-aligned.

## Current Behavior

When creating a text input with centered alignment and a placeholder:

```rust
text_input(
    String::new(),
    |state: &mut AppState, new_value| {
        state.update_value(new_value);
    }
)
.text_alignment(parley::Alignment::Center)
.placeholder("Group")
```

**Expected:** Both the actual text and placeholder should be center-aligned.

**Actual:** The actual text is centered, but the placeholder remains left-aligned.

## Context

This issue was discovered while implementing the active glyph metrics panel in Runebender Xilem, where we needed a clean, bento-style UI with all text inputs center-aligned including their placeholder hints.

### Use Case

The panel has three rows of text inputs:
- **Row 1**: Glyph name and Unicode
- **Row 2**: Left kern (placeholder: "Kern"), LSB, RSB, Right kern (placeholder: "Kern")
- **Row 3**: Left kern group (placeholder: "Group"), Width, Right kern group (placeholder: "Group")

All inputs should be center-aligned for visual consistency, including the placeholder text.

## Code Location

File: `src/views/editor.rs`

Lines affected:
- Line 267: Left kern input with "Kern" placeholder
- Line 295: Right kern input with "Kern" placeholder
- Line 312: Left kern group input with "Group" placeholder
- Line 331: Right kern group input with "Group" placeholder

## Technical Details

### Current Implementation

In `xilem-0.4.0/src/view/text_input.rs`:

```rust
pub fn text_alignment(mut self, text_alignment: TextAlign) -> Self {
    self.text_alignment = text_alignment;
    self
}

pub fn placeholder(mut self, placeholder_text: impl Into<ArcStr>) -> Self {
    self.placeholder = placeholder_text.into();
    self
}
```

The `text_alignment` field is set, but the placeholder rendering likely doesn't use this field.

### Type Definitions

- `TextAlign` is defined as: `pub use masonry::parley::Alignment as TextAlign;`
- Valid alignment values: `Start`, `End`, `Left`, `Center`, `Right`, `Justify`

## Suggested Fix

The placeholder text rendering should respect the `text_alignment` property. This likely needs changes in:

1. **Xilem layer** (`xilem/src/view/text_input.rs`):
   - Ensure `text_alignment` is passed to the underlying Masonry widget

2. **Masonry layer** (`masonry/src/widgets/text_input.rs` or similar):
   - The placeholder rendering code should use the same alignment as the main text
   - Check the `paint()` or similar method where placeholder is rendered

## Workaround

Currently, there is no workaround. The code is set up correctly with:

```rust
.text_alignment(parley::Alignment::Center)
.placeholder("Text")
```

But the placeholder remains left-aligned until this is fixed upstream.

## Verification

After the fix is implemented, all text inputs in `src/views/editor.rs` lines 257-336 should display their placeholders center-aligned, matching the actual text content alignment.

## Related Files

- `xilem-0.4.0/src/view/text_input.rs` - Text input view definition
- `masonry-0.4.0/src/widgets/text_input.rs` - Underlying widget implementation (likely location of placeholder rendering)
- Parley alignment: `parley-0.6.0/src/layout/mod.rs` - Alignment enum definition

## Priority

Medium - This is a visual consistency issue that affects the polish of the UI but doesn't break functionality.
