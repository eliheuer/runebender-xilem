# Babelfont canonical history lane

Status: **ACTIVE — canonical layer and source-metadata history implemented; document-owned storage, structural history and caller migration remain**.

This lane owns `src/document/history.rs`, focused canonical-history tests and this progress record.
The lead task retains canonical model storage, Project/source/application wiring and the central migration documents.
Parallel authorization supersedes only the checklist's single-writer rule; M05 acceptance remains unchanged.

## Baseline

The clean isolated checkout was fast-forwarded from `314aa3235c372ed8d5fef7a2cddb8be3a07ad1da` to the shared migration commit `fa6caca673fb28827d29e69fff8f7cf4e5b70183`.
The metadata continuation branch `codex/babelfont-history-metadata` starts from lead commit `bae2bb19665ad0c48e537e5a984c46050768bcba`, which includes the atomic source-metadata boundary from `bfdc4a5`.

## Completed slice

Implementation commit: `3342b0fd4f7ad45ba146c5b6eac61547ee3ba5ea` (`Add canonical per-layer history`).

`CanonicalHistory` stores exact before-and-after values by `GlyphLayerAddress`, so default and auxiliary layers have independent stacks while retaining stable `SourceId` and `LayerId` references.
It records no-op edits as nothing, invalidates redo only after a real edit on the same layer, coalesces a drag without losing its original state and removes a coalesced step that returns to its origin.
Replay checks the expected live state before invoking the document restore transaction and moves the stack only after that transaction succeeds.
Stale and rejected replay therefore leave history unchanged.
Rename moves every layer stack atomically and rejects a destination-name collision without partial movement.

Focused tests cover exact geometry, fractional metrics, ordinary metadata and an opaque source-extension value in the same history state.
They also cover default/auxiliary isolation, drag grouping, explicit no-op discard, stale rejection, rejected restore, redo invalidation, rename/collision behavior, preservation of an unrelated layer and source removal/restoration with the same stable identity followed by an older undo and redo.

Executed checks:

```sh
cargo fmt --all --check
git diff --check
CARGO_BUILD_JOBS=2 cargo test --locked --test canonical_history -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --locked --lib document::history:: -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo clippy --locked --test canonical_history -- -D warnings
```

The canonical suite passed 7 tests and the unchanged legacy history suite passed 6 tests.
Warning-denied Clippy passed for the canonical-history integration target.
The existing `block v0.1.6` future-incompatibility notice remains a dependency notice.

## Project integration slice

Local prerequisite commit: `a978f9f` (`Adopt guarded canonical layer snapshots`).
This is the exact four-source-file patch from the lead's committed `d840ec5` prerequisite, imported without its unrelated M04 parent or central documentation changes.

Implementation commit: `169ccc3` (`Integrate canonical document history`).

`DocumentHistory` specializes the reusable stack for `CanonicalLayerSnapshot` and the Project capture/guarded-restore boundary.
Callers capture a before-state, commit a direct document edit and record the completed live state.
Failed and no-op edits record nothing and retain redo.
Undo and redo repeat the expected-state comparison inside the Project restore transaction, advance the revision once on a change and leave both document and stack untouched on stale replay.

Two additional Project-level regressions create a real canonical document and verify exact geometry, fractional width/height, note and opaque lib values through edit, undo and redo.
They also verify one revision advance per replay and rejection of a later same-layer edit without moving the history stack.
The transitional `edit_layer` call in one test exists only to create a metadata-different after-state until M07's direct metadata draft lands; history capture and replay remain canonical and contain no Norad value.

Executed checks after integration:

```sh
cargo fmt --all --check
git diff --check
CARGO_BUILD_JOBS=2 cargo test --locked --test canonical_history -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --locked --lib document::history:: -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo clippy --locked --tests -- -D warnings
CARGO_BUILD_JOBS=2 cargo doc --locked --no-deps
```

The expanded canonical suite passed 9 tests and the legacy history suite passed 6 tests.
Warning-denied Clippy passed across test targets and public API documentation built successfully.

## Source-metadata transaction slice

Implementation commit: `95ccc5e` (`Add canonical source metadata history`).

`SourceMetadataHistory` specializes `TransactionHistory` for the complete `CanonicalSourceMetadataSnapshot`.
Capture and replay use the lead's stable-`SourceId`, display-order-independent Project boundary, so one transaction may cover several sources without serial application or partial restoration.
Completed edits record exact before-and-after metadata sets, no-op edits retain redo, coalescing retains the original before-state and stale or rejected replay leaves both document and stack unchanged.
Successful undo and redo refresh compatibility projections through Project and advance the canonical revision exactly once.

Two focused Project regressions cover exact feature-text undo and redo, one revision advance per replay, no-op redo retention and rejection of a later metadata edit without stack movement.
The boundary's existing Project regression covers multi-source atomic restoration, source reordering with the same stable identities, stale values and source-set conflicts.

Executed checks:

```sh
cargo fmt --all --check
git diff --check
CARGO_BUILD_JOBS=2 cargo test --locked --test canonical_history -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --locked --lib document::history:: -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
```

The canonical suite passed 13 tests and the history unit suite passed 8 tests.
Warning-denied Clippy and public API documentation passed.
The existing `block v0.1.6` future-incompatibility notice remains a dependency notice.

## Remaining integration dependency

The lead supplied and integrated the requested layer capture/restore API in `d840ec5`.
The lead still owns placement of `DocumentHistory` in canonical Project storage and migration of shared Project/source/application call sites.
The lead supplied the atomic whole-source metadata boundary in `bfdc4a5`; `95ccc5e` now supplies its concrete history wrapper.
Source-structural history still needs an exact canonical structural snapshot/restore boundary coordinated with the lead.

## Next concrete step

Integrate `95ccc5e` after `bfdc4a5` and add the document-owned history fields and caller migration in lead-owned files.
Coordinate source-structural history and application call sites with the lead rather than editing its owned files here.
The legacy Norad `EditHistory` remains intentionally compiling for staged callers and is not counted as migrated or complete.
M05 remains open until integrated acceptance passes and the temporary history is removed from migrated callers.
