#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Deterministic conformance tests for the pure anchor recipes."""

from __future__ import annotations

from copy import deepcopy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import unittest


ROOT = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("anchor_recipes", ROOT / "anchor_recipes.py")
assert SPEC and SPEC.loader
recipes = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(recipes)


def fixture(name: str) -> dict:
    return json.loads((ROOT / name).read_text(encoding="utf-8"))


class AnchorRecipeTests(unittest.TestCase):
    def test_canonical_move_matches_expected_and_preserves_guards(self) -> None:
        value = fixture("fixture-move-input.json")
        before = deepcopy(value)
        result = recipes.run("move_named_anchors", value)
        expected = fixture("fixture-move-expected.json")
        self.assertEqual(result, expected)
        self.assertEqual(value, before)
        self.assertEqual(
            [edit["target"] for edit in result["edits"]],
            [
                value["layers"][1]["guard"],
                value["layers"][0]["guard"],
            ],
        )

    def test_list_reports_source_layer_name_coordinates_and_unnamed_anchor(self) -> None:
        result = recipes.run("list_anchors", fixture("fixture-list-input.json"))
        lines = result["report"].splitlines()
        self.assertEqual(lines[0], "Listed 5 anchor(s)")
        self.assertIn("glyph=A source=7 layer=Regular name=bottom", lines[1])
        self.assertIn("x=0.125 y=-3.75", lines[-1])
        self.assertEqual(len(result["reads"]), 2)
        self.assertEqual(result["edits"], [])

    def test_duplicate_names_move_as_distinct_ids(self) -> None:
        result = recipes.run("move_named_anchors", fixture("fixture-move-input.json"))
        operations = result["edits"][1]["operations"]
        self.assertEqual([operation["anchor_id"] for operation in operations], ["anchor-top-1", "anchor-top-2"])
        self.assertEqual([(operation["x"], operation["y"]) for operation in operations], [(101.0, 699.0), (12.75, 510.25)])

    def test_empty_scope_has_no_reads_or_edits(self) -> None:
        value = fixture("fixture-move-input.json")
        value["parameters"]["glyphs"] = []
        result = recipes.run("move_named_anchors", value)
        self.assertEqual(result["reads"], [])
        self.assertEqual(result["edits"], [])
        self.assertEqual(result["report"], "Proposed 0 anchor move(s)")

    def test_no_op_preserves_guard_as_read_and_emits_no_edit(self) -> None:
        value = fixture("fixture-move-input.json")
        value["parameters"]["dx"] = 0.0
        value["parameters"]["dy"] = 0
        result = recipes.run("move_named_anchors", value)
        self.assertEqual(result["edits"], [])
        self.assertEqual([guard["glyph"] for guard in result["reads"]], ["A", "B"])
        self.assertEqual(result["report"].splitlines()[0], "Proposed 0 anchor move(s)")

    def test_invalid_and_nonfinite_parameters_are_rejected(self) -> None:
        for key, value in (("dx", float("nan")), ("dy", float("inf"))):
            envelope = fixture("fixture-move-input.json")
            envelope["parameters"][key] = value
            with self.subTest(parameter=key), self.assertRaises(recipes.RecipeError):
                recipes.run("move_named_anchors", envelope)

    def test_unsupported_scope_is_reported_without_edits(self) -> None:
        envelope = fixture("fixture-move-input.json")
        envelope["parameters"]["sources"] = [7, 8]
        with self.assertRaises(recipes.RecipeError) as context:
            recipes.run("move_named_anchors", envelope)
        self.assertEqual(context.exception.status, "unsupported")

    def test_output_is_stable_when_capture_order_changes(self) -> None:
        value = fixture("fixture-move-input.json")
        reversed_value = deepcopy(value)
        reversed_value["layers"].reverse()
        for layer in reversed_value["layers"]:
            layer["anchors"].reverse()
        reversed_value["parameters"]["glyphs"] = ["B", "A"]
        self.assertEqual(
            recipes.run("move_named_anchors", value),
            recipes.run("move_named_anchors", reversed_value),
        )

    def test_layer_and_operation_limits_are_not_split(self) -> None:
        too_many_layers = fixture("fixture-move-input.json")
        too_many_layers["layers"] = [
            {
                "guard": {
                    "glyph": f"g{index}",
                    "glyph_id": f"id{index}",
                    "layer": "Regular",
                    "expected_revision": f"r{index}",
                },
                "width": 500,
                "anchors": [],
            }
            for index in range(65)
        ]
        too_many_layers["parameters"] = {"recipe": "list_anchors", "glyphs": []}
        with self.assertRaises(recipes.RecipeError) as context:
            recipes.run("list_anchors", too_many_layers)
        self.assertEqual(context.exception.status, "unsupported")

        too_many_operations = fixture("fixture-move-input.json")
        too_many_operations["layers"] = [too_many_operations["layers"][0]]
        too_many_operations["layers"][0]["guard"]["glyph"] = "A"
        too_many_operations["layers"][0]["anchors"] = [
            {"id": f"a{index}", "name": "top", "x": index + 0.25, "y": index + 0.5}
            for index in range(257)
        ]
        too_many_operations["parameters"]["glyphs"] = ["A"]
        with self.assertRaises(recipes.RecipeError) as context:
            recipes.run("move_named_anchors", too_many_operations)
        self.assertEqual(context.exception.status, "unsupported")

    def test_cli_is_one_json_stdout_line_with_stderr_diagnostics(self) -> None:
        completed = subprocess.run(
            [sys.executable, "-I", str(ROOT / "anchor_recipes.py")],
            input=json.dumps(fixture("fixture-move-input.json")),
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0)
        self.assertEqual(len(completed.stdout.splitlines()), 1)
        result = json.loads(completed.stdout)
        self.assertEqual(result["job_id"], "fixture-anchor-move-1")
        self.assertEqual(result["input_hash"], "fixture-capture-sha256-1")
        self.assertIn("recipe=move_named_anchors status=ok", completed.stderr)


if __name__ == "__main__":
    unittest.main()
