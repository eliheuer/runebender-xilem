# Native asynchronous compiled proofs

The native live adapter exposes `proof_start`, `proof_status`, `proof_cancel` and `proof_release`.
These tools capture the canonical unsaved family and never save or mutate the font.
The older `proof` tool remains a source-geometry proof and does not establish compiled-font lineage.

## Capture and identity

Read the exact document epoch and canonical document revision before starting a proof.
Supply `expected_document_epoch`, `expected_document_revision`, a document-local `operation_key` and a recipe.
The recipe has `text`, `normalized_location`, `right_to_left`, `features`, and optional `script` and `language`.
Coordinates are normalized values in document axis order, one per axis, with at most 64 axes.
Text is bounded to 4096 UTF-8 bytes and shaping to 1024 glyphs.
Use explicit script/language and direction for a single shaping run; this API does not implement mixed-script paragraph layout.

The application captures immutable inputs on its owning thread.
One process-wide background worker compiles, shapes, extracts outlines and renders the PNG from the same compiled font bytes.
The worker survives document replacement so reloads cannot accumulate compiler threads.
A proof handle retains one artifact and recipe; it does not retain an externally reusable font snapshot or grant an export capability.

An identical `proof_start` retry returns the same retained handle even after the document revision advances.
Reusing that key with a different request rejects.
New requests for a stale revision, an active canvas gesture, invalid coordinates or exhausted retention reject before compilation.
Compilation and rendering are asynchronous; source capture still runs on the application thread.

## Polling and images

Call `proof_status` with the exact epoch and returned `proof_id`.
Its status is `queued`, `running`, `completed`, `failed` or `cancelled_before_start`.
`captured_document_epoch` and `captured_document_revision` always describe the original capture.
The ordinary `document_revision` envelope describes the current live document at the time of the response.
`current` and `stale` explicitly compare those identities; a completed old proof stays retrievable but cannot claim the new revision.

A completed result includes `font_sha256`, `canonical_input_sha256`, compiler identity, recipe and shaped glyph IDs/names, clusters, advances and offsets.
Compiler identity currently covers the package version and pinned Cargo lockfile, not the executable hash; client acceptance evidence records the executable separately.
Set `include_image: true` to receive the worker-rendered PNG.
The socket/CLI payload carries `png_base64`; the MCP adapter moves those exact bytes into an `image/png` content block and retains the metadata in a text block.
It neither recaptures nor renders a second image.
Metadata-only polling avoids retransmitting the PNG.
A failed job has `proof_error` and no image.
Protocol image delivery does not establish actual model image receipt or interpretation; those require separate client trials.

## Cancellation and retention

Each document retains at most eight proof jobs, including terminal artifacts and retry keys.
The single process-wide queue permits at most 16 pending and 32 retained jobs across documents.
`proof_cancel` prevents queued work from starting.
A running compiler cannot be interrupted and reports `too_late`; this does not affect the font or change the result's captured identity.
Proof cancellation is separate from edit cancellation.

Use `proof_release` on a terminal job to discard its artifact and operation key.
Queued/running release rejects with `proof_busy`; cancel queued work first.
After release, the old handle is unknown and the operation key may be used for a new capture.
Closing or replacing a document cancels its queued jobs, releases terminal artifacts and marks running results for bounded collection on the next proof operation.
No replacement document can query the old handles, even when opening the same path.
