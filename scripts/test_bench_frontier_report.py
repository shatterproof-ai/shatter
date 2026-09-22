#!/usr/bin/env python3
"""Unit tests for bench_frontier_report.py (str-hjrnp.4)."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import bench_frontier_report as r  # noqa: E402


def row(arm, seed, disc, fixture="f", regime="executions", stratum="z3-easy", rank_log=None, wall=100):
    return {
        "fixture": fixture, "stratum": stratum, "seed": seed, "arm": arm, "regime": regime, "budget": 60,
        "total_executions": 60, "wall_ms": wall, "discoveries": disc, "branch_discoveries": [],
        "expected_return_values_hit": 1, "expected_return_values_total": 1, "methods": {},
        "rank_log": rank_log or [], "decision_tokens": 0, "oracle_tokens": 0,
    }


class ExecutionsToCover(unittest.TestCase):
    def test_returns_index_of_last_target_discovery(self):
        self.assertEqual(r.executions_to_cover(row("a", 1, [[2, 3], [3, 9], [7, 20]]), {2, 3}), 9)

    def test_none_when_a_target_is_missing(self):
        self.assertIsNone(r.executions_to_cover(row("a", 1, [[2, 3]]), {2, 3}))

    def test_none_for_empty_targets(self):
        self.assertIsNone(r.executions_to_cover(row("a", 1, [[2, 3]]), set()))

    def test_coverage_fraction(self):
        self.assertAlmostEqual(r.coverage_fraction(row("a", 1, [[2, 3], [9, 1]]), {2, 3, 4, 5}), 0.25)


class PairedDeltas(unittest.TestCase):
    def test_pairs_by_fixture_seed_regime(self):
        rows = [row("base", 1, [[1, 10]]), row("x", 1, [[1, 4]]), row("base", 2, [[1, 8]]), row("x", 2, [[1, 8]])]
        deltas = r.paired_deltas(rows, "base", "x", lambda rw: r.executions_to_cover(rw, {1}))
        self.assertEqual(sorted(deltas), [-6, 0])

    def test_unpaired_rows_are_skipped(self):
        rows = [row("base", 1, [[1, 10]]), row("x", 2, [[1, 4]])]
        self.assertEqual(r.paired_deltas(rows, "base", "x", lambda rw: r.executions_to_cover(rw, {1})), [])

    def test_uncovered_pairs_are_skipped(self):
        rows = [row("base", 1, [[1, 10]]), row("x", 1, [[9, 4]])]
        self.assertEqual(r.paired_deltas(rows, "base", "x", lambda rw: r.executions_to_cover(rw, {1})), [])


class Bootstrap(unittest.TestCase):
    def test_ci_brackets_median_and_is_deterministic(self):
        lo, hi = r.bootstrap_ci([1, 2, 3, 4, 100], seed=3)
        self.assertLessEqual(lo, 3)
        self.assertGreaterEqual(hi, 3)
        self.assertEqual((lo, hi), r.bootstrap_ci([1, 2, 3, 4, 100], seed=3))

    def test_empty_is_nan(self):
        lo, hi = r.bootstrap_ci([])
        self.assertNotEqual(lo, lo)  # NaN
        self.assertNotEqual(hi, hi)


class Calibration(unittest.TestCase):
    def test_bins_hit_rate_by_score(self):
        # branch 5 (side 10) found at exec 12, scored 0.9 at exec 10 -> hit;
        # branch 6 never found, scored 0.1 -> miss;
        # branch 5 scored again at exec 12 -> not a hit (strictly after).
        rows = [row("jev", 1, [[10, 12]], rank_log=[[1, 10, 5, 0.9], [1, 10, 6, 0.1], [2, 12, 5, 0.95]])]
        bins = r.calibration_bins(rows, bins=10, window=20)
        top = next(b for b in bins if b["lo"] == 0.9)
        self.assertEqual(top["n"], 2)
        self.assertAlmostEqual(top["hit_rate"], 0.5)
        bottom = next(b for b in bins if b["lo"] == 0.1)
        self.assertEqual(bottom["hit_rate"], 0.0)

    def test_score_one_lands_in_top_bin(self):
        rows = [row("jev", 1, [], rank_log=[[1, 0, 1, 1.0]])]
        bins = r.calibration_bins(rows, bins=10)
        self.assertEqual(bins[-1]["n"], 1)


class SanityGates(unittest.TestCase):
    def test_passes_when_ordered(self):
        s = {"median_exec_to_cover": {"cheating": 5, "heuristic": 10, "random": 20}}
        self.assertEqual(r.sanity_gates(s), [])

    def test_fails_when_random_beats_heuristic(self):
        s = {"median_exec_to_cover": {"cheating": 5, "heuristic": 20, "random": 10}}
        self.assertTrue(any("random" in m for m in r.sanity_gates(s)))

    def test_fails_when_cheating_ties_heuristic(self):
        s = {"median_exec_to_cover": {"cheating": 10, "heuristic": 10, "random": 20}}
        self.assertTrue(any("cheating" in m for m in r.sanity_gates(s)))

    def test_missing_arms_are_not_gated(self):
        s = {"median_exec_to_cover": {"heuristic": 10}}
        self.assertEqual(r.sanity_gates(s), [])


class Summarize(unittest.TestCase):
    def test_end_to_end_summary_and_markdown(self):
        ref = {"f": {"side_ids": [2, 3]}}
        rows = [
            row("heuristic", 1, [[2, 1], [3, 10]]), row("cheating", 1, [[2, 1], [3, 4]]), row("random", 1, [[2, 1], [3, 30]]),
            row("heuristic", 1, [[2, 1], [3, 10]], regime="wallclock"), row("cheating", 1, [[2, 1]], regime="wallclock"),
        ]
        s = r.summarize(rows, ref)
        self.assertEqual(s["median_exec_to_cover"], {"cheating": 4, "heuristic": 10, "random": 30})
        self.assertEqual(s["paired_vs_heuristic"]["cheating"]["median_delta"], -6)
        self.assertAlmostEqual(s["median_side_coverage_wall"]["cheating"], 0.5)
        self.assertEqual(r.sanity_gates(s), [])
        md = r.render_markdown(s)
        self.assertIn("| cheating | 4.0 |", md)
        self.assertIn("## Per stratum", md)


if __name__ == "__main__":
    unittest.main()
