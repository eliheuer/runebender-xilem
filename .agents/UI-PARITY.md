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

## CURRENT BLOCKER (2026-08-17): live-window rendering

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
bump cannot change live rendering. Step (1) still stands: calc at
the ref checkout's HEAD (7fe469d ≈ main) is a valid comparison
binary. Screen Recording permission is still not granted
(screencapture returns wallpaper only), so live QA needs a human
look or that permission.

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
