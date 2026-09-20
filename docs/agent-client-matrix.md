# Client connection and capability matrix

Research date: 2026-09-19 Pacific / 2026-09-20 UTC.
This is a setup design and primary-source review, not a completed interoperability test.
See the [architecture report](agent-interface-research.md) for the implementation gate and [acceptance plan](agent-interface-plan.md) for required runtime evidence.

## Connection matrix

Provider authentication belongs to the conversation client.
MCP server authentication and Runebender document authorization are separate boundaries.
Local stdio launches a Runebender adapter, which connects to the editor's private Unix endpoint; it does not load a second font with `--font`.
No client in this table should infer the intended document from a filename alone.

| Client/surface | Verified connection/configuration path | Authentication boundary | Runebender recommendation and verification status |
|---|---|---|---|
| Claude Code on the editor machine | Stdio; project `.mcp.json` or `claude mcp add --transport stdio --scope project`; HTTP also documented | Existing Claude login stays in Claude; project server trust is client-managed; HTTP supports OAuth | Use `runebender mcp --live`; verify `/mcp`, discovery, selected document and actual PNG consumption; not run here |
| Codex CLI on the editor machine | Stdio or Streamable HTTP; `~/.codex/config.toml` or trusted project `.codex/config.toml` | Existing Codex login stays in Codex; HTTP supports bearer/OAuth | Use local stdio; a root `.mcp.json` alone is not the documented Codex configuration path; not run here |
| Codex local desktop host | Official docs currently call this the ChatGPT desktop app and describe shared Codex-host configuration with CLI/IDE | Host permissions and provider login stay in the host | Verify the actual installed local host, trust and binary environment; do not infer ordinary ChatGPT chat support from a Codex-host tool connection; not run here |
| Codex remote/cloud execution | Stdio starts on the selected execution host; a remote host's filesystem is not the designer's machine | Host/network permission and remote server authentication | Require an explicitly connected local host or supported authenticated remote route; do not pass the laptop's Unix path to a cloud process |
| ChatGPT web developer-mode app | Remote MCP over streaming HTTP/SSE; current docs also offer Secure MCP Tunnel to private stdio/HTTP | App OAuth, no-auth or mixed modes documented; tunnel has control-plane credentials and org/workspace association | Evaluate tunnel with a restricted Runebender document binding; no direct Unix socket or repository configuration discovery; account capability and writes must be tested |
| Other ChatGPT surfaces, including mobile/agent/deep-research modes | No blanket local-MCP contract established by the reviewed Codex-host docs | Surface and workspace policy | Do not advertise live editing until that exact surface passes the harness; opening a chat is not proof of tool/write access |
| Pi without an extension | Pi explicitly omits built-in MCP; can invoke documented CLI tools or an extension | Pi provider login/key; local process rights | CLI is a legitimate first path; no automatic `.mcp.json` loading in bare Pi; image delivery needs an actual supported image tool/extension |
| Pi with `pi-mcp-adapter` | Third-party adapter reads project `.mcp.json`; stdio and HTTP with SSE fallback documented | Adapter owns server auth, including OAuth/bearer; Pi owns provider auth | Optional integration, pin/test adapter separately; do not make it a Runebender dependency or imply it ships with Pi |
| OhMyPi / OMP | Native stdio, HTTP and SSE; `.omp/mcp.json`, user/profile config; root `.mcp.json` fallback | OMP server OAuth/header configuration; provider login remains separate | Existing root config is usable, but discovery precedence can shadow it; use `/mcp reload` and `/mcp test runebender`; source retains MCP image blocks; current version not run here |
| Local model through Pi/OMP or another tool host | Host performs tool loop; local inference endpoint is distinct from MCP | Local runtime authentication/configuration depends on host | Reuse the same live contract; verify tool and vision capability for the exact model/runtime; no model download needed for research |
| Local model called directly | Ollama documents function/tool calls; application must execute calls and return results | Inference service policy plus Runebender grants | Optional future worker/host adapter; never assume an inference endpoint itself discovers MCP servers or has font-editing authority |

