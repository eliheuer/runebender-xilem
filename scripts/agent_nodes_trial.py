#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Run a credential-free stdio MCP trial of the native live Nodes tools.

The trial creates and owns a disposable two-glyph UFO, opens it in the native
headless host, and talks to that host only through an actual ``mcp --live``
stdio process plus the host's ordinary undo control. It never saves the UFO or
uses an external model.
"""

from __future__ import annotations

import argparse
import base64
import binascii
import hashlib
import json
from pathlib import Path
import selectors
import subprocess
import sys
import tempfile
import time
from typing import Any, BinaryIO
import zlib

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
NODE_TOOLS = {
    "nodes_discover",
    "nodes_snapshot",
    "nodes_mutate",
    "nodes_run",
    "nodes_status",
    "nodes_cancel",
    "nodes_release",
    "nodes_apply",
    "nodes_image",
}
ACTOR = "nodes-stdio-trial"
INITIAL_WIDTH = 400.0
CHANGED_WIDTH = 500.0
SCRIPT = """import json, sys
p=json.load(sys.stdin)
edits=[{"target":layer["guard"],"operations":[{"op":"set_width","width":layer["width"]+100}]} for layer in p["layers"]]
json.dump({"schema_version":1,"job_id":p["job_id"],"input_hash":p["input_hash"],"report":"Increase selected widths by 100","reads":[],"edits":edits},sys.stdout)
"""


class TrialError(RuntimeError):
    """A failed acceptance condition with a concise evidence message."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise TrialError(message)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def manifest(root: Path) -> dict[str, str]:
    return {
        str(path.relative_to(root)): sha256_file(path)
        for path in sorted(root.rglob("*"))
        if path.is_file()
    }


def canonical_json_sha256(value: Any) -> str:
    data = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return sha256_bytes(data)


def plist_dictionary(body: str) -> str:
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" '
        '"http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n'
        f'<plist version="1.0"><dict>{body}</dict></plist>\n'
    )


def write_fixture(root: Path) -> Path:
    """Create a minimal UFO v3 containing exactly .notdef and A."""
    ufo = root / "NodesTrial.ufo"
    glyphs = ufo / "glyphs"
    glyphs.mkdir(parents=True)
    (ufo / "metainfo.plist").write_text(
        plist_dictionary(
            "<key>creator</key><string>runebender-nodes-trial</string>"
            "<key>formatVersion</key><integer>3</integer>"
        ),
        encoding="utf-8",
    )
    (ufo / "fontinfo.plist").write_text(
        plist_dictionary(
            "<key>familyName</key><string>Disposable Nodes Trial</string>"
            "<key>styleName</key><string>Regular</string>"
            "<key>unitsPerEm</key><integer>1000</integer>"
            "<key>ascender</key><integer>800</integer>"
            "<key>descender</key><integer>-200</integer>"
        ),
        encoding="utf-8",
    )
    (ufo / "layercontents.plist").write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" '
        '"http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n'
        '<plist version="1.0"><array><array><string>public.default</string>'
        "<string>glyphs</string></array></array></plist>\n",
        encoding="utf-8",
    )
    (glyphs / "contents.plist").write_text(
        plist_dictionary(
            "<key>.notdef</key><string>notdef.glif</string>" "<key>A</key><string>A.glif</string>"
        ),
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
        '<glyph name="A" format="2"><unicode hex="0041"/><advance width="400"/>'
        '<outline><contour><point x="40" y="0" type="line"/>'
        '<point x="360" y="0" type="line"/><point x="360" y="700" type="line"/>'
        '<point x="40" y="700" type="line"/></contour></outline></glyph>\n',
        encoding="utf-8",
    )
    return ufo


def valid_png(data: bytes) -> bool:
    if not data.startswith(PNG_SIGNATURE):
        return False
    offset = len(PNG_SIGNATURE)
    first = True
    while offset + 12 <= len(data):
        length = int.from_bytes(data[offset : offset + 4], "big")
        end = offset + 12 + length
        if end > len(data):
            return False
        kind = data[offset + 4 : offset + 8]
        payload = data[offset + 8 : offset + 8 + length]
        crc = int.from_bytes(data[offset + 8 + length : end], "big")
        if zlib.crc32(kind + payload) & 0xFFFFFFFF != crc:
            return False
        if first and (kind != b"IHDR" or length != 13):
            return False
        first = False
        offset = end
        if kind == b"IEND":
            return length == 0 and offset == len(data)
    return False


