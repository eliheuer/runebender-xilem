# Refit Feature (Cmd+Shift+T)

## Purpose

Refit existing outlines onto a raster background image for **variable font workflows**. The user duplicates a UFO, creates a designspace, imports a reference image of the target weight (e.g. bold), and uses refit to adjust the existing outlines to match. The result must be **interpolation-compatible** with the original — same point count, types, winding direction.

## Keyboard Shortcuts

- **Cmd+T**: Fresh trace — runs img2bez on the background image, replaces outlines
- **Cmd+Shift+T**: Refit — warps existing outlines to match the background image shape

Dispatch is in `src/components/editor_canvas/keyboard.rs` → `handle_trace_image(shift: bool)`.

## Key Files

- `src/editing/tracing.rs` — All refit logic lives here
  - `trace_background_image()` — Fresh trace (Cmd+T), works correctly
  - `refit_background_image()` — Refit entry point (Cmd+Shift+T)
  - `refit_path_minimal()` — Core algorithm: smooth piecewise warp
  - `PiecewiseWarp` — Monotone cubic Hermite spline for smooth warping
  - `detect_vertical_stems()` / `detect_horizontal_stems()` — Stem edge detection
  - `match_contours()` — Matches existing contours to target by centroid
  - `rebuild_paths_with_new_positions()` — Maps warped positions onto original PathPoint metadata (preserves EntityIds, point types, smooth flags)
  - `extract_points_from_bezpath()` — Extracts points respecting CubicPath's rotate_left(1) closed-path convention
  - `convert_img2bez_bezpath()` — Bridges img2bez's kurbo 0.13 to local kurbo 0.12
  - `bezpath_to_cubic()` — Converts kurbo BezPath to CubicPath with proper point types
- `src/editing/background_image.rs` — BackgroundImage struct, loading, positioning
- `src/components/editor_canvas/keyboard.rs` — Keyboard dispatch
- `src/path/cubic.rs` — CubicPath struct with rotate_left(1) convention for closed paths

## Current Algorithm (Working)

### Smooth Piecewise Warp

1. **Trace the background image** using img2bez (same as Cmd+T) to get a target contour in design space
2. **Match contours** by centroid proximity
3. **Detect vertical stems** in both existing and target contours — finds the two longest near-vertical LineTo segments (the stem edges of letters like "I", "H", etc.)
4. **Build X warp** with 3 zones:
   - Left serif: `[existing_bbox.x0, left_stem]` → `[target_bbox.x0, target_left_stem]`
   - Stem: `[left_stem, right_stem]` → `[target_left_stem, target_right_stem]`
   - Right serif: `[right_stem, existing_bbox.x1]` → `[target_right_stem, target_bbox.x1]`
5. **Detect horizontal stems** — if interior crossbars exist (like in "H"), warp Y similarly. If horizontal lines are only at bbox edges (like in "I"), **keep Y unchanged** to preserve baseline and cap height.
6. **Apply smooth warp** using monotone cubic Hermite interpolation (Fritsch-Carlson method) — no sharp kinks at breakpoints, preserves collinearity of points across zone boundaries

### Why Smooth Warp (Not Piecewise Linear)

A piecewise linear warp has sharp kinks at the stem edge breakpoints. Control points in bracket curves span this boundary, so they get different scale factors from their on-curve neighbors, breaking smooth curves. The cubic Hermite spline transitions gradually.

### Why Keep Y Unchanged for "I"-like Letters

The existing contour's horizontal lines at y=0 and y=720 ARE the bbox edges — there are no interior horizontal stems. The target's bbox has y≈22.8 at the bottom (a tracing artifact from img2bez), but the baseline should stay at y=0. Shifting Y would misalign the outline with the baseline.

## Critical Design Principles

1. **Minimize changes to original point structure** — The user will do manual cleanup. The algorithm should get it 90% there with minimal distortion.
2. **Preserve H/V handle alignment** — If a handle is perfectly horizontal or vertical in the original, it must stay that way for smooth variable font interpolation.
3. **Same point count, types, winding** — Required for interpolation compatibility between masters.
4. **Preserve collinearity** — Points that are collinear in the original should remain collinear after refit.

## Known Limitations / Future Work

### Bracket Curves Don't Fully Conform to Image

The smooth warp preserves the original curve shapes scaled, but the bracket curves (stem-to-serif transitions) don't snap to the actual target image contour. They're smoothly warped but may not match the bold weight's bracket shape exactly. The user needs to manually adjust these.

### Target Contour is Coarse

img2bez produces only ~10 path elements for a serif "I" — very coarse compared to the 20 on-curve points in the existing contour. Previous attempts to project existing points onto this coarse target all failed:
- **Nearest-point**: Stem points jump to closer serif edges
- **Arc-length proportional**: Proportion mismatch between weights shifts all points
- **Radial from centroid**: Concave "I" shape causes rays to hit wrong edges
- **Axis-aligned projection**: Points outside target's straight stem zone project onto bracket curves
- **Walk-based correspondence**: Only 25 polyline points, forward walk jumps to wrong sections

The smooth piecewise warp approach sidesteps this by detecting structural features (stems) directly rather than point-by-point projection.

### Letters Without Vertical Stems

The stem detection requires long vertical LineTo segments. Letters with only curved edges (like "O", "S") won't have detectable stems and will fall back to simple bbox scaling. Consider adding curve-based stem detection (e.g., detecting x-extrema of the contour).

### Letters With Multiple Stems

Currently takes the two longest vertical lines. For letters like "M" or "W" with multiple stems, this might pick the wrong pair. Could be extended to detect all stems and build a multi-zone warp.

### Horizontal Crossbar Detection

Letters like "H", "E", "A" have horizontal crossbars that should warp in Y. The current code detects horizontal stems and warps Y with a 3-zone approach, but this hasn't been tested yet (only "I" has been tested so far).

## kurbo Version Mismatch

runebender uses kurbo 0.12, img2bez uses kurbo 0.13. `convert_img2bez_bezpath()` bridges by extracting raw (x, y) coordinates from img2bez's kurbo types and constructing local kurbo types. Both are re-exported: `img2bez::kurbo` for 0.13, `kurbo` for 0.12.

## Test Data

The primary test case is the serif "I" from Instruments Serif:
- Regular master: `InstrumentsSerif-Regular.ufo` — 20 on-curve points, stem at x=89-160, bbox [19,0]-[230,720]
- Bold target image: `instruments-serif/temp/I.png`
- Bold traced contour: ~10 elements, stem at x≈50-202, bbox [9.6,22.8]-[243.6,718.8]
- Designspace: `InstrumentsSerif.designspace` (2 masters)

## Approaches That Failed (Historical)

Attempts 1-7 all tried to project individual existing points onto the target contour boundary. All produced jumbled/mangled outlines because:
- The target contour is too coarse (only ~25 polyline points)
- The "I" shape is concave, causing geometric projections to hit wrong edges
- Proportion differences between regular and bold weights cause arc-length mismatch

The breakthrough was abandoning point-by-point projection entirely and instead detecting structural features (stems) to build a global coordinate warp.
