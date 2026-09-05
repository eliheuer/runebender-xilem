<!-- Copyright 2026 the Runebender Authors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# GPUI parity plan

Implementation priorities: [XILEM-SWITCH.md](XILEM-SWITCH.md) defines the owner's
immediate requirements: Virtua Grotesk Latin/Arabic editing, node/local AI workflows
for a talk, and polished UI throughout. Use that workflow order rather than treating
the numbered phases below as a mandatory sequential schedule. Full parity remains
the target.

Research baseline: 2026-09-05. Goal: bring Xilem up to the GPUI editor's useful
capabilities, keep both shells while that happens, and make Xilem the eventual
primary editor. This is an implementation handoff, not a claim of tested parity.

The architectural goal is to move as much as practical onto the Linebender stack,
follow its design practices, and create opportunities for the owner to contribute
to that ecosystem. GPUI is the behavioral reference, not the implementation template.
Parity is a milestone toward a Linebender-native editor, not the final objective.

For each task, inspect the pinned Linebender APIs, examples and conventions before
porting GPUI code. Prefer core's norad/kurbo operations, Xilem's reactive views,
Masonry widgets and event lifecycle, Parley/Fontique for UI text, AccessKit semantics,
and imaging with a suitable Vello backend. Keep the font's own shaping in core's
text engine. Share concepts and font operations; do not impose GPUI's entity/input
architecture on Xilem. Use the ecosystem's existing platform integrations where
appropriate rather than demanding that every dependency be Linebender-owned.

When a reusable capability is missing, record its proposed upstream home, current
API or issue, a minimal demonstration, and a contribution-sized next step. An
application adapter can unblock parity, but should not quietly become a permanent
replacement for ecosystem work. Upstream collaboration proceeds alongside app
development; it need not wait until parity is complete.

## Assessment

Native parity looks feasible. This audit found substantial unfinished application
work and several framework integration costs, but no demonstrated architectural
barrier requiring a wholesale Xilem fork. The largest coupled task is the text
editing/preview workflow; the highest-risk task is preserving document and undo
semantics while porting commands. Desktop integration and compile-time growth
deserve early proofs before filling out every panel.

The engine is already shared. Both manifests pin core at `8cc5370` with default
features disabled. Xilem's `FontModel` already wraps core's `Project`; do not start
by replacing it with another document model. Extract toolkit-independent font
operations into core when necessary, and keep painting, widgets, and OS integration
in the shells. There is no reason to reproduce GPUI's tessellation workarounds.

The baseline is local source at these commits, not a measured release comparison:

| Repository | Commit |
| --- | --- |
| runebender-gpui | `b403ba7d056b5ea952ac053bf5e70bf41de2233c` |
| runebender-xilem | `ad21ddb93465506d7a494a2c264bf055951dfc48` |
| Upstream Xilem dependency | `b81d8d7a` |

Source and pinned upstream APIs were inspected. Neither GUI was launched, and no
build, interaction test, performance benchmark, or rendering comparison was run
for this planning change. Existing code means “verify/finish,” not “done.” No
percentage or calendar estimate is justified until the baseline passes below.

## What already exists

These are starting points to preserve, rather than features to rewrite:

