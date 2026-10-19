# Font engineering

Baseline rules for keeping a font's sources sound while they are edited,
for any font. A font's own context file adds its grid, build and naming
rules and wins where the two disagree.

## Sources and masters

- A designspace holds masters at axis locations and the instances built
  between them. Every master must have, for every glyph, the same
  contours in the same order, the same number of points, the same point
  types and the same component references. Only coordinates, advance
  widths and anchor positions may differ.
- An edit that changes point count or order in one master breaks
  interpolation for that glyph until every master matches. Prefer edits
  that move existing points over edits that redraw.
- Sources are addressed by a stable identity, not by display order or
  name. Read the project before choosing one.

## Outlines

- Cubic outlines (UFO, designspace) use on-curve points with off-curve
  handles. An on-curve point marked smooth promises a tangent-continuous
  join; the geometry must honor the mark.
- Outer contours and counters wind in opposite directions so fills come
  out right. Keep the winding the design uses; do not reverse a contour
  to fix a fill, fix the geometry.
- Coordinates are integers in font units unless the design works on a
  finer grid. Many designs commit to a coarser grid for machine edits and
  reserve finer steps for hand corrections; the font's context file says.
- Open contours, overlapping contours and stray single points are errors
  in a finished master unless the design uses overlaps deliberately and
  removes them at build time.

## Components and anchors

- A composite glyph places other glyphs by reference, with a transform.
  Editing the base glyph changes every composite that uses it; say so
  when proposing a change to a base.
- Anchors position marks (such as `top` on a base and `_top` on a mark).
  Moving an anchor moves every mark that attaches there. Anchor names and
  presence must match across masters.
- Do not decompose a component to edit a composite unless the designer
  asks; decomposition loses the link and the compatibility that came with
  it.

## Metrics and spacing

- The advance width is a glyph's own; sidebearings are what is left of
  it on either side of the drawing. Changing an outline changes the
  sidebearings unless the advance changes with it.
- Kerning adjusts pairs on top of spacing. Fix spacing first; kerning
  that compensates for bad sidebearings has to be redone when they are
  fixed. Kerning groups apply one value to a whole class; a change to a
  group changes every pair in it.
- Vertical metrics (ascender, descender, x-height, cap height, and the
  OS/2 and hhea values derived from them) are family-wide. A glyph that
  exceeds them is either a deliberate exception or an error.

## Encoding and names

- Glyph names and Unicode values are separate. A name such as `five` is
  the font's; the codepoint U+0035 is the character it shows. Several
  glyphs can share a codepoint through features, and some glyphs have no
  codepoint at all.
- Empty and absent are different: an empty glyph exists with no outline;
  an absent glyph has no record. Create the record before drawing into
  it.
- Renaming a glyph touches components, kerning groups, features and
  composites that reference it. Treat a rename as a project-wide edit.

## Proposals, review and saving

- Edits by an agent are proposals: they are written to a separate layer
  and installed only when the designer accepts them. Read a glyph's
  revision before proposing against it; a proposal against a stale
  revision is rejected.
- The designer's grading marks (such as a color meaning approved) are
  not the agent's to set or clear. One agent mark, with a dated note, may
  ask for the designer's eyes.
- Nothing an agent does saves the font. The designer saves, builds and
  commits.

## Checks that catch most errors

- After an outline edit: point count and order match the other masters,
  smooth points are actually smooth, extrema are on-curve, coordinates
  are on the design's grid, the glyph still fits its vertical metrics.
- After a width edit: both sidebearings are intentional, the glyph still
  reads in words, and composites that reuse it still align.
- After an anchor or component edit: every composite and mark that
  depends on it, in every master, still renders as intended.
- After any edit: the proposal lists what changed in numbers, and the
  designer, not the agent, decides whether it stays.
