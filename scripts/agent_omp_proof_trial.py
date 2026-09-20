#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Run one bounded OMP receipt/edit and compiled-PNG image trial.

The trial owns a disposable UFO and a file-backed headless Workspace.
It never opens or edits a user font, changes OMP configuration, saves the UFO,
or records model reasoning or credentials.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
from pathlib import Path
import random
import re
import secrets
import selectors
import shutil
import subprocess
import sys
from typing import Any
import zlib

MODEL = "openai-codex/gpt-5.6-luna"
EDIT_GLYPH = "A"
EDIT_WIDTH = 630.0
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
ALLOWED_TOOLS = {
    "editor_connect",
    "project_info",
    "editor_context",
    "glyph_inventory",
    "read_glyph",
    "agent_apply",
    "agent_receipt",
    "agent_cancel",
    "proof_start",
    "proof_status",
    "proof_release",
}


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def manifest(root: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        result[str(path.relative_to(root))] = sha256_file(path)
    return result


def redact_text(text: str, root: Path, session: str | None = None) -> str:
    text = text.replace(str(Path.home()), "<home>")
    text = text.replace(str(root), "<trial>")
    if session:
        text = text.replace(session, "<session>")
    return text


def redact(value: Any, root: Path, session: str | None = None, key: str = "") -> Any:
    lowered = key.lower()
    if lowered in {
        "authorization",
        "token",
        "password",
        "secret",
        "credential",
        "api_key",
    }:
        return "<redacted>"
    if isinstance(value, dict):
        return {name: redact(item, root, session, name) for name, item in value.items()}
    if isinstance(value, list):
        return [redact(item, root, session, key) for item in value]
    if isinstance(value, str):
        return redact_text(value, root, session)
    return value


def plist(body: str) -> str:
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0"><dict>'
        + body
        + "</dict></plist>\n"
    )


def write_fixture(root: Path, seed: int) -> tuple[Path, str, str, str]:
    """Create a minimal disposable UFO with an ordinary edit glyph and opaque PUA marker."""

    rng = random.Random(seed)
    marker_name = f"q{secrets.token_hex(8)}"
    marker_unicode = 0xE000 + rng.randrange(0x100)
    ufo = root / "OpaqueProof.ufo"
    glyphs = ufo / "glyphs"
    glyphs.mkdir(parents=True)
    (ufo / "metainfo.plist").write_text(
        plist(
            "<key>creator</key><string>runebender-omp-proof-trial</string><key>formatVersion</key><integer>3</integer>"
        ),
        encoding="utf-8",
    )
    (ufo / "fontinfo.plist").write_text(
        plist(
            "<key>familyName</key><string>Disposable Proof Trial</string>"
            "<key>styleName</key><string>Regular</string>"
            "<key>unitsPerEm</key><integer>1000</integer>"
            "<key>ascender</key><integer>800</integer>"
            "<key>descender</key><integer>-200</integer>"
        ),
        encoding="utf-8",
    )
    (ufo / "layercontents.plist").write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n'
        '<plist version="1.0"><array><array><string>public.default</string><string>glyphs</string></array></array></plist>\n',
        encoding="utf-8",
    )
    (glyphs / "contents.plist").write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n'
        '<plist version="1.0"><dict><key>.notdef</key><string>notdef.glif</string>'
        f"<key>A</key><string>A.glif</string><key>{marker_name}</key>"
        f"<string>{marker_name}.glif</string></dict></plist>\n",
        encoding="utf-8",
    )
    (glyphs / "notdef.glif").write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<glyph name=".notdef" format="2"><advance width="600"/>'
        '<outline><contour><point x="80" y="0" type="line"/>'
        '<point x="520" y="0" type="line"/><point x="520" y="700" type="line"/>'
        '<point x="80" y="700" type="line"/></contour></outline></glyph>\n',
        encoding="utf-8",
    )
    (glyphs / "A.glif").write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<glyph name="A" format="2"><unicode hex="0041"/><advance width="600"/>'
        '<outline><contour><point x="80" y="0" type="line"/>'
        '<point x="520" y="0" type="line"/><point x="520" y="700" type="line"/>'
        '<point x="80" y="700" type="line"/></contour></outline></glyph>\n',
        encoding="utf-8",
    )
    marker_shape = "diamond" if rng.randrange(2) == 0 else "rounded"
    jitter = rng.randrange(12)
    if marker_shape == "diamond":
        points = [
            (300, 760 + jitter),
            (520 + jitter, 500),
            (300, 0),
            (80 - jitter, 500),
        ]
    else:
        points = [
            (300, 760 + jitter),
            (384, 740),
            (456, 676),
            (508, 592),
            (520 + jitter, 500),
            (508, 408),
            (456, 324),
            (384, 260),
            (300, 240 - jitter),
            (216, 260),
            (144, 324),
            (92, 408),
            (80 - jitter, 500),
            (92, 592),
            (144, 676),
            (216, 740),
        ]
    point_xml = [f'<point x="{x}" y="{y}" type="line"/>' for x, y in points]
    (glyphs / f"{marker_name}.glif").write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        f'<glyph name="{marker_name}" format="2"><unicode hex="{marker_unicode:04X}"/>'
        '<advance width="600"/><outline><contour>'
        + "".join(point_xml)
        + "</contour></outline></glyph>\n",
        encoding="utf-8",
    )
    return ufo, marker_name, chr(marker_unicode), marker_shape


