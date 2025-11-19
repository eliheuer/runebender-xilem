# Kurbo Version Mismatch: Spline Crate Issue

## Overview

The `spline` crate from linebender uses kurbo 0.9.5, while runebender-xilem uses kurbo 0.12. This creates a type incompatibility that requires a workaround.

## The Problem

### Version Dependencies

```
runebender-xilem
├── kurbo = "0.12"
└── spline (git)
    └── kurbo = "0.9.5"
```

### Type Incompatibility

When you try to pass a `kurbo::Point` (0.12) to `SplineSpec::move_to()`, Rust sees them as completely different types:

```rust
// This fails:
let point = kurbo::Point::new(100.0, 200.0);  // kurbo 0.12
spec.move_to(point);  // expects kurbo 0.9.5 Point
```

Error:
```
error[E0308]: mismatched types
  --> src/hyper_path.rs:161:22
   |
   |         spec.move_to(points_vec[0].point);
   |              ------- ^^^^^^^^^^^^^^^^^^^ expected `kurbo::point::Point`, found `kurbo::Point`
```

Even though both `Point` types have the same internal structure (`x: f64, y: f64`), Rust treats them as incompatible because they come from different crate versions.

## Current Workaround

### 1. Add Aliased Dependency

In `Cargo.toml`:
```toml
kurbo = "0.12"
kurbo_09 = { package = "kurbo", version = "0.9.5" }
```

### 2. Convert Between Versions

In `src/hyper_path.rs`:
```rust
// Convert kurbo 0.12 Point to kurbo 0.9 Point for spline input
let to_spline_point = |p: Point| -> kurbo_09::Point {
    kurbo_09::Point::new(p.x, p.y)
};

spec.move_to(to_spline_point(my_point));

// Convert spline output back to kurbo 0.12
for el in spline_bezpath.elements() {
    match el {
        kurbo_09::PathEl::MoveTo(p) => {
            result.move_to(Point::new(p.x, p.y));
        }
        kurbo_09::PathEl::CurveTo(p1, p2, p3) => {
            result.curve_to(
                Point::new(p1.x, p1.y),
                Point::new(p2.x, p2.y),
                Point::new(p3.x, p3.y),
            );
        }
        // ... etc
    }
}
```

## Upstream Fix Options

### Option 1: Update Spline Crate to kurbo 0.12 (Recommended)

The cleanest solution is to update the spline crate's kurbo dependency.

**Repository**: https://github.com/linebender/spline

**Required Changes**:
1. Update `Cargo.toml`:
   ```toml
   kurbo = "0.12"  # was "0.9.5"
   ```
2. Fix any breaking API changes between kurbo 0.9.5 and 0.12
3. Run tests to ensure spline functionality still works

**Potential Breaking Changes** (kurbo 0.9 → 0.12):
- Check the kurbo CHANGELOG for breaking changes
- The `BezPath` and `Point` APIs are likely stable
- May need to update method calls if APIs changed

**PR Template**:
```markdown
## Summary
Update kurbo dependency from 0.9.5 to 0.12 for compatibility with downstream projects using newer linebender ecosystem crates.

## Motivation
Projects using both `spline` and other linebender crates (masonry, xilem, etc.) that depend on kurbo 0.12 currently need workarounds to convert between kurbo versions.

## Changes
- Updated kurbo dependency to 0.12
- [List any API adjustments needed]

## Testing
- [ ] All existing tests pass
- [ ] Spline solving produces correct results
- [ ] BezPath rendering is unchanged
```

### Option 2: Make Spline Generic Over kurbo Version

A more flexible but complex approach would be to make the spline crate generic over the geometry types.

```rust
pub trait SplinePoint {
    fn new(x: f64, y: f64) -> Self;
    fn x(&self) -> f64;
    fn y(&self) -> f64;
}

impl SplinePoint for kurbo::Point { /* ... */ }
```

This is likely overkill for this situation.

### Option 3: Re-export kurbo from Spline

The spline crate could re-export its kurbo dependency:

```rust
// In spline/src/lib.rs
pub use kurbo;
```

Then users would use `spline::kurbo::Point` consistently. However, this still requires conversion if you're using kurbo elsewhere.

## Recommended Action

**File a PR to update spline to kurbo 0.12**.

Steps:
1. Fork https://github.com/linebender/spline
2. Update Cargo.toml kurbo version
3. Fix any compilation errors
4. Run `cargo test`
5. Open PR with the template above

Once merged, we can:
1. Remove `kurbo_09` from our Cargo.toml
2. Remove all the Point conversion code in `hyper_path.rs`
3. Use kurbo types directly

## Files Affected by This Workaround

- `Cargo.toml` - Contains the aliased dependency
- `src/hyper_path.rs` - Contains conversion code in `rebuild_bezier()`

## References

- Spline crate: https://github.com/linebender/spline
- kurbo crate: https://github.com/linebender/kurbo
- kurbo CHANGELOG: https://github.com/linebender/kurbo/blob/main/CHANGELOG.md
- Rust RFC on semver: https://doc.rust-lang.org/cargo/reference/semver.html
