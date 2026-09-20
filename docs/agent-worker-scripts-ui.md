# Scripts UI worker handoff

This phase adds the native Xilem chat-artifact and Scripts-panel presentation seam.
It recognizes only fully closed `python` or `py` Markdown fences, so streaming prose and unterminated fences cannot become drafts.
The Chat panel shows a completed artifact while streaming and retains it after the turn ends.
Opening an artifact is explicit and only copies it into the editor-owned draft buffer.
Opening does not save, execute, contact a model, or mutate the font.
The Scripts panel provides a multiline Python text area and marks name or source edits as unsaved.
An edited draft rejects an assistant artifact replacement until the user saves or discards it.

The runtime worker owns the persistent `ScriptLibrary`, subprocess queue, and pure recipe-result validation.
The integrated UI owns Workspace input capture, scope and parameter binding, document/script staleness, preview state, cancellation controls, and explicit receipt-backed Apply through the existing history path.
There is one Workspace recipe queue shared with other recipe consumers, and the Scripts panel inspects and discards only its own retained job handle.
The panel does not create a second library, executor, recipe protocol, preview store, or undo path.
Library loading preserves the runtime revision on the draft, and Save and Rename use expected-revision conflict handling.
An external change reports a conflict without replacing the edited draft.
Changing the document, script, parameter values, source, or selected-glyph scope marks the retained preview stale before Apply.
Python stays unavailable in the browser, where the shared panel states the storage and execution limit honestly.

The first runnable vertical slice supports a read-only report and guarded edit proposals using the strict six-field recipe result.
An editing recipe only previews and then explicitly Applies through the existing guarded `AgentEditRequest` receipt path.
The first mutation scope remains one explicit source with at most 64 guarded layers and 256 operations.
Cross-source and all-master mutation must remain unavailable until an engine-backed atomic scope exists.

The UI was designed with existing Xilem and Masonry controls, palette tokens, and the shared text-input behavior.
The multiline editor uses the framework's `InsertNewline::OnEnter` behavior and carries no promise of source-code syntax support.
The shared draggable rail splitter lets users widen the editor for longer source lines.
The code and parameter fields scroll in both directions and use the system monospace family.
They keep a local Undo and Redo history of at most 64 snapshots and 1 MiB, independent from font and graph history.
Restoring an earlier snapshot resets the caret because the pinned text API does not expose selection replacement.
The Scripts rail is reachable for a headless native capture with `RUNEBENDER_RAIL=scripts`.
The expected visual proof is an idle Gray and Light capture with an unsaved draft, editable parameters, and the explicit glyph/source scope visible.
That capture does not prove foreground pointer, IME, GPU, or browser-process behavior.
