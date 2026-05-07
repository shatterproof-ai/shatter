"""Tests for scripts/protocol-codegen.py — determinism + check-mode fixture.

Verifies the skeleton invariants for str-1hlk.6 and the TypeScript emitter
added in str-1hlk.7:
  * The generator produces byte-identical output across runs (determinism).
  * `--check` exits 0 when every checked-in artifact matches the contract.
  * `--check` exits non-zero with a diff on stderr when any artifact drifts.
  * The TS enum module's command/status/error-code unions stay aligned with
    the registry.
"""

from __future__ import annotations

import re
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
CHECKED_IN_TS_ENUMS = (
    REPO_ROOT / "shatter-ts" / "src" / "generated" / "protocol-enums.ts"
)


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


class TypeScriptEmitterTest(unittest.TestCase):
    """str-1hlk.7: the TS emitter participates in --write/--check."""

    def _registry_keys(self, top: str) -> list[str]:
        import yaml  # local import: keeps test importable without pyyaml

        with REGISTRY.open("r", encoding="utf-8") as fh:
            data = yaml.safe_load(fh)
        return sorted((data.get(top) or {}).keys())

    def _ts_const_tuple(self, source: str, name: str) -> list[str]:
        match = re.search(
            rf"export const {name} = \[\s*((?:\".*?\",\s*)+)\] as const;",
            source,
            re.DOTALL,
        )
        if not match:
            self.fail(f"could not find {name} in TS enum module")
        return re.findall(r'"([^"]+)"', match.group(1))

    def test_ts_enum_module_is_checked_in(self) -> None:
        self.assertTrue(
            CHECKED_IN_TS_ENUMS.is_file(),
            f"missing checked-in TS enum module at {CHECKED_IN_TS_ENUMS}",
        )

    def test_ts_enum_module_passes_check(self) -> None:
        """--check must succeed against the checked-in TS module."""
        proc = _run(["--check"])
        self.assertEqual(
            proc.returncode,
            0,
            f"--check failed against checked-in artifacts:\n"
            f"stdout:\n{proc.stdout}\nstderr:\n{proc.stderr}",
        )

    def test_ts_emitter_is_deterministic(self) -> None:
        """Running --ts-out twice must produce byte-identical output."""
        with tempfile.TemporaryDirectory() as tmp_a, tempfile.TemporaryDirectory() as tmp_b:
            out_a = Path(tmp_a) / "protocol-enums.ts"
            out_b = Path(tmp_b) / "protocol-enums.ts"
            for out in (out_a, out_b):
                proc = _run(
                    [
                        "--registry",
                        str(REGISTRY),
                        "--ts-out",
                        str(out),
                        "--write",
                    ]
                )
                self.assertEqual(
                    proc.returncode, 0, f"generator failed: {proc.stderr}"
                )
            self.assertEqual(
                out_a.read_bytes(),
                out_b.read_bytes(),
                "TS emitter output is not deterministic",
            )

    def test_ts_check_mode_fails_on_drift(self) -> None:
        """--check must exit non-zero with a diff when the TS module drifts."""
        with tempfile.TemporaryDirectory() as tmp:
            stale = Path(tmp) / "protocol-enums.ts"
            shutil.copyfile(CHECKED_IN_TS_ENUMS, stale)
            stale.write_text(
                stale.read_text(encoding="utf-8").replace(
                    '"analyze"', '"DRIFT_analyze"', 1
                ),
                encoding="utf-8",
            )
            proc = _run(
                [
                    "--registry",
                    str(REGISTRY),
                    "--ts-out",
                    str(stale),
                    "--check",
                ]
            )
            self.assertNotEqual(
                proc.returncode,
                0,
                "expected --check to fail on drifted TS module",
            )
            combined = proc.stdout + proc.stderr
            self.assertIn(
                "DRIFT_analyze",
                combined,
                "expected unified diff mentioning the drifted token",
            )

    def test_command_union_matches_registry(self) -> None:
        """Generated ALL_COMMANDS must equal sorted registry command keys."""
        source = CHECKED_IN_TS_ENUMS.read_text(encoding="utf-8")
        ts_commands = self._ts_const_tuple(source, "ALL_COMMANDS")
        registry_commands = self._registry_keys("commands")
        self.assertEqual(ts_commands, registry_commands)

    def test_error_code_union_matches_registry(self) -> None:
        source = CHECKED_IN_TS_ENUMS.read_text(encoding="utf-8")
        ts_codes = self._ts_const_tuple(source, "ALL_ERROR_CODES")
        registry_codes = self._registry_keys("error_codes")
        self.assertEqual(ts_codes, registry_codes)

    def test_response_status_union_includes_error(self) -> None:
        """ResponseStatus must include the universal "error" status."""
        source = CHECKED_IN_TS_ENUMS.read_text(encoding="utf-8")
        ts_statuses = self._ts_const_tuple(source, "ALL_RESPONSE_STATUSES")
        self.assertIn("error", ts_statuses)


if __name__ == "__main__":
    unittest.main()
