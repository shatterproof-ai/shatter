from __future__ import annotations

import argparse
import concurrent.futures
import contextlib
import importlib.util
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import textwrap
import threading
import time
import unittest
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
    @contextlib.contextmanager
    def _using_snapshot_cache(self, cache_dir: Path):
        had_original = hasattr(examples_checkout, "SNAPSHOT_CACHE_DIR")
        original = getattr(examples_checkout, "SNAPSHOT_CACHE_DIR", None)
        examples_checkout.SNAPSHOT_CACHE_DIR = cache_dir
        try:
            yield
        finally:
            if had_original:
                examples_checkout.SNAPSHOT_CACHE_DIR = original
            else:
                del examples_checkout.SNAPSHOT_CACHE_DIR

    def _init_git_checkout(self, checkout_dir: Path, content: str = "version one\n") -> None:
        checkout_dir.mkdir(parents=True)
        subprocess.run(
            ["git", "init", "--quiet", "--initial-branch", examples_checkout.DEFAULT_BRANCH],
            cwd=checkout_dir,
            check=True,
        )
        subprocess.run(
            ["git", "config", "user.email", "test@example.com"],
            cwd=checkout_dir,
            check=True,
        )
        subprocess.run(
            ["git", "config", "user.name", "Test User"],
            cwd=checkout_dir,
            check=True,
        )
        (checkout_dir / "example.txt").write_text(content, encoding="utf-8")
        subprocess.run(["git", "add", "example.txt"], cwd=checkout_dir, check=True)
        subprocess.run(
            ["git", "commit", "--quiet", "-m", "fixture"],
            cwd=checkout_dir,
            check=True,
        )

    def test_ensure_existing_checkout_takes_shared_lock_once(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            checkout_dir = Path(tmp) / "examples"
            snapshot_cache = Path(tmp) / "snapshots"
            self._init_git_checkout(checkout_dir)
            examples_checkout._refresh_marker_path(checkout_dir).touch()
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
            examples_checkout.DEFAULT_DIR = checkout_dir
            examples_checkout._locked_shared_checkout = tracked_lock
            try:
                with self._using_snapshot_cache(snapshot_cache):
                    snapshot = examples_checkout.ensure_examples_checkout(args)
                self.assertNotEqual(snapshot, checkout_dir.resolve())
                self.assertEqual(
                    (snapshot / "example.txt").read_text(encoding="utf-8"),
                    "version one\n",
                )
            finally:
                examples_checkout.DEFAULT_DIR = original_dir
                examples_checkout._locked_shared_checkout = original_lock

    def test_fresh_shared_clone_is_marked_before_return(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            checkout_dir = Path(tmp) / "examples"
            snapshot_cache = Path(tmp) / "snapshots"
            args = argparse.Namespace(fresh=False, no_update=False, cleanup=False)

            def fake_clone(repo_url: str, target: Path) -> Path:
                self.assertEqual(target, checkout_dir)
                self._init_git_checkout(target)
                return target.resolve()

            original_dir = examples_checkout.DEFAULT_DIR
            original_clone = examples_checkout.clone_checkout
            examples_checkout.DEFAULT_DIR = checkout_dir
            examples_checkout.clone_checkout = fake_clone
            try:
                with self._using_snapshot_cache(snapshot_cache):
                    examples_checkout.ensure_examples_checkout(args)
                self.assertTrue(
                    examples_checkout._refresh_marker_path(checkout_dir).exists(),
                    "shared clone must be marked fresh before consumers can use it",
                )
            finally:
                examples_checkout.DEFAULT_DIR = original_dir
                examples_checkout.clone_checkout = original_clone

    def test_same_head_reuses_one_published_snapshot(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            checkout_dir = Path(tmp) / "examples"
            snapshot_cache = Path(tmp) / "snapshots"
            self._init_git_checkout(checkout_dir)
            examples_checkout._refresh_marker_path(checkout_dir).touch()
            args = argparse.Namespace(fresh=False, no_update=True, cleanup=False)
            clone_count = 0

            def counting_git(args: list[str], cwd: Path | None = None) -> str:
                nonlocal clone_count
                if args and args[0] == "clone":
                    clone_count += 1
                return original_run_git(args, cwd)

            original_dir = examples_checkout.DEFAULT_DIR
            original_run_git = examples_checkout.run_git
            examples_checkout.DEFAULT_DIR = checkout_dir
            examples_checkout.run_git = counting_git
            try:
                with self._using_snapshot_cache(snapshot_cache):
                    first = examples_checkout.ensure_examples_checkout(args)
                    second = examples_checkout.ensure_examples_checkout(args)
                self.assertEqual(first, second)
                self.assertEqual(clone_count, 1)
                self.assertEqual(
                    [
                        path
                        for path in snapshot_cache.iterdir()
                        if not path.name.startswith(".")
                    ],
                    [first.parent],
                )
            finally:
                examples_checkout.DEFAULT_DIR = original_dir
                examples_checkout.run_git = original_run_git

    def test_published_snapshot_worktree_is_read_only_but_git_metadata_works(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            checkout_dir = Path(tmp) / "examples"
            snapshot_cache = Path(tmp) / "snapshots"
            self._init_git_checkout(checkout_dir)
            executable = checkout_dir / "tool.sh"
            executable.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            executable.chmod(0o755)
            subprocess.run(["git", "add", "tool.sh"], cwd=checkout_dir, check=True)
            subprocess.run(
                ["git", "commit", "--quiet", "--amend", "-m", "fixture"],
                cwd=checkout_dir,
                check=True,
            )
            examples_checkout._refresh_marker_path(checkout_dir).touch()
            args = argparse.Namespace(fresh=False, no_update=True, cleanup=False)

            original_dir = examples_checkout.DEFAULT_DIR
            examples_checkout.DEFAULT_DIR = checkout_dir
            try:
                with self._using_snapshot_cache(snapshot_cache):
                    snapshot = examples_checkout.ensure_examples_checkout(args)

                self.assertEqual(snapshot.stat().st_mode & 0o222, 0)
                self.assertEqual((snapshot / "example.txt").stat().st_mode & 0o222, 0)
                self.assertEqual((snapshot / "tool.sh").stat().st_mode & 0o222, 0)
                self.assertNotEqual((snapshot / "tool.sh").stat().st_mode & 0o111, 0)
                with self.assertRaises(PermissionError):
                    (snapshot / "example.txt").write_text("mutated\n", encoding="utf-8")

                status = subprocess.run(
                    ["git", "status", "--porcelain"],
                    cwd=snapshot,
                    check=True,
                    text=True,
                    capture_output=True,
                )
                self.assertEqual(status.stdout, "")
                metadata_probe = snapshot / ".git" / "write-probe"
                metadata_probe.write_text("ok\n", encoding="utf-8")
                self.assertEqual(metadata_probe.read_text(encoding="utf-8"), "ok\n")
            finally:
                examples_checkout.DEFAULT_DIR = original_dir

    def test_failed_snapshot_clone_leaves_no_partial_published_revision(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            checkout_dir = Path(tmp) / "examples"
            snapshot_cache = Path(tmp) / "snapshots"
            self._init_git_checkout(checkout_dir)
            examples_checkout._refresh_marker_path(checkout_dir).touch()
            args = argparse.Namespace(fresh=False, no_update=True, cleanup=False)

            def failing_clone(args: list[str], cwd: Path | None = None) -> str:
                if args and args[0] == "clone":
                    destination = Path(args[-1])
                    destination.mkdir(parents=True)
                    (destination / "partial").touch()
                    raise SystemExit("simulated interrupted clone")
                return original_run_git(args, cwd)

            original_dir = examples_checkout.DEFAULT_DIR
            original_run_git = examples_checkout.run_git
            examples_checkout.DEFAULT_DIR = checkout_dir
            examples_checkout.run_git = failing_clone
            try:
                with self._using_snapshot_cache(snapshot_cache):
                    with self.assertRaisesRegex(SystemExit, "interrupted clone"):
                        examples_checkout.ensure_examples_checkout(args)
                self.assertTrue(snapshot_cache.is_dir())
                self.assertEqual(list(snapshot_cache.iterdir()), [])
            finally:
                examples_checkout.DEFAULT_DIR = original_dir
                examples_checkout.run_git = original_run_git

    def test_no_update_waits_for_clone_then_validates_under_lock(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            checkout_dir = Path(tmp) / "examples"
            args = argparse.Namespace(fresh=False, no_update=True, cleanup=False)
            clone_started = threading.Event()
            finish_clone = threading.Event()

            def incomplete_clone() -> None:
                with examples_checkout._locked_shared_checkout(checkout_dir):
                    checkout_dir.mkdir()
                    clone_started.set()
                    finish_clone.wait(timeout=5)

            original_dir = examples_checkout.DEFAULT_DIR
            examples_checkout.DEFAULT_DIR = checkout_dir
            clone_thread = threading.Thread(target=incomplete_clone)
            clone_thread.start()
            self.assertTrue(clone_started.wait(timeout=5))
            try:
                with concurrent.futures.ThreadPoolExecutor(max_workers=1) as executor:
                    reader = executor.submit(examples_checkout.ensure_examples_checkout, args)
                    with self.assertRaises(concurrent.futures.TimeoutError):
                        reader.result(timeout=0.1)
                    finish_clone.set()
                    with self.assertRaisesRegex(SystemExit, "not a git repository"):
                        reader.result(timeout=5)
            finally:
                finish_clone.set()
                clone_thread.join(timeout=5)
                examples_checkout.DEFAULT_DIR = original_dir

    def test_returned_snapshot_survives_refresh_after_freshness_expiry(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            checkout_dir = Path(tmp) / "examples"
            snapshot_cache = Path(tmp) / "snapshots"
            self._init_git_checkout(checkout_dir)
            examples_checkout._refresh_marker_path(checkout_dir).touch()
            no_update = argparse.Namespace(fresh=False, no_update=True, cleanup=False)
            update = argparse.Namespace(fresh=False, no_update=False, cleanup=False)

            original_dir = examples_checkout.DEFAULT_DIR
            original_run_git = examples_checkout.run_git
            examples_checkout.DEFAULT_DIR = checkout_dir
            try:
                with self._using_snapshot_cache(snapshot_cache):
                    first_snapshot = examples_checkout.ensure_examples_checkout(no_update)
                self.assertNotEqual(first_snapshot, checkout_dir.resolve())
                self.assertEqual(
                    (first_snapshot / "example.txt").read_text(encoding="utf-8"),
                    "version one\n",
                )

                marker = examples_checkout._refresh_marker_path(checkout_dir)
                stale_time = time.time() - examples_checkout.REFRESH_WINDOW_SECONDS - 1
                os.utime(marker, (stale_time, stale_time))

                def refreshing_git(
                    args: list[str], cwd: Path | None = None
                ) -> str | None:
                    if args[:2] in (["fetch", "--quiet"], ["checkout", "--quiet"]):
                        return
                    if args[:2] == ["reset", "--hard"]:
                        assert cwd == checkout_dir
                        (checkout_dir / "example.txt").write_text(
                            "version two\n", encoding="utf-8"
                        )
                        subprocess.run(["git", "add", "example.txt"], cwd=cwd, check=True)
                        subprocess.run(
                            ["git", "commit", "--quiet", "--amend", "-m", "refreshed"],
                            cwd=cwd,
                            check=True,
                        )
                        return
                    if args == ["clean", "-fdx"]:
                        return
                    return original_run_git(args, cwd)

                examples_checkout.run_git = refreshing_git
                with self._using_snapshot_cache(snapshot_cache):
                    second_snapshot = examples_checkout.ensure_examples_checkout(update)

                self.assertNotEqual(second_snapshot, first_snapshot)
                self.assertEqual(
                    (second_snapshot / "example.txt").read_text(encoding="utf-8"),
                    "version two\n",
                )
                self.assertEqual(
                    (first_snapshot / "example.txt").read_text(encoding="utf-8"),
                    "version one\n",
                    "a later refresh must not mutate an in-flight consumer's snapshot",
                )
                self.assertEqual(
                    len(
                        [
                            path
                            for path in snapshot_cache.iterdir()
                            if not path.name.startswith(".")
                        ]
                    ),
                    2,
                )
            finally:
                examples_checkout.DEFAULT_DIR = original_dir
                examples_checkout.run_git = original_run_git

    def test_cleanup_removes_checkout_and_marker_under_lock(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            checkout_dir = Path(tmp) / "examples"
            snapshot_cache = Path(tmp) / "snapshots"
            checkout_dir.mkdir()
            marker = examples_checkout._refresh_marker_path(checkout_dir)
            marker.touch()
            (snapshot_cache / "commit" / "examples").mkdir(parents=True)
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
                with self._using_snapshot_cache(snapshot_cache):
                    examples_checkout.cleanup_checkout(checkout_dir)
                self.assertTrue(lock_held is False)
                self.assertFalse(checkout_dir.exists())
                self.assertFalse(marker.exists())
                self.assertFalse(snapshot_cache.exists())
            finally:
                examples_checkout._locked_shared_checkout = original_lock

    def test_cleanup_script_tmp_uses_locked_helper_for_examples_cache(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            fixture_root = Path(tmp) / "project"
            fixture_scripts = fixture_root / "scripts"
            fixture_scripts.mkdir(parents=True)
            shutil.copy2(REPO_ROOT / "scripts" / "cleanup.sh", fixture_scripts)
            shutil.copy2(MODULE_PATH, fixture_scripts)

            tmp_root = Path(tmp) / "tmp"
            tmp_root.mkdir()
            checkout_dir = tmp_root / "shatter-examples-main"
            checkout_dir.mkdir()
            marker = tmp_root / ".shatter-examples-main.last-refresh"
            marker.touch()
            snapshot_cache = tmp_root / "shatter-examples-snapshots"
            (snapshot_cache / "commit" / "examples").mkdir(parents=True)

            # Keep the broad temp-file globs non-destructive so this test
            # isolates caches that must be removed by Python's locked helper.
            fake_bin = Path(tmp) / "bin"
            fake_bin.mkdir()
            fake_rm = fake_bin / "rm"
            fake_rm.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            fake_rm.chmod(0o755)
            env = os.environ.copy()
            env["PATH"] = f"{fake_bin}:{env['PATH']}"
            env["TMPDIR"] = str(tmp_root)

            with examples_checkout._locked_shared_checkout(checkout_dir):
                cleanup = subprocess.Popen(
                    ["bash", str(fixture_scripts / "cleanup.sh"), "--tmp"],
                    cwd=fixture_root,
                    env=env,
                    text=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                )
                time.sleep(0.2)
                if cleanup.poll() is not None:
                    early_stdout, early_stderr = cleanup.communicate()
                    self.fail(
                        "cleanup must wait for the examples checkout lock; "
                        f"stdout={early_stdout!r} stderr={early_stderr!r}"
                    )

            stdout, stderr = cleanup.communicate(timeout=5)
            self.assertEqual(cleanup.returncode, 0, stderr)
            self.assertIn("no active examples readers", stdout)
            self.assertFalse(checkout_dir.exists())
            self.assertFalse(marker.exists())
            self.assertFalse(snapshot_cache.exists())

    def test_run_git_strips_leaked_repo_context_and_checkout_credentials(self) -> None:
        leaked_env = {
            "GIT_CONFIG_COUNT": "1",
            "GIT_CONFIG_KEY_0": "http.https://github.com/.extraheader",
            "GIT_CONFIG_VALUE_0": "AUTHORIZATION: basic ***",
            "GIT_COMMON_DIR": "/repo/.git",
            "GIT_DIR": "/repo/.git",
            "GIT_INDEX_FILE": "/repo/.git/index",
            "GIT_OBJECT_DIRECTORY": "/repo/.git/objects",
            "GIT_WORK_TREE": "/repo",
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
