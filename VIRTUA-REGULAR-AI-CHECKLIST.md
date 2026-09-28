# Virtua Regular AI workflow checklist

Status: B1, B2, C1 and C2 validated; B3 assigned next; trial preflight complete, target preference requested.
Created: 2026-09-28.
Coordinator: this Codex task, with Astra owning architecture and integration review.
Worktree: /Users/eli/.codex/worktrees/virtua-regular-ai/runebender-xilem.
Branch: codex/virtua-regular-ai.
Starting commit: c9cb462db20e02c4908d4278fbe3737b296f8760.
Automation: runebender-ai-architecture-pass, repurposed exclusively for this checklist at a 20-minute interval.

## Outcome and scope

Finish Virtua Grotesk Regular efficiently by developing and proving a reusable workflow for one unfinished Arabic glyph at a time.
The first acceptance milestone is one missing Regular Arabic form drafted from a sketch or model image, compared against approved references, then improved using actual specimen feedback.
Software completion and Eli's aesthetic approval are separate gates.
Do not claim the Regular font is finished because these software gates pass.
After the first human-reviewed milestone, report the evidence and stop this implementation campaign; further glyphs use the resulting workflow without an open-ended architecture expansion.

Two user-facing actions share the same machinery:
- Draft this Arabic glyph: reference selection, optional sketch, a few candidate generators, calibrated tracing, comparison, explicit Apply/Undo.
- Refine this glyph: a comment such as "this looks too heavy in the paragraph" drives bounded propose/render/compare iterations, retaining the best earlier result.

Regular only.
No Bold work, whole-font batches, training campaign, ComfyUI dependency, general plugin marketplace, or unrelated architecture cleanup.
ComfyUI is an interaction reference: typed wires, immutable branch versions, reusable recipes, visible progress and alternatives.
This is Runebender's own workflow engine and UI.
MCP, CLI and the native interface adapt the same document/workflow operations.
Agents may author a validated workflow from English; creation, execution, and application are separate actions.
Cloud reasoning/vision and local specialized inference have separate roles and must not be conflated with MCP transport.

## Isolation and execution rules

- Preserve /Users/eli/GH/repos/runebender-xilem and all concurrent icon/font work.
Do not edit assets/icons, the user's live editor, or original Virtua sources.
Use scratch copies for model and proof trials.
- Only this worktree is the implementation branch.
Read AGENTS.md and relevant module headers; follow Linebender formatting and one prose sentence per source line.
The fetched website architecture page is older than current code; use current Project ownership and repository AGENTS.md when they disagree.
- The coordinator owns this checklist, assignment ledger, review, validation and local commits.
Workers do not edit the checklist, commit, push, merge, remove worktrees, or change Git branches.
Stage explicit paths only.
- At most one GPT-6 Sol implementation worker and one GPT-5.6 Terra documentation/fixture worker at once.
Use bounded assignments with disjoint files.
No duplicate assignment while a worker is active.
Use this task/Astra for contract decisions and difficult integration.
- Aim for a concrete reviewable patch per 10-30 minute worker assignment.
At the 20-minute heartbeat inspect state and continue dependency-ready work, rather than re-researching the plan or reopening completed gates.
A heartbeat is a continuation opportunity, not a promise of completion at that exact time.
- Serialize all Cargo jobs in this task.
Use CARGO_TARGET_DIR=/Users/eli/GH/repos/runebender-xilem/target.
Use CARGO_BUILD_JOBS=2 until host memory is established; workers may not launch Cargo.
Wait for existing builds; never kill unrelated processes.
- Run focused tests for each patch; broader native/browser checks only when affected and at final integration.
Record exact commands, results, commits and validation limits.
Offline fixture success is not real-model or aesthetic evidence.
- Existing local checkpoints may be used in bounded scratch trials.
Do not download weights or start training.
An available authorized image-generation tool may produce a selected scratch glyph when the reference package is ready.
Do not connect accounts, provision services or start separately billed API loops without an established budget.
Prepare offline adapter fixtures while account-dependent validation is unavailable.
- No foreground GUI launch while Eli is drawing.
Use headless proofs; ask for a coordinated interactive review only when required.
- Notify on meaningful verified milestones, blockers requiring input or completion.
Keep unchanged heartbeat runs quiet.
If only human review, provider access or another external prerequisite remains, preserve all evidence and pause the automation rather than repeatedly spending tokens.
Pause the automation when this bounded checklist is complete.
Do not revive AI-ARCHITECTURE-CHECKLIST.md's deferred roadmap.

