# Complete the Babelfont editing-model migration

Audit date: 2026-09-18.
Audited implementation: `6350cf3`, including the pipeline implementation at `5cbf51b`.
Status: **IN PROGRESS — M00–M03 complete; M04 and later cutovers active**.
This document is the implementation checklist and handoff for the active integration task and its bounded parallel lanes.
The active integration checkout is `/Users/eli/.codex/worktrees/f236/runebender-xilem` on branch `codex/babelfont-integration`.
The earlier continuation worktree and branch remain preserved for review.

## Product goal and scope

Build the best possible font editor for Eli's tastes and type-design work, following [Design](../DESIGN.md).
Counterpunch and Fontra are references to evaluate, not specifications to copy.
This work completes the Babelfont-backed editing model while retaining exact, first-class UFO/Designspace persistence.
It preserves the existing native editor, shared browser editor, headless tools, live editing, proposals and experimental font versions.
It does not redesign the interface or require new competitor features.

The previous phase delivered live variable compilation and source-authoring features, but most editing still mutates Norad structures and reconciles them into Babelfont.
Passing those existing tests does not mean this migration is complete.

## Definition of complete

All of the following must be true before changing the status to **COMPLETE**.

- The Project has one authoritative editable document backed by Babelfont, with explicitly owned precision and metadata extensions where Babelfont cannot represent the source faithfully.
- Ordinary geometry, metadata, interpolation, history, proposal and experimental-version operations use that document directly.
- A point move, metadata edit, undo or compile does not materialize or reconcile an entire Norad font.
- There are no persistent editable `norad::Font` or `norad::Glyph` mirrors in Project, source caches, sessions, histories or experimental versions.
- Norad use is confined to explicit source-format/serialization adapters and boundary fixtures; public editing APIs and UI state do not expose Norad types, including aliases that hide them.
- Known editable values have one owner; opaque source payloads preserve unsupported data but do not supply a second mutable value for the same field.
- The same document operations serve native, browser, CLI, live-agent and Nodes paths.
- Every already-supported source field and workflow passes the preservation and behavioral checks below.
- Mutable compatibility guards and the legacy Master editing model are removed, not merely deprecated or unused by the main canvas.
- All required milestones have implementation commits and executed acceptance evidence, and the final architecture audit and clean-checkout checks pass.

Norad remains a useful UFO/Designspace codec.
Removing the dependency, exposing Babelfont types throughout the application, or replacing every Kurbo operation with a Babelfont helper is not a completion requirement.
Exact-value extensions and format preservation are part of the document contract, not an excuse to keep a second complete editable font.

## Audited ownership and callers

The current source tree has 74 Rust files mentioning `norad`, including comments and test fixtures.
That raw count is an inventory aid, not a percentage-complete measurement or a target to drive to zero.
The audit followed the runtime paths below rather than classifying every mention as migration work.

