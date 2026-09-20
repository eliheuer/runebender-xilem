#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Pure, bounded anchor recipes for the version-one script envelope."""

from __future__ import annotations

import json
import math
import sys
from typing import Any, NoReturn


SCHEMA_VERSION = 1
MAX_GUARDED_LAYERS = 64
MAX_OPERATIONS = 256
MAX_STRING_BYTES = 256
RECIPES = ("list_anchors", "move_named_anchors")

TOP_LEVEL_KEYS = {
    "schema_version",
    "job_id",
    "input_hash",
    "source",
    "parameters",
    "layers",
}
PARAMETER_KEYS = {"recipe", "glyphs", "anchor_names", "include_unnamed", "dx", "dy"}
UNSUPPORTED_PARAMETER_KEYS = {
    "all_sources",
    "apply",
    "authorization",
    "font_path",
    "save",
    "source_ids",
    "sources",
    "socket",
}


class RecipeError(Exception):
    """A user-visible recipe validation or bounded-scope failure."""

    def __init__(self, message: str, *, status: str = "error") -> None:
        super().__init__(message)
        self.status = status


def _fail(message: str, *, status: str = "error") -> NoReturn:
    raise RecipeError(message, status=status)


def _is_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def _finite_number(value: Any, label: str) -> float:
    if not _is_number(value) or not math.isfinite(float(value)):
        _fail(f"{label} must be a finite number")
    return float(value)


def _string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value or len(value.encode()) > MAX_STRING_BYTES:
        _fail(f"{label} must be a non-empty string of at most {MAX_STRING_BYTES} bytes")
    return value


def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        _fail(f"{label} must be an object")
    return value


def _exact_keys(value: dict[str, Any], allowed: set[str], label: str) -> None:
    unknown = sorted(set(value) - allowed)
    if unknown:
        if any(key in UNSUPPORTED_PARAMETER_KEYS for key in unknown):
            _fail(f"unsupported {label} field: {unknown[0]}", status="unsupported")
        _fail(f"unknown {label} field: {unknown[0]}")


def _string_list(value: Any, label: str) -> list[str]:
    if not isinstance(value, list):
        _fail(f"{label} must be an array")
    result = [_string(item, f"{label}[{index}]") for index, item in enumerate(value)]
    if len(set(result)) != len(result):
        _fail(f"{label} must not contain duplicates")
    return sorted(result)


def _guard(value: Any) -> dict[str, Any]:
    guard = _object(value, "layer guard")
    required = {"glyph", "glyph_id", "layer", "expected_revision"}
    if set(guard) != required:
        _fail("layer guard must contain exactly glyph, glyph_id, layer and expected_revision")
    return {
        "glyph": _string(guard["glyph"], "guard.glyph"),
        "glyph_id": _string(guard["glyph_id"], "guard.glyph_id"),
        "layer": _string(guard["layer"], "guard.layer"),
        "expected_revision": _string(guard["expected_revision"], "guard.expected_revision"),
    }


def _layers(value: Any) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        _fail("layers must be an array")
    if len(value) > MAX_GUARDED_LAYERS:
        _fail(
            f"captured scope contains {len(value)} layers; the limit is "
            f"{MAX_GUARDED_LAYERS}",
            status="unsupported",
        )

    result: list[dict[str, Any]] = []
    addresses: set[tuple[str, str]] = set()
    for layer_index, raw_layer in enumerate(value):
        layer = _object(raw_layer, f"layers[{layer_index}]")
        if set(layer) != {"guard", "width", "anchors"}:
            _fail("each layer must contain exactly guard, width and anchors")
        guard = _guard(layer["guard"])
        address = (guard["glyph"], guard["layer"])
        if address in addresses:
            _fail(f"duplicate guarded layer: {guard['glyph']} / {guard['layer']}")
        addresses.add(address)
        width = _finite_number(layer["width"], f"layers[{layer_index}].width")
        anchors_value = layer["anchors"]
        if not isinstance(anchors_value, list):
            _fail(f"layers[{layer_index}].anchors must be an array")
        anchors: list[dict[str, Any]] = []
        anchor_ids: set[str] = set()
        for anchor_index, raw_anchor in enumerate(anchors_value):
            anchor = _object(raw_anchor, f"anchors[{anchor_index}]")
            if set(anchor) - {"id", "name", "x", "y"}:
                _fail("anchors may contain only id, optional name, x and y")
            if "name" not in anchor and "id" not in anchor:
                _fail("anchors require id, x and y")
            if set(anchor) < {"id", "x", "y"}:
                _fail("anchors require id, x and y")
            anchor_id = _string(anchor["id"], f"anchors[{anchor_index}].id")
            if anchor_id in anchor_ids:
                _fail(f"duplicate anchor id in {guard['glyph']} / {guard['layer']}: {anchor_id}")
            anchor_ids.add(anchor_id)
            name = anchor.get("name")
            if name is not None and not isinstance(name, str):
                _fail(f"anchors[{anchor_index}].name must be a string or null")
            anchors.append(
                {
                    "id": anchor_id,
                    "name": name,
                    "x": _finite_number(anchor["x"], f"anchors[{anchor_index}].x"),
                    "y": _finite_number(anchor["y"], f"anchors[{anchor_index}].y"),
                }
            )
        result.append({"guard": guard, "width": width, "anchors": anchors})

    return sorted(result, key=lambda item: (item["guard"]["glyph"], item["guard"]["layer"]))