class Host:
    def __init__(self, binary: Path, font: Path, glyph: str, duration: int) -> None:
        self.process = subprocess.Popen(
            [
                str(binary),
                "agent",
                "serve",
                "--font",
                str(font),
                "--glyph",
                glyph,
                "--duration-seconds",
                str(duration),
                "--json",
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )

    def read(self, timeout: float = 20) -> dict[str, Any]:
        if self.process.stdout is None:
            raise RuntimeError("headless host has no stdout")
        selector = selectors.DefaultSelector()
        selector.register(self.process.stdout, selectors.EVENT_READ)
        try:
            if not selector.select(timeout):
                raise RuntimeError("headless host did not return JSON before timeout")
            line = self.process.stdout.readline()
        finally:
            selector.close()
        try:
            value = json.loads(line)
        except json.JSONDecodeError as error:
            raise RuntimeError("headless host returned non-JSON output") from error
        if not isinstance(value, dict):
            raise RuntimeError("headless host response was not an object")
        return value

    def control(self, action: str) -> dict[str, Any]:
        if self.process.stdin is None:
            raise RuntimeError("headless host has no stdin")
        self.process.stdin.write(json.dumps({"action": action}) + "\n")
        self.process.stdin.flush()
        return self.read()

    def stop(self) -> None:
        try:
            if self.process.poll() is None:
                try:
                    self.control("shutdown")
                except (OSError, RuntimeError):
                    self.process.terminate()
                self.process.wait(timeout=15)
        finally:
            if self.process.poll() is None:
                self.process.kill()
                self.process.wait(timeout=5)


def call_omp(
    omp: Path, root: Path, prompt: str, max_time: int
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            str(omp),
            "--model",
            MODEL,
            "--cwd",
            str(root),
            "--no-session",
            "--no-tools",
            "--no-lsp",
            "--no-pty",
            "--no-extensions",
            "--no-skills",
            "--no-rules",
            "--no-title",
            "--max-time",
            str(max_time),
            "--mode",
            "json",
            "--print",
            prompt,
        ],
        cwd=root,
        capture_output=True,
        text=True,
        timeout=max_time + 30,
        check=False,
    )


def json_candidates(value: Any) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    if isinstance(value, dict):
        result.append(value)
        for item in value.values():
            result.extend(json_candidates(item))
    elif isinstance(value, list):
        for item in value:
            result.extend(json_candidates(item))
    elif isinstance(value, str) and value.lstrip().startswith("{"):
        try:
            parsed = json.loads(value)
        except json.JSONDecodeError:
            return result
        result.extend(json_candidates(parsed))
    return result