| Capability | Xilem evidence | Remaining qualification |
| --- | --- | --- |
| Shared project, masters, undo | `src/model.rs`, `src/edit/session.rs` | Verify every mutation path, not just point edits. |
| Glyph grid/list, filters, search, marks | `src/view/canvas/{grid,list}.rs`, `src/edit/sidebar.rs` | Audit selection, search semantics, and scale. |
| Select, pen, hyper pen, shapes, knife, measure | `src/view/canvas/editor.rs`, `src/edit/session.rs` | Detailed gesture/command coverage is incomplete. |
| Tabs and parked sessions | `src/edit/session.rs`, `src/view/panels/tabs.rs` | Reload currently reconstructs the workspace. |
| Native macOS menu and shortcuts | `src/actions.rs`, `src/widgets/shortcuts.rs` | Menu is a subset; non-macOS menu install is empty. |
| Context menu layers | `src/widgets/context_menu.rs` | Custom Masonry integration; test focus/dismissal. |
| Watch/reload, save, new template font | `src/platform/{watch,host}.rs` | No file dialog; New writes beside the current source. |
| Background/reference outlines and axis preview | `src/view/panels/sections.rs`, `src/model.rs` | Advanced layers/variation editing still missing. |
| Text tool | `src/edit/text_tool.rs`, `src/view/canvas/editor.rs` | Widget-owned buffer; keyboard path needs IME/clipboard audit. |
| Preview strip | `src/view/panels/preview.rs` | Draws one glyph; GPUI draws the text buffer's line. |
| Kerning/groups/features panels | `src/view/panels/editor_info.rs`, `src/edit/inspector.rs` | Not evidence of all GPUI controls or semantics. |
| Nodes, Local AI, proposals, live endpoint | `src/edit/{nodes,local_ai}.rs`, `src/platform/live.rs` | Chat/experiments are separate GPUI additions. |
| Headless screenshots | `src/platform/screenshot.rs`, `src/launch.rs` | CPU screenshots do not validate native GPU behavior. |

`docs/XILEM-GAPS.md` is historical. Its missing-menu, missing-tabs, missing-watch,
missing-background and missing-new-font statements no longer describe this tree.
Its GPUI comparison also assumes gpui-component, which GPUI no longer depends on.
The scaffolding line counts should not be used to estimate migration cost.

## Main blockers and how to approach them

| Problem | Classification | Approach / contribution opportunity |
| --- | --- | --- |
| Open/Save As/export/import workflows absent from Xilem shell | Application + platform integration | Core handles font work; add dialogs and async process/file results with cancellation and errors. |
| Text buffer hidden inside canvas; preview cannot consume it | Application architecture | Define shared plain render data and explicit editing events; preserve widget-local IME/focus state. Do not force core's non-Send `TextBuffer` into a Send/Sync view. |
| Shortcut routing, menu state, native edit commands | Application + reusable framework integration | Test focused fields versus canvas first; a command scope abstraction could benefit upstream. |
| Linux/Windows menu shell unfinished | Platform integration | In-window menus can use layers. Pinned Masonry `DriverCtx::window` exists; absence of an ergonomic Xilem view API is not proof that OS access is impossible. |
| Popups/dialogs need custom Masonry plumbing | Framework ergonomics | Keep application adapters initially; consider reusable Xilem layer ownership, result delivery, and focus restoration APIs. |
| Deep view/style types cause compile/link growth | Framework/compiler interaction | Preserve `.boxed()` boundaries, measure clean and incremental builds, minimize a repro. Local root boxing is already documented; upstream issue #1777 reports a related style-chain problem. Do not assume identical causes. |
| Many GPUI controls have no Xilem equivalent in this application | Application work | Build vertical slices; use existing upstream Split/TextInput/etc. before adding custom widgets. |
| Font cache/session invalidation across tabs, masters, tasks | Shared-state correctness | All edits reach core history; test round trips and undo after switching context, task completion, and reload. |
| Rendering suitability at editor workloads unmeasured | Renderer validation | Dense outlines, winding, extreme zoom, blur, images, and overlays need correctness/performance fixtures on the actual GPU backend. |
| Native and browser are different migration scopes | Platform architecture | Native first. A Xilem DOM backend is not an automatic port of custom Masonry widgets. Browser parity gets a separate gate. |

GPUI's `src/view/paint.rs::paint_batched` explicitly splits batches to work around
a documented `u16` tessellation vertex limit and silently drops a single failed
subpath. The preview also uses CPU rasterization for blur. These are concrete
reasons to investigate another renderer, although this audit did not reproduce
the user's particular rendering failures.

The pinned Masonry shell uses `masonry_imaging`; `masonry_winit` defaults to
`imaging_vello` and exposes a Hybrid feature as well. This app's screenshot path
uses `imaging_vello_cpu`. Select and record the actual backend rather than treating
all Vello implementations as interchangeable. General path blur must be proved
through the chosen imaging/backend combination; the old statement that Vello
“blurs what it is asked to” is not an acceptance test.

