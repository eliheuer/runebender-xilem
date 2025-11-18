# Runebender Xilem

A font editor built with [Xilem](https://github.com/linebender/xilem), a Rust UI framework from the [Linebender](https://linebender.org/) ecosystem. This is a port of an earlier font editor called [Runebender](https://github.com/linebender/runebender) from [Druid](https://github.com/linebender/druid) to [Xilem](https://github.com/linebender/xilem).

**Status**: Very alpha and actively under development.

<img width="960" height="663" alt="Image" src="https://github.com/user-attachments/assets/458e37a8-5cb2-4ace-91e1-83e0adfc7cd1" />

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
| `Space` (hold) | Temporary preview mode |
| `Backspace` / `Delete` | Delete selected points |
| `T` | Toggle point type (corner/smooth) |
| `R` | Reverse contour direction |
| Arrow keys | Nudge selection (1 unit) |
| `Shift` + Arrow keys | Nudge selection (10 units) |
| `Cmd/Ctrl` + Arrow keys | Nudge selection (100 units) |

## Contributing

Contributions are welcome! Make a PRs or issue, but keep in mind this project is very early and things can change quickly. If anyone besides Eli become a regular contributor to this we can move it off my personal Github to a new org.

## License

Apache-2.0

