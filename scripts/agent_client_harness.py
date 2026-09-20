#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Bounded Milestone 1D client and live-session harness.

This driver only talks to an explicitly supplied Runebender executable and
Unix session endpoint. It never discovers a font by filename or falls back to
disk operations.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import selectors
import shutil
import subprocess
import sys
import time
from typing import Any


CLIENTS = ("codex", "claude", "omp", "pi")
CODEX_BUNDLED_CANDIDATES = (
    Path("/Applications/ChatGPT.app/Contents/Resources/codex"),
)
LIVE_REQUIRED_TOOLS = {
    "project_info",
    "editor_context",
    "read_glyph",
    "agent_apply",
    "agent_receipt",
    "agent_history",
}
PENDING_CAPABILITIES = {
    "compiled_proof": "The compiled-proof API and validated binary are not frozen for this harness; no image trial was attempted.",
    "disconnect_after_commit": "This harness does not inject a socket response drop; exact retry is exercised, but disconnect evidence remains pending.",
    "cancellation": "No cancellation lifecycle is exposed by the current schema or serial adapter.",
}


def sha256_file(path: Path) -> str | None:
    try:
        digest = hashlib.sha256()
        with path.open("rb") as source:
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(chunk)
        return digest.hexdigest()
    except OSError:
        return None


def sha256_json(value: Any) -> str:
    payload = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(payload).hexdigest()


def redact_text(text: str) -> str:
    """Remove common local path prefixes without attempting to parse secrets."""

    home = str(Path.home())
    text = text.replace(home, "<home>")
    text = text.replace(str(Path.cwd()), "<worktree>")
    return text


def redact(value: Any, key: str = "") -> Any:
    """Redact credentials, local paths and bulky proof payloads recursively."""

    lowered = key.lower()
    if lowered in {"authorization", "token", "password", "secret", "credential"}:
        return "<redacted>"
    if lowered in {"svg_content", "png", "image", "image_data"}:
        if isinstance(value, str):
            return {"redacted": True, "bytes": len(value.encode()), "sha256": sha256_json(value)}
        return {"redacted": True}
    if isinstance(value, dict):
        return {name: redact(item, name) for name, item in value.items()}
    if isinstance(value, list):
        return [redact(item, key) for item in value]
    if isinstance(value, str):
        return redact_text(value)
    return value


