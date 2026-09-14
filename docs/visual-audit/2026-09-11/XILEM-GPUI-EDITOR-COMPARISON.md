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


## 2026-09-12 overnight: compact glyph rail

Removed the extra container inset around the glyph grid and fitted its nearest
column count to whole pixels. The default 44-pixel target now yields five
40-pixel thumbnails across the 246-pixel dock. Compact rows target square cells
and fit the nearest complete row count, centered in the available area.
The editor tab band is 40 pixels. A 28-pixel footer shows the filtered glyph
count and an independent thumbnail-size slider; overview sizing stays separate.

- [Xilem compact rail at 1280 x 720](10-xilem-compact-rail-gray.png)
- [Xilem compact rail at 1100 x 720](11-xilem-compact-rail-1100-gray.png)
- [Freshly reverified GPUI reference](04-gpui-live-font-r-gray.png)

Both matching 1280-pixel captures now show five columns and eleven complete
rows through glyph V, with 48-pixel horizontal and vertical pitch. Both show
863 glyphs in the footer at y664–692. At x15, the first ten saturated cell-fill
runs start at y135 + 48r in Xilem and y136 + 48r in GPUI: the one-pixel interior
inset reflects the remaining border rasterization difference. This replaces
the four-column, roughly 69-pixel-pitch Xilem rail in capture 06.

The current GPUI source has a 1.18 row multiplier, but its existing web bundle
renders the square, 48-pixel-pitch cells shown here. The inspected bundle is the
visual reference for this step; its hash is recorded independently of checkout
HEAD. We did not rebuild or edit GPUI to force agreement with its current source.

Grid painting, hit testing, and scroll extent share the fitted geometry.
Changing thumbnail size resets the grid scroll and requests layout so its clip
inset updates. Tests exercise every visible cell, reject inter-cell gutters and
bottom padding, and reach the final row at small/default/large size settings.

Verification: five grid tests and seven render/tab tests passed. Formatting,
whitespace checks, and workspace all-target Clippy with warnings denied passed.
The final grid tests and Clippy were rerun after the sizing-refresh adjustment.
Fresh GPUI and Xilem repeat captures were byte-identical; the 1100-pixel capture
was visually inspected with the inspector and footer controls still visible.
See [artifact hashes and measurements](rail-density-evidence.json).

Next: metrics-card labels and placement, then inspector hierarchy. Rail border
weight/shadows, tab faces, search-control widths, Gray surface values, and slider
styling still differ. Native pointer/keyboard accessibility and full functional
parity require further validation; this step does not establish deprecation readiness.


## 2026-09-12 overnight: metrics-card labels and placement

The card now uses a compact 288-by-88-pixel footprint, centered with 12 pixels
above the canvas bottom. LSB and RSB labels sit beside three 64-by-20-pixel
editable fields. Values align to the input inset. Kerning-group names remain
available on their own second row and no longer occupy the labels' positions.
The card has a neutral header and divider, matching the observed GPUI bundle.
Painting and hit testing share the card origin and field rectangles; its geometry
is named in the design tokens. The card hides when there is insufficient room.

- [Xilem metrics card at 1280 x 720](12-xilem-metrics-card-gray.png)
- [Xilem metrics card at 1100 x 720](13-xilem-metrics-card-1100-gray.png)
- [Freshly reverified GPUI reference](04-gpui-live-font-r-gray.png)

Both screenshots put the card's top border at y451 and bottom border at y538.
At y530, Xilem's side borders are x496 and x783; GPUI's are x496 and x782.
These samples use RGB channels below 100. The one-pixel right-edge difference,
field-border weight, and Gray surface shades remain visible. We claim matched
placement and a comparable footprint, not pixel-identical rendering.

As with the rail, the current GPUI source differs from its existing reference
bundle: its newer card is wider and mark-colored. This step follows the captured
reference. GPUI was not edited or rebuilt; its bundle and font hashes are recorded
in [metrics-card evidence](metrics-card-evidence.json).

Seven editor-widget tests passed, including pointer clicks on all three moved
fields, a width edit committed through a keyboard event, narrow/short-pane hiding,
and centered placement at both tested widths. Formatting, diff whitespace checks,
and workspace all-target Clippy with warnings denied passed. Repeated matching
captures were byte-identical in both editors; the 1100-pixel Xilem capture was
visually inspected. No interactive GUI or font save was used.

Next: inspector grouping and default disclosure. Field accessibility, native IME
behavior, and full editor parity remain outside this headless visual proof.


## 2026-09-12 overnight: inspector grouping and disclosure

The inspector now begins with Glyph (closed), Coordinates, Transformations,
Curves, and Background (open), matching the reference's main hierarchy.
Coordinates includes the current selection count or `nothing selected`.
Transformations owns both its icon tools and path operations; the extra closed
Path Operations section is removed. Parameter labels and fields share one row,
and Add extremes joins the three paired rows of existing path-operation buttons.
These controls retain their existing operations and Enter-to-apply behavior.

- [Xilem default inspector, 1280 x 720](14-xilem-inspector-gray.png)
- [Xilem default inspector, 1100 x 720](15-xilem-inspector-1100-gray.png)
- [Transformations folded as one group](16-xilem-transformations-folded-gray.png)
- [Freshly reverified GPUI reference](04-gpui-live-font-r-gray.png)

The default and folded screenshots were inspected. Folding Transformations hides
its icon tools, buttons, and parameter fields together. At 1100 pixels the fields
and inspector remain within the window; lower sections scroll as before.
The canvas, proof, rail, and metrics-card allocations remain unchanged.

This establishes grouping, not complete inspector parity. GPUI still exposes
additional transform icons, Stroke width, and Fit curve %. Those missing controls
and row-spacing differences leave the current Curves and Background sections
higher in Xilem. Background control labeling/layout and the initial parameter
values also differ. Do not hide these gaps with empty placeholder controls.

Verification: 40 editing tests passed, including parameterized filters, geometry,
undo, coordinates, and background operations; three existing integration tests
were ignored (two model runs and real-font bidi). Formatting, diff checks, and
workspace all-target Clippy with warnings denied passed. Matching GPUI and Xilem
repeat captures were byte-identical, with GPUI loading 3,028 real font files.
See [hashes and verification](inspector-grouping-evidence.json).

Next: account for the missing inspector controls using existing core operations
where available, then refine shared row spacing, typography, and Gray surfaces.
Full native interaction/accessibility and deprecation readiness remain unproven.


## 2026-09-12 overnight: functional stroke and curve-fit fields

Stroke width and Fit curve % now invoke the existing core outline operations on
Enter. Stroke width is positive finite font units and targets contours containing
selected points, or all contours when selection is empty. Fit curve accepts
1–150 percent of the tangent-intersection distance, changing handle lengths while
retaining directions and selection. Both refuse a selected component and commands
do not operate on an interpolated preview. No core implementation was duplicated.

Successful changes flow through the existing session/document undo mechanism.
Invalid values and unchanged fits create no undo record. Stroke expansion clears
point selection because its output topology changes. The new fields use the same
single-line inspector recipe and do not alter the font merely by typing.

- [Xilem with both working fields, 1280 x 720](17-xilem-inspector-effects-gray.png)
- [Xilem at 1100 x 720](18-xilem-inspector-effects-1100-gray.png)
- [Freshly reverified GPUI reference](04-gpui-live-font-r-gray.png)

The compared screenshots show both previously missing rows and their labels.
They remain fully visible at 1100 pixels. Inspector boundaries are closer, but
row spacing, parameter defaults, field borders, and extra transform icons still
differ. The GPUI bundle shows Fit curve %, although its current source has since
removed that field; the existing core operation supplies the Xilem behavior.

