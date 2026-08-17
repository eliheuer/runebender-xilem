# UI/UX parity: runebender-web -> runebender-xilem

Date: 2026-08-14. Source: full component-by-component audit of both
repos. Goal: make this app look and behave like runebender-web
(the reference UX at runebender.org). Work top to bottom inside a
phase; phases are ordered by user-visible impact per unit of work.
Check items off here as they land.

Many missing behaviors already exist in web's Rust core
(`runebender-web/core/src`). Prefer porting that code (ideally via
`runebender-core`) over reimplementing. License note: web is
GPL-3.0; Eli owns the code and can relicense into Apache-2.0 core,
but do it deliberately per module.

## Phase 1 — interaction quick wins (no new panels)

- [x] Esc returns to grid from the editor (suppressed in text
      mode).
- [x] Tab/Shift+Tab cycles selected points; panels toggle moved
      to backtick.
- [x] Pen Backspace walks back the in-progress contour one point
      instead of deleting selection.
- [x] Shift-hold shift-lock for Shapes and Knife tools.
- [x] Rotate 180 action + TransformPanel button (2x6 grid).
- [x] Coordinate panel W/H fields resize the selection about the
      quadrant reference point (also fixed inverted anchor in
      scale_selection).
- [x] LSB/RSB editing and unicode commit wired in the bottom
      panel. Glyph NAME editing still stubbed — needs
      workspace-level rename (map key, component references,
      kerning groups); tracked as its own item below.
- [ ] Glyph rename: workspace-level rename_glyph (update glyphs
      map key, component references, kerning group members), then
      wire the name field.
- [ ] Shortcut guard: verify interactively whether single-key
      tool shortcuts fire while a panel text_input has focus.
      Masonry routes keys to the focused widget, so the web-style
      leak may not exist; confirm before adding a guard.
- [x] Cmd+V pastes system-clipboard text into the sort buffer in
      text mode (via arboard).

## RESOLVED (2026-08-17, same evening): live-window rendering

FIXED. Two app-side changes, verified by live window screenshots
of both tabs (grid: all three columns incl. glyph-info panel;
editor: full tool palette, sensible zoom, right panel visible):

1. `views/editor.rs` preview: `MultiGlyphWidget::measure` now
   fills offered space when `fit_to_bounds` is set instead of
   reporting its declared size (was 10000x10000) as fixed.
2. `lib.rs tabbed_view`: each tab is wrapped in
   `sized_box(...).dims(Dimensions::new(Dim::Ratio(1.0),
   Dim::Ratio(1.0)))`, pinning it to 100% of the window.
   IndexedStack otherwise sizes the active tab from fit-content
   measurement, and the size_tracker fed that oversized width
   back into AppState (reported 1494 in a 1280 window), so the
   grid rebuilt wider every pass and never converged.
   GOTCHA: xilem `Style::width()`/`.height()` each RESET the
   other axis to Auto — chaining them keeps only the last one.
   Use `.dims()` for both axes.

All 72 tests incl. 14 render tests still pass. The headless
harness clamps sizes instead of honoring measurement, which is
why it could never reproduce this; a live-window screenshot QA
pass (Screen Recording + `RB_OPEN_GLYPH` + window-id capture via
`~/Temp/winid`) is the tool for this class of bug.

Original blocker notes kept below for history.

## Historical blocker notes (2026-08-17): live-window rendering

