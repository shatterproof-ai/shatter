#!/usr/bin/env python3
"""Run external profiling scenarios and capture repeatable timing artifacts."""

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
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parent.parent
SCENARIO_FILE = REPO_ROOT / "perf" / "scenarios.json"
DEFAULT_RESULTS_DIR = REPO_ROOT / "perf" / "results"
PERF_STAT_EVENTS = [
    "task-clock",
    "cycles",
    "instructions",
    "branches",
    "branch-misses",
    "cache-misses",
]


@dataclass
class Scenario:
    id: str
    kind: str
    description: str
    command: list[str]
    workdir: Path
    env: dict[str, str]
    cache_mode: str
    warmups: int
    iterations: int
    timeout_seconds: int
    profilers: list[str]


def load_scenarios() -> dict[str, Scenario]:
    payload = json.loads(SCENARIO_FILE.read_text())
    defaults = payload.get("defaults", {})
    scenarios: dict[str, Scenario] = {}
    for raw in payload["scenarios"]:
        merged_env = dict(defaults.get("env", {}))
        merged_env.update(raw.get("env", {}))
        scenario = Scenario(
            id=raw["id"],
            kind=raw["kind"],
            description=raw["description"],
            command=list(raw["command"]),
            workdir=REPO_ROOT / raw.get("workdir", defaults.get("workdir", ".")),
            env=merged_env,
            cache_mode=raw.get("cache_mode", defaults.get("cache_mode", "cold")),
            warmups=int(raw.get("warmups", defaults.get("warmups", 0))),
            iterations=int(raw.get("iterations", defaults.get("iterations", 1))),
            timeout_seconds=int(
                raw.get("timeout_seconds", defaults.get("timeout_seconds", 300))
            ),
            profilers=list(raw.get("profilers", [])),
        )
        scenarios[scenario.id] = scenario
    return scenarios


def format_command(command: list[str]) -> str:
    return " ".join(subprocess.list2cmdline([part]) for part in command)


def make_cache_env(base_dir: Path) -> dict[str, str]:
    return {
        "SHATTER_CACHE_DIR": str(base_dir / "shatter-cache"),
        "SHATTER_SEEDS_DIR": str(base_dir / "shatter-cache" / "seeds"),
        "XDG_CACHE_HOME": str(base_dir / "xdg-cache"),
        "GOCACHE": str(base_dir / "go-cache"),
        "CARGO_TARGET_DIR": str(base_dir / "cargo-target"),
    }


def parse_perf_stat(raw_text: str) -> dict[str, Any]:
    counters: dict[str, dict[str, Any]] = {}
    for line in raw_text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        fields = stripped.split(",")
        if len(fields) < 3:
            continue
        raw_value = fields[0].strip()
        unit = fields[1].strip()
        event = fields[2].strip()
        if event not in PERF_STAT_EVENTS:
            continue
        normalized = raw_value.replace("<not counted>", "").replace("<not supported>", "")
        value: float | None
        if normalized:
            try:
                value = float(normalized)
            except ValueError:
                value = None
        else:
            value = None
        counters[event] = {
            "value": value,
            "unit": unit,
            "raw": raw_value,
        }
    return {"events": counters}


def translate_go_test_args(command: list[str]) -> tuple[Path, str, list[str]]:
    if len(command) < 3 or command[0] != "go" or command[1] != "test":
        raise SystemExit("pprof requires a go test scenario")
    package = ""
    translated: list[str] = []
    index = 2
    while index < len(command):
        token = command[index]
        if token.startswith("./") or token.startswith("../") or "/" in token:
            if package:
                raise SystemExit("pprof supports exactly one go test package per scenario")
            package = token
            index += 1
            continue
        if token == "-run" and index + 1 < len(command):
            translated.extend(["-test.run", command[index + 1]])
            index += 2
            continue
        if token.startswith("-run="):
            translated.append("-test.run=" + token.split("=", 1)[1])
            index += 1
            continue
        if token == "-count" and index + 1 < len(command):
            translated.extend(["-test.count", command[index + 1]])
            index += 2
            continue
        if token.startswith("-count="):
            translated.append("-test.count=" + token.split("=", 1)[1])
            index += 1
            continue
        raise SystemExit(f"unsupported go test argument for pprof: {token}")
    if not package:
        raise SystemExit("pprof requires a go test package path in the scenario command")
    go_workdir = REPO_ROOT
    normalized_package = package
    if package.startswith("./shatter-go/"):
        go_workdir = REPO_ROOT / "shatter-go"
        normalized_package = "./" + package.removeprefix("./shatter-go/")
    return go_workdir, normalized_package, translated


