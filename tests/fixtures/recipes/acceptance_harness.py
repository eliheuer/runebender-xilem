#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Bounded acceptance harness for pure recipe and later runtime evidence."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys
from typing import Any


ROOT = Path(__file__).resolve().parent
MAX_OUTPUT_BYTES = 128 * 1024


def _run(recipe: str, input_path: Path) -> dict[str, Any]:
    completed = subprocess.run(
        [sys.executable, "-I", str(ROOT / "anchor_recipes.py")],
        input=input_path.read_text(encoding="utf-8"),
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )
    if len(completed.stdout.encode()) > MAX_OUTPUT_BYTES or len(completed.stderr.encode()) > MAX_OUTPUT_BYTES:
        raise RuntimeError(f"{recipe} exceeded the bounded output limit")
    lines = completed.stdout.splitlines()
    if len(lines) != 1:
        raise RuntimeError(f"{recipe} emitted {len(lines)} stdout lines, expected one JSON result")
    result = json.loads(lines[0])
    if completed.returncode != 0:
        raise RuntimeError(f"{recipe} failed: {result.get('report', result)}")
    if not isinstance(result, dict):
        raise RuntimeError(f"{recipe} result is not an object")
    return {"result": result, "stderr": completed.stderr}


def _optional_runtime_evidence(path: Path | None) -> dict[str, Any]:
    if path is None:
        return {
            "status": "not_run",
            "reason": "No coordinator-supplied reviewed runtime/UI fixture was provided.",
            "source_manifests": [],
            "receipts": [],
            "undo": [],
        }
    evidence = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(evidence, dict):
        raise RuntimeError("runtime evidence must be a JSON object")
    return {
        "status": "recorded",
        "source_manifests": evidence.get("source_manifests", []),
        "receipts": evidence.get("receipts", []),
        "undo": evidence.get("undo", []),
    }


def run(output_evidence: Path | None = None) -> dict[str, Any]:
    move = _run("move_named_anchors", ROOT / "fixture-move-input.json")
    expected = json.loads((ROOT / "fixture-move-expected.json").read_text(encoding="utf-8"))
    if move["result"] != expected:
        raise RuntimeError("move recipe did not match the canonical fixture result")
    listing = _run("list_anchors", ROOT / "fixture-list-input.json")
    if listing["result"]["report"].splitlines()[0] != "Listed 5 anchor(s)":
        raise RuntimeError("list recipe did not return the five selected fixture anchors")
    return {
        "schema_version": 1,
        "kind": "anchor_recipe_acceptance",
        "writes_performed": False,
        "pure_recipe_tests": {
            "status": "pass",
            "cases": ["canonical_move", "canonical_list", "single_json_stdout", "stderr_diagnostics"],
        },
        "application_evidence": _optional_runtime_evidence(output_evidence),
        "model_evidence": {
            "status": "not_run",
            "reason": "No model, OMP, API, or credentialed trial is part of this worker.",
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime-evidence", type=Path)
    args = parser.parse_args()
    print(json.dumps(run(args.runtime_evidence), sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
