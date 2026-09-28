# Temporary AI architecture checklist

This task coordinates the implementation of the September 2026 architecture review.
The goal is a reliable foundation for agent editing, reusable tools, and node workflows that can use local or cloud model workers.
Every completed item needs integrated code, focused regression coverage, and recorded verification.
This file is temporary working state; retire it after the final acceptance review and move enduring guidance into the maintained documentation.

## Current status: stopped at the user's request

The user requested that the current patch be finished and recurring work stopped on 2026-09-28.
The current patch is committed as 273562b; the heartbeat is PAUSED and must not resume without a new user request.
Unchecked items are deferred work, not an instruction to continue automatically.
The integration branch is local and has not been pushed or merged into the original checkout.
This closes the scheduled pass, not the entire AI product roadmap or the final H release gates.

## Working context

- Coordinator task: 01a0e667-56d8-79d2-9d56-d78f56424f17.
- Implementation checkout: /Users/eli/.codex/worktrees/runebender-ai-foundation/runebender-xilem.
- Integration branch: codex/ai-foundation.
- Original checkout: /Users/eli/GH/repos/runebender-xilem.
- Reviewed original HEAD: c2620698205c5cc49eec7a478493f712709aaa46.
- Baseline snapshot commit: 34cdbbab80603aa9dbd4d3f6976fb49e0a9a3b3a.
- The baseline commit preserves the user's pre-existing CLI and Babelfont file splits plus the Cargo change.
- Preserve the original checkout and its uncommitted work.
- Make local coherent commits on the integration branch after review; do not push, merge into the original checkout, or remove worktrees automatically.
- All workers must use the implementation checkout explicitly.
- Only the coordinator changes this checklist or commits shared-tree work.
- Sol handles bounded Rust changes; Luna handles mechanical maintenance; the coordinator handles architecture and integration.
- Assign disjoint file ownership and keep at most two implementation workers active after the initial independent correctness fixes.
- Serialize Cargo commands using the coordinator's build slot.
- Build cache: CARGO_TARGET_DIR=/Users/eli/GH/repos/runebender-xilem/target.
- Do not count ignored tests or mocked provider runs as real model/runtime validation.

## Agreed design constraints

Project remains the canonical document owner.
Font mutations go through typed, guarded transactions with stable identities and undo.
Native UI, CLI, MCP, scripts, and graph nodes adapt shared operations.
Background workers receive immutable captures and return candidates or artifacts.
A graph run must not implicitly save or apply to the open font.
Provider-specific SDKs, credentials, and network clients stay outside the font engine.
A provider result is untrusted data that must pass format validation and proposal staging.
Receipt identity, document lifetime, semantic graph identity, and durable artifact identity remain distinct.
Persistence must not automatically replay uncertain mutations.
Keep the current domain layout and improve ownership boundaries where concrete implementations need it.
Do not create unused registries, placeholder providers, or abstractions with only a speculative caller.
New dependencies require explicit review, not automatic acceptance.
QuiverAI means the SVG-generation/vectorization service at quiver.ai, inferred from the user's example.
Prepare its integration boundary and offline contract tests in this pass.
Actual account connection, paid calls, model downloads, or publication require a separate user request.
Trusted local Python is not an OS sandbox; do not imply otherwise.

## Checklist and acceptance gates

### A. Restore correctness and trustworthy validation

- [x] A1. Fix disk graph cache inputs, including font metadata and relevant dependencies, preserving unchanged-run hits.
  Regression: a metrics change regenerates the proof; changed executable identity invalidates an external node result.
  Integrated as a4d7575 after coordinator review and a real CLI executable-replacement regression.
  Verified: 535 library tests and all 18 CLI tests passed; source invalidation is deliberately conservative.
  Initial owner: workflow_cache (GPT-6 Sol).
- [x] A2. Make identical canonical compiler inputs yield stable bytes without wall-clock test flakes.
  Preserve source creation metadata and actual semantic undo coverage.
  Integrated as ae88325; all 12 compiler integration tests passed, including fixed-date and checksum assertions.
  Scope: repeated builds of the same captured input; canonical source-date import is tracked in B6.
  Initial owner: compiler_determinism (GPT-6 Sol).
- [x] A3. Repair the Norad boundary gate for moved modules and out-of-line test-only modules without weakening production boundaries.
  Integrated as 5e4ca09: exact moved item paths plus an explicit file-level cfg(test) restriction.
  Production modules named tests remain inspected; no generic Rust module resolver was added.
  Initial owner: architecture_gate (GPT-6 Luna).
- [x] A4. Run focused checks, then the relevant native suite; record remaining known failures honestly.

### B. Establish shared operation contracts

- [x] B1. Inventory current command, proposal, live-edit, recipe, and node capabilities; choose one typed owner per operation.
  Coordinator source review and the ownership map below establish the implementation routing; no registry implementation is claimed.
- [x] B2. Centralize transport-facing schemas, result contracts, and effect metadata while preserving strongly typed engine APIs.
  Initial consumed descriptor slice integrated as 0380293: per-surface effects, shared host input schemas, and concrete result schemas for project_info, editor_connect, editor_sessions, and export_proof.
  CLI discovery exposes the same descriptors through opt-in agent tools --contracts; default assistant tool payloads remain compatible.
  Proof-job result contracts are integrated as 7a8e80e with typed lifecycle payloads, shared generated schemas, and real MCP decoding checks.
  Guarded apply/receipt/history/cancellation and all nine live graph tools now have typed result ownership and shared generated schemas in 199042d; disk nodes_run has its own typed result contract.
  Verified real MCP editing, rejected receipts, graph execution, exact proof-image transport, explicit apply, ordinary undo, exact retries, stale status and release.
  Output schemas describe serialization, including required nullable fields and operational failures that retain receipts instead of a top-level error.
  Unverified output schemas remain absent rather than advertising a generic object as a complete contract.
- [x] B3. Return structured MCP results and truthful schemas/annotations, preserving text and proof images for compatibility.
  Integrated as 0380293 with negotiated protocol feature gates and one shared metadata value for text and structuredContent.
  Verified CLI/MCP success/error parity, four supported protocol versions plus fallback, proof image separation, existing retry/cancellation regressions, and real application proof/edit fixtures.
  Output schemas cover the verified discovery/export, proof-job, guarded-edit and graph families tracked in B2; this is not certification of every MCP feature or every legacy proposal/experiment result.
- [x] B4. Extend recipe captures to useful outline operations and provide guarded structural editing for shape generators.
  Integrated as 36ac4dd with version-two immutable outline/component captures, captured-point moves, and bounded append_contours operations through the canonical transaction owner.
  Engine-minted contour/point identities survive undo and redo; no replacement/removal operation was added.
  Verified exact retries, stale/deleted targets, full-batch rejection, dependent glyph invalidation, detached graph preview/proofs, explicit Apply, and one ordinary undo step in engine and real MCP tests.
  Legacy version-one captures preserve their hash and wire shape; version-one results remain limited to width/anchor operations.
  Scripts that strictly require version-one inputs or exact old layer keys need parser updates; the bundled anchor recipe supports both versions and was executed through the real runner.
- [x] B5. Remove identified reverse dependencies: engine mark metadata must not depend on loading a UI theme; live workflows must call typed engine operations rather than a transport dispatcher.
  Integrated as 0ca60b6 with raw canonical mark values, application palette resolution, typed workflow application, and dependency-direction regression checks.
  Verified: 979 native tests, strict Clippy, browser release build and quality matrix, plus inspected Gray/Light headless captures.

- [x] B6. Preserve explicit source creation timestamps at the canonical metadata/import boundary and define a deterministic absent-date fallback.
  Integrated as 837c300 with raw canonical per-source dates, strict compiler validation, and a fixed 2000-01-01 UTC compilation fallback for undated sources.
  File > New Font records its intentional creation time once; compiling an undated import does not add a source date.
  Verified independent UFO reopen/compile byte equality, exact date import/export, default-source selection, malformed-date rejection, and unchanged source bytes.

