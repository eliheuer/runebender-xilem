# Live agent implementation coordination

The user requested that the original task retain core work, planning and delegation, with parallel Sol, Terra and Luna tasks following the Babelfont migration pattern.
The original worker integration checkpoint was `ef08043034d3f9b090cde1ecbad2c67eee5ccc0e` on the agent research branch.
The released migration baseline is `e40bd4ce338cb8270f2356a515946e52d2b6b21b`; the user subsequently authorized merging all validated agent work into the native checkout main branch and pushing upstream on 2026-09-20.
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

## September 20 noon testing target

At approximately 09:10 Pacific, the user requested a coherent testing candidate around noon, followed by UI/UX testing and bug fixing.
This is the current delivery target, not evidence of completion or a promise that every broader acceptance item will pass by that time.
Prioritize the complete Python report/proposal/Apply/Undo flow, then the native graph's shared baseline, editable script and two specimen outputs, using the same guarded agent commands.
Retain the separate full-family, client-image, clean-checkout and native interaction acceptance requirements; do not silently count partial implementations as passing them.

The user also requested sidebar and schedule cleanup.
The completed cancellation, procedural/OMP harness, Python runtime and Python recipe tasks were archived with their committed work preserved on their named Git branches for central review.
Archiving these handoffs does not imply integration.
The metaball task was restored on user request on 2026-09-20 because that work is not finished; leave it open.
Their obsolete schedules were already removed.
At the initial cleanup, four schedules remained; the completed Scripts and Nodes execution schedules were subsequently removed as recorded below.
The existing desktop Virtua trial remains open for testing.
Use bounded temporary reviews for independent research and code review instead of adding duplicate implementation tasks.

## Candidate handoff and user-led testing

At approximately 11:47 Pacific, the validated candidate was fast-forwarded to main and pushed to upstream at `44c59409f3b2e0230e56b8197e04df8e74cbacf0`.
The remote ref was directly verified and both checkouts were clean.
The installed native release and main-checkout release executables have SHA-256 `6f34ac9f3bf31862bb595334160fc1765b587aea529cd0278e4407fe8ac3b0f1`; the main-checkout debug executable has SHA-256 `07b6bf93e9a7b8d4ad556f0bea6f7b29713a2ba03d088ac1d64d6e4778911891`.
The runtime source is `edb5e3e`; later candidate commits change documentation only.
The previous executables and installation manifest are preserved under `/private/tmp/runebender-noon-candidate`.
All completed implementation workers are now archived, and only the central continuation remains scheduled.
The metaball task remains open.

The user then preferred to use the application directly and work with a chat on concrete UI/UX issues.
Central agreed to hold further UI changes until that feedback, finish the already-running clean-source validation, and retain architecture and integration coordination here.
The fresh disposable UI copy is `/private/tmp/runebender-noon-ui-virtua/sources/VirtuaGrotesk.designspace`, with a 1,744-file manifest alongside it.
Fresh 1000-by-720 headless entry captures in `/private/tmp/runebender-noon-ui-audit` show Chat, Scripts and Nodes; they are not a completed interactive flow audit.
All copied files still match their hashes after capture.

Desktop MCP initially retained the older tool catalog after the executable update.
After the task environment refreshed, all nine Nodes commands and asynchronous proof commands became callable, and `editor_sessions` returned successfully.
The server is project-scoped in `.codex/config.toml`, with no tool allowlist.
Computer Use explicitly disallows controlling Codex's own settings, so central requested a manual server refresh rather than bypassing that restriction.
The refresh request is now resolved; do not ask for another restart without a new failure.
The separate foreground-window request remains pending, and the user prefers to drive manual UI testing.
This does not block manual use of the native editor.

The existing desktop task subsequently completed the actual Nodes image trial using the refreshed MCP tools.
It configured a Regular-source `AA` comparison, ran a Python width proposal, received both PNG image blocks, and described the second A shifting right without an outline change.
The coordinator independently retrieved and viewed the same retained PNGs and verified their hashes against the task report.
The proposal was not applied: canonical, cache and session advance all remained 716, document revision remained 2, the host was unmodified and both history depths were zero.
The run was released, the headless host was shut down, and all 1,744 copied source files retained their hashes.
Evidence is `/private/tmp/runebender-noon-desktop-model-image/desktop-model-image-trial.json` with `coordinator-verification.json` and the original/changed PNGs alongside it.
This proves actual desktop model image receipt and bounded visual interpretation, but not the separate visual-only synthetic-marker, OMP image or foreground native interaction scenarios.

