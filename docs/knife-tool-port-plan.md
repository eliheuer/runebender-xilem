# Knife Tool Port Plan: Druid to Xilem

## Overview

Port the knife tool from Runebender Druid to Runebender Xilem. The knife tool allows users to cut paths by drawing a line across them, splitting them into multiple contours.

**Source**: https://github.com/linebender/runebender/blob/master/runebender-lib/src/tools/knife.rs

---

## Architecture Comparison

| Aspect | Druid (Source) | Xilem (Target) |
|--------|----------------|----------------|
| Tool trait | `Tool` with keyboard/mouse/paint | `Tool` extends `MouseDelegate` |
| Mouse events | `MouseDelegate` trait | Same pattern, different methods |
| Rendering | Piet `RenderContext` | Vello `Scene` |
| Path types | `norad`-based | Custom `CubicPath`, `HyperPath` |
| State machine | Enum in tool | Same pattern |

---

## Implementation Plan

### Phase 1: Foundation Setup

#### Task 1.1: Add ToolId variant
- **File**: `src/tools/mod.rs`
- Add `Knife` to `ToolId` enum

#### Task 1.2: Add ToolBox variant
- **File**: `src/tools/mod.rs`
- Add `Knife(KnifeTool)` to `ToolBox` enum
- Implement all match arms for the new variant

#### Task 1.3: Create knife tool module
- **File**: `src/tools/knife.rs`
- Create basic struct with state enum
- Import required types

---

### Phase 2: Core Data Structures

#### Task 2.1: Define KnifeTool struct
```rust
pub struct KnifeTool {
    gesture: GestureState,
    shift_locked: bool,
    intersections: Vec<Intersection>,
}

enum GestureState {
    Ready,
    Cutting { start: Point, current: Point },
}

struct Intersection {
    point: Point,
    path_index: usize,
    segment_info: SegmentInfo,
    t: f64,
}
```

---

### Phase 3: Line-Path Intersection Algorithm

This is the most complex part - finding where the knife line crosses paths.

#### Task 3.1: Implement line-segment intersection
- **File**: `src/path_segment.rs` (add helper methods)
- Line-line intersection
- Line-cubic bezier intersection (requires root finding)
- Line-quadratic bezier intersection

#### Task 3.2: Implement intersection finding
- **File**: `src/tools/knife.rs`
- Iterate all paths and segments
- Find all intersections with knife line
- Sort intersections along the knife line
- Store with path index, segment info, and parametric t

**Algorithm notes**:
- For cubic beziers, use kurbo's `CubicBez::intersect_line()` or implement bezier clipping
- Handle edge cases: tangent touches, endpoint intersections
- Filter duplicate intersections at path corners

---

### Phase 4: Path Splitting Algorithm

#### Task 4.1: Add path splitting to EditSession
- **File**: `src/edit_session.rs`
- Method to split a path at two parametric points
- Uses `Segment::subdivide_cubic()` already in codebase

#### Task 4.2: Implement recursive slice algorithm
- **File**: `src/tools/knife.rs`
- For each pair of intersections on same path:
  1. Split path at both points
  2. Create two new closed paths
  3. Insert closing line segments
- Handle complex cases:
  - Multiple cuts on same path
  - Cuts that create more than 2 pieces
  - Open vs closed paths

**Core algorithm** (from Druid):
```rust
fn slice_path_impl(
    line: Line,
    path_idx: usize,
    paths: &mut Vec<Path>,
    depth: usize,
) {
    // 1. Find intersections
    // 2. If < 2 intersections, return
    // 3. Split at first two intersections
    // 4. Recursively process remaining line
}
```

---

### Phase 5: Mouse Event Handling

#### Task 5.1: Implement MouseDelegate
- **File**: `src/tools/knife.rs`

| Method | Behavior |
|--------|----------|
| `left_down` | Record start point, initialize state |
| `left_drag_changed` | Update current point, recalculate intersections |
| `left_drag_ended` | Execute path slicing, reset state |
| `cancel` | Reset to Ready state |
| `mouse_moved` | Track position for shift-lock preview |

#### Task 5.2: Implement shift-lock constraint
- When Shift is held, constrain line to horizontal/vertical
- Lock direction based on initial movement (>45° = vertical)

---

### Phase 6: Visual Rendering

#### Task 6.1: Implement Tool::paint
- **File**: `src/tools/knife.rs`

Draw:
1. **Knife line**: Dashed line from start to current point
2. **Intersection markers**: Small perpendicular lines at each intersection

