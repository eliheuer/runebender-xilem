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
| `r03-text-selection-gray.png` | Gray | text tool and preview | same text; selected logical beh after the kasra, with visible bidi selection |
| `r03-text-selection-light.png` | Light | text tool and preview | same state |
| `t04-shaping-options-gray.png` | Gray | text tool and preview | `R لا 123 بِ`; Urdu locale; `rlig` disabled and lam-alef separated |
| `t04-shaping-options-light.png` | Light | text tool and preview | same state |
| `r01-arabic-beh-gray.png` | Gray | Arabic editor | Regular `beh-ar`; U+0628; advance 944; fitted outline |
| `r01-arabic-beh-light.png` | Light | Arabic editor | same state |
| `r04-ai-compare-gray.png` | Gray | Local AI review | disposable Regular UFO; R foreground with amber `bolden` proposal; Local AI rail |
| `r04-ai-compare-light.png` | Light | Local AI review | same state |
| `r05-nodes-review-gray.png` | Gray | review-only nodes | five-node Bolden → Compare graph; no Install node |
| `r05-nodes-review-light.png` | Light | review-only nodes | same state |
| `a07-chat-gray.png` | Gray | Local Chat | R open; three discovered GGUF models; qwen3-4b selected; prompt and Send visible |
| `a07-chat-light.png` | Light | Local Chat | same state |
| `e08-features-check-gray.png` | Gray | Features inspector | read-only Virtua feature text; honest Generate and Check actions |
| `e08-features-check-light.png` | Light | Features inspector | same state |
| `e08-features-edit-gray.png` | Gray | editable Features inspector | Virtua feature draft with Generate, Apply, Revert and Check |
| `e08-features-edit-light.png` | Light | editable Features inspector | same state |
| `e06-arabic-anchors-gray.png` | Gray | Arabic anchor editor | Regular `zero-ar`; four named mark anchors at two positions |
| `e06-arabic-anchors-light.png` | Light | Arabic anchor editor | same state |
| `e06-components-gray.png` | Gray | Shapes rail and component editor | Regular `beh-ar.fina`; selected `dotbelow-ar` component and edit controls |
| `e06-components-light.png` | Light | Shapes rail and component editor | same state |
| `e06-mark-cloud-gray.png` | Gray | Arabic attachment preview | Regular `behDotless-ar`; matching marks ghosted at top and bottom anchors |
| `e06-mark-cloud-light.png` | Light | Arabic attachment preview | same state |

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

The R04 comparison captures use a disposable copy after the verified local CPU
`bolden` command has written its proposal layer. With that copy at
`$trial_root/Regular.ufo`, reproduce either theme with:

```sh
RUNEBENDER_SCREENSHOT=docs/parity/2026-09-11/r04-ai-compare-gray.png \
RUNEBENDER_SIZE=1100x720 RUNEBENDER_THEME=gray RUNEBENDER_GLYPH=R \
RUNEBENDER_RAIL=ai RUNEBENDER_MODEL=/Users/eli/.runebender/models/virtua-12m-bolden \
RUNEBENDER_PROPOSAL_PREVIEW=bolden target/debug/runebender-xilem \
"$trial_root/Regular.ufo"
```

These files were individually inspected at 1100×720. They prove that the pending
proposal and foreground are visually distinguishable and that the proposal row
does not clip its status or decisions; they do not prove a native button click.

The R05 files use the checked-in review-only graph and the read-only source font:

```sh
RUNEBENDER_SCREENSHOT=docs/parity/2026-09-11/r05-nodes-review-gray.png \
RUNEBENDER_SIZE=1100x720 RUNEBENDER_THEME=gray \
RUNEBENDER_NODES=docs/parity/2026-09-11/bolden-review.nodes.json \
RUNEBENDER_MODE=nodes target/debug/runebender-xilem \
../virtua-grotesk/sources/VirtuaGrotesk.designspace
```

The A07 files use the read-only designspace and scan only local model metadata;
they do not load weights or start a chat turn:

```sh
RUNEBENDER_SCREENSHOT=docs/parity/2026-09-11/a07-chat-gray.png \
RUNEBENDER_SIZE=1100x720 RUNEBENDER_THEME=gray RUNEBENDER_GLYPH=R \
RUNEBENDER_RAIL=chat target/debug/runebender-xilem \
../virtua-grotesk/sources/VirtuaGrotesk.designspace
```

