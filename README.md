# Runebender Xilem

A font editor built with [Xilem](https://github.com/linebender/xilem), a Rust UI framework from the [Linebender](https://linebender.org/) ecosystem. This is a port of an earlier font editor called [Runebender](https://github.com/linebender/runebender) from [Druid](https://github.com/linebender/druid) to [Xilem](https://github.com/linebender/xilem).

**Status**: Very alpha and actively under development.

<img width="960" height="663" alt="Image" src="https://github.com/user-attachments/assets/458e37a8-5cb2-4ace-91e1-83e0adfc7cd1" />

<img width="1512" height="982" alt="Image" src="https://github.com/user-attachments/assets/6b27b22c-2124-4ca7-8330-6f2d0ccb7d50" />

<img width="1142" height="940" alt="Image" src="https://github.com/user-attachments/assets/eb8493f3-26dd-40bf-9e13-f37fdc02ba32" />

## Building from source

Make sure [Rust](https://rust-lang.org/) is installed on your system (MSRV: 1.88), clone the repository and build/run the application:

```bash
git clone https://github.com/eliheuer/runebender-xilem.git
cd runebender-xilem
cargo run
```

## Usage

```bash
cargo run                            # Opens a file picker
cargo run -- assets/untitled.ufo     # Open a specific UFO file
cargo run -- --verbose               # Run with verbose logging
```

## Keyboard Shortcuts

### General

| Shortcut | Action |
|----------|--------|
| `Cmd/Ctrl` + `S` | Save |
| `Cmd/Ctrl` + `Z` | Undo |
| `Cmd/Ctrl` + `Shift` + `Z` | Redo |
| `Cmd/Ctrl` + `+` or `=` | Zoom in |
| `Cmd/Ctrl` + `-` | Zoom out |
| `Cmd/Ctrl` + `0` | Fit glyph to window |
| `Tab` | Toggle side panels |
| `Space` (hold) | Temporary preview mode |

### Editing

| Shortcut | Action |
|----------|--------|
| `Backspace` / `Delete` | Delete selected points or background image |
| Arrow keys | Nudge selection (1 unit) |
| `Shift` + Arrow keys | Nudge selection (10 units) |
| `Cmd/Ctrl` + Arrow keys | Nudge selection (100 units) |
| `T` | Toggle point type (smooth/corner) |
| `R` | Reverse contour direction |
| `Cmd/Ctrl` + `C` | Copy selected contours |
| `Cmd/Ctrl` + `V` | Paste contours |
| `Cmd/Ctrl` + `Shift` + `H` | Convert hyperbezier paths to cubic |

### Tools

| Shortcut | Action |
|----------|--------|
| `V` | Select tool |
| `P` | Pen tool |
| `H` | Hyperbezier pen tool |
| `K` | Knife tool |

### Background Image & Autotrace

| Shortcut | Action |
|----------|--------|
| `Cmd/Ctrl` + `I` | Import background image (file dialog) |
| `Cmd/Ctrl` + `T` | Autotrace background image into bezier paths |
| `Cmd/Ctrl` + `Shift` + `T` | Refit existing outlines to match background image |
| `Cmd/Ctrl` + `L` | Toggle background image lock |

Import a reference image (PNG/JPEG) with `Cmd+I`, position and scale it behind your glyph using the drag handles, then press `Cmd+T` to trace it into editable cubic bezier contours using [img2bez](https://github.com/eliheuer/img2bez). Use `Cmd+Shift+T` to refit existing outlines onto the background image — this warps the current outlines to match the target shape while preserving point count, types, and winding direction for variable font interpolation compatibility. The background image is kept after tracing so you can compare the result.

## Features

### Hyperbezier Path Support

Runebender Xilem supports on-curve hyperbezier paths - smooth curves defined by only their on-curve points, with control points automatically computed by a spline solver. This makes drawing visually smooth curves easier.

See [docs/hyperbezier-ufo-extension.md](docs/hyperbezier-ufo-extension.md) for the complete specification. Try using the Hyperbezier tool from the edit mode toolbar or load the example file `hyper-matisse.ufo` from the assets directory.

### Background Image Tracing

Import bitmap images (scanned sketches, reference drawings) as background layers in the glyph editor. Position and scale the image to match your glyph metrics, then autotrace it into editable bezier outlines. Tracing is powered by [img2bez](https://github.com/eliheuer/img2bez). Tracing parameters (corner detection threshold, grid snapping) can be adjusted in `src/settings.rs`.

## Contributing

Contributions are welcome! Make a PR or issue, but keep in mind this project is very early and things can change quickly. If anyone besides Eli becomes a regular contributor to this we can move it off my personal Github to a new org.

## License

Apache-2.0
