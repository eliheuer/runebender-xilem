# Design QA: compact edit title bar

Final result: passed

## Inputs

- Source: `/Users/eli/Desktop/Screenshot 2026-09-14 at 7.42.04 AM.png`
  (2356 by 204 physical pixels, 144 dpi / 2x native macOS capture).
- Implementation: `src/view/chrome.rs` and `src/view/design.rs`.
- Reference behavior: `/Users/eli/GH/repos/runebender-gpui/src/view/chrome.rs`
  and `/Users/eli/GH/repos/runebender-gpui/src/workspace.rs`.

## Exact rendering conditions

- Gray: `docs/visual-audit/2026-09-11/139-xilem-editor-topbar-gray.png`,
  1280 by 720 logical pixels, density scale 1.
- Light: `docs/visual-audit/2026-09-11/140-xilem-editor-topbar-light.png`,
  1280 by 720 logical pixels, density scale 1.
- Narrow Gray: `docs/visual-audit/2026-09-11/141-xilem-editor-topbar-gray-1000.png`,
  1000 by 680 logical pixels, density scale 1.

## Focused comparison and iterations

The supplied native capture showed a roughly 36-pixel logical header. The code
explained the excess: its general toolbar recipe allocated 24-pixel tools plus
eight pixels of inset above and below, producing 40 pixels in the headless
capture.
The first implementation pass replaced that recipe with a fixed 28-pixel row,
removed vertical inset, kept eight-pixel horizontal inset, changed tool slots
from 24 to 20 pixels, and replaced the active fill with icon contrast. A direct
header crop comparison showed the text, tools, and tabs on one centerline; no
second visual adjustment was needed. Gray, Light, and the 1000-pixel-wide state
were then checked as final proof.

## Fidelity surfaces

- Visual fidelity: 28-pixel header, centered document text, 20-pixel tools,
  unchanged 21-pixel tabs, and no active-tool box.
- Interaction states: inactive tools use half-strength header ink; the active
  tool uses full-strength ink; inactive hover retains the existing quiet face.
- Responsive behavior: the flexible drag region yields at 1000 pixels without
  overlapping the document identity, tools, tabs, or new-tab control.
- Content: file name, save status, all eight edit tools, workspace tabs, active
  glyph, and add-tab control remain present.
- Edge cases: Gray and Light palettes both preserve active/inactive contrast;
  native traffic lights remain AppKit-owned and are accommodated by the compact
  height and existing macOS leading spacer.

## Validation

- `cargo fmt --check`
- `cargo clippy --offline --all-targets`
- `cargo test --offline` (135 unit tests and 17 CLI tests passed; the live-agent
  integration test requires an unrestricted local Unix socket)
- `cargo test --offline --test live_agent` (passed outside the sandbox)
- `git diff --check`
