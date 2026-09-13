# Xilem / GPUI editor visual comparison

Date: 2026-09-11

This is a screenshot-first comparison of the Xilem editor against the GPUI
reference at the same 1280 x 720 viewport, using glyph `R` and the gray theme.
The Xilem capture uses the real Virtua Grotesk designspace. The GPUI capture
uses its embedded read-only demo font, so this audit compares application
layout and hierarchy rather than exact outline geometry.

![Xilem editor, glyph R, gray theme](./01-xilem-editor-r-gray.png)

The current-run GPUI reference was inspected in the in-app browser. Browser
security prevented exporting that capture to this directory, so the findings
below record only directly observed differences and avoid pixel-diff claims.

## Outcome

Xilem now reads unmistakably as the same Runebender product: the dock widths,
dark title strip, gray surface stack, thin keylines, glyph-mark colors, and
point/handle vocabulary are close. It is not yet visually matched, chiefly
because the center workspace and the right inspector assign emphasis
differently.

The next phase should begin with macro geometry. Matching the canvas/proof
split and glyph fit will produce a larger improvement than further palette or
point-token tuning.

## Priorities

### P0: restore the editor canvas as the dominant region

In the GPUI reference, the canvas occupies roughly 510 pixels vertically and
the proof strip roughly 145 pixels. In the Xilem capture, the visually distinct
proof-and-controls region occupies roughly 275 pixels, leaving a shallower
editing canvas. The `R` is consequently about one third smaller and sits high
in the available space, while the proof text becomes the strongest object in
the window.

Match the effective pane split, then match glyph fit and centering. Xilem
already declares a 140-pixel proof drawing height, so the first implementation
task is to trace the composed flex/layout result rather than merely changing
that constant.

### P1: match effective glyph-rail density

Both editors use a five-column rail, and Xilem already declares 44-pixel rail
cells. The rendered packing still differs: the GPUI reference exposes about
eleven compact rows in the same height, while Xilem exposes about seven. This
makes the left rail feel heavier and reduces navigation density.

Compare the full vertical pitch: cell height, grid gaps, rail padding, and any
scroll viewport inset. Do not treat the cell-size constant alone as the cause.

### P1: restore the metrics card's semantic clarity

The floating card's width, border, and shadow are already close. GPUI visibly
labels the sidebearing fields `LSB` and `RSB`; Xilem presents three bare numeric
values. Match the labels and row rhythm, then place the card relative to the
new canvas geometry.

### P1: align inspector hierarchy and default disclosure

The GPUI reference opens a fuller Transformations section, including its
named operations and parameter fields, and shows a clear `nothing selected`
state in Coordinates. Xilem's corresponding inspector is much more collapsed
and leaves empty coordinate fields visible. Some equivalent commands exist in
other sections, but the initial hierarchy is materially different.

Match section order, default open/closed state, header density, and grouping
before refining individual icons. The goal is the same scan path, not simply
the same inventory of commands.

### P2: tune type, spacing, and drawing tokens after geometry

Xilem's panel labels and section rows look slightly larger or taller, and the
proof text is oversized relative to the editor. Point and handle rendering is
broadly close; apparent size differences should be revisited only after the
glyph is fit to the same canvas area.

The top menu is intentionally excluded from the defect list: the GPUI web
build renders an in-window menu, while the Xilem headless capture suppresses
the native menu. These are not comparable in this capture setup.

## Accessibility follow-up

- Bare metrics values rely on position for meaning; visible labels should be
  restored, and accessible names should be checked separately.
- Icon-only transformation buttons need tooltip and accessibility-name
  verification.
- Muted text contrast should be measured from the resolved theme values.
- Static screenshots cannot establish keyboard order, focus visibility,
  screen-reader naming, or scroll behavior.

## Audit steps

1. **GPUI reference capture — strong reference.** Captured at 1280 x 720 in
   the current run and inspected at full size. The embedded font makes outline
   shape non-comparable, but the shell and layout are usable references.
2. **Xilem matched-state capture — usable, hierarchy diverges.** Captured at
   1280 x 720 with Virtua Grotesk, glyph `R`, and the gray theme. The result is
   deterministic and locally saved.
3. **Side-by-side comparison — priorities are clear.** Product identity and
   styling are close; center-pane allocation, rail density, card labeling, and
   inspector disclosure prevent visual parity.

## Recommended implementation order

1. Match center canvas/proof allocation and glyph fit.
2. Match effective glyph-rail vertical pitch.
3. Match metrics-card labels and placement.
4. Match inspector grouping and default disclosure.
5. Recapture gray and dark themes at 1280 x 720 before token-level polish.


## 2026-09-11 overnight: canvas allocation

The original audit above is retained as the baseline. The active overnight
scope is Gray only; its recommendation to capture Dark is superseded.

The first implementation corrects the meaning of the 140-pixel proof token.
GPUI applies that height to its entire resizable proof pane. Xilem had added
36 pixels for a control row outside that budget. Xilem now includes both
control rows and the drawing in a total 140-pixel strip. The glyph-fit formula
is unchanged in both editors: 62% of canvas height frames ascender to descender.

All evidence below uses the real Virtua Grotesk designspace, Regular master,
glyph `R`, Gray, and 1280 x 720 at 1x. The matching proof text is `R`.
The default `Runebender` proof was checked separately and remains unclipped.

- [Xilem before](02-xilem-before-proof-r-gray.png)
- [Xilem after](03-xilem-after-proof-r-gray.png)
- [GPUI live-font reference](04-gpui-live-font-r-gray.png)
- [Xilem after with default proof text](05-xilem-after-default-proof-gray.png)

