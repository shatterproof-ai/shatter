from __future__ import annotations

import os
import shutil
import stat
import subprocess
import tempfile
import textwrap
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