def _envelope(value: Any, recipe: str) -> tuple[dict[str, Any], list[dict[str, Any]], dict[str, Any]]:
    envelope = _object(value, "input")
    _exact_keys(envelope, TOP_LEVEL_KEYS, "input")
    if envelope.get("schema_version") != SCHEMA_VERSION:
        _fail(f"schema_version must be {SCHEMA_VERSION}")
    job_id = _string(envelope.get("job_id"), "job_id")
    input_hash = _string(envelope.get("input_hash"), "input_hash")
    source = envelope.get("source")
    if not isinstance(source, int) or isinstance(source, bool) or source < 0:
        _fail("source must be a non-negative integer")
    parameters = _object(envelope.get("parameters"), "parameters")
    _exact_keys(parameters, PARAMETER_KEYS, "parameters")
    if "recipe" in parameters and parameters["recipe"] != recipe:
        _fail("parameters.recipe does not match the selected recipe")
    glyphs = _string_list(parameters.get("glyphs", []), "parameters.glyphs")
    anchor_names = _string_list(parameters.get("anchor_names", []), "parameters.anchor_names")
    include_unnamed = parameters.get("include_unnamed", True)
    if not isinstance(include_unnamed, bool):
        _fail("parameters.include_unnamed must be a boolean")
    if recipe == "move_named_anchors":
        if "dx" not in parameters or "dy" not in parameters:
            _fail("move_named_anchors requires parameters.dx and parameters.dy")
        dx = _finite_number(parameters["dx"], "parameters.dx")
        dy = _finite_number(parameters["dy"], "parameters.dy")
    else:
        if "dx" in parameters or "dy" in parameters:
            _fail("list_anchors does not accept dx or dy", status="unsupported")
        dx = dy = 0.0
    layers = _layers(envelope.get("layers"))
    normalized = {
        "schema_version": SCHEMA_VERSION,
        "job_id": job_id,
        "input_hash": input_hash,
        "source": source,
        "glyphs": glyphs,
        "anchor_names": anchor_names,
        "include_unnamed": include_unnamed,
        "dx": dx,
        "dy": dy,
    }
    return (
        {"schema_version": SCHEMA_VERSION, "job_id": job_id, "input_hash": input_hash, "source": source},
        layers,
        normalized,
    )


def _selected_layers(layers: list[dict[str, Any]], parameters: dict[str, Any]) -> list[dict[str, Any]]:
    selected = set(parameters["glyphs"])
    return [layer for layer in layers if layer["guard"]["glyph"] in selected]


def _anchor_report(source: int, guard: dict[str, Any], anchor: dict[str, Any]) -> str:
    name = anchor["name"] if anchor["name"] is not None else "<unnamed>"
    return (
        f"glyph={guard['glyph']} source={source} layer={guard['layer']} "
        f"name={name} id={anchor['id']} x={anchor['x']!r} y={anchor['y']!r}"
    )


def _matching_anchors(
    layer: dict[str, Any], parameters: dict[str, Any], *, move: bool
) -> list[dict[str, Any]]:
    names = set(parameters["anchor_names"])
    include_unnamed = parameters["include_unnamed"]
    matches = []
    for anchor in layer["anchors"]:
        name = anchor["name"]
        if move and not name:
            continue
        if name is None and not include_unnamed:
            continue
        if names and name not in names:
            continue
        matches.append(anchor)
    return sorted(matches, key=lambda anchor: anchor["id"])


