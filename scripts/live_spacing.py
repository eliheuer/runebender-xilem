#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Prepare or execute one guarded live spacing batch.

This is a deliberately small procedural example, rather than a general client
library. It changes only an already-open, unsaved Workspace through a caller-
supplied executable and Unix-session endpoint.
"""

from __future__ import annotations

import argparse
import json
import math
from dataclasses import dataclass
from pathlib import Path
import subprocess
import sys
from typing import Any


MAX_GLYPHS = 64
DEFAULT_ACTOR = "procedural-live-spacing"
DEFAULT_HISTORY_NAME = "Procedural spacing adjustment"


class ScriptError(RuntimeError):
    """An expected validation or transport failure."""


@dataclass
class ToolResponse:
    """One parsed CLI result, including a result returned with an error exit code."""

    result: dict[str, Any] | None
    returncode: int | None
    stderr: str
    transport_error: str | None = None

    @property
    def ambiguous(self) -> bool:
        """Whether this call might have reached the Workspace without a usable reply."""

        return self.result is None


def emit(value: dict[str, Any]) -> None:
    """Write one stable JSON report for a script caller."""

    print(json.dumps(value, indent=2, sort_keys=True))


def require_string(value: Any, field: str) -> str:
    """Return a nonempty JSON string or explain the protocol violation."""

    if not isinstance(value, str) or not value:
        raise ScriptError(f"response is missing a nonempty {field}")
    return value


def call_tool(binary: Path, session: Path, name: str, payload: str) -> ToolResponse:
    """Call exactly one live tool through stdin without invoking a shell."""

    argv = [
        str(binary),
        "agent",
        "call",
        name,
        "--session",
        str(session),
        "--args-file",
        "-",
    ]
    try:
        completed = subprocess.run(
            argv,
            input=payload,
            capture_output=True,
            text=True,
            check=False,
            timeout=35,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        return ToolResponse(None, None, "", str(error))
    try:
        envelope = json.loads(completed.stdout)
    except json.JSONDecodeError:
        return ToolResponse(None, completed.returncode, completed.stderr, "CLI did not return JSON")
    if not isinstance(envelope, dict):
        return ToolResponse(None, completed.returncode, completed.stderr, "CLI returned a non-object JSON value")
    result = envelope.get("result", envelope)
    if not isinstance(result, dict):
        return ToolResponse(None, completed.returncode, completed.stderr, "CLI result was not a JSON object")
    return ToolResponse(result, completed.returncode, completed.stderr)


def request_tool(binary: Path, session: Path, name: str, arguments: dict[str, Any]) -> ToolResponse:
    """Encode a newly constructed read-only request."""

    return call_tool(binary, session, name, json.dumps(arguments, separators=(",", ":")))


def require_ok(response: ToolResponse, phase: str) -> dict[str, Any]:
    """Return a successful response or preserve the server's actual failure."""

    if response.result is None:
        detail = response.transport_error or "no usable response"
        if response.stderr.strip():
            detail = f"{detail}: {response.stderr.strip()}"
        raise ScriptError(f"{phase}: {detail}")
    if response.result.get("ok") is not True:
        detail = response.result.get("error", "unknown live-tool failure")
        code = response.result.get("error_code")
        if isinstance(code, str):
            detail = f"{code}: {detail}"
        raise ScriptError(f"{phase}: {detail}")
    return response.result


def validate_endpoint(binary: Path, session: Path) -> None:
    """Require explicit, usable transport paths and never fall back to PATH or disk mode."""

    if not binary.is_file():
        raise ScriptError(f"--binary is not a file: {binary}")
    if not binary.is_absolute():
        raise ScriptError("--binary must be an absolute path to the pinned executable")
    if not session.is_absolute():
        raise ScriptError("--session must be an absolute path to the live Unix socket")


def identity_from_request(request: dict[str, Any]) -> dict[str, str]:
    """Extract the immutable receipt identity without inventing a replacement key."""

    return {
        field: require_string(request.get(field), field)
        for field in ("expected_document_epoch", "actor", "operation_key")
    }