The app renders BROKEN in the real window (user screenshot: only
the file tile + a canvas-like grid; toolbars/panels/cells missing)
but renders PIXEL-PERFECT headless. The headless harness is in
src/components/mod.rs (render_tests): builds any xilem view via a
bare ViewCtx and renders through masonry_testing's TestHarness to
/tmp/rb-shots/*.png (`cargo test render_ -- --nocapture`). Verified
correct headless: every custom widget, full grid tab, full editor
tab, full app root incl. indexed_stack + watcher fork, the
VirtuaGrotesk designspace (799 glyphs, masters), simulated rebuild
and window resizes. Conclusion: fault is in the LIVE pipeline —
masonry_winit window/layer compositing with wgpu on macOS at rev
7819435 (upstream is refactoring masonry_winit, issue #1836).
Next steps: (1) run xilem's own calc example live (built at
~scratchpad/xilem-ref/target/debug/examples/calc) and check if it
also misrenders -> upstream bug, pin older rev or file issue with
repro; (2) grant Screen Recording permission to the terminal so
the agent can screencapture the live window and iterate solo.

Update 2026-08-17 (later): checked upstream — xilem main is only
2 commits past our pin (spinner tweak #1825, TextInput a11y
#1832); neither touches masonry_winit or compositing, so a rev
bump cannot change live rendering.

RESOLVED HYPOTHESIS 2026-08-17 (evening, with Screen Recording
granted — live screenshots now work): it is NOT an upstream
compositing bug. Evidence:

- masonry's `gallery` example renders perfectly live at this rev
  (compared against in-repo screenshots). xilem's `calc` renders
  its keypad fine (its display row is legitimately empty at
  startup).
- runebender's GRID tab mostly renders live: cells, glyphs,
  category sidebar, colors all paint. Broken bits: content
  overflows the right edge (clipped column, stray text from
  right-side panels pushed offscreen).
- The EDITOR tab reproduces the reported break exactly (file
  tile + canvas background grid only). Use the new
  `RB_OPEN_GLYPH=<name>` env hook to reach it without clicks.
- A paint-time probe showed the editor canvas laying out at
  **10000.0 × 8784.0** in the live window. The 10000 sentinel is
  OURS: `views/editor.rs` preview panel calls
  `multi_glyph_view(glyph_paths, 10000.0, 10000.0, upm)` and
  relies on `.fit_to_bounds()`. Headless, TestHarness passes
  bounded FitContent everywhere and it clamps; live, the
  measure path (MinContent/MaxContent requests, see also
  `measure_fill` returning the full offered space) takes these
  huge preferred sizes at face value, flex children get absurd
  lengths, and siblings get pushed offscreen — which also
  explains the grid tab's right-edge overflow.

Fix direction: audit the 17 custom widgets' `measure` responses.
Widgets meant to fill must not report huge/offered space as
content size (report content-based or minimum lengths; expansion
belongs to the container via flex). Start with the
`multi_glyph_view` 10000 preview and `measure_fill`'s
`FitContent(space) => space` arm.

## Phase 2 — editor shell parity

- [x] Editor top row: file info tile (left, flex) + master
      switcher + grid-return button + tool palette (right); tool
      sub-toolbars (Shapes/Text direction) float top-right.
      SystemMenu button joins when SystemMenuPanel lands.
      NEEDS VISUAL QA in the running app.
- [ ] SystemMenuPanel: New UFO / New designspace, Open, Open
      Recent, Reopen last, Save, Save As, Close; theme picker
      later. Today xilem has only a Save button.
- [ ] Editor left sidebar (web EditorSidebar): start with the
      Overview tab (mini glyph grid + search) so glyph switching
      does not require leaving the editor; Shapes and Axes tabs
      after.
- [ ] Compat-error badge in the top row + on-canvas markers
      (xilem src/editing/compat.rs exists, has no UI).
- [ ] Contour context menu (right-click): set start point,
      reverse contour, round corners, move contour up/down, add /
      delete anchor. Actions mostly exist without UI.
- [ ] Background-image context menu: lock/unlock, trace with
      profile / output mode / style options (tracing exists,
      keyboard-only today).

## Phase 3 — missing panels (Select tool stack)

- [ ] SelectPanel: measure/analysis overlay toggles (colorize by
      popcount, handle lengths, segment lengths, stem/counter
      spans, sidebearing columns). Port web core measure.rs.
- [ ] CurvePanel: curvature comb, continuity dots, Harmonize,
      Balance (Tunni), Optimize + tolerance slider. Port
      `harmonize_selection` / `balance_selection` /
      `optimize_selection` from web core.
- [ ] LayersPanel + background layer / reference glyph model
      (`set_background_outline`, `set_reference_outline`).
- [ ] AnchorPanel + full anchor model: add / select / drag /
      rename / delete, X/Y editing. Xilem has no anchor support.
- [ ] Select-tool sidebearing edge dragging + hover cursor.

## Phase 4 — glyph grid parity

- [ ] Search box (All/Name/Unicode modes, match case, regex) +
      sort by name/unicode. Web CategorySidebar.
- [ ] Variable-width grid cells (columnSpan for wide glyphs).
- [ ] Compat-error badge + unicode label on cells.
- [ ] Language groups with missing-glyph counts + generation
      modal.
- [ ] Copy selection as text.

## Phase 5 — text and preview

- [ ] Text tabs in the top bar (persisted per master lib, like
      web); "Font" tab = grid.
- [ ] Waterfall preview tile (same glyph at several sizes).
- [ ] Bottom preview panel parity (drag-resizable, SVG text
      preview) — xilem's split exists; match behavior.
- [ ] TextDirectionToolbar: surface resolved direction.

## Phase 6 — big-ticket

- [ ] Sketch tool + SketchPanel (brush/erase overlay, autotrace,
      Virtua model draft, training-pair banking).
- [ ] Axes sliders (port web core var_model.rs interpolation UI).
- [ ] .glyphs import (web core glyphs_import.rs).
- [ ] MarkColorPanel "all masters" toggle.
- [ ] Window-blur flush of deferred glyph sync.

## Reference pointers

Full audit with file paths for every item: see the parity report
in this file's git history and `.agents/MASONRY-UPGRADE.md` for
the stack alignment that unblocked code porting. Web shell:
`runebender-web/src/Runebender.vue` (template from line 10935).
Web keyboard: onKeyDown at 10594. Xilem keyboard:
`src/components/editor_canvas/keyboard.rs`.
