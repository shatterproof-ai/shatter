"""Regression test for scripts/git_sandbox_test_lib.py (str-jttrf).

git hooks export GIT_DIR/GIT_WORK_TREE pointing at the real invoking repo
before running any hook script. Passing `cwd=` to `subprocess.run` does not
override an explicit GIT_DIR, so a Python test that builds a throwaway
fixture repo without sanitizing its environment silently targets the real
repo instead. This proves `sanitized_git_env()` neutralizes a pre-set
GIT_DIR (and friends) so a subsequently built fixture repo cannot see or
mutate a real repo the leaked env points at.
"""

from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from scripts.git_sandbox_test_lib import GIT_SANDBOX_ENV_VARS, sanitized_git_env


def _git(args: list[str], cwd: Path, env: dict[str, str] | None = None) -> str:
    return subprocess.run(
        ["git", *args],
        cwd=cwd,
        env=env,
        text=True,
        capture_output=True,
        check=True,
    ).stdout.strip()


class SanitizedGitEnvTests(unittest.TestCase):
    def test_strips_every_sandbox_env_var(self) -> None:
        base = {var: "/leaked/path" for var in GIT_SANDBOX_ENV_VARS}
        base["PATH"] = "/usr/bin"
        result = sanitized_git_env(base)
        self.assertEqual(result, {"PATH": "/usr/bin"})

    def test_defaults_to_os_environ(self) -> None:
        original = os.environ.get("GIT_DIR")
        os.environ["GIT_DIR"] = "/leaked/path"
        self.addCleanup(
            lambda: (
                os.environ.pop("GIT_DIR", None)
                if original is None
                else os.environ.__setitem__("GIT_DIR", original)
            )
        )
        self.assertNotIn("GIT_DIR", sanitized_git_env())


class LeakedGitDirIsolationTests(unittest.TestCase):
    """End-to-end proof: a fixture built with sanitized_git_env() cannot see
    or mutate a real repo even when GIT_DIR/GIT_WORK_TREE are preset to it."""

    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.scratch = Path(self.tmp.name)

        self.real_repo = self.scratch / "real-repo"
        self.real_repo.mkdir()
        _git(["init", "-q"], self.real_repo)
        _git(["config", "user.email", "test@example.com"], self.real_repo)
        _git(["config", "user.name", "Test"], self.real_repo)
        (self.real_repo / "README").write_text("real\n", encoding="utf-8")
        _git(["add", "README"], self.real_repo)
        _git(["commit", "-q", "-m", "real repo init"], self.real_repo)
        self.real_head_before = _git(["rev-parse", "HEAD"], self.real_repo)

        leaked_env = {
            "GIT_DIR": str(self.real_repo / ".git"),
            "GIT_WORK_TREE": str(self.real_repo),
            "GIT_INDEX_FILE": str(self.real_repo / ".git" / "index"),
            "GIT_OBJECT_DIRECTORY": str(self.real_repo / ".git" / "objects"),
            "GIT_ALTERNATE_OBJECT_DIRECTORIES": str(self.real_repo / ".git" / "objects"),
            "GIT_COMMON_DIR": str(self.real_repo / ".git"),
        }
        originals = {key: os.environ.get(key) for key in leaked_env}
        os.environ.update(leaked_env)

        def _restore() -> None:
            for key, value in originals.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

        self.addCleanup(_restore)

    def test_setup_reproduces_the_leak(self) -> None:
        # Sanity check: an un-sanitized git call from an unrelated cwd really
        # does resolve to the leaked real repo -- proves this test's
        # simulated hook environment reproduces the bug being guarded
        # against, rather than trivially passing.
        toplevel = _git(["rev-parse", "--show-toplevel"], self.scratch)
        self.assertEqual(Path(toplevel).resolve(), self.real_repo.resolve())

    def test_sanitized_env_isolates_a_fixture_from_the_leaked_real_repo(self) -> None:
        fixture = self.scratch / "fixture"
        fixture.mkdir()
        env = sanitized_git_env()

        _git(["init", "-q"], fixture, env=env)
        _git(["config", "user.email", "test@example.com"], fixture, env=env)
        _git(["config", "user.name", "Test"], fixture, env=env)
        _git(["checkout", "-q", "-b", "sandbox-marker-branch"], fixture, env=env)
        (fixture / "marker.txt").write_text("fixture\n", encoding="utf-8")
        _git(["add", "marker.txt"], fixture, env=env)
        _git(["commit", "-q", "-m", "fixture commit"], fixture, env=env)

        fixture_toplevel = _git(["rev-parse", "--show-toplevel"], fixture, env=env)
        self.assertEqual(Path(fixture_toplevel).resolve(), fixture.resolve())

        real_branches = _git(["branch", "--list"], self.real_repo, env=env)
        self.assertNotIn("sandbox-marker-branch", real_branches)

        real_head_after = _git(["rev-parse", "HEAD"], self.real_repo, env=env)
        self.assertEqual(real_head_after, self.real_head_before)


if __name__ == "__main__":
    unittest.main()
