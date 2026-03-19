#!/usr/bin/env python3
"""Compare two perf result directories and flag regressions."""

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


def load_latest_runs(results_dir: Path) -> dict[str, ScenarioRun]:
    grouped: dict[str, list[ScenarioRun]] = {}
    if not results_dir.exists():
        return {}

    for summary_path in sorted(results_dir.glob("*/*/summary.json")):
        timestamp = summary_path.parent.name
        scenario_id = summary_path.parent.parent.name
        summary = json.loads(summary_path.read_text(encoding="utf-8"))
        grouped.setdefault(scenario_id, []).append(
            ScenarioRun(
                scenario_id=scenario_id,
                timestamp=timestamp,
                summary_path=summary_path,
                summary=summary,
            )
        )

    latest: dict[str, ScenarioRun] = {}
    for scenario_id, runs in grouped.items():
        runs.sort(key=lambda item: item.timestamp)
        latest[scenario_id] = runs[-1]
    return latest


def classify_delta(
    baseline: float | None,
    candidate: float | None,
    abs_threshold: float,
    pct_threshold: float,
) -> dict[str, Any]:
    if baseline is None or candidate is None:
        return {
            "baseline": baseline,
            "candidate": candidate,
            "delta": None,
            "delta_pct": None,
            "status": "missing",
        }

    delta = candidate - baseline
    delta_pct = None if baseline == 0 else (delta / baseline) * 100.0

    if delta > 0 and delta >= abs_threshold and (delta_pct is None or delta_pct >= pct_threshold):
        status = "regressed"
    elif delta < 0 and abs(delta) >= abs_threshold and (delta_pct is None or abs(delta_pct) >= pct_threshold):
        status = "improved"
    else:
        status = "unchanged"

    return {
        "baseline": baseline,
        "candidate": candidate,
        "delta": delta,
        "delta_pct": delta_pct,
        "status": status,
    }


def compare_phase_lists(
    baseline_phases: list[dict[str, Any]],
    candidate_phases: list[dict[str, Any]],
    abs_threshold_ms: float,
    pct_threshold: float,
) -> list[dict[str, Any]]:
    baseline_by_name = {phase["phase_path"]: phase for phase in baseline_phases}
    candidate_by_name = {phase["phase_path"]: phase for phase in candidate_phases}

    regressions: list[dict[str, Any]] = []
    for phase_path in sorted(baseline_by_name.keys() & candidate_by_name.keys()):
        baseline = float(baseline_by_name[phase_path].get("median_ms", 0.0))
        candidate = float(candidate_by_name[phase_path].get("median_ms", 0.0))
        comparison = classify_delta(baseline, candidate, abs_threshold_ms, pct_threshold)
        if comparison["status"] != "regressed":
            continue
        regressions.append(
            {
                "phase_path": phase_path,
                **comparison,
            }
        )

    regressions.sort(key=lambda item: (item["delta"], item["delta_pct"] or 0.0), reverse=True)
    return regressions


def build_report(
    baseline_dir: Path,
    candidate_dir: Path,
    scenario_abs_seconds: float,
    scenario_pct: float,
    timing_abs_ms: float,
    timing_pct: float,
    phase_abs_ms: float,
    phase_pct: float,
) -> dict[str, Any]:
    baseline_runs = load_latest_runs(baseline_dir)
    candidate_runs = load_latest_runs(candidate_dir)

    baseline_only = sorted(baseline_runs.keys() - candidate_runs.keys())
    candidate_only = sorted(candidate_runs.keys() - baseline_runs.keys())
    compared_ids = sorted(baseline_runs.keys() & candidate_runs.keys())

    scenarios: list[dict[str, Any]] = []
    regression_count = 0

    for scenario_id in compared_ids:
        baseline_run = baseline_runs[scenario_id]
        candidate_run = candidate_runs[scenario_id]
        baseline_summary = baseline_run.summary
        candidate_summary = candidate_run.summary

        wall_time = classify_delta(
            float(baseline_summary.get("median_seconds", 0.0)),
            float(candidate_summary.get("median_seconds", 0.0)),
            scenario_abs_seconds,
            scenario_pct,
        )

        timing_metrics: dict[str, dict[str, Any]] = {}
        baseline_timing = baseline_summary.get("timing", {})
        candidate_timing = candidate_summary.get("timing", {})
        baseline_metric_map = baseline_timing.get("metrics_ms", {})
        candidate_metric_map = candidate_timing.get("metrics_ms", {})
        for metric_name in sorted(baseline_metric_map.keys() & candidate_metric_map.keys()):
            timing_metrics[metric_name] = classify_delta(
                float(baseline_metric_map[metric_name].get("median", 0.0)),
                float(candidate_metric_map[metric_name].get("median", 0.0)),
                timing_abs_ms,
                timing_pct,
            )

        phase_regressions = compare_phase_lists(
            baseline_timing.get("phases", []),
            candidate_timing.get("phases", []),
            phase_abs_ms,
            phase_pct,
        )

        scenario_status = "unchanged"
        if (
            wall_time["status"] == "regressed"
            or any(metric["status"] == "regressed" for metric in timing_metrics.values())
            or phase_regressions
        ):
            scenario_status = "regressed"
            regression_count += 1
        elif wall_time["status"] == "improved" or any(
            metric["status"] == "improved" for metric in timing_metrics.values()
        ):
            scenario_status = "improved"

        scenarios.append(
            {
                "scenario_id": scenario_id,
                "description": candidate_summary.get("description", ""),
                "status": scenario_status,
                "baseline_summary_path": display_path(baseline_run.summary_path),
                "candidate_summary_path": display_path(candidate_run.summary_path),
                "wall_time": wall_time,
                "timing_metrics": timing_metrics,
                "phase_regressions": phase_regressions[:5],
            }
        )

    return {
        "baseline_dir": display_path(baseline_dir),
        "candidate_dir": display_path(candidate_dir),
        "thresholds": {
            "scenario_abs_seconds": scenario_abs_seconds,
            "scenario_pct": scenario_pct,
            "timing_abs_ms": timing_abs_ms,
            "timing_pct": timing_pct,
            "phase_abs_ms": phase_abs_ms,
            "phase_pct": phase_pct,
        },
        "baseline_only_scenarios": baseline_only,
        "candidate_only_scenarios": candidate_only,
        "regression_count": regression_count,
        "scenarios": scenarios,
    }