def image_candidates(value: Any) -> list[bytes]:
    result: list[bytes] = []
    if isinstance(value, dict):
        if (
            value.get("type") == "image"
            and value.get("mimeType") == "image/png"
            and isinstance(value.get("data"), str)
        ):
            try:
                result.append(base64.b64decode(value["data"], validate=True))
            except (ValueError, base64.binascii.Error):
                pass
        for item in value.values():
            result.extend(image_candidates(item))
    elif isinstance(value, list):
        for item in value:
            result.extend(image_candidates(item))
    return result


def is_png(data: bytes) -> bool:
    if not data.startswith(PNG_SIGNATURE):
        return False
    offset = len(PNG_SIGNATURE)
    first = True
    while offset + 12 <= len(data):
        length = int.from_bytes(data[offset : offset + 4], "big")
        chunk_end = offset + 12 + length
        if chunk_end > len(data):
            return False
        chunk_type = data[offset + 4 : offset + 8]
        chunk_data = data[offset + 8 : offset + 8 + length]
        expected_crc = int.from_bytes(data[offset + 8 + length : chunk_end], "big")
        if zlib.crc32(chunk_type + chunk_data) & 0xFFFFFFFF != expected_crc:
            return False
        if first and (chunk_type != b"IHDR" or length != 13):
            return False
        first = False
        offset = chunk_end
        if chunk_type == b"IEND":
            return length == 0 and offset == len(data)
    return False


def tool_kind(name: Any) -> str:
    wire_name = name if isinstance(name, str) else ""
    for candidate in ALLOWED_TOOLS:
        if wire_name == candidate or wire_name.endswith(f"_{candidate}"):
            return candidate
    return wire_name


def tool_result(value: Any) -> dict[str, Any]:
    candidates = json_candidates(value)
    for candidate in candidates:
        if "ok" in candidate:
            return candidate
    return {}


def summarize_tool_result(value: Any, root: Path, session: str) -> dict[str, Any]:
    selected: dict[str, Any] = {}
    candidate = tool_result(value)
    for key in (
        "status",
        "proof_id",
        "document_epoch",
        "document_revision",
        "captured_document_epoch",
        "captured_document_revision",
        "current",
        "stale",
        "font_sha256",
        "canonical_input_sha256",
        "compiler",
        "recipe",
        "glyphs",
        "history_state",
        "replayed",
        "root_changed",
        "saved",
        "error_code",
        "receipt",
        "cancellation_status",
        "released",
        "ok",
    ):
        if key in candidate:
            selected[key] = redact(candidate[key], root, session, key)
    return selected


def parse_transcript(stdout: str, root: Path, session: str) -> dict[str, Any]:
    reviewed: list[dict[str, Any]] = []
    calls: list[dict[str, Any]] = []
    starts: dict[str, dict[str, Any]] = {}
    assistant: list[dict[str, Any]] = []
    proof_image_seen = False
    for line in stdout.splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        kind = event.get("type")
        if kind == "tool_execution_start":
            call_id = event.get("toolCallId")
            name = event.get("toolName") or event.get("tool")
            start = {
                "tool": tool_kind(name),
                "wire_tool": name,
                "args": event.get("args", {}),
            }
            if isinstance(call_id, str):
                starts[call_id] = start
            reviewed.append(
                {
                    "type": kind,
                    "tool": name,
                    "args": redact(event.get("args", {}), root, session),
                }
            )
        elif kind == "tool_execution_end":
            call_id = event.get("toolCallId")
            name = event.get("toolName") or event.get("tool")
            start = starts.pop(call_id, {}) if isinstance(call_id, str) else {}
            images = image_candidates(event.get("result"))
            call = {
                "tool": start.get("tool", tool_kind(name)),
                "wire_tool": start.get("wire_tool", name),
                "args": start.get("args", {}),
                "result": tool_result(event.get("result")),
                "images": images,
                "is_error": event.get("isError") is True,
            }
            calls.append(call)
            if call["tool"] == "proof_status" and any(
                is_png(image) for image in images
            ):
                proof_image_seen = True
            reviewed.append(
                {
                    "type": kind,
                    "tool": name,
                    "result": summarize_tool_result(event.get("result"), root, session),
                    "image_block": bool(images),
                    "valid_png": any(is_png(image) for image in images),
                }
            )
        elif kind == "message_end":
            message = event.get("message")
            if not isinstance(message, dict) or message.get("role") != "assistant":
                continue
            texts = [
                item.get("text", "")
                for item in message.get("content", [])
                if isinstance(item, dict) and item.get("type") == "text"
            ]
            if texts:
                text_value = redact_text("\n".join(texts), root, session)
                item = {"text": text_value, "after_proof_image": proof_image_seen}
                assistant.append(item)
                reviewed.append({"type": "assistant_text", **item})
    return {"reviewed": reviewed, "calls": calls, "assistant": assistant}