### C. Make worker execution consistent

- [x] C1. Define a common job lifecycle with explicit cancellation capability, deadlines, output bounds, progress, and retained results.
  Integrated as 718290d with workflows::process terminal outcomes, shared cancellation tokens, configurable per-process limits, bounded line callbacks and captured terminal bytes.
  Existing recipe queues and host job records retain their ownership; no unused generic scheduler was added.
  Cancellation kills and reaps the direct child, including callback unwind cleanup; it does not terminate descendants or undo external side effects.
- [x] C2. Migrate disk workflow, Local AI, and Chat subprocess supervision onto the supported lifecycle.
  Integrated as 718290d for those callers, Python recipes, interpreter availability and native/CLI task discovery.
  Verified hung/noisy/failed workers, blocked input, descendant handles, pre-spawn cancellation, late-result suppression, queue publication races, downstream cancellation and uncached failed candidates with offline fixtures.
  Model calls default to 30 minutes, 1 MiB input, 4 MiB stdout and 256 KiB stderr; recipes retain their shorter existing limits and task discovery uses two seconds.
  Chat caps accepted events per turn; native disk graphs coalesce progress per node and cancel on owner drop.
  RunContext exposes disk cancellation to hosts; this phase does not add a disk-workflow Cancel button or interrupt running core font operations.
- [x] C3. Support a persistent local-model worker adapter without making model memory or runtime state part of Project.
  Integrated as aa5c81e with a bounded loopback client consumed by native Chat through RUNEBENDER_CHAT_ENDPOINT and optional RUNEBENDER_CHAT_MODEL.
  An externally managed font-ml serve process owns resident model state; Runebender does not start it, download weights, or claim to stop server inference when cancelling an HTTP request.
  The application owns a bounded read/proposal tool loop, pins each call and transcript publication to the captured document lifetime, and never exposes install/save/arbitrary workflow tools through this route.
  Offline HTTP and real live-editor fixtures verify repeated turns through one server, unsaved reads, detached proposals, cancellation, stale-document rejection and failures.
  Real model residency, model performance, cloud compatibility and server restart management were not exercised.
- [x] C4. Keep browser availability explicit and preserve native/browser build boundaries.
  Verified 718290d and aa5c81e with separate browser release builds and headless smoke tests; the process and local HTTP clients return BrowserUnavailable on wasm and native UI execution guards remain explicit.
  Re-run the browser gate when later shared-source changes extend this boundary.

### D. Generalize live graphs

- [x] D1. Replace the fixed four-node comparison planner with a validated directed-acyclic-graph execution plan.
  Integrated as acf8a36 with deterministic topological planning and consumed native/MCP execution.
  Verified real Python chains, independent branches, four compiled proofs, per-node failures and dependent-node suppression through the application MCP fixture.
  Version 2 supports one captured live.font, zero to sixteen live.python transforms and one to eight live.proof nodes.
- [x] D2. Preserve immutable version lineage, semantic hashes independent of layout, bounded retention, and guarded result application.
  Canonical lineage groundwork is caf92b0; acf8a36 consumes per-node candidates and verifies parent hashes through execution, proof and Apply.
  A selected result commits its complete retained ancestry through the existing receipt/history owner, preserving generated identities and ordinary Undo/Redo.
  Verified unchanged root before Apply, sibling isolation, layout independence, explicit selection, exact retry, changed-key-payload rejection, stale suppression, cancellation and release.
  At most eight heavy application run results are retained alongside bounded receipt tombstones; script/proof queues and cumulative canonical transaction limits remain enforced.
- [ ] D3. Bridge external workers through detached captures and validated candidate imports; remove implicit root-save requirements from live execution.
  Native Local AI foundation is integrated as 273562b: detached current-document export, validated session candidates, explicit grouped Install, ordinary Undo/Redo and cleanup.
  Live DAG model nodes and the legacy disk-oriented Nodes save-first path remain unfinished; D3 is intentionally not checked off.
- [x] D4. Make cache policy and side effects explicit per node implementation; carry structured Rows inputs faithfully.
  Integrated as 870cf65 with consumed versioned policies, conservative external defaults, content-verified cache reuse and bounded JSON Rows arguments.
  Real CLI fixtures verify nested/Unicode/empty Rows, cache hits and invalidation; runner regressions verify model/adapter artifacts and pre-spawn failures.
  Complete native suite: 1081 passed, 0 failed, 4 ignored; strict lint/docs/headless checks and browser build/smoke passed.
  External workers remain trusted processes; full-directory model hashing has a documented disk I/O cost.
- [x] D5. Version graph/node contracts and preserve existing saved graphs through an explicit compatibility path.
  acf8a36 separates execution versions 1 and 2 from unchanged authoring/file schema version 1 for the current live.font/live.python/live.proof family.
  Omitted execution_version preserves the four-node comparison, its capture encoding, version IDs, successful report text, proof hash form and branch image selectors.
  Version 2 explicitly opts into DAG results and node selectors; unsupported versions/topologies reject before dispatch.
  Existing saved graph round trips and legacy MCP fixtures pass; future external node and extension manifest evolution remains part of D3/G1.

### E. Make tools discoverable and extensible

- [ ] E1. Introduce a command/generator registry used by existing implementations and metadata consumers.
- [ ] E2. Extract an interactive-tool lifecycle for begin/update/cancel/commit and preview, using existing shape tools as concrete clients.
- [ ] E3. Demonstrate adding a generator through the extension surface without editing central canvas dispatch throughout.
  Verify preview, cancellation, one undo entry, and access from commands and graphs.
- [ ] E4. Provide a small documented recipe/extension example that an agent can implement and validate.

### F. Prepare provider integrations

- [ ] F1. Separate assistant-provider orchestration from model-worker execution and font document operations.
- [ ] F2. Add a provider-neutral, consumed contract for capabilities, progress, cancellation, artifacts, provenance, and errors.
- [ ] F3. Implement an offline vector-provider fixture exercising QuiverAI-style SVG results through validation, coordinate conversion, proposal proof, apply, and undo.
  Do not report this as a live QuiverAI integration.
  The current formats/svg.rs importer ignores groups and transforms; validate a supported SVG subset explicitly or implement those semantics before accepting provider output.
  Bound bytes, paths, coordinates and nesting; reject unsupported content instead of silently changing its appearance.
- [ ] F4. Keep cloud credentials and sendable context in host adapters; support explicit scope and effect checks before dispatch.

### G. Define extension trust and recoverable runs

- [ ] G1. Define supported extension trust tiers and a versioned manifest with enforced capability declarations.
  Document precisely which restrictions are enforced and which trusted-local privileges remain.
- [ ] G2. Bind execution and document-edit grants to host-owned scope rather than treating caller strings as security credentials.
- [ ] G3. Persist bounded run manifests and artifact lineage; reconcile completion, failure, cancellation, and ambiguous outcomes after restart without automatic mutation replay.
- [ ] G4. Add restart/reconnect tests proving stale artifacts cannot silently apply to a replacement document.

### H. Acceptance and documentation

- [ ] H1. Complete the end-to-end extension acceptance: one shape generator and one model node use supported interfaces with preview, cancellation, apply, and undo.
- [ ] H2. Run formatting, copyright, strict Clippy, docs, native tests, release build, advisories, and no-default-features library checks as applicable.
- [ ] H3. Run browser build and smoke checks when shared source changes affect it; inspect Gray and Light headless captures when UI changes.
- [ ] H4. Update maintained architecture, agent/MCP, workflow, and limitations guidance to match implemented behavior.
  Website changes must be narrowly scoped to this architecture pass and coordinated with its checkout.
- [ ] H5. Review the complete integration diff, report exact commits and remaining validation limits, and disable the recurring coordinator.
  All required items must be implemented and verified before declaring this pass complete.

## Operation ownership map

