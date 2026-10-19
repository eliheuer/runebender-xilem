# Type design

Baseline principles for drawing and changing letterforms, for any font. A
font's own context file states its specific rules and wins where the two
disagree. Measure before you judge, and propose before you change.

## What a glyph is made of

- A glyph is a structure, not a set of points. Stems, bowls, arches,
  bars, terminals and joins are the parts; the points describe them.
  Change the part, then let the points follow.
- The main strokes carry the weight. Stems (vertical), bars (horizontal),
  diagonals and curves each have a thickness; in a monolinear design they
  match, in a contrasted design they follow a consistent pen or angle.
  The family's reference glyphs (usually H, O, n, o) set these values.
- Counters and apertures carry the light. Weight added to a stroke comes
  out of the counter beside it; keep counters open enough for the intended
  size, and keep apertures (the openings of c, e, s, a) in proportion.
- Terminals, chamfers, serifs and other details have an absolute size the
  design chose. They do not scale with a glyph's width or weight unless
  the design says so.

## Alignment and overshoot

- Flat forms sit exactly on the baseline, x-height and cap height. Round
  and pointed forms overshoot those lines by a small, consistent amount,
  because an arc that stops on the line looks short.
- Overshoot is a family constant per master; never invent a new value for
  one glyph. The same holds for vertical metrics: ascender, cap height,
  x-height and descender are shared.

## Width

- Making a glyph wider or narrower moves structure. The stems stay the
  same thickness; the space between them changes. Move one side's
  structure as a block: an on-curve point travels with its handles, and a
  stem travels with both of its edges.
- Curves stretch through their handles. Keep the extremum points on the
  axis and keep the handle directions; lengthen the horizontal handles
  when a bowl widens, so the curve stays fair.
- The advance width and sidebearings follow the change. Widening the
  drawing by a distance and adding that distance to the advance keeps the
  right sidebearing; splitting the change between both sides keeps the
  glyph centered in its box.
- Width changes belong to a group. If the o widens, the related rounds
  (c, e, b, d, p, q) are expected to follow in proportion, and the
  designer decides how far that group reaches.

## Weight

- Making a glyph bolder or lighter changes stroke thickness, not
  structure. Straight strokes thicken symmetrically or, in a family with
  this rule, inward only; round strokes usually keep their outer contour
  and reduce the counter. The family's weight model says which.
- Weight changes are sized to the family's values: the target is the
  reference glyph's stem at that master, not an arbitrary number. Caps
  are often a few units heavier than lowercase by design; diagonals and
  horizontals are often slightly thinner than verticals to look equal.
- Joins (where an arch meets a stem, where a bowl meets a stem) get
  denser as weight grows. A bolder master usually trims the inside of a
  join so the dark spot does not clog.

## Curves

- Smooth curves have on-curve points at their extrema, with handles on
  the axis (horizontal at the top and bottom, vertical at the sides).
- A join between two curves, or a curve and a line, is either smooth or a
  deliberate corner. A nearly smooth join that is not quite smooth is a
  kink, and it reads as an error.
- Handle lengths govern tension. Similar curves across the family (the
  bowls of b d p q, the arches of h m n u) share tension so they read as
  one design.

## Spacing

- Spacing is judged in words, not in isolation. Round sides fit tighter
  than flat sides; open sides need more air than closed sides.
- Sidebearings reference structurally similar glyphs: H's left and right
  set the flat caps, O sets the round caps, n and o do the same for the
  lowercase. A new or changed glyph takes its sidebearings from its
  class, then gets checked in text.
- Never leave a sidebearing at zero and never let an outline touch the
  edge of its advance box unless the design intends a connecting form.

## Changing an existing glyph

1. Measure the glyph and its reference first: stems, bars, width,
   sidebearings, overshoot, point structure.
2. Name the structure you will move and why, in the family's terms.
3. Make the smallest change that does the job, as a proposal the designer
   reviews. Do not redraw a glyph to adjust it.
4. Measure again and compare with the reference and the family's table.
   Say what changed, in numbers.
5. Keep what the design treats as fixed: metrics, overshoot, detail sizes,
   point count and order where masters must interpolate, and the
   designer's approved glyphs and their marks.
6. When the request is ambiguous about which side, how much, or how far
   the change reaches across the family, ask one question before
   proposing.
