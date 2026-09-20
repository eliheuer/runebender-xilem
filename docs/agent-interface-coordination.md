# Live agent implementation coordination

The user requested that the original task retain core work, planning and delegation, with parallel Sol, Terra and Luna tasks following the Babelfont migration pattern.
The current worker checkpoint is `ef08043034d3f9b090cde1ecbad2c67eee5ccc0e` on the isolated research branch.
The released migration baseline is `e40bd4ce338cb8270f2356a515946e52d2b6b21b`; no automatic merge or push is authorized by this coordination plan.
The [implementation checklist](agent-interface-plan.md) remains the acceptance authority, and [live context notes](agent-live-context.md) describe what is currently implemented.

## Ownership

| Owner | Task ID | Scope |
|---|---|---|
| Coordinator | `01a0bc97-ee49-7cd0-a4a7-013a7973feb9` | Core session protocol and receipts, application integration, fixture endpoint, shared schemas/adapters, review and final acceptance |
| Sol/Luna (complete) | `01a0bd66-eefd-7723-ae8b-abaa492918c1` | Typed session receipts, bounded retry ledger and atomic transaction boundary in `agent_session.rs` |
| Terra (complete) | `01a0bd66-f64c-7911-b0d7-20a3f095f58d` | Bounded background immutable compiled proof jobs in `proof_jobs.rs` |

The new bounded tasks started with Sol and Terra with high reasoning effort.
Sol reached model capacity after writing the receipt implementation; the same receipt task resumed with Luna for validation, preserving its code and scope.
The three original worker tasks are complete and archived; their engine, proof and client harness commits remain integrated.
Workers operate in separate worktrees at the shared checkpoint and commit their own validated phases.
They report exact commits, interfaces, evidence and blockers to the coordinator, who reviews before integration.
Workers do not create more tasks, change central checklist/changelog files, or expand into another owner's adapter files.
The coordinator remains responsible for reconciling documentation and generated schemas after integration.

## Agreed interfaces and remaining decisions

Sol's staged engine batch is initially limited to one source and nonstructural width, point and existing-anchor edits.
Fallible preparation precedes one guarded publication, one revision change and one named history group.
The same Project-owned history handle must serve application undo and agent-targeted undo, with explicit state/conflict checks so neither can reverse an operation twice.
The coordinator has added the application undo entry, source-aware view refresh and operation receipt around that API.
Cross-source metadata, add-anchor/structural edits and durable journals remain outside this first batch.

Terra captures owned canonical compilation inputs and a revision before background compilation.
Compilation, shaping, outline extraction and PNG rendering must share immutable compiled bytes and the returned font hash.
Use the compiler's glyph order for names, including ligatures and unencoded glyphs; cmap alone is insufficient.
The coordinator owns epoch binding, proof-handle lifetime, asynchronous application dispatch and late-result checks.
Source-only experiment proofs cannot masquerade as full variable-family proofs.

Luna's harness takes an explicit executable path and Unix endpoint.
The real Workspace-backed headless fixture and model-driven OMP trials now pass.
Receipt/proof tool names still need to be frozen; the harness records missing capabilities rather than treating skips as coverage.
Actual model image delivery and model interpretation require separate evidence.
CLI-generated live prompts and MCP initialization now share live source, session and authorization guidance; complete server-side schema validation remains pending.

## Scheduled continuation and build coordination

