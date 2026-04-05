#!/usr/bin/env python3
"""Resolve or prepare the external examples checkout."""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


DEFAULT_REPO_URL = "https://github.com/shatterproof-ai/examples.git"
DEFAULT_BRANCH = "main"
DEFAULT_DIR = Path(tempfile.gettempdir()) / "shatter-examples-main"


def run_git(args: list[str], cwd: Path | None = None) -> None:
    completed = subprocess.run(
        ["git", *args],
        cwd=cwd,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if completed.returncode == 0:
        return
    detail = completed.stderr.strip() or completed.stdout.strip() or "git command failed"
    raise SystemExit(detail)


def clone_checkout(repo_url: str, checkout_dir: Path) -> Path:
    checkout_dir.parent.mkdir(parents=True, exist_ok=True)
    run_git(["clone", "--quiet", "--branch", DEFAULT_BRANCH, repo_url, str(checkout_dir)])
    return checkout_dir.resolve()


def refresh_checkout(checkout_dir: Path) -> Path:
    if not checkout_dir.join(".git").exists():
        raise SystemExit(
            f"examples checkout path exists but is not a git repository: {checkout_dir}"
        )
    run_git(["fetch", "--quiet", "origin", DEFAULT_BRANCH], cwd=checkout_dir)
    run_git(["checkout", "--quiet", DEFAULT_BRANCH], cwd=checkout_dir)
    run_git(["reset", "--hard", f"origin/{DEFAULT_BRANCH}"], cwd=checkout_dir)
    run_git(["clean", "-fdx"], cwd=checkout_dir)
    return checkout_dir.resolve()


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
        temp_root = Path(
            tempfile.mkdtemp(prefix="shatter-examples.", dir=tempfile.gettempdir())
        )
        clone_checkout(repo_url, temp_root)
        return temp_root.resolve()

    checkout_dir = DEFAULT_DIR
    if not checkout_dir.exists():
        return clone_checkout(repo_url, checkout_dir)
    if args.no_update:
        return checkout_dir.resolve()
    return refresh_checkout(checkout_dir)


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
        help="reuse the default /tmp checkout without fetching origin/main",
    )
    parser.add_argument(
        "--cleanup",
        action="store_true",
        help="delete the default reusable /tmp checkout if it exists",
    )
    args = parser.parse_args()

    if args.cleanup:
        if DEFAULT_DIR.exists():
            shutil.rmtree(DEFAULT_DIR)
        return

    print(ensure_examples_checkout(args))


if __name__ == "__main__":
    main()
