# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Runebender Xilem is a font editor built with Xilem, a Rust reactive UI framework from the Linebender ecosystem. It edits UFO (Unified Font Object) font sources and designspace (variable font) files. Status: very alpha.

## Build and Development Commands

```bash
cargo build                          # Debug build
cargo run                            # Run (opens file picker)
cargo run -- assets/untitled.ufo     # Open a specific UFO file
cargo run -- --verbose               # Run with verbose logging
cargo check                          # Type-check without building
cargo clippy                         # Lint (uses .clippy.toml with Linebender canonical lints)
cargo fmt                            # Format (uses .rustfmt.toml)
cargo build --release                # Release build
```

There is no test suite. No CI/CD pipeline.

## Architecture

### Data Flow

Xilem reactive architecture with single-direction data flow:
```
AppState → app_logic() → View Tree → Masonry Widgets → Vello Rendering
```
The entire UI is rebuilt from `AppState` on each update. State mutations happen in button/event callbacks.

### Key State Types

- **`AppState`** (`src/data.rs`) — Central app state: loaded workspace, selected glyph, active edit session, current tab, window metadata
- **`Workspace`** (`src/model/workspace.rs`) — Font data model wrapping `norad` UFO types. Thread-safe via `Arc<RwLock<Workspace>>`. Glyphs sorted by Unicode codepoint
- **`EditSession`** (`src/editing/session.rs`) — Per-glyph editing state: editable paths, selection, current tool, viewport, undo/redo history, text buffer for multi-glyph editing

### Module Layout

```
src/
├── lib.rs                # Root app_logic(), window setup
├── main.rs               # Entry point
├── data.rs               # AppState — central hub
├── theme.rs              # Color constants
├── settings.rs           # Config constants
│
├── path/                 # Path representation & geometry
│   ├── mod.rs            # Path enum, re-exports
│   ├── cubic.rs          # Cubic bezier paths (UFO default)
│   ├── quadratic.rs      # Quadratic/TrueType paths
│   ├── hyper.rs          # Hyperbezier paths (spline solver)
│   ├── point.rs          # PathPoint, PointType
│   ├── point_list.rs     # PathPoints collection
│   ├── segment.rs        # Segment types for hit-testing
│   └── quadrant.rs       # Quadrant geometry utility
│
├── editing/              # Editing model & interaction
│   ├── mod.rs            # Re-exports
│   ├── session.rs        # Per-glyph editing state
│   ├── selection.rs      # Entity selection set
│   ├── edit_types.rs     # Undo grouping types
│   ├── undo.rs           # Undo/redo system
│   ├── hit_test.rs       # Cursor hit-testing
│   ├── mouse.rs          # Mouse event state machine
│   └── viewport.rs       # Design↔screen coordinate transform
│
├── model/                # Font data model
│   ├── mod.rs            # Re-exports EntityId
│   ├── workspace.rs      # UFO font data (Workspace, Glyph, etc.)
│   ├── designspace.rs    # Variable font designspace support
│   ├── kerning.rs        # Kerning lookup algorithm
│   ├── entity_id.rs      # Unique entity identifiers
│   └── glyph_renderer.rs # Glyph contour → BezPath conversion
│
├── components/           # UI components & widgets
├── views/                # Top-level views (welcome, grid, editor)
├── tools/                # Editing tools (Select, Pen, Knife, etc.)
├── shaping/              # Text shaping (Arabic joining, etc.)
└── sort/                 # Multi-glyph text buffer
```

### UI Layer

- **`src/lib.rs`** — Root `app_logic()` switches between welcome screen and tabbed editor
- **`src/views/`** — Top-level views: `welcome.rs`, `glyph_grid.rs` (grid tab), `editor.rs` (glyph editing tab)
- **`src/components/`** — UI components: `editor_canvas.rs` (main canvas widget), `glyph_preview_widget.rs`, toolbars, panels
- **`src/tools/`** — Editing tools implementing a `Tool` trait: Select, Pen, HyperPen, Preview, Knife, Measure, Shapes, Text

### Path Abstraction

`src/path/mod.rs` defines a `Path` enum supporting three curve types:
- **Cubic** (`path/cubic.rs`) — Standard cubic bezier (UFO default)
- **Quadratic** (`path/quadratic.rs`) — TrueType-style
- **Hyper** (`path/hyper.rs`) — Hyperbezier (smooth curves from on-curve points only, solved via `spline` crate)

All convert to `kurbo::BezPath` for rendering.

### Text Shaping (`src/shaping/`)

Real-time script-specific shaping without font compilation. Includes Arabic contextual joining with positional forms.

### Multi-Glyph Text Editing (`src/sort/`)

`SortBuffer` manages sequences of glyph instances with cursor support for text-mode editing.

### Coordinate System

- UFO: Y-up, origin at baseline
- Screen: Y-down, origin at top-left
- Transformation in `GlyphWidget::paint()` handles the Y-flip and baseline positioning
- All glyphs scaled uniformly by `widget_height / upm`

## Important Patterns

### Custom Widget Reactivity

In multi-window Xilem apps, use `MessageResult::Action(())` instead of `MessageResult::RequestRebuild` to ensure all windows see state updates. `RequestRebuild` only rebuilds the current window.

### Thread Safety

Xilem views must be `Send + Sync` (required for `portal()` scrolling). Pre-compute data before view construction to avoid capturing mutable references.

## Code Style

- **Line width**: Target 80 chars, 100 max
- **Section separators** between major code blocks:
  ```rust
  // ============================================================================
  // SECTION NAME
  // ============================================================================
  ```
- **Reduce nesting**: Extract helpers, use early returns with `?`, avoid deep closures
- **Variable names**: Full words, not abbreviations
- **Function order**: Public before private, constructors first
- Reference examples: `src/theme.rs`, `src/settings.rs`, `src/editing/undo.rs`, `src/model/workspace.rs`

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `xilem` / `masonry` / `winit` | UI framework, widget library, windowing |
| `norad` | UFO font file format |
| `kurbo` | Bezier curves and 2D geometry |
| `spline` (git) | Hyperbezier spline solver |
| `parley` / `peniko` | Text layout, 2D graphics primitives |
| `rfd` | Native file dialogs |

## Rust Toolchain

- Edition: 2024
- MSRV: 1.88