def normalized_args(call: dict[str, Any]) -> dict[str, Any]:
    args = call.get("args")
    if not isinstance(args, dict):
        return {}
    result = args.copy()
    # OMP adds this presentation-only intent field outside the MCP input schema.
    result.pop("i", None)
    return result


def receipt(call: dict[str, Any]) -> dict[str, Any]:
    result = call.get("result")
    value = result.get("receipt") if isinstance(result, dict) else None
    return value if isinstance(value, dict) else {}


def is_sha256(value: Any) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def classify_marker(text: str) -> str:
    matches = re.findall(
        r"(?im)^\s*MARKER_CLASSIFICATION:\s*(diamond|rounded)\s*$", text
    )
    return matches[-1].lower() if matches else "unknown"


def evaluate_trial(
    evidence: dict[str, Any],
    state: dict[str, Any],
    source_unchanged: bool,
    returncode: int,
    marker_name: str,
    marker_text: str,
    expected_marker: str,
) -> dict[str, Any]:
    calls = evidence["calls"]
    by_tool = {
        tool: [call for call in calls if call.get("tool") == tool]
        for tool in ALLOWED_TOOLS
    }
    apply_calls = by_tool["agent_apply"]
    receipt_calls = by_tool["agent_receipt"]
    cancel_calls = by_tool["agent_cancel"]
    read_calls = by_tool["read_glyph"]
    proof_start_calls = by_tool["proof_start"]
    proof_status_calls = by_tool["proof_status"]
    proof_release_calls = by_tool["proof_release"]
    inventory_calls = by_tool["glyph_inventory"]

    first_receipt = receipt(apply_calls[0]) if len(apply_calls) == 2 else {}
    retry_receipt = receipt(apply_calls[1]) if len(apply_calls) == 2 else {}
    looked_up_receipt = receipt(receipt_calls[0]) if len(receipt_calls) == 1 else {}
    first_result = apply_calls[0].get("result", {}) if len(apply_calls) == 2 else {}
    retry_result = apply_calls[1].get("result", {}) if len(apply_calls) == 2 else {}
    receipt_outcome = first_receipt.get("outcome", {})
    exact_apply = normalized_args(apply_calls[0]) if len(apply_calls) == 2 else {}
    exact_identity = {
        key: exact_apply.get(key)
        for key in ("expected_document_epoch", "actor", "operation_key")
    }
    edits = exact_apply.get("edits")
    operations = (
        edits[0].get("operations") if isinstance(edits, list) and edits else None
    )
    target = edits[0].get("target", {}) if isinstance(edits, list) and edits else {}
    expected_edit = (
        exact_apply.get("actor") == "omp-proof-trial"
        and exact_apply.get("operation_key") == "omp-edit-01"
        and exact_apply.get("authorization") == "user-approved"
        and exact_apply.get("source") == 0
        and target.get("glyph") == EDIT_GLYPH
        and target.get("layer") == "public.default"
        and isinstance(target.get("glyph_id"), str)
        and bool(target.get("glyph_id"))
        and isinstance(target.get("expected_revision"), str)
        and bool(target.get("expected_revision"))
        and operations == [{"op": "set_width", "width": EDIT_WIDTH}]
    )
    exact_retry = bool(
        len(apply_calls) == 2
        and exact_apply == normalized_args(apply_calls[1])
        and first_result.get("replayed") is False
        and first_result.get("ok") is True
        and first_result.get("root_changed") is True
        and retry_result.get("replayed") is True
        and retry_result.get("ok") is True
        and retry_result.get("root_changed") is False
        and receipt_calls[0].get("result", {}).get("ok") is True
        and first_receipt
        and first_receipt == retry_receipt == looked_up_receipt
        and first_receipt.get("document_epoch")
        == exact_identity.get("expected_document_epoch")
        and first_receipt.get("actor") == exact_identity.get("actor")
        and first_receipt.get("operation_key") == exact_identity.get("operation_key")
        and is_sha256(first_receipt.get("payload_sha256"))
        and receipt_outcome.get("status") == "committed"
    )
    receipt_identity = (
        len(receipt_calls) == 1 and normalized_args(receipt_calls[0]) == exact_identity
    )
    committed_cancel = (
        len(cancel_calls) == 1
        and normalized_args(cancel_calls[0]) == exact_identity
        and cancel_calls[0].get("result", {}).get("ok") is False
        and cancel_calls[0].get("is_error") is True
        and cancel_calls[0].get("result", {}).get("cancellation_status") == "committed"
    )
    host_state = (
        state.get("ok") is True
        and state.get("canonical_advance") == EDIT_WIDTH
        and state.get("cache_advance") == EDIT_WIDTH
        and state.get("session_advance") == EDIT_WIDTH
        and state.get("document_revision") == receipt_outcome.get("after_revision")
    )

    completed_proof = next(
        (
            call
            for call in proof_status_calls
            if call.get("result", {}).get("status") == "completed"
            and any(is_png(image) for image in call.get("images", []))
        ),
        None,
    )
    proof_result = completed_proof.get("result", {}) if completed_proof else {}
    proof_recipe = proof_result.get("recipe", {})
    proof_start_result = (
        proof_start_calls[0].get("result", {}) if len(proof_start_calls) == 1 else {}
    )
    proof_start_args = (
        normalized_args(proof_start_calls[0]) if len(proof_start_calls) == 1 else {}
    )
    proof_id = proof_start_result.get("proof_id")
    proof_lineage = bool(
        len(proof_start_calls) == 1
        and completed_proof
        and proof_start_result.get("ok") is True
        and proof_start_args.get("expected_document_epoch")
        == exact_identity.get("expected_document_epoch")
        and proof_start_args.get("expected_document_revision")
        == state.get("document_revision")
        and proof_start_args.get("operation_key") == "omp-proof-01"
        and proof_start_args.get("recipe") == proof_recipe
        and normalized_args(completed_proof).get("proof_id") == proof_id
        and normalized_args(completed_proof).get("include_image") is True
        and proof_result.get("ok") is True
        and proof_result.get("proof_id") == proof_id
        and proof_result.get("captured_document_epoch")
        == exact_identity.get("expected_document_epoch")
        and proof_result.get("current") is True
        and proof_result.get("stale") is False
        and proof_result.get("captured_document_revision")
        == state.get("document_revision")
        and is_sha256(proof_result.get("font_sha256"))
        and is_sha256(proof_result.get("canonical_input_sha256"))
        and isinstance(proof_recipe, dict)
        and proof_recipe.get("text") == marker_text
    )
    released_proof = (
        len(proof_release_calls) == 1
        and normalized_args(proof_release_calls[0]).get("proof_id") == proof_id
        and proof_release_calls[0].get("result", {}).get("ok") is True
        and proof_release_calls[0].get("result", {}).get("released") is True
    )
    initial_a_read = next(
        (
            call
            for call in read_calls
            if normalized_args(call).get("glyph") == EDIT_GLYPH
            and call.get("result", {}).get("advance") == 600.0
        ),
        None,
    )
    marker_was_not_read = bool(
        initial_a_read
        and all(
            normalized_args(call).get("glyph") != marker_name for call in read_calls
        )
    )
    marker_codepoint = ord(marker_text)
    marker_discovered = any(
        any(
            glyph.get("glyph") == marker_name
            and marker_codepoint in glyph.get("codepoints", [])
            for glyph in call.get("result", {}).get("glyphs", [])
            if isinstance(glyph, dict)
        )
        for call in inventory_calls
    )
    only_allowed_tools = all(call.get("tool") in ALLOWED_TOOLS for call in calls)
    required_context = all(
        by_tool[tool] for tool in ("editor_connect", "project_info", "editor_context")
    )
    protocol_statuses_expected = all(
        (
            call.get("tool") == "agent_cancel"
            and call.get("is_error") is True
            and call.get("result", {}).get("ok") is False
            and call.get("result", {}).get("cancellation_status") == "committed"
        )
        or (
            call.get("tool") != "agent_cancel"
            and call.get("is_error") is False
            and call.get("result", {}).get("ok") is True
        )
        for call in calls
    )

    post_image_text = [
        item["text"] for item in evidence["assistant"] if item.get("after_proof_image")
    ]
    final_text = post_image_text[-1] if post_image_text else ""
    interpretation = classify_marker(final_text)
    checks = {
        "client_completed": returncode == 0,
        "only_allowed_tools": only_allowed_tools,
        "required_context_reads": required_context,
        "protocol_statuses_expected": protocol_statuses_expected,
        "expected_edit_payload": expected_edit,
        "exact_apply_retry": exact_retry,
        "receipt_identity": receipt_identity,
        "committed_cancel_did_not_undo": committed_cancel and host_state,
        "host_state_matches_receipt": host_state,
        "source_manifest_unchanged": source_unchanged,
        "marker_discovered_by_inventory": marker_discovered,
        "marker_not_read": marker_was_not_read,
        "compiled_proof_lineage": proof_lineage,
        "proof_released": released_proof,
        "assistant_after_proof_image": bool(post_image_text),
        "visual_classification": interpretation == expected_marker,
    }
    proof_images = completed_proof.get("images", []) if completed_proof else []
    return {
        "passed": all(checks.values()),
        "checks": checks,
        "final_text": final_text,
        "interpretation": interpretation,
        "proof_result": proof_result,
        "proof_image": proof_images[-1] if proof_images else None,
        "tool_names": [call.get("wire_tool", "") for call in calls],
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--omp", type=Path, default=Path(shutil.which("omp") or ""))
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--duration-seconds", type=int, default=300)
    parser.add_argument("--max-time", type=int, default=180)
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if not args.binary.is_file() or not args.binary.is_absolute():
        raise SystemExit("--binary must be an absolute executable path")
    if not args.omp.is_file() or not args.omp.is_absolute():
        raise SystemExit("--omp must be an absolute executable path")
    if args.output_dir.exists():
        raise SystemExit(f"output directory already exists: {args.output_dir}")
    args.output_dir.mkdir(parents=True)
    seed = secrets.randbits(64)
    font, marker_name, marker_text, expected_marker = write_fixture(
        args.output_dir, seed
    )
    before_manifest = manifest(font)
    config_dir = args.output_dir / ".omp"
    config_dir.mkdir()
    config = config_dir / "mcp.json"
    config.write_text(
        json.dumps(
            {
                "mcpServers": {
                    "runebender": {
                        "type": "stdio",
                        "command": str(args.binary),
                        "args": ["mcp", "--live"],
                    }
                }
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    host = Host(args.binary, font, EDIT_GLYPH, args.duration_seconds)
    ready: dict[str, Any] | None = None
    try:
        ready = host.read()
        if ready.get("ok") is not True or not isinstance(ready.get("session"), str):
            raise RuntimeError(f"headless host was not ready: {ready}")
        session = ready["session"]
        prompt = (
            "This is a bounded, already-authorized Runebender MCP trial on a disposable font. "
            f"Connect to exactly {session} using editor_connect. Use only Runebender MCP tools; "
            "do not use filesystem tools, save, export, legacy proof, or read the marker glyph with read_glyph. "
            "First call project_info, editor_context, and glyph_inventory with the returned exact document epoch. "
            "The fixture contains an ordinary glyph named A and one private-use Unicode marker glyph. "
            "For the ordinary A glyph, read it, then apply one set_width operation to 630 font units using "
            "agent_apply with actor omp-proof-trial, operation_key omp-edit-01, authorization user-approved, "
            "source 0, a history name, and the exact glyph/layer/revision identities from read_glyph. "
            "Call agent_receipt, then retry the identical complete agent_apply request once. "
            "Call agent_cancel with the same identity after the committed retry and verify that it reports committed; "
            "this cancellation must not undo the edit. "
            "After that, use glyph_inventory's private-use codepoint to construct proof text for the marker glyph. "
            "Do not infer or report its shape from text metadata. Start proof_start with the current exact epoch and "
            "document revision, operation_key omp-proof-01, normalized_location [], right_to_left false, features [], "
            "and null script/language. Poll proof_status with include_image=true until completed, then release the "
            "terminal proof with proof_release. "
            "After the image block is delivered, describe only what the marker silhouette visibly resembles: "
            "a pointed diamond or a rounded form. This visual classification is the only marker judgment requested. "
            "Include exactly one separate final line, MARKER_CLASSIFICATION: diamond or "
            "MARKER_CLASSIFICATION: rounded, based only on that image. "
            "Report the proof font hash, proof status, image receipt, the A before/after widths, the retry result, "
            "and the post-commit cancellation result."
        )
        result = call_omp(args.omp, args.output_dir, prompt, args.max_time)
        evidence = parse_transcript(result.stdout, args.output_dir, session)
        state = host.control("state")
        after_manifest = manifest(font)
        evaluation = evaluate_trial(
            evidence,
            state,
            before_manifest == after_manifest,
            result.returncode,
            marker_name,
            marker_text,
            expected_marker,
        )
        image = evaluation["proof_image"]
        image_path = None
        if image is not None:
            image_path = args.output_dir / "compiled-proof.png"
            image_path.write_bytes(image)
        report = {
            "status": "pass" if evaluation["passed"] else "fail",
            "model_trial": True,
            "model": MODEL,
            "client": {
                "path": redact_text(str(args.omp), args.output_dir),
                "sha256": sha256_file(args.omp),
            },
            "binary": {
                "path": redact_text(str(args.binary), args.output_dir),
                "sha256": sha256_file(args.binary),
            },
            "config_sha256": sha256_file(config),
            "seed": seed,
            "marker": {
                "glyph_name": marker_name,
                "unicode": f"U+{ord(marker_text):04X}",
                "ground_truth": expected_marker,
            },
            "fixture_ready": redact(ready, args.output_dir, session),
            "source_manifest_before": before_manifest,
            "source_manifest_after": after_manifest,
            "source_manifest_unchanged": before_manifest == after_manifest,
            "fixture_state_after_model": redact(state, args.output_dir, session),
            "checks": evaluation["checks"],
            "proof_metadata": redact(
                evaluation["proof_result"], args.output_dir, session
            ),
            "image": {
                "path": (
                    redact_text(str(image_path), args.output_dir)
                    if image_path
                    else None
                ),
                "sha256": sha256_file(image_path) if image_path else None,
                "valid_png_in_proof_status_result": image_path is not None,
                "assistant_response_after_image_block": evaluation["checks"][
                    "assistant_after_proof_image"
                ],
            },
            "model_interpretation": evaluation["interpretation"],
            "model_final_text": evaluation["final_text"],
            "tool_names": evaluation["tool_names"],
            "client_returncode": result.returncode,
            "stderr_sha256": sha256_bytes(result.stderr.encode()),
        }
        (args.output_dir / "reviewed-transcript.json").write_text(
            json.dumps({"events": evidence["reviewed"]}, indent=2, sort_keys=True)
            + "\n",
            encoding="utf-8",
        )
        (args.output_dir / "report.json").write_text(
            json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        print(json.dumps(report, indent=2, sort_keys=True))
        return 0 if report["status"] == "pass" else 4
    except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
        fallback = {
            "status": "blocked",
            "error": redact_text(str(error), args.output_dir),
            "binary_sha256": sha256_file(args.binary),
            "client_sha256": sha256_file(args.omp),
        }
        (args.output_dir / "report.json").write_text(
            json.dumps(fallback, indent=2) + "\n", encoding="utf-8"
        )
        print(json.dumps(fallback, indent=2), file=sys.stderr)
        return 4
    finally:
        host.stop()


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
