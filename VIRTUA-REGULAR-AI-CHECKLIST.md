# Virtua Regular AI workflow checklist

Status: implementation started; first assignments below are in progress, not validated.
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
Its original recipe lacks viewing-size/layout parameters and scene rendering fixes scale to 160 px/em.
The new proof contract must address that before claiming reading-size comparison.
- src/application/platform/live_proofs.rs binds epochs/revisions.
src/application/editor/tools/nodes and src/workflows/nodes_session.rs retain branches/proofs/candidates and explicit Apply.
Version 2 comparison currently does not enforce identical proof recipes.
- src/font/project/edit_transactions.rs and src/automation/agent_edit.rs support points/anchors/width/append.
Full replacement exists in a separate edit_batch path, but is not wired through the guarded live recipe path.
Candidate compile projection must support changed topology without mutating the root.
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
- [ ] B1. Add bounded, backward-compatible viewing-size/layout settings to compiled proof recipes and render from them; freeze defaults for older recipes.
- [ ] B2. Capture editor text settings and exact shaped target occurrence; send the same recipe to baseline and candidate proofs; reject comparison recipe mismatch.
- [ ] B3. Supply reading-size and enlarged-detail proof artifacts with document/font/recipe/renderer identity.
Acceptance: meaningful geometry/size tests, invalid/oversized input rejection, old fixture compatibility, Arabic contextual/ligature/mark occurrence identity and matched baseline/candidate recipes.
Actual headless proof inspection is required before visual claims.

### C. Safe missing-outline candidates (Sol; follows B1 to avoid proof.rs ownership conflict)
- [ ] C1. Add bounded complete-outline replacement through canonical guarded transactions, agent operations and private candidate compilation.
- [ ] C2. Add calibrated image/model-outline import into a detached candidate; preserve unrelated width/anchors/components/metadata unless explicitly addressed.
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
| A1-A3 | Terra (next available worker) | tests/fixtures/glyph_workflow/** only | Queued |
| B1 | regular_proof_sol (GPT-6 Sol) | src/font/compiler/proof.rs and narrowly required recipe constructor call sites | Running: /root/regular_proof_sol |

Only B1 is active.
Terra dispatch hit the agent thread capacity limit; retry once after the Sol worker completes or at the next heartbeat, without duplicating B1.
Record worker IDs, handoffs and scope changes here before dispatch.
Workers report tests recommended, do not run Cargo or stage changes.

## Evidence log

- 2026-09-28: coordinator read current main, clean at c9cb462, created isolated worktree and codex/virtua-regular-ai.
No font or icon edits.
No checklist implementation acceptance is claimed yet.
- Host memory and process inspection were sandbox-blocked; use two build jobs and coordinator-only Cargo until confirmed.
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
