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
- [ ] B2. Centralize transport-facing schemas, result contracts, and effect metadata while preserving strongly typed engine APIs.
- [ ] B3. Return structured MCP results and truthful schemas/annotations, preserving text and proof images for compatibility.
  Verify CLI/MCP parity, errors, retries, and cancellation against supported protocol versions.
- [ ] B4. Extend recipe captures to useful outline operations and provide guarded structural editing for shape generators.
  Verify exact retries, stale reads, full-batch rejection, dependent glyph invalidation, and one-step undo.
- [ ] B5. Remove identified reverse dependencies: engine mark metadata must not depend on loading a UI theme; live workflows must call typed engine operations rather than a transport dispatcher.

- [ ] B6. Preserve explicit source creation timestamps at the canonical metadata/import boundary and define a deterministic absent-date fallback.
  Verify repeat open/compile behavior separately from same-snapshot compiler determinism.

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

The first shared-operation slice should remove the live workflow's call into the JSON transport dispatcher and establish explicit result/effect descriptors with existing consumers.
The structural-edit slice must prove generated contours can preview, reject stale input, apply atomically and undo before expanding provider integration.
The cache fix hashes full UFO inputs conservatively; precise dependency-aware caching is a later performance improvement, not a prerequisite for correctness.
Compiler timestamp normalization covers repeated builds of the same captured Babelfont input.
A separate metadata follow-up is required before claiming reproducibility across reopening source files: canonical UFO import currently retains `openTypeHeadCreated` in source-format preservation but does not map it into `font.date`, and an undated import samples its creation fallback on load.
Resolve that at the canonical import/metadata boundary, not with a compiler-only UFO projection workaround.

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


Next dependency-ready work after A: B5 reverse dependencies, followed by the consumed B2/B3 operation descriptors and structured MCP results.
Use at most two disjoint implementation workers and keep source-date work B6 separate from transport changes.
The recurring coordinator is active every two hours and should remain quiet unless there is a verified milestone, actionable failure, completion, or user decision.
