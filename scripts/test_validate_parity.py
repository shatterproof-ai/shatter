"""Tests for scripts/validate-parity.py."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("validate-parity.py")
SPEC = importlib.util.spec_from_file_location("validate_parity", MODULE_PATH)
assert SPEC is not None
assert SPEC.loader is not None
validate_parity = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = validate_parity
SPEC.loader.exec_module(validate_parity)


class LoadMatrixTest(unittest.TestCase):
    def test_rejects_duplicate_keys(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            matrix = Path(tmp) / "parity-matrix.yaml"
            matrix.write_text(
                """
version: "0.1.0"
commands: {}
commands:
  handshake:
    status: required
    frontends: {}
complex_type_capabilities: {}
""".lstrip()
            )

            with self.assertRaises(SystemExit):
                validate_parity.load_matrix(matrix)


if __name__ == "__main__":
    unittest.main()