## How to execute the checklist

Every unchecked item needs implementation or verification. Labels mean:
**M** = missing shell capability found in source; **P** = partial implementation;
**V** = present or plausible but behavioral equivalence unverified; **R** = research
spike. These labels do not assert upstream impossibility.

For each ID record the implementing commit, automated checks, fixture, platform,
any remaining limitation, and the Linebender API used or contribution opportunity
before checking it. A visible button or successful
compile is insufficient. Split grouped items into child tasks before coding.
Do not copy bugs from GPUI or silently narrow the target to what is easy in Xilem.

Paths in the “GPUI reference” sentences are relative to the GPUI repository;
other paths are relative to this repository. Use the baseline commits above if
GPUI moves. Do not put local sibling checkout paths into committed config files.

### Phase 0 — establish a stable target

- [ ] **L01 / R** Maintain a contribution map alongside this checklist: need,
  app/core/Xilem/Masonry/masonry_winit/imaging/Vello/Parley/AccessKit ownership,
  existing upstream API or issue, minimal repro, proposed change, and status.
  Read the relevant upstream architecture and contributor guidance for each area.
  Identify a small initial contribution that supports an actual parity task.
- [ ] **B01 / V** Record clean-tree status, toolchains, dependency pins and local
  Cargo overrides; run current native tests/build gates in both shells. Keep
  dependency updates separate from feature ports so failures are attributable.
- [ ] **B02 / V** Make a command inventory from GPUI `src/actions.rs`, `src/launch.rs`,
  `src/edit/commands/`, `src/wiring.rs`, panels and context menus. Map each action to
  a checklist ID and an Xilem handler or an explicit gap; include panel-only actions.
- [ ] **B03 / V** Establish disposable fixtures: Virtua Grotesk designspace, Arabic
  and mixed-direction text, empty UFO, composite/layer/color/image cases, malformed
  inputs, and a dense synthetic outline. Record input hashes and expected results.
- [ ] **B04 / R** Measure clean/incremental build time and peak memory around representative
  large views; document type-erasure boundaries and a minimal style-chain repro.
- [ ] **B05 / R** Prove one file-dialog round trip, focused-field versus canvas shortcut
  routing, one popup with focus restoration, and a dense native GPU frame before
  investing in all remaining panels. Classify failures by app/Xilem/Masonry/backend.

### Phase 1 — a dependable document shell

GPUI reference: `src/edit/commands/file.rs`, `src/platform/host.rs`,
`src/launch.rs`, `src/actions.rs`, `src/platform/{config,journal}.rs`.

- [ ] **D01 / M** Launch without a positional font; offer New/Open with error recovery.
  Opening an empty UFO must not fail merely because it contains no glyphs.
  - [x] **D01.a** Empty UFO overview and first glyph: a zero-glyph UFO now opens
    in the overview with no selection or tab, and the existing New Glyph command
    opens its first created glyph normally. Regression coverage:
    `platform::host::tests::opens_an_empty_ufo_and_creates_its_first_glyph`
    creates and loads a real empty UFO, then creates `A`; checked with
    `cargo test opens_an_empty_ufo_and_creates_its_first_glyph` on 2026-09-05.
    Launching without a path and the New/Open UI are still separate D01 work.
  - [x] **D01.b / R** No-document launch boundary: GPUI launches with
    `Workspace::project: Option<Project>` and renders File → Open when it is
    `None` (`runebender-gpui/src/{launch,workspace}.rs`). Xilem currently
    constructs `Workspace` only through `Workspace::open`, and its live
    `FontModel`/session are read throughout rendering and commands (221
    `session` references on 2026-09-05). Do not represent “no document” with a
    fabricated template font: that gives Save/New and live tools a false source.
    Preferred design: introduce a small application-root state that owns an
    optional `Workspace`, rendering a no-document view and routing only
    document-independent actions until Open/New succeeds. Alternative: make
    every existing workspace field optional, which has a much larger regression
    surface. Resolve this state boundary before implementing the welcome view or
    file dialogs; it is application architecture, not a demonstrated Linebender
    API gap. Implemented with Xilem's pinned `lens` adapter on 2026-09-05:
    no path and a failed path keep the window in a no-document state, while a
    loaded document continues through the existing workspace view. Regression
    coverage: `workspace::tests::{no_path_starts_without_a_document,
    failed_path_keeps_the_window_open_with_an_error}`; headless evidence:
    `RUNEBENDER_SCREENSHOT=... cargo run --quiet` produced an 800×500
    no-document frame. Open/New controls and file dialogs remain separate work.