```rust
fn paint(&mut self, scene: &mut Scene, session: &EditSession, transform: &Affine) {
    if let GestureState::Cutting { start, current } = self.gesture {
        // Draw dashed knife line
        let line = Line::new(
            session.viewport.to_screen(start),
            session.viewport.to_screen(current),
        );
        let stroke = Stroke::new(1.0).with_dashes(0.0, [4.0, 4.0]);
        scene.stroke(&stroke, Affine::IDENTITY, Color::ORANGE, None, &line);

        // Draw intersection markers
        for intersection in &self.intersections {
            // Draw perpendicular marks
        }
    }
}
```

---

### Phase 7: Integration & Testing

#### Task 7.1: Register tool in mod.rs
- Export knife module
- Add to `ToolBox::for_id()`

#### Task 7.2: Add keyboard shortcut
- **File**: Need to find where shortcuts are defined
- Typical shortcut: `K` for knife

#### Task 7.3: Add to toolbar (if applicable)
- Need to locate toolbar widget
- Add knife tool button/icon

#### Task 7.4: Testing
- Test simple rectangle cut
- Test curved path cut
- Test multiple path cuts
- Test shift-constrained cuts
- Test edge cases (tangent, endpoint)

---

## Dependencies & Utilities Needed

### Existing (can reuse)
- `Segment::subdivide_cubic()` - bezier subdivision
- `Segment::subdivide_quadratic()` - quadratic subdivision
- `SegmentInfo` - segment metadata
- `PathPoints` - point storage
- `EntityId` - unique identifiers

### Need to implement
- Line-bezier intersection (may use kurbo if available)
- Path splitting at multiple points
- Path reconstruction after cutting

---

## Checklist

### Phase 1: Foundation
- [ ] Add `Knife` to `ToolId` enum
- [ ] Add `Knife(KnifeTool)` to `ToolBox` enum
- [ ] Implement all `ToolBox` match arms
- [ ] Create `src/tools/knife.rs` with basic structure

### Phase 2: Data Structures
- [ ] Define `KnifeTool` struct
- [ ] Define `GestureState` enum
- [ ] Define `Intersection` struct

### Phase 3: Intersection Algorithm
- [ ] Implement line-line intersection
- [ ] Implement line-cubic intersection
- [ ] Implement line-quadratic intersection (if needed)
- [ ] Find all intersections for a knife line
- [ ] Sort intersections along knife line

### Phase 4: Path Splitting
- [ ] Implement path split at single point
- [ ] Implement path split into two contours
- [ ] Implement recursive multi-cut algorithm
- [ ] Handle open vs closed paths
- [ ] Update EditSession with split operation

### Phase 5: Mouse Events
- [ ] Implement `left_down`
- [ ] Implement `left_drag_changed`
- [ ] Implement `left_drag_ended`
- [ ] Implement `cancel`
- [ ] Implement shift-lock constraint

### Phase 6: Rendering
- [ ] Draw knife line (dashed)
- [ ] Draw intersection markers
- [ ] Use correct coordinate transforms

### Phase 7: Integration
- [ ] Export from `tools/mod.rs`
- [ ] Add keyboard shortcut
- [ ] Add to toolbar (if exists)
- [ ] Test basic cuts
- [ ] Test curved cuts
- [ ] Test edge cases

---

## Risk Areas

1. **Line-bezier intersection**: Most mathematically complex part. May need to use kurbo's intersection methods or implement bezier clipping algorithm.

2. **Path reconstruction**: After splitting, need to properly reconstruct paths with correct point types, IDs, and handle tangent continuity.

3. **Multiple cuts**: Recursive algorithm needs careful handling to avoid infinite loops and handle all edge cases.

4. **Coordinate spaces**: Must correctly transform between screen (mouse) and design (path) coordinates.

5. **Undo integration**: Path splitting creates new paths and modifies existing ones - need proper undo support.

---

## Estimated Complexity

- **Phase 1-2**: Low - straightforward boilerplate
- **Phase 3**: High - mathematical intersection algorithms
- **Phase 4**: High - path manipulation and reconstruction
- **Phase 5**: Medium - event handling patterns exist
- **Phase 6**: Low - rendering patterns exist
- **Phase 7**: Medium - integration and testing

---

## Notes

- The original Druid implementation uses `norad` paths; we use custom `CubicPath`/`HyperPath`
- May need to add methods to `CubicPath` for splitting operations
- Consider whether knife should work on `HyperPath` (would need to convert to cubic first)
- The knife tool should only modify selected paths, or all paths if none selected
