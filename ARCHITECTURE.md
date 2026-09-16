# Runebender architecture

Runebender is one Cargo package with one executable. The package has two targets:

- `src/lib.rs` exposes the font engine used by the editor and headless commands.
- `src/main.rs` is the composition root for the Xilem application and command line.

The library target is an internal boundary, not a second product or frontend. It keeps font
behavior testable without constructing a window and lets the Xilem browser build reuse the same
engine.

## Design goals

1. **The filesystem is the first index.** A contributor should be able to guess where a feature
   lives before using search.
2. **Use font-editor language.** Prefer names such as `glyph`, `outline`, `metaballs`, `text`, and
   `workspace` over generic names such as `manager`, `service`, or `utils`.
3. **Separate behavior from presentation.** Font operations do not depend on Xilem. Views show
   state; editor modules interpret user intent; the font engine performs reusable operations.
4. **Keep roots small.** `src/` contains `main.rs`, `lib.rs`, the `app/` boundary, and the font
   engine's domain modules. Application implementation files do not accumulate beside the roots.
5. **Teach through module headers.** Every module root says what belongs there and, when useful,
   points to the next layer involved in the same feature.

## Source map

```text
src/
├── lib.rs                 font-engine public map
├── main.rs                executable composition root
├── analysis/              read and measure font data
├── document/              projects, masters, history, workflows
├── formats/               source formats and persistent metadata
├── outline/               reusable geometry and outline operations
├── text/                  shaping, joining, features, text layout
├── ui/                    toolkit-independent editor data
└── app/                   Xilem application
    ├── mod.rs             application map
    ├── actions.rs         shared menu and shortcut action table
    ├── cli.rs             headless command adapter
    ├── font_model.rs      application-facing project cache
    ├── workspace.rs       open-document and presentation state
    ├── launch.rs          native startup and headless capture
    ├── browser.rs         browser host for the same Xilem view tree
    ├── editor/            user intent and editing interaction
    │   ├── commands.rs
    │   ├── session.rs
    │   ├── inspector.rs
    │   ├── sidebar.rs
    │   └── tools/         named tools and tool-like workflows
    ├── platform/          files, dialogs, watching, live IPC, screenshots
    ├── view/              Xilem views and Masonry canvas widgets
    └── widgets/           reusable UI primitives missing from Xilem/Masonry
```

The desired dependency direction is:

```text
main.rs → app → runebender library
             ↘ Xilem / Masonry / platform adapters

view → workspace + editor
editor → workspace + font engine
workspace → font_model + font engine
font engine -X→ Xilem, Masonry, dialogs, or window state
```

`main.rs` re-exports a few application modules at crate scope so existing call sites can use
readable paths such as `crate::view` and `crate::workspace`. The files still have one physical
home under `app/`.

## Where do I make a change?

| Goal | Start here | Related engine code |
|---|---|---|
| Add or change an editor tool | `app/editor/tools/` | usually `outline/` or `text/` |
| Change Metaballs | `app/editor/tools/metaballs.rs` | `outline/metaballs.rs`, `formats/metaballs.rs` |
| Change text-mode interaction | `app/editor/tools/text.rs` | `text/buffer/`, `text/shape.rs` |
| Change Nodes interaction | `app/editor/tools/nodes.rs` | `document/nodes*.rs`, `ui/nodes.rs` |
| Change selection or undo | `app/editor/session.rs` | `ui/editing/`, `document/history.rs` |
| Add a menu item or shortcut | `app/actions.rs` | `app/editor/commands.rs` |
| Change the edit canvas | `app/view/canvas/editor.rs` | `app/editor/session.rs` |
| Change a panel | `app/view/panels/` | matching editor or document module |
| Change reusable control styling | `app/view/recipes.rs` | `app/view/design.rs`, `theme.rs` |
| Add a file format | `formats/` | dispatch in `document/project.rs` |
| Add a headless command | `app/cli.rs` | operation in the matching library domain |
| Change native or browser hosting | `app/platform/`, `app/launch.rs`, `app/browser.rs` | none |

For a first reading, follow this path:

1. `src/main.rs` — decide between a headless command and the editor.
2. `src/app/mod.rs` — see the application pieces.
3. `src/app/workspace.rs` and `font_model.rs` — understand application state.
4. `src/app/editor/session.rs` — understand one active glyph-editing session.
5. `src/app/view/render.rs` — see how the application modes compose their views.
6. `src/lib.rs` — enter the reusable font domains as needed.

## Adding a tool

1. Put interaction state and pointer/key behavior in `app/editor/tools/<tool>.rs`.
2. Put reusable geometry or font mutations in the matching library domain, most often `outline/`.
3. Put canvas painting in `app/view/canvas/` and controls in `app/view/panels/`.
4. Add its command and shortcut to the shared action table rather than creating a second dispatch
   path.
5. Test the font operation without a window; test interaction with the smallest useful Masonry
   harness.
6. Add the tool to the routing table above when its location would not be obvious to a newcomer.

## File and module size

Line count is a prompt to inspect responsibility, not a target to game. At roughly 500 lines,
check whether a file now contains independently nameable concerns. Above 1,000 lines, prefer a
small module directory with a clear `mod.rs` map when a real boundary exists. Do not split a
cohesive algorithm merely to satisfy a number.

Current large files are architectural debt, not examples to copy. Good future splits include CLI
subcommands by domain, editor-canvas input/paint/layout, and project loading/saving/history. Make
those splits when working in the area so behavior changes and movement can be reviewed together.

## Reference projects

This layout borrows principles rather than copying another editor's technology choices:

- [Linebender Xilem](https://github.com/linebender/xilem/blob/main/ARCHITECTURE.md) keeps crate
  roots small and groups code by durable roles such as widgets, properties, passes, and views.
- [Fontra](https://github.com/fontra/fontra) makes editor, font overview, font info, core, and
  storage backends visible in its directory structure.
- [Shift](https://github.com/shift-editor/shift) documents explicit app/domain boundaries, named
  tool and command locations, and a file-size review guideline. Runebender adopts the navigation
  discipline without adopting Shift's multi-package architecture.
- [Counterpunch](https://github.com/counterpunchspace/editor) maintains prominent architecture and
  developer-documentation areas beside its application. Runebender keeps one canonical guide here
  rather than scattering per-directory context files.

GPUI Runebender remains a behavioral and visual reference. It is not the architecture template;
new application structure should stay idiomatic to Xilem and Masonry.
