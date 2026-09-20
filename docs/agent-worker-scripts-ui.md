# Scripts UI worker handoff

This phase adds the native Xilem chat-artifact and Scripts-panel presentation seam.
It recognizes only fully closed `python` or `py` Markdown fences, so streaming prose and unterminated fences cannot become drafts.
The Chat panel shows a completed artifact while streaming and retains it after the turn ends.
Opening an artifact is explicit and only copies it into the editor-owned draft buffer.
Opening does not save, execute, contact a model, or mutate the font.
The Scripts panel provides a multiline Python text area and marks name or source edits as unsaved.
An edited draft rejects an assistant artifact replacement until the user saves or discards it.

The runtime worker owns the persistent `ScriptLibrary`, subprocess queue, and pure recipe-result validation.
The UI integration owns Workspace input capture, scope and parameter binding, document/script staleness, preview state, cancellation controls, and explicit receipt-backed Apply through the existing history path.
This phase deliberately does not create a second library, executor, recipe protocol, preview store, or undo path.
Until that runtime is registered, the panel states that Script storage and Run controls are unavailable instead of showing controls that do nothing.
After integration, library loading must preserve the runtime revision on the draft, Save must use its expected-revision conflict handling, and an external change must never overwrite an edited draft.
Changing the document, script, parameter values, source, or selected-glyph scope must invalidate any preview before Apply.
The native runner must keep Python unavailable in the browser, while this shared panel remains buildable and states the limit honestly.

The intended first runnable vertical slice is a read-only anchor report.
An editing recipe may only preview and then explicitly Apply through the existing guarded `AgentEditRequest` receipt path.
The first mutation scope remains one explicit source with at most 64 guarded layers and 256 operations.
Cross-source and all-master mutation must remain unavailable until an engine-backed atomic scope exists.

The UI was designed with existing Xilem and Masonry controls, palette tokens, and the shared text-input behavior.
The multiline editor uses the framework's `InsertNewline::OnEnter` behavior and carries no promise of source-code syntax support.
The Scripts rail is reachable for a headless native capture with `RUNEBENDER_RAIL=scripts`.
The expected visual proof is an idle Gray and Light capture after the runner has enabled its persisted list and commands.
That capture does not prove foreground pointer, IME, GPU, or browser-process behavior.
