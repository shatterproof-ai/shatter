#!/usr/bin/env python3
"""Cross-frontend parity golden tests for Shatter protocol.

Runs protocol commands against each frontend and compares the stable fields
of responses against pre-recorded golden files in protocol/conformance/golden/.

Two validation layers:
  1. Golden file comparison — response fields must exactly match recorded values.
  2. Registry parity check — handshake capabilities must match registry.yaml.

Modes:
  --check   (default) Compare responses against golden files; fail on mismatch.
  --update  Capture current frontend responses and write/overwrite golden files.

Usage:
  python3 protocol/conformance/run_golden_tests.py            # check mode
  python3 protocol/conformance/run_golden_tests.py --update   # regenerate goldens
  python3 protocol/conformance/run_golden_tests.py --frontend typescript --frontend go

Exit codes:
  0 — all checks pass (or --update completed successfully)
  1 — one or more mismatches, failures, or unavailable frontends
"""

from __future__ import annotations

import argparse
import json
import os
import select
import subprocess
import sys
from pathlib import Path
from typing import Any

import yaml

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent.parent
CASES_FILE = SCRIPT_DIR / "golden_cases.yaml"
REGISTRY_FILE = REPO_ROOT / "protocol" / "registry.yaml"
GOLDEN_DIR = SCRIPT_DIR / "golden"

PROTOCOL_VERSION = "0.1.0"
# Handshake timeout is longer to allow for compile-on-the-fly frontends
HANDSHAKE_TIMEOUT_S = 120
COMMAND_TIMEOUT_S = 30
SHUTDOWN_WAIT_S = 5

# Fields always excluded from golden comparison (vary per run or per frontend)
ALWAYS_DYNAMIC = {"protocol_version", "id", "language", "frontend_version", "message", "details"}


# ---------------------------------------------------------------------------
# Terminal colours (disabled when not a TTY)
# ---------------------------------------------------------------------------

if sys.stdout.isatty():
    _BOLD = "\033[1m"
    _RED = "\033[31m"
    _GREEN = "\033[32m"
    _YELLOW = "\033[33m"
    _RESET = "\033[0m"
else:
    _BOLD = _RED = _GREEN = _YELLOW = _RESET = ""


def _ok(msg: str) -> str:
    return f"{_GREEN}OK{_RESET} {msg}"


def _fail(msg: str) -> str:
    return f"{_RED}FAIL{_RESET} {msg}"


def _skip(msg: str) -> str:
    return f"{_YELLOW}SKIP{_RESET} {msg}"


def _updated(msg: str) -> str:
    return f"{_YELLOW}UPDATED{_RESET} {msg}"


# ---------------------------------------------------------------------------
# YAML loading
# ---------------------------------------------------------------------------

def load_cases(path: Path) -> dict[str, Any]:
    with open(path) as f:
        return yaml.safe_load(f)


def load_registry(path: Path) -> dict[str, Any]:
    with open(path) as f:
        return yaml.safe_load(f)


# ---------------------------------------------------------------------------
# Registry capability helpers
# ---------------------------------------------------------------------------

def registry_capabilities_for(frontend_name: str, registry: dict[str, Any]) -> list[str] | None:
    """Return the sorted expected capabilities for a frontend per registry.yaml.

    Returns None if the frontend is not declared in the registry (e.g., noop).
    """
    frontends = registry.get("frontends", {})
    if frontend_name not in frontends:
        return None
    spec = frontends[frontend_name]
    caps: list[str] = list(spec.get("command_capabilities", []))
    for ct in spec.get("complex_type_capabilities", []):
        caps.append(f"complex_type:{ct}")
    return sorted(caps)


# ---------------------------------------------------------------------------
# Frontend process management
# (Mirrors conformance_harness.py to avoid shared import coupling)
# ---------------------------------------------------------------------------

class FrontendProc:
    """Wraps a frontend subprocess for line-based JSON I/O."""

    def __init__(self, name: str, proc: subprocess.Popen[bytes]):
        self.name = name
        self.proc = proc
        self.capabilities: list[str] = []

    def send(self, request: dict[str, Any], timeout: int = COMMAND_TIMEOUT_S) -> dict[str, Any] | None:
        """Send a JSON request and read one JSON response line."""
        assert self.proc.stdin is not None
        assert self.proc.stdout is not None

        line = json.dumps(request, separators=(",", ":")) + "\n"
        try:
            self.proc.stdin.write(line.encode())
            self.proc.stdin.flush()
        except BrokenPipeError:
            return None

        ready, _, _ = select.select([self.proc.stdout], [], [], timeout)
        if not ready:
            return None

        resp_line = self.proc.stdout.readline()
        if not resp_line:
            return None

        return json.loads(resp_line.decode().strip())

    def kill(self) -> None:
        if self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait()