Verification: 15 session tests and two command tests passed. New tests cover
selected versus all-contour stroke expansion, selected versus all-curve fitting,
invalid/nonfinite values, unchanged fits, retained selection, undo snapshots,
document undo/redo, and an unchanged disposable UFO on disk. Formatting, diff
whitespace checks, and workspace all-target Clippy with warnings denied passed.
Fresh real-font captures repeated byte-identically in both editors. See
[artifact hashes and validation](inspector-effects-evidence.json).

Next: remaining transform icons and shared inspector spacing/Gray surface
polish. Screenshots and disposable-font tests do not establish native IME,
accessibility, GPU/platform parity, or readiness to retire GPUI.


## 2026-09-12 overnight: transformation actions and rotation direction

The first inspector icon row now contains Flip horizontal, Flip vertical,
Rotate counterclockwise, Rotate clockwise, Duplicate, and Duplicate Repeat.
The four Boolean actions stay on the second row. Both use 24-pixel tiles with
8-pixel gaps, matching the reference's 32-pixel horizontal pitch. Shared core
icon paths are reused. Decompose remains available through the existing menu
and context-menu commands; its misleading close icon is removed from this row.

The old clockwise icon invoked the positive 90-degree transform, which is
counterclockwise in the font's upward-Y coordinates. It now invokes a separate
negative 90-degree transform; the counterclockwise icon uses the existing action.
Both act around the selection center and use the existing undo mechanism.

- [Xilem completed icon rows, 1280 x 720](19-xilem-transform-icons-gray.png)
- [Xilem at 1100 x 720](20-xilem-transform-icons-1100-gray.png)
- [Freshly reverified GPUI reference](04-gpui-live-font-r-gray.png)

Sixteen session tests passed, including a new test for opposite rotation
directions, untouched unselected contours, inverse transforms restoring the
original, and undo-record counts. Existing Duplicate Repeat and component tests
also passed. Formatting, whitespace checks, and workspace all-target Clippy
with warnings denied passed; Clippy was rerun after the spacing adjustment.
Fresh matching captures were byte-identical on repetition in both editors,
and the narrow layout was visually inspected. See
[artifact hashes and validation](transform-icons-evidence.json).

The action set and horizontal pitch now agree. The Xilem widget still draws
icons with an internal inset, making their ink smaller, and the rows sit slightly
lower. Shared inspector spacing, icon scale, and Gray surface mappings are next;
background controls and native interaction/accessibility also remain to verify.


## 2026-09-12 overnight: measured inspector icon scale

Inspector transformation actions now use a dedicated 22-pixel icon extent
inside their existing 24-pixel tiles. The shared icon widget accepts an optional
size; other actions retain their default inset and rail tabs retain their own
18-pixel token. Rebuilds refresh rendering when the size changes.

The screenshot comparison corrected the initial assumption that a 24-pixel
button implied 24-pixel visible ink. The reference's first flip icon measures
22 x 22 dark pixels; Xilem now measures the same, previously 20 x 18.
The measured bounds are Xilem (1043,240)..(1064,261) and GPUI
(1045,236)..(1066,257), inclusive, with max RGB below 120. Thus placement still
differs by two pixels horizontally and four vertically; this is a scale fix,
not a claim of full pixel parity.

- [Xilem inspector icon scale, 1280 x 720](21-xilem-icon-scale-gray.png)
- [Xilem at 1100 x 720](22-xilem-icon-scale-1100-gray.png)
- [Freshly reverified GPUI reference](04-gpui-live-font-r-gray.png)

Both window sizes were visually inspected. Repeated captures were byte-identical
in each editor and the fresh GPUI capture matched the saved reference. Seven
editor widget tests passed; formatting, whitespace checks and workspace
all-target Clippy with warnings denied passed after the final size adjustment.
See [measurements and artifact hashes](icon-scale-evidence.json).

Next: inspector vertical spacing and Gray surface mappings. Parameter defaults,
background controls, canvas point styling and native interaction/accessibility
remain to verify before considering GPUI retirement.


## 2026-09-12 overnight: transformation row spacing

The transformation body now groups its two icon rows and parameter form beneath
one disclosure. A four-pixel heading gap puts the icons at the reference's
vertical position; the icon rows retain their 32-pixel pitch. A measured
six-pixel gap separates the icons from the form, whose 28-pixel controls now
use four-pixel row gaps for a consistent 32-pixel pitch.

All eight action/parameter row borders now have the same vertical coordinates
as the reference: top edges at 297,329,361,393,425,457,489,521 and bottom edges
at 324,356,388,420,452,484,516,548. The first flip icon's dark bounds now share
y236..257 with GPUI. Its horizontal placement remains two pixels left.

- [Xilem transformation spacing, 1280 x 720](23-xilem-transform-spacing-gray.png)
- [Xilem at 1100 x 720](24-xilem-transform-spacing-1100-gray.png)
- [Transformations folded as one group](25-xilem-transform-spacing-folded-gray.png)
- [Freshly reverified GPUI reference](04-gpui-live-font-r-gray.png)

Sixteen existing session tests passed, covering the transformation/filter
operations, selection and undo behavior. Formatting, whitespace checks and
workspace all-target Clippy with warnings denied passed. Both matching captures
repeated byte-identically; narrow and folded Xilem layouts were visually
inspected. See [measurements and hashes](transform-spacing-evidence.json).

This fixes the transformation body's vertical rhythm. Section-header insets,
the divider below this group, Curves/Background spacing and Gray surfaces still
differ; those shared recipes need a separate pass. No native interaction,
accessibility or GPUI retirement readiness is implied.


## 2026-09-13 daytime: Gray surface palette

The pending Gray palette correction is now visually verified. The reference
bundle embeds panel neutral.76, canvas neutral.81 and fieldOutline neutral.55;
Xilem's shared Core theme had .73, .76 and .23 respectively. Only those three
Gray tokens change. The complete Gray theme object now equals the embedded
reference object; no application RGB overrides or other theme changes were added.

Matching flat samples in both captures are canvas/fields RGB193, panel/proof
RGB177, and a parameter-field border RGB113. Precise sample locations and hashes
are in [the evidence](gray-surfaces-evidence.json).

- [Xilem Gray surfaces, 1280 x 720](26-xilem-gray-surfaces.png)
- [Xilem Gray surfaces, 1100 x 720](27-xilem-gray-surfaces-1100.png)
- [Verified GPUI reference](04-gpui-live-font-r-gray.png)

The 18 Core theme tests, including contrast checks, formatting and workspace
all-target Clippy passed before the overnight cutoff. The resumed build and
both Xilem sizes were inspected; both editors repeat byte-identically. GPUI's
existing bundle remains an older artifact than current main, independently
identified by its hash. This comparison does not assert a current-main GPUI build.

Today's user priority is autonomous visual iteration, starting with the largest
visible differences. Remaining editor differences include full-width guides and
canvas point styles, grid shadows/tab faces, inspector headers/Coordinates and
Curves/Background spacing. Overview and node editor comparisons follow. Runtime
interaction and platform certification remain separate from screenshot proof.


## 2026-09-13 daytime: glyph-local metric frame

Metric rules now span only the glyph's advance width, using the neutral
metricsLine role at its authored opacity. The sidebearing frame extends to
max(UPM, ascender), rather than stopping at the ascender. Baseline, UPM,
ascender, descender, x-height and cap-height levels are finite-filtered and
deduplicated, avoiding coincident overpainting. Rules use the shared hairline
width in screen space so zoom does not make them heavier.

- [Xilem glyph metric frame, 1280 x 720](28-xilem-metric-frame-gray.png)
- [Xilem at 1100 x 720](29-xilem-metric-frame-1100-gray.png)
- [Freshly reverified GPUI reference](04-gpui-live-font-r-gray.png)

