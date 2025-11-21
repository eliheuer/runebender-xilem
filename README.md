# Runebender Xilem

A font editor built with [Xilem](https://github.com/linebender/xilem), a Rust UI framework from the [Linebender](https://linebender.org/) ecosystem. This is a port of an earlier font editor called [Runebender](https://github.com/linebender/runebender) from [Druid](https://github.com/linebender/druid) to [Xilem](https://github.com/linebender/xilem).

**Status**: Very alpha and actively under development.

<img width="960" height="663" alt="Image" src="https://github.com/user-attachments/assets/458e37a8-5cb2-4ace-91e1-83e0adfc7cd1" />

<img width="1512" height="982" alt="Image" src="https://github.com/user-attachments/assets/6b27b22c-2124-4ca7-8330-6f2d0ccb7d50" />

## Building from source

Make sure [Rust](https://rust-lang.org/) is installed on your system, clone the repository and build/run the application:

```bash
git clone https://github.com/eliheuer/runebender-xilem.git
cd runebender-xilem
cargo run
```

## Usage

**Open a specific UFO file directly:**

```bash
cargo run -- assets/untitled.ufo
```

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd/Ctrl` + `S` | Save |
| `Cmd/Ctrl` + `+` or `=` | Zoom in |
| `Cmd/Ctrl` + `-` | Zoom out |
| `Cmd/Ctrl` + `0` | Fit glyph to window |
| `Cmd/Ctrl` + `Z` | Undo |
| `Cmd/Ctrl` + `Shift` + `Z` | Redo |
| `Cmd/Ctrl` + `Shift` + `H` | Convert selected hyperbezier paths to cubic |
| `Space` (hold) | Temporary preview mode |
| `Backspace` / `Delete` | Delete selected points |
| `T` | Toggle point type (corner/smooth) |
| `R` | Reverse contour direction |
| Arrow keys | Nudge selection (1 unit) |
| `Shift` + Arrow keys | Nudge selection (10 units) |
| `Cmd/Ctrl` + Arrow keys | Nudge selection (100 units) |

## Features

### Hyperbezier Path Support

Runebender Xilem supports **hyperbezier paths** - smooth curves defined by only their on-curve points, with control points automatically computed by a spline solver. This makes drawing smooth curves easier and more intuitive.

- **Create**: Use the HyperPen tool to draw hyperbezier paths
- **Edit**: Move on-curve points, curves update automatically
- **Convert**: Select points and press `Cmd/Ctrl + Shift + H` to convert to cubic bezier for manual control
- **Persist**: Hyperbezier paths are saved in UFO format with `identifier="hyperbezier"`

See [docs/hyperbezier-ufo-extension.md](docs/hyperbezier-ufo-extension.md) for the complete specification.

## Contributing

Contributions are welcome! Make a PR or issue, but keep in mind this project is very early and things can change quickly. If anyone besides Eli becomes a regular contributor to this we can move it off my personal Github to a new org.

## License

Apache-2.0
