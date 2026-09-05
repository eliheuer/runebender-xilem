<!-- Copyright 2026 the Runebender Authors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Switching daily work to Xilem

Priority brief, 2026-09-05. Supplements [GPUI-PARITY.md](GPUI-PARITY.md);
does not replace its full parity target or mark any implementation complete.

## The product we need first

The owner's immediate work is Virtua Grotesk, a Latin/Arabic font, and a talk
about the node editor and local AI workflows. The first usable Xilem release
must support all three of these outcomes:

1. Real Latin/Arabic design and spacing work, with dependable RTL interaction.
2. Local AI and node-based automation applied to Virtua Grotesk.
3. A coherent, polished interface that designers and design engineers want to use.

These are joint acceptance gates. Arabic and Local AI are not optional later
features. Visual quality is part of every delivered workflow, not a final phase.
The target remains Linebender-native architecture, with reusable gaps feeding
contributions to the ecosystem. No new framework fork is implied by this brief.

Talk date and exact live demonstration remain to be confirmed. Browser and Linux
requirements for the owner's initial switch are also unconfirmed; do not treat
them as abandoned or as prerequisites without that decision.

## Verified planning inputs

Source inspection only; no font edits, training or model runs were performed.
Paths below refer to the Virtua Grotesk repository unless noted otherwise.

- `sources/VirtuaGrotesk.designspace` names Regular (400) and Bold (700)
  masters, plus Regular, Medium, SemiBold and Bold instances. It also contains
  an `a` to `a.bold` substitution rule over weights 500–700. Preserve this data.
- `nodes/bolden.nodes.json` connects the source, `virtua-12m-bolden` model,
  boldening, comparison against Bold and an install node.
- `nodes/train-adapter.nodes.json` connects Regular/Bold source data, training,
  an adapter, boldening and comparison. Its existence is not proof that the
  installed runtime, model and node registry can execute it today.
- Xilem already has Local AI and node runners calling core and an external
  `font-ml` process. `src/edit/local_ai.rs` includes proposal and undo integration;
  `src/edit/nodes.rs` owns graph/run presentation state.
- Core's `text::buffer` carries feature-based shaping and an Arabic fallback.
  Xilem supplies a glyph inventory and invokes Arabic shaping. Its canvas
  `on_text_event` currently matches keyboard events; native composition handling
  therefore needs an explicit audit. No claim of complete Arabic support follows.

Use disposable copies of the real source tree for mutation tests and rehearsals.
Record source revision, model identifier, adapter, parameters and runtime version
for every demonstrated result. Do not assume a Latin-trained model works well
on Arabic; measure its actual coverage and present limitations honestly.

## Gate A — edit Virtua Grotesk in Latin and Arabic

Checklist focus: D01–D07, D09, E01–E03, E06, E08, E13, T01–T07, Q01–Q03.

- Open the designspace, move between Regular/Bold and instance previews, edit
  Latin and Arabic outlines, anchors and metrics, undo, save and reopen.
  Preserve features, kerning, layers and substitution rules.
- Establish a compact proof corpus selected from the font's actual coverage:
  Latin spacing strings; Arabic joining and non-joining contexts; supported
  ligatures; combining marks; Arabic with Latin words, numbers and punctuation;
  multiline text. Include unsupported characters and missing glyph behavior.
- Verify glyph identities, advances, mark offsets and run direction against
  the font's intended features. Distinguish successful feature compilation from
  fallback shaping; do not accept fallback output as proof that features work.
- Test typing, paste, composition/dead keys, visual caret motion, selection,
  deletion and hit testing. Clicking a shaped glyph must activate the intended
  editable source; ligatures and marks need explicit expectations.
- Edit in context and see the line/preview update without losing text or placing
  the edit on the wrong glyph/master. Verify kerning adjustment and its undo.
- Font defects and editor defects need separate reports. Improving Arabic
  coverage in Virtua Grotesk is different work from fixing the shell or shaper.

## Gate B — run and explain a real local AI workflow

Checklist focus: A01–A05, D06–D09, S04–S05, Q01, Q03, Q06.

