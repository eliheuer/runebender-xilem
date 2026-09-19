# Babelfont migration progress

Status: **IN PROGRESS — M00 complete; M01 active**.
The definition of complete and milestone dependencies remain in [the checklist](babelfont-migration-checklist.md).
No model ownership has changed yet.

## Continuation checkout

- Worktree: `/Users/eli/.codex/worktrees/790d/runebender-xilem`.
- Branch: `codex/babelfont-migration`.
- Baseline: `624879a1c447d3e9f012c34f4b5cb091bb0df6cb`.
- Setup verified that clean starting commit `5c37be7717e780aac3fcb369b033148f57026ef9` was an ancestor, created the isolated branch, and fast-forwarded it to the exact baseline.
- The first implementation run started clean; main and the originating task's branch were not changed.
- The checklist contains 15 milestones and 76 acceptance steps.

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
