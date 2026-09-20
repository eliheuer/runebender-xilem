# Receipt-backed live transactions

The native Unix Workspace exposes `agent_apply`, `agent_receipt` and `agent_history` through the existing CLI and MCP adapters.
These tools edit the same unsaved canonical Project as the editor and never save source files.
They do not change the older proposal/experiment tools into idempotent operations.
The generated schemas are available from `RUNEBENDER_LIVE_SESSION=/path/to/session.sock runebender agent tools` and MCP `tools/list`.

## Read, apply and reconcile

Connect to an explicit editor endpoint, read `editor_context`, and choose the stable source from `project_info`.
Read each target and dependency with `read_glyph`, retaining its `glyph_id`, exact layer name and `revision` token.
An apply request must include the endpoint's `expected_document_epoch`; the optional legacy guard behavior does not apply to these three tools.

The following shape illustrates a width edit; substitute the identities and revision from the actual read.

```json
{
  "expected_document_epoch": "EPOCH_FROM_RESPONSE",
  "actor": "spacing-assistant",
  "operation_key": "spacing-pass-001",
  "authorization": "user-approved",
  "source": 0,
  "history_name": "Adjust A advance",
  "reads": [],
  "edits": [{
    "target": {
      "glyph": "A",
      "glyph_id": "GLYPH_ID_FROM_READ",
      "layer": "LAYER_FROM_READ",
      "expected_revision": "REVISION_FROM_READ"
    },
    "operations": [{"op": "set_width", "width": 430.0}]
  }]
}
```

Existing point and anchor movement use `set_point` with `point_id`, `x`, `y`, or `set_anchor` with `anchor_id`, `x`, `y`.
These are opaque IDs from the guarded layer, not positional indices.
The logical glyph ID also rejects a replacement glyph that happens to reuse the same name and geometry.
The batch supports one explicit source, at most 64 total target/dependency entries and 256 operations.
Every edited layer must contain at least one operation.
Structural edits, adding anchors, cross-source edits and implicit active-selection targets are outside this batch.
Additional `reads` guard dependencies even when no operation writes to them.

Only send `authorization: "user-approved"` within the user's existing authorization for the edit.
The actor is a caller label and key namespace, not authentication or a grant of authority.
Unknown request fields and unsupported operation shapes reject before dispatching a transaction.
All IDs and revisions are checked before the canonical engine stages and publishes the complete batch once.
An error in a later operation cannot publish an earlier operation.

The result contains an immutable `receipt` and a separate current `history_state`.
The receipt records the original epoch, actor, operation key, complete normalized payload hash, outcome, revisions and committed history handle.
Committed outcomes include the affected glyph/source/layer addresses as they were at commit time.
They are not rewritten after a later rename or edit.
The envelope's `document_revision` describes the current document when the response is made, while the receipt's revisions remain the original revisions.
`saved: false` states that the call did not save the font.

After a lost apply response, either call `agent_receipt` with the epoch, actor and operation key, or retry the exact original `agent_apply` request.
An exact retry returns the original receipt with `replayed: true` and `root_changed: false`.
It does not stage again, advance the document revision, refresh views, or add another undo entry.
Changing the payload under the same key returns `payload_mismatch`.
After reviewing a terminal rejection and revising the request, use a new key.
Rejected and unchanged admitted operations retain receipts too; malformed requests or capacity failures are rejected before admission.
An exact retry after undo reports the original committed receipt and current `history_state: "undone"`; it does not redo the edit.

Each document epoch admits at most eight actors and 256 receipts per actor.
Ledgers reject new requests when full and never evict a key that could then execute again.
Existing receipt lookups and exact retries remain available at capacity.
Receipts are in memory only and disappear with the Workspace/document lifetime.
They do not provide crash recovery or a durable journal.

## One history group

A changed batch creates one Project-owned history group and one application history entry.
Grid entries, affected parked tabs and the current session are refreshed from canonical state once.
Edits to an inactive source do not replace active-source geometry; canonical dependency invalidation may still refresh active-source caches.
Switching sources rebuilds views from the selected canonical source.
Application history tracks the group's position relative to overview batches so ordinary Undo/Redo preserves their ordering.
An explicitly addressed auxiliary-layer edit belongs to that glyph's ordinary history even while its foreground is displayed; replay does not copy auxiliary geometry into the foreground.

`agent_history` takes the epoch, actor and original operation key, plus `direction: "undo"` or `"redo"` and the existing user authorization.
It replays the same group that ordinary editor Undo/Redo uses and moves the same application entry.
A second undo cannot reverse the operation again.
An overlapping later edit rejects replay, while unrelated later edits remain intact.
The engine retains at most 128 groups; an older receipt can remain available after its history handle becomes unavailable.
An application redo entry can also become unavailable after a new edit starts a different history path.

History replay itself is not an idempotent apply operation.
If its response is lost, inspect `agent_receipt` and its separate history state before deciding whether replay remains appropriate.
The original apply receipt never changes to describe a later undo or redo.

## Gesture and transport limits

A new edit or history replay cannot replace an active canvas gesture.
Receipt lookup and exact apply retries remain available during a gesture because they do not mutate or refresh the document.
A newly admitted apply rejected by the busy-gesture check keeps its terminal rejection; a later revised attempt needs a new key.

The socket remains serial with an eight-MiB frame limit and a 30-second response timeout.
The adapter can reconcile a dropped apply response, but independent edit cancellation remains unsupported and is advertised as false.
Legacy proposal/experiment mutations have no receipt lookup or duplicate-apply protection.
Compiled proof jobs and model image delivery remain separate integration work.

## Validation boundary

Application tests use real Unix sockets and the real Workspace, including a caller disconnect before receiving a committed result.
They check retry identity, unchanged cache/session references on retry, one undo entry, ordinary/targeted replay, stale read dependencies, invalid later operations, point/anchor identities, source reorder, inactive-source views, overview ordering, gesture ownership, admission bounds and document replacement.
The tests use synthetic fonts without saving them.
They do not certify native pointer/IME behavior, actual model-client use of these new tools, or image interpretation.
