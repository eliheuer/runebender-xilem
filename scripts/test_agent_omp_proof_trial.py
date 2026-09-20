#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Credential-free tests for the OMP compiled-proof evidence parser."""

from __future__ import annotations

import base64
import copy
import json
from pathlib import Path
import tempfile
import unittest

import agent_omp_proof_trial as trial

EPOCH = "test-document-epoch"
MARKER_NAME = "q-private"
MARKER_TEXT = "\ue012"
PNG = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
)


def start(call_id: str, tool: str, args: dict[str, object]) -> dict[str, object]:
    return {
        "type": "tool_execution_start",
        "toolCallId": call_id,
        "toolName": f"mcp__runebender_{tool}",
        "args": {"i": f"calling {tool}", **copy.deepcopy(args)},
    }


def end(
    call_id: str,
    tool: str,
    result: dict[str, object],
    image: bytes | None = None,
    is_error: bool = False,
) -> dict[str, object]:
    content: list[dict[str, object]] = [{"type": "text", "text": json.dumps(result)}]
    if image is not None:
        content.append(
            {
                "type": "image",
                "mimeType": "image/png",
                "data": base64.b64encode(image).decode(),
            }
        )
    return {
        "type": "tool_execution_end",
        "toolCallId": call_id,
        "toolName": f"mcp__runebender_{tool}",
        "result": {"content": content},
        "isError": is_error,
    }


def assistant(text: str) -> dict[str, object]:
    return {
        "type": "message_end",
        "message": {"role": "assistant", "content": [{"type": "text", "text": text}]},
    }


def event_stream(events: list[dict[str, object]]) -> str:
    return "\n".join(json.dumps(event) for event in events) + "\n"


def passing_events() -> list[dict[str, object]]:
    identity = {
        "expected_document_epoch": EPOCH,
        "actor": "omp-proof-trial",
        "operation_key": "omp-edit-01",
    }
    apply = {
        **identity,
        "authorization": "user-approved",
        "source": 0,
        "history_name": "OMP proof trial",
        "edits": [
            {
                "target": {
                    "glyph": "A",
                    "glyph_id": "glyph-a",
                    "layer": "public.default",
                    "expected_revision": "glif-sha256:before",
                },
                "operations": [{"op": "set_width", "width": trial.EDIT_WIDTH}],
            }
        ],
    }
    receipt = {
        "document_epoch": EPOCH,
        "actor": "omp-proof-trial",
        "operation_key": "omp-edit-01",
        "payload_sha256": "1" * 64,
        "outcome": {
            "status": "committed",
            "before_revision": 1,
            "after_revision": 2,
        },
    }
    proof_recipe = {
        "text": MARKER_TEXT,
        "normalized_location": [],
        "right_to_left": False,
        "features": [],
        "script": None,
        "language": None,
    }
    events = [
        start("connect", "editor_connect", {"session": "/tmp/test.sock"}),
        end("connect", "editor_connect", {"ok": True, "document_epoch": EPOCH}),
        start("project", "project_info", {"expected_document_epoch": EPOCH}),
        end("project", "project_info", {"ok": True, "document_epoch": EPOCH}),
        start("context", "editor_context", {"expected_document_epoch": EPOCH}),
        end("context", "editor_context", {"ok": True, "document_epoch": EPOCH}),
        start("inventory", "glyph_inventory", {"expected_document_epoch": EPOCH}),
        end(
            "inventory",
            "glyph_inventory",
            {
                "ok": True,
                "glyphs": [
                    {"glyph": "A", "codepoints": [0x41]},
                    {"glyph": MARKER_NAME, "codepoints": [ord(MARKER_TEXT)]},
                ],
            },
        ),
        start("read", "read_glyph", {"glyph": "A", "source": 0}),
        end("read", "read_glyph", {"ok": True, "advance": 600}),
        start("apply-1", "agent_apply", apply),
        end(
            "apply-1",
            "agent_apply",
            {"ok": True, "replayed": False, "root_changed": True, "receipt": receipt},
        ),
        start("receipt", "agent_receipt", identity),
        end("receipt", "agent_receipt", {"ok": True, "receipt": receipt}),
        start("apply-2", "agent_apply", apply),
        end(
            "apply-2",
            "agent_apply",
            {"ok": True, "replayed": True, "root_changed": False, "receipt": receipt},
        ),
        start("cancel", "agent_cancel", identity),
        end(
            "cancel",
            "agent_cancel",
            {"ok": False, "cancellation_status": "committed"},
            is_error=True,
        ),
        start(
            "proof-start",
            "proof_start",
            {
                "expected_document_epoch": EPOCH,
                "expected_document_revision": 2,
                "operation_key": "omp-proof-01",
                "recipe": proof_recipe,
            },
        ),
        end("proof-start", "proof_start", {"ok": True, "proof_id": "7"}),
        start(
            "proof-status",
            "proof_status",
            {"expected_document_epoch": EPOCH, "proof_id": "7", "include_image": True},
        ),
        end(
            "proof-status",
            "proof_status",
            {
                "ok": True,
                "proof_id": "7",
                "status": "completed",
                "current": True,
                "stale": False,
                "captured_document_epoch": EPOCH,
                "captured_document_revision": 2,
                "font_sha256": "2" * 64,
                "canonical_input_sha256": "3" * 64,
                "recipe": proof_recipe,
            },
            PNG,
        ),
        start(
            "proof-release",
            "proof_release",
            {"expected_document_epoch": EPOCH, "proof_id": "7"},
        ),
        end("proof-release", "proof_release", {"ok": True, "released": True}),
        assistant("The visible marker is pointed.\nMARKER_CLASSIFICATION: diamond"),
    ]
    return events