| Area | Current dependency | Required destination |
|---|---|---|
| `document/variable.rs` | Babelfont geometry, `VariableGlyph.layers: BTreeMap<LayerId, norad::Glyph>`, glyph-free Norad templates, scoped reconciliation | One document plus precise field/metadata extensions; no full glyph mirrors |
| `document/babelfont.rs` | UFO-to-Babelfont geometry and index-based restoration of original contour/point/component/anchor metadata | Boundary conversion keyed by preserved identity and explicit edit semantics |
| `document/project.rs`, `source.rs` | `Vec<Master>`, each with a full Norad font, editable methods, history and paint caches | Source metadata, immutable derived render caches and document commands |
| `document/sources.rs` | Structural snapshots clone Masters, VariableData and a Norad Designspace document | Canonical structural transactions with stable source/layer references |
| `outline/glyph_ops.rs`, `point_ops.rs`, `segment_ops.rs` | Editing on Norad glyphs/contours and index-based point addresses | Geometry operations over document layers, preserving selection and metadata |
| Other `outline/` modules | Cleanup, knife, booleans, effects, embolden, conversion, drawing and component resolution use Norad | Same document geometry boundary and existing Kurbo algorithms |
| `outline/path/hyper_model.rs`, `formats/metaballs.rs`, `formats/lib_keys.rs` | Hyperbezier adapters, live metaballs, masks and HOI data depend on UFO payloads | Explicit editable extensions with format encoding at the boundary |
| `document/history.rs`, `outline/glyph_ops.rs::GlyphSnapshot` | Norad-based full glyph snapshots | Document snapshots/deltas covering geometry and exact metadata together |
| `application/editor/session.rs` | Mutable Norad glyph, component contours and pending Norad history records | A document edit draft/transaction plus selection and viewport state |
| `application/workspace.rs`, `font_model.rs`, canvas and panels | Norad clipboard, groups/kerning snapshots, mutable font access, paint-time type checks | Document-facing queries/commands and presentation DTOs |
| `document/font_ops.rs`, `composites.rs`, `compose.rs`, metadata helpers | Rename, kerning, Unicode, composite alignment and generation mutate Norad | Canonical operations with explicit dependent-glyph invalidation |
| `document/interpolation.rs`, `project.rs`, `var_model.rs`, `axis.rs` | Numeric backend is already Babelfont/fontdrasil; extraction/results still use Norad glyphs | Interpolation directly over canonical layers and exact values |
| `document/compile.rs`, `compile_metadata.rs` | Geometry cloned from Babelfont; masters, names, metrics, kerning, groups and features rebuilt from Norad projections | Immutable compiler snapshot from the authoritative document only |
| `document/experiments.rs`, `nodes_live.rs` | Full Norad baselines/working Masters and source-index references | Isolated canonical document versions, stable IDs and guarded apply |
| `document/proposal.rs`, `edit_batch.rs`, `live.rs` | Proposals and live edits operate on Norad; revision hashes serialize GLIF | Canonical operations with stable external revision/proposal contracts |
| `application/cli.rs`, `document/nodes_run.rs`, `analysis/` | Standalone Master editing, direct Norad loads and Norad analysis inputs | Source loading at the boundary, then shared Project operations |
| `text/features.rs`, `text/buffer/mod.rs`, `text/shape.rs` | Norad inventory, kerning and feature-generation helpers remain | Document queries and compiled-font shaping with no duplicate kerning |
| `formats/`, `document/font_memory.rs`, new-font creation, browser bootstrap | Mixed codecs and live model construction; browser bootstrap installs a Master | Common document constructors; Norad stays inside explicit codecs |

### Important hazards found

1. The pinned Babelfont Layer width is `f32`, master kerning is `i16`, and some master metrics are integers.
   `tests/babelfont_contract.rs` already demonstrates width and kerning loss.
   Preserve editable `f64` values explicitly and quantize only for binary compilation.
2. Babelfont stores a decomposed component transform, while UFO stores six affine coefficients.
   Preserve exact matrices and define which representation wins after an edit; repeated conversions must not drift.
3. `project_layer` currently restores metadata by array position.
   Direct Babelfont insert/delete/reorder operations would make that unsafe without identity-aware mapping.
   A UFO identifier or lib must never silently move to a different point, contour, component or anchor.
4. `Project::edit_layer` still accepts `&mut norad::Glyph` and installs the result through source guards.
   Routing tools through that API unchanged would not finish the migration.
5. Undo ownership is split among source history, auxiliary-layer history, session pending records, overview history and application metadata snapshots.
   Preserve their user-visible grouping and conflict protections while changing storage.
6. Experimental versions retain source indices and complete Norad fonts.
   The current source-removal/reorder guard exists for this reason and cannot simply be deleted.
7. Runtime mutations are not limited to the GUI: CLI, live-agent operations, Nodes and proposal installation can bypass an incomplete canvas-only migration.
8. Some helpers under `formats/` perform live editing, such as mask baking and metaball metadata updates.
   Allowing all of `formats/` to depend on Norad would hide unfinished work.
9. The in-memory UFO loader currently reads a limited subset and omits additional layers/images/data.
   Preserve the existing documented browser boundary and reject unsupported input explicitly; do not introduce silent loss while consolidating constructors.
10. Metadata-dependent compilation must be invalidated when features, groups, kerning, categories, axes, instances or anchors change, even when point coordinates do not.

## Execution and evidence rules

Work in the assigned isolated lane worktree and return exact reviewed commits to the integration checkout.
The integration baseline is `3f209776d35dbc7e88e35facb3a48e9f7edd68e0`; do not start again from an older main branch.
Read `AGENTS.md`, `ARCHITECTURE.md`, `DESIGN.md`, the [format decision](variable-project-decision.md), and this checklist before changing the model.
The worker must update `AGENTS.md` and architecture guidance when its changes make their old compatibility rules obsolete.

