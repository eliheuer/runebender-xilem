# Local AI plan

This is the one living place for Runebender's local AI plans, ideas, and progress.
It is kept current as work lands; the campaign checklists it links are historical records.
Started 2026-10-09.

Related files:

- [Local models](LOCAL-MODELS.md): how a model package and its runtime are installed.
- [Virtua Regular AI checklist](VIRTUA-REGULAR-AI-CHECKLIST.md): the Regular Arabic brush campaign and its log (September 2026).
- [AI architecture checklist](AI-ARCHITECTURE-CHECKLIST.md): the architecture foundation campaign, stopped on 2026-09-28 with sections E through H open.
- [Virtua Regular testing](VIRTUA-REGULAR-TESTING.md): hands-on instructions for the brush trial.
- The model crate lives in the sibling `font-ml` repository; training lives in glyphlab.
- The design thesis is [Virtua Grotesk](https://elih.net/blog/virtua-grotesk/).

## The end goal

A type designer should only have to draw what the machine cannot infer.
You draw a core set of glyphs well in one master, and the editor drafts the rest in your conventions for you to grade and fix.
Every accepted draft is new training data in your own convention, so the model gets better at your font the longer you work on it.
The near-term purpose of all of this is to finish Virtua Grotesk.

## Rules that every feature respects

- The target a model learns from is always a clean, grid-native source.
  Never train on Google Fonts, OFL collections, or compiled fonts as targets.
  Inputs may be rough, foreign, or noisy; outputs must look like the designer drew them.
- Drafts land on the 8-unit machine grid.
  Humans make the 2-unit optical corrections.
- A draft is a proposal, never a direct edit.
  Install and Undo go through the ordinary proposal flow.
- Green and purple glyphs are frozen.
  Only a human assigns those grades; machine drafts are marked orange.
- Bold must stay point-compatible with Regular.
  References calibrate the engine; they are not copied.
- Work in batches of 10 to 20 glyphs, then review, as the bolden runbook says.
- Measure before trusting.
  Every engine is scored against drawn pairs, and a plain mean shift is the baseline it has to beat.
- Nothing downloads, nothing phones home.
  Models are directories on disk.

## The features, in the order a designer feels them

1. **Draft the other master.**
   For any glyph Bold lacks, one action drafts it from Regular, point-compatible and on the grid.
2. **Draft from a sketch.**
   Rough a glyph with the brush and the model redraws it in the font's conventions.
3. **Fill a missing character with no drawing.**
   Borrow the letterform from a chosen reference font, trace it, and redraw it in the font's conventions.
4. **Learn this font.**
   A short finetune on the glyphs drawn so far makes 1 through 3 speak another family's conventions.
5. **Spacing and kerning drafts.**
   Propose sidebearings and kern pairs on the grid from the glyphs already spaced.

Search over fonts, consistency checks, and embedding models are support work.
None of them saves a designer an afternoon; the five above do.

## Done

- [x] Model library discovery, selection, Refresh, Open folder (`~/runebender/models`).
- [x] Brush panel: temporary ink, calibrated img2bez tracing, Draft with Virtua, guarded comparison, Apply and Undo.
- [x] Local AI rail: runs font-ml tasks on a background thread with detached captures, proposal layers, Install, Discard, and Undo.
- [x] Shared job lifecycle with cancellation, deadlines, and output bounds for recipes, Local AI, and Chat workers.
- [x] font-ml bolden task: structure-forced deltas, 8-grid output, about 0.5 s per glyph on Metal.
- [x] font-ml eval: per-glyph error for a model against drawn Regular/Bold pairs, with mean-shift baseline.
- [x] Geometric anisotropic offset learned from drawn pairs (`outline::embolden`).
- [x] Weight debt block (2026-10-09): lists Bold glyphs still byte-identical to Regular by script, shows measured offset and model error per script, drafts batches of 20 by offset or by model, measures the model in the background, marks installed drafts orange.
- [x] `RUNEBENDER_MASTER` headless override for screenshots of a chosen master.

## Measured state of Virtua Bold

Recorded 2026-10-09 from the Weight debt block and font-ml eval.

| Group | Owed | Drawn pairs | Offset vs. baseline |
| --- | ---: | ---: | --- |
| Latin | 2 | many | about even |
| Arabic | 11 | 130 | offset wins 108 |
| Hebrew | 71 | 5 | offset wins 4 |
| Marks | 1 | few | shared fit |
| Figures and symbols | 30 | some | shared fit |

The neural model beats the offset on Latin and Hebrew; the offset beats the model on Arabic.
Both barely beat the mean shift overall, which is why Hebrew, with five drawn pairs, is the group to watch.

## To do

### Feature 1, Bold from Regular

- [ ] Use the Weight debt block to clear the Hebrew debt in batches of 20 and record how many drafts survived review unchanged, nudged, or redrawn.
- [ ] Analogy engine: copy the deltas from the nearest already-boldened glyph of the same script, score it with eval beside offset and model.
- [ ] Composites: draft the 61 composite Bold glyphs by re-pointing components once their bases are drawn, instead of boldening outlines.
- [ ] Show the three engine errors side by side per glyph in the review strip, not only per script.
- [ ] Feed accepted Bold drafts back into eval as new drawn pairs without a manual save-and-rerun.
- [ ] Fix the one clipped label in the 220 px rail when a group uses the shared fit.

### Feature 2, sketch to draft

- [ ] Record the kaf-ar.medi failure and later trials as a fixture set with sketch, settings, model identity, output, and verdict.
- [ ] Add the missing glyph-name tokens for Arabic contextual forms, or condition on Unicode plus position instead of names.
- [ ] Decide whether the sketch model should move to font-ml so the brush and the rail share one runtime.

### Feature 3, missing character from a reference

- [ ] Pick the reference-font input path: raster render then calibrated trace, then redraw with the sketch model.
- [ ] Training change: teach the model to take a traced foreign skeleton as input with the grid-native source as target.
- [ ] Editor flow: choose reference, press Draft, grade.

### Feature 4, learn this font

- [ ] Finetune button in the Local AI rail that runs glyphlab on the accepted drafts of the open project.
- [ ] Define the minimum drawn set for a useful finetune and show it as a readiness line in the panel.

### Feature 5, spacing and kerning

- [ ] font-ml spacing and kerning tasks over the already-spaced glyphs.
- [ ] Proposal flow for metrics and kern pairs, with the same Install and Undo.

### Foundation left open from the architecture campaign

- [ ] D3. Bridge external workers through detached captures and validated candidate imports.
- [ ] E1 to E4. Command and generator registry, tool lifecycle, and a documented extension example.
- [ ] F1 to F4. Provider-neutral contracts for cloud workers, kept out of the font engine.
- [ ] G1 to G4. Extension trust tiers and recoverable runs.
- [ ] H4. Update the website's architecture page for the Local AI rail and Weight debt block.

## Ideas not yet scheduled

- Consistency checks: flag a glyph whose stems or terminals disagree with the rest of the master.
- Embedding search over a font or a library of sources to find the nearest drawn neighbor.
- Fonts as models rather than outline tables; the long game from the blog post, not next.
- A persistent font-ml serve process so repeated drafts skip model load.

## Log

- 2026-10-09: Weight debt block landed; plan document started.