Both frames now begin at y59 and the x-height rule is confined to the glyph
at y197. The canvas outside the frame stays RGB193, including (900,197).
Non-background coverage at y59,197,400 spans x536..744 in Xilem versus
x537..743 in GPUI: edge antialiasing still differs by one pixel, so this is
not an exact pixel match. Seven editor widget tests, formatting, whitespace
checks and workspace all-target Clippy passed. Matching captures repeat
byte-identically, and the narrow layout was visually inspected. See
[measurements and hashes](metric-frame-evidence.json).

Next: point/handle/start-node styling, then rail shadows/tab faces and inspector
field/header polish. This change concerns metric painting only; editable font
and glyph guidelines are separate behavior, not removed by this change.


## 2026-09-13 daytime: point scale and neutral handles

Point markers now follow the reference's smooth zoom-dependent scale, staying
compact at the fitted glyph view and growing for close editing. Corner and
curve radii, selection growth, ring width and halo allowance have named design
tokens. Hit targets are unchanged. Handle lines use the shared secondary text
color; selected points retain a dark keyline around their yellow fill.

- [Xilem point sizing, 1280 x 720](30-xilem-point-scale-gray.png)
- [Selected points retain their keylines](31-xilem-point-scale-selected-gray.png)
- [Xilem at 1100 x 720](32-xilem-point-scale-1100-gray.png)
- [Freshly reverified GPUI reference](04-gpui-live-font-r-gray.png)

The sampled purple off-curve marker has matching colored bounds in both
captures, x689..695/y135..140. The normal, selected and narrow Xilem captures
were visually inspected, and repeat images match byte-for-byte in each editor.
Seven editor widget tests, formatting, whitespace checks and workspace
all-target Clippy passed; a superseded selection-ring accessor was removed
following its unused-code lint. See [measurements and hashes](point-scale-evidence.json).

Start nodes still use orange markers rather than direction arrows, and anchors
still differ from the reference's filled diamonds. Close-zoom grid details and
native pointer delivery are not certified by these fitted-view screenshots.
Those marker details are the next canvas pass, followed by rail and inspector
polish. The GPUI bundle provenance remains the same older, independently hashed
reference artifact.


## 2026-09-13 daytime: distinct anchor diamonds

Anchors now use dark filled diamonds with pink keylines, matching the inspected
GPUI bundle. They scale with the point-marker curve and use the named diamond
width factor. The center dot is removed; selected anchors use the shared yellow
fill and dark keyline. Anchor coordinates, hit testing and editing are unchanged.

- [Xilem anchor diamonds, 1280 x 720](33-xilem-anchor-style-gray.png)
- [Xilem at 1100 x 720](34-xilem-anchor-style-1100-gray.png)
- [Arabic zero anchor fixture](35-xilem-anchor-style-zero-ar-gray.png)
- [Freshly reverified GPUI reference](04-gpui-live-font-r-gray.png)

The R baseline anchor center is RGB29 in both captures. Pink coverage spans
x637..647 in both; vertical coverage is y370..380 in Xilem and y369..380 in
GPUI, retaining a one-pixel rasterization difference. Matching R, narrow and
Arabic zero captures were visually inspected. Sixteen session tests including
anchor drag transactions and anchor-locked components passed, as did formatting,
whitespace checks and workspace all-target Clippy. Both editors' repeated
captures are byte-identical. See [measurements and hashes](anchor-style-evidence.json).

Provenance matters here: current GPUI source has a newer pink-filled/dark-ring
anchor design. The existing bundle has the dark-filled/pink-ring design above,
confirmed by pixel sampling rather than inferred from current source. This pass
matches that bundle. Selected-anchor native interaction was not exercised by
these captures. Start-point direction markers remain the next canvas difference.


## 2026-09-13 daytime: contour direction arrows

Closed contours now show a separate direction arrow beside their first on-curve
point. Ordinary points retain their corner or smooth color. Arrow size and offset
use named tokens and the shared marker zoom curve; selection uses the shared
yellow fill. Open paths, empty contours and coincident start directions produce
no arrow.

- [Xilem start arrows, 1280 x 720](36-xilem-start-arrows-gray.png)
- [Selected points and arrows](37-xilem-start-arrows-selected-gray.png)
- [Xilem at 1100 x 720](38-xilem-start-arrows-1100-gray.png)
- [Freshly reverified GPUI reference](04-gpui-live-font-r-gray.png)

Both R arrows appear in matching positions. The sampled upper arrow retains a
one-pixel horizontal coverage difference. All four views were visually inspected;
repeat captures are byte-identical in each editor. Eight editor widget tests,
formatting, whitespace checks and workspace all-target Clippy passed.
See [measurements and hashes](start-arrows-evidence.json).

The geometry matches the GPUI implementation before commit 6e25392 and the
inspected older bundle. Current GPUI source uses the start point itself as a
triangle, so it is not the source of truth for this capture comparison. This
pass does not certify native pointer interaction. Next: inspect outline fill,
inspector spacing and glyph-rail styling against the same reference.


## 2026-09-13 daytime: flat glyph tiles and crisp borders

The glyph grid now uses the panel ground, flat tile faces and a one-pixel border
painted entirely inside each tile. Removed the offset shadows; centered exterior
strokes previously blurred into the gaps. Grid packing, selection and hit geometry
are unchanged. The shared grid renderer also applies this treatment in overview.

- [Xilem glyph rail, 1280 x 720](39-xilem-flat-tiles-gray.png)
- [Xilem at 1100 x 720](40-xilem-flat-tiles-1100-gray.png)
- [Overview regression check](41-xilem-flat-tiles-overview-gray.png)
- [Freshly reverified GPUI reference](04-gpui-live-font-r-gray.png)

The empty space tile and surrounding gap now match the reference pixel-for-pixel
in the sampled rectangle x0..50/y130..179. The ground is RGB177 and the border
RGB29 in both. Other rail differences remain, including glyph rasterization and
selected-tile appearance; this is not whole-grid parity. The suspected editor
outline-fill difference was also sampled: both already render RGB143, so no
outline-fill change was made.

Five existing grid tests passed, covering compact rail geometry, hit testing,
size changes, scrolling, packing and thumbnail layout. Final formatting,
whitespace checks and workspace all-target Clippy passed. Matching editor,
narrow and overview images were inspected; repeated editor captures match
byte-for-byte in both applications. GPUI fetched 3028 real-font files and still
matches the same older bundle reference hash. See [measurements and hashes](flat-tiles-evidence.json).

Next: inspector Coordinates geometry, Curves/Background spacing, and rail tab
faces. Overview was checked for this shared paint change but still needs its
own matched-state comparison before broader visual parity claims.


## 2026-09-13 daytime: Coordinates picker and field geometry

Coordinates now uses the reference's 52-pixel boxed nine-point picker, without
connecting rules. The label widths, picker-to-field gap and paired-column gap
leave equal numeric fields at the reference's horizontal positions. Inputs have
an explicit standard control height. Existing quadrant buttons and coordinate
editing callbacks remain in place.

- [Xilem Coordinates, 1280 x 720](42-xilem-coordinates-gray.png)
- [Selected point values](43-xilem-coordinates-selected-gray.png)
- [Xilem at 1100 x 720](44-xilem-coordinates-1100-gray.png)
- [Fresh GPUI reference](04-gpui-live-font-r-gray.png)

Both input columns occupy x1128..1182 and x1217..1271, matching GPUI exactly.
The field borders are one pixel above the reference (y134/161/166/193 versus
135/162/167/194); Transformations starts one pixel below its prior position.
Header and lower-group spacing still require a coordinated follow-up. These
small vertical offsets are recorded rather than hidden by a whole-inspector
padding change. The boxed picker follows the source before GPUI commit6e25392
and the inspected older bundle; current GPUI source has a different picker.

