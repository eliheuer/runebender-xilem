# Agent session receipt worker report

This worker adds a transport-independent, in-memory session metadata and idempotency ledger around canonical Project edit transactions.
The application supplies the document epoch, actor identity, actor-local operation key and a digest of its complete semantic canonical typed payload.
Those canonical payload bytes must cover the complete semantic request, including every guard and requested edit that can affect the result.
The SHA-256 digest is an idempotency identity, not authentication or authorization.
The module does not parse socket or MCP messages and does not own or copy font data.

`AgentSession::apply_document_edit` checks an existing key and ledger capacity before it invokes the staging callback.
The callback receives an immutable Project borrow and returns a staged `CanonicalDocumentEditTransaction`, so the only publication path is the Project's guarded atomic commit.
A newly admitted apply attempt records one immutable committed, unchanged or terminal rejected receipt.
An exact retry returns that receipt without staging again, while a key reused with another payload digest is rejected.
The non-evicting ledger rejects a new key when full, before staging or mutation, so an old key can never become executable through eviction.

`AgentApplyResult::is_new_commit` is true only for the call that first records a committed transaction.
Application integration can use that signal to refresh caches and insert its matching UI-history entry exactly once.
The committed receipt retains the original before and after revision, invalidation scope and Project-owned history-group handle.
It deliberately does not retain mutable applied or undone state; callers query `Project::document_edit_history_group_state` separately.
An exact retry after undo therefore replays the original apply receipt and does not redo the edit.

This in-memory ledger does not provide durable crash recovery.
It does not advertise cancellation, timeout resolution or independent concurrent request handling.
Those behaviors remain transport and application integration work and require their own race and fault-injection evidence.
