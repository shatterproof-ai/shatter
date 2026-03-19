#!/usr/bin/env python3
"""Cross-mode isolation report: compare wall time and key phases across none/function/serial."""

from __future__ import annotations

import argparse
import json
import statistics
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parent.parent
ISOLATION_MODES = ("none", "function", "serial")


def display_path(path: Path) -> str:
    try:
        return str(path.relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


def strip_isolation_suffix(scenario_id: str) -> str:
    """Return the base scenario id with the isolation-mode suffix removed."""
    for mode in ISOLATION_MODES:
        suffix = f"-{mode}"
        if scenario_id.endswith(suffix):
            return scenario_id[: -len(suffix)]
    return scenario_id


def load_latest_summaries(results_dir: Path) -> dict[str, dict[str, Any]]:
    """Load the most recent summary.json for each scenario id."""
    grouped: dict[str, list[tuple[str, dict[str, Any]]]] = {}
    if not results_dir.exists():
        return {}
    for summary_path in sorted(results_dir.glob("*/*/summary.json")):
        timestamp = summary_path.parent.name
        scenario_id = summary_path.parent.parent.name
        summary = json.loads(summary_path.read_text(encoding="utf-8"))
        grouped.setdefault(scenario_id, []).append((timestamp, summary))

    latest: dict[str, dict[str, Any]] = {}
    for scenario_id, entries in grouped.items():
        entries.sort(key=lambda pair: pair[0])
        latest[scenario_id] = entries[-1][1]
    return latest


def _overhead_pct(baseline: float | None, candidate: float | None) -> float | None:
    if baseline is None or candidate is None or baseline == 0.0:
        return None
    return (candidate - baseline) / baseline * 100.0


def _fmt_ms(value: float | None, *, width: int = 8) -> str:
    if value is None:
        return "n/a".rjust(width)
    return f"{value:.1f}ms".rjust(width)


def _fmt_pct(value: float | None, *, width: int = 8) -> str:
    if value is None:
        return "n/a".rjust(width)
    sign = "+" if value >= 0 else ""
    return f"{sign}{value:.1f}%".rjust(width)


def _median_ms(summary: dict[str, Any], key: str) -> float | None:
    """Extract a timing metric median (in ms) from a scenario summary."""
    timing = summary.get("timing", {})
    metrics_ms = timing.get("metrics_ms", {})
    entry = metrics_ms.get(key)
    if entry:
        return entry.get("median")
    return None


def _key_phase_median(summary: dict[str, Any], phase_label: str) -> float | None:
    """Return the median of a named key phase across timing runs, or None."""
    timing = summary.get("timing", {})
    # key_phases_ms is written per timing run; summarize_timing_runs doesn't
    # aggregate it yet, so we read from phases instead.
    phases = timing.get("phases", [])
    # perf_runner.py records the key_phases_ms labels as phase_path entries
    # inside each run. The aggregated phases list has median_ms per phase_path.
    # Map label → prefix from the runner's _KEY_PHASE_PREFIXES convention:
    label_to_prefix = {
        "handshake_ms": "frontend.remote.handshake",
        "setup_ms": "frontend.remote.setup",
        "module_load_ms": "frontend.remote.setup.module_load",
        "execute_ms": "frontend.remote.execute",
        "invoke_ms": "frontend.remote.execute.invoke_function",
        "await_ms": "frontend.remote.execute.await_result",
        "shrink_refine_ms": "solver.shrink",
    }
    prefix = label_to_prefix.get(phase_label)
    if prefix is None:
        return None
    total = sum(
        float(p.get("median_ms", 0.0))
        for p in phases
        if p["phase_path"].startswith(prefix)
    )
    return total if total > 0.0 else None


def build_report(results_dir: Path) -> dict[str, Any]:
    summaries = load_latest_summaries(results_dir)

    # Group scenario ids by base name (prefix without -none/-function/-serial).
    base_ids: dict[str, dict[str, dict[str, Any]]] = {}
    for scenario_id, summary in summaries.items():
        base = strip_isolation_suffix(scenario_id)
        mode = summary.get("isolation_mode")
        if mode is None:
            # Only include scenarios that have isolation_mode metadata.
            continue
        base_ids.setdefault(base, {})[mode] = summary

    groups: list[dict[str, Any]] = []
    for base_id in sorted(base_ids.keys()):
        by_mode = base_ids[base_id]

        wall: dict[str, float | None] = {}
        for mode in ISOLATION_MODES:
            s = by_mode.get(mode)
            wall[mode] = float(s["median_seconds"]) * 1000.0 if s else None

        none_wall = wall.get("none")

        # Key phases: build per-mode dict of label → median_ms
        key_phases: dict[str, dict[str, float | None]] = {}
        phase_labels = [
            "setup_ms",
            "module_load_ms",
            "execute_ms",
            "shrink_refine_ms",
        ]
        for label in phase_labels:
            by_mode_values: dict[str, float | None] = {}
            for mode in ISOLATION_MODES:
                s = by_mode.get(mode)
                by_mode_values[mode] = _key_phase_median(s, label) if s else None
            if any(v is not None for v in by_mode_values.values()):
                key_phases[label] = by_mode_values

        groups.append(
            {
                "base_id": base_id,
                "description": (by_mode.get("none") or next(iter(by_mode.values()))).get(
                    "description", ""
                ),
                "wall_ms": wall,
                "fn_overhead_pct": _overhead_pct(none_wall, wall.get("function")),
                "serial_overhead_pct": _overhead_pct(none_wall, wall.get("serial")),
                "key_phases": key_phases,
                "modes_present": sorted(by_mode.keys()),
            }
        )

    return {
        "results_dir": display_path(results_dir),
        "groups": groups,
    }


def render_markdown(report: dict[str, Any]) -> str:
    lines = [
        "# Isolation Mode Performance Comparison",
        "",
        f"Results directory: `{report['results_dir']}`",
        "",
        "Wall time by isolation mode (median ms across iterations):",
        "",
        "| Scenario | none | function | serial | fn overhead | serial overhead |",
        "| --- | ---: | ---: | ---: | ---: | ---: |",
    ]

    for group in report["groups"]:
        wall = group["wall_ms"]
        lines.append(
            "| {base_id} | {none} | {fn} | {ser} | {fn_pct} | {ser_pct} |".format(
                base_id=group["base_id"],
                none=_fmt_ms(wall.get("none"), width=1),
                fn=_fmt_ms(wall.get("function"), width=1),
                ser=_fmt_ms(wall.get("serial"), width=1),
                fn_pct=_fmt_pct(group["fn_overhead_pct"], width=1),
                ser_pct=_fmt_pct(group["serial_overhead_pct"], width=1),
            )
        )

    if report["groups"]:
        lines += [
            "",
            "## Key Cost Centers",
            "",
            "Phase timing by isolation mode. `n/a` = phase not present in timing data.",
            "",
        ]
        _PHASE_LABEL_DISPLAY = {
            "setup_ms": "setup (handshake)",
            "module_load_ms": "module_load",
            "execute_ms": "execute (total)",
            "shrink_refine_ms": "shrink/refine",
        }
        for group in report["groups"]:
            if not group["key_phases"]:
                continue
            lines.append(f"### {group['base_id']}")
            lines.append("")
            lines.append("| Phase | none | function | serial |")
            lines.append("| --- | ---: | ---: | ---: |")
            for label, by_mode in group["key_phases"].items():
                display = _PHASE_LABEL_DISPLAY.get(label, label)
                lines.append(
                    "| {label} | {none} | {fn} | {ser} |".format(
                        label=display,
                        none=_fmt_ms(by_mode.get("none"), width=1),
                        fn=_fmt_ms(by_mode.get("function"), width=1),
                        ser=_fmt_ms(by_mode.get("serial"), width=1),
                    )
                )
            lines.append("")

    if not report["groups"]:
        lines += [
            "",
            "No isolation-mode scenarios found in the results directory.",
            "Run `npx task perf-isolation` first to collect data.",
        ]

    return "\n".join(lines) + "\n"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--results-dir",
        required=True,
        help="Directory written by perf_runner.py (e.g. .shatter/perf-runs/isolation)",
    )
    parser.add_argument("--markdown-out", help="Markdown output path")
    parser.add_argument("--json-out", help="JSON output path")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    results_dir = Path(args.results_dir)
    report = build_report(results_dir)
    markdown = render_markdown(report)

    if args.markdown_out:
        out = Path(args.markdown_out)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(markdown, encoding="utf-8")
        print(f"wrote {display_path(out)}")
    else:
        print(markdown, end="")

    if args.json_out:
        out = Path(args.json_out)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {display_path(out)}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
