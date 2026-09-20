# Live agent implementation coordination

The user requested that the original task retain core work, planning and delegation, with parallel Sol, Terra and Luna tasks following the Babelfont migration pattern.
The validated shared checkpoint is `170d14a756af58a041caa44878447c0fb03adbc8` on the isolated research branch.
Main remains `e40bd4ce338cb8270f2356a515946e52d2b6b21b`; no automatic merge or push is authorized by this coordination plan.
The [implementation checklist](agent-interface-plan.md) remains the acceptance authority, and [live context notes](agent-live-context.md) describe what is currently implemented.

## Ownership

| Owner | Task ID | Scope |
|---|---|---|
| Coordinator | `01a0bc97-ee49-7cd0-a4a7-013a7973feb9` | Core session protocol and receipts, application integration, fixture endpoint, shared schemas/adapters, review and final acceptance |
| Sol | `01a0bd20-95e1-75a1-9f37-3064db712235` | Canonical bounded atomic edits and Project-owned grouped history |
| Terra | `01a0bd20-a303-7e83-bda0-3d42ab5b7dfd` | Immutable compiled proof inputs/bytes, shaping, outlines, PNG and lineage evidence |
| Luna | `01a0bd20-b84e-7232-aedb-a18c1b9559b6` | Client capability inventory, isolated connection setup and conformance harness |

All three requested model assignments were verified in their task contexts.
Workers operate in separate worktrees at the shared checkpoint and commit their own validated phases.
They report exact commits, interfaces, evidence and blockers to the coordinator, who reviews before integration.
Workers do not create more tasks, change central checklist/changelog files, or expand into another owner's adapter files.
The coordinator remains responsible for reconciling documentation and generated schemas after integration.

## Agreed interfaces and remaining decisions

Sol's staged engine batch is initially limited to one source and nonstructural width, point and existing-anchor edits.
Fallible preparation precedes one guarded publication, one revision change and one named history group.
The same Project-owned history handle must serve application undo and agent-targeted undo, with explicit state/conflict checks so neither can reverse an operation twice.
The coordinator will add the application undo entry, UI refresh and operation receipt around that API.
Cross-source metadata, add-anchor/structural edits and durable journals remain outside this first batch.

Terra captures owned canonical compilation inputs and a revision before background compilation.
Compilation, shaping, outline extraction and PNG rendering must share immutable compiled bytes and the returned font hash.
Use the compiler's glyph order for names, including ligatures and unencoded glyphs; cmap alone is insufficient.
The coordinator owns epoch binding, proof-handle lifetime, asynchronous application dispatch and late-result checks.
Source-only experiment proofs cannot masquerade as full variable-family proofs.

Luna's harness takes an explicit executable path and Unix endpoint.
The coordinator still needs to supply the real Workspace-backed headless fixture command and freeze receipt/proof tool names.
Until those exist, the harness records pending capabilities rather than simulating successes or treating passing skips as coverage.
Actual model image delivery and model interpretation require separate evidence.
The remaining CLI-generated live prompt also needs review alongside server-side schema validation; the MCP live instructions were updated in the context checkpoint.

## Scheduled continuation and build coordination

Each worker has an hourly continuation in its existing task.
The coordinator's existing ten-minute continuation was updated to respect this ownership split and review worker progress without duplicating their work.
Continuations stay quiet when unchanged or non-actionable, report meaningful results or blockers, and are removed when their bounded work is complete.

Before any shared-cache build, exclusively create `/private/tmp/runebender-agent-build-lease` and write an owner record with task ID, worktree and process/command.
If occupied, inspect/coordinate and continue other useful work; never delete another task's lease or kill its build.
Use at most two Cargo jobs and hold the lease through build and test execution.
Copy executables required after lease release into a task-specific evidence directory, then release only the owned lease.
Approved caches are `/Users/eli/.codex/worktrees/5d82/runebender-xilem/target` and its `web/target`.
Do not overwrite the pinned final migration executable or evidence directory.

Tests use synthetic or disposable copied fonts; original font sources remain untouched.
Use headless checks and explicit client configuration boundaries.
The full integrated native/browser matrix and a clean-checkout proof remain coordinator acceptance work, not something inferred from worker task creation.