This is the reviewed routing decision for B1, not a claim that the transport consolidation is implemented.
Keep small typed Rust APIs at these boundaries; transport schemas and registries describe them without making the document depend on JSON dispatch.

| Capability | Current implementation | Chosen owner and next integration |
| --- | --- | --- |
| Canonical reads and existing-object edits | `font/project/edit_transactions.rs`, `automation/agent_edit.rs` | `Project` owns capture, validation, atomic commit and grouped undo; automation maps bounded wire requests onto those operations. |
| Structural outline changes and generators | `font/babelfont/edit_*`, `font/edit_batch.rs`, `application/editor/session.rs` | Extend the existing canonical transaction surface with structural operations, then route generators and recipe results through it; retain legacy proposal adapters until parity is proven. |
| Proposal and version application | `font/project/proposal_transactions.rs`, `font/proposal.rs`, `font/experiments.rs` | Typed font APIs own conflicts, dependency updates and history; UI and live workflow adapters call them directly rather than `automation::live::call`. |
| Agent identity, exact retries and cancellation | `automation/agent_session.rs`, `automation/agent_cancellation.rs`, `application/platform/live_edits.rs` | Automation owns bounded receipts and request contracts; host owns document epochs and grants; neither substitutes for canonical stale-state checks. |
| Proof capture and rendering | `font/compiler/proof.rs`, `font/compiler/proof_jobs.rs` | Compiler owns immutable font/proof inputs and rendering; host owns scheduling and presentation; running compiler cancellation remains explicitly unsupported unless implementation changes. |
| Recipe input and candidate validation | `automation/script_recipe.rs`, `application/platform/script_jobs.rs` | Automation owns the versioned recipe contract; host supplies bounded execution; recipes return candidates and never receive mutable Project access. |
| Graph editing, planning and retained run state | `workflows/nodes.rs`, `workflows/nodes_session.rs`, `application/editor/tools/nodes/execution.rs` | Workflows owns graph contracts and pure planning; host dispatches worker jobs and accepts results against captured lineage. |
| Disk graph execution and external workers | `workflows/nodes_run.rs`, `application/editor/tools/local_ai.rs` | Share process supervision and immutable capture conventions; keep filesystem publication explicit and preserve headless operation. |
| CLI, MCP and assistant tool descriptions | `automation/agent*.rs`, `automation/live.rs`, `application/cli/mcp.rs` | One consumed automation descriptor supplies versioned input/output metadata and effects; each transport preserves its framing and compatibility behavior. |
| Interactive tool lifecycle | `application/workspace.rs`, `application/editor/session.rs`, `application/view/canvas/editor.rs`, `application/widgets/tool_group.rs` | Application owns gestures and preview state; reusable shape construction calls typed engine operations; UI metadata comes from the consumed registry. |
| Assistant and model providers | `application/editor/tools/chat.rs`, `application/editor/tools/local_ai.rs` | Keep assistant conversations separate from model-worker jobs; host adapters own credentials, process/network calls and sendable context; results share validated artifact/proposal routes. |

The initial shared-operation slices remove the live workflow's call into the JSON transport dispatcher and provide result/effect descriptors consumed by CLI discovery and MCP.
B2 now supplies consumed typed result contracts for guarded edits, receipts, proof jobs and live/disk graph operations.
Unverified legacy result schemas remain absent; typed transport metadata does not replace document guards or host-owned authority.
The structural-edit slice must prove generated contours can preview, reject stale input, apply atomically and undo before expanding provider integration.
The cache fix hashes full UFO inputs conservatively; precise dependency-aware caching is a later performance improvement, not a prerequisite for correctness.
Compiler timestamp normalization covers repeated builds of the same captured Babelfont input.
B6 additionally preserves exact canonical `openTypeHeadCreated` values and maps the default source date into `font.date`.
Undated sources use a fixed compilation-only date while remaining undated in source metadata.
Independent UFO reopening and compilation are verified separately from same-capture determinism; this is not a blanket guarantee of byte reproducibility across every compiler version or platform.

## Execution record

2026-09-28: Created an isolated integration worktree and copied 21 pre-existing dirty paths into a local baseline commit.
The original checkout remains untouched.
Started three independent correctness workers.
A1 and A2 passed coordinator review and targeted tests, and are committed locally.
B1 is complete as the reviewed ownership map above.
A3 and A4 are complete for this milestone.
The full native suite passed 972 tests with 0 failures and 4 ignored tests.
The ignored real-model and larger font workflows are not runtime coverage.
Strict Clippy initially caught integer-literal style and a 64 KiB stack buffer; b22fde4 fixes them without lint allowances.
After that adjustment, both cache CLI regressions passed again, strict all-target Clippy passed, formatting passed, and the no-default-features library check passed.
Copyright and git diff checks passed.
The existing dependency warning for block 0.1.6 remains; no new dependency was added.
Release, advisory, browser, visual and real-provider acceptance remains in H and must not be inferred from this native checkpoint.
All 21 original dirty baseline paths were rehashed and remain unchanged.

Verification logs:

- /tmp/runebender-ai-foundation-correctness-tests.log: 535 library, 18 CLI, 12 compiler tests passed.
- /tmp/runebender-ai-foundation-native-tests.log: complete native suite, 972 passed, 4 ignored.
- /tmp/runebender-ai-foundation-clippy.log: strict all-target Clippy passed after corrections.
- /tmp/runebender-ai-foundation-cache-final.log: both cache CLI regressions passed after the buffer change.
- /tmp/runebender-ai-foundation-headless.log: no-default-features library check passed.


Deferred next work, only after a new user request: complete D3 live DAG external-worker bridging and validated candidate import.
A-C and D1/D2/D4/D5 are complete for their recorded acceptance scope; provider-neutral cloud/vector contracts, extension registries and restart recovery remain separate work.
The current live executor supports only live.font, live.python and live.proof, with one active operation per run and independent candidate branches.
Do not route live model nodes through disk execution that saves the open source font.
First trace one existing external task's offline input/output contract and connect it to a captured, detached font input with validated candidate import.
A temporary export owned by the run may be appropriate, but Project and its original source files must remain unchanged until explicit Apply.
Reuse the shared process lifecycle and canonical proposal/transaction owners; do not add an unused provider registry or placeholder model node.
Preserve the consumed D4 node execution metadata and lossless bounded Rows argument contract when adding external live nodes.
Preserve the existing explicit execution-version compatibility path when extending accepted live node types.
For chained candidates, use script_recipe::capture_staged and AgentEditRequest::stage_after, or the corresponding typed import/proposal boundary, without introducing another mutable Project.
Derive proofs from the captured root CompileProofInput and the selected node's complete candidate.
Explicit Apply must commit that exact candidate through the shared receipt/history owner; rebuilding operations remints generated identities.
Bind retries to the selected node and complete ancestry, keep source/glyph scope explicit, and suppress late results after cancellation or semantic/document changes.
Use at most two implementation workers with disjoint file ownership and keep all Cargo commands serialized through the coordinator.
The persistent local Chat client is inference-only and externally managed; it is not a completed model-node or cloud-provider integration.
Do not launch real models, download weights, connect providers or modify the sibling font-ml repository in this pass.
Use Project-owned identities and existing atomic commit/history paths; B4 supplies generated outline operations without caller-created canonical identities or mutable font wrappers.
Receipt-bearing rejected or cancelled edits may return ok=false with an error inside receipt.outcome; preserve those shapes explicitly rather than assuming every failure uses the generic top-level error envelope.
Use at most two disjoint implementation workers.
The recurring coordinator is active every 30 minutes and should remain quiet unless there is a verified milestone, actionable failure, completion, or user decision.


2026-09-28: User requested a 30-minute cadence; the existing heartbeat was updated and remains active.
Started two GPT-6 Sol implementation subagents inside this coordinator task: engine_mark_boundary and live_workflow_boundary.
Both completed the two parts of B5 with disjoint file ownership.
The coordinator reviewed the patches, added dependency-direction regression checks, validated the combined result, and integrated commit 0ca60b6.
These are internal subagents, not separate sidebar tasks.


