# Kerning Implementation

## Overview

Add full kerning support to Runebender Xilem so that text in the editor and text buffer preview respects kerning pairs and kerning groups defined in the UFO file.

## Background

### UFO Kerning Specification

From the [UFO spec](https://unifiedfontobject.org/versions/ufo3/kerning.plist/):

**File Location**: `kerning.plist` (XML Property List, optional)

**Structure**:
```xml
<dict>
  <key>A</key>
  <dict>
    <key>V</key>
    <integer>-50</integer>
  </dict>
  <key>public.kern1.O</key>
  <dict>
    <key>A</key>
    <integer>-50</integer>
  </dict>
</dict>
```

**Kerning Groups**:
- `public.kern1.*` - Groups for first member of pair (left side)
- `public.kern2.*` - Groups for second member of pair (right side)
- Groups defined in `groups.plist` OR in individual glyph lib data
- Both glyph names and group names can appear in kerning pairs

**Lookup Precedence** (highest to lowest):
1. Glyph + glyph pairs
2. Glyph + group pairs
3. Group + glyph pairs
4. Group + group pairs
5. Default to zero

### Glyphs.app Kerning Model

From [Glyphs.app kerning docs](https://glyphsapp.com/learn/kerning):

**Three-tier system**:
1. Group-to-group (most general)
2. Group-to-glyph or glyph-to-group (exceptions)
3. Glyph-to-glyph (specific overrides)

**Visual feedback**:
- Negative kerning (light blue) = tighter spacing
- Positive kerning (light yellow) = looser spacing

**Best practices**:
- Don't kern more than 40% of glyph width
- Use groups for efficiency

## Current State

### What We Have

1. **Kerning group metadata** - Already loaded from glyph lib data:
   - `src/workspace.rs:181-186` - Reads `public.kern1` and `public.kern2` from glyph lib
   - `src/workspace.rs:352-363` - Saves kerning groups back to glyph lib
   - Fields: `Glyph::left_group` and `Glyph::right_group`

2. **Text rendering locations** - Two places where sorts are laid out:
   - `src/components/editor_canvas.rs:537-599` - Main editor canvas rendering
   - `src/views/editor.rs:456-482` - Text buffer preview panel

3. **Current layout logic**:
   ```rust
   let mut x_offset = 0.0;
   for sort in buffer.iter() {
       // render glyph at x_offset
       x_offset += advance_width;  // NO KERNING APPLIED
   }
   ```

### What's Missing

1. **Kerning data loading** - No code to load `kerning.plist`
2. **Kerning group loading** - No code to load `groups.plist` (though glyph-level groups work)
3. **Kerning lookup function** - No code to query kerning value between two glyphs
4. **Kerning application** - No code to apply kerning when positioning sorts

## Implementation Plan

### Phase 1: Data Structures and Loading

**Files to modify**: `src/workspace.rs`

- [ ] Add kerning data structures to `Workspace`:
  ```rust
  /// Kerning pairs: first_member -> (second_member -> kern_value)
  pub kerning: HashMap<String, HashMap<String, f64>>,

  /// Kerning groups: group_name -> [glyph_names]
  /// e.g., "public.kern1.O" -> ["O", "D", "Q"]
  pub groups: HashMap<String, Vec<String>>,
  ```

- [ ] Add function to load `kerning.plist`:
  ```rust
  fn load_kerning(path: &Path) -> Result<HashMap<String, HashMap<String, f64>>>
  ```

- [ ] Add function to load `groups.plist`:
  ```rust
  fn load_groups(path: &Path) -> Result<HashMap<String, Vec<String>>>
  ```

- [ ] Integrate loading in `Workspace::load()`:
  - Load `kerning.plist` if it exists
  - Load `groups.plist` if it exists
  - Merge glyph-level groups with groups.plist groups

- [ ] Add function to save `kerning.plist` in `Workspace::save()`:
  ```rust
  fn save_kerning(&self) -> Result<()>
  ```

- [ ] Add function to save `groups.plist` in `Workspace::save()`:
  ```rust
  fn save_groups(&self) -> Result<()>
  ```

### Phase 2: Kerning Lookup Algorithm

**Files to create/modify**: `src/kerning.rs` (new module)

- [ ] Create `src/kerning.rs` module for kerning logic

- [ ] Implement kerning lookup function:
  ```rust
  /// Look up kerning value between two glyphs
  /// Returns 0.0 if no kerning is defined
  pub fn lookup_kerning(
      kerning_pairs: &HashMap<String, HashMap<String, f64>>,
      groups: &HashMap<String, Vec<String>>,
      left_glyph: &str,
      left_group: Option<&str>,
      right_glyph: &str,
      right_group: Option<&str>,
  ) -> f64
  ```

- [ ] Implement lookup precedence:
  1. Check glyph + glyph
  2. Check glyph + right_group
  3. Check left_group + glyph
  4. Check left_group + right_group
  5. Return 0.0

- [ ] Add helper function to check if glyph is in group:
  ```rust
  fn glyph_in_group(
      groups: &HashMap<String, Vec<String>>,
      glyph_name: &str,
      group_name: &str,
  ) -> bool
  ```

- [ ] Add unit tests for lookup algorithm with various scenarios

### Phase 3: Apply Kerning in Editor Canvas

**Files to modify**: `src/components/editor_canvas.rs`

- [ ] Modify `render_sorts()` method (lines 532-599):
  - Get workspace reference from `self.session.workspace`
  - Track previous glyph name and group for kerning lookup
  - Calculate kerning value before advancing x_offset
  - Apply kerning: `x_offset += advance_width + kerning_value`

- [ ] Update `render_sort_metrics()` to show kerning visually:
  - Draw kerning adjustment indicator
  - Use different colors for positive/negative kerning (like Glyphs.app)

- [ ] Modify `find_sort_at_position()` (lines 867-896):
  - Apply kerning when calculating sort bounding boxes
  - Ensure click detection accounts for kerned positions

- [ ] Modify `activate_sort()` (lines 950-985):
  - Apply kerning when calculating `active_sort_x_offset`

### Phase 4: Apply Kerning in Text Buffer Preview

**Files to modify**: `src/views/editor.rs`

- [ ] Modify `text_buffer_preview_pane_centered()` (lines 456-482):
  - Get workspace reference from session
  - Track previous glyph for kerning lookup
  - Calculate kerning value before advancing x_offset
  - Apply kerning: `x_offset += advance_width + kerning_value`

### Phase 5: Testing and Validation

**Test files to create**:

- [ ] Create test UFO with kerning data:
  - Add `assets/test-kerning.ufo/kerning.plist`
  - Add `assets/test-kerning.ufo/groups.plist`
  - Define standard kerning pairs (A-V, T-o, etc.)
  - Define kerning groups for round letters, capitals, etc.

- [ ] Manual testing checklist:
  - [ ] Load UFO with kerning data
  - [ ] Type text in editor with kerned pairs (e.g., "WAVE")
  - [ ] Verify sorts move closer/apart based on kerning
  - [ ] Verify text buffer preview shows same kerning
  - [ ] Verify clicking on kerned sorts works correctly
  - [ ] Verify active sort x-offset is correct with kerning
  - [ ] Test with empty kerning.plist (should work gracefully)
  - [ ] Test with missing kerning.plist (should default to 0)

- [ ] Unit tests:
  - [ ] Test kerning lookup with all precedence levels
  - [ ] Test with glyph-to-glyph pairs
  - [ ] Test with group-to-group pairs
  - [ ] Test with mixed glyph/group pairs
  - [ ] Test with non-existent pairs (should return 0)
  - [ ] Test loading valid kerning.plist
  - [ ] Test loading invalid/corrupt kerning.plist

### Phase 6: UI for Kerning Editing (Future)

**Not in initial scope, but planned for later**:

- [ ] Add kerning value display in active glyph panel
- [ ] Add ability to edit kerning values
- [ ] Add kerning pair browser/editor
- [ ] Visual kerning adjustment (drag to kern)
- [ ] Kerning group editor

## Current Kerning Group Storage

Kerning groups are currently stored in two places:

1. **Glyph-level** (already implemented):
   - Location: `glyph.lib["public.kern1"]` and `glyph.lib["public.kern2"]`
   - Loaded: `src/workspace.rs:181-186`
   - Saved: `src/workspace.rs:352-363`
   - Stored in: `Glyph::left_group` and `Glyph::right_group`

2. **Font-level** (TODO):
   - Location: `groups.plist` in UFO root
   - Format: `{ "public.kern1.O": ["O", "D", "Q"], ... }`
   - Need to load and merge with glyph-level groups

## Files to Modify

1. `src/workspace.rs` - Add kerning/groups data structures and loading
2. `src/kerning.rs` - NEW: Kerning lookup algorithm
3. `src/components/editor_canvas.rs` - Apply kerning in main editor
4. `src/views/editor.rs` - Apply kerning in text buffer preview
5. `src/lib.rs` - Add `pub mod kerning;`

## Dependencies

- `plist` crate (already in use for UFO loading)
- No new dependencies needed

## Technical Considerations

### Performance

- Kerning lookup happens for every glyph pair during layout
- Cache lookup results if performance becomes an issue
- Current approach: ~O(1) HashMap lookups should be fast enough

### Edge Cases

- Missing kerning.plist → default to empty HashMap
- Missing groups.plist → use only glyph-level groups
- Invalid kerning values → log warning, use 0
- Circular group references → detect and warn
- Glyph not in font → kerning still works (per spec)

### Compatibility

- UFO 3 spec compliant
- Works with or without kerning data
- Backwards compatible with existing UFO files

## Success Criteria

1. ✅ Load kerning pairs from `kerning.plist`
2. ✅ Load kerning groups from `groups.plist` and glyph lib
3. ✅ Apply kerning when laying out sorts in editor
4. ✅ Apply kerning when rendering text buffer preview
5. ✅ Kerning respects lookup precedence (glyph > group)
6. ✅ Visual feedback shows kerning adjustments
7. ✅ Click detection works with kerned positions
8. ✅ Active sort offset calculation includes kerning
9. ✅ Graceful handling of missing/invalid kerning data
10. ✅ Unit tests pass for all kerning lookup scenarios

## Future Enhancements

- Kerning value editing in UI
- Visual kerning adjustment (drag glyphs)
- Kerning pair browser
- Kerning group editor
- Auto-kerning suggestions
- Kerning import/export
- Class-based kerning (OpenType feature code)
