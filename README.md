# runebender-core

The editing engine behind the [Runebender][rb] font editor, with no
interface attached.

Every operation that changes a font lives here: point edits, segment
surgery, overlap removal, components and anchors, kerning with group
fallback, curvature analysis, shaping, measurement, interpolation.
The front-ends own their window, input and drawing, and call this
crate for the rest.

One rule decides what belongs: **if an edit changes a font, it goes
here.** The test is not whether the code is about drawing. It is
whether another front-end would need it. Where a click lands is the
shell's business. What that click does to the outline is this crate's.

Two things follow. The hard parts are testable without opening a
window, so they have tests that run in milliseconds. And a new
front-end is a new interface rather than a second editor.

## The command line

```sh
cargo install --git https://github.com/eliheuer/runebender-core

runebender info Font.ufo
runebender measure Font.ufo --glyph eight
runebender check --a Light.ufo --b Bold.ufo
```

Every command takes `--json`. Exit codes are 0 ok, 1 findings, 2 usage,
4 failed, matching [font-ml][fml] so the two are driven the same way.
An operation you can only reach by opening a window is one a script, a
build, or an agent cannot use.

## Layout

Modules are named for what they do to a font. `src/lib.rs` carries
the map; the long form, with the conventions and the CI gate, is at
[runebender.org/docs/code-layout.html][layout].

```
src/
├── glyph_ops.rs      point edits, segment surgery, pen primitives,
│                     decompose, overlap, metrics, kerning
├── cleanup.rs        tidy, direction, rounding, extremes, handle fitting
├── effects.rs        offset, extrude, roughen, expand stroke, bolden
├── convert.rs        quadratic ↔ cubic
├── knife.rs          slicing            shape.rs      primitives
├── measure.rs        stems, sidebearings, joining bands
├── optical.rs        ink density        spacing.rs    grid checks
├── curve.rs          curvature: continuity, kinks, extrema
├── lib_keys.rs       every com.runebender.* key, one reader and writer each
├── metrics_keys.rs   color_font.rs  mark_color.rs   shared keys
├── svg.rs            binary_import.rs  glyphs_import.rs  image_trace.rs
├── project.rs        the document: Master and Project
├── var_model.rs      interpolation      composites.rs components, anchors
├── shaping.rs        text.rs            shaping and the text buffer
├── glyph_paths.rs    norad → kurbo      path/         segment maths
├── editing/          selection, undo, viewport
├── theme*.rs         the OKLCH token file every editor resolves from
├── test_fonts.rs     where the tests find Virtua Grotesk
└── bin/runebender.rs the command line
```

## Checks

CI runs `cargo fmt --check`, `cargo clippy --all-targets` and
`cargo doc` with warnings denied, and the tests on Linux and macOS
and at the minimum Rust. `unsafe` is forbidden in the manifest.

The tests load Virtua Grotesk from [runebender-web][web], cloned
beside this repository or named by `RUNEBENDER_TEST_FONTS`.

## The format is the model

Sources are edited as UFO through [norad][norad], rather than read into
a private model and written back. Nothing is lost in translation, and
another tool can read the sources mid-session. The cost is that the
file's shape is the editor's shape, awkward parts of UFO included.

This is also why a script or an agent can work alongside a running
editor: the editor reloads what changed on disk.

## Two versions of kurbo

This crate carries `kurbo` twice, as `kurbo_09` and through its
consumers. Front-ends are pinned to their toolkit's version, and a library can
only declare one. Rather than hold every front-end to the oldest, the
geometry types are pinned here and converted at the boundary. Every
consumer is on kurbo 0.13 now, so the only conversion left is around
the hyperbezier solver.

## Front-ends

| | |
| --- | --- |
| [runebender-gpui][gpui] | The current editor. Native and browser from one codebase. |
| [runebender-xilem][xilem] | The same editor on Xilem, the Linebender stack. More experimental. |
| [runebender-web][web] | Vello and Kurbo in WebAssembly, with a Vue interface. |
| [runebender-druid][druid] | The original, kept as project history. |

## License

Apache-2.0.

[rb]: https://runebender.org
[gpui]: https://github.com/eliheuer/runebender-gpui
[xilem]: https://github.com/eliheuer/runebender-xilem
[web]: https://github.com/eliheuer/runebender-web
[druid]: https://github.com/linebender/runebender
[norad]: https://github.com/linebender/norad
[fml]: https://github.com/eliheuer/font-ml
[layout]: https://runebender.org/docs/code-layout.html
