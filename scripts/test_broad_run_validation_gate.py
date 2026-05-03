"""Unit + property-style tests for scripts/broad_run_validation_gate.py.

Follows the project's existing test-script idiom (stdlib unittest +
import-by-spec). No third-party deps. Property tests are deterministic
(small bounded enumerations) so we don't pull in `hypothesis` for one
script — matches the project's no-new-toolchain norm.
"""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("broad_run_validation_gate.py")
SPEC = importlib.util.spec_from_file_location(
    "broad_run_validation_gate", MODULE_PATH
)
assert SPEC is not None
assert SPEC.loader is not None
gate = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = gate
SPEC.loader.exec_module(gate)


class CompareThresholdsTest(unittest.TestCase):
    def test_min_pass_when_actual_at_or_above(self) -> None:
        self.assertIsNone(gate.compare_min(5, 5, "fld"))
        self.assertIsNone(gate.compare_min(6, 5, "fld"))

    def test_min_fail_when_actual_below(self) -> None:
        msg = gate.compare_min(4, 5, "fld")
        assert msg is not None
        self.assertIn("fld", msg)
        self.assertIn("4", msg)
        self.assertIn("5", msg)

    def test_min_skipped_when_threshold_none(self) -> None:
        self.assertIsNone(gate.compare_min(0, None, "fld"))

    def test_max_pass_when_actual_at_or_below(self) -> None:
        self.assertIsNone(gate.compare_max(5, 5, "fld"))
        self.assertIsNone(gate.compare_max(4, 5, "fld"))

    def test_max_fail_when_actual_above(self) -> None:
        msg = gate.compare_max(6, 5, "fld")
        assert msg is not None
        self.assertIn("fld", msg)

    def test_range_combines_min_and_max(self) -> None:
        # Both bounds.
        self.assertEqual(gate.compare_range(3, 1, 5, "fld"), [])
        self.assertEqual(len(gate.compare_range(0, 1, 5, "fld")), 1)
        self.assertEqual(len(gate.compare_range(6, 1, 5, "fld")), 1)
        # Out of both bounds is impossible (lo<=hi assumed); enumerate
        # property: for (lo,hi) with lo<=hi, in-range value passes; outside
        # produces exactly one error.
        for lo in range(0, 5):
            for hi in range(lo, lo + 6):
                for actual in range(-2, lo + hi + 4):
                    errors = gate.compare_range(actual, lo, hi, "f")
                    if lo <= actual <= hi:
                        self.assertEqual(errors, [], msg=f"{lo}<={actual}<={hi}")
                    else:
                        self.assertEqual(
                            len(errors), 1, msg=f"actual={actual} lo={lo} hi={hi}"
                        )


class CollectReferencedPathsTest(unittest.TestCase):
    def test_walks_nested_dicts_and_lists(self) -> None:
        report = {
            "codebase": {
                "skipped_functions": [
                    {"file_path": "/abs/path/foo.ts", "no_target_reason": "x"}
                ],
            },
            "functions": [
                {"file_path": "/abs/other/bar.go"},
            ],
            "logline": "INFO message with /tmp/space embedded — should be skipped",
            "version": 1,
        }
        paths = gate.collect_referenced_paths(report)
        self.assertIn("/abs/path/foo.ts", paths)
        self.assertIn("/abs/other/bar.go", paths)
        # Whitespace string should be excluded.
        self.assertNotIn(report["logline"], paths)

    def test_relative_paths_excluded(self) -> None:
        report = {"file": "relative/foo.ts"}
        self.assertEqual(gate.collect_referenced_paths(report), [])

    def test_qualified_function_ids_excluded(self) -> None:
        # The wire format uses `/abs/path.ts::Symbol` for function ids; that
        # is not a filesystem path and must be filtered out.
        report = {"functions": [{"id": "/abs/path.ts::Symbol"}]}
        self.assertEqual(gate.collect_referenced_paths(report), [])


