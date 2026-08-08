# AGENTS.md

Context for AI coding agents working on `runebender-xilem`. Evergreen
info only — architecture, build, conventions, load-bearing gotchas.
Task-specific plans live under `.agents/`. New agents: read this
top-to-bottom before touching code, then check `.agents/` for active
plans and `.agents/active/` for in-flight claims by other agents.

Agent-name-agnostic: Codex (native `AGENTS.md` convention),
Claude Code (via `CLAUDE.md` → here), and any other human-driven
agent that reads this file get the same instructions.

## What this is

Runebender Xilem is a native Rust font editor built with Xilem, the
Linebender reactive UI framework. It edits UFO (Unified Font Object)
font sources and designspace (variable-font) files. Status: alpha.

Status note (2026-08): this repo is currently dormant and behind.
`runebender-web` is the actively developed and actively used editor;
it started as a port of this repo and has since grown features this
repo lacks (anchors, components, layers, feature shaping, Glyphs
import). The plan is to upgrade this repo to current Xilem/Masonry
(landing on kurbo 0.13), then move the kurbo-typed editing/model
logic into `runebender-core` so both editors share it.

## Sister repos

All assumed to be siblings under `~/GH/repos/`:

| Repo | License | Role |
|---|---|---|
| `runebender-xilem` | Apache-2.0 | **This repo.** Native (Xilem) editor. Dormant, pending Masonry upgrade. |
| `runebender-core` | Apache-2.0 | Shared kurbo-free editing/model crate. Local `path = "../runebender-core"` dep of both editors. |
| `runebender-web` | GPL-3.0 | Vue 3 + Rust/WASM web editor. **Currently the primary, actively developed editor.** Its `core/` crate was ported from this repo and also depends on `runebender-core`. |
| `runebender-comfy` | GPL-3.0 | WASM/Vue port for ComfyUI. Imports the web editor's package. |

Fresh-clone needs `runebender-core` checked out as a sibling (or
switch to a git dep before public publish).

## ⚠ Load-bearing gotcha: the kurbo version split

- `runebender-xilem` is pinned to **kurbo 0.12** (via masonry 0.4).
- `runebender-web` (and `runebender-comfy`) are on **kurbo 0.13**
  (forced by peniko 0.5 / vello 0.8).
- The `spline` crate uses kurbo 0.9 internally; conversion happens
  at the boundary.

Sharing kurbo-using modules between xilem and web/comfy currently
produces ~289 errors of `masonry::kurbo::X is not kurbo::X`.
Switching xilem to masonry-2 is a multi-week project.

**Do not naively bump this repo's kurbo.** Only modules with NO kurbo
types in their public API can move into `runebender-core`. Today
that's selection, undo, edit_types, entity_id, kerning, category.

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

- **`AppState`** (`src/data/mod.rs`) — Central app state: loaded workspace, selected glyph, active edit session, current tab, window metadata
- **`Workspace`** (`src/model/workspace.rs`) — Font data model wrapping `norad` UFO types. Thread-safe via `Arc<RwLock<Workspace>>`. Glyphs sorted by Unicode codepoint
- **`EditSession`** (`src/editing/session/mod.rs`) — Per-glyph editing state: editable paths, selection, current tool, viewport, undo/redo history, text buffer for multi-glyph editing

### Module Layout

