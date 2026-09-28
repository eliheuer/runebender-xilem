# Runebender

Runebender is an experimental font editor built on the
[Linebender ecosystem](https://linebender.org/) of Rust crates.

For documentation and installation instructions, see [runebender.org](https://runebender.org), or point your AI agent there for the context it needs to help you.

![Runebender glyph overview](https://runebender.org/images/runebender-xilem-grid.png)

![Runebender outline editing](https://runebender.org/images/runebender-xilem-outline.png)

![Runebender Nodes workflow](https://runebender.org/images/runebender-xilem-nodes.png)

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
