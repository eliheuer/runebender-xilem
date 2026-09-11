# Xilem parity execution checklist — 2026-09-11

This is the current execution plan. GPUI-PARITY.md, MENU-PARITY.md,
VISUAL-PARITY.md, and XILEM-SWITCH.md retain historical research and evidence;
their checked boxes are not current cross-platform acceptance. Update this file
as work lands. The objective is one polished Linebender-native application,
including headless Core, not continued development of two frontends.

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

Implement several bounded items per run while useful work remains. Start with
V01–V08, M01–M06, D01–D04, then editing/Arabic and node workflows. Do not spend the
whole day on one unavailable platform, rewrite the app, or repeatedly run the
same passing checks. A blocked item must name the exact blocker, evidence, and
next action; move to another independent item. No placeholder controls presented
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
| [ ] | V01 | Repair missing disclosure/status glyphs. Compare `view/recipes.rs`, `widgets/input_typography.rs`, sidebar/inspector labels and GPUI icons. Use vector icons or deliberate font fallback; no tofu in all three themes. |
| [ ] | V02 | Create matched-state overview/editor/nodes proof manifest and reference metrics from GPUI `view/` and DESIGN. Record exact sizes, not approximate impressions. |
| [ ] | V03 | Header and sidebar tabs: align common top boundary, active outline joins, four expected overview icons, conditional editor icons, radii and gap. Verify at 1×/2×; no duplicate dark stroke or light seam. |
| [ ] | V04 | Search and all inspector inputs: shared font/size, equal intended inset, unclipped ascenders/descenders, caret/selection, placeholder and typed text. Include gyp, Arabic marks, long names, numeric values and focused state. |
| [ ] | V05 | Panel edges and scrolling: no white remnant during hover, wheel, idle, resize or nested scroll. Retain wheel/trackpad and keyboard reachability. Check grid, both sidebars, menus, node panels. |
| [ ] | V06 | Swatches: match palette values, diameter, uniform centers, selected ring, clear swatch centered dark X and hit boxes. Verify mark-color changes undo and update selected cells. |
| [ ] | V07 | Grid: match cell aspect/zoom, outline scale/baseline, labels/Unicode, selection colors, strokes/shadows and spacing. At small size use deliberate clipping/ellipsis; never accidental descender loss. |
| [ ] | V08 | Sidebar density: category/script/filter row heights, counts, disclosure indentation, full-width separators and selected highlight. Ensure lower filters remain reachable at short window sizes. |
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
| [ ] | E05 | Glyph metadata/Unicode/advance and sidebearings: validation, live update, dirty state, undo and save/reload. |
| [ ] | E06 | Anchors, components and composition: add/edit/delete, transformed composites, recursion/errors and attachment preview. |
| [ ] | E07 | Layers/masters/axes: selection, edit targeting, add/rename/delete where GPUI supports them, interpolation preview and incompatible outlines. |
| [ ] | E08 | Kerning/groups/features: inspect GPUI commands against panels; editing, validation, shaping refresh and round trip. No display-only panel counted as editing parity. |
| [ ] | E09 | Background images/SVG/trace: import, transform, visibility, opacity, remove, export and undo; preserve document-relative resources. |
| [ ] | E10 | Annotation/color/compare/filter/font-info inventory: port supported GPUI workflows or document intentional scope decision; exercise handlers, not labels. |
| [ ] | E11 | Export formats actually supported by GPUI: build/export a disposable fixture and reopen/inspect output; report diagnostics without false success. |

## Latin/Arabic and text checklist

| Done | ID | Work and acceptance |
|---|---|---|
| [ ] | T01 | Native IME composition and committed input in canvas text tool; preserve preedit/cancel and avoid duplicate insertion. Source Keyboard-only path is an audit priority. |
| [ ] | T02 | Arabic joining/marks/ligatures and mixed Latin/Arabic/digits/punctuation with live font changes. Compare actual shaping output, not character reversal. |
| [ ] | T03 | RTL caret, selection, arrows, deletion and pointer hit mapping across ligatures/clusters. Core buffer and UI must agree on logical versus visual positions. |
| [ ] | T04 | Shaping options/script/language/features and kerning refresh after glyph/feature edits. Distinguish compiled-font shaping from fallback outline preview. |
| [ ] | T05 | Preview text and editor text state, per-tab context and direction controls: no stale initial-only binding or cross-document contamination. |
| [ ] | T06 | Arabic UI-input fallback, combining marks and clipboard round trip in search/metadata/preview. Do not confuse incomplete source-font coverage with editor bugs. |

## Local AI and nodes checklist

| Done | ID | Work and acceptance |
|---|---|---|
| [ ] | A01 | Compare installed `font-ml tasks --json` registry with UI task list and node types; expose missing-binary/model errors clearly without downloads. |
| [ ] | A02 | Run/cancel/progress/failure lifecycle with fake or tiny fixture subprocess; UI stays responsive and processes terminate. |
| [ ] | A03 | Proposal review/install/discard: audit current immediate install and Undo install behavior; preserve explicit user-controlled install semantics and original layer. |
| [ ] | A04 | Stale result safety: document/master/glyph/revision changes, close and reload cannot apply output to another target. Test both single task and node core.install path. |
| [ ] | A05 | Graph new/open/save/run, port type validation, parameter edits, graph tabs and failure location; round trip bolden and train-adapter fixtures without expensive training. |
| [ ] | A06 | Live automation interface and headless Core: same operations/undo semantics, scoped authorization, conflict errors and discoverability; no disk writer bypass over live font root. |
| [ ] | A07 | Chat panel parity: currently unconnected. Trace GPUI behavior and implement its local workflow or clearly record dependency; never claim the panel is functional. |
| [ ] | A08 | Talk/demo path: disposable Virtua sample → graph proposal → comparison → explicit install → undo. Record reproducible setup and missing model blockers. |

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

First worker action: read current git status and this checklist, build current
binary, reproduce V01, implement and inspect the correction, then continue V02–V08.
Add dated ledger entries with IDs, before/after image paths, commits, commands and
honest remaining gates. Never check all rows based on a generic test suite pass.

Review validation: `cargo test --workspace --locked -- --test-threads=1` passed
429 tests (343 library, 2 binary, 15 CLI, 1 fixture, 5 integration, 63 editor;
zero failures). Full log retained locally at `/private/tmp/xilem-parity-audit-tests.log`.
`git diff --check` passed. This review did not rerun release/clippy or certify
native/Linux/browser interaction. Those remain phase gates for implementation.
