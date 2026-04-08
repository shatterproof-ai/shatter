from __future__ import annotations

import os
import shutil
import stat
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
WALKTHROUGH = REPO_ROOT / "demo" / "walkthrough.sh"


class WalkthroughExamplesCheckoutTest(unittest.TestCase):
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