def evaluate(events: list[dict[str, object]]) -> dict[str, object]:
    evidence = trial.parse_transcript(event_stream(events), Path("/trial"), EPOCH)
    state = {
        "ok": True,
        "canonical_advance": trial.EDIT_WIDTH,
        "cache_advance": trial.EDIT_WIDTH,
        "session_advance": trial.EDIT_WIDTH,
        "document_revision": 2,
    }
    return trial.evaluate_trial(
        evidence,
        state,
        True,
        0,
        MARKER_NAME,
        MARKER_TEXT,
        "diamond",
    )


class TranscriptEvidenceTests(unittest.TestCase):
    def test_complete_correlated_evidence_passes(self) -> None:
        result = evaluate(passing_events())
        self.assertTrue(result["passed"], result["checks"])

    def test_unrelated_image_does_not_prove_model_delivery(self) -> None:
        events = passing_events()
        for event in events:
            if (
                event.get("toolCallId") == "proof-status"
                and event["type"] == "tool_execution_end"
            ):
                event["result"]["content"] = event["result"]["content"][:1]
            if (
                event.get("toolCallId") == "read"
                and event["type"] == "tool_execution_end"
            ):
                event["result"]["content"].append(
                    {
                        "type": "image",
                        "mimeType": "image/png",
                        "data": base64.b64encode(PNG).decode(),
                    }
                )
        result = evaluate(events)
        self.assertFalse(result["passed"])
        self.assertFalse(result["checks"]["compiled_proof_lineage"])
        self.assertFalse(result["checks"]["assistant_after_proof_image"])

    def test_changed_retry_payload_is_rejected_by_evidence(self) -> None:
        events = passing_events()
        retry = next(
            event
            for event in events
            if event.get("toolCallId") == "apply-2"
            and event["type"] == "tool_execution_start"
        )
        retry["args"]["edits"][0]["operations"][0]["width"] = 999
        result = evaluate(events)
        self.assertFalse(result["passed"])
        self.assertFalse(result["checks"]["exact_apply_retry"])

    def test_nonterminal_cancellation_status_is_rejected(self) -> None:
        events = passing_events()
        cancel = next(
            event
            for event in events
            if event.get("toolCallId") == "cancel"
            and event["type"] == "tool_execution_end"
        )
        payload = json.loads(cancel["result"]["content"][0]["text"])
        payload["cancellation_status"] = "too_late"
        cancel["result"]["content"][0]["text"] = json.dumps(payload)
        result = evaluate(events)
        self.assertFalse(result["passed"])
        self.assertFalse(result["checks"]["committed_cancel_did_not_undo"])

    def test_png_signature_without_png_structure_is_rejected(self) -> None:
        events = passing_events()
        proof = next(
            event
            for event in events
            if event.get("toolCallId") == "proof-status"
            and event["type"] == "tool_execution_end"
        )
        proof["result"]["content"][1]["data"] = base64.b64encode(
            trial.PNG_SIGNATURE
        ).decode()
        result = evaluate(events)
        self.assertFalse(result["passed"])
        self.assertFalse(result["checks"]["compiled_proof_lineage"])

    def test_pre_image_answer_does_not_count_as_visual_interpretation(self) -> None:
        events = passing_events()
        events.insert(0, assistant("MARKER_CLASSIFICATION: diamond"))
        events[-1] = assistant("I received the proof artifact.")
        result = evaluate(events)
        self.assertFalse(result["passed"])
        self.assertFalse(result["checks"]["visual_classification"])

    def test_fixture_randomizes_private_ground_truth(self) -> None:
        observed: set[str] = set()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for seed in range(32):
                trial_root = root / str(seed)
                ufo, marker_name, _, shape = trial.write_fixture(trial_root, seed)
                self.assertTrue((ufo / "glyphs" / "notdef.glif").is_file())
                marker = (ufo / "glyphs" / f"{marker_name}.glif").read_text()
                self.assertNotIn('type="move"', marker)
                observed.add(shape)
        self.assertEqual(observed, {"diamond", "rounded"})

    def test_reviewed_transcript_redacts_authorization(self) -> None:
        evidence = trial.parse_transcript(
            event_stream(passing_events()), Path("/trial"), EPOCH
        )
        apply = next(
            event
            for event in evidence["reviewed"]
            if event["type"] == "tool_execution_start"
            and str(event["tool"]).endswith("agent_apply")
        )
        self.assertEqual(apply["args"]["authorization"], "<redacted>")


if __name__ == "__main__":
    unittest.main()