The completed workers used ten-minute continuations in their existing tasks, shortened at the user's request.
Sol's original engine phase is committed at `de184bc25eaf2fbb4e33304bb49b275b8515b467` and its completed continuation has been removed.
Terra's bounded proof phase is committed at `1b8cc22` with five focused tests and strict Clippy passing, and its completed continuation has also been removed.
Luna's bounded harness phase is committed at `8a88aaeb599e3ca5a7c2674bd8deb324a75cf63d`, and its completed continuation has been removed.
Its final fixture evidence is `/private/tmp/runebender-agent-client-fixture-20260920-run5/report.json`, against coordinator fixture commit `999e6db`.
The report verifies unsaved width 412, authorized width 430, the specific authorization/stale rejection reasons, application cache/session agreement, ordinary undo/redo and no source write.
The three worker commits are integrated as `7fe9376`, `f3c3ff7` and `ef1dcb8` on this isolated branch.
The combined base passed 818 serial workspace tests using disposable fonts, with four tests ignored.
Proof review fixes are integrated through `ef08043`; eight focused proof tests and strict native Clippy pass.
The browser release build, strict browser Clippy and interaction quality at DPR 1, 2 and 1.25 also pass.
Evidence is retained in `/private/tmp/runebender-agent-integration-20260920`; the final complete acceptance matrix remains pending.
The [OMP model-client read/edit trials](agent-client-trials.md) now pass against fixture `999e6db`; the desktop task trial remains pending.
The original three tasks were archived at the user's request, and their schedules are deleted.
The new receipt and proof-job tasks completed their bounded work as `5b495b2` and `b375a7a`, integrated here as `92769ae` and `3efadba`.
Both workers removed their completed ten-minute continuations.
Each worker reports seven focused tests and strict Clippy passing.
The integrated native tree passes 837 tests with four explicitly ignored tests, strict all-target Clippy and warnings-denied documentation.
The combined browser release build, strict Clippy and interaction quality at DPR 1, 2 and 1.25 also pass.
This runtime checkpoint evidence is `/private/tmp/runebender-agent-runtime-20260920/evidence.json`.
Both completed workers are archived; only the central continuation remains active.
Integration also gates the native proof queue out of WASM and binds context tokens to the exact socket epoch.
The coordinator's existing ten-minute continuation remains the central integration schedule.
Continuations stay quiet when unchanged or non-actionable, report meaningful results or blockers, and are removed when their bounded work is complete.

Before any shared-cache build, exclusively create `/private/tmp/runebender-agent-build-lease` and write an owner record with task ID, worktree and process/command.
If occupied, inspect/coordinate and continue other useful work; never delete another task's lease or kill its build.
Use at most two Cargo jobs and hold the lease through build and test execution.
Copy executables required after lease release into a task-specific evidence directory, then release only the owned lease.
Approved caches are `/Users/eli/.codex/worktrees/5d82/runebender-xilem/target` for native checks and `/Users/eli/GH/repos/runebender-xilem/web/target` for browser checks.
The old `5d82` browser cache no longer exists; do not recreate it.
Do not overwrite the pinned final migration executable or evidence directory.

Tests use synthetic or disposable copied fonts; original font sources remain untouched.
Use headless checks and explicit client configuration boundaries.
The full integrated native/browser matrix and a clean-checkout proof remain coordinator acceptance work, not something inferred from worker task creation.

## Live transaction adapter checkpoint

The native adapter now exposes `agent_apply`, `agent_receipt` and `agent_history` with strict typed requests and exact endpoint epochs.
It admits eight actor ledgers with 256 non-evicting receipts each, and records one application entry and one canonical view refresh for each new committed group.
Exact retries retain the original receipt even after undo and do not recreate the application entry.
Ordinary editor, overview and targeted undo share the engine handle, including auxiliary-layer edits and inactive-source changes.
The native suite passes 846 tests with four ignored tests, strict all-target Clippy and warnings-denied library documentation.
The browser release build, strict browser Clippy and headless interaction checks at DPR 1, 2 and 1.25 also pass.
The first browser check could not find an expired temporary Playwright installation; rerunning against the bundled runtime passed without source changes.
The real stdio MCP process test covers generated schema discovery, apply, retry, receipt lookup, ordinary undo and targeted redo.
These automated protocol checks are distinct from an actual desktop or OMP model trial of the new tools.
The local evidence directory is `/private/tmp/runebender-agent-live-transactions-20260920`; its preserved binary and schema have SHA-256 records.

