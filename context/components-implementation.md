# UFO Components Implementation Plan

## Overview

Components in UFO allow glyphs to reference other glyphs as reusable building blocks. This is heavily used in Arabic fonts where base letter forms are combined with dots and marks.

## UFO Component Specification

From the UFO spec, a component has:
- `base` (required): Name of the glyph to reference
- Transform attributes: `xScale`, `xyScale`, `yxScale`, `yScale`, `xOffset`, `yOffset`
- Default transform is identity: `[1, 0, 0, 1, 0, 0]`

## Implementation Phases

### Phase 1: Data Model & Loading ✅
- [x] Add `Component` struct to `workspace.rs`
- [x] Add `components: Vec<Component>` to `Glyph` struct
- [x] Load components from UFO via norad
- [x] Save components back to UFO
- [x] Components accessible via `session.glyph.components`

### Phase 2: Basic Rendering ✅
- [x] Render components in editor canvas (filled, different color)
- [x] Recursively resolve component references (handle nested components)
- [x] Apply component transforms (Affine from 6 values)
- [x] Render in preview pane and text buffer
- [x] Render in grid view

### Phase 3: Selection & Repositioning ✅
- [x] Add component to hit testing (winding number test)
- [x] Single-click to select component (visual feedback)
- [x] Drag to reposition (translate transform offset)
- [x] Visual selection feedback (brighter color + orange outline)
- [x] Save repositioned components back to UFO (via to_glyph())
- [x] Arrow keys to nudge selected component

### Phase 4: Advanced Editing (Glyphs-like) ✅
- [x] Double-click component to add base glyph to buffer for editing
- [x] Show component selection outline (orange stroke when selected)
- [ ] Component info in coordinate panel (future enhancement)

## Data Structures

```rust
/// A component reference to another glyph
#[derive(Debug, Clone)]
pub struct Component {
    /// Name of the referenced glyph
    pub base: String,
    /// Affine transformation matrix
    pub transform: kurbo::Affine,
    /// Unique identifier for selection
    pub id: EntityId,
}
```

## Key Files to Modify

1. `src/workspace.rs` - Add Component struct, load/save
2. `src/edit_session.rs` - Add components to session
3. `src/components/editor_canvas.rs` - Render and hit-test components
4. `src/tools/select.rs` - Component selection logic
5. `src/theme.rs` - Component colors

## Reference

- UFO Spec: https://unifiedfontobject.org/versions/ufo3/glyphs/glif/
- Glyphs Smart Components: https://glyphsapp.com/learn/smart-components
- runebender-druid: https://github.com/linebender/runebender
