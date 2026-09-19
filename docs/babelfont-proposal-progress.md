# Babelfont proposal and version migration

This document records the M10 document-layer migration for proposals, live tools and isolated experimental versions.
It describes verified implementation state, not the completion state of later application or headless caller migrations.

## Canonical ownership

Proposal glyphs are canonical auxiliary layers addressed by stable `SourceId` and `LayerId` values.
Creating, listing, previewing, installing and discarding a proposal no longer requires a persistent mutable Norad font.
An installed root glyph commits through Project's guarded canonical replacement API and records one Project-owned history step when the foreground actually changes.
Proposal creation and discard never mutate a foreground layer.

Experimental versions own cloned canonical layer drafts and canonical group and kerning metadata.
They do not own a Norad `Font`, `Glyph` or `Master` baseline or working copy.
Versions remain session-only, have a 16-version limit, retain parent provenance and use stable source identities across source display reordering.
Removing a source makes root application fail cleanly without retargeting the version.

## Revision and compatibility boundary

The public proposal revision remains `glif-sha256` over the exact serialized GLIF payload.
Canonical layers materialize a transient glyph only at this named revision and external proposal codec boundary.
Ordinary batch operations address canonical point, component and anchor identities directly.
Complete outline replacement crosses the codec boundary because `DrawingContour` and the external proposal-layer protocol define a complete UFO outline payload.
Proposal installation copies contours, components, anchors and exact advance values directly between canonical drafts.
It retains matching stable object identities and foreground glyph metadata without materializing either canonical layer as a Norad glyph.

Canonical installation requires every proposal glyph to carry its foreground revision.
Missing, malformed and stale revisions fail closed and leave both the foreground and proposal unchanged.
Optional structure checking compares canonical contour and point roles directly.
External UFO proposal layers can be adopted explicitly, but an unguarded external glyph cannot be installed.

## Atomic operations and history

Batch validation and proposal drafts complete before the first canonical proposal layer is added.
Canonical composition plans capture each foreground revision and stage every payload and layer identity before one guarded source-structure commit.
Empty, inconsistent, stale or destination-conflicting plans leave canonical layers, document revision and histories unchanged.
Install candidates, revision checks, structure checks and replacement snapshots complete before the first foreground mutation.
Stale and incompatible candidates remain reviewable in the proposal layer.
Successful selected installs remove only the installed proposal glyphs and leave unselected or skipped glyphs in place.
Installing or discarding the last proposal glyph removes the empty auxiliary layer container from canonical persistence and the compatibility projection, so save and reopen cannot resurrect it.

Experimental apply checks every selected layer and optional metadata change against the shared root baseline before mutation.
Unrelated root edits survive selective apply and guarded undo-apply.
Changed root layers enter Project-owned layer history, while session-version edits remain isolated until an explicitly authorized apply.

## Live and Nodes routing

Live proposal commands now dispatch to canonical Project or isolated-version operations instead of mutating a transient `Master`.
Root installation still requires explicit `user-approved` authorization.
Live project information exposes stable source identities, and Nodes font-version values store a stable source identity plus an optional branch name.
Transient Norad fonts remain only for current proof, shaping and explicit new-UFO export adapters while their owning later migration lanes finish.

## Focused verification

The canonical proposal tests cover guarded root installation and undo, stable object identity and metadata preservation, stale revision rejection, unguarded external proposal rejection, isolated-version installation and removal of the last proposal layer across save and reopen.
The composition proposal tests cover proposal-only publication, explicit install and undo, direct revision records and atomic rejection of invalid, stale, empty and conflicting plans.
The experiment tests cover stable source reorder, removed sources, atomic conflict rejection, selective apply, unrelated root edits, guarded undo and isolated child state.
The live tests cover authorization, canonical proposal creation and installation, structural refusal, drawing installation and canonical undo and redo.
The Nodes live tests cover stable branch connections, source reorder and removal, missing-binding refusal, isolated edits, result preservation, explicit apply routing and new-UFO export.

Focused results on 2026-09-19 were 11 proposal batch tests, 8 composition tests, 8 proposal tests, 7 experiment tests, 5 live-tool tests and 5 Nodes live tests, all passing.
Strict workspace validation remains the integration branch's responsibility after the M06 and M11 callers land.

## Remaining integration boundaries

The legacy standalone `Font` proposal functions remain for M11 file-command compatibility and are not used by the live canonical path.
The application-owned local-AI and Nodes callers are part of M06 and must consume these canonical APIs without reintroducing a mutable font copy.
M11 file workflows can use `canonical_glyph_revision`, `compatible_layers`, `propose_project`, `write_composition_project`, `install_project`, `discard_project` and `adopt_external_project`.