def load_request(path: Path) -> tuple[str, dict[str, Any]]:
    """Read a prepared request verbatim so exact retries preserve its payload and key."""

    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        raise ScriptError(f"cannot read request file {path}: {error}") from error
    try:
        request = json.loads(text)
    except json.JSONDecodeError as error:
        raise ScriptError(f"request file is not JSON: {error}") from error
    if not isinstance(request, dict):
        raise ScriptError("request file must contain a JSON object")
    identity_from_request(request)
    return text, request


def receipt(binary: Path, session: Path, request: dict[str, Any]) -> ToolResponse:
    """Look up the existing immutable receipt for one prepared request."""

    return request_tool(binary, session, "agent_receipt", identity_from_request(request))


def prepare(args: argparse.Namespace) -> int:
    """Read the open document and create a new guarded width-edit request file."""

    if args.request_file is None:
        raise ScriptError("--request-file is required when preparing a request")
    if args.source is None:
        raise ScriptError("--source is required and must be the stable source ID")
    if args.source < 0:
        raise ScriptError("--source must be a nonnegative stable source ID")
    if args.delta is None:
        raise ScriptError("--delta is required when preparing a request")
    if not args.glyph:
        raise ScriptError("supply at least one explicit --glyph")
    if len(args.glyph) > MAX_GLYPHS:
        raise ScriptError(f"at most {MAX_GLYPHS} explicit --glyph values are allowed")
    if len(set(args.glyph)) != len(args.glyph):
        raise ScriptError("each --glyph must be unique within one batch")
    delta = float(args.delta)
    if not math.isfinite(delta):
        raise ScriptError("--delta must be finite")
    actor = require_string(args.actor, "actor")
    operation_key = require_string(args.operation_key, "operation_key")
    history_name = require_string(args.history_name, "history_name")
    if args.request_file.exists():
        raise ScriptError(f"refusing to overwrite request file: {args.request_file}")

    context = require_ok(request_tool(args.binary, args.session, "editor_context", {}), "editor_context")
    context_body = context.get("context")
    if not isinstance(context_body, dict):
        raise ScriptError("editor_context did not include context")
    epoch = require_string(context_body.get("document_epoch"), "document_epoch")
    if context_body.get("busy_gesture") is True:
        raise ScriptError("editor_context reports an active canvas gesture; wait before preparing")

    project = require_ok(
        request_tool(
            args.binary,
            args.session,
            "project_info",
            {"expected_document_epoch": epoch},
        ),
        "project_info",
    )
    sources = project.get("sources")
    if not isinstance(sources, list):
        raise ScriptError("project_info did not include sources")
    if not any(isinstance(source, dict) and source.get("id") == args.source for source in sources):
        raise ScriptError(f"stable source {args.source} is not present in project_info")

    edits: list[dict[str, Any]] = []
    for glyph in args.glyph:
        read = require_ok(
            request_tool(
                args.binary,
                args.session,
                "read_glyph",
                {
                    "expected_document_epoch": epoch,
                    "source": args.source,
                    "glyph": glyph,
                },
            ),
            f"read_glyph {glyph}",
        )
        if read.get("glyph") != glyph or read.get("source_id") != args.source:
            raise ScriptError(f"read_glyph {glyph}: returned a different glyph or source")
        width = read.get("advance")
        if not isinstance(width, (int, float)) or isinstance(width, bool) or not math.isfinite(width):
            raise ScriptError(f"read_glyph {glyph}: advance was not a finite number")
        target = {
            "glyph": glyph,
            "glyph_id": require_string(read.get("glyph_id"), f"glyph_id for {glyph}"),
            "layer": require_string(read.get("layer"), f"layer for {glyph}"),
            "expected_revision": require_string(read.get("revision"), f"revision for {glyph}"),
        }
        new_width = float(width) + delta
        if not math.isfinite(new_width):
            raise ScriptError(f"read_glyph {glyph}: delta produces a non-finite width")
        edits.append(
            {
                "target": target,
                "operations": [{"op": "set_width", "width": new_width}],
            }
        )

    key_probe_request = {
        "expected_document_epoch": epoch,
        "actor": actor,
        "operation_key": operation_key,
    }
    key_probe = request_tool(args.binary, args.session, "agent_receipt", key_probe_request)
    if key_probe.result is not None and key_probe.result.get("ok") is True:
        raise ScriptError("operation key already has a receipt; preserve that request for retry or choose a new key")
    if key_probe.result is None:
        raise ScriptError(f"agent_receipt key check was ambiguous: {key_probe.transport_error}")
    if key_probe.result.get("error_code") != "unknown_operation":
        detail = key_probe.result.get("error", "unexpected receipt lookup result")
        raise ScriptError(f"agent_receipt key check: {detail}")

    request = {
        "expected_document_epoch": epoch,
        "actor": actor,
        "operation_key": operation_key,
        "authorization": "user-approved",
        "source": args.source,
        "history_name": history_name,
        "reads": [],
        "edits": edits,
    }
    encoded = json.dumps(request, indent=2, sort_keys=True) + "\n"
    try:
        with args.request_file.open("x", encoding="utf-8") as output:
            output.write(encoded)
    except OSError as error:
        raise ScriptError(f"cannot create request file {args.request_file}: {error}") from error
    emit(
        {
            "ok": True,
            "mode": "prepared",
            "request_file": str(args.request_file),
            "document_epoch": epoch,
            "actor": actor,
            "operation_key": operation_key,
            "source": args.source,
            "glyphs": args.glyph,
            "delta": delta,
            "mutation_authorized": False,
        }
    )
    return 0


