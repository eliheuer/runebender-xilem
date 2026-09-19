# Babelfont filesystem adapter progress

This bounded M12 slice moves ordinary UFO and Designspace loading and saving behind a document-owned filesystem adapter.
It does not change native Save As or New Font callers, headless commands, imported Glyphs, binary or Python Babelfont workflows, or feature-include relocation.
M12 remains globally incomplete until those separately owned callers and guarantees are migrated.

## Import boundary

The adapter loads every full UFO source in a Designspace before Project construction begins.
Norad remains the transient UFO and Designspace codec, while Project receives validated source projections with their resolved destinations and preservation records.
The preservation record retains exact per-layer GLIF paths and every regular on-disk file outside Norad's supported UFO model.
Symbolic links are rejected explicitly rather than followed or silently dropped.

## Export boundary

Project first creates an immutable export plan containing every canonical source snapshot, its unchanged destination, its preservation record and optional Designspace metadata.
The plan rejects empty, duplicate or nested destinations before serialization.
Every UFO is written to a temporary sibling, receives its original metainfo, exact GLIF paths and unrecognized file payloads, and is reloaded for validation.
The Designspace is also written and reloaded in staging.
Only after every artifact validates does the plan replace the live destinations, retaining backups for rollback until publication completes.
Dirty flags are cleared only after the complete plan succeeds.

## Focused evidence

The compact filesystem tests use temporary sources and prove ordinary save and reload preserve custom layer order and directories, custom `contents.plist` GLIF filenames, images, nested data, unknown lib values, custom metainfo and an unrecognized binary root payload.
A two-source Designspace regression makes the second source fail Norad validation and proves the first live UFO is unchanged, both sources remain dirty and staged files are removed.
The complete `variable_project` integration suite passes 63 tests with the configured Virtua Grotesk sources.
Strict warning-denied library and test Clippy passes.