B5 verification:

- /tmp/runebender-ai-foundation-boundaries-tests.log: 979 passed, 0 failed, 4 ignored in the full native suite.
- /tmp/runebender-ai-foundation-boundaries-clippy.log: strict all-target Clippy passed.
- Formatting, copyright and git diff checks passed.
- /tmp/runebender-ai-foundation-boundaries-web.log: complete browser release build passed from a fresh cache.
- /tmp/runebender-ai-foundation-boundaries-browser-quality.log: the repository quality matrix passed at 1x, 2x and 1.25x display densities.
- /tmp/runebender-ai-boundaries-proofs/gray.png and light.png: headless captures inspected; glyph mark display remains consistent across both themes.
- The temporary loopback browser test server was stopped after validation.

The legacy nodes_live::apply helper has no production caller in the current application.
Its typed refactor removes a dependency and propagates engine errors correctly; it does not replace the current guarded Nodes Apply path.
The font engine now exposes custom mark labels and typed colors without reading theme files; the application retains the existing Gray-palette display classification.
No accounts or real model providers were connected, and no native foreground window was opened.
All 21 original dirty baseline paths remain unchanged.


2026-09-28: The first half-hour continuation completed the structured MCP milestone in 038029385409dcaa171b05b300db823bf61b3826.
The coordinator verified the clean integration branch and completed workers before dispatching two GPT-6 Sol assignments: tool_contracts and mcp_contract_tests.
The descriptor worker owned only the automation contract module; the test worker owned CLI/live transport regression files.
The coordinator implemented protocol state and response framing, reviewed both patches, strengthened real Workspace proof checks, and serialized all Cargo commands.

MCP now supplies structuredContent for negotiated 2025-06-18 and 2025-11-25 clients while retaining the same JSON text and separate images.
2024-11-05 clients receive neither annotations nor structured-result fields; 2025-03-26 clients receive annotations without structured-result fields.
The existing unknown-version fallback remains 2025-11-25; pre-initialize compatibility behavior uses that same default.
The supported version set was not expanded.
Shared effect metadata distinguishes connection/session changes, font mutations, file writes, retained artifact removal, and open-ended local program execution.
Local Python/model programs are conservatively described as capable of file writes and destructive effects; annotations are descriptive hints and do not enforce grants.
B2 remains open for the richer typed result contracts, and G remains responsible for actual host-owned authorization.

Verification for 0380293:

- /tmp/runebender-ai-foundation-contracts-unit.log: all five shared descriptor regressions passed.
- /tmp/runebender-ai-foundation-contracts-integration.log: 19 CLI, 6 live-agent, and 4 real application fixture tests passed.
- /tmp/runebender-ai-foundation-contracts-tests.log: full native suite, 986 passed, 0 failed, 4 ignored across 23 suites.
- /tmp/runebender-ai-foundation-contracts-clippy.log: strict all-target Clippy passed after adding a required diagnostic message to a new fixture assertion.
- /tmp/runebender-ai-foundation-contracts-doc.log: workspace documentation passed.
- /tmp/runebender-ai-foundation-contracts-headless.log: no-default-features library check passed.
- Formatting, copyright, and staged diff checks passed.
- /tmp/runebender-ai-foundation-contracts-web.log: browser release build passed.
- /tmp/runebender-ai-foundation-contracts-browser-smoke.log: the repository's headless smoke check passed at 1x density; the temporary loopback server was stopped.

This milestone did not change views, so the full density matrix and native Gray/Light visual captures from B5 were not repeated.
The four ignored real-model/larger font tests remain unexecuted runtime coverage, and the existing block 0.1.6 future-compatibility warning remains.
No new dependency was added, no provider/account was connected, and no native foreground window was opened.
The coordinator remains active every 30 minutes.
All 21 original dirty baseline paths were rehashed and remain unchanged.


2026-09-28: The second continuation completed B6 in 837c300d4cf67a2405d7237c4bf14584252ed3d0 and the proof-job portion of B2 in 7a8e80e1d286cffe94cf4b070073665f334490c3.
The coordinator checked the integration branch, existing workers and shared Cargo slot before dispatching two GPT-6 Sol workers with disjoint font metadata and proof contract ownership.
Both workers are complete, and their diffs were reviewed before integration; there are no outstanding assignments from this continuation.
The source-date change preserves exact per-source values and rejects malformed explicit dates before compilation.
The proof contract change keeps the existing flat JSON representation while requiring complete metadata for completed results and an error for failed results.
The coordinator strengthened the lifecycle representation, integrated shared MCP output schemas, preserved root-relative schema definitions, and added typed decoding to the real application proof fixture.
Schema verification covers state-specific required fields, local references and actual typed payloads; it does not use an external JSON Schema validation engine.

Verification for both implementation commits:

- /tmp/runebender-ai-foundation-proof-dates-unit.log: 51 focused automation tests passed.
- /tmp/runebender-ai-foundation-proof-dates-integration.log: 7 canonical font-info, 16 compiler and 4 real application fixture tests passed.
- /tmp/runebender-ai-foundation-proof-dates-tests.log: full native suite, 994 passed, 0 failed, 4 ignored across 23 suites.
- /tmp/runebender-ai-foundation-proof-dates-clippy.log: strict all-target Clippy passed.
- /tmp/runebender-ai-foundation-proof-dates-doc.log: workspace documentation passed.
- /tmp/runebender-ai-foundation-proof-dates-headless.log: no-default-features library check passed.
- Formatting, copyright, and staged diff checks passed.
- /tmp/runebender-ai-foundation-proof-dates-web.log: browser release build passed.
- /tmp/runebender-ai-foundation-proof-dates-browser-smoke.log: headless smoke check passed at 1x density; the temporary loopback server was stopped.

No view behavior changed, so the earlier B5 density matrix and Gray/Light native captures were not repeated.
Native release and advisory checks remain part of H acceptance and were not rerun in this continuation.
The four ignored real-model/larger font tests and existing block 0.1.6 dependency warning remain unchanged.
No new dependency was added, no account or model provider was connected, and nothing was pushed or merged into the original checkout.
All 21 original dirty baseline files were rehashed and remain unchanged.
The 30-minute coordinator remains active because the broader architecture checklist is not yet complete.


2026-09-28: The third continuation completed B2 in 199042d39b2d4a8f3e7ea25dbd7cac6f70317051.
The coordinator verified the integration branch and idle workers, then used two GPT-6 Sol assignments with disjoint edit-result and graph-result ownership.
The coordinator reviewed both diffs, integrated the shared descriptors and disk CLI producer, and added real MCP regressions.
All assignments are complete; no worker is left writing into the integration branch.
Generated schemas now use serialization mode, preserving required nullable fields such as history_state, report and stderr.
Operational false results retain their typed receipt or cancellation body; generic admission/transport errors remain a separate alternative.
Graph session capture types gained output schemas without adding unchecked deserialization to constructor-validated domain captures.
The schema checks cover required and state-specific fields, root-relative references, typed decoding where supported, actual serialized graph payloads, and MCP publication parity.
They do not use an external JSON Schema validation engine.
The live graph fixture executes a deterministic local Python recipe, keeps its candidate detached until Apply, verifies exact retained PNG transport and metric differences, then proves ordinary undo, exact retry behavior, stale lineage and release.
This is offline workflow coverage, not local-model or cloud-provider coverage.

Verification for 199042d:

