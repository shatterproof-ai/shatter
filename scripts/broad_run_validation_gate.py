#!/usr/bin/env python3
"""Broad-run validation corpus gate (str-jeen.14).

Runs `shatter scan` over each fixture declared in the corpus manifest and
asserts the JSON report against the expected thresholds. Exits non-zero on
any regression.

The gate intentionally does NOT modify scan internals, reporting code, or
protocol — it is a black-box check over JSON output. Per the str-jeen.14
plan, thresholds capture *current* (possibly buggy) behavior so the gate
flags regressions; they tighten as parent-epic bugs land (see the
`tighten_when:` map per fixture and docs/validation/broad-run-corpus.md).

Usage:
    scripts/broad_run_validation_gate.py [-v] [--corpus PATH]
                                         [--shatter-bin PATH]
                                         [--filter ID]
                                         [--list]

The script is purposely standalone (stdlib + PyYAML only — same baseline
as scripts/validate-parity.py). PyYAML is already a precondition for
several existing Taskfile targets.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import yaml

# Default exit codes.
EXIT_OK = 0
EXIT_REGRESSION = 1
EXIT_BAD_INVOCATION = 2

# Default scan timeouts; deliberately tight so a hung fixture does not stall
# the whole gate. Override via environment if a slower host needs it.
DEFAULT_SCAN_TIMEOUT_SECONDS = int(
    os.environ.get("SHATTER_BROAD_RUN_GATE_SCAN_TIMEOUT", "120")
)

# Stripped-PATH for the rust-unavailable fixture: keeps common system bins
# (so the TS frontend's `node` still resolves) but excludes any directory
# that might house a `shatter-rust` binary. The shatter-cli unit test
# (`rust_frontend_availability_test.rs`) strips PATH entirely because it
# runs an only-Rust scenario and never spawns node; the corpus needs the
# *mixed* path where TS still runs, so we keep `/usr/bin:/bin`.
RUST_STRIPPED_PATH = "/usr/bin:/bin"

REPO_ROOT = Path(__file__).resolve().parents[1]


@dataclass
class FixtureResult:
    """Outcome of running and asserting one fixture."""

    fixture_id: str
    passed: bool
    failures: list[str] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)
    duration_seconds: float = 0.0


# ---------- Pure threshold helpers (covered by unit tests) ----------


def compare_min(actual: int, minimum: int | None, label: str) -> str | None:
    """Return None on pass, an error string on fail."""
    if minimum is None:
        return None
    if actual < minimum:
        return f"{label}: actual {actual} < min {minimum}"
    return None


def compare_max(actual: int, maximum: int | None, label: str) -> str | None:
    if maximum is None:
        return None
    if actual > maximum:
        return f"{label}: actual {actual} > max {maximum}"
    return None


def compare_range(
    actual: int,
    minimum: int | None,
    maximum: int | None,
    label: str,
) -> list[str]:
    """Return a list of error strings (empty list on pass)."""
    errors: list[str] = []
    error_min = compare_min(actual, minimum, label)
    if error_min is not None:
        errors.append(error_min)
    error_max = compare_max(actual, maximum, label)
    if error_max is not None:
        errors.append(error_max)
    return errors


def collect_referenced_paths(report: dict[str, Any]) -> list[str]:
    """Walk the JSON report and return every absolute path that appears as
    a string value. Used by the dangling-artifact check."""
    paths: list[str] = []

    def walk(node: Any) -> None:
        if isinstance(node, dict):
            for value in node.values():
                walk(value)
            return
        if isinstance(node, list):
            for value in node:
                walk(value)
            return
        if isinstance(node, str) and node.startswith("/") and len(node) < 4096:
            # Heuristic: report fields like file_path are absolute. Exclude
            # purely textual strings (e.g. log lines containing "/foo bar"),
            # and the qualified function-id form `path::symbol` which is not
            # a filesystem path even though it starts with `/`.
            if " " not in node and "\n" not in node and "::" not in node:
                paths.append(node)

    walk(report)
    return paths


def unresolved_paths(paths: list[str]) -> list[str]:
    """Return the subset that does not resolve on disk."""
    return [path for path in paths if not Path(path).exists()]


def find_no_target_reasons(report: dict[str, Any]) -> dict[str, str]:
    """Best-effort extraction of `(file_basename -> no_target_reason)` from a
    scan JSON report. The schema for skipped/no-target entries varies across
    versions (str-jeen.21–.25 in flight); the gate accepts either:

      codebase.skipped_functions[].file_path + .no_target_reason
      codebase.no_target_files[].file_path + .reason
      no_target_reasons[].file + .reason  (top-level summary)

    Whichever is present is read; missing data is silently empty so the
    gate can still PASS for fixtures whose expected list is empty.
    """
    out: dict[str, str] = {}
    candidate_lists: list[tuple[Any, str, str]] = []
    codebase = report.get("codebase") or {}
    if isinstance(codebase, dict):
        candidate_lists.append(
            (codebase.get("skipped_functions"), "file_path", "no_target_reason")
        )
        candidate_lists.append(
            (codebase.get("no_target_files"), "file_path", "reason")
        )
    candidate_lists.append((report.get("no_target_reasons"), "file", "reason"))

    for entries, file_field, reason_field in candidate_lists:
        if not isinstance(entries, list):
            continue
        for entry in entries:
            if not isinstance(entry, dict):
                continue
            file_value = entry.get(file_field)
            reason_value = entry.get(reason_field)
            if not isinstance(file_value, str) or not isinstance(reason_value, str):
                continue
            out[Path(file_value).name] = reason_value
    return out


# ---------- IO + scan invocation ----------


def find_shatter_bin(explicit: str | None) -> Path:
    """Locate the `shatter` binary. Prefer an explicit path, then env var,
    then the workspace target dirs."""
    if explicit is not None:
        path = Path(explicit).resolve()
        if not path.exists():
            raise FileNotFoundError(f"--shatter-bin path does not exist: {path}")
        return path

    env_path = os.environ.get("SHATTER_BIN")
    if env_path:
        path = Path(env_path).resolve()
        if not path.exists():
            raise FileNotFoundError(f"SHATTER_BIN does not exist: {path}")
        return path

    candidates = [
        REPO_ROOT / "target" / "debug" / "shatter",
        REPO_ROOT / "target" / "release" / "shatter",
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate
    raise FileNotFoundError(
        "shatter binary not found. Build with `cargo build -p shatter-cli` "
        "or pass --shatter-bin / set SHATTER_BIN."
    )


def ensure_node_modules(fixture_path: Path) -> Path | None:
    """If a TS fixture declares package.json but has no node_modules dir,
    create an empty one so preflight finds it. Return the created path
    (so the caller can remove it) or None if no action was taken.

    Avoids tracking an empty `node_modules/` in git (the repo's root
    .gitignore excludes the name) while still letting the TS preflight
    succeed for fixtures with no runtime deps.
    """
    pkg = fixture_path / "package.json"
    nm = fixture_path / "node_modules"
    if pkg.exists() and not nm.exists():
        nm.mkdir()
        # Marker so a stray rmtree never deletes a real install.
        (nm / ".broad-run-corpus-stub").write_text("stub\n", encoding="utf-8")
        return nm
    return None


def run_scan(
    shatter_bin: Path,
    fixture_path: Path,
    extra_args: list[str],
    *,
    strip_path: bool = False,
    cwd: Path | None = None,
    timeout_seconds: int = DEFAULT_SCAN_TIMEOUT_SECONDS,
) -> tuple[int, dict[str, Any] | None, str]:
    """Invoke `shatter scan <fixture_path> -o <tmp.json>` and return
    (exit_code, parsed_json_or_none, stderr_tail)."""
    with tempfile.TemporaryDirectory(prefix="broad-run-gate-") as tmp:
        out_path = Path(tmp) / "report.json"
        cmd = [
            str(shatter_bin),
            "scan",
            str(fixture_path),
            "-o",
            str(out_path),
            "--log-level",
            "warn",
        ] + extra_args
        env = dict(os.environ)
        if strip_path:
            env["PATH"] = RUST_STRIPPED_PATH
        run_cwd = cwd if cwd is not None else fixture_path
        try:
            completed = subprocess.run(
                cmd,
                cwd=str(run_cwd),
                env=env,
                capture_output=True,
                text=True,
                timeout=timeout_seconds,
            )
        except subprocess.TimeoutExpired:
            return (124, None, f"timeout after {timeout_seconds}s")
        report: dict[str, Any] | None = None
        if out_path.exists():
            try:
                report = json.loads(out_path.read_text(encoding="utf-8"))
            except json.JSONDecodeError as exc:
                return (
                    completed.returncode,
                    None,
                    f"json decode error: {exc}; stderr_tail={_tail(completed.stderr)}",
                )
        return (completed.returncode, report, _tail(completed.stderr))


def _tail(text: str, max_chars: int = 800) -> str:
    if text is None:
        return ""
    text = text.strip()
    if len(text) <= max_chars:
        return text
    return "...\n" + text[-max_chars:]


# ---------- Fixture assertions ----------


def assert_fixture(
    fixture: dict[str, Any],
    report: dict[str, Any] | None,
    return_code: int,
    stderr_tail: str,
) -> list[str]:
    """Apply expected thresholds to (return_code, report). Return list of
    failure messages (empty list on pass)."""
    failures: list[str] = []
    expected = fixture.get("expected") or {}

    if expected.get("run_must_succeed", True):
        if return_code != 0:
            failures.append(
                f"run_must_succeed: shatter exited {return_code}; stderr_tail={stderr_tail}"
            )

    # Some scans legitimately produce no JSON report (exit 0, "No functions
    # found to scan."). Treat the absence as a failure only when an
    # assertion in this fixture's `expected` actually needs the report.
    report_dependent_keys = {
        "min_total_discovered_functions", "max_total_discovered_functions",
        "min_completed_functions", "max_completed_functions",
        "min_failed_functions", "max_failed_functions",
        "min_skipped_functions", "max_skipped_functions",
        "min_unsupported_functions", "max_unsupported_functions",
        "no_target_reasons",
    }
    if report is None:
        needs_report = bool(report_dependent_keys & set(expected.keys()))
        if expected.get("artifact_paths_must_resolve", True) and not needs_report:
            # `artifact_paths_must_resolve` is vacuously true when the
            # report doesn't exist — there are no paths to validate.
            return failures
        if needs_report:
            failures.append("no JSON report produced; remaining checks skipped")
        return failures

    codebase = report.get("codebase")
    if isinstance(codebase, dict):
        thresholds = [
            ("min_total_discovered_functions", "max_total_discovered_functions",
             "total_discovered_functions"),
            ("min_completed_functions", "max_completed_functions",
             "completed_functions"),
            ("min_failed_functions", "max_failed_functions", "failed_functions"),
            ("min_skipped_functions", "max_skipped_functions",
             "skipped_functions_count"),
            ("min_unsupported_functions", "max_unsupported_functions",
             "unsupported_functions"),
        ]
        for min_key, max_key, field_name in thresholds:
            actual = codebase.get(field_name)
            if not isinstance(actual, int):
                if min_key in expected or max_key in expected:
                    failures.append(
                        f"{field_name}: missing or non-integer in report"
                    )
                continue
            failures.extend(
                compare_range(
                    actual,
                    expected.get(min_key),
                    expected.get(max_key),
                    field_name,
                )
            )

    expected_reasons = expected.get("no_target_reasons") or []
    if expected_reasons:
        actual_reasons = find_no_target_reasons(report)
        for entry in expected_reasons:
            file_name = entry.get("file")
            single = entry.get("reason")
            choices = entry.get("reason_one_of")
            actual = actual_reasons.get(file_name)
            if actual is None:
                failures.append(
                    f"no_target_reason for {file_name}: missing from report"
                )
                continue
            if single is not None and actual != single:
                failures.append(
                    f"no_target_reason for {file_name}: got '{actual}', expected '{single}'"
                )
            elif choices is not None and actual not in choices:
                failures.append(
                    f"no_target_reason for {file_name}: got '{actual}', expected one of {choices}"
                )

    if expected.get("artifact_paths_must_resolve", True):
        referenced = collect_referenced_paths(report)
        # Filter to paths that look like fixture artifacts (under the worktree
        # root or a canonical artifact tree). We don't want every absolute
        # string everywhere to gate the run.
        candidates = [
            p for p in referenced
            if p.startswith(str(REPO_ROOT)) or "/.shatter" in p or "/shatter-artifacts" in p
        ]
        unresolved = unresolved_paths(candidates)
        if unresolved:
            sample = unresolved[:5]
            failures.append(
                f"artifact_paths_must_resolve: {len(unresolved)} dangling path(s); "
                f"first {len(sample)}: {sample}"
            )

    return failures


# ---------- Source-churn driver ----------


def run_source_churn(
    fixture: dict[str, Any], shatter_bin: Path
) -> list[str]:
    """Two-phase orchestration: copy initial/, scan; copy added-file/, scan
    again; compare reports. No sleeps."""
    failures: list[str] = []
    fixture_path = REPO_ROOT / fixture["path"]
    initial_dir = fixture_path / "initial"
    added_file_dir = fixture_path / "added-file"
    expected_churn = (fixture.get("expected") or {}).get("churn") or {}
    extra_args = fixture.get("args") or []

    with tempfile.TemporaryDirectory(prefix="broad-run-churn-") as tmp:
        workdir = Path(tmp) / "work"
        shutil.copytree(initial_dir, workdir)

        rc1, report1, stderr1 = run_scan(
            shatter_bin, workdir, extra_args, cwd=workdir
        )
        if rc1 != 0:
            failures.append(
                f"phase1: shatter exited {rc1}; stderr_tail={stderr1}"
            )
            return failures
        if report1 is None:
            failures.append("phase1: no JSON report produced")
            return failures

        # Mutate: copy each file from added-file/ into workdir.
        for added in added_file_dir.iterdir():
            if added.is_file():
                shutil.copy2(added, workdir / added.name)

        rc2, report2, stderr2 = run_scan(
            shatter_bin, workdir, extra_args, cwd=workdir
        )
        if rc2 != 0:
            failures.append(
                f"phase2: shatter exited {rc2}; stderr_tail={stderr2}"
            )
            return failures
        if report2 is None:
            failures.append("phase2: no JSON report produced")
            return failures

        functions1 = _function_keys(report1)
        functions2 = _function_keys(report2)

        min1 = expected_churn.get("phase1_min_functions")
        if isinstance(min1, int) and len(functions1) < min1:
            failures.append(
                f"phase1_min_functions: got {len(functions1)} < {min1}"
            )
        min2 = expected_churn.get("phase2_min_functions")
        if isinstance(min2, int) and len(functions2) < min2:
            failures.append(
                f"phase2_min_functions: got {len(functions2)} < {min2}"
            )
        if expected_churn.get("phase2_must_be_superset", False):
            missing = functions1 - functions2
            if missing:
                failures.append(
                    "phase2_must_be_superset: phase1 functions absent from phase2: "
                    f"{sorted(missing)}"
                )
            new_functions = functions2 - functions1
            if not new_functions:
                failures.append(
                    "phase2_must_be_superset: phase2 added no new functions despite "
                    "added-file/ being copied in"
                )

    return failures


def _function_keys(report: dict[str, Any]) -> set[str]:
    """Stable identifier set for the functions array in a scan report."""
    out: set[str] = set()
    functions = report.get("functions")
    if not isinstance(functions, list):
        return out
    for entry in functions:
        if not isinstance(entry, dict):
            continue
        name = entry.get("function_name")
        path = entry.get("file_path")
        if isinstance(name, str) and isinstance(path, str):
            out.add(f"{path}::{name}")
        elif isinstance(name, str):
            out.add(name)
    return out


# ---------- Driver ----------


def load_manifest(corpus_path: Path) -> dict[str, Any]:
    raw = corpus_path.read_text(encoding="utf-8")
    parsed = yaml.safe_load(raw)
    if not isinstance(parsed, dict):
        raise ValueError(f"corpus manifest is not a mapping: {corpus_path}")
    if "fixtures" not in parsed or not isinstance(parsed["fixtures"], list):
        raise ValueError("corpus manifest missing 'fixtures:' list")
    return parsed


def run_one(
    fixture: dict[str, Any],
    shatter_bin: Path,
    *,
    verbose: bool,
) -> FixtureResult:
    import time

    fixture_id = fixture["id"]
    started = time.monotonic()
    command = fixture.get("command", "scan")
    extra_args = fixture.get("args") or []
    fixture_path = REPO_ROOT / fixture["path"]

    if verbose:
        print(f"  -> {fixture_id} ({command}) {fixture_path}", flush=True)

    if command == "source_churn":
        failures = run_source_churn(fixture, shatter_bin)
        return FixtureResult(
            fixture_id=fixture_id,
            passed=len(failures) == 0,
            failures=failures,
            duration_seconds=time.monotonic() - started,
        )

    if command not in ("scan", "scan_with_path_stripped"):
        return FixtureResult(
            fixture_id=fixture_id,
            passed=False,
            failures=[f"unknown command '{command}'"],
            duration_seconds=time.monotonic() - started,
        )

    if not fixture_path.exists():
        return FixtureResult(
            fixture_id=fixture_id,
            passed=False,
            failures=[f"fixture path does not exist: {fixture_path}"],
            duration_seconds=time.monotonic() - started,
        )

    strip = command == "scan_with_path_stripped"
    cwd = None
    if strip:
        # cwd must be outside the worktree so the `./shatter-rust/...`
        # local-build candidates miss too. Use a fresh tempdir each call.
        cwd_holder = tempfile.mkdtemp(prefix="broad-run-strip-")
        cwd = Path(cwd_holder)

    created_nm = ensure_node_modules(fixture_path)
    try:
        rc, report, stderr_tail = run_scan(
            shatter_bin,
            fixture_path,
            extra_args,
            strip_path=strip,
            cwd=cwd,
        )
        failures = assert_fixture(fixture, report, rc, stderr_tail)
    finally:
        if cwd is not None:
            shutil.rmtree(cwd, ignore_errors=True)
        if created_nm is not None and (created_nm / ".broad-run-corpus-stub").exists():
            shutil.rmtree(created_nm, ignore_errors=True)

    return FixtureResult(
        fixture_id=fixture_id,
        passed=len(failures) == 0,
        failures=failures,
        duration_seconds=time.monotonic() - started,
    )


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="broad_run_validation_gate",
        description=(
            "Run the broad-run validation corpus and check the JSON output "
            "against the per-fixture thresholds in the manifest. Exits 0 on "
            "PASS, 1 on regression, 2 on bad invocation."
        ),
    )
    parser.add_argument(
        "--corpus",
        type=Path,
        default=REPO_ROOT / "tests/broad-run-corpus/manifest.yaml",
        help="path to the corpus manifest (default: tests/broad-run-corpus/manifest.yaml)",
    )
    parser.add_argument(
        "--shatter-bin",
        type=str,
        default=None,
        help="path to the shatter binary (default: target/debug/shatter or SHATTER_BIN)",
    )
    parser.add_argument(
        "--filter",
        type=str,
        default=None,
        help="run only fixtures whose id contains this substring",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="list fixture ids and exit without running",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="emit per-fixture progress",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    try:
        manifest = load_manifest(args.corpus)
    except (FileNotFoundError, ValueError, yaml.YAMLError) as exc:
        print(f"[ERROR] {exc}", file=sys.stderr)
        return EXIT_BAD_INVOCATION

    fixtures = manifest["fixtures"]
    if args.filter:
        fixtures = [f for f in fixtures if args.filter in f.get("id", "")]
        if not fixtures:
            print(f"[ERROR] --filter matched no fixtures: {args.filter}", file=sys.stderr)
            return EXIT_BAD_INVOCATION

    if args.list:
        for fixture in fixtures:
            print(fixture["id"])
        return EXIT_OK

    try:
        shatter_bin = find_shatter_bin(args.shatter_bin)
    except FileNotFoundError as exc:
        print(f"[ERROR] {exc}", file=sys.stderr)
        return EXIT_BAD_INVOCATION

    if args.verbose:
        print(f"[gate] corpus={args.corpus} shatter={shatter_bin}", flush=True)
        print(f"[gate] running {len(fixtures)} fixture(s)", flush=True)

    results = [run_one(f, shatter_bin, verbose=args.verbose) for f in fixtures]

    failed = [r for r in results if not r.passed]
    print()
    print("== Broad-run validation corpus gate ==")
    for result in results:
        marker = "PASS" if result.passed else "FAIL"
        print(f"  [{marker}] {result.fixture_id} ({result.duration_seconds:.1f}s)")
        for failure in result.failures:
            print(f"        - {failure}")
    print(f"\n{len(results) - len(failed)} pass / {len(failed)} fail")

    return EXIT_OK if not failed else EXIT_REGRESSION


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
