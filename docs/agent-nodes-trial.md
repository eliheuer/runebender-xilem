# Native Nodes stdio MCP acceptance trial

`scripts/agent_nodes_trial.py` is a credential-free acceptance trial for the native headless host and the real newline-delimited stdio MCP server.
It creates a disposable UFO containing exactly `.notdef` and `A`, and it never opens or saves a user font.
It requires an explicit absolute Runebender binary and a new evidence directory.

Run it after building the native binary elsewhere:

```sh
python3 scripts/agent_nodes_trial.py \
  --binary /absolute/path/to/runebender \
  --output-dir /absolute/path/to/new-evidence-directory \
  --evidence-label preliminary
```

The trial starts `agent serve`, reads its socket and document epoch, starts a separate `mcp --live` process, performs the MCP handshake and `tools/list`, and explicitly selects the returned socket with `editor_connect`.
It exercises all nine Nodes tools against the application-owned comparison graph.
The graph patch installs a deterministic local Python recipe that increases the selected `A` width by 100 units and changes both proof recipes to `AA`.

The run waits under a bounded deadline for native Python and compiled-proof completion.
It requests the original and changed artifacts through `nodes_image` and saves the exact MCP image-block bytes.
For each image, it verifies PNG structure and CRCs, the terminal run artifact identity, the published image-byte SHA-256, the compiled-font SHA-256, and the canonical compiler-input SHA-256.
This establishes image delivery and lineage through the actual MCP transport.
It does not establish that any model viewed or interpreted an image because no model is involved.

The trial applies the staged edit, performs an exact Apply retry, resolves the common `agent_receipt`, uses the headless host's ordinary Undo, and repeats the exact Apply request to prove that a replay does not reapply an undone edit.
It also repeats the exact run request, observes stale status after Undo, records a terminal `nodes_cancel` as `too_late`, and releases the run.
The source UFO manifest must remain byte-for-byte unchanged throughout.

The evidence directory contains `evidence.json`, `artifact-manifest.json`, `original.png`, and `changed.png` after a passing run.
`artifact-manifest.json` hashes every other retained evidence artifact.
The JSON evidence identifies and hashes the supplied binary, hashes the discovered Nodes schemas and every tool argument set, records bounded host states and receipts, and states the limits of the trial's claims.
Failure evidence retains each completed handshake or tool step, the source manifest, process diagnostics, binary identity, and an explicit statement that the remaining checks were not reached.
Temporary sockets, the disposable source, and process working files are removed during cleanup.

The parser and fixture checks require only Python's standard library:

```sh
python3 -m unittest discover -s scripts -p 'test_agent_nodes_trial.py' -v
```

No native trial evidence is committed here because the validated central binary is supplied separately.