- /tmp/runebender-ai-foundation-operation-results-unit.log: cargo test --locked --lib automation:: passed 58 focused tests.
- /tmp/runebender-ai-foundation-operation-results-integration.log: focused CLI, live_agent and live_fixture targets passed 19, 6 and 5 tests respectively.
- /tmp/runebender-ai-foundation-operation-results-tests.log: cargo test --workspace --locked -- --test-threads=1 passed 1002 tests with 0 failures and 4 ignored across 23 suites.
- /tmp/runebender-ai-foundation-operation-results-clippy.log: cargo clippy --workspace --all-targets --locked -- -D warnings passed after adding the required assertion diagnostic.
- /tmp/runebender-ai-foundation-operation-results-doc.log: cargo doc --workspace --no-deps --locked passed.
- /tmp/runebender-ai-foundation-operation-results-headless.log: cargo check --lib --no-default-features --locked passed.
- cargo fmt --all --check, bash .github/scripts/copyright.sh and staged git diff --check passed, including the new tracked result modules.
- /tmp/runebender-ai-foundation-operation-results-web.log: ./web/build.sh passed using the separate browser workspace/cache.
- /tmp/runebender-ai-foundation-operation-results-browser-smoke.log: web/smoke.cjs passed at 1x density against a temporary loopback server; the server was stopped afterward.

All native Cargo checks were serialized using CARGO_TARGET_DIR=/Users/eli/GH/repos/runebender-xilem/target.
Native tests used RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources.
No view behavior changed; the earlier B5 full density matrix and Gray/Light native captures were not repeated.
Native release and advisory checks remain part of H acceptance and were not rerun in this continuation.
The four ignored model/larger font tests and the existing block 0.1.6 dependency warning remain outside the passing runtime claims.
No dependency, account connection, paid API call, model download, push or merge was introduced.
All 21 original dirty baseline paths were rehashed and remain unchanged.
The coordinator remains ACTIVE every 30 minutes; the remaining B4 and C-H implementation work prevents final acceptance or disabling the schedule.


2026-09-28: The fourth continuation completed B4 in 36ac4dd50d6f4ef4b9cc33136ee759703f32d081.
The coordinator verified the branch and inactive workers, then used two GPT-6 Sol assignments for engine transactions and recipe captures, plus GPT-6 Luna for the bundled Python example and documentation checks.
All worker diffs were reviewed; no worker remains active or owns an unfinished assignment.
The coordinator integrated the existing DrawingContour wire format, shared input/output schemas, real MCP fixtures, and the recipe result validator.
The font engine owns generated geometry validation, transaction-wide limits, canonical IDs, atomic publication, dependency invalidation and history.
Captures include direct contours and separate component references/transforms; they do not flatten component outlines or claim automatic read dependencies on uncaptured base glyphs.
New shape batches are limited to 256 contours and 4096 points, reject nonfinite or out-of-range coordinates and malformed topology, and cannot supply canonical IDs.
Recipes can move captured points and append ordinary contours, including cubic and quadratic geometry; replacing/removing existing contours is outside this slice.
Real graph execution retains detached candidates and exact proof-image transport until explicit Apply.
Actual MCP apply/undo/redo tests verify newly created identities, full-batch rejection, stale guards and receipt reconciliation without saving source files.
Final review also found and repaired a panic when staging an old capture after its glyph was deleted; the engine now returns a typed MissingLayer error.

Verification for 36ac4dd:

- /tmp/runebender-ai-foundation-structural-lib.log: cargo test --locked --lib passed 566 tests before the final deleted-target regression was added.
- /tmp/runebender-ai-foundation-structural-live.log: live_fixture and live_agent targets passed 6 tests each, including real graph outline capture/preview/apply/undo and direct generated-contour MCP coverage.
- /tmp/runebender-ai-foundation-structural-tests.log: cargo test --workspace --locked -- --test-threads=1 passed 1016 tests, 0 failed, 4 ignored across 23 suites before the final deleted-target fix and expanded example runtime test.
- /tmp/runebender-ai-foundation-structural-guards.log: after the final deleted-target fix, all 10 font::project::edit_transactions::tests passed, including the new regression; the entire native suite was not repeated after this focused fix.
- /tmp/runebender-ai-foundation-structural-example.log: the expanded corrected_anchor_example_deserializes_and_validates_against_real_hash runtime test passed for both v1 and newly hashed v2 inputs through the real Rust queue and Python process.
- /tmp/runebender-ai-foundation-structural-python.log: all 14 Python recipe tests and the pure-recipe acceptance harness passed.
- /tmp/runebender-ai-foundation-structural-clippy-final.log: strict all-target Clippy passed on the final source state; three integer literal style errors found in the first Clippy pass were corrected without lint allowances.
- /tmp/runebender-ai-foundation-structural-doc.log: workspace documentation passed.
- /tmp/runebender-ai-foundation-structural-headless.log: no-default-features library check passed.
- cargo fmt --all --check, bash .github/scripts/copyright.sh and staged git diff --check passed, including the new generated geometry module.
- /tmp/runebender-ai-foundation-structural-web-final.log: ./web/build.sh passed on the final source state with the separate browser workspace/cache.
- /tmp/runebender-ai-foundation-structural-browser-smoke.log: web/smoke.cjs passed at 1x density; the temporary loopback server was stopped afterward.

All Cargo commands were serialized; native checks used CARGO_TARGET_DIR=/Users/eli/GH/repos/runebender-xilem/target.
Native integration tests used RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources.
The Python harness correctly reports its own application/model evidence as not_run; the separate Rust/MCP fixtures supply application evidence, and no actual model trial was performed.
No view behavior changed, so the earlier B5 density matrix and Gray/Light native captures were not repeated.
Native release and advisory checks remain part of H acceptance and were not rerun in this continuation.
The four ignored model/larger font tests and existing block 0.1.6 dependency warning remain outside passing runtime claims.
The existing v1 JSON fixture remains byte/hash compatible, but copied scripts that strictly enforce v1 inputs must update their parser for v2 captures; no automatic fallback execution is attempted.
The bundled anchor worker supports both versions and ignores outline/component context for its anchor-only calculation.
Website guidance was audited read-only; H4 should document v2 captures and guarded generated geometry in the scripting and MCP guides.
All 21 original dirty baseline files were rehashed and remain unchanged; nothing was pushed, merged, published, connected to an account or downloaded as a model.
The 30-minute coordinator remains ACTIVE for C-H; no C implementation assignment has been launched yet.


2026-09-28: The fifth continuation completed C1/C2 and the current C4 boundary in 718290d4fd212606362a865c8ba990dd57a4da4b.
The coordinator verified a clean integration branch and completed workers before assigning two GPT-6 Sol workers: process_supervision and model_process_callers.
The supervisor worker owned the shared process module and recipe integration; the adapter worker owned Chat and Local AI.
GPT-6 Luna audited maintained documentation read-only.
The coordinator reviewed every worker diff, integrated native/CLI disk workflows and discovery, and serialized all Cargo commands.
All workers are complete and no next-phase assignment is active.

The shared runner uses private temporary standard streams with bounded retained buffers and line callbacks, avoiding blocked stdin pipes and descendant-held EOF hangs.
It validates deadlines and byte limits before spawn, checks output after exit, and retains typed terminal outcomes.
Capture files can grow beyond their retained byte cap between polls; this is not a hard disk quota or an OS sandbox.
Cancellation and callback unwinding kill and reap only the direct child; processes can already have changed disk sources and those changes are not rolled back.
Disk graph cancellation prevents later nodes from starting and suppresses candidate publication/cache entries, while running core operations remain non-preemptible.
Chat, Local AI and Nodes cancel their children on owner drop; late cancellation prevents pending candidate import or final message publication.
The recipe queue preserves its bounded retention, source isolation and strict result validation, with a final state-locked publication gate.
A pre-spawn cancellation remains distinct from cancellation of an actual running child.
Chat limits a turn to 8192 accepted events, and the disk UI retains the most recent progress per node between pumps.
No model/runtime state or provider-specific client was added to Project.

Verification for 718290d:

