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

- 2026-08-22, slice 5: **Pen tool.** A `Tool { Select, Pen }` enum on
  the app, Select/Pen buttons in the editor toolbar, and pen handling
  in the island: click places corner points (`glyph_ops::start_contour`
  / `append_segment`), clicking near the first point closes
  (`close_contour`), a faint accent segment previews from the last
  point to the cursor, Escape cancels the open path. Verified headless
  by scripting a triangle. Line-only for now; click-drag for bezier
  handles is a later slice. Forced:
  - **Tool state is a rebuild input.** The tool lives on the app and is
    threaded into the island through the view; switching tools rebuilds
    and cancels any open pen path. Clean, but it is the pattern the
    canvas kit should formalize (a tool is a state machine with a
    cursor, an overlay, and a keymap scope, DESIGN.md 6).

- 2026-08-22, slice 6: **save.** `FontModel::save` writes the norad
  font back to its UFO; the app flushes the open glyph first
  (`refresh_open_glyph`) so in-progress edits are included. A Save
  button in the toolbar works in any mode; Cmd+S works in the editor.
  A modified dot (`Save •`) and a "Saved <path>" status line give
  feedback. Verified on disk: a scripted pen triangle round-trips
  (space.glif 0 → 1 contour, 3 points, reloads). Forced, and this is
  the recurring finding:
  - **Save is the clearest case of the window-shortcut gap.** Cmd+S
    only fires when a focusable island has focus, so it is wired in the
    editor and would need wiring in the grid too. The toolbar button is
    the reliable cross-mode path. A window-level action + keymap + menu
    layer (DESIGN.md 9) is the missing framework piece; every app works
    around it the same way.

- 2026-08-22, slice 7: **sidebar with search and category filter.**
  A left panel in overview mode: a search box (name or unicode) and
  category rows (All, Letter, Number, Punctuation, Symbol, Mark, Other)
  with live counts, driven by core's `GlyphCategory`. The grid selects
  by font index now, so filtering the visible set does not break
  selection or open. Verified: Number narrows 657 glyphs to the 18
  numerals. Forced:
  - **Filtering is app state, recomputed each view.** `filtered_cells`
    runs every rebuild; fine at 657 glyphs, but the pattern (derive a
    filtered projection, feed the island) is what a `virtual_grid`
    part plus a selection model should own.
  - Language groups, GF coverage, sort modes, and search scopes/regex
    from gpui's sidebar are still to do; this is the base.

Parity estimate after slice 7: the overview, select + pen tools,
viewport, point editing, undo, save, and a filtering sidebar are in.
Roughly a quarter of runebender-gpui's surface. Remaining large blocks:
bezier pen (curve handles), components/anchors, boolean/transform ops,
measure/curvature tools, kerning + text tool, designspace masters +
interpolation, native menus + window shortcuts, and the browser build.

- 2026-08-22, slice 8: **bezier pen (curve handles), ported from
  runebender-xilem.** The pen now distinguishes click from click-drag:
  a click places a corner on-curve point; a click-drag places a smooth
  on-curve point with symmetric off-curve handles, the outgoing handle
  following the cursor and the incoming handle mirrored. Logic lifted
  from `runebender-xilem/src/tools/pen.rs` (which is the same
  Xilem/Masonry family) and rewritten onto norad points via a pen
  buffer in the session (`pen_corner`, `pen_smooth_begin/drag`,
  `pen_close`). Verified headless: a smooth point produces a real cubic
  arch (blue on-curve, orange/purple handles, 5 points, closes to a
  filled contour). Note:
  - runebender-xilem is the best source for the remaining tools
    (HyperPen, Knife, Measure, Shapes, Select marquee) and its
    `mouse.rs` gesture recognizer, because it targets the same
    framework. Porting is "adapt its `Path`/`MouseDelegate` logic to
    norad + the island's pointer handlers", not a rewrite.

- 2026-08-22, slice 9: **marquee selection.** In the select tool, a
  primary drag on empty space draws a rubber-band rectangle
  (`selection` role, translucent) and selects the points inside on
  release; shift adds to the selection. Middle/secondary drag still
  pans. Build-verified; the drag interaction is not screenshot-checked
  (headless can't drag), but the rect-contains logic is trivial and the
  selection rendering is already proven.

- 2026-08-22, slice 10: **anchors display.** Glyph anchors
  (`glyph.anchors`) draw as small accent diamonds. Verified on "A"
  (top/bottom/ogonek anchors on the baseline). Editing anchors (add,
  move, name, delete) is a later slice; this is display only.

- 2026-08-22, slice 11: **Shapes tool (rectangle, ellipse), ported
  from runebender-xilem.** Rect and Ellipse tools with a drag gesture
  and live preview; on release the session adds a closed contour
  (`add_rect` = 4 corner Line points; `add_ellipse` = 4 smooth Curve
  on-curves with 8 off-curve controls at the 0.5523 kappa). Logic from
  `runebender-xilem/src/tools/shapes.rs`. Verified: a rect and a proper
  cubic ellipse (16 points). Shift-lock (square/circle) is a later
  refinement.

- 2026-08-22, slice 12: **transform and boolean ops (from the gpui
  port's command set).** Session methods over core: `flip_horizontal`
  / `flip_vertical` / `rotate_90` (via `transform_selection` with a raw
  affine, which core centers on the selection bbox), `reverse`
  (`reverse_contours`), and `remove_overlap` (core's linesweeper
  union). Bound to unmodified keys in the editor (h/v/r/]/o) for now;
  these belong on a menu once the window-level action layer exists.
  Verified: an overlapping rect + ellipse union into one 19-point
  contour. Boolean subtract/intersect/exclude are the same call with a
  different `linesweeper::BinaryOp`; a later slice adds them plus
  decompose.

- 2026-08-22, slice 13: **boolean subtract/intersect/exclude and
  decompose.** `session.boolean(BoolOp)` maps a local enum to
  `linesweeper::BinaryOp` and runs `glyph_ops::boolean_contours`;
  `session.decompose()` turns components into editable contours.
  Component contours are precomputed at session creation
  (`resolve_components`) because the whole `norad::Font` is not
  Send/Sync (its datastore holds a `RefCell`), so the session cannot
  hold the font; it holds the resolved contours instead. Verified: Á
  (A + acute, 2 components) decomposes into 3 contours, 25 editable
  points. Note the Send/Sync constraint is a real one for this
  architecture: island widgets must be Send, so anything they own
  must be too.

Parity after slice 13 (~a third of runebender-gpui): overview grid +
sidebar, select/pen/rect/ellipse tools, marquee, point editing, undo,
save, flips/rotate/reverse, boolean ops, remove-overlap, decompose,
anchor + component display. Remaining: HyperPen/Knife/Measure tools,
anchor + component editing, kerning + text/shaping tool, designspace
masters + interpolation, curvature/measure overlays, native menus +
window shortcuts, and the browser build (feasible via the xix web
driver). runebender-xilem remains the direct source for the tools.

- 2026-08-22, slice 14: **Knife tool.** Drag a line; a live red
  preview shows the line and the points where it crosses the outline
  (`knife_hit_points`); on release `knife_cut_glyph` splits the
  contours at those crossings. Both are core functions on
  `norad::Glyph`. Verified: a horizontal cut through "A" adds 8 points
  (20 → 28) at the crossings.

