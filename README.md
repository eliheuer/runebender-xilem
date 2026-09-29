# Runebender

Runebender is an experimental font editor built on the
[Linebender ecosystem](https://linebender.org/) of Rust crates.

For documentation and installation instructions, see [runebender.org](https://runebender.org), or point your AI agent there for the context it needs to help you.

![Runebender glyph overview](https://runebender.org/images/runebender-xilem-grid.png)

![Runebender outline editing](https://runebender.org/images/runebender-xilem-outline.png)

![Runebender Nodes workflow](https://runebender.org/images/runebender-xilem-nodes.png)

## Local sketch models

The native Brush panel can draft with an installed Virtua model and compare the result before applying it.
Put compatible model packages in `~/runebender/models`; see [Local models](LOCAL-MODELS.md) for the package layout and runtime requirements.
Use the [scratch-font testing guide](VIRTUA-REGULAR-TESTING.md) for the first Arabic trial.

## Persistent local chat

The native Chat panel can reuse an already running `font-ml serve` model through its loopback HTTP endpoint.
Start the server separately with a model that is already installed, then launch this Runebender build with the endpoint configured:

```sh
font-ml serve --model /path/to/model --bind 127.0.0.1:8790
# In a second terminal:
RUNEBENDER_CHAT_ENDPOINT=http://127.0.0.1:8790 RUNEBENDER_CHAT_MODEL=font-ml cargo run --locked -- /path/to/font.ufo
```

`RUNEBENDER_CHAT_MODEL` defaults to `font-ml`; an optional `/v1` endpoint suffix is accepted.
This adapter supports fixed-length, non-streaming chat completions on a literal loopback address or `localhost` with an explicit port.
It does not connect to cloud providers, launch the server, or download models.
Without `RUNEBENDER_CHAT_ENDPOINT`, Chat retains the existing per-turn local process path.
A configured invalid endpoint reports an error instead of selecting another backend.

Runebender owns the read/proposal tool loop and pins calls to the open document's lifetime; inference state remains in the external server.
Cancel closes the request and discards late results, but the server may continue computing and already dispatched tool operations are not undone.
The server stays running across turns and when Runebender closes; stop it separately when finished.
This route requires the native Unix live-editor endpoint and is unavailable in the browser.

## External workflow worker contract

`runebender nodes types --tool /path/to/font-ml --json` exposes the discovered task ports and execution policy.
External tasks can advertise this optional block in each task returned by `tasks --json`:

```json
{"execution":{"schema_version":1,"cache":"input_fingerprint","rows_input":"json_argument"}}
```

Without this block, external tasks remain executable but run every time and reject Rows inputs.
An explicit block requires version 1; unknown versions, fields, or policy values exclude that task from discovery.
The authoring graph file remains version 1.

`json_argument` sends each Rows input as one compact JSON array argument after its port flag, replacing underscores with hyphens in the flag name.
For example, `report_rows` becomes `--report-rows '[{"glyph":"A","score":0.5}]'`; the runner builds argument vectors directly without a shell.
Empty arrays, nested JSON, and Unicode are preserved.
Each input permits at most 4096 rows, and all Rows arguments together permit at most 65536 UTF-8 bytes.
Unsupported or oversized inputs fail before process launch, and a declared Rows output must be a JSON array in the worker's final report.

`input_fingerprint` is an explicit promise by the trusted worker that declared inputs describe its dependencies.
Cache reuse verifies the node definition, executable, input values and files, and retained output content; older cache files are invalidated.
Model and adapter directories are hashed from their actual bytes, including additional artifacts; manifest digests alone are not trusted.
This conservative check can add disk I/O for large local models.
Workers depending on time, random state, environment variables, network responses, or unlisted files should use `cache: "never"` unless those dependencies are supplied as inputs.
External effects are reported as `trusted_process`; this metadata grants no permissions and provides no process sandbox.
Font-writing builtins and live document nodes are never reused from the disk cache.
External declarations of `write_font` or `live_document` likewise force `cache: "never"` while retaining the `trusted_process` classification.

These are offline worker contracts, not a claim of cloud-provider or model compatibility.
The native Local AI action exports current in-memory data to an owned temporary directory and retains validated output separately until explicit Install.
Install commits the complete candidate as one undo group; Discard leaves the root document unchanged.
This initial adapter accepts ordinary contour point moves, existing anchor moves, and advance changes, with at most 64 captured glyphs and 256 changed values per candidate.
Changed topology, components, hyperbeziers, unsupported metadata, stale results, or out-of-scope glyphs are rejected.
At most 16 candidates are retained, keyed by source and task, for this document session.
An existing saved proposal for the same task must be reviewed or discarded before another model run.
Live DAG model nodes and migration of the older disk-oriented Nodes action remain unfinished; the latter still uses its save-first path.
