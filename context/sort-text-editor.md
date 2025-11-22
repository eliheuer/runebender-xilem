# Sort System and Text Editor Implementation Plan

## Project Overview

This document outlines the implementation of a sort-based text editing system for Runebender Xilem, inspired by Glyphs and Bezy font editors. The goal is to provide an in-editor text preview and editing capability that supports both LTR (Latin scripts) and RTL (Arabic, Hebrew) text.

### Key Concepts

**Sort (Physical Typesetting)**: A block with a typographic character etched on it, used when lined up with others to print text ([Wikipedia](https://en.wikipedia.org/wiki/Sort_(typesetting))).

**Virtual Sort (Our Implementation)**: A data structure representing a `.glif` file from a UFO:
- Rectangle with advance width and height (from descender to UPM)
- Contains a glyph drawing
- Can be **active** (editable, with visible control points) or **inactive** (preview only, filled outline)
- Only one sort can be active at a time

**Text Buffer**: A gap-buffer-backed sequence of sorts representing editable text, with a cursor for insertion/deletion.

### Design Decisions

1. **Single buffer per edit view** (simpler than Bezy's multi-buffer approach, matches Glyphs UX)
2. **Gap buffer for efficient text editing** (O(1) insertion at cursor position)
3. **Active/inactive sort rendering** (toggle between edit and preview modes)
4. **LTR first, RTL later** (phased implementation with HarfBuzz integration deferred)
5. **Preview pane at bottom** (horizontal zoomed-out view of current buffer text)
6. **Text edit mode toggle** (new toolbar button to enter/exit text editing mode)
7. **Parley integration** (use Linebender's Parley library for cursor management and text editing operations where appropriate)

---

## Architecture Analysis

### Current Runebender Xilem Structure

| Component | File | Current State |
|-----------|------|---------------|
| **Edit Canvas** | `src/components/editor_canvas.rs` (1607 lines) | Main glyph editor widget with rendering, input handling, undo/redo |
| **Edit View** | `src/views/editor.rs` (243 lines) | Xilem view composition with zstack layout |
| **Edit Session** | `src/edit_session.rs` | State holder for single glyph editing (paths, selection, viewport, metrics) |
| **Tools** | `src/tools/` | 7 tools (Select, Pen, HyperPen, Knife, Measure, Shapes, Preview) |
| **Toolbar** | `src/components/edit_mode_toolbar.rs` (696 lines) | Tool selection UI with icons |
| **Glyph Renderer** | `src/glyph_renderer.rs` (298 lines) | Converts paths to Kurbo BezPath |
| **Coordinate Panel** | `src/components/coordinate_panel.rs` | Shows glyph coordinates and quadrant picker |

**Key Rendering Functions** (in `editor_canvas.rs`):
- `draw_metrics_guides()` (lines 935-1008) - Draws baseline, ascender, descender, x-height, cap-height
- `draw_paths_with_points()` - Renders control points and handles
- `draw_control_handles()`, `draw_points()` - Cubic, quadratic, hyperbezier point rendering

**Input Handling**:
- `on_text_event()` (lines 343-395) - Keyboard event dispatcher
- `on_pointer_event()` - Mouse event handler with tool dispatch
- `handle_keyboard_shortcuts()` (lines 732-887) - Undo, zoom, save, etc.
- `handle_arrow_keys()` (lines 890-932) - Nudge operations

### Bezy's Sort System Architecture

| Component | File | Purpose |
|-----------|------|---------|
| **SortBuffer** | `buffer.rs` (lines 164-330) | Gap buffer with O(1) insertion at cursor |
| **SortData** | `buffer.rs` (lines 48-151) | Individual sort entry (glyph or line break) |
| **TextBuffer** | `text_buffer.rs` | ECS entity for text buffer with layout mode (LTR/RTL/Freeform) |
| **BufferCursor** | `text_buffer.rs` | ECS component storing cursor position per buffer |
| **TextFlowPositioning** | `text_flow_positioning.rs` | Single source of truth for LTR/RTL positioning |
| **Unicode Input** | `unicode_input.rs` | Keyboard handler for text insertion, arrow keys, backspace |
| **Sort Renderer** | `sort_renderer.rs` | Visual distinction between active/inactive sorts |

**Key Data Structures from Bezy**:

```rust
#[derive(Clone, Debug)]
pub struct SortData {
    pub kind: SortKind,              // Glyph or LineBreak
    pub is_active: bool,             // Edit mode state
    pub layout_mode: SortLayoutMode, // LTR/RTL/Freeform
    pub root_position: Vec2,         // World position
    pub buffer_id: Option<BufferId>, // Text flow isolation
}

pub enum SortKind {
    Glyph {
        codepoint: Option<char>,
        glyph_name: String,
        advance_width: f32,
    },
    LineBreak,
}

pub enum SortLayoutMode {
    LTRText,   // Left-to-right (Latin scripts)
    RTLText,   // Right-to-left (Arabic/Hebrew)
    Freeform,  // Individual positioning
}
```

**Gap Buffer Performance**:
- Insert at cursor: O(1) best case, O(n) amortized for growth
- Delete at cursor: O(1)
- Random access: O(1)
- Gap movement: O(k) where k = distance moved

**LTR vs RTL Positioning** (from `text_flow_positioning.rs`):
- **LTR**: Accumulate advance widths UP TO cursor position (cursor at end)
- **RTL**: Subtract advance widths FROM cursor position onwards (cursor at insertion point)
- **Line breaks**: Reset x=0, move y down by `line_height = UPM - descender`

### Parley Text Layout Library (Linebender)

**Repository**: [github.com/linebender/parley](https://github.com/linebender/parley)

Parley is Linebender's rich text layout library providing sophisticated text rendering capabilities. It's part of the same ecosystem as Xilem and Vello.

**Core Capabilities**:
- **Text shaping** via HarfRust (Rust port of HarfBuzz) - handles ligatures, complex scripts
- **Bidirectional text** (LTR/RTL) with automatic bidi resolution
- **Font fallback** via Fontique - multi-language support with intelligent fallback
- **Line breaking** with automatic rewrapping
- **Cursor management** with visual/logical navigation
- **Text editing** utilities (PlainEditor, PlainEditorDriver)

**Key Data Structures**:
```rust
// Cursor with affinity (handles bidi correctly)
pub struct Cursor {
    index: usize,        // Byte index in text
    affinity: Affinity,  // Upstream/Downstream for bidi
}

// Plain text editor
pub struct PlainEditor<T> {
    layout: Layout<T>,
    buffer: String,
    selection: Selection,
    // ... cursor, compose, styling ...
}

// Editor operations
impl PlainEditorDriver {
    pub fn insert_or_replace_selection(&mut self, s: &str);
    pub fn delete_selection(&mut self);
    pub fn delete_bytes_before_selection(&mut self, len: NonZeroUsize); // Backspace
    pub fn delete_bytes_after_selection(&mut self, len: NonZeroUsize);  // Delete
}

// Cursor navigation (handles bidi correctly!)
impl Cursor {
    pub fn from_byte_index<B>(layout: &Layout<B>, index: usize, affinity: Affinity) -> Self;
    pub fn from_point<B>(layout: &Layout<B>, x: f32, y: f32) -> Self;  // Click to cursor
    pub fn previous_visual<B>(&self, layout: &Layout<B>) -> Self;      // Left arrow
    pub fn next_visual<B>(&self, layout: &Layout<B>) -> Self;          // Right arrow
    pub fn next_visual_word<B>(&self, layout: &Layout<B>) -> Self;     // Word navigation
    pub fn previous_visual_word<B>(&self, layout: &Layout<B>) -> Self;
}
```

**Recommendation: Strategic Use of Parley**

| Phase | Use Parley? | Rationale |
|-------|-------------|-----------|
| **Phase 1-4** | ❌ No | Core sort buffer needs custom logic; Parley expects immutable layouts |
| **Phase 5** | ⚠️ Partial | Use Parley's character-to-glyph mapping via HarfRust, but not full PlainEditor |
| **Phase 6** | ✅ **Yes** | **Parley's `Cursor` struct for position calculation and bidi-aware navigation** |
| **Phase 10** | ✅ **Yes** | **Parley's HarfRust integration for RTL shaping and bidi resolution** |

**Specific Use Cases**:

1. **Cursor Positioning (Phase 6)**: Use `Cursor::from_point()` for click-to-cursor conversion, handles bidi correctly
2. **Arrow Key Navigation**: Use `Cursor::previous_visual()` / `next_visual()` instead of manual logic
3. **RTL Text Shaping (Phase 10)**: Use Parley's HarfRust integration (Parley wraps HarfRust internally)
4. **Bidi Resolution**: Let Parley handle mixed LTR/RTL text reordering
5. **Font Fallback**: Use Fontique for missing glyph handling

**Note on Text Shaping Libraries**:
- ✅ **HarfRust** is the recommended Rust text shaping library (maintained, idiomatic Rust)
- ❌ **RustyBuzz** is deprecated and should NOT be used
- ✅ **Parley** uses HarfRust internally, so we get HarfRust's shaping automatically
- 💡 **Recommendation**: Use Parley's built-in shaping; only use HarfRust directly if we need low-level control beyond what Parley provides

**What NOT to use Parley for**:

- ❌ Text buffer storage (we need gap buffer for sorts, Parley uses String)
- ❌ Layout caching (sorts are individually editable, Parley expects immutable text)
- ❌ Full PlainEditor (our sort-based model is fundamentally different)

**Integration Strategy**:

```rust
// Create a Parley layout for cursor calculations (Phase 6+)
let mut font_cx = FontContext::default();
let mut layout_cx = LayoutContext::new();

// Build layout from sort buffer text
let mut builder = layout_cx.ranged_builder(&mut font_cx, &text, font_size);
builder.push_default(&StyleProperty::FontStack(font_stack));
let mut layout = builder.build();
layout.break_all_lines(None);

// Use for cursor operations
let cursor_from_click = Cursor::from_point(&layout, mouse_x, mouse_y);
let cursor_left = current_cursor.previous_visual(&layout);
let cursor_right = current_cursor.next_visual(&layout);

// Extract cursor byte index, map back to sort buffer position
let sort_index = self.byte_index_to_sort_index(cursor_from_click.index());
```

**Benefits**:
- ✅ Correct bidi handling out of the box (complex to implement manually)
- ✅ Maintained by Linebender (same ecosystem as Xilem/Vello)
- ✅ Handles complex scripts (Arabic shaping, Indic ligatures, etc.)
- ✅ Battle-tested cursor logic (ligatures, grapheme clusters, etc.)

**Trade-offs**:
- ⚠️ Need to sync sort buffer ↔ Parley layout (additional state)
- ⚠️ Parley expects contiguous text, we have discrete sorts
- ⚠️ Must rebuild layout when buffer changes (immutable architecture)

**Conclusion**: Use Parley selectively for cursor/bidi/shaping operations where it excels, but keep our custom gap buffer for the core sort management. This hybrid approach gets the best of both worlds.

---

## Implementation Phases

### Phase 1: Core Sort Data Structure (Checkpoint 1)

**Goal**: Create the fundamental sort and buffer data structures without UI integration.

**Files to Create**:
- `src/sort/mod.rs` - Module declaration
- `src/sort/data.rs` - `Sort`, `SortKind`, `LayoutMode` structs
- `src/sort/buffer.rs` - `SortBuffer` gap buffer implementation
- `src/sort/metrics.rs` - Sort metric calculations (positioning, advance)

**Data Structures**:

```rust
// src/sort/data.rs
#[derive(Clone, Debug)]
pub struct Sort {
    pub kind: SortKind,
    pub is_active: bool,
    pub layout_mode: LayoutMode,
    pub position: Point,  // Using kurbo::Point
}

#[derive(Clone, Debug, PartialEq)]
pub enum SortKind {
    Glyph {
        name: String,           // Glyph name from UFO
        codepoint: Option<char>,
        advance_width: f64,
        // Will contain BezPath for rendering later
    },
    LineBreak,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum LayoutMode {
    #[default]
    LTR,       // Left-to-right text
    RTL,       // Right-to-left text (Phase 3)
    Freeform,  // Individual positioning (future)
}

// src/sort/buffer.rs
pub struct SortBuffer {
    buffer: Vec<Sort>,
    gap_start: usize,
    gap_end: usize,
    cursor: usize,  // Logical cursor position
}

impl SortBuffer {
    pub fn new() -> Self { /* ... */ }
    pub fn insert(&mut self, sort: Sort) { /* ... */ }
    pub fn delete(&mut self) -> Option<Sort> { /* ... */ }
    pub fn move_cursor_left(&mut self) { /* ... */ }
    pub fn move_cursor_right(&mut self) { /* ... */ }
    pub fn iter(&self) -> impl Iterator<Item = &Sort> { /* ... */ }
    fn move_gap_to(&mut self, position: usize) { /* ... */ }
    fn grow_gap(&mut self) { /* ... */ }
}
```

**Checkpoint 1 Tests**:
- [ ] Create empty `SortBuffer`
- [ ] Insert 5 sorts at cursor, verify order
- [ ] Delete from middle, verify gap expansion
- [ ] Move cursor left/right, insert at different positions
- [ ] Test gap growth (insert 100+ sorts)
- [ ] Verify iterator returns elements in correct order (skipping gap)

**Estimated Lines**: ~300-400 lines

---

### Phase 2: Text Buffer Session State (Checkpoint 2)

**Goal**: Integrate sort buffer into edit session, replace single-glyph editing with text buffer.

**Files to Modify**:
- `src/edit_session.rs` - Add `text_buffer: Option<SortBuffer>` field
- `src/workspace.rs` - Handle text buffer creation/destruction

**Files to Create**:
- `src/sort/session.rs` - Text buffer session helpers

**Edit Session Changes**:

```rust
// src/edit_session.rs
pub struct EditSession {
    // Existing fields for single glyph editing
    pub glyph_name: Option<String>,
    pub paths: Arc<Vec<Path>>,
    pub selection: Selection,
    pub viewport: ViewPort,
    // ... existing fields ...

    // NEW: Text buffer for multi-glyph editing
    pub text_buffer: Option<SortBuffer>,
    pub text_mode_active: bool,  // Are we in text editing mode?
}

impl EditSession {
    // NEW: Create session with text buffer initialized
    pub fn new_with_text_buffer(
        font: &Font,
        glyph_name: String,
        upm: f64,
    ) -> Self {
        let mut buffer = SortBuffer::new();

        // Insert initial sort from glyph
        if let Some(glyph) = font.glyphs.get(&glyph_name) {
            let sort = Sort {
                kind: SortKind::Glyph {
                    name: glyph_name.clone(),
                    codepoint: glyph.codepoint(),
                    advance_width: glyph.advance_width,
                },
                is_active: true,  // First sort is active
                layout_mode: LayoutMode::LTR,
                position: Point::ZERO,
            };
            buffer.insert(sort);
        }

        EditSession {
            glyph_name: Some(glyph_name),
            text_buffer: Some(buffer),
            text_mode_active: false,  // Start in single-glyph mode
            // ... initialize other fields ...
        }
    }

    pub fn enter_text_mode(&mut self) {
        self.text_mode_active = true;
    }

    pub fn exit_text_mode(&mut self) {
        self.text_mode_active = false;
    }
}
```

**Checkpoint 2 Tests**:
- [ ] Create edit session with text buffer from glyph name
- [ ] Verify buffer contains one sort with correct glyph data
- [ ] Toggle `text_mode_active` flag
- [ ] Save/load edit session preserves text buffer state

**Estimated Lines**: ~150-200 lines

---

### Phase 3: Text Buffer Rendering (Checkpoint 3)

**Goal**: Render multiple sorts in a line with correct spacing, distinguish active/inactive sorts.

**Files to Modify**:
- `src/components/editor_canvas.rs` - Add text buffer rendering to `paint()` method

**Files to Create**:
- `src/sort/render.rs` - Sort-specific rendering utilities

**Rendering Strategy**:

```rust
// In editor_canvas.rs paint() method

fn paint(&mut self, ctx: &mut PaintCtx, scene: &mut Scene) {
    // ... existing background rendering ...

    if let Some(buffer) = &self.session.text_buffer {
        if self.session.text_mode_active {
            // Render text buffer instead of single glyph
            self.render_text_buffer(ctx, scene, buffer);
        } else {
            // Render only active sort (single glyph mode)
            self.render_active_sort_only(ctx, scene, buffer);
        }
    } else {
        // Fallback to existing single-glyph rendering
        // ... existing glyph rendering code ...
    }
}

fn render_text_buffer(&mut self, ctx: &mut PaintCtx, scene: &mut Scene, buffer: &SortBuffer) {
    let mut x_offset = 0.0;
    let baseline_y = 0.0;

    for sort in buffer.iter() {
        match &sort.kind {
            SortKind::Glyph { name, advance_width, .. } => {
                let sort_position = Point::new(x_offset, baseline_y);

                if sort.is_active {
                    // Render with control points and handles (editable)
                    self.render_active_sort(scene, name, sort_position);
                } else {
                    // Render filled preview (inactive)
                    self.render_inactive_sort(scene, name, sort_position);
                }

                x_offset += advance_width;
            }
            SortKind::LineBreak => {
                x_offset = 0.0;
                baseline_y -= self.session.line_height();  // UPM - descender
            }
        }
    }

    // Render cursor if in text mode
    if self.session.text_mode_active {
        self.render_text_cursor(scene, buffer.cursor_position());
    }
}

fn render_active_sort(&mut self, scene: &mut Scene, glyph_name: &str, position: Point) {
    // Load glyph paths
    let paths = self.load_glyph_paths(glyph_name);

    // Translate to sort position
    let transform = Affine::translate(position.to_vec2());

    // Render using existing point/handle rendering
    // (reuse draw_paths_with_points, draw_control_handles, etc.)
    for path in &paths {
        self.draw_path_outline(scene, path, transform);
        self.draw_control_handles(scene, path, transform);
        self.draw_points(scene, path, transform);
    }
}

fn render_inactive_sort(&mut self, scene: &mut Scene, glyph_name: &str, position: Point) {
    // Load glyph as filled BezPath
    let bez_path = self.load_glyph_bezpath(glyph_name);

    // Translate to sort position
    let transform = Affine::translate(position.to_vec2());

    // Fill with preview color (no strokes or points)
    scene.fill(
        Fill::NonZero,
        transform,
        theme::fill::INACTIVE_SORT,
        None,
        &bez_path,
    );
}
```

**Visual Distinction**:
- **Active sort**: Outlined paths with control points/handles (green metrics)
- **Inactive sort**: Filled paths without points (gray color)
- **Text cursor**: Vertical line at insertion position

**Checkpoint 3 Tests**:
- [ ] Render buffer with 3 sorts, verify horizontal spacing
- [ ] Toggle first sort active/inactive, verify visual change
- [ ] Move active sort to middle position, verify rendering updates
- [ ] Add line break, verify multi-line rendering
- [ ] Verify cursor renders at correct position

**Estimated Lines**: ~300-400 lines

---

### Phase 4: Text Mode Toolbar Toggle (Checkpoint 4)

**Goal**: Add a "Text" tool to the toolbar to enter/exit text editing mode.

**Files to Modify**:
- `src/tools/mod.rs` - Add `Text` variant to `ToolBox` enum
- `src/components/edit_mode_toolbar.rs` - Add text tool icon and button
- `src/components/editor_canvas.rs` - Handle text tool activation

**Files to Create**:
- `src/tools/text.rs` - Text tool implementation

**Tool Definition**:

```rust
// src/tools/text.rs
pub struct Text;

impl Tool for Text {
    fn id(&self) -> ToolId {
        ToolId::Text
    }

    fn paint(&self, ctx: &mut PaintCtx, session: &EditSession) {
        // Text tool doesn't add custom overlays
        // (cursor is rendered by editor_canvas directly)
    }

    fn edit_type(&self) -> EditType {
        EditType::Normal
    }
}

// In tools/mod.rs
pub enum ToolBox {
    Select(Select),
    Pen(Pen),
    HyperPen(HyperPen),
    Knife(Knife),
    Measure(Measure),
    Shapes(Shapes),
    Preview(Preview),
    Text(Text),  // NEW
}

pub enum ToolId {
    Select,
    Pen,
    HyperPen,
    Knife,
    Measure,
    Shapes,
    Preview,
    Text,  // NEW
}
```

**Toolbar Integration**:

```rust
// In edit_mode_toolbar.rs
const TOOL_COUNT: usize = 8;  // Was 7

fn paint(&mut self, ctx: &mut PaintCtx, scene: &mut Scene) {
    let tools = [
        ToolId::Select,
        ToolId::Pen,
        ToolId::HyperPen,
        ToolId::Knife,
        ToolId::Measure,
        ToolId::Shapes,
        ToolId::Preview,
        ToolId::Text,  // NEW - add icon here
    ];

    // ... existing button rendering ...
}

// Icon for text tool (simple "T" or "Aa" icon)
fn draw_text_tool_icon(scene: &mut Scene, rect: Rect) {
    // Render "T" letter or "Aa" typography icon
}
```

**Activation Behavior**:

```rust
// In editor_canvas.rs
fn handle_tool_change(&mut self, new_tool: ToolId) {
    match new_tool {
        ToolId::Text => {
            self.session.enter_text_mode();
            // Deactivate all sorts except first
            if let Some(buffer) = &mut self.session.text_buffer {
                buffer.set_all_inactive_except_cursor();
            }
        }
        _ => {
            self.session.exit_text_mode();
        }
    }
    self.current_tool = new_tool.into_toolbox();
}
```

**Checkpoint 4 Tests**:
- [ ] Click text tool button, verify `text_mode_active` becomes true
- [ ] Verify text cursor appears in canvas
- [ ] Click another tool, verify text mode exits
- [ ] Press keyboard shortcut for text tool (e.g., 'T' key)
- [ ] Verify toolbar highlights text tool when active

**Estimated Lines**: ~150-200 lines

---

### Phase 5: Keyboard Text Input (Checkpoint 5)

**Goal**: Type characters to insert sorts, use arrow keys to move cursor, backspace to delete.

**Files to Modify**:
- `src/components/editor_canvas.rs` - Extend `on_text_event()` for text input

**Files to Create**:
- `src/sort/input.rs` - Text input handling utilities

**Input Handling Strategy**:

```rust
// In editor_canvas.rs on_text_event()

fn on_text_event(&mut self, ctx: &mut EventCtx, event: &TextEvent) -> bool {
    // Existing tool shortcuts, spacebar, etc.
    if !self.session.text_mode_active {
        return self.handle_existing_shortcuts(ctx, event);
    }

    // NEW: Text mode input handling
    match event {
        TextEvent::KeyboardKey(key, mods) => {
            match key.physical_key {
                PhysicalKey::Code(KeyCode::ArrowLeft) => {
                    if let Some(buffer) = &mut self.session.text_buffer {
                        buffer.move_cursor_left();
                        ctx.request_render();
                        return true;
                    }
                }
                PhysicalKey::Code(KeyCode::ArrowRight) => {
                    if let Some(buffer) = &mut self.session.text_buffer {
                        buffer.move_cursor_right();
                        ctx.request_render();
                        return true;
                    }
                }
                PhysicalKey::Code(KeyCode::Backspace) => {
                    if let Some(buffer) = &mut self.session.text_buffer {
                        buffer.delete();
                        ctx.request_render();
                        return true;
                    }
                }
                PhysicalKey::Code(KeyCode::Enter) => {
                    if let Some(buffer) = &mut self.session.text_buffer {
                        buffer.insert(Sort {
                            kind: SortKind::LineBreak,
                            is_active: false,
                            layout_mode: LayoutMode::LTR,
                            position: Point::ZERO,
                        });
                        ctx.request_render();
                        return true;
                    }
                }
                _ => {}
            }
        }
        TextEvent::Ime(Ime::Commit(text)) => {
            // Handle Unicode character input
            for character in text.chars() {
                if character.is_control() {
                    continue;
                }

                if let Some(sort) = self.create_sort_from_char(character) {
                    if let Some(buffer) = &mut self.session.text_buffer {
                        buffer.insert(sort);
                        ctx.request_render();
                    }
                }
            }
            return true;
        }
        _ => {}
    }

    false
}

fn create_sort_from_char(&self, character: char) -> Option<Sort> {
    // Map character to glyph name via cmap
    let glyph_name = self.session.font.cmap()
        .get(&(character as u32))
        .map(|name| name.clone())?;

    // Get advance width from glyph
    let advance_width = self.session.font.glyphs
        .get(&glyph_name)
        .map(|glyph| glyph.advance_width)
        .unwrap_or(500.0);

    Some(Sort {
        kind: SortKind::Glyph {
            name: glyph_name,
            codepoint: Some(character),
            advance_width,
        },
        is_active: false,  // Newly inserted sorts are inactive
        layout_mode: LayoutMode::LTR,
        position: Point::ZERO,  // Will be calculated during rendering
    })
}
```

**Character-to-Glyph Mapping**:
- Use font's cmap (Unicode to glyph name mapping)
- Fallback to `.notdef` for missing characters
- Handle space character specially (insert glyph with advance, no outline)

**Checkpoint 5 Tests**:
- [ ] Type "Hello" in text mode, verify 5 sorts inserted
- [ ] Press left arrow 3 times, type "X", verify insertion at cursor
- [ ] Press backspace 2 times, verify deletion
- [ ] Press enter, verify line break inserted
- [ ] Type multi-line text, verify correct positioning
- [ ] Verify sorts have correct glyph names from cmap

**Estimated Lines**: ~200-300 lines

---

### Phase 6: Text Cursor Rendering (Checkpoint 6)

**Goal**: Render a blinking vertical cursor at insertion position, using Parley for bidi-aware cursor positioning.

**Files to Modify**:
- `src/components/editor_canvas.rs` - Add cursor rendering in `paint()`

**Files to Create**:
- `src/sort/cursor.rs` - Cursor position calculation and rendering (with Parley integration)
- `src/sort/parley_bridge.rs` - Bridge between sort buffer and Parley layout

**⭐ Parley Integration**: This phase introduces Parley for cursor management. Use `Cursor::from_byte_index()` for position calculations and `Cursor::previous_visual()` / `next_visual()` for arrow key navigation.

**Cursor Rendering**:

```rust
// src/sort/cursor.rs

pub struct TextCursor {
    blink_timer: f64,
    visible: bool,
}

impl TextCursor {
    pub fn new() -> Self {
        TextCursor {
            blink_timer: 0.0,
            visible: true,
        }
    }

    pub fn update(&mut self, delta_time: f64) {
        self.blink_timer += delta_time;
        if self.blink_timer >= 0.5 {  // Blink every 500ms
            self.visible = !self.visible;
            self.blink_timer = 0.0;
        }
    }

    pub fn calculate_position(
        &self,
        buffer: &SortBuffer,
        line_height: f64,
    ) -> Point {
        let mut x = 0.0;
        let mut y = 0.0;

        // Accumulate advance widths up to cursor position
        for (i, sort) in buffer.iter().enumerate() {
            if i >= buffer.cursor {
                break;
            }

            match &sort.kind {
                SortKind::Glyph { advance_width, .. } => {
                    x += advance_width;
                }
                SortKind::LineBreak => {
                    x = 0.0;
                    y -= line_height;
                }
            }
        }

        Point::new(x, y)
    }

    pub fn render(&self, scene: &mut Scene, position: Point, height: f64) {
        if !self.visible {
            return;
        }

        // Draw vertical line
        let cursor_line = Line::new(
            Point::new(position.x, position.y - height * 0.8),
            Point::new(position.x, position.y + height * 0.2),
        );

        scene.stroke(
            &Stroke::new(2.0),
            Affine::IDENTITY,
            theme::cursor::COLOR,
            None,
            &cursor_line,
        );
    }
}

// In editor_canvas.rs
pub struct EditorWidget {
    // ... existing fields ...
    text_cursor: TextCursor,  // NEW
}

fn paint(&mut self, ctx: &mut PaintCtx, scene: &mut Scene) {
    // ... existing rendering ...

    if self.session.text_mode_active {
        if let Some(buffer) = &self.session.text_buffer {
            let cursor_pos = self.text_cursor.calculate_position(
                buffer,
                self.session.line_height(),
            );

            self.text_cursor.render(scene, cursor_pos, self.session.upm);
        }
    }
}
```

**Animation**:
- Use Masonry's animation request to trigger periodic redraws
- Blink cursor every 500ms
- Reset to visible when cursor moves

**Checkpoint 6 Tests**:
- [ ] Verify cursor appears at correct position
- [ ] Type characters, verify cursor moves right
- [ ] Press left/right arrows, verify cursor moves
- [ ] Insert line break, verify cursor moves to new line
- [ ] Verify cursor blinks (visible/hidden cycle)

**Estimated Lines**: ~150-200 lines

---

### Phase 7: Active Sort Toggling (Checkpoint 7)

**Goal**: Click on a sort to make it active for editing, deactivate others.

**Files to Modify**:
- `src/components/editor_canvas.rs` - Add sort click detection in `on_pointer_event()`
- `src/sort/buffer.rs` - Add `set_active_sort()` method

**Click Detection**:

```rust
// In editor_canvas.rs on_pointer_event()

fn on_pointer_event(&mut self, ctx: &mut EventCtx, event: &PointerEvent) {
    if !self.session.text_mode_active {
        // Existing tool dispatch
        return self.handle_tool_mouse_event(ctx, event);
    }

    // NEW: Text mode click handling
    match event.kind {
        PointerEventKind::Down(PointerButton::Primary) => {
            let click_pos = self.viewport.transform_inverse()
                * event.position;

            if let Some(buffer) = &mut self.session.text_buffer {
                if let Some(sort_index) = self.find_sort_at_position(buffer, click_pos) {
                    buffer.set_active_sort(sort_index);
                    ctx.request_render();
                }
            }
        }
        _ => {}
    }
}

fn find_sort_at_position(&self, buffer: &SortBuffer, pos: Point) -> Option<usize> {
    let mut x_offset = 0.0;
    let mut y_offset = 0.0;
    let line_height = self.session.line_height();

    for (i, sort) in buffer.iter().enumerate() {
        match &sort.kind {
            SortKind::Glyph { advance_width, .. } => {
                let sort_rect = Rect::new(
                    x_offset,
                    y_offset - self.session.descender,
                    x_offset + advance_width,
                    y_offset + self.session.ascender,
                );

                if sort_rect.contains(pos) {
                    return Some(i);
                }

                x_offset += advance_width;
            }
            SortKind::LineBreak => {
                x_offset = 0.0;
                y_offset -= line_height;
            }
        }
    }

    None
}

// In sort/buffer.rs
impl SortBuffer {
    pub fn set_active_sort(&mut self, index: usize) {
        // Deactivate all sorts
        for i in 0..self.buffer.len() {
            if i >= self.gap_start && i < self.gap_end {
                continue;  // Skip gap
            }
            if let Some(sort) = self.buffer.get_mut(i) {
                sort.is_active = false;
            }
        }

        // Activate target sort
        let actual_index = if index >= self.gap_start {
            index + (self.gap_end - self.gap_start)
        } else {
            index
        };

        if let Some(sort) = self.buffer.get_mut(actual_index) {
            sort.is_active = true;
        }
    }
}
```

**Checkpoint 7 Tests**:
- [ ] Render 5 sorts, click on 3rd sort, verify it becomes active
- [ ] Verify previously active sort becomes inactive
- [ ] Click on active sort, verify it stays active
- [ ] Click outside all sorts, verify no changes
- [ ] Click on sort in second line of multi-line text

**Estimated Lines**: ~150-200 lines

---

### Phase 8: Active Sort Editing (Checkpoint 8)

**Goal**: When a sort is active, editing operations (pen tool, select tool) modify that sort's paths.

**Files to Modify**:
- `src/components/editor_canvas.rs` - Load/save active sort paths
- `src/tools/pen.rs` - Use active sort instead of session glyph
- `src/tools/select.rs` - Use active sort instead of session glyph

**Active Sort Path Loading**:

```rust
// In editor_canvas.rs

impl EditorWidget {
    fn load_active_sort_paths(&mut self) {
        if let Some(buffer) = &self.session.text_buffer {
            if let Some(active_sort) = buffer.find_active_sort() {
                if let SortKind::Glyph { name, .. } = &active_sort.kind {
                    // Load paths from font
                    if let Some(glyph) = self.session.font.glyphs.get(name) {
                        self.session.paths = Arc::new(
                            glyph.contours.iter()
                                .map(|contour| Path::from_norad(contour))
                                .collect()
                        );
                    }
                }
            }
        }
    }

    fn save_active_sort_paths(&mut self) {
        if let Some(buffer) = &self.session.text_buffer {
            if let Some(active_sort) = buffer.find_active_sort() {
                if let SortKind::Glyph { name, .. } = &active_sort.kind {
                    // Save paths back to font
                    if let Some(glyph) = self.session.font.glyphs.get_mut(name) {
                        glyph.contours = self.session.paths.iter()
                            .map(|path| path.to_norad())
                            .collect();
                    }
                }
            }
        }
    }
}

// Call during tool dispatch
fn handle_tool_mouse_event(&mut self, ctx: &mut EventCtx, event: &PointerEvent) {
    // Load active sort before tool operation
    self.load_active_sort_paths();

    // Dispatch to tool
    let action = match &self.current_tool {
        ToolBox::Pen(tool) => tool.mouse_event(event, &self.session),
        ToolBox::Select(tool) => tool.mouse_event(event, &self.session),
        // ... other tools ...
    };

    // Save active sort after tool operation
    if action.modified_paths() {
        self.save_active_sort_paths();
    }
}
```

**Checkpoint 8 Tests**:
- [ ] Activate second sort in buffer
- [ ] Use pen tool to add point to active sort
- [ ] Verify only active sort is modified
- [ ] Switch to different sort, verify edits persist
- [ ] Use select tool to move point in active sort
- [ ] Verify inactive sorts remain unchanged

**Estimated Lines**: ~100-150 lines

---

### Phase 9: Preview Pane (Checkpoint 9)

**Goal**: Add horizontal preview pane at bottom of window showing zoomed-out text buffer.

**Files to Create**:
- `src/components/text_preview_pane.rs` - Preview pane widget

**Files to Modify**:
- `src/views/editor.rs` - Add preview pane to layout

**Preview Pane Layout**:

```rust
// In editor.rs view composition

pub fn editor_view(workspace: WorkspaceState) -> impl View<WorkspaceState> {
    zstack((
        // Main editor canvas (existing)
        editor_canvas(workspace.clone()),

        // Main toolbar (existing)
        edit_mode_toolbar().align_left().align_top().margin(10.0),

        // Shapes toolbar (existing, conditional)
        // ...

        // NEW: Text preview pane at bottom (conditional on text mode)
        if workspace.edit_session.text_mode_active {
            Some(
                text_preview_pane()
                    .align_bottom()
                    .height(100.0)  // Fixed height
                    .width_percent(100.0)
            )
        } else {
            None
        },
    ))
}

// src/components/text_preview_pane.rs

pub struct TextPreviewPane {
    session: Arc<EditSession>,
}

impl Widget for TextPreviewPane {
    fn paint(&mut self, ctx: &mut PaintCtx, scene: &mut Scene) {
        // Draw background
        let bg_rect = ctx.size().to_rect();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            theme::background::PREVIEW_PANE,
            None,
            &bg_rect,
        );

        // Calculate scale to fit all text
        if let Some(buffer) = &self.session.text_buffer {
            let text_width = self.calculate_total_width(buffer);
            let text_height = self.calculate_total_height(buffer);

            let scale_x = (ctx.size().width - 20.0) / text_width;
            let scale_y = (ctx.size().height - 20.0) / text_height;
            let scale = scale_x.min(scale_y);

            // Center text in pane
            let x_offset = (ctx.size().width - text_width * scale) / 2.0;
            let y_offset = (ctx.size().height + text_height * scale) / 2.0;

            let transform = Affine::translate((x_offset, y_offset).into())
                * Affine::scale(scale);

            // Render all sorts as filled (inactive)
            for sort in buffer.iter() {
                if let SortKind::Glyph { name, .. } = &sort.kind {
                    let bez_path = self.load_glyph_bezpath(name);
                    scene.fill(
                        Fill::NonZero,
                        transform * Affine::translate(sort.position.to_vec2()),
                        theme::fill::PREVIEW_SORT,
                        None,
                        &bez_path,
                    );
                }
            }
        }
    }
}
```

**Checkpoint 9 Tests**:
- [ ] Enter text mode, verify preview pane appears
- [ ] Type text in main canvas, verify preview updates
- [ ] Verify preview scales to fit pane width
- [ ] Multi-line text displays correctly in preview
- [ ] Exit text mode, verify preview pane disappears

**Estimated Lines**: ~200-250 lines

---

### Phase 10: RTL Text Support (Checkpoint 10)

**Goal**: Add right-to-left text layout mode using Parley's HarfBuzz shaping and bidi resolution.

**Dependencies**:
- Parley (already added in Phase 6) provides HarfRust integration
- Fontique for font fallback

**Files to Modify**:
- `src/sort/data.rs` - Extend `LayoutMode` handling
- `src/sort/buffer.rs` - Add RTL positioning logic
- `src/sort/render.rs` - Handle RTL rendering
- `src/sort/parley_bridge.rs` - Extend for bidi text

**Files to Create**:
- `src/sort/bidi.rs` - Bidi resolution using Parley

**⭐ Parley Integration**: Use Parley's built-in HarfRust shaping and bidi resolution instead of manual rustybuzz integration. Parley handles all the complexity of mixed LTR/RTL text.

**RTL Positioning (Parley approach)**:

```rust
// src/sort/bidi.rs
// Use Parley's built-in HarfRust shaping (accessed via Layout)

use parley::{FontContext, LayoutContext, layout::Layout, style::StyleProperty};

pub fn shape_text_with_parley(
    text: &str,
    font_cx: &mut FontContext,
    layout_cx: &mut LayoutContext,
    font_size: f32,
) -> Layout {
    // Parley automatically detects text direction and applies HarfRust shaping
    let mut builder = layout_cx.ranged_builder(font_cx, text, font_size);

    // Add font and style properties
    builder.push_default(&StyleProperty::FontStack(/* your font stack */));

    let mut layout = builder.build();
    layout.break_all_lines(None); // No width constraint

    // Parley has now shaped the text with HarfRust and resolved bidi
    layout
}

pub struct ShapedGlyph {
    pub glyph_id: u32,
    pub cluster: u32,
    pub x_advance: f64,
    pub y_advance: f64,
    pub x_offset: f64,
    pub y_offset: f64,
}

// In sort/buffer.rs
impl SortBuffer {
    pub fn insert_text_with_shaping(
        &mut self,
        text: &str,
        font_cx: &mut FontContext,
        layout_cx: &mut LayoutContext,
        layout_mode: LayoutMode,
    ) {
        // Let Parley do all the shaping via HarfRust
        let layout = shape_text_with_parley(text, font_cx, layout_cx, font_size);

        // Extract glyphs from Parley's layout
        for line in layout.lines() {
            for item in line.items() {
                if let Some(run) = item.as_text_run() {
                    for glyph in run.glyphs() {
                        let glyph_name = self.glyph_id_to_name(glyph.id);
                        let sort = Sort {
                            kind: SortKind::Glyph {
                                name: glyph_name,
                                codepoint: None,  // Parley/HarfRust handles complex scripts
                                advance_width: glyph.advance,
                            },
                            is_active: false,
                            layout_mode: layout_mode.clone(),
                            position: Point::new(glyph.x, glyph.y),
                        };
                        self.insert(sort);
                    }
                }
            }
        }
    }
}
```

**RTL Rendering**:

```rust
// In render_text_buffer()

fn render_text_buffer(&mut self, ctx: &mut PaintCtx, scene: &mut Scene, buffer: &SortBuffer) {
    let mut x_offset = 0.0;
    let baseline_y = 0.0;

    for sort in buffer.iter() {
        match &sort.kind {
            SortKind::Glyph { name, advance_width, .. } => {
                let sort_position = match sort.layout_mode {
                    LayoutMode::RTL => {
                        // RTL: Position glyph THEN subtract advance
                        let pos = Point::new(x_offset, baseline_y);
                        x_offset -= advance_width;
                        pos
                    }
                    _ => {
                        // LTR: Position glyph THEN add advance
                        let pos = Point::new(x_offset, baseline_y);
                        x_offset += advance_width;
                        pos
                    }
                };

                if sort.is_active {
                    self.render_active_sort(scene, name, sort_position);
                } else {
                    self.render_inactive_sort(scene, name, sort_position);
                }
            }
            SortKind::LineBreak => {
                x_offset = 0.0;
                baseline_y -= self.session.line_height();
            }
        }
    }
}
```

**Checkpoint 10 Tests**:
- [ ] Create RTL text buffer with Arabic text
- [ ] Verify glyphs render right-to-left
- [ ] Type Arabic characters, verify correct shaping
- [ ] Mix LTR and RTL text (bidi), verify correct positioning
- [ ] Verify cursor moves right-to-left in RTL mode

**Estimated Lines**: ~300-400 lines

---

## Testing Strategy

### Unit Tests

Each phase should include unit tests:
- `tests/sort_buffer_test.rs` - Gap buffer operations
- `tests/sort_rendering_test.rs` - Rendering calculations
- `tests/text_input_test.rs` - Input handling
- `tests/rtl_shaping_test.rs` - RTL layout

### Integration Tests

- End-to-end text editing workflow
- Multi-line text with mixed LTR/RTL
- Active sort switching and editing
- Preview pane synchronization

### Manual Testing Checklist

- [ ] Type English text, verify correct rendering
- [ ] Type Arabic/Hebrew text, verify RTL layout
- [ ] Switch between sorts, verify active state
- [ ] Edit active sort with pen tool
- [ ] Use keyboard shortcuts (arrow keys, backspace, enter)
- [ ] Verify preview pane updates in real-time
- [ ] Save and reload edit session with text buffer
- [ ] Test with fonts missing glyph coverage

---

## File Structure Summary

```
src/
├── sort/
│   ├── mod.rs              # Module declaration
│   ├── data.rs             # Sort, SortKind, LayoutMode structs
│   ├── buffer.rs           # SortBuffer gap buffer implementation
│   ├── metrics.rs          # Sort positioning calculations
│   ├── render.rs           # Sort rendering utilities
│   ├── input.rs            # Text input handling
│   ├── cursor.rs           # Cursor position and rendering
│   ├── bidi.rs             # Bidi resolution and shaping via Parley (RTL)
│   └── session.rs          # Session integration helpers
├── components/
│   ├── editor_canvas.rs    # MODIFY: Add text buffer rendering
│   ├── edit_mode_toolbar.rs # MODIFY: Add text tool button
│   └── text_preview_pane.rs # NEW: Preview pane widget
├── views/
│   └── editor.rs           # MODIFY: Add preview pane to layout
├── tools/
│   ├── mod.rs              # MODIFY: Add Text tool variant
│   └── text.rs             # NEW: Text tool implementation
├── edit_session.rs         # MODIFY: Add text_buffer field
└── workspace.rs            # MODIFY: Handle text buffer lifecycle
```

**Estimated Total Lines**: ~2000-2500 lines (across all phases)

---

## Dependencies to Add

```toml
# Cargo.toml additions

[dependencies]
# Existing dependencies...

# Phase 6+: Cursor management and text layout (Linebender ecosystem)
parley = "0.2"      # Text layout, cursor navigation, bidi support (includes HarfRust)
fontique = "0.2"    # Font enumeration and fallback (used by Parley)

# Phase 10: RTL text shaping
# NOTE: Parley includes HarfRust internally for text shaping.
# Only add HarfRust directly if you need low-level shaping control beyond Parley.
# harfrust = "0.1"  # Optional - only if direct shaping API needed
#
# DO NOT USE: rustybuzz is deprecated, use HarfRust instead
# ❌ rustybuzz = "0.18"  # DEPRECATED - do not use
```

---

## Open Questions & Future Enhancements

### Questions to Resolve

1. **Undo/Redo**: How should text buffer operations integrate with existing undo stack?
   - Option A: Each character insertion is one undo step
   - Option B: Group typing sequences into single undo step
   - **Recommendation**: B (group by time threshold, like Bezy)

2. **Multi-line cursor navigation**: Should up/down arrows maintain column position?
   - **Recommendation**: Yes (Bezy's approach with x-offset tracking)

3. **Glyph fallback**: What to render when character not in font?
   - **Recommendation**: Use `.notdef` glyph with advance width

4. **Save format**: Should text buffer state be saved with edit session?
   - **Recommendation**: Yes, serialize to JSON in session file

### Future Enhancements

- **Kerning support**: Apply kerning pairs during rendering
- **Optical size**: Support variable fonts with size-specific glyphs
- **Ligature substitution**: Handle contextual alternates (fi, fl, etc.)
- **Vertical text**: Add top-to-bottom layout mode
- **Multiple buffers**: Support Bezy-style multi-buffer editing
- **Color fonts**: Render COLR/CPAL and SVG glyphs
- **Animation**: Interpolate between masters in variable fonts

---

## References

- **Bezy Source Code**: `/tmp/bezy/src/` (cloned for reference)
- **Runebender Xilem Codebase**: `/Users/eli/GH/repos/runebender-xilem/src/`
- **UFO Specification**: [unifiedfontobject.org](https://unifiedfontobject.org/)
- **Parley Documentation**: [docs.rs/parley](https://docs.rs/parley/) and [github.com/linebender/parley](https://github.com/linebender/parley)
- **HarfRust Documentation**: [docs.rs/harfrust](https://docs.rs/harfrust/) (use via Parley, not directly)
- **Kurbo Documentation**: [docs.rs/kurbo](https://docs.rs/kurbo/)
- **Gap Buffer Algorithm**: [Wikipedia](https://en.wikipedia.org/wiki/Gap_buffer)

---

## Implementation Checkpoints Summary

| Phase | Checkpoint | Deliverable | Lines |
|-------|-----------|-------------|-------|
| 1 | Core Data Structures | `Sort`, `SortBuffer` with gap buffer | 300-400 |
| 2 | Session Integration | `EditSession` with text buffer | 150-200 |
| 3 | Buffer Rendering | Multi-sort rendering with active/inactive | 300-400 |
| 4 | Toolbar Toggle | Text tool button and mode switching | 150-200 |
| 5 | Keyboard Input | Character insertion, arrow keys, backspace | 200-300 |
| 6 | Cursor Rendering | Blinking cursor at insertion position | 150-200 |
| 7 | Active Sort Toggle | Click to activate sort for editing | 150-200 |
| 8 | Active Sort Editing | Pen/select tools modify active sort | 100-150 |
| 9 | Preview Pane | Bottom horizontal preview of buffer | 200-250 |
| 10 | RTL Support | Parley/HarfRust integration for Arabic/Hebrew | 300-400 |

**Total Estimated Lines**: ~2000-2500 lines

---

## Next Steps

1. Review this document and ask clarifying questions
2. Begin Phase 1: Create core sort data structures
3. Write unit tests for gap buffer operations
4. Proceed through phases sequentially, testing at each checkpoint
5. Iterate on design decisions as implementation reveals edge cases

This is a substantial but achievable project. We'll work incrementally, testing at each checkpoint to ensure stability before moving forward. Let's start with Phase 1 when you're ready!