- [ ] **D02 / M** Open dialog and supported import dispatch: UFO/designspace,
  `.glyphs`, `.glyphspackage`, and compiled-font import as supported by GPUI/core.
  Verify format conversion and destination semantics, not just extension acceptance.
  - [x] **D02.a** Transactional path-open boundary: `AppState::open_path` now
    delegates source dispatch to core through `Workspace::open`, replaces the
    visible workspace only after a successful load, and retains the existing
    document with an error notice on failure. Regression coverage:
    `workspace::tests::failed_open_preserves_the_current_document` (2026-09-05).
    A platform picker that can choose both files and UFO/package directories is
    still required before D02 can be completed.
  - [ ] **D02.b / R** Native picker decision: a trial with `rfd` supplied
    separate file and folder pickers, which is necessary because UFO and Glyphs
    packages are directories, but it added 13 locked packages and failed
    `cargo vet --locked` as unvetted on 2026-09-05. It was not committed: do
    not certify that graph without a human supply-chain review. Evaluate an
    already-vetted platform integration or audit a bounded dependency graph;
    then route its result through D02.a. Open from a loaded document and its
    dirty-document policy remain separate D02/D09 work.
- [ ] **D03 / P** New font destination dialog, cancellation and replacement of the
  current document. Stop implicitly creating Untitled beside the source as the only workflow.
- [ ] **D04 / M** Save As for UFO/designspace with correct master/resource paths;
  Save errors and cancellation must preserve the in-memory document and dirty state.
- [ ] **D05 / M** Export workflow and fontc/build-tool discovery, progress, diagnostics,
  successful output and missing-tool behavior. Reuse core serialization/format logic.
- [ ] **D06 / V** Save/reload multi-master edits, layers, images, features, kerning,
  lib values and proposals without loss; verify disk data after reopening.
  - [x] **D06.a** Multi-master glyph persistence: a new glyph is written through
    core to every source of a temporary two-master designspace and remains present
    after reopening each master. Regression coverage:
    `platform::host::tests::save_reopen_keeps_a_new_glyph_in_every_master`
    (2026-09-05). Layers, images, features, kerning, lib values, and proposals
    still need their own lossless round-trip coverage.
  - [x] **D06.b** Multi-master kerning-group persistence: the inspector's group
    suffix is applied through core to every master, survives save/reopen, and is
    visible after switching to Bold. Regression coverage:
    `platform::host::tests::save_reopen_keeps_kerning_groups_in_every_master`
    (2026-09-05). Layers, images, features, lib values, and proposals remain
    separate D06 work.
- [ ] **D07 / P** Watch external edits without discarding unsaved work; preserve tabs,
  selected master, viewport, filters and text when a reload is accepted. Current
  `reload_from_disk` replaces the workspace and restores only some state.
  - [x] **D07.a** Overview preferences on reload: accepted reloads retain grid/list
    mode, detail mode, collapsed sidebar state, and search scope/case/regex state,
    rebuilding the compiled search expression. Regression coverage extends
    `platform::host::tests::opens_an_empty_ufo_and_creates_its_first_glyph`
    through save and reload (2026-09-05).
  - [x] **D07.b** Tab and viewport restoration: an accepted reload rebuilds each
    tab's session from the fresh core font (discarding stale outlines and undo
    state) while retaining tab order, active tab, tool, viewport and fitted state.
    Regression coverage: `platform::host::tests::reload_keeps_open_tabs_and_their_viewports`
    opens `A` and `B`, gives each a distinct viewport, saves and reloads (2026-09-05).
    Point selection and text-tool context still need their own compatibility and
    undo-safety tests.
  - [x] **D07.c** Designspace reload source and active master: reload now opens
    core's project source rather than the active master's UFO, so it retains all
    masters and restores the selected one. Regression coverage:
    `platform::host::tests::reload_keeps_the_active_master_of_a_designspace`
    creates two temporary UFOs plus a designspace and reloads from Bold
    (2026-09-05). Point selection and text-tool context remain separate D07 work.
