# Native compiled-proof job worker

This worker owns `document::proof_jobs` only.
It accepts a `compiled_proof::CompileProofInput` captured by the application/document-owning thread and never borrows a `Project`, workspace, view, socket, or another editable font model.

Every submitted request includes an application-owned opaque document epoch plus the immutable input's canonical revision.
Epochs are nonempty UTF-8 strings of at most 256 bytes, and the worker retains the supplied value exactly without parsing, hashing, truncating, or regenerating it.
Submission rejects a mismatched revision before it enters the worker.
The worker preserves that original epoch/revision on every terminal completion; an application adapter must compare it with the active session before calling a late proof current.

The queue uses exactly one native worker thread and a bounded pending-work channel of at most 16 jobs.
It also bounds retained job handles and completed PNG images to at most 32 jobs.
When the retained bound is full, submission rejects new work until the adapter polls and discards terminal handles.
The worker never creates a thread per proof.

Queued work can be cancelled before compilation begins.
Cancellation changes its terminal state to `CancelledBeforeStart`, and the worker skips it when it reaches the channel.
Running work returns `TooLate` because the font compiler is not interruptible.
A running compiler is allowed to complete, but its result retains the original lineage and cannot be relabeled current.

`poll_completions` reports each terminal result once while retaining it for explicit `discard`.
Completed outcomes carry a bounded PNG through an `Arc`.
Failed outcomes carry an error and never contain a prior image or compiled snapshot.
The queue's retained-image bound applies only to its own `Arc` references; an adapter that keeps a completion's cloned `Arc` retains those bytes independently and must bound that ownership itself.

Shutdown first marks the queue closed and queued jobs cancelled under its state lock, then drops the sender, so an already received queue item observes cancellation rather than beginning after the shutdown boundary.
Dropping or shutting down a queue rejects new submissions and cancels queued work without joining a running compiler on the UI thread.
The one in-flight worker finishes naturally and exits after its channel closes.
This is intentionally a native worker; browser/session integration must choose a supported asynchronous path separately.