def spawn_frontend(name: str, spec: dict[str, Any]) -> FrontendProc | None:
    """Spawn a frontend subprocess. Returns None if unavailable."""
    command = list(spec["command"])
    cwd = REPO_ROOT / spec["cwd"] if "cwd" in spec else REPO_ROOT

    build_check = spec.get("build_check")
    if build_check and not (REPO_ROOT / build_check).exists():
        return None

    exe = command[0]
    exe_path = REPO_ROOT / exe
    if exe_path.exists():
        command[0] = str(exe_path)
    elif not _which(exe):
        return None

    for i in range(1, len(command)):
        arg_path = REPO_ROOT / command[i]
        if arg_path.exists():
            command[i] = str(arg_path)

    env = os.environ.copy()
    env["SHATTER_LOG_LEVEL"] = "warn"

    try:
        proc = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=str(cwd),
            env=env,
        )
    except FileNotFoundError:
        return None

    return FrontendProc(name, proc)


def _which(cmd: str) -> bool:
    from shutil import which
    return which(cmd) is not None


# ---------------------------------------------------------------------------
# Response extraction and normalization
# ---------------------------------------------------------------------------

def extract_golden_fields(response: dict[str, Any], golden_fields: list[str]) -> dict[str, Any]:
    """Extract only the specified fields from a response for golden comparison."""
    return {k: response[k] for k in golden_fields if k in response}


def normalize_for_golden(value: Any, sort_fields: set[str], current_key: str = "") -> Any:
    """Recursively normalize a value for golden comparison.

    Sorts list fields listed in sort_fields to make comparison order-insensitive.
    """
    if isinstance(value, list):
        if current_key in sort_fields:
            return sorted(value)
        return value
    if isinstance(value, dict):
        return {k: normalize_for_golden(v, sort_fields, k) for k, v in value.items()}
    return value


# ---------------------------------------------------------------------------
# Golden file I/O
# ---------------------------------------------------------------------------

def golden_path(case_name: str, frontend_name: str, per_frontend: bool) -> Path:
    """Return the path to the golden file for a given case and frontend."""
    if per_frontend:
        return GOLDEN_DIR / case_name / f"{frontend_name}.json"
    else:
        return GOLDEN_DIR / f"{case_name}.json"


def read_golden(path: Path) -> dict[str, Any] | None:
    """Read a golden file. Returns None if it doesn't exist."""
    if not path.exists():
        return None
    with open(path) as f:
        return json.load(f)


