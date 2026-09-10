# Menu parity

Audited 2026-09-10 against `runebender-gpui`'s `src/actions.rs`,
`src/launch.rs`, and `src/widgets/menu_bar.rs`. GPUI is the behavioural
reference; the implementation here remains a Xilem/Masonry application adapter.

## Acceptance checklist

### One command model

- [ ] One command table supplies native macOS menus, the Windows/Linux in-window
  bar, accelerator labels, shortcut matching, enabled state, and checked state.
- [ ] Every visible item dispatches a working command. Commands unavailable in
  the Xilem shell are recorded below and are not presented as working.
- [ ] Focused text input consumes Copy, Paste, Select All, Undo, and Redo before
  the document command scope. Native accelerators do not dispatch twice.

### Menu inventory

- [ ] **Runebender:** Quit Runebender. macOS additionally uses the standard About
  and Hide items supplied by the platform.
- [ ] **File:** New Font; Open…; Save; Save As…; Export….
- [ ] **Nodes:** New Nodes; Open Nodes…; Save Nodes; Run Nodes.
- [ ] **Edit:** Undo; Redo; Copy; Paste; Copy Selected Glyphs as Text; Select All;
  Deselect All; Invert Selection.
- [ ] **Glyph:** New Glyph; Duplicate Glyph; Remove Glyph; Update Metrics;
  Reinterpolate; Decompose Components; Check Joining; Compose from Anchors; Bake
  Masks; Export Glyph as SVG; Trace Image…; Bolden With Model…; Place Image…;
  Import SVG…; Remove Image.
- [ ] **Path:** Tidy Up Paths; Add Extremes; Round Coordinates; Correct Path
  Direction; Reverse Contours; Set Start Point; Remove Overlap; Union; Subtract;
  Intersect; Exclude; Flip Horizontal; Flip Vertical; Rotate 90° Left; Rotate 90°
  Right; Rotate 180°; Duplicate Selection; Duplicate + Repeat; Harmonize;
  Balance; Optimize; Hyperbezier to Cubic; Quadratic to Cubic; Cubic to Quadratic.
- [ ] **Filter:** Offset Curve; Extrude; Roughen; Round Corners; Slanter; Add
  Extremes; Remove Overlap.
- [ ] **View:** Zoom to Fit; Show All Masters; Sort Glyphs by Name; Sort Glyphs by
  Unicode; Next/Previous Master; Next/Previous Sample String; Grid, Measure, and
  Theme submenus with current values checked.

### State and interaction

- [ ] Undo/Redo reflect history; document edits require a document; selection
  commands require a compatible editor selection; Save reflects dirty/writeable
  state; document-only navigation is disabled without a document.
- [ ] Tool, sort, grid, measure, theme, and Show All Masters choices expose their
  current checked state.
- [ ] Mouse click opens a menu; moving across titles switches it; choosing a row
  dispatches once; clicking outside dismisses it.
- [ ] Alt/F10 focuses the bar on Windows/Linux. Left/Right changes the top-level
  menu; Up/Down changes the row; Enter/Space activates; Escape closes one level
  and then restores the previous focus.
- [ ] Submenus open by pointer or Right/Enter, stay inside the window, return with
  Left/Escape, and expose their hierarchy to accessibility.
- [ ] Menu bar, menus, menu items, separators, disabled items, checked items, and
  shortcut labels have appropriate AccessKit roles/state/names.

### Evidence

- [ ] Pure command-table tests cover ordering, titles, shortcuts, duplicate
  accelerators, state predicates, and native/in-window conversion.
- [ ] Masonry interaction tests cover keyboard traversal, submenu traversal,
  dispatch, outside dismissal, Escape, and focus restoration.
- [ ] Gray and Light headless screenshots show the closed bar and representative
  open menus at the same size.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets`, `cargo doc --no-deps`,
  `cargo test`, and a release build pass.
- [ ] macOS native behaviour is tested on macOS. Linux behaviour is tested on
  Linux or explicitly reported as compile-only; Windows is compile-only unless a
  Windows interaction run is recorded.

## Current delta at audit time

Xilem's existing table contains File (New Font, Save), Nodes (Show/New/Save/Run),
Edit (Copy/Paste/Duplicate), Glyph (Generate Missing Glyphs and five outline
operations), View (sort, theme cycle, overview), and seven tools. It has no
separators, submenus, state predicates, or enabled predicates. macOS builds that
subset with `muda`; other platforms install nothing. The shortcut host matches a
second hand-written keymap.

The missing GPUI commands are application work, not evidence of a Xilem
limitation. The in-window popup lifecycle and focus/result plumbing are reusable
framework-integration work.

## Upstream contribution opportunities

1. A Xilem view for Masonry layer creation, teardown, result delivery, placement,
   and focus restoration.
2. Scoped commands whose metadata drives shortcuts, native/in-window menus,
   checked state, enabled state, and focused-text precedence.
3. Reusable accessible menu-bar/menu/menu-item widgets plus keyboard conformance
   tests.
4. A desktop-service facade for native menu/dialog/window-handle lifecycle rather
   than application code reaching through driver internals.