## Design and grading contract

- Green: approved style/weight/geometry references; never edit.
Purple: protected, never edit.
Yellow/orange: explicitly selected refinement targets.
Red: explicitly selected empty/junk/unfinished targets eligible for replacement.
Blue: AI candidate awaiting Eli's grading.
Unknown/uncolored values are not automatic targets.
Read actual color values, not guesses from screenshots.
Never automatically mark a glyph green.
- Select relevant green Arabic relatives by intended form, bowl, terminal, dot, joining behavior and weight.
Do not pick the alphabetically first green glyphs or substitute Latin anchor measurements.
Snapshots record reference names, source, revision, grades and measured geometry.
- A target is one actual glyph/form in one Regular source, not merely a Unicode character.
Resolve a clicked specimen occurrence through shaping glyph IDs/names and clusters.
Arabic contextual forms, ligatures and marks must remain identifiable.
- A sketch conveys shape intent; approved glyphs and the design contract supply weight and style.
Measure and verify placement from an explicit pixel-to-font transform; do not stretch every image to the full em.
Strip reference-sheet guides from ink and retain calibration data.
- Reuse good geometry where appropriate; image generation is not compulsory.
Compare a direct trace, local sketch interpretation and cloud-image trace only where useful, with at most three initial candidates.
Do not rerasterize an already editable model outline.
- Preserve img2bez extrema/handle/line structure and deliberate chamfers.
Do not apply generic simplification that destroys those constraints.
Tracing does not establish Arabic correctness, style quality or release readiness.
- Structural defaults prefer the established grid; optical refinement may propose bounded fine-grid moves consistent with the font's 2-unit correction vocabulary.
The user's current request authorizes such proposals.
Do not resnap accepted optical adjustments onto the coarser grid.
Curve continuity and perceived weight outrank pleasing coordinate numbers.
- Classify "too heavy" before editing: stroke thickness, dark junction, small counter, terminal, proportion or spacing.
Default to narrow point/handle edits preserving advance, joining placement, dots and anchors; expose intentional exceptions.
Do not apply a whole-outline shrink blindly.
- Freeze text, viewing size, wrapping/layout, direction, language, features, colors and renderer identity across comparisons.
Retain original, current candidate and best prior candidate.
Show reading-size context plus a magnified detail and at least a second word context.
Screen proofs do not establish print behavior.
- Optical loop default: at most three rounds, at most two candidate alternatives per round, one glyph.
Record wall-time/attempt budget and external usage where available.
Stop on no improvement, disagreement/uncertainty, invalid geometry, cancellation, stale input or budget exhaustion.
A model may critique/rank between iterations; that is not Eli's grade.
Do not ask Eli to approve every internal iteration.
Apply only an explicitly selected current candidate as one ordinary undo group.
- Preserve all proposals until deliberately discarded.
Store model/checkpoint, prompt, seed where supported, parameters, source/reference hashes, calibration, edits, proof recipes, model assessments and human feedback.
Human before/after corrections are potential training examples, not automatically pure optical truth or automatic training authorization.

## Reuse and known implementation gaps