def run_command(argv: list[str], *, env: dict[str, str] | None = None, timeout: float = 15) -> dict[str, Any]:
    started = time.monotonic()
    try:
        completed = subprocess.run(
            argv,
            env=env,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
        return {
            "argv": redact(argv),
            "returncode": completed.returncode,
            "stdout": redact_text(completed.stdout),
            "stderr": redact_text(completed.stderr),
            "duration_ms": round((time.monotonic() - started) * 1000),
        }
    except (OSError, subprocess.TimeoutExpired) as error:
        return {
            "argv": redact(argv),
            "returncode": None,
            "error": redact_text(str(error)),
            "duration_ms": round((time.monotonic() - started) * 1000),
        }


class FixtureProcess:
    """Own the optional unpublished application fixture control process."""

    def __init__(self, binary: Path, duration_seconds: int) -> None:
        self.binary = binary
        self.duration_seconds = duration_seconds
        self.process: subprocess.Popen[str] | None = None

    def _readline(self, timeout: float) -> dict[str, Any]:
        if self.process is None or self.process.stdout is None:
            raise RuntimeError("fixture process is not running")
        selector = selectors.DefaultSelector()
        selector.register(self.process.stdout, selectors.EVENT_READ)
        try:
            if not selector.select(timeout):
                raise RuntimeError("fixture did not return a control response before timeout")
            line = self.process.stdout.readline()
        finally:
            selector.close()
        try:
            value = json.loads(line)
        except json.JSONDecodeError as error:
            raise RuntimeError("fixture returned non-JSON control output") from error
        if not isinstance(value, dict):
            raise RuntimeError("fixture control response was not an object")
        return value

    def start(self) -> dict[str, Any]:
        self.process = subprocess.Popen(
            [str(self.binary), "agent", "fixture", "--duration-seconds", str(self.duration_seconds)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        readiness = self._readline(15)
        if readiness.get("ok") is not True or readiness.get("fixture") is not True:
            raise RuntimeError(f"fixture was not ready: {readiness}")
        if not isinstance(readiness.get("session"), str):
            raise RuntimeError("fixture readiness did not include a session path")
        return readiness

    def control(self, action: str) -> dict[str, Any]:
        if len(action.encode()) > 1024:
            raise RuntimeError("fixture control action exceeds 1024 bytes")
        if self.process is None or self.process.stdin is None:
            raise RuntimeError("fixture process is not running")
        self.process.stdin.write(json.dumps({"action": action}, separators=(",", ":")) + "\n")
        self.process.stdin.flush()
        return self._readline(15)

    def stop(self) -> dict[str, Any] | None:
        if self.process is None:
            return None
        response: dict[str, Any] | None = None
        try:
            if self.process.poll() is None:
                try:
                    response = self.control("shutdown")
                except (OSError, RuntimeError):
                    response = None
        finally:
            if self.process.stdin is not None:
                self.process.stdin.close()
            try:
                self.process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                self.process.terminate()
                self.process.wait(timeout=5)
        return response


def probe_clients() -> dict[str, Any]:
    """Probe installed command wrappers without reading or changing config."""

    results: dict[str, Any] = {}
    for name in CLIENTS:
        resolved = shutil.which(name)
        if resolved is None:
            results[name] = {"status": "unavailable", "reason": "not found on PATH"}
            continue
        path = Path(resolved).resolve()
        version = run_command([resolved, "--version"], timeout=8)
        version_ok = version.get("returncode") == 0
        results[name] = {
            "status": "ready" if version_ok else "probe_failed",
            "command": redact_text(resolved),
            "executable": redact_text(str(path)),
            "executable_sha256": sha256_file(path),
            "version_probe": version,
            "config_mutated": False,
            "login_attempted": False,
        }
    codex = results["codex"]
    for candidate in CODEX_BUNDLED_CANDIDATES:
        if not candidate.is_file():
            continue
        version = run_command([str(candidate), "--version"], timeout=8)
        help_probe = run_command([str(candidate), "--help"], timeout=8)
        codex.setdefault("bundled_candidates", []).append(
            {
                "status": "ready" if version.get("returncode") == 0 else "probe_failed",
                "executable": redact_text(str(candidate)),
                "executable_sha256": sha256_file(candidate),
                "version_probe": version,
                "help_probe": help_probe,
                "config_mutated": False,
                "login_attempted": False,
            }
        )
    return results


class Harness:
    def __init__(
        self,
        binary: Path,
        session: Path,
        output_dir: Path,
        fixture: FixtureProcess | None = None,
        fixture_commit: str | None = None,
        fixture_readiness: dict[str, Any] | None = None,
    ) -> None:
        self.binary = binary
        self.session = session
        self.output_dir = output_dir
        self.fixture = fixture
        self.fixture_commit = fixture_commit
        self.fixture_readiness = fixture_readiness
        self.transcript: list[dict[str, Any]] = []
        self.tools: dict[str, dict[str, Any]] = {}

    def record(self, phase: str, command: dict[str, Any], parsed: Any = None) -> Any:
        entry: dict[str, Any] = {"phase": phase, "command": command}
        if parsed is not None:
            entry["response"] = redact(parsed)
        self.transcript.append(entry)
        return parsed

    def call(self, phase: str, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        encoded = json.dumps(arguments, separators=(",", ":"))
        command = run_command(
            [
                str(self.binary),
                "agent",
                "call",
                name,
                "--session",
                str(self.session),
                "--args",
                encoded,
            ],
            timeout=35,
        )
        parsed: Any = None
        if command.get("stdout"):
            try:
                parsed = json.loads(command["stdout"])
            except json.JSONDecodeError:
                pass
        self.record(phase, command, parsed)
        if not isinstance(parsed, dict):
            raise RuntimeError(f"{phase}: executable did not return JSON")
        result = parsed.get("result", parsed)
        if not isinstance(result, dict):
            raise RuntimeError(f"{phase}: result was not an object")
        return result

    @staticmethod
    def check_epoch(result: dict[str, Any], epoch: str, phase: str) -> None:
        if result.get("document_epoch") != epoch:
            raise RuntimeError(f"{phase}: document epoch changed during the scenario")

    @staticmethod
    def require_ok(result: dict[str, Any], phase: str) -> None:
        if result.get("ok") is not True:
            raise RuntimeError(f"{phase}: live call failed: {result.get('error', 'unknown error')}")

    @staticmethod
    def require_error_category(result: dict[str, Any], phase: str, expected: str) -> None:
        if result.get("ok") is not False or expected not in result.get("error", ""):
            raise RuntimeError(f"{phase}: expected {expected!r} error category")

    @staticmethod
    def require_error_code(result: dict[str, Any], phase: str, expected: str) -> None:
        if result.get("ok") is not False or result.get("error_code") != expected:
            raise RuntimeError(f"{phase}: expected {expected!r} error code")

    @staticmethod
    def require_rejected_receipt(result: dict[str, Any], phase: str, expected: str) -> None:
        receipt = result.get("receipt")
        outcome = receipt.get("outcome") if isinstance(receipt, dict) else None
        error = outcome.get("error", "") if isinstance(outcome, dict) else ""
        if (
            result.get("ok") is not False
            or not isinstance(outcome, dict)
            or outcome.get("status") != "rejected"
            or expected not in error
        ):
            raise RuntimeError(f"{phase}: expected rejected receipt containing {expected!r}")

    def load_tools(self) -> dict[str, dict[str, Any]]:
        env = os.environ.copy()
        env["RUNEBENDER_LIVE_SESSION"] = str(self.session)
        command = run_command([str(self.binary), "agent", "tools"], env=env)
        parsed: Any = None
        if command.get("stdout"):
            try:
                parsed = json.loads(command["stdout"])
            except json.JSONDecodeError:
                pass
        self.record("schema", command, parsed)
        if not isinstance(parsed, dict) or not isinstance(parsed.get("tools"), list):
            raise RuntimeError("schema: executable did not return a live tools array")
        self.tools = {
            item["name"]: item
            for item in parsed["tools"]
            if isinstance(item, dict) and isinstance(item.get("name"), str)
        }
        return self.tools

    def run_fixture_controls(
        self,
        before_state: dict[str, Any],
        before_advance: float,
        after_advance: float,
    ) -> None:
        if self.fixture is None:
            return
        if before_state.get("source_exists") is not False:
            raise RuntimeError("before_apply: fixture source must remain unwritten")
        if not all(
            before_state.get(field) == before_advance
            for field in ("canonical_advance", "cache_advance", "session_advance")
        ):
            raise RuntimeError("before_apply: canonical/cache/session advances disagree")
        after_apply = self.fixture.control("state")
        self.record("fixture_state_after_apply", {"control": "state"}, after_apply)
        undone = self.fixture.control("undo")
        self.record("fixture_undo", {"control": "undo"}, undone)
        after_undo = self.fixture.control("state")
        self.record("fixture_state_after_undo", {"control": "state"}, after_undo)
        redone = self.fixture.control("redo")
        self.record("fixture_redo", {"control": "redo"}, redone)
        after_redo = self.fixture.control("state")
        self.record("fixture_state_after_redo", {"control": "state"}, after_redo)
        for phase, state in (
            ("after_apply", after_apply), ("undo", undone), ("after_undo", after_undo),
            ("redo", redone), ("after_redo", after_redo),
        ):
            self.require_ok(state, phase)
            if state.get("source_exists") is not False:
                raise RuntimeError(f"{phase}: fixture source must remain unwritten")
        valid_apply = all(
            after_apply.get(field) == after_advance
            for field in ("canonical_advance", "cache_advance", "session_advance")
        )
        valid_undo = all(
            after_undo.get(field) == before_advance
            for field in ("canonical_advance", "cache_advance", "session_advance")
        )
        valid_redo = all(
            after_redo.get(field) == after_advance
            for field in ("canonical_advance", "cache_advance", "session_advance")
        )
        self.fixture_summary = {
            "status": "pass" if valid_apply and valid_undo and valid_redo else "fail",
            "source_path_exists_before": before_state.get("source_exists"),
            "apply_cache_session_match": valid_apply,
            "undo_restored_before": valid_undo,
            "redo_restored_after": valid_redo,
            "source_path_exists_after_redo": after_redo.get("source_exists"),
        }
        if not (valid_apply and valid_undo and valid_redo):
            raise RuntimeError("fixture: canonical/cache/session advances disagree across undo/redo")

    def run_scenario(self, glyph: str, width: float, apply: bool) -> dict[str, Any]:
        tools = self.load_tools()
        names = set(tools)
        missing = sorted(LIVE_REQUIRED_TOOLS - names)
        summary: dict[str, Any] = {
            "schema": {
                "tool_count": len(names),
                "tool_names": sorted(names),
                "sha256": sha256_json(tools),
                "missing_required": missing,
            },
            "capabilities": {},
        }
        for capability, reason in PENDING_CAPABILITIES.items():
            summary["capabilities"][capability] = {"status": "pending", "reason": reason}
        if missing:
            raise RuntimeError(f"schema is missing required tools: {', '.join(missing)}")

        project = self.call("connect_project", "project_info", {})
        self.require_ok(project, "connect_project")
        epoch = project.get("document_epoch")
        if not isinstance(epoch, str):
            raise RuntimeError("project_info did not return document_epoch")
        sources = project.get("sources")
        if not isinstance(sources, list) or not sources:
            raise RuntimeError("project_info did not return a stable source list")
        source = project.get("active_source")
        if not isinstance(source, int):
            source = sources[0].get("id") if isinstance(sources[0], dict) else None
        if not isinstance(source, int):
            raise RuntimeError("project_info did not return a stable source id")
        epoch_guard = {"expected_document_epoch": epoch}
        source_guard = {"source": source, **epoch_guard}
        context = self.call("context", "editor_context", epoch_guard)
        self.require_ok(context, "context")
        self.check_epoch(context, epoch, "context")
        context_source = context.get("context", {}).get("source_id")
        if context_source != source:
            raise RuntimeError("context: active source does not match the bound source")
        before = self.call("read_before", "read_glyph", {"glyph": glyph, **source_guard})
        self.require_ok(before, "read_before")
        self.check_epoch(before, epoch, "read_before")
        if before.get("source_id") != source:
            raise RuntimeError("read_before: returned source does not match the bound source")
        revision = before.get("revision")
        if revision is None:
            raise RuntimeError("read_glyph did not return revision")
        if self.fixture_readiness is not None:
            expected_advance = self.fixture_readiness.get("unsaved_advance")
            if before.get("advance") != expected_advance:
                raise RuntimeError("read_before: advance does not match fixture readiness")
        if not math.isfinite(width):
            raise RuntimeError("requested width must be finite")
        if width == before.get("advance"):
            raise RuntimeError("requested width is unchanged; choose a different width")
        actor = "agent-client-harness"
        operation_key = f"harness-spacing-{time.time_ns()}"
        request = {
            "expected_document_epoch": epoch,
            "actor": actor,
            "operation_key": operation_key,
            "authorization": "user-approved",
            "source": source,
            "history_name": "Bounded harness spacing edit",
            "reads": [],
            "edits": [
                {
                    "target": {
                        "glyph": glyph,
                        "glyph_id": before.get("glyph_id"),
                        "layer": before.get("layer"),
                        "expected_revision": revision,
                    },
                    "operations": [{"op": "set_width", "width": width}],
                }
            ],
        }
        for field in ("glyph_id", "layer"):
            if not isinstance(request["edits"][0]["target"][field], str):
                raise RuntimeError(f"read_before did not return {field}")
        prepared_digest = sha256_json(request)
        before_state = None
        if self.fixture is not None:
            before_state = self.fixture.control("state")
            self.record("fixture_state_before_apply", {"control": "state"}, before_state)
        summary["capabilities"]["prepare_guarded_apply"] = {
            "status": "pass",
            "document_epoch": epoch,
            "source_id": source,
            "context_revision": context.get("context_revision"),
            "before_revision": revision,
            "before_advance": before.get("advance"),
            "operation_key": operation_key,
            "prepared_request_sha256": prepared_digest,
            "fixture_unsaved_advance": (
                self.fixture_readiness.get("unsaved_advance")
                if self.fixture_readiness is not None
                else None
            ),
            "prepared": True,
        }

        if apply:
            unauthorized = json.loads(json.dumps(request))
            unauthorized["operation_key"] = f"{operation_key}-unauthorized"
            unauthorized["authorization"] = "not-user-approved"
            authorization_result = self.call("authorization_guard", "agent_apply", unauthorized)
            self.require_error_code(authorization_result, "authorization_guard", "authorization_required")
            installed = self.call("authorized_apply", "agent_apply", request)
            self.require_ok(installed, "authorized_apply")
            self.check_epoch(installed, epoch, "authorized_apply")
            receipt = installed.get("receipt")
            if not isinstance(receipt, dict):
                raise RuntimeError("authorized_apply: response did not contain a receipt")
            if receipt.get("outcome", {}).get("status") not in {"committed", "unchanged"}:
                raise RuntimeError("authorized_apply: receipt did not record a successful outcome")
            retry = self.call("exact_retry", "agent_apply", request)
            self.require_ok(retry, "exact_retry")
            self.check_epoch(retry, epoch, "exact_retry")
            if retry.get("replayed") is not True or retry.get("root_changed") is not False:
                raise RuntimeError("exact_retry: operation was not replayed without a second root change")
            if retry.get("receipt") != receipt:
                raise RuntimeError("exact_retry: receipt changed across an exact retry")
            looked_up = self.call(
                "receipt_lookup",
                "agent_receipt",
                {"expected_document_epoch": epoch, "actor": actor, "operation_key": operation_key},
            )
            self.require_ok(looked_up, "receipt_lookup")
            self.check_epoch(looked_up, epoch, "receipt_lookup")
            if looked_up.get("receipt") != receipt or looked_up.get("history_state") != "applied":
                raise RuntimeError("receipt_lookup: receipt or applied history state changed")
            after = self.call("read_after_apply", "read_glyph", {"glyph": glyph, **source_guard})
            self.require_ok(after, "read_after_apply")
            self.check_epoch(after, epoch, "read_after_apply")
            if after.get("source_id") != source:
                raise RuntimeError("read_after_apply: returned source does not match the bound source")
            if after.get("advance") != width:
                raise RuntimeError("read_after_apply: advance does not match requested width")
            stale = json.loads(json.dumps(request))
            stale["operation_key"] = f"{operation_key}-stale"
            stale["edits"][0]["operations"][0]["width"] = width + 1
            stale_result = self.call("stale_write", "agent_apply", stale)
            self.require_rejected_receipt(stale_result, "stale_write", "guarded layer changed")
            if before_state is not None:
                self.run_fixture_controls(before_state, before.get("advance"), after.get("advance"))
                summary["capabilities"]["ordinary_undo_redo_and_source_path_absence"] = self.fixture_summary
            undone = self.call(
                "targeted_undo",
                "agent_history",
                {
                    "expected_document_epoch": epoch,
                    "actor": actor,
                    "operation_key": operation_key,
                    "authorization": "user-approved",
                    "direction": "undo",
                },
            )
            self.require_ok(undone, "targeted_undo")
            self.check_epoch(undone, epoch, "targeted_undo")
            if undone.get("history_state") != "undone":
                raise RuntimeError("targeted_undo: receipt history state is not undone")
            after_undo = self.call(
                "receipt_after_undo",
                "agent_receipt",
                {"expected_document_epoch": epoch, "actor": actor, "operation_key": operation_key},
            )
            self.require_ok(after_undo, "receipt_after_undo")
            self.check_epoch(after_undo, epoch, "receipt_after_undo")
            if after_undo.get("history_state") != "undone":
                raise RuntimeError("receipt_after_undo: history state is not undone")
            if after_undo.get("receipt") != receipt:
                raise RuntimeError("receipt_after_undo: immutable receipt changed after undo")
            restored = self.call(
                "read_after_targeted_undo",
                "read_glyph",
                {"glyph": glyph, **source_guard},
            )
            self.require_ok(restored, "read_after_targeted_undo")
            self.check_epoch(restored, epoch, "read_after_targeted_undo")
            if restored.get("source_id") != source or restored.get("advance") != before.get("advance"):
                raise RuntimeError("read_after_targeted_undo: original glyph width was not restored")
            if self.fixture is not None:
                targeted_undo_state = self.fixture.control("state")
                self.record(
                    "fixture_state_after_targeted_undo",
                    {"control": "state"},
                    targeted_undo_state,
                )
                self.require_ok(targeted_undo_state, "targeted_undo_state")
                if targeted_undo_state.get("source_exists") is not False:
                    raise RuntimeError("targeted_undo_state: fixture source must remain unwritten")
                if not all(
                    targeted_undo_state.get(field) == before.get("advance")
                    for field in ("canonical_advance", "cache_advance", "session_advance")
                ):
                    raise RuntimeError("targeted_undo_state: canonical/cache/session advances disagree")
            summary["capabilities"]["apply_retry_and_readback"] = {
                "status": "pass",
                "receipt_backed": True,
                "authorization_rejected": True,
                "stale_revision_rejected": True,
                "exact_retry_replayed": True,
                "root_changed_on_retry": False,
                "after_revision": after.get("revision"),
                "after_advance": after.get("advance"),
                "targeted_undo": True,
                "restored_advance": restored.get("advance"),
            }
            summary["capabilities"]["receipt_lookup_and_targeted_undo"] = {
                "status": "pass",
                "receipt_outcome": receipt.get("outcome", {}).get("status"),
                "history_state_after_undo": after_undo.get("history_state"),
            }
        else:
            summary["capabilities"]["apply_retry_and_readback"] = {
                "status": "not_tested",
                "reason": "pass --apply against a disposable application fixture to apply, retry, receipt-check and undo",
            }
        return summary

    def write_report(self, summary: dict[str, Any], client_results: dict[str, Any]) -> None:
        report = {
            "harness": "runebender-agent-client-harness/1",
            "binary": redact_text(str(self.binary)),
            "binary_sha256": sha256_file(self.binary),
            "session": redact_text(str(self.session)),
            "fixture_commit": self.fixture_commit,
            "clients": client_results,
            "summary": redact(summary),
            "transcript": self.transcript,
        }
        (self.output_dir / "report.json").write_text(
            json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, help="Explicit Runebender executable")
    endpoint = parser.add_mutually_exclusive_group()
    endpoint.add_argument("--session", type=Path, help="Explicit live Unix socket")
    endpoint.add_argument("--fixture", action="store_true", help="Own the unpublished application fixture")
    parser.add_argument("--fixture-duration-seconds", type=int, default=300, help="Fixture lifetime (1..3600 seconds)")
    parser.add_argument("--fixture-commit", help="Fixture source commit for evidence provenance")
    parser.add_argument("--output-dir", type=Path, help="New directory for redacted evidence")
    parser.add_argument("--glyph", help="Fixture glyph for the bounded edit")
    parser.add_argument("--width", type=float, default=760, help="Proposed fixture width")
    parser.add_argument("--apply", action="store_true", help="Perform the authorized apply on the disposable fixture")
    parser.add_argument("--probe-only", action="store_true", help="Only inspect client commands and executable hashes")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    clients = probe_clients()
    if args.probe_only:
        print(json.dumps({"clients": clients}, indent=2, sort_keys=True))
        return 0
    if args.binary is None or args.output_dir is None or (args.session is None and not args.fixture):
        print("--binary, --session/--fixture and --output-dir are required unless --probe-only is used", file=sys.stderr)
        return 2
    if args.fixture and not 1 <= args.fixture_duration_seconds <= 3600:
        print("--fixture-duration-seconds must be between 1 and 3600", file=sys.stderr)
        return 2
    if not args.binary.is_file():
        print(f"binary is not a file: {args.binary}", file=sys.stderr)
        return 2
    if args.session is not None and not args.session.exists():
        print(f"session endpoint does not exist: {args.session}", file=sys.stderr)
        return 2
    if not args.glyph:
        print("--glyph is required unless --probe-only is used", file=sys.stderr)
        return 2
    if args.output_dir.exists():
        print(f"output directory already exists: {args.output_dir}", file=sys.stderr)
        return 2
    args.output_dir.mkdir(parents=True)
    fixture = FixtureProcess(args.binary, args.fixture_duration_seconds) if args.fixture else None
    try:
        readiness = fixture.start() if fixture is not None else None
        session = Path(readiness["session"]) if readiness is not None else args.session
        assert session is not None
        harness = Harness(
            args.binary,
            session,
            args.output_dir,
            fixture,
            args.fixture_commit,
            readiness,
        )
        if readiness is not None:
            harness.record("fixture_ready", {"control": "ready"}, readiness)
        summary = harness.run_scenario(args.glyph, args.width, args.apply)
    except (OSError, RuntimeError) as error:
        if "harness" in locals():
            harness.write_report({"status": "fail", "error": str(error)}, clients)
        print(f"harness failed: {error}", file=sys.stderr)
        if fixture is not None:
            fixture.stop()
        return 4
    if fixture is not None:
        shutdown = fixture.stop()
        harness.record("fixture_shutdown", {"control": "shutdown"}, shutdown)
    harness.write_report(summary, clients)
    print(json.dumps({"ok": True, "report": str(args.output_dir / "report.json"), "summary": summary}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