Each assigned lane retains its history and worktree between runs.
Choose the first unblocked item inside that lane's explicit ownership and finish a coherent substep with focused verification.
Continue a running build using its session rather than launching another copy.
The integration lane reviews and incorporates exact returned commits; lanes do not merge one another or create additional worktrees without a new explicit assignment.

Keep each milestone's evidence in `docs/babelfont-migration-progress.md` with:

- Milestone/substep, status, implementation commit, affected paths and any remaining callers.
- Exact verification commands, results and durable artifact/log paths where relevant.
- Blockers and the next concrete action; distinguish application failures from environment permissions.
- Any changed ownership decision and why it still satisfies this document's completion criteria.

Mark a checkbox complete only after its acceptance checks execute successfully.
Do not mark an item complete because a wrapper compiles, a Norad conversion hides beneath a new name, or a test was removed/ignored.
Do not weaken the acceptance criteria, expand the permitted Norad boundary, or turn migration items into permanent exclusions to finish the list.
Split large substeps further in the progress log without silently changing scope.
Routine decisions within this design are authorized; escalate a real unresolved product choice or preservation blocker with a concrete example.

Preserve unrelated work and make coherent local commits using explicit paths.
Do not merge into main, push, publish, delete other worktrees or edit real font sources.
Use generated fixtures or temporary copies of Virtua Grotesk, honoring any font-specific protection rules.
Keep visual checks headless and preserve the existing UI unless an API change requires adaptation.
Keep native and browser lockfiles reproducible; never commit local path patches.

If an item is blocked, record the cause and work on another dependency-ready item.
If all remaining items require unavailable input or external state, pause the heartbeat and report the actionable blocker instead of repeating costly no-op runs.
On completion, record the final commit and evidence, pause the heartbeat, and leave the task open for review.
The automation must not claim completion merely because it reached a time or usage limit.

### Parallel lane ownership

The user authorized parallel scheduled lanes on 2026-09-19 without changing any completion or preservation requirement.
The integration lane owns `project.rs`, `variable.rs`, `babelfont.rs`, `source.rs`, document parent wiring, narrow cross-lane model hooks, shared architecture/checklist/progress documents and cross-lane integration tests.
The M05 history lane, task `01a0ba27-44fc-7243-a672-aacc3e5b05de`, owns `history.rs`, `sources.rs`, optional new history modules, `tests/canonical_history.rs` and `docs/babelfont-history-progress.md`.
The M06 application lane, task `01a0ba61-b1cf-7541-853c-6558bae092d5`, owns all of `src/application/`, including editor sessions and commands, `FontModel`, workspace, local AI, Nodes, CLI adapters and view/platform synchronization.
The M07 metadata lane, task `01a0ba27-8ba1-71c0-bc43-69eae82b773a`, owns `document/font_ops.rs`, `document/model/glyph_metadata.rs`, `document/compose.rs`, `document/composites.rs`, `text/features.rs`, dedicated metadata tests and `docs/babelfont-metadata-progress.md`.
M07 owns the canonical document algorithms in those files; M06 retains their application callers, and the integration lane supplies required Project-level whole-font transactions.
The M08/M09 pipeline lane, task `01a0ba2a-5670-7451-b05a-bd71293b2229`, owns `interpolation.rs`, `compile.rs`, `compile_metadata.rs`, dedicated pipeline tests and `docs/babelfont-pipeline-progress.md`.
The M10 lane, task `01a0ba89-8a9f-7c81-b89c-f37805ff39c6`, owns `proposal.rs`, `edit_batch.rs`, `experiments.rs`, `live.rs`, `nodes_live.rs`, dedicated tests and its lane progress document.
M10 may add a narrow explicit boundary codec when the external proposal contract requires one, but it does not edit shared Project/VariableData files or application-owned callers.
M10 is specifically authorized to add `document/babelfont/proposal_edit.rs` and the single `mod proposal_edit;` declaration needed to compile it; no other shared-parent edits are transferred.
The M11 document/headless lane reuses task `01a0ba27-44fc-7243-a672-aacc3e5b05de` and owns `document/nodes_run.rs`, `analysis/`, dedicated tests and `docs/babelfont-headless-progress.md`.
It retains `history.rs` and `sources.rs` only for review or integration corrections to its completed M05 cutover; M06 continues to own all application CLI adapters.
M11 is narrowly authorized to add `document/variable/source_builder.rs`, optional `document/project/source_builder.rs`, their parent declarations and the existing `sources.rs` caller needed to replace the legacy new-source clone path.
The integration lane retains M04 special-source extensions and alone integrates returned commits.
M07 is narrowly authorized to add `document/project/glyph_transactions.rs`, `document/variable/glyph_transactions.rs` and `document/babelfont/glyph_transactions.rs`, their parent module declarations, and the directly required stable `GlyphId` field, import and initialization sites in `variable.rs`.
That exception covers only the atomic whole-glyph lifecycle and its canonical constructors, identity propagation and tests.
Worker lanes request narrow shared-model APIs from this lane and return exact reviewed commits for integration.
No lane edits another lane's owned files, merges into main, pushes or weakens the acceptance criteria.
Shared Clippy, documentation and broader suites run at coherent integration checkpoints after focused lane checks.

