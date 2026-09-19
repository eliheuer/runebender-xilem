# Babelfont migration progress

Status: **IN PROGRESS — M00–M03 complete; M04 and later cutovers active**.
The definition of complete and milestone dependencies remain in [the checklist](babelfont-migration-checklist.md).
Canonical queries, layer and source-metadata edits, snapshots and auxiliary-layer structure now operate on Babelfont plus typed extensions.
The remaining migration milestones still own ordinary topology tools, history replacement, application callers, broader metadata, interpolation, compilation, experiments and removal of compatibility state.

## Continuation checkout

The active integration continuation is `/Users/eli/.codex/worktrees/f236/runebender-xilem` on branch `codex/babelfont-integration`.
It started from clean commit `3f209776d35dbc7e88e35facb3a48e9f7edd68e0` and does not modify or replace the earlier continuation below.
The earlier worktree and its branch remain preserved for review.

- Worktree: `/Users/eli/.codex/worktrees/790d/runebender-xilem`.
- Branch: `codex/babelfont-migration`.
- Baseline: `624879a1c447d3e9f012c34f4b5cb091bb0df6cb`.
- Setup verified that clean starting commit `5c37be7717e780aac3fcb369b033148f57026ef9` was an ancestor, created the isolated branch, and fast-forwarded it to the exact baseline.
- The first implementation run started clean; main and the originating task's branch were not changed.
- The checklist contains 15 milestones and 76 acceptance steps.

### Active integration checkpoint

The current integration series owns canonical Designspace structure, guarded Designspace source transactions, direct font-information and source-comparison reads, direct curve conversion and handle cleanup, typed component alignment, typed glyph-layer metadata and canonical proposal/version state.
It also preserves auxiliary history across source restore and invalidates metric-dependent application state when metadata history replays.
Component alignment, mark color, metrics keys and formulas, metaball payloads and related source metadata now have typed canonical ownership while UFO keys are rehydrated only at projection boundaries.

The accepted curve-conversion and cleanup implementation is integrated through `2b15baa`.
Independent review found and then verified the correction that prevents harmonize and balance from reinterpreting implied quadratic chains as cubic segments.
The dedicated canonical handle-cleanup suite passes 11 tests, and the complete variable-project suite passes 60 tests with the configured Virtua Grotesk fixtures.

M04 remains active because the central checklist still includes special editable-source preservation and complete selection and metadata acceptance.
M05 remains active in its owned history lane; the Project-level Designspace transaction hooks are foundations rather than a completion claim.

The 2026-09-19 identity audit reopened one M01 substep because `VariableGlyph` did not yet store the stable `GlyphId` promised by the field-ownership contract.
Evidence commits `15f065b`, `b2498c8`, `f8a31da`, `6f2ddce` and `14accbe` close that gap with stable glyph identity, atomic whole-glyph add, duplicate, rename and removal transactions, fresh copied object identities and typed smart-metadata initialization.
The follow-up proof covers filling a missing source layer without replacing `GlyphId`, preserves active-source command eligibility and retains the established invalid-Unicode fallback for one-character glyph names.
The save/reopen follow-up `7524a94` additionally preserves component references, exact metrics-key spelling, opaque lib data, advances and cleared Unicode after duplicate and rename.
All 11 focused lifecycle tests, the smart-component identity regression and warning-denied library/test Clippy pass on the integrated branch.

### Canonical smart metadata, rendering and re-interpolation checkpoint

Evidence commit: `0e6b147` (`Own smart metadata and reinterpolation canonically`).

Smart-component axes, stable-`ComponentId` values and pole selections now have one typed owner outside the opaque glyph lib.
Import removes each valid known key from the residual lib, projection writes it once in current component order, component removal drops its values, and copied or reconciled layers rebind entries to their fresh or retained stable identities.
The focused storage regression and all seven codec tests pass.

Canonical re-interpolation now installs contours and exact width as one guarded layer transaction, preserves other layer fields, records one Project-owned history step, and constructs no UFO glyph.
The focused Virtua Grotesk re-interpolation and undo regression passes.

Hyperbezier rendering is integrated through `e7fbedc`, exact accumulated component-affine rendering through `656a434`, and the integration correction plus selected-layer to same-source-default fallback are included in `0e6b147`.
Evidence commits `8713aa0`, `7d34a07` and `3a44690` add typed metaball and one-axis or two-axis smart-component rendering, route `Project::document_layer_path` through it with same-source pole collection and apply the same explicit recursion bound to ordinary and smart paths.
All 10 focused renderer tests and all five matched component-graph integration tests pass, including nested metaballs, bilinear smart poles, full affine accumulation, selected-layer fallback, nonfinite rejection and the generated depth-limit chain.

Evidence commit `4419585` moves valid HOI intermediate points into typed exact-preserving layer storage and makes interpolation read that canonical value directly.
Malformed HOI payloads remain untouched in the opaque lib, and import/export retains the sole boundary key and exact valid dictionary representation.
The existing HOI interpolation and boundary tests plus the new typed valid/invalid payload test pass.

Evidence commit `9ecb21e` replaces interpolated-source UFO cloning with one staged canonical source transaction using fresh object identities and retained logical `GlyphId` values.
The combined variable-project suite passes 63 tests, including failure atomicity, undo/redo, save/reopen, metadata preservation and source identity.

### Canonical proposals, versions and headless callers

Evidence commits: `42850df`, `8f300a2`, `7d4bd9b`, `d5ece36`, `9ae2bc4` and `7718d1c`.

Proposal batches, review layers, installation, isolated experimental versions, live tools and Nodes-live routing now use canonical layer snapshots and stable source/layer identities.
Invalid task names and invalid later glyphs reject before publication or revision changes.
Installing or discarding the final proposal glyph now removes its empty auxiliary-layer container from canonical storage and the compatibility projection, with save/reopen proof.
Live Nodes refuse an absent source binding and never silently retarget after source removal.
Focused evidence passes 10 proposal-batch tests, seven experiment tests, five live tests and five Nodes-live tests.

Headless proposal installation and source comparison now call the same canonical Project operations as the editor.
The headless Nodes suite passes six tests, including exact shifted-geometry comparison behavior.

Composition plans now carry foreground revision tokens and publish the complete proposal layer through one guarded canonical source-structure commit.
Selective proposal and isolated-version installation build canonical layer drafts directly, preserving stable identities, foreground metadata and Project-owned undo while retaining the explicit external proposal serialization contract.
The hardening follow-up `087e0e7` matches identifiers before structural fallbacks across reordered contours, points, components and anchors and includes all object metadata in direct payload equality.
The integrated proposal, edit-batch, compose and experiment suites pass 8, 11, 8 and 7 tests respectively.

### Canonical application mutation checkpoint

Evidence commit: `2e16e09` (`Move application edits into canonical transactions`).

FontModel add, add-missing, duplicate, remove and rename wrappers now delegate to the atomic whole-glyph Project APIs.
Component addition and alignment toggles commit guarded canonical layer transactions and Project-owned history.
The complete application binary suite passes 166 enabled tests with four documented ignores, and strict workspace all-target Clippy passes.
M06 remains open because Session still uses a transitional projected-glyph rebase and mixed legacy history while its remaining canvas, clipboard, compose and CLI callers move.

### Stable canonical component-resolution boundary

Evidence commit: `81fd9e7` (`Resolve canonical components by stable identity`).

The component resolver now returns one exact rendered path and one recursively resolved canonical contour set for each top-level `ComponentId`.
The exact path preserves the current hit-testing and selection-feedback contract, while the rounded `CopiedContour` values preserve the existing decomposition contract and source metadata.
Missing references, cycles, excessive depth and nonfinite transforms remain explicit errors.

The focused component-resolution regression passes and requires top-level identity order, exact combined geometry and nonempty decomposition payloads.
Warning-denied library/test Clippy, formatting and diff checks also pass at this integration checkpoint.
M06 owns consuming this API and removing the session's Norad component cache.

Independent review found that the first direct compiler-structure cutover did not retain the resolved on-disk directory used by feature includes.
The lane-owned correction is integrated as `6087674`, and its two include-path regressions, all nine variable-compiler tests, all five text-feature tests and the canonical text-input parity regression pass.
Production native text inventory, kerning and generated mark features now read canonical Project queries through `0b3e008`.

### Guarded cross-lane Project hooks

Evidence commit: `9e2f519` (`Add guarded canonical integration hooks`).

`Project::commit_document_layer_replacement` installs an opaque canonical snapshot only when its stable address and expected live value still match, then records exactly one Project-owned history step for a real change.
Unchanged, stale and address-mismatched replacements preserve document state and history.
This is the selective proposal and isolated-version installation boundary requested by M10.

`Project::document_layer_path` resolves one `GlyphLayerAddress` into its component-inclusive canonical `BezPath` without constructing a UFO glyph or Master.
Missing root glyphs, missing component bases, cycles, excessive depth and nonfinite component geometry remain explicit errors.
This is the component-inclusive bounds and proof input requested by M11.

Both focused regressions and warning-denied library plus variable-project Clippy pass.
The Project whole-glyph lifecycle transaction required by M07 remains the next shared-model dependency.

## M00 — Establish the continuation and measurable baseline

Run date: 2026-09-18 America/Los_Angeles (2026-09-19 UTC).
Evidence commit: `Record the Babelfont migration baseline and behavior contracts` (the commit introducing this file).
Resolve its exact ID with `git log --diff-filter=A --format=%H -- docs/babelfont-migration-progress.md`; this avoids a self-referential commit hash.
There is no model implementation commit in M00.
Affected paths: this log, the checklist and [the reviewed inventory](babelfont-migration-inventory.md).
All production callers remain on the baseline implementation.

