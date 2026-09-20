# OMP compiled-proof worker report

This worker owns `scripts/agent_omp_proof_trial.py` and this report.

It does not change Rust, central coordination documents, user OMP configuration, credentials, or original font sources.

## Trial design

The script creates a disposable minimal UFO with `.notdef`, an edit glyph and a marker glyph.

`A` is the ordinary receipt-backed edit target.

The second glyph has a randomized opaque name, private-use Unicode value and one of two silhouettes.

Its diamond or rounded geometric marker is generated from a private seed retained only in the local report.

The prompt and text tool metadata contain neither the marker shape nor the seed.

The model discovers the private-use codepoint through `glyph_inventory` and must not call `read_glyph` for the marker.

The model uses `agent_apply`, `agent_receipt`, and an exact `agent_apply` retry for the separate `A` width edit.

It then calls `agent_cancel` on the committed identity to verify that cancellation reports `committed` and does not undo the edit.

The current protocol represents that terminal non-prevention result as `ok: false` and an MCP tool error carrying `cancellation_status: committed`; the harness requires that exact combination.

It then starts `proof_start`, polls `proof_status` with `include_image: true`, and classifies the received marker image.

It releases the terminal proof after retaining its metadata and image evidence.

The harness records the compiled font hash, canonical input hash, recipe, shaped glyph metadata, image bytes and hash, model/client/binary hashes, and before/after source-byte manifests.

Raw model reasoning, credentials, and unreviewed OMP output are not written to the report.

The reviewed transcript retains tool names, redacted arguments, selected receipt and proof metadata, proof-status image-block presence, and assistant text only.

The pass predicate requires the exact apply payload twice, an immutable matching receipt from all three responses, `replayed: true` with no second root change, committed cancellation, matching headless host state, unchanged source bytes, current proof lineage, terminal proof release, and an assistant response after the proof-status PNG block.

## Intended command

```text
python3 scripts/agent_omp_proof_trial.py \
  --binary /absolute/path/to/runebender \
  --omp /absolute/path/to/omp \
  --output-dir /private/tmp/runebender-agent-omp-proof-trial-20260920 \
  --max-time 180
```

The disposable MCP configuration points only to the explicitly supplied pinned executable.

The normal OMP credential store is used only when the user authorizes the external model trial.

No profile, client configuration, or credential file is edited by the script.

## Evidence boundary

An MCP tool result or local PNG inspection is not model evidence.

A passing model trial requires an actual OMP run with `gpt-5.6-luna`, a valid PNG block from the completed `proof_status` call, and a final assistant response after that block that identifies the privately randomized marker silhouette correctly.

Transport success, receipt-backed editing correctness, PNG delivery, and visual interpretation are reported separately.

The report calls the observed sequence `valid_png_in_proof_status_result` and `assistant_response_after_image_block`; it does not turn a local PNG or an unrelated image tool result into model evidence.

## Current status

Eight credential-free unit tests cover complete evidence, changed retries, cancellation outcomes, malformed and unrelated images, pre-image guesses, redaction, and randomized marker ground truth.

Run them with:

```text
python3 -m unittest discover -s scripts -p 'test_agent_omp_proof_trial.py' -v
```

The external OMP invocation was not run during this review because the prior automatic approval rejection remains in force.

No OMP model success, image reception, or visual interpretation is claimed until that authorization and run occur.