```
src/
├── lib.rs                # Root app_logic(), window setup
├── main.rs               # Entry point
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
│   ├── session/          # Per-glyph editing state (EditSession)
│   │   ├── mod.rs        # Struct, constructors, component methods, tests
│   │   ├── text_buffer.rs # Sort creation, Arabic shaping, buffer management
│   │   ├── hit_testing.rs # Point/segment/component hit tests
│   │   └── path_editing.rs # Point movement, deletion, contour operations
│   ├── hit_test.rs       # Cursor hit-testing
│   ├── mouse.rs          # Mouse event state machine
│   ├── viewport.rs       # Design↔screen coordinate transform
│   ├── background_image.rs # Background reference image
│   ├── tracing.rs        # img2bez autotrace integration
│   ├── quiver.rs         # QuiverAI cloud vectorization
│   └── compat.rs         # Compatibility/error checks
│   # selection, undo, edit_types moved to runebender-core (re-exported here)
│
├── model/                # Font data model
│   ├── mod.rs            # Re-exports (incl. EntityId/kerning from runebender-core)
│   ├── workspace.rs      # UFO font data (Workspace, Glyph, etc.)
│   ├── designspace.rs    # Variable font designspace support
│   └── glyph_renderer.rs # Glyph contour → BezPath conversion
│
├── data/                 # AppState — central hub
│   ├── mod.rs            # AppState struct, Tab enum, new(), Default
│   ├── file_io.rs        # Font loading/saving (open, load, save)
│   ├── grid.rs           # Glyph grid operations (columns, scroll, filter)
│   ├── editor.rs         # Editor session management
│   └── kerning.rs        # Kerning & glyph property updates
│
├── components/           # UI components & widgets
│   ├── editor_canvas/    # Main glyph editor canvas
│   │   ├── mod.rs        # EditorWidget struct, Widget impl
│   │   ├── paint.rs      # Paint helpers (background, glyph modes)
│   │   ├── text_buffer.rs # Text buffer rendering (multi-sort layout)
│   │   ├── pointer.rs    # Pointer event handlers
│   │   ├── keyboard.rs   # Keyboard shortcut handlers
│   │   ├── drawing.rs    # Standalone drawing functions (points, metrics)
│   │   └── view.rs       # Xilem View wrapper (EditorView)
│   └── ...               # Other components (toolbars, panels)
│
├── views/                # Top-level views (welcome, grid, editor)
│   ├── editor.rs         # Glyph editing tab
│   ├── welcome.rs        # Welcome screen
│   └── glyph_grid/       # Glyph grid tab
│       ├── mod.rs        # Grid tab view, toolbar panels, grid building
│       └── glyph_cell.rs # GlyphCellWidget, GlyphCellView wrapper
├── tools/                # Editing tools (Select, Pen, Knife, etc.)
├── shaping/              # Text shaping (Arabic joining, etc.)
└── sort/                 # Multi-glyph text buffer
```

### UI Layer

- **`src/lib.rs`** — Root `app_logic()` switches between welcome screen and tabbed editor
- **`src/views/`** — Top-level views: `welcome.rs`, `glyph_grid/` (grid tab), `editor.rs` (glyph editing tab)
- **`src/components/`** — UI components: `editor_canvas/` (main canvas widget), `glyph_preview_widget.rs`, toolbars, panels
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
- Reference examples: `src/theme.rs`, `src/settings.rs`, `src/model/workspace.rs`

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

## Git workflow

- **Commit locally as you work, push only when a phase is coherent.**
  Don't push every commit. Squash iteration commits before pushing.
- Don't squash commits that have already been pushed.
- Do not include `Co-Authored-By: Claude` (or similar agent
  attribution) in commit messages.

## Multi-agent coordination

Multiple agents (Claude Code, Codex, Hermes, future others) may be
working in this repo concurrently — possibly across machines. The
protocol uses git as the lowest-common-denominator coordination
channel and lives in `.agents/active/`.

**Before starting any non-trivial task:**

1. **Pull `main` and skim `.agents/active/*.md`.** Each file is a
   claim by an agent currently working on something. If your task
   overlaps an existing claim's `touches:` list, pick a different
   slice or check with the human.
2. **Write your own claim file** to `.agents/active/<slug>.md` using
   `.agents/active/_template.md`. `<slug>` is short kebab-case. One
   file per concurrent task.
3. **Commit and push the claim immediately.** This is an explicit
   exception to the "push at milestones" rule above — the claim is
   coordination state, not feature work, and is useless if other
   agents can't see it. Commit message: `claim: <slug>`.
4. **Work in a git worktree, not the main checkout:**
   ```sh
   git fetch origin
   git worktree add ~/Temp/worktrees/runebender-xilem-<slug> \
     -b agent/<slug> origin/main
   ```
   Worktrees isolate `target/`, dev runs, and editor state. `~/Temp/`
   is user-policy for scratch dirs.
5. **Bump `last_touched:`** in the claim file when you resume after
   an idle stretch (hour+).
6. **Delete the claim file** when you finish, hand off, or abandon.
   Commit + push the deletion. A claim with `last_touched:` older
   than ~24h is stale — don't silently reclaim, ping the human first.

When the feature work merges, the worktree can be removed:
`git worktree remove ~/Temp/worktrees/runebender-xilem-<slug>`.

**Cross-repo work:** if your task spans this repo plus
`runebender-core` or `runebender-comfy`, file the claim in your
primary repo and list cross-repo paths in `touches:` (e.g.,
`../runebender-core/src/selection.rs`). Skim the other repos'
`.agents/active/` too.

Long-lived multi-session plans (design notes, refactor proposals)
still live at `.agents/<NAME>.md`, not under `active/`. `active/` is
only for in-flight claims.
