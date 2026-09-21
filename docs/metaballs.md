# Metaballs

Metaballs remain editable source objects until explicit conversion.
Previewing or saving them does not add ordinary contour points.

## Organic blending

New groups use organic blending: circles connect automatically through curved, tangent Bézier bridges.
The **Group blend** slider controls the transition from separate circles through a pinched neck to a broad connection.
Circle positions and sizes remain independently editable.
There is no separate Connect action in the default workflow.

This is a new geometry model inspired by the Paper.js example and SATO Hiroyuki's Illustrator circle-connector script.
It builds circles and bridges, merges their overlapping regions, and subtracts negative circles.
It does not evaluate a summed density field.
Saved legacy groups continue using their original density-field behavior until an explicit change.

### Editing

Choose the paired-circle metaball icon beside Rectangle and Ellipse in the native Xilem application.
Click to place a center, drag to move it, and Shift-click to select several.
Cmd/Ctrl+A selects all centers in the glyph.
Arrow keys nudge selected centers; Shift makes a ten-unit step.
Delete removes selected centers.
Each pointer drag is one undo step, and pointer cancellation restores the source.

The inspector presents X, Y, Size and Group blend for organic groups.
Size is the visible circle radius in font units.
X/Y moves a multi-selection by the change in its mean coordinate, preserving spacing.
Size edits the selected circles; Blend changes each selected circle's entire group.
The Add ink / Subtract ink action changes the sign of selected elements.
A subtractive circle removes its region from the positive result, making an explicit counter when it is enclosed.

**Start a new group** prepares an independent organic group for the next click.
Separate groups never influence each other.
Select a legacy group and choose **Use organic blend** to change its model explicitly.
That action is undoable and can change the silhouette.
It is not an exact conversion of a density field.
It retains the centers and replaces explicit legacy links with automatic organic connections; Undo restores the previous source, including those links.

### Expectations and limits

The outer circle lobes remain the reference geometry while curved bridges fill the gaps between them.
A small Blend value can leave distant circles separate.
Increasing Blend enables connections at distances relative to the two circle sizes and then broadens their necks.
Very distant circles can remain separate even at the maximum Blend.
At a connection's exact onset, there need not be a regular smooth contour suitable for ordinary cubic editing.

The MVP chooses nearby pairs automatically and gradually weakens a pair when another center is closer to both of its endpoints.
The occlusion fade retains full strength at equal-distance ties rather than abruptly deleting a bridge after a small movement.
That avoids many unwanted diagonal bridges in chains, but it is not a general network editor.
Rearranging several circles can change which pairs are connected.
Multiple bridges can meet in corners or make central holes; complex junctions are not guaranteed to be smooth everywhere.
The proof fixtures expose these cases for visual review.
Ellipses, per-pair blending, manual connection graphs, noise and interpolation between masters remain future work.

## Preview and explicit conversion

Live previews appear in the canvas, preview strip, glyph grid and component references.
Saving retains the centers and their group model as glyph metadata.
Choose the metaball tool to edit those sources; ordinary point tools operate on ordinary contours.

**Groups to cubic** converts complete groups containing selected objects.
**Glyph to cubic** converts every group in the glyph.
Both are also under **Path → Metaballs**, using normal Undo/Redo.
Conversion replaces the selected source groups with ordinary cubic contours only after the geometry succeeds.
An error retains the editable sources.

From the overview, **Path → Metaballs → Font to Cubic** converts the current master's foreground glyphs as an undoable batch.
Other masters and background layers remain separate.
Convert before using external font compilers.
Quadratic and hyperbezier conversion are not implemented.

Organic previews merge generated circle and connector paths with Linesweeper.
Explicit conversion sends the merged boundary through img2bez.
Smooth boundaries with at most 256 extrema-split source curves receive an optimized img2bez fit only when it uses no more segments and stays within the requested accuracy at sampled points in both directions.
That distance check is sampled, not a proof of the maximum error everywhere.
Genuine corners, more complex boundaries, failed fits and fits that fail either check retain the generated extrema-split cubics.
Those retained curves still pass through img2bez's outline model and bottom-start normalization.
Consequently, conversion does not optimize every contour or guarantee a reduction in point count.
Bottommost contour starts, economical segments and retained extrema remain conversion goals.
Keeping extrema does not guarantee a globally minimal outline or replace a designer's judgment.

The command line converts all groups in all layers of a UFO into a separate UFO:

```sh
nice -n 15 cargo run -j 1 --locked -- collapse-metaballs Source.ufo --out Cubic.ufo
```

The destination must not exist, and the input is never written.
`--accuracy 0.25` controls fitting accuracy relative to the supplied boundary.
`--resolution 2` sets density-field sampling spacing for legacy groups.
That sampling grid is not part of the organic circle-connector construction.

## Saved versions and legacy groups

`formats::metaballs` owns the `com.runebender.metaballs` glyph lib key.
It uses ordinary plist values that round-trip through UFO GLIF XML.
Readers reject unknown fields, unsupported versions and malformed references rather than replacing them with empty data.