def bounded_text(path: Path, limit: int = 64 * 1024) -> str:
    with path.open("rb") as source:
        return source.read(limit).decode("utf-8", errors="replace")


def read_json_line(stream: Any, timeout: float, label: str) -> dict[str, Any]:
    selector = selectors.DefaultSelector()
    selector.register(stream, selectors.EVENT_READ)
    try:
        if not selector.select(timeout):
            raise TrialError(f"{label} did not return JSON within {timeout:.1f}s")
        line = stream.readline()
    finally:
        selector.close()
    if not line:
        raise TrialError(f"{label} closed its output")
    try:
        value = json.loads(line)
    except json.JSONDecodeError as error:
        raise TrialError(f"{label} returned non-JSON output") from error
    if not isinstance(value, dict):
        raise TrialError(f"{label} returned a non-object JSON value")
    return value


class Host:
    def __init__(self, binary: Path, font: Path, duration: int, stderr: BinaryIO) -> None:
        self.process = subprocess.Popen(
            [
                str(binary),
                "agent",
                "serve",
                "--font",
                str(font),
                "--glyph",
                "A",
                "--duration-seconds",
                str(duration),
                "--json",
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=stderr,
            text=True,
            bufsize=1,
        )

    def read(self, timeout: float) -> dict[str, Any]:
        require(self.process.stdout is not None, "headless host has no stdout")
        return read_json_line(self.process.stdout, timeout, "headless host")

    def control(self, action: str, timeout: float) -> dict[str, Any]:
        require(self.process.stdin is not None, "headless host has no stdin")
        self.process.stdin.write(json.dumps({"action": action}) + "\n")
        self.process.stdin.flush()
        return self.read(timeout)

    def stop(self, timeout: float = 10.0) -> None:
        if self.process.poll() is None:
            try:
                require(self.process.stdin is not None, "headless host has no stdin")
                self.process.stdin.write(json.dumps({"action": "shutdown"}) + "\n")
                self.process.stdin.flush()
            except (OSError, TrialError):
                self.process.terminate()
        try:
            self.process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)


def parse_tool_content(result: dict[str, Any]) -> tuple[dict[str, Any], list[bytes]]:
    content = result.get("content")
    require(isinstance(content, list), "MCP tool result has no content list")
    texts: list[dict[str, Any]] = []
    images: list[bytes] = []
    for item in content:
        require(isinstance(item, dict), "MCP content item is not an object")
        if item.get("type") == "text":
            try:
                value = json.loads(item.get("text", ""))
            except json.JSONDecodeError as error:
                raise TrialError("MCP text content is not JSON") from error
            require(isinstance(value, dict), "MCP text content is not a JSON object")
            texts.append(value)
        elif item.get("type") == "image":
            require(item.get("mimeType") == "image/png", "MCP image is not image/png")
            try:
                images.append(base64.b64decode(item.get("data", ""), validate=True))
            except (ValueError, binascii.Error) as error:
                raise TrialError("MCP image data is not canonical base64") from error
    require(len(texts) == 1, "MCP tool response must have exactly one JSON text block")
    return texts[0], images


