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
            fixture.control.side_effect = [state(430), state(412), broken, state(430), state(430)]
            runner = harness.Harness(Path("unused"), Path("unused"), Path("unused"), fixture)
            with self.assertRaises(RuntimeError):
                runner.run_fixture_controls(412, 430)

    def test_missing_proof_artifact_fails_the_run(self) -> None:
        runner = harness.Harness(Path("unused"), Path("unused"), Path("unused"))
        with self.assertRaises(RuntimeError):
            runner.write_proof_artifact({"ok": True})


if __name__ == "__main__":
    unittest.main()
