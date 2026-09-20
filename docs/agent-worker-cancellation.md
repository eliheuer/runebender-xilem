# Independent live edit cancellation worker report

## Scope

This worker implements cancellation for receipt-backed native `agent_apply` operations only.
It does not cancel legacy proposal or experiment mutations, undo committed edits, save fonts, or move Workspace mutation off the application thread.

The cancellation identity is the exact tuple of document epoch, actor and operation key, bound to the admitted request's payload digest.
The same operation key under another actor or document epoch is independent.
A different payload cannot reuse or rewrite an existing pending or terminal cancellation state.

## Interface

The live tool inventory now includes `agent_cancel` with the same required identity fields as `agent_receipt`.
It returns one of the following `cancellation_status` values:

- `prevented`: cancellation won before the canonical commit boundary.
- `already_prevented`: the same exact request was already prevented.
- `too_late`: the application claimed the commit boundary, but has not yet published its terminal result.
- `committed`: the operation already committed and cancellation did not undo it.
- `completed`: the operation finished unchanged or rejected without a commit.
- `unknown`: no operation with that exact identity was admitted in this endpoint lifetime.

A prevented apply records an immutable receipt outcome with status `cancelled`.
An exact retry returns that receipt without staging, publishing, refreshing caches or adding history.
Committed edits still require `agent_history` for conflict-aware undo or redo.

The live context advertises cancellation only after the socket, Workspace and stdio MCP paths are present.
It reports the socket queue, connection and cancellation-entry bounds.

## Routing and resource bounds

The Unix endpoint accepts connections independently so a cancellation call is not blocked behind the apply connection it needs to stop.
It retains at most 16 application-mailbox requests, 32 simultaneous connection handlers and 2048 non-evicting cancellation identities for one endpoint lifetime.
Request and response frames are bounded below 8 MiB.
Only the application thread reads or mutates the canonical Project.

Accepted Unix streams are explicitly returned to blocking mode on macOS.
Each response is serialized and size-checked completely before one `write_all`, and a partial write never receives a second appended JSON error frame.

The stdio MCP adapter keeps ordinary requests on one ordered worker so existing pipelined connection and tool-call behavior remains serialized.
Its input thread continues reading while that worker waits on a live call.
Before an `agent_apply` enters the ordered worker, the adapter reserves its exact typed payload in the endpoint's bounded registry.
An explicit `agent_cancel` tool call bypasses the ordered worker, and the standard MCP `notifications/cancelled` notification maps an in-progress `agent_apply` request ID to its exact Runebender identity.
When that notification wins, the worker still delivers the prevented apply to the Workspace solely to record its immutable cancellation receipt, while suppressing the MCP response as required by MCP.
The explicit tool returns the semantic cancellation outcome.
The ordered worker queue is bounded at 32 requests and stdout writes share one lock, preserving newline-delimited JSON-RPC framing.
Duplicate MCP request IDs are rejected before reservation, and queue-admission failure releases only a still-pending reservation owned by the exact same payload.

Dropping the Unix server stops admission, interrupts waiting handlers on a short poll and removes the endpoint.
MCP end-of-file drains already accepted ordered requests, preserving the previous CLI behavior.

## Validation

Focused tests cover:

- exact epoch, actor and operation-key isolation;
- bounded non-evicting cancellation state;
- payload-conflict rejection without rewriting an existing terminal state;
- prevented, too-late, committed, completed and unknown states;
- a cancelled terminal receipt and exact retry without mutation;
- a second real Unix connection cancelling a queued apply;
- the claim-versus-cancel race without falsely reporting an uncommitted operation as committed;
- endpoint shutdown while a connection waits;
- one multi-megabyte response containing escaped text as one complete parseable frame;
- lost-response receipt reconciliation and existing receipt/actor capacity checks;
- a real stdio MCP process reserving an `agent_apply`, reading `notifications/cancelled` before the apply reaches the application mailbox, then delivering it to record terminal state;
- suppression of the cancelled MCP response and continued valid framing for later replies;
- the existing pipelined live MCP sequence, headless Workspace fixture, no-save behavior and ordinary undo/redo.

Validation commands and final commit are recorded in the coordinator message after the worker branch is finalized.

Final worker validation passed `cargo fmt --all --check`, the repository copyright check, and `git diff --check`.
Strict `cargo clippy --workspace --all-targets --locked -- -D warnings` passed.
The full locked workspace suite passed with 863 tests passing and 4 installed-model or large-fixture tests ignored.
The focused cancellation matrix passed 6 registry tests, 9 receipt-session tests, 7 socket tests, 10 Workspace tests, 4 real CLI/MCP tests and 3 headless application-fixture tests.

## Known limits

Cancellation is cooperative at one deliberately narrow boundary after complete staging and immediately before canonical publication.
Once that claim succeeds, the result is `too_late`; the code never claims an undo or a committed result before publication actually finishes.

MCP cancellation notifications are fire-and-forget and therefore do not deliver the semantic status.
Clients that need `prevented`, `too_late`, `committed` or `unknown` must call `agent_cancel` or inspect `agent_receipt`.
A notification received without a connected endpoint or without a successful typed reservation can only cancel that MCP transport request; it does not claim semantic cancellation or create a receipt.

The registry is in memory and endpoint-scoped.
Closing or replacing the document creates a new epoch, and old cancellation identities become unknown rather than replaying into the new document.

No native foreground GUI, pointer, IME, accessibility, GPU, external model or Windows transport evidence was gathered in this bounded worker.
