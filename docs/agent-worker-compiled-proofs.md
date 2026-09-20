# Compiled proof worker

This worker owns `document::compiled_proof` only.
It captures immutable canonical compiler inputs and document revision on the document thread.
It compiles those captured inputs on a worker into one immutable OpenType byte snapshot.
It identifies the captured canonical source and compiled bytes separately with SHA-256 and records the pinned compiler-source identity.
Capture resolves feature include content and clears compiler source paths before worker work begins.
HarfRust shaping and Skrifa outline extraction both consume that same byte snapshot at the recipe's normalized location.
The local Designbot renderer receives only data-only outlines and returns one bounded PNG.
The module does not borrow application state while compiling or rendering.
The application/session owner must bind epoch and snapshot handles and reject late proof results.
Failed compilation returns no compiled snapshot and therefore no image.
Branch and experiment proofs remain source-only or unsupported until an owner implements a complete canonical family overlay.
The PNG renderer is currently a native local-process path and is not a browser proof implementation.

Focused validation uses the existing Virtua Grotesk Designspace fixture with Latin, Hebrew and Arabic text at a source location and midpoint.
The focused test also proves an unsaved width edit changes a later snapshot without changing the source fixture on disk.
The test fixture directory must be provided through `RUNEBENDER_TEST_FONTS` when this worktree has no adjacent Virtua Grotesk checkout.