def run(recipe: str, value: Any) -> dict[str, Any]:
    """Run one recipe against an already decoded immutable envelope."""

    if recipe not in RECIPES:
        _fail(f"unknown recipe: {recipe}")
    identity, layers, parameters = _envelope(value, recipe)
    source = identity["source"]
    selected = _selected_layers(layers, parameters)
    reads: list[dict[str, Any]] = []
    edits: list[dict[str, Any]] = []
    report_lines: list[str] = []
    changed = 0
    for layer in selected:
        guard = layer["guard"]
        matches = _matching_anchors(layer, parameters, move=recipe == "move_named_anchors")
        report_lines.extend(_anchor_report(source, guard, anchor) for anchor in matches)
        if recipe != "move_named_anchors":
            reads.append(guard)
            continue
        operations = []
        for anchor in matches:
            x = anchor["x"] + parameters["dx"]
            y = anchor["y"] + parameters["dy"]
            if not math.isfinite(x) or not math.isfinite(y):
                _fail(f"move for anchor {anchor['id']} is not finite")
            if x == anchor["x"] and y == anchor["y"]:
                continue
            changed += 1
            operations.append(
                {"op": "set_anchor", "anchor_id": anchor["id"], "x": x, "y": y}
            )
        if operations:
            edits.append({"target": guard, "operations": operations})
        else:
            reads.append(guard)

    operation_count = sum(len(edit["operations"]) for edit in edits)
    if len(edits) + len(reads) > MAX_GUARDED_LAYERS or operation_count > MAX_OPERATIONS:
        _fail(
            f"proposal contains {len(edits) + len(reads)} guarded layers and "
            f"{operation_count} operations; "
            f"limits are {MAX_GUARDED_LAYERS} layers and {MAX_OPERATIONS} operations",
            status="unsupported",
        )

    reads.sort(key=lambda item: (item["glyph"], item["layer"]))
    edits.sort(key=lambda item: (item["target"]["glyph"], item["target"]["layer"]))
    for edit in edits:
        edit["operations"].sort(key=lambda operation: operation["anchor_id"])
    summary = (
        f"Listed {len(report_lines)} anchor(s)"
        if recipe == "list_anchors"
        else f"Proposed {changed} anchor move(s)"
    )
    report = "\n".join([summary, *report_lines])
    result: dict[str, Any] = {**identity, "report": report, "reads": reads, "edits": edits}
    return result


def error_result(recipe: str | None, value: Any, error: RecipeError) -> dict[str, Any]:
    """Return one envelope-shaped error result, retaining valid input identity."""

    if isinstance(value, dict):
        job_id = value.get("job_id") if isinstance(value.get("job_id"), str) else "invalid"
        input_hash = value.get("input_hash") if isinstance(value.get("input_hash"), str) else "invalid"
        schema_version = value.get("schema_version", SCHEMA_VERSION)
    else:
        job_id, input_hash, schema_version = "invalid", "invalid", SCHEMA_VERSION
    return {
        "schema_version": schema_version,
        "job_id": job_id,
        "input_hash": input_hash,
        "report": f"recipe={recipe or '<unknown>'} status={error.status}: {error}",
        "reads": [],
        "edits": [],
    }


def cli(recipe: str | None = None, argv: list[str] | None = None) -> int:
    """Read one JSON value from stdin and write exactly one JSON result."""

    del argv
    try:
        raw = sys.stdin.read()
        value = json.loads(raw)
        selected_recipe = recipe
        if selected_recipe is None and isinstance(value, dict):
            parameters = value.get("parameters")
            if isinstance(parameters, dict):
                selected_recipe = parameters.get("recipe")
        result = run(selected_recipe, value)
        print(f"recipe={selected_recipe} status=ok", file=sys.stderr)
        exit_code = 0
    except json.JSONDecodeError as error:
        recipe_error = RecipeError(f"input is not valid JSON: {error}")
        value = None
        result = error_result(recipe, value, recipe_error)
        print(str(recipe_error), file=sys.stderr)
        exit_code = 2
    except RecipeError as error:
        result = error_result(recipe, value if "value" in locals() else None, error)
        print(str(error), file=sys.stderr)
        exit_code = 2 if error.status == "error" else 0
    except (TypeError, ValueError) as error:
        recipe_error = RecipeError(f"input could not be processed: {error}")
        result = error_result(recipe, value if "value" in locals() else None, recipe_error)
        print(str(recipe_error), file=sys.stderr)
        exit_code = 2
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(cli(sys.argv[1] if len(sys.argv) == 2 else None))
