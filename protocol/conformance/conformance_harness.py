#!/usr/bin/env python3
"""Black-box protocol conformance harness for Shatter frontends.

Spawns each frontend subprocess, sends identical NDJSON request sequences,
and compares responses for structural drift. Dynamic fields (language,
version, performance, etc.) are normalized before cross-frontend comparison.

Exit codes:
  0 — all checks pass
  1 — shape validation failure or cross-frontend drift detected
"""

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
CASES_FILE = SCRIPT_DIR / "conformance_cases.yaml"

PROTOCOL_VERSION = "0.1.0"
# Handshake timeout is longer to allow for compile-on-the-fly frontends (go run, cargo run)
HANDSHAKE_TIMEOUT_S = 120
COMMAND_TIMEOUT_S = 30
SHUTDOWN_WAIT_S = 5


# ---------------------------------------------------------------------------
# Terminal colours (disabled when not a TTY)
# ---------------------------------------------------------------------------

if sys.stdout.isatty():
    _BOLD = "\033[1m"
    _RED = "\033[31m"
    _GREEN = "\033[32m"
    _YELLOW = "\033[33m"
    _BLUE = "\033[34m"
    _RESET = "\033[0m"
else:
    _BOLD = _RED = _GREEN = _YELLOW = _BLUE = _RESET = ""


def _ok(msg: str) -> str:
    return f"{_GREEN}OK{_RESET} {msg}"


def _fail(msg: str) -> str:
    return f"{_RED}FAIL{_RESET} {msg}"


def _skip(msg: str) -> str:
    return f"{_YELLOW}SKIP{_RESET} {msg}"


def _drift(msg: str) -> str:
    return f"{_RED}DRIFT{_RESET} {msg}"


# ---------------------------------------------------------------------------
# YAML loading
# ---------------------------------------------------------------------------

def load_cases(path: Path) -> dict[str, Any]:
    with open(path) as f:
        return yaml.safe_load(f)


# ---------------------------------------------------------------------------
# Frontend process management
# ---------------------------------------------------------------------------

class FrontendProc:
    """Wraps a frontend subprocess for line-based JSON I/O."""

    def __init__(self, name: str, proc: subprocess.Popen[bytes]):
        self.name = name
        self.proc = proc
        self.capabilities: list[str] = []
        self.stderr_lines: list[str] = []

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

        # Read with timeout using select
        ready, _, _ = select.select([self.proc.stdout], [], [], timeout)
        if not ready:
            return None

        resp_line = self.proc.stdout.readline()
        if not resp_line:
            return None

        return json.loads(resp_line.decode().strip())

    def drain_stderr(self) -> None:
        """Collect any stderr output (non-blocking)."""
        assert self.proc.stderr is not None
        while True:
            ready, _, _ = select.select([self.proc.stderr], [], [], 0.1)
            if not ready:
                break
            line = self.proc.stderr.readline()
            if not line:
                break
            self.stderr_lines.append(line.decode().rstrip())

    def kill(self) -> None:
        if self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait()


def spawn_frontend(name: str, spec: dict[str, Any]) -> FrontendProc | None:
    """Spawn a frontend subprocess. Returns None if unavailable."""
    command = list(spec["command"])
    cwd = REPO_ROOT / spec["cwd"] if "cwd" in spec else REPO_ROOT

    # Check build artifact exists if specified
    build_check = spec.get("build_check")
    if build_check and not (REPO_ROOT / build_check).exists():
        return None

    # Resolve command[0] relative to REPO_ROOT if it's a relative path
    exe = command[0]
    exe_path = REPO_ROOT / exe
    if exe_path.exists():
        command[0] = str(exe_path)
    elif not _which(exe):
        return None

    # Resolve any remaining relative path args (e.g. script paths)
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
    """Check if a command is available on PATH."""
    from shutil import which
    return which(cmd) is not None


# ---------------------------------------------------------------------------
# Response normalization and structural comparison
# ---------------------------------------------------------------------------

def normalize_response(response: dict[str, Any], dynamic_fields: list[str]) -> dict[str, Any]:
    """Replace dynamic fields with type-stable placeholders for comparison."""
    result: dict[str, Any] = {}
    for key, value in response.items():
        if key in dynamic_fields:
            result[key] = _placeholder(value)
        elif isinstance(value, dict):
            result[key] = normalize_response(value, dynamic_fields)
        elif isinstance(value, list):
            result[key] = [
                normalize_response(item, dynamic_fields) if isinstance(item, dict) else item
                for item in value
            ]
        else:
            result[key] = value
    return result