- /tmp/runebender-ai-foundation-process-compile.log: cargo test --workspace --no-run --locked compiled every native test target after correcting a new coordinator test's Result assertion.
- /tmp/runebender-ai-foundation-process-focused.log: cargo test --locked --lib workflows:: -- --test-threads=1 passed all 54 focused tests, including callback unwind cleanup and actual core-node cancellation before disk writes.
- /tmp/runebender-ai-foundation-process-tests.log: cargo test --workspace --locked -- --test-threads=1 passed 1036 tests, 0 failed, 4 ignored across 23 suites before the final pre-spawn recipe receipt preservation adjustment.
- /tmp/runebender-ai-foundation-process-recipe-final.log: all 9 recipe queue tests passed on the final receipt logic, including the real v1/v2 Python example and cancellation before publication; the entire native suite was not repeated for this focused adjustment.
- /tmp/runebender-ai-foundation-process-clippy.log: strict all-target Clippy passed on the final code after two explicit-default style corrections in the CLI adapter.
- /tmp/runebender-ai-foundation-process-doc.log: workspace documentation passed.
- /tmp/runebender-ai-foundation-process-headless.log: the no-default-features library check passed.
- cargo fmt --all --check, bash .github/scripts/copyright.sh and staged git diff --check passed, including the shared process module.
- /tmp/runebender-ai-foundation-process-web.log: ./web/build.sh passed using the separate browser workspace/cache.
- /tmp/runebender-ai-foundation-process-browser-smoke.log: web/smoke.cjs passed at 1x density; the temporary loopback server was stopped afterward.

Validation exposed three fixture assumptions that were corrected without weakening the intended behavior checks.
The normal external-worker fixture now allows two seconds for cold executable startup while the deliberately hung case retains its short deadline.
The late-proposal fixture now opens the canonical document before the external disk proposal is written, so cancellation actually exercises candidate import suppression.
The running-child cancellation fixture waits for a child-created readiness marker instead of assuming queue Running means spawn has already completed.
The ignored real-model workflow test was updated for coalesced UI progress; it was not executed and remains outside runtime coverage.
No view code changed; the prior B5 density matrix and Gray/Light native captures were not repeated.
Native release/advisory checks remain part of H acceptance, and no Linux/Windows runtime proof or real model trial is claimed.
The existing block 0.1.6 dependency warning remains; no dependency was added.
Website guidance was audited read-only and remains accurate for this migration; H4 will document the new shared boundary and configured limits with the broader final changes.
Only font-ml help and existing source were inspected for C3 planning; no persistent server or model was launched.
All 21 original dirty baseline files were rehashed and remain unchanged; no push, merge, account connection, paid API call, model download or publication occurred.
The coordinator remains ACTIVE every 30 minutes because C3 and D-H are still incomplete.


2026-09-28: The sixth continuation completed C3 and revalidated C4 in aa5c81e50ddf373d8757a9396dab917e51008a11.
The coordinator verified a clean integration branch and no active implementation assignments or Cargo jobs before starting GPT-6 Sol workers persistent_client and persistent_chat_host with disjoint ownership.
The client worker owned workflows/local_chat.rs and its module export; the host worker owned Chat orchestration and configuration.
GPT-6 Luna audited the configuration documentation read-only.
The coordinator reviewed both worker diffs, added real HTTP/live-editor regression fixtures, implemented the document-lifetime publication guard, updated the panel and README, and serialized all Cargo validation.
All workers are complete and no D-phase assignment is active.

The client connects only to an explicitly configured literal loopback IP or localhost with a nonzero port, plain HTTP and root or /v1 path.
It uses the narrow fixed-length, nonstreaming completion protocol advertised by the installed font-ml serve help and inspected in the sibling source; the exact installed binary's inference implementation was not exercised.
It rejects remote endpoints, credentials, redirects, ambiguous framing, unsupported transfer encoding, oversized/truncated responses and malformed completion envelopes.
Configured model identity and stream=false are host-owned; no provider state or network client enters Project.
No new dependency was added.

The application sends at most seven completions and six read/proposal tool calls per turn, with a 64-message maximum for accepted inference context, a 1 MiB request/context cap and a 4 MiB HTTP response cap.
It checks a 120-second turn budget before inference and tool dispatch and after results; an already dispatched live operation remains non-preemptible under the existing socket timeout.
Cancellation closes the client request and suppresses later dispatch/publication, but does not stop computation in the independently managed server or undo prior proposal creation.
The server lifetime remains external and survives editor closure.
Only project_info, font_info, read_glyph, proof, proposal_list and propose_edits are exposed through this route.
The host replaces model-supplied document epochs, validates reply framing, allowed tools and top-level argument shapes for the complete call batch before dispatch, gives calls distinct transcript identities and does not retry uncertain live replies.
The canonical operation owner validates each tool's nested arguments and edit guards when it executes.
A changed document lifetime cancels the job and clears old model context before any pending final reply or artifact can publish, even when inference returned no tool calls.
The existing per-turn GGUF process path remains available when no endpoint is configured.
With an endpoint configured, malformed or non-UTF-8 endpoint/model values fail explicitly instead of falling back.

Verification for aa5c81e:

- /tmp/runebender-ai-foundation-persistent-client.log: all 6 focused local-client fixtures passed, including resident-server reuse, malformed/error/oversized/truncated responses, pre-cancellation and in-flight cancellation/deadlines.
- /tmp/runebender-ai-foundation-persistent-chat.log: all 12 Chat tests passed, including 4 host-loop tests and 3 real HTTP/live-editor integration fixtures.
- The real entry-point fixture ran with no selected GGUF or font-ml executable, read an unsaved width of 412, proposed 500, retained foreground width 412 and kept the document path absent on disk; the second user turn reused the same fixture server and conversation.
- The other integration fixtures prove a cancelled inference response cannot dispatch its tool and a completed no-tool response cannot publish into a replacement document lifetime.
- /tmp/runebender-ai-foundation-persistent-tests.log: cargo test --workspace --locked -- --test-threads=1 passed 1049 tests, 0 failed, 4 ignored across 23 suites.
- /tmp/runebender-ai-foundation-persistent-clippy.log: strict all-target Clippy passed after literal-suffix, unit-return semicolon and assertion-message corrections; these were the only code changes after the full native suite and did not change behavior.
- /tmp/runebender-ai-foundation-persistent-doc.log: workspace documentation passed.
- /tmp/runebender-ai-foundation-persistent-headless.log: no-default-features library check passed.
- cargo fmt --all --check, copyright and staged diff checks passed.
- /tmp/runebender-ai-foundation-persistent-web.log: the separate browser release build passed in 4m 08s.
- /tmp/runebender-ai-foundation-persistent-browser-smoke.log: the 1x headless browser smoke check passed; its temporary loopback server was stopped afterward.
- /tmp/runebender-ai-persistent-proofs/chat-gray.png and chat-light.png: inspected native headless captures show the configured server label and hidden per-turn model controls with idle scrollbars; no foreground GUI was launched.

Validation first caught a local variable shadowing the deadline helper, corrected before running the Chat fixtures.
The integration fixture also exposed that the new-font template already contains A at width 600; setup now makes the intended width 412 through the canonical edit API before testing the read.
No test expectations were weakened to accommodate those failures.
Four ignored model/large-workflow tests remain outside runtime coverage; no real model, resident-weight measurement, cloud provider, Linux/Windows runtime, native pointer/IME/accessibility/GPU proof or server restart recovery is claimed.
Native release/advisory checks remain in H, and the existing block 0.1.6 dependency warning remains.
README documents the consumed configuration route and cancellation/server-lifetime limits.
H4 should extend the website local-models guidance with the endpoint option and qualify the FAQ's “nothing is sent anywhere” wording to distinguish local loopback context transfer from cloud transfer.
All 21 original dirty baseline paths were rehashed and remain unchanged; no push, merge, publication, account connection, paid API call or model download occurred.
The 30-minute coordinator remains ACTIVE for D-H.