- Audit the two existing graphs against the installed node registry, `font-ml`
  binary, model files and execution requirements. Start with a small glyph set.
  Use boldening as the candidate first rehearsal; the final demo is owner-selected.
- Make the workflow discoverable from the UI: open graph, choose source/master,
  select model and glyphs, adjust parameters, run, read progress, compare results
  and install or discard with clear semantics. Graph edit/save/reopen must work.
- Keep the interface responsive during inference/training. Cover cancellation,
  missing model, invalid graph, process failure and a stale result arriving after
  a document/master switch. Identify input revisions and output destinations.
- Confirm whether `core.install` runs automatically or requires a separate user
  action; expose that behavior clearly. Do not narrate a review step that execution
  bypasses. Verify undo for installed results and no changes outside the intended scope.
- Rehearse a full chain on a copy: source -> model/adapter -> proposal -> comparison
  -> accepted font edit -> undo -> save/reopen. For training, record elapsed time
  and distinguish a newly trained adapter from a previously prepared artifact.
- Audit chat and live automation against the intended talk narrative. They remain
  parity requirements; their position within this first gate depends on the demo.
- Prepare a repeatable small live run and clearly labeled previously generated
  results for explanation when a long training run exceeds the talk's time budget.
  A staged recording is not evidence that live execution passes.

## Gate C — visual and interaction quality

Checklist focus: S04–S07, E13, T04, Q02–Q04. Follow [DESIGN.md](DESIGN.md).

- Capture representative states: overview, Latin editing, Arabic editing in
  context, node graph, Local AI progress, comparison and an actionable error.
  Inspect actual screenshots; source review alone cannot pass this gate.
- Check Gray and Light, and the theme selected for the talk. Test the intended
  presentation size and display scaling as well as ordinary editing dimensions.
- Keep typography, control heights, panel rules, spacing and icon treatment
  consistent. Use the existing theme/design tokens and shared view recipes.
  Provide legible mixed-script labels and values with no missing-glyph boxes.
- Make node ports, wires, direction, selection and run state understandable.
  Controls and labels remain readable while navigating a graph; status cannot
  depend on color alone. Long model names and errors must not break the layout.
- Keep outline contrast and control-point feedback precise at editing zooms.
  Chrome must not obstruct the shape. Menus, splitters, focus indicators, scrolling
  and pointer feedback must feel reliable, not merely look correct in one frame.
- Verify frame/input responsiveness on the real render backend. CPU snapshots
  provide layout evidence; they cannot establish GPU quality or native IME behavior.
- Record visual review findings and owner acceptance. Do not claim that an
  attractive static mockup means the working editor is ready for the talk.

## Work order and architectural research

Finish the implementation slice already in progress. Then select bounded tasks
from this order rather than walking the entire parity checklist sequentially:

1. Essential document shell and save/undo reliability, with polished empty/open states.
2. Text/canvas/preview state boundary and the Arabic proof corpus. Resolve the
   architecture here before asking implementation tasks to duplicate state.
3. A small end-to-end Arabic editing workflow and a small end-to-end node/AI
   workflow. Refine their UI as they land; neither gate waits for the other to finish.
4. Fill the workflow gaps identified by real Virtua Grotesk sessions and demo rehearsals.
5. Finish remaining full-parity features and decide retirement/platform scope.

Research should answer narrow questions that unblock implementation:

- Which state lives in core, the Xilem app, or a Masonry widget? How do canvas,
  text preview and asynchronous jobs consume the same current document revision?
- What is one undoable operation for kerning, features, metadata, designspace,
  node-driven batches and proposals? Core's current per-glyph snapshot history is
  not evidence that all these document changes have transaction coverage.
- Which missing input/popup/command/rendering capability belongs upstream, and
  what small reproducible example would demonstrate its value beyond Runebender?

The recommendation is to freeze new GPUI feature work while retaining it as a
fallback; this brief does not itself change GPUI's maintenance instructions.
Switch daily work once A, B and C pass. Retire GPUI only after remaining parity
and platform obligations are explicitly resolved. A successful talk rehearsal
and full daily-use readiness are separate evidence.
