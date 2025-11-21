# Hyperbezier UFO Extension Specification

## Overview

This document defines an extension to the [Unified Font Object (UFO)](https://unifiedfontobject.org/) specification to support hyperbezier paths. Hyperbezier paths are smooth curves defined by only their on-curve points, with off-curve control points automatically computed to maintain G2 continuity.

## Motivation

### Why Hyperbeziers?

Hyperbeziers simplify curve drawing by:
1. **Reducing complexity**: Only on-curve points need to be specified
2. **Automatic smoothness**: The spline solver ensures G2 continuity between segments
3. **LLM-friendly**: AI models can generate curves by specifying only integer coordinates of on-curve points, without needing to understand bezier control point mathematics

### Design Goals

This extension is designed to be:
- **Simple**: Only on-curve point coordinates are stored
- **LLM-optimized**: Integer coordinates, minimal metadata, easy to generate from natural language descriptions
- **Backward compatible**: Falls back gracefully to cubic approximation in non-supporting editors
- **Round-trip safe**: Preserves hyperbezier data through save/load cycles

## Specification

### Contour Point Representation

Hyperbezier contours are stored in the standard UFO `<contour>` element within a glyph's `.glif` file with an `identifier="hyperbezier"` attribute. Each on-curve point is represented as a `<point>` element with `type="curve"`.

#### Detection

Hyperbezier contours are marked with the `identifier="hyperbezier"` attribute:

```xml
<contour identifier="hyperbezier">
    <point x="100" y="0" type="curve"/>
    <!-- ... -->
</contour>
```

**Primary detection:** Check for `identifier` attribute containing "hyper"
**Fallback detection:** Contours with all on-curve points (no off-curve control points)

This dual approach ensures:
- Explicit marking prevents ambiguity
- Fallback heuristic supports files without the identifier
- The spline solver automatically computes control points from on-curve points

#### Point Attributes

Each hyperbezier point has these attributes:

- `x` (required): X coordinate as an integer
- `y` (required): Y coordinate as an integer
- `type` (required): `"curve"` for smooth points, `"line"` for corner points
- `smooth` (automatic): Set to `"yes"` automatically for smooth hyperbezier points when saved
- `name` (optional): Point identifier for reference

### Example

```xml
<glyph name="a" format="2">
  <advance width="600"/>
  <unicode hex="0061"/>
  <outline>
    <contour identifier="hyperbezier">
      <!-- Smooth hyperbezier contour - integer coordinates, type="curve" -->
      <point x="100" y="0" type="curve"/>
      <point x="500" y="0" type="curve"/>
      <point x="500" y="500" type="curve"/>
      <point x="100" y="500" type="curve"/>
    </contour>
    <contour identifier="hyperbezier">
      <!-- Mixed smooth and corner points -->
      <point x="200" y="200" type="curve"/>
      <point x="400" y="200" type="line"/>  <!-- corner point -->
      <point x="400" y="400" type="curve"/>
      <point x="200" y="400" type="line"/>  <!-- corner point -->
    </contour>
  </outline>
</glyph>
```

**Comparison with Regular Cubic Bezier:**

```xml
<!-- Regular cubic bezier - has off-curve control points, NO identifier -->
<contour>
  <point x="100" y="0" type="line"/>
  <point x="300" y="50"/>              <!-- off-curve control point -->
  <point x="400" y="100"/>             <!-- off-curve control point -->
  <point x="500" y="0" type="curve"/>  <!-- on-curve endpoint -->
</contour>

<!-- Hyperbezier - identifier="hyperbezier", only on-curve points -->
<contour identifier="hyperbezier">
  <point x="100" y="0" type="curve"/>
  <point x="500" y="0" type="curve"/>
  <!-- Spline solver computes the two control points automatically -->
</contour>
```

### Library Metadata (Optional)

While not required for detection (see Detection section above), fonts MAY include this metadata in the font-level `lib.plist` to explicitly declare hyperbezier support:

```xml
<lib>
  <dict>
    <key>com.github.linebender.runebender.hyperbezier</key>
    <true/>
  </dict>
</lib>
```

This is purely informational and not used for detection. Hyperbeziers are automatically detected by the absence of off-curve control points.

### Contour Closure

Hyperbezier contours follow standard UFO rules:
- If the first and last points have the same coordinates, the contour is open
- Otherwise, the contour is implicitly closed with a segment from the last point back to the first

### Differences from Standard UFO

#### Standard UFO Cubic Bezier
```xml
<contour>
  <point x="100" y="200" type="line"/>
  <point x="200" y="300"/>           <!-- off-curve -->
  <point x="300" y="300"/>           <!-- off-curve -->
  <point x="400" y="200" type="curve"/>
</contour>
```

#### Hyperbezier
```xml
<contour>
  <point x="100" y="200" type="hyper"/>
  <point x="400" y="200" type="hyper"/>
  <!-- Off-curve points computed automatically -->
</contour>
```

## Fallback Strategy for Non-Supporting Editors

When a UFO with hyperbezier paths is opened in an editor that doesn't support this extension:

1. **Reading**: The editor will likely ignore the `type="hyper"` attribute and treat points as on-curve cubic bezier points, connecting them with straight lines
2. **Saving**: The editor may discard the hyperbezier metadata and convert to standard cubic beziers

To preserve hyperbezier data when round-tripping through non-supporting editors:
- Applications SHOULD create a backup before opening in unknown editors
- Consider exporting a parallel "flattened" cubic version for maximum compatibility

## Implementation Notes

### Computing Off-Curve Points

When rendering or converting hyperbezier paths:

1. Parse on-curve points from the UFO
2. Use a spline solver (e.g., the `spline` crate) to compute off-curve control points
3. The solver ensures:
   - G2 continuity at smooth points
   - Independent segments at corner points
4. Generate cubic bezier segments for rendering

### Integer Coordinates

All coordinates are stored as integers, matching standard UFO practice. This:
- Simplifies LLM generation (no floating point)
- Maintains compatibility with font tooling
- Ensures deterministic round-tripping

### LLM Generation Example

An LLM can generate a hyperbezier oval with a simple prompt:

**Prompt**: "Create a smooth oval hyperbezier path centered at (300, 400) with width 200 and height 150"

**Output**:
```xml
<contour>
  <point x="200" y="400" type="hyper"/>
  <point x="300" y="475" type="hyper"/>
  <point x="400" y="400" type="hyper"/>
  <point x="300" y="325" type="hyper"/>
</contour>
```

The LLM only needs to:
1. Understand basic geometry (center, width, height)
2. Calculate 4 integer coordinate pairs
3. Mark them as `type="hyper"`

No bezier mathematics required!

## Version History

### Version 1.0 (2025)
- Initial specification
- Support for `hyper` and `hyper corner` point types
- Integer coordinate storage only
- LLM-optimized design

## References

- [UFO Specification](https://unifiedfontobject.org/)
- [Hyperbezier Blog Post](https://www.cmyr.net/blog/hyperbezier.html)
- [Runebender Font Editor](https://github.com/linebender/runebender)
- [Spline Crate](https://github.com/raphlinus/spline)

## License

This specification is released under CC0 1.0 Universal (Public Domain).
