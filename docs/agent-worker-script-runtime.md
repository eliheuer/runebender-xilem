# Python recipe runtime boundary

The native recipe foundation runs explicit Python scripts against immutable bounded JSON captures.
It does not expose a mutable font object and never applies a returned edit.
The application must present a validated proposal and route an explicit Apply action through the existing authorization, receipt, history and guarded edit path.

## Version 1 contract

`document::script_recipe` defines the language-neutral schema.
`ScriptRecipeInput` contains `schema_version`, `job_id`, `input_hash`, one explicit stable `source`, bounded `parameters` and immutable guarded `layers`.
Each layer contains its existing `AgentLayerGuard`, exact width and existing anchors with stable identity, optional name and coordinates.
The constructor computes `input_hash` over every other typed input field.
Validation recomputes that hash so a parameter or scope change invalidates the capture.

`ScriptRecipeResult` echoes the schema version, job identity and input hash.
It contains one bounded report plus optional `reads` and `edits` using the existing `AgentLayerGuard` and `AgentLayerEdits` types.
Result validation requires every guard to match an exact captured guard and every anchor operation to address an anchor in that layer capture.
Version 1 rejects point operations because its input does not expose point identities.
It also rejects unknown fields, nonfinite geometry, altered identity, more than 64 total guarded result entries or more than 256 operations.

The reusable contract contains no document epoch, actor, operation key or authorization field.
Those values belong to the application at the later explicit Apply boundary and cannot be minted by child output.

## Native process runner

`application::platform::script_jobs` owns one bounded worker and retained job table.
The caller supplies the Python executable explicitly; the runner does not install or download Python.
An unavailable executable is a structured job failure, and `ScriptRuntimeAvailability::check` supports an explicit preflight status.
Browser builds retain the contract and job API shape but report that native subprocess execution is unavailable.

For each run, the worker creates a fresh temporary directory and copies the exact submitted saved or draft content to `recipe.py`.
It invokes the configured executable as `python -I recipe.py`, uses no shell, clears the inherited environment and writes the one input JSON value to standard input.
The script must be self-contained because isolated mode does not use an ambient `PYTHONPATH` or adjacent library files.
Standard output must contain exactly one strict result JSON value.
Standard error is retained only as bounded human-readable diagnostics.

Script content is limited to 256 KiB, serialized input and standard output are each limited to 1 MiB, and standard error is limited to 64 KiB.
One queue accepts at most eight pending jobs and retains at most sixteen records.
Each configured deadline must be no longer than five minutes.
The worker kills and waits for its direct child on cancellation, deadline or output overflow and removes its temporary directory when the run returns.

This is not an operating-system sandbox.
The interpreter and user-authored code retain the user's filesystem privileges.
The runner does not claim containment of descendants created by a script, and it cannot guarantee cleanup of independently surviving descendants.
The host must not include font paths, private endpoints, sockets, model credentials or other secrets in recipe input.

## Script directory

`application::platform::script_library` opens one existing selected directory and reads only direct, non-symlink `.py` files.
Names cannot contain path separators, hidden traversal components or another extension.
Files must be UTF-8 and no larger than 256 KiB.
Metadata includes the direct filename, a bounded optional `# Description:` header, byte size, modification time and SHA-256 content revision.

A new save requires the destination to be absent.
Replacing or renaming a script requires the exact revision previously observed by the caller.
Save writes and synchronizes a create-new temporary file in the same directory, rechecks the observed destination revision, renames the staged file and synchronizes the directory on Unix.
An observed external change returns a conflict and leaves that external content untouched.
The UI retains unsaved draft text and decides whether to reload or save under another name.

The revision checks are not a universal filesystem lock or an atomic compare-and-swap against arbitrary external writers.
Another process can still race the final check and rename on filesystems without cooperative locking.
The library promises conflict detection for changes it observes, same-directory replacement and no intentional overwrite after an observed mismatch.
