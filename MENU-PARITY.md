# Menu parity

Audited 2026-09-10 against `runebender-gpui`'s `src/actions.rs`,
`src/launch.rs`, and `src/widgets/menu_bar.rs`. GPUI is the behavioural
reference; the implementation here remains a Xilem/Masonry application adapter.

## Acceptance checklist

### One command model

- [x] One command table supplies native macOS menus, the Windows/Linux in-window
  bar, accelerator labels, shortcut matching, enabled state, and checked state.
- [ ] Every visible item dispatches a working command. Commands unavailable in
  the Xilem shell are recorded below and are not presented as working.
- [ ] Focused text input consumes Copy, Paste, Select All, Undo, and Redo before
  the document command scope. Native accelerators do not dispatch twice.

### Menu inventory

- [x] **Runebender:** Quit Runebender. macOS additionally uses the standard About
  and Hide items supplied by the platform.
- [x] **File:** New Font; Open…; Save; Save As…; Export….
- [x] **Nodes:** New Nodes; Open Nodes…; Save Nodes; Run Nodes.
- [x] **Edit:** Undo; Redo; Copy; Paste; Copy Selected Glyphs as Text; Select All;
  Deselect All; Invert Selection.
- [ ] **Glyph:** New Glyph; Duplicate Glyph; Remove Glyph; Update Metrics;
  Reinterpolate; Decompose Components; Check Joining; Compose from Anchors; Bake
  Masks; Export Glyph as SVG; Trace Image…; Bolden With Model…; Place Image…;
  Import SVG…; Remove Image.
- [x] **Path:** Tidy Up Paths; Add Extremes; Round Coordinates; Correct Path
  Direction; Reverse Contours; Set Start Point; Remove Overlap; Union; Subtract;
  Intersect; Exclude; Flip Horizontal; Flip Vertical; Rotate 90° Left; Rotate 90°
  Right; Rotate 180°; Duplicate Selection; Duplicate + Repeat; Harmonize;
  Balance; Optimize; Hyperbezier to Cubic; Quadratic to Cubic; Cubic to Quadratic.
- [ ] **Filter:** Offset Curve; Extrude; Roughen; Round Corners; Slanter; Add
  Extremes; Remove Overlap.
- [x] **View:** Zoom to Fit; Show All Masters; Sort Glyphs by Name; Sort Glyphs by
  Unicode; Next/Previous Master; Next/Previous Sample String; Grid, Measure, and
  Theme submenus with current values checked.

### State and interaction

- [ ] Undo/Redo reflect history; document edits require a document; selection
  commands require a compatible editor selection; Save reflects dirty/writeable
  state; document-only navigation is disabled without a document.
- [x] Tool, sort, grid, measure, theme, and Show All Masters choices expose their
  current checked state.
- [x] Mouse click opens a menu; moving across titles switches it; choosing a row
  dispatches once; clicking outside dismisses it.
- [ ] Alt/F10 focuses the bar on Windows/Linux. Left/Right changes the top-level
  menu; Up/Down changes the row; Enter/Space activates; Escape closes one level
  and then restores the previous focus.
- [x] Submenus open by pointer or Right/Enter, stay inside the window, return with
  Left/Escape, and expose their hierarchy to accessibility.
- [x] Menu bar, menus, menu items, separators, disabled items, checked items, and
  shortcut labels have appropriate AccessKit roles/state/names.

### Evidence

- [ ] Pure command-table tests cover ordering, titles, shortcuts, duplicate
  accelerators, state predicates, and native/in-window conversion.
- [x] Masonry interaction tests cover keyboard traversal, submenu traversal,
  dispatch, outside dismissal, Escape, and focus restoration.
- [x] Gray and Light headless screenshots show the closed bar and representative
  open menus at the same size.
- [x] `cargo fmt --check`, `cargo clippy --all-targets`, `cargo doc --no-deps`,
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

## Implemented and verified on this branch

- The non-macOS shell is a full-window Masonry widget whose dropdown and nested
  Theme/Measure menus are real layers. F10/Alt enters it, arrows/Home/End
  navigate, Enter/Space dispatch once, Escape dismisses, pointer hover switches
  titles, and prior focus is restored.
- The command table now owns accelerator matching, enabled state, and checked
  state. macOS `muda` items update those states on rebuild; the in-window menu
  draws and exposes them through AccessKit menu-item nodes.
