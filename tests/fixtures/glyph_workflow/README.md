# Glyph workflow fixtures

These deliberately small, synthetic fixtures define the data later glyph-workflow code may consume.
They are workflow contracts, not source-font facts, proof results, model results, or approval records.

Each fixture records one target, source revision guard, grade status, relevant references, proof requirements, and bounded outcomes.
`missing_arabic_regular_form.json` exercises a missing contextual Arabic Regular form.
`too_heavy_in_paragraph.json` exercises a paragraph-feedback refinement without changing a protected sibling.

The authoritative context read for these contracts is `/Users/eli/GH/repos/virtua-grotesk/AGENTS.md`, `/Users/eli/GH/repos/virtua-grotesk/DESIGN.md`, `/Users/eli/GH/repos/virtua-grotesk/documentation/source/arabic-grammar.md`, and `/Users/eli/GH/repos/virtua-grotesk/.agents/skills/anchor-sheet-glyphs/LESSONS.md`.
Those sources require continuity and measured green Arabic references before numeric tidiness.

Glyph names, revisions, text, measurements, and identifiers beginning with `demo_` are illustrative.
They must never be used to identify a Virtua source glyph or to infer a real measurement.

Consumers must keep the five evidence classes separate.

- `structural_hard_constraints` reject invalid proposals.
- `measured_reference_data` informs comparison and remains explicitly illustrative here.
- `design_preferences` guide ranking but cannot make a change valid.
- `model_assessment` is advisory and cannot create a human grade.
- `human_grade` is the sole approval record; only a human can set `green`.

`grade_legend` includes every status relevant to selection.
Green is an approved reference, purple is protected, blue awaits human review, yellow and orange are explicitly selected refinement work, red is eligible work, and unknown is never an automatic target.
All candidates remain detached until a selected candidate passes its revision guard and is applied as one undoable edit.

Arabic fixtures require a built-font shaped RTL proof that resolves the named occurrence and checks joining, anchors, dots, and marks in two contexts.
The sheet is only supporting evidence.
Demo context strings are labels, not runnable Arabic specimens.
Later integration must supply actual Unicode text and shaped occurrences.
These fixtures do not establish Arabic shaping or visual review.

The workflow retains no more than three initial candidates.
Optical refinement permits no more than three rounds with two alternatives in each round.
Structural drafts use the machine grid.
Bounded detached optical proposals may make finer 2-unit adjustments after structural constraints hold.
This follows Eli's authorization for this workflow and extends the older DESIGN.md policy that reserves those adjustments to humans.
It does not grant permission to change approved references or assign a green grade.
Explicit selection is required for Apply, and only a human can assign the final green grade.
An invalid proposal, stale revision, missing proof identity, no measured improvement, uncertainty, cancellation, or exhausted budget produces `abstain` and preserves the prior best candidate.
