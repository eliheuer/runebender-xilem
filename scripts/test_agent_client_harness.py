# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Unit tests for the bounded client harness's local, non-transport logic."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock


MODULE_PATH = Path(__file__).with_name("agent_client_harness.py")
SPEC = importlib.util.spec_from_file_location("agent_client_harness", MODULE_PATH)
assert SPEC and SPEC.loader
harness = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(harness)


class HarnessTests(unittest.TestCase):
    def test_redaction_removes_paths_credentials_and_svg_payload(self) -> None:
        value = {
            "path": str(Path.home() / "font.ufo"),
            "authorization": "user-approved",
            "svg_content": "<svg>fixture</svg>",
        }
        result = harness.redact(value)
        self.assertNotIn(str(Path.home()), json.dumps(result))
        self.assertEqual(result["authorization"], "<redacted>")
        self.assertTrue(result["svg_content"]["redacted"])

    def test_schema_digest_is_order_independent(self) -> None:
        self.assertEqual(
            harness.sha256_json({"a": 1, "b": 2}),
            harness.sha256_json({"b": 2, "a": 1}),
        )

    def test_required_schema_is_receipt_backed(self) -> None:
        self.assertEqual(
            harness.LIVE_REQUIRED_TOOLS,
            {
                "project_info",
                "editor_context",
                "read_glyph",
                "agent_apply",
                "agent_receipt",
                "agent_history",
            },
        )
        self.assertNotIn("propose_edits", harness.LIVE_REQUIRED_TOOLS)
        self.assertNotIn("proposal_install", harness.LIVE_REQUIRED_TOOLS)

    def test_compiled_proof_is_reported_pending_without_image_trial(self) -> None:
        reason = harness.PENDING_CAPABILITIES["compiled_proof"]
        self.assertIn("not frozen", reason)
        self.assertIn("no image trial", reason)

    def test_failed_live_response_is_not_accepted_as_success(self) -> None:
        with self.assertRaises(RuntimeError):
            harness.Harness.require_ok({"ok": False, "error": "stale"}, "stale_write")

    def test_expected_error_categories_are_checked(self) -> None:
        harness.Harness.require_error_category(
            {"ok": False, "error": "explicit user authorization required"},
            "authorization_guard",
            "explicit user authorization required",
        )
        with self.assertRaises(RuntimeError):
            harness.Harness.require_error_category(
                {"ok": False, "error": "unexpected"}, "stale_write", "stale revision"
            )

    def test_receipt_negative_cases_are_checked(self) -> None:
        harness.Harness.require_error_code(
            {"ok": False, "error_code": "authorization_required"},
            "authorization_guard",
            "authorization_required",
        )
        harness.Harness.require_rejected_receipt(
            {
                "ok": False,
                "receipt": {"outcome": {"status": "rejected", "error": "guarded layer changed"}},
            },
            "stale_write",
            "guarded layer changed",
        )

    def test_probe_only_does_not_require_a_live_endpoint(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            result = harness.probe_clients()
            self.assertIsInstance(result, dict)
            self.assertEqual(Path(directory).exists(), True)

    def test_fixture_mismatch_and_source_write_fail_the_run(self) -> None:
        def state(width, *, source_exists=False):
            return {"ok": True, "canonical_advance": width, "cache_advance": width,
                    "session_advance": width, "source_exists": source_exists}

        for broken in (state(999), state(412, source_exists=True)):
            fixture = Mock()
            fixture.control.side_effect = [broken, state(430), state(412), broken, state(430)]
            runner = harness.Harness(Path("unused"), Path("unused"), Path("unused"), fixture)
            with self.assertRaises(RuntimeError):
                runner.run_fixture_controls(state(412), 412, 430)

    def test_fixture_undo_redo_requires_source_path_absence(self) -> None:
        def state(width, *, source_exists=False):
            return {"ok": True, "canonical_advance": width, "cache_advance": width,
                    "session_advance": width, "source_exists": source_exists}

        fixture = Mock()
        fixture.control.side_effect = [state(430), state(412), state(412), state(430), state(430)]
        runner = harness.Harness(Path("unused"), Path("unused"), Path("unused"), fixture)
        runner.run_fixture_controls(state(412), 412, 430)
        self.assertEqual(runner.fixture_summary["status"], "pass")
        self.assertFalse(runner.fixture_summary["source_path_exists_before"])
        self.assertFalse(runner.fixture_summary["source_path_exists_after_redo"])


if __name__ == "__main__":
    unittest.main()