def receipt_outcome_status(result: dict[str, Any]) -> str | None:
    """Return the original apply outcome recorded by a successful receipt lookup."""

    receipt_body = result.get("receipt")
    if not isinstance(receipt_body, dict):
        return None
    outcome = receipt_body.get("outcome")
    if not isinstance(outcome, dict):
        return None
    status = outcome.get("status")
    return status if isinstance(status, str) else None


def reconcile_ambiguous(
    binary: Path,
    session: Path,
    request: dict[str, Any],
    phase: str,
    response: ToolResponse,
    *,
    required_history_state: str | None = None,
) -> int:
    """Inspect receipt state after a lost reply without retrying or changing its identity."""

    status = receipt(binary, session, request)
    outcome = status.result and receipt_outcome_status(status.result)
    outcome_succeeded = outcome in {"committed", "unchanged"}
    history_succeeded = (
        required_history_state is None
        or status.result is not None and status.result.get("history_state") == required_history_state
    )
    if status.result is not None and status.result.get("ok") is True and outcome_succeeded and history_succeeded:
        emit(
            {
                "ok": True,
                "mode": "reconciled",
                "phase": phase,
                "ambiguous_response": response.transport_error,
                "receipt": status.result,
                "receipt_outcome": outcome,
                "retry": "reuse this exact request file and operation key only if a retry remains necessary",
            }
        )
        return 0
    emit(
        {
            "ok": False,
            "mode": "ambiguous",
            "phase": phase,
            "error": response.transport_error or "no usable response",
            "receipt_lookup": status.result,
            "receipt_lookup_transport_error": status.transport_error,
            "receipt_outcome": outcome,
            "required_history_state": required_history_state,
            "retry": "do not generate a new key; inspect the session or retry this exact request payload",
        }
    )
    return 4


