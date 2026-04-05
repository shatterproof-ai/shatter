from __future__ import annotations

import importlib.util
import json
import sys
import unittest
from pathlib import Path


SCRIPT_PATH = (
    Path(__file__).resolve().parents[1] / "scripts" / "discover_targets.py"
)
SPEC = importlib.util.spec_from_file_location("discover_targets", SCRIPT_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


FIXTURES_DIR = Path(__file__).resolve().parent / "fixtures"


class DiscoverTargetsTest(unittest.TestCase):
    def test_discovers_nested_targets_and_excludes_vendored_paths(self) -> None:
        report = MODULE.build_report(FIXTURES_DIR / "mixed-repo")

        self.assertEqual(report["target_count"], 3)
        self.assertFalse(report["requires_confirmation"])
        self.assertIn("docs/CI-INTEGRATION.md", report["docs_scanned"])
        self.assertEqual(report["doc_hints"][0]["type"], "taskfile")

        targets = {target["root"]: target for target in report["targets"]}
        self.assertEqual(set(targets), {"go-service", "rust-lib", "ts-app"})
        self.assertIn("node_modules", report["excluded_paths"])
        self.assertIn("vendor", report["excluded_paths"])

        ts_surfaces = targets["ts-app"]["surfaces"]
        self.assertEqual(ts_surfaces[0]["type"], "package-json-scripts")
        self.assertEqual(ts_surfaces[0]["scope"], "local")
        self.assertEqual(ts_surfaces[1]["type"], "taskfile")
        self.assertEqual(ts_surfaces[1]["scope"], "ancestor")

        rust_surfaces = targets["rust-lib"]["surfaces"]
        self.assertEqual(rust_surfaces[0]["type"], "taskfile")
        self.assertEqual(rust_surfaces[0]["scope"], "ancestor")

    def test_requires_confirmation_when_more_than_four_targets_exist(self) -> None:
        report = MODULE.build_report(FIXTURES_DIR / "over-four-repo")

        self.assertEqual(report["target_count"], 5)
        self.assertTrue(report["requires_confirmation"])

    def test_json_output_is_serializable(self) -> None:
        report = MODULE.build_report(FIXTURES_DIR / "mixed-repo")
        payload = json.dumps(report, sort_keys=True)
        self.assertIn("\"target_count\": 3", payload)


if __name__ == "__main__":
    unittest.main()
