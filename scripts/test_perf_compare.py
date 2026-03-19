from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("perf_compare.py")
SPEC = importlib.util.spec_from_file_location("perf_compare", MODULE_PATH)
assert SPEC is not None
assert SPEC.loader is not None
perf_compare = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = perf_compare
SPEC.loader.exec_module(perf_compare)


def write_summary(root: Path, scenario_id: str, timestamp: str, summary: dict) -> None:
    summary_dir = root / scenario_id / timestamp
    summary_dir.mkdir(parents=True, exist_ok=True)
    (summary_dir / "summary.json").write_text(json.dumps(summary), encoding="utf-8")


class PerfCompareTest(unittest.TestCase):
    def test_flags_wall_and_timing_regressions(self) -> None:
        with tempfile.TemporaryDirectory() as baseline_tmp, tempfile.TemporaryDirectory() as candidate_tmp:
            baseline_dir = Path(baseline_tmp)
            candidate_dir = Path(candidate_tmp)
            write_summary(
                baseline_dir,
                "explore-ts",
                "20260318T000000Z",
                {
                    "description": "baseline",
                    "median_seconds": 1.0,
                    "timing": {
                        "metrics_ms": {
                            "total_ms": {"median": 1000.0},
                            "shatter_overhead_ms": {"median": 800.0},
                        },
                        "phases": [
                            {"phase_path": "frontend.remote.execute.total", "median_ms": 300.0},
                            {"phase_path": "core.explore_command", "median_ms": 900.0},
                        ],
                    },
                },
            )
            write_summary(
                candidate_dir,
                "explore-ts",
                "20260318T010000Z",
                {
                    "description": "candidate",
                    "median_seconds": 1.4,
                    "timing": {
                        "metrics_ms": {
                            "total_ms": {"median": 1400.0},
                            "shatter_overhead_ms": {"median": 1150.0},
                        },
                        "phases": [
                            {"phase_path": "frontend.remote.execute.total", "median_ms": 420.0},
                            {"phase_path": "core.explore_command", "median_ms": 1180.0},
                        ],
                    },
                },
            )

            report = perf_compare.build_report(
                baseline_dir,
                candidate_dir,
                scenario_abs_seconds=0.25,
                scenario_pct=10.0,
                timing_abs_ms=25.0,
                timing_pct=10.0,
                phase_abs_ms=50.0,
                phase_pct=10.0,
            )

            self.assertEqual(report["regression_count"], 1)
            scenario = report["scenarios"][0]
            self.assertEqual(scenario["status"], "regressed")
            self.assertEqual(scenario["wall_time"]["status"], "regressed")
            self.assertEqual(
                scenario["timing_metrics"]["shatter_overhead_ms"]["status"], "regressed"
            )
            phase_names = {phase["phase_path"] for phase in scenario["phase_regressions"]}
            self.assertIn("frontend.remote.execute.total", phase_names)

    def test_ignores_small_changes_and_reports_missing_scenarios(self) -> None:
        with tempfile.TemporaryDirectory() as baseline_tmp, tempfile.TemporaryDirectory() as candidate_tmp:
            baseline_dir = Path(baseline_tmp)
            candidate_dir = Path(candidate_tmp)
            write_summary(
                baseline_dir,
                "explore-go",
                "20260318T000000Z",
                {"description": "baseline", "median_seconds": 2.0},
            )
            write_summary(
                candidate_dir,
                "explore-go",
                "20260318T010000Z",
                {"description": "candidate", "median_seconds": 2.05},
            )
            write_summary(
                candidate_dir,
                "explore-rust",
                "20260318T010000Z",
                {"description": "extra", "median_seconds": 1.0},
            )

            report = perf_compare.build_report(
                baseline_dir,
                candidate_dir,
                scenario_abs_seconds=0.25,
                scenario_pct=10.0,
                timing_abs_ms=25.0,
                timing_pct=10.0,
                phase_abs_ms=50.0,
                phase_pct=10.0,
            )

            self.assertEqual(report["regression_count"], 0)
            self.assertEqual(report["candidate_only_scenarios"], ["explore-rust"])
            self.assertEqual(report["scenarios"][0]["status"], "unchanged")


if __name__ == "__main__":
    unittest.main()