Replace `gray` with `light` for the paired capture. Both were inspected at
1100×720: model names, selected state, prompt, and Send fit the 244 px rail.
Their SHA-256 hashes are `0fc4bd17388b4ce60781f1f60a6b528aab0eb99e79909ee31b33c72716088b02`
and `b3d75716f452e0ab1a3054c96386c58bca0b5d004627d14541c3be620937ebb9`.

The E08 files expand the Features section over the read-only Virtua source:

```sh
RUNEBENDER_SCREENSHOT=docs/parity/2026-09-11/e08-features-check-gray.png \
RUNEBENDER_SIZE=1100x720 RUNEBENDER_THEME=gray \
RUNEBENDER_SELECTED=beh-ar RUNEBENDER_EXPAND=Features \
target/debug/runebender-xilem \
../virtua-grotesk/sources/VirtuaGrotesk.designspace
```

Replace `gray` with `light` for the paired capture. Both were individually
inspected: the feature source stays inside its portal and the Generate and Check
actions remain visible. These pre-editor captures record the intermediate state
before the later editable E08 files. Their SHA-256 hashes are
`a4f8234bcdd43a3bd077faafee93ce3fc490356607c7ebb1d7dc8766d5bde58b` and
`690abff7180d1b22969b1352aded31d3b1afa0943da6f8b62e8f10111a66d158`.

The later E08 editable captures use the same command with
`e08-features-edit-<theme>.png`. Both were individually inspected: the feature
text is legible and contained in Gray and Light, while Generate, Apply, Revert,
and Check remain visible below the editor. Their SHA-256 hashes are
`afb0aa39e8efe3abbb2a8caeb66727e70b5bfcae0f87023487d0bd9a0b5c3b9f` and
`ef3b1b4f4a8c0c8bf6d2cdea0a2bac73824a48eb04426993929e0a36442ab7f0`.

The E06 anchor captures open a real Arabic glyph whose Regular source contains
`bottom`, `top`, `bottomDots`, and `topDots` anchors:

```sh
RUNEBENDER_SCREENSHOT=docs/parity/2026-09-11/e06-arabic-anchors-gray.png \
RUNEBENDER_SIZE=1100x720 RUNEBENDER_THEME=gray RUNEBENDER_OPEN=zero-ar \
target/debug/runebender-xilem \
../virtua-grotesk/sources/VirtuaGrotesk.designspace
```

Replace `gray` with `light` for the paired capture. Both were individually
inspected: all four anchors render as paired diamonds at the source's two shared
positions, the header/status agree on `zero-ar`, U+0660 and advance 320, and the
font stays read-only. Their SHA-256 hashes are
`756d1c9fd9c8ac9589362d459855e6e39ef6f802ef8b6dbd67d789ea11d4c47b` and
`3936a2f7960b6e321417447446e79145c879dcf7eeaa88fca68029fec73a51e0`.

The component captures use the same read-only source with a real two-component
Arabic glyph. The second top-level component is selected through the headless
state hook, making both its canvas outline and Duplicate/Delete actions visible:

```sh
RUNEBENDER_SCREENSHOT=docs/parity/2026-09-11/e06-components-gray.png \
RUNEBENDER_SIZE=1100x720 RUNEBENDER_THEME=gray RUNEBENDER_OPEN=beh-ar.fina \
RUNEBENDER_RAIL=shapes RUNEBENDER_COMPONENT=1 target/debug/runebender-xilem \
../virtua-grotesk/sources/VirtuaGrotesk.designspace
```

Replace `gray` with `light` for the paired capture. Both were individually
inspected: the Shapes rail identifies `behDotless-ar.fina` and the offset
`dotbelow-ar`, the selected dot has a distinct outline, and Base glyph/Add plus
Unlock/Duplicate/Delete are unclipped. Their SHA-256 hashes are
`e4df5e4fb278f464948bd4941658dc9e051abc56f596150ff13a28bf94683c97` and
`3949e11ad81d764be19eab986d74fad00d6fbe8c38c02480438cb2559916dbcb`.

