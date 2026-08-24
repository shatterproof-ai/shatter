#!/usr/bin/env python3
"""Resolve or prepare the external examples checkout."""

from __future__ import annotations

import argparse
import contextlib
import fcntl
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from pathlib import Path


DEFAULT_REPO_URL = "https://github.com/shatterproof-ai/examples.git"
DEFAULT_BRANCH = "main"
DEFAULT_DIR = Path(tempfile.gettempdir()) / "shatter-examples-main"
SNAPSHOT_CACHE_DIR = Path(tempfile.gettempdir()) / "shatter-examples-snapshots"
SNAPSHOT_PERMISSIONS_MARKER = ".shatter-read-only-v1"
GIT_LOCAL_ENV_VARS = (
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_CONFIG",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
    "GIT_DIR",
    "GIT_GRAFT_FILE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PREFIX",
    "GIT_REPLACE_REF_BASE",
    "GIT_SHALLOW_FILE",
    "GIT_WORK_TREE",
)

# Skip the network fetch + reset + clean if the canonical checkout was refreshed
# more recently than this. Concurrent `task` invocations (e.g. two `task
# smoke` runs, or smoke + e2e) each call this script independently; without a
# freshness window every one of them redundantly refreshes the canonical copy.
# Consumers receive independent snapshots, so correctness does not depend on
# this optimization window outliving their work. See str-35vtk.4.
REFRESH_WINDOW_SECONDS = 600


def _sanitized_git_env() -> dict[str, str]:
    # actions/checkout injects the job's GITHUB_TOKEN as an http.extraheader
    # override via GIT_CONFIG_COUNT/GIT_CONFIG_KEY_n/GIT_CONFIG_VALUE_n env
    # vars so it's inherited by later steps. Since examples.git is a
    # different, public repo, that leaked auth header gets sent anyway and
    # is rejected, producing a bogus "could not read Username" failure.
    env = os.environ.copy()
    # Hooks export repository-local variables. Letting those leak into the
    # examples clone makes Git operate on the parent Shatter repository.
    for key in GIT_LOCAL_ENV_VARS:
        env.pop(key, None)
    for key in list(env):
        if key.startswith("GIT_CONFIG_KEY_") or key.startswith("GIT_CONFIG_VALUE_"):
            del env[key]
    env["GIT_TERMINAL_PROMPT"] = "0"
    return env


