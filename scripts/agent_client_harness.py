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
    "propose_edits",
    "proposal_install",
}
PENDING_CAPABILITIES = {
    "receipt_lookup": "No receipt/status tool is present in the checkpoint schema.",
    "compiled_proof": "No compiled-snapshot proof tool is present in the checkpoint schema.",
    "grouped_atomic_apply": "Proposal install reports per-glyph history, not a receipt-backed group.",
    "disconnect_after_commit": "The current CLI cannot inject a response drop after commit.",
    "ui_refresh_and_undo": "The harness endpoint does not expose application UI undo.",
    "cancellation": "No cancellation lifecycle is exposed by the checkpoint schema.",
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

    def write_proof_artifact(self, result: dict[str, Any]) -> None:
        svg = result.get("svg_content")
        if not isinstance(svg, str):
            return
        path = self.output_dir / "live-proof.svg"
        path.write_text(svg, encoding="utf-8")
        self.transcript.append(
            {
                "phase": "proof_artifact",
                "path": redact_text(str(path)),
                "bytes": len(svg.encode()),
                "sha256": sha256_file(path),
                "format": "svg",
                "compiled_snapshot": False,
            }
        )

    def run_fixture_controls(self, before_advance: float, after_advance: float) -> None:
        if self.fixture is None:
            return
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
            "apply_cache_session_match": valid_apply,
            "undo_restored_before": valid_undo,
            "redo_restored_after": valid_redo,
            "source_exists": after_redo.get("source_exists"),
        }

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
        task = f"harness-spacing-{int(time.time())}"
        batch = {
            "task": task,
            "reason": "bounded Milestone 1D harness edit",
            "edits": [
                {
                    "glyph": glyph,
                    "expected_revision": revision,
                    "operations": [{"op": "set_width", "width": width}],
                }
            ],
            **source_guard,
        }
        proposed = self.call("proposal", "propose_edits", batch)
        self.require_ok(proposed, "proposal")
        self.check_epoch(proposed, epoch, "proposal")
        summary["capabilities"]["connect_epoch_context_read_propose"] = {
            "status": "pass",
            "document_epoch": epoch,
            "source_id": source,
            "context_revision": context.get("context_revision"),
            "before_revision": revision,
            "before_advance": before.get("advance"),
            "fixture_unsaved_advance": (
                self.fixture_readiness.get("unsaved_advance")
                if self.fixture_readiness is not None
                else None
            ),
            "proposal_ok": proposed.get("ok") is True,
        }
        proof = self.call("proof_before_apply", "proof", {"glyphs": [glyph], **source_guard})
        self.require_ok(proof, "proof_before_apply")
        self.check_epoch(proof, epoch, "proof_before_apply")
        self.write_proof_artifact(proof)
        summary["capabilities"]["editable_svg_proof"] = {
            "status": "pass" if proof.get("ok") is True else "fail",
            "compiled_snapshot": False,
        }

        if apply:
            unauthorized = self.call(
                "authorization_guard",
                "proposal_install",
                {"task": task, "keep_structure": True, **source_guard},
            )
            self.check_epoch(unauthorized, epoch, "authorization_guard")
            self.require_error_category(
                unauthorized, "authorization_guard", "explicit user authorization required"
            )
            installed = self.call(
                "authorized_apply",
                "proposal_install",
                {
                    "task": task,
                    "keep_structure": True,
                    "authorization": "user-approved",
                    **source_guard,
                },
            )
            self.require_ok(installed, "authorized_apply")
            self.check_epoch(installed, epoch, "authorized_apply")
            after = self.call("read_after_apply", "read_glyph", {"glyph": glyph, **source_guard})
            self.require_ok(after, "read_after_apply")
            self.check_epoch(after, epoch, "read_after_apply")
            if after.get("source_id") != source:
                raise RuntimeError("read_after_apply: returned source does not match the bound source")
            if after.get("advance") != width:
                raise RuntimeError("read_after_apply: advance does not match requested width")
            stale = self.call(
                "stale_write",
                "propose_edits",
                {
                    "task": f"{task}-stale",
                    "reason": "expected stale rejection",
                    "edits": [
                        {
                            "glyph": glyph,
                            "expected_revision": revision,
                            "operations": [{"op": "set_width", "width": width + 1}],
                        }
                    ],
                    **source_guard,
                },
            )
            self.check_epoch(stale, epoch, "stale_write")
            self.require_error_category(stale, "stale_write", "stale revision")
            summary["capabilities"]["authorized_apply_and_stale_write"] = {
                "status": (
                    "pass"
                    if installed.get("ok") is True
                    and "explicit user authorization required" in unauthorized.get("error", "")
                    and "stale revision" in stale.get("error", "")
                    else "fail"
                ),
                "authorization_rejected_without_grant": True,
                "authorization_error_category": "explicit user authorization required",
                "after_revision": after.get("revision"),
                "after_advance": after.get("advance"),
                "stale_rejected": True,
                "stale_error_category": "stale revision",
            }
            self.run_fixture_controls(before.get("advance"), after.get("advance"))
            if self.fixture is not None:
                summary["capabilities"]["ui_refresh_and_undo"] = self.fixture_summary
        else:
            summary["capabilities"]["authorized_apply_and_stale_write"] = {
                "status": "not_tested",
                "reason": "pass --apply against a disposable application fixture to mutate",
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