### Current critical path

| Work | Current state | Next concrete dependency |
|---|---|---|
| M01 glyph identity | Complete through `15f065b` and `b2498c8`: stable glyph identity now joins the accepted source, layer, contour, point, component and anchor identities. | Keep the stable identity APIs intact while later caller cutovers remove compatibility projections. |
| M04 canonical topology and cleanup | Direct curve conversion and handle cleanup are integrated through `2b15baa`; the independent quadratic-chain correction passes. | Finish special editable-source preservation and the remaining selection/metadata acceptance before checking M04 complete. |
| M05 history and structural replay | Canonical layer/source snapshots and Project-owned history foundations exist; `5e9d2f1` adds sparse-source registration, instance removal and stable structural undo/redo. | Retire the compatibility history stores after the Session callers move. |
| M06 application cutover | Whole-glyph/component edits, composition publication and CLI glyph inspection use canonical Project APIs; `7fd409b` moves point selection, clipboard paste/history, reload rebasing and Dimensions onto stable canonical identities and queries, while `98874fe` moves component/anchor identities and pointer gestures into canonical transactions. | Use the detached compatibility bridge in `ed4ad11` to remove Session's persistent UFO glyph and mixed legacy history, then move the remaining canvas/panel readers. |
| M07 metadata | Composition, alignment, feature planning, smart-component codecs and atomic whole-glyph lifecycle are integrated; `5cfe3a9` also stabilizes component identity on the first alignment edit. | Finish the remaining metadata callers and validate source save/reload without unrelated changes. |
| M08/M09 pipeline | Canonical interpolation, source construction, compiler metadata, Designspace structure, typed special rendering and typed HOI ownership are integrated; `352e513` also prevents cleared canonical compiler metadata from inheriting stale snapshot values. | Finish export acceptance and remaining application callers. |
| M10 proposals and versions | Composition publication and selective proposal/version installation are direct guarded canonical transactions; `94eb639` moves root live glyph inspection to canonical Project state. | Retain the isolated experiment snapshot boundary until M13 and remove superseded compatibility code after application callers land. |
| M11 headless and analysis | `278d173`, `1b5234e` and `9c51a7b` move headless proof, glyph inspection and curve input to the complete typed renderer; `efa4362` moves the CLI caller. | Move the remaining Session and panel analysis callers; direct Norad functions then become M13 boundary-removal candidates. |
| M12 adapters and constructors | Shared canonical new-font and in-memory UFO constructors are integrated; `fb629a0` moves the 863-glyph browser bootstrap off app-side Norad/Master replacement, `f33e193` stages complete UFO/Designspace saves before publication while preserving exact custom GLIF paths and opaque filesystem payloads, and the reviewed source-format allowlist is enforced by whole-UFO and Designspace regressions. | Move the remaining native, CLI and imported-format callers and include external feature dependencies in watched-source conflict detection. |
| M13/M14 removal and final proof | `SourceFormatData` replaces the long-lived full UFO templates, while the remaining Master shell, mutable guards and detached layer bridge are still active. | Finish caller cutovers, delete the residual compatibility state, then reserve one coherent final native/browser/preservation/clean-checkout proof. |

Today's target is completion without changing the definition of complete.
The immediate feasibility risk is the number of production Session, proposal/version, headless and constructor callers still using Norad, followed by the required M13 removal and M14 clean-checkout proof.
Focused checks belong with each coherent change; unchanged broad gates are deferred until an integration boundary or the final proof.

## Ordered implementation checklist

Completed boxes have executed evidence in [the progress log](babelfont-migration-progress.md).
M00 establishes the baseline; it does not change document ownership or complete any M01–M14 implementation work.
Dependencies identify the minimum prerequisite, not permission for multiple concurrent writers.

