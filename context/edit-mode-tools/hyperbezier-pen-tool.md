# Hyperbezier Pen Tool - Porting Plan

## Overview

The hyperbezier pen tool is a novel drawing tool that uses a special family of curves designed to maintain G2 continuity (smooth curvature transitions) automatically. Unlike traditional cubic beziers where users must carefully position handles to maintain smooth curves, hyperbezier curves use "auto-points" that adjust automatically.

## Reference Links

- [Runebender pen.rs source](https://github.com/linebender/runebender/blob/master/runebender-lib/src/tools/pen.rs)
- [Hyperbezier blog post (cmyr.net)](https://www.cmyr.net/blog/hyperbezier.html)
- [Hacker News discussion](https://news.ycombinator.com/item?id=25554205)
- [Affinity forum discussion](https://forum.affinity.serif.com/index.php?/topic/130175-hyperbezier-pen-tool/)
- [Spline crate](https://github.com/linebender/spline)

---

## How Hyperbezier Works

### Core Mathematical Model

The hyperbezier is a 4-parameter curve family that:
- Uses two on-curve points and two off-curve (control) points per segment
- Enforces G2 continuity by construction (no manual handle adjustment needed)
- When both bias values are 1.0, the curve is an Euler spiral
- Uses Cesàro equations with a doubly nested integral approach

### Point Types

**On-Curve Points:**
- **Corner points** (green squares): Allow sharp direction changes
- **Smooth points** (blue circles): Automatically adjust neighboring auto-points

**Off-Curve Points:**
- **Auto-points** (dashed lines with 'x'): Automatically positioned
- **Manual points** (solid lines with 'o'): User-controlled

### Interaction Model

**Pen Tool Operations:**
- Click: Add line segment with corner point
- Alt + click: Add automatic curve segment with smooth point
- Click + drag: Create curve segment with manual control point
- Alt + existing point: Toggle point type

**Selection Tool:**
- Drag auto-points to convert to manual control
- Double-click on-curve points to toggle smooth/corner
- Arrow keys nudge selections (1px, 10px with shift, 100px with ctrl/cmd)

---

## Current runebender-xilem Architecture

### Tool System (`src/tools/`)

**Files:**
- `mod.rs` - Tool trait, ToolId enum, ToolBox wrapper
- `pen.rs` - PenTool implementation
- `select.rs` - SelectTool implementation
- `preview.rs` - PreviewTool implementation

**Key Structures:**

```rust
// ToolId enum - add HyperPen variant here
pub enum ToolId {
    Select,
    Pen,
    Preview,
    // HyperPen,  // Add this
}

// ToolBox enum - wraps all tools
pub enum ToolBox {
    Select(select::SelectTool),
    Pen(pen::PenTool),
    Preview(preview::PreviewTool),
    // HyperPen(hyper_pen::HyperPenTool),  // Add this
}
```

### Current PenTool State

```rust
pub struct PenTool {
    current_path_points: Vec<PathPoint>,
    drawing: bool,
    mouse_pos: Option<kurbo::Point>,
    snapped_segment: Option<(SegmentInfo, f64)>,
}
```

**Limitations:**
- Only creates corner points (no smooth points)
- No drag-based handle drawing
- No modifier key support (Alt for smooth points)

### Path System (`src/path.rs`)

```rust
pub enum Path {
    Cubic(CubicPath),
    Quadratic(QuadraticPath),
    // Hyperbezier(HyperbezierPath),  // Add this
}
```

The path system already has a comment: "HyperPath support can be added later."

### Point Types (`src/point.rs`)

```rust
pub enum PointType {
    OnCurve { smooth: bool },
    OffCurve { auto: bool },
}
```

The `auto` field in `OffCurve` already exists for auto-points, but we may need additional fields.

### Edit Mode Toolbar (`src/components/edit_mode_toolbar.rs`)

```rust
const TOOLBAR_TOOLS: &[ToolId] = &[ToolId::Select, ToolId::Pen, ToolId::Preview];
```

Tools are displayed based on `TOOLBAR_TOOLS` array. Each tool needs:
- Icon function (returns `BezPath`)
- Entry in `icon_for_tool()` match

---

## Implementation Plan

### Phase 1: Foundation

#### 1.1 Add spline crate dependency

Add to `Cargo.toml`:
```toml
spline = { git = "https://github.com/linebender/spline" }
```

The spline crate provides:
- `SplinePoint` type
- Spline solving/evaluation
- Curve conversion utilities

#### 1.2 Extend PointType (if needed)

Consider adding hyperbezier-specific point attributes:
```rust
pub enum PointType {
    OnCurve { smooth: bool },
    OffCurve {
        auto: bool,
        // Maybe: bias or curvature hint for hyperbezier
    },
}
```

#### 1.3 Add HyperbezierPath

Create `src/hyperbezier_path.rs`:
```rust
pub struct HyperbezierPath {
    pub points: PathPoints,
    pub closed: bool,
    pub id: EntityId,
}
```

Update `Path` enum in `src/path.rs`:
```rust
pub enum Path {
    Cubic(CubicPath),
    Quadratic(QuadraticPath),
    Hyperbezier(HyperbezierPath),
}
```

### Phase 2: Tool Implementation

#### 2.1 Add HyperPen ToolId

In `src/tools/mod.rs`:
```rust
pub enum ToolId {
    Select,
    Pen,
    HyperPen,  // New
    Preview,
}
```

#### 2.2 Create HyperPenTool

Create `src/tools/hyper_pen.rs`:

```rust
pub struct HyperPenTool {
    current_path_points: Vec<PathPoint>,
    drawing: bool,
    mouse_pos: Option<kurbo::Point>,
    snapped_segment: Option<(SegmentInfo, f64)>,
    // Hyperbezier-specific state:
    drag_start: Option<kurbo::Point>,
    pending_handle: Option<kurbo::Point>,
}
```

Key differences from PenTool:
- Handle drag-based point creation
- Support Alt modifier for smooth/corner toggle
- Create hyperbezier paths instead of cubic

#### 2.3 Implement MouseDelegate

**left_click:**
- Check for segment snap (insert point on curve)
- Check for path close
- Alt + click: Add smooth point with auto-handles
- Click: Add corner point

**left_drag_began:**
- Record drag start position
- Prepare for handle positioning

**left_drag_changed:**
- Update pending handle position
- Show preview of control point

**left_drag_ended:**
- Create point with manual handle
- Convert from corner to smooth if dragged

#### 2.4 Update ToolBox

In `src/tools/mod.rs`:
```rust
pub enum ToolBox {
    Select(select::SelectTool),
    Pen(pen::PenTool),
    HyperPen(hyper_pen::HyperPenTool),  // New
    Preview(preview::PreviewTool),
}

impl ToolBox {
    pub fn for_id(id: ToolId) -> Self {
        match id {
            // ...
            ToolId::HyperPen => ToolBox::HyperPen(hyper_pen::HyperPenTool::default()),
        }
    }
}
```

Add all the delegate method dispatches for the new variant.

### Phase 3: Toolbar Integration

#### 3.1 Add Toolbar Button

In `src/components/edit_mode_toolbar.rs`:

```rust
const TOOLBAR_TOOLS: &[ToolId] = &[
    ToolId::Select,
    ToolId::Pen,
    ToolId::HyperPen,  // New
    ToolId::Preview,
];
```

#### 3.2 Create Hyperbezier Icon

Add a `hyper_pen_icon()` function. The original Runebender uses a distinct icon for the hyperbezier pen. Consider:
- A pen with a curve symbol
- A pen with an 'H' marker
- Or use the icon from VirtuaGrotesk if available

```rust
fn hyper_pen_icon() -> BezPath {
    // TODO: Design icon
    let mut bez = BezPath::new();
    // ... bezier path data
    bez
}
```

Update `icon_for_tool()`:
```rust
fn icon_for_tool(tool: ToolId) -> BezPath {
    match tool {
        ToolId::Select => select_icon(),
        ToolId::Pen => pen_icon(),
        ToolId::HyperPen => hyper_pen_icon(),  // New
        ToolId::Preview => preview_icon(),
    }
}
```

### Phase 4: Rendering & Solving

#### 4.1 Spline Solving

The spline crate provides curve solving. Key functions:
- Solve for auto-point positions given on-curve points
- Evaluate curve at parameter t
- Convert to cubic bezier for rendering

#### 4.2 Preview Rendering

In HyperPenTool's `paint()` method:
- Draw current path points
- Show auto-handles with dashed lines and 'x' markers
- Show manual handles with solid lines and 'o' markers
- Draw the actual hyperbezier curve (converted to beziers)

#### 4.3 Final Path Rendering

The Path::Hyperbezier variant needs:
- `to_bezpath()` - Convert to kurbo BezPath for rendering
- This likely involves approximating the hyperbezier with cubics

### Phase 5: Persistence & Interop

#### 5.1 Save/Load

HyperbezierPath needs:
- `to_contour()` - Convert to norad format for saving
- `from_contour()` - Load from norad format

**Challenge:** UFO format doesn't have native hyperbezier support. Options:
1. Convert to cubic on save (loses hyperbezier semantics)
2. Store as custom extension/lib data
3. Use a separate sidecar file

#### 5.2 Path Conversion

Consider utilities for:
- `HyperbezierPath::to_cubic()` - Convert to CubicPath
- `CubicPath::to_hyperbezier()` - Attempt conversion (may not always work well)

---

## Design Decisions to Make

### 1. Separate Tool or Mode?

**Option A: Separate Tool (HyperPen)**
- Pro: Clear separation, easier to understand
- Pro: Can have different default behaviors
- Con: More code duplication with PenTool

**Option B: Mode in Pen Tool**
- Pro: Less code, shared infrastructure
- Pro: Matches original Runebender (which uses `hyperbezier_mode: bool`)
- Con: More complex state management

**Recommendation:** Start with Option A (separate tool) for clarity. Can refactor later if there's significant duplication.

### 2. Path Storage

**Option A: Native Hyperbezier Storage**
- Store as `Path::Hyperbezier` in memory
- Convert to cubic on save
- Keeps full edit semantics while drawing

**Option B: Cubic Storage with Metadata**
- Store as cubic paths with hyperbezier hints
- Simpler, but loses auto-point behavior

**Recommendation:** Option A - use native storage for the full hyperbezier experience.

### 3. Auto-Point Solving

**When to solve:**
- On every point add/move?
- On demand (lazy solving)?
- Continuously during drag?

**Recommendation:** Solve on every point change for responsive feel. May need optimization if slow.

---

## Files to Create/Modify

### New Files

1. `src/hyperbezier_path.rs` - HyperbezierPath struct and methods
2. `src/tools/hyper_pen.rs` - HyperPenTool implementation

### Modified Files

1. `Cargo.toml` - Add spline dependency
2. `src/lib.rs` or `src/main.rs` - Add module declarations
3. `src/path.rs` - Add Hyperbezier variant
4. `src/tools/mod.rs` - Add ToolId::HyperPen, ToolBox::HyperPen
5. `src/components/edit_mode_toolbar.rs` - Add to TOOLBAR_TOOLS, add icon
6. `src/edit_session.rs` - May need updates for hyperbezier path handling

---

## Implementation Order

1. **Add spline crate** - Get the dependency building
2. **Create HyperbezierPath** - Basic struct, defer solving
3. **Add ToolId/ToolBox** - Register the tool in the system
4. **Create minimal HyperPenTool** - Copy from PenTool, same behavior
5. **Add toolbar button** - Get it showing in UI
6. **Implement click-to-add** - Basic point placement
7. **Implement drag-to-handle** - Create manual handles
8. **Add modifier support** - Alt for smooth points
9. **Implement spline solving** - Auto-point positioning
10. **Add path rendering** - Convert to beziers
11. **Handle save/load** - Persistence

---

## Testing Strategy

### Manual Testing

1. Click to add corner points
2. Alt+click to add smooth points
3. Drag to create handles
4. Close paths by clicking first point
5. Cancel with Escape
6. Verify undo/redo works
7. Save and reload glyph

### Unit Tests

1. Spline solving produces valid results
2. Path conversion preserves points
3. Hit testing works on hyperbezier segments

---

## Known Challenges

### 1. Solver Robustness

From the HN discussion: "The solver is still a little rough and has some robustness issues when points are positioned very closely together."

**Mitigation:** Add bounds checking, handle degenerate cases gracefully.

### 2. UFO Compatibility

UFO format doesn't have hyperbezier curves. Need to:
- Convert to cubic on save
- Store original as lib data if round-tripping needed

### 3. Curve Conversion Quality

Converting hyperbezier to cubic may require many segments for accuracy.

**Mitigation:** Adaptive subdivision based on error tolerance.

### 4. Performance

Spline solving on every mouse move could be slow.

**Mitigation:**
- Only solve affected segments
- Debounce solving during drag
- Consider async solving

---

## References from Original Runebender

### Key State from pen.rs

```rust
pub struct Pen {
    hyperbezier_mode: bool,
    this_edit_type: Option<EditType>,
    state: State,
}

enum State {
    Ready,
    AddPoint(EntityId),
    DragHandle(EntityId),
}
```

### Mouse Handling Patterns

- `left_down()` - Point creation, segment splitting
- `left_up()` - Trailing segment cleanup
- `left_drag_began()` - Line-to-curve upgrades
- `left_drag_changed()` - Real-time handle updates
- `cancel()` - State reset

### Modifier Support

```rust
fn current_drag_pos(event: &MouseEvent) -> Point {
    // Axis-locking when shift is held
    if event.mods.shift() {
        // constrain to horizontal/vertical
    }
}
```

### Alt-Click Behavior

In hyperbezier mode, Alt+click on existing point toggles point type between smooth and corner.

---

## Next Steps

1. [ ] Study the spline crate API in detail
2. [ ] Design the icon for the toolbar
3. [ ] Set up the basic file structure
4. [ ] Implement Phase 1 (foundation)
5. [ ] Get a basic tool working (click to add points)
6. [ ] Add visual feedback for the tool
7. [ ] Implement spline solving
8. [ ] Full testing and refinement

---

## Progress Log

_Update this section as work progresses_

### Date: 2025-11-18 - Initial Port Complete

**Completed:**
- Added spline crate dependency to Cargo.toml
- Created `src/hyper_path.rs` with HyperPath struct
- Added `Path::Hyper` variant to the Path enum
- Created `src/tools/hyper_pen.rs` with HyperPenTool
- Added `ToolId::HyperPen` and `ToolBox::HyperPen`
- Updated all match statements in `edit_session.rs`, `select.rs`, `editor_canvas.rs`
- Added hyper pen icon to toolbar
- Build succeeds with only warnings

**Current State:**
- Tool appears in toolbar with 4 tools: Select, Pen, HyperPen, Preview
- Basic click-to-add points works
- Path rendering uses cubic bezier fallback (spline solver not yet integrated)
- Drag-to-create-handle not fully tested

**Next Steps:**
1. Investigate spline crate API and integrate proper curve solving
2. Test Alt+click for smooth point creation
3. Test path closing behavior
4. Add proper visual feedback for auto vs manual points
5. Test save/load round-trip

**Known Limitations:**
- Spline solver not integrated - curves render as cubics based on point positions
- No shift-lock during drag (simplified for now)
- No keyboard support (backspace to delete, etc.)

### Date: [Initial]
- Created this planning document
- Analyzed runebender-xilem architecture
- Studied original Runebender implementation
- Gathered reference materials
