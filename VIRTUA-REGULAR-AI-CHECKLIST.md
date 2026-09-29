# Virtua Regular AI workflow checklist

Status: B1-B3 and C1-C3 validated; D1 validated; D2 real local inference completed but drawing failed visual inspection; connected local candidate transport pending.
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

## Revised session goal: prove the drawing workflow first

Updated by Eli's explicit direction on 2026-09-28 after the failed kaf-ar.medi trial.
This section supersedes the earlier implementation-first ordering, not the source-protection or human-approval rules.
The next deliverable is a side-by-side experiment on one Regular Arabic form, with useful and rejected outputs, diagnosis, and remaining manual correction work.
Do not equate infrastructure completion with progress toward a usable drawing.
Finish reviewing the already-delivered bounded local transport patch; defer additional framework and optical-loop implementation until the experiment identifies a useful drawing path.
D/E software gates remain open where unproven and are not prerequisites for scratch experiments using existing tools.

### Fresh reading and implications

- [Eli: Virtua Grotesk](https://elih.net/blog/virtua-grotesk/), reread 2026-09-28: consistent source data and reversible representations are central; structural 8-unit defaults coexist with 2-unit optical corrections.
Grid compliance is measurable but does not establish design quality.
The post's weight-transfer evidence must not be treated as evidence of missing Regular Arabic generation or capability of the separate sketchpre checkpoint.
Measurement popcount colors describe measurements; they are not glyph approval grades.
- [Simon Cozens: The State of AI Font Generation](https://simoncozens.github.io/state-of-ai-font-generation/), reread 2026-09-28: type-designer evaluation, vector sequence difficulty, raster-to-vector quality, and cross-script transfer are central concerns.
Use this survey to identify hypotheses and primary research, not as proof that a listed model supports Arabic or produces release-ready outlines.
His reported tokenizer collapse motivates intermediate representation checks; it does not diagnose our different model's failure.
- Coordinator hypothesis: reasoning over copied approved geometry plus bounded local edits may require less invention than generating every coordinate.
Reference-conditioned imagery plus calibrated img2bez is a separate credible route to test.
Neither hypothesis is an accepted result yet.

### Brush workflow restoration, user-directed priority

Eli identified /Users/eli/GH/repos/runebender-web and explicitly requested finding and porting its earlier sketch workflow.
Confirmed implementation: src/components/SketchPanel.vue, src/Runebender.vue sketch raster/crop/trace/draft functions, and server/serve.mjs sketch2glyph endpoint.
Source checkout HEAD a6375df8e82ac38f118baf12eb3aba628c1899e5 has unrelated uncommitted work; inspect only and preserve it.
Prioritize a font-unit brush sketch layer, erase/clear, and deterministic Trace-to-draft in the native editor, then restored local-model drafting with verified calibration.
Sketch input expresses Eli's intended construction; Rubik is optional supporting reference, not a skeleton to copy mechanically.
Do not port old automatic blue grading or direct source installation; use current detached candidate review and explicit Apply/Undo.
- [x] S1. Restore separate native brush sketch input with font-unit widths, erase/clear and stable glyph/viewport placement; no outline mutation while drawing.
- [ ] S2. Connect the actual sketch to deterministic calibrated tracing and existing staged candidate review; verify Apply/Undo and protected grades.
- [ ] S3. Fix and verify the local runner's placement before enabling Draft with Virtua; reproduce a known sketch control before judging new Arabic generation.
Confirmed X2 defect: installed img2bez defaults to canvas fitting and ignores --lsb in that mode, but the runner passes ink-box placement arguments without --fit-source ink.
Saved source bounds [-80,0,808,768] become traced bounds [64,168,728,742] before inference; tokenizer retains two contours and these bounds, with only small grid rounding.
The 174-token prefix is within the 1848-token input guard; neither unknown name nor excessive prefix length explains this trial.
The prior model result remains rejected, but it is not a correctly calibrated model-quality test.
S1-S2 brush patch is delivered and under coordinator integration/visual review.
S3 is assigned to regular_proof_sol: local_sketch.rs, private calibrated launcher/fixtures, and narrow nodes/trace.rs provenance/test updates only.
Coordinator owns UI corrections, serialized validation and checklist; further general framework work remains deferred.

### Experiment gates, in priority order

- [ ] X1. Freeze one shared experiment packet for red Regular kaf-ar.medi: source/reference identities, initial/final kaf and medial lam relevance, baseline geometry, intended joining form, calibrated sketch, and identical word/detail proof recipes.
Use at least two shaped word contexts; include paragraph proof only through a renderer that actually supports the layout.
Do not claim current single-line proof capture supports paragraph layout.
- [ ] X2. Diagnose the local pipeline before repeating inference: retain raster, img2bez input outline, tokenizer round-trip, raw generated outline, and post-conformance outline.
Check vocabulary/form conditioning, token limits, contour boundaries, coordinate range, and whether grid conformance damages geometry.
A base U+0643 codepoint alone does not specify a medial form.
Use a frozen approved glyph copy as a reconstruction control without changing or regrading the original; distinguish reconstruction from novel generation.
No new training or downloads.
- [ ] X3. Produce a detached geometry-reuse candidate: copy appropriate approved strokes into the target, preserve reference sources, and record the construction hypothesis and constrained adjustments.
- [ ] X4. Produce a reference-guided image candidate with the authorized image tool, using the same sketch and labeled green references, then calibrated img2bez conversion.
Retain prompt, actual inputs/output, calibration, outline, and available model metadata; do not invent unavailable provider identities.
No separately billed API loop without an established budget.
- [ ] X5. Compare at most three initial alternatives, counting the existing failed local result unless a specific diagnosed correction justifies replacing that arm.
Compare Arabic form/joins, weight/counters/terminals, reading-size appearance, editable outline quality, elapsed time and remaining manual corrections.
Metrics and model critique supplement visual review; neither assigns approval grades.
- [ ] X6. Try one targeted optical correction on the best usable candidate, retain the previous best, and render identical proofs.
Use the existing maximum three-round/two-alternative limits; stop earlier on uncertainty or no improvement.
If none is usable, report the failure and next specific hypothesis rather than implementing an autonomous refinement framework around bad outputs.
- [ ] X7. Deliver the comparison packet and recommend the next implementation based on evidence and Eli's verdict.
Pause at a human-review or external-prerequisite boundary rather than spend repeated heartbeats on unchanged work.

Success means less manual work to obtain a usable Regular Arabic drawing, with intact editability and font behavior.
A model process returning successfully, a high overlap score, or an on-grid outline is insufficient.
The longer-term Regular release goal is unchanged; Bold, whole-font generation, and broad research/model-training campaigns remain out of scope.

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
Purple: approved composite, frozen reference; never edit or remove.
Yellow/orange: explicitly selected refinement targets; preserve the drawing, never overwrite, not references.
Red: explicitly selected empty/junk/unfinished targets eligible for replacement.
Blue: composite needing diagnosis and repair, not an approved reference.
Pink: composite needing smaller edits, not an approved reference.
Only red authorizes overwriting; preserve frozen component glyphs when editing composites.
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
- [x] B3. Supply reading-size and enlarged-detail proof artifacts with document/font/recipe/renderer identity.
Acceptance: meaningful geometry/size tests, invalid/oversized input rejection, old fixture compatibility, Arabic contextual/ligature/mark occurrence identity and matched baseline/candidate recipes.
Actual headless proof inspection is required before visual claims.

### C. Safe missing-outline candidates (Sol; follows B1 to avoid proof.rs ownership conflict)
- [x] C1. Add bounded complete-outline replacement through canonical guarded transactions, agent operations and private candidate compilation.
- [x] C2. Add calibrated image/model-outline import into a detached candidate; preserve unrelated width/anchors/components/metadata unless explicitly addressed.
- [x] C3. Make the same candidate inspectable/selectable through existing UI and MCP, with one Apply/Undo and no automatic save.
Acceptance: empty-to-drawn and junk-to-replaced, no root mutation before apply, stale rejection, unrelated/green preservation, proof of replacement, undo/redo restoration.
Do not repair or synthesize Bold to make the trial pass.
Regular-only drafts must not be mislabeled variable-font release artifacts.

### D. Reference selection and generator execution (Sol; context A, candidates C)
- [x] D1. Resolve grades and relevant Arabic references from current canonical/scratch data; no arbitrary alphabetic selection; refuse protected/unknown automatic targets.
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
B2 review is resolved; B3 is under coordinator review with corrections assigned to /root/regular_proof_sol.
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

### B3 first validation, pending
Nine-file patch reviewed in progress; no B3 acceptance or commit.
The serialized `cargo test --lib --locked font::compiler::proof::tests -- --test-threads=1` with the established font/target/jobs environment failed compilation.
Errors: ambiguous detail closure Result error type, formatting an unfinished SHA-256 digest, and ambiguous floating-point pen type; two unnecessary-qualification warnings also need correction.
Sol owns these corrections and an empty-target detail regression: an empty missing glyph must produce a blank baseline detail rather than abort the proof.
Renderer PATH resolution must skip non-executable files on Unix; executable provenance remains limited to the invoked entrypoint, not its transitive helpers.
No Cargo jobs remain active; coordinator will rerun after worker handoff.

### B3 second validation, pending
Worker compile and empty-baseline corrections reviewed; serialized proof tests passed 17, agent_proof passed 2, agent_nodes passed 7, with no failures or ignored tests.
Commands used `cargo test --lib --locked` with filters `font::compiler::proof::tests`, `automation::agent_proof::`, and `automation::agent_nodes::`, each with `-- --test-threads=1` and the established font/target/jobs environment.
All-target Clippy with warnings denied failed four checks: reference predicate for Copy enum, complex borrowed image tuple, 64KiB stack buffer, and missing RenderedScene Debug.
Sol is correcting these structurally and preparing a temporary real Arabic paired-image driver for coordinator inspection.
B3 remains unchecked and uncommitted; browser validation and actual image inspection remain pending.
No Cargo jobs remain active after this run.

### B3 acceptance and C3 handoff
B3 implementation committed at ad453f7; earlier failures above are resolved.
All-target Clippy with warnings denied, cargo fmt check and git diff check passed.
Focused library tests passed 17 proof, 2 agent_proof and 7 agent_nodes tests before the final structural lint corrections.
After those corrections, `cargo test --bin runebender --locked application::platform::live_proofs:: -- --test-threads=1` passed 2 and `cargo test --bin runebender --locked application::editor::tools::nodes:: -- --test-threads=1` passed 22; one existing Bold test stayed ignored, not coverage.
Actual Arabic context/detail PNGs were rendered from the immutable compiled Virtua snapshot and inspected at /private/tmp/runebender-b3-arabic-proof/{context,detail}.png.
The 32 px/em context is intact and the selected kasra is fully visible at 320 px/em; neighboring base ink may cross the detail crop by design.
This establishes rendering and target visibility, not aesthetic approval or a generated-glyph trial.
Font identity: sha256:d89a8384959be76bb2d64ea078f672787afffc5842af5b19d334f0e113208e9c.
Recipe identity: sha256:70559a8e2409820a678ef2e5ff97fcc4d7efd8f0402336a76b49eca5f4acd262.
Renderer entrypoint identity: sha256:679656960abd698378971ad806a2f019d33c39598fc9e78f9868d89b7d2dd63f.
Temporary driver is /private/tmp/runebender-b3-proof-driver/src/main.rs, compiled with rustc against `cargo build --lib --locked` output and the shared dependency directory.
An initial standalone Cargo driver build was stopped because its separate manifest selected different cached dependency versions; final proof evidence uses the repository locked build.
Browser checks remain F4; no font sources were changed.
C3 assigned to regular_proof_sol: connect calibrated detached replacement candidates to existing UI/MCP inspection, selection and guarded Apply/Undo; own existing agent edit/node and native node adapters as narrowly required.
Workers still do not run Cargo, commit or edit this checklist.

### C3 first validation, pending
Three-file nodes_trace patch compiled; both focused calibrated_trace integration tests failed at the post-Apply width assertion (actual 600, expected 400).
Command: `cargo test --bin runebender --locked application::editor::tools::nodes::workspace::tests::calibrated_trace -- --test-threads=1`, with established test-font/target/jobs environment.
The chained agent_nodes tests and Clippy did not run after this failure.
Sol must distinguish fixture width initialization from any real mutation and compare against captured-before metadata without masking a mutation.
Coordinator also flagged synchronous img2bez tracing in the Workspace request path; review the smallest existing-worker integration to avoid blocking editing.
C3 remains uncommitted and unchecked; no original font changes and no Cargo job remains active.

### C3 asynchronous tracing decision
Worker diagnosis: the 400-unit test expectation was wrong; compare applied width against the captured original width.
Coordinator approves a bounded asynchronous trace job, not a second candidate store.
Submission captures document lifetime, source/layer and graph guards and returns a handle without mutating font or graph.
Decode and img2bez execute off the editor thread; keep one outstanding trace per session and a global bounded worker capacity, rejecting excess work.
Status/pump may install the resulting recipe only after rechecking document, layer and graph guards; stale or cancelled completions never mutate the graph.
Cancellation suppresses publication even if img2bez cannot be interrupted; do not claim immediate compute cancellation or free worker capacity early.
Reuse the existing graph mutation receipt for exact retry and keep terminal retention bounded with release; graph installation is exactly once.
Keep job logic in a cohesive adjacent module; Workspace only adapts requests and owns session lifecycle.
Sol owns this narrow expansion into the nodes trace helper, agent request/result types and associated lifecycle tests; no Cargo, commits or checklist edits.
C3 remains pending runtime validation; no broader job framework or CLI subsystem is authorized by this decision.

### Active grading hold
Eli is grading /Users/eli/GH/repos/virtua-grotesk and will explicitly report when finished.
Do not edit that repository, select a target, capture green references or run a real-font trial during this pass.
After grading-done, read labels fresh; earlier inventory is not authoritative for eligibility.
Implementation and synthetic fixture tests may continue in this isolated worktree.
The active Sol worker has received this constraint; asynchronous C3 handoff is still pending, so no duplicate assignment or Cargo job was started.

### C3 async validation, pending
`cargo test --locked --bin runebender calibrated_trace -- --test-threads=1` passed both empty/junk end-to-end synthetic candidate tests, including proofs, Apply and Undo/Redo.
`cargo test --locked --bin runebender application::editor::tools::nodes::trace:: -- --test-threads=1` passed cancellation but failed stale-document publication at trace.rs620: add_document_glyph in the fixture did not change the revision expected by the test.
Sol must establish an actual revision-changing edit and assert the fixture revision changed, preserving the stale guard.
Unused GraphIdentity import and TraceSession field visibility warnings remain; chained agent_nodes and Clippy checks were not reached.
Coordinator also requested released-retry correctness when another handle is retained, with a regression.
No Cargo jobs remain; C3 stays unchecked/uncommitted and the grading hold remains active.

### Grading complete and canonical color semantics
Eli reports the grading pass committed and pushed; the active grading hold is lifted.
Read current labels fresh before target/reference selection; do not reuse the earlier eligibility inventory.
Canonical definitions now live in /Users/eli/GH/repos/virtua-grotesk/README.md#semantic-colors and its AGENTS.md links there.
Blue means a composite needing attention, not AI output; purple is an approved frozen composite reference; pink is a composite needing smaller edits.
Orange/yellow drawings may be edited but not overwritten or used as references; only red permits replacement.
Green and purple remain frozen; models may not assign approval colors.

### C3 acceptance and D1 assignment
Implementation commit: 2567736.
Serialized synthetic tests: calibrated_trace (2 passed, preceding run), nodes::trace (3 passed), automation::agent_nodes (7 passed).
Final commands: `cargo test --locked --bin runebender application::editor::tools::nodes::trace:: -- --test-threads=1`, `cargo test --locked --lib automation::agent_nodes:: -- --test-threads=1`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check`, `git diff --check`.
Clippy passed; formatting was corrected and rechecked after the coordinator fixture change.
The stale-document fixture previously attempted to add existing starter glyph B; it now asserts a unique name absent and commits its addition, verifying revision change.
Cancellation and released-retry behavior pass; both empty/junk candidates preserve width and unrelated glyph geometry through Apply/Undo/Redo.
C3 proves generic candidate transport and history; semantic grading enforcement is D1, and native interactive/browser validation remains F4.
No real font was edited or selected in this validation.
D1 ownership: regular_proof_sol, a cohesive reusable glyph grading/reference helper plus narrow nodes_trace policy integration and synthetic fixtures/tests as needed.
Implement current seven-color semantics from Virtua README; only red replacement, green/purple frozen references, orange/yellow non-overwriting refinement, blue/pink composite repair without frozen-base edits.
Do not infer reference relevance from alphabetical order; require explicit related reference selection and record current grade/source/revision/geometry.
No actual glyph target chosen automatically; fresh source inventory can be read only after generic policy implementation.
Workers do not run Cargo, commit or edit this checklist.

### D1 acceptance
Implementation: 87a4234.
Serialized focused tests passed: automation::glyph_grading (2), nodes::trace (3), calibrated_trace (2), automation::agent_nodes (7); zero failures or ignored tests in successful runs.
Commands used `cargo test --locked --lib` for automation filters and `cargo test --locked --bin runebender` for native filters, each with `-- --test-threads=1`, shared CARGO_TARGET_DIR and CARGO_BUILD_JOBS=2.
Coordinator corrected palette String borrowing/import errors before successful tests and an obfuscated-if lint afterward.
Final `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check`, and `git diff --check` passed.
Only red Regular replacement is allowed in nodes_trace; explicit green/purple references carry relevance, revisions and resolved geometry identity.
Changing a reference component base invalidates publication even when the reference shallow revision stays unchanged.
Synthetic Arabic fixture is consumed by executable tests; no claim of real glyph/model quality or automatic relevance assessment.
D2 bounded assignment: regular_proof_sol to inspect installed local sketch runner and implement an offline-tested, non-installing candidate adapter using scratch output and pinned runtime/model identities.
No model execution until coordinator selects a calibrated scratch trial; no training, downloads, source installs, or broad provider framework.
Actual D2 acceptance still requires a recorded real local result; adapter tests alone cannot complete it.

### D2 adapter evidence, actual trial pending
Adapter committed at a8bfa63; D2 remains unchecked until a real installed-model result is recorded.
`cargo test --locked --lib workflows::local_sketch -- --test-threads=1` passed 4 offline process-fixture tests; all-target Clippy with warnings denied, cargo fmt check and git diff check passed.
Coordinator corrected a macOS /var versus /private/var fixture path expectation and redundant image module qualifications; virtualenv entrypoint symlink remains preserved.
Adapter uses scratch input, never --install, returns editable contours and pins executable/checkpoint/local-module/tracer identities.
Limits: installed runner fits an ink box rather than C2 full-image calibration; generated geometry may move; Python dependency wheels are not attested; cancellation kills the direct child only.
No actual inference or font edits occurred.
Next bounded Sol assignment is read-only fresh post-grading trial preparation: eligible red Regular Arabic target shortlist, explicit green/purple related references, and existing suitable sketch input if present.
Do not choose or edit font sources, run models, or treat model scores as approval; return concrete trial recommendations to coordinator before execution.

### First real-trial selection and palette correction
Coordinator selects red Regular kaf-ar.medi for a scratch-only trial, with medial context مكتبة.
Fresh worker inventory at Virtua d3184df9abfb4fedce16dd9f5b075324132847c7 finds target width 808, two contours, 68 points and four anchors.
Closest approved references: kaf-ar.init and kaf-ar.fina; lam-ar.medi supplies joining context.
These green-labeled references use legacy 0.09,0.72,0.44,1, already recognized by existing theme regression tests, but D1 exact current-palette matching rejects them.
Sol owns a narrow D1 compatibility correction: recognize this documented exact legacy green alias without hue-nearest authorization, keep unknown/conflicting labels rejected, add regression coverage.
No source recoloring, permission widening to red-like hues, or source edits.
After corrected reference capture, prepare a scratch input from the red target as a model smoke test, not a claim that its existing flawed shape is good.
Suggested 512-square calibration: 2 font units per pixel, left boundary -112, pixel baseline 400, font baseline 0; measure actual raster ink bounds.
Use concrete runs/sketchpre checkpoint; no Bold edits or direct --install.
Original checkout had unrelated notdef.glif plus documentation changes; preserve all.

### Scratch trial preparation, legacy test pending
Scratch Regular source copied read-only from original to /private/tmp/runebender-kaf-trial-sjiv0m10/VirtuaGrotesk-Regular.ufo, with source-manifest.json recording each copied file SHA-256.
Temporary reviewed driver: /private/tmp/runebender-kaf-d2-trial-driver/src/main.rs; not executed yet.
Legacy palette test failed at glyph_grading.rs457 with InvalidLayerMetadata while constructing legacy/conflicting metadata through the strict editor setter; two other grading tests passed.
Sol is correcting the fixture to use the import/preservation path rather than weakening setter or grading policy.
No Cargo job or inference remains active; real D2 result still pending.

### First real local inference: execution passes, drawing fails
The exact legacy-green import regression now passes: `cargo test --locked --lib automation::glyph_grading -- --test-threads=1` passed 3 tests.
All-target Clippy with warnings denied, cargo fmt check, git diff check and `cargo build --lib --locked` passed with the shared target and two build jobs.
The temporary driver linked the repository locked library directly; no standalone dependency resolution was used for the actual run.
One scratch-only sketchpre inference completed with seed 20260928, temperature 0, one candidate and a 120-second process deadline.
Result: /private/tmp/runebender-kaf-trial-sjiv0m10/result-01/candidate.json; runtime/checkpoint/input identities and grading context are retained alongside it.
The input was the existing red drawing rasterized as a smoke test, not a newly approved sketch; references were captured but the installed model does not consume them at inference.
Output contains one contour and 159 points; script score -8.653044521870807 is not aesthetic evidence.
Coordinator rendered candidate.svg/candidate.png from the returned vector points and inspected it: tangled overlapping geometry, lost medial kaf structure, unusable as a replacement.
No Apply, source save, color change, training, download or second inference occurred.
D2 stays unchecked: execution is demonstrated, useful output and connected candidate workflow are not yet demonstrated.
Sol owns the bounded local-sketch-to-existing-node-candidate integration and offline lifecycle tests; no Cargo, inference, commits or checklist edits.
Next design trial should use a better sketch or reference-conditioned alternative, rather than blindly rerunning this checkpoint.

### Revised experiment priority and worker handoff
Eli authorized the experiment-first goals above and requested a fresh reading of both linked essays.
Sol delivered the four-file local-sketch transport patch; it is awaiting coordinator Cargo validation and integration review, not accepted or committed.
No new implementation worker assignment is active; next work follows X1-X7 rather than automatically expanding E.

### Local candidate transport validation and X2 progress
The four-file local transport patch passed serialized tests: automation::agent_nodes 8, nodes::trace 5, workflows::local_sketch 4, nodes::workspace 7.
Commands used `cargo test --locked --lib` or `--bin runebender` with those fully qualified filters and `-- --test-threads=1`, shared target, two build jobs and the configured source fixture directory.
All-target Clippy initially rejected a large TraceOutput enum variant; coordinator boxed the local candidate variant structurally.
Clippy with warnings denied then passed, followed by all 5 trace tests again; formatting and diff checks pass.
This validates explicit backend selection, retained provenance, guarded publication and cancellation; it does not establish usable local-model output or native interactive/browser behavior.
The local runner internally selects one result from up to three samples; it does not expose three alternatives for human comparison, so D4 is not complete.
No additional framework implementation is assigned.
Scratch comparison packet started at /private/tmp/runebender-kaf-trial-sjiv0m10/experiment/packet.json with a rendered, coordinator-inspected reference-sheet.png from the copied target and three approved references.
X1 remains open: matching shaped word proofs are still pending.
Read-only X2 findings: exact target name and U+0643 exist in sketchpre vocabulary; saved image bounds map correctly to source geometry.
Generated output loses one contour and its upper region; the sampled result was the sole candidate, so scoring did not select among alternatives.
Sol is retaining scratch trace and tokenizer round-trip intermediates without inference to distinguish preprocessing failure from generation failure; X2 remains open.

### S3 scratch tracing correction experiment
Coordinator ran installed img2bez only, with no model inference or original source edits.
Adding --fit-source ink alone fails explicitly because this binary also requires --fit-y.
Successful command uses saved input.png, --grid 2 --profile clean --target-height 768 --y-offset 0 --lsb -80 --fit-source ink --fit-y 0:768.
Output /private/tmp/runebender-kaf-trial-sjiv0m10/experiment/diagnosis/ink-fit.ufo retains two contours and 65 points at scale 2.00, with bounds x[-80,806], y[0,764].
This removes the gross shift/compression; small fitting/grid differences remain relative to source x[-80,808], y[0,768].
A second scratch trace with --width 808 still reports advance 870 under ink fitting; never import this generated advance into the target.
The existing local adapter retains captured advance independently; the corrected runner must continue doing so.
S3 remains open: explicit placement has not yet been integrated into the pinned model runner or tested with model inference.
Active Sol brush-port ownership approved: sketch module, narrow workspace Tool/canvas/panel/commands bridge and trace entry invocation; local_sketch.rs remains coordinator-owned.
Do not expose the local Draft action until calibrated preprocessing is verified; deterministic sketch tracing may proceed.

### Hands-on handoff priority and first brush validation
Eli requests wrapping up existing work toward an application testing session, then pausing scheduled development for immediate UI and visual feedback.
Required handoff: reviewed brush/trace port, corrected and verified local preprocessing before model drafting, scratch font and reproducible launch, sketch/preview/Apply/Undo checks.
Cloud comparison and general autonomous optical refinement are deferred rather than prerequisites for the first hands-on session.
Brush first validation: `cargo test --locked --bin runebender application::editor::tools::sketch -- --test-threads=1` passed 2; `cargo test --locked --bin runebender application::view::canvas::editor::tests -- --test-threads=1` passed 33.
Shared Cargo target, two jobs and configured fixture source directory were used; no other Cargo jobs overlapped.
All-target Clippy failed only two unnecessary Mutex qualifications in editor.rs; Sol owns corrections.
Coordinator additionally requested preserving selected brush/erase controls across glyph mask initialization, rejecting clipped border ink, and a SketchChanged dispatch regression proving canonical state remains unchanged.
S1/S2 remain unchecked; native visual and browser checks are pending, and no foreground application has been launched.

### Brush regression results and current integration boundary
Brush tests now pass 4 cases, including clipped-edge rejection and retained brush settings across glyph initialization.
Canvas editor tests pass 34 cases, including SketchChanged leaving canonical document revision, geometry, modified state and undo history unchanged.
All-target Clippy with warnings denied, formatting and native build passed before the inspector alignment correction.
Gray and Light headless captures exposed vertically centered overflow hiding the brush controls; coordinator changed the inspector to top alignment.
The follow-up build encountered the active S3 worker midway through calibrated_model_input implementation; this is an incomplete working patch, not a validated regression result.
No Cargo job remains active; wait for the worker's compile-ready handoff before rebuilding.
S3 uses calibrated Rust img2bez Clean/grid2 pen operations and a pinned private launcher overriding only sketch2glyph.trace, bypassing the miscalibrated installed tracer.
No new inference or original font mutation occurred.

### Calibrated local input patch accepted offline
Sol delivered the private calibrated launcher and exact model-input provenance; coordinator reviewed the full-image transform and installed trace-call signature.
`cargo test --locked --lib workflows::local_sketch -- --test-threads=1` passed 5 tests, including a real Python stub proving the legacy trace slot is bypassed.
`cargo test --locked --bin runebender application::editor::tools::nodes::trace -- --test-threads=1` passed 5 tests, preserving staged publication, current-document guards and cancellation.
Coordinator corrected one doc-markdown lint; all-target Clippy with warnings denied, cargo fmt check and git diff check then passed.
The adapter preserves the requested advance and rejects model vocabulary overflow instead of silent coordinate clamping.
S3 remains unchecked until a real known-sketch control is tested; offline placement correctness does not establish generated drawing quality.

### Testing handoff blockers and bounded follow-up
Calibrated local input is committed at 27aba5d; it is offline-validated, not a new successful model trial.
The native build passed, but Gray/Light screenshots still hid the top brush controls; alignment must be applied to inspector_stack in render.rs rather than the inner info panel.
Coordinator owns that narrow correction and headless verification.
Read-only handoff audit found the default Latin proof can hide the Arabic target change, and the brush UI lacks trace status/release needed for retry.
Sol now owns only sketch.rs, panels/sketch.rs and narrowly required workspace/host state for guarded status/cancel/release and an actionable selected-Arabic-proof prerequisite.
No new framework, model run, original source edit or foreground GUI is authorized by this assignment.
Do not call the application ready for hands-on testing until those two blockers are resolved and checked.

### Final state of this coordinator slice
The inspector-stack alignment alone did not resolve clipping in the new Gray/Light screenshots.
Coordinator found the edge-keyline ZStack wrapper centers overflowing content and changed its content alignment to TOP_LEFT; this final correction is not yet built or visually verified.
The last build also encountered Sol's in-progress SketchTraceUi field; do not run another build until Sol signals a stable handoff.
No Cargo job is active, Sol is the only active worker, and no additional assignment should be launched.
The only completed commit in this slice is 27aba5d; brush UI and retry/proof guards remain uncommitted pending integrated validation.

### Authorized final promotion and disk cleanup
Eli authorizes committing, merging the validated work into main and pushing main when the current hands-on testing fixes are ready.
This supersedes the earlier no-push/no-main/no-worktree-removal restriction for this final phase only.
Finish active worker patches and integrated checks first; retain a runnable test binary outside cleaned target directories.
Inventory each worktree from this effort, preserve unrelated changes and verify every retained commit is on main and pushed before removal.
Main currently has an unrelated assets/icons/icons.ufo/glyphs/invert.glif edit; preserve it exactly.
Use cargo clean for scoped build artifacts only after all builds stop; preserve source fonts, models, scratch review evidence and unrelated worktrees.
Record final commit, remote synchronization, removed worktrees and measured disk recovery, then pause the coordinator for user testing.

### Integrated testing controls and MCP corrections
Sol delivered retained trace status/error, Check/Cancel/Release/Retry/Open comparison controls and strict selected-Arabic proof-context checks.
Coordinator corrected a String error-type mismatch; all 6 brush tests and all 5 panel resize tests pass.
All-target Clippy passes and native build succeeds.
Gray/Light headless captures confirm top controls are visible after anchoring edge-keyline content; shortened preset labels address horizontal clipping, with refreshed captures pending.
The full library run passed 635 tests and exposed two omitted nodes_trace registrations: strict socket epoch retention and effect metadata.
Coordinator added trace/start/status/cancel/release registrations; all 75 automation tests now pass, including both failing regression cases.
The native application suite is running serially; no worker remains active.
VIRTUA-REGULAR-TESTING.md provides the scratch launch and manual review procedure with explicit limits: deterministic Trace is exposed, local Draft quality and controls remain pending.
The original main icon edit has a pre-promotion SHA-256 receipt in the scratch review packet.

### Native regression completion and browser boundary
Native application suite: 286 passed, 4 explicitly ignored model-dependent tests, zero failures.
The library suite's 635 passing cases plus the subsequent 75 passing automation cases cover its two corrected MCP failures; a single all-green workspace rerun is not claimed.
Formatting, diff and copyright checks pass.
Browser check first failed because local_sketch used image while image was optional behind application; promoted the already-pinned PNG dependency to the library and added only its dependency edge to web/Cargo.lock offline.
Browser wasm check now passes with no version changes.
A locked release browser build is running serially with log /private/tmp/runebender-kaf-trial-sjiv0m10/experiment/browser-build.log; wait before any other Cargo command or browser smoke test.
Remaining promotion gates: browser build/smoke, final native build/theme capture, retained executable, coherent commits, fast-forward main preserving icon hash, push verification and scoped cargo/worktree cleanup.
No actual post-calibration model inference has run and local Draft remains unavailable in the Brush UI.

### Validated native testing handoff
Implementation committed at 2207f33; calibrated model-input boundary at 27aba5d.
Locked browser release build passed in 6m26s; existing headless browser quality suite passed at DPR 1 (120 frames, median 5.2ms, p95 8.1ms locally).
Other browser densities were not rerun; no foreground native pointer, IME or GPU certification is claimed.
Final native build and all-target Clippy with warnings denied pass; final Gray/Light captures confirm all brush presets and actions fit the inspector.
Retained executable: /private/tmp/runebender-kaf-trial-sjiv0m10/runebender-tested, with exact SHA-256 and implementation commit in tested-build.json.
Scratch font, proofs, rejected candidate and calibration diagnosis remain in the same review packet.
S1 is complete under automated geometry/event checks and headless layout evidence; S2/S3 and visual acceptance remain open for the hands-on trial and real calibrated model control.
The font is not complete, and local Draft/cloud/optical generation is not advertised as completed.
Proceed with the authorized fast-forward main promotion and push, then scoped cleanup and pause for Eli's testing feedback.
