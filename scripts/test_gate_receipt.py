"""Contract tests for the gate receipt v1 writer (str-35vtk.22)."""

from __future__ import annotations

from pathlib import Path
import unittest

import yaml


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "gate-receipt.py"
SPEC = ROOT / "docs" / "perf" / "gate-receipt-v1.md"
TASKFILE = ROOT / "Taskfile.yml"


class ReceiptSurfaceTests(unittest.TestCase):
    def test_writer_script_exists_and_is_executable(self) -> None:
        self.assertTrue(SCRIPT.is_file(), f"missing {SCRIPT}")
        self.assertTrue(SCRIPT.stat().st_mode & 0o111, f"not executable: {SCRIPT}")

    def test_normative_spec_records_schema_digest_and_storage_contract(self) -> None:
        text = SPEC.read_text()
        for phrase in (
            "Gate Receipt v1",
            "candidate_tree",
            "base_tree",
            "keys sorted recursively",
            "scripts/gate-wrapper.sh",
            "scripts/gate-receipt.py",
            "shatter-gate-receipts/v1",
            "mode 0700",
            "mode 0600",
            "atomically",
        ):
            with self.subTest(phrase=phrase):
                self.assertIn(phrase, text)

    def test_meta_runs_receipt_contract_tests(self) -> None:
        tasks = yaml.safe_load(TASKFILE.read_text())["tasks"]
        self.assertIn("scripts/gate-receipt.py", tasks["meta"]["sources"])
        self.assertIn("scripts/test_gate_receipt.py", tasks["meta"]["sources"])
        self.assertIn(
            "python3 -m unittest scripts.test_gate_receipt",
            tasks["meta"]["cmds"],
        )


if __name__ == "__main__":
    unittest.main()
