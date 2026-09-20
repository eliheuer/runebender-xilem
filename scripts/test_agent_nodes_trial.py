#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Credential-free parser and fixture tests for agent_nodes_trial.py."""

from __future__ import annotations

import base64
import json
from pathlib import Path
import shutil
import struct
import tempfile
import unittest
import zlib

import agent_nodes_trial as trial


def chunk(kind: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    )


def one_pixel_png() -> bytes:
    header = struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0)
    pixels = zlib.compress(b"\x00\x00\x00\x00\xff")
    return (
        trial.PNG_SIGNATURE + chunk(b"IHDR", header) + chunk(b"IDAT", pixels) + chunk(b"IEND", b"")
    )


class TrialHelpersTest(unittest.TestCase):
    def test_fixture_contains_exactly_two_glyphs_and_is_stable(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            ufo = trial.write_fixture(Path(temporary))
            first = trial.manifest(ufo)
            contents = (ufo / "glyphs" / "contents.plist").read_text(encoding="utf-8")
            self.assertEqual(contents.count("<key>"), 2)
            self.assertIn("<key>.notdef</key>", contents)
            self.assertIn("<key>A</key>", contents)
            self.assertEqual(first, trial.manifest(ufo))

    def test_designspace_copy_includes_both_same_directory_sources(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            source.mkdir()
            regular = trial.write_fixture(source)
            regular.rename(source / "Regular.ufo")
            shutil.copytree(source / "Regular.ufo", source / "Bold.ufo")
            designspace = source / "Family.designspace"
            designspace.write_text(
                """<?xml version="1.0" encoding="UTF-8"?>
<designspace format="5"><sources>
<source filename="Regular.ufo"/><source filename="Bold.ufo"/>
</sources></designspace>
""",
                encoding="utf-8",
            )
            copied_font, copied_root, names = trial.copy_designspace_family(
                designspace, root / "trial"
            )
            self.assertEqual(copied_font.name, "Family.designspace")
            self.assertEqual(names, ["Bold.ufo", "Family.designspace", "Regular.ufo"])
            self.assertEqual(trial.manifest(copied_root), trial.manifest(source))

    def test_parse_tool_content_preserves_actual_image_bytes(self) -> None:
        png = one_pixel_png()
        result = {
            "content": [
                {"type": "text", "text": json.dumps({"ok": True, "artifact_id": "proof-1"})},
                {
                    "type": "image",
                    "mimeType": "image/png",
                    "data": base64.b64encode(png).decode("ascii"),
                },
            ]
        }
        value, images = trial.parse_tool_content(result)
        self.assertEqual(value["artifact_id"], "proof-1")
        self.assertEqual(images, [png])
        self.assertTrue(trial.valid_png(images[0]))

    def test_png_validation_rejects_changed_bytes(self) -> None:
        png = bytearray(one_pixel_png())
        png[-5] ^= 1
        self.assertFalse(trial.valid_png(bytes(png)))

    def test_proof_outputs_requires_two_artifacts(self) -> None:
        status = {
            "run": {
                "outputs": [
                    {"node": 1, "value": {"kind": "report", "text": "done"}},
                    {
                        "node": 2,
                        "value": {
                            "kind": "proof",
                            "artifact_id": "original",
                            "content_sha256": "sha256:" + "a" * 64,
                        },
                    },
                    {
                        "node": 3,
                        "value": {
                            "kind": "proof",
                            "artifact_id": "changed",
                            "content_sha256": "sha256:" + "b" * 64,
                        },
                    },
                ]
            }
        }
        self.assertEqual(set(trial.proof_outputs(status)), {"original", "changed"})

    def test_comparison_edits_use_wire_type_and_preserve_recipe(self) -> None:
        snapshot = {
            "graph": {
                "nodes": [
                    {"id": 1, "type": "live.font"},
                    {"id": 2, "type": "live.python", "values": {"parameters": {}}},
                    {
                        "id": 3,
                        "type": "live.proof",
                        "values": {"recipe": {"text": "A", "right_to_left": False}},
                    },
                    {
                        "id": 4,
                        "type": "live.proof",
                        "values": {"recipe": {"text": "B", "right_to_left": True}},
                    },
                ]
            }
        }
        edits = trial.comparison_edits(snapshot)
        self.assertEqual([edit["node"] for edit in edits], [2, 3, 4])
        self.assertEqual(edits[1]["value"], {"text": "AA", "right_to_left": False})
        self.assertEqual(edits[2]["value"], {"text": "AA", "right_to_left": True})


if __name__ == "__main__":
    unittest.main()