def run_git(args: list[str], cwd: Path | None = None) -> str:
    completed = subprocess.run(
        ["git", *args],
        cwd=cwd,
        env=_sanitized_git_env(),
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if completed.returncode == 0:
        return completed.stdout.strip()
    detail = completed.stderr.strip() or completed.stdout.strip() or "git command failed"
    raise SystemExit(detail)


def clone_checkout(repo_url: str, checkout_dir: Path) -> Path:
    checkout_dir.parent.mkdir(parents=True, exist_ok=True)
    run_git(["clone", "--quiet", "--branch", DEFAULT_BRANCH, repo_url, str(checkout_dir)])
    return checkout_dir.resolve()


def _shared_lock_path(checkout_dir: Path) -> Path:
    return checkout_dir.parent / f".{checkout_dir.name}.lock"


def _refresh_marker_path(checkout_dir: Path) -> Path:
    return checkout_dir.parent / f".{checkout_dir.name}.last-refresh"


@contextlib.contextmanager
def _locked_shared_checkout(checkout_dir: Path):
    # Serializes every process that touches the shared checkout (clone,
    # refresh, and the freshness check itself) so concurrent `task`
    # invocations never run `git fetch`/`reset --hard`/`clean -fdx` against
    # the same working tree at once, and never race to clone into it.
    lock_path = _shared_lock_path(checkout_dir)
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    with open(lock_path, "w", encoding="utf-8") as lock_file:
        fcntl.flock(lock_file, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(lock_file, fcntl.LOCK_UN)


def _is_recently_refreshed(checkout_dir: Path) -> bool:
    try:
        age = time.time() - _refresh_marker_path(checkout_dir).stat().st_mtime
    except FileNotFoundError:
        return False
    return age < REFRESH_WINDOW_SECONDS


def _validate_shared_checkout_locked(checkout_dir: Path) -> None:
    if not (checkout_dir / ".git").exists():
        raise SystemExit(
            f"examples checkout path exists but is not a git repository: {checkout_dir}"
        )


def _refresh_checkout_locked(checkout_dir: Path) -> Path:
    _validate_shared_checkout_locked(checkout_dir)
    if _is_recently_refreshed(checkout_dir):
        return checkout_dir.resolve()
    run_git(["fetch", "--quiet", "origin", DEFAULT_BRANCH], cwd=checkout_dir)
    run_git(["checkout", "--quiet", DEFAULT_BRANCH], cwd=checkout_dir)
    run_git(["reset", "--hard", f"origin/{DEFAULT_BRANCH}"], cwd=checkout_dir)
    run_git(["clean", "-fdx"], cwd=checkout_dir)
    _refresh_marker_path(checkout_dir).touch()
    return checkout_dir.resolve()


def refresh_checkout(checkout_dir: Path) -> Path:
    with _locked_shared_checkout(checkout_dir):
        return _refresh_checkout_locked(checkout_dir)


def _clone_shared_checkout(repo_url: str, checkout_dir: Path) -> Path:
    cloned = clone_checkout(repo_url, checkout_dir)
    _refresh_marker_path(checkout_dir).touch()
    return cloned


def _make_snapshot_worktree_read_only(snapshot_dir: Path) -> None:
    """Remove worktree write bits while leaving .git writable and usable."""
    marker = snapshot_dir / ".git" / SNAPSHOT_PERMISSIONS_MARKER
    if marker.exists():
        return

    directories = [snapshot_dir]
    for root, dir_names, file_names in os.walk(snapshot_dir):
        root_path = Path(root)
        if root_path == snapshot_dir and ".git" in dir_names:
            dir_names.remove(".git")
        directories.extend(root_path / name for name in dir_names)
        for name in file_names:
            path = root_path / name
            if path == snapshot_dir / ".git" or path.is_symlink():
                continue
            mode = stat.S_IMODE(path.stat().st_mode)
            path.chmod(0o444 | (mode & 0o111))

    for directory in reversed(directories):
        directory.chmod(0o555)
    marker.write_text("worktree content is read-only\n", encoding="utf-8")


def _remove_tree(path: Path) -> None:
    """Remove a tree whose directories may intentionally be read-only."""
    for root, dir_names, _file_names in os.walk(path):
        Path(root).chmod(0o700)
        for name in dir_names:
            directory = Path(root) / name
            if not directory.is_symlink():
                directory.chmod(0o700)
    shutil.rmtree(path)


def _snapshot_shared_checkout_locked(checkout_dir: Path) -> Path:
    """Return an immutable, independently stored snapshot of canonical HEAD."""
    _validate_shared_checkout_locked(checkout_dir)
    commit = run_git(["rev-parse", "HEAD"], cwd=checkout_dir)
    if len(commit) not in (40, 64) or any(
        character not in "0123456789abcdef" for character in commit
    ):
        raise SystemExit(f"examples checkout returned invalid HEAD revision: {commit!r}")

    published_root = SNAPSHOT_CACHE_DIR / commit
    published_snapshot = published_root / "examples"
    if (published_snapshot / ".git").exists():
        _make_snapshot_worktree_read_only(published_snapshot)
        return published_snapshot.resolve()
    if published_root.exists():
        raise SystemExit(f"examples snapshot cache entry is incomplete: {published_root}")

    SNAPSHOT_CACHE_DIR.mkdir(parents=True, exist_ok=True)
    temporary_root = Path(
        tempfile.mkdtemp(prefix=f".{commit}.", dir=SNAPSHOT_CACHE_DIR)
    )
    temporary_snapshot = temporary_root / "examples"
    try:
        # --no-hardlinks makes the snapshot independent even if --cleanup later
        # removes the canonical checkout. The clone is local-only and does not
        # contact origin.
        run_git(
            [
                "clone",
                "--quiet",
                "--local",
                "--no-hardlinks",
                "--branch",
                DEFAULT_BRANCH,
                str(checkout_dir),
                str(temporary_snapshot),
            ]
        )
        _make_snapshot_worktree_read_only(temporary_snapshot)
        # Readers only discover the commit-named directory after the clone is
        # complete. The canonical checkout lock serializes publishers.
        temporary_root.rename(published_root)
    except BaseException:
        shutil.rmtree(temporary_root, ignore_errors=True)
        raise
    return published_snapshot.resolve()


def cleanup_checkout(checkout_dir: Path) -> None:
    """Remove shared examples state; callers guarantee no active readers."""
    with _locked_shared_checkout(checkout_dir):
        if checkout_dir.exists():
            shutil.rmtree(checkout_dir)
        _refresh_marker_path(checkout_dir).unlink(missing_ok=True)
        if SNAPSHOT_CACHE_DIR.exists():
            _remove_tree(SNAPSHOT_CACHE_DIR)


def ensure_examples_checkout(args: argparse.Namespace) -> Path:
    explicit_dir = os.environ.get("SHATTER_EXAMPLES_DIR")
    if explicit_dir and not args.fresh:
        checkout_dir = Path(explicit_dir).expanduser()
        if not checkout_dir.exists():
            raise SystemExit(
                f"SHATTER_EXAMPLES_DIR does not exist: {checkout_dir}"
            )
        return checkout_dir.resolve()

    repo_url = os.environ.get("SHATTER_EXAMPLES_REPO", DEFAULT_REPO_URL)
    if args.fresh:
        # --fresh always clones into a brand-new per-process temp dir, so it
        # never touches the shared checkout and needs no locking.
        temp_root = Path(
            tempfile.mkdtemp(prefix="shatter-examples.", dir=tempfile.gettempdir())
        )
        clone_checkout(repo_url, temp_root)
        return temp_root.resolve()

    checkout_dir = DEFAULT_DIR
    with _locked_shared_checkout(checkout_dir):
        if not checkout_dir.exists():
            _clone_shared_checkout(repo_url, checkout_dir)
        elif not args.no_update:
            _refresh_checkout_locked(checkout_dir)
        else:
            # The existence check and repository validation must happen under
            # the same lock as cloning. A clone creates its destination before
            # .git is ready, so an unlocked --no-update check can otherwise
            # expose a partial checkout.
            _validate_shared_checkout_locked(checkout_dir)
        return _snapshot_shared_checkout_locked(checkout_dir)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--fresh",
        action="store_true",
        help="clone examples into a fresh temporary directory under /tmp",
    )
    parser.add_argument(
        "--no-update",
        action="store_true",
        help="snapshot the cached /tmp checkout without fetching origin/main",
    )
    parser.add_argument(
        "--cleanup",
        action="store_true",
        help="delete shared caches (requires no active examples readers)",
    )
    args = parser.parse_args()

    if args.cleanup:
        cleanup_checkout(DEFAULT_DIR)
        return

    print(ensure_examples_checkout(args))


if __name__ == "__main__":
    main()
