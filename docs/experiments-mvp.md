# Live experiment MVP

The GPUI Nodes workspace includes live version cards above the general graph canvas.
These are a first version of the experiment graph, not yet arbitrary wired nodes in
`.nodes.json`. Xilem exposes the same core operations through MCP; it does not yet
have the version-card UI.

## Start

Install current `designbot`, `runebender-core`, and the native editor. Restart the
editor after updating. In OMP run `/mcp reload` and connect to the intended editor.
Read `design_context`, the official type-design guide, and the font's DESIGN.md.

The root is the open, unsaved master. Click Fork to capture it. Fork that baseline
twice to make versions with exactly the same inputs, even if the root changes later.
Each branch has its own font and proposal layers. Use `branch` and `master` on every
agent operation targeting a version; omitting `branch` addresses the root.

Example OMP brief:

> In master 0, fork baseline, then kern-a and kern-b from baseline. Use the same
> approved references and text for both. Work only on kern-a in this conversation.
> Read its kerning revision, propose your pair values with experiment_kern, and
> produce a specimen of "AVATAR To Wa". Leave the root unchanged.

A second OMP session can connect to the same editor and work on kern-b. The editor
serializes calls; the sessions share the named versions. The MVP does not launch
or select AI providers automatically. Keep the provider and model in OMP, and record
which model/brief was used in the fork's reason.

## Draw and kern

- `experiment_fork`: root snapshot or child of a named parent; unique name and reason.
- `experiment_list`: versions, parents, changed glyphs, kerning status and recent intent.
- `read_glyph`, `glyph_inventory`, `proof`, `propose_edits`, `proposal_install`: accept
  branch; install a drawing into that branch before applying it to the root.
- `read_kerning`: table, group membership and revision.
- `experiment_kern`: branch-only pair changes; numeric value sets a pair, null removes
  it. Existing glyphs or side-specific groups are supported. Group editing is excluded.
- `specimen`: a one-page, basic Latin proof at 18, 24, 36 and 48 points. Live outlines
  use harfrust positioning plus UFO kerning; feature kern is disabled to avoid applying
  kerning twice. Other feature positioning remains active. Unsupported feature includes
  or missing characters fail explicitly. This is not a complete compiled-font proof.
- `export_proof` (MCP): same snapshot as PNG/PDF, explicit output path, no overwrite.

## Review and apply

Cards can render a glyph sheet or a fixed kerning specimen. To choose other glyphs
or text, ask OMP for a proof, then click Show latest agent proof on that card.
The PNG shown and exported PDF use the same scene. Images are snapshots: refresh after
edits. Export PNG opens a save dialog for full-size inspection outside the thumbnail.

Apply all accepts that version's changed glyphs and kerning; the label explicitly
allows changed contour structure. Use `experiment_apply` through OMP for selective
existing glyphs or kerning-only application and an explicit keep_structure policy.
Root changes since the baseline produce conflicts before any mutation. Unrelated edits
survive. No operation saves the root to disk: inspect, undo if needed, then Save normally.

Undo last application restores the affected glyphs and/or kerning only if they still
match what was applied. It refuses to overwrite newer work. Glyph edits also enter the
ordinary glyph undo history; mixing undo routes can cause the transaction undo to refuse,
which preserves later work rather than guessing intent.

## Limits

Versions are session-only and disappear when the font closes. Normal Save persists
accepted root changes, not experimental versions. Maximum 16 versions per document.
The MVP does not persist/reopen experiment graphs, merge overlapping conflicts, create
absent glyph records, edit groups, or automatically dispatch multiple AI jobs. Family-wide
interpolation validation remains a separate review. Keep experimental proofs as files
and record accepted decisions in the font project's design brief.
