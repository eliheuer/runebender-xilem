# Temporary AI architecture checklist

This task coordinates the implementation of the September 2026 architecture review.
The goal is a reliable foundation for agent editing, reusable tools, and node workflows that can use local or cloud model workers.
Every completed item needs integrated code, focused regression coverage, and recorded verification.
This file is temporary working state; retire it after the final acceptance review and move enduring guidance into the maintained documentation.

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

- [ ] C1. Define a common job lifecycle with explicit cancellation capability, deadlines, output bounds, progress, and retained results.
  Reuse existing queues where they already satisfy this contract.
- [ ] C2. Migrate disk workflow, Local AI, and Chat subprocess supervision onto the supported lifecycle.
  Test hung, noisy, failed, cancelled, and late-result workers.
- [ ] C3. Support a persistent local-model worker adapter without making model memory or runtime state part of Project.
- [ ] C4. Keep browser availability explicit and preserve native/browser build boundaries.

### D. Generalize live graphs

- [ ] D1. Replace the fixed four-node comparison planner with a validated directed-acyclic-graph execution plan.
  Acceptance: chained transforms, branching, multiple proofs, and partial failure.
- [ ] D2. Preserve immutable version lineage, semantic hashes independent of layout, bounded retention, and guarded result application.
- [ ] D3. Bridge external workers through detached captures and validated candidate imports; remove implicit root-save requirements from live execution.
- [ ] D4. Make cache policy and side effects explicit per node implementation; carry structured Rows inputs faithfully.
- [ ] D5. Version graph/node contracts and preserve existing saved graphs through an explicit compatibility path.

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


Next dependency-ready work: C1/C2 shared subprocess supervision consumed by existing disk workflow, Local AI and Chat callers.
All B items are complete for their recorded acceptance scope; provider execution, general DAGs and extension registries remain separate implementation work.
Start from the existing script_jobs deadline/cancellation/output capture implementation and preserve its browser-unavailable behavior.
The current Local AI, Chat and disk workflow subprocess paths read unbounded output and do not share the recipe runner's deadline contract.
Agree the reusable native process boundary before assigning at most two Sol workers with disjoint supervisor and caller ownership; keep it outside Project and provider-specific model state.
Retain bounded progress/results, make cancellation capabilities explicit, and test blocked stdin, hung/noisy workers, descendants retaining standard handles, failed exits, cancellation races and late results.
A shared unused scheduler is not acceptance; migrate the actual callers and preserve their proposal, conversation and graph semantics.
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