def render_markdown(report: dict[str, Any]) -> str:
    lines = [
        "# Perf Regression Compare",
        "",
        f"Baseline: `{report['baseline_dir']}`",
        f"Candidate: `{report['candidate_dir']}`",
        "",
        "| Scenario | Status | Wall delta | Timing regressions | Phase regressions |",
        "| --- | --- | ---: | ---: | ---: |",
    ]

    for scenario in report["scenarios"]:
        wall = scenario["wall_time"]
        delta_pct = wall["delta_pct"]
        delta_text = "n/a" if delta_pct is None else f"{delta_pct:+.1f}%"
        timing_regressions = sum(
            1 for metric in scenario["timing_metrics"].values() if metric["status"] == "regressed"
        )
        lines.append(
            "| {scenario_id} | {status} | {delta} | {timing_regressions} | {phase_regressions} |".format(
                scenario_id=scenario["scenario_id"],
                status=scenario["status"],
                delta=delta_text,
                timing_regressions=timing_regressions,
                phase_regressions=len(scenario["phase_regressions"]),
            )
        )
        if scenario["status"] == "regressed":
            lines.append(
                "  Baseline: `{}` Candidate: `{}`".format(
                    scenario["baseline_summary_path"],
                    scenario["candidate_summary_path"],
                )
            )
            for metric_name, metric in scenario["timing_metrics"].items():
                if metric["status"] != "regressed":
                    continue
                lines.append(
                    f"  Timing metric `{metric_name}`: {metric['baseline']:.1f}ms -> {metric['candidate']:.1f}ms ({metric['delta_pct']:+.1f}%)"
                )
            for phase in scenario["phase_regressions"]:
                lines.append(
                    f"  Phase `{phase['phase_path']}`: {phase['baseline']:.1f}ms -> {phase['candidate']:.1f}ms ({phase['delta_pct']:+.1f}%)"
                )

    if report["baseline_only_scenarios"]:
        lines.extend(["", f"Baseline-only scenarios: {', '.join(report['baseline_only_scenarios'])}"])
    if report["candidate_only_scenarios"]:
        lines.extend(["", f"Candidate-only scenarios: {', '.join(report['candidate_only_scenarios'])}"])
    if not report["scenarios"]:
        lines.extend(["", "No common scenarios were found."])
    return "\n".join(lines) + "\n"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-dir", required=True, help="Baseline perf result directory")
    parser.add_argument("--candidate-dir", required=True, help="Candidate perf result directory")
    parser.add_argument(
        "--scenario-abs-seconds",
        type=float,
        default=0.25,
        help="Minimum absolute wall-time increase to count as a regression",
    )
    parser.add_argument(
        "--scenario-pct",
        type=float,
        default=10.0,
        help="Minimum wall-time percentage increase to count as a regression",
    )
    parser.add_argument(
        "--timing-abs-ms",
        type=float,
        default=25.0,
        help="Minimum timing metric increase in milliseconds to count as a regression",
    )
    parser.add_argument(
        "--timing-pct",
        type=float,
        default=10.0,
        help="Minimum timing metric percentage increase to count as a regression",
    )
    parser.add_argument(
        "--phase-abs-ms",
        type=float,
        default=50.0,
        help="Minimum phase median increase in milliseconds to count as a regression",
    )
    parser.add_argument(
        "--phase-pct",
        type=float,
        default=10.0,
        help="Minimum phase median percentage increase to count as a regression",
    )
    parser.add_argument("--markdown-out", help="Markdown output path")
    parser.add_argument("--json-out", help="JSON output path")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    baseline_dir = Path(args.baseline_dir)
    candidate_dir = Path(args.candidate_dir)

    report = build_report(
        baseline_dir,
        candidate_dir,
        args.scenario_abs_seconds,
        args.scenario_pct,
        args.timing_abs_ms,
        args.timing_pct,
        args.phase_abs_ms,
        args.phase_pct,
    )
    markdown = render_markdown(report)

    if args.markdown_out:
        markdown_out = Path(args.markdown_out)
        markdown_out.parent.mkdir(parents=True, exist_ok=True)
        markdown_out.write_text(markdown, encoding="utf-8")
        print(f"wrote {display_path(markdown_out)}")
    else:
        print(markdown, end="")

    if args.json_out:
        json_out = Path(args.json_out)
        json_out.parent.mkdir(parents=True, exist_ok=True)
        json_out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {display_path(json_out)}")

    return 1 if report["regression_count"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
