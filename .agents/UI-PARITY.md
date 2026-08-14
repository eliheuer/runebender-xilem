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

- [ ] Esc returns to grid from the editor (suppressed in text
      mode). Web: Runebender.vue:10607. Xilem has NO Escape
      handler at all.
- [ ] Fix Tab conflict: web uses Tab/Shift+Tab to cycle selected
      points (`cycle_selected_point`); xilem binds Tab to panel
      visibility. Adopt web behavior; move panels toggle elsewhere
      (e.g. backtick).
- [ ] Pen Backspace walks back the in-progress contour one point
      (web `pen_delete_last_point`) instead of deleting selection.
- [ ] Shift-hold shift-lock for Shapes and Knife tools
      (`set_shape_shift_locked` / `set_knife_shift_locked`).
- [ ] Rotate 180 action + TransformPanel button (web
      `rotate_selection_180`; xilem only has +/-90).
- [ ] Coordinate panel W/H fields actually resize the selection
      (currently no-op callbacks; web `resize_selection_reference`).
- [ ] Wire the bottom-panel TODO stubs: LSB/RSB editing, glyph
      name and unicode commit (views/editor.rs).
- [ ] Shortcut guard: suppress single-key tool shortcuts while any
      text_input has focus (web gates on `eventTargetAcceptsText`).
- [ ] Cmd+V pastes text into the sort buffer in text mode.

## Phase 2 — editor shell parity

- [ ] Editor top row like web: SystemMenu button (left) + file
      info tile + EditModeToolbar (right). Today xilem's editor
      has no top bar at all.
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
