#!/usr/bin/env python3
"""Flat-vs-static execution-budget allocation benchmark (str-03mfx.4).

Runs `shatter scan --concolic` over a corpus once per arm and seed, each in
a fresh copy of the corpus with `--no-cache --no-seeds --parallelism 1`, and
compares the scan JSON (`functions[]`: branch_count, branches_covered,
lines_covered, total_lines, iterations, completion_outcome,
budget_allocated, budget_claimed) between `flat` and `static` at identical
Σ budget_allocated. Writes `report.md` and `report.json` into the results
directory. A sanity gate checks that the `flat` arm equals a knob-absent run
with the same seed (seeded runs are repeatable since str-03mfx.5); a
mismatch exits 1.

    python3 scripts/bench_budget_alloc.py --corpus <dir> [--include '*.ts'] \
        [--seeds 1,2,3] [--max-iterations 100] [--results-dir target/bench-budget]
    python3 scripts/bench_budget_alloc.py --from-json flat.json static.json   # parse only
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ARMS = ("flat", "static")


def load_report(path: Path) -> list[dict]:
    """Per-function rows from a scan JSON report, sorted by qualified id."""
    value = json.loads(path.read_text(encoding="utf-8"))
    rows = []
    for f in value.get("functions", []):
        rows.append({
            "id": f.get("qualified_id") or f.get("function_name"),
            "branch_count": int(f.get("branch_count") or 0),
            "branches_covered": int(f.get("branches_covered") or 0),
            "lines_covered": int(f.get("lines_covered") or 0),
            "total_lines": int(f.get("total_lines") or 0),
            "iterations": int(f.get("iterations") or 0),
            "outcome": str(f.get("completion_outcome") or ""),
            "budget_allocated": int(f.get("budget_allocated") or 0),
            "budget_claimed": int(f.get("budget_claimed") or 0),
        })
    rows.sort(key=lambda r: r["id"])
    return rows


def load_report_rows(functions: list[dict]) -> list[dict]:
    """Same normalisation as load_report, from in-memory function dicts."""
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as tmp:
        json.dump({"functions": functions}, tmp)
        name = tmp.name
    try:
        return load_report(Path(name))
    finally:
        os.unlink(name)


def totals(rows: list[dict]) -> dict:
    t = {k: sum(r[k] for r in rows) for k in ("branch_count", "branches_covered", "lines_covered", "total_lines", "iterations", "budget_allocated", "budget_claimed")}
    t["functions"] = len(rows)
    t["outcomes"] = {}
    for r in rows:
        t["outcomes"][r["outcome"]] = t["outcomes"].get(r["outcome"], 0) + 1
    return t


def coverage_signature(rows: list[dict]) -> list[tuple]:
    return [(r["id"], r["branches_covered"], r["lines_covered"]) for r in rows]


def paired_deltas(per_seed: dict) -> dict:
    """per_seed: {seed: {"flat": totals, "static": totals}} → medians of static − flat."""
    out = {}
    for key in ("branches_covered", "lines_covered", "iterations", "wall_s"):
        deltas = [s["static"][key] - s["flat"][key] for s in per_seed.values() if "flat" in s and "static" in s]
        out[key] = {"n": len(deltas), "median_delta": statistics.median(deltas) if deltas else None, "deltas": deltas}
    return out


def run_scan(shatter: str, corpus: Path, out: Path, seed: int, max_iterations: int, include: str | None, arm: str | None, extra: list[str]) -> float:
    with tempfile.TemporaryDirectory(prefix="bench-budget-") as tmp:
        copy = Path(tmp) / "corpus"
        shutil.copytree(corpus, copy, symlinks=True)
        cmd = [shatter, "scan", str(copy), "--concolic", "--parallelism", "1", "--seed", str(seed),
               "--max-iterations", str(max_iterations), "--no-cache", "--no-seeds", "-o", str(out)]
        if include:
            cmd += ["--include", include]
        if arm:
            cmd += ["--set", f"defaults.exploration.budget_allocation={arm}"]
        cmd += extra
        t0 = time.perf_counter()
        env = dict(os.environ, SHATTER_ALLOW_HOST_WRITES="1")
        proc = subprocess.run(cmd, capture_output=True, text=True, env=env)
        wall = time.perf_counter() - t0
        if proc.returncode != 0 or not out.exists():
            print(f"scan failed ({proc.returncode}): {' '.join(cmd)}\n{proc.stderr[-2000:]}", file=sys.stderr)
            raise SystemExit(2)
        return wall


def render_markdown(summary: dict) -> str:
    lines = [
        "# Budget-allocation benchmark",
        "",
        f"Corpus: `{summary['corpus']}`. Seeds: {summary['seeds']}. Per-function flat budget: {summary['max_iterations']} paths × 5 executions.",
        "",
        "## Totals (median over seeds)",
        "",
        "| arm | Σ branches covered / total | Σ lines covered / total | Σ executions used | Σ allocated | Σ claimed | wall s |",
        "|---|---|---|---|---|---|---|",
    ]
    for arm in ARMS:
        m = summary["median"][arm]
        lines.append(f"| {arm} | {m['branches_covered']:.0f} / {m['branch_count']:.0f} | {m['lines_covered']:.0f} / {m['total_lines']:.0f} | {m['iterations']:.0f} | {m['budget_allocated']:.0f} | {m['budget_claimed']:.0f} | {m['wall_s']:.1f} |")
    d = summary["paired"]
    lines += ["", "## Paired static − flat (per seed)", "", "| metric | n | median Δ | per-seed |", "|---|---|---|---|"]
    for k in ("branches_covered", "lines_covered", "iterations", "wall_s"):
        v = d[k]
        med = "—" if v["median_delta"] is None else f"{v['median_delta']:+.1f}"
        lines.append(f"| {k} | {v['n']} | {med} | {', '.join(f'{x:+.1f}' for x in v['deltas'])} |")
    lines += ["", "## Outcomes (seed %s)" % summary["seeds"][0], "", "| arm | " + " | ".join(summary["outcome_keys"]) + " |", "|---|" + "---|" * len(summary["outcome_keys"])]
    for arm in ARMS:
        oc = summary["first_seed"][arm]["outcomes"]
        lines.append(f"| {arm} | " + " | ".join(str(oc.get(k, 0)) for k in summary["outcome_keys"]) + " |")
    lines += ["", f"## Per function (seed {summary['seeds'][0]}, static allocation descending)", "", "| function | allocated | claimed | executions flat/static | branches flat/static | lines flat/static |", "|---|---|---|---|---|---|"]
    for r in summary["per_function"]:
        lines.append(f"| {r['id']} | {r['allocated']} | {r['claimed']} | {r['it_flat']}/{r['it_static']} | {r['br_flat']}/{r['br_static']} | {r['ln_flat']}/{r['ln_static']} |")
    lines += ["", f"Sanity gate (flat == knob-absent, seed {summary['seeds'][0]}): {'PASS' if summary['sanity_ok'] else 'FAIL'}."]
    return "\n".join(lines) + "\n"


def summarize(corpus: str, seeds: list[int], max_iterations: int, runs: dict, sanity_ok: bool) -> dict:
    """runs: {seed: {arm: {"rows": rows, "wall_s": float}}}"""
    per_seed = {}
    for seed, arms in runs.items():
        per_seed[seed] = {}
        for arm, run in arms.items():
            t = totals(run["rows"]); t["wall_s"] = run["wall_s"]
            per_seed[seed][arm] = t
    median = {}
    for arm in ARMS:
        median[arm] = {}
        for key in ("branch_count", "branches_covered", "lines_covered", "total_lines", "iterations", "budget_allocated", "budget_claimed", "wall_s"):
            vals = [s[arm][key] for s in per_seed.values() if arm in s]
            median[arm][key] = statistics.median(vals) if vals else 0.0
    first = seeds[0]
    flat_rows = {r["id"]: r for r in runs[first]["flat"]["rows"]}
    static_rows = {r["id"]: r for r in runs[first]["static"]["rows"]}
    per_function = []
    for fid, s in static_rows.items():
        f = flat_rows.get(fid, {})
        per_function.append({"id": fid, "allocated": s["budget_allocated"], "claimed": s["budget_claimed"],
                             "it_flat": f.get("iterations", 0), "it_static": s["iterations"],
                             "br_flat": f.get("branches_covered", 0), "br_static": s["branches_covered"],
                             "ln_flat": f.get("lines_covered", 0), "ln_static": s["lines_covered"]})
    per_function.sort(key=lambda r: -r["allocated"])
    outcome_keys = sorted({k for s in per_seed.values() for arm in s.values() for k in arm["outcomes"]})
    return {"corpus": corpus, "seeds": seeds, "max_iterations": max_iterations, "median": median,
            "paired": paired_deltas(per_seed), "per_seed": per_seed, "per_function": per_function,
            "first_seed": per_seed[first], "outcome_keys": outcome_keys, "sanity_ok": sanity_ok}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--corpus", type=Path)
    ap.add_argument("--include", default=None, help="glob passed to shatter scan --include")
    ap.add_argument("--seeds", default="1,2,3")
    ap.add_argument("--max-iterations", type=int, default=100)
    ap.add_argument("--results-dir", type=Path, default=Path("target/bench-budget"))
    ap.add_argument("--shatter", default=os.environ.get("SHATTER_BIN", "target/release/shatter"))
    ap.add_argument("--from-json", nargs=2, metavar=("FLAT", "STATIC"), help="parse two existing reports instead of scanning")
    ap.add_argument("extra", nargs="*", help="extra args passed to shatter scan (after --)")
    args = ap.parse_args()

    if args.from_json:
        runs = {1: {"flat": {"rows": load_report(Path(args.from_json[0])), "wall_s": 0.0},
                    "static": {"rows": load_report(Path(args.from_json[1])), "wall_s": 0.0}}}
        summary = summarize(str(args.from_json), [1], args.max_iterations, runs, True)
        args.results_dir.mkdir(parents=True, exist_ok=True)
    else:
        if not args.corpus:
            ap.error("--corpus is required unless --from-json is given")
        seeds = [int(s) for s in args.seeds.split(",") if s.strip()]
        args.results_dir.mkdir(parents=True, exist_ok=True)
        runs: dict = {}
        for seed in seeds:
            runs[seed] = {}
            for arm in ARMS:
                out = args.results_dir / f"{arm}-seed{seed}.json"
                wall = run_scan(args.shatter, args.corpus, out, seed, args.max_iterations, args.include, arm, args.extra)
                runs[seed][arm] = {"rows": load_report(out), "wall_s": wall}
                print(f"{arm} seed={seed}: {len(runs[seed][arm]['rows'])} functions, {wall:.1f}s", file=sys.stderr)
        absent = args.results_dir / f"absent-seed{seeds[0]}.json"
        run_scan(args.shatter, args.corpus, absent, seeds[0], args.max_iterations, args.include, None, args.extra)
        sanity_ok = coverage_signature(load_report(absent)) == coverage_signature(runs[seeds[0]]["flat"]["rows"])
        summary = summarize(str(args.corpus), seeds, args.max_iterations, runs, sanity_ok)

    (args.results_dir / "report.json").write_text(json.dumps(summary, indent=2, default=str), encoding="utf-8")
    md = render_markdown(summary)
    (args.results_dir / "report.md").write_text(md, encoding="utf-8")
    print(md)
    if not summary["sanity_ok"]:
        print("SANITY GATE FAILED: flat arm differs from the knob-absent run with the same seed", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
