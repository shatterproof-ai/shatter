"""Fixture tests for the land-work project verifier (str-35vtk.24).

Exercises scripts/land_work_verifier.sh against a disposable Git repository
with a fake `task` binary standing in for the real gate, so these tests
never run a real product gate. They prove:

  * exactly one `task check` invocation runs (the old test-standard/parity/
    conformance trio is gone);
  * a passing gate writes a local-tier gate receipt (via
    scripts/gate-receipt.py) whose candidate_tree/base_tree match
    `git rev-parse HEAD^{tree}` / `git rev-parse "$(git merge-base HEAD
    origin/main)^{tree}"` exactly;
  * a missing origin/main fails verification before the gate ever runs;
  * a receipt-write failure fails verification without losing the gate's
    own stdout/stderr.
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from scripts.git_sandbox_test_lib import sanitized_git_env


ROOT = Path(__file__).resolve().parents[1]
VERIFIER = ROOT / "scripts" / "land_work_verifier.sh"
GATE_RECEIPT = ROOT / "scripts" / "gate-receipt.py"

TASK_STDOUT_SENTINEL = "TASK-STDOUT-SENTINEL-9f21\n"
TASK_STDERR_SENTINEL = "TASK-STDERR-SENTINEL-b47c\n"

TOOL_OUTPUTS = {
    "cargo": "cargo 1.91.0 (fixture)",
    "go": "go version go1.25.0 fixture",
    "node": "v24.1.0",
    "npm": "11.4.0",
    "rustc": "rustc 1.91.0 (fixture)",
}

OLD_TRIO_TOKENS = ("test-standard", "task parity", "task conformance")


class VerifierFixture:
    """Disposable repo + fake-tool PATH for exercising land_work_verifier.sh."""

    def __init__(self, *, with_origin_main: bool = True) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.repo = self.root / "repo"
        self.runtime = self.root / "runtime"
        self.fake_bin = self.root / "bin"
        self.task_log = self.root / "task-invocations.log"
        self.repo.mkdir()
        self.runtime.mkdir(mode=0o700)
        self.fake_bin.mkdir()

        self._git("init", "--quiet", "-b", "main")
        self._git("config", "user.email", "fixture@example.test")
        self._git("config", "user.name", "Fixture")

        self._write("Taskfile.yml", b"version: '3'\ntasks: {}\n")
        self._git("add", "-A")
        self._git("commit", "--quiet", "-m", "base")
        self.base_commit = self._rev_parse("HEAD")
        self.base_tree = self._rev_parse("HEAD^{tree}")

        if with_origin_main:
            self._git("update-ref", "refs/remotes/origin/main", self.base_commit)

        self._write("Taskfile.yml", b"version: '3'\ntasks: {check: {}}\n")
        self._write(
            "scripts/gate-wrapper.sh", b"#!/bin/sh\nexec \"$@\"\n", executable=True
        )
        self._write(
            "scripts/gate-receipt.py", GATE_RECEIPT.read_bytes(), executable=True
        )
        self._write(
            "scripts/land_work_verifier.sh", VERIFIER.read_bytes(), executable=True
        )
        self._git("add", "-A")
        self._git("commit", "--quiet", "-m", "candidate")
        self.candidate_commit = self._rev_parse("HEAD")
        self.candidate_tree = self._rev_parse("HEAD^{tree}")

        self._install_fake_tools()

    def close(self) -> None:
        self._tmp.cleanup()

    def _git(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["git", *args],
            cwd=self.repo,
            env=sanitized_git_env(),
            text=True,
            capture_output=True,
            check=True,
        )

    def _rev_parse(self, ref: str) -> str:
        return self._git("rev-parse", ref).stdout.strip()

    def _write(self, relative: str, data: bytes, *, executable: bool = False) -> None:
        path = self.repo / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        if executable:
            path.chmod(0o755)

    def _install_fake_tools(self) -> None:
        real_git = subprocess.run(
            ["sh", "-c", "command -v git"], text=True, capture_output=True, check=True
        ).stdout.strip()
        git_shim = self.fake_bin / "git"
        git_shim.write_text(f'#!/bin/sh\nexec "{real_git}" "$@"\n')
        git_shim.chmod(0o755)

        for name, output in TOOL_OUTPUTS.items():
            tool = self.fake_bin / name
            expected_arg = "version" if name == "go" else "--version"
            tool.write_text(
                "#!/bin/sh\n"
                f'if [ "$#" -eq 1 ] && [ "$1" = "{expected_arg}" ]; then\n'
                f"  printf '  %s  \\n' '{output}'\n"
                "  exit 0\n"
                "fi\n"
                "exit 3\n"
            )
            tool.chmod(0o755)

        task = self.fake_bin / "task"
        task.write_text(
            "#!/bin/sh\n"
            'if [ "$#" -eq 1 ] && [ "$1" = "--version" ]; then\n'
            "  printf '  Task version: v3.44.1  \\n'\n"
            "  exit 0\n"
            "fi\n"
            'printf \'%s\\n\' "$*" >> "$SHATTER_TEST_TASK_LOG"\n'
            'printf \'%s\' "$SHATTER_TEST_TASK_STDOUT"\n'
            'printf \'%s\' "$SHATTER_TEST_TASK_STDERR" >&2\n'
            'exit "${SHATTER_TEST_TASK_EXIT:-0}"\n'
        )
        task.chmod(0o755)

    @property
    def env(self) -> dict[str, str]:
        import os

        env = sanitized_git_env()
        env.update(
            {
                "PATH": f"{self.fake_bin}:{os.environ.get('PATH', '')}",
                "XDG_RUNTIME_DIR": str(self.runtime),
                "SHATTER_TEST_TASK_LOG": str(self.task_log),
                "SHATTER_TEST_TASK_STDOUT": TASK_STDOUT_SENTINEL,
                "SHATTER_TEST_TASK_STDERR": TASK_STDERR_SENTINEL,
                "SHATTER_TEST_TASK_EXIT": "0",
            }
        )
        return env

    def run(
        self, extra_env: dict[str, str] | None = None
    ) -> subprocess.CompletedProcess[str]:
        env = self.env
        if extra_env:
            env.update(extra_env)
        return subprocess.run(
            ["bash", str(self.repo / "scripts" / "land_work_verifier.sh")],
            cwd=self.repo,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )

    def task_invocations(self) -> list[str]:
        if not self.task_log.exists():
            return []
        return [line for line in self.task_log.read_text().splitlines() if line]

    def receipt_files(self) -> list[Path]:
        return sorted(self.runtime.rglob(f"{self.candidate_tree}.json"))


def final_result(stdout: str) -> dict:
    lines = [line for line in stdout.splitlines() if line.strip()]
    return json.loads(lines[-1])


class LandWorkVerifierTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fixture = VerifierFixture()
        self.addCleanup(self.fixture.close)

    def test_failing_gate_runs_task_check_exactly_once_no_old_trio(self) -> None:
        result = self.fixture.run(
            extra_env={
                "SHATTER_TEST_TASK_EXIT": "7",
            }
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.fixture.task_invocations(), ["check"])
        self.assertIn(TASK_STDOUT_SENTINEL.strip(), result.stdout)
        self.assertIn(TASK_STDERR_SENTINEL.strip(), result.stderr)

        final = final_result(result.stdout)
        self.assertEqual(final["schema_version"], 1)
        self.assertEqual(final["status"], "failed")
        names = [check["name"] for check in final["selected_checks"]]
        self.assertEqual(names, ["task check"])

        source = VERIFIER.read_text()
        for token in OLD_TRIO_TOKENS:
            self.assertNotIn(token, source, f"old trio token {token!r} still present")

    def test_missing_origin_main_fails_before_gate_runs(self) -> None:
        fixture = VerifierFixture(with_origin_main=False)
        self.addCleanup(fixture.close)
        result = fixture.run()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(fixture.task_invocations(), [])
        final = final_result(result.stdout)
        self.assertEqual(final["schema_version"], 1)
        self.assertEqual(final["status"], "failed")

    def test_success_writes_local_receipt_with_exact_trees(self) -> None:
        result = self.fixture.run()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.fixture.task_invocations(), ["check"])
        self.assertIn(TASK_STDOUT_SENTINEL.strip(), result.stdout)
        self.assertIn(TASK_STDERR_SENTINEL.strip(), result.stderr)

        final = final_result(result.stdout)
        self.assertEqual(final["status"], "passed")

        receipts = self.fixture.receipt_files()
        self.assertEqual(len(receipts), 1)
        receipt = json.loads(receipts[0].read_text())
        self.assertEqual(receipt["schema"], 1)
        self.assertEqual(receipt["candidate_tree"], self.fixture.candidate_tree)
        self.assertEqual(receipt["base_tree"], self.fixture.base_tree)
        self.assertEqual(receipt["tier"], "local")
        self.assertEqual(len(receipt["gate_results"]), 1)
        gate_result = receipt["gate_results"][0]
        self.assertEqual(set(gate_result), {"gate", "argv", "started_at", "ended_at", "exit_code"})
        self.assertEqual(gate_result["gate"], "task check")
        self.assertEqual(gate_result["argv"], ["task", "check"])
        self.assertEqual(gate_result["exit_code"], 0)

    def test_receipt_write_failure_fails_verification_but_preserves_gate_output(
        self,
    ) -> None:
        # gate-receipt.py's ensure_private_directory() must mkdir this path;
        # pre-creating it as a plain file makes the write step fail
        # deterministically without relying on filesystem permission checks
        # (which some sandboxes/containers run as root and bypass).
        (self.fixture.runtime / "shatter-gate-receipts").touch()

        result = self.fixture.run()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.fixture.task_invocations(), ["check"])
        self.assertIn(TASK_STDOUT_SENTINEL.strip(), result.stdout)
        self.assertIn(TASK_STDERR_SENTINEL.strip(), result.stderr)

        final = final_result(result.stdout)
        self.assertEqual(final["schema_version"], 1)
        self.assertEqual(final["status"], "failed")
        self.assertEqual(self.fixture.receipt_files(), [])


if __name__ == "__main__":
    unittest.main()
