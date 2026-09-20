# Procedural live spacing example

`scripts/live_spacing.py` is a bounded stdlib example for changing the advance widths of at most 64 named glyphs in one already-open native Workspace.
It is not an SDK, a source-file editor, or a replacement for the interactive editor.

The script requires an absolute `--binary` path so it never chooses a Runebender executable from `PATH`.
The script requires an absolute `--session` path so it never falls back to a disk-font call.
The session endpoint is the one supplied by the native Workspace host.
It does not start an editor, load a font, write a source file, or save an open document.

## Prepare a request

Preparation is the default mode and is read-only except for creating the new request file with exclusive creation.
It calls `editor_context`, verifies that no canvas gesture is active, calls `project_info`, and confirms the requested stable source ID is present.
It reads every explicit glyph in the requested stable source and records each returned glyph ID, exact layer name, and revision.
It passes the context epoch to `project_info` and every `read_glyph` call so the socket rejects a replaced document before the handler executes.
The context epoch and those read revisions become the guarded `agent_apply` payload.
It checks that the actor/key has no existing receipt after the reads and before creating the request file.
The request contains one finite `set_width` operation for each glyph and an explicit empty `reads` array.
The script rejects duplicate glyph names and batches larger than 64 glyphs.

Use a new operation key for a new request.
The key is actor-local and cannot be reused with a different payload.
The preparation check refuses a key that already has a receipt.
Choosing a request filename that already exists also fails rather than overwriting the prior payload.

```sh
python3 scripts/live_spacing.py \
  --binary /absolute/path/to/runebender \
  --session /absolute/path/to/workspace.sock \
  --source 7 \
  --glyph A \
  --glyph B \
  --delta 12 \
  --actor spacing-example \
  --operation-key spacing-2026-09-20-01 \
  --request-file /absolute/path/to/spacing-2026-09-20-01.json
```

The JSON report explicitly says `mutation_authorized: false` after preparation.
Writing a request that contains `authorization: "user-approved"` does not itself grant authorization or mutate the Workspace.

## Apply exactly that request

Applying requires both `--apply-request` and `--apply`.
The `--apply` flag is the caller's confirmation that the user has already authorized this specific mutation.
The script forwards the request file's original JSON text through `agent call --args-file -` with `subprocess` argument vectors and no shell.
It does not regenerate the operation key, epoch, revisions, payload, or history name.

```sh
python3 scripts/live_spacing.py \
  --binary /absolute/path/to/runebender \
  --session /absolute/path/to/workspace.sock \
  --apply \
  --apply-request /absolute/path/to/spacing-2026-09-20-01.json
```

The result includes the immutable receipt and the current history state supplied by `agent_apply`.
The Workspace reports `saved: false` because receipt-backed edits change only the open in-memory Project.

If the apply response is lost, malformed, or times out, the script calls `agent_receipt` with the exact epoch, actor, and operation key.
If that lookup records a `committed` or `unchanged` outcome, the script reports the reconciled receipt without retrying the mutation.
If the receipt records `rejected`, the script reports that failure even though receipt lookup itself succeeded.
If it cannot determine the result, the script reports the ambiguity and retains the same request file for a deliberate exact retry.
It never creates a replacement operation key automatically.

## Inspect status and undo

Status is read-only and can be run after an apply, a lost response, or an ordinary editor undo.
It calls `agent_receipt` with the request's immutable identity and reports both the original receipt and current `history_state`.

```sh
python3 scripts/live_spacing.py \
  --binary /absolute/path/to/runebender \
  --session /absolute/path/to/workspace.sock \
  --status-request /absolute/path/to/spacing-2026-09-20-01.json
```

Undo also requires `--apply` because it mutates the open Project through `agent_history`.
The script first confirms that the receipt's history state is `applied`.
It then sends the receipt identity with `direction: "undo"` and `authorization: "user-approved"`.
An undo response can be ambiguous, so the script reconciles it through `agent_receipt` instead of guessing or issuing a second history replay.
It reports a lost undo response as successful only when the current receipt history state is `undone`.

```sh
python3 scripts/live_spacing.py \
  --binary /absolute/path/to/runebender \
  --session /absolute/path/to/workspace.sock \
  --apply \
  --undo-request /absolute/path/to/spacing-2026-09-20-01.json
```

Receipt lookup and an exact `agent_apply` retry are idempotent within the document epoch.
`agent_history` is not idempotent, so status must decide whether another undo or redo is appropriate.
An epoch change means the document was replaced or the session no longer identifies the same live document, and the request must not be silently adapted.

## Validated native trial

On 2026-09-20, the example completed preparation, apply, exact retry, receipt lookup, targeted undo and final status against a disposable real Virtua Grotesk native Workspace.
The selected red `.notdef` in stable source 0 changed from 600 to 602 units and returned to 600 after undo.
The identical retry retained the same receipt and document revision without applying again.
The original and copied source files remained unchanged.
Evidence is retained alongside the [desktop trial](agent-client-trials.md) under `/private/tmp/runebender-desktop-virtua-20260920/script-*.json`.
Two focused recovery tests distinguish a rejected receipt from a successful apply and verify that a lost undo response is reconciled by an `undone` history state.
