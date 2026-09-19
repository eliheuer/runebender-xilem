# Babelfont canonical history lane

Status: **ACTIVE — reusable per-layer history implemented; document integration waiting on the shared capture/restore API**.

This lane owns `src/document/history.rs`, focused canonical-history tests and this progress record.
The lead task retains canonical model storage, Project/source/application wiring and the central migration documents.
Parallel authorization supersedes only the checklist's single-writer rule; M05 acceptance remains unchanged.

## Baseline

The clean isolated checkout was fast-forwarded from `314aa3235c372ed8d5fef7a2cddb8be3a07ad1da` to the shared migration commit `fa6caca673fb28827d29e69fff8f7cf4e5b70183`.
Work is on `codex/babelfont-history-c903`.

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

## Integration dependency

The lead acknowledged and is implementing the requested minimal API.
History needs an opaque canonical layer snapshot containing Babelfont layer values plus exact preservation extensions, and a Project operation that atomically replaces a layer only when its current snapshot matches the expected state.
The restore operation must distinguish stale, missing and changed outcomes; stale or rejected restore must not change document contents, revision, compatibility projection or history; changed restore must advance the revision once and refresh derived state.
The lead will send the committed prerequisite when it lands.

## Next concrete step

After the committed capture/restore API arrives, integrate `CanonicalHistory<CanonicalLayerSnapshot>` into the document-owned per-layer histories and add real Project regressions for exact edit/undo/redo, stale rejection and removal/restore followed by older undo.
Coordinate source-structural and application call sites with the lead rather than editing its owned files here.
The legacy Norad `EditHistory` remains intentionally compiling for staged callers and is not counted as migrated or complete.
M05 remains open until integrated acceptance passes and the temporary history is removed from migrated callers.