class McpClient:
    def __init__(self, binary: Path, stderr: BinaryIO, timeout: float) -> None:
        self.timeout = timeout
        self.next_id = 1
        self.transcript: list[dict[str, Any]] = []
        self.process = subprocess.Popen(
            [str(binary), "mcp", "--live"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=stderr,
            text=True,
            bufsize=1,
        )

    def request(self, method: str, params: dict[str, Any]) -> dict[str, Any]:
        request_id = self.next_id
        self.next_id += 1
        require(self.process.stdin is not None, "MCP process has no stdin")
        require(self.process.stdout is not None, "MCP process has no stdout")
        frame = {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
        self.process.stdin.write(json.dumps(frame, separators=(",", ":")) + "\n")
        self.process.stdin.flush()
        deadline = time.monotonic() + self.timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TrialError(f"MCP request {method} timed out")
            response = read_json_line(self.process.stdout, remaining, "MCP server")
            if response.get("id") != request_id:
                continue
            if "error" in response:
                raise TrialError(f"MCP request {method} failed: {response['error']}")
            result = response.get("result")
            require(isinstance(result, dict), f"MCP request {method} has no result object")
            return result

    def notify(self, method: str, params: dict[str, Any]) -> None:
        require(self.process.stdin is not None, "MCP process has no stdin")
        frame = {"jsonrpc": "2.0", "method": method, "params": params}
        self.process.stdin.write(json.dumps(frame, separators=(",", ":")) + "\n")
        self.process.stdin.flush()

    def tool(self, name: str, arguments: dict[str, Any]) -> tuple[dict[str, Any], list[bytes]]:
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        value, images = parse_tool_content(result)
        self.transcript.append(
            {
                "tool": name,
                "arguments_sha256": canonical_json_sha256(arguments),
                "ok": value.get("ok"),
                "is_error": result.get("isError", False),
                "image_sha256": [sha256_bytes(image) for image in images],
            }
        )
        require(result.get("isError") is not True, f"{name} returned MCP isError: {value}")
        require(value.get("ok") is True, f"{name} was rejected: {value}")
        return value, images

    def stop(self, timeout: float = 10.0) -> None:
        if self.process.stdin is not None:
            try:
                self.process.stdin.close()
            except OSError:
                pass
        try:
            self.process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)


def strip_sha256(value: Any) -> str:
    require(isinstance(value, str), "published content hash is not a string")
    result = value.removeprefix("sha256:")
    require(len(result) == 64, "published content hash is not SHA-256")
    require(all(character in "0123456789abcdef" for character in result), "invalid SHA-256")
    return result


def proof_outputs(status: dict[str, Any]) -> dict[str, dict[str, Any]]:
    result: dict[str, dict[str, Any]] = {}
    outputs = status.get("run", {}).get("outputs", [])
    require(isinstance(outputs, list), "terminal run outputs are absent")
    for output in outputs:
        value = output.get("value", {}) if isinstance(output, dict) else {}
        if value.get("kind") == "proof":
            artifact = value.get("artifact_id")
            require(isinstance(artifact, str), "proof output has no artifact identity")
            result[artifact] = value
    require(len(result) == 2, "terminal run did not publish exactly two proof outputs")
    return result


def comparison_edits(snapshot: dict[str, Any]) -> list[dict[str, Any]]:
    nodes = snapshot.get("graph", {}).get("nodes", [])
    require(isinstance(nodes, list), "graph snapshot has no node list")
    edits: list[dict[str, Any]] = []
    proof_count = 0
    python_count = 0
    for node in nodes:
        require(isinstance(node, dict), "graph node is not an object")
        if node.get("type") == "live.python":
            python_count += 1
            edits.append(
                {
                    "edit": "set_value",
                    "node": node["id"],
                    "field": "code",
                    "value": SCRIPT,
                }
            )
        elif node.get("type") == "live.proof":
            proof_count += 1
            recipe = dict(node.get("values", {}).get("recipe", {}))
            recipe["text"] = "AA"
            edits.append(
                {
                    "edit": "set_value",
                    "node": node["id"],
                    "field": "recipe",
                    "value": recipe,
                }
            )
    require(python_count == 1 and proof_count == 2, "unexpected comparison starter topology")
    return edits


def state_width(state: dict[str, Any], expected: float, label: str) -> None:
    for name in ("canonical_advance", "cache_advance", "session_advance"):
        require(
            state.get(name) == expected, f"{label} {name} is {state.get(name)}, expected {expected}"
        )


def redact_socket(value: dict[str, Any]) -> dict[str, Any]:
    result = dict(value)
    if "session" in result:
        result["session"] = "<headless-session>"
    if "font_path" in result:
        result["font_path"] = "<disposable-ufo>"
    return result


def run_trial(
    binary: Path, output: Path, timeout: float, evidence: dict[str, Any]
) -> dict[str, Any]:
    host: Host | None = None
    mcp: McpClient | None = None
    with tempfile.TemporaryDirectory(prefix="runebender-nodes-trial-") as temporary:
        root = Path(temporary)
        session: str | None = None
        ufo = write_fixture(root)
        before = manifest(ufo)
        evidence["fixture"] = {"glyphs": [".notdef", "A"], "manifest_before": before}
        host_stderr_path = root / "host.stderr"
        mcp_stderr_path = root / "mcp.stderr"
        try:
            with host_stderr_path.open("w+b") as host_stderr, mcp_stderr_path.open(
                "w+b"
            ) as mcp_stderr:
                host = Host(binary, ufo, max(60, int(timeout * 4)), host_stderr)
                ready = host.read(timeout)
                require(ready.get("ok") is True, f"headless host failed: {ready}")
                require(isinstance(ready.get("session"), str), "host returned no socket")
                require(isinstance(ready.get("document_epoch"), str), "host returned no epoch")
                epoch = ready["document_epoch"]
                session = ready["session"]
                source = ready.get("source_id")
                evidence["host_ready"] = redact_socket(ready)
                evidence["claims"]["native_host_used"] = True

                initial = host.control("state", timeout)
                state_width(initial, INITIAL_WIDTH, "initial state")

                mcp = McpClient(binary, mcp_stderr, timeout)
                evidence["transcript"] = mcp.transcript
                initialized = mcp.request(
                    "initialize",
                    {
                        "protocolVersion": "2025-11-25",
                        "capabilities": {},
                        "clientInfo": {"name": "agent-nodes-trial", "version": "1"},
                    },
                )
                mcp.notify("notifications/initialized", {})
                tools = mcp.request("tools/list", {})
                evidence["claims"]["actual_stdio_mcp_used"] = True
                listed = {tool.get("name") for tool in tools.get("tools", [])}
                require(
                    NODE_TOOLS <= listed,
                    f"Nodes tools missing from tools/list: {sorted(NODE_TOOLS - listed)}",
                )
                tool_schemas = {
                    tool["name"]: tool.get("inputSchema")
                    for tool in tools.get("tools", [])
                    if tool.get("name") in NODE_TOOLS
                }
                evidence["mcp"] = {
                    "protocol_version": initialized.get("protocolVersion"),
                    "server_info": initialized.get("serverInfo"),
                    "nodes_tools": sorted(NODE_TOOLS),
                    "nodes_tool_schemas_sha256": canonical_json_sha256(tool_schemas),
                }

                connected, images = mcp.tool("editor_connect", {"session": session})
                require(not images, "editor_connect unexpectedly returned an image")
                require(connected.get("document_epoch") == epoch, "editor_connect epoch mismatch")

                discovered, _ = mcp.tool("nodes_discover", {"expected_document_epoch": epoch})
                identity = discovered.get("identity")
                require(isinstance(identity, dict), "nodes_discover returned no identity")
                require(identity.get("document_epoch") == epoch, "graph identity epoch mismatch")

                snap, _ = mcp.tool(
                    "nodes_snapshot",
                    {"expected_document_epoch": epoch, "identity": identity},
                )
                snapshot = snap.get("snapshot")
                require(isinstance(snapshot, dict), "nodes_snapshot returned no snapshot")
                edits = comparison_edits(snapshot)
                mutate_args = {
                    "expected_document_epoch": epoch,
                    "request": {
                        "guard": {"identity": identity, "revision": snapshot["revision"]},
                        "actor": ACTOR,
                        "operation_key": "configure-1",
                        "mutation": {"mutation": "patch", "edits": edits},
                    },
                }
                mutated, _ = mcp.tool("nodes_mutate", mutate_args)
                mutation = mutated.get("mutation", {})
                require(mutation.get("disposition") == "applied", "graph patch was not applied")
                configured = mutated.get("snapshot")
                require(isinstance(configured, dict), "nodes_mutate returned no snapshot")

                run_args = {
                    "expected_document_epoch": epoch,
                    "guard": {
                        "identity": identity,
                        "semantic_revision": configured["semantic_revision"],
                        "semantic_hash": configured["semantic_hash"],
                    },
                    "actor": ACTOR,
                    "operation_key": "run-1",
                    "source": source,
                    "glyphs": ["A"],
                }
                started, _ = mcp.tool("nodes_run", run_args)
                require(started.get("replayed") is False, "first nodes_run was replayed")
                handle = started.get("run", {}).get("receipt", {}).get("handle")
                require(isinstance(handle, int) and handle > 0, "nodes_run returned no handle")
                status_args = {
                    "expected_document_epoch": epoch,
                    "identity": identity,
                    "handle": handle,
                }
                deadline = time.monotonic() + timeout
                while True:
                    status, _ = mcp.tool("nodes_status", status_args)
                    phase = status.get("run", {}).get("status")
                    if phase == "completed":
                        break
                    require(phase in {"queued", "running"}, f"Nodes run ended as {phase}: {status}")
                    require(
                        time.monotonic() < deadline, "Nodes run did not complete before timeout"
                    )
                    time.sleep(0.05)
                require(status.get("current") is True, "completed result is not current")
                require(status.get("can_apply") is True, "completed result cannot be applied")
                published = proof_outputs(status)

                delivered: dict[str, dict[str, Any]] = {}
                for branch in ("original", "changed"):
                    image_result, image_blocks = mcp.tool(
                        "nodes_image", {**status_args, "branch": branch}
                    )
                    require(len(image_blocks) == 1, f"{branch} did not deliver one MCP image block")
                    data = image_blocks[0]
                    require(valid_png(data), f"{branch} image block is not a valid PNG")
                    require(
                        "png_base64" not in image_result,
                        f"{branch} leaked png_base64 into text content",
                    )
                    artifact = image_result.get("artifact_id")
                    require(artifact in published, f"{branch} image has unknown artifact identity")
                    expected_hash = strip_sha256(published[artifact].get("content_sha256"))
                    actual_hash = sha256_bytes(data)
                    require(
                        actual_hash == expected_hash,
                        f"{branch} MCP bytes differ from published output",
                    )
                    require(
                        image_result.get("font_sha256") == published[artifact].get("font_sha256"),
                        f"{branch} font lineage differs from published output",
                    )
                    require(
                        image_result.get("canonical_input_sha256")
                        == published[artifact].get("canonical_input_sha256"),
                        f"{branch} compiler lineage differs from published output",
                    )
                    image_path = output / f"{branch}.png"
                    image_path.write_bytes(data)
                    delivered[branch] = {
                        "artifact_id": artifact,
                        "sha256": actual_hash,
                        "bytes": len(data),
                        "font_sha256": image_result.get("font_sha256"),
                        "canonical_input_sha256": image_result.get("canonical_input_sha256"),
                    }
                require(
                    delivered["original"]["sha256"] != delivered["changed"]["sha256"],
                    "original and changed PNG bytes are equal",
                )
                require(
                    delivered["original"]["font_sha256"] != delivered["changed"]["font_sha256"],
                    "original and changed compiled-font hashes are equal",
                )

                apply_args = {
                    **status_args,
                    "actor": ACTOR,
                    "operation_key": "apply-1",
                    "authorization": "user-approved",
                }
                applied, _ = mcp.tool("nodes_apply", apply_args)
                require(applied.get("replayed") is False, "first Apply was replayed")
                require(applied.get("root_changed") is True, "first Apply did not change root")
                receipt = applied.get("receipt")
                require(isinstance(receipt, dict), "first Apply returned no receipt")
                applied_state = host.control("state", timeout)
                state_width(applied_state, CHANGED_WIDTH, "applied state")

                exact_apply, _ = mcp.tool("nodes_apply", apply_args)
                require(exact_apply.get("replayed") is True, "exact Apply retry was not replayed")
                require(exact_apply.get("root_changed") is False, "exact Apply retry changed root")
                require(exact_apply.get("receipt") == receipt, "exact Apply retry receipt changed")

                receipt_result, _ = mcp.tool(
                    "agent_receipt",
                    {
                        "expected_document_epoch": epoch,
                        "actor": ACTOR,
                        "operation_key": "apply-1",
                    },
                )
                require(receipt_result.get("receipt") == receipt, "agent_receipt identity mismatch")
                require(receipt_result.get("history_state") == "applied", "receipt is not applied")

                undone = host.control("undo", timeout)
                state_width(undone, INITIAL_WIDTH, "undone state")
                require(undone.get("redo_depth", 0) > 0, "ordinary host undo made no redo entry")

                retry_after_undo, _ = mcp.tool("nodes_apply", apply_args)
                require(
                    retry_after_undo.get("replayed") is True, "post-undo retry was not replayed"
                )
                require(
                    retry_after_undo.get("root_changed") is False, "post-undo retry changed root"
                )
                require(retry_after_undo.get("receipt") == receipt, "post-undo receipt changed")
                require(
                    retry_after_undo.get("history_state") == "undone",
                    "post-undo history state mismatch",
                )
                still_undone = host.control("state", timeout)
                state_width(still_undone, INITIAL_WIDTH, "post-retry state")

                run_retry, _ = mcp.tool("nodes_run", run_args)
                require(run_retry.get("replayed") is True, "exact run retry was not replayed")
                require(
                    run_retry.get("run") == started.get("run"), "exact run retry response changed"
                )

                stale, _ = mcp.tool("nodes_status", status_args)
                require(
                    stale.get("stale") is True and stale.get("current") is False,
                    "undone run is not stale",
                )

                cancelled, _ = mcp.tool(
                    "nodes_cancel",
                    {
                        "expected_document_epoch": epoch,
                        "request": {
                            "identity": identity,
                            "handle": handle,
                            "actor": ACTOR,
                            "operation_key": "cancel-after-complete",
                        },
                    },
                )
                cancellation = cancelled.get("cancellation", {})
                require(
                    cancellation.get("receipt", {}).get("outcome") == "too_late",
                    "terminal cancellation was not recorded as too_late",
                )

                released, _ = mcp.tool("nodes_release", status_args)
                require(released.get("released") is True, "nodes_release did not release the run")

                after = manifest(ufo)
                require(after == before, "disposable source manifest changed")
                evidence["fixture"]["manifest_after"] = after
                evidence["host_states"] = {
                    "initial": initial,
                    "applied": applied_state,
                    "undone": undone,
                    "post_retry": still_undone,
                }
                evidence["images"] = delivered
                evidence["receipts"] = {
                    "apply": receipt,
                    "apply_retry_history_state": retry_after_undo.get("history_state"),
                    "cancel_outcome": cancellation.get("receipt", {}).get("outcome"),
                }
                evidence["checks"] = {
                    "all_nine_nodes_tools_listed_and_called": NODE_TOOLS
                    <= {entry["tool"] for entry in mcp.transcript},
                    "source_manifest_unchanged": after == before,
                    "mcp_png_bytes_match_published_output_hashes": True,
                    "compiled_images_differ": True,
                    "apply_exact_retry_is_idempotent": True,
                    "ordinary_undo_restores_width": True,
                    "post_undo_apply_retry_does_not_reapply": True,
                    "run_exact_retry_is_idempotent": True,
                    "post_undo_status_is_stale": True,
                    "terminal_cancel_is_too_late": True,
                    "release_succeeds": True,
                }
                require(all(evidence["checks"].values()), "one or more acceptance checks failed")
                evidence["transcript"] = mcp.transcript
                evidence["result"] = "passed"
        finally:
            if mcp is not None:
                mcp.stop()
            if host is not None:
                host.stop()
            host_stderr = bounded_text(host_stderr_path) if host_stderr_path.exists() else ""
            mcp_stderr = bounded_text(mcp_stderr_path) if mcp_stderr_path.exists() else ""
            for old, new in ((str(root), "<trial>"), (session or "", "<headless-session>")):
                if old:
                    host_stderr = host_stderr.replace(old, new)
                    mcp_stderr = mcp_stderr.replace(old, new)
            evidence["process_stderr"] = {
                "host": host_stderr,
                "mcp": mcp_stderr,
            }
            evidence["process_exit"] = {
                "host": host.process.returncode if host is not None else None,
                "mcp": mcp.process.returncode if mcp is not None else None,
            }
            after_cleanup = manifest(ufo)
            evidence["fixture"]["manifest_after_cleanup"] = after_cleanup
            evidence["checks"]["source_manifest_unchanged_after_cleanup"] = after_cleanup == before
    if evidence.get("result") == "passed":
        require(
            evidence["process_exit"] == {"host": 0, "mcp": 0},
            "trial processes did not exit cleanly",
        )
        require(
            evidence["fixture"]["manifest_after_cleanup"] == before,
            "disposable source manifest changed during cleanup",
        )
    return evidence


def write_evidence(output: Path, evidence: dict[str, Any]) -> None:
    evidence_path = output / "evidence.json"
    evidence_path.write_text(
        json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    artifacts = {
        path.name: {"bytes": path.stat().st_size, "sha256": sha256_file(path)}
        for path in sorted(output.iterdir())
        if path.is_file() and path.name != "artifact-manifest.json"
    }
    (output / "artifact-manifest.json").write_text(
        json.dumps({"schema_version": 1, "artifacts": artifacts}, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary", required=True, type=Path, help="absolute native Runebender binary"
    )
    parser.add_argument("--output-dir", required=True, type=Path, help="new evidence directory")
    parser.add_argument(
        "--timeout-seconds", type=float, default=90.0, help="per-operation and run timeout"
    )
    parser.add_argument(
        "--evidence-label",
        default="unspecified",
        help="short label such as preliminary or final",
    )
    args = parser.parse_args()
    if not args.binary.is_absolute():
        parser.error("--binary must be an absolute path")
    if args.timeout_seconds < 5 or args.timeout_seconds > 300:
        parser.error("--timeout-seconds must be between 5 and 300")
    if (
        not args.evidence_label
        or len(args.evidence_label.encode()) > 64
        or any(character.isspace() for character in args.evidence_label)
    ):
        parser.error("--evidence-label must contain 1..=64 non-whitespace UTF-8 bytes")
    return args


def main() -> int:
    args = parse_args()
    if args.output_dir.exists():
        print(f"agent_nodes_trial: output path already exists: {args.output_dir}", file=sys.stderr)
        return 1
    args.output_dir.mkdir(parents=True)
    binary = args.binary.resolve()
    evidence: dict[str, Any] = {
        "schema_version": 1,
        "trial": "native_nodes_stdio_mcp",
        "evidence_label": args.evidence_label,
        "result": "running",
        "claims": {
            "external_model_used": False,
            "model_interpretation_tested": False,
            "native_host_used": False,
            "actual_stdio_mcp_used": False,
        },
        "requested": {"binary": str(binary), "timeout_seconds": args.timeout_seconds},
        "transcript": [],
        "checks": {},
    }
    try:
        require(binary.is_file(), f"binary does not exist: {binary}")
        version = subprocess.run(
            [str(binary), "--version"],
            capture_output=True,
            text=True,
            timeout=min(args.timeout_seconds, 15),
            check=False,
        )
        require(version.returncode == 0, f"binary --version failed: {version.stderr.strip()}")
        evidence["binary"] = {
            "path": str(binary),
            "sha256": sha256_file(binary),
            "version": version.stdout.strip(),
        }
        run_trial(binary, args.output_dir, args.timeout_seconds, evidence)
    except (OSError, subprocess.SubprocessError, TrialError) as error:
        evidence["result"] = "failed"
        evidence["error"] = str(error)
        evidence["limitations"] = [
            "The trial did not reach every acceptance check.",
            "A failed run does not establish end-to-end Nodes behavior or model interpretation.",
        ]
        write_evidence(args.output_dir, evidence)
        print(f"agent_nodes_trial: {error}", file=sys.stderr)
        return 1
    write_evidence(args.output_dir, evidence)
    print(json.dumps({"ok": True, "evidence": str(args.output_dir / "evidence.json")}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
