# Object receipt worker report

## Scope

This worker adds immutable `changed_objects` results for the existing one-source nonstructural width, point, and anchor transaction surface.
Each committed object result contains the guarded glyph name and stable glyph ID, stable source/layer identity, a `kind`, and the stable point or anchor ID when applicable.
The canonical transaction derives the list from its final staged before/after snapshots, preserving operation order and deduplicating repeated changes to the same object.
Objects returned to their original state before publication are excluded.
Unchanged and rejected outcomes return an empty list.
Exact retries and receipt lookup replay the original immutable list after undo without applying another transaction.

## Validation

`cargo test --lib document::project::edit_transactions --locked -- --test-threads=1` passed 5 tests.
`cargo test --lib document::agent_session --locked -- --test-threads=1` passed 7 tests.
`cargo test --bin runebender application::platform::live_edits --locked -- --test-threads=1` passed 8 tests.
`cargo clippy --workspace --all-targets --locked -- -D warnings` passed.
All Cargo checks used the shared native cache with two jobs or fewer and a task-owned lease.

## Coordination and limits

The cancellation worker owns the additive precommit hook and cancelled terminal outcome.
This worker leaves cancellation semantics unchanged and limits `changed_objects` to committed outcomes.
The Milestone 1 receipt-field review finds actor, operation key, immutable payload digest, before/after revision, changed identities, history handle, current history state, document epoch, and `saved=false` present for a committed operation.
Independent cancellation and the wider Milestone 1 proof/client acceptance matrix remain outside this worker scope.
