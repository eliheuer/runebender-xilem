# Outline Transforms & Boolean Operations

## Overview

Add copy/flip/rotate/scale transforms and boolean path operations
(union/subtract/intersect) to runebender-xilem, enabling workflows like
drawing half a serif and mirroring it, or combining overlapping shapes
into a single contour.

## Reference: How Other Editors Do It

### Glyphs
- **Transformations palette** (Cmd+Opt+P): flip buttons, rotation angle
  field, scale percentage fields, boolean op buttons (union, subtract,
  intersect). See screenshot — rows of icon buttons for pivot mode,
  flip, and path alignment, with numeric fields for scale, rotate,
  skew, plus boolean ops at the bottom.
- **Dedicated tools**: Rotate (R), Scale (S) — click canvas to set
  pivot, drag to transform
- **Pivot**: 9-point bounding box selector, click-to-set reference
  point, or metric-based origins (baseline, x-height center, etc.)
- **Remove Overlap**: Path menu (Cmd+Shift+O) or palette button

### RoboFont
- **Transform mode** (Cmd+T): hold Opt to rotate, Cmd to scale
- **Inspector bar**: flip buttons, numeric fields, origin selector
- **Cmd+D**: repeat last transform (great for rotational copies)
- **Boolean ops** via `booleanOperations` Python library or extensions

### Fontra
- **Transformations panel**: numeric fields for move, scale, rotate,
  skew, plus flip buttons and boolean op buttons
- **Origin selector**: bounding box positions + custom x,y
- All values applied on Enter

### Common Patterns Across All Three
1. Flip H/V are quick-access buttons (no numeric input needed)
2. Rotate and scale take numeric values or interactive drag
3. Pivot/origin is configurable (bounding box center is default)
4. Boolean ops work on selected-vs-unselected contours
5. All transforms apply to selection (or all if nothing selected)

---

## Design for Runebender Xilem

### UX Approach: Keyboard Shortcuts + Transform Panel

Two parallel interfaces to the same underlying EditSession methods:

1. **Keyboard shortcuts** for quick access to common operations
2. **Transform panel** on the right side of the editor for numeric
   input, buttons, and the full range of operations

### Keyboard Shortcuts

These will be added to the README alongside existing shortcuts:

| Shortcut | Action | Notes |
|----------|--------|-------|
| Cmd+D | Duplicate selection | Copy + paste in place with offset |
| Cmd+Shift+D | Duplicate + repeat last transform | Chain transforms |
| Shift+H | Flip selection horizontally | Around selection bbox center |
| Shift+V | Flip selection vertically | Around selection bbox center |
| Cmd+Shift+R | Rotate selection 90 CW | Around selection bbox center |
| Cmd+Shift+L | Rotate selection 90 CCW | Around selection bbox center |
| Cmd+Shift+O | Remove overlap (union) | Union all contours in glyph |

These avoid conflicts with existing bindings. Specifically checked:
- `R` alone = reverse contours (no conflict with Cmd+Shift+R)
- `H` alone = HyperPen tool (no conflict with Shift+H)
- `V` alone = Select tool (no conflict with Shift+V)
- `Cmd+Shift+H` = convert hyper to cubic (no conflict)

#### README Additions

Add a new "Transforms" section to the Editing shortcuts table:

```markdown
### Transforms

| Shortcut | Action |
|----------|--------|
| `Cmd/Ctrl` + `D` | Duplicate selection |
| `Cmd/Ctrl` + `Shift` + `D` | Duplicate + repeat last transform |
| `Shift` + `H` | Flip selection horizontally |
| `Shift` + `V` | Flip selection vertically |
| `Cmd/Ctrl` + `Shift` + `R` | Rotate selection 90 clockwise |
| `Cmd/Ctrl` + `Shift` + `L` | Rotate selection 90 counter-clockwise |
| `Cmd/Ctrl` + `Shift` + `O` | Remove overlap (union all contours) |
```

### Pivot Point

Default: **center of selection bounding box**. This is the simplest
approach and matches what users expect. The transform panel will later
add a 9-point selector to choose other pivot positions (corners, edge
midpoints).

Computing the pivot:
```
for each selected point -> collect into bounding Rect
pivot = rect.center()
```

If no points are selected, use the bounding box of all paths.

---