### M00 — Establish the continuation and measurable baseline

Depends on: none.
Start in: this document, `tests/variable_project.rs`, `tests/variable_compile.rs`, `tests/babelfont_contract.rs`, `web/README.md`.

- [x] Verify the isolated task checkout includes the committed migration and this plan; record its branch, baseline and clean/owned-dirty state.
- [x] Create the progress log and a reviewed inventory that separates production Norad use from boundary codecs, test fixtures, comments and obsolete APIs.
- [x] Record the existing 569-passing/4-ignored validation as historical evidence; run the three focused suites to establish the worker's environment.
- [x] Record the exact existing behavior of drag grouping, auxiliary history, metadata history, source undo, proposal apply and experimental-version conflicts before changing ownership.

Acceptance: commands run successfully in the continuation checkout, and every runtime family in the inventory has an assigned milestone.

### M01 — Define exact values, identities and preservation ownership

Depends on: M00.
Start in: `document/variable.rs`, `document/babelfont.rs`, `document/model/entity_id.rs`, `formats/lib_keys.rs`, the pinned Babelfont types and contract tests.

- [x] Write the field-ownership contract for geometry, width/height, raw affine matrices, fractional kerning/metrics, Unicode, names, categories, guides, images, notes, libs, source/instance metadata and format-specific extensions.
- [x] Introduce typed exact-value/metadata extensions without complete Norad glyph/font copies; maintain one authoritative editable value for each field.
- [x] Implement the defined identity and mapping rules for sources, layers, glyphs, contours, points, components and anchors, including rename, copy/paste, deletion, reorder and undo.
- [x] Replace index-based preservation matching with the identity-aware design before enabling direct topology edits.
- [x] Add round-trip fixtures for two widths that narrow to the same `f32`, fractional kerning, six-coefficient transforms, identifiers, per-object libs, guides, images and unknown metadata.

Acceptance: no-op import/save and edit/undo/save preserve exact supported values; inserting or reordering an object cannot attach another object's metadata to it.
Normal compiler quantization is tested separately from editable-source fidelity.

### M02 — Add direct document queries and transactional mutations

Depends on: M01.
Start in: `document/project.rs`, `variable.rs`, `source.rs`, `sources.rs`.

- [x] Add document-facing glyph/layer/source readers and edit drafts/transactions backed by Babelfont plus the M01 extensions.
- [x] Make geometry, metadata and structural edits atomic with accurate changed/no-change results and revision invalidation.
- [x] Provide explicit change information for affected layers, dependent components, source metadata and compilation; keep paint caches derived and read-only.
- [x] Introduce canonical clone/snapshot support needed by history and experimental versions, without cloning a parallel Norad document.
- [x] Keep temporary compatibility entry points clearly isolated and tracked until their callers are migrated; do not add new ones.

Acceptance: an unsaved direct document edit immediately affects queries, interpolation inputs and compiler snapshots; failed and no-op transactions leave contents, revisions and history unchanged.

### M03 — Migrate geometry queries and ordinary point operations

Depends on: M02.
Start in: `outline/glyph_paths.rs`, `glyph_ops.rs`, `point_ops.rs`, `segment_ops.rs`, `analysis/curve.rs`, `analysis/dimensions.rs`.

- [x] Move path conversion, bounds, point/anchor extraction, hit-testing inputs and component resolution onto document geometry.
- [x] Port point movement, selection transforms, smoothing, sidebearing shifts and segment conversion without per-operation UFO materialization.
- [x] Preserve open contours, cyclic closed contours, quadratic implied points, empty glyphs and mixed path/component ordering.
- [x] Port geometry analysis inputs and verify recursive component cycles/missing references still fail explicitly.

Acceptance: existing geometry tests use the canonical model, and targeted old/new fixture comparisons establish equivalent behavior where the intended algorithm has not changed.

### M04 — Migrate topology edits and special outline tools

Depends on: M03.
Start in: `outline/cleanup.rs`, `knife.rs`, `effects.rs`, `embolden.rs`, `convert.rs`, `drawing.rs`, `component_ops.rs`, `path/hyper_model.rs`, `metaballs.rs` and related format helpers.

