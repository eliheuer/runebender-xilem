# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Run the installed sketch model with one pinned, calibrated outline input."""

import hashlib
import json
import os
import sys


def required(name):
    value = os.environ.pop(name, None)
    if not value:
        raise RuntimeError(f"missing private launcher value {name}")
    return value


ops_path = required("RUNEBENDER_PRETRACE_OPS")
expected_hash = required("RUNEBENDER_PRETRACE_SHA256")
expected_png = os.path.realpath(required("RUNEBENDER_PRETRACE_PNG"))
expected_glyph = required("RUNEBENDER_PRETRACE_GLYPH")
expected_advance = float(required("RUNEBENDER_PRETRACE_ADVANCE"))
expected_metrics = tuple(
    float(required(name))
    for name in (
        "RUNEBENDER_PRETRACE_HEIGHT",
        "RUNEBENDER_PRETRACE_BOTTOM",
        "RUNEBENDER_PRETRACE_LEFT",
    )
)
repository = required("RUNEBENDER_GLYPHLAB_REPOSITORY")
with open(ops_path, "rb") as source:
    body = source.read()
if "sha256:" + hashlib.sha256(body).hexdigest() != expected_hash:
    raise RuntimeError("calibrated model input changed before invocation")
ops = json.loads(body)
if not isinstance(ops, list) or not ops:
    raise RuntimeError("calibrated model input has no pen operations")
arity = {"moveTo": 1, "lineTo": 1, "curveTo": 3, "closePath": 0}
for operation in ops:
    if (
        not isinstance(operation, list)
        or len(operation) != 2
        or operation[0] not in arity
        or len(operation[1]) != arity[operation[0]]
    ):
        raise RuntimeError("calibrated model input has an unsupported pen operation")

sys.path.insert(0, repository)
from glyphlab import sketch2glyph  # noqa: E402

used = False


def calibrated_trace(png_path, glyph, width, height, left, bottom):
    global used
    if (
        used
        or os.path.realpath(png_path) != expected_png
        or glyph != expected_glyph
        or float(width) != expected_advance
        or (float(height), float(bottom), float(left)) != expected_metrics
    ):
        raise RuntimeError("installed model requested a different trace input")
    used = True
    return [(name, points) for name, points in ops]


# Only the imported trace slot is replaced. Sampling, scoring, decoding and
# GLIF writing remain the installed sketch2glyph implementation.
sketch2glyph.trace = calibrated_trace
sketch2glyph.main()
if not used:
    raise RuntimeError("installed model did not consume calibrated trace input")
