<!-- Copyright 2025 the Runebender Xilem Authors -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Architecture Guide

Runebender Xilem is a font editor built with Xilem, a reactive UI framework
from the Linebender ecosystem. It edits UFO (Unified Font Object) font files.
This document explains how the pieces fit together.

## 1. Reactive Data Flow

Xilem uses a single-direction reactive loop. On every state change the entire
UI is rebuilt from a single `AppState` value:

```
  AppState  -->  app_logic()  -->  View Tree  -->  Masonry Widgets  -->  Vello
  (data)         (src/lib.rs)      (diffed)        (layout+paint)       (GPU)
     ^                                                    |
     +-------- event callbacks mutate state  <------------+
```

1. `AppState` (defined in `src/data/mod.rs`) holds everything: the loaded
   font workspace, which tab is active, the current edit session, etc.
2. `app_logic()` in `src/lib.rs` reads `AppState` and returns a view tree --
   either a welcome screen or a tabbed editor with a glyph grid and editor.
3. Xilem diffs the new view tree against the previous one and updates only
   the Masonry widgets that changed.
4. Masonry widgets paint themselves using Vello, a GPU-accelerated renderer.
5. When the user clicks a button or drags a point, the event callback
   mutates `AppState` in place, which triggers a new cycle.

Key source files:
- `src/lib.rs` -- `app_logic()`, window setup
- `src/data/mod.rs` -- `AppState` struct and `Tab` enum
- `src/views/` -- top-level views (welcome, glyph grid, editor)

## 2. Coordinate System

Font files (UFO) use Y-up coordinates with the origin at the baseline.
Screen rendering uses Y-down with the origin at the top-left corner.
`ViewPort` (in `src/editing/viewport.rs`) handles the conversion:

```
  Design space (UFO)            Screen space
  Y increases upward            Y increases downward

       ^ +Y                     (0,0) --------> +X
       |                          |
       |   * point (200,600)      |   * point mapped here
       |                          |
  -----+---------> +X            v +Y
  (0,0) baseline

  to_screen:   screen.x =  design.x * zoom + offset.x
               screen.y = -design.y * zoom + offset.y
                           ^
                           | Y-flip
```

The `ViewPort::affine()` method returns a `kurbo::Affine` matrix that
encodes scale, Y-flip, and translation in one step. All pointer events
are converted from screen to design coordinates with `screen_to_design()`
before tools see them.

## 3. Key Data Types

```
  AppState  (src/data/mod.rs)
  |
  |-- workspace: Arc<RwLock<Workspace>>     (src/model/workspace.rs)
  |   |-- glyphs: HashMap<String, Glyph>    sorted by Unicode codepoint
  |   |-- font_info (UPM, ascender, ...)
  |   +-- kerning, groups
  |
  +-- editor_session: Option<EditSession>   (src/editing/session/mod.rs)
      |-- glyph: Arc<Glyph>                 the glyph being edited
      |-- paths: Arc<Vec<Path>>             editable path data (see sec 4)
      |-- selection: Selection              selected points/segments
      |-- current_tool: ToolBox             active editing tool (see sec 5)
      |-- viewport: ViewPort                zoom + scroll offset
      |-- undo: UndoState<UndoSnapshot>     undo/redo history (see sec 6)
      +-- text_buffer: Option<SortBuffer>   multi-glyph text editing
```

`Workspace` wraps the `norad` UFO parser and converts its types into owned
Rust structs (`Glyph`, `Contour`, `ContourPoint`). It is shared via
`Arc<RwLock<>>` so the edit session can read font data without blocking
the UI thread.

`EditSession` is created when the user opens a glyph for editing and
destroyed when they close it. Changes are synced back to the `Workspace`
on save.

## 4. Path Abstraction

Glyph outlines are represented by a `Path` enum (`src/path/mod.rs`) that
supports three curve types:

