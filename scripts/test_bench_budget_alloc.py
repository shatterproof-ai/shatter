#!/usr/bin/env python3
"""Unit tests for bench_budget_alloc.py (str-03mfx.4)."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import bench_budget_alloc as b  # noqa: E402


def report(functions):
    return {"version": 1, "functions": functions, "codebase": {}, "test_order": []}


def fn(name, it, bc, bcov, lc, tl, alloc=None, claimed=None, outcome="completed"):
    f = {"qualified_id": name, "function_name": name, "iterations": it, "branch_count": bc, "branches_covered": bcov,
         "lines_covered": lc, "total_lines": tl, "completion_outcome": outcome}
    if alloc is not None:
        f["budget_allocated"] = alloc
    if claimed is not None:
        f["budget_claimed"] = claimed
    return f


class LoadAndTotals(unittest.TestCase):
    def test_load_defaults_missing_budget_fields_to_zero(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "r.json"
            p.write_text(json.dumps(report([fn("b", 10, 4, 3, 8, 10), fn("a", 20, 6, 6, 12, 12, alloc=300, claimed=25)])))
            rows = b.load_report(p)
        self.assertEqual([r["id"] for r in rows], ["a", "b"])
        self.assertEqual(rows[1]["budget_allocated"], 0)
        self.assertEqual(rows[0]["budget_claimed"], 25)
        t = b.totals(rows)
        self.assertEqual(t["branches_covered"], 9)
        self.assertEqual(t["budget_allocated"], 300)
        self.assertEqual(t["outcomes"], {"completed": 2})

    def test_ids_are_corpus_relative(self):
        self.assertEqual(b.normalize_id("/tmp/bench-budget-abc/corpus/x/y.ts::f"), "x/y.ts::f")
        self.assertEqual(b.normalize_id("plain::f"), "plain::f")

    def test_coverage_signature_ignores_iterations(self):
        r1 = [{"id": "a", "branches_covered": 1, "lines_covered": 2, "iterations": 5}]
        r2 = [{"id": "a", "branches_covered": 1, "lines_covered": 2, "iterations": 9}]
        self.assertEqual(b.coverage_signature(r1), b.coverage_signature(r2))


class Summary(unittest.TestCase):
    def runs(self):
        flat = [fn("a", 100, 4, 3, 8, 10), fn("b", 100, 10, 5, 20, 30)]
        static = [fn("a", 40, 4, 3, 8, 10, alloc=40), fn("b", 160, 10, 8, 26, 30, alloc=160, claimed=0)]
        return {1: {"flat": {"rows": b.load_report_rows(flat), "wall_s": 10.0}, "static": {"rows": b.load_report_rows(static), "wall_s": 11.0}},
                2: {"flat": {"rows": b.load_report_rows(flat), "wall_s": 10.0}, "static": {"rows": b.load_report_rows(static), "wall_s": 9.0}}}

    def test_paired_deltas_and_markdown(self):
        s = b.summarize("corpus", [1, 2], 20, self.runs(), True)
        self.assertEqual(s["paired"]["branches_covered"]["median_delta"], 3)
        self.assertEqual(s["paired"]["lines_covered"]["deltas"], [6, 6])
        self.assertEqual(s["median"]["static"]["budget_allocated"], 200)
        self.assertEqual(s["per_function"][0]["id"], "b", "sorted by allocation descending")
        md = b.render_markdown(s)
        self.assertIn("| static | 11 / 14 |", md)
        self.assertIn("PASS", md)

    def test_sanity_flag_renders_fail(self):
        s = b.summarize("corpus", [1, 2], 20, self.runs(), False)
        self.assertIn("FAIL", b.render_markdown(s))

    def test_sanity_none_renders_not_run(self):
        s = b.summarize("corpus", [1, 2], 20, self.runs(), None)
        md = b.render_markdown(s)
        self.assertIn("not run", md)
        self.assertNotIn("PASS", md)

    def test_per_seed_comparison_reports_gains_losses_claims_and_conservation(self):
        s = b.summarize("corpus", [1, 2], 20, self.runs(), True)
        c = s["comparison"][1]
        self.assertEqual(c["gained"], ["b"])
        self.assertEqual(c["lost"], [])
        self.assertEqual(c["claimed"], 0)
        self.assertTrue(c["allocation_conserved"], "40 + 160 == 2 × 20 × 5")
        md = b.render_markdown(s)
        self.assertIn("| 1 | +3 | +6 |", md)
        self.assertIn("| 1 (b) | yes |", md)

    def test_conservation_flag_false_when_static_total_differs(self):
        flat = [fn("a", 100, 4, 3, 8, 10)]
        static = [fn("a", 40, 4, 2, 8, 10, alloc=40)]
        runs = {1: {"flat": {"rows": b.load_report_rows(flat), "wall_s": 1.0}, "static": {"rows": b.load_report_rows(static), "wall_s": 1.0}}}
        s = b.summarize("corpus", [1], 20, runs, True)
        c = s["comparison"][1]
        self.assertFalse(c["allocation_conserved"])
        self.assertEqual(c["lost"], ["a"])
        self.assertIn("NO (40 vs 100)", b.render_markdown(s))

    def test_per_function_is_the_union_of_both_arms(self):
        flat = [fn("a", 100, 4, 3, 8, 10), fn("only_flat", 5, 1, 1, 2, 2)]
        static = [fn("a", 40, 4, 3, 8, 10, alloc=40), fn("only_static", 5, 1, 1, 2, 2, alloc=60)]
        runs = {1: {"flat": {"rows": b.load_report_rows(flat), "wall_s": 1.0}, "static": {"rows": b.load_report_rows(static), "wall_s": 1.0}}}
        s = b.summarize("corpus", [1], 10, runs, True)
        ids = {r["id"]: r for r in s["per_function"]}
        self.assertEqual(set(ids), {"a", "only_flat", "only_static"})
        self.assertEqual(ids["only_flat"]["missing_in"], "static")
        self.assertEqual(ids["only_static"]["missing_in"], "flat")
        self.assertEqual(s["comparison"][1]["only_in_flat"], ["only_flat"])
        self.assertIn("only_flat (missing in static)", b.render_markdown(s))


if __name__ == "__main__":
    unittest.main()