All matching, selected and narrow screenshots were inspected. Sixteen existing
session tests passed, as did formatting, whitespace checks and workspace
all-target Clippy. Repeated captures are byte-identical in both editors; GPUI
loaded3028files and matches the saved reference hash. Native picker clicking
and coordinate text entry were not exercised by these screenshot checks.
See [measurements and hashes](coordinates-evidence.json).

Next: reconcile inspector header and Curves/Background spacing, including the
one-pixel Coordinates offset above; then rail tab faces and matched overview.


## 2026-09-13 daytime: inspector group spacing aligned

Inspector groups now use the reference's six-pixel vertical inset and retain
eight pixels horizontally. Coordinates separates its header from a fixed-height
count line and the numeric field block. The transformation header gap compensates
for the tighter inset, while Curves and Background use compact section gaps.
The two Curves buttons now share the available width.

- [Xilem inspector spacing, 1280 x 720](45-xilem-inspector-spacing-gray.png)
- [Xilem at 1100 x 720](46-xilem-inspector-spacing-1100-gray.png)
- [Transformations folded](47-xilem-inspector-spacing-folded-gray.png)
- [Overview shared-group regression check](48-xilem-inspector-spacing-overview-gray.png)
- [Reverified GPUI reference](04-gpui-live-font-r-gray.png)

Measured group dividers match at y74,201,555,653. Coordinates field borders now
match at y135,162,167,194, resolving the prior one-pixel offset. Transformation
button borders match at y297/324,329/356,361/388; Curves matches y587/614. The
Background header therefore starts at the same vertical position as GPUI.
Background's button composition and some field styling still differ.

Sixteen existing session tests, formatting, whitespace checks and workspace
all-target Clippy passed. Normal, narrow, folded and overview images were
visually inspected. Repeat captures are byte-identical in each editor; the fresh
GPUI capture loaded3028files and matches the same saved older-bundle reference.
See [measurements and hashes](inspector-spacing-evidence.json).

Next: Background button composition, inspector input styling and rail tab faces.
Overview still needs a separate matched-state GPUI audit; this pass checks the
shared group inset there, without claiming whole-overview parity.


## 2026-09-13 daytime: Background controls and aligned fields

Background now has two equal-width visibility toggles, a full-width Send to
background action, paired Swap/Clear actions and an inline Reference field with
a glyph-name placeholder. The toggle reflects the visibility setting independently
of whether the current glyph contains a background. Existing actions remain.
The shared inspector label width is now88px, aligning transformation, curve and
Reference inputs at x1134 in the 1280px view.

- [Xilem, Background enabled](49-xilem-background-gray.png)
- [Full Background section with Transformations folded](50-xilem-background-folded-gray.png)
- [Narrow folded view](51-xilem-background-1100-gray.png)
- [Matched folded GPUI reference](52-gpui-background-folded-gray.png)
- [Xilem Background disabled](53-xilem-background-off-gray.png)

The folded button rows match y365..392,397..424,429..456; the Reference input
matches x1134..1271/y461..488. Enabled toggle fill is RGB64 in both. Its keyline
and text rasterization still differ. Use RUNEBENDER_BACKGROUND=1 to match the
reference's enabled setting; Xilem's default remains off. Color and Shaping
have different folded defaults below this section and are outside this comparison.

Two existing command tests, formatting, whitespace checks and workspace all-target
Clippy passed. Normal, folded, narrow and off-state views were inspected. Normal
repeat captures are byte-identical in both editors. GPUI loaded3028files and
matches the same older-bundle reference hash. The capture helper now saves the
folded view after capturing the normal view. No live font was edited or saved;
the screenshots do not exercise native background action clicks.
See [measurements and hashes](background-controls-evidence.json).

Next: active toggle keylines, inspector input styling and rail tab faces, then
matched overview and node-editor views.


## 2026-09-13 daytime: toggle and metrics-field keylines

Enabled text and square toggles now retain the dark panel keyline around their
selected fill. The metrics card's three inputs use the shared field-outline color
and draw the hairline inside their bounds, avoiding the former dark blurred edge.
Focused metrics inputs retain the focus color; their hit rectangles and editing
behavior are unchanged.

- [Xilem borders, 1280 x 720](54-xilem-keylines-gray.png)
- [Enabled Background toggle, folded view](55-xilem-keylines-folded-gray.png)
- [Xilem at 1100 x 720](56-xilem-keylines-1100-gray.png)
- [Reverified folded GPUI reference](52-gpui-background-folded-gray.png)

Sampled toggle border/fill pixels match RGB29/64; sampled metrics borders and
interiors match RGB113/193. Full perimeter comparisons are recorded in the
[evidence](keylines-evidence.json). Text and the outer metrics-card frame still
have rendering differences, so these matches do not imply whole-control identity.

Eight existing editor widget tests passed, including metrics click targeting and
keyboard commit. Formatting, whitespace checks and workspace all-target Clippy
passed. Normal, folded and narrow captures were inspected; normal repeats are
byte-identical in each editor. GPUI loaded3028files and matches both saved normal
and folded reference hashes. No live font was edited or saved.

Next: rail tab faces, then matched overview and node-editor views. The metric
card's outer frame and text rendering remain smaller canvas differences.


## 2026-09-13 daytime: editor rail tabs

Editor tabs now have eight-pixel side/top insets and gaps,24px inactive faces,
a32px active face and aligned18px icons. The selected face meets the panel at
y80 instead of leaving a recessed gap. The rail uses the existing middle neutral
surface and inactive tabs the field surface, matching the inspected bundle.
Overview retains its previous geometry and surfaces. Small fractional-width
edge rasterization differences remain.

- [Editor tabs](57-xilem-rail-tabs-gray.png)
- [Narrow editor](58-xilem-rail-tabs-1100-gray.png)
- [Overview regression check](59-xilem-rail-tabs-overview-gray.png)

Five grid tests, formatting, whitespace checks and workspace all-target Clippy
passed. All three views were inspected; repeats match in both editors and the
fresh GPUI normal reference hash remains unchanged. See [evidence](rail-tabs-evidence.json).

The user reported that panels cannot be resized. Live code inspection confirms
fixed246px dock widths and a fixed proof strip with decorative dividers. Restoring
panel resizing is now the priority before continuing cosmetic comparison.


## 2026-09-13 user-reported gap: panel resizing restored

The user found that panels could not be resized. The render tree confirmed fixed
246px side docks, a fixed proof height and decorative dividers. Both docks and
the editor/proof boundary now use native Xilem Split views. Their one-pixel bars
have eight-pixel pointer targets and native resize cursors, focus and arrow-key
behavior. Side widths and proof height stay in pixels when the window changes size.
The proof's duplicate decorative divider was removed.

Three interaction tests build the actual production splitter views and send
pointer/keyboard events. Both docks grow246to326px through dragging; after a
window resize they remain326px. The proof grows140to200px and keeps its height
through a rebuild/window resize. Tests also cover minimum sizes and collapse/
reopen behavior. Formatting, whitespace checks, workspace all-target Clippy and
the application build passed.

- [Default editor layout](60-xilem-resizable-panels-gray.png)
- [Narrow editor layout](61-xilem-resizable-panels-1100-gray.png)
- [Overview layout](62-xilem-resizable-panels-overview-gray.png)
- [Interaction evidence and limitations](panel-resizing-evidence.json)

These full application screenshots were visually inspected. No foreground GUI
was launched and no live font was saved. Splitter bars use Masonry's native
neutral/focus colors; exact divider-color parity is deferred to preserve working
native interaction. Sizes are currently session state rather than disk preferences;
reopening a collapsed left dock restores its initial width. User should restart
the rebuilt Xilem application and drag either side divider or the line above proof.


## 2026-09-13 daytime: overview footer and grid fit

The overview footer used the full toolbar inset, making it36px tall instead of
the reference28px. A compact named inset restores the grid's eight missing pixels.
Both captures now have four153px rows starting at y46,207,368,529. The grid also
clips and rejects clicks in its vertical margins, removing the following-row
sliver that leaked below the last complete row. Continuous scrolling is retained.