def _placeholder(value: Any) -> str:
    """Return a type-stable placeholder for a dynamic value."""
    if isinstance(value, str):
        return "<string>"
    if isinstance(value, bool):
        return "<bool>"
    if isinstance(value, int):
        return "<int>"
    if isinstance(value, float):
        return "<float>"
    if isinstance(value, list):
        return "<array>"
    if isinstance(value, dict):
        return "<object>"
    if value is None:
        return "<null>"
    return "<unknown>"


def extract_structure(obj: Any) -> Any:
    """Recursively extract a type skeleton for structural comparison."""
    if isinstance(obj, dict):
        return {k: extract_structure(v) for k, v in sorted(obj.items())}
    if isinstance(obj, list):
        if not obj:
            return "array(empty)"
        # Use first element as representative
        return f"array({extract_structure(obj[0])})"
    if isinstance(obj, str):
        return "string"
    if isinstance(obj, bool):
        return "bool"
    if isinstance(obj, int):
        return "int"
    if isinstance(obj, float):
        return "float"
    if obj is None:
        return "null"
    return str(type(obj))


# ---------------------------------------------------------------------------
# Shape validation
# ---------------------------------------------------------------------------

TYPE_MAP = {
    "string": str,
    "int": int,
    "float": (int, float),
    "bool": bool,
    "array": list,
    "object": dict,
    "any": object,
}


def validate_shape(response: dict[str, Any], expect: dict[str, Any], request: dict[str, Any]) -> list[str]:
    """Validate a single response against expected shape. Returns error strings."""
    errors: list[str] = []

    # Check protocol_version
    if response.get("protocol_version") != PROTOCOL_VERSION:
        errors.append(
            f"protocol_version: expected {PROTOCOL_VERSION!r}, "
            f"got {response.get('protocol_version')!r}"
        )

    # Check id matches request
    if response.get("id") != request.get("id"):
        errors.append(f"id: expected {request.get('id')}, got {response.get('id')}")

    # Check status — supports both exact match and oneof
    actual_status = response.get("status")
    if "status" in expect:
        expected_status = expect["status"]
        if actual_status != expected_status:
            errors.append(f"status: expected {expected_status!r}, got {actual_status!r}")
    elif "status_oneof" in expect:
        allowed = expect["status_oneof"]
        if actual_status not in allowed:
            errors.append(f"status: expected one of {allowed}, got {actual_status!r}")

    # Determine required fields — supports status-dependent fields
    required_fields: dict[str, Any] = {}
    if "required_fields" in expect:
        required_fields = expect["required_fields"] or {}
    elif "required_fields_by_status" in expect and actual_status:
        required_fields = expect["required_fields_by_status"].get(actual_status, {}) or {}

    # Check required fields
    errors.extend(_validate_required_fields(response, required_fields, path=""))

    return errors


def _validate_required_fields(
    container: Any,
    required_fields: dict[str, Any],
    path: str,
) -> list[str]:
    """Check each field name exists on `container` and satisfies its constraints.

    Supports one level of nested validation via `fields:` on object constraints,
    and value-set validation via `enum:` on leaf constraints. Used so cases can
    assert structural shape (e.g., outcome.status ∈ enum) without duplicating
    the flat-key contract.
    """
    errors: list[str] = []
    if not isinstance(container, dict):
        errors.append(f"{path or '<root>'}: expected object, got {type(container).__name__}")
        return errors

    for field_name, constraints in required_fields.items():
        qualified = f"{path}.{field_name}" if path else field_name
        if field_name not in container:
            errors.append(f"missing required field: {qualified}")
            continue

        value = container[field_name]
        expected_type = constraints.get("type", "any")
        if expected_type != "any":
            py_type = TYPE_MAP.get(expected_type)
            if py_type and not isinstance(value, py_type):
                errors.append(
                    f"field {qualified}: expected type {expected_type}, "
                    f"got {type(value).__name__}"
                )
                continue

        min_length = constraints.get("min_length")
        if min_length is not None and isinstance(value, list) and len(value) < min_length:
            errors.append(
                f"field {qualified}: expected min_length {min_length}, "
                f"got {len(value)}"
            )

        allowed_values = constraints.get("enum")
        if allowed_values is not None and value not in allowed_values:
            errors.append(
                f"field {qualified}: expected one of {allowed_values}, got {value!r}"
            )

        nested_fields = constraints.get("fields")
        if nested_fields:
            errors.extend(_validate_required_fields(value, nested_fields, qualified))

        item_fields = constraints.get("first_item_fields")
        if item_fields:
            if not isinstance(value, list) or not value:
                errors.append(
                    f"field {qualified}: expected non-empty array for first_item_fields"
                )
            else:
                errors.extend(
                    _validate_required_fields(
                        value[0], item_fields, f"{qualified}[0]"
                    )
                )

    return errors


# ---------------------------------------------------------------------------
# Cross-frontend comparison
# ---------------------------------------------------------------------------

