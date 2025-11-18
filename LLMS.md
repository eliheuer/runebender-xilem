# LLMS.md

This file provides guidance to AI assistants (LLMs) when working with code in
this repository.

## Project Overview

Runebender Xilem is a font editor built with Xilem, a Rust UI framework from
the Linebender ecosystem. This is a port of Runebender from Druid
to Xilem.

## Build and Run Commands

```bash
# Build the project
cargo build

# Run the application (opens file picker)
cargo run

# Open a specific UFO file
cargo run path/to/font.ufo

# Build for release
cargo build --release

# Run release binary with a file
./target/release/runebender path/to/font.ufo

# Check for compilation errors without building
cargo check

# Run with verbose output
cargo run -- --verbose
```

## Architecture Overview

### Core Design Pattern: Xilem View Layer

Runebender Xilem follows Xilem's reactive architecture where the UI is rebuilt
from app state on each update. The app uses a single-direction data flow:

```
AppState → app_logic() → View Tree → Masonry Widgets → Vello Rendering
```

### Key Modules

- **src/main.rs**: Entry point, minimal - just calls `runebender::run()`
- **src/lib.rs**: Application logic and view construction
  - `app_logic()`: Root view builder that decides between welcome screen and
    main editor
  - View composition using `flex_col`, `flex_row`, `button`, `label`, `portal`
    from Xilem
  - Glyph grid rendered as rows of cells (9 columns per row)

- **src/data.rs**: AppState struct and state management
  - `AppState`: Holds workspace, error messages, selected glyph
  - File dialog integration via `rfd` crate
  - Glyph selection and metadata access methods

- **src/workspace.rs**: Font data model and UFO loading
  - `Workspace`: Represents a loaded UFO font with all glyphs and metrics
  - Internal `Glyph`, `Contour`, `ContourPoint` types (thread-safe, owned data)
  - Converts from `norad` (UFO library) types to internal representation
  - Glyphs sorted by Unicode codepoint in `glyph_names()`

- **src/glyph_renderer.rs**: Glyph outline conversion
  - Converts workspace `Glyph` to `kurbo::BezPath`
  - Handles UFO point types: Move, Line, OffCurve, Curve, QCurve
  - Complex curve reconstruction logic for cubic/quadratic beziers
  - Provides `glyph_bounds()` for bounding box calculations

- **src/glyph_widget.rs**: Custom Masonry widget for glyph rendering
  - `GlyphWidget`: Masonry widget that paints glyphs using Vello
  - Uniform scaling based on UPM (units per em) for consistent glyph sizes
  - Y-axis flipping transform (UFO coords are Y-up, screen is Y-down)
  - `GlyphView`: Xilem View wrapper implementing View trait
  - `glyph_view()`: View constructor function used in UI code

### Glyph Rendering Pipeline

1. `glyph_grid_view()` iterates workspace glyphs
2. For each glyph: `glyph_renderer::glyph_to_bezpath()` converts contours to
   `BezPath`
3. `glyph_cell()` creates a button containing `glyph_view(path, ...)`
4. `GlyphWidget::paint()` applies transform (scale + Y-flip + centering)
5. Vello renders transformed path via `fill_color()`

## Important Patterns

### Xilem View Composition
- Views are immutable descriptions of UI, rebuilt on each update
- Use `Either::A`/`Either::B` for conditional views (e.g., welcome vs editor)
- State mutation happens in button callbacks: `button(label, |state: &mut AppState| { ... })`

### Thread-Safety Requirements
- Xilem views must be `Send + Sync` to work with `portal()` (scrolling)
- Internal glyph data (`Workspace`, `Glyph`, etc.) is cloneable and owned (no
  references)
- Pre-compute data before view construction to avoid capturing mutable state
  references

### Coordinate System
- UFO coordinates: Y-axis increases upward, origin at baseline
- Screen coordinates: Y-axis increases downward, origin at top-left
- Transformation in `GlyphWidget::paint()` handles Y-flip and baseline
  positioning

### UPM Scaling
- `units_per_em` (UPM) is the font's design grid size (typically 1000, 1024, or 2048)
- All glyphs scaled uniformly by `widget_height / upm` for consistent visual
  size
- Prevents large glyphs from dominating and tiny glyphs from disappearing

## Code Style Guidelines

These guidelines ensure consistent, readable code throughout the project. Follow
these patterns when writing or refactoring code.

### Line Width
- **Line width: 80-100 characters**
- Try for 80, but 100 is fine if it helps readablity
- Break long lines across multiple lines
- Use proper indentation for continuation lines
- Example: Break long function calls, error messages, and doc comments
- optimize for readability and aesthetics, make the code look nice.