The attachment-preview captures enable the Mark cloud on a real Arabic base:

```sh
RUNEBENDER_SCREENSHOT=docs/parity/2026-09-11/e06-mark-cloud-gray.png \
RUNEBENDER_SIZE=1100x720 RUNEBENDER_THEME=gray RUNEBENDER_OPEN=behDotless-ar \
RUNEBENDER_MARK_CLOUD=1 target/debug/runebender-xilem \
../virtua-grotesk/sources/VirtuaGrotesk.designspace
```

Replace `gray` with `light` for the paired capture. Both were individually
inspected: matching top and bottom marks form distinct faint clouds around the
real outline, the source anchors remain legible, and the active Mark cloud
control is not clipped. Their SHA-256 hashes are
`106208f4ec686243c66af562f2a9c22c8f1f457e7645c95d053205ac50843e11` and
`96d7b9723201dc58f4d954276aecade48ec6017fc3126ea9adcbf51ad2e0d590`.

The real integration run uses `RUNEBENDER_AI_DEVICE=cpu` semantics on a copied
designspace, not this read-only screenshot command. It selects R and S, reports
progress, runs Compare, and hands the proposal to explicit review without an
Install node.

The R01 Arabic editor evidence uses the normal editor command with
`RUNEBENDER_GLYPH=beh-ar` and the read-only designspace. Both files were
individually inspected; the header, mini-cell and status agree on `beh-ar`,
U+0628 and advance 944.

The R03 selection evidence uses the read-only designspace and an initial logical
range solely to make the keyboard-selection render deterministic:

```sh
RUNEBENDER_SCREENSHOT=docs/parity/2026-09-11/r03-text-selection-gray.png \
RUNEBENDER_SIZE=1100x720 RUNEBENDER_THEME=gray RUNEBENDER_GLYPH=R \
RUNEBENDER_TOOL=text RUNEBENDER_TEXT='R لا 123 بِ' \
RUNEBENDER_PREVIEW_TEXT='R لا 123 بِ' RUNEBENDER_TEXT_SELECTION=9:10 \
target/debug/runebender-xilem \
../virtua-grotesk/sources/VirtuaGrotesk.designspace
```

Replace `gray` with `light` for the paired capture. Both were individually
inspected: the beh selection is visible, the caret sits at its bidi edge, and
the text and preview remain legible. This proves rendering, not native key or
pointer delivery.

The T04 files add these variables to the same read-only text command:

```sh
RUNEBENDER_TEXT_FEATURES_DISABLED=rlig \
RUNEBENDER_TEXT_SCRIPT=arab RUNEBENDER_TEXT_LANGUAGE=ur
```

Both captures were individually inspected. The second control row fits at
1100×720, Urdu and every feature except `rlig` have unambiguous selected state,
and the separated lam-alef is visible in both the editor and preview. The real
Virtua integration test independently asserts that shaping change.

The V09 overview-preview pair exercises a tall outlined glyph and a blank glyph
against the real, read-only Virtua designspace after reducing the preview tile to
fit below the standard collapsed inspector headers:

```sh
RUNEBENDER_SCREENSHOT=docs/parity/2026-09-11/v09-preview-tall-gray.png \
RUNEBENDER_SIZE=1100x720 RUNEBENDER_THEME=gray RUNEBENDER_SELECTED=Aring \
target/debug/runebender-xilem \
../virtua-grotesk/sources/VirtuaGrotesk.designspace

RUNEBENDER_SCREENSHOT=docs/parity/2026-09-11/v09-preview-empty-light.png \
RUNEBENDER_SIZE=1100x720 RUNEBENDER_THEME=light RUNEBENDER_SELECTED=space \
target/debug/runebender-xilem \
../virtua-grotesk/sources/VirtuaGrotesk.designspace
```

Both were individually inspected at 1100×720. `Aring` now shows its complete
outline and top control point with padding, while `space` keeps a deliberately
blank tile; neither requires an initial inspector scroll. Their SHA-256 hashes
are `3716743e9db488bda03f08bfc4b87da5fc2d55e328cb4b53e7dd76faa1cc2db3`
and `c40903a0c66716fb1950aa4e9eb94133c814448b91b61a953a1d7e2b1d3e7502`.

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