def apply_request(args: argparse.Namespace) -> int:
    """Submit one unchanged prepared payload after an explicit authorization flag."""

    if not args.apply:
        raise ScriptError("--apply is required to submit a prepared mutating request")
    text, request = load_request(args.apply_request)
    if request.get("authorization") != "user-approved":
        raise ScriptError("request authorization must be exactly user-approved")
    response = call_tool(args.binary, args.session, "agent_apply", text)
    if response.ambiguous:
        return reconcile_ambiguous(args.binary, args.session, request, "agent_apply", response)
    emit(
        {
            "ok": response.result.get("ok") is True,
            "mode": "applied",
            "returncode": response.returncode,
            "response": response.result,
            "stderr": response.stderr.strip() or None,
        }
    )
    return 0 if response.result.get("ok") is True else 4


def status_request(args: argparse.Namespace) -> int:
    """Report current receipt and history state without submitting a mutation."""

    _, request = load_request(args.status_request)
    response = receipt(args.binary, args.session, request)
    if response.ambiguous:
        raise ScriptError(f"agent_receipt: {response.transport_error}")
    emit(
        {
            "ok": response.result.get("ok") is True,
            "mode": "status",
            "returncode": response.returncode,
            "response": response.result,
            "stderr": response.stderr.strip() or None,
        }
    )
    return 0 if response.result.get("ok") is True else 4


def undo_request(args: argparse.Namespace) -> int:
    """Undo one committed request after checking its current receipt state."""

    if not args.apply:
        raise ScriptError("--apply is required to undo a receipt-backed history group")
    _, request = load_request(args.undo_request)
    current = receipt(args.binary, args.session, request)
    current_result = require_ok(current, "agent_receipt before undo")
    if current_result.get("history_state") != "applied":
        raise ScriptError("agent_receipt does not report an applied history group; refusing to undo")
    history = identity_from_request(request)
    history.update({"authorization": "user-approved", "direction": "undo"})
    response = request_tool(args.binary, args.session, "agent_history", history)
    if response.ambiguous:
        return reconcile_ambiguous(
            args.binary,
            args.session,
            request,
            "agent_history undo",
            response,
            required_history_state="undone",
        )
    emit(
        {
            "ok": response.result.get("ok") is True,
            "mode": "undone",
            "returncode": response.returncode,
            "response": response.result,
            "stderr": response.stderr.strip() or None,
        }
    )
    return 0 if response.result.get("ok") is True else 4


def parse_args(argv: list[str]) -> argparse.Namespace:
    """Parse the bounded example's explicit transports and modes."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True, help="absolute path to one pinned Runebender executable")
    parser.add_argument("--session", type=Path, required=True, help="absolute path to the open Workspace Unix socket")
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--apply-request", type=Path, help="submit this unchanged request JSON")
    modes.add_argument("--status-request", type=Path, help="look up this request's receipt and history state")
    modes.add_argument("--undo-request", type=Path, help="undo this request's history group after --apply")
    parser.add_argument("--apply", action="store_true", help="confirm existing user authorization for a mutation")
    parser.add_argument("--source", type=int, help="explicit stable source ID for preparation")
    parser.add_argument("--glyph", action="append", help="one explicit glyph name; repeat at most 64 times")
    parser.add_argument("--delta", help="finite advance-width delta in font units for preparation")
    parser.add_argument("--request-file", type=Path, help="new JSON file to create during preparation")
    parser.add_argument("--actor", default=DEFAULT_ACTOR, help="receipt actor namespace for preparation")
    parser.add_argument("--operation-key", help="new actor-local receipt key for preparation")
    parser.add_argument("--history-name", default=DEFAULT_HISTORY_NAME, help="one grouped history label for preparation")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    """Dispatch the requested safe mode and print an honest JSON result."""

    args = parse_args(argv)
    try:
        validate_endpoint(args.binary, args.session)
        if args.apply_request is not None:
            return apply_request(args)
        if args.status_request is not None:
            if args.apply:
                raise ScriptError("--apply has no effect with --status-request")
            return status_request(args)
        if args.undo_request is not None:
            return undo_request(args)
        if args.apply:
            raise ScriptError("--apply needs --apply-request or --undo-request")
        return prepare(args)
    except (ScriptError, ValueError) as error:
        emit({"ok": False, "error": str(error)})
        return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