The clean source archive at `60d203d` also passed formatting, 932 native tests with four ignored, strict all-target Clippy, warnings-denied documentation, the optimized build, advisories and copyright checks.
Its runtime source matches the promoted candidate; the later candidate differences are documentation only.
The archive used the existing dependency/build cache and disposable test fonts, without local source patches.
Because an archive has no Git metadata, the copyright command used the candidate tracked-file manifest with its worktree redirected to the archive.
The release build was interrupted by the task environment restart; after verifying its process was gone, only the unfinished release and final checks were resumed.
Evidence is `/private/tmp/runebender-noon-candidate/clean-checkout-evidence.json`; the owned build lease has been released.

The testing candidate and preparation handoff are complete.
The user-led UI/UX phase remains open, with further UI changes held for concrete feedback.
This does not mark the broader agent-interface milestones complete: OMP image acceptance, the visual-only marker scenario, native interaction/platform checks, expanded shared script editing and graph-history controls remain tracked work.

An earlier integrated runtime was `7eb0a86`, with coordination and workflow documentation through `c7de387` on main and upstream.
The Python runtime handoff is `c8ea6d49`; anchor recipes are `7d431ead` plus corrected contract handling in `8314665e`.
The cancellation handoff is `fd0ddcbc`, whose receipt parent is already integrated and must not be applied twice.
The OMP proof harness handoff is `b6421908`; its external model image trial remains unproven.
The Nodes canvas content-layout seam is `642d710`, which does not yet establish working inline editing or image rendering.

## Noon candidate integration in progress

At approximately 11:40 Pacific, source checkpoint `edb5e3ece3fb344f64dc72f1fac72b94db00abcd` has passed 932 native tests with four explicitly ignored tests, strict all-target Clippy, and warnings-denied documentation.
The integrated candidate includes native Python scripts, guarded live Nodes, full-family proof overlays, independent cancellation, direct MCP PNG delivery, bounded source-editor Undo/Redo, ordinary font Undo from Nodes, and explicit comparison graph save/reopen.
Gray and Light headless captures of Scripts and the actual compiled Nodes outputs are in `/private/tmp/runebender-noon-candidate/ui-final`.
They establish rendered layout and theme behavior, not foreground pointer, IME, accessibility or GPU acceptance.

The exact candidate's credential-free stdio MCP trial passed on a copied full Virtua Grotesk designspace.
All nine Nodes tools were used, exact returned PNG bytes matched their published hashes, A changed from 716 to 816, ordinary Undo restored 716, and retry after Undo did not reapply.
All 1,744 files in both input and copy matched before, after and after cleanup.
Evidence is `/private/tmp/runebender-noon-candidate/transport-2/virtua-mcp/evidence.json`.
The pinned executable has SHA-256 `07b6bf93e9a7b8d4ad556f0bea6f7b29713a2ba03d088ac1d64d6e4778911891`.
This does not establish actual model image interpretation.

The separate full-family anchor trial passed with `A` followed by the noncomposing U+030B combining mark.
Moving the Regular A top anchor by 100 units changed the shaped mark offset by 100 units at the source and 50 units at the family midpoint; the Bold source remained unchanged.
Both corresponding PNG pairs differ, ordinary Undo restores the anchor, exact retry after Undo does not reapply, and all source hashes remain unchanged.
Evidence is `/private/tmp/runebender-nodes-virtua-anchor-20260920-1250/evidence.json`, SHA-256 `6edb29fb3c4327832254586a6d741a91eb2706c6a28b24337d5746f6cdf51ce5`.
The earlier A-plus-acute specimen composed into Aacute and therefore did not exercise mark positioning; it is not counted as passing anchor evidence.

