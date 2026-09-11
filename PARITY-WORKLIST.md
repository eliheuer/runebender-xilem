# Xilem parity execution checklist — 2026-09-11

This is the current execution plan. GPUI-PARITY.md, MENU-PARITY.md,
VISUAL-PARITY.md, and XILEM-SWITCH.md retain historical research and evidence;
their checked boxes are not current cross-platform acceptance. Update this file
as work lands. The objective is one polished Linebender-native application,
including headless Core, not continued development of two frontends.

## Immediate milestone: real Virtua Grotesk work

Updated by the user September 11: prioritize reaching the point where this editor
can be used for real Virtua Grotesk Latin/Arabic editing and local AI work.
Daily use should then drive bug discovery and fixes. Full GPUI parity remains the
long-term target, but exhaustive cosmetic, browser, Chat, and less-used feature
parity must not delay a dependable desktop workflow.

The next delivery is **ready for a supervised real-work trial**, with these gates:

- [x] R01 — Open the Virtua Grotesk designspace, choose the intended master/glyph,
  navigate/search Latin and Arabic, and see accurate outlines and metadata.
- [x] R02 — Make representative outline, metrics and anchor edits; undo/redo;
  save a disposable copy and reopen it without losing unrelated font data.
  Dirty-state, failed-save and external-change behavior must be trustworthy.
- [ ] R03 — Enter and preview representative Arabic and mixed Latin/Arabic text;
  verify joining, marks, direction, caret/selection and relevant shaping options.
  Explicitly identify any limitations preventing the user's actual Arabic work.
- [ ] R04 — Discover available local AI tasks/models, run a bounded disposable
  example, inspect/compare its proposal, explicitly install it, and undo it.
  Verify cancellation, clear failures and stale document/master/glyph protection.
  If a required model is unavailable, name the exact missing dependency; UI-only
  or fake-worker tests do not complete the real local-AI trial gate.
- [x] R05 — Exercise the node-editor route for that local workflow, including
  graph open/save, parameters, run/progress and output comparison. Provide a
  reproducible small demo rather than starting expensive training.
- [ ] R06 — Menus, text inputs, panels and canvas are clear and usable for these
  steps: no missing icons, clipped essential text, inaccessible controls or
  misleading enabled actions. Preserve the GPUI visual target while fixing
  workflow-blocking defects ahead of minor cosmetic differences.
- [ ] R07 — Deliver exact launch/trial steps, tested fixture and model details,
  screenshots, commits, validation and remaining limitations. Distinguish
  headless proof from native interactive verification. Invite user feedback
  through real use; do not label this milestone full parity.

Map work to existing D/E/T/A/M/V IDs and record evidence for both those IDs and
these trial gates. Prioritize data safety first, then an end-to-end editing and
AI workflow, then polish and bugs exposed by those workflows. Continue useful
independent work if one gate is blocked. Do not defer Arabic as optional polish.
This goal does not authorize modifying the user's real font sources: implement
and validate against disposable copies until the user begins the real trial.
The current scheduled workday still ends at 8 PM September 11; a multi-day goal
is not an extension of the runner's authorization.

## Baseline and findings

Reviewed Xilem main `e9553fcdbabb6bc5dd7addc60a0125c469ff533d` and GPUI
`79e3ab1d096bdcf2538d946c796a8a0cf01f0573`. Xilem tracked files were clean;
`.worktrees/` is untracked historical material and must be preserved. Core is
already imported at `crates/runebender-core`; do not repeat consolidation.
Xilem/Masonry resolve to upstream `b81d8d7a`, despite unused local xix patch
warnings. Read resolved sources, not a sibling framework checkout by assumption.

This review inspected current application/platform/widget code and prior plans,
and rendered the current overview headlessly. It is not a native interactive,
Linux, or browser certification. Fresh workspace test results are recorded below.

Concrete findings:

- The 1100×720 Gray overview capture has missing-glyph boxes for disclosure
  arrows in both sidebars and some status icons. This is visible in the fresh
  capture, not merely inferred from dependencies. Search and inspector typography
  have recent fixes, but all labels and fallback need validation together.
- Native menu construction exists in `src/actions.rs`; `with_default_menu(false)`
  prevents winit replacing it. Startup source fixes and forced in-window tests do
  not certify live native menu clicks, focused-input routing, or close behavior.
  The native update loop relies on ACTIONS and MENUS sharing order; they currently
  do. Protect that invariant or bind entries explicitly; it is not a proven bug.
- `src/platform/dialogs.rs::font` uses file-or-folder selection on macOS but only
  file selection elsewhere. Linux UFO directory opening needs a real solution.
- `src/view/canvas/editor.rs` handles Keyboard events and returns for other text
  events. Committed/preedit IME input needs investigation and implementation;
  key-event typing alone does not establish Arabic input parity.
- `src/view/panels/tabs.rs` explicitly says Chat is not connected. Do not mark
  local-AI parity complete just because subprocess and node infrastructure exist.