- [x] Port pen creation/closure, point insertion/deletion, contour reversal, split/join, copy/paste and shape creation.
- [ ] Port booleans, overlap removal, knife, cleanup, fit/simplify, embolden and component decomposition with explicit metadata behavior when topology is replaced.
- [ ] Adapt hyperbezier conversion without promoting its legacy intermediate Glyph into another live font model.
- [ ] Preserve live metaball groups, masks and HOI data as editable extensions; retain source data until the existing explicit conversion/bake commands run.
- [ ] Test selection remapping and identifier/lib preservation for insert, delete, reorder, duplicate and undo operations.

Acceptance: all existing tools remain callable and undoable, including auxiliary layers; no operation silently destroys special editable source data or preserves identifiers on the wrong objects.

### M05 — Migrate history and structural snapshots

Depends on: M02–M04.
Start in: `document/history.rs`, `outline/glyph_ops.rs::GlyphSnapshot`, `document/sources.rs`, session `HistoryOp`, workspace metadata/overview history.

- [ ] Replace Norad snapshots with canonical document snapshots/deltas that include exact-value and metadata extensions.
- [ ] Preserve drag coalescing, no-op history removal, redo invalidation, per-glyph and auxiliary-layer history, and multi-glyph command boundaries.
- [ ] Make source/layer structural history use stable identities and canonical data only.
- [ ] Retain guards against undo overwriting later unrelated edits; explicitly test removal/restore followed by undo of an older edit.
- [ ] Preserve metadata edits across source reorder and reconcile selection/active-layer state after undo/redo.

Acceptance: undo/redo restores geometry, exact values, metadata, revision-dependent preview and selection behavior; a rejected replay changes nothing.

### M06 — Migrate editor sessions, application commands and render inputs

Depends on: M03–M05.
Start in: `application/editor/session.rs`, `commands.rs`, `font_model.rs`, `workspace.rs`, `view/canvas/editor.rs`, panels and render helpers.

- [ ] Replace Session's Norad glyph/component contours and pending records with the canonical edit draft and history API.
- [ ] Replace FontModel's mutable font/master access with explicit Project operations and derived query/cache data.
- [ ] Migrate clipboard, source switching, tab parking/resume, overview edits, component/anchor tools and background-layer commands.
- [ ] Make views and panels read presentation/document data instead of Norad point types and mutable font structures.
- [ ] Verify native headless Gray/Light scenes and real browser pointer dragging, undo/redo, source switching and text interactions.

Acceptance: UI edit paths mutate the canonical document directly and do not synchronize a complete source font after each input event.
The shared browser and native widget tree keep the existing interaction contract.

### M07 — Migrate font-wide and glyph metadata operations

Depends on: M02, M05; integrate application callers after M06.
Start in: `document/font_ops.rs`, `model/glyph_metadata.rs`, `compose.rs`, `composites.rs`, `application/editor/inspector.rs`, `text/features.rs`, `formats/lib_keys.rs`, `metrics_keys.rs` and `metaballs.rs`.

- [ ] Move names, metrics, glyph order, Unicode, export/category flags, notes, colors, guides, images and editable custom data into canonical ownership.
- [ ] Port fractional kerning and group operations, metrics formulas, glyph rename/add/duplicate/remove and dependent component updates.
- [ ] Port composite alignment, composition, effective-anchor queries and user-requested feature generation.
- [ ] Keep default-source feature ownership and per-source feature preservation; draft checking must remain non-mutating.
- [ ] Split live behavior from serialization helpers currently under `formats/`; each persisted key retains one constant, reader and writer at its boundary.

Acceptance: metadata changes immediately affect the intended UI/preview, undo correctly and survive source save/reload without precision loss or unrelated field changes.

### M08 — Migrate interpolation and source structure

Depends on: M02, M05, M07.
Start in: `document/interpolation.rs`, `project.rs`, `sources.rs`, `axis.rs`, `var_model.rs`.

- [ ] Read/interpolate canonical layer geometry, exact advances, anchors and affine coefficients without constructing Norad glyphs.
- [ ] Keep the existing fontdrasil numeric backend and mapped user/design/normalized coordinate contract unless evidence requires a change.
- [ ] Move axes, source locations, sparse participation, instances and rule data into canonical document ownership; keep Designspace serialization separate.
- [ ] Port interpolated-source creation, rename/relocation, source reorder/removal, brace promotion and auxiliary-layer operations to canonical transactions.
- [ ] Verify sparse and auxiliary-only glyphs, incompatible sources, default-source protection, mapped two-axis locations and independent active-source selection.

Acceptance: the variable-project fixture and source-authoring workflows pass without Norad interpolation inputs or a mutable Designspace document serving as the live editing model.

