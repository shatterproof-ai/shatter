#!/usr/bin/env python3
"""Generate summary reports from perf runner artifacts."""

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parent.parent


def display_path(path: Path) -> str:
    try:
        return str(path.relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


@dataclass
class ScenarioRun:
    scenario_id: str
    timestamp: str
    summary_path: Path
    summary: dict[str, Any]


def load_runs(results_dir: Path) -> dict[str, list[ScenarioRun]]:
    grouped: dict[str, list[ScenarioRun]] = {}
    if not results_dir.exists():
        return grouped
    for summary_path in sorted(results_dir.glob("*/*/summary.json")):
        timestamp = summary_path.parent.name
        scenario_id = summary_path.parent.parent.name
        summary = json.loads(summary_path.read_text())
        grouped.setdefault(scenario_id, []).append(
            ScenarioRun(
                scenario_id=scenario_id,
                timestamp=timestamp,
                summary_path=summary_path,
                summary=summary,
            )
        )
    return grouped


def format_pct(delta: float | None) -> str:
    if delta is None:
        return "n/a"
    sign = "+" if delta >= 0 else ""
    return f"{sign}{delta:.1f}%"


def is_unstable(summary: dict[str, Any]) -> bool:
    min_seconds = summary.get("min_seconds")
    max_seconds = summary.get("max_seconds")
    if not min_seconds or not max_seconds:
        return False
    if min_seconds == 0:
        return True
    return (max_seconds / min_seconds) > 1.5


def choose_next_step(summary: dict[str, Any], regression_pct: float | None) -> str:
    profiler = summary.get("active_profiler")
    if profiler == "perf-record":
        return "File optimization issue"
    if profiler == "pprof":
        return "Inspect go tool pprof"
    if regression_pct is not None and regression_pct > 10:
        return "Run perf-record"
    if profiler == "perf-stat":
        return "Review counters"
    return "Run perf-stat"


def build_report(results_dir: Path) -> dict[str, Any]:
    grouped = load_runs(results_dir)
    scenarios: list[dict[str, Any]] = []
    for scenario_id, runs in grouped.items():
        runs.sort(key=lambda item: item.timestamp)
        current = runs[-1]
        previous = runs[-2] if len(runs) > 1 else None
        current_median = current.summary.get("median_seconds")
        previous_median = previous.summary.get("median_seconds") if previous else None
        regression_pct = None
        if previous_median not in (None, 0):
            regression_pct = ((current_median - previous_median) / previous_median) * 100.0
        scenario_report = {
            "scenario_id": scenario_id,
            "description": current.summary.get("description", ""),
            "timestamp": current.timestamp,
            "median_seconds": current_median,
            "previous_median_seconds": previous_median,
            "regression_pct": regression_pct,
            "unstable": is_unstable(current.summary),
            "active_profiler": current.summary.get("active_profiler"),
            "summary_path": display_path(current.summary_path),
            "perf_record_paths": current.summary.get("perf_record_paths", []),
            "pprof_profiles": current.summary.get("pprof_profiles", []),
            "next_step": choose_next_step(current.summary, regression_pct),
        }
        scenarios.append(scenario_report)
    scenarios.sort(key=lambda item: item["median_seconds"], reverse=True)
    return {"results_dir": display_path(results_dir), "scenarios": scenarios}


def render_markdown(report: dict[str, Any]) -> str:
    lines = [
        "# External Profiling Summary",
        "",
        f"Results directory: `{report['results_dir']}`",
        "",
        "| Scenario | Median (s) | Delta vs prior | Unstable | Next step |",
        "| --- | ---: | ---: | --- | --- |",
    ]
    for scenario in report["scenarios"]:
        lines.append(
            "| {scenario_id} | {median:.3f} | {delta} | {unstable} | {next_step} |".format(
                scenario_id=scenario["scenario_id"],
                median=scenario["median_seconds"],
                delta=format_pct(scenario["regression_pct"]),
                unstable="yes" if scenario["unstable"] else "no",
                next_step=scenario["next_step"],
            )
        )
        lines.append(
            f"Artifacts: `{scenario['summary_path']}`"
        )
        for artifact in scenario["perf_record_paths"]:
            lines.append(f"- perf-record: `{artifact}`")
        for artifact in scenario["pprof_profiles"]:
            lines.append(
                f"- pprof: `{artifact['profile']}` (binary `{artifact['binary']}`)"
            )
    if not report["scenarios"]:
        lines.extend(["", "No profiling results were found."])
    return "\n".join(lines) + "\n"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--results-dir",
        required=True,
        help="Directory containing perf runner artifacts",
    )
    parser.add_argument(
        "--markdown-out",
        help="Markdown output path (defaults to <results-dir>/report-latest.md)",
    )
    parser.add_argument(
        "--json-out",
        help="JSON output path (defaults to <results-dir>/report-latest.json)",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    results_dir = Path(args.results_dir)
    report = build_report(results_dir)
    markdown = render_markdown(report)

    markdown_out = (
        Path(args.markdown_out)
        if args.markdown_out
        else results_dir / "report-latest.md"
    )
    json_out = (
        Path(args.json_out)
        if args.json_out
        else results_dir / "report-latest.json"
    )
    markdown_out.parent.mkdir(parents=True, exist_ok=True)
    json_out.parent.mkdir(parents=True, exist_ok=True)
    markdown_out.write_text(markdown, encoding="utf-8")
    json_out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    print(f"wrote {display_path(markdown_out)}")
    print(f"wrote {display_path(json_out)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