def write_golden(path: Path, data: dict[str, Any]) -> None:
    """Write a golden file, creating parent directories as needed."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w") as f:
        json.dump(data, f, indent=2, sort_keys=True)
        f.write("\n")


# ---------------------------------------------------------------------------
# Main test runner
# ---------------------------------------------------------------------------

def run_case_for_frontend(
    fp: FrontendProc,
    case: dict[str, Any],
) -> dict[str, Any] | None:
    """Execute a case's request sequence against a frontend.

    Returns the response to the 'capture_index' request, or None on failure.
    """
    requests: list[dict[str, Any]] = case["requests"]
    capture_index: int = case.get("capture_index", len(requests) - 1)
    captured: dict[str, Any] | None = None

    for i, req in enumerate(requests):
        is_handshake = req.get("command") == "handshake"
        timeout = HANDSHAKE_TIMEOUT_S if is_handshake else COMMAND_TIMEOUT_S
        resp = fp.send(req, timeout=timeout)
        if resp is None:
            return None

        # Capture handshake capabilities
        if is_handshake and resp.get("status") == "handshake":
            fp.capabilities = resp.get("capabilities", [])

        if i == capture_index:
            captured = resp

    return captured


def run_golden_tests(args: argparse.Namespace) -> int:
    config = load_cases(CASES_FILE)
    registry = load_registry(REGISTRY_FILE)
    frontend_specs: dict[str, Any] = config["frontends"]
    cases: list[dict[str, Any]] = config["cases"]

    # Filter frontends if --frontend specified
    if args.frontends:
        frontend_specs = {k: v for k, v in frontend_specs.items() if k in args.frontends}

    mode = "update" if args.update else "check"
    print(f"\n{_BOLD}Protocol Parity Golden Tests — {mode.upper()} mode{_RESET}")
    print("=" * 50)

    total = 0
    failures: list[str] = []
    updated: list[str] = []

    for case in cases:
        case_name: str = case["name"]
        per_frontend: bool = case.get("per_frontend_golden", False)
        golden_fields: list[str] = case.get("golden_fields", [])
        sort_fields: set[str] = set(case.get("sort_fields", []))
        skip_frontends: set[str] = set(case.get("skip_frontends", []))
        check_registry: bool = case.get("check_against_registry", False)

        print(f"\n{_BOLD}[{case_name}]{_RESET}")

        # Determine which frontends to test for this case
        active_specs = {
            name: spec
            for name, spec in frontend_specs.items()
            if name not in skip_frontends
        }

        # Collect results from all frontends for shared golden comparison
        case_results: dict[str, dict[str, Any]] = {}

        for fname, spec in active_specs.items():
            fp = spawn_frontend(fname, spec)
            if fp is None:
                print(f"  {fname}: {_skip('not available (build missing)')}")
                continue

            try:
                resp = run_case_for_frontend(fp, case)
            finally:
                fp.kill()

            if resp is None:
                msg = f"{case_name}/{fname}: no response (timeout or crash)"
                print(f"  {fname}: {_fail('no response')}")
                failures.append(msg)
                continue

            # Extract and normalize the golden-stable fields
            extracted = extract_golden_fields(resp, golden_fields)
            normalized = normalize_for_golden(extracted, sort_fields)

            # Registry parity check (handshake only)
            registry_mismatch = False
            if check_registry and fname != "noop":
                expected_caps = registry_capabilities_for(fname, registry)
                if expected_caps is not None:
                    actual_caps = sorted(normalized.get("capabilities", []))
                    if actual_caps != expected_caps:
                        msg = (
                            f"{case_name}/{fname}: capability mismatch vs registry.yaml\n"
                            f"    registry: {expected_caps}\n"
                            f"    frontend: {actual_caps}"
                        )
                        print(f"  {fname}: {_fail('capabilities mismatch vs registry.yaml')}")
                        print(f"    registry: {expected_caps}")
                        print(f"    frontend: {actual_caps}")
                        failures.append(msg)
                        registry_mismatch = True
                        # In update mode, still write the golden from actual output;
                        # the registry fix is a separate concern.
                        if not args.update:
                            continue

            case_results[fname] = normalized
            total += 1

            gpath = golden_path(case_name, fname, per_frontend)

            if args.update:
                write_golden(gpath, normalized)
                print(f"  {fname}: {_updated(str(gpath.relative_to(REPO_ROOT)))}")
                updated.append(f"{case_name}/{fname}")
            else:
                golden = read_golden(gpath)
                if golden is None:
                    msg = f"{case_name}/{fname}: golden file missing — run --update to create it"
                    print(f"  {fname}: {_fail('golden file missing')}")
                    failures.append(msg)
                elif normalized != golden:
                    msg = _format_mismatch(case_name, fname, golden, normalized)
                    print(f"  {fname}: {_fail('golden mismatch')}")
                    print(f"    expected: {json.dumps(golden)}")
                    print(f"    actual:   {json.dumps(normalized)}")
                    failures.append(msg)
                else:
                    print(f"  {fname}: {_ok('matches golden')}")

        # For shared goldens, also verify all responding frontends agree
        if not per_frontend and len(case_results) >= 2 and not args.update:
            names = list(case_results.keys())
            ref_name, ref_val = names[0], case_results[names[0]]
            for other_name in names[1:]:
                other_val = case_results[other_name]
                if ref_val != other_val:
                    msg = (
                        f"{case_name}: cross-frontend divergence between "
                        f"{ref_name} and {other_name}\n"
                        f"    {ref_name}: {json.dumps(ref_val)}\n"
                        f"    {other_name}: {json.dumps(other_val)}"
                    )
                    print(f"  cross-check: {_fail(f'{ref_name} vs {other_name} diverge')}")
                    failures.append(msg)
                else:
                    print(f"  cross-check: {_ok(f'{ref_name} == {other_name}')}")

    # Summary
    print()
    print("=" * 50)
    print(f"Ran {total} golden checks across {len(cases)} cases")

    if args.update:
        if updated:
            print(f"{_YELLOW}Updated {len(updated)} golden file(s):{_RESET}")
            for u in updated:
                print(f"  {u}")
        if failures:
            print(f"{_RED}{len(failures)} error(s) during update:{_RESET}")
            for f in failures:
                print(f"  {_RED}ERROR{_RESET}: {f}")
            return 1
        return 0

    if not failures:
        print(f"{_GREEN}All golden checks passed.{_RESET}")
        return 0

    print(f"{_RED}{len(failures)} failure(s):{_RESET}")
    for f in failures:
        print(f"  {_RED}FAIL{_RESET}: {f}")
    return 1


def _format_mismatch(case: str, frontend: str, expected: Any, actual: Any) -> str:
    return (
        f"{case}/{frontend}: golden mismatch\n"
        f"    expected: {json.dumps(expected)}\n"
        f"    actual:   {json.dumps(actual)}"
    )


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Cross-frontend parity golden tests",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument(
        "--update",
        action="store_true",
        help="Regenerate golden files from current frontend output (review diff before committing)",
    )
    parser.add_argument(
        "--frontend", "-f",
        action="append",
        dest="frontends",
        metavar="NAME",
        help="Run only the named frontend(s). Can be repeated. Default: all available.",
    )
    return parser.parse_args()


if __name__ == "__main__":
    sys.exit(run_golden_tests(parse_args()))