Primary sources: [Claude Code MCP](https://code.claude.com/docs/en/mcp), [OpenAI MCP host configuration](https://learn.chatgpt.com/docs/extend/mcp?surface=cli), [ChatGPT developer mode](https://developers.openai.com/api/docs/guides/developer-mode), [ChatGPT plugin connection](https://developers.openai.com/plugins/deploy/connect-chatgpt), [Secure MCP Tunnel](https://developers.openai.com/api/docs/guides/secure-mcp-tunnels), [Pi README](https://github.com/earendil-works/pi/blob/d1230ea2000d876b479a69b8b061f9d670f262f5/packages/coding-agent/README.md), [Pi adapter](https://github.com/nicobailon/pi-mcp-adapter/blob/97435aabf74e5fbcf1112e7244f931172b9db624/README.md), [OMP configuration](https://github.com/can1357/oh-my-pi/blob/10b867cb2eeb7809b883a88dfebe1919ff0c2764/docs/mcp-config.md), [Ollama tools](https://docs.ollama.com/capabilities/tool-calling).

Current OpenAI API documentation describes web developer-mode read/write tools and multiple eligible plans.
Availability still depends on actual account/workspace controls; do not turn that statement into a guarantee for Eli's account or every ChatGPT mode.
The requested matrix is about verified integration paths, not a claim that every provider or model understands the same images, tools, events or approvals.

## Setup recipes to validate after the gate

Provider sign-in paths are independent of these MCP recipes:

| Host | Documented provider authentication |
|---|---|
| Claude Code | `/login` for subscription OAuth; `ANTHROPIC_API_KEY` for direct API use; third-party providers have their own configuration |
| Codex local CLI/desktop | ChatGPT sign-in or API key; CLI browser flow starts with `codex login`, and `codex login status` reports the active method |
| ChatGPT web | The user's ChatGPT account/workspace login; the custom MCP app has its own server authentication |
| Pi | `/login` or a configured provider API key; `/model` selects the model |
| OMP | Provider account/API configuration and `/login` where supported; custom local providers use `~/.omp/agent/models.yml` |

Sources: [Claude authentication](https://code.claude.com/docs/en/authentication), [OpenAI authentication](https://learn.chatgpt.com/docs/auth), [Pi provider setup](https://github.com/earendil-works/pi/blob/d1230ea2000d876b479a69b8b061f9d670f262f5/packages/coding-agent/README.md#providers--models), [OMP provider setup](https://github.com/can1357/oh-my-pi/blob/10b867cb2eeb7809b883a88dfebe1919ff0c2764/README.md).
Do not copy these provider credentials into Runebender or use an MCP grant as a model-provider credential.

These snippets document existing configuration mechanisms.
They were not installed or executed in this research task.
Use an absolute path to the binary built from the released final main, and record its hash; a stale `runebender` on PATH can expose different tools.

### Claude Code

```sh
claude mcp add --transport stdio --scope project runebender -- /absolute/path/runebender mcp --live
```

Alternatively use the existing project `.mcp.json`, after accepting the client's project-server trust flow.
Inspect `claude mcp get runebender` and `/mcp`; configuration creation alone does not prove a healthy server.
Do not alter unrelated Claude permissions or credentials.

### Codex CLI and local desktop host

Use the existing trusted-project `.codex/config.toml` with the selected binary:

```toml
[mcp_servers.runebender]
command = "/absolute/path/runebender"
args = ["mcp", "--live"]
```

For CLI-managed user setup, the documented command shape is:

```sh
codex mcp add runebender -- /absolute/path/runebender mcp --live
```

Verify the intended configuration scope, then `/mcp` and a read call on the actual host.
Shared configuration does not imply shared process lifetime or approval state across every task.
MCP authentication is unnecessary for this local stdio process; the existing Unix endpoint trusts processes under the same user.
That trust is not isolation from malicious same-user software.

### OMP

OMP's current pinned guide prefers `.omp/mcp.json`; the existing root `.mcp.json` is a portable fallback.
Avoid duplicate entries with different binary paths in multiple discovery sources.
Inspect the resolved source and active profile before testing.

```json
{
  "mcpServers": {
    "runebender": {
      "type": "stdio",
      "command": "/absolute/path/runebender",
      "args": ["mcp", "--live"]
    }
  }
}
```

Run `/mcp reload`, `/mcp list`, and `/mcp test runebender` inside OMP.
The pinned [tool bridge](https://github.com/can1357/oh-my-pi/blob/10b867cb2eeb7809b883a88dfebe1919ff0c2764/packages/coding-agent/src/mcp/tool-bridge.ts#L188) forwards image content.
This establishes source support, not proof that the selected model received or interpreted the image.

### Pi

Without an adapter, document the CLI sequence: `runebender sessions`, explicit `--session`, read schema/context, then calls through `--args-file -`.
Never substitute `--font` if a live call fails.
For optional MCP support, the third-party adapter documents `pi install npm:pi-mcp-adapter`, restarting Pi, and `/mcp` setup/discovery.
Before installation in a real workflow, select a reviewed package revision and test it in an isolated Pi configuration; no installation is authorized by this research recipe alone.
The adapter's support for `.mcp.json` belongs to the extension, not Pi core.

### ChatGPT web

The first experiment should use a developer-mode connection to a private, explicitly shared disposable document.
Current plugin docs allow a public HTTPS MCP endpoint or Secure MCP Tunnel.
For the tunnel route, create/associate the tunnel using the official flow, configure the local tunnel client to launch the approved stdio adapter, verify its health, then select the tunnel when creating the app.
Keep credentials in the host's supported secret store/environment, not committed configuration or tool arguments.
No tunnel, account setting or exposed service was created here.

Treat this as an interoperability experiment after local correctness, because host reconnection and tool invocation may not preserve Runebender's current implicit per-process connection state.
Require explicit document identity in the new protocol and scope the bridge to allowed documents.
Refresh discovered tool metadata after changing schemas and retest writes and confirmation behavior in a new conversation.
Provider-side confirmation is independent of Runebender's operation grant.

### Local inference

Pi documents custom models through `~/.pi/agent/models.json` for Ollama, vLLM and LM Studio, including compatibility settings and authentication placeholders for keyless runtimes.
Use the user's installed runtime and model; record the actual model hash, tool parser, context limit and image capability.
Do not infer vision from a model's successful JSON tool call.
[Pi model configuration](https://github.com/earendil-works/pi/blob/d1230ea2000d876b479a69b8b061f9d670f262f5/packages/coding-agent/docs/models.md).

## Per-client proof checklist

For every tested combination, record client version, model/provider, OS/host, configuration scope and server binary commit/hash.
Then record separate outcomes for:

1. Server launch and protocol negotiation, tool discovery and schema validation.
2. Correct selection among two live documents, including stale endpoint rejection.
3. Reading an unsaved edit that differs from disk.
4. Delivering structured values and a proof image to the model, with a visual-only identification check.
5. Making a valid proposal, applying an authorized scoped edit, then undoing without a save.
6. Rejecting a stale proposal and preserving a concurrent designer change.
7. Recovering from disconnect/timeout using a receipt, and observing cancellation's final state.
8. Handling denied/out-of-scope operations without widening permissions or silently falling back to files.

Record `pass`, `fail`, `unsupported`, or `not tested` per capability.
Do not collapse these into one “MCP works” label.
Current status for all new combinations in this report is `not tested`.
The older `docs/ai-type-design.md` records an OMP 18.1.10 synthetic image-delivery result; that is historical evidence, not a current all-client or type-design-quality result.