### Section Organization
- Use section separators to organize code into logical groups:
  ```rust
  // ============================================================================
  // SECTION NAME
  // ============================================================================
  ```
- Common sections: `CONSTANTS`, `DATA STRUCTURES`, `TYPES`, `IMPLEMENTATIONS`,
  `TESTS`
- Place section separators before major code blocks (structs, impl blocks, test
  modules)

### Reducing Nesting and Rightward Drift
- **Extract helper functions** instead of deeply nested closures or match
  statements
- **Use early returns** with `?` operator to reduce nesting
- **Break complex operations** into smaller, focused functions
- Example: Instead of nested `.map()` chains, extract conversion functions:
  ```rust
  // Good: Extract helper functions
  fn convert_glyph(norad_glyph: &NoradGlyph) -> Glyph {
      let contours = norad_glyph.contours.iter()
          .map(Self::convert_contour)
          .collect();
      // ...
  }
  
  fn convert_contour(norad_contour: &norad::Contour) -> Contour {
      // ...
  }
  ```

### Documentation Comments
- Break long doc comments across multiple lines to stay under 80 characters
- Use clear, descriptive language
- Include examples for complex functions when helpful
- Format doc comments with proper line breaks:
  ```rust
  /// Undo to the previous state
  ///
  /// Returns the previous state if available, moving the current state
  /// onto the redo stack. The caller is responsible for applying this
  /// state.
  ```

### Function Organization
- Group related functions together
- Place public functions before private helpers
- Use consistent spacing between function groups
- Order functions logically (constructors, main operations, helpers, queries)

### Visual Style
- Use consistent indentation (4 spaces)
- Add blank lines between logical sections within functions
- Keep related code together (e.g., struct definition and its impl block)
- Use meaningful full word (not abriviations) variable names that reduce need
  for comments

### Reference Examples
- See `src/theme.rs`, `src/settings.rs`, `src/undo.rs`, and `src/workspace.rs`
  for examples of this style
- These files demonstrate proper section organization, line width limits, and
  reduced nesting

## Development Notes

### Adding New UI Features
- Modify `AppState` in data.rs for new state
- Add methods to `AppState` for state mutations
- Update `app_logic()` or view functions in lib.rs
- Button callbacks can mutate state directly

### Working with Glyphs
- Access glyphs via `workspace.get_glyph(name)` → returns `Option<&Glyph>`
- Glyph names come from `workspace.glyph_names()` (sorted by Unicode)
- Use `glyph_renderer::glyph_to_bezpath()` to get drawable paths
- UFO point types require special handling (see glyph_renderer.rs)

### Font Saving
- Currently unimplemented (`Workspace::save()` returns error)
- Would require converting internal types back to `norad` format

### Custom Widget Reactivity in Multi-Window Apps
When creating custom Masonry widgets that emit actions to update AppState in a
multi-window Xilem application, use `MessageResult::Action(())` instead of
`MessageResult::RequestRebuild`:

```rust
fn message(
    &self,
    _view_state: &mut Self::ViewState,
    message: &mut MessageContext,
    _element: Mut<'_, Self::Element>,
    app_state: &mut State,
) -> MessageResult<()> {
    match message.take_message::<SessionUpdate>() {
        Some(update) => {
            // Update AppState via callback
            (self.on_session_update)(app_state, update.session);

            // Return Action(()) to propagate to root and trigger full app rebuild
            // MessageResult::RequestRebuild doesn't work for child windows in multi-window apps
            MessageResult::Action(())
        }
        None => MessageResult::Stale,
    }
}
```

**Why this is necessary:**
- In multi-window apps, `MessageResult::RequestRebuild` only rebuilds the
  current window
- It doesn't trigger `app_logic()` to be called, so other windows won't see
  state updates
- `MessageResult::Action(())` propagates the action to the root, triggering a
  full app rebuild
- This causes `app_logic()` to run, recreating all windows with fresh state
  from AppState

**Data flow pattern:**
1. Custom widget emits action: `ctx.submit_action::<SessionUpdate>(SessionUpdate { session })`
2. View's `message()` method handles action and updates AppState
3. Return `MessageResult::Action(())` to trigger app rebuild
4. Xilem calls `app_logic()` which reads fresh state from AppState
5. All windows recreated with updated data, UI reflects changes

This pattern is essential for reactive UI in multi-window Xilem apps where
state changes in one window need to be reflected in others.

### Testing UFO Files
- Project requires valid UFO v3 directory structure
- Use existing font editor UFOs or UFO test files
- Path must exist and be a valid UFO directory
- **Test file included in repo**: `assets/untitled.ufo`
  - Run with: `cargo run -- assets/untitled.ufo`