- Existing Xilem operations are wired for Undo/Redo, selection, transforms,
  booleans, curve operations, fit/master navigation, explicit themes, and
  measure toggles. Unimplemented GPUI commands remain absent rather than inert.
- Path cleanup, extrema insertion, coordinate rounding, direction correction,
  Union, and all three curve conversions now call the pinned core operations
  through the same undo-aware session path as the canvas tools.
- View parity includes all-master references, sample-string cycling, checked
  dot/line grid modes, segment-size boxes, and stem/counter spans.
- New, Duplicate, and Remove Glyph follow GPUI's naming and copying rules in
  every master. Removing an open glyph also removes its tabs without disturbing
  tabs for other glyphs.
- Update Metrics, Reinterpolate, Check Joining, Compose from Anchors, Bake Masks,
  and Export Glyph as SVG now call the same core operations and preserve the
  active editing session where applicable.
- `docs/screenshots/menu-parity/view-gray.png`, `view-light.png`,
  `file-gray.png`, `glyph-gray.png`, and `welcome-gray.png` are matched 1100x720
  headless captures of representative document and application menus. The
  headless driver processes real layer lifecycle signals instead of dropping
  them.
- The menu and shortcut scopes now wrap `AppState`, remain present on the welcome
  screen, and disable document commands when there is no workspace. The
  in-window Quit row and Ctrl-Q issue Masonry's real driver exit signal; macOS
  retains its predefined application-menu Quit behavior.
- New Font works from the welcome screen as well as an open document, and Save
  is enabled only while the document is dirty.
- A single cross-platform dialog adapter now backs Open, Save As, Open Nodes,
  Trace Image, Place Image, and Import SVG. Remove Image is undoable through the
  same editing-session history as outline commands.
- Export saves dirty sources, then runs a repository build script when present
  or falls back to `fontc`, all on a background worker with completion reported
  back through the Xilem task pump.
- Copy Selected Glyphs as Text preserves GPUI's name ordering, skips unencoded
  selections, and writes the result to the system clipboard.
- Verified locally on macOS: 58 tests pass serially and all-target Clippy passes
  with warnings denied. The three tab tests have a pre-existing parallel temp-UFO
  filename race; the unfiltered suite can intermittently fail in parallel and
  passes with `--test-threads=1`. Linux and Windows have not been run yet.

## Upstream contribution opportunities

1. A Xilem view for Masonry layer creation, teardown, result delivery, placement,
   and focus restoration.
2. Scoped commands whose metadata drives shortcuts, native/in-window menus,
   checked state, enabled state, and focused-text precedence.
3. Reusable accessible menu-bar/menu/menu-item widgets plus keyboard conformance
   tests.
4. A desktop-service facade for native menu/dialog/window-handle lifecycle rather
   than application code reaching through driver internals.

## Remaining work

- [x] Move the menu/command scope to `AppState` so File and application commands
  exist on the welcome screen as well as inside a loaded `Workspace`.
- [x] Open, Save As, Export, and Open Nodes use real platform or background
  workflows; none of their menu rows are inert.
- [ ] Port the remaining working GPUI handlers: Bolden With Model and
  parameterized filters.
- [x] Add the in-window Runebender/Quit command through the Xilem driver rather
  than pretending a workspace mutation can exit the process.
- [ ] Finish focused-field precedence for native macOS accelerators, including a
  regression proving one dispatch. The Masonry shortcut scope already runs only
  after a focused descendant declines the key.
- [x] Direct accessibility actions open menu titles and activate enabled items;
  disabled and checked state are present on their AccessKit nodes.
- [ ] Run the in-window implementation and its interaction suite on Linux. Cross
  compilation alone is not interaction evidence; no Linux or Windows run has
  happened on this macOS host.

## Integration and launch handoff

The isolated checkout is
`/Users/eli/GH/repos/runebender-xilem/.worktrees/menu-parity` on
`codex/xilem-menu-parity`. On Linux the menu shell is automatic. On macOS it can
be forced without replacing the native bar for headless proof or review:

```sh
RUNEBENDER_IN_WINDOW_MENU=1 cargo run -- \
  /Users/eli/GH/repos/virtua-grotesk/sources/VirtuaGrotesk.designspace
```

For a non-foreground screenshot, add `RUNEBENDER_SCREENSHOT=/tmp/menu.png`,
`RUNEBENDER_MENU_OPEN=View`, `RUNEBENDER_THEME=gray`, and
`RUNEBENDER_SIZE=1100x720`.
