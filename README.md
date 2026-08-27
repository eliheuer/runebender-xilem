# runebender-xix

A font editor built on [xix](https://github.com/eliheuer/xix), a fork of
Xilem. This is the third Runebender shell; it shares the editing engine
[runebender-core](https://github.com/eliheuer/runebender-core) with
[runebender-gpui](https://github.com/eliheuer/runebender-gpui) and
[runebender-web](https://github.com/eliheuer/runebender-web).

The port is a test of xix: Runebender is the kind of application xix is
for, and `PORT.md` records what each slice forced into the framework.

## Run it

```sh
cargo run --release -- path/to/Font.ufo
```

A `.designspace` opens its first master. Any UFO works; try one from a
font project you have, or use the small test font in the
runebender-xilem checkout:

```sh
cargo run --release -- ../runebender-xilem/assets/hyper-matisse.ufo
```

Editing writes back to the UFO on save, so **work on a copy** if you
care about the source.

## What works

Overview grid (with glyph name + codepoint labels, mark colors, a
search + category sidebar) and a glyph editor with these tools:

- **Select** — click a point, shift-click to add, drag to move, drag
  empty space for a marquee. Middle/right-drag pans; the wheel zooms.
- **Pen** — click for corner points, click-drag for smooth points with
  bezier handles, click the first point to close.
- **HyperPen** — click to place hyperbezier points (the curve is solved
  automatically, no handles); alt-click for a corner; click the first
  point to close.
- **Rect / Ellipse** — drag to draw.
- **Knife** — drag a line to cut contours.
- **Measure** — shows segment/stem lengths and side bearings.

### Keyboard (while the editor canvas has focus)

| Key | Action |
| --- | --- |
| Arrows | nudge selection (Shift = ×10) |
| Delete / Backspace | delete selected points |
| Cmd/Ctrl+A | select all |
| Cmd/Ctrl+Z, Cmd/Ctrl+Shift+Z | undo, redo |
| Cmd/Ctrl+S | save |
| Escape | cancel the pen, or return to the overview |
| h / v | flip horizontal / vertical |
| r | rotate 90° |
| ] | reverse contour direction |
| o | remove overlap (union) |
| d | decompose components |

The single-letter operation keys (h/v/r/]/o/d) are temporary. They exist
because xix does not yet have a window-level menu/shortcut layer, so
these live on the editor's own key handler for now. See `PORT.md`.

Click a glyph in the overview to select it, click again to open the
editor. The toolbar's `‹ Overview` returns.

## Develop against local xix / core

Nothing in this repository points at a local path. To build against
sibling checkouts of xix and runebender-core, put a
`.cargo/config.toml` in the directory **above** the repositories:

```toml
paths = ["runebender-core"]

[patch."https://github.com/eliheuer/xix"]
xilem = { path = "xix/xilem" }
masonry = { path = "xix/masonry" }
```

## License

Apache-2.0 OR MIT, the Linebender convention.
