# AI-assisted type design

Runebender should be a local AI font application in its own right. Local inference,
reusable workflows, direct editing, and external agents should share its font operations.
ComfyUI is an architectural reference, not a service Runebender needs to connect to.
GPT-6 Astra is an intended client; the editing contract must also work with local models,
Python programs, and ordinary shell scripts.

## Research and decision

Reviewed 2026-09-05. Counterpunch source was inspected at
[`1cc976ae88de2f7b95c823796b39040fede30188`](https://github.com/counterpunchspace/editor/tree/1cc976ae88de2f7b95c823796b39040fede30188).
This was source inspection, not a runtime comparison or an agent benchmark.

Counterpunch runs Python in the browser with Pyodide. Its `Font()` wrappers expose the
live JavaScript font model. The assistant can execute Python, discover API documentation,
read editor state, compile and inspect fonts, and author reusable scripts. See its
[Python API](https://github.com/counterpunchspace/editor/blob/1cc976ae/API.md) and
[assistant tool definitions](https://github.com/counterpunchspace/editor/blob/1cc976ae/webapp/js/assistant-config.ts).

Its Python hooks snapshot the font, derive a change set after execution, and feed that
change set into history and synchronization. Assistant execution is serialized and waits
for committed updates. A script can make changes and then fail; the assistant reports
partial commits explicitly. See
[post-execution handling](https://github.com/counterpunchspace/editor/blob/1cc976ae/webapp/js/python-post-execution.ts)
and [assistant execution](https://github.com/counterpunchspace/editor/blob/1cc976ae/webapp/js/ai-assistant.ts#L3036).

The useful lesson is the complete loop: discover the API, inspect the actual document,
compose operations, see the result, and recover. Python enables flexible loops and reusable
programs. The difficult integration work is maintaining authoritative state and edit
history across scripting and UI boundaries. Python does not require live object mutation.

Blender exposes scripting and background rendering through its
[command line](https://docs.blender.org/manual/en/5.1/advanced/command_line/arguments.html).
[Astra's documented tools](https://developers.openai.com/api/docs/models/gpt-6-astra)
include function calling and MCP. The inference that scriptability plus visual verification
helps an agent is an engineering judgment; these sources do not establish why Astra succeeds
on any particular Blender task or predict its type-design quality.

ComfyUI's [local runtime API](https://docs.comfy.org/development/comfyui-server/comms_routes)
provides model discovery, workflow submission, queues, progress, history, and interruption.
Those are useful capabilities for Runebender to own. A node canvas alone is insufficient.

## Architecture

Keep font behavior in Rust over norad and kurbo. Frontends own the window and input.
Inference workers own model runtimes and may be written in any language. A Python client
can compose structured operations without embedding Python or duplicating geometry.

The target is one document session with revisions, transactions, undo, proofs, and events.
The editor, CLI, node workflows, and MCP should be adapters to that session. Standalone
file operations remain useful for batch jobs. They must not pretend to represent unsaved
editor state. Model jobs should consume a declared snapshot and return a proposal tied
to it, not mutate the open document behind the editor.

## Available first phase

- `project_info` lists loaded master indices, names, and source paths. Agent operations on
  multi-master projects require `master`; they never silently select the first source.
- `read_glyph` returns points in contour order, component transforms, anchors, metrics, a
  canonical GLIF SHA-256 revision, and the selected source and layer. Point indices are
  zero-based and valid only for that revision.
- `propose_edits` validates an entire batch on private glyph copies. Supported operations
  are `set_width`, `set_point`, `translate`, and `set_anchor`. Translate moves outline
  points, component offsets, and anchors together, keeping advance width unchanged.
- A batch needs a unique task, a design reason, and each glyph's expected foreground
  revision. Unknown fields, stale revisions, duplicate glyphs, invalid indices, nonfinite
  values, negative advances, and empty/no-op edits fail. It creates a proposal, not an
  installed edit. It preserves contour/point order; it does not guarantee smoothness,
  optical quality, or family-wide interpolation quality.
- Proposal creation writes only a new layer and `layercontents.plist`. It does not rewrite
  foreground GLIFs. Its publication rechecks the layer index and edited glyphs. Cooperative
  proposal writers use a lock. External applications do not share that lock: this is **not**
  a filesystem transaction against concurrent editor saves. Coordinate saves until the
  session service owns them. A crash can leave an unreferenced layer or lock requiring
  inspection; do not remove another running writer's lock.
- Installation through core rejects stale guarded glyphs even with `--any-structure`.
  It retains the existing per-glyph install/undo semantics. A subset may install while
  stale glyphs remain proposed. Legacy model proposals without revision metadata retain
  their old behavior; they are not retroactively guarded.
- `proof --layer NAME` and the `proof` agent tool render proposal layers. Components use
  the proposal overlay, falling back to foreground bases. Agent proofs include SVG text,
  a unique temporary path, and metrics. SVG is not a native MCP raster image; clients need
  to display it in a browser or render it before claiming visual verification.
- Agent workflow execution uses `--proposal-only`, rejecting install, feature-writing,
  and unknown core nodes before execution. Trusted local `font-ml` programs still own
  their proposal-writing contract; this flag is not an executable sandbox.
- `agent call --args-file FILE` accepts large batches without shell quoting or argument
  length problems. `--args-file -` reads JSON from stdin. The same tools are listed by MCP.

## Working session

Build and use this revision of the binary (`cargo build --locked`), or install it explicitly.
An older binary on PATH will not have the new tools. For an MCP client, launch:

```sh
runebender-core mcp --font /absolute/path/Family.designspace
```

The host chooses Astra or a local language model; no provider SDK or API key belongs in
core. Remote model providers receive whatever context their host sends. Local font files
alone do not make a remote chat private or offline.

Start with `project_info`, choose the intended master, then read the relevant foreground
glyphs. Use `agent tools --json` for the published input schemas. For example:

```sh
runebender-core agent call project_info --font /absolute/path/Family.designspace
runebender-core agent call read_glyph --font /absolute/path/Family.designspace \
  --args '{"master":0,"glyph":"n"}'
```

Create a JSON file using the returned revision and measured width, not these placeholders:

```json
{
  "master": 0,
  "task": "n-spacing-01",
  "reason": "Compare a 12-unit increase in the right sidebearing",
  "edits": [{
    "glyph": "n",
    "expected_revision": "COPY THE FOREGROUND REVISION HERE",
    "operations": [{"op": "set_width", "width": 600}]
  }]
}
```

```sh
runebender-core agent call propose_edits --font /absolute/path/Family.designspace \
  --args-file edits.json
runebender-core agent call proof --font /absolute/path/Family.designspace \
  --args '{"master":0,"glyphs":["n"],"layer":"com.runebender.proposal.n-spacing-01"}'
```

Compare foreground and proposal proofs at the same scale. Inspect text specimens before
accepting spacing decisions; a cell proof alone is insufficient. Installation is a separate,
authorized action in an editor using this core, or:

```sh
runebender-core --json proposal install /absolute/path/Master.ufo --task n-spacing-01
```

The CLI's in-memory undo history does not survive process exit. Use the editor's undo or
versioned source backups for persistent recovery. Existing installation writes the UFO;
coordinate other writers. The web editor can show on-disk updates through
`runebender-serve /absolute/path/Family.designspace --open`; native proposal review and the
web editor's draft UI are different interfaces, and this phase does not unify them.

`examples/propose_spacing.py` demonstrates a Python client: it reads a glyph, calculates
an advance, submits a proposal, and returns both proofs. Its default mode only prints the
batch. Start on a copy of a font before using `--write` on working sources.

## Live native editor sessions

The native GPUI and Xilem editors now create one private Unix socket per open document
lifetime. Core handles calls on the UI thread against the editor's actual `Project`.
Opening another document replaces the endpoint. There is no fallback from a failed live
connection to files on disk. Windows and browser transports are not implemented.

```sh
runebender-core sessions
runebender-core agent call project_info --session /absolute/path/session.sock
runebender-core agent call read_glyph --session /absolute/path/session.sock \
  --args '{"master":0,"glyph":"n"}'
runebender-core mcp --session /absolute/path/session.sock
```

Use a path from `sessions`; call `project_info` to verify which project it represents.
The list can include stale endpoints after a crash. Select a document explicitly instead
of guessing the most recent window. Multiple agent clients can use the same endpoint;
requests are serialized on the UI thread and conflicting proposals fail revision checks.

Live tools support `project_info`, `font_info`, `read_glyph`, `proof`, `propose_edits`,
`proposal_list`, and `proposal_discard`. Proofs require 1–256 explicit glyph names and
return SVG content and metrics without writing a file. Disk-based model and node jobs
are excluded from this tool list. Proposals remain unsaved in the document and are
reviewed through the existing Local AI panel. Installation uses core's existing glyph
undo history. A later manual edit makes an earlier proposal stale at installation.

The socket lives in a directory created with mode 0700. Local processes running as the
same user can access it; this is not a sandbox against that user's other applications.
Frames are limited to 8 MiB, requests expire after 30 seconds, and expired queued requests
are not executed. A timeout after dispatch can have an uncertain result: inspect the
proposal list before retrying. Reusing a task name never overwrites an existing proposal.
Selection subscriptions, progress events and a shared inference job queue remain future work.

## OMP and the live editor: first testing workflow

Keep OpenAI authentication and conversation in OMP. The editor owns the document; OMP
uses the live MCP tools. The project `.omp/mcp.json` files configure one stable command:

```json
{
  "mcpServers": {
    "runebender": {
      "type": "stdio",
      "command": "runebender-core",
      "args": ["mcp", "--live"]
    }
  }
}
```

Install this core binary and rebuild/restart the native editor. In an existing OMP
session in the configured project, run `/mcp reload`, then `/mcp test runebender`.
Ask OMP: **Connect to my Runebender editor, confirm the font and master, and inspect n
without making changes.** The `editor_sessions` and `editor_connect` tools handle the
endpoint selection. No socket path needs to be pasted into a config file when the editor
reopens. If multiple editors are open, identify the intended font before proposing edits.

OMP's [MCP configuration guide](https://github.com/can1357/oh-my-pi/blob/main/docs/mcp-config.md)
documents project `.omp/mcp.json` discovery, `/mcp reload`, and `/mcp test`.
The setup changes no OMP model, provider, login, or approval preferences.

Then try: **Read n in master 0 and propose 12 more units of right sidebearing. Leave it
as a proposal for me to review.** Inspect the proposal in the editor's Local AI panel,
install it there, and undo once to verify recovery. This is a mechanical integration
test, not a spacing recommendation for Virtua Grotesk. Keep early tests in a font copy.

The same server works with an external Codex TUI/CLI or desktop session:

```toml
[mcp_servers.runebender]
command = "/absolute/path/runebender-core"
args = ["mcp", "--live"]
```

The desktop app, CLI and IDE share MCP configuration on the same Codex host, according to
OpenAI's [MCP documentation](https://learn.chatgpt.com/docs/extend/mcp?surface=cli).
ChatGPT web does not read local Codex configuration.

GPUI's local GGUF chat now also uses the live endpoint through
`RUNEBENDER_LIVE_SESSION`, inherited by `font-ml`'s core tool subprocesses. It no longer
saves the font before a turn. Xilem exposes the same external tools but has no chat pane.
Embedded Codex-login chat is deferred while the OMP workflow is tested. A preliminary
Codex subprocess experiment authenticated, but its MCP calls required approval that a
noninteractive turn could not request; no embedded Codex option ships in this phase.

## Drawing missing glyphs

Prefer typed contour/point operations and proofs for font geometry. Computer use is useful
for seeing the editor and checking its interaction, but mouse coordinates are a poor
contract for precise outlines. `set_outline` supplies complete validated UFO contours;
`set_smooth` records on-curve intent. Existing empty records can receive drawings, but
entirely absent records must still be created in the editor. `glyph_inventory` resolves
mark labels and Unicode scalars per master. `read_glyph` exposes numeric join diagnostics.
Live `proposal_install` requires an explicit structure policy and records per-glyph undo.
It is intended only for an explicit user request to apply a reviewed proposal.

The CLI MCP adapter sends data-only scenes to Designbot for PNG image blocks. This keeps
rendering off the GUI thread and out of editor library dependencies. Actual client image
delivery was tested through OMP 18.1.10 using a synthetic live document: the configured
model correctly identified an upright triangle from the proof image without reading
coordinates. This verifies transport, not aesthetic quality. A bounded Latin specimen and independent kerning experiments now ship in the [experiment MVP](experiments-mvp.md).

`design_context` points agents to the official type-design guide, MCP guide, and consolidated
`llms-full.txt`. The website is the technique reference; project `DESIGN.md` and explicit
user instructions determine the font's style and approved references. Docs contain no
executable instructions that bypass the proposal/revision contract. PDF review uses the
client's PDF tools and must retain page/glyph/pair evidence and unresolved findings.

## Next phases and acceptance criteria

1. **Document session:** extend the live master/proposal bridge with active layer/selection,
   subscriptions, richer transactions, and shared events. Verify that an edit arriving during a model
   job is preserved and that cancelling a batch leaves no partial foreground edits.
2. **Visual evaluation:** native raster MCP proof output, side-by-side and overlay proofs,
   shaped text with kerning, size-controlled specimens, and explicit master interpolation.
   Verify actual images and shaping results, not only coordinates or tool success.
3. **Local runtime:** shared job IDs, queue, cancellation, model capabilities and hashes,
   seeds/settings, run history, and reusable workflows. Cache every input actually read,
   including font metadata, components, runtime version, and model configuration.
4. **Broader editing:** kerning/group transactions, contour creation and deletion, reference
   glyph constraints, and family-wide compatibility checks. Keep small typed tools and
   batched/scripted composition available together.
5. **Virtua Grotesk production:** agree coverage and release criteria, inventory unfinished
   glyphs/masters, establish approved reference glyphs, then work in small design batches.
   Review rhythm, spacing, curves, marks, and interpolation in real text. Preserve accepted
   decisions and provenance. Compile and validate before calling the font finished.

A passing CLI test proves a tool contract, not that Astra is a competent type designer.
Evaluate the same bounded tasks with Astra and local models: correct master, grounded
measurements, valid proposals, useful visual revision, honest completion reports, and
reliable recovery. Do not auto-accept an aesthetic decision merely because validation passes.