2026-09-28: Integrated canonical staged lineage and exact preview application as caf92b07ca27d1ae87b1b2f1184cd2cd3ffa7c93.
Two GPT-6 Sol workers implemented disjoint engine and automation slices; the coordinator reviewed both and connected native Nodes to the exact retained candidate.
A GPT-6 Luna worker reviewed the application authorization, cancellation, receipt, stale-result and release paths without editing them.
All workers are complete; no assignment is active at this checkpoint.

Project::extend_document_edit_transaction validates the parent's complete root read set and requires new guards to match its staged overlay.
It composes final snapshots into one atomic transaction while retaining original root guards and generated contour/point identities.
The original candidate remains immutable and may produce independent siblings.
Limits accumulate across the entire lineage: 64 unique guarded layers, 256 operations, 256 generated contours and 4096 generated points.
Net reversion produces no changed-object receipt or undo entry.
The recipe capture/staging adapters expose exact staged geometry and opaque guards without exporting a mutable font wrapper.
The existing native four-node flow now retains its proofed candidate and commits it directly through the shared application edit adapter, preserving authorization, cancellation, retry receipts and ordinary undo.
Previously Apply rebuilt AppendContours operations and therefore minted different identities from the preview.

Verification for caf92b0:

- /tmp/runebender-ai-foundation-lineage-build.log: all native test targets compiled with cargo test --workspace --locked --no-run.
- /tmp/runebender-ai-foundation-lineage-tests.log: full native suite passed 1057 tests, 0 failed, 4 ignored across 23 suites.
- The real application Nodes fixture runs Python, compiles both proof images, checks the exact preview snapshot after Apply, then verifies generated identities through ordinary Undo/Redo and receipt replay.
- /tmp/runebender-ai-foundation-lineage-bounds.log: all 6 lineage tests passed after adding two final test-only cases for the exact 4096-point and 64-layer limits and their rejection boundaries.
- The complete native suite preceded those last two test-only additions; no behavioral code changed afterward.
- /tmp/runebender-ai-foundation-lineage-clippy.log: strict workspace/all-target Clippy passed on the final implementation and tests.
- /tmp/runebender-ai-foundation-lineage-doc.log: workspace documentation passed.
- /tmp/runebender-ai-foundation-lineage-headless.log: the no-default-features library check passed.
- cargo fmt --all --check, copyright and git diff checks passed.
- /tmp/runebender-ai-foundation-lineage-web.log: separate browser release build passed in 4m 09s.
- /tmp/runebender-ai-foundation-lineage-browser-smoke.log: headless 1x smoke check passed with 140 measured frames, 4.7 ms median and 8.5 ms p95 on this host.
- Browser artifacts are under /tmp/runebender-ai-lineage-browser-proofs; the temporary loopback server was stopped.

This is a D2 foundation checkpoint, not completion of D1 or D2.
The live planner and host still accept the existing four-node comparison; chained nodes, branches, multiple independently configured proofs and partial failures are the next consumed integration.
No graph wire/file schema changed in this checkpoint, and no general DAG, provider, extension or restart-recovery acceptance is claimed.
Four ignored tests remain outside runtime coverage; real models, cloud providers, Linux/Windows runtime and native pointer/IME/accessibility/GPU behavior were not exercised.
Native release/advisory checks remain in H; the existing block 0.1.6 dependency warning remains.
All 21 original dirty baseline paths were rehashed unchanged.
No user font was saved, and no push, merge, publication, account connection, paid call or model download occurred.
The 30-minute coordinator remains ACTIVE for D-H.


2026-09-28: Completed D1/D2 and the current D5 compatibility scope in acf8a36835605cf8738dfff79fc45b066ba95e1f.
Two GPT-6 Sol implementation workers owned the pure session/planner and native runtime respectively; the coordinator integrated proof ownership, native controls, selected-result receipts and transport fixtures.
A separate GPT-6 Sol read-only review identified proof cleanup and legacy hash-format regressions; both were fixed and the final integration review found no remaining blockers in those paths.
All workers have finished; no assignment remains active at this checkpoint.

The version 2 plan has one root capture and deterministic topological order.
Each transform reads its parent's staged overlay and retains an immutable candidate with complete root guards.
Successful sibling branches continue when a transform or proof fails, and every blocked dependent receives a structured dependency_failed result.
Each proof uses its own captured recipe and exact parent content hash through the existing shared compiler worker.
A terminal partially_failed run retains its successful proofs and candidates; Apply requires an explicit transform selector when more than one proven result is eligible.
Native canvas selection maps a proof to its input transform; native Run requests execution version 2, while omitted MCP versions preserve legacy comparison behavior.
The host binds selected-result Apply receipts to graph identity, chosen node, request and complete candidate content, then commits the exact retained transaction with one ordinary undo step.
Completed historical artifacts may be inspected with stale metadata; late stale or cancelled work cannot publish new outputs or apply.
Cancellation and invalid terminal completion immediately release owned proof jobs, including proofs completed earlier in the same run.
Version 1 retains fail-fast script behavior; independent failure recovery is explicitly version 2 behavior.
Authoring/file schema version 1 remains unchanged and no save/reopen migration is required for existing graph files.

Verification for acf8a36:

- /tmp/runebender-ai-foundation-dag-build.log: all native test targets compiled with cargo test --workspace --locked --no-run.
- /tmp/runebender-ai-dag-contract.log: all 22 session/planner tests passed.
- /tmp/runebender-ai-dag-full-tests.log: full native suite passed 1071 tests, 0 failed, 4 ignored across 23 suites.
- The full suite preceded the final v1 fail-fast regression, proof-terminal cleanup refinement and internal ProofReady rename.
- /tmp/runebender-ai-dag-final-nodes.log: all 20 affected Nodes tests passed on the final implementation; one real-font workflow remained ignored.
- /tmp/runebender-ai-dag-final-live.log: all 7 real application/MCP fixtures passed on the final implementation, including old comparison clients and the new chained/branching DAG.
- The DAG fixture checks four actual compiled proof images and their parent hashes, partial failure, stable generated point IDs through selected Apply/Undo/Redo, exact retry, stale status, release and absence of a saved root font.
- /tmp/runebender-ai-dag-clippy.log: strict workspace/all-target Clippy passed.
- /tmp/runebender-ai-dag-doc.log: workspace documentation passed.
- /tmp/runebender-ai-dag-headless.log: no-default-features library check passed.
- Formatting, copyright and git diff checks passed.
- /tmp/runebender-ai-dag-web.log: separate browser release build passed in 4m 24s.
- /tmp/runebender-ai-dag-browser-smoke.log: headless 1x smoke passed with 142 measured frames, 5.1 ms median and 8.7 ms p95 on this host.
- Browser artifacts are under /tmp/runebender-ai-dag-browser-proofs; the temporary loopback server was stopped.
- /tmp/runebender-ai-dag-proofs/nodes-gray.png and nodes-light.png: completed native comparison captures inspected in both themes.

The first full run exposed an intermittent existing disk cancellation fixture failure; the isolated reproduction passed and the original cause was not conclusively reproduced.
Local commit b867c75141382ca16d708a171588da13894af4da strengthens that fixture to verify the actual progress callback, cancellation flag and absence of subsequent worker dispatch, with diagnostic events and a five-second deadline.
The strengthened fixture passed in the full rerun; this is a test change, not a claimed production cancellation repair.

This milestone executes local Python transforms and compiler proofs, not model/provider nodes.
External model-worker bridging, structured Rows/effects/cache policy, extension permission manifests, cloud/vector adapters and durable recovery remain open.
Script nodes currently require guarded edits; report-only transforms and multi-parent merge nodes are not supported by this execution contract.
Python is trusted local execution, not an OS sandbox, and compiler cancellation abandons results rather than claiming to interrupt a running compiler.
Four ignored tests remain outside runtime coverage; no real models, cloud providers, native pointer/IME/accessibility/GPU behavior or Linux/Windows runtime were exercised.
Native release/advisory gates remain in H, and the existing block 0.1.6 future-compatibility warning remains.
H4 must update maintained workflow/MCP/architecture guidance from the old four-node-only description and explain execution versions, node selectors, partial results and current limits.
No dependency was added, no user font was saved, and no push, merge, publication, provider/account connection, paid API call or model download occurred.
All 21 original dirty baseline paths were rehashed unchanged.
The 30-minute coordinator remains ACTIVE for the remaining checklist work.


