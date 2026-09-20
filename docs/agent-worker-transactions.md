# Agent transaction worker record

## Scope

This worker implements only the font-engine portion of Milestone 1B on checkpoint `170d14a756af58a041caa44878447c0fb03adbc8`.
It adds a bounded one-source transaction for existing width, point-position and anchor-position operations.
The operations use stable canonical point and anchor identities and never construct a mutable UFO or second font model.
Structural edits, anchor insertion, cross-source metadata, receipt persistence, authorization, transport and durable journals remain out of scope.

## Engine API

`DocumentLayerEdit` pairs an opaque `CanonicalLayerSnapshot` with ordered stable-identity `DocumentEditOperation` values.
`Project::begin_document_edit_transaction` validates the source, group name, operation bounds, duplicate targets and complete supplied read/write snapshot set while applying every fallible operation to private canonical drafts.
The current bounds are 64 guarded layer entries, 256 operations, a 256-byte history name and 128 retained history groups.
`Project::commit_document_edit_transaction` rechecks every target and read dependency, publishes all changed drafts through the existing atomic `commit_layer_edits` boundary and advances the document revision once.
Its changed result reports before and after revisions, the exact `DocumentChange` affected addresses and a session-stable `EditHistoryGroupId`.
A no-op reports the unchanged revision and records no history.

## History and conflicts

The transaction records one Project-owned before/after group instead of duplicating the operation into the per-layer history piles.
`Project::replay_document_edit_history_group` is the shared undo/redo boundary for targeted agent replay and the application undo item.
Every affected current layer must equal the expected side before any layer is replaced.
Later unrelated edits survive replay, while an overlapping edit returns `HistoryConflict` without changing the document or group state.
A successful undo changes the group state from `Applied` to `Undone`, so a second undo through another caller returns `WrongHistoryState` and cannot reverse the operation twice.
Redo performs the inverse guarded transition.
The application integrator must place the returned group handle in the ordinary undo ordering and call this grouped replay API for both undo and redo.

## Validation evidence

The invalid-operation test makes the third operation reference an anchor from another glyph after two valid staged operations.
It verifies that both target snapshots, the document revision, source dirty flag and both per-layer undo and redo depths remain unchanged.
The stale-read test changes a separately guarded dependency after preparation and verifies that all writes are rejected without changing the immediate pre-commit revision, dirty state or existing history.
The group replay test verifies one revision for a two-glyph commit, stable point and anchor identities, no per-layer history duplication, ordinary and targeted replay through the same handle and explicit double-undo rejection.
The conflict test verifies that an unrelated later glyph edit survives targeted undo and that a later overlapping edit blocks the complete replay without partial reversal.

The following scoped commands passed with `CARGO_BUILD_JOBS=2` and the approved shared target cache:

```text
cargo test --locked project::edit_transactions -- --test-threads=1
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --locked component_selection_cancel_duplicate_and_delete_keep_stable_identity -- --test-threads=1
```

The focused transaction run passed four tests.
The existing component-selection identity and undo regression passed unchanged.
The dependency graph still reports the pre-existing future-incompatibility warning for `block v0.1.6`.

## Integration requirements

The adapter must turn external expected revisions and dependency reads into canonical snapshots before calling the engine API.
The adapter owns epoch checks, authorization, operation IDs, idempotent receipts, cancellation and status lookup.
The receipt can serialize the group handle with `EditHistoryGroupId::to_wire` and parse it with `from_wire` within the same document epoch.
The receipt should use the engine's before and after revisions and `DocumentChange::affected_layers` rather than reconstructing cache invalidation from the wire request.
The application must keep one undo item carrying the same group handle and must not create per-layer history items for this transaction.
The final integration matrix must still exercise the real application adapter, disconnect-after-commit recovery and browser/native checks.
