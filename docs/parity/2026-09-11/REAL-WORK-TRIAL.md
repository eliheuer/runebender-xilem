# Supervised Virtua Grotesk work trial

This is a bounded first trial of the Xilem editor, not a full parity claim. It
uses a disposable copy because the real Virtua Grotesk sources contain frozen
green and protected purple work that this trial must not touch.

## Prepare and launch

From the `runebender-xilem` checkout:

```sh
trial_root="$(mktemp -d /private/tmp/runebender-virtua-trial.XXXXXX)"
cp -R ../virtua-grotesk/sources "$trial_root/sources"
cargo run --locked -- "$trial_root/sources/VirtuaGrotesk.designspace"
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
   Also replace text through the system input method so native preedit, cancel,
   and commit behavior is exercised.

Automated proof covers the real Virtua designspace, lam-alef substitution, mark
ordering, mixed bidi layout, and logical cursor movement. Native input-method
behavior still needs this supervised pass. Text-range selection and pointer hit
mapping across RTL clusters are not implemented, so this trial must not treat
the text tool as ready for production Arabic editing yet.

## Local AI check

The tested local runtime is `/Users/eli/.cargo/bin/font-ml`, SHA-256
`7d7c15a6e36d6cb6175e95971de3665eb657c6ed150026cbac33faf5a35ab`.
It does not expose a version flag. `font-ml tasks --json` reported `bolden` and
`train` implemented; complete, generate, spacing, kerning, and field were not
implemented.

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

## Evidence and limits

Implementation commits are `588b605` (disposable edit/save safety), `1d96364`
(real Arabic and local-model proof with explicit install), and `dd0159a`
(proposal comparison). Headless inspected captures are
`r03-mixed-text-{gray,light}.png` and `r04-ai-compare-{gray,light}.png` in this
directory. They use the CPU renderer and do not prove native GPU, input method,
pointer, cancellation, or platform-menu behavior.

Validation at `dd0159a`:

```sh
cargo test --locked --bin runebender-xilem -- --test-threads=1
cargo test --locked --bin runebender-xilem -- --ignored --test-threads=1
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Results: 70 normal tests passed; all three ignored real Virtua/model tests
passed; clippy passed. Report any mismatch with the disposable path, master,
glyph, action, expected result, and whether it reproduced after reopening.