- Root Project is the canonical document owner.
Babelfont/Norad stay behind font/format adapters.
- src/font/compiler/proof.rs provides immutable compile inputs, shaped glyph identities and PNG proofs.
B1 now provides bounded viewing-size/layout settings with the original 160 px/em default preserved.
Matched context capture and real reading-size comparison remain B2/B3.
- src/application/platform/live_proofs.rs binds epochs/revisions.
src/application/editor/tools/nodes and src/workflows/nodes_session.rs retain branches/proofs/candidates and explicit Apply.
Version 2 comparison currently does not enforce identical proof recipes.
- src/font/project/edit_transactions.rs and src/automation/agent_edit.rs support points/anchors/width/append.
C1 now wires contour replacement through guarded live edits and private candidate compilation.
Image calibration and user-facing candidate integration remain C2/C3.
- src/formats/image_trace.rs accepts image bytes and returns canonical contours.
Current UI tracing replaces the glyph and fits the ascender-descender band; draft workflows need calibrated, staged import.
- /Users/eli/GH/repos/font-garden-lab/glyphlab/sketch2glyph.py has the earlier MLX sketch-to-outline implementation.
runs/sketch1 currently points to clean1; sketch4 and sketchpre weights were observed locally.
Pin and inspect a concrete checkpoint before a trial; a symlink name is not stable model identity.
The sketch model does not consume a fresh sheet of green references at inference.
Missing-Arabic quality is unverified.
- /Users/eli/GH/repos/font-ml/src/task.rs marks complete/generate unavailable.
Reuse or adapt the actual sketch runtime rather than advertising an unimplemented task.
Do not assume the bolden checkpoint can invent missing shapes.
- The Virtua image-generation harness already has reference sheets/calibration/extraction.
Older docs/metrics/grade handling conflict in places; measure current Arabic greens and record discrepancies.
Do not blindly port old whole-UFO save or install scripts into the editor.
- font-garden-lab/optics/extract_deltas.py extracts historical orange-to-green differences.
Check provenance/topology/unrelated changes before treating any such difference as optical training data.

## Checklist and acceptance gates

### A. Context and fixtures (Terra; no code dependency)
- [ ] A1. Add a compact agent-readable optical/reference context under tests/fixtures/glyph_workflow, reusing the authoritative Virtua design/Arabic docs.
- [ ] A2. Include fixtures for a missing Arabic form and a "too heavy in paragraph" refinement, with all grades, explicit Arabic references, protected shapes, proof conditions, limits and an abstain outcome.
- [ ] A3. Distinguish structural hard constraints, measured reference data, design preferences, model assessments and human grades.
Acceptance: coordinator reviews sources and semantics; JSON parses; later B/D consume these fixtures rather than leaving decorative schemas.

### B. Reproducible proof context (Sol; independent of C)
- [x] B1. Add bounded, backward-compatible viewing-size/layout settings to compiled proof recipes and render from them; freeze defaults for older recipes.
- [x] B2. Capture editor text settings and exact shaped target occurrence; send the same recipe to baseline and candidate proofs; reject comparison recipe mismatch.
- [ ] B3. Supply reading-size and enlarged-detail proof artifacts with document/font/recipe/renderer identity.
Acceptance: meaningful geometry/size tests, invalid/oversized input rejection, old fixture compatibility, Arabic contextual/ligature/mark occurrence identity and matched baseline/candidate recipes.
Actual headless proof inspection is required before visual claims.

### C. Safe missing-outline candidates (Sol; follows B1 to avoid proof.rs ownership conflict)
- [x] C1. Add bounded complete-outline replacement through canonical guarded transactions, agent operations and private candidate compilation.
- [x] C2. Add calibrated image/model-outline import into a detached candidate; preserve unrelated width/anchors/components/metadata unless explicitly addressed.
- [ ] C3. Make the same candidate inspectable/selectable through existing UI and MCP, with one Apply/Undo and no automatic save.
Acceptance: empty-to-drawn and junk-to-replaced, no root mutation before apply, stale rejection, unrelated/green preservation, proof of replacement, undo/redo restoration.
Do not repair or synthesize Bold to make the trial pass.
Regular-only drafts must not be mislabeled variable-font release artifacts.

### D. Reference selection and generator execution (Sol; context A, candidates C)
- [ ] D1. Resolve grades and relevant Arabic references from current canonical/scratch data; no arbitrary alphabetic selection; refuse protected/unknown automatic targets.
- [ ] D2. Run a pinned installed sketch checkpoint over one calibrated scratch image, returning an outline without invoking its unsafe direct-install option.
- [ ] D3. Export a cloud reference package and ingest a generated image with provenance via MCP/workflow operations; reuse available image-generation clients before adding an account-settings subsystem.
- [ ] D4. Retain at most three initial candidates and a direct-trace baseline where relevant; expose honest unsupported/missing-runtime errors and cancellation.
Acceptance: offline process/provider fixtures and at least one recorded real local-model result plus one actual cloud-image-to-outline result or a clearly recorded external blocker.
No claimed model quality from mocked responses.

