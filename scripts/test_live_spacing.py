#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Focused recovery tests for the procedural live spacing example."""

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch


MODULE_PATH = Path(__file__).with_name("live_spacing.py")
SPEC = importlib.util.spec_from_file_location("live_spacing", MODULE_PATH)
assert SPEC and SPEC.loader
spacing = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = spacing
SPEC.loader.exec_module(spacing)


REQUEST = {
    "expected_document_epoch": "epoch",
    "actor": "spacing-example",
    "operation_key": "spacing-01",
}


def receipt(outcome: str, history_state: str | None) -> spacing.ToolResponse:
    """Make a successful receipt lookup with a chosen original outcome and current state."""

    return spacing.ToolResponse(
        {
            "ok": True,
            "history_state": history_state,
            "receipt": {"outcome": {"status": outcome}},
        },
        0,
        "",
    )


class RecoveryTests(unittest.TestCase):
    def reconcile(self, status: spacing.ToolResponse, *, history: str | None = None) -> tuple[int, dict]:
        """Run reconciliation with a mocked receipt lookup and capture its JSON report."""

        output = io.StringIO()
        ambiguous = spacing.ToolResponse(None, None, "", "response timeout")
        with contextlib.redirect_stdout(output), patch.object(spacing, "receipt", return_value=status):
            code = spacing.reconcile_ambiguous(
                Path("/binary"),
                Path("/session"),
                REQUEST,
                "agent_apply",
                ambiguous,
                required_history_state=history,
            )
        return code, json.loads(output.getvalue())

    def test_rejected_receipt_does_not_make_lost_apply_successful(self) -> None:
        code, report = self.reconcile(receipt("rejected", None))
        self.assertEqual(code, 4)
        self.assertFalse(report["ok"])
        self.assertEqual(report["receipt_outcome"], "rejected")

    def test_undone_history_state_reconciles_lost_undo(self) -> None:
        code, report = self.reconcile(receipt("committed", "undone"), history="undone")
        self.assertEqual(code, 0)
        self.assertTrue(report["ok"])
        self.assertEqual(report["receipt"]["history_state"], "undone")


if __name__ == "__main__":
    unittest.main()