- [GPUI overview reference](63-gpui-overview-gray.png)
- [Xilem overview](64-xilem-overview-fit-gray.png)
- [Narrow overview](65-xilem-overview-fit-1100-gray.png)
- [Measurements and provenance](overview-fit-evidence.json)

Five grid tests and three panel-resizing tests pass, along with formatting,
whitespace and workspace all-target Clippy checks. Both overview sizes were
visually inspected. The editor screenshot is byte-identical to capture60.

This comparison covers the unselected visible grid: Xilem has A selected outside
the viewport and shows its preview, while GPUI has no selection and Masters open.
The existing GPUI bundle is older than main. Remaining overview gaps include
thumbnail ink scale/placement, fractional cell edges, search-control widths and
sidebar spacing. Extra Xilem navigation and the native titlebar need considered
treatment, rather than deleting features to match the older web reference.


## 2026-09-13 daytime: glyph search controls

Scope, regex and case toggles now share a24px width and28px height. Their
inactive labels use the muted text token. Removing the editor search strip's
extra surrounding frame leaves its bottom rule and aligns the controls with
the reference. The editor field spans x8..154 and the three toggles span
x158..182,186..210,214..238 (half-open), all at y89..117. Every perimeter
pixel matches the reference for all four controls. The editor screenshot
changes only within x0..246/y81..127; the canvas and inspector are unchanged.
Overview horizontal edges also match; its extra navigation rail remains.

- [Editor search](66-xilem-search-controls-gray.png)
- [Overview search](67-xilem-search-controls-overview-gray.png)
- [Narrow editor](68-xilem-search-controls-1100-gray.png)
- [Measurements and provenance](search-controls-evidence.json)

Five grid tests, formatting, whitespace and workspace all-target Clippy passed.
All three captures were inspected. Fresh GPUI normal and overview captures are
byte-identical to saved references04 and63.

The initial thumbnail investigation found that current GPUI's
`cell_glyph_transform` and Xilem's `fit_transform` both center visible ink and
apply a0.92 inset. GPUI commit6e25392 introduced this behavior. The older
reference bundle's baseline alignment and larger ink are not grounds to revert
that shared behavior. This is source agreement, not a fresh-current-GPUI
render certification. Next compare sidebar spacing or the node editor.


## 2026-09-13 daytime: node workspace baseline and limits

The comparison helper now captures Nodes from the initial overview, then returns
to Font before its existing editor sequence. Two node captures are byte-identical;
normal editor and overview hashes still match04 and63.

- [GPUI default Nodes](69-gpui-default-nodes-gray.png)
- [Xilem default Nodes](70-xilem-default-nodes-gray.png)
- [Capture provenance and limits](nodes-baseline-evidence.json)

These are **different graphs, not a matched node-geometry comparison**. Xilem
opens the saved bolden graph beside the font (six nodes, seven links). The GPUI
web bundle opens its unsaved live-font starter with Current font, Font version
and Designbot proof controls. GPUI's web Open action is desktop-only according
to its current source, so the same saved graph cannot be imported by that route.

Xilem's `src/edit/nodes.rs::node_registry` explicitly excludes `live.*` types
because their canvas actions are implemented only in GPUI. This is a functional
migration gate, not something to hide by drawing inert buttons. Core's classic
and live node widths also differ intentionally (176 vs256 canvas units), so
these screenshots do not justify changing that geometry. Keep A02 open.

No application code changed. JavaScript syntax, formatting, whitespace and
workspace all-target Clippy checks passed. No foreground GUI, graph run or
font save occurred. Continue sidebar visual work independently; resolving live
node actions and obtaining a matched native/current-build graph capture remain
separate work before GPUI can be deprecated.


## 2026-09-13 daytime: complete category list and section spacing

The sidebar now reads Core's canonical category list instead of maintaining a
second list that omitted Separator. Its four glyphs are visible and use the
existing category filter callback. Category, script and filter groups use a
named six-pixel vertical inset, matching GPUI.

Measured dividers are y81/271/537 in GPUI and y118/308/574 in Xilem: a constant
37-pixel offset for Xilem's retained navigation band. Categories are now190px
tall and Global Scripts266px in both. The editor capture is byte-identical to66;
overview changes are confined to the left sidebar.

- [Overview sidebar](71-xilem-sidebar-sections-gray.png)
- [Narrow sidebar](72-xilem-sidebar-sections-1100-gray.png)
- [Measurements and provenance](sidebar-sections-evidence.json)

Four existing category tests and five grid tests passed. Formatting, whitespace
and workspace all-target Clippy checks passed. Both overview captures were
visually inspected; the fresh GPUI overview matches reference63. No foreground
GUI or live-font saves occurred. Remaining sidebar differences include trailing
count insets, filter-row markers/height, selected-row keylines and text rendering.


## 2026-09-13 daytime: sidebar count and filter-row alignment

Removed the redundant trailing spacer from marked rows. The14px row inset
already clears the overlay scrollbar. Inactive rows no longer reserve a
transparent border; selected rows use the outline token. The selected All row's
full perimeter now matches GPUI after the known37px navigation offset.

Coverage filters are plain rows without bullets. Exporting and incompatible
totals share their19px height and horizontal inset; they remain read-only totals.
GPUI makes those two entries selectable, which is a separate behavioral gap.
No inert buttons were added to imply otherwise.

- [Aligned sidebar](73-xilem-sidebar-rows-gray.png)
- [Narrow sidebar](74-xilem-sidebar-rows-1100-gray.png)
- [Measurements and provenance](sidebar-rows-evidence.json)

For the recorded normalized crops, differing pixel counts fall from4976to3191
for categories,6571to4607 for scripts and5780to3415 for filters. These are local
image measurements, not full-parity scores. Text rendering still differs.
The editor remains byte-identical to66. Both overview sizes were inspected;
five grid tests, formatting, whitespace and workspace all-target Clippy passed.
Fresh GPUI overview equals63. No foreground GUI or live-font saves occurred.

Next compare an expanded overview inspector with the same selected glyph, or
other unverified control states; avoid repeatedly polishing text rasterization
or comparing the different default node graphs.


## 2026-09-13 daytime: matched overview identity inspector

The helper's optional `--overview-inspector` argument selects A through search,
clears the search, folds Masters and expands Glyph. Both apps show Regular,
A,0041 and width716. Two GPUI captures match exactly. This finally compares the
same selected glyph and panel state rather than a preview against no selection.

Xilem now shows the active master and labels the existing fields Glyph name,
Width and Unicode. Fixed21px label boxes,18px master readout and compact spacing
align the identity controls. The empty overview summary no longer adds a blank
row. Rename still commits on Enter; width and Unicode retain their existing
change callbacks. The name input is x1042..1272/y118..146 versus GPUI y117..145;
after the one-pixel native-header offset, every perimeter pixel matches.

- [GPUI selected A](75-gpui-overview-inspector-a-gray.png)
- [Xilem before](76-xilem-overview-inspector-before-gray.png)
- [Xilem identity controls](77-xilem-overview-identity-gray.png)
- [Narrow inspector](78-xilem-overview-identity-1100-gray.png)
- [Measurements and remaining gaps](overview-identity-evidence.json)

The larger inspector gap remains functional: sidebearings, kerning groups,
metrics keys, production name, note, smart-axis and switch controls are present
in GPUI and absent from this Xilem overview panel. E06 remains open. Width stays
full-width rather than filling missing slots with inert inputs.

Two existing overview tests, formatting, whitespace, workspace all-target Clippy
and helper syntax checks passed. Both inspector sizes were visually inspected.
The glyph editor is byte-identical to66. No live font was saved or foreground
GUI launched. These changes align existing controls; they do not complete the
overview inspector's behavior.


