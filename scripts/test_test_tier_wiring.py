import re
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
    def test_nextest_serializes_every_rust_frontend_harness_binary(self) -> None:
        config = tomllib.loads((ROOT / ".config/nextest.toml").read_text())
        self.assertEqual(config["test-groups"]["rust-frontend-harness"]["max-threads"], 1)

        discovered = {"e2e_concolic_rust"}
        for test_file in (ROOT / "shatter-core/tests").glob("*.rs"):
            if "rust_frontend_harness.rs" in test_file.read_text():
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
        self.assertIn("--run-ignored all", contents["core"])

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
