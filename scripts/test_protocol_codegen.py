"""Tests for scripts/protocol-codegen.py — determinism + check-mode fixture.

Verifies the skeleton invariants for str-1hlk.6:
  * The generator produces byte-identical output across runs (determinism).
  * `--check` exits 0 when the checked-in artifact matches the contract.
  * `--check` exits non-zero with a diff on stderr when the artifact drifts.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT = REPO_ROOT / "scripts" / "protocol-codegen.py"
REGISTRY = REPO_ROOT / "protocol" / "registry.yaml"
CHECKED_IN_MANIFEST = REPO_ROOT / "protocol" / "generated" / "manifest.json"


def _run(args: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), *args],
        capture_output=True,
        text=True,
        check=False,
    )


class ProtocolCodegenTest(unittest.TestCase):
    def test_script_exists(self) -> None:
        self.assertTrue(SCRIPT.is_file(), f"missing {SCRIPT}")

    def test_deterministic_output(self) -> None:
        """Running the generator twice must produce byte-identical output."""
        with tempfile.TemporaryDirectory() as tmp_a, tempfile.TemporaryDirectory() as tmp_b:
            out_a = Path(tmp_a) / "manifest.json"
            out_b = Path(tmp_b) / "manifest.json"
            for out in (out_a, out_b):
                proc = _run(
                    ["--registry", str(REGISTRY), "--out", str(out), "--write"]
                )
                self.assertEqual(
                    proc.returncode, 0, f"generator failed: {proc.stderr}"
                )
            self.assertEqual(
                out_a.read_bytes(),
                out_b.read_bytes(),
                "generator output is not deterministic",
            )

    def test_check_mode_passes_on_checked_in_artifact(self) -> None:
        """--check must succeed against the checked-in manifest."""
        self.assertTrue(
            CHECKED_IN_MANIFEST.is_file(),
            f"missing checked-in manifest at {CHECKED_IN_MANIFEST}",
        )
        proc = _run(["--check"])
        self.assertEqual(
            proc.returncode,
            0,
            f"--check failed against checked-in manifest:\n"
            f"stdout:\n{proc.stdout}\nstderr:\n{proc.stderr}",
        )

    def test_check_mode_fails_on_drift(self) -> None:
        """--check must exit non-zero with a diff when the artifact drifts."""
        with tempfile.TemporaryDirectory() as tmp:
            stale = Path(tmp) / "manifest.json"
            shutil.copyfile(CHECKED_IN_MANIFEST, stale)
            # Mutate the artifact to simulate drift.
            stale.write_text(
                stale.read_text(encoding="utf-8").replace(
                    '"protocol_version"', '"DRIFT_protocol_version"', 1
                ),
                encoding="utf-8",
            )
            proc = _run(
                [
                    "--registry",
                    str(REGISTRY),
                    "--out",
                    str(stale),
                    "--check",
                ]
            )
            self.assertNotEqual(
                proc.returncode, 0, "expected --check to fail on drifted manifest"
            )
            combined = proc.stdout + proc.stderr
            self.assertIn(
                "DRIFT_protocol_version",
                combined,
                "expected unified diff mentioning the drifted token",
            )


if __name__ == "__main__":
    unittest.main()