Read AGENTS, ARCHITECTURE, DESIGN, the variable-project decision, the checklist and `web/README.md`.
Read the [Linebender formatting scheme](https://linebender.org/wiki/formatting-scheme/) before writing documentation; prose uses one sentence per source line.
The reported 569 passed / four ignored full native suite is historical evidence from the preceding task, not a result executed in this checkout.
The four ignored tests do not provide runtime coverage.
No full native gate, browser build, visual proof or performance claim is made for M00.

### Inventory method

Ran `rg -l '\bnorad\b' src tests examples web --glob '*.rs'` and inspected production signatures, mutation guards, histories and their callers.
The inventory distinguishes live model use, codecs, tests, comments and candidate obsolete compatibility methods.
It also follows callers without a literal Norad import; a text-count reduction is not migration completion.
Each runtime family has a migration owner, including `ui/theme.rs` and live operations currently under `formats/`.
The reviewed inventory is versioned evidence; raw search output is in `/tmp/runebender-babelfont-migration-m00/norad-occurrences.txt` and `compatibility-callers.txt` for this run only.

### Behavior to preserve

| Family | Baseline behavior and inspected implementation | Acceptance evidence |
|---|---|---|
| Drag grouping | `Session::record(Drag)` queues the pre-edit glyph only when entering a gesture; `DragUp` closes it. Point movement uses positions captured at gesture start. Anchor, component and advance drags share the grouping mechanism. `sync_session_from` drains pending records into the source's per-glyph history and clears metadata redo on a new record. | Session tests for anchor/metric transactions, component editing and source history; `document::history` tests. |
| No-op history | A failed knife cut removes its pending record, or emits `DiscardLast` if the record was already drained. Parameterized filters record only real changes. This is not a claim that every no-op gesture already suppresses history: `begin_point_drag` records before movement. | Session filter tests and history discard test; code inspection of `knife_cut` and `begin_point_drag`. |
| Auxiliary history | `Project::edit_layer` clones the target, rejects unchanged payloads and renames, and records default-layer changes in the source history. Other layers use `VariableData.histories` keyed by `LayerId`, with per-glyph stacks. `undo_layer` replays without switching the active source. | `layer_edits_and_history_round_trip_all_source_data` and `guarded_legacy_edits_commit_to_canonical_layers_before_save`. |
| Metadata history | Rename, Unicode and font-data history are separate workspace stacks. Replay requires the selected glyph name and current glyph undo depth to match the recorded context. Unicode/groups/kerning/features snapshots carry source IDs and are reordered against current IDs; a changed source set rejects replay. Rename preserves the glyph's history name. | Inspector metadata tests and host cross-master/source-reorder tests. |
| Source undo | Structural history captures masters, variable data, source names/locations, brace sources, Designspace and active selection. Undo/redo compares expected font contents, paths, source names/locations and Designspace; a mismatch restores the popped step and rejects replay. Restore retains the larger source-ID counter. Removal retains UFO files. Live experiments prevent source removal/reorder while they reference source indices. | Variable-project source-authoring and source-undo tests; host source-command test. |
| Proposal apply | Installation is per glyph, with optional selection and structure checks. Missing foregrounds and stale proposals carrying base revisions are skipped and retained. Legacy proposals without a base revision still follow the existing compatibility contract. Installation copies contours, components, anchors and width, preserving foreground height, Unicode, note, guides, image and glyph lib; installed proposals leave the layer and an empty layer is removed. The source wrapper records one undo step per installed glyph. | Edit-batch stale/atomic/metadata tests, proposal tests and source proposal-history test. |
| Experimental conflicts | Forks keep a root baseline, including parent forks. Apply validates all selected glyphs and optional kerning before mutating the root; duplicate selections, stale glyph revisions, structure failures, changed root groups/kerning and an empty apply fail. Unrelated root edits survive. Apply records ordinary glyph undo plus a whole-apply record. `undo_apply` rejects changed affected glyphs or changed kerning/group revisions; it leaves unrelated edits alone and itself records glyph history. Versions remain session-only. | `document::experiments` tests and existing live entry points. |

`EditHistory::amend` replaces the recorded snapshot; its test deliberately undoes to the amended value.
The current Session drag path does not call it, so it must not be mistaken for the mechanism that preserves a drag's starting value.
The inspected `GlyphSnapshot` includes contours, components, anchors, exact advances, Unicode, note, guidelines, image and lib.
M05 must preserve those fields together while eliminating Norad storage.

### Executed checks

The focused baseline command completed successfully:

```sh
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources \
  cargo test --locked --test babelfont_contract --test variable_project --test variable_compile
```

Result: Babelfont contract 2 passed, variable compilation 6 passed, variable project 12 passed; zero failed or ignored.
The first build finished in 1m 48s and emitted a future-incompatibility notice for dependency `block v0.1.6`.
Log: `/tmp/runebender-babelfont-migration-m00/focused.log`; the counts and commands in this versioned document are the durable evidence if temporary logs expire.
Cargo used this worktree's ignored `target/`; no duplicate build was started.
Process-list inspection was denied by the sandbox, but the build was tracked to successful exit through its returned session.
The fixtures use disposable directories and the real font path is available read-only for tests that need it.

The following commands ran with the same `RUNEBENDER_TEST_FONTS` environment and `-- --test-threads=1` appended to each command.
Every row passed with zero failures and zero ignored tests.
Log names are relative to `/tmp/runebender-babelfont-migration-m00/`.

| Command | Passed | Log |
|---|---:|---|
| `cargo test --locked --lib document::history::` | 6 | `history.log` |
| `cargo test --locked --lib document::experiments::` | 4 | `experiments.log` |
| `cargo test --locked --lib document::edit_batch::` | 4 | `edit-batch.log` |
| `cargo test --locked --lib document::proposal::` | 5 | `proposal.log` |
| `cargo test --locked --lib document::source::tests::a_proposal_installs_one_undo_step_per_glyph` | 1 | `proposal-history.log` |
| `cargo test --locked --bin runebender application::editor::session::tests::` | 15 | `session.log` |
| `cargo test --locked --bin runebender application::editor::inspector::size_tests::` | 8 | `inspector-corrected.log` |
| `cargo test --locked --bin runebender application::platform::host::tests::source_commands_preserve_glyph_history_across_removal_and_reorder` | 1 | `source-history.log` |
| `cargo test --locked --bin runebender application::platform::host::tests::unicode_and_rename_undo_atomically_across_masters` | 1 | `metadata-history.log` |

Total: 65 selected tests passed, including the 20 focused baseline tests.
An initial inspector filter used `inspector::tests::` and matched zero tests; that invocation is excluded from the total.
Inspected `cargo test --locked --bin runebender -- --list`, corrected the filter to `inspector::size_tests::`, and executed all eight matching tests.
No ignored or filtered-out tests are counted as coverage.

Inventory validation found 74 `src/` Rust files with a literal whole-word Norad mention and verified that every one has a path entry in the reviewed inventory.
The inventory also documents indirect production routes, fixture-only occurrences, comment-only occurrences and candidate obsolete compatibility methods.
Documentation validation includes `git diff --check` and explicit whitespace checks on the newly added files.
M00's four acceptance steps are complete; M01–M14 remain unchecked.

### Next action

Main promotion completed at `314aa3235c372ed8d5fef7a2cddb8be3a07ad1da`; the originating task verified the actual checkout and `origin/main` at that exact commit.
M01 has begun with the field and identity ownership contract in [Babelfont document field and identity ownership](babelfont-field-ownership.md).
The next substep is to introduce the typed exact-value and object-metadata structures, then replace positional preservation matching before direct topology edits.
There is no known external blocker.

## M01 — Define exact values, identities and preservation ownership

Status: complete.
Evidence commit: `Define Babelfont document field and identity ownership` (the commit adding the contract).
Resolve its exact ID with `git log --diff-filter=A --format=%H -- docs/babelfont-field-ownership.md`; this avoids a self-referential commit hash.
Affected paths: `docs/babelfont-field-ownership.md`, this log and the checklist.
No runtime ownership changed in this substep.

The ownership contract assigns geometry, exact advances, component matrices, object metadata, Unicode, names, categories, guides, images, notes, libs, font/source/instance data and opaque format extensions to one authoritative location.
It defines typed identities and behavior for rename, insert, delete, reorder, duplicate, copy/paste, topology replacement, undo, proposals and experimental versions.
It explicitly identifies the current full Norad payloads, templates, Master fonts, reconciliation guards and positional matching as violations still to remove.

Evidence:

- Inspected the pinned Babelfont layer, shape, node and anchor definitions at `29bdedbb`; layer width is `f32`, components store decomposed transforms, and object format-specific values cannot serve as a second live owner.
- Inspected Norad 0.13.0 object identifiers, object libs, image placement and the current Runebender projection code.
- `git diff --check` and the documentation one-sentence-per-line check run before commit.

At that point, M01 still required typed extensions, identity-aware mapping, adversarial round-trip fixtures and full acceptance tests.

### Identity-aware projection substep

Evidence commit: `Preserve Babelfont object metadata by identity` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Preserve Babelfont object metadata by identity$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/document/variable.rs`, the checklist and this log.

The Babelfont adapter now assigns distinct typed session identities to contours, points, components and anchors and carries those identities in a private Babelfont format-specific token.
Projection resolves the preserving UFO object by identity rather than by current array index.
Reordering points, components or anchors therefore moves their identifiers, libs, names, colors and exact component matrices with the intended object.
An inserted object without an identity starts without another object's metadata.

`VariableGlyph` stores a transitional `LayerPreservation` beside Babelfont geometry and keeps its existing read API for unmigrated callers.
That preservation structure still contains a complete Norad glyph, so this substep does not complete the typed-extension checkbox or satisfy M01 acceptance by itself.
Direct Babelfont topology mutation remains intentionally unavailable until the remaining exact fields move into typed extensions and the import/reconciliation path can retain identities across edits.

Executed evidence:

```sh
cargo test --locked --lib document::babelfont::tests:: -- --test-threads=1
cargo test --locked --test babelfont_contract --test variable_project -- --test-threads=1
cargo clippy --locked --lib -- -D warnings
```

The identity test passed and covers point insertion plus point, component and anchor reorder.
It verifies that UFO identifiers and per-object libs remain attached to their objects, a new point receives no inherited metadata, and a six-coefficient component matrix remains exact through Babelfont's decomposed representation.
The two Babelfont contract and 12 variable-project tests passed with zero failures or ignored tests.
The existing `block v0.1.6` future-incompatibility notice remains a dependency notice, not a test failure.

Remaining M01 work: replace the complete preserving glyph with typed exact-value and metadata extensions, retain identity through compatibility reconciliation, add the full adversarial no-op/edit/undo/save fixture, and execute M01 acceptance.

### Typed layer preservation substep

Evidence commit: `Replace preserving glyph mirrors with typed layer data` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Replace preserving glyph mirrors with typed layer data$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/document/variable.rs`, `src/document/project.rs`, `src/document/interpolation.rs`, `src/document/sources.rs`, `src/document/compile.rs`, the affected integration tests, the checklist and this log.

`LayerPreservation` no longer contains a `norad::Glyph`.
It owns exact horizontal and vertical advances, Unicode, note, guidelines, image placement, glyph lib, exact six-coefficient component transforms and identity-keyed contour/point/component/anchor metadata.
Babelfont remains the sole stored owner of ordinary geometry.
The adapter constructs temporary Norad glyphs only when a source-format or unmigrated compatibility caller requests a materialized layer.

`VariableGlyph` now exposes stable layer addresses and layer presence rather than references into a persistent Norad glyph mirror.
`Project::glyph_layer` is the explicit transitional materialization entry point used by interpolation, compilation metadata, auxiliary-layer commands and remaining tests.
Moving those consumers onto direct document queries remains M03, M08 and M09 work; this change does not misclassify their temporary conversions as the final architecture.

Compatibility reconciliation compares a temporary projection with the incoming UFO glyph before replacing canonical geometry or incrementing its revision.
No-op scoped access therefore remains a no-op.
An actual compatibility edit rebuilds typed preservation and Babelfont geometry together, while object identity inside direct Babelfont projection continues to prevent positional metadata attachment.

Executed evidence:

```sh
cargo test --locked --lib document::babelfont::tests:: -- --test-threads=1
cargo test --locked --test babelfont_contract --test variable_project --test variable_compile -- --test-threads=1
cargo clippy --locked --lib -- -D warnings
cargo fmt --all --check
git diff --check
```

The identity unit test and all 20 focused integration tests passed with zero failures or ignored tests.
Warning-denied library Clippy and formatting passed.
The existing `block v0.1.6` future-incompatibility notice remains unchanged.
A source search confirms that `document/babelfont.rs` and `document/variable.rs` no longer store `glyph: norad::Glyph` or `BTreeMap<LayerId, norad::Glyph>`.

The typed extensions still use narrow Norad leaf values for codec metadata such as identifiers, colors, guidelines and image placement.
Those are no longer geometry or complete glyph mirrors, and M07/M12/M13 still own replacing their remaining public/runtime exposure and enforcing the final codec allowlist.
The glyph-free `norad::Font` source templates and full Master compatibility fonts also remain tracked for M12/M13; this substep does not claim the final architecture.

Remaining M01 work: retain object identity through compatibility reconciliation, add adversarial fixtures for colliding `f32` widths, fractional metadata and every object kind, then execute no-op/edit/undo/save acceptance.

### Compatibility-reconciliation identity substep

Evidence commit: `Retain object identity through compatibility reconciliation` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Retain object identity through compatibility reconciliation$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/document/variable.rs` and this log.

An actual edit through the transitional Norad compatibility guard now reconciles the rebuilt Babelfont layer with its previous typed preservation data.
Contours, points, components and anchors retain their session identities through reorder and geometry changes when a unique identifier, exact-object or metadata signature match exists.
Ambiguous or unmatched objects receive new identities rather than inheriting metadata by position.
This keeps identity-keyed metadata attached to the intended objects while the remaining compatibility callers migrate to direct document operations.

Executed evidence:

```sh
cargo test --locked --lib document::babelfont::tests:: -- --test-threads=1
cargo test --locked --test babelfont_contract --test variable_project --test variable_compile -- --test-threads=1
cargo clippy --locked --lib -- -D warnings
cargo fmt --all --check
git diff --check
```

The two Babelfont adapter unit tests and all 20 focused integration tests passed with zero failures or ignored tests.
The new reconciliation test changes coordinates and transforms while reordering identified points, components and anchors, then verifies both identity retention and exact projected UFO output.
Warning-denied library Clippy and formatting passed after simplifying one redundant component-signature expression reported by Clippy during development.
The existing `block v0.1.6` future-incompatibility notice remains unchanged.

Remaining M01 work: add the adversarial no-op/edit/undo/save fixture with colliding `f32` widths, fractional metadata and every object kind, then execute the full M01 acceptance checks.

### Adversarial round-trip and M01 acceptance substep

Evidence commit: `Complete Babelfont preservation contracts` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Complete Babelfont preservation contracts$' -1`.
Affected paths: `tests/variable_project.rs`, `tests/variable_compile.rs`, the checklist and this log.

The adversarial project fixture contains distinct `f64` advances that narrow to the same Babelfont `f32`, fractional kerning, exact six-coefficient component and image transforms, object identifiers and libs, glyph and font unknown-data entries, guides and image data.
It verifies initial import without loss, no-op save and reload, an exact-value and topology edit, undo, redo, a second save and a second reload.
The expected source projection remains byte-value exact at the editable data level, including values Babelfont or OpenType cannot represent directly.

A separate compilation regression sets a `500.6` advance and `-50.5` kerning pair, checks those editable values before and after compilation, and verifies that the compiled shaped advance uses the expected rounded OpenType values.
This establishes the boundary between source fidelity and compiler quantization without making quantized values authoritative.

Executed evidence:

```sh
cargo test --locked --lib document::babelfont::tests:: -- --test-threads=1
cargo test --locked --test babelfont_contract --test variable_project --test variable_compile -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo fmt --all --check
git diff --check
```

The two identity-aware Babelfont unit tests passed.
The two upstream contract tests, seven variable-compiler tests and 13 variable-project tests passed with zero failures or ignored tests.
Warning-denied Clippy for the library and integration tests passed after adding explanatory assertion messages and documenting the fixture's intentional narrowing casts.
Formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

M01 is complete: field ownership, typed preservation, identity rules, identity-aware projection and reconciliation, adversarial source fidelity and compiler quantization are all covered by executed evidence.
The next dependency-ready milestone is M02, beginning with direct document-facing glyph, layer and source readers plus transactional mutations over Babelfont and the typed extensions.

## M02 — Add direct document queries and transactional mutations

Status: active.

### Canonical read views substep

Evidence commit: `Add canonical document read views` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Add canonical document read views$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/document/variable.rs`, `src/document/project.rs`, `src/document/mod.rs`, `src/lib.rs`, `ARCHITECTURE.md`, the M01 ownership contract, `tests/variable_project.rs` and this log.

Project now exposes read-only source, glyph and layer views backed directly by canonical storage.
The views provide stable typed identities for contours, points, components and anchors, exact horizontal and vertical advances, source names and locations, point geometry and roles, exact component matrices and anchor data without constructing a Norad glyph or font.
Existing materializing accessors remain available only for unmigrated callers and format boundaries tracked by later milestones.

The new integration test reads four source identities, all five layers of an adversarial variable glyph, exact metrics and representative contour, point, component and anchor values.
It also verifies that the direct layer view immediately reflects an unsaved compatibility edit after reconciliation.

Executed evidence:

```sh
cargo test --locked --test variable_project document_views_read_exact_canonical_layers_and_stable_source_identity -- --exact --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused direct-reader test passed, warning-denied Clippy passed for library and integration-test targets, and public API documentation built successfully.
Formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

This is the reader half of M02's first checklist item, so that item remains unchecked until the canonical edit draft and transaction API lands.
The next substep is a canonical layer transaction that owns before/after state, reports no-op versus changed results and invalidates revision-dependent data only when it commits.

### Atomic canonical layer transaction substep

Evidence commit: `Add atomic canonical layer transactions` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Add atomic canonical layer transactions$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/document/variable.rs`, `src/document/project.rs`, `src/document/mod.rs`, `tests/variable_project.rs`, `tests/variable_compile.rs`, `ARCHITECTURE.md`, the checklist and this log.

`Project::edit_document_layer` now creates an owned canonical draft, applies a fallible edit closure and commits Babelfont geometry plus typed exact-value extensions together.
The initial draft operations cover exact horizontal and vertical advances, point position, type and smooth state, exact component transforms and anchor position by stable object identity.
Every in-place operation reports whether it changed its value and rejects non-finite numeric input.

An error discards the complete draft, and a draft that returns to its starting state reports `Unchanged`; neither case advances the document revision.
A changed draft replaces both canonical halves atomically, advances the revision once and invalidates the compiled-preview cache through its existing revision key.
The commit then refreshes one temporary Master glyph projection for callers not yet migrated; that synchronization is isolated in `synchronize_compatibility_layer` and remains scheduled for removal in M12.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_layer_transactions_commit_atomically_and_skip_noops -- --exact --test-threads=1
cargo test --locked --test variable_compile canonical_layer_transaction_invalidates_compiled_preview -- --exact --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The transaction test passed for no-op detection, failed-edit rollback, exact metrics, direct point, component and anchor mutation, one-step revision invalidation and compatibility projection refresh.
The compiler test passed and verified that a canonical advance edit replaces the cached preview and reaches shaped OpenType output while the document retains its exact `f64` value.
Warning-denied Clippy passed for library and integration-test targets, and public API documentation built successfully.
Formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

M02's first checklist item is complete.
The next substep is richer change information for geometry, metrics, dependent components and compilation, followed by document-level metadata and structural transaction coverage.

### Canonical change information substep

Evidence commit: `Report canonical document changes` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Report canonical document changes$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/document/variable.rs`, `src/document/project.rs`, `tests/variable_project.rs`, `tests/variable_compile.rs`, `ARCHITECTURE.md` and this log.

Changed layer transactions now return a `DocumentChange` with the direct glyph-layer address, every canonical layer containing a component that references the edited glyph, and separate geometry, exact-metrics, metadata, source-metadata and compilation invalidation signals.
The change classification compares the finished draft with canonical before-state, so an operation that changes and then restores a value still produces no change.
Dependent-layer discovery reads Babelfont components across the canonical glyph map and does not inspect Master projections.

The adversarial fixture now includes a component dependency from every B source layer to A.
Its transaction test verifies four dependent B layers are reported after editing A, with geometry, metrics and compilation marked stale while metadata and source metadata remain unchanged.
The compiler regression consumes the same compilation signal and still proves the cached preview is replaced.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_layer_transactions_commit_atomically_and_skip_noops -- --exact --test-threads=1
cargo test --locked --test variable_compile canonical_layer_transaction_invalidates_compiled_preview -- --exact --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

Both focused transaction tests passed.
Warning-denied Clippy passed for library and integration-test targets, and public API documentation built successfully.
Formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

M02's broader change-information item remains open until source-metadata and structural transactions emit the same contract.
The next substep is a canonical source-metadata transaction with atomic rollback, precise source and compilation invalidation and no mutation through a Norad font.

### Canonical source metadata substep

Evidence commit: `Make feature text canonical source metadata` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Make feature text canonical source metadata$' -1`.
Affected paths: `src/document/variable.rs`, `src/document/project.rs`, `src/document/babelfont.rs`, `src/document/mod.rs`, `src/document/compile.rs`, `tests/variable_compile.rs`, `ARCHITECTURE.md`, the checklist and this log.

VariableData now owns each source's OpenType feature text as typed source metadata.
Glyph-free UFO templates clear their feature text, and source-format projection restores it from the canonical record, removing one duplicated known field from the template.
Compiler snapshots read the canonical feature text directly.

`Project::edit_document_source_metadata` applies an owned metadata draft with the same failure rollback, no-op detection and one-step revision behavior as layer transactions.
Its `DocumentChange` reports the affected source, metadata and compilation invalidation without claiming a layer, geometry or metric change.
The existing `set_feature_text` command now delegates to this transaction, while the temporary Master feature value is refreshed only for unmigrated readers.

Executed evidence:

```sh
cargo test --locked --test variable_compile canonical_source_metadata_transaction_is_atomic_and_invalidates_compile -- --exact --test-threads=1
cargo test --locked --test variable_compile shared_feature_edits_and_variable_drafts_do_not_depend_on_selected_master -- --exact --test-threads=1
cargo test --locked --test variable_project exact_values_and_object_metadata_survive_import_edit_undo_and_save -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --lib document::source::tests:: -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The source-metadata test passed for no-op detection, explicit rejection rollback, source-only change information, compatibility and format projections, and compiled-preview invalidation.
The existing selected-source independence test and adversarial save/reload test passed with canonical feature ownership.
All 13 source-model unit tests passed after supplying the documented `RUNEBENDER_TEST_FONTS` path; an initial invocation without it failed only because this worktree has no adjacent fixture checkout.
Warning-denied Clippy passed for library and integration-test targets, and public API documentation built successfully.
Formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

M02's explicit change-information and compatibility-isolation checklist items are complete.
The remaining M02 work is atomic structural mutation and a canonical whole-document snapshot suitable for history and experimental versions without cloning Master fonts or UFO templates.

### Canonical document snapshot substep

Evidence commit: `Add canonical document snapshots` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Add canonical document snapshots$' -1`.
Affected paths: `src/document/variable.rs`, `src/document/project.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, the checklist and this log.

`Project::document_snapshot` now clones the complete currently canonical editing state: Babelfont glyph geometry, typed exact-value and object-metadata extensions, typed source feature metadata and stable source identity order.
The snapshot deliberately excludes UFO templates, Master projections, compiled caches and edit histories.
It exposes the same direct layer and feature readers as the live document, so future history and experimental versions can compare or retain canonical content without a source-format round trip.

The integration test verifies source and glyph order, exact layer reads, equality after cloning and isolation from later canonical geometry and source-metadata commits.
Existing SourceFrame and experiment callers are intentionally unchanged here; M05 and M10 own replacing their Norad snapshots with this canonical substrate.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_snapshot_isolated_from_later_edits_and_format_projections -- --exact --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused snapshot test passed.
Warning-denied Clippy passed for library and integration-test targets, and public API documentation built successfully.
Formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

M02's canonical snapshot checklist item is complete.
The remaining M02 item is atomic structural mutation, beginning with direct canonical auxiliary-layer copy and removal while preserving the existing guarded structural undo contract.

### Canonical auxiliary-layer structure substep

Evidence commit: `Make auxiliary layer structure canonical` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Make auxiliary layer structure canonical$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/document/variable.rs`, `src/document/project.rs`, `src/document/sources.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, the checklist and this log.

Auxiliary-layer copy and removal now update canonical Babelfont geometry and typed preservation records directly.
A copy allocates fresh contour, point, component and anchor document identities while retaining the copied layer's exact values and source metadata.
Only the affected glyph is projected into the temporary Master layer, so the structural command no longer clones and replaces a complete Norad font as its editing model.
The existing `SourceFrame` still captures Master compatibility fonts for guarded structural undo; M05 owns replacing those history frames with canonical snapshots and deltas.

Validation happens before mutation.
A duplicate copy or missing-layer removal returns an error without changing canonical contents, revision or structural history.
Undo and redo restore the copied canonical identities, and removing one glyph's auxiliary layer retains the layer and its other glyphs.

The source-reader regression review also established the single-source location invariant.
New-font, direct-source and loaded-UFO constructors now store one empty normalized location for their one stable source, so `document_source` and `document_sources` expose it consistently with canonical snapshot source IDs.

Executed evidence:

```sh
cargo test --locked --test variable_project auxiliary_layer_structure_mutates_the_canonical_document_atomically -- --exact --test-threads=1
cargo test --locked --test variable_project single_source_constructors_expose_canonical_source_metadata -- --exact --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --lib document::source::tests:: -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The two focused structural and single-source reader tests passed.
All 18 variable-project integration tests and all 13 source-model unit tests passed.
Warning-denied Clippy passed for library and integration-test targets, and public API documentation built successfully.
Formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

M02 is complete.
M03 is the next dependency-ready milestone and begins by moving geometry queries and ordinary point operations from Norad projections to the canonical layer API.

### Structural revision review correction

Evidence commit: `Keep structural history revisions monotonic` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Keep structural history revisions monotonic$' -1`.
Affected paths: `src/document/sources.rs`, `tests/variable_project.rs` and this log.

Review found that structural undo restored the historical `VariableData::revision` before synchronizing the restored content.
That could reuse the current public revision or move it backward even though canonical contents changed.
`SourceFrame::restore` now preserves the live revision generation while restoring historical document contents, then lets structural synchronization advance it.

The regression test copies two glyphs into one auxiliary layer, verifies undo removes only the second glyph, verifies redo restores it, and requires each changed state to receive a newer revision.
It then performs a canonical layer edit and verifies the normal one-step revision contract continues from the restored generation.

Executed evidence:

```sh
cargo test --locked --test variable_project structural_undo_and_redo_advance_the_live_document_revision -- --exact --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused revision regression and all 19 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

## M03 — Migrate geometry queries and ordinary point operations

Status: active.

### Canonical contour path conversion substep

Evidence commit: `Convert canonical contours directly to paths` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Convert canonical contours directly to paths$' -1`.
Affected paths: `src/outline/glyph_paths.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md` and this log.

`ordinary_layer_contours_to_bezpath` now converts a canonical `LayerView` directly to a Kurbo path without constructing a Norad glyph.
The canonical and compatibility entry points share one point-sequence converter so their ordinary line, cubic and quadratic behavior cannot drift while callers migrate.
The converter retains explicit open contours, cyclic closed contours and implied on-curve points between consecutive quadratic controls.
All-off-curve closed quadratic contours now produce their implied segments instead of being silently omitted.

The fixture comparison covers closed lines, an open contour, consecutive quadratic controls, an all-off-curve contour and an empty glyph.
It requires exact `BezPath` equality between canonical and compatibility inputs and separately verifies the expected implied quadratic segments.
Component resolution, mixed shape ordering, point and anchor extraction, hit-testing inputs and analysis callers remain for the next M03 substeps, so no M03 checklist item is complete yet.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_contour_paths_match_legacy_conversion_and_keep_implied_quadratics -- --exact --test-threads=1
cargo test --locked --lib outline::glyph_paths -- --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused canonical path test, both smart-component compatibility tests and all 20 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Contour-closure transaction correction

Evidence commit: `Keep point roles and contour closure coherent` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Keep point roles and contour closure coherent$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs` and this log.

Review found that changing the first point between `Move` and another segment role changed UFO closure semantics without updating the canonical Babelfont path's `closed` flag.
`LayerEditDraft::set_point_type` now changes the first point role and contour closure together.
It rejects a `Move` role on any noninitial point with `DocumentEditError::NonInitialMove`, leaving the complete draft available for atomic rollback.
A change followed by restoration in the same draft remains a no-op and does not advance the document revision.

The regression test covers open-to-closed and closed-to-open edits, exact canonical-versus-projected path equality, rejected noninitial moves, unchanged snapshots and revisions after failure, and change-then-restore no-op detection.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_point_roles_keep_contour_closure_coherent -- --exact --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused closure regression and all 21 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Canonical component-resolution substep

Evidence commit: `Resolve components from canonical layers` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Resolve components from canonical layers$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/document/mod.rs`, `src/outline/glyph_paths.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md` and this log.

`LayerView::shapes` now exposes contours and components in canonical Babelfont storage order with typed views and stable object identities.
`ordinary_layer_to_bezpath` walks that ordered shape stream, resolves component bases through a caller-supplied canonical layer lookup and applies the exact six-coefficient component transform.
The operation returns `ComponentResolveError` for a missing base, a named reference cycle or an excessive graph depth instead of silently dropping broken geometry.
Hyperbezier contours and smart-component pole interpolation remain on their existing paths until M04 migrates those special editable formats.

The comparison test proves the direct canonical outline matches the existing recursive Norad result for a transformed component.
It also verifies canonical shape order and exact missing-reference and cycle errors.
Bounds, flattened point and anchor inputs, hit testing and geometry-analysis callers remain for subsequent M03 substeps.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_component_resolution_matches_legacy_and_reports_broken_graphs -- --exact --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused component-resolution regression and all 22 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Canonical measurement-input substep

Evidence commit: `Measure canonical layer geometry directly` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Measure canonical layer geometry directly$' -1`.
Affected paths: `src/analysis/measure.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md` and this log.

`ordinary_layer_measurements` now reads point positions and on-curve roles from canonical contour views and scans the direct canonical Kurbo path for curve-bounded spans.
`ordinary_layer_side_bearings` derives exact advance and extreme-point geometry from the same layer without constructing a Norad glyph or the legacy editable contour model.
The existing compatibility functions share the extracted measurement and side-bearing calculations, retaining their established closed-contour start convention while callers migrate.

The adversarial comparison requires identical ordered measurements and side-bearing geometry from canonical and legacy inputs, including exact layer advance values.
The originating-task review also independently verified nested reflected and skewed component transforms, repeated shared bases and a singular outer transform against constructed expected paths; no resolver defect was found.
Application session callers remain on their Norad glyph until M06, and special hyperbezier measurement input remains assigned to M04.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_measurement_inputs_match_legacy_geometry -- --exact --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused measurement comparison and all 23 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Canonical curve-analysis input substep

Evidence commit: `Analyze canonical contour curves directly` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Analyze canonical contour curves directly$' -1`.
Affected paths: `src/outline/glyph_paths.rs`, `src/analysis/curve.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md` and this log.

`ordinary_contour_to_bezpath` now exposes one canonical ordinary contour as a Kurbo path without a UFO contour.
`ordinary_cubics_from_layer` converts canonical lines, quadratics and cubics into the existing analysis segments and reads smooth state directly from stable point views.
The Norad compatibility entry point and canonical entry point share the extracted path-to-cubic conversion, including quadratic elevation, closing segments and smooth-point matching.

The path fixture now requires identical ordered curve-analysis segments for closed lines, an open contour, consecutive quadratic controls and an all-off-curve implied quadratic contour.
Hyperbezier analysis still uses its solver-backed compatibility route pending the M04 special-outline migration.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_contour_paths_match_legacy_conversion_and_keep_implied_quadratics -- --exact --test-threads=1
cargo test --locked --lib analysis::curve -- --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused path-and-analysis comparison, all six curve-analysis unit tests and all 23 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Open-contour measurement correction

Evidence commit: `Stop measurements at open contour endpoints` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Stop measurements at open contour endpoints$' -1`.
Affected paths: `src/analysis/measure.rs`, `src/outline/path/`, `tests/variable_project.rs` and this log.

Review found that the shared measurement algorithm treated every point list as cyclic and therefore measured an imaginary closing segment on open contours.
Each shared measurement input now carries its explicit closure state.
Segment and handle-neighbor traversal wraps only for closed contours, while open endpoints stop without synthesizing geometry.
The reusable path types expose their closure state so both canonical and compatibility measurement entry points use the same corrected behavior.

The regression test requires exactly two 100-unit segments for an open three-point path and retains the two edges plus 141-unit closing diagonal for the equivalent closed path.
The existing canonical-versus-compatibility comparison continues to pass.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_measurements_do_not_close_open_contours -- --exact --test-threads=1
cargo test --locked --test variable_project canonical_measurement_inputs_match_legacy_geometry -- --exact --test-threads=1
cargo test --locked --lib outline::path -- --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

Both focused measurement tests, all 13 path unit tests and all 24 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Canonical selection-transform substep

Evidence commit: `Transform canonical point selections directly` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Transform canonical point selections directly$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md` and this log.

`LayerEditDraft::transform_points` now applies an affine transform to stable point identities around the selected points' bounding-box center.
An empty selection targets every point, matching the existing editor command.
The draft validates every selected identity and every affine coefficient before mutation, reports whether coordinates changed and preserves document transaction rollback and no-op revision behavior.

The adversarial comparison selects points across two contours and requires the canonical projected glyph to equal the existing index-based selection transform after a reflected nonuniform rotation.
It also verifies missing-point and nonfinite-transform rejection leave the canonical snapshot and revision unchanged, and that an identity transform produces `Unchanged`.
Handle-aware snapped dragging, smoothing, sidebearing shifts and segment conversion remain for later M03 substeps.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_selection_transform_matches_legacy_geometry_atomically -- --exact --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused selection-transform comparison and all 25 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Selection-transform overflow correction

Evidence commit: `Reject nonfinite derived point transforms` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Reject nonfinite derived point transforms$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs` and this log.

Review found that finite affine coefficients could overflow while composing the selection-centered transform or applying it to finite points.
`LayerEditDraft::transform_points` now computes a finite-safe bounding-box center, validates the composed affine and precomputes every transformed position before changing the draft.
Any nonfinite derived value rejects the complete operation with `DocumentEditError::NonFinite`; values are never clamped into the editable source.

The regression test applies a finite `f64::MAX` scale to ordinary finite points and requires the canonical snapshot and revision to remain unchanged.
The valid reflected transform, missing-point rejection, explicitly nonfinite affine and identity no-op cases continue to pass.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_selection_transform_matches_legacy_geometry_atomically -- --exact --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused transform regression and all 25 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Canonical handle-aware point-drag substep

Evidence commit: `Move canonical point selections with handles` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Move canonical point selections with handles$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/outline/point_ops.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

`LayerEditDraft::translate_points` now moves stable point identities directly in canonical Babelfont paths.
It shares the editor's representation-neutral snapping, adjacent-handle carrying and smooth-tangent logic without materializing a UFO glyph for the operation.
All requested identities, supplied drag origins and derived coordinates are validated before the draft mutates, so callers can catch an error inside a transaction without committing a partial edit.

The shared drag routine now derives carried handles' drag-start positions from their selected on-curve owner.
This keeps carried handles rigid across repeated total-delta pointer events even though the editor records explicit origins only for selected points.

The integration comparison requires canonical and compatibility glyphs to remain equal for an on-curve drag, repeated drag event and selected smooth-handle drag.
It also verifies stable point identities and atomic rejection of a missing identity, a nonfinite delta and finite inputs whose derived coordinate overflows.
Smoothing commands, sidebearing shifts and segment conversion remain for later M03 substeps.

Executed evidence:

```sh
cargo test --locked --lib outline::point_ops -- --test-threads=1
cargo test --locked --test variable_project canonical_point_drag_matches_legacy_handle_behavior_atomically -- --exact --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

All seven point-operation unit tests, the focused canonical comparison and all 26 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Linear point-edit application correction

Evidence commit: `Apply canonical point replacements linearly` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Apply canonical point replacements linearly$' -1`.
Affected paths: `src/document/babelfont.rs` and this log.

Review measured quadratic scaling in the validated replacement phase of `LayerEditDraft::transform_points` because every canonical node searched the full replacement list.
The transform and handle-aware drag operations now retain their prevalidation pass, index validated replacements by stable raw point identity and apply them in one ordered node traversal.

A standalone debug-library microbenchmark measured the transform call alone for five samples at each size.
Median time was 0.777 ms for 512 points, 1.244 ms for 1,024, 1.929 ms for 2,048, 2.966 ms for 4,096 and 6.051 ms for 8,192.
These measurements establish linear size scaling for the corrected application pass and are supporting evidence for the later M14 performance gate; they are not release-mode or UI-latency claims.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_ -- --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

All 13 canonical-filtered and all 26 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Canonical smoothing and sidebearing-shift substep

Evidence commit: `Edit canonical smoothing and sidebearings` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Edit canonical smoothing and sidebearings$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md` and this log.

`LayerEditDraft::toggle_smooth_points` now applies the editor's bulk smooth/corner toggle directly to stable canonical point identities while leaving selected off-curve controls unchanged.
`LayerEditDraft::shift_points_and_anchors_x` now performs the geometry half of a left-sidebearing edit directly in Babelfont, moving contour points and anchors while preserving component transforms and exact advance width.

Both operations validate their complete input before mutation.
The sidebearing shift also validates every derived coordinate, so a finite delta that overflows one later point cannot leave earlier geometry partially shifted when the caller catches the error.

The integration comparison matches the existing smooth-toggle result, checks the exact projected glyph after a fractional sidebearing shift and verifies that advance and component transforms stay fixed.
It also catches missing-point, nonfinite and derived-overflow failures inside a transaction and requires the canonical snapshot and revision to remain unchanged.
Segment conversion remains before the second M03 checklist item can close.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_smoothing_and_sidebearing_shift_match_legacy_geometry_atomically -- --exact --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused canonical comparison and all 27 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Explicit point-drag origin correction

Evidence commit: `Capture every persistent point-drag origin` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Capture every persistent point-drag origin$' -1`.
Affected paths: `src/outline/point_ops.rs`, `src/document/babelfont.rs`, `src/application/editor/session.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

Review found that reconstructing an automatically carried handle's origin from its selected owner's previous snapped displacement lost fractional offsets.
The final handle position could therefore depend on how many intermediate pointer events occurred even when the final total delta was identical.

The editor now captures explicit start positions for selected points, adjacent carried handles and smooth-coupled opposite handles before the first drag event.
Keyboard nudges continue to use current positions through an empty origin map.
Canonical `LayerView::point_drag_origins` exposes the same stable-ID capture, and both canonical and compatibility translation reject a nonempty persistent-drag origin set that omits any affected point.
Smooth mirroring uses the captured opposite-handle baseline when that handle is not otherwise moving.

Unit and integration regressions use off-grid handles across snapping thresholds and require one total-delta event to equal multiple intermediate events.
They also verify explicit rejection of the formerly accepted selected-only persistent origin set while preserving caught-error transaction atomicity.

Executed evidence:

```sh
cargo test --locked --lib outline::point_ops -- --test-threads=1
cargo test --locked --test variable_project canonical_point_drag_matches_legacy_handle_behavior_atomically -- --exact --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo test --locked --bin runebender -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

All nine point-operation unit tests, the focused canonical regression, all 27 variable-project integration tests and 165 application binary tests passed; four installed-model or external-font tests remained intentionally ignored.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Multi-handle drag correction

Evidence commit: `Keep selected handles from carrying controls` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Keep selected handles from carrying controls$' -1`.
Affected paths: `src/outline/point_ops.rs` and this log.

Review found that the refactored moved-point classifier treated an unselected off-curve as carried when any adjacent point was selected, including another off-curve control.
Only a selected on-curve point may carry its adjacent handles.

The regression selects a smooth handle both alone and with an unrelated adjacent handle, then requires the unselected smooth opposite to retain the same mirrored position in both cases.

Executed evidence:

```sh
cargo test --locked --lib outline::point_ops -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo fmt --all --check
git diff --check
```

All ten point-operation unit tests passed.
Warning-denied Clippy, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Canonical line-segment conversion substep

Evidence commit: `Convert canonical line segments directly` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Convert canonical line segments directly$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, the migration checklist and this log.

`LayerEditDraft::convert_line_to_curve` now converts a direct canonical on-curve segment to a cubic without materializing or reconciling a UFO glyph.
It inserts snapped one-third and two-third controls with fresh stable identities, retains endpoint identity and metadata and supports the wraparound closing segment of a cyclic contour.
Missing points and endpoint pairs that no longer identify a direct line fail before mutation.

The integration comparison converts both the forward and closing segments of a metadata-bearing cyclic contour and requires the complete projected glyph to equal the existing segment operation after each step.
It verifies the original endpoint identities, the new control identities and caught-error transaction atomicity.

This completes M03's second checklist item: point movement, selection transforms, smoothing, sidebearing shifts and line-segment conversion now operate directly on canonical geometry.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_line_segments_convert_with_stable_endpoint_identity -- --exact --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused segment comparison and all 28 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Canonical segment hit-testing and M03 acceptance

Evidence commit: `Hit-test canonical ordinary segments` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Hit-test canonical ordinary segments$' -1`.
Affected paths: `src/outline/segment_ops.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, the migration checklist and this log.

`ordinary_layer_segments` now enumerates ordinary canonical line, quadratic and cubic segments with stable endpoint and control identities.
`nearest_ordinary_layer_segment_with_t` supplies the same nearest-segment geometry and parameter used by existing hit-testing callers without constructing a UFO glyph.

The expanded canonical path fixture compares every segment with the path drawn from the same canonical layer and checks a concrete nearest hit.
It also verifies that empty glyphs produce neither path nor hit-test segments.

M03 is complete.
Canonical readers and targeted comparisons now cover ordered path conversion and bounds, point and anchor extraction, ordinary hit-testing input, recursive component resolution, point edits, analysis inputs, open and cyclic contours, implied quadratics, empty glyphs and mixed path/component order.
Missing components and recursive cycles remain explicit errors.
Hyperbezier and topology-changing tools remain assigned to M04.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_contour_paths_match_legacy_conversion_and_keep_implied_quadratics -- --exact --test-threads=1
cargo test --locked --lib outline::segment_ops -- --test-threads=1
cargo test --locked --lib outline::glyph_paths -- --test-threads=1
cargo test --locked --lib analysis::curve -- --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused canonical hit-test comparison, geometry unit suites and all 28 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Implied-quadratic hit-testing correction

Evidence commit: `Match canonical hit tests to implied quadratics` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Match canonical hit tests to implied quadratics$' -1`.
Affected paths: `src/outline/segment_ops.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

Independent review reopened the first and third M03 checklist items because the original hit-test comparison inherited the same quadratic omissions from the Norad compatibility enumerator.
Consecutive quadratic controls were collapsed into one cubic, and closed all-off-curve contours produced no hit-test segments even though the canonical path drew their implied quadratics.

`ordinary_layer_segments` now expands quadratic chains and all-off-curve closed contours with the same geometry as the canonical path converter.
An explicit endpoint retains one stable point identity, while an implied endpoint retains the two source-control identities that define its midpoint.
Segment point identity lists deduplicate controls shared by an implied endpoint.

The integration coverage directly compares hit-test geometry with canonical drawn segments for two-control and longer quadratic chains and all-off-curve contours.
It verifies a nearest hit at an implied join and the source identities on both sides of that join.
The exact two-test independent-review reproducer also passes against the corrected library.
These checks re-establish the first and third M03 checklist items and M03 acceptance.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_hit_testing_matches_implied_quadratic_geometry_and_identities -- --exact --test-threads=1
cargo test --locked --test variable_project canonical_contour_paths_match_legacy_conversion_and_keep_implied_quadratics -- --exact --test-threads=1
rustc --edition=2024 /private/tmp/runebender-migration-review.porJgX/quadratic_hit_testing.rs -L dependency=target/debug/deps --extern runebender=target/debug/deps/librunebender-b47720e92f7db4a3.rlib --extern norad=target/debug/deps/libnorad-52f3943fd0fc68ef.rlib --extern kurbo=target/debug/deps/libkurbo-78ec2af22253a897.rlib --test -o /private/tmp/runebender-migration-review.porJgX/quadratic_hit_testing_fixed
/private/tmp/runebender-migration-review.porJgX/quadratic_hit_testing_fixed --test-threads=1
cargo test --locked --lib outline::segment_ops -- --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused regression tests, independent-review reproducer, segment-operation unit suite and all 29 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Line-conversion endpoint-kind correction

Evidence commit: `Make every canonical line conversion cubic` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Make every canonical line conversion cubic$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

Independent review reopened M03's second checklist item because a zero-control quadratic endpoint is drawn as a line but retained its quadratic role after conversion.
The inserted controls consequently produced two quadratic segments instead of the one cubic promised by line-to-curve conversion.

Every segment accepted as a direct geometric line now changes its endpoint role to cubic after the two controls are inserted.
The direct geometry and type oracle covers open and wraparound closing segments with quadratic endpoint roles and verifies that endpoint identities, names and contour ordering remain stable.
The exact independent-review reproducer passes against the corrected library.
These checks re-establish M03's second checklist item and acceptance.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_line_conversion_sets_quadratic_endpoints_to_cubic -- --exact --test-threads=1
cargo test --locked --test variable_project canonical_line_segments_convert_with_stable_endpoint_identity -- --exact --test-threads=1
rustc --edition=2024 /private/tmp/runebender-migration-review.porJgX/line_conversion_kind.rs -L dependency=target/debug/deps --extern runebender=target/debug/deps/librunebender-b47720e92f7db4a3.rlib --extern norad=target/debug/deps/libnorad-52f3943fd0fc68ef.rlib --extern kurbo=target/debug/deps/libkurbo-78ec2af22253a897.rlib --test -o /private/tmp/runebender-migration-review.porJgX/line_conversion_kind_fixed
/private/tmp/runebender-migration-review.porJgX/line_conversion_kind_fixed --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused direct-oracle tests, independent-review reproducer and all 31 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

## M04 — Migrate topology edits and special outline tools

### Canonical pen topology substep

Evidence commit: `Build pen contours in canonical layers` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Build pen contours in canonical layers$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md` and this log.

`LayerEditDraft` now starts an open contour, appends line or cubic segments and closes it with a line or cubic return segment directly in canonical Babelfont geometry.
Each new contour and point receives a stable document identity when created, and the matching preservation records are inserted atomically with empty source metadata.
Invalid coordinates, missing contours and attempts to close or extend a non-open contour reject the draft before committed state changes.

The integration comparison builds line, cubic and curved-closing segments through both the canonical draft and existing Norad operation and requires identical projected contours.
It checks returned identity order, contour closure, geometry invalidation and rejection atomicity after a second close.
Pen creation and closure are complete within M04's first checklist item.
Point insertion, deletion, reversal, split/join, copy/paste and shape creation remain, so the item stays open.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_pen_builds_closed_contours_with_stable_new_identities -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused pen comparison and all 30 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Canonical shape-creation substep

Evidence commit: `Create canonical rectangle and ellipse contours` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Create canonical rectangle and ellipse contours$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md` and this log.

`LayerEditDraft::add_shape_contour` now creates closed rectangle and cubic ellipse contours directly in canonical geometry.
It retains the existing rounded-coordinate and ellipse-control contract while assigning fresh stable identities to the contour and all new points.
Nonfinite rectangles are rejected before any draft mutation.

The integration comparison requires identical projected rectangle and ellipse contours from the canonical and existing operations.
It verifies point ordering, stable identity ordering and rejected-edit atomicity.
Shape creation is complete within M04's first checklist item.
Point insertion, deletion, reversal, split/join and copy/paste remain, so the item stays open.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_shape_creation_matches_existing_geometry_with_stable_identities -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused shape-creation comparison and all 32 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

Independent review additionally exercised pen extension, closure and shape creation on an imported glyph containing contour, point, anchor, component and glyph metadata.
It verified existing live identities, uniqueness of fresh identities, empty metadata on new points, exact metrics and reflected/skewed component transforms, and complete glyph equality after saving and reopening a disposable UFO.
Its rollback case confirmed that a later topology error discards earlier edits in the same draft and that caught ellipse-coordinate overflow leaves no partial contour or revision change.

```sh
/private/tmp/runebender-migration-review.porJgX/topology_preservation --test-threads=1
```

Both independent preservation tests passed against `dec7966` without repository changes.

### Canonical segment-insertion substep

Evidence commit: `Insert points directly into canonical segments` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Insert points directly into canonical segments$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md` and this log.

`LayerEditDraft::insert_point_on_segment` now subdivides direct stored-endpoint lines, single-control quadratics and cubics without constructing or reconciling a UFO glyph.
It supports open and wraparound closing segments and retains the identities and source metadata of existing controls as their positions move through De Casteljau subdivision.
Inserted controls and on-curve points receive fresh stable identities, and invalid endpoint pairs reject the draft atomically.
Quadratic segments whose start or end is an implied midpoint remain for the implied-topology substep.

The integration comparison requires the same snapped coordinates, point roles, smooth state and complete drawn paths as the existing line, quadratic and cubic insertion operation.
It separately verifies preserved control and endpoint identities and names, fresh-identity uniqueness, closing-segment ordering and cross-contour rejection atomicity.
Stored-endpoint point insertion is complete within M04's first checklist item.
Implied-endpoint insertion, deletion, reversal, split/join and copy/paste remain, so the item stays open.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_segment_insertion_preserves_existing_control_identities -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused insertion comparison and all 33 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Segment-insertion finite-result correction

Evidence commit: `Reject nonfinite canonical subdivisions` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Reject nonfinite canonical subdivisions$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

Independent review found that extreme but finite endpoint coordinates could overflow during interpolation and commit an infinite inserted point.
Every computed line, quadratic and cubic subdivision coordinate is now validated before an existing control moves or a new point is inserted.
The regression catches the error inside the draft and requires an unchanged outcome, snapshot and revision.

The independent review's exact overflow reproducer now passes.
Its companion geometry test also passes all twelve combinations of four closed-cubic storage rotations and three split parameters.

```sh
cargo test --locked --test variable_project canonical_segment_insertion_preserves_existing_control_identities -- --exact --test-threads=1
/private/tmp/runebender-migration-review.porJgX/segment_insertion_review_fixed --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused repository regression, both independent-review tests and all 33 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Canonical implied-quadratic insertion substep

Evidence commit: `Insert points on implied canonical quadratics` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Insert points on implied canonical quadratics$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/document/mod.rs`, `src/outline/segment_ops.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md` and this log.

Canonical segment endpoint identity now belongs to the document model while `outline::segment_ops` retains its compatibility re-export.
`LayerEditDraft::insert_point_on_quadratic_segment` accepts the stored or implied endpoints reported by canonical hit testing and subdivides that exact quadratic directly.
When moving the source control would change an implied start or end midpoint, the operation first materializes that midpoint as a fresh stored on-curve point.
The source control retains its stable identity and metadata, while the inserted point, new right control and materialized endpoints receive fresh identities.
All endpoint and subdivision coordinates are validated before mutation.

The integration oracle covers an open consecutive-control chain and an all-off-curve closed contour.
It requires exact De Casteljau geometry after insertion, verifies stable source-control identities and names, checks every returned identity exists and confirms caught overflow leaves the snapshot and revision unchanged.
Point insertion is now complete within M04's first checklist item.
Deletion, reversal, split/join and copy/paste remain, so the item stays open.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_implied_quadratic_insertion_materializes_stable_endpoints -- --exact --test-threads=1
cargo test --locked --lib outline::segment_ops -- --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused implied-quadratic regression, segment-operation unit suite and all 34 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Implied-quadratic stale-topology correction

Evidence commit: `Reject stale implied quadratic hits` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Reject stale implied quadratic hits$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

Independent review found that adjacency alone did not prove two controls still defined an implied quadratic endpoint.
A retained hit could therefore be applied after the chain's terminating point changed from quadratic to cubic, replacing one current cubic with three quadratics.

Implied endpoint validation now follows the consecutive controls to their terminating on-curve point and requires a quadratic endpoint.
A closed contour made entirely of off-curve points remains an explicitly valid quadratic chain.
The repository regression commits the topology change, catches the stale-hit rejection in a later draft and requires its snapshot and revision to remain unchanged.

The independent review's stale-hit reproducer now passes.
Its companion coverage also passes all 36 combinations of every segment in a three-control open chain, every rotation of a three-control all-off-curve contour and three split parameters, while preserving neighboring geometry.

```sh
cargo test --locked --test variable_project canonical_implied_quadratic_insertion_rejects_stale_segment_identity -- --exact --test-threads=1
cargo test --locked --test variable_project canonical_implied_quadratic_insertion_materializes_stable_endpoints -- --exact --test-threads=1
/private/tmp/runebender-migration-review.porJgX/implied_insertion_review_fixed --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused repository regressions, both independent-review tests and all 35 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Canonical point-deletion substep

Evidence commit: `Delete points directly from canonical contours` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Delete points directly from canonical contours$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md` and this log.

`LayerEditDraft::delete_points` now deletes stable point identities directly from canonical paths.
Deleting an on-curve point removes its incoming controls, while deleting one control removes every control on that segment and reconnects it as a line.
Affected open and closed contours are rebuilt from the surviving canonical nodes and preservation records, and contours with no remaining on-curve point are removed.
Surviving points and contours retain their identities, names, identifiers and object libraries.

The integration comparison follows the existing editor operation through control deletion, on-curve deletion, removal of an all-off-curve contour and deletion of an open contour's move point.
It requires equivalent drawn geometry while separately verifying stronger identity and metadata preservation, open/closed state, contour identity and missing-point rejection atomicity.
Point deletion is complete within M04's first checklist item.
Reversal, split/join and copy/paste remain, so the item stays open.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_point_deletion_preserves_surviving_identities_and_metadata -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused deletion comparison and all 36 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Quadratic point-deletion correction

Evidence commit: `Delete only the selected quadratic segment` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Delete only the selected quadratic segment$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

Independent review showed that the compatibility deletion algorithm did not understand implied quadratic segments.
Deleting one control from an all-off-curve contour removed the entire contour, while deleting a middle control from a longer quadratic chain flattened every neighboring segment.

Canonical deletion now identifies each selected control's actual quadratic segment.
It materializes the segment's implied endpoints, removes only that control and leaves a line between those endpoints while preserving adjacent quadratic controls, identities and metadata.
Shared implied boundaries are materialized once when multiple controls are selected, and selecting every original point removes the contour before any replacement points are created.
Every computed midpoint is checked for finiteness before mutation.

The repository geometry oracle covers a middle control in an open three-control chain and one control in a closed all-off-curve contour.
It verifies the replacement line, both neighboring quadratics, unselected control identities and names, fresh-point metadata and global identity uniqueness.
The existing deletion comparison now removes the all-off-curve contour through full-contour selection, proving materialized points do not survive that selection.
Both exact independent-review reproducers pass against the corrected library.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_quadratic_control_deletion_preserves_neighbor_segments -- --exact --test-threads=1
cargo test --locked --test variable_project canonical_point_deletion_preserves_surviving_identities_and_metadata -- --exact --test-threads=1
/private/tmp/runebender-migration-review.porJgX/quadratic_deletion_review_fixed --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused repository regressions, both independent-review tests and all 37 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Multi-contour deletion atomicity correction

Evidence commit: `Reverse contours in canonical layers` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Reverse contours in canonical layers$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

Independent review found that deletion could mutate an earlier contour before rejecting a nonfinite implied midpoint in a later contour.
If the caller caught that method error inside the draft closure, the earlier mutation could commit.

`LayerEditDraft::delete_points` now applies the complete deletion to a staged draft and replaces the caller's draft only after every affected contour succeeds.
The repository regression and the independent review's exact reproducer select points in two contours, force midpoint overflow in the second and require the caught error to leave the snapshot, revision and identities unchanged.
The review's exhaustive companion coverage also passes all 28 nonempty control-selection subsets across an open three-control chain and every rotation of a three-control all-off-curve contour.

### Canonical contour-reversal substep

Evidence commit: `Reverse contours in canonical layers` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Reverse contours in canonical layers$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

`LayerEditDraft::reverse_contours` now reverses selected or all contours directly in canonical Babelfont geometry.
It reorders existing point objects and transfers each incoming segment role to the correct reversed endpoint without changing point, contour or metadata ownership.
Open contours move the `Move` role to the new start, while closed contours retain their first stored point and restore the exact original storage after a second reversal.
Closed all-off-curve contours reverse without being expanded into explicit endpoints or cubic segments.

The integration oracle covers selected open and closed mixed-segment contours, an unselected all-off-curve contour, empty-selection reversal, stable selection identities, exact source metadata and caught missing-point rejection.
It requires reversed Kurbo geometry for stored-endpoint contours, opposite signed area for the all-off-curve contour and exact canonical snapshot restoration after a second reversal.
Contour reversal is complete within M04's first checklist item.
Split/join and copy/paste remain, so the item stays open.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_point_deletion_is_atomic_across_contours -- --exact --test-threads=1
cargo test --locked --test variable_project canonical_contour_reversal_preserves_identities_metadata_and_storage -- --exact --test-threads=1
/private/tmp/runebender-migration-review.porJgX/deletion_transaction_review_fixed --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused repository regressions, both independent-review tests and all 39 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Canonical contour-start reordering substep

Evidence commit: `Reorder canonical contour starts` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Reorder canonical contour starts$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

`LayerEditDraft::set_contour_start` now rotates a closed contour directly around a selected canonical on-curve point.
Canonical nodes and their preservation records rotate together, so the contour and every point retain identity, names, identifiers and object libraries.
Open contours, off-curve controls and a point that is already first are explicit no-ops.

The integration oracle requires stable contour and point identities, unchanged line and cubic segments, exact source metadata and an unchanged revision for every rejected target kind.
This completes M04's direct contour-reordering path.
Split/join and copy/paste remain, so the first checklist item stays open.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_contour_start_reorders_without_replacing_points -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused reorder regression and all 40 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Symmetric contour-reversal correction

Evidence commit: `Report symmetric reversals as no-ops` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Report symmetric reversals as no-ops$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

Independent review verified exact reversed geometry, metadata ownership and two-reversal snapshot restoration across 29 open, rotated closed and all-off-curve variants.
It also found that reversing a closed contour with two off-curve controls preserves the exact canonical state but returned `true` from the draft method.

Canonical reversal now compares the resulting nodes with their original state and reports a change only when canonical geometry, roles or ordering changed.
The focused repository regression and the review's exact reproducer require `false`, an unchanged edit outcome and a stable revision for the symmetric case.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_contour_reversal_reports_symmetric_noop -- --exact --test-threads=1
/private/tmp/runebender-migration-review.porJgX/reversal_review_fixed --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused no-op regression, both independent-review tests and all 41 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Canonical contour open-close substep

Evidence commit: `Open and close canonical contours` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Open and close canonical contours$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

`LayerEditDraft::toggle_contour_open` now implements the editor's split/join operation directly on canonical topology.
Opening a closed path removes the selected endpoint's incoming controls, rotates the surviving on-curve point and its preservation record to the front, changes it to a move point and marks the path open.
Closing an open path preserves storage order, changes the initial move to a line and marks the path closed.
Contour identity, surviving point identities and exact surviving source metadata persist in both directions.

The integration comparison covers closed cubic and quadratic endpoints plus an open line contour.
It verifies stable identity ordering, source metadata and unchanged revisions when a closed off-curve control or a singleton contour cannot be opened.
Split/join is complete within M04's first checklist item.
Copy/paste remains, so the item stays open.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_contour_open_close_produces_persistable_topology -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused open-close comparison and all 42 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Contour open-close persistence correction

Evidence commit: `Remove orphaned controls when opening contours` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Remove orphaned controls when opening contours$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

Independent review found that the inherited open-contour behavior left the selected endpoint's incoming controls at the end of the new open path.
Saving succeeded, but Norad rejected the resulting UFO during reload as `TrailingOffCurves`; closing that path produced `UnexpectedPointAfterOffCurve` instead.

Canonical opening now counts the selected endpoint's incoming cubic or quadratic controls before mutation and removes their nodes and preservation records after rotation.
It refuses to open a contour when removing the incoming controls would leave fewer than two points.
Closing the resulting path cannot reintroduce orphaned controls.

The revised integration oracle starts from a saved and reloaded valid UFO, opens cubic and quadratic endpoints, verifies the exact surviving identities and metadata, saves and reloads, closes the paths, then saves and reloads again.
Both of the independent review's cubic persistence reproducers pass against the correction.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_contour_open_close_produces_persistable_topology -- --exact --test-threads=1
/private/tmp/runebender-migration-review.porJgX/contour_toggle_persistence_fixed --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused save-reopen regression, both independent-review reproducers and all 42 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Canonical contour copy-paste substep

Evidence commit: `Copy and paste canonical contours` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Copy and paste canonical contours$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/document/mod.rs`, `tests/variable_project.rs`, `docs/babelfont-migration-checklist.md`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

`LayerView::copy_contours` now captures selected contours, or every contour for an empty selection, as owned canonical geometry plus exact source metadata without constructing a UFO glyph.
`LayerEditDraft::paste_contours` appends those contours with fresh stable document identities.
Names and object libraries survive, while every copied object that carried an identifier or library receives a fresh UFO identifier so repeated projections and saved glyphs remain stable and unique.
`LayerEditDraft::duplicate_contours` applies the same identity and metadata policy after validating a requested offset and every resulting coordinate.

The integration oracle copies one selected contour, duplicates another by the editor's twenty-unit offset and requires unchanged original identities, fresh unique contour and point identities, exact names and libraries, fresh UFO identifiers and exact translated coordinates.
It saves and reopens the result and verifies that empty operations and a nonfinite offset leave the snapshot and revision unchanged.
This completes M04's first checklist item covering direct topology creation, insertion, deletion, reversal, open-close, reordering, copy, paste and duplication.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_copy_paste_and_duplicate_assign_fresh_identities -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused copy-paste regression and all 43 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Canonical boolean and overlap-removal substep

Evidence commit: `Replace canonical contours after boolean operations` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Replace canonical contours after boolean operations$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

`LayerEditDraft::boolean_contours` and `LayerEditDraft::remove_overlap` now feed canonical contour geometry directly to Linesweeper and install its paths without a UFO materialization or reconciliation pass.
Topology replacement assigns fresh contour and point identities and deliberately clears names, source identifiers and object libraries because output objects cannot be matched reliably to input objects.
Smooth flags are restored only at retained on-curve positions, matching the existing editor policy.
Components and anchors keep their identities and exact metadata, and components retain their relative order around the replacement contour block.

The integration comparison unions two overlapping rectangles through the canonical and existing operations and requires the same segments up to closed-contour rotation.
It verifies old topology identities disappear, new identities are unique, the explicit empty-metadata policy holds, a retained smooth point stays smooth, components and anchors remain exact, and the saved UFO reopens unchanged.
It also checks insufficient-input boolean rejection and a second canonical overlap-removal replacement.
Boolean and overlap removal are complete within M04's second checklist item.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_boolean_and_overlap_replacement_clear_old_topology_metadata -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused boolean/overlap regression and all 44 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

### Empty boolean-result correction

Evidence commit: `Apply empty canonical boolean results` (the commit containing this correction).
Resolve its exact ID with `git log --format=%H --grep='^Apply empty canonical boolean results$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

Independent review found that the shared contour-replacement helper treated an empty path list as failure even when Linesweeper had completed successfully.
Disjoint intersection and identical-shape difference or XOR therefore kept both original contours and reported an unchanged edit.

Successful replacement now distinguishes empty topology from invalid input or engine failure.
It removes the existing contour block and preservation records, commits the geometry change and retains component and anchor identities plus exact metadata.
This contract also applies to overlap removal and future callers of the shared replacement helper.

The repository regression covers intersection of disjoint rectangles plus difference and XOR of identical rectangles.
Each case requires a changed revision, zero contours, exact components and anchors and a successful save-reopen cycle.
All three exact independent-review reproducers pass.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_boolean_successfully_clears_empty_results -- --exact --test-threads=1
/private/tmp/runebender-migration-review.porJgX/empty_boolean_review_fixed --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused empty-result regression, all three independent-review tests and all 45 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

The next M04 substep moves the knife operation onto canonical geometry and replacement policy.

### Canonical knife and single-cubic-loop correction

Evidence commit: `Cut canonical contours with the knife` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Cut canonical contours with the knife$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/outline/knife.rs`, `src/outline/path/mod.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

Canonical contour views now convert directly to the existing cubic, quadratic and hyperbezier path engine without materializing a UFO glyph.
Knife preview reads those paths directly, and `LayerEditDraft::knife_cut` installs sliced topology atomically.
Missed contours retain their stable contour and point identities plus exact source metadata.
Sliced contours receive fresh document identities and empty names, identifiers and object libraries because the new objects cannot be matched reliably to the source topology.
Quadratic slices remain quadratic, while a sliced hyperbezier becomes explicit cubic geometry under the existing knife contract.
Components and anchors retain their identities and exact metadata.

The regression covers preview intersections, a no-op miss, a cubic split, untouched hyperbezier preservation, quadratic output, component and anchor preservation and save-reopen persistence.
Knife change detection now compares retained engine identities as well as output count, so simultaneous splits and joins cannot be mistaken for a no-op when their contour counts cancel.

Independent review also exposed a valid closed one-cubic loop that the shared boolean replacement helper discarded because it required two on-curve nodes.
Closed output now requires one on-curve node, while open output still requires two.
The repository regression covers overlap removal on the isolated loop and boolean union with a second contour, including area preservation and save-reopen persistence.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_knife_replaces_only_cut_contours_and_preserves_quadratics -- --exact --test-threads=1
cargo test --locked --test variable_project canonical_boolean_replacement_retains_single_cubic_loops -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --lib knife -- --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The two focused integration regressions, all 12 knife unit tests and all 47 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The first knife unit run omitted `RUNEBENDER_TEST_FONTS`, so its only failure was the expected missing-fixture guard; the exact rerun with the configured fixture directory passed all 12 tests.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

The next M04 substep moves cleanup and fit/simplify operations onto canonical geometry and replacement policy.

### Canonical cleanup and curve fitting

Evidence commit: `Clean and fit canonical contours` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Clean and fit canonical contours$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/outline/segment_ops.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

`LayerEditDraft` now performs duplicate-line cleanup, coordinate rounding, path-direction correction, cubic-handle fitting and cubic-extrema insertion directly on canonical contours.
Tidy removes preservation records only for the points it removes.
Rounding, direction correction and fitting retain every contour and point identity plus exact source metadata.
Extrema insertion keeps existing endpoint and control identities and metadata while assigning fresh identities and empty metadata to the new topology.
The insertion sequence is staged on an owned draft so a later failure cannot commit earlier extrema.

Canonical segment enumeration now skips editable hyperbezier contours, matching the existing extrema operation's source-preservation boundary.
Direction correction continues to reverse editable hyperbezier source points without converting them, and the other non-topology cleanup operations retain their source metadata.

The regressions compare ordinary geometry with the existing cleanup algorithms while requiring the stronger canonical metadata contract.
They cover duplicate removal, rounding, nested contour winding, no-op reruns, selection-scoped handle fitting, extrema insertion, stable old identities, fresh new identities and save-reopen persistence.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_cleanup_preserves_surviving_identities_and_metadata -- --exact --test-threads=1
cargo test --locked --test variable_project canonical_fit_and_extremes_match_existing_geometry_with_stable_objects -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --lib outline::cleanup -- --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The two focused canonical regressions, all four cleanup unit tests and all 49 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

The next M04 substep moves embolden, outline effects and component decomposition onto canonical geometry.

### Canonical curve-kind correction and embolden

Evidence commit: `Preserve canonical curve kinds and embolden` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Preserve canonical curve kinds and embolden$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/outline/embolden.rs`, `src/outline/path/cubic.rs`, `src/outline/path/mod.rs`, `src/outline/path/quadratic.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

Independent review of the canonical knife path found that whole-contour curve classification changed mixed cubic/quadratic geometry and collapsed closed all-off-curve quadratic contours.
Canonical and legacy path adapters now choose the mixed-capable path representation whenever a contour contains a cubic endpoint.
Its segment iterator distinguishes one-control quadratics from two-control cubics, including the closing segment, and output conversion restores the matching endpoint type.
Closed all-off-curve contours now materialize their implied midpoint joins for the knife engine.
The regression verifies exact visible segments and analytical preview intersections, then slices both contour forms and requires quadratic output plus save-reopen persistence.

The learned embolden model now accepts canonical layer pairs directly.
`LayerEditDraft::embolden` applies its anisotropic normal offset in place, and `apply_bolden_deltas` consumes model output in the existing outline-reader order.
Both operations reject nonfinite results before mutation and retain point order, roles, stable identities, names, identifiers and object libraries.
The integration regression compares both geometry paths with the existing algorithms and verifies stable identities, metadata and save-reopen persistence.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_knife_preserves_all_off_curve_and_mixed_degree_geometry -- --exact --test-threads=1
cargo test --locked --test variable_project canonical_embolden_preserves_structure_identities_and_metadata -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --lib outline::knife -- --test-threads=1
cargo test --locked --lib outline::embolden -- --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The two focused regressions, all 12 knife unit tests, all seven embolden unit tests and all 51 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

The next M04 substep moves remaining topology-replacing outline effects and component decomposition onto canonical geometry.

### Quadratic-chain correction and canonical component decomposition

Evidence commit: `Resolve canonical components and quadratic chains` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Resolve canonical components and quadratic chains$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/outline/component_ops.rs`, `src/outline/glyph_paths.rs`, `src/outline/path/mod.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

Follow-up knife review found that QCurve endpoints with two or more preceding controls still lost their implied joins in pure quadratic and mixed cubic/quadratic contours.
The canonical and compatibility path adapters now normalize each QCurve control run into explicit midpoint joins for the geometry engine.
This normalization covers open and closed contours, closing runs, rotated starts, all-off-curve contours and mixed-degree contours without changing canonical source topology.
The regression matrix compares visible and engine segments for one, two and three-control quadratic runs with and without neighboring cubic segments.
It also checks analytical preview intersections, slicing, retained quadratic output and save-reopen persistence.

Canonical component decomposition now resolves nested layer shapes directly through caller-supplied canonical layers.
The resolver composes exact component transforms and rounds only at the established decomposition boundary.
`LayerEditDraft::decompose_components` retains existing contours and anchors, removes component objects and pastes resolved contours with fresh document and UFO identities.
Names and object libraries from the base contours remain attached, avoiding duplicate source identifiers when the same base is decomposed more than once.
Missing bases, cycles, excessive depth and nonfinite transformed geometry are explicit errors.
Components whose resolved bases contain no contours are still removed successfully.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_knife_preserves_all_off_curve_and_mixed_degree_geometry -- --exact --test-threads=1
cargo test --locked --test variable_project canonical_component_decomposition_resolves_nested_metadata_safely -- --exact --test-threads=1
cargo test --locked --test variable_project canonical_measurement_inputs_match_legacy_geometry -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --lib outline::knife -- --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --lib component -- --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused quadratic-chain, decomposition and compatibility regressions, all 12 knife unit tests, all eight component-filtered unit tests and all 52 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

The next M04 substep moves the remaining topology-replacing outline effects onto canonical geometry.

### Hyperbezier copy preservation and canonical filter effects

Evidence commit: `Preserve hyper copies and port canonical filters` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Preserve hyper copies and port canonical filters$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/outline/effects.rs`, `tests/variable_project.rs`, `docs/babelfont-field-ownership.md`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

Independent review found that copy, duplicate and component decomposition replaced a hyperbezier contour's identifying UFO string with an ordinary UUID.
Because editable hyper kind was inferred from that string, the copied contour rendered as a polygon despite retaining the same stored points.
Canonical contour preservation now owns the hyperbezier kind explicitly.
Copies and repeated component bases retain that kind while receiving distinct document identities and fresh UFO identifiers that still encode the format convention.
The regression covers copy, duplicate, two identity components referencing one base, curved rendering and save-reopen persistence.

Stroke expansion, offset, extrusion and roughening now consume canonical Kurbo paths through shared geometry helpers.
The canonical transaction preserves untargeted contours, components and anchors exactly, while every replaced contour and point receives a fresh identity and empty source metadata.
Effect results round at the existing command boundary, and successful empty whole-layer results remain valid replacements.
Nonfinite parameters fail before mutation.
The regression compares each filter with the established geometry, checks targeted identity and metadata behavior, verifies no-op atomicity and saves and reopens every result.

Executed evidence:

```sh
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project canonical_hyper_copy_duplicate_and_decomposition_retain_editable_kind -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project canonical_filter_effects_replace_only_targeted_topology -- --exact --test-threads=1
cargo test --locked --lib outline::effects -- --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused hyperbezier and filter regressions, all four effect unit tests and all 54 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

The next M04 substep moves corner application and rounded-corner replacement onto canonical geometry, then addresses explicit hyperbezier conversion and special outline extensions.

### Guarded canonical layer snapshots for the history lane

Evidence commit: `Add guarded canonical layer snapshots` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Add guarded canonical layer snapshots$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/document/variable.rs`, `src/document/project.rs`, `src/document/mod.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md`, the migration checklist and this log.

The parallel M05 history lane requested an opaque per-layer value that contains complete Babelfont geometry and exact preservation extensions without a UFO glyph.
`CanonicalLayerSnapshot` is bound to a stable `GlyphLayerAddress`, supports cloning and exact comparison and keeps its payload private.
`Project::capture_document_layer` reads that value directly from canonical ownership.
`Project::restore_document_layer_if_current` rejects missing, stale and address-mismatched restores before mutation.
A changed restore commits once through the canonical layer boundary, advances the document revision once, reports normal geometry/metrics/metadata invalidation and refreshes the compatibility projection without adding a legacy history record.
An identical replacement is an explicit unchanged outcome.

The focused regression captures before and after states, restores geometry and exact metrics, verifies projection refresh, redoes the change, and proves stale, missing, mismatched and no-op attempts preserve document contents and revisions.
This API is the narrow shared prerequisite for task `01a0ba27-44fc-7243-a672-aacc3e5b05de`.
The current parallel ownership map is recorded in the checklist; metadata and pipeline lanes will return typed modules and request only the central hooks they need.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_layer_snapshot_restore_is_atomic_and_stale_safe -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused restore regression and all 55 variable-project integration tests passed.
Warning-denied Clippy, public API documentation, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

The history lane can now build its per-address before/after stack while this lane resumes the remaining M04 special tools.

### Canonical source group and kerning ownership

Evidence commit: `Own source groups and kerning canonically` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Own source groups and kerning canonically$' -1`.
Affected paths: `src/document/canonical_metadata.rs`, `src/document/font_ops.rs`, `src/document/model/glyph_metadata.rs`, `src/document/variable.rs`, `src/document/project.rs`, `src/document/mod.rs`, `tests/canonical_metadata.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md`, supporting migration documentation and this log.

The M07 lane supplied typed `CanonicalFontMetadata` values and UFO adapters, followed by a required correction that preserves legal duplicate group members exactly and adds strict glyph-metadata boundary operations.
An additional atomic group-rename operation updates every typed pair reference while rejecting kind, side and destination collisions before mutation.
The integration lane promoted the metadata module to the document boundary and stores one canonical group and exact `f64` kerning value per stable `SourceId` beside canonical feature text.
UFO preservation templates clear groups and kerning after import and rehydrate them only when producing a source snapshot or compatibility projection.
`Project::document_font_metadata`, `DocumentSnapshot::font_metadata` and the owned `SourceMetadataEditDraft` provide immutable reads and atomic replacement without exposing a mutable source font.
A committed metadata edit advances the canonical revision once, refreshes the compatibility source and emits the existing source-metadata and compilation invalidation scope.

Executed evidence:

```sh
cargo clippy --locked --test variable_project --test canonical_metadata -- -D warnings
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project canonical_source_metadata_edits_are_atomic_and_round_trip_exactly -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project source_authoring_keeps_identity_and_round_trips_the_designspace -- --exact --test-threads=1
cargo test --locked --test canonical_metadata -- --test-threads=1
cargo fmt --all --check
git diff --check
```

The atomic source-metadata regression, source-reorder regression and all eight canonical metadata tests passed.
Warning-denied targeted Clippy, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.
M07 remains incomplete because canonical glyph metadata, rename/remove transactions, history and application callers have not been integrated.
The next shared prerequisite is a narrow canonical-layer snapshot rebind operation for explicitly authorized glyph renames; the reviewed M05 integration remains unmerged until its embedded snapshot addresses are updated atomically.

### Canonical layer snapshot rename prerequisite

Evidence commit: `Add guarded snapshot rename rebinding` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Add guarded snapshot rename rebinding$' -1`.
Affected paths: `src/document/babelfont.rs` and this log.

Independent review rejected the first M05 Project integration because moving only a history map key left each `CanonicalLayerSnapshot` bound to its old glyph address and preservation name.
`CanonicalLayerSnapshot::rebind_glyph` is a crate-private operation for that explicit rename transaction.
It requires the complete old address and an unchanged stable `LayerId`, then changes only the snapshot glyph address and preserved glyph name.
Stale old addresses and cross-layer requests fail without mutation, so ordinary guarded restore still rejects arbitrary cross-glyph snapshots.

Executed evidence:

```sh
cargo test --locked --lib document::babelfont::tests::layer_snapshot_rebind_requires_the_exact_old_address_and_layer -- --exact --test-threads=1
cargo clippy --locked --lib -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The focused stale-address, cross-layer and successful rename cases passed while retaining exact width and note values.
The M05 integration must still preflight destination collisions and rebind every before/after value across undo and redo atomically before it is accepted.

### Canonical interpolation and compiler metadata inputs

Evidence commits: `Interpolate canonical layer geometry directly`, `Preserve canonical shape order in interpolation`, `Check compiler snapshot quantization`, `Normalize compiler metadata inputs`, `Compile canonical groups and kerning directly` and `Use canonical interpolation inputs`.
Resolve their exact IDs with `git log --format='%H %s' --grep='canonical layer geometry\|canonical shape order\|compiler snapshot quantization\|compiler metadata inputs\|canonical groups and kerning\|canonical interpolation inputs'`.
Affected paths: `src/document/interpolation.rs`, `src/document/compile.rs`, `src/document/compile_metadata.rs`, `src/document/babelfont.rs`, `src/document/project.rs`, `tests/canonical_pipeline.rs`, `tests/variable_compile.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

Interpolation now validates and combines borrowed canonical `LayerView` values without converting every source to a UFO glyph.
It retains the default layer's shape order, closure, point roles, smooth flags, names and stable identities while interpolating exact `f64` advances, coordinates, named anchors and all six affine coefficients.
Current application-facing callers still materialize one final UFO result after interpolation; removing that output adapter remains M06/M08 work.
Kerning interpolation resolves each stable source's `CanonicalFontMetadata` directly.

Compiler snapshot construction now reads canonical groups and typed exact kerning pairs by `SourceId` without consulting compatibility font maps.
UPM, metrics and kerning quantization reject nonfinite and out-of-range values before integer conversion and never modify canonical inputs.
The unsaved pipeline regression changes fractional kerning in the default and non-default sources, verifies one revision and compilation invalidation per edit, checks exact source snapshots, then confirms changed compiled bytes and the expected checked integer kerning in shaped advances.

Executed evidence:

```sh
cargo test --locked --lib document::interpolation::tests -- --test-threads=1
cargo test --locked --lib document::compile_metadata::tests -- --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project interpolation_is_glyph_local_and_independent_of_selected_source -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project interpolation_preserves_precision_and_varies_anchors_and_components -- --exact --test-threads=1
cargo test --locked --test variable_compile -- --test-threads=1
cargo test --locked --test canonical_pipeline -- --test-threads=1
cargo clippy --locked --lib --tests -- -D warnings
cargo fmt --all --check
git diff --check
```

The two canonical interpolation unit tests, two Project interpolation regressions, two compiler metadata unit tests, nine variable compiler tests and the canonical unsaved pipeline regression passed.
Warning-denied library/test Clippy, formatting and diff checks passed.
M08 and M09 remain incomplete because the output presentation path, source structure, canonical glyph metadata and remaining font information still use compatibility adapters.

### Guarded whole-source metadata snapshots

Evidence commit: `Add guarded source metadata snapshots` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Add guarded source metadata snapshots$' -1`.
Affected paths: `src/document/variable.rs`, `src/document/project.rs`, `src/document/mod.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

`CanonicalSourceMetadataSnapshot` is an opaque cloneable value containing canonical feature text plus `CanonicalFontMetadata` for the complete source set keyed by stable `SourceId`.
It does not contain UFO maps or display ordering.
`Project::capture_document_source_metadata` records the whole scope, and `restore_document_source_metadata_if_current` validates the live and replacement source sets before comparing expected values.
A stale value or missing, added or mismatched source identity leaves contents, projections and revision unchanged.
A real replacement installs every source value atomically, advances the revision once, refreshes only affected compatibility projections and reports ordinary metadata plus compilation invalidation.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_source_metadata_snapshot_restore_is_atomic_and_order_independent -- --exact --test-threads=1
cargo clippy --locked --test variable_project --lib -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The regression passed changed and unchanged replay after source reorder, stale rejection and both expected-live and replacement source-set mismatch cases.
All failed cases retained the complete canonical document and exact revision.
This supplies the atomic Project boundary required by `TransactionHistory<CanonicalSourceMetadataSnapshot>`; document-owned placement and application caller migration remain M05/M06 work.

### Parallel ownership update after history integration

Coordination checkpoint: `0b06ad2` plus the canonical HOI interpolation integration.

The M05 history lane now owns `src/document/sources.rs` in addition to `history.rs` and its focused tests so canonical source/layer structural capture and replay have one writer.
The new M06 application lane owns `src/application/`, editor sessions and commands, `FontModel`, workspace and view/platform synchronization after its task starts.
This integration lane retains `project.rs`, `variable.rs`, `babelfont.rs`, `source.rs`, M04 special outline tools and extensions, shared module wiring, integration tests and central documentation.
It will provide narrow shared hooks, place canonical histories in Project-owned state and integrate reviewed lane commits without duplicating history in frontends.
API foundations do not complete M05 or M06; structural replay, Project-owned history routing and migrated application callers still require executed acceptance evidence.

### Canonical Designspace source authoring and structural history

Evidence commits: `Add canonical Designspace structure model`, `Own canonical Designspace in Project`, `Guard canonical Designspace source transactions` and `Route source history through canonical Designspace`.
Resolve their exact IDs with `git log --format='%H %s' --grep='canonical Designspace structure model\|Own canonical Designspace in Project\|Guard canonical Designspace source transactions\|Route source history through canonical Designspace'`.
Affected paths: `src/document/model/designspace.rs`, `src/document/model/mod.rs`, `src/document/project.rs`, `src/document/variable.rs`, `src/document/sources.rs`, `tests/canonical_designspace.rs`, `tests/canonical_source_history.rs` and this log.

`CanonicalDesignspace` now owns axes, exact mapped locations, stable full and sparse source identities, exact serialized source order, instances, rule processing, ordered rules and supported root metadata.
Its checked codec rejects unsupported or lossy Designspace structures before mutation, and Project load assigns stable source and default-layer identities instead of deriving them from mutable display indices.
`Project` retains that canonical value in `VariableData`, exposes immutable compiler structure, and supplies a guarded clone-and-install boundary for source transactions.
The structural snapshot contains canonical Designspace state but deliberately excludes active selection, history stacks, revision, dirty flags and allocator state.

Full-source add, remove, move and descriptor update now edit `CanonicalDesignspace` first and materialize Norad only at the compatibility serialization boundary.
Sparse source entries retain their exact positions when full sources move, and promoting a sparse location removes only its interpolation descriptor while retaining the auxiliary UFO layer.
`SourceFrame` now contains only the opaque canonical structural snapshot plus active `SourceId`; restore rebuilds names, normalized locations, sparse projections and source paths from stable canonical descriptors.
Project load no longer discards canonical Designspace ownership after `from_designspace` returns.

Executed evidence:

```sh
cargo test --locked --test canonical_designspace -- --test-threads=1
cargo test --locked --test canonical_source_history -- --test-threads=1
cargo test --locked --test canonical_history -- --test-threads=1
cargo test --locked --lib history -- --test-threads=1
cargo test --locked --test variable_project source_authoring_keeps_identity_and_round_trips_the_designspace -- --test-threads=1
cargo test --locked --test variable_project full_source_can_replace_intermediate_participation_without_losing_the_layer -- --test-threads=1
cargo test --locked --test variable_project source_undo_refuses_to_overwrite_later_edits_and_layer_operations_preserve_other_glyphs -- --test-threads=1
cargo test --locked --bin runebender application::platform::host::tests::source_commands_preserve_glyph_history_across_removal_and_reorder -- --test-threads=1
cargo clippy --locked --lib --tests -- -D warnings
cargo fmt --all --check
git diff --check
```

The canonical Designspace suite passed 5 tests, structural source history passed 4, canonical history passed 13 and filtered library history passed 10.
All three Project source-authoring regressions and the application host regression passed.
Warning-denied library/test Clippy, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

M05 remains active until the M06 application cutover retires `Master.history`, `VariableData.histories` and their compatibility undo callers.
Those compatibility stores are parked by stable `SourceId` and `LayerId` across structural removal and restore so the transition does not lose existing undo or redo piles.

### Canonical sparse structure, headless rendering and caller cutover

Evidence commits: `5e9d2f1`, `278d173`, `36535d3`, `1b5234e`, `9c51a7b`, `94eb639` and `efa4362`.

Canonical Designspace edits now register sparse sources and remove instances atomically, and structural replay refreshes instance projections from stable canonical state.
The brace-layer interpolation, Designspace instance round-trip and curved trajectory regressions pass through those transactions.

Headless proof and glyph analysis now read canonical Project layers and render components through the typed special-outline renderer.
The component-selection renderer follows exact transforms, auxiliary-layer fallback, smart poles, nested metaballs, recursion limits and explicit cycle failures while preserving structural decomposition semantics for edit operations.
Curve analysis exposes a borrowed canonical-layer entry point for callers that do not need a UFO boundary value.

Application composition commands publish canonical proposal plans, root live glyph inspection reads canonical Project state, and the CLI agent reader loads Project plus an exact stable source before producing the existing JSON and revision contract.
Experiment-branch inspection intentionally retains its isolated snapshot until the version boundary is removed during M13.

Executed integration evidence:

```sh
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources CARGO_BUILD_JOBS=2 cargo test --locked --lib -- --test-threads=1
cargo test --locked --lib document::live_socket::tests::round_trip_requires_editor_dispatch_and_drop_removes_endpoint -- --exact --nocapture
cargo check --workspace --all-targets --locked
cargo clippy --locked --lib -- -D warnings
cargo fmt --all --check
git diff --check
```

The integrated library contains 445 tests.
The sandbox run passed 444 and failed only when the Unix live-socket test could not create its endpoint; that exact test passed outside the sandbox.
The all-target workspace check, warning-denied library Clippy, formatting and whitespace checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.
M06 remains open for Session, clipboard, canvas, history and panel callers, and M12 remains open for shared constructors and source-boundary consolidation.

### Intermediate native testing checkpoint

On 2026-09-19 the user authorized promoting the immutable integration checkpoint `bb2180773ffd17c92db7f9870fea44dd4ff231c5` for hands-on native Xilem testing.
The coordinating task fast-forwarded the primary checkout and verified local `main` and GitHub `origin/main` at that exact commit.
The previous main commit `314aa3235c372ed8d5fef7a2cddb8be3a07ad1da` is retained by local rollback branch `codex/pre-babelfont-testing-20260919`.

The exact checkpoint passed all 451 library tests, including the Unix live-socket test outside the sandbox, plus 326 binary and integration tests with four documented binary ignores.
Warning-denied native all-target Clippy, formatting, copyright, rustdoc and doctests, native release build, advisory review, optimized browser build and warning-denied WASM Clippy passed.
The browser interaction and export matrix passed at device-pixel ratios 1, 2 and 1.25, and Gray and Light native/browser captures were inspected.
A clean local clone reused dependency caches without source or path-patch changes.
All 1,744 original Virtua Grotesk source files were SHA-256 verified unchanged after the proof.
The checkpoint logs and native/browser result manifests are stored under `/private/tmp/runebender-main-checkpoint-fr6dvh0y-evidence`.
The disposable testing copy and pinned native executable and launcher are stored under `/private/tmp/runebender-virtua-test-bb21807`.

This promotion is an intermediate testing checkpoint, not migration completion.
Later isolated commits `fb629a0`, `352e513` and `7fd409b` respectively move browser construction, stale compiler-metadata clearing, and Session point-selection/clipboard state further toward the canonical architecture; they are not part of the promoted checkpoint.
M06 Session/canvas/gesture/history work, remaining M12 native callers, M13 compatibility removal and M14 final proof remain required before the migration can be marked complete.
The pinned test copy and executable are reserved for user testing and must not be modified by migration work.

### Canonical editor objects and transaction compatibility bridge

Evidence commits: `98874fe` and `ed4ad11`.
Affected paths: `src/application/editor/session.rs`, `src/document/babelfont.rs`, `src/document/project.rs`, `tests/variable_project.rs` and application tests changed by the editor lane.

The editor now retains stable canonical component and anchor identities, and its pointer gestures mutate owned canonical layer transactions.
The remaining legacy outline algorithms can receive a detached, short-lived UFO glyph from `CanonicalLayerTransaction` and reconcile their result back into the same canonical draft.
That bridge is transitional codec access rather than persistent editor state.
It retains matched contour, point, component and anchor identities, reports identical results as unchanged, and validates numeric geometry plus typed smart-component metadata before changing the draft.
This unblocks removal of Session's persistent UFO glyph and mixed Master history without rewriting every remaining outline algorithm in the same change.
It is not an accepted M13 boundary: every production caller must be inventoried and replaced with a direct canonical operation, then both public bridge methods and their reconciliation wrapper must be deleted before final acceptance.

Executed integration evidence:

```sh
cargo test --locked --test variable_project canonical_layer_transaction_reconciles_detached_ufo_algorithms -- --exact --test-threads=1
cargo clippy --lib --tests --locked -- -D warnings
cargo fmt --all --check
git diff --check
```

The focused reconciliation contract passed no-op, changed geometry and metrics, stable point identity, rejected nonfinite component geometry and atomic draft preservation.
Warning-denied library/test Clippy, formatting and whitespace checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.
M06 remains active until the persistent Session glyph, `HistoryOp`, Master history and remaining canvas/panel compatibility readers are gone.

### Staged UFO and Designspace persistence

Evidence commit: `f33e193`.
Affected paths: `src/document/filesystem.rs`, `src/document/project.rs`, `src/document/source.rs`, architecture and filesystem-adapter documentation, plus focused tests.

Project load now decodes every complete source before document construction, and save stages every dirty UFO and Designspace artifact before replacing any live destination.
The filesystem preservation record retains source destinations, UFO metainfo, layer order and directories, exact `contents.plist` GLIF paths, images, data and otherwise unrecognized regular files.
Export rejects symlinks and overlapping destinations, validates the complete staged set, and uses rollback backups if publication fails.
The exact custom GLIF filename regression closes a concrete preservation gap found during the adapter audit.

Executed integration evidence:

```sh
cargo test --locked document::filesystem::tests:: -- --test-threads=1
```

Both focused tests passed: one preserves custom GLIF paths and opaque payloads across a canonical edit/save, and one proves an invalid later source prevents replacement of every destination.
M12 remains active for CLI and imported-format construction, native New Font and Save As, feature-include relocation, watched-source acceptance and the final serialization allowlist.

### Direct canonical hyperbezier conversion

Evidence commit: `Convert canonical hyperbeziers directly` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Convert canonical hyperbeziers directly$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md` and this log.

`LayerEditDraft::convert_hyper_to_cubic` now solves selected editable hyperbezier contours from their canonical views and installs explicit cubic topology without a UFO glyph projection.
An empty selection converts every hyperbezier contour.
Converted contours and points receive fresh stable identities and empty source metadata, matching the existing topology-replacement contract, while ordinary and unselected hyperbezier contours remain exact.
The outline-path construction shared with the canonical knife replacement was factored into one checked helper so cubic, quadratic and hyperbezier output retain the same finite-coordinate and point-role validation.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_hyper_conversion_replaces_only_selected_topology -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --lib outline::knife -- --test-threads=1
cargo test --locked --test variable_project canonical_knife_replaces_only_cut_contours_and_preserves_quadratics -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --lib --tests --locked -- -D warnings
cargo fmt --all --check
git diff --check
```

The focused hyperbezier conversion regression and all 65 variable-project tests passed selected-only conversion, fresh replacement identities and metadata, exact retention of the unselected contour, invalid-selection atomicity, convert-all behavior and save/reopen persistence.
All 12 knife tests and the canonical knife integration regression passed after sharing the replacement helper.
Warning-denied library/test Clippy passed, and the unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.
The M06 caller still needs to invoke this direct draft operation before M04's explicit-conversion checkbox can close.

### Direct canonical mask baking

Evidence commit: `Bake masks from canonical contours` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Bake masks from canonical contours$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

`LayerEditDraft::bake_masks` now decodes valid mask contour indices at the persisted-key boundary, subtracts those canonical paths from the unmasked paths, and replaces the result without materializing or reconciling a UFO glyph.
The mask key is removed only after Linesweeper returns a valid result and canonical topology replacement succeeds.
Replacement contours and points receive fresh identities and empty source metadata under the established boolean-result policy.
Invalid or out-of-range entries cannot select nonexistent contours, an empty or all-mask selection is a no-op, and a second bake records no change.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_mask_baking_replaces_topology_and_clears_the_boundary_key -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --lib --tests --locked -- -D warnings
cargo fmt --all --check
git diff --check
```

The focused bake regression and all 66 variable-project tests passed established outline geometry, fresh replacement identities and metadata, key removal, no-op replay and save/reopen persistence.
Warning-denied library/test Clippy passed, and the unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.
Mask storage remains an opaque persisted-key boundary rather than a typed contour-identity extension, so M04's special-source ownership item remains open until that field moves and the M06 command uses this direct draft operation.

### Guarded source image-resource insertion

Evidence commit: `Install source images without mutable fonts` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Install source images without mutable fonts$' -1`.
Affected paths: `src/document/variable.rs`, `src/document/project.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

`Project::install_document_source_image` validates a stable source, flat relative image path and PNG payload before changing document state.
It installs or replaces the resource in the source-format preservation store, refreshes the compatibility projection needed during M13, marks the source dirty and advances the document revision exactly once.
An identical resource is a no-op, while invalid paths and payloads preserve the document snapshot, revision and dirty state.
The application can pair this operation with `LayerEditDraft::set_image` without obtaining a mutable source font.

Executed evidence:

```sh
cargo test --locked --test variable_project source_image_install_is_validated_and_saved_without_mutable_font_access -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --lib --tests --locked -- -D warnings
cargo fmt --all --check
git diff --check
```

The focused resource regression and all 68 variable-project tests passed valid insert, identical no-op, invalid path and payload rejection, canonical glyph attachment, staged save and reload with exact image bytes.
Warning-denied library/test Clippy passed, and the unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.
The long-lived UFO template remains tracked M13 debt; this operation narrows application mutation now and must be redirected to typed `SourceFormatData` when templates are removed.

### Imported-format publication safety

Evidence commit: `Make imported font publication explicit` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Make imported font publication explicit$' -1`.

Glyphs conversion now rejects every reported conversion warning before writing output.
Generated relative paths and duplicates are checked, the complete file set is staged and reloaded through the UFO or Designspace adapter, and publication chooses an unused sibling directory instead of replacing an earlier conversion.

Compiled TTF and OTF imports now enter through the common canonical single-source constructor.
Their documented import boundary remains unchanged: names, metrics, encodings and outlines are imported, while kerning and features are not decompiled.
An occupied sibling UFO path is preserved and the unsaved imported document receives a collision-free destination.

Filesystem export preflight now compares normalized destination keys.
It resolves existing path aliases without requiring the final destinations to exist, so `masters/../Regular.ufo`, `Regular.ufo` and an existing symlink spelling cannot select overlapping publication targets.

The focused filesystem suite passed eight tests and the binary constructor regression passed.
The remaining Python Babelfont multi-source construction path still assembles compatibility masters at its format boundary.
Save As feature-include relocation also remains open and is not claimed by this substep.

### Canonical Python Babelfont construction

Evidence commit: `Construct Python Babelfont imports canonically` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Construct Python Babelfont imports canonically$' -1`.

The Python Babelfont adapter still decodes its documented package fields into transient UFO boundary values and retains its explicit rejection rules.
Single-source imports now use the common canonical UFO constructor.
Multi-source imports validate every decoded UFO and install all source and auxiliary layers into one canonical document before compatibility Masters are derived for transitional callers.
The canonical Designspace is then installed with the same mapped axes, full and intermediate sources and instances as before.

The direct constructor regression verifies that malformed standard glyph metadata returns an error before compatibility projection.
The existing package tests continue to cover exact fractional layer widths, interpolation, auxiliary and intermediate layers, mapped axes, instances, save and reload, unsupported metadata and byte-for-byte preservation of the original package.

This substep does not expand the Python package format contract or add Rust Babelfont JSON support.
Save As feature-include relocation remains a separate open M12 gap.

### Direct canonical metaball collapse

Evidence commit: `Collapse canonical metaballs directly` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Collapse canonical metaballs directly$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

`LayerEditDraft::collapse_metaballs` samples and fits selected or all typed live metaball groups without materializing a UFO glyph.
The complete operation runs against a staged canonical draft, so an unknown group, empty sampled outline or invalid replacement contour preserves both geometry and live source data.
Generated cubic contours receive fresh stable identities and empty source metadata, while existing contours, components and anchors remain exact.
Successful conversion removes only the selected groups and inserts generated paths before components in the established glyph paint order.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_metaball_collapse_is_selected_atomic_and_persistable -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --lib outline::metaballs -- --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --lib --tests --locked -- -D warnings
cargo fmt --all --check
git diff --check
```

The focused regression compares selected conversion with the established UFO-boundary implementation, checks draft and Project atomicity on an unknown group, converts the remaining groups and verifies save/reopen persistence.
The metaball unit suite and full variable-project suite passed, as did warning-denied library/test Clippy.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.
The M06 application callers still need to invoke this direct draft operation before the explicit-conversion portion of M04 can close.

### Direct canonical hyperbezier drawing

Evidence commit: `Draw canonical hyperbeziers directly` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Draw canonical hyperbeziers directly$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, `ARCHITECTURE.md`, `CHANGELOG.md` and this log.

`LayerEditDraft` now starts typed hyperbezier contours, appends smooth or corner on-curve points and closes them through stable `ContourId` and `PointId` values.
The editable kind remains canonical typed preservation data, paired with a fresh unique hyperbezier identifier retained for the UFO compatibility boundary.
Ordinary contours and already closed contours are rejected before mutation, and nonfinite points cannot enter the draft.

Executed evidence:

```sh
cargo test --locked --test variable_project canonical_hyper_pen_uses_stable_typed_contours -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --lib --tests --locked -- -D warnings
cargo fmt --all --check
git diff --check
```

The focused regression matches the established hyper-pen point semantics, verifies stable typed identities, rejects an ordinary contour atomically and saves and reloads the editable hyper kind.
The full variable-project suite and warning-denied library/test Clippy passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.
The M06 caller still needs to consume these draft operations before the corresponding temporary bridge paths can be deleted.

### Project-owned Save As retargeting

Evidence commit: `Retarget Save As through Project` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Retarget Save As through Project$' -1`.

`Project::save_as` now plans a complete copy from canonical source snapshots, stages every UFO, optional Designspace and external relative feature include, and publishes the set before changing any live source path.
The native command no longer rewrites Designspace metadata or source destinations through `SourcesEdit`.
Successful publication installs the checked canonical Designspace filenames and retargets the Project; any validation or publication error leaves the original Project and source tree unchanged.

Save As preserves relative include spelling while relocating recursively resolved include files within the selected copy directory.
Shared dependencies deduplicate only when their resolved source and bytes agree.
Existing destinations, normalized aliases, overlapping outputs, collisions and include paths that would escape the selected directory are refused before Project mutation.

The focused Project tests cover a single UFO with a nested relative include, an occupied dependency destination, unchanged originals, a two-source Designspace sharing one include and reopening the published copies.
The native regressions cover Save As and reopen for both single-UFO and Designspace workspaces with relative includes.

Executed evidence:

```sh
cargo test --locked --lib document::project::save_as::tests -- --test-threads=1
cargo test --locked --lib document::filesystem::tests -- --test-threads=1
cargo test --locked --test compiler_include_path -- --test-threads=1
cargo test --locked --test variable_project -- --test-threads=1
cargo test --locked --bin runebender -- --test-threads=1
cargo clippy --locked --lib --tests -- -D warnings
cargo fmt --all --check
bash .github/copyright.sh
git diff --check
```

The three Project Save As tests, eight filesystem tests, two compiler-include tests and all 65 variable-project tests passed.
The complete native binary suite passed 168 tests with its four documented model or fixture ignores.
Warning-denied library/test Clippy, formatting, copyright and whitespace checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

M12 still has four explicit close-out gaps.
The native New Font paths still serialize a temporary Norad font before reopening it instead of constructing and publishing the common canonical Project directly.
The legacy CLI `open_master` and `save_master` boundary still loads and saves `Master` directly for UFO editing commands.
External include files copied by Save As are not yet inputs to the native watcher fingerprint, so the existing watched-source conflict coverage remains limited to UFO and Designspace roots.
The exact codec and preservation allowlist must be consolidated before M13 removes the compatibility adapters.

### Glyph-free source-format preservation

Evidence commit: `Replace UFO templates with source format data` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Replace UFO templates with source format data$' -1`.
Affected paths: `src/document/source_format.rs`, `src/document/variable.rs`, its structural transaction modules, architecture and migration documentation.

`VariableData` and `CanonicalSourceStructureSnapshot` no longer contain `BTreeMap<SourceId, norad::Font>` persistence templates.
They retain explicit `SourceFormatData` records containing glyph-free layer order, names, exact paths, layer libs and colors, UFO metainfo, residual font-info and lib values, images and data.
Features, groups, kerning, canonical font information, glyph metadata and geometry remain single-owned by their typed canonical stores.
Source image insertion, auxiliary-layer structure, proposals, interpolated-source construction and structural restore now update the explicit record.
A transient glyph-free UFO codec value is reconstructed only when an existing source-snapshot boundary requests it, then populated from canonical layers and metadata.

Executed evidence:

```sh
cargo test --locked --lib document::source_format::tests::record_is_glyph_free_and_preserves_exact_format_structure -- --exact --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo test --locked --lib document::filesystem::tests -- --test-threads=1
cargo clippy --lib --tests --locked -- -D warnings
cargo fmt --all --check
git diff --check
```

The focused source-format invariant test, all 70 variable-project tests and all eight staged-filesystem tests passed, including exact layer structure, metadata/resource persistence, source image insertion, auxiliary layers, interpolated sources, custom GLIF paths and opaque filesystem payloads.
Warning-denied library/test Clippy, formatting and whitespace checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.
M13 still must replace the `Master` projection with a source shell, eliminate mutable source guards and delete remaining source-snapshot consumers before the Norad boundary can be restricted to codecs.

### Atomic semantic glyph marks

Evidence commit: `Edit semantic glyph marks atomically` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Edit semantic glyph marks atomically$' -1`.
Affected paths: `src/document/babelfont.rs`, `src/document/model/glyph_metadata.rs`, `src/ui/theme.rs`, `tests/canonical_layer_metadata.rs`, architecture and migration documentation.

`LayerEditDraft::set_mark` now sets or clears `public.markColor` and `com.runebender.markLabel` as one validated canonical metadata edit.
It rejects a missing half, an empty label or a nonfinite/out-of-range typed color before mutation.
An equivalent mark retains the exact valid source spelling of the public color and commits as unchanged, while clear removes both keys.
The label key now has one canonical constant beside the public-color key, and the remaining UFO theme adapter consumes those constants rather than owning a duplicate spelling.

Executed evidence:

```sh
cargo test --locked --test canonical_layer_metadata
cargo clippy --locked --lib --tests -- -D warnings
cargo fmt --all --check
git diff --check
```

Both canonical layer-metadata tests passed, including atomic rejection, semantic no-op preservation, paired update and clear behavior, unrelated-lib preservation and save/reopen persistence.
Warning-denied library/test Clippy, formatting and whitespace checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.
The M06 foreground and overview callers still need to pass the theme's frozen label/color pair to this draft operation before their whole-glyph compatibility paths can be deleted.

### Direct imported-contour boundaries

Evidence commit: `Import contours without whole-glyph reconciliation` (the commit containing this substep).
Resolve its exact ID with `git log --format=%H --grep='^Import contours without whole-glyph reconciliation$' -1`.
Affected paths: `src/document/babelfont.rs`, `tests/variable_project.rs`, architecture and migration documentation.

`LayerEditDraft::append_imported_contours` and `replace_imported_contours` decode an explicit `norad::Contour` boundary payload directly into canonical paths.
Every inserted contour and point receives a fresh stable document identity, while source names, identifiers and object libraries retain their exact boundary values.
The complete payload is checked for nonfinite geometry before mutation.
Replacement changes contours only, preserving components, anchors, exact advances and layer metadata, and an exact no-op retains the existing stable identities.

Executed evidence:

```sh
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project -- --test-threads=1
cargo clippy --locked --lib --tests -- -D warnings
cargo fmt --all --check
git diff --check
```

The full variable-project suite passed 71 tests, including explicit append and replace proof for invalid-input atomicity, no-op identity preservation, source metadata, unrelated components/anchors/advance and save/reopen persistence.
Warning-denied library/test Clippy, formatting and whitespace checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.
The M06 Trace Image and SVG callers still need to consume these narrow methods; background swap also needs its Project-owned auxiliary-layer transaction before the remaining whole-glyph bridge can be deleted.
