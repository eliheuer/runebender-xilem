# Live experiment MVP

The native GPUI Nodes workspace uses connected nodes for the live font, independent
versions, Designbot proofs, and explicit application to the root. Controls and preview
images live inside the canvas. Xilem exposes experiments through MCP; its canvas does
not yet implement these live node controls.

## Start

Install current `designbot`, `runebender-core`, and the native editor. Restart the
editor after updating. In OMP run `/mcp reload` and connect to the intended editor.
Read `design_context`, the official type-design guide, and the font's DESIGN.md.

Open Nodes (or click New if an older workflow is open). The starter graph connects
Current font to two Font version nodes, each with its own Designbot proof. Run creates
unrun versions in connection order and renders the proofs; it never executes Apply.
Create version snapshots a connected input once. Repeating it preserves the result.
Fork direction adds another connected version and proof. A created version retains
its original input connection; fork another direction to try a different input.

The root is the open, unsaved master. For comparisons made at different times, create
a baseline version and fork that twice, so both directions have exactly the same input.
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

The Font version node displays the branch name to give OMP. Branches created through
MCP also appear as connected version and proof nodes in the open live graph.

Designbot proof nodes render selected glyphs (up to 256; six drawn glyphs when there
is no selection), or the fixed kerning specimen "AVATAR To Wa". For custom text or
reference sets, ask OMP for a proof, then click Latest OMP proof in that proof node.
The PNG shown and exported PDF use the same scene. Images are snapshots: refresh after
edits. Export PNG opens a save dialog for full-size inspection outside the thumbnail.

Add apply node creates a connected output. Clicking Apply changes accepts the input
version's changed glyph outlines and kerning, including contour-structure changes. Use `experiment_apply` through OMP for selective
existing glyphs or kerning-only application and an explicit keep_structure policy.
Root changes since the baseline produce conflicts before any mutation. Unrelated edits
survive. No operation saves the root to disk: inspect, undo if needed, then Save normally.

Undo last application restores the affected glyphs and/or kerning only if they still
match what was applied. It refuses to overwrite newer work. Glyph edits also enter the
ordinary glyph undo history; mixing undo routes can cause the transaction undo to refuse,
which preserves later work rather than guessing intent.

## Limits

Save as new UFO exports a version's complete master to a new .ufo directory and
refuses existing destinations. Discard version removes a leaf experiment without
changing the root; discard its children first if it has any.

Versions are session-only and disappear when the font closes. The Nodes Save action
stores layout and connections in .nodes.json, but does not persist version contents.
Reopening that workflow requires creating new versions; it cannot restore old AI
results. Save important versions as new UFOs before closing. Normal font Save persists
accepted root changes. Maximum 16 versions per document.

Live font ports deliberately cannot connect to the older disk-based model-task ports.
OMP remains the AI client for this increment. Local/cloud process nodes, persistent
version bundles, and comparison controls beyond adjacent proof previews remain future
work. The MVP does not merge overlapping conflicts, create absent glyph records,
edit groups, or automatically dispatch multiple AI jobs. Family-wide
interpolation validation remains a separate review. Keep experimental proofs as files
and record accepted decisions in the font project's design brief.