## Receipt integration constraints

The coordinator's receipt ledgers are scoped to one document epoch and do not own font data.
An operation key must bind to a canonical payload digest and actor; a retry with a different payload rejects.
A retry of a committed operation returns its original receipt without reapplying the engine transaction or adding another application undo item.
Bound the ledger by rejecting new operations at capacity instead of silently evicting keys and making an old retry execute again.
Original receipts retain before/after revision and history handle; status can separately report the handle's current applied/undone state.

Cancellation must distinguish requests prevented from committing from operations already committed.
The existing serial socket accept loop cannot deliver an independent cancellation request while waiting on an earlier call, and the synchronous MCP loop cannot read cancellation notifications while a tool is running.
Do not advertise cancellation until those routing limits and queue/commit races have actual fault-injection coverage.
The new `agent_apply` path now reconciles lost responses through `agent_receipt`, including a real socket disconnect test.
Legacy mutations and non-idempotent history replay still require inspecting current state before retrying.

## First user trials

The user selected a local task chat in the Codex/ChatGPT desktop application and OMP CLI as the first two clients.
Both should address the same native Xilem live document through its existing local protocol.
First prove context/read, one bounded unsaved edit, application cache/session refresh and ordinary undo/redo using the synthetic Workspace fixture.
Then connect each actual client to a disposable font session and retain the transport and visible result evidence.
A bundled Codex CLI check does not establish that the desktop task has loaded the tools, and a tools-list response does not establish model image delivery.
Full Milestone 1 acceptance still requires independent edit cancellation, asynchronous compiled proof delivery, actual client image evidence and the final validation matrix.

## Next core implementation boundary

The native Workspace now owns bounded actor ledgers around the canonical Project, constructed from the socket server's exact epoch.
Strict edit requests retain their required epoch through mailbox dispatch, while legacy tools keep the earlier optional-guard stripping behavior.
The browser must not advertise a live native session merely because it shares the Workspace type.

The first atomic wire operation resolves explicit source/layer/glyph and existing point/anchor IDs against canonical reads, compares existing external glyph revision tokens, then stages the complete read/write set through `begin_document_edit_transaction`.
The engine's guarded commit remains the final publication boundary.
Validate receipt capacity and operation-key conflicts before publication; retain terminal unchanged and rejected outcomes as well as committed receipts.
An exact retry must return its original receipt without repeating application refresh or adding another history entry.

One application `MetadataEdit::AgentGroup` carries the Project-owned group handle, affected addresses and overview history depth.
The engine's read-only `check_document_edit_history_group` supports conflict-aware availability without mutating the document; replay rechecks the same guard before publication.
Ordinary editor undo/redo and agent-targeted replay must use the same engine group, update the same application history entries and retain unrelated later edits.
Real-socket tests cover targeted undo followed by ordinary undo, ordinary undo followed by targeted undo, conflicts after later edits, inactive-source cache refresh, auxiliary layers and overview history ordering.
The existing per-layer proposal-install bookkeeping remains separate from this integrated group path.

Keep cancellation unadvertised until the serial socket and MCP loops can accept it independently of a running request, with explicit queued/committed race tests.
Proof jobs must capture immutable inputs on the application thread, compile/render on workers, and return epoch/revision-bound handles without allowing a late result to become the current proof.
Keep one bounded native queue across document replacements rather than creating an unbounded sequence of detached compilers.
The adapter must bound retained completion references separately from the queue's own retention because cloned `Arc` results keep image bytes alive after queue discard.
The current MCP `proof_content` path renders a scene synchronously; compiled job results must instead deliver the worker's already-rendered PNG with its captured font hash and revision, without recapture or rerender.
The initial queue retains proof artifacts, not reusable compiled-font snapshot handles; do not advertise arbitrary later shaping or export against a retained font handle until that retention exists.