## Transform Architecture

All transforms go through `EditSession` methods that:
1. Compute selection bounding box center (pivot)
2. Build a `kurbo::Affine` transform
3. Apply to all selected points (and their adjacent off-curve handles
   if an on-curve is selected)
4. Snap to grid after transform
5. Enforce smooth constraints (iterative cascade)
6. Record edit for undo

```rust
impl EditSession {
    pub fn selection_bounding_box(&self) -> Option<kurbo::Rect>;
    pub fn transform_selection(&mut self, affine: Affine);
    pub fn flip_selection_horizontal(&mut self);
    pub fn flip_selection_vertical(&mut self);
    pub fn rotate_selection(&mut self, degrees: f64);
    pub fn scale_selection(&mut self, sx: f64, sy: f64);
    pub fn skew_selection(&mut self, sx: f64, sy: f64);
    pub fn duplicate_selection(&mut self) -> Vec<EntityId>;
    pub fn remove_overlap(&mut self);
}
```

The core method is `transform_selection(affine)`. All others are
convenience wrappers that compute the right Affine and call it.

### Transform Implementation Detail

`transform_selection(affine)`:
1. Collect selected point IDs + adjacent off-curve handles (same logic
   as `move_selection`)
2. For each point to transform:
   - `new_pos = affine * point.pos`
3. After transform, snap on-curve points to grid
4. Run `enforce_smooth_constraints` with all transformed points as
   "disturbed"

**Flip**: `Affine::translate(center) * Affine::scale_non_uniform(-1, 1)
* Affine::translate(-center)` (horizontal). Note: flip reverses path
direction, so we should reverse the affected contours to maintain
correct winding. Detect by checking `affine.determinant() < 0`.

**Rotate 90**: `Affine::rotate_about(PI/2, center)` — snap to grid
after since rotated coords may be off-grid.

### Duplicate Selection

`duplicate_selection()`:
1. Collect all paths that contain selected points
2. Clone those paths with fresh EntityIds
3. Offset by (+20, +20) in design space
4. Append to paths list
5. Set selection to the new points
6. Return new IDs

### Repeat Last Transform

Store in EditSession:
```rust
pub last_transform: Option<Affine>,
```

"Duplicate + repeat" (Cmd+Shift+D):
1. Duplicate selection
2. If `last_transform` is set, apply it to the new selection
3. Update `last_transform` (stays the same — enables chaining)

---

## Boolean Operations

### Crate Choice: `linesweeper`

**Primary choice**: `linesweeper` (v0.1.0) by jneem (Linebender
contributor). Works natively with `kurbo::BezPath`. Early but
functional.

**Fallback**: `flo_curves` (v0.8). More mature but requires conversion
to/from its own path types.

### Boolean Operations Flow

```
Path (our type) -> kurbo::BezPath -> boolean op -> kurbo::BezPath -> Path
```

The tricky part is the last step: converting a `kurbo::BezPath` back to
our `Path` type with proper `EntityId`s and point types. We need a
`Path::from_bezpath()` constructor.

### Operations

| Operation | Description | UI |
|-----------|-------------|-----|
| Union (Remove Overlap) | Merge all contours into one | Cmd+Shift+O + panel button |
| Subtract | Remove selected from unselected | Panel button |
| Intersect | Keep only overlapping area | Panel button |
| Exclude (XOR) | Keep non-overlapping areas | Panel button |

**Union is highest priority** — it's the most commonly used boolean op
in font editing.

### BezPath -> Path Conversion

A `kurbo::BezPath` from boolean ops contains `MoveTo`, `LineTo`,
`CurveTo`, `ClosePath` elements. To convert back:

1. Split into sub-paths (each `MoveTo` starts a new contour)
2. For each sub-path, create `PathPoint`s with fresh `EntityId`s
3. Classify: `MoveTo`/`LineTo` endpoints -> OnCurve,
   `CurveTo` control points -> OffCurve, `CurveTo` endpoint -> OnCurve
4. Detect smooth points: if incoming/outgoing tangent vectors are
   collinear at an on-curve point, mark it `smooth: true`
5. Create `CubicPath` (boolean ops produce cubics)

---

## Transform Panel Design

A right-side panel similar to the Glyphs Transformations palette,
positioned in the editor view alongside the existing coordinate panel
and glyph info panel. Toggled with Tab along with other panels.