- Xilem has no GPUI-equivalent browser host/Trunk entry point in this baseline;
  native dependencies are unconditional. Web is a substantial separate port,
  not the same acceptance criterion as the Linux in-window menu bar.
- `src/widgets/scroll_viewport.rs` is an application workaround preserving Portal
  scrolling while moving scrollbar controls outside its clip. Validate active,
  idle, and resized states. An upstream visibility API is a contribution opportunity.
- Screenshot rendering uses `imaging_vello_cpu`; native Masonry uses its selected
  imaging texture backend. Record the resolved runtime backend before making
  claims about Vello GPU performance. CPU screenshots cannot prove GPU effects.
- Existing documents still contain obsolete worktree, external Core pin, and
  screenshot-command instructions. Cleanup should reconcile these, not remove history.

Reference: [Linebender projects](https://linebender.org/) describes Xilem/Masonry,
Kurbo, Vello, Parley, Fontique and Norad. Its project descriptions have their own
publication dates; the checked-out dependency source is authoritative for APIs.
Use these components in their intended roles. Visual spacing, borders, colors,
and missing icons are predominantly application work, not reasons to fork Xilem.

## Execution contract

Work on main in this repository. GPUI is a read-only reference; legacy sibling
Core is read-only. Read AGENTS.md and DESIGN.md. Preserve unrelated changes.
Do not launch a foreground GUI while the user works on other projects. Use
headless captures and available nonintrusive test infrastructure; record native
interaction gates as pending when they cannot be exercised. Never call a build,
a screenshot, or source presence full behavioral parity.

Implement several bounded items per run while useful work remains. Follow the
immediate real-work milestone above: prioritize D01–D04 and workflow-critical M/V
fixes, then the E/T/A items needed for R01–R07. Do not finish all visual rows
before beginning Arabic and local AI workflow validation. Do not spend the whole
day on one unavailable platform, rewrite the app, or repeatedly run the same
passing checks. A blocked item must name the exact blocker, evidence, and next
action; move to another independent item. No placeholder controls presented
as finished features. Preserve focus, accessibility, undo, and document identity.

Each row begins unchecked. Status may be `open`, `in progress`, `implemented /
verification pending`, `blocked`, or `verified`. To check a box, append commit,
validation, and evidence paths to the evidence ledger. Recheck baseline assumptions
before editing. Add narrowly scoped discovered gaps with new IDs instead of
silently expanding a row. If time ends, provide a useful handoff, not a parity claim.

Use disposable font copies for mutating tests. Never modify Virtua Grotesk sources
or run model installation/training over them. Green is frozen human-approved,
purple is human do-not-touch, blue is awaiting review. AI output must be reviewable
and explicitly installed; investigate existing automatic install behavior rather
than treating it as permission to alter real fonts. No downloads or costly model
jobs required merely to prove UI wiring. Do not push without a further request.

## Visual acceptance fixture

Use VirtuaGrotesk.designspace Regular, initial overview and selected ampersand,
then glyph R with a fixed text sample, and the bolden graph. Record selected
master, glyphs, scroll offset, zoom, expanded panels, theme, dimensions, scale,
font asset hash, renderer, and commit beside every capture. Compare the same
logical window size and DPI; normalize user-provided GPUI crops only with their
scale documented. Different column counts at different zooms are not a comparison.

Capture Gray and Light for each changed region, plus a Dark contrast check for
shared colors. Inspect the actual images including edges at 2×. Use persistent
`docs/parity/2026-09-11/` evidence for selected before/after images, not hundreds
of generated files. Keep a manifest with exact reproduction commands. If the
native reference cannot safely be launched, use GPUI source metrics and supplied
screenshots and label the comparison provisional.

Example current headless command (not the stale `--bin screenshot` instruction):

```sh
RUNEBENDER_SCREENSHOT=/tmp/parity-overview.png RUNEBENDER_SIZE=1100x720 \
RUNEBENDER_THEME=gray RUNEBENDER_SELECTED=ampersand RUNEBENDER_EXPAND=Glyph \
target/debug/runebender-xilem /path/to/VirtuaGrotesk.designspace
```

Build the binary first after source edits. Headless output is CPU rendering.

## Visual and interaction checklist

| Done | ID | Work and acceptance |
|---|---|---|
| [x] | V01 | Repair missing disclosure/status glyphs. Compare `view/recipes.rs`, `widgets/input_typography.rs`, sidebar/inspector labels and GPUI icons. Use vector icons or deliberate font fallback; no tofu in all three themes. |
| [x] | V02 | Create matched-state overview/editor/nodes proof manifest and reference metrics from GPUI `view/` and DESIGN. Record exact sizes, not approximate impressions. |
| [x] | V03 | Header and sidebar tabs: align common top boundary, active outline joins, four expected overview icons, conditional editor icons, radii and gap. Verify at 1×/2×; no duplicate dark stroke or light seam. |
| [ ] | V04 | Search and all inspector inputs: shared font/size, equal intended inset, unclipped ascenders/descenders, caret/selection, placeholder and typed text. Include gyp, Arabic marks, long names, numeric values and focused state. |
| [ ] | V05 | Panel edges and scrolling: no white remnant during hover, wheel, idle, resize or nested scroll. Retain wheel/trackpad and keyboard reachability. Check grid, both sidebars, menus, node panels. |
| [x] | V06 | Swatches: match palette values, diameter, uniform centers, selected ring, clear swatch centered dark X and hit boxes. Verify mark-color changes undo and update selected cells. |
| [x] | V07 | Grid: match cell aspect/zoom, outline scale/baseline, labels/Unicode, selection colors, strokes/shadows and spacing. At small size use deliberate clipping/ellipsis; never accidental descender loss. |
| [x] | V08 | Sidebar density: category/script/filter row heights, counts, disclosure indentation, full-width separators and selected highlight. Ensure lower filters remain reachable at short window sizes. |
| [ ] | V09 | Right panels: section order/default expansion, header heights, field alignment, Masters list and preview fit. Large and tiny glyph bounds must remain inside preview. |
| [ ] | V10 | Footer: grid/list icons, count, zoom slider, swatch row and dividers align with GPUI. All icons resolve; click/keyboard states work. |
| [ ] | V11 | Editor: point shapes/colors, selected rings, handles, anchors, metrics/guides, glyph info overlay and neighboring glyphs. Capture the same R/sample/zoom as reference. |
| [ ] | V12 | Curvature/continuity and preview: toggle behavior, clipping, overlay colors, blur/invert controls and text sizing. Distinguish CPU proof from runtime effects. |
| [ ] | V13 | Node editor: dotted grid, node headers, port spacing, curves, selected states, toolbar and graph tabs. Match bolden fixture and test pan/zoom/connect/drag. |
| [ ] | V14 | Responsive shell and resizable panels: inspect GPUI `widgets/resizable.rs`; preserve minimum widths, avoid overlap at short/narrow sizes, persist size only where intended. |
| [ ] | V15 | Theme and UI-font consistency: inventory remaining direct Xilem labels/inputs and raw colors; unify recipes without breaking fallback. Verify bundled UI font versus editable document glyph rendering separately. |

## Menus and platform checklist

Sources: Xilem `actions.rs`, `widgets/menu_shell.rs`, `widgets/context_menu.rs`,
`launch.rs`, `platform/dialogs.rs`; compare GPUI counterparts. Maintain an action
matrix of label, shortcut, context, enabled/checked state and handler result.

| Done | ID | Work and acceptance |
|---|---|---|
| [ ] | M01 | Diff full GPUI action/menu inventory, including submenus/separators. Map each action to a working Xilem command or explicit gap; no inert exposed actions. |
| [ ] | M02 | Native macOS startup and focus: menu survives initial window, reopen and app switch. Record real runtime verification separately from headless forced-menu tests. |
| [ ] | M03 | Menu state refresh: enabled/checked values track selection, document, tool, theme and undo. Protect ACTIONS/MENUS ordering or store action identity with menu items. |
| [ ] | M04 | Focus routing: menu-click and shortcut Undo/Redo/Copy/Paste/Select All affect focused input first; canvas otherwise. Audit Cut against GPUI. Test search, numeric input, preview and graph text. |
| [ ] | M05 | Quit/close/open/new with dirty document: Save/Discard/Cancel works, no native predefined Quit bypass, failed save never closes. Test isolated font. |
| [ ] | M06 | In-window menu keyboard/pointer: Alt access, arrows, Enter, Escape, outside click, submenu hover, disabled entries and focus restoration. Verify shortcuts execute exactly once. |
| [ ] | M07 | Linux dialogs can open UFO and glyphspackage directories as well as files; cancellation is harmless. Test on Linux, not just macOS cfg compilation. |
| [ ] | M08 | Linux X11/Wayland runtime menu and clipboard smoke checks where infrastructure exists. Record distro/session and unavailable cases honestly. |
| [ ] | M09 | Context menus respect selection and dismiss/focus rules in grid, editor and nodes; right-click does not silently mutate the wrong target. |
| [ ] | M10 | Accessibility and keyboard navigation: focus traversal, labels, menu roles and activation; ensure custom icon and scrollbar fixes retain semantics. |

## Document and editing checklist

Sources: Xilem `edit/`, `platform/host.rs`, `platform/watch.rs`, `workspace.rs`,
`view/panels/`; GPUI `edit/commands/`, `platform/config.rs`, `platform/journal.rs`.
Map behavior, not file count. Existing functions are verify/finish work.

| Done | ID | Work and acceptance |
|---|---|---|
| [ ] | D01 | Open/save/save-as/reopen designspace and UFO round trip preserves layers, masters, lib data and relative paths; failure surfaces and dirty state are accurate. |
| [ ] | D02 | External reload and unsaved edits: no watcher overwrite, stale cache, duplicate save loop or target change. Verify document/master identity and conflict handling. |
| [ ] | D03 | Undo grouping across drag, numeric edit, paste, path operation, master/glyph switch and AI install. Undo targets the originating document and remains reversible. |
| [ ] | D04 | Tabs/session/selection: active glyph, master, title, preview and status agree; close/reopen behavior matches. Investigate persistence/journal differences explicitly. |
| [ ] | E01 | Selection: point/path/marquee/additive, hit tolerance across zoom, keyboard navigation and cancel restore. Compare GPUI interactions with disposable glyphs. |
| [ ] | E02 | Drawing tools: pen, shape, knife, ruler, hand and text inventory; match modifiers, previews, commit/cancel and undo. Record absent tools instead of fake buttons. |
| [ ] | E03 | Point/path commands: join/split/reverse/delete, curve conversions, extrema, simplify and boolean operations retain valid contours and metadata. |
| [ ] | E04 | Coordinates/transforms: X/Y/W/H, origin matrix, flips, rotate, scale, align and distribute operate correctly on empty/single/multiple selections. |
| [x] | E05 | Glyph metadata/Unicode/advance and sidebearings: validation, live update, dirty state, undo and save/reload. Verified in `df4891b` and `8386791`: editor and overview fields reject invalid values, preserve correct LSB/RSB semantics, and Undo/Redo Unicode and rename atomically across masters in order with glyph edits; disposable one- and two-master fixtures save and reopen exactly. |
| [ ] | E06 | Anchors, components and composition: add/edit/delete, transformed composites, recursion/errors and attachment preview. |
| [ ] | E07 | Layers/masters/axes: selection, edit targeting, add/rename/delete where GPUI supports them, interpolation preview and incompatible outlines. |
| [ ] | E08 | Kerning/groups/features: inspect GPUI commands against panels; editing, validation, shaping refresh and round trip. No display-only panel counted as editing parity. |
| [ ] | E09 | Background images/SVG/trace: import, transform, visibility, opacity, remove, export and undo; preserve document-relative resources. |
| [ ] | E10 | Annotation/color/compare/filter/font-info inventory: port supported GPUI workflows or document intentional scope decision; exercise handlers, not labels. |
| [ ] | E11 | Export formats actually supported by GPUI: build/export a disposable fixture and reopen/inspect output; report diagnostics without false success. |

## Latin/Arabic and text checklist

| Done | ID | Work and acceptance |
|---|---|---|
| [ ] | T01 | Native IME composition and committed input in canvas text tool; preserve preedit/cancel and avoid duplicate insertion. Implemented in `793c3f9`; native runtime verification pending. |
| [x] | T02 | Arabic joining/marks/ligatures and mixed Latin/Arabic/digits/punctuation with live font changes. Verified in `1d96364` and `2acb9b3`: real Virtua shaping covers lam-alef, kasra placement and bidi runs, and existing text now reshapes immediately from a live Arabic glyph/metric/feature refresh without losing editing state. |
| [x] | T03 | RTL caret, selection, arrows, deletion and pointer hit mapping across ligatures/clusters. Verified in `1de39f6`: Core and UI share logical ranges while the canvas merges absorbed ligature sorts into visible selection geometry. |
| [x] | T04 | Shaping options/script/language/features and kerning refresh after glyph/feature edits. Verified in `2acb9b3` and `7063a0b`: text and preview share `liga`/`rlig`/`kern`/`mark`/`mkmk` plus Auto/Arabic/Urdu controls, and real Virtua proves disabling `rlig` changes lam-alef shaping while live refresh preserves the compiled-font path. |
| [x] | T05 | Preview text and editor text state, per-tab context and direction controls. Verified in `6ac0f14`: each tab parks committed editor text, preview text, direction, language, and feature choices under a stable document/tab identity; switching tabs or replacing/reloading the document restores the intended buffer instead of carrying stale widget state across contexts. |
| [x] | T06 | Arabic UI-input fallback, combining marks and clipboard round trip in search/metadata/preview. Verified in `efae328` and `e070f24`: canvas copy/cut/paste preserves logical Unicode, marks, and normalized line breaks while reshaping Arabic; the real-window-equivalent renderer proves distinct Arabic fallback glyphs in the shared search/metadata/preview input style on macOS. Incomplete source-font coverage remains a separate font-data issue. |

## Local AI and nodes checklist

| Done | ID | Work and acceptance |
|---|---|---|
| [x] | A01 | Compare installed `font-ml tasks --json` registry with UI task list and node types; expose missing-binary/model errors clearly without downloads. Verified in `9221fc8`: the installed registry reports `bolden` and `train` implemented and `complete`, `generate`, `spacing`, `kerning`, and `field` unavailable; panel rows and node types share that declaration, and runtime/exit/schema/JSON failures are visible. |
| [x] | A02 | Run/cancel/progress/failure lifecycle with fake or tiny fixture subprocess; UI stays responsive and processes terminate. Verified with deterministic worker tests in `a7630e5`; native pointer verification remains in R04. |
| [x] | A03 | Proposal review/install/discard: audit current immediate install and Undo install behavior; preserve explicit user-controlled install semantics and original layer. Verified in `1d96364` and `dd0159a`; native pointer verification remains part of R04. |
| [x] | A04 | Stale result safety: document/master/glyph/revision changes, close and reload cannot apply output to another target. Verified in `4dd2518` for direct tasks and node `core.install`, including edited targets, editor glyph switches, replacement documents, and changed all-glyph inventories. |
| [x] | A05 | Graph new/open/save/run, port type validation, parameter edits, graph tabs and failure location; round trip bolden and train-adapter fixtures without expensive training. Verified in `6d7cc51` plus the real review graph in `ce75e3b`; the toolbar uses the native Open picker, and a disposable train-adapter graph is saved, reopened, edited, validated, and given a node-local failure without starting training. |
| [x] | A06 | Live automation interface and headless Core: same operations/undo semantics, scoped authorization, conflict errors and discoverability; no disk writer bypass over live font root. Verified in `824d3ea`: CLI and MCP share the editor-owned unsaved `Project`; foreground mutations require `authorization=user-approved`; revisions, private endpoints, Core undo, and no-disk-write behavior are tested. |
| [x] | A07 | Chat panel parity: local GGUF selection, prompt/transcript/tool rows, streaming status, multi-turn context, cancellation, clearing, and live proposal refresh. Verified in `7dc5fab` with a deterministic `font-ml chat` process-contract test and inspected Gray/Light panel captures; an actual model turn remains a supervised trial step. |
| [x] | A08 | Talk/demo path: disposable Virtua sample → graph proposal → comparison → explicit install → undo. Verified in `ce75e3b` with the checked-in review-only graph and real CPU integration test. |

## Browser workstream

Dependent on safe platform boundaries; it must not block desktop progress.

| Done | ID | Work and acceptance |
|---|---|---|
| [ ] | W01 | Establish wasm compile feasibility with pinned Xilem/Masonry and identify each native-only dependency/API; write exact errors and minimal target-gating plan. |
| [ ] | W02 | Browser application entry/render host using Linebender stack; map GPUI `platform/web_host.rs`, Trunk/index and workspace-server protocol. No substitute static mockup. |
| [ ] | W03 | Browser file/workspace load/save and live reload preserve ETag/conflict semantics; asynchronous dialogs, clipboard and font loading have explicit errors. |
| [ ] | W04 | Browser menu pointer/keyboard/focus behavior and reserved shortcuts tested in an actual browser, including key-up. Desktop in-window tests are insufficient. |
| [ ] | W05 | Browser text/IME/RTL, renderer fallback and representative editing smoke test; record browser/GPU capability and unsupported features. |
| [ ] | W06 | Local AI browser bridge is explicit, scoped and cancellable; do not expose unrestricted local execution or pretend native subprocess APIs work in wasm. |

## Cleanup, contribution and release gates

| Done | ID | Work and acceptance |
|---|---|---|
| [ ] | Q01 | Update stale AGENTS/build/workspace/pin instructions and current-plan pointers. Keep historical evidence clearly historical; do not delete worktrees. |
| [ ] | Q02 | Consolidate repeated UI recipes/tokens only where touched; no broad churn or GPUI architecture transplant. Add changelog notes for shipped behavior. |
| [ ] | Q03 | Document minimal upstream opportunities: scrollbar visibility, text-input sizing/fallback, menu lifecycle/focus, IME/accessibility, browser host. Provide small reproducer before proposing framework patch. |
| [ ] | Q04 | Record runtime renderer and benchmark representative large-font grid, drag/zoom, clipping/transparency/blur and node graph. CPU screenshot speed is not runtime evidence. |
| [ ] | Q05 | Focused tests per change; coherent phase runs workspace fmt, clippy, docs, tests and release build per CI. Dependencies changed → vet/deny evidence; do not hide failures. |
| [ ] | Q06 | Final platform matrix and remaining gaps: macOS native, Linux in-window, browser each separate. Screenshot manifest, commits, test results and runnable review instructions. |

## Evidence ledger and resume point

Initial screenshot: `docs/parity/2026-09-11/audit-overview-gray.png`, generated
headlessly at 1100×720 from existing debug binary on September 11. Binary freshness
relative to source must be established by the worker before treating it as a
regression baseline. The image visibly confirms missing icon glyphs; it does not
prove a native window state. Capture logged sandbox-denied live-tool startup,
which is not an application networking regression.

Resume action: read current git status and the evidence ledger, finish any
current bounded fix, then select the next blocker to R01–R07. V01 is already
recorded below; do not restart completed work or follow the superseded
visual-only order.
Add dated ledger entries with IDs, before/after image paths, commits, commands and
honest remaining gates. Never check all rows based on a generic test suite pass.

Review validation: `cargo test --workspace --locked -- --test-threads=1` passed
429 tests (343 library, 2 binary, 15 CLI, 1 fixture, 5 integration, 63 editor;
zero failures). Full log retained locally at `/private/tmp/xilem-parity-audit-tests.log`.
`git diff --check` passed. This review did not rerun release/clippy or certify
native/Linux/browser interaction. Those remain phase gates for implementation.

### 2026-09-11 implementation ledger

- V01 — verified in `5a5adfc`: the bundled font-dependent triangles, bullets,
  and Layers marker were replaced by theme-colored vector geometry matching the
  GPUI 10×10 marker source. Fresh Gray, Light, and Dark captures are
  `v01-overview-{gray,light,dark}.png`; visual inspection found no tofu.
  `cargo test --locked --bin runebender-xilem -- --test-threads=1` passed 63/63.
- V02 — verified in `5a5adfc`: `docs/parity/2026-09-11/MANIFEST.md` records
  the exact 1100×720 overview, editor-R, and bolden-node states, source-derived
  GPUI metrics, font/graph hashes, renderer, theme, commands, and limitations.
  Gray and Light state captures are retained beside it.
- V03 — verified by `e9553fc` and fresh captures from `5a5adfc`: the common
  top rule, four overview tabs, five conditional editor tabs, selected tab join,
  18 px icons, 6 px top radius, and 4 px gap were inspected at 1× and at a true
  fixed-logical-size 2×. Evidence: `v01-overview-gray.png`,
  `v01-overview-gray-2x.png`, `editor-r-gray.png`, and
  `editor-r-gray-2x.png`. No duplicate top rule or open-edge seam was visible.
- V08 — verified by current source plus `5a5adfc`: GPUI's 19 px rows, 14 px
  inset, 10 px painted markers, right-aligned counts, full-width group rules,
  and inverted selected row are present in the fresh overview captures.
  `v08-overview-short-gray.png` records the 1100×480 layout with a fixed mark
  bar; `widgets::scroll_viewport::tests::active_scroll_and_resize_never_paint_bars`
  exercised two-axis wheel scrolling and resize while the groups remain in the
  Masonry portal. Native trackpad behavior remains part of the later platform gate.
- V06 — verified in `dd9257a`: the existing 24 px slots, 18 px circles,
  uniform 6 px gutters, selected ring, and painted clear X are visible in the
  retained Gray/Light/Dark overview captures. The mark action now targets the
  actual overview selection, records one source-identified batch for a
  multi-selection, refreshes glyph and cell caches, and supports Undo/Redo.
  Core `GlyphSnapshot` now includes the glyph lib and the rest of glyph metadata,
  rather than silently restoring outlines while leaving the mark behind.
  `overview_mark_batch_updates_cells_and_undoes_once` and both Core
  `snapshot_restore_roundtrip` tests pass. Full Xilem tests pass 64/64; full
  Core library tests pass 343/343 when allowed to create their temporary Unix
  socket (the sandboxed run passed 342 and denied that one socket operation).
- V04 — in progress in `793c3f9`: a focused Masonry input test verifies the
  shared 28 px control does not vertically clip selected `gyp` descenders or
  signed decimal text, in addition to the existing placeholder/typed-text ink
  equivalence test. Arabic fallback, long-name horizontal clipping, and a live
  native caret/selection pass remain before verification.
- T01 — implemented / native verification pending in `793c3f9`: the canvas text
  tool now treats `Ime::Preedit` as visible uncommitted state, clears it on
  cancellation/disable, inserts `Ime::Commit` exactly once, and consumes the
  corresponding logical character key so application shortcuts cannot duplicate
  or steal it. The focused widget test covers preedit, cancellation, commit,
  duplicate avoidance, and parked state; the 65-test Xilem binary suite and
  workspace clippy pass. A real macOS/Linux input-method session is still required
  before checking T01.
- V07 — verified in `a8f8268`: source comparison corrected overview padding to
  GPUI's 8×8 px, restored the editor rail's 44 px target with 6 px padding, and
  made Cmd/Shift selection preserve its primary and extend existing selections.
  Caption thresholds now reserve stable 0/1/2/3-line blocks at the same 48/90 px
  boundaries for encoded and unencoded glyphs. Square tiles are encoded as true
  rectangles; this fixes Vello CPU dropping Gray-theme outline and label draws
  after a zero-radius rounded rectangle. Fresh inspected evidence is
  `v07-overview-{gray,light,dark}.png`, `v07-overview-gray-2x.png`, and
  `v07-editor-r-gray.png`. Three focused grid tests plus the modifier-selection
  test pass in the 68-test Xilem binary suite; workspace clippy passes. The same
  commit refits the text viewport after every IME transition so a cancelled or
  rejected composition cannot leave stale composition geometry. Native GPU and
  trackpad interaction remain covered by the later platform trial, not this row.
- R02 — verified in `588b605`: an ignored integration test copies the full
  adjacent 13 MB Virtua source tree, edits a point, advance and anchor on Regular
  R, exercises Undo/Redo, saves and reopens, keeps the designspace bytes intact,
  and compares the complete reopened font after normalizing only R. Navigation
  no longer dirties the document; a normal test proves an external reload cannot
  overwrite unsaved work. The existing unwritable-source test proves save failure
  remains dirty and reports `Save failed`. All passed in the 70-test normal suite
  plus the three-test real-fixture run.
- R01 — verified in `a40808b`: the real designspace integration opens both
  Regular and Bold, exercises the Arabic script filter and name search, opens R,
  beh-ar, kasra-ar and unencoded lam_alef-ar, and compares the full session glyph,
  Unicode field and advance with the active source before switching Bold and back.
  The same test continues into R02's disposable save/reopen proof. Individually
  inspected editor evidence is `r01-arabic-beh-{gray,light}.png`; native pointer
  interaction remains part of the supervised trial, not this data-path gate.
- R03 in progress / T02 and T03 verified — `1d96364`, `1de39f6`, and
  `2acb9b3`: the real Virtua integration test
  shapes `R لا 123 بِ`, verifies source coverage, lam-alef substitution, mark
  ordering, finite outline paths, bidi layout, logical pointer mapping through
  lam-alef, merged visible selection geometry, and Arabic reshaping after deleting
  the selected beh. Shift+arrows/Home/End extend bidi-aware logical ranges; typing
  and IME commits replace them. Gray and Light evidence is
  `r03-mixed-text-{gray,light}.png` and `r03-text-selection-{gray,light}.png`.
  T03 is verified. Native IME and pointer-gesture verification remain for the
  supervised trial, so R03 is not checked.
- T02 — completed in `2acb9b3`: refreshing the live font inventory now rebuilds
  base sorts and reruns the complete shaper immediately, instead of leaving old
  substitutions and advances visible until another keystroke. A normal Arabic
  test proves updated positional-form width plus selection/manual-kern retention;
  the real Virtua test changes the shaped beh form and observes the existing
  mixed line update by exactly 17 units.
- T04 — verified in `7063a0b`: a second preview row now exposes common OpenType
  feature toggles and Auto/Arabic/Urdu script-language choices to both the text
  tool and specimen preview. The real Virtua test proves `rlig` off separates
  lam-alef into two editable sorts. Inspected Gray/Light evidence is
  `t04-shaping-options-{gray,light}.png`; Urdu and all features except `rlig` are
  visibly selected without clipping.
- T05 — verified in `6ac0f14`: the canvas reports every committed logical-text
  mutation back to plain application state, while each editor tab parks its own
  editor text, preview text, direction, language, and feature choices behind a
  stable document/tab identity. Focused tests switch between two distinct
  contexts and preserve the active context through disk reload; the full normal
  workspace suite passes 453 tests. Native tab clicks remain part of R06 rather
  than a condition of this state-isolation row.
- T06 — verified in `efae328` and `e070f24`: the canvas text tool now routes
  system clipboard copy, cut, paste, and select-all through logical Unicode text,
  normalizes CRLF/CR to line breaks, and reshapes after every cut or paste. Core
  preserves beh plus kasra and multiline selections; the focused widget test
  covers clipboard signals and round trip; the ignored real Virtua test confirms
  pasted beh plus kasra survives shaping and selection. A macOS headless test
  uses the same system-font-enabled renderer as the real window and renders beh
  and alef distinctly through the shared search/metadata/preview input style.
  Native OS clipboard delivery remains a supervised R03/R06 interaction check.
- M04 — native-menu blocker recorded after source audit: focused Masonry text
  widgets correctly consume keyboard Undo/Redo/Copy/Cut/Paste/Select All before
  canvas commands, but macOS `muda` menu clicks arrive outside the widget event
  tree and dispatch `AppAction` directly. Masonry inputs are canvas-rendered, so
  AppKit predefined edit selectors cannot target them. This remains unchecked
  pending a window-command hook or focused-widget command proxy; no native-menu
  behavior is inferred from the keyboard tests.
- A03 — verified in `1d96364` and `dd0159a`: completed single-glyph jobs remain
  proposals until explicit Install or Discard. The Local AI panel now toggles a
  warm on-canvas proposal overlay; Install/Discard clear review state and Undo
  restores the foreground. The real CPU test over disposable Virtua R moved
  40/40 points with advance delta +18, left the original foreground untouched,
  installed explicitly, and undid to byte-equivalent glyph data. Individually
  inspected Gray/Light captures are `r04-ai-compare-{gray,light}.png`.
- R04 — in progress through `1d96364`, `dd0159a`, `a7630e5`, and `4dd2518`:
  installed task discovery,
  a bounded real model run, pending proposal, comparison, explicit install and
  Undo are verified. Direct and node jobs now capture the document, master,
  editor glyph, and canonical target revisions; completed work is rejected after
  reload, target edits or switches, and all-glyph inventory changes. A
  deterministic subprocess test observes progress, kills the worker, waits for
  termination, and proves no proposal layer remains; another preserves every
  stderr diagnostic from a failing worker. Native progress/Cancel and failure
  interaction still require the supervised trial, so R04 remains unchecked.
- A02 — verified in `a7630e5`: model work remains on a background thread; the
  deterministic fake worker proves progress reaches shared state, Cancel kills
  and joins the child promptly, the job clears, and no proposal or foreground
  mutation survives. A separate failing worker proves multi-line diagnostics
  are not reduced to the last line. Both focused tests and workspace clippy pass.
- A01 — verified in `9221fc8` against the installed `font-ml tasks --json`:
  `bolden` and `train` are implemented; `complete`, `generate`, `spacing`,
  `kerning`, and `field` are unavailable. The Local AI rows and generated node
  types retain the same names, titles, ports, and availability. A missing binary
  keeps the install/path instruction; command failure, nonzero exit, malformed
  JSON, and a missing tasks array now remain visible instead of producing an
  unexplained empty rail. No model was downloaded or changed.
- R05 / A08 — verified in `ce75e3b`: the checked-in five-node
  `bolden-review.nodes.json` has no Install node. Node runs now give Core the
  designspace source so sibling masters resolve, target the open or explicitly
  selected glyphs instead of accidentally treating an empty editor selection as
  every glyph, and preserve the configured model device in cache identity. A real
  ignored integration test copies Virtua, edits/saves/reopens the graph, runs R
  and S through the CPU model, observes start/progress/end and Compare output,
  adopts the proposal without changing foregrounds, then explicitly installs and
  undoes R. Gray/Light captures are `r05-nodes-review-{gray,light}.png`. The
  installed tool's optional fitted-reference path panicked during investigation;
  the fixture records that blocker by leaving Bolden's reference port disconnected
  while keeping the separate Bold-master Compare node.
- A05 — verified in `6d7cc51` and `ce75e3b`: New creates a collision-safe graph
  beside the font, Open invokes the existing native `.nodes.json` picker, Save
  rescans graph tabs, and the real review graph exercises parameters, progress,
  proposal output, and reopening. A normal disposable train-adapter graph test
  validates typed ports, edits and persists parameters, reopens it, and retains
  the exact failing-node diagnostic without running training.
- A06 — verified in `824d3ea`: the real CLI and MCP adapters connect to the same
  editor-owned, unsaved `Project`, read its current revision, create a proposal,
  reject an install without explicit `authorization=user-approved`, install only
  with that authorization, and reread the changed advance without ever creating
  the project UFO path. The live schemas advertise this required enum for proposal
  install and experiment apply/undo; Core retains atomic revision conflicts and
  undo semantics. The focused live tests pass 5/5, the Unix-socket integration
  passes 1/1 when allowed to create its private endpoint, and workspace clippy
  passes with warnings denied.
- A07 — verified in `7dc5fab`: the placeholder is replaced by the GPUI-shaped
  local workflow over `font-ml chat` and the editor's private live endpoint.
  It discovers GGUF plus tokenizer folders, prefers the 4B model, keeps multi-turn
  messages, streams prose and tool-result rows, exposes Cancel and Clear, kills a
  child when its workspace closes, and refreshes proposal review state when a turn
  finishes. A deterministic fake process verifies JSON-line events and exact
  conversation transfer; transcript markup tests and the complete 89-test Xilem
  suite pass, as does workspace clippy. Inspected 1100×720 evidence is
  `a07-chat-{gray,light}.png`. No model ran for the captures, so the real 4B turn
  remains in the supervised trial rather than being claimed from static evidence.
- E05 — verified in `df4891b` and `8386791`: inspector-originated Unicode and metric changes
  now transfer their pending session history into Core before replacing the live
  glyph, so Undo/Redo no longer silently loses those edits. Numeric fields reject
  NaN and infinities; LSB moves ink without changing advance; a rejected rename
  restores the real glyph name and reports the collision. A disposable UFO test
  validates all of those paths and reopens the saved Unicode, RSB-derived width,
  and successful rename. Unicode and glyph-name changes now form one ordered,
  invertible metadata history across every designspace master; Core moves each
  renamed glyph's existing undo/redo pile to its new identity, and overview width
  edits join the same command ordering. A two-master fixture proves editor and
  overview Unicode, rename, and width Undo/Redo followed by save/reopen. The full
  workspace suite passes 459 tests with four expensive tests ignored, and
  workspace clippy passes with warnings denied.
- R07 — trial instructions, exact local runtime/model hashes, warnings, capture
  commands, commits, validation, and remaining native/RTL limits are recorded in
  `docs/parity/2026-09-11/REAL-WORK-TRIAL.md`. It remains unchecked until the
  supervised native trial is completed.
- R06 — the retained 1100×720 Gray/Light pairs for Arabic editing, text
  selection, Local AI comparison, and the review-only node graph were reinspected
  after `6d7cc51`. Essential text and actions remain legible, selection/caret and
  proposal/foreground states are distinguishable, and no new headless visual
  blocker was found. Native menus, input delivery, and pointer behavior remain
  trial work, so R06 stays unchecked.