## 2026-09-13 daytime: Dimensions readout and Font info baseline

Font info comparison confirms E07: Xilem displays eight read-only values while
GPUI offers editable family/style names and paired numeric fields. That needs
per-master editing, history and cache refresh, not styling read-only values to
imply they are inputs. Font info is recorded as a baseline and unchanged.

Dimensions reports the same readings in both applications. Xilem now gives each
row a21px line box, a2px row gap and8px column gaps, with4px after the section
header. The section ends at y256 versus GPUI y255, matching after the native
header offset; before it ended at236. The section helper exposes its actual
Flex widget type so only this section's spacing is overridden. Other sections
retain their existing layout.

- [GPUI Font info](79-gpui-font-info-gray.png)
- [Xilem Font info baseline](80-xilem-font-info-gray.png)
- [GPUI Dimensions](81-gpui-dimensions-gray.png)
- [Xilem Dimensions before](82-xilem-dimensions-before-gray.png)
- [Aligned Dimensions](83-xilem-dimensions-gray.png)
- [Narrow Dimensions](84-xilem-dimensions-1100-gray.png)
- [Measurements and limitations](dimensions-font-info-evidence.json)

The helper adds optional `--font-info` and `--dimensions` modes. The Dimensions
reference repeats byte-for-byte. Both Xilem sizes were inspected; normal editor
output is byte-identical to66. Two core measurement tests and the application
dimension/undo regression passed. No foreground GUI, graph run or live-font save
occurred. These font-wide panels do not depend on the different default glyph
selection/preview elsewhere in the window.

Formatting, whitespace and workspace all-target Clippy checks also passed.


## 2026-09-13 daytime: Kerning rows and flexible inputs

Both applications show the same 124 pairs. Xilem now uses three equally flexible
inputs, left-aligned pair names, right-aligned values and plain delete controls.
The name and value load the pair into the editor; deleting remains a separate
button. Rows use a 25px pitch, the list is capped at 220px and shrinks with short
results, and the count gets a 21px line box. Horizontal scroll constraining lets
the list fill the dock instead of sizing itself to the text's intrinsic width.

After the one-pixel native-header offset, all four input perimeters match GPUI
exactly. The section divider is y520 versus GPUI y519; before it was y513. Initial
unconstrained flexible rows collapsed to their minimum width; the final images
confirm that the existing scroll adapter's horizontal constraint resolves it.

- [GPUI Kerning](85-gpui-kerning-gray.png)
- [Xilem before](86-xilem-kerning-before-gray.png)
- [Aligned Kerning](87-xilem-kerning-gray.png)
- [Narrow Kerning](88-xilem-kerning-1100-gray.png)
- [Measurements and validation](kerning-evidence.json)

The helper now accepts `--kerning`; repeated reference output is identical.
The existing kerning/groups editing, shaping refresh and undo regression,
formatting, whitespace, helper syntax and workspace all-target Clippy checks
passed. Both screenshot sizes were inspected. Normal editor output is unchanged
from 66. No foreground GUI or live-font save occurred. Long row names clip rather
than showing GPUI's ellipsis; clicking still loads their complete names into the
editor fields. Native interaction remains outside these headless captures.


## 2026-09-13 daytime: Groups input and shelf spacing

The Groups creation field now shows `new group · o or |o`, with no empty caption
above it. Enter retains the existing group-creation callback. Its border is
x1042..1272/y239..267, versus GPUI y238..266; every perimeter pixel matches after
the native one-pixel header offset. Before, the blank caption pushed it to y261.

Group chips now have square keylines and a 23px height. The Add-selection chip
uses muted text. Shelf gaps increase to 4px, and the body retains 8px between the
input and shelves. The first eight visible member chips start at y300,352,404,
456,508,560,612,664: exactly GPUI plus one pixel, with 52px shelf spacing instead
of 46px. A scoped chip style preserves compact chips in Features and Related.

- [GPUI Groups](89-gpui-groups-gray.png)
- [Xilem before](90-xilem-groups-before-gray.png)
- [Aligned Groups](91-xilem-groups-gray.png)
- [Narrow Groups](92-xilem-groups-1100-gray.png)
- [Measurements and limits](groups-evidence.json)

The helper accepts `--groups`; its reference repeats byte-for-byte. Both window
sizes were visually inspected, and the normal editor remains identical to 66.
No live-font edit or save, foreground GUI, or graph run occurred.

Chip wrapping remains an estimated 224px layout, not a width-aware wrapping
container. These screenshots do not certify reflow after dragging a dock.
Both implementations cap this font's 89 kerning groups at 40 and members at 24;
this pass compares the visible top shelves. Native interaction and lower-list
behavior remain outside the screenshot proof.

The existing kerning/groups editing, shaping refresh and undo regression passed,
along with formatting, whitespace, helper syntax and workspace all-target Clippy.


## 2026-09-13 daytime: Compare paragraph wrapping

Both panels report Bold versus Regular: 863 glyphs, 0 missing, 392 advance
changes, 77 versus 124 kerning pairs and 0 structurally incompatible glyphs.
Xilem now uses the compact summary paragraph, omitting the extra metrics-match
line when no metric differs. Masonry WordWrap constrains the text to the dock;
Parley's absolute line height uses the existing 21px ControlSize::Row token.
Name and incompatibility readouts also wrap, while retaining their text colors.

The section divider moves from y391 to y369, versus GPUI y368. The paragraph stays
two lines tall in both renderers; GPUI breaks before `advance`, Xilem after it.
That remaining word-break difference is recorded rather than forcing a newline
that would be wrong for another dock width. Summary calculations are unchanged.

- [GPUI Compare](93-gpui-compare-gray.png)
- [Xilem before](94-xilem-compare-before-gray.png)
- [Wrapping Compare summary](95-xilem-compare-gray.png)
- [Narrow Compare](96-xilem-compare-1100-gray.png)
- [Measurements and limits](compare-evidence.json)

The helper adds `--compare`, and its reference repeats identically. Both sizes
were visually inspected. The final build reproduces the verified screenshot;
normal editor output remains identical to 66. The existing incompatible-master
fixture regression, formatting, whitespace, helper syntax and workspace
all-target Clippy checks passed. A narrowly documented lint expectation covers
the exactly representable control-height token's conversion to Parley's f32 API.
No live-font edit/save, graph Run or foreground GUI occurred. These comparisons
do not establish native interaction parity.


## 2026-09-13 user report: white/gray resize end caps

The user observed white squares at panel ends during dragging, turning gray on
release and disappearing after clicking away. Masonry Split draws an expanded
focus rectangle: opaque during pointer capture, half-opacity while focused.
Neighboring panel backgrounds cover its sides, but the end caps escape the
splitter container. Kurbo's positive `inset(2)` expands the rectangle.

A focused harness reproduces the actual cap pixels against a padded gray
background. Its margin-pixel assertion fails before the fix. The application
now puts each splitter region inside a native Portal, constrained on both axes
with content required to fill it. This clips paint at the panel boundary and
cannot scroll or show scrollbars. The native Split continues to own focus,
dragging, keyboard and accessibility resizing, retained lengths and limits.

- [White end caps reproduced](97-resize-endcaps-before.png)
- [Dock while dragging, fixed](98-resize-dock-drag-fixed.png)
- [Dock after release, fixed](99-resize-dock-released-fixed.png)
- [Proof divider after release, fixed](100-resize-proof-released-fixed.png)
- [Validation evidence](resize-endcaps-evidence.json)

The new regression checks both orientations during drag, release, pointer-away
and blur; it also asserts that focus is retained before blur. It passes along
with the three existing panel-resize interaction tests. Formatting, whitespace
and workspace all-target Clippy pass. The standard editor is byte-identical to 66;
the narrow editor was inspected. No live font or foreground app was modified.

