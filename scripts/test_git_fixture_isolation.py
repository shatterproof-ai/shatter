#!/usr/bin/env python3
"""Run Git fixture entrypoints against a disposable caller, never the checkout."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
ENTRYPOINTS = (
    ("bash", "scripts/test_target_dir_report_json.sh"),
    ("bash", "scripts/test_cleanup_merged_remote_branches.sh"),
    ("bash", "scripts/test_git_sandbox_test_lib.sh"),
    (sys.executable, "-m", "unittest",
     "scripts.test_git_sandbox_test_lib.LeakedGitDirIsolationTests"),
    (sys.executable, "-m", "unittest",
     "scripts.test_affected_gates.AffectedGateWiringTests.test_governed_executor_propagates_selector_git_failure"),
    (sys.executable, "-m", "unittest",
     "scripts.test_gate_receipt.InputRejectionTests.test_commit_oid_is_not_accepted_as_a_tree"),
    (sys.executable, "-m", "unittest",
     "scripts.test_walkthrough_examples_checkout.WalkthroughExamplesCheckoutTest.test_published_snapshot_worktree_is_read_only_but_git_metadata_works"),
)


def snapshot(root: Path) -> dict[str, tuple[int, str]]:
    """Include config, refs, index, objects, worktree registration and dirty files."""
    result = {}
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            content = os.readlink(path).encode()
        elif path.is_file():
            content = path.read_bytes()
        else:
            continue
        result[str(path.relative_to(root))] = (
            stat.S_IMODE(path.lstat().st_mode), hashlib.sha256(content).hexdigest()
        )
    return result


class GitFixtureIsolationTests(unittest.TestCase):
    def test_entrypoints_preserve_the_callers_git_state(self) -> None:
        # Bootstrap independently of the sanitizer under test. Even its broken
        # version must only be able to damage this disposable sentinel.
        clean_env = {key: value for key, value in os.environ.items()
                     if not key.startswith("GIT_")}
        clean_env.update({"GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull,
                          "GIT_CONFIG_SYSTEM": os.devnull, "GIT_OPTIONAL_LOCKS": "0",
                          "GIT_TERMINAL_PROMPT": "0"})
        for command in ENTRYPOINTS:
            for contamination in ("repository", "configuration"):
                with self.subTest(command=command, contamination=contamination):
                    with tempfile.TemporaryDirectory(prefix="git-fixture-isolation-") as tmp:
                        caller = Path(tmp) / "caller"
                        caller.mkdir()

                        def git(*args: str) -> None:
                            subprocess.run(["git", "-C", str(caller), *args],
                                           env=clean_env, check=True, capture_output=True)

                        git("init", "-q", "-b", "main")
                        git("config", "user.name", "Caller")
                        git("config", "user.email", "caller@example.invalid")
                        readme = caller / "README"
                        readme.write_text("committed\n")
                        git("add", "README")
                        git("commit", "-q", "-m", "caller sentinel")
                        readme.write_text("staged caller work\n")
                        git("add", "README")
                        readme.write_text("staged caller work\nunstaged caller work\n")
                        (caller / "untracked.txt").write_text("untracked caller work\n")
                        before = snapshot(caller)
                        env = dict(clean_env)
                        if contamination == "repository":
                            git_dir = caller / ".git"
                            env.update({
                                "GIT_DIR": str(git_dir), "GIT_WORK_TREE": str(caller),
                                "GIT_INDEX_FILE": str(git_dir / "index"),
                                "GIT_COMMON_DIR": str(git_dir),
                                "GIT_OBJECT_DIRECTORY": str(git_dir / "objects"),
                                "GIT_ALTERNATE_OBJECT_DIRECTORIES": str(git_dir / "objects"),
                            })
                        else:
                            env.update({
                                "GIT_CONFIG": str(caller / ".git" / "config"),
                                "GIT_CONFIG_COUNT": "1", "GIT_CONFIG_KEY_0": "core.bare",
                                "GIT_CONFIG_VALUE_0": "true",
                                "GIT_CONFIG_PARAMETERS": "'core.bare=true'",
                                "GIT_CONFIG_KEY_7": "unused.payload",
                                "GIT_CONFIG_VALUE_7": "must not propagate",
                            })
                        result = subprocess.run(command, cwd=ROOT, env=env, text=True,
                                                capture_output=True, timeout=60)
                        self.assertEqual(snapshot(caller), before,
                                         "fixture modified its caller's repository")
                        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