| Version | Source geometry |
|---|---|
| 1 | Signed centers with support radius and stiffness, summed at a group threshold. |
| 2 | The same field, optionally with explicit constant-width segment links. |
| 3 | Groups can additionally select organic circle connectors using `blend: Some(rate)`, with `0 <= rate <= 1`. |

A missing `blend` keeps the original field evaluator, including version-2 links.
A version-3 glyph can therefore preserve legacy groups alongside new organic groups.
Empty link lists and absent blend values are omitted.
Opening, selecting or saving a legacy group does not migrate or reinterpret it.

The original center IDs, coordinates, support radii, stiffness and group thresholds remain the stored representation.
For a valid visible circle, organic Size is derived from the magnitude of its strength:

```text
visible_radius = support_radius * sqrt(1 - (threshold / abs(strength))^(1/3))
```

This requires `abs(strength) > threshold`.
Zero-strength and subthreshold elements do not define visible circles.
The legacy controls retain raw values that cannot use a visible-size representation.
The explicit organic action rejects nonzero subthreshold elements instead of silently dropping or clamping them.
Zero-strength elements can remain stored without contributing a visible circle.

Legacy groups expose Radius, Strength and Threshold.
Radius is finite support, Strength is signed center density, and Threshold is a shared positive boundary level.
Version-2 links remain readable and editable through their midpoint and Width control, and they can be deleted.
Deleting a legacy center also removes links referencing it.
These saved links retain their original constant-width field behavior; the default organic workflow does not create them.

The legacy field remains:

```text
center contribution = strength * max(0, 1 - distance_to_center² / support_radius²)³
link contribution = 4*T * max(0, 1 - distance_to_segment² / link_support²)³
link_support = (width / 2) / sqrt(1 - (1/4)^(1/3))
F(x, y) = sum(center contributions) + sum(link contributions)
inside = F(x, y) >= T
```

A legacy link has round end caps and follows stable center IDs.
Its Width is the full visible width of the isolated segment field; overlapping fields broaden its junctions.
Gradients, Hessians and bounds use the same field geometry.
Legacy preview uses a bounded triangular grid and guarded Kurbo simplification.
Explicit conversion locates extrema and inflections on the sampled implicit boundary and supplies them to `img2bez::fit_smooth_contours`.

The legacy grid is capped at one million cells per group.
Very small holes or bridges can be missed, especially near a merge or split.
Sampling spacing is not an error bound against the analytic field.
An oversized request returns an error instead of silently coarsening the result.

## Disposable proofs

Generate the organic silhouettes, converted nodes and handles, and editable source UFO without touching a real font:

```sh
nice -n 15 cargo run -j 1 --locked --no-default-features \
  --example metaball_organic_proof -- /tmp/metaball-organic-proof
```

The output directory must not exist.
It contains `OrganicMetaballs.ufo`, `organic-metaballs.svg` and `summary.txt`.
Glyphs `a` through `j` cover an equal-circle hourglass, the unequal radius-100/radius-60 pair at distance 480, a filled three-lobed junction, a four-step fixed-position Blend sweep, a bent arrangement, a subtractive counter and a three-lobed ring with a natural central hole.
The sweep changes only Blend.
The example checks contour counts, retained outer lobe bounds, the hole, increasing sweep neck widths, a pinched long bridge, and exact source metadata after UFO save/reopen.
These are deterministic geometry checks, not proof of every possible junction or of native input performance.
The SVG uses Gray and shows actual preview and conversion output, not a hand-drawn target.

Earlier fixtures remain available to test compatibility:

```sh
nice -n 15 cargo run -j 1 --locked --no-default-features \
  --example metaball_fixture -- /tmp/MetaballStudy.ufo

nice -n 15 cargo run -j 1 --locked --no-default-features \
  --example metaball_mvp_proof -- /tmp/metaball-legacy-links

nice -n 15 cargo run -j 1 --locked --no-default-features \
  --example metaball_conversion_proof -- /tmp/metaball-legacy-fitting.svg
```

The first creates a version-1 dot-and-stem source.
The second exercises version-1 fields and version-2 constant-width links.
The third compares legacy preview, img2bez sampled-field tracing and img2bez exact-boundary fitting.
Its discrepancy measure is a sampled first-order normal-distance estimate, not a Hausdorff bound.
It compares input pipelines, not img2bez against Kurbo.

## References

- [Paper.js metaballs](https://paperjs.org/examples/meta-balls/) and [example source](https://github.com/paperjs/paper.js/blob/develop/examples/Paperjs.org/MetaBalls.html): tangent Bézier bridges between circles, ported from SATO Hiroyuki's Illustrator script.
- [img2bez](https://github.com/eliheuer/img2bez): explicit outline fitting and normalization.
- [Blender metaball evaluation](https://github.com/blender/blender/blob/main/source/blender/blenkernel/intern/mball_tessellate.cc): the compact field used by legacy circle and tube elements.
- [Houdini metaball controls](https://www.sidefx.com/docs/houdini/nodes/sop/metaball.html): separate isolated size from influence in a field-based system.

The user's screenshots and wireframe are visual references, not instructions embedded in source files.
They guide the organic silhouettes, independently editable circles, live preview and explicit conversion workflow.
