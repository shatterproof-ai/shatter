import os
import re
import shutil
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
HARNESS_BINARIES = {
    "e2e_concolic_rust",
    "rust_explore_integration",
    "self_hosting_explore",
}


class TestTestTierWiring(unittest.TestCase):
    @staticmethod
    def _task_body(taskfile: str, task_name: str) -> str:
        match = re.search(
            rf"(?ms)^  {re.escape(task_name)}:\n(.*?)(?=^  [a-z][a-z0-9-]*:|\Z)",
            taskfile,
        )
        if match is None:
            raise AssertionError(f"task {task_name!r} not found")
        return match.group(1)

    @staticmethod
    def _canonical_cached_task_body(body: str) -> str:
        lines = []
        skipping_env = False
        for line in body.splitlines():
            if line.startswith("    env:"):
                skipping_env = True
                continue
            if skipping_env and line.startswith("      "):
                continue
            skipping_env = False
            if line.lstrip().startswith("#") or line.lstrip().startswith("desc:"):
                continue
            lines.append(line.replace("core:test-ignored-fast", "core:test-ignored"))
        return "\n".join(lines).strip()

    def test_nextest_serializes_every_rust_frontend_harness_binary(self) -> None:
        config = tomllib.loads((ROOT / ".config/nextest.toml").read_text())
        self.assertEqual(config["test-groups"]["rust-frontend-harness"]["max-threads"], 1)

        discovered = set()
        for test_file in (ROOT / "shatter-core/tests").glob("*.rs"):
            source = test_file.read_text()
            if (
                "rust_frontend_harness.rs" in source
                or "fn rust_frontend_path()" in source
            ):
                discovered.add(test_file.stem)
        self.assertEqual(discovered, HARNESS_BINARIES)

        for profile_name in ("default", "ci"):
            profile = config["profile"][profile_name]
            self.assertEqual(profile["test-threads"], 4)
            overrides = profile["overrides"]
            harness_override = next(
                item
                for item in overrides
                if item.get("test-group") == "rust-frontend-harness"
            )
            filtered = set(re.findall(r"binary\(([^)]+)\)", harness_override["filter"]))
            self.assertEqual(filtered, HARNESS_BINARIES)

    def test_rust_tasks_keep_nextest_fallbacks_and_doctests(self) -> None:
        taskfiles = {
            "core": ROOT / "shatter-core/Taskfile.yml",
            "cli": ROOT / "shatter-cli/Taskfile.yml",
            "rust": ROOT / "shatter-rust/Taskfile.yml",
            "runtime": ROOT / "shatter-rust-runtime/Taskfile.yml",
        }
        contents = {name: path.read_text() for name, path in taskfiles.items()}
        for content in contents.values():
            self.assertIn("command -v cargo-nextest", content)
            self.assertIn("cargo nextest run", content)
            self.assertIn("cargo test", content)
        self.assertIn("cargo test -p shatter-core --doc", contents["core"])
        self.assertIn("cargo test --doc", contents["rust"])
        self.assertIn("cargo test --doc", contents["runtime"])
        self.assertIn("--run-ignored all", contents["core"])
        for standalone in (contents["rust"], contents["runtime"]):
            self.assertIn(
                "cargo nextest run --config-file ../.config/nextest-standalone.toml",
                standalone,
            )
            self.assertNotIn("--config-file ../.config/nextest.toml", standalone)

        standalone_config = tomllib.loads(
            (ROOT / ".config/nextest-standalone.toml").read_text()
        )
        for profile_name in ("default", "ci"):
            profile = standalone_config["profile"][profile_name]
            self.assertEqual(profile["test-threads"], 4)
            self.assertEqual(profile["slow-timeout"]["period"], "60s")
            self.assertEqual(profile["slow-timeout"]["terminate-after"], 2)

    def test_fast_and_full_task_budgets_are_distinct(self) -> None:
        root_taskfile = (ROOT / "Taskfile.yml").read_text()
        self.assertIn("deps: [core:clippy, cli:clippy, ts:test-fast, go:test-short]", root_taskfile)
        self.assertGreaterEqual(root_taskfile.count("PROPTEST_CASES: '32'"), 2)
        self.assertGreaterEqual(root_taskfile.count("SHATTER_FUZZ_CASES: '32'"), 2)
        self.assertIn("PROPTEST_CASES: '256'", root_taskfile)
        self.assertIn("SHATTER_FUZZ_CASES: '1000'", root_taskfile)
        self.assertIn("SHATTER_FAST_CHECK_NUM_RUNS: default", root_taskfile)
        self.assertIn(
            "cargo test --test e2e_concolic -- --include-ignored",
            root_taskfile,
        )

    def test_case_budgeted_rust_tasks_never_reuse_a_different_tier_cache(self) -> None:
        root_taskfile = (ROOT / "Taskfile.yml").read_text()
        core_taskfile = (ROOT / "shatter-core/Taskfile.yml").read_text()
        self.assertIn("- task: workspace-test-quick", root_taskfile)
        self.assertIn("- task: workspace-test", root_taskfile)
        self.assertIn("task core:test-ignored-fast", root_taskfile)
        self.assertIn("- task: core:test-ignored", root_taskfile)
        self.assertRegex(root_taskfile, r"(?m)^  workspace-test-quick:$")
        self.assertRegex(core_taskfile, r"(?m)^  test-ignored-fast:$")
        self.assertEqual(
            self._canonical_cached_task_body(
                self._task_body(root_taskfile, "workspace-test")
            ),
            self._canonical_cached_task_body(
                self._task_body(root_taskfile, "workspace-test-quick")
            ),
        )
        self.assertEqual(
            self._canonical_cached_task_body(
                self._task_body(core_taskfile, "test-ignored")
            ),
            self._canonical_cached_task_body(
                self._task_body(core_taskfile, "test-ignored-fast")
            ),
        )

    def test_quick_rust_budget_reaches_workspace_test_process(self) -> None:
        task = shutil.which("task")
        if task is None:
            self.skipTest("task is not installed")

        with tempfile.TemporaryDirectory() as temp_dir:
            tool_dir = Path(temp_dir)
            capture = tool_dir / "cargo-env.log"
            fake_tool = """#!/usr/bin/env bash
if [[ "$(basename "$0")" == cargo ]]; then
  printf '%s|%s|%s\\n' "$*" "${PROPTEST_CASES:-}" "${SHATTER_FUZZ_CASES:-}" >> "$SHATTER_TEST_ENV_CAPTURE"
fi
exit 0
"""
            for tool_name in ("cargo", "go", "npm"):
                tool = tool_dir / tool_name
                tool.write_text(fake_tool)
                tool.chmod(0o755)

            env = os.environ.copy()
            env["PATH"] = f"{tool_dir}:{env['PATH']}"
            env["SHATTER_TEST_ENV_CAPTURE"] = str(capture)
            # Model a default quick-tier invocation even when this regression
            # runs inside the Full gate, whose ambient budgets intentionally
            # remain valid operator overrides.
            env.pop("PROPTEST_CASES", None)
            env.pop("SHATTER_FUZZ_CASES", None)
            subprocess.run(
                [task, "--force", "test-quick"],
                cwd=ROOT,
                env=env,
                check=True,
                capture_output=True,
                text=True,
            )

            workspace_test = next(
                line
                for line in capture.read_text().splitlines()
                if line.startswith("test --workspace|")
            )
        self.assertEqual(workspace_test, "test --workspace|32|32")

    def test_every_ts_e2e_test_is_in_the_integration_tier(self) -> None:
        source = (ROOT / "shatter-core/tests/e2e_concolic.rs").read_text()
        self.assertEqual(source.count("#[tokio::test]"), 26)
        self.assertEqual(
            source.count(
                '#[ignore = "subprocess E2E; run via task e2e-ts or core:test-ignored"]'
            ),
            26,
        )

    def test_fast_check_overrides_cover_explicit_case_counts(self) -> None:
        test_files = list((ROOT / "shatter-ts/src").glob("*.test.ts"))
        explicit_counts = []
        helper_calls = []
        for test_file in test_files:
            source = test_file.read_text()
            explicit_counts.extend(re.findall(r"\{\s*numRuns\s*:", source))
            helper_calls.extend(re.findall(r"fastCheckParameters\([0-9]+\)", source))
        self.assertEqual(explicit_counts, [])
        self.assertEqual(len(helper_calls), 16)

        jest_config = (ROOT / "shatter-ts/jest.config.js").read_text()
        self.assertIn("fast-check-setup.ts", jest_config)
        ts_taskfile = (ROOT / "shatter-ts/Taskfile.yml").read_text()
        self.assertIn("SHATTER_FAST_CHECK_NUM_RUNS: '32'", ts_taskfile)

    def test_go_runner_partitions_rapid_packages(self) -> None:
        go_taskfile = (ROOT / "shatter-go/Taskfile.yml").read_text()
        runner = (ROOT / "scripts/go-test-tier.sh").read_text()
        self.assertIn("go-test-tier.sh full", go_taskfile)
        self.assertIn("go-test-tier.sh short", go_taskfile)
        self.assertIn("rapid_checks=100", runner)
        self.assertIn("rapid_checks=32", runner)
        self.assertIn('"pgregory.net/rapid"', runner)

    def test_go_runner_classifies_external_rapid_tests(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            fake_go = Path(temp_dir) / "go"
            fake_go.write_text(
                """#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == "list" && "$2" == "-f" ]]; then
  [[ "$3" == *XTestImports* ]] && printf '%s\\n' example/external
elif [[ "$1" == "list" ]]; then
  printf '%s\\n' example/external
elif [[ "$1" == "test" ]]; then
  printf '%s\\n' "$*"
fi
"""
            )
            fake_go.chmod(0o755)
            env = os.environ.copy()
            env["PATH"] = f"{temp_dir}:{env['PATH']}"
            result = subprocess.run(
                ["bash", str(ROOT / "scripts/go-test-tier.sh"), "short"],
                cwd=ROOT / "shatter-go",
                env=env,
                check=True,
                capture_output=True,
                text=True,
            )
        self.assertIn("example/external -rapid.checks=32", result.stdout)

    def test_go_runner_fails_when_package_discovery_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            fake_go = Path(temp_dir) / "go"
            fake_go.write_text(
                """#!/usr/bin/env bash
if [[ "$1" == "list" && "$2" == "-f" ]]; then
  exit 0
elif [[ "$1" == "list" ]]; then
  echo "package discovery failed" >&2
  exit 23
fi
exit 99
"""
            )
            fake_go.chmod(0o755)
            env = os.environ.copy()
            env["PATH"] = f"{temp_dir}:{env['PATH']}"
            result = subprocess.run(
                ["bash", str(ROOT / "scripts/go-test-tier.sh"), "short"],
                cwd=ROOT / "shatter-go",
                env=env,
                check=False,
                capture_output=True,
                text=True,
            )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("package discovery failed", result.stderr)

    def test_go_runner_rejects_inconsistent_package_discovery(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            fake_go = Path(temp_dir) / "go"
            fake_go.write_text(
                """#!/usr/bin/env bash
if [[ "$1" == "list" && "$2" == "-f" ]]; then
  printf '%s\\n' example/rapid
elif [[ "$1" == "list" ]]; then
  printf '%s\\n' example/plain
elif [[ "$1" == "test" ]]; then
  exit 0
fi
"""
            )
            fake_go.chmod(0o755)
            env = os.environ.copy()
            env["PATH"] = f"{temp_dir}:{env['PATH']}"
            result = subprocess.run(
                ["bash", str(ROOT / "scripts/go-test-tier.sh"), "short"],
                cwd=ROOT / "shatter-go",
                env=env,
                check=False,
                capture_output=True,
                text=True,
            )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not returned by go list ./...", result.stderr)

    def test_go_runner_help_succeeds(self) -> None:
        result = subprocess.run(
            ["bash", str(ROOT / "scripts/go-test-tier.sh"), "--help"],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0)
        self.assertIn("full|short", result.stdout)

    def test_ci_declares_full_property_budgets(self) -> None:
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        self.assertIn('PROPTEST_CASES: "256"', workflow)
        self.assertIn('SHATTER_FUZZ_CASES: "1000"', workflow)
        self.assertIn("SHATTER_FAST_CHECK_NUM_RUNS: default", workflow)
        self.assertEqual(workflow.count("run: task check"), 1)
        self.assertNotIn("task ts:test-fast", workflow)
        self.assertNotIn("task go:test-short", workflow)


if __name__ == "__main__":
    unittest.main()
