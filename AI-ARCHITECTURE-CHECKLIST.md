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

- [ ] A1. Fix disk graph cache inputs, including font metadata and relevant dependencies, preserving unchanged-run hits.
  Regression: a metrics change regenerates the proof; changed executable identity invalidates an external node result.
  Initial owner: workflow_cache (GPT-6 Sol).
- [ ] A2. Make identical canonical compiler inputs yield stable bytes without wall-clock test flakes.
  Preserve source creation metadata and actual semantic undo coverage.
  Initial owner: compiler_determinism (GPT-6 Sol).
- [ ] A3. Repair the Norad boundary gate for moved modules and out-of-line test-only modules without weakening production boundaries.
  Initial owner: architecture_gate (GPT-6 Luna).
- [ ] A4. Run focused checks, then the relevant native suite; record remaining known failures honestly.

### B. Establish shared operation contracts

- [ ] B1. Inventory current command, proposal, live-edit, recipe, and node capabilities; choose one typed owner per operation.
- [ ] B2. Centralize transport-facing schemas, result contracts, and effect metadata while preserving strongly typed engine APIs.
- [ ] B3. Return structured MCP results and truthful schemas/annotations, preserving text and proof images for compatibility.
  Verify CLI/MCP parity, errors, retries, and cancellation against supported protocol versions.
- [ ] B4. Extend recipe captures to useful outline operations and provide guarded structural editing for shape generators.
  Verify exact retries, stale reads, full-batch rejection, dependent glyph invalidation, and one-step undo.
- [ ] B5. Remove identified reverse dependencies: engine mark metadata must not depend on loading a UI theme; live workflows must call typed engine operations rather than a transport dispatcher.

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

## Execution record

2026-09-28: Created an isolated integration worktree and copied 21 pre-existing dirty paths into a local baseline commit.
The original checkout remains untouched.
Started three independent correctness workers.
No checklist implementation item has passed coordinator review yet.
