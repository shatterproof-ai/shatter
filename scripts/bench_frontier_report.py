#!/usr/bin/env python3
"""Summarize frontier-ranking benchmark rows (str-hjrnp.4).

Reads the JSONL written by ``shatter-core/tests/bench_frontier_ranking.rs``
and ``benchmarks/frontier-ranking/reference.json``; writes ``report.json``
and ``report.md`` next to the rows. Exits 1 when a sanity gate fails:
the cheating ranker must beat the heuristic and the heuristic must beat
random on median executions-to-cover, otherwise the harness cannot
separate a ceiling from a floor and no other arm's number means anything.

Row schema (one JSON object per line):
  fixture, stratum, seed, arm, regime, budget, total_executions, wall_ms,
  discoveries: [[side_id, execution_index]], branch_discoveries,
  expected_return_values_hit, expected_return_values_total,
  methods: {DiscoveryMethod: count}, rank_log: [[round, exec, branch_id, score]],
  decision_tokens, oracle_tokens
where side_id = branch_id * 2 + taken.
"""

from __future__ import annotations

import argparse
import json
import random
import statistics
import sys
from pathlib import Path
from typing import Callable

BASE_ARM = "heuristic"
EXEC_REGIME = "executions"
WALL_REGIME = "wallclock"


def load_rows(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def executions_to_cover(row: dict, target_ids: set[int]) -> int | None:
    """Execution index at which every target side id has been discovered, else None."""
    if not target_ids:
        return None
    seen = {int(sid): int(idx) for sid, idx in row["discoveries"]}
    if not target_ids.issubset(seen):
        return None
    return max(seen[s] for s in target_ids)


def censored_executions_to_cover(row: dict, target_ids: set[int]) -> int | None:
    """Like executions_to_cover, but a run that never covers within budget
    counts as ``budget + 1`` instead of being dropped, so medians and paired
    deltas do not suffer survivorship bias (an arm that only covers the easy
    fixtures would otherwise look faster than one that covers them all).
    Returns None only when there are no targets."""
    if not target_ids:
        return None
    covered = executions_to_cover(row, target_ids)
    if covered is not None:
        return covered
    return int(row["budget"]) + 1


def coverage_fraction(row: dict, target_ids: set[int]) -> float:
    if not target_ids:
        return 0.0
    seen = {int(sid) for sid, _ in row["discoveries"]}
    return len(seen & target_ids) / len(target_ids)


def _key(row: dict) -> tuple:
    return (row["fixture"], row["seed"], row["regime"])


def paired_deltas(
    rows: list[dict],
    base_arm: str,
    arm: str,
    metric: Callable[[dict], float | None],
) -> list[float]:
    """metric(arm) - metric(base) for every (fixture, seed, regime) both arms ran."""
    base = {_key(r): r for r in rows if r["arm"] == base_arm}
    out: list[float] = []
    for r in rows:
        if r["arm"] != arm or _key(r) not in base:
            continue
        a, b = metric(r), metric(base[_key(r)])
        if a is None or b is None:
            continue
        out.append(a - b)
    return out


def bootstrap_ci(values: list[float], iters: int = 2000, seed: int = 0, alpha: float = 0.05) -> tuple[float, float]:
    """Percentile bootstrap interval for the median. Deterministic in `seed`."""
    if not values:
        return (float("nan"), float("nan"))
    rng = random.Random(seed)
    meds = sorted(statistics.median(rng.choices(values, k=len(values))) for _ in range(iters))
    lo = meds[int(alpha / 2 * iters)]
    hi = meds[min(iters - 1, int((1 - alpha / 2) * iters))]
    return (lo, hi)


def calibration_bins(rows: list[dict], bins: int = 10, window: int = 20) -> list[dict]:
    """Hit rate per score bin.

    A rank-log entry (round, exec, branch_id, score) is a hit when some side
    of `branch_id` is first discovered at an observation index in
    (exec, exec + window]. `exec` is recorded in the orchestrator's
    `total_executions` space, while discovery indices count observations
    (`observed_executions`), so `exec` is rescaled by
    observed_executions / total_executions before comparing. Bin index is
    min(int(score * bins), bins - 1), clamped at 0.
    """
    hits: list[list[int]] = [[] for _ in range(bins)]
    for r in rows:
        first_by_branch: dict[int, list[int]] = {}
        for sid, idx in r["discoveries"]:
            first_by_branch.setdefault(int(sid) // 2, []).append(int(idx))
        total = int(r.get("total_executions") or 0)
        observed = int(r.get("observed_executions") or total)
        scale = (observed / total) if total else 1.0
        for _round, at_exec, bid, score in r["rank_log"]:
            at_obs = int(round(at_exec * scale))
            b = max(0, min(int(float(score) * bins), bins - 1))
            found = any(at_obs < idx <= at_obs + window for idx in first_by_branch.get(int(bid), []))
            hits[b].append(1 if found else 0)
    return [
        {
            "lo": round(i / bins, 2),
            "hi": round((i + 1) / bins, 2),
            "n": len(h),
            "hit_rate": (sum(h) / len(h)) if h else None,
        }
        for i, h in enumerate(hits)
    ]


def _median(values: list[float]) -> float | None:
    return statistics.median(values) if values else None


def summarize(rows: list[dict], reference: dict) -> dict:
    arms = sorted({r["arm"] for r in rows})
    strata = sorted({r["stratum"] for r in rows})

    def targets(r: dict) -> set[int]:
        return {int(x) for x in reference.get(r["fixture"], {}).get("side_ids", [])}

    def cover(r: dict) -> float | None:
        return censored_executions_to_cover(r, targets(r))

    def covered(r: dict) -> bool:
        return executions_to_cover(r, targets(r)) is not None

    def frac(r: dict) -> float:
        return coverage_fraction(r, targets(r))

    exec_rows = [r for r in rows if r["regime"] == EXEC_REGIME]
    wall_rows = [r for r in rows if r["regime"] == WALL_REGIME]

    def by_arm(arm: str, pool: list[dict]) -> list[dict]:
        return [r for r in pool if r["arm"] == arm]

    summary: dict = {
        "arms": arms,
        "n_rows": len(rows),
        "median_exec_to_cover": {
            a: _median([c for r in by_arm(a, exec_rows) for c in [cover(r)] if c is not None]) for a in arms
        },
        "cover_rate": {
            a: (
                sum(1 for r in by_arm(a, exec_rows) if covered(r)) / len(by_arm(a, exec_rows))
                if by_arm(a, exec_rows)
                else None
            )
            for a in arms
        },
        "median_side_coverage_exec": {a: _median([frac(r) for r in by_arm(a, exec_rows)]) for a in arms},
        "median_side_coverage_wall": {a: _median([frac(r) for r in by_arm(a, wall_rows)]) for a in arms},
        "median_wall_ms_exec": {a: _median([r["wall_ms"] for r in by_arm(a, exec_rows)]) for a in arms},
        "tokens": {a: sum(r["decision_tokens"] + r["oracle_tokens"] for r in by_arm(a, rows)) for a in arms},
        "paired_vs_heuristic": {},
        "per_stratum": {},
        "calibration": calibration_bins([r for r in rows if r["arm"] == "jev"]),
    }
    for a in arms:
        if a == BASE_ARM:
            continue
        d = paired_deltas(exec_rows, BASE_ARM, a, cover)
        summary["paired_vs_heuristic"][a] = {
            "n": len(d),
            "median_delta": _median(d),
            "ci95": bootstrap_ci(d),
        }
    for s in strata:
        pool = [r for r in exec_rows if r["stratum"] == s]
        summary["per_stratum"][s] = {
            a: _median([c for r in by_arm(a, pool) for c in [cover(r)] if c is not None]) for a in arms
        }
    return summary


def sanity_gates(summary: dict) -> list[str]:
    m = summary["median_exec_to_cover"]
    failures: list[str] = []
    base = m.get(BASE_ARM)
    if base is None:
        return ["heuristic arm has no covering runs; cannot evaluate gates"]
    cheat = m.get("cheating")
    if cheat is not None and not cheat < base:
        failures.append(f"cheating ranker ({cheat}) did not beat heuristic ({base})")
    rnd = m.get("random")
    if rnd is not None and not base < rnd:
        failures.append(f"heuristic ({base}) did not beat random ({rnd})")
    return failures


def _fmt(v: float | None, nd: int = 2) -> str:
    if v is None:
        return "—"
    if isinstance(v, float) and v != v:  # NaN
        return "—"
    if isinstance(v, (int, float)):
        return f"{v:.{nd}f}"
    return str(v)


def render_markdown(summary: dict) -> str:
    arms = summary["arms"]
    lines = [
        "# Frontier-ranking benchmark",
        "",
        f"Rows: {summary['n_rows']}. Executions-to-cover counts observations until every reference branch side is seen (fixed-execution regime); runs that never cover are censored at budget + 1.",
        "",
        "## Executions to cover all reference sides (median)",
        "",
        "| arm | median | cover rate | Δ vs heuristic (95% CI) | median side coverage | tokens |",
        "|---|---|---|---|---|---|",
    ]
    for a in arms:
        p = summary["paired_vs_heuristic"].get(a)
        if p and p["median_delta"] is not None:
            lo, hi = p["ci95"]
            delta = f"{p['median_delta']:+.1f} ({lo:+.1f}, {hi:+.1f}), n={p['n']}"
        else:
            delta = "—"
        lines.append(
            f"| {a} | {_fmt(summary['median_exec_to_cover'][a], 1)} | {_fmt(summary['cover_rate'][a])} | {delta} | "
            f"{_fmt(summary['median_side_coverage_exec'][a])} | {summary['tokens'][a]} |"
        )
    lines += [
        "",
        "## Pareto: side coverage under fixed wall-clock vs median ms per fixed-execution run",
        "",
        "| arm | median side coverage (wall-clock) | median wall ms (executions) |",
        "|---|---|---|",
    ]
    for a in arms:
        lines.append(f"| {a} | {_fmt(summary['median_side_coverage_wall'][a])} | {_fmt(summary['median_wall_ms_exec'][a], 0)} |")
    lines += ["", "## Per stratum (median executions to cover)", "", "| stratum | " + " | ".join(arms) + " |", "|---|" + "---|" * len(arms)]
    for s, per in summary["per_stratum"].items():
        lines.append(f"| {s} | " + " | ".join(_fmt(per[a], 1) for a in arms) + " |")
    lines += ["", "## Jev calibration (some side of the scored branch found within 20 executions, by score bin)", "", "| bin | n | hit rate |", "|---|---|---|"]
    for b in summary["calibration"]:
        lines.append(f"| {b['lo']}–{b['hi']} | {b['n']} | {_fmt(b['hit_rate'])} |")
    return "\n".join(lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--rows", type=Path, required=True, help="rows.jsonl from the bench runner")
    ap.add_argument("--reference", type=Path, default=Path("benchmarks/frontier-ranking/reference.json"))
    args = ap.parse_args()
    rows = load_rows(args.rows)
    if not rows:
        print("no rows", file=sys.stderr)
        return 1
    reference = json.loads(args.reference.read_text(encoding="utf-8"))
    summary = summarize(rows, reference)
    (args.rows.parent / "report.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    md = render_markdown(summary)
    (args.rows.parent / "report.md").write_text(md, encoding="utf-8")
    print(md)
    failures = sanity_gates(summary)
    for f in failures:
        print(f"SANITY GATE FAILED: {f}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
