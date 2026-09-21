# Metaballs

Metaballs remain editable source objects until explicit conversion. They are not
UFO anchors, and previewing or saving them does not add ordinary contour points.

## Xilem editor

This implementation belongs to this workspace's Xilem application and internal
Runebender's outline modules. Choose the paired-circle metaball icon beside
Rectangle and Ellipse. Click to place a center, drag to move it, and Shift-click
to select several. Cmd/Ctrl+A selects all centers in the glyph. Arrow keys nudge
selected centers; Shift makes a ten-unit step. Delete removes selected centers.

The right inspector edits X, Y, Radius, Strength and Threshold. Press Enter to
commit a field; each commit is undoable. Blank fields mean no selection or mixed
values. X/Y assign the entered coordinate to every selected center. Radius is
the support radius in font units, not the visible outline radius. Strength is
the field strength at the center; negative values subtract ink. Threshold is a
positive boundary level shared by a group.

Centers in a group blend together. **Start a new group** prepares an independent
group for the next click. **Groups to cubic** converts complete groups
containing selected centers. **Glyph to cubic** converts every group in the glyph.
Both are also under **Path → Metaballs**, using normal Undo/Redo.

From the overview, **Path → Metaballs → Font to Cubic** converts the current
master's foreground glyphs as one undoable batch. Other masters and background
layers remain separate. The command below converts every layer of one UFO.

Live previews appear in the canvas, glyph grid and component references. Saving
keeps the centers as glyph metadata. Ordinary point tools operate on ordinary
contours; choose the metaball tool to edit centers. Roundness, noise, quadratic
and hyperbezier conversion, and interpolation of parameters between masters are
future work. Convert before using external font compilers.

## Whole-font conversion

The command line converts all groups in all layers of one UFO into a separate UFO:

```sh
cargo run -- collapse-metaballs Source.ufo --out Cubic.ufo
```

The destination must not exist. The input is never written. `--resolution 2` sets
the grid spacing in font units; smaller values sample finer details. `--accuracy
0.25` sets Kurbo's curve fitting accuracy relative to the sampled boundary.
Conversion across glyphs is prepared before any in-memory installation. Errors
retain the source; a group with no sampled boundary cannot be collapsed.

A disposable font matching the dot-and-stem idea can be generated with:

```sh
cargo run --example metaball_fixture -- /tmp/MetaballStudy.ufo
cargo run -- /tmp/MetaballStudy.ufo
```

## Implementation

`formats::metaballs` owns the versioned `com.runebender.metaballs` glyph lib key.
It stores groups with stable IDs, thresholds, and centers with stable IDs,
coordinates, radii and stiffness. Invalid or unknown versions return errors;
readers must not replace them with empty data. It is regular plist data and
round-trips through UFO GLIF XML.

`outline::metaballs` evaluates the compact polynomial field:

```text
F(x, y) = sum(stiffness * max(0, 1 - distance² / radius²)³)
inside = F(x, y) >= group.threshold
```

A triangular grid extracts oriented boundary loops.
Bisection locates field crossings; analytic gradients supply cubic tangents.
The interactive preview fits the whole loop with Kurbo's `simplify::simplify_bezpath`.

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
cargo run --no-default-features --example metaball_conversion_proof -- /tmp/metaball-comparison.svg
```

The proof compares the previous whole-loop preview, img2bez's sampled-field tracing with bounded cleanup, and img2bez's exact-boundary fitting on a circle, a blended stem, an unequal diagonal blend, and a counter.
These compare input information and pipeline choices, not img2bez against Kurbo.
The diagonal blend needs more segments with constrained fitting; retaining extrema does not guarantee a globally minimal outline or replace a designer's judgment.
The reported discrepancy is a first-order normal-distance estimate sampled along the fitted curves, not a Hausdorff error bound.

Sampling spacing is not a guaranteed error bound against the analytic field.
Very small holes or bridges, or multiple feature roots within a grid edge, can be missed, especially near a merge or split.
The grid is capped at one million cells per group; an oversized request returns an error instead of silently coarsening the result.
Interactive previews and conversion use a 2-unit grid by default.
A later adaptive sampler can improve both speed and small-feature fidelity without changing the source format.

## References

- [Blender metaball properties](https://docs.blender.org/manual/en/4.4/modeling/metas/properties.html): separate element radius/stiffness and group threshold.
- [Metaballs](https://en.wikipedia.org/wiki/Metaballs): summed implicit fields and compact support.
- [User-supplied Desmos example](https://www.desmos.com/calculator/j6jheyeh7x): the interactive graph could not be retrieved during this implementation; its exact formula was not assumed.

The user's wireframe guided the tool placement, center controls, preview, and
explicit conversion workflow. It was treated as a visual reference.