The Nodes execution and canvas tasks are validated and archived, and all worker schedules are removed.
The Scripts task remains open for final integration verification; only the central ten-minute continuation remains scheduled.
The existing desktop trial remains available, and the metaball task is open at the user's request.
Before the combined candidate promotion recorded above, main and upstream remained `790702d7d0e00a5602aca30f49b72aef43b652c2`.
The native optimized release build and a separate full-family stdio MCP trial of that optimized executable passed.
The latest-source browser release build, strict browser Clippy and quality checks at DPR 1, 2 and 1.25 passed.
The combined evidence manifest is `/private/tmp/runebender-noon-candidate/candidate-evidence.json`.
Advisories and Python harness checks passed with the unchanged dependencies.
At the initial candidate checkpoint, clean-checkout acceptance, actual desktop/OMP model image interpretation, expanded shared script editing and exposed graph-history controls remained open.
Subsequent desktop image evidence is recorded in the handoff section above.

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
The [OMP model-client read/edit trials](agent-client-trials.md) now pass against fixture `999e6db`; the subsequent desktop task trial is recorded below.
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

## Native desktop task and main promotion

The user confirmed the target is native Xilem from the completed Babelfont migration in `~/GH/repos/runebender-xilem`.
The agent branch includes that main baseline and preserves its view/widget sources.
The user explicitly requested all latest validated work on main and pushed upstream.

The separate user-facing desktop task is `01a0bf07-bd97-7821-b5c0-fff48e09cff4`, “Try Virtua Grotesk through Runebender MCP”.
Its actual MCP tool discovery and guarded real-font edit/retry trial pass, replacing the earlier host-reload blocker.
The [client evidence](agent-client-trials.md) separates desktop MCP success from native window and compiled-image coverage.
The new file-backed headless host and the synthetic fixture share the same bounded application dispatch loop.
The host process tests pass, including byte-for-byte source preservation after edits and ordinary undo/redo.
The native suite now passes 847 tests with four ignored tests, and strict all-target Clippy passes.
Desktop and procedural examples operate on disposable font copies; original sources remain untouched.
The central task remains the only scheduled continuation, while the desktop task is the user-facing trial surface.


## Compiled proof integration and second worker wave

The user requested another wave of bounded scheduled workers on 2026-09-20.
Sol task `01a0bf34-0cef-7001-905e-1e3848e05cfb` owns independent edit cancellation and concurrent socket/MCP routing, with ten-minute continuation.
Terra task `01a0bf34-1694-7050-b5d3-c191f6b1d0c3` completed changed-object receipts, reviewed and integrated as `937345b`.
Luna task `01a0bf34-26bb-7af0-8deb-c4571d507bfb` completed the receipt-backed Python harness; review fixes restored negative cases, post-undo verification and accurate binary provenance, integrated as `6a5b355`.
The two completed worker continuations have been removed; their reports retain the detailed scope and evidence.
The central ten-minute continuation owns proof integration, client image trials, final acceptance and validated main promotion.

The [compiled proof contract](agent-compiled-proofs.md) now defines strict start/status/cancel/release requests and document-local retry identities.
One process-wide queue survives document replacement while each Workspace owns at most eight proof handles.
Completed PNGs pass directly through MCP without scene rendering, with separate captured and current document revisions.
Handles retain one proof artifact, not a reusable compiled-font object.
Actual desktop and OMP image interpretation, cancellation integration and the remaining acceptance matrix stay pending until separately tested.


The integrated compiled-proof checkpoint passes 852 native tests with four explicitly ignored tests and 12 Python checks.
The real socket/stdio MCP proof test verifies the original 412-unit glyph advance in a captured proof after the live glyph changes to 430, direct PNG byte equality across socket/MCP, and a new current proof with the changed advance and a different font hash.
The image trial exposed partial socket response formatting on larger payloads; accepted streams now use blocking I/O and responses are serialized before writing, with no second error frame appended after a partial send.
Sol is incorporating the complete bounded-response helper and a multi-megabyte regression into the concurrent cancellation transport.
Native evidence lives in `/private/tmp/runebender-agent-compiled-live-20260920`.
The initial targeted run lacked `RUNEBENDER_TEST_FONTS`; the final suite used disposable sources and passed.
These are automated protocol checks, not an actual model image-interpretation trial.

