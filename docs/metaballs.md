# Metaballs

Metaballs remain editable source objects until explicit conversion.
They are not UFO anchors, and previewing or saving them does not add ordinary contour points.

## Xilem editor

Choose the paired-circle metaball icon beside Rectangle and Ellipse in the native Xilem application.
Click to place a center, drag to move it, and Shift-click to select several.
Cmd/Ctrl+A selects all centers in the glyph.
Arrow keys nudge selected centers; Shift makes a ten-unit step.
Delete removes selected centers and connections that depend on those centers.

The right inspector uses sliders for X, Y, Size and Blend reach when selected positive elements can be expressed that way.
Size is the radius of the visible circle when an element is alone, in font units.
Blend reach is the distance from that isolated circle to the outer edge of its influence.
Increasing Blend reach adjusts the underlying radius and strength together, preserving the element's isolated Size.
Other elements in the group still contribute to the actual blended silhouette.
X/Y moves a multi-selection by the change in its mean coordinate, preserving spacing between selected centers.
Each slider drag or center drag is one undo step.
Pointer cancellation restores the source without adding an undo step.

Negative, zero-strength, subthreshold and otherwise unmappable legacy elements retain raw Radius, Strength and Threshold controls where needed.
Mixed selections retain their raw representation when a shared Size/Blend presentation is not available.
Radius is the finite support radius, Strength is the signed field value at the center, and Threshold is a positive boundary level shared by a group.
Negative strength subtracts density; the resulting counter depends on surrounding elements, rather than having a fixed circular size.
Opening a document or selecting a control never rewrites these stored values.

### Connections

Select exactly two centers in one group, then choose **Connect selected**.
A connection contributes a constant-width segment field between those centers and follows their stable identities when they move.
Click its midpoint to select the connection and adjust **Width** in the inspector.
Delete removes a selected connection without deleting its endpoint centers.

Width is the full visible width of the isolated segment field.
Other fields broaden the junctions, and subtractive elements can narrow or interrupt the bridge.
It is a reference width, not a constraint on the final blended contour.
Connections have rounded end caps and no taper controls.
Ordinary nearby centers still blend automatically; a connection makes a deliberate bridge between more distant centers.

### Conversion and preview

Centers and connections in a group blend together.
**Start a new group** prepares an independent group for the next click.
**Groups to cubic** converts complete groups containing selected objects.
**Glyph to cubic** converts every group in the glyph.
Both are also under **Path → Metaballs**, using normal Undo/Redo.

From the overview, **Path → Metaballs → Font to Cubic** converts the current master's foreground glyphs as one undoable batch.
Other masters and background layers remain separate.
The command below converts every layer of one UFO.

Live previews appear in the canvas, preview strip, glyph grid and component references.
Saving keeps centers and connections as glyph metadata.
Ordinary point tools operate on ordinary contours; choose the metaball tool to edit source objects.
Roundness, noise, ellipses, tapering, quadratic and hyperbezier conversion, and interpolation of parameters between masters are future work.
Convert before using external font compilers.

## Whole-font conversion

The command line converts all groups in all layers of one UFO into a separate UFO:

```sh
cargo run -- collapse-metaballs Source.ufo --out Cubic.ufo
```

The destination must not exist.
The input is never written.
`--resolution 2` sets the grid spacing in font units; smaller values sample finer details.
`--accuracy 0.25` sets img2bez's cubic fitting accuracy relative to the sampled boundary.
Conversion across glyphs is prepared before any in-memory installation.
Errors retain the source; a group with no sampled boundary cannot be collapsed.

A disposable font matching the dot-and-stem idea can be generated with:

```sh
nice -n 15 cargo run -j 1 --locked --no-default-features \
  --example metaball_fixture -- /tmp/MetaballStudy.ufo
```

## Implementation

`formats::metaballs` owns the versioned `com.runebender.metaballs` glyph lib key.
Version 1 stores groups with stable IDs, thresholds, and centers with stable IDs, coordinates, support radii and stiffness.
Size and Blend reach only reparameterize those existing values; they do not introduce a new kernel or upgrade the source version.
For positive strength `s > T`, the mapping is:

```text
visible_size = R * sqrt(1 - (T / s)^(1/3))
blend_reach = R - visible_size

R = visible_size + blend_reach
s = T / (1 - (visible_size / R)^2)^3
```

Unrepresentable settings and existing raw values must not be silently clamped or reinterpreted.
Size and Blend reach are not defined for every valid version-1 element.

Adding the first connection upgrades the glyph's metaball metadata to version 2.
Each group can then store `links`, with stable connection IDs, `start` and `end` center IDs, and a full reference `width`.
Endpoint references must resolve to distinct centers in that same group.
Empty link lists are omitted, so version-1 source serialization remains unchanged.
Readers reject unknown fields, unknown versions and malformed references rather than replacing them with empty data.
Older Runebender versions that only understand version 1 cannot edit version-2 metadata.
The data remains ordinary plist values and round-trips through UFO GLIF XML.

`outline::metaballs` evaluates the compact polynomial field:

```text
center contribution = stiffness * max(0, 1 - distance_to_center² / radius²)³
link contribution = 4*T * max(0, 1 - distance_to_segment² / link_support²)³
link_support = (width / 2) / sqrt(1 - (1/4)^(1/3))
F(x, y) = sum(center contributions) + sum(link contributions)
inside = F(x, y) >= T
```

Distance to a segment uses its nearest point, including either endpoint outside the segment's projected span.
Thus its support is a capsule with round end caps.
Coincident endpoint positions reduce geometrically to a round field even though the two endpoint IDs remain distinct.
Bounds, gradients and Hessians must use the same source geometry as field evaluation.
The segment-to-cap join has a continuous first derivative but can change second derivative; fitting must not assume one radial formula throughout.

A triangular grid extracts oriented boundary loops.
Bisection locates field crossings; analytic gradients supply cubic tangents.
The interactive preview fits the whole loop with Kurbo's `simplify::simplify_bezpath`.
A bounds and sampled field check rejects simplifications that introduce spikes or leave the boundary; those previews retain the dense sampled Hermite contour.
This fallback does not invoke img2bez while dragging.

Explicit conversion additionally locates horizontal and vertical extrema and curvature sign changes on the implicit field.
It projects these feature points back onto the boundary and passes the ordered samples, tangents and feature labels to `img2bez::fit_smooth_contours`.
Img2bez owns the constrained cubic fitting, using Kurbo internally to fit each span.
This retains extrema and their tangents while optimizing segment count between them.
Inflections can fall inside a cubic and no longer require an extra node.
Converted contours start at their bottommost on-curve point, with ties resolved to the left.
Extremum handles are exactly horizontal or vertical, and conversion retains fractional coordinates without rounding to a font-unit grid.
A feature that cannot be resolved safely returns an error and leaves the source intact.

[img2bez](https://github.com/eliheuer/img2bez) is the conversion library; Runebender supplies the metaball field information.
The pinned img2bez dependency also offers `trace_sdf`, which can fit a sampled field without a PNG intermediate.
Its optional `cleanup_max_deviation` checks cleanup candidates against the original fitted contour, preserving earlier geometry when a candidate exceeds the sampled limit.
The exact-boundary entry point uses the supplied geometry directly, without image cleanup.
The comparison below exercises that alternative rather than assuming it improves every shape.

Generate a reproducible SVG with nodes, handles, segment counts, and sampled field discrepancy:

```sh
nice -n 15 cargo run -j 1 --locked --no-default-features \
  --example metaball_conversion_proof -- /tmp/metaball-comparison.svg
```

The proof compares the previous whole-loop preview, img2bez's sampled-field tracing with bounded cleanup, and img2bez's exact-boundary fitting on a circle, a blended stem, an unequal diagonal blend, and a counter.
These compare input information and pipeline choices, not img2bez against Kurbo.
The diagonal blend needs more segments with constrained fitting; retaining extrema does not guarantee a globally minimal outline or replace a designer's judgment.
The reported discrepancy is a first-order normal-distance estimate sampled along the fitted curves, not a Hausdorff error bound.

Sampling spacing is not a guaranteed error bound against the analytic field.
Very small holes or bridges, or multiple feature roots within a grid edge, can be missed, especially near a merge or split.
Exact first contact can be a singular boundary rather than a regular smooth outline.
The grid is capped at one million cells per group, including connection bounds; an oversized request returns an error instead of silently coarsening the result.
Interactive previews and conversion use a 2-unit grid by default.
A later adaptive sampler can improve both speed and small-feature fidelity without changing the source format.

## Disposable MVP proofs

Generate seven fixed source fixtures, an SVG comparing preview and converted cubic nodes, and a text summary without touching a real font:

```sh
nice -n 15 cargo run -j 1 --locked --no-default-features \
  --example metaball_mvp_proof -- /tmp/metaball-mvp-proof
```

The output directory must not exist.
It contains `MetaballMvp.ufo`, `metaballs.svg` and `summary.txt`.
Glyphs `a` through `g` cover separated circles, near contact, broad union, an unequal long bridge, a chain, a branch and a counter.
The near-contact fixture is slightly connected, not exactly at the singular contact distance.
The example checks expected contour counts and verifies that the generated UFO retains its exact source metadata after reopening.
The SVG is a monochrome geometry proof; it does not establish native pointer behavior or preview latency.

## References

- [Blender metaball properties](https://docs.blender.org/manual/en/4.4/modeling/metas/properties.html): separate element radius/stiffness and group threshold.
- [Blender metaball evaluation](https://github.com/blender/blender/blob/main/source/blender/blenkernel/intern/mball_tessellate.cc): compact fields for round and tube elements.
- [Houdini metaball controls](https://www.sidefx.com/docs/houdini/nodes/sop/metaball.html): threshold-radius controls that preserve isolated visible size while changing influence.
- [Metaballs](https://en.wikipedia.org/wiki/Metaballs): summed implicit fields and compact support.
- [User-supplied Desmos example](https://www.desmos.com/calculator/j6jheyeh7x): the interactive graph could not be retrieved during this implementation; its exact formula was not assumed.

The user's wireframe guided the tool placement, center controls, preview and explicit conversion workflow.
It was treated as a visual reference.