D3 preparation from the D4 integration review:

- Add a read-only detached capture export beside `Project::save_as`, reusing `SaveAsPlan` and the canonical persistence adapters.
  Calling `save_as` itself would retarget source paths, clear dirty flags and bump the document revision.
  Preserve all sources, designspace, feature includes and preservation payload; let the run own temporary-directory lifetime.
- Separate the worker's capture path from the original master identity in `AiJob`.
  Existing completion checks compare the open source with the worker path, which would reject detached results.
- Both native `local_ai::run_task` and disk-oriented `nodes::run_nodes` currently save first.
  Replace this route when bridging model nodes; do not reuse it as a live graph backend.
- Do not call `adopt_proposal_from_disk` on successful completion.
  It discards an existing proposal and mutates Project; `adopt_external_project` is a per-glyph mutation loop without captured revision validation.
  Validate the exact expected proposal, nonempty scoped glyphs, source identity and captured/current revisions, then retain a session candidate for explicit Apply through the existing atomic transaction/history owner.
  Reject stale/cancelled results and avoid treating proposal layers already present in the capture as newly generated output.
- Use one real offline subprocess fixture matching the existing `font-ml bolden` CLI contract before claiming model-node support.
  No real model or sibling font-ml changes are authorized by this milestone.


2026-09-28: D4 execution policy and structured Rows milestone.
Two GPT-6 Sol workers owned catalog metadata and runner behavior; the coordinator owned real CLI fixtures, documentation, integration review and serialized validation.
Independent review found and corrected two cache holes before acceptance: external mutator declarations losing their no-cache restriction, and changed model files failing to invalidate downstream workers when manifest digests stayed unchanged.

Implemented behavior:

- Every builtin and live node declares versioned cache/effect/Rows metadata; native and MCP discovery serialize the same catalog.
  Legacy external tasks default to never-cache and unsupported Rows inputs.
  Explicit task metadata requires schema version 1 and known fields; malformed/future policies are not registered.
  External tasks are always classified as trusted processes, and declared font/session effects force never-cache.
- The disk runner consumes policy for cache reads and writes, fingerprints the node definition and retained output bytes, and invalidates old cache files.
  Model and adapter inputs and outputs hash complete directory contents, including weights, tokenizers and additional shards; a manifest digest alone is insufficient.
  This deliberately adds disk I/O for large models.
  Source-writing builtins and live document nodes remain uncached.
- Opted-in Rows inputs travel as one compact JSON array per normalized CLI port flag without shell evaluation.
  Empty/nested/Unicode data survives real linked subprocess execution.
  The runner rejects unsupported inputs, more than 4096 rows per input, or more than 65536 aggregate encoded bytes before spawning the worker.
  Declared Rows outputs must actually be arrays; missing/malformed output is an error.
- README documents the implemented worker ABI and compatibility/trust limits.
  No models, cloud providers or accounts were used.
  The existing live model-worker save-first behavior remains D3 work and is not counted as resolved by this metadata milestone.

Native verification on the final D4 code:

- /tmp/runebender-ai-d4-final-tests.log: complete native suite, 1081 passed, 0 failed, 4 ignored across 23 suites.
  Includes the real linked Rows CLI fixture, metadata-only policy changes, executable/artifact invalidation, changed model/adapter files, bounds, malformed outputs and existing live MCP/Apply/Undo regressions.
- /tmp/runebender-ai-d4-clippy.log: strict all-target Clippy passed with warnings denied.
- /tmp/runebender-ai-d4-doc.log: workspace documentation passed.
- /tmp/runebender-ai-d4-headless.log: no-default-features library check passed.
- Formatting, copyright and git diff checks passed.
  The existing block 0.1.6 dependency future-compatibility warning remains.
  Ignored real-model tests are not runtime coverage; release/advisory/final clean-checkout acceptance remains in H.

D4 integrated commit: 870cf65709ee7733574cd32e128eb5b542216e0a.

- /tmp/runebender-ai-d4-web.log: browser release build passed in 4m 23s.
- /tmp/runebender-ai-d4-browser-smoke.log: repository headless browser smoke passed at 1x density.
  The temporary loopback server was stopped.
  No views changed; this does not claim foreground native pointer/IME/GPU validation or real model runtime coverage.
- All 21 original dirty baseline paths remain byte-for-byte unchanged.
  No pushes, merges, account connections, model downloads or worktree deletions occurred.
  D3 and E-H remain open, and the coordinator remains active every 30 minutes.


2026-09-28: User-requested wrap-up after native detached model-worker checkpoint 273562b3824c70a52825b51002bad205b16912e6.
Two GPT-6 Sol workers implemented disjoint canonical export and proposal-candidate boundaries; the coordinator connected native review/Install/history, reviewed both patches, and added real offline subprocess fixtures.
No worker assignment remains active.

Native Local AI now exports current in-memory data, including unsaved changes, into an owned temporary directory without saving or retargeting the source.
The export preserves sources, designspace and feature dependencies and removes only the expected task's old proposal layer from the copy.
The worker receives that detached UFO; completion validates source/revision/scope and supported geometry, then retains a candidate outside Project's persistent layers.
Install commits the exact guarded transaction as one history group; ordinary Undo/Redo and Undo install use the existing history owner.
Discard and cancellation release session state without changing the root.
Candidates are keyed by source and task so independent masters cannot hide or overwrite each other's results.
At most 16 candidates are retained; each captures at most 64 glyphs and 256 changed point/anchor/width values.
Components, hyperbeziers, topology changes and unsupported metadata are rejected explicitly.
Existing saved proposals for the same task require explicit review/discard before another native model run.
These are bounded offline-verified contracts, not claims of successful real model/provider integration.

Verification for 273562b:

- /tmp/runebender-ai-d3-capture.log: 3 capture/export tests passed, including dirty source state, unsaved font, designspace/includes and unchanged original bytes/history.
- /tmp/runebender-ai-d3-candidate.log: 3 candidate tests passed, including exact staged IDs, all-or-nothing staging, grouped undo, stale/out-of-scope/malformed/unsupported results.
- /tmp/runebender-ai-d3-local-ai-final.log: 17 Local AI tests passed, 1 real-model test ignored.
  Real offline subprocesses verified unsaved inputs, unchanged source/root until Install, atomic Undo/Redo, per-master candidates, rejected output and cancelled-result cleanup.
- /tmp/runebender-ai-d3-boundaries.log: all 6 architecture-boundary tests passed.
- /tmp/runebender-ai-d3-clippy-final.log: strict workspace/all-target Clippy passed.
- /tmp/runebender-ai-d3-headless.log: no-default-features library check passed.
- /tmp/runebender-ai-d3-web-check-final.log: release-profile wasm32 browser compilation check passed.
  The first browser check found native-only history methods referenced from shared code; explicit browser-unavailable guards fixed it, and native tests/Clippy were rerun afterward.
- Formatting, copyright and staged diff checks passed.

To keep this requested wrap-up bounded, the complete native suite and browser release rendering/smoke matrix were not repeated for this patch.
The prior D4 full suite remains 1081 passed with 4 ignored; it is baseline evidence, not a final-patch full-suite claim.
Native Gray/Light screenshots, real model inference, cloud providers, release/advisory and clean-checkout acceptance are not claimed for this patch.
The existing block 0.1.6 future-compatibility warning remains.
All 21 original dirty baseline files remain unchanged.

The user explicitly chose: finish this patch, then stop recurring work.
The automation update confirmed runebender-ai-architecture-pass is PAUSED.
D3's remaining graph work, E-H, cloud/QuiverAI adapters, extension registries and durable recovery are deferred rather than silently expanded into more scheduled work.
