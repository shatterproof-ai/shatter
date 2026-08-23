from __future__ import annotations

import argparse
import contextlib
import os
import shutil
import stat
import subprocess
import tempfile
import textwrap
import time
import unittest
import importlib.util
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
WALKTHROUGH = REPO_ROOT / "demo" / "walkthrough.sh"
MODULE_PATH = REPO_ROOT / "scripts" / "examples_checkout.py"
SPEC = importlib.util.spec_from_file_location("examples_checkout", MODULE_PATH)
assert SPEC is not None
assert SPEC.loader is not None
examples_checkout = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = examples_checkout
SPEC.loader.exec_module(examples_checkout)


class WalkthroughExamplesCheckoutTest(unittest.TestCase):
    def test_ensure_existing_checkout_takes_shared_lock_once(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            checkout_dir = Path(tmp) / "examples"
            (checkout_dir / ".git").mkdir(parents=True)
            args = argparse.Namespace(fresh=False, no_update=False, cleanup=False)
            lock_depth = 0

            @contextlib.contextmanager
            def tracked_lock(path: Path):
                nonlocal lock_depth
                self.assertEqual(path, checkout_dir)
                self.assertEqual(lock_depth, 0, "shared checkout lock was acquired recursively")
                lock_depth += 1
                try:
                    yield
                finally:
                    lock_depth -= 1

            original_dir = examples_checkout.DEFAULT_DIR
            original_lock = examples_checkout._locked_shared_checkout
            original_run_git = examples_checkout.run_git
            examples_checkout.DEFAULT_DIR = checkout_dir
            examples_checkout._locked_shared_checkout = tracked_lock
            examples_checkout.run_git = lambda args, cwd=None: None
            try:
                self.assertEqual(
                    examples_checkout.ensure_examples_checkout(args),
                    checkout_dir.resolve(),
                )
            finally:
                examples_checkout.DEFAULT_DIR = original_dir
                examples_checkout._locked_shared_checkout = original_lock
                examples_checkout.run_git = original_run_git

    def test_fresh_shared_clone_is_marked_before_return(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            checkout_dir = Path(tmp) / "examples"
            args = argparse.Namespace(fresh=False, no_update=False, cleanup=False)

            def fake_clone(repo_url: str, target: Path) -> Path:
                self.assertEqual(target, checkout_dir)
                (target / ".git").mkdir(parents=True)
                return target.resolve()

            original_dir = examples_checkout.DEFAULT_DIR
            original_clone = examples_checkout.clone_checkout
            examples_checkout.DEFAULT_DIR = checkout_dir
            examples_checkout.clone_checkout = fake_clone
            try:
                examples_checkout.ensure_examples_checkout(args)
                self.assertTrue(
                    examples_checkout._refresh_marker_path(checkout_dir).exists(),
                    "shared clone must be marked fresh before consumers can use it",
                )
            finally:
                examples_checkout.DEFAULT_DIR = original_dir
                examples_checkout.clone_checkout = original_clone

    def test_cleanup_removes_checkout_and_marker_under_lock(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            checkout_dir = Path(tmp) / "examples"
            checkout_dir.mkdir()
            marker = examples_checkout._refresh_marker_path(checkout_dir)
            marker.touch()
            lock_held = False

            @contextlib.contextmanager
            def tracked_lock(path: Path):
                nonlocal lock_held
                self.assertEqual(path, checkout_dir)
                lock_held = True
                try:
                    yield
                finally:
                    lock_held = False

            original_lock = examples_checkout._locked_shared_checkout
            examples_checkout._locked_shared_checkout = tracked_lock
            try:
                examples_checkout.cleanup_checkout(checkout_dir)
                self.assertTrue(lock_held is False)
                self.assertFalse(checkout_dir.exists())
                self.assertFalse(marker.exists())
            finally:
                examples_checkout._locked_shared_checkout = original_lock

    def test_run_git_strips_leaked_actions_checkout_credentials(self) -> None:
        leaked_env = {
            "GIT_CONFIG_COUNT": "1",
            "GIT_CONFIG_KEY_0": "http.https://github.com/.extraheader",
            "GIT_CONFIG_VALUE_0": "AUTHORIZATION: basic ***",
        }
        original_environ = os.environ.copy()
        os.environ.update(leaked_env)
        captured: dict[str, object] = {}

        def fake_subprocess_run(*args, **kwargs):
            captured["env"] = kwargs.get("env")
            return subprocess.CompletedProcess(args, 0, stdout="", stderr="")

        original_run = subprocess.run
        subprocess.run = fake_subprocess_run
        try:
            examples_checkout.run_git(["status"])
        finally:
            subprocess.run = original_run
            os.environ.clear()
            os.environ.update(original_environ)

        passed_env = captured["env"]
        self.assertIsNotNone(passed_env)
        for key in leaked_env:
            self.assertNotIn(key, passed_env)

    def test_refresh_checkout_accepts_existing_git_checkout(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            checkout_dir = Path(tmp) / "examples"
            (checkout_dir / ".git").mkdir(parents=True)
            recorded_calls: list[tuple[list[str], Path | None]] = []

            def fake_run_git(args: list[str], cwd: Path | None = None) -> None:
                recorded_calls.append((args, cwd))

            original_run_git = examples_checkout.run_git
            examples_checkout.run_git = fake_run_git
            try:
                resolved = examples_checkout.refresh_checkout(checkout_dir)
            finally:
                examples_checkout.run_git = original_run_git

            self.assertEqual(resolved, checkout_dir.resolve())
            self.assertEqual(
                recorded_calls,
                [
                    (["fetch", "--quiet", "origin", examples_checkout.DEFAULT_BRANCH], checkout_dir),
                    (["checkout", "--quiet", examples_checkout.DEFAULT_BRANCH], checkout_dir),
                    (
                        ["reset", "--hard", f"origin/{examples_checkout.DEFAULT_BRANCH}"],
                        checkout_dir,
                    ),
                    (["clean", "-fdx"], checkout_dir),
                ],
            )

    def test_refresh_checkout_skips_git_calls_within_freshness_window(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            checkout_dir = Path(tmp) / "examples"
            (checkout_dir / ".git").mkdir(parents=True)
            recorded_calls: list[tuple[list[str], Path | None]] = []

            def fake_run_git(args: list[str], cwd: Path | None = None) -> None:
                recorded_calls.append((args, cwd))

            original_run_git = examples_checkout.run_git
            examples_checkout.run_git = fake_run_git
            try:
                examples_checkout.refresh_checkout(checkout_dir)
                self.assertEqual(len(recorded_calls), 4)
                # A second refresh within the freshness window must not
                # re-fetch/reset/clean — this is what protects a concurrent
                # in-flight consumer from having the checkout yanked out
                # from under it.
                examples_checkout.refresh_checkout(checkout_dir)
                self.assertEqual(len(recorded_calls), 4)
            finally:
                examples_checkout.run_git = original_run_git

    def test_refresh_checkout_refreshes_again_after_window_expires(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            checkout_dir = Path(tmp) / "examples"
            (checkout_dir / ".git").mkdir(parents=True)
            recorded_calls: list[tuple[list[str], Path | None]] = []

            def fake_run_git(args: list[str], cwd: Path | None = None) -> None:
                recorded_calls.append((args, cwd))

            original_run_git = examples_checkout.run_git
            examples_checkout.run_git = fake_run_git
            try:
                examples_checkout.refresh_checkout(checkout_dir)
                self.assertEqual(len(recorded_calls), 4)
                marker = examples_checkout._refresh_marker_path(checkout_dir)
                stale_time = time.time() - examples_checkout.REFRESH_WINDOW_SECONDS - 1
                os.utime(marker, (stale_time, stale_time))
                examples_checkout.refresh_checkout(checkout_dir)
                self.assertEqual(len(recorded_calls), 8)
            finally:
                examples_checkout.run_git = original_run_git

    def test_clones_clean_examples_into_temp_dir(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "demo").mkdir()
            shutil.copy2(WALKTHROUGH, root / "demo" / "walkthrough.sh")
            (root / "examples" / "rust" / "target").mkdir(parents=True)
            (root / "examples" / "rust" / "target" / "CACHEDIR.TAG").write_text(
                "leftover build dir",
                encoding="utf-8",
            )

            fake_bin = root / "bin"
            fake_bin.mkdir()
            git_log = root / "git.log"
            fake_git = fake_bin / "git"
            fake_git.write_text(
                textwrap.dedent(
                    f"""\
                    #!/usr/bin/env bash
                    set -euo pipefail
                    printf '%s\\n' "$*" >> "{git_log}"
                    if [[ "${{1:-}}" == "submodule" ]]; then
                        echo "unexpected submodule init" >&2
                        exit 97
                    fi
                    if [[ "${{1:-}}" == "clone" ]]; then
                        dest="${{@: -1}}"
                        mkdir -p "$dest/standalone/ts"
                        printf 'export function classifyNumber() {{ return 0; }}\\n' > "$dest/standalone/ts/01-arithmetic.ts"
                        exit 0
                    fi
                    exit 0
                    """
                ),
                encoding="utf-8",
            )
            fake_git.chmod(fake_git.stat().st_mode | stat.S_IEXEC)

            env = os.environ.copy()
            env["PATH"] = f"{fake_bin}:{env['PATH']}"

            result = subprocess.run(
                ["bash", "demo/walkthrough.sh"],
                cwd=root,
                env=env,
                text=True,
                capture_output=True,
            )

            self.assertNotEqual(result.returncode, 0)
            combined_output = result.stdout + result.stderr
            self.assertIn("shatter binary not found", combined_output)
            self.assertNotIn("Initializing examples submodule", combined_output)

            git_calls = git_log.read_text(encoding="utf-8") if git_log.exists() else ""
            self.assertNotIn("submodule update --init examples", git_calls)


if __name__ == "__main__":
    unittest.main()
