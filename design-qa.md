# Design QA: edit grid and Text-tool entry

final result: passed

## Inputs and state

- Overall GPUI edit-view reference: user-supplied
  `/Users/eli/Desktop/Screenshot 2026-09-13 at 11.49.42 PM.png`, 2466 by
  1646 device pixels at macOS 2x density.
- Component reference: `runebender-gpui/src/view/canvas/editor.rs` at
  `79e3ab1d096bdcf2538d946c796a8a0cf01f0573`, especially
  `paint_design_grid`, `grid_dot_sizes`, and `round_dot`.
- Text behavior reference: `runebender-gpui/src/edit/text_tool.rs` and
  `runebender-gpui/src/edit/session.rs` at the same revision. A new text
  buffer retains the open glyph as its active sort.
- Implementation: `src/view/canvas/editor.rs`, `src/edit/text_tool.rs`,
  `src/widgets/icon_button.rs`, and their call sites. The captures open the
  Virtua Grotesk `five` glyph in a 1280 by 720 logical viewport at density 1.

## Rendered implementation

- `docs/visual-audit/2026-09-14/145-xilem-round-dot-grid-gray.png`:
  Gray theme, Select tool, edit-canvas zoom 8. The grid is intentionally
  isolated at this zoom so its circular silhouettes are directly inspectable.
- `docs/visual-audit/2026-09-14/146-xilem-text-tool-seeded-gray.png`:
  Gray theme, Text tool, empty saved text context. The open `five` glyph is
  retained as the active sort instead of disappearing.
- `docs/visual-audit/2026-09-14/147-xilem-round-dot-grid-light.png` and
  `148-xilem-text-tool-seeded-light.png`: the same two states in Light.

## Comparison history

- Earlier P1: switching from Select to Text constructed a buffer from an empty
  string, so the canvas replaced the open glyph with an empty line. Fix: seed
  an empty buffer with the tab's glyph name, Unicode value when present, and
  live advance; activate the matching sort when initial text already contains
  it, or reset a stale line that has no valid edit target. Both Gray and Light
  captures retain `five` with its advance box and caret.
- Earlier P1: selecting the Text toolbar icon left native keyboard/IME focus
  outside the editor. Fix: the per-workspace editor widget id is registered at
  build time and the Text tool transfers focus during its pointer event. A
  Masonry harness verifies that the target becomes focused; the existing IME
  harness verifies commit, preedit, selection, arrows, line navigation, cut,
  copy, and paste once focused.
- Earlier P2: design-grid points were axis-aligned `Rect` paths. Fix: preserve
  GPUI's spacing, fade, and zoom-dependent diameter but build each mark as a
  Kurbo `Circle`. At zoom 8, every inspected dot has a round silhouette in both
  themes; the geometry test also proves a bounding-box corner lies outside.

## Fidelity surfaces

- Fonts and typography: no interface font, size, metric-card type, or proof
  typography changed.
- Spacing and layout rhythm: grid spacing remains exactly 8 design units with
  the existing 2-unit close level; Text mode retains the established line-fit
  and caret layout.
- Colors and visual tokens: both grid levels continue to use the existing
  `designGridCoarse` role and fade values; Gray and Light were inspected.
- Image and icon fidelity: no raster or generated asset was introduced. The
  round grid is native Kurbo geometry, and the existing Text toolbar vector is
  unchanged.
- Copy and content: the open glyph remains `five`; its name, U+0035 identity,
  advance, rail selection, and inspector content are preserved on tool entry.

## Validation

- `cargo fmt --check`
- `cargo test --offline --bin runebender` (139 passed, 4 ignored), including
  the focus-transfer harness, Unicode and non-Unicode Text seed tests, the
  existing IME/navigation/clipboard harness, and round-grid geometry.
- `cargo test --offline --test cli` (17 passed)
- `cargo test --offline --test live_agent` (passed with local Unix-socket
  permission)
- `cargo clippy --offline --all-targets -- -D warnings`
- `git diff --check`
- Gray and Light headless captures visually inspected at 1280 by 720