### E. Bounded optical refinement (Sol; B/C/D)
- [ ] E1. Implement one-glyph propose/render/compare loop state with attempt/time limits, model critique/ranking, abstention and preserved best candidate.
- [ ] E2. Provide constrained point/handle edits for targeted optical hypotheses; enforce protected glyphs and stale guards; leave final human grade separate.
- [ ] E3. Expose the loop as an executable, inspectable recipe through MCP and a minimal native comparison surface.
Acceptance: worse/failed attempts retain best; no-progress/uncertainty and budget stop; cancelled/late results cannot apply; equal proof conditions; one selected Apply/Undo.
A model may iterate autonomously within this scope, but tests must not equate a model score with aesthetic truth.

### F. Integrated trial and handoff (coordinator)
- [ ] F1. Choose one actual unfinished Regular Arabic form from a scratch copy with current green references; record baseline and why the target is eligible.
- [ ] F2. Produce and inspect candidates, render shaped words and paragraph at reading size, and exercise a bounded optical correction from explicit feedback.
- [ ] F3. Record useful vs rejected candidates, elapsed time, remaining manual corrections, exact input/runtime identities and all validation limits.
- [ ] F4. Run relevant native/browser regression checks, review all diffs, commit coherent changes locally and provide Eli a reproducible launch/recipe.
- [ ] F5. Obtain Eli's visual verdict; only Eli can make the glyph green.
If only F5 or another external prerequisite remains, report "ready for review" and pause instead of pretending the checklist or font is complete.
- [ ] F6. Pause the coordinator after completion or the review handoff; summarize actual results and remaining font work without reopening broad architecture tasks.

## Assignment ledger

