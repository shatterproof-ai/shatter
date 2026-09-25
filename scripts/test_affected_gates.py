from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

import yaml

from scripts.git_sandbox_test_lib import sanitized_git_env


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "affected-gates.py"
TASKFILE = ROOT / "Taskfile.yml"


def list_all_tasks(cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["task", "--list-all", "--json", "--dry"],
        cwd=cwd,
        text=True,
        capture_output=True,
        check=True,
    )


def load_module():
    spec = importlib.util.spec_from_file_location("affected_gates", SCRIPT)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load {SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class AffectedGateMappingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.module = load_module()

    def assert_gates(self, paths: list[str], expected: set[str]) -> None:
        self.assertEqual(set(self.module.select_gates(paths)), expected)

    def test_documentation_precedes_crate_classification(self) -> None:
        self.assert_gates(
            ["README.md", "docs/guide.md", "shatter-go/CLAUDE.md"],
            {"docs"},
        )

    def test_meta_paths(self) -> None:
        self.assert_gates(
            [
                ".github/workflows/ci.yml",
                ".semgrep/shatter.yml",
                "install.sh",
                "scripts/drift-patrol.py",
                "scripts/setup-hooks.sh",
                "shatter-go-tool/cmd/shatter/main.go",
            ],
            {"meta"},
        )

    def test_nested_build_graph_files_use_full_check_fail_safe(self) -> None:
        for path in (
            "shatter-core/Taskfile.yml",
            "shatter-rust/Cargo.toml",
            "shatter-rust-runtime/Cargo.lock",
        ):
            with self.subTest(path=path):
                self.assert_gates([path], {"smoke", "check"})

    def test_core_and_cli_paths(self) -> None:
        self.assert_gates(
            ["shatter-core/src/cache.rs"],
            {"core:test", "core:clippy", "smoke"},
        )
        self.assert_gates(
            ["shatter-cli/src/embedded_frontend.rs"],
            {"cli:test", "cli:clippy", "smoke"},
        )

    def test_pipeline_and_cli_surface_paths_add_required_e2e_and_gauntlet(self) -> None:
        all_e2e = {"e2e-ts", "e2e-go", "e2e-rust"}
        self.assert_gates(
            ["shatter-core/src/solver.rs"],
            {"core:test", "core:clippy", "smoke", *all_e2e},
        )
        self.assert_gates(
            ["shatter-cli/src/commands/explore.rs"],
            {"cli:test", "cli:clippy", "smoke", "gauntlet", *all_e2e},
        )
        self.assert_gates(
            ["shatter-cli/src/render.rs"],
            {"cli:test", "cli:clippy", "smoke", "gauntlet"},
        )
        self.assert_gates(
            ["shatter-core/src/orchestrator/mod.rs"],
            {"core:test", "core:clippy", "smoke", *all_e2e},
        )

    def test_e2e_test_files_run_their_own_frontend_gate(self) -> None:
        cases = {
            "shatter-core/tests/e2e_concolic.rs": "e2e-ts",
            "shatter-core/tests/e2e_concolic_go.rs": "e2e-go",
            "shatter-core/tests/e2e_concolic_rust.rs": "e2e-rust",
        }
        for path, gate in cases.items():
            with self.subTest(path=path):
                self.assertIn(gate, self.module.select_gates([path]))

    def test_frontend_and_runtime_rows(self) -> None:
        self.assert_gates(
            ["shatter-ts/src/main.ts"],
            {"ts:test", "ts:typecheck", "smoke", "e2e-ts", "parity", "conformance"},
        )
        self.assert_gates(
            ["shatter-go/wrapper/wrapper.go"],
            {"go:test", "go:vet", "smoke", "e2e-go", "parity", "conformance"},
        )
        self.assert_gates(
            ["shatter-rust/src/main.rs"],
            {
                "rust-fe:test",
                "rust-fe:clippy",
                "smoke",
                "e2e-rust",
                "parity",
                "conformance",
            },
        )
        self.assert_gates(
            ["shatter-rust-runtime/src/lib.rs"],
            {"rust-rt:test", "rust-rt:clippy", "smoke", "e2e-rust"},
        )

    def test_protocol_demo_and_example_rows(self) -> None:
        self.assert_gates(
            ["protocol/registry.yaml"],
            {"schemas", "parity", "conformance", "ts:test", "go:test", "rust-fe:test", "smoke"},
        )
        self.assert_gates(["demo/walkthrough.sh"], {"walkthrough"})
        self.assert_gates(["demo/gauntlet-scan-allowlist.yaml"], {"gauntlet"})
        self.assert_gates(["demo/fixtures/arithmetic-v1.ts"], {"gauntlet"})
        self.assert_gates(
            ["benchmarks/sample-manifest.json"],
            {"walkthrough", "gauntlet"},
        )
        self.assert_gates(
            ["examples/go/05-conditional-merge.go"],
            {"smoke", "e2e-ts", "e2e-go", "e2e-rust"},
        )

    def test_ignored_paths_emit_nothing(self) -> None:
        self.assert_gates([".beads/issues.jsonl", ".claude/settings.json"], set())

    def test_mixed_paths_union_and_unmatched_fail_safe(self) -> None:
        self.assert_gates(
            ["docs/guide.md", "shatter-go/config/config.go"],
            {"docs", "go:test", "go:vet", "smoke", "e2e-go", "parity", "conformance"},
        )
        self.assert_gates(
            ["shatter-go/config/config.go", "unmatched/config.file"],
            {"smoke", "check"},
        )

    def test_historical_merge_selections(self) -> None:
        fixtures = {
            "2871872faaad54ff868f41b04df3dda238a06288": {"docs"},
            "4b387d3babfee1b17b23fa62559e8e5c866c6624": {"meta"},
            "2a626706e341ded4ee98c64b9c44a33ff4ddcb19": {
                "go:test",
                "go:vet",
                "smoke",
                "e2e-go",
                "parity",
                "conformance",
            },
            "9c2db7e023e1a9540ead9d68ebca0ed2a0034e6e": {
                "docs",
                "core:test",
                "core:clippy",
                "smoke",
                "e2e-ts",
                "e2e-go",
                "e2e-rust",
            },
            "acb3cee66137c4c60d8ab306048f02508bbbe19f": {"smoke", "check"},
        }
        unavailable = [
            merge_sha
            for merge_sha in fixtures
            if subprocess.run(
                ["git", "cat-file", "-e", f"{merge_sha}^1"],
                cwd=ROOT,
                capture_output=True,
            ).returncode
        ]
        if unavailable:
            self.skipTest(
                "historical merge objects unavailable (expected in a shallow clone): "
                + ", ".join(unavailable)
            )
        for merge_sha, expected in fixtures.items():
            with self.subTest(merge=merge_sha):
                paths = self.module.changed_paths(f"{merge_sha}^1", merge_sha, ROOT)
                self.assertEqual(set(self.module.select_gates(paths)), expected)

    def test_every_emitted_gate_is_a_real_task(self) -> None:
        completed = list_all_tasks(ROOT)
        listed = {task["name"] for task in json.loads(completed.stdout)["tasks"]}
        self.assertEqual(set(self.module.GATE_ORDER) - listed, set())

    def test_listing_does_not_write_checksums_or_cache_a_source_leaf(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            fixture = Path(temp_dir)
            leaf_dir = fixture / "leaf"
            leaf_dir.mkdir()
            (fixture / "Taskfile.yml").write_text(
                "version: '3'\nincludes:\n  leaf:\n    taskfile: ./leaf/Taskfile.yml\n    dir: ./leaf\n"
            )
            (leaf_dir / "Taskfile.yml").write_text(
                "version: '3'\ntasks:\n  run:\n    desc: Fixture source leaf\n"
                "    sources: [source.txt]\n    cmds: [echo LEAF_RAN]\n"
            )
            source = leaf_dir / "source.txt"
            source.write_text("before\n")

            checksum_dir = fixture / ".task" / "checksum"

            def checksums() -> dict[str, bytes]:
                if not checksum_dir.exists():
                    return {}
                return {
                    str(path.relative_to(checksum_dir)): path.read_bytes()
                    for path in checksum_dir.rglob("*")
                    if path.is_file()
                }

            before = checksums()
            list_all_tasks(fixture)
            self.assertEqual(checksums(), before)

            source.write_text("after\n")
            completed = subprocess.run(
                ["task", "leaf:run"],
                cwd=fixture,
                text=True,
                capture_output=True,
                check=True,
            )
            output = completed.stdout + completed.stderr
            self.assertIn("task: [leaf:run] echo LEAF_RAN", output)
            self.assertNotIn('Task "leaf:run" is up to date', output)

    def test_test_scripts_guard_task_list_commands(self) -> None:
        unguarded = []
        command_pattern = re.compile(
            r"\[\s*['\"]task['\"]\s*,[^\]]*['\"]--list(?:-all)?['\"][^\]]*\]",
            re.DOTALL,
        )
        for directory in (ROOT / "scripts", ROOT / "demo"):
            for path in directory.glob("test_*.py"):
                for match in command_pattern.finditer(path.read_text()):
                    if not re.search(r"['\"]--dry['\"]", match.group()):
                        unguarded.append(f"{path.relative_to(ROOT)}: {match.group()}")
        self.assertEqual(unguarded, [])


class AffectedGateWiringTests(unittest.TestCase):
    def test_affected_task_is_one_governed_serial_executor(self) -> None:
        tasks = yaml.safe_load(TASKFILE.read_text())["tasks"]
        self.assertEqual(
            tasks["affected"]["cmds"],
            ["bash scripts/gate-wrapper.sh affected task affected-governed"],
        )
        implementation = "\n".join(tasks["affected-governed"]["cmds"])
        self.assertIn("python3 scripts/affected-gates.py", implementation)
        self.assertIn("AFFECTED_BASE:-origin/main", implementation)
        self.assertIn('if ! gates="$(python3 scripts/affected-gates.py', implementation)
        self.assertIn('task "$gate"', implementation)
        self.assertNotIn("&", implementation)

    def test_governed_executor_propagates_selector_git_failure(self) -> None:
        tasks = yaml.safe_load(TASKFILE.read_text())["tasks"]
        implementation = "\n".join(tasks["affected-governed"]["cmds"])
        with tempfile.TemporaryDirectory() as temp_dir:
            fixture = Path(temp_dir)
            scripts = fixture / "scripts"
            scripts.mkdir()
            shutil.copy2(SCRIPT, scripts / SCRIPT.name)
            subprocess.run(
                ["git", "init", "-q"], cwd=fixture, env=sanitized_git_env(), check=True
            )
            marker = fixture / "task-invoked"
            fake_bin = fixture / "bin"
            fake_bin.mkdir()
            fake_task = fake_bin / "task"
            fake_task.write_text(f"#!/bin/sh\ntouch {marker}\n")
            fake_task.chmod(0o755)
            completed = subprocess.run(
                ["bash", "-c", implementation],
                cwd=fixture,
                env=sanitized_git_env()
                | {"PATH": f"{fake_bin}:{os.environ['PATH']}"},
                text=True,
                capture_output=True,
            )
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("affected gate selection failed", completed.stdout)
            self.assertFalse(marker.exists())

    def test_meta_runs_affected_gate_regressions(self) -> None:
        tasks = yaml.safe_load(TASKFILE.read_text())["tasks"]
        self.assertIn(
            "python3 -m unittest scripts.test_affected_gates",
            tasks["meta"]["cmds"],
        )


if __name__ == "__main__":
    unittest.main()