def compare_structures(
    responses: dict[str, dict[str, Any]],
    dynamic_fields: list[str],
) -> list[str]:
    """Compare normalized response structures across frontends. Returns drift errors."""
    if len(responses) < 2:
        return []

    normalized = {
        name: normalize_response(resp, dynamic_fields)
        for name, resp in responses.items()
    }

    skeletons = {
        name: extract_structure(norm)
        for name, norm in normalized.items()
    }

    names = list(skeletons.keys())
    reference_name = names[0]
    reference_skeleton = skeletons[reference_name]
    drifts: list[str] = []

    for name in names[1:]:
        if skeletons[name] != reference_skeleton:
            diff = _describe_skeleton_diff(reference_name, reference_skeleton, name, skeletons[name])
            drifts.append(diff)

    return drifts


def _describe_skeleton_diff(
    name_a: str, skel_a: Any, name_b: str, skel_b: Any, path: str = ""
) -> str:
    """Human-readable description of structural differences."""
    if type(skel_a) != type(skel_b):
        return f"{path or 'root'}: {name_a} has {_type_label(skel_a)}, {name_b} has {_type_label(skel_b)}"

    if isinstance(skel_a, dict) and isinstance(skel_b, dict):
        keys_a = set(skel_a.keys())
        keys_b = set(skel_b.keys())
        parts = []
        for key in keys_a - keys_b:
            parts.append(f"{path}.{key}: present in {name_a}, missing in {name_b}")
        for key in keys_b - keys_a:
            parts.append(f"{path}.{key}: missing in {name_a}, present in {name_b}")
        for key in keys_a & keys_b:
            if skel_a[key] != skel_b[key]:
                parts.append(
                    _describe_skeleton_diff(name_a, skel_a[key], name_b, skel_b[key], f"{path}.{key}")
                )
        return "; ".join(parts) if parts else f"{path}: structures differ"

    if skel_a != skel_b:
        return f"{path or 'root'}: {name_a}={skel_a!r}, {name_b}={skel_b!r}"

    return ""


def _compare_grouped_by_status(
    responses: dict[str, dict[str, Any]],
    dynamic_fields: list[str],
) -> list[str]:
    """Group responses by status, then compare structures within each group."""
    groups: dict[str, dict[str, dict[str, Any]]] = {}
    for name, resp in responses.items():
        status = resp.get("status", "unknown")
        groups.setdefault(status, {})[name] = resp

    drifts: list[str] = []
    for status, group in groups.items():
        if len(group) >= 2:
            group_drifts = compare_structures(group, dynamic_fields)
            for d in group_drifts:
                drifts.append(f"(status={status}) {d}")

    return drifts


def _type_label(skel: Any) -> str:
    if isinstance(skel, dict):
        return "object"
    if isinstance(skel, str):
        return skel
    return str(type(skel).__name__)


# ---------------------------------------------------------------------------
# Main harness
# ---------------------------------------------------------------------------

def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Protocol conformance harness")
    parser.add_argument(
        "--frontend", "-f",
        action="append",
        dest="frontends",
        help="Run only the named frontend(s). Can be repeated. Default: all available.",
    )
    return parser.parse_args()