### Layout (Top to Bottom)

```
+----------------------------------+
|  Transformations                 |
+----------------------------------+
|                                  |
|  [Flip H]  [Flip V]             |  Flip buttons
|                                  |
+----------------------------------+
|                                  |
|  Scale X  [____100%]  [+] [-]   |  Scale fields
|  Scale Y  [____100%]  [+] [-]   |  with lock icon
|  [lock icon]                     |  for proportional
|                                  |
+----------------------------------+
|                                  |
|  Rotate   [_____90]   [CW][CCW] |  Rotate field
|                                  |
+----------------------------------+
|                                  |
|  Skew X   [______5]   [+] [-]   |  Skew fields
|  Skew Y   [______5]   [+] [-]   |
|                                  |
+----------------------------------+
|                                  |
|  [Union] [Subtract]             |  Boolean ops
|  [Intersect] [Exclude]          |
|                                  |
+----------------------------------+
```

### Panel Implementation

- **File**: `src/components/transform_panel.rs`
- **View function**: `transform_panel(state: &AppState) -> impl
  WidgetView<AppState>`
- **Position**: Right side of editor, below coordinate panel (or
  stacked with it)
- **Width**: 240px (matching coordinate panel width)
- **Toggle**: Shares `panels_visible` with other panels

### Panel Behavior

- **Flip buttons**: Immediate action, no numeric input. Call
  `session.flip_selection_horizontal()` / `vertical()` directly.
- **Scale fields**: Default 100%. User types a value and presses Enter
  to apply. [+]/[-] buttons increment/decrement by 10%.
- **Rotate field**: Default 0. User types degrees and presses Enter.
  [CW]/[CCW] buttons apply the current value clockwise/counterclockwise.
- **Skew fields**: Default 0. Same Enter-to-apply pattern.
- **Boolean buttons**: Immediate action. Grayed out if fewer than 2
  contours exist.
- **All operations** record undo and sync to workspace.

### Panel Component Architecture

Follow the same pattern as `coordinate_panel.rs`:
- Custom widget with `paint()` for rendering
- Hit-test regions for buttons
- Text input fields for numeric values
- Dispatches actions to EditSession methods

Alternatively, since Xilem now has better text input support, build it
with Xilem views (`flex_col`, `flex_row`, `label`, `textbox`, `button`)
similar to `glyph_info_panel.rs` — this would be simpler and more
maintainable.

### Editor View Integration

In `src/views/editor.rs`, add the transform panel to the zstack layout:

```rust
// Right side panels (when panels_visible)
if session.panels_visible {
    // Existing: coordinate panel (bottom-right)
    // New: transform panel (right side, above coordinate panel)
    positioned(transform_panel(state), right_x, transform_y)
}
```

---

## Implementation Checklist

### Phase 1: Core Transform Infrastructure
- [x] Add `selection_bounding_box()` method to EditSession
- [x] Add `transform_selection(affine: Affine)` to EditSession
  - Collects selected points + adjacent off-curve handles
  - Applies affine to each point
  - Reverses contour winding if determinant < 0 (reflections)
  - Enforces smooth constraints on all transformed points
- [x] Add `EditType::Transform` variant for undo grouping
- [x] Add `last_transform: Option<Affine>` field to EditSession

### Phase 2: Flip Operations
- [x] Add `flip_selection_horizontal()` to EditSession
- [x] Add `flip_selection_vertical()` to EditSession
- [x] Wire Shift+H shortcut in keyboard.rs
- [x] Wire Shift+V shortcut in keyboard.rs
- [ ] Test: flip a few points, flip entire contour, flip with smooth
  constraints

### Phase 3: Rotate Operations
- [x] Add `rotate_selection(degrees: f64)` to EditSession
- [x] Wire Cmd+Shift+R (90 CW) in keyboard.rs
- [x] Wire Cmd+Shift+L (90 CCW) in keyboard.rs
- [ ] Test: rotate 90 with grid snap, rotate with smooth points

### Phase 4: Duplicate + Repeat
- [x] Add `duplicate_selection()` to EditSession
- [x] Wire Cmd+D shortcut in keyboard.rs
- [x] Add repeat-last-transform logic
  - Stores last_transform after flip/rotate/scale
  - Cmd+Shift+D: duplicate + apply last_transform