class UnresolvedPathsTest(unittest.TestCase):
    def test_returns_only_missing_paths(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            existing = Path(tmp) / "exists.txt"
            existing.write_text("x")
            missing = Path(tmp) / "missing.txt"
            unresolved = gate.unresolved_paths([str(existing), str(missing)])
            self.assertEqual(unresolved, [str(missing)])

    def test_empty_list_returns_empty(self) -> None:
        self.assertEqual(gate.unresolved_paths([]), [])


class FindNoTargetReasonsTest(unittest.TestCase):
    def test_reads_codebase_skipped_functions_shape(self) -> None:
        report = {
            "codebase": {
                "skipped_functions": [
                    {
                        "file_path": "/x/y/foo.d.ts",
                        "no_target_reason": "declaration_only",
                    },
                    {
                        "file_path": "/x/y/bar.tsx",
                        "no_target_reason": "jsx_component_only",
                    },
                ]
            }
        }
        result = gate.find_no_target_reasons(report)
        self.assertEqual(result["foo.d.ts"], "declaration_only")
        self.assertEqual(result["bar.tsx"], "jsx_component_only")

    def test_reads_codebase_no_target_files_shape(self) -> None:
        report = {
            "codebase": {
                "no_target_files": [
                    {"file_path": "/x/y/build.rs", "reason": "build_script"},
                ]
            }
        }
        self.assertEqual(
            gate.find_no_target_reasons(report)["build.rs"], "build_script"
        )

    def test_reads_top_level_summary_shape(self) -> None:
        report = {
            "no_target_reasons": [
                {"file": "lib.rs", "reason": "test_module"},
            ]
        }
        self.assertEqual(gate.find_no_target_reasons(report)["lib.rs"], "test_module")

    def test_returns_empty_when_absent(self) -> None:
        self.assertEqual(gate.find_no_target_reasons({}), {})
        self.assertEqual(gate.find_no_target_reasons({"codebase": {}}), {})


class FunctionKeysTest(unittest.TestCase):
    def test_combines_path_and_name(self) -> None:
        report = {
            "functions": [
                {"file_path": "/a/b.go", "function_name": "Run"},
                {"file_path": "/a/c.go", "function_name": "Run"},
            ]
        }
        keys = gate._function_keys(report)
        self.assertEqual(keys, {"/a/b.go::Run", "/a/c.go::Run"})

    def test_handles_missing_file_path(self) -> None:
        report = {"functions": [{"function_name": "Solo"}]}
        self.assertEqual(gate._function_keys(report), {"Solo"})

    def test_handles_missing_functions_array(self) -> None:
        self.assertEqual(gate._function_keys({}), set())
        self.assertEqual(gate._function_keys({"functions": "oops"}), set())


class AssertFixtureTest(unittest.TestCase):
    """Black-box checks against the assertion logic, with synthetic reports."""

    def _fixture(self, **expected: object) -> dict[str, object]:
        return {"id": "synthetic", "expected": expected}

    def test_run_must_succeed_pass(self) -> None:
        report = {"codebase": {"total_discovered_functions": 1}}
        failures = gate.assert_fixture(
            self._fixture(run_must_succeed=True), report, 0, ""
        )
        self.assertEqual(failures, [])

    def test_run_must_succeed_fail(self) -> None:
        failures = gate.assert_fixture(
            self._fixture(run_must_succeed=True), {}, 7, "boom"
        )
        self.assertTrue(any("exited 7" in f for f in failures))

    def test_min_completed_functions_violation(self) -> None:
        report = {"codebase": {"completed_functions": 0}}
        failures = gate.assert_fixture(
            self._fixture(min_completed_functions=1), report, 0, ""
        )
        self.assertTrue(any("completed_functions" in f for f in failures))

    def test_artifact_paths_must_resolve_flags_dangling(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            real = Path(tmp) / "real.txt"
            real.write_text("x")
            fake = str(Path(tmp) / "missing.txt")
            # Inject paths that pass the REPO_ROOT prefix filter so the
            # assertion exercises the resolution path.
            report = {
                "functions": [
                    {"file_path": str(gate.REPO_ROOT / "tests" / "tmp.txt")},
                    {"file_path": str(real)},
                ]
            }
            del fake  # silence unused
            failures = gate.assert_fixture(
                self._fixture(artifact_paths_must_resolve=True),
                report,
                0,
                "",
            )
            # The synthetic path under REPO_ROOT does not exist; expect a
            # dangling-paths failure.
            self.assertTrue(
                any("artifact_paths_must_resolve" in f for f in failures)
            )

    def test_no_target_reason_match_pass(self) -> None:
        report = {
            "codebase": {
                "skipped_functions": [
                    {
                        "file_path": "/x/foo.d.ts",
                        "no_target_reason": "declaration_only",
                    }
                ]
            }
        }
        failures = gate.assert_fixture(
            self._fixture(
                no_target_reasons=[
                    {"file": "foo.d.ts", "reason": "declaration_only"}
                ]
            ),
            report,
            0,
            "",
        )
        self.assertEqual(failures, [])

    def test_no_target_reason_one_of(self) -> None:
        report = {
            "codebase": {
                "skipped_functions": [
                    {
                        "file_path": "/x/lib.rs",
                        "no_target_reason": "frontend_unavailable",
                    }
                ]
            }
        }
        failures = gate.assert_fixture(
            self._fixture(
                no_target_reasons=[
                    {
                        "file": "lib.rs",
                        "reason_one_of": [
                            "skipped_by_unavailable_frontend",
                            "frontend_unavailable",
                        ],
                    }
                ]
            ),
            report,
            0,
            "",
        )
        self.assertEqual(failures, [])

    def test_no_target_reason_mismatch(self) -> None:
        report = {
            "codebase": {
                "skipped_functions": [
                    {"file_path": "/x/foo.d.ts", "no_target_reason": "unclassified"}
                ]
            }
        }
        failures = gate.assert_fixture(
            self._fixture(
                no_target_reasons=[
                    {"file": "foo.d.ts", "reason": "declaration_only"}
                ]
            ),
            report,
            0,
            "",
        )
        self.assertTrue(any("foo.d.ts" in f and "unclassified" in f for f in failures))


class ManifestLoaderTest(unittest.TestCase):
    def test_real_manifest_loads(self) -> None:
        # Repo manifest must be syntactically valid YAML and contain the
        # expected fixture IDs.
        manifest = gate.load_manifest(
            gate.REPO_ROOT / "tests/broad-run-corpus/manifest.yaml"
        )
        ids = [f["id"] for f in manifest["fixtures"]]
        for required in ("go-internal-method", "rust-unavailable", "source-churn"):
            self.assertIn(required, ids)

    def test_rejects_non_mapping(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            bad = Path(tmp) / "bad.yaml"
            bad.write_text("- just\n- a\n- list\n")
            with self.assertRaises(ValueError):
                gate.load_manifest(bad)


if __name__ == "__main__":
    unittest.main()