### M09 — Finish the direct compilation, shaping and export boundary

Depends on: M07–M08.
Start in: `document/compile.rs`, `compile_metadata.rs`, `text/buffer/`, `text/shape.rs`, `application/editor/tools/text.rs`, `platform/export.rs`.

- [ ] Build compiler snapshots from canonical geometry, masters, names, metrics, groups, kerning, features, axes, instances and rules, without reading source-font projections.
- [ ] Keep exact values in the document and perform checked quantization in the immutable compiler snapshot.
- [ ] Migrate inventory/kerning/feature helpers and retain HarfRust/Skrifa coordinate agreement with no duplicate kerning.
- [ ] Exercise invalidation for every compile-relevant metadata edit, stale worker results, undo/redo and slider-only revision reuse.
- [ ] Preserve unsaved native export, browser downloads, CLI overwrite refusal and explicit unsupported-compiler errors.

Acceptance: all six existing variable-compiler cases plus direct-mutation and metadata-invalidation regressions pass; export-before/after-unsaved-edit still changes the binary outline.
Whole-font compilation and synchronous browser compilation are existing performance boundaries, not reasons to rebuild the document model again.

### M10 — Migrate proposals, live tools and experimental versions

Depends on: M05, M07–M09.
Start in: `document/proposal.rs`, `edit_batch.rs`, `experiments.rs`, `live.rs`, `nodes_live.rs`, `application/editor/tools/local_ai.rs`, `nodes.rs`.

- [ ] Run revision-checked batches and proposal creation/install/discard over canonical document layers.
- [ ] Preserve the external UFO proposal-layer contract and the meaning of revision tokens; any serialization needed for compatibility belongs in an explicit boundary adapter.
- [ ] Replace experimental Norad baselines/working Masters with isolated canonical versions and stable SourceId/LayerId references.
- [ ] Preserve atomic selective apply, conflict detection, unrelated-root edits, undo-apply and session-only version semantics.
- [ ] Audit Nodes connections and source reorder/removal during live versions; remove source-index restrictions only when stable-identity behavior is proved.

Acceptance: proposal/model results cannot bypass revision checks or mutate the root implicitly; source reorder never redirects a proposal or version to another source.

### M11 — Migrate headless commands, analysis and file-based workflows

Depends on: M03–M05, M07–M10.
Start in: `application/cli.rs`, `analysis/`, `document/nodes_run.rs`, `document/source.rs`, agent adapters.

- [ ] Replace `open_master` and standalone mutable Master workflows with Project plus explicit source selection.
- [ ] Port read/analysis/proof inputs to document queries, keeping JSON schemas, exit codes, revision semantics and layer selection compatible.
- [ ] Make file-based Nodes editing/import/proposal actions call the same canonical operations as the live editor.
- [ ] Restrict direct Norad load/save to source adapters; CLI glue must not own a second set of editing algorithms.
- [ ] Test equivalent GUI/document/headless operations on the same disposable fixture, including invalid batches and multi-source ambiguity.

Acceptance: no production headless editing path remains on the legacy Master model, and existing CLI/agent/Nodes tests continue to pass.

### M12 — Consolidate source adapters and document construction

Depends on: M01, M07–M11.
Start in: `formats/`, `document/font_memory.rs`, `new_font.rs`, `project.rs`, `application/browser.rs`, native load/save/reload code.

- [ ] Create explicit import/export/preservation boundaries that translate supported UFO/Designspace data into and out of the canonical document.
- [ ] Move new-font creation and browser bootstrap onto common document constructors; avoid installing/replacing a legacy Master.
- [ ] Preserve format-specific unknown payloads, layer order and paths, images/data, source destinations, feature includes and existing unsupported-format errors.
- [ ] Keep imported Glyphs/binary/Python Babelfont workflows within their documented guarantees; do not claim new lossless support for them or Rust Babelfont JSON.
- [ ] Test native save, Save As, reload, watched-source conflict behavior and in-memory construction using temporary sources.

Acceptance: supported sources round-trip through the new document, and unsupported input cannot be silently dropped by a constructor or save path.
Source removal still leaves the original UFO on disk.

### M13 — Remove compatibility state and enforce the architecture

Depends on: M00–M12.
Start in: `document/variable.rs`, `source.rs`, `project.rs`, `application/font_model.rs`, public module exports and the runtime inventory.