- [ ] **D08 / M** Match GPUI config precedence and optional session journal. Reuse a
  shared parser/schema where appropriate; the journal is not autosave or replay.
- [ ] **D09 / V** Test dirty-document New/Open/close/quit behavior against GPUI and
  explicitly resolve any data-loss behavior found in either shell before switching.
  - [x] **D09.a** Dirty New safety: Xilem now retains the active workspace when
    New Font is invoked with unsaved changes, reporting that the user must save or
    discard first. This prevents the prior in-memory replacement loss while the
    New destination/discard UI is still absent. Regression coverage:
    `platform::host::tests::new_font_keeps_a_dirty_document_open` (2026-09-05).
    GPUI currently also replaces its project directly, so a common confirmation
    policy and Open/close/quit coverage remain separate D09 work.
  - [x] **D09.b** Dirty Open safety: `AppState::open_path` now keeps a modified
    workspace intact and reports the required save/discard decision before it
    invokes core's replacement load. Regression coverage:
    `workspace::tests::open_keeps_a_dirty_document_open` opens a real second UFO
    and verifies that the dirty source remains active (2026-09-05). Native picker
    completion and an explicit discard/confirmation affordance remain separate work.

### Phase 2 — commands, focus and shell controls

GPUI reference: `src/actions.rs`, `src/launch.rs`, `src/widgets/`, `src/view/chrome.rs`.

- [ ] **S01 / P** Full menu/shortcut/context-menu coverage including Undo/Redo,
  Open/Save As/export, all path commands, view toggles, and master/sample navigation.
  Enabled/checked state must reflect the current document and selection.
- [ ] **S02 / P** Text field Copy/Paste/Select All/Undo must edit the field; canvas
  shortcuts must edit the document. Ensure native accelerators do not dispatch twice.
- [ ] **S03 / M** Linux in-window menus; verify pointer/keyboard navigation, submenu
  clipping, Escape, outside click and focus restoration. Track Windows separately
  if it is a supported release target; current CI covers macOS/Linux.
- [ ] **S04 / P** Resizable inspector/sidebar/preview regions, constraints at small
  window sizes, collapse/restore behavior and splitter keyboard accessibility.
- [ ] **S05 / V** Numeric and text editing: partial numbers, validation, Enter/Escape,
  blur commit, selection, clipboard, tab order, Unicode and IME. Use upstream widgets
  where they suffice; custom canvas metric fields need their own coverage.
- [ ] **S06 / V** Tabs preserve glyph, tool, selection, viewport and text context;
  rename/delete/master switch/reload/undo cannot leave a stale glyph index.
- [ ] **S07 / V** Match useful theme/chrome behavior and labels; check readable text,
  scroll reachability and layout at multiple sizes/scales using headless snapshots.
  Preserve Xilem's renderer strengths rather than demanding identical antialiasing.

### Phase 3 — editing tools and font data

GPUI reference: `src/edit/{editing,input,inspector}.rs`, `src/edit/commands/`,
`src/view/canvas/editor.rs`, `src/view/panels/{editor_sidebar,glyph_info}.rs`.

- [ ] **E01 / V** Select/hit-test/box-select, point and handle drags, axis constraints,
  snapping, nudge grouping, cancel gesture, pan, wheel zoom and fit. Verify one undo
  transaction per user gesture, including release outside the canvas.
- [ ] **E02 / V** Pen/hyper pen continuity, open/close/cancel, shapes, knife intersections
  and measurement gestures. Match modifiers and selection after each operation.
