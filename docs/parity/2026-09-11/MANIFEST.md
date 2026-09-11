# Parity capture manifest — 2026-09-11

These are deterministic, non-foreground captures of the Xilem application. They
are evidence for the named visual state only; they do not certify native input,
GPU rendering, Linux, or browser behavior.

## Fixture

- Application baseline before this phase: `2ce79e5358f84f8b0ce84cd5232c0db438e6b734`.
- Font: `../virtua-grotesk/sources/VirtuaGrotesk.designspace`, Regular master
  (`wght=400`), selected from Virtua Grotesk commit
  `f9edce3221d79114859374df2edf63f939f96185` with unrelated working-tree
  changes left untouched.
- Designspace SHA-256: `0ac18f6e0827af3949a36b3c7043f22a50613948bcc8e8193f268f21539fedf3`.
- Regular UFO deterministic file-list SHA-256:
  `a738e43397d3f8693c175e968615964b21caeed4c129fdeafccc4cd1e0dccb15`.
- Bundled UI font SHA-256:
  `149e6e676d2c2b232135c5812f8c3e168d36f119c37a4719ec29688afb86c7c4`.
- Node graph: `../virtua-grotesk/nodes/bolden.nodes.json`, SHA-256
  `36cb60919e03ede5b1044d73ac519b0e88eac4b5433f43214cb8680e9b6a5374`.
- Logical window: 1100×720. Scale is 1 unless the filename ends in `-2x`.
- Renderer: `imaging_vello_cpu::VelloCpuRenderer`; this is not the native
  Masonry texture backend.
- Screenshot startup could not bind the Unix live-tools socket in the sandbox
  (`Operation not permitted`). The application continued without it; this is
  not recorded as an editor networking regression.

The source font is a read-only visual fixture. No command in this manifest saves
or otherwise mutates it.

## Captures

| File | Theme | State | Selection / viewport |
|---|---|---|---|
| `audit-overview-gray.png` | Gray | pre-V01 overview | ampersand; initial grid offset and zoom; Glyph section expanded |
| `v01-overview-gray.png` | Gray | post-V01 overview | same state, 1× |
| `v01-overview-light.png` | Light | post-V01 overview | same state, 1× |
| `v01-overview-dark.png` | Dark | post-V01 overview | same state, 1× |
| `v01-overview-gray-2x.png` | Gray | post-V01 overview | same 1100×720 logical state, rendered at 2200×1440 pixels |
| `v08-overview-short-gray.png` | Gray | short overview | 1100×480; scrollable groups extend behind the fixed mark bar |
| `editor-r-gray.png` | Gray | editor | glyph R; fitted initial viewport; default sample `Runebender`; no point selection |
| `editor-r-gray-2x.png` | Gray | editor | same 1100×720 logical state, rendered at 2200×1440 pixels |
| `editor-r-light.png` | Light | editor | same state |
| `nodes-bolden-gray.png` | Gray | nodes | bolden graph; stored graph viewport; six nodes and seven links |
| `nodes-bolden-light.png` | Light | nodes | same state |
| `v07-overview-gray.png` | Gray | corrected overview grid | ampersand; 8 px grid padding; fitted 96 px target; full captions |
| `v07-overview-light.png` | Light | corrected overview grid | same state |
| `v07-overview-dark.png` | Dark | corrected overview grid | same state |
| `v07-overview-gray-2x.png` | Gray | corrected overview grid | same 1100×720 logical state, rendered at 2200×1440 pixels |
| `v07-editor-r-gray.png` | Gray | corrected editor rail | glyph R; fitted 44 px target; captions deliberately omitted |
| `r03-mixed-text-gray.png` | Gray | text tool and preview | `R لا 123 بِ`; automatic direction; caret at logical end |
| `r03-mixed-text-light.png` | Light | text tool and preview | same state |

Build first:

```sh
cargo build --locked
```

Overview, replacing `gray` with `light` or `dark` as required:

```sh
RUNEBENDER_SCREENSHOT=docs/parity/2026-09-11/v01-overview-gray.png \
RUNEBENDER_SIZE=1100x720 RUNEBENDER_THEME=gray \
RUNEBENDER_SELECTED=ampersand RUNEBENDER_EXPAND=Glyph \
target/debug/runebender-xilem \
../virtua-grotesk/sources/VirtuaGrotesk.designspace
```

The V07 overview files use the same commands with `v07-overview-<theme>.png` as
the output path. `v07-editor-r-gray.png` uses the editor command below. These
captures were inspected individually; the Gray capture specifically proves the
CPU renderer no longer drops same-colour paths after a zero-radius rounded cell.

Matched logical size at 2× device scale:

```sh
RUNEBENDER_SCREENSHOT=docs/parity/2026-09-11/v01-overview-gray-2x.png \
RUNEBENDER_SIZE=1100x720 RUNEBENDER_SCALE=2 RUNEBENDER_THEME=gray \
RUNEBENDER_SELECTED=ampersand RUNEBENDER_EXPAND=Glyph \
target/debug/runebender-xilem \
../virtua-grotesk/sources/VirtuaGrotesk.designspace
```

Editor and nodes:

```sh
RUNEBENDER_SCREENSHOT=docs/parity/2026-09-11/editor-r-gray.png \
RUNEBENDER_SIZE=1100x720 RUNEBENDER_THEME=gray RUNEBENDER_GLYPH=R \
target/debug/runebender-xilem \
../virtua-grotesk/sources/VirtuaGrotesk.designspace

RUNEBENDER_SCREENSHOT=docs/parity/2026-09-11/nodes-bolden-gray.png \
RUNEBENDER_SIZE=1100x720 RUNEBENDER_THEME=gray \
RUNEBENDER_NODES=../virtua-grotesk/nodes/bolden.nodes.json \
RUNEBENDER_MODE=nodes target/debug/runebender-xilem \
../virtua-grotesk/sources/VirtuaGrotesk.designspace
```

## GPUI reference metrics

These are source-derived metrics, not a claim that the GPUI application was
launched during this run:

- header and navigation rail: 36 logical pixels;
- active rail tab: 32 px; inactive rail tab: 28 px; icon: 18 px; radius: 6 px;
- sidebar row: 19 px; horizontal inset: 14 px; marker box: 10×10 px;
- overview grid cell: 96 px; editor mini-cell: 44 px;
- footer: 28 px; footer icon slot: 20 px.

Sources: GPUI `src/view/chrome.rs`, `src/view/panels/tabs.rs`,
`src/edit/sidebar.rs`, and `src/workspace.rs` at
`79e3ab1d096bdcf2538d946c796a8a0cf01f0573`.
