# Supervised Virtua Grotesk work trial

This is a bounded first trial of the Xilem editor, not a full parity claim. It
uses a disposable copy because the real Virtua Grotesk sources contain frozen
green and protected purple work that this trial must not touch.

## Prepare and launch

From the `runebender-xilem` checkout:

```sh
trial_root="$(mktemp -d /private/tmp/runebender-virtua-trial.XXXXXX)"
cp -R ../virtua-grotesk/sources "$trial_root/sources"
mkdir -p "$trial_root/nodes"
cp docs/parity/2026-09-11/bolden-review.nodes.json "$trial_root/nodes/"
RUNEBENDER_AI_DEVICE=cpu cargo run --locked -- \
  "$trial_root/sources/VirtuaGrotesk.designspace"
```

Keep the terminal open. The disposable path printed by `printf '%s\n'
"$trial_root"` is the only font tree this trial should change. The tested source
is the Regular master in `VirtuaGrotesk.designspace`.

## Editing and Arabic checks

1. Confirm the Regular master is selected. Search for `R`, open it, change one
   point, its advance, and an anchor. Exercise Undo and Redo after each kind of
   edit, save, close, and reopen the disposable designspace.
2. Navigate to representative Arabic glyphs and confirm that the selected glyph,
   outline, Unicode, advance, anchors, and master agree across grid, canvas, and
   inspector.
3. Choose the Text tool and enter `R لا 123 بِ`. Confirm lam-alef joining, the
   kasra placement, mixed-direction ordering, and Left/Right/Up/Down/Home/End.
   Hold Shift with those movement keys to select through the Arabic run; use
   Backspace/Delete and typed replacement, and confirm joining reshapes around
   the edit. Click on both sides of the lam-alef and confirm the caret follows
   the visible cluster.
   Also replace text through the system input method so native preedit, cancel,
   and commit behavior is exercised.

Automated proof covers the real Virtua designspace, lam-alef substitution, mark
ordering, mixed bidi layout, logical cursor movement, selection and deletion,
replacement, and pointer hit mapping through the lam-alef cluster. Native
input-method and pointer delivery still need this supervised pass, so production
Arabic readiness remains a trial result rather than an automated claim.

## Local AI check

The tested local runtime is `/Users/eli/.cargo/bin/font-ml`, SHA-256
`7d7c15a6e36d6cb6175e95971de3665eb657c6ed150026cbac33faf5a35ab`.
It does not expose a version flag. `font-ml tasks --json` reported `bolden` and
`train` implemented; complete, generate, spacing, kerning, and field were not
implemented. Commit `9221fc8` verifies that the Local AI task rows and generated
node types preserve this installed declaration and makes task-registry launch,
exit, schema, and JSON failures visible in the rail.

Choose `virtua-12m-bolden` in the Local AI rail, open `R`, and run **Bolden: this
glyph**. Do not run every glyph or training during this trial. When the run
finishes:

1. The foreground must remain unchanged and the proposal must remain pending.
2. **Compare on canvas** must show the amber proposed outline over the editable
   foreground and toggle off again without changing the font.
3. **Install** must apply the proposal only after that explicit click.
4. **Undo install** must restore the original R. Run again and verify **Discard**
   removes the proposal without touching the foreground.
5. Start one more bounded R run and press **Cancel**; confirm progress stops and
   no proposal is silently installed.

The model is a local draft trained 2026-07-16 and promoted 2026-09-01. Its
recorded MAE is 16.8 against a 19.0 baseline; fitted strength rarely converges
and advance deltas skew narrow. Review its output as a suggestion, not an
approved design result. Verified model hashes are:

- `config.json`: `1e13ab95e0c66c221f5276a54f5a62e82ac84fb27bf00b03f64acf9e8a4d2318`
- manifest: `075324830bf02cdfd08199c01ef40ee01a117bb6e7919a1dafa96a3535dc410e`
- weights: `797431ab079a39f8cc7f32cfc38160f6f3f20374a0825c36419206e31276f11c`

No model was downloaded, installed, or trained during implementation. The real
CPU integration test moved 40/40 R points, reported advance delta +18, left the
foreground intact, installed explicitly, and restored the original with Undo.

## Review-only node graph

The copied `bolden-review.nodes.json` is a five-node, five-link demo: Font and
Model feed Bolden, the Bold master feeds Compare, and there is deliberately no
Install node. In the overview, select R and Cmd-click S, open **Nodes**, choose
**bolden-review**, change Strength if desired, Save, then Run. Both selected
glyphs—not the whole font—must report progress. When it finishes, return to R
and the Local AI rail: the graph proposal must be pending in the same Compare →
Install/Discard → Undo workflow described above.

The integration test copies the full designspace, edits and reopens the graph,
runs R and S through the real CPU model, observes start/progress/end and Compare
output, proves neither foreground changed, then explicitly installs and undoes
R. The fixture leaves Bolden's optional `reference` input disconnected because
the current installed `font-ml` panicked in its fitted-reference path during
this trial (`swap_remove index (is 0) should be < len (is 0)`). The separate
Compare node still compares the proposal with the Bold master. The direct Local
AI rail likewise uses its explicit Strength control rather than hidden automatic
reference fitting.

Commit `6d7cc51` also wires the Nodes toolbar Open button to the native graph
picker. A normal disposable test covers New, Save, graph-tab rescan, parameter
editing, typed validation, reopen, and exact node-local failure reporting for a
train-adapter graph without invoking training.

## Evidence and limits

Implementation commits are `a40808b` (Latin/Arabic navigation and metadata),
`588b605` (disposable edit/save safety), `1d96364`
(real Arabic and local-model proof with explicit install), and `dd0159a`
(proposal comparison), `ce75e3b` (review-only node route), and `1de39f6`
(bidi text selection). Headless inspected captures are
`r01-arabic-beh-{gray,light}.png`, `r03-mixed-text-{gray,light}.png`,
`r03-text-selection-{gray,light}.png`, and
`r04-ai-compare-{gray,light}.png` in this directory;
`r05-nodes-review-{gray,light}.png` shows the five-node graph. They use
the CPU renderer and do not prove native GPU, input method, pointer, cancellation,
or platform-menu behavior.

Commit `a7630e5` adds a deterministic worker proof for progress, cancellation,
process termination, absence of proposal/foreground changes, and complete
multi-line failure diagnostics. The native Cancel step above remains in the
trial because background correctness does not prove its pointer interaction or
visible timing.

Commit `4dd2518` captures the document, master, editor glyph, and canonical
foreground revisions when direct and node jobs start. Completion refuses to
adopt a proposal or reload an installed node result after any target changed;
normal tests cover reload, glyph switching, edited target glyphs, and changed
all-glyph inventories. This safety check does not replace the native interaction
steps above.

Validation through `6d7cc51`:

```sh
cargo test --locked --bin runebender-xilem -- --test-threads=1
cargo test --locked --bin runebender-xilem -- --ignored --test-threads=1
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Results: all 448 normal workspace tests passed; all four ignored real
Virtua/model tests passed; clippy passed. Report any mismatch with the disposable
path, master, glyph, action, expected result, and whether it reproduced after
reopening.
