"""Guard the Go tool wrapper's documented install path (str-49drv.101).

The repo has no root go.mod, so Go resolves
github.com/shatterproof-ai/shatter/<X> to the <X>/ directory at the repo root.
The wrapper module path must therefore end in its directory name, and the path
documented in docs/distribution.md (install command and Renovate regex) must
match the module line. The build test resolves the documented path against the
local checkout with a replace directive, so it needs no network.
"""

import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
MODULE_DIR = "shatter-go-tool"
REPO_PATH = "github.com/shatterproof-ai/shatter"
DOCS = REPO / "docs" / "distribution.md"


def module_path() -> str:
    first = (REPO / MODULE_DIR / "go.mod").read_text().splitlines()[0]
    match = re.fullmatch(r"module\s+(\S+)", first.strip())
    assert match, f"unexpected go.mod first line: {first!r}"
    return match.group(1)


class GoToolModulePath(unittest.TestCase):
    def test_module_path_matches_directory(self):
        self.assertEqual(module_path(), f"{REPO_PATH}/{MODULE_DIR}")

    def test_install_command_uses_module_path(self):
        text = DOCS.read_text()
        match = re.search(r"^go get -tool (\S+?)/cmd/shatter@", text, re.M)
        self.assertIsNotNone(match, "go get -tool command missing from docs")
        self.assertEqual(match.group(1), module_path())

    def test_renovate_regex_uses_module_path(self):
        text = DOCS.read_text()
        escaped = module_path().replace(".", "\\\\.")
        self.assertIn(f"{escaped}/cmd/shatter", text)

    @unittest.skipUnless(shutil.which("go"), "go toolchain not installed")
    def test_documented_path_builds_from_local_checkout(self):
        path = module_path()
        with tempfile.TemporaryDirectory() as tmp:
            Path(tmp, "go.mod").write_text(
                "module example.com/consumer\n\ngo 1.24.0\n\n"
                f"require {path} v0.0.0\n\n"
                f"replace {path} => {REPO / MODULE_DIR}\n"
            )
            Path(tmp, "tools.go").write_text(
                f'//go:build tools\n\npackage tools\n\nimport _ "{path}/cmd/shatter"\n'
            )
            env = dict(os.environ, GOFLAGS="-mod=mod", GOPROXY="off")
            result = subprocess.run(
                ["go", "build", "-o", os.devnull, f"{path}/cmd/shatter"],
                cwd=tmp, env=env, capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