## Python scripting product wave

The full remaining validation matrix passed at `7eb0a86`, and native main and origin/main were directly verified at that checkpoint.
The user then approved the scripting workflow and requested new scheduled tasks to implement it.
The final language decision is Python only; no Rust scripting prototype is assigned.
The [workflow contract](agent-scripting-workflow.md) defines the first usable product loop and shared process boundary.

| Task | Model | Continuation | Ownership |
|---|---|---|---|
| `01a0bf72-cbd3-7631-9675-a9dfc3b782b8` | Sol | `complete-python-script-runtime`, 10 minutes | Typed recipe contract, bounded subprocess runner, persistent script library |
| `01a0bf73-2c77-7723-8cea-f9b86cd94e06` | Terra | `complete-chat-and-scripts-panel`, 10 minutes | Chat artifacts, Scripts list and editing, application preview/apply integration |
| `01a0bf73-79cb-76d2-b4ec-ef8361f2a23b` | Luna | `complete-python-anchor-recipes`, 20 minutes | Anchor examples, thin helper, deterministic conformance and failure tests |

These workers have started in isolated worktrees and received each other's task IDs and an agreed version-1 recipe schema.
The runtime owner publishes exact signatures and commits before consumers integrate them.
Workers remove their own continuations when ready for review; central archives them only after successful integration.
Central retains API review, cancellation integration, combined validation, disposable application acceptance and coherent main/upstream promotion.
No completed worker is restarted without a separate bounded need.

The previous cancellation worker finished at `fd0ddcbc4df1d852e42035aace55e2a732cdd885` and removed its continuation.
Its report claims 863 native tests passed with four ignored tests, strict Clippy and race/transport tests; this remains worker evidence until central review and integration.
Its parent is Terra's receipt commit `3decf8b`, so do not integrate that earlier change twice.
Preserve central compiled-proof image forwarding while resolving the dispatcher overlap.

The previous OMP image continuation is deleted and its unreviewed harness is preserved at `b642190`.
The automatic approval block on credentialed external-model image submission is unresolved and must not be bypassed.
Python recipe implementation and disposable local acceptance are authorized; a foreground native GUI still requires an agreed interruption.
Full-family atomic script edits and remaining variable/multilingual/client/platform acceptance remain in the broader plan.

## Native Nodes and agent integration wave

The user explicitly authorized a native Nodes scripting/agent implementation pass and new scheduled workers.
The [Nodes plan](agent-nodes-plan.md) records the current gaps, ComfyUI evidence, shared service boundary and acceptance criteria.
No ComfyUI installation or runtime dependency is part of this work.

| Task | Model | Continuation | Ownership |
|---|---|---|---|
| `01a0bf8c-4d25-7591-992b-25d1b0904ae4` | Sol | `complete-native-live-nodes-execution`, 10 minutes | Typed live graph session/commands, immutable run state and native execution adapter |
| `01a0bf8c-9e56-7370-8a0f-f40afa21043c` | Terra | `complete-script-and-specimen-node-ui`, 20 minutes | Actual inline Python editor and movable/resizable image nodes, focus/input and layout |

Both workers started in isolated worktrees and must publish interfaces early.
Central owns live graph agent exposure through existing native adapters, compiled-family overlay integration, cross-worker review and acceptance.
The existing Script UI worker retains script buffer/library controls and application capture/preview/Apply; it is not replaced by a second script implementation.

Runtime worker `c8ea6d4` is complete and awaiting central review; its continuation is deleted.
It reports strict native/browser Clippy and 14 focused tests, including corrected Python recipes executed through the real queue and validated by Rust.
Recipes `7d431ea` plus `8314665` now pass 12 centrally rerun Python tests and their pure subprocess harness, and the runtime worker confirmed cross-language schema/hash/proposal validation.
The script artifact/draft UI phase `af6bc7a` plus ownership clarification `bad3901` awaits review and follow-up for real storage, Run, preview, Apply and captures.
These worker handoffs are not evidence that the complete scripting or Nodes workflow is usable yet.