Both debug and optimized release builds passed. The rebuilt release executable
also reproduces the standard editor screenshot exactly. The user can relaunch
the release app to load this change.


## 2026-09-13 user report: restore glyph-tile shadows

The user's side-by-side native screenshots exposed a mismatch hidden by the
older GPUI browser bundle used for the earlier flat-tile comparison: current
GPUI gives every glyph tile a solid two-pixel lower-left shadow, increased to
three pixels for the selected tile. Xilem showed only its inside keyline.

Xilem now paints the same hard offset before each square tile and uses the same
recessed grid-ground and derived shadow-color formulas as GPUI main at
`79e3ab1`. The tile rectangle, packing, hit testing, scroll geometry, glyph
painting and captions are unchanged. Both the overview and compact editor rail
use the shared renderer, so they receive the treatment together.

- [Gray overview with selected tile](101-xilem-grid-shadow-gray.png)
- [Narrow Gray overview](102-xilem-grid-shadow-1100-gray.png)
- [Light-theme check](103-xilem-grid-shadow-light.png)
- [Capture provenance and limits](grid-shadow-evidence.json)

The supplied GPUI reference and final Xilem crop were inspected side by side;
the hard shadow direction, ordinary two-pixel depth and selected three-pixel
depth agree. These are component-level comparisons because the supplied windows
have different dimensions. Both Xilem target sizes and Light were also inspected.
Five existing grid tests, formatting, whitespace, workspace all-target Clippy
and an optimized release build pass. No foreground GUI or live font was modified.


## 2026-09-13 user report: unify panel boundaries and grid ground

The supplied native screenshots made three separate paint defects visible. The
Xilem splitter supplied its own `#717179` bar instead of the shared `#1d1d1d`
outline used by GPUI; the overview footer had no top rule; and the grid's
widget-level clip prevented its recessed ground from painting in the fitted
top and bottom margins.

The native Split widgets still retain dragging, keyboard resizing,
accessibility actions, minimum widths, and their eight-pixel pointer target.
Their hard-coded visible bar is now suppressed, while panel-owned one-pixel
keylines use the palette outline token. The same edge recipe draws the proof
and status rules. The grid now paints its ground before applying a local cell
clip, so cells remain contained while the two margin strips match the interior
gaps.

- [Gray overview](104-xilem-panel-boundaries-gray.png)
- [Narrow Gray overview](105-xilem-panel-boundaries-1100-gray.png)
- [Light-theme overview](106-xilem-panel-boundaries-light.png)
- [Gray editor and proof divider](107-xilem-panel-boundaries-editor-gray.png)
- [Capture provenance and pixel checks](panel-boundaries-evidence.json)

The two supplied screenshots, before capture, four final captures, and both
themes were inspected. At 1280 pixels, both dock boundaries and the status
rule are `#1d1d1d`; the grid top, interior gap, and bottom are all `#919191`.
The new grid-margin regression and all four existing splitter interaction tests
pass with the other 126 active tests. Formatting, whitespace, all-target Clippy,
and the optimized build pass; its Gray capture is byte-identical to 104.
Headless images verify paint and layout, not native pointer delivery or GPU
rasterization. No foreground GUI or live font was modified.


## 2026-09-13 user report: overview preview and footer controls

The supplied native screenshots identified three small overview mismatches.
GPUI's glyph preview sits on the lighter `canvas` surface (`#c1c1c1` in Gray),
while Xilem let the inspector's `panel` surface (`#b1b1b1`) show through. The
mark row's 24-pixel slots were aligned to the top of its remaining space, and
the selected ring used the slot's complete radius. The grid/list toggles were
Unicode substitutes whose appearance depended on interface-font coverage.

The preview now explicitly uses the shared canvas token. The swatch row takes
the space below its one-pixel rule and centers there; the selected ring stays
one pixel inside its slot, yielding three pixels above and two below in the
27-pixel row. The footer uses the same source geometry as GPUI's
`glyph_free_icon`: four outlined cells for Grid and three rules for List. Both
remain labeled buttons in the accessibility tree.

- [Gray overview](108-xilem-overview-controls-gray.png)
- [Narrow Gray overview](109-xilem-overview-controls-1100-gray.png)
- [Light-theme overview](110-xilem-overview-controls-light.png)
- [Capture provenance and pixel checks](overview-controls-evidence.json)

All three Xilem captures were visually inspected. The Gray preview sample is
`#c1c1c1` against the inspector's `#b1b1b1`; the selected swatch ring is clear
of all four slot edges; and both view marks render without font glyphs. The
geometry regression, formatting, whitespace, 128 active tests, all-target
Clippy, and an optimized release build pass; four model/font integration tests
remain ignored by default. The release capture is byte-identical to 108.
Headless screenshots verify static paint and layout, not native pointer,
keyboard, screen-reader, or GPU behavior. The title bar remains deliberately
outside this pass.


## 2026-09-13 user report: fill and resize the overview glyph preview

The previous surface correction exposed a layout mismatch: Xilem gave the
preview a fixed 260-pixel height inside a taller inspector portal. The canvas
therefore ended early, leaving a panel-colored tail below it, and the glyph was
centered only within the upper fixed box. GPUI instead makes its preview
`flex_1` with a 200-pixel minimum and fits the actual ink bounds to 88% of the
available width or height, whichever is tighter.

Xilem now separates the overview section list from the preview with a native
vertical Split. Its initial boundary follows the bottom of the ten collapsed
headers at 340 pixels, so the preview consumes every remaining pixel. The
existing one-pixel Masters rule remains the only visible boundary, with an
eight-pixel invisible drag target. Dragging it down makes the preview smaller;
both section list and preview retain 120-pixel minimums, and the dragged size
survives view rebuilds. The glyph continues to fit from its real ink bounds and
is centered in the complete allocation.

- [Gray overview with centered A](111-xilem-overview-resizable-preview-gray.png)
- [Narrow Gray overview](112-xilem-overview-resizable-preview-1100-gray.png)
- [Light-theme overview](113-xilem-overview-resizable-preview-light.png)
- [Expanded Glyph section](114-xilem-overview-expanded-sections-gray.png)
- [Capture provenance and layout checks](glyph-preview-layout-evidence.json)

All four final captures were visually inspected. In Gray, the `#c1c1c1`
canvas runs from y378 through the bottom pixel at y719; Light likewise remains
`#f2f2f2` throughout. The A's visual bounds are centered on the preview's
vertical midpoint. The focused interaction regression drags the boundary from
340 to 440 pixels, verifies the resulting 242-pixel preview, rebuilds the view,
and then verifies the 120-pixel minimum. Static screenshots do not establish
native pointer, keyboard, accessibility, or GPU behavior; the interaction test
runs Masonry's event path headlessly. All 129 active tests, formatting,
whitespace, strict all-target Clippy, and the optimized build pass; four
model/font integration tests remain ignored by default. The release capture is
byte-identical to 111.


## 2026-09-13 user report: glyph-grid trackpad and arrow navigation

The Xilem grid owned a pixel scroll offset and correctly received native wheel
events, but `filtered_cells` produces a fresh `Arc` during every view rebuild.
The grid treated each new allocation as changed content and reset its offset to
zero, so a Mac trackpad gesture could visibly snap or jitter as unrelated state
rebuilt the view. A clicked grid accepted focus but did not implement a text-event
handler, leaving all four arrow keys inert.

The grid now resets its viewport only when the displayed glyph indices or their
order actually change. Equivalent view rebuilds replace refreshed cell data while
retaining the current scroll offset. With grid focus, Left and Right move one item
in display order, Up and Down move by the fitted column count, and the destination
row is scrolled fully into view. Unmodified arrows clear multi-selection through
the same `Selected` action as an ordinary click, matching GPUI's bounded movement.