- [ ] **E03 / P** Complete path command set: conversions, tidy/direction/rounding,
  extrema, start point, point type cycling, reverse, harmonize/balance/optimize,
  round corners, booleans, overlap removal and decomposition. Compare results to core.
- [ ] **E04 / M** Remaining transforms/effects: scale/size, arbitrary supported transforms,
  stroke expansion, offset, curve fit, extrude, roughen, slant, duplicate-repeat and
  both rotation directions. Reuse GPUI's core calls and expose their parameters.
- [ ] **E05 / P** Copy/paste/duplicate selection and glyph-level commands: add/remove,
  duplicate glyph, copy glyph selection text, export glyph SVG, generate missing.
  Match metadata/reference updates, selection and undo semantics.
- [ ] **E06 / P** Glyph inspector: production name, note, export/mark state, metrics
  keys, Unicode, rename, LSB/RSB/advance, coordinates and selection width/height.
  Existing basic fields must continue to work while adding the rest.
- [ ] **E07 / M** Editable font information and advanced metrics/style linking.
  Current `font_info_section` and `font_advanced_section` show key/value rows only.
- [ ] **E08 / P** Anchors: naming, selection, move/delete, composition from anchors,
  dependent recomposition and supported joining checks. Verify component transforms.
- [ ] **E09 / P** Background send/swap/clear plus named backups, brace layers,
  swap/delete layer glyph, mask toggling and baking, with save/undo coverage.
- [ ] **E10 / M** Image placement/removal/transform, trace and SVG import. Confirm
  image resources survive Save As and reload; errors leave the document intact.
- [ ] **E11 / M** Annotations, color palettes/layers, COLRv1 conversion and supported
  gradients; verify editing, rendering, serialization and export separately.
- [ ] **E12 / P** Grid/list selection, keyboard navigation, sort/search scopes/regex,
  coverage filters, missing glyph generation, marks and context actions at large font sizes.
- [ ] **E13 / P** Match overlays: grid dots/lines, handles, segments, bearings, counts,
  sizes/spans, continuity, curvature, reference masters and clean filled Preview tool.
  Xilem has analysis overlays but no `Tool::Preview` equivalent in its tool enum.

### Phase 4 — text, spacing, preview and variation

GPUI reference: `src/edit/text_tool.rs`, `src/view/panels/preview.rs`,
`src/edit/commands/{masters,features}.rs`, `src/edit/inspector.rs`.

- [ ] **T01 / R** Design the text state boundary: authoritative core buffer, widget
  input/composition state, and shared layout/render data for canvas and preview.
  Demonstrate that rebuilds and tab changes preserve text. `TextInputs` explicitly
  documents why `TextBuffer` cannot simply be stored inside a Send/Sync view.
- [ ] **T02 / P** Text selection, navigation, deletion, multiline input, paste, sample
  strings, click/double-click activation and editing a glyph in its line context.
  Match GPUI behavior with Latin, Arabic, combining marks and mixed-direction runs.
- [ ] **T03 / P** IME preedit/commit/cancel and dead keys in the custom canvas;
  ensure tool shortcuts never consume composition. Test native events as well as
  widget tests. Shared shaping does not provide an OS input bridge by itself.
- [ ] **T04 / M** Render the text line in the preview with interpolation/substitution,
  ink-based centering, inversion, blur and resizing. Keep layout semantics consistent
  with the canvas; separately verify the chosen backend's path-filter capabilities.
- [ ] **T05 / P** Kerning pair/group editing, deletion/filtering, side group membership,
  in-context kerning adjustment and writeback from the text buffer, including undo.
- [ ] **T06 / P** Editable feature source, generated-block replacement, compile errors,
  positional/ligature/mark features and preview refresh; preserve hand-authored code.
- [ ] **T07 / P** Master switching, ghost references, axis maps/location, interpolation,
  reinterpolate, synchronize metrics, easing, instance create/update/delete and preview.
- [ ] **T08 / M** Shape-switch rules, brace behavior and smart-component axes/values
  supported by GPUI. Prove compatible/incompatible master behavior and round trips.

