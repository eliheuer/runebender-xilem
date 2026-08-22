# Porting Runebender to xix

This file logs what the port forces into the framework, in order. It is
the feedback loop from DESIGN.md section 11 ("Runebender under 5,000
lines at PARITY.md"). Reference: runebender-gpui (13,809 lines, one
file) and its `PARITY.md`.

## Log

- 2026-08-22: repository created. First slice: open a UFO or the first
  master of a designspace through `runebender-core`, list glyphs, edit
  one glyph's outline in a canvas island (pan, zoom, select, drag).

  Done the same day: `src/main.rs`, 630 lines, renders headless
  (`XIX_SCREENSHOT`). What it forced, in order of cost:

  1. **Two implementations per island.** The canvas needs a Masonry
     `Widget` impl (measure, layout, paint, pointer events,
     accessibility, children) and a Xilem `View` impl (build, rebuild,
     teardown, message) plus an event type and a constructor: about
     140 lines before any drawing. DESIGN.md 5 ("one implementation
     per widget") is confirmed as the first thing to fix.
  2. **Style wrappers change the type.** `.dims(..)` is not available
     on an `impl WidgetView` return value, so the sidebar is wrapped in
     `sized_box(..).fixed_width(..)`. Same trap as calc's `.color()`.
  3. **No virtualized grid.** The glyph list is 400 `text_button`s in
     a `portal`; it needs the glyph grid the gpui port has (ink-box
     cells, mark colors, categories). This is the first real part to
     build.
  4. **Theme bridge is hand-written.** Surfaces, text, and roles from
     the shared OKLCH file are mapped by name into a `Palette`; stock
     widgets (buttons, inputs) still use Masonry's default property
     set, so the sidebar does not match the canvas.
  5. **Core gap.** `runebender_core::path::Path` has `points()` but no
     mutable accessor; moving points means matching on the variant
     and calling `points.make_mut()`. Core should own "translate these
     point ids by this delta" (it has `point_ops::translate_points`
     for the norad form; the `Path` form needs the same).
  6. **Keyboard is untouched.** No nudge, no delete, no undo yet.
     Window-level shortcuts are the known Xilem gap (PLAN.md 4.1).

  What worked without friction: `norad::Font::load`, designspace first
  master, `Path::from_contour` and `append_to_bezpath`, `ViewPort`
  (fit, pan, zoom-about), `Painter::fill/stroke(..).draw()` with kurbo
  shapes, `EventCtx::local_position`, `capture_pointer`,
  `submit_action`, and the headless screenshot path.

- 2026-08-22, slice 2: **glyph grid + mode switch.** Refactored into
  modules (`theme`, `model`, `grid`, `editor`, `main`). Added a cached
  `FontModel` (per-glyph outline, ink box, mark, category) and a
  `GridWidget` island that paints every visible cell into one scene,
  scrolls, selects, and reports open/select events. Click selects,
  click again opens the editor; `‹ Overview` returns. Verified on a
  657-glyph font (Newsreader). Forced:
  - **A third canvas island already repeats itself.** Grid and editor
    both hand-roll: measure/layout/paint, scroll math, pointer routing,
    the View wrapper, an event enum, `submit_action`. The canvas kit in
    DESIGN.md 6 (viewport, gesture delegate, scene, hit testing) would
    remove most of both files. This is now the highest-value framework
    piece.
  - **No virtual grid widget.** The grid is hand-virtualized (only
    visible rows painted). Fine as an island, but a general
    `virtual_grid` part would serve the media bin, asset browser, etc.
  - **Mode switch is `one_of::Either`.** Works, but the two arms must
    have the same `State`; fine here since both are `App`.
  - **Cell labels still missing** (name, codepoint): needs text in the
    scene, i.e. `Painter::glyphs` with a shaped line. Next slice.

- 2026-08-22, slice 3: **keyboard editing and undo on the island.**
  Arrow keys nudge the selection (shift = x10), Delete/Backspace remove
  selected points, Cmd+A selects all, Cmd+Z / Cmd+Shift+Z undo/redo
  (core `UndoState<Vec<Path>>`), Escape returns to the overview. Forced,
  and this is the important finding:
  - **Widget-focused keys work; window shortcuts do not.** Masonry
    routes key events to the focused widget, so editing keys land once
    the canvas has focus (it grabs focus on pointer down). That covers
    tool keys and nudges. It does NOT cover global shortcuts like Cmd+S
    or menu accelerators that must fire regardless of focus, which is
    the documented Xilem gap (PLAN.md 4.1, the zstack workaround). So
    the shortcut story splits cleanly: island-local keys are fine today;
    a window-level action/keymap/menu layer is still the framework's to
    build (DESIGN.md 9).
  - **Undo lives in the island**, as in runebender-xilem, because the
    view layer would rebuild over it. A framework undo tied to a typed
    command stack would let this move out of the widget.
  - Noticed an orphaned `session.rs` (a norad-glyph-based session using
    `glyph_ops`/`point_ops`) committed but not wired in; removed here.
    The editor's `Path`-based session stays for now; moving to the
    norad-glyph session (which unlocks real ops: boolean, decompose,
    transforms, save) is a planned refactor.

- 2026-08-22, slice 4: **adopt the norad-glyph session.** The editor
  island now owns a `session::Session` that works on `norad::Glyph`
  directly, so `runebender_core::glyph_ops` and `point_ops` apply with
  no conversion. Undo is snapshot-based (`glyph_ops::snapshot/restore`),
  drags measure from the gesture origin via `translate_points` with
  drag-originals (no snap drift), and edits flow back to the grid cache
  (`FontModel::replace_glyph`) so the overview preview updates. Start
  nodes and resolved components now draw. This is the base every real
  op (boolean, decompose, transform, save) builds on. Forced:
  - **The edit/refresh round-trip is manual.** On `Edited`, the app
    clones the glyph out of the session, writes it back into the font
    and the cell cache. A framework document/session abstraction would
    own this; here it is app code, but small.
  - `Arc::get_mut` on the font works only because nothing else holds the
    font Arc; the norad-glyph session clones the glyph, not the font.
    Multiple live sessions (tabs) will need the font behind a shared
    cell or an explicit commit step.