- [x] Wire Cmd+Shift+D shortcut in keyboard.rs

### Phase 5: Scale + Skew
- [x] Add `scale_selection(sx, sy)` to EditSession
- [x] Add `skew_selection(sx, sy)` to EditSession
- [ ] Wire to transform panel (no keyboard shortcut planned)

### Phase 6: Boolean Operations (Union)
- [ ] Add `linesweeper` to Cargo.toml dependencies
- [ ] Add `Path::from_bezpath(bezpath: &kurbo::BezPath)` constructor
  - Split BezPath into sub-paths
  - Create PathPoints with fresh EntityIds
  - Auto-detect smooth points from tangent continuity
- [ ] Add `remove_overlap()` to EditSession
  - Convert all paths to BezPath
  - Iteratively union
  - Convert back to Path objects
  - Replace paths, clear selection, record undo
- [ ] Wire Cmd+Shift+O shortcut in keyboard.rs
- [ ] Test: overlapping rectangles, overlapping curves

### Phase 7: Additional Boolean Operations
- [ ] Add `subtract_selection()` to EditSession
  - Selected contours subtracted from unselected
- [ ] Add `intersect_selection()` to EditSession
- [ ] Add `exclude_selection()` (XOR) to EditSession

### Phase 8: Transform Panel UI
- [ ] Create `src/components/transform_panel.rs`
  - Panel header "Transformations"
  - Flip H / Flip V buttons
  - Scale X/Y fields with +/- buttons and lock toggle
  - Rotate field with CW/CCW buttons
  - Skew X/Y fields with +/- buttons
  - Boolean op buttons (Union, Subtract, Intersect, Exclude)
- [ ] Add transform panel to editor view layout
  (src/views/editor.rs)
  - Position on right side, toggled with other panels
- [ ] Wire panel buttons to EditSession methods
- [ ] Wire numeric fields (Enter to apply)
- [ ] Gray out boolean buttons when < 2 contours

### Phase 9: README + Documentation
- [x] Add "Transforms" section to README keyboard shortcuts
- [ ] Document transform panel in README Features section

---

## Files to Create/Modify

| File | Changes |
|------|---------|
| `src/editing/session/mod.rs` | Add `last_transform` field, `selection_bounding_box()` |
| `src/editing/session/path_editing.rs` | Add `transform_selection()`, flip/rotate/scale/skew/duplicate methods |
| `src/editing/edit_types.rs` | Add `EditType::Transform` |
| `src/components/editor_canvas/keyboard.rs` | Wire new shortcuts |
| `src/path/mod.rs` | Add `Path::from_bezpath()` constructor |
| `src/path/cubic.rs` | Add `CubicPath::from_bezpath()` |
| `src/components/transform_panel.rs` | **New file** — transform panel component |
| `src/components/mod.rs` | Register transform_panel module |
| `src/views/editor.rs` | Add transform panel to editor layout |
| `Cargo.toml` | Add `linesweeper` dependency (Phase 6) |
| `README.md` | Add Transforms shortcut section |

---

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| linesweeper too buggy | Fall back to flo_curves with conversion layer |
| BezPath->Path loses smooth info | Tangent-angle heuristic to detect smooth points |
| Flip reverses winding | Auto-reverse contours when determinant < 0 |
| Grid snap after rotate | Round to snap grid; user can adjust |
| Smooth constraint cascade | Already solved with iterative enforcement |
| Panel text input UX | Start with simple Enter-to-apply; iterate |

---

## Open Questions

1. **Partial selection transforms**: Should flip/rotate apply to entire
   contours only, or also work on a few selected points within a
   contour? **Recommendation: support both** — partial selection is
   useful for tweaking curves.

2. **Winding correction after flip**: Should we auto-reverse contour
   direction? **Recommendation: yes** — detect via affine determinant.

3. **Remove overlap scope**: All contours or only selected?
   **Recommendation: all contours** when nothing selected (Glyphs
   behavior), selected contours only when there's a selection.

4. **Panel position**: Should the transform panel replace the glyph
   info panel, stack below it, or be a separate toggleable panel?
   **Recommendation: stack on right side**, toggled with Tab alongside
   other panels. Can revisit layout as more panels are added.