### Phase 5 — complete the remaining GPUI surfaces

GPUI reference: `src/edit/{local_ai,nodes,chat,experiments}.rs`,
`src/view/panels/{local_ai,chat}.rs`, `src/platform/live.rs`.

- [ ] **A01 / V** Local model discovery, task list, progress, cancellation, failure,
  proposal preview/install/discard and undo. Models remain external subprocesses.
- [ ] **A02 / P** Nodes open/new/save/run, node parameter editing, canvas interaction,
  progress, errors and proposals. Compare every node/panel action, not screenshots alone.
- [ ] **A03 / M** Chat pane, model choice, send/stream/cancel/clear, live mailbox and
  proposal refresh. Match GPUI process lifetime and error behavior without linking models.
- [ ] **A04 / M** Live experiment cards and their renderer/state integration from core.
- [ ] **A05 / V** Live editing endpoint and asynchronous job results cannot overwrite
  a different document/master after switches; integrate with the same history and caches.
  Follow the [live document and undo contract](XILEM-SWITCH.md#live-document-and-undo-contract).
  Split into reproductions, stable job targeting, save-failure handling and live
  node application. Existing disk-install/reload behavior does not establish undo parity.
  - [x] **A05.a** Save-failure launch guard: `Workspace::save` reports success,
    and node/AI launches stop before creating a worker when a modified document
    cannot save. Regression coverage:
    `platform::host::tests::save_reports_failure_for_an_unwritable_source`
    (2026-09-05). Stable job identity and live result application remain separate
    A05 work.
  - [x] **A05.b** AI master/glyph targeting: `AiJob` captures the launched source,
    master path, and glyph name rather than retaining a grid index. Completion
    rejects a changed master/source or deleted target before adopting a proposal
    (2026-09-05). Document-session identity and revision conflict checks remain
    separate A05 work.
  - [x] **A05.c** Replacement-session guard for AI and nodes: every
    `Workspace::open` gets a process-local document identity, captured by both
    background job types. A completed AI job now rejects the same source after
    it has been reloaded, and a node `core.install` result skips its disk reload
    after a document/master replacement. Regression coverage:
    `edit::{local_ai,nodes}::tests::*replacement_document*` reload real empty UFOs
    before finishing captured jobs; `cargo test --locked -- --test-threads=1` passed
    all 27 tests on 2026-09-05. Per-glyph revision conflict checks and live node
    application with core undo remain separate A05 work.

### Phase 6 — evidence for making Xilem primary

- [ ] **Q01 / V** Differential workflow tests: same fixture + same actions -> equivalent
  norad document, selection and undo results. Normalize only irrelevant serialization differences.
  Include the [state/undo regression scenarios](XILEM-SWITCH.md#regression-scenarios-and-implementation-order),
  including non-outline data; core's current glyph snapshot is not a complete document snapshot.
- [ ] **Q02 / R** Rendering corpus: high contour counts, a single huge outline, holes,
  self-intersections, overlapping components, open contours, cubic/quadratic/hyper
  curves, tiny and extreme zoom, transformed images, color layers and blur. Include
  actual user-reported GPUI failure cases. Never use GPUI output as the sole oracle.
- [ ] **Q03 / R** Record GPU/backend/driver, viewport/DPI and font for timings: frame
  latency (including p95/p99), drag/input responsiveness, scroll, cache invalidation,
  startup, memory and idle CPU. Agree acceptance budgets after measuring the baseline.
- [ ] **Q04 / V** macOS and Linux interaction checks plus keyboard/accessibility checks:
  named controls, focus order, popup dismissal and useful canvas semantics. A canvas
  exposing only `Role::Canvas` does not establish an accessible editing workflow.
- [ ] **Q05 / V** Run repository gates: fmt, clippy, docs, tests and release build under
  warning denial, plus vet/deny when dependencies change. Use CPU snapshots for
  deterministic layout coverage and an explicit native GPU check for rendering.
- [ ] **Q06 / V** Use Xilem for complete real font-editing sessions: open, edit several
  masters, type/space, change metadata/features, export, undo and reopen. Log each
  reason GPUI is still needed and map it to a remaining ID.

### Separate browser gate

- [ ] **W01 / R** Decide whether retiring GPUI includes replacing its published browser
  build. Evaluate the Masonry/winit WASM route versus a distinct DOM frontend;
  neither should be assumed to reuse all custom widgets unchanged.
- [ ] **W02 / M** If required, port build/hosting, bundled fonts, opening/saving,
  clipboard/IME, renderer fallback and web host integration, with browser tests.
  Track service-worker/isolation constraints according to the selected stack.
- [ ] **W03 / V** Compare against the *working* GPUI web capabilities. GPUI's own
  roadmap lists focus/menu/clipboard and workspace-server gaps; fixing those is
  useful extra scope, not evidence that Xilem lacks a working GPUI feature.

Native parity is complete when B–Q are checked with evidence. GPUI retirement is
a separate decision after sustained Xilem use and an explicit resolution of W.
Do not silently exclude chat, experiments, color, or advanced variation to declare
“complete” parity. Record any user-approved scope change in this document.

## Contribution strategy

Start upstream-compatible and use Runebender as a demanding application that helps
improve the stack. Keep application adapters small; develop reusable capabilities
with upstream feedback after proving the need in Runebender. Avoid inventing a
parallel component framework by default. Suitable bounded contributions:

1. A minimized compile-growth case and an ergonomic fix, informed by
   [Xilem #1777](https://github.com/linebender/xilem/issues/1777).
2. Scoped commands/shortcuts with focused-text precedence and menu state.
3. Xilem views for popup/layer lifecycle and result delivery, with focus/accessibility tests.
4. Desktop service integration: window handles/lifecycle, dialogs and event delivery.
5. Canvas input/IME examples, testing helpers and renderer regression fixtures.

These span Xilem's views, Masonry's widgets, masonry_winit's shell and imaging/Vello.
They should not all be forced into a Xilem fork. Consult the current widget inventory
before adding controls: [tracking issue #1710](https://github.com/linebender/xilem/issues/1710).
The pinned source already has Split, text inputs, portals and Masonry layers.

Becoming a substantial contributor is compatible with this plan. Taking over
maintenance would require agreement with existing maintainers; this audit found
no reason to assume the project is abandoned. If a fork becomes necessary, give
each patch a motivating blocker/test, an upstream issue, and a condition for
dropping it. Keep a rebaseable patch series rather than combining the entire editor
port with a toolkit redesign. No upstream issue or message was posted for this audit.

Linebender's [LLM contribution policy](https://linebender.org/wiki/llm-policy/)
matters to the later contribution workflow: disclose applicable assistance,
including implementation based on generated plans; review the work yourself.
It disallows end-to-end agent-generated PRs and generated PR descriptions, and
asks contributors not to post generated analyses to GitHub/Zulip. This checklist
is a local project handoff, not text to post upstream. Write upstream proposals
yourself from reproduced findings and follow the policy current at submission.

## Sources and freshness

- [Linebender homepage](https://linebender.org/) explains the ecosystem and the
  historical Runebender/Druid connection. Its project catalogue explicitly dates
  itself to 2024-12-07, so do not use it alone for present capability claims.
- [Linebender in 2026 Q1](https://linebender.org/blog/tmil-25/) describes Masonry's
  move to imaging, broader backend support, new widgets and IME integration work.
  The local pinned source corroborates imaging and the Split view.
- [Xilem repository](https://github.com/linebender/xilem) explains the distinction
  between the reactive UI layer and Masonry and links the contributor community.
- [Vello repository](https://github.com/linebender/vello) distinguishes classic,
  CPU and Hybrid renderers; its production direction includes Hybrid.
- [Archived long-term roadmap](https://linebender.org/wiki/long-term-roadmap/)
  explicitly warns that it becomes stale. It is context, not today's backlog.

Recheck upstream issues and APIs against the dependency revision used for each
implementation. Do not mistake an old gap document, an issue title, or a method
name for a reproduced framework blocker.
