#!/usr/bin/env python3
"""Regression tests for root workspace Cargo task phases.

The fixture keeps this test independent of the repository's real Rust build:
it replaces Cargo with a logger and supplies no-op frontend Taskfiles.  That
lets us assert the argv and process count that go-task actually executes,
including its source-cache invalidation behavior.
"""

from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


REPO_ROOT = Path(__file__).resolve().parents[1]
TASKFILE = REPO_ROOT / "Taskfile.yml"
PHASES = {
    "build": ["build", "--workspace"],
    "test": ["test", "--workspace"],
    "lint": ["clippy", "--workspace", "--", "-D", "warnings"],
}


class RootCargoPhaseTests(unittest.TestCase):
    def test_root_phases_run_one_workspace_cargo_and_invalidate_for_llm(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            fixture = Path(temp_dir)
            shutil.copy2(TASKFILE, fixture / "Taskfile.yml")
            self._write_fixture_tree(fixture)

            log = fixture / "cargo.log"
            environment = os.environ | {
                "PATH": f"{fixture / 'bin'}:{os.environ['PATH']}",
                "CARGO_LOG": str(log),
            }
            for task, argv in PHASES.items():
                self._run_task(fixture, environment, task)
                self.assertEqual(self._logged_argv(log), [argv])

                # No source changes means go-task skips the cached root phase.
                self._run_task(fixture, environment, task)
                self.assertEqual(self._logged_argv(log), [argv])

                llm_source = fixture / "shatter-llm" / "src" / "lib.rs"
                # go-task fingerprints source content, so a touch writes a
                # harmless marker rather than relying on mtime granularity.
                llm_source.write_text(f"// touch invalidation for {task}\n")
                self._run_task(fixture, environment, task)
                self.assertEqual(self._logged_argv(log), [argv, argv])
                log.unlink()

                llm_source.write_text(f"// LLM-only failure fixture for {task}\n")
                (fixture / ".llm-only-failure").touch()
                failed = self._run_task(
                    fixture,
                    environment,
                    task,
                    check=False,
                    force=True,
                )
                self.assertNotEqual(failed.returncode, 0)
                self.assertEqual(self._logged_argv(log), [argv])
                log.unlink()
                (fixture / ".llm-only-failure").unlink()

    @staticmethod
    def _run_task(
        fixture: Path,
        environment: dict[str, str],
        task: str,
        *,
        check: bool = True,
        force: bool = False,
    ) -> subprocess.CompletedProcess[str]:
        completed = subprocess.run(
            ["task", *( ["--force"] if force else [] ), task],
            cwd=fixture,
            env=environment,
            text=True,
            capture_output=True,
        )
        if check and completed.returncode:
            raise AssertionError(
                f"task {task} failed:\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
            )
        return completed

    @staticmethod
    def _logged_argv(log: Path) -> list[list[str]]:
        if not log.exists():
            return []
        return [line.split("\x1f") for line in log.read_text().splitlines()]

    @staticmethod
    def _write_fixture_tree(fixture: Path) -> None:
        for path in ("Cargo.toml", "Cargo.lock", "shatter-llm/src/lib.rs"):
            destination = fixture / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.touch()

        examples_checkout = fixture / "scripts" / "examples_checkout.py"
        examples_checkout.parent.mkdir()
        examples_checkout.write_text("print('/tmp')\n")

        for directory in (
            "shatter-core",
            "shatter-cli",
            "shatter-ts",
            "shatter-go",
            "shatter-rust",
            "shatter-rust-runtime",
        ):
            taskfile = fixture / directory / "Taskfile.yml"
            taskfile.parent.mkdir(parents=True, exist_ok=True)
            taskfile.write_text("version: '3'\ntasks:\n  build:\n    cmds: [true]\n  test:\n    cmds: [true]\n  clippy:\n    cmds: [true]\n  vet:\n    cmds: [true]\n")

        cargo = fixture / "bin" / "cargo"
        cargo.parent.mkdir()
        cargo.write_text(
            "#!/bin/sh\n"
            "printf '%s\\n' \"$*\" | tr ' ' '\\037' >> \"$CARGO_LOG\"\n"
            "if [ -f .llm-only-failure ] && [ \"$2\" = \"--workspace\" ]; then\n"
            "  exit 42\n"
            "fi\n"
        )
        cargo.chmod(0o755)


if __name__ == "__main__":
    unittest.main()
