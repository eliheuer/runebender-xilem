# Counterpunch workflow comparison

This review was refreshed on September 20, 2026, while the native Agentic Control candidate was undergoing combined validation.
It compares documented workflows and source implementations, not measured performance or a complete product audit.
Counterpunch describes itself as alpha and marks several features as partial in its [official feature overview](https://github.com/counterpunchspace/editor#feature-overview).
Its font infrastructure already uses Rust, including Babelfont and fontc; the architectural distinction is ownership and integration, not simply whether a product uses Rust.

## Useful features to adapt

| Workflow | Counterpunch evidence | Native Runebender decision |
|---|---|---|
| Find reusable scripts by purpose | The [run dialog](https://github.com/counterpunchspace/editor/blob/main/webapp/js/run-python-script-dialog.ts) and [header parser](https://github.com/counterpunchspace/editor/blob/main/webapp/js/python-script-header.ts) index descriptive script metadata. | Existing Scripts searches filenames and descriptions; add title/keyword metadata and recent scripts during the next usability pass. |
| Run an unsaved draft and inspect errors | The [script editor workflow](https://github.com/counterpunchspace/editor/blob/main/documentation/python/02-script-editor-workflow.md) keeps authoring, execution and output close together. | The native draft already runs independently of Save and returns report/stderr; prioritize readable errors, scrolling, keyboard behavior and a small recent-run history. |
| Turn an inspection into a useful glyph view | The [general scripting guide](https://github.com/counterpunchspace/editor/blob/main/documentation/python/04-writing-general-scripts.md) describes scripts used for glyph filtering. | Later add a bounded read-only result that opens matching glyphs in Overview; do not overload the current six-field recipe contract during candidate validation. |
| Help the assistant discover supported work | The [assistant configuration](https://github.com/counterpunchspace/editor/blob/main/webapp/js/assistant-config.ts) and [assistant implementation](https://github.com/counterpunchspace/editor/blob/main/webapp/js/ai-assistant.ts) provide tool and documentation discovery. | Native MCP and local-chat prompts now share the recipe contract; next expose compact in-app help generated from the same capability definitions. |
| Keep source and variation context visible | The [Python API](https://github.com/counterpunchspace/editor/blob/main/documentation/python/06-python-api.md) exposes font, glyph, layer and variation concepts. | Keep explicit source/glyph scope in the UI and immutable captures in Python; expand family context through Rust adapters when the atomic scope supports it. |
| Compare actual rendered results | The official overview describes live compilation and shaping. | Native Nodes already uses one unchanged family capture and a staged family overlay for two identically configured compiled specimens; retained hashes and revision labels bind the UI and agent results. |

## Architecture and delivery priority

Retain one authoritative Rust Project, one guarded edit/receipt path, ordinary application history, and shared compiler/proof ownership.
Chat, Python scripts, Nodes and external agents are clients of those operations.
Python can propose edits from captured data but does not own live mutable font objects.
The shared browser widget build remains a compatibility host; it does not require moving native subprocess or document ownership into JavaScript.

This arrangement should reduce duplicated behavior and make tests and error recovery easier to follow.
It does not establish a speed advantage by itself.
Measure opening the same Virtua project, interactive edit latency, proof completion, agent round trips, cancellation, memory use and undo correctness before making comparative performance claims.
Whole-family compilation may remain slower than a correct dependency-aware subset compiler even when both implementations use Rust.

For the noon testing candidate, finish transport, input routing, Apply/Undo, source preservation and combined validation.
During the subsequent UI/UX pass, prioritize script navigation and text editing, readable run history, proof labels, graph persistence and explicit graph-versus-font history controls.
Plugin marketplaces, cloud collaboration, automatic package installation and broad mutable Python font APIs are separate product decisions and are not required for this workflow.

The remaining acceptance items are tracked in [the scripting workflow](agent-scripting-workflow.md), [Nodes plan](agent-nodes-plan.md) and [agent interface plan](agent-interface-plan.md).
