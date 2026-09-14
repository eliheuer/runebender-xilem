# Design QA: native title-bar optical balance

Final result: passed

## Inputs and state

- Source visual truth: `/Users/eli/Desktop/Screenshot 2026-09-14 at 9.45.33 AM.png`.
- Source pixels: 2302 by 114 at 144 dpi (2x native macOS capture).
- Implementation: `src/view/design.rs` and `src/view/panels/tabs.rs` on the
  edit view with glyph `one`, saved state, and the Select tool active.
- Reference behavior: `/Users/eli/GH/repos/runebender-gpui/src/view/chrome.rs`
  and `/Users/eli/GH/repos/runebender-gpui/src/workspace.rs`.

## Rendered evidence

- Gray full view: `docs/visual-audit/2026-09-11/142-xilem-editor-topbar-balanced-gray.png`,
  1280 by 720 CSS/logical pixels, device density 1, 1280 by 720 output pixels.
- Light full view: `docs/visual-audit/2026-09-11/143-xilem-editor-topbar-balanced-light.png`,
  1280 by 720 CSS/logical pixels, device density 1, 1280 by 720 output pixels.
- Gray density-matched view:
  `docs/visual-audit/2026-09-11/144-xilem-editor-topbar-balanced-gray-1000-2x.png`,
  1000 by 680 CSS/logical pixels, device density 2, 2000 by 1360 output pixels.
- Focused comparison: the 1960-by-52-pixel native window/header crop and the
  2000-by-60-pixel implementation header crop were centered without horizontal
  stretching and stacked at their shared 2x density.

## Comparison history

- Earlier P2: the 28-pixel compact pass left less native margin below the
  AppKit traffic lights than above and put the code-owned centerline too high.
  Fix: use a 30-pixel title bar. Post-fix evidence: the 2x capture adds two
  physical pixels beneath the native control position and moves the document
  text, tools, and tabs down together by one logical pixel.
- Earlier P2: the square new-tab chip competed with the rounded window corner.
  Fix: preserve the 21-by-21-pixel slot but give only the `+` chip a full
  radius. Post-fix evidence: all three renders show a circular terminal control
  while the Font, Nodes, and glyph tabs remain rounded rectangles.

## Fidelity surfaces

- Fonts and typography: the established 13-pixel interface type and copy are
  unchanged; the shared row center corrects the title/status vertical position
  without a font-specific baseline offset.
- Spacing and layout rhythm: the 30-pixel bar centers 20-pixel tool slots and
  21-pixel tabs, retains eight-pixel horizontal inset, and remains complete at
  the 1000-pixel viewport.
- Colors and visual tokens: existing header, full-strength active ink, and
  half-strength inactive ink are unchanged in Gray and Light.
- Image and icon fidelity: the existing `runebender-core` vector toolbar assets
  remain sharp at 1x and 2x; no replacement or generated assets were needed.
- Copy and content: file identity, Saved state, eight tools, Font, Nodes, active
  glyph, and new-tab action remain visible and unaltered.

## Remaining validation boundary

Headless capture verifies the code-owned height, centerline, control geometry,
themes, and narrow layout. AppKit owns the traffic-light coordinates and does
not paint them in the headless renderer; their final optical balance is inferred
from the supplied native 2x capture and the two-pixel height correction.

## Validation

- `cargo fmt --check`
- `cargo clippy --offline --all-targets`
- `cargo test --offline --bin runebender` (135 passed, 4 ignored)
- `cargo test --offline --test cli` (17 passed)
- `cargo test --offline --test live_agent` (passed with local Unix-socket
  permission)
- `git diff --check`