def run_conformance() -> int:
    args = parse_args()

    config = load_cases(CASES_FILE)
    frontend_specs = config["frontends"]
    cases = config["cases"]

    # Filter frontends if --frontend specified
    if args.frontends:
        frontend_specs = {
            k: v for k, v in frontend_specs.items() if k in args.frontends
        }

    print(f"\n{_BOLD}Protocol Conformance Testing{_RESET}")
    print("=" * 40)

    # Phase 1: Spawn frontends
    print(f"\n{_BOLD}Spawning frontends:{_RESET}")
    frontends: dict[str, FrontendProc] = {}
    for name, spec in frontend_specs.items():
        fp = spawn_frontend(name, spec)
        if fp is None:
            print(f"  {_skip(name)}")
        else:
            frontends[name] = fp
            print(f"  {_ok(name)}")

    if not frontends:
        print(f"\n{_RED}No frontends available. Nothing to test.{_RESET}")
        return 1

    total_checks = 0
    failures: list[str] = []
    drifts: list[str] = []

    try:
        # Phase 2: Run handshake first to learn capabilities
        handshake_case = cases[0]
        assert handshake_case["name"] == "handshake", "First case must be handshake"

        print(f"\n{_BOLD}Running {len(cases)} conformance cases against {len(frontends)} frontends:{_RESET}\n")

        for case in cases:
            case_name = case["name"]
            request = case["request"]
            expect = case["expect"]
            dynamic_fields = expect.get("dynamic_fields", [])
            expect_exit = expect.get("expect_exit", False)

            print(f"  [{case_name}]")

            case_responses: dict[str, dict[str, Any]] = {}
            skip_frontends = set(case.get("skip_frontends", []))
            allowlist = case.get("frontends")
            allow_frontends = set(allowlist) if allowlist is not None else None

            for fname, fp in list(frontends.items()):
                # Skip frontends explicitly excluded from this case
                if fname in skip_frontends:
                    print(f"    {fname}: {_skip('(excluded from case)')}")
                    continue

                # Skip frontends not in the case's inclusion list (if provided)
                if allow_frontends is not None and fname not in allow_frontends:
                    print(f"    {fname}: {_skip('(not in case allowlist)')}")
                    continue

                total_checks += 1

                # Skip commands the frontend doesn't support (based on capabilities)
                cmd = request.get("command", "")
                if fp.capabilities and cmd not in ("handshake", "shutdown"):
                    if cmd not in fp.capabilities:
                        print(f"    {fname}: {_skip('(not in capabilities)')}")
                        continue

                # Use longer timeout for handshake (frontends may compile on first request).
                # Cases that exercise on-demand compilation (e.g. the Rust executor's
                # cargo build) can override via the optional `timeout_s` field.
                if case_name == "handshake":
                    timeout = HANDSHAKE_TIMEOUT_S
                else:
                    timeout = case.get("timeout_s", COMMAND_TIMEOUT_S)
                resp = fp.send(request, timeout=timeout)
                if resp is None:
                    msg = f"{fname} / {case_name} -- no response (timeout or crash)"
                    print(f"    {fname}: {_fail('no response')}")
                    failures.append(msg)
                    fp.drain_stderr()
                    continue

                # Validate individual shape
                errs = validate_shape(resp, expect, request)
                if errs:
                    for err in errs:
                        msg = f"{fname} / {case_name} -- {err}"
                        failures.append(msg)
                    print(f"    {fname}: {_fail('; '.join(errs))}")
                else:
                    status = resp.get("status", "?")
                    print(f"    {fname}: {_ok(f'(status={status})')}")
                    case_responses[fname] = resp

                # Capture capabilities from handshake
                if case_name == "handshake" and resp.get("status") == "handshake":
                    fp.capabilities = resp.get("capabilities", [])

                # Wait for clean exit after shutdown
                if expect_exit and resp is not None:
                    try:
                        fp.proc.wait(timeout=SHUTDOWN_WAIT_S)
                    except subprocess.TimeoutExpired:
                        msg = f"{fname} / {case_name} -- process did not exit after shutdown"
                        failures.append(msg)
                        fp.kill()

            # Cross-frontend comparison
            if len(case_responses) >= 2:
                if "status_oneof" in expect:
                    # Group by status and compare within each group
                    drift_errs = _compare_grouped_by_status(case_responses, dynamic_fields)
                else:
                    drift_errs = compare_structures(case_responses, dynamic_fields)
                if drift_errs:
                    for d in drift_errs:
                        drifts.append(f"{case_name} -- {d}")
                    print(f"    cross-check: {_drift('; '.join(drift_errs))}")
                else:
                    print(f"    cross-check: {_ok('structures match')}")
            elif len(case_responses) == 1:
                print(f"    cross-check: {_skip('only 1 frontend responded')}")

            print()

    finally:
        # Cleanup: kill any remaining processes
        for fp in frontends.values():
            fp.drain_stderr()
            fp.kill()

    # Phase 3: Report
    known_drift_patterns = [
        kd["pattern"] for kd in config.get("known_drifts", [])
    ]

    # Partition drifts into known (warnings) and unexpected (failures)
    unexpected_drifts: list[str] = []
    known_drift_hits: list[str] = []
    for d in drifts:
        if any(pat in d for pat in known_drift_patterns):
            known_drift_hits.append(d)
        else:
            unexpected_drifts.append(d)

    print("=" * 40)
    n_frontends = len(frontends)
    print(f"Tested {n_frontends} frontends x {len(cases)} cases = {total_checks} checks")

    if known_drift_hits:
        print(f"{len(known_drift_hits)} known drift(s) (warnings):")
        for d in known_drift_hits:
            print(f"  {_YELLOW}KNOWN{_RESET}: {d}")

    if not failures and not unexpected_drifts:
        print(f"{_GREEN}All checks passed.{_RESET}")
        return 0

    print(f"{len(failures)} failure(s), {len(unexpected_drifts)} unexpected drift(s):")
    for f in failures:
        print(f"  {_RED}FAIL{_RESET}: {f}")
    for d in unexpected_drifts:
        print(f"  {_RED}DRIFT{_RESET}: {d}")

    return 1


if __name__ == "__main__":
    sys.exit(run_conformance())