| Assignment | Worker | Owned files | State |
| --- | --- | --- | --- |
| A1-A3 | regular_context_terra (GPT-5.6 Terra) | tests/fixtures/glyph_workflow/** only | Reviewed and JSON-validated; fixture consumption remains pending |
| B1 | regular_proof_sol (GPT-6 Sol) | src/font/compiler/proof.rs and five recipe constructor call sites | Validated; d6f50f7 |
| C1 | regular_proof_sol (GPT-6 Sol), reused | src/font/project/edit_transactions.rs, src/font/babelfont/edit_contours.rs, src/automation/agent_edit.rs, src/automation/agent_edit/results.rs, src/font/compiler/proof.rs; narrowly necessary exhaustive-match sites | Validated; c57edc3 |
| C2 | regular_proof_sol (GPT-6 Sol), reused | src/formats/image_trace.rs, src/automation/agent_edit.rs; narrowly necessary canonical contour conversion helper only | Validated; a67dfe4 |
| B2 | regular_proof_sol (GPT-6 Sol), reused | src/text/buffer/**, src/font/compiler/proof.rs, existing editor/proof request call sites as needed | Validated; 621bf11 |

B2 correction dispatch initially hit capacity; after Terra completed, one retry successfully resumed Sol.
Terra regular_trial_inventory completed tests/fixtures/glyph_workflow/local-trial-inventory.md; coordinator reviewed it.
B2 review is resolved; B3 is the next bounded assignment.
B3 ownership: regular_proof_sol, src/font/compiler/proof.rs (or adjacent proof artifact module), src/formats/designbot.rs, and existing proof result adapters only as needed.
B3 must deliver callable paired reading/detail artifacts and honest renderer provenance; no UI redesign or model inference.
Coordinator retains all Cargo validation and checklist ownership.
Terra has completed the reviewed fixture preparation.
The earlier capacity limit cleared after Sol completed; the queued Terra assignment was dispatched once.
Worker completion means a patch is ready for review, not that its acceptance gates passed.
Record worker IDs, handoffs and scope changes here before dispatch.
Workers report tests recommended, do not run Cargo or stage changes.

## Evidence log

- 2026-09-28: coordinator read current main, clean at c9cb462, created isolated worktree and codex/virtua-regular-ai.
No font or icon edits.
No checklist implementation acceptance is claimed yet.
- Process inspection was initially sandbox-blocked; a scoped read subsequently confirmed no Cargo/Rust compiler process before testing.
Use two build jobs and coordinator-only Cargo until host memory is confirmed.
- B1: d6f50f7 adds explicit proof scale/layout/colors with legacy JSON defaults preserved.
`RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources CARGO_TARGET_DIR=/Users/eli/GH/repos/runebender-xilem/target CARGO_BUILD_JOBS=2 cargo test --locked font::compiler::proof::tests -- --test-threads=1` passed: 13 tests, zero failures or ignored tests.
The first attempt without RUNEBENDER_TEST_FONTS had four missing-fixture failures; the configured rerun resolved them.
`cargo fmt --all --check` and `git diff --check` passed.
This validates B1 only: advance-based wrapping remains; no paragraph layout or visual-quality claim, no B2/B3 acceptance, and no real-model trial yet.
Main remains clean and unchanged; no font/icon edits.
- Context fixtures: coordinator reviewed the three files and requested corrections for source attribution, explicit detached state, illustrative context labels and the authorized fine-grid proposal policy.
`jq -e 'type == "object"' tests/fixtures/glyph_workflow/*.json` passed for both files; `git diff --check` passed.
The worker-reported `jq -e empty` parses input but returns exit 4, so the coordinator used the explicit successful object check instead.
A gates remain unchecked until executable consumers use the fixtures; synthetic labels are not Arabic proof evidence.
- C1: c57edc3 provides guarded replacement/clearing, distinct removal receipts, preserved non-outline data and detached candidate compilation.
Tests ran serially with RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources, CARGO_TARGET_DIR=/Users/eli/GH/repos/runebender-xilem/target and CARGO_BUILD_JOBS=2.
`cargo test --lib --locked font::project::edit_transactions::tests -- --test-threads=1`: 18 passed.
`cargo test --lib --locked automation::agent_edit:: -- --test-threads=1`: 8 passed.
`cargo test --lib --locked font::compiler::proof::tests -- --test-threads=1`: 14 passed.
`cargo test --lib --locked automation::script_recipe::tests -- --test-threads=1`: 10 passed.
No ignored tests or failures in those four suites.
`cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check` and `git diff --check` passed.
Initial Clippy found six default-trait-access warnings from B1 constructors; f0973b7 corrects them without behavioral changes.
Cargo still reports a dependency future-incompatibility notice for block 0.1.6.
Replacement compile coverage is a single-source Regular UFO; multi-master topology may reject and no other master is modified.
Browser build/smoke, actual Arabic visual proof, protected-grade policy and real-model trials remain pending integration gates.
Context fixtures are committed at 17e920b; executable consumption remains pending.
- C2: a67dfe4 adds explicit full-image pixel calibration, image/lockfile provenance and guarded replacement adaptation.
With the established test-font/target/jobs environment, `cargo test --lib --locked formats::image_trace::tests -- --test-threads=1` passed 5 tests and `cargo test --lib --locked automation::agent_edit::tests -- --test-threads=1` passed 6 tests.
`cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check` and `git diff --check` passed.
Coordinator checked pinned img2bez 80bdb2e neutral-em coordinate semantics against the explicit calibration math.
Legacy tracing is retained; structured outline models can use C1 replacement directly without rasterization.
Encoded images are capped at 4 MiB, 1024 pixels per edge and 262144 total pixels; larger generator outputs need resizing with adjusted calibration before import.
Synthetic tests establish placement and detached staging, not Arabic style quality; C3 user-facing retention/provenance integration and real-model trials remain pending.
- B2 review, not accepted: 13-file uncommitted patch adds selected occurrence capture and a canvas/panel/Workspace event bridge plus pair validation.
The narrow ownership expansion into existing view/canvas/editor.rs, view/panels/editor.rs, workspace.rs, platform/host.rs and nodes/execution.rs was necessary for a connected implementation.
`selected_arabic_occurrence_survives_real_compiled_proof_and_rejects_drift` fails at the auto-direction Arabic mark after TextBuffer::clear: proof_selection reads pinned fallback direction rather than the effective line direction.
The 33-test canvas editor suite has 31 passes and two event-queue failures: parked_text_does_not_consume_outline_tool_typing and select_double_click_activates_the_composed_sort.
Tests must explicitly verify the additional proof event without weakening text/edit assertions.
The starter recipe test passes with scoped local Unix-socket permission; its first sandbox run failed only because the native live socket was unavailable.
The v2 comparison-pair test rejects mismatched pair recipes but its distinct-DAG acceptance assertion fails; diagnose the actual validation error and preserve valid differing proof recipes outside comparison pairs.
All-target Clippy reports collapsible_if in proof target validation and two usize-to-u32 truncation casts; fix structurally.
Review also requires an explicit error for missing/wrong-context proof capture in an active text workflow instead of silently using unrelated starter text; genuinely no-text legacy starter remains supported.
No B2 code committed or gate checked; exact corrective tests, broader text/proof/nodes checks and Clippy remain pending.
- Trial preflight: current qaf-ar is blue with two components; exact-red Arabic entries found are U+0600 through U+0603 signs, not joining forms.
Coordinator spot-checked actual GLIF colors and green relatives through contents.plist; original files were read only.
Installed checkpoint/runtime paths are recorded in tests/fixtures/glyph_workflow/local-trial-inventory.md, without executing inference or hashing weights yet.
Asked Eli for a preferred first glyph/form while implementation continues.
Arabic signs are already in scope; absence of a same-construction green reference does not introduce a new authorization gate.
F1 remains pending an actual selected scratch trial, not accepted by this inventory.
- B2 accepted at 621bf11 after coordinator review and serialized validation with the established test-font/target/jobs environment.
`cargo test --lib --locked font::compiler::proof::tests -- --test-threads=1`: 16 passed.
`cargo test --lib --locked text::buffer:: -- --test-threads=1`: 90 passed.
`cargo test --bin runebender --locked application::view::canvas::editor::tests -- --test-threads=1`: 33 passed.
`cargo test --bin runebender --locked application::editor::tools::nodes:: -- --test-threads=1`: 22 passed, one existing Bold model test ignored and not counted as coverage.
`cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check` and `git diff --check` passed.
The real Arabic mark test exposed a renderer bug: RTL now right-anchors the complete HarfRust visual-order run and accumulates advances with GPOS offsets intact.
Numeric mark placement and actual compiled Arabic proof generation pass; this is not human visual approval.
RTL overflow explicitly rejects unsupported wrapping; mixed-direction and multiline capture remain unsupported rather than silently changed.
Browser build/smoke and visual artifact inspection remain pending integration gates; no real model trial or font edits occurred.
- Add exact implementation commits, validation commands/results and human-review status here as each gate completes.

## Context sources

Read only the relevant source, not the entire archive.
- /Users/eli/GH/repos/virtua-grotesk/AGENTS.md
- /Users/eli/GH/repos/virtua-grotesk/DESIGN.md, especially grid curriculum and curve/optics priority
- /Users/eli/GH/repos/virtua-grotesk/documentation/source/arabic-grammar.md
- /Users/eli/GH/repos/virtua-grotesk/documentation/glyph-ai-harness-workflow.md
- /Users/eli/GH/repos/virtua-grotesk/documentation/proofs/PROOF_SPEC.md
- /Users/eli/GH/repos/virtua-grotesk/harness/RUNBOOK-codex.md
- /Users/eli/GH/repos/virtua-grotesk/.agents/skills/anchor-sheet-glyphs/LESSONS.md
- /Users/eli/GH/repos/font-garden-lab/notes/sketch2glyph-system.md and glyphlab/sketch2glyph.py
- [Virtua dataset and model rationale](https://elih.net/blog/virtua-grotesk/)
- [img2bez rationale](https://elih.net/blog/img2bez/)
- [Optical corrections: Design with FontForge](https://github.com/fontforge/designwithfontforge.com/blob/gh-pages/en-US/Trusting_Your_Eyes.md)
- [Context and viewing size: Peter Bilak](https://www.typotheque.com/articles/designing-type-systems)
- [Arabic shaping](https://learn.microsoft.com/en-us/typography/script-development/arabic)
- [OpenAI image generation](https://developers.openai.com/api/docs/guides/image-generation)
- [OpenAI vision limitations](https://developers.openai.com/api/docs/guides/images-vision)
