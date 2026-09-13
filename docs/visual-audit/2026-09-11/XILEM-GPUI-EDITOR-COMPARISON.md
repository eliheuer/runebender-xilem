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
