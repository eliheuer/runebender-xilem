# Local Virtua models

The native Brush panel can discover compatible local checkpoints, select one, and run **Draft with Virtua** without overwriting the source glyph.
Use **Open folder** to reveal the library and **Refresh** after adding a package.
The default library is `~/runebender/models`; `RUNEBENDER_MODELS_DIR` can override it.

## A model package

Each model gets its own folder, named with letters, digits, hyphens or underscores:

```text
models/
  runtime.json
  virtua-clean1/
    manifest.json
    config.json
    vocab.txt
    weights.safetensors
```

The manifest identifies the supported adapter:

```json
{"name":"Virtua Clean1", "format":"virtua-sketch-v1"}
```

Copy the three checkpoint files together from the same trained run.
Do not mix vocabularies, configurations and weights from different runs.
The current adapter supports the installed Virtua sketch architecture and tokenizer, not arbitrary Hugging Face weights.
Inference validates the checkpoint and retains its hashes with the candidate.
A discovered package marked Ready has the required files and an installed runtime; inference and drawing quality still require testing.

## Installed runtime

Model folders contain data, not commands to execute.
The host's `runtime.json` points to the separately installed, trusted FontGarden environment:

```json
{"repository":"/absolute/path/to/font-garden-lab"}
```

That repository must contain `glyphlab` and `.venv/bin/python` with its inference dependencies installed.
The existing adapter also checks `~/.cargo/bin/img2bez` for provenance, while the model input uses Runebender's calibrated Rust tracer.
This is not yet an automatic runtime installer.
`RUNEBENDER_SKETCH_REPOSITORY` overrides the configured runtime; legacy requests without a selected package can still use `RUNEBENDER_SKETCH_CHECKPOINT`.
Model copying leaves FontGarden's training and development files unchanged.

## Drawing and review

Select the intended Arabic occurrence in Text, switch to Brush, draw temporary ink, and choose an approved reference and rationale.
Choose a ready local model and **Draft with Virtua**.
Identity strength follows the Web control, from 0 to 1.5.
Each draft runs at most three samples with temperature 0.5 and seed 0, retaining the script-selected candidate; its score is not visual approval.
The optional Unicode hint is explicit conditioning, not an edit to the glyph's encoding.
For example, a medial kaf experiment may explicitly use `0643`; a contextual glyph may have no source Unicode and may not exist in the checkpoint's name vocabulary.
The model does not consume the selected green reference at inference; that reference supports grading guards and human review.

Check status, open the comparison, run the matched proofs, and inspect the candidate before Apply.
Retry retains whether the previous request was plain tracing or Virtua inference.
No model result is automatically approved or graded green.
Keep the sketch, settings, model identity, output and visual feedback when recording failures for future training.
