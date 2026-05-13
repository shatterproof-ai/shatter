#!/usr/bin/env python3
"""Broad-run validation gate.

Runs `shatter scan` against each sub-fixture under
`tests/fixtures/broad-run-corpus/` and asserts:

  1. Denominator integrity — for full_scan fixtures,
     completed + failed + skipped + unsupported == attempted, and
     attempted >= manifest's min_attempted.
  2. Artifact integrity — every file_path in failed/skipped/functions
     points to a real file on disk.
  3. Failure-class presence — for preflight_failure fixtures, stderr
     matches the pinned regex; for full_scan fixtures with
     require_failure_reason, at least one failed[].reason matches.
  4. Stale-source detection — copy a transient template, scan, delete
     it, rescan, and assert no scan artifact references the deleted
     path.

Exit code 0 on success, 1 on any assertion failure.

Usage: tests/scripts/broad_run_validation.py [-v|--verbose] [-h|--help]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
CORPUS_ROOT = REPO_ROOT / "tests" / "fixtures" / "broad-run-corpus"
MANIFEST_PATH = CORPUS_ROOT / "manifest.yaml"
SCAN_ARTIFACT_KEYS = ("failed", "skipped_functions", "functions")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Broad-run validation gate for shatter scan.",
    )
    parser.add_argument(
        "-v", "--verbose", action="store_true", help="Print scan output."
    )
    parser.add_argument(
        "--shatter",
        default=os.environ.get("SHATTER_BIN"),
        help="Path to shatter binary (default: $SHATTER_BIN or target/debug/shatter).",
    )
    return parser.parse_args()


def load_manifest() -> dict[str, Any]:
    try:
        import yaml
    except ImportError:
        sys.exit("error: pyyaml not installed (pip install pyyaml)")
    with MANIFEST_PATH.open() as fh:
        return yaml.safe_load(fh)


def resolve_shatter(explicit: str | None) -> str:
    if explicit and Path(explicit).exists():
        return explicit
    candidate = REPO_ROOT / "target" / "debug" / "shatter"
    if candidate.exists():
        return str(candidate)
    candidate = REPO_ROOT / "target" / "release" / "shatter"
    if candidate.exists():
        return str(candidate)
    sys.exit(
        "error: shatter binary not found. Build first: cargo build -p shatter-cli"
    )


def run_scan_raw(
    shatter: str,
    directory: Path,
    language: str,
    budget: dict[str, Any],
    verbose: bool,
) -> subprocess.CompletedProcess:
    cmd = [
        shatter,
        "scan",
        str(directory),
        "--language",
        language,
        "--format",
        "json",
        "--max-iterations",
        str(budget["max_iterations"]),
        "--timeout-per-fn",
        str(budget["timeout_per_fn_seconds"]),
        "--timeout-total",
        str(budget["timeout_total_seconds"]),
        "-q",
    ]
    if verbose:
        print(f"  $ {' '.join(cmd)}")
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if verbose and proc.stderr:
        for line in proc.stderr.splitlines():
            print(f"  [stderr] {line}")
    return proc


def parse_scan_json(proc: subprocess.CompletedProcess) -> dict[str, Any]:
    if not proc.stdout.strip():
        raise RuntimeError(
            f"scan produced no JSON output (exit={proc.returncode});"
            f" stderr={proc.stderr.strip()[:300]}"
        )
    return json.loads(proc.stdout)


# --- Assertions ---------------------------------------------------------


def assert_denominator(report: dict[str, Any], min_attempted: int) -> list[str]:
    errors: list[str] = []
    cb = report.get("codebase", {})
    attempted = cb.get("attempted_functions", 0)
    completed = cb.get("completed_functions", 0)
    failed = cb.get("failed_functions", 0)
    skipped = cb.get("skipped_functions_count", 0)
    unsupported = cb.get("unsupported_functions", 0)
    total_discovered = cb.get("total_discovered_functions", 0)
    # str-21w2: `skipped_functions_count` now equals the length of the
    # `skipped_functions` array (Expected + Unsupported), so it already
    # includes `unsupported_functions`. The attempt-side identity is
    # therefore `completed + failed + (skipped - unsupported) == attempted`
    # because unsupported targets are filtered before any attempt. The
    # discovery-side identity covers the full universe.
    expected_skipped = skipped - unsupported
    sum_attempted = completed + failed + expected_skipped
    if sum_attempted != attempted:
        errors.append(
            f"attempt denominator mismatch: completed({completed}) + failed({failed}) "
            f"+ expected_skipped({expected_skipped}) = {sum_attempted} "
            f"!= attempted({attempted}) "
            f"(skipped_functions_count={skipped}, unsupported={unsupported})"
        )
    sum_discovered = completed + failed + skipped
    if sum_discovered != total_discovered:
        errors.append(
            f"discovery denominator mismatch: completed({completed}) + failed({failed}) "
            f"+ skipped({skipped}) = {sum_discovered} "
            f"!= total_discovered_functions({total_discovered})"
        )
    if unsupported > skipped:
        errors.append(
            f"unsupported_functions({unsupported}) exceeds "
            f"skipped_functions_count({skipped}); they must be a sub-count"
        )
    if attempted < min_attempted:
        errors.append(
            f"attempted_functions={attempted} < min_attempted={min_attempted} "
            f"(silent skip suspected)"
        )
    if total_discovered < min_attempted:
        errors.append(
            f"total_discovered_functions={total_discovered} < "
            f"min_attempted={min_attempted}"
        )
    return errors


def collect_referenced_paths(report: dict[str, Any]) -> list[str]:
    cb = report.get("codebase", {})
    paths: list[str] = []
    for key in SCAN_ARTIFACT_KEYS:
        for entry in cb.get(key, []) or []:
            p = entry.get("file_path")
            if p:
                paths.append(p)
    for entry in report.get("functions", []) or []:
        p = entry.get("file_path")
        if p:
            paths.append(p)
    return paths


def assert_artifacts_exist(report: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    for path in collect_referenced_paths(report):
        if not Path(path).exists():
            errors.append(f"dangling artifact path: {path}")
    return errors


def assert_failure_reason_present(
    report: dict[str, Any], pattern: str
) -> list[str]:
    regex = re.compile(pattern)
    failed_entries = report.get("codebase", {}).get("failed", []) or []
    for entry in failed_entries:
        if regex.search(entry.get("reason", "")):
            return []
    reasons = [e.get("reason", "") for e in failed_entries]
    return [
        f"no failed[].reason matched /{pattern}/; reasons observed: {reasons}"
    ]


# --- Phases -------------------------------------------------------------


def run_per_fixture(
    shatter: str, manifest: dict[str, Any], verbose: bool
) -> tuple[int, int]:
    """Returns (errors, failure_class_matches)."""
    budget = manifest["global"]
    error_count = 0
    matches = 0
    for fixture in manifest["fixtures"]:
        path = CORPUS_ROOT / fixture["path"]
        mode = fixture.get("mode", "full_scan")
        print(f"[scan:{mode}] {fixture['path']} (language={fixture['language']})")
        proc = run_scan_raw(shatter, path, fixture["language"], budget, verbose)

        if mode == "preflight_failure":
            pattern = fixture.get("stderr_pattern")
            if not pattern:
                print("  [FAIL] preflight_failure fixture lacks stderr_pattern")
                error_count += 1
                continue
            if re.search(pattern, proc.stderr):
                print(f"  [ok] stderr matched /{pattern}/")
                matches += 1
            else:
                print(
                    f"  [FAIL] stderr did not match /{pattern}/; "
                    f"stderr={proc.stderr.strip()[:300]}"
                )
                error_count += 1
            continue

        # full_scan
        try:
            report = parse_scan_json(proc)
        except (RuntimeError, json.JSONDecodeError) as e:
            print(f"  [FAIL] {e}")
            error_count += 1
            continue

        errors: list[str] = []
        errors.extend(assert_denominator(report, fixture.get("min_attempted", 0)))
        errors.extend(assert_artifacts_exist(report))
        if "require_failure_reason" in fixture:
            sub_errors = assert_failure_reason_present(
                report, fixture["require_failure_reason"]
            )
            if not sub_errors:
                matches += 1
            errors.extend(sub_errors)

        if errors:
            for err in errors:
                print(f"  [FAIL] {err}")
            error_count += 1
        else:
            cb = report.get("codebase", {})
            print(
                f"  [ok] attempted={cb.get('attempted_functions')} "
                f"completed={cb.get('completed_functions')} "
                f"failed={cb.get('failed_functions')} "
                f"skipped={cb.get('skipped_functions_count')} "
                f"unsupported={cb.get('unsupported_functions')}"
            )
    return error_count, matches


def run_stale_source_phase(
    shatter: str, manifest: dict[str, Any], verbose: bool
) -> int:
    cfg = manifest.get("stale_source")
    if not cfg:
        return 0
    fixture_dir = CORPUS_ROOT / cfg["path"]
    template = fixture_dir / cfg["transient_template"]
    target = fixture_dir / cfg["transient_target"]
    budget = manifest["global"]
    print(f"[stale-source] {cfg['path']}")
    if not template.exists():
        print(f"  [FAIL] template not found: {template}")
        return 1

    errors = 0
    try:
        # Phase 1: transient file present.
        shutil.copyfile(template, target)
        proc1 = run_scan_raw(
            shatter, fixture_dir, cfg["language"], budget, verbose
        )
        try:
            report1 = parse_scan_json(proc1)
        except RuntimeError as e:
            print(f"  [FAIL] phase 1 scan: {e}")
            return 1
        paths1 = collect_referenced_paths(report1)
        if not any(str(target.resolve()) == p for p in paths1):
            print(
                f"  [warn] phase 1 report did not reference transient file "
                f"{target} — fixture may not exercise stale-source"
            )

        # Phase 2: transient file removed; rescan.
        target.unlink()
        proc2 = run_scan_raw(
            shatter, fixture_dir, cfg["language"], budget, verbose
        )
        try:
            report2 = parse_scan_json(proc2)
        except RuntimeError as e:
            print(f"  [FAIL] phase 2 scan: {e}")
            return 1
        for path in collect_referenced_paths(report2):
            if str(target.resolve()) == path:
                print(f"  [FAIL] dangling reference to deleted file: {path}")
                errors += 1
        cb2 = report2.get("codebase", {})
        if cb2.get("attempted_functions", 0) == 0:
            print(
                "  [FAIL] phase 2 attempted=0 — kept function should still "
                "be discovered after sibling removal"
            )
            errors += 1
        if errors == 0:
            print("  [ok] no dangling artifacts after source removal")
    finally:
        if target.exists():
            target.unlink()
    return errors


# --- Entry point --------------------------------------------------------


def main() -> int:
    args = parse_args()
    manifest = load_manifest()
    shatter = resolve_shatter(args.shatter)
    print(f"shatter binary: {shatter}")
    print(f"corpus root:    {CORPUS_ROOT}")
    print()

    per_fixture_errors, failure_class_matches = run_per_fixture(
        shatter, manifest, args.verbose
    )
    print()
    stale_errors = run_stale_source_phase(shatter, manifest, args.verbose)
    print()

    floor = manifest["global"].get("min_failure_class_count", 0)
    failure_floor_errors = 0
    if failure_class_matches < floor:
        print(
            f"[FAIL] failure-class matches {failure_class_matches} < "
            f"min_failure_class_count {floor}"
        )
        failure_floor_errors = 1

    total_errors = per_fixture_errors + stale_errors + failure_floor_errors
    print(
        f"summary: per-fixture errors={per_fixture_errors} "
        f"stale-source errors={stale_errors} "
        f"failure-class matches={failure_class_matches} "
        f"(floor={floor})"
    )
    if total_errors:
        print(f"FAIL: {total_errors} error(s)")
        return 1
    print("OK: broad-run validation gate passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