Focused Masonry regressions send a pixel-delta pointer event, replace the cells
with an equivalent allocation, and verify that the offset stays at 96 pixels.
A second interaction test focuses the grid, sends all four named arrow keys,
checks the emitted glyph indices, and verifies that moving below the viewport
advances its scroll. The complete application and CLI suites pass with 152 active
tests and four ignored model integrations; the live-agent test also passes when
run outside the macOS filesystem sandbox. Formatting and strict all-target Clippy
pass. The supplied screenshot is static evidence of the affected surface and does
not itself establish native trackpad delivery or keyboard focus behavior.

A follow-up native screenshot exposed one remaining difference: retaining the
pixel offset still allowed a trackpad gesture to stop between rows, leaving the
first and last cells sliced. GPUI stores `scroll_row` as an integer and derives
the pixel position from the fitted row height. Xilem now follows that behavior:
every non-zero wheel gesture advances at least one row, larger deltas advance a
bounded whole-row count, keyboard reveal uses the same row index, and a resize
recomputes the offset from that index. The pointer regression deliberately sends
a 17-pixel delta into a 48-pixel row and verifies a 48-pixel result, making the
half-cell state unrepresentable through the tested interaction path.


## 2026-09-14 overnight: compact edit metrics card

GPUI's floating edit metrics card is a 320 by 58 pixel pane with a 22-pixel
marked header, five equal fields in one row, square corners in the Gray theme,
and a hard shadow offset four pixels left and down. Xilem previously used a
288 by 88 pixel neutral rounded card, split the kerning groups into a second
row, and placed external LSB and RSB labels beside three wider metric fields.

Xilem now uses the GPUI geometry and ordering: left kerning group, LSB, width,
RSB, and right kerning group. The glyph mark colors the header and selects its
contrasting ink; an unmarked glyph uses a quiet derived header surface. The
three numeric fields retain their existing click targets and editing behavior.

- [Gray compact metrics card](115-xilem-editor-compact-metrics-gray.png)
- [Light compact metrics card](116-xilem-editor-compact-metrics-light.png)

Both final captures were visually inspected. The card remains horizontally
centered with its existing 12-pixel canvas clearance, while its shorter body
returns 30 pixels of vertical space to the outline canvas. A focused geometry
test covers the new field positions and narrow-height hiding threshold. Static
headless captures verify paint and layout, not native pointer, keyboard,
screen-reader, or GPU behavior.


## 2026-09-14 overnight: restore the Path Operations disclosure

GPUI presents geometric transforms and path operations as two adjacent,
independently collapsible inspector sections. Xilem had placed both sets of
controls under Transformations, so the Path Operations header was absent and
the default inspector hierarchy did not match the reference.

Xilem now keeps the two icon rows under Transformations and places the six
named operations plus their parameter fields under a separate Path Operations
disclosure. Both sections remain expanded by default, matching GPUI, and each
can be folded without changing the other.

- [Gray inspector hierarchy](117-xilem-editor-path-operations-gray.png)
- [Light inspector hierarchy](118-xilem-editor-path-operations-light.png)

Both 1280 by 720 captures were visually inspected. The new divider and header
align with the neighboring Coordinates and Curves groups in both themes.
Static headless captures verify paint and layout, not native pointer,
keyboard, screen-reader, or GPU behavior.


## 2026-09-14 overnight: quiet Coordinates empty state

GPUI leaves the reference picker and X/Y/W/H fields visible before a point is
selected but does not spend a separate row announcing the empty state. Xilem's
`nothing selected` row made the section visibly taller and pushed every section
below it farther down the inspector.

Xilem now follows GPUI: the count row appears only when one or more points are
selected, using the same concise `1 point` or `n points` wording. The picker and
fields remain available in the empty state.

- [Gray compact Coordinates section](119-xilem-editor-coordinates-gray.png)
- [Light compact Coordinates section](120-xilem-editor-coordinates-light.png)

Both 1280 by 720 captures were visually inspected. In the empty state the
picker now sits directly below the Coordinates header and the lower sections
move up by 21 pixels. Static headless captures verify paint and layout, not
native pointer, keyboard, screen-reader, or GPU behavior.


## 2026-09-14 overnight: connected Coordinates picker

GPUI's coordinate-reference picker is one 52-pixel control: six crossing rules
connect nine circular targets, with the active reference point filled. Xilem
used nine independent button widgets inside an outlined box, leaving the rules
absent and making the control read as a generic keypad.

Xilem now paints a native Masonry rule layer behind the existing nine buttons.
Each target therefore retains its direct pointer and accessibility action and
refreshes the coordinate fields through the existing workspace action. The
inactive circles mask the rules beneath them, while the active circle uses the
inspector text color, matching GPUI's construction.

- [Gray connected coordinate picker](121-xilem-editor-coordinate-picker-gray.png)
- [Light connected coordinate picker](122-xilem-editor-coordinate-picker-light.png)

Both 1280 by 720 captures were visually inspected. The rule and circle
contrast remains legible in Gray and Light without adding a surrounding box.
Static headless captures verify paint and layout, not native pointer,
keyboard, screen-reader, or GPU behavior.


## 2026-09-14 overnight: inspector action-label casing

The GPUI inspector uses title case for the multiword action labels `Add
Extremes`, `Round Corners`, and `Curvature Comb`. Xilem used sentence case for
those three controls, which was visible in direct comparison even though their
geometry and actions already matched.

Xilem now uses the GPUI labels without changing the commands, control sizes,
or state.

- [Gray inspector labels](123-xilem-editor-inspector-labels-gray.png)
- [Light inspector labels](124-xilem-editor-inspector-labels-light.png)

Both 1280 by 720 captures were visually inspected; all three labels fit their
existing controls in both themes. Static headless captures verify paint and
layout, not native pointer, keyboard, screen-reader, or GPU behavior.


## 2026-09-14 overnight: shared sidebar-tab geometry

GPUI uses the same 36-pixel navigation band in the font overview and edit
sidebar: four pixels of top and horizontal inset, four pixels between tabs,
32-pixel active faces, and 28-pixel inactive faces. Xilem's edit mode used an
eight-pixel inset and gap, a 40-pixel band, 24-pixel inactive faces, and a
different rail surface, so the selected and inactive tabs shifted relative to
the otherwise matching overview tabs.

Xilem now uses one GPUI geometry and surface treatment in both modes. The
active tab reaches the bottom rule, inactive tabs stop four pixels above it,
and the selected icon retains the two-pixel rise used by the reference.

- [Gray shared tab geometry](125-xilem-editor-sidebar-tabs-gray.png)
- [Light shared tab geometry](126-xilem-editor-sidebar-tabs-light.png)

Both 1280 by 720 captures were visually inspected. The active Glyphs tab joins
the panel and the five icons share a baseline in Gray and Light. Static
headless captures verify paint and layout, not native pointer, keyboard,
screen-reader, or GPU behavior.


## 2026-09-14 overnight: edit inspector section order

GPUI begins its edit inspector with Coordinates, Transformations, and Path
Operations, then places the collapsed Glyph metadata section before Curves and
Background. Xilem kept Glyph at the top in both overview and edit mode, so the
right panel's first glance differed even after the individual groups matched.

Xilem now follows the GPUI edit order while preserving Glyph as the first
overview section. The optional edit groups disappear in overview mode, so the
same view sequence serves both modes without duplicating the inspector.

- [Gray edit inspector order](127-xilem-editor-inspector-order-gray.png)
- [Light edit inspector order](128-xilem-editor-inspector-order-light.png)

Both 1280 by 720 captures were visually inspected. Coordinates now occupies
the top edge, and the collapsed Glyph header sits directly between Path
Operations and Curves in both themes. Static headless captures verify paint
and layout, not native pointer, keyboard, screen-reader, or GPU behavior.