| Variant      | Module             | Description                        |
|------------- |--------------------|------------------------------------|
| `Cubic`      | `src/path/cubic.rs`     | Standard cubic bezier (UFO default) |
| `Quadratic`  | `src/path/quadratic.rs` | TrueType-style quadratic curves    |
| `Hyper`      | `src/path/hyper.rs`     | Hyperbezier -- smooth curves from on-curve points only, solved by the `spline` crate |

All three implement `to_bezpath() -> kurbo::BezPath`, which is the common
currency for rendering (Vello draws `BezPath`s) and hit-testing (kurbo
provides geometric queries on `BezPath`s).

Conversion flow when opening a glyph:

```
  norad::Glyph  -->  workspace::Contour  -->  Path::from_contour()
                     (owned point data)       detects point types:
                                                QCurve? -> Quadratic
                                                Hyper?  -> Hyper
                                                else    -> Cubic
```

When saving, `Path::to_contour()` converts back to `workspace::Contour`
format, which is then written to the UFO file via `norad`.

## 5. Tool System and Event Dispatch

Tools handle user interaction in the glyph editor. The design uses a trait
and an enum wrapper:

```
  trait MouseDelegate              trait Tool : MouseDelegate
  (src/editing/mouse.rs)           (src/tools/mod.rs)
  |                                |
  | left_down(), left_up(),        | id() -> ToolId
  | left_click(),                  | paint(scene, session, transform)
  | left_drag_began/changed/ended  | edit_type() -> Option<EditType>
  | mouse_moved(), cancel()        |
  +--------------------------------+

  enum ToolBox  -- wraps all tool structs, delegates to the active one
  |-- Select   (src/tools/select.rs)
  |-- Pen      (src/tools/pen.rs)
  |-- HyperPen (src/tools/hyper_pen.rs)
  |-- Preview  (src/tools/preview.rs)
  |-- Knife    (src/tools/knife.rs)
  |-- Measure  (src/tools/measure.rs)
  |-- Shapes   (src/tools/shapes.rs)
  +-- Text     (src/tools/text.rs)
```

Event flow:

1. `EditorWidget` (in `src/components/editor_canvas/`) receives a raw
   pointer event from Masonry.
2. It converts screen coordinates to design coordinates via `ViewPort`.
3. It wraps the event in a `MouseEvent` and feeds it to the `Mouse` state
   machine (`src/editing/mouse.rs`).
4. `Mouse` classifies the gesture (click vs. drag, using a 3px threshold)
   and calls the matching `MouseDelegate` method on the `ToolBox`.
5. `ToolBox` dispatches to the active tool variant (e.g., `SelectTool`).
6. The tool mutates `EditSession` (moves points, adds segments, etc.).

Each tool can also paint overlays (selection rectangles, guide lines) by
implementing `Tool::paint()`.

## 6. Undo / Redo

The undo system (`src/editing/undo.rs`) is a generic `UndoState<T>` that
stores snapshots in two bounded deques:

```
  UndoState<T>
  |-- undo_stack: VecDeque<T>    (max 128 entries)
  +-- redo_stack: VecDeque<T>

  add_undo_group(snapshot)   push snapshot, clear redo stack
  update_current_undo(snap)  overwrite latest entry (for grouping drags)
  undo(current) -> Option<T> pop undo, push current onto redo
  redo(current) -> Option<T> pop redo, push current onto undo
```

`EditType` (`src/editing/edit_types.rs`) controls grouping:
- `Normal` -- each action gets its own undo entry
- `Drag` / `DragUp` -- all mouse-drag movements collapse into one entry
  (via `update_current_undo`) so a single undo reverses the whole drag
- `NudgeUp/Down/Left/Right` -- consecutive arrow-key nudges of the same
  direction are grouped together

The snapshot type `T` is the path + selection state of the `EditSession`.
When the user presses Cmd+Z, the session swaps its current state with the
top of the undo stack.