- [ ] Remove `SourceEdit`, `SourceFontEdit`, `SourcesEdit`, `active_font_mut`, `edit_source(s)` compatibility mutation paths and the full Norad-backed Master editing model.
- [ ] Remove full Norad glyph mirrors/templates used as editable state, reconciliation scans and obsolete helper/constructor overloads, including the transitional `CanonicalLayerTransaction` detached-glyph bridge after every residual algorithm has a direct canonical operation.
- [ ] Keep paint caches derived from document revisions and verify a single-glyph edit does not clone or compare every source font.
- [ ] Add a focused architecture check with a reviewed per-module boundary allowlist; it must catch prohibited Norad imports, aliases, fields, mutable accessors and hidden round-trip edit wrappers.
- [ ] Update AGENTS, ARCHITECTURE, module headers, the decision record, limitations and changelog to describe the resulting ownership accurately.

Acceptance: a fresh runtime inventory has zero unexplained production Norad dependencies outside boundary codecs, and ordinary editing never traverses a Norad conversion path.
A documented allowlist cannot exempt a live editor, geometry, history, interpolation or command module merely to pass the check.

### M14 — Final correctness, performance and clean-checkout proof

Depends on: M13.

- [ ] Run the complete native gate below on the final implementation and record actual counts, ignored cases and any dependency notices.
- [ ] Run a fresh optimized browser build, warning-denied browser lint and the full interaction/export matrix at 1×, 2× and 1.25×.
- [ ] Inspect Gray and Light native/browser captures covering ordinary editing, variable proof text, sources and auxiliary layers; certify only what these checks exercise.
- [ ] Run a disposable Virtua Grotesk edit → undo/redo → variable proof → export → source save → reopen workflow; compare supported data and confirm originals were untouched.
- [ ] Measure edit cost and memory on a multi-source fixture, reporting methodology and values; verify removal of whole-source reconciliation rather than claiming improvement from intuition.
- [ ] Clone or archive the final committed tree into a temporary clean checkout, run the documented native and browser gates, and resolve any dependence on local untracked files or path patches.
- [ ] Review the final diff and the architecture inventory in a separate review pass; fix findings and rerun only the checks those fixes invalidate.
- [ ] Verify every milestone has evidence, record the final commit and remaining product limitations, mark COMPLETE, and pause the scheduled heartbeat.

Acceptance: all definition-of-complete criteria hold together at the final recorded commit.
Local macOS/browser proof is not a claim of Linux/Windows/native IME/accessibility certification; report unavailable platform coverage honestly.
The migration can finish without adding unrelated features, but it cannot finish with a compatibility editing model still in use.

## Verification commands and coverage

Use `RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources` read-only when available, otherwise supply a temporary fixture directory.
The prior full-suite count was 569 passed and four ignored; it is a baseline, not the expected count after adding migration coverage.
Two ignored tests require local models and two require a hard-coded adjacent font checkout; do not count them as executed coverage.

Focused baseline and regression suites:

```sh
cargo test --locked --test babelfont_contract --test variable_project --test variable_compile
```

During a milestone, run affected unit/integration suites and native/browser compilation appropriate to the changed API.
Run broader gates at integration boundaries and the complete final gate once the model and callers are migrated.
Do not repeatedly run expensive optimized builds when no intervening change justifies them.

Final native gate:

```sh
cargo fmt --all --check
bash .github/copyright.sh
git diff --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo doc --workspace --no-deps --locked
cargo test --workspace --locked -- --test-threads=1
cargo build --workspace --release --locked
cargo deny --locked check advisories
```

Follow `web/README.md` for the separate browser workspace's build, Clippy and `web/quality.cjs` checks.
Use the existing fixture generator and assertions in `tests/variable_project.rs` rather than modifying real fonts.
Native Unix-socket tests may require approved execution outside the filesystem sandbox; an IPC permission error is not an application regression or a passing test.

The final evidence matrix must include geometry/topology, precision, object metadata, history, source/layer structure, interpolation, features/kerning/anchors, compiler invalidation, proposals/experiments, external edit conflicts, headless tools and browser parity.

## Separate product backlog

Subset compilation, browser worker hosting, new axis-authoring/import-source UI, Fontra-style glyph-local axes, VARC, exhaustive new format support and new shaping features remain separate product work.
Do not silently add them to this migration, remove existing support for them, or use them to redefine completion.
If migration exposes a defect in an already-supported behavior, repair and test it in the corresponding milestone.