def run_once(
    scenario: Scenario,
    run_dir: Path,
    cache_dir: Path,
    mode: str,
    sequence: int,
    profiler: str | None,
) -> dict[str, Any]:
    env = os.environ.copy()
    env.update(scenario.env)
    env.update(make_cache_env(cache_dir))
    env["SHATTER_PERF_SCENARIO"] = scenario.id

    for key in (
        "SHATTER_CACHE_DIR",
        "SHATTER_SEEDS_DIR",
        "XDG_CACHE_HOME",
        "GOCACHE",
        "CARGO_TARGET_DIR",
    ):
        Path(env[key]).mkdir(parents=True, exist_ok=True)

    stdout_path = run_dir / "stdout.log"
    stderr_path = run_dir / "stderr.log"
    started_at = datetime.now(UTC)
    started = time.perf_counter()
    perf_stat_path = run_dir / "perf-stat.txt"
    perf_record_path = run_dir / "perf.data"
    pprof_binary_path = run_dir / "go-test-binary"
    pprof_profile_path = run_dir / "cpu.pprof"
    command = scenario.command
    if profiler == "perf-stat":
        command = [
            "perf",
            "stat",
            "-x,",
            "-o",
            str(perf_stat_path),
            "-e",
            ",".join(PERF_STAT_EVENTS),
            "--",
            *scenario.command,
        ]
    elif profiler == "perf-record":
        command = [
            "perf",
            "record",
            "--call-graph",
            "dwarf",
            "-o",
            str(perf_record_path),
            "--",
            *scenario.command,
        ]
    elif profiler == "pprof":
        go_workdir, package, translated_args = translate_go_test_args(scenario.command)
        build_command = ["go", "test", "-c", "-o", str(pprof_binary_path), package]
        build = subprocess.run(
            build_command,
            cwd=go_workdir,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        if build.returncode != 0:
            (run_dir / "stdout.log").write_text(build.stdout, encoding="utf-8")
            (run_dir / "stderr.log").write_text(build.stderr, encoding="utf-8")
            result = {
                "sequence": sequence,
                "mode": mode,
                "scenario_id": scenario.id,
                "command": scenario.command,
                "executed_command": build_command,
                "workdir": str(scenario.workdir.relative_to(REPO_ROOT)),
                "cache_mode": scenario.cache_mode,
                "cache_dir": str(cache_dir.relative_to(REPO_ROOT)),
                "started_at": datetime.now(UTC).isoformat(),
                "duration_seconds": 0.0,
                "exit_code": build.returncode,
                "stdout_path": str((run_dir / "stdout.log").relative_to(REPO_ROOT)),
                "stderr_path": str((run_dir / "stderr.log").relative_to(REPO_ROOT)),
                "profiler": profiler,
            }
            (run_dir / "result.json").write_text(json.dumps(result, indent=2) + "\n")
            return result
        command = [
            str(pprof_binary_path),
            *translated_args,
            f"-test.cpuprofile={pprof_profile_path}",
        ]
    with stdout_path.open("w", encoding="utf-8") as stdout_file, stderr_path.open(
        "w", encoding="utf-8"
    ) as stderr_file:
        completed = subprocess.run(
            command,
            cwd=scenario.workdir,
            env=env,
            stdout=stdout_file,
            stderr=stderr_file,
            timeout=scenario.timeout_seconds,
            check=False,
        )
    duration_seconds = time.perf_counter() - started
    result = {
        "sequence": sequence,
        "mode": mode,
        "scenario_id": scenario.id,
        "command": scenario.command,
        "executed_command": command,
        "workdir": str(scenario.workdir.relative_to(REPO_ROOT)),
        "cache_mode": scenario.cache_mode,
        "cache_dir": str(cache_dir.relative_to(REPO_ROOT)),
        "started_at": started_at.isoformat(),
        "duration_seconds": round(duration_seconds, 6),
        "exit_code": completed.returncode,
        "stdout_path": str(stdout_path.relative_to(REPO_ROOT)),
        "stderr_path": str(stderr_path.relative_to(REPO_ROOT)),
        "profiler": profiler,
    }
    if profiler == "perf-stat":
        raw_perf = perf_stat_path.read_text(encoding="utf-8") if perf_stat_path.exists() else ""
        parsed_perf = parse_perf_stat(raw_perf)
        perf_json_path = run_dir / "perf-stat.json"
        perf_json_path.write_text(json.dumps(parsed_perf, indent=2) + "\n")
        result["perf_stat_path"] = str(perf_stat_path.relative_to(REPO_ROOT))
        result["perf_stat_json_path"] = str(perf_json_path.relative_to(REPO_ROOT))
        result["perf_stat"] = parsed_perf
    elif profiler == "perf-record":
        result["perf_record_path"] = str(perf_record_path.relative_to(REPO_ROOT))
    elif profiler == "pprof":
        result["pprof_binary_path"] = str(pprof_binary_path.relative_to(REPO_ROOT))
        result["pprof_profile_path"] = str(pprof_profile_path.relative_to(REPO_ROOT))
    (run_dir / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def summarize(
    scenario: Scenario, measured: list[dict[str, Any]], profiler: str | None
) -> dict[str, Any]:
    durations = [entry["duration_seconds"] for entry in measured]
    summary = {
        "scenario_id": scenario.id,
        "description": scenario.description,
        "iterations": len(measured),
        "cache_mode": scenario.cache_mode,
        "profilers": scenario.profilers,
        "active_profiler": profiler,
        "min_seconds": round(min(durations), 6),
        "median_seconds": round(statistics.median(durations), 6),
        "max_seconds": round(max(durations), 6),
        "mean_seconds": round(statistics.fmean(durations), 6),
        "exit_codes": [entry["exit_code"] for entry in measured],
    }
    if profiler == "perf-stat":
        event_summary: dict[str, dict[str, Any]] = {}
        for event in PERF_STAT_EVENTS:
            values = [
                entry["perf_stat"]["events"][event]["value"]
                for entry in measured
                if event in entry.get("perf_stat", {}).get("events", {})
                and entry["perf_stat"]["events"][event]["value"] is not None
            ]
            if not values:
                continue
            unit = measured[0]["perf_stat"]["events"][event]["unit"]
            event_summary[event] = {
                "unit": unit,
                "min": min(values),
                "median": statistics.median(values),
                "max": max(values),
                "mean": statistics.fmean(values),
            }
        summary["perf_stat"] = event_summary
    elif profiler == "perf-record":
        summary["perf_record_paths"] = [
            entry["perf_record_path"] for entry in measured if "perf_record_path" in entry
        ]
    elif profiler == "pprof":
        summary["pprof_profiles"] = [
            {
                "binary": entry["pprof_binary_path"],
                "profile": entry["pprof_profile_path"],
            }
            for entry in measured
            if "pprof_profile_path" in entry
        ]
    return summary


def print_summary(summary: dict[str, Any], result_root: Path) -> None:
    print(
        f"{summary['scenario_id']}: median={summary['median_seconds']:.3f}s "
        f"min={summary['min_seconds']:.3f}s max={summary['max_seconds']:.3f}s "
        f"runs={summary['iterations']} results={result_root.relative_to(REPO_ROOT)}"
    )
    if summary.get("perf_stat"):
        for event in PERF_STAT_EVENTS:
            metric = summary["perf_stat"].get(event)
            if metric is None:
                continue
            unit = metric["unit"] or "count"
            print(
                f"  {event}: median={metric['median']:.3f} {unit} "
                f"mean={metric['mean']:.3f} {unit}"
            )
    if summary.get("perf_record_paths"):
        print(f"  perf-record: {summary['perf_record_paths'][0]}")
    if summary.get("pprof_profiles"):
        first = summary["pprof_profiles"][0]
        print(f"  pprof: {first['profile']} ({first['binary']})")


def select_scenarios(
    scenarios: dict[str, Scenario],
    scenario_ids: list[str] | None,
    scenario_file: str | None,
    run_all: bool,
) -> list[Scenario]:
    if run_all:
        return list(scenarios.values())
    if scenario_file:
        scenario_path = Path(scenario_file)
        scenario_ids = scenario_ids or []
        for line in scenario_path.read_text(encoding="utf-8").splitlines():
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue
            scenario_ids.append(stripped)
    if not scenario_ids:
        raise SystemExit("pass --scenario <id> or --all")
    selected = []
    for scenario_id in scenario_ids:
        if scenario_id not in scenarios:
            raise SystemExit(f"unknown scenario: {scenario_id}")
        selected.append(scenarios[scenario_id])
    return selected


def run_scenario(
    scenario: Scenario, results_dir: Path, dry_run: bool, profiler: str | None
) -> None:
    timestamp = datetime.now(UTC).strftime("%Y%m%dT%H%M%SZ")
    result_root = results_dir / scenario.id / timestamp
    result_root.mkdir(parents=True, exist_ok=True)
    cache_root = result_root / "cache"
    if scenario.cache_mode == "warm":
        cache_root.mkdir(parents=True, exist_ok=True)

    manifest = {
        "scenario_id": scenario.id,
        "kind": scenario.kind,
        "description": scenario.description,
        "command": scenario.command,
        "workdir": str(scenario.workdir.relative_to(REPO_ROOT)),
        "cache_mode": scenario.cache_mode,
        "warmups": scenario.warmups,
        "iterations": scenario.iterations,
        "timeout_seconds": scenario.timeout_seconds,
        "profilers": scenario.profilers,
    }
    (result_root / "scenario.json").write_text(json.dumps(manifest, indent=2) + "\n")

    if dry_run:
        print(f"[dry-run] {scenario.id}: {format_command(scenario.command)}")
        return

    measured: list[dict[str, Any]] = []
    total_runs = scenario.warmups + scenario.iterations
    for index in range(total_runs):
        mode = "warmup" if index < scenario.warmups else "measured"
        run_dir = result_root / f"run-{index + 1:03d}"
        run_dir.mkdir(parents=True, exist_ok=True)

        if scenario.cache_mode == "cold":
            cache_dir = Path(tempfile.mkdtemp(prefix=f"{scenario.id}-", dir=run_dir))
        else:
            cache_dir = cache_root

        result = run_once(scenario, run_dir, cache_dir, mode, index + 1, profiler)
        if scenario.cache_mode == "cold":
            shutil.rmtree(cache_dir, ignore_errors=True)

        if result["exit_code"] != 0:
            raise SystemExit(
                f"{scenario.id} failed on run {index + 1} with exit code {result['exit_code']}"
            )
        if mode == "measured":
            measured.append(result)

    summary = summarize(scenario, measured, profiler)
    summary_path = result_root / "summary.json"
    summary_path.write_text(json.dumps(summary, indent=2) + "\n")
    print_summary(summary, result_root)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    list_parser = subparsers.add_parser("list", help="List available scenarios")
    list_parser.add_argument("--json", action="store_true", help="Emit JSON")

    run_parser = subparsers.add_parser("run", help="Run one or more scenarios")
    run_parser.add_argument(
        "--scenario",
        action="append",
        help="Scenario ID to run. May be passed multiple times.",
    )
    run_parser.add_argument(
        "--scenario-file",
        help="File containing one scenario ID per line",
    )
    run_parser.add_argument("--all", action="store_true", help="Run the full corpus")
    run_parser.add_argument("--dry-run", action="store_true", help="Print commands only")
    run_parser.add_argument(
        "--profiler",
        choices=["perf-stat", "perf-record", "pprof"],
        help="External profiler wrapper to apply",
    )
    run_parser.add_argument(
        "--results-dir",
        default=str(DEFAULT_RESULTS_DIR),
        help="Directory for captured artifacts",
    )

    return parser.parse_args()


def main() -> int:
    args = parse_args()
    scenarios = load_scenarios()

    if args.command == "list":
        payload = [
            {
                "id": scenario.id,
                "kind": scenario.kind,
                "description": scenario.description,
                "cache_mode": scenario.cache_mode,
                "profilers": scenario.profilers,
            }
            for scenario in scenarios.values()
        ]
        if args.json:
            print(json.dumps(payload, indent=2))
        else:
            for scenario in payload:
                print(
                    f"{scenario['id']}: {scenario['kind']} "
                    f"[{scenario['cache_mode']}] {scenario['description']}"
                )
        return 0

    selected = select_scenarios(scenarios, args.scenario, args.scenario_file, args.all)
    if args.profiler in {"perf-stat", "perf-record"} and not args.dry_run and shutil.which("perf") is None:
        raise SystemExit("perf not found in PATH")
    if args.profiler == "pprof" and shutil.which("go") is None:
        raise SystemExit("go not found in PATH")
    results_dir = Path(args.results_dir)
    results_dir.mkdir(parents=True, exist_ok=True)
    for scenario in selected:
        if args.profiler and args.profiler not in scenario.profilers:
            raise SystemExit(
                f"scenario {scenario.id} does not allow profiler {args.profiler}"
            )
        run_scenario(scenario, results_dir, args.dry_run, args.profiler)
    return 0


if __name__ == "__main__":
    sys.exit(main())