The reference was captured in headless Chromium with WebGPU/SwiftShader,
loading 3,028 font files through the workspace server. It is no longer the
embedded demo-font comparison. Connecting resets GPUI to the grid; searching
`0052`, opening the resulting glyph, then clearing the search reaches the
matching editor state. No GUI was launched, no save was invoked, and GPUI
was not edited. The existing web bundle was used; its hash is recorded
separately from the reference checkout HEAD rather than claiming a fresh build.

| Measurement | Xilem before | Xilem after | GPUI reference |
| --- | ---: | ---: | ---: |
| Canvas vertical interval, end excluded | 42–510 | 42–546 | 41–551 |
| Canvas height | 468 px | 504 px | 510 px |
| Green on-curve marker pixel bounds, inclusive | x565–720, y128–351 | x559–726, y135–374 | x559–726, y135–376 |

Canvas boundaries were inspected and sampled at x=300. Marker bounds use the
same green-pixel predicate within x=500–769 and y=80–409; they describe visible
markers rather than claiming outline-exact or renderer-exact pixel parity.
The six-pixel remaining canvas-height difference comes from the surrounding
header/status allocation. The original estimate of a one-third glyph-size
shortfall is superseded by this live-font evidence. Repeated GPUI and Xilem
matching-state captures were byte-identical. See
[artifact and font hashes](canvas-allocation-evidence.json).

Verification: 7 render/tab tests and 5 editor-widget tests passed;
`cargo fmt --check`, `git diff --check`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings` passed.
Cargo still reports pre-existing unused local patch notices and the dependency
future-compatibility notice for `block`; no new lint failures were introduced.

Remaining gaps: the proof drawing itself is shorter in Xilem because its two
control rows share the pane; GPUI puts its proof controls in the status bar.
Control placement and exact status/header sizing remain follow-up work.
The current screenshots show four Xilem rail columns versus five in GPUI,
correcting the original audit's statement that both use five. Rail density is
next, followed by card labels/placement and inspector hierarchy. Gray surface
values, proof centering, and drawing tokens still differ and are not claimed
as matched by this change.

For another reference capture, run the existing `runebender-serve` against the
real designspace with `--port 18765` and without `--open`, then run
`capture-gpui.cjs` with the GPUI dist directory, font-server URL, output
directory, and installed Chromium executable as arguments. Set `NODE_PATH` to
the installed Playwright module directory. It uses a temporary localhost
static server on port 18321 and closes the browser/server after capturing.
For Xilem, use `RUNEBENDER_SCREENSHOT`, `RUNEBENDER_SIZE=1280x720`,
`RUNEBENDER_THEME=gray`, `RUNEBENDER_GLYPH=R`, and
`RUNEBENDER_PREVIEW_TEXT=R` with `cargo run -- <designspace>`.


## 2026-09-12 overnight: full proof drawing and compact footer

Completed the pending control relocation and verified the final narrow-window
fix. The proof drawing now gets 140 pixels, with a separate one-pixel divider.
Invert and Blur are in the 28-pixel editor footer alongside Zoom. Preview text
is in the existing Shaping section with direction, features, and language.
The redundant header divider is removed. The previous section's remaining
six-pixel canvas deficit and reduced proof drawing are now resolved.

Proof fitting preserves the text advance horizontally and centers actual ink
vertically with 16-pixel padding, matching the GPUI reference. Empty outline
paths no longer contaminate the ink bounds. Long text fits the available width.
The footer readout can shrink and clip so controls do not widen the center pane
and displace the inspector at 1100 pixels.

- [Xilem matching R proof, 1280 x 720](06-xilem-full-proof-r-gray.png)
- [Preview text and shaping controls](07-xilem-proof-shaping-gray.png)
- [Invert with blur radius 2](08-xilem-proof-invert-blur-gray.png)
- [Narrow window with default Runebender proof](09-xilem-proof-1100-gray.png)
- [Freshly reverified GPUI reference](04-gpui-live-font-r-gray.png)

The reference was captured again from the real font server (3,028 files) using
the existing bundle and was byte-identical to the saved reference. Both editors'
repeated matching captures were byte-identical. No interactive GUI was launched.

| Measurement | Xilem | GPUI |
| --- | ---: | ---: |
| Canvas vertical interval, end excluded | 41–551 | 41–551 |
| Canvas height | 510 px | 510 px |
| Proof drawing interval, end excluded | 552–692 | 552–692 |
| Proof drawing height | 140 px | 140 px |
| Footer interval, end excluded | 692–720 | 692–720 |
| R proof dark-pixel bounds, inclusive | x604–677, y568–675 | x604–677, y568–675 |

Canvas/proof boundaries were sampled at x=300. Proof bounds use RGB values below
70 in the central proof region; this verifies placement and scale, not identical
antialiasing. See [hashes, predicates, and provenance](proof-layout-evidence.json).

Verification: 16 focused tests passed (proof geometry, editor widget, render/tabs,
and real-font text shaping including the normally ignored Arabic/bidi test).
Formatting, diff whitespace checks, and workspace all-target Clippy with warnings
denied passed. The existing unused-patch and block future-compatibility notices
remain. Screenshots at 1280 and 1100 pixels were visually inspected; native
keyboard, pointer, and IME behavior are not certified by these headless checks.

Next: five-column glyph-rail density, metrics-card labels and placement,
inspector hierarchy, then Gray typography, spacing, and surface values. Canvas
metric guides and point styling also differ. GPUI remains the reference and
fallback; these improvements do not establish full parity or deprecation readiness.
