#!/usr/bin/env python3
"""Validate frontend capabilities against the checked-in parity contract.

Fails (exit 1) if any of the following are detected:
  - A command/type with status=implemented for a frontend is absent from
    that frontend's handler code (MISSING_REQUIRED)
  - A frontend's handler advertises a command/type that the matrix marks
    as not_implemented for that frontend (UNDOCUMENTED)
  - A command or complex_type detected in handler code is not present in
    the parity matrix at all (UNDOCUMENTED)

Emits warnings (not failures) if:
  - A mismatch is covered by an entry in allowed_divergences with
    status: accepted or status: tracked

Parity matrix schema (protocol/parity-matrix.yaml):

    commands:
      <name>:
        status: required | optional | frontend_specific
        frontends:
          typescript: implemented | not_implemented | partial
          go: implemented | not_implemented | partial
          rust: implemented | not_implemented | partial
        notes: '...'

    complex_type_capabilities:
      <type>:
        status: optional
        frontends:
          typescript: supported | not_supported
          go: supported | not_supported
          rust: supported | not_supported
        language_affinity: [ts, go, rust]

    allowed_divergences:
      - id: '...'
        description: '...'
        affected_frontends: [...]
        affected_commands: [...]
        status: tracked | accepted | resolved
        resolution: '...'

Usage:
    python3 scripts/validate-parity.py [--matrix PATH] [--verbose]
"""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent

DEFAULT_MATRIX_PATH = REPO_ROOT / "protocol" / "parity-matrix.yaml"
REGISTRY_PATH = REPO_ROOT / "protocol" / "registry.yaml"

# Frontend source files
TS_HANDLERS = REPO_ROOT / "shatter-ts" / "src" / "handlers.ts"
GO_CONSTANTS = REPO_ROOT / "shatter-go" / "protocol" / "constants.go"
GO_HANDLER = REPO_ROOT / "shatter-go" / "protocol" / "handler.go"
RUST_HANDLER = REPO_ROOT / "shatter-rust" / "src" / "handler.rs"

# Commands not listed in registry capabilities (always handled, never negotiated)
IMPLICIT_COMMANDS = {"handshake", "shutdown"}

SCHEMA_HELP = """
Expected parity matrix schema (protocol/parity-matrix.yaml):

    commands:
      <name>:
        status: required | optional | frontend_specific
        frontends:
          typescript: implemented | not_implemented | partial
          go: implemented | not_implemented | partial
          rust: implemented | not_implemented | partial
        notes: '...'

    complex_type_capabilities:
      <type>:
        status: optional
        frontends:
          typescript: supported | not_supported
          go: supported | not_supported
          rust: supported | not_supported
        language_affinity: [ts, go, rust]

    allowed_divergences:
      - id: '...'
        description: '...'
        affected_frontends: [...]
        affected_commands: [...]
        status: tracked | accepted | resolved
        resolution: '...'

This file is produced and maintained by str-7jgm.1 (parity contract spec).
"""


# ---------------------------------------------------------------------------
# Result accumulator
# ---------------------------------------------------------------------------

@dataclass
class Result:
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    def error(self, msg: str) -> None:
        self.errors.append(msg)

    def warn(self, msg: str) -> None:
        self.warnings.append(msg)

    def ok(self) -> bool:
        return not self.errors


# ---------------------------------------------------------------------------
# Matrix loader (requires pyyaml)
# ---------------------------------------------------------------------------

def load_matrix(path: Path) -> dict:
    """Load and minimally validate the parity matrix YAML."""
    try:
        import yaml
    except ImportError:
        print("ERROR: pyyaml is required — pip install pyyaml", file=sys.stderr)
        sys.exit(1)

    if not path.exists():
        print(f"ERROR: Parity matrix not found at: {path}", file=sys.stderr)
        print(SCHEMA_HELP, file=sys.stderr)
        print(
            "  str-7jgm.1 is responsible for populating this file.\n"
            "  Until that work lands, a bootstrap matrix is checked into the repo.",
            file=sys.stderr,
        )
        sys.exit(1)

    with path.open() as f:
        data = yaml.safe_load(f)

    if not isinstance(data, dict):
        print(f"ERROR: Parity matrix at {path} must be a YAML mapping", file=sys.stderr)
        sys.exit(1)

    missing = [k for k in ("commands", "complex_type_capabilities") if k not in data]
    if missing:
        print(
            f"ERROR: Parity matrix at {path} is missing required keys: {missing}",
            file=sys.stderr,
        )
        sys.exit(1)

    return data


# ---------------------------------------------------------------------------
# Registry loader (regex-based, no pyyaml dependency)
# ---------------------------------------------------------------------------

def load_registry_capabilities(path: Path) -> dict[str, set[str]]:
    """Extract valid command and complex_type capability names from registry.yaml."""
    if not path.exists():
        return {"command": set(), "complex_type": set()}

    text = path.read_text()

    cap_match = re.search(r"^capabilities:\s*$", text, re.MULTILINE)
    if not cap_match:
        return {"command": set(), "complex_type": set()}

    cap_block = text[cap_match.end():]
    end = re.search(r"^\S", cap_block, re.MULTILINE)
    cap_block = cap_block[: end.start()] if end else cap_block

    def extract_sublist(block: str, key: str) -> set[str]:
        m = re.search(rf"^\s+{re.escape(key)}:\s*$", block, re.MULTILINE)
        if not m:
            return set()
        items: set[str] = set()
        for line in block[m.end():].splitlines():
            if not line or re.match(r"^\s*#", line):
                continue
            if line[0] not in (" ", "\t"):
                break
            item = re.match(r"^\s{4,}-\s+(\S+)", line)
            if item:
                items.add(item.group(1))
        return items

    return {
        "command": extract_sublist(cap_block, "command"),
        "complex_type": extract_sublist(cap_block, "complex_type"),
    }


# ---------------------------------------------------------------------------
# Frontend capability detectors (static analysis)
# ---------------------------------------------------------------------------

@dataclass
class DetectedState:
    commands: set[str] = field(default_factory=set)
    complex_types: set[str] = field(default_factory=set)


def detect_typescript(verbose: bool) -> DetectedState:
    """Detect capabilities from shatter-ts/src/handlers.ts."""
    state = DetectedState()

    if not TS_HANDLERS.exists():
        if verbose:
            print(f"  [ts] handler file not found: {TS_HANDLERS}")
        return state

    text = TS_HANDLERS.read_text()

    # SUPPORTED_CAPABILITIES = ["analyze", "execute", ..., "complex_type:date", ...]
    cap_match = re.search(
        r"SUPPORTED_CAPABILITIES\s*=\s*\[([^\]]+)\]", text, re.DOTALL
    )
    if cap_match:
        for item in re.findall(r'"([^"]+)"', cap_match.group(1)):
            if item.startswith("complex_type:"):
                state.complex_types.add(item.removeprefix("complex_type:"))
            else:
                state.commands.add(item)
    elif verbose:
        print("  [ts] WARNING: SUPPORTED_CAPABILITIES not found in handlers.ts")

    return state


def detect_go(verbose: bool) -> DetectedState:
    """Detect capabilities from shatter-go/protocol/constants.go and handler.go."""
    state = DetectedState()

    # CommandCapabilities from constants.go
    if GO_CONSTANTS.exists():
        const_text = GO_CONSTANTS.read_text()
        m = re.search(
            r"CommandCapabilities\s*=\s*\[\]string\{([^}]+)\}", const_text, re.DOTALL
        )
        if m:
            for item in re.findall(r'"([^"]+)"', m.group(1)):
                state.commands.add(item)
        elif verbose:
            print("  [go] WARNING: CommandCapabilities not found in constants.go")
    elif verbose:
        print(f"  [go] constants file not found: {GO_CONSTANTS}")

    # Complex type capabilities from handleHandshake in handler.go
    if GO_HANDLER.exists():
        handler_text = GO_HANDLER.read_text()
        hh_match = re.search(
            r"func \(\w+ \*Handler\) handleHandshake\b.*?^}", handler_text,
            re.DOTALL | re.MULTILINE,
        )
        block = hh_match.group(0) if hh_match else handler_text
        for item in re.findall(r'"complex_type:([^"]+)"', block):
            state.complex_types.add(item)
    elif verbose:
        print(f"  [go] handler file not found: {GO_HANDLER}")

    return state


def detect_rust(verbose: bool) -> DetectedState:
    """Detect capabilities from shatter-rust/src/handler.rs."""
    state = DetectedState()

    if not RUST_HANDLER.exists():
        if verbose:
            print(f"  [rust] handler file not found: {RUST_HANDLER}")
        return state

    text = RUST_HANDLER.read_text()

    # resp.capabilities = Some(vec!["analyze", "execute", ...])
    hh_match = re.search(
        r"fn handle_handshake\b.*?^\s*\}", text, re.DOTALL | re.MULTILINE
    )
    block = hh_match.group(0) if hh_match else text

    for item in re.findall(r'"([^"]+)"\.to_string\(\)', block):
        if item.startswith("complex_type:"):
            state.complex_types.add(item.removeprefix("complex_type:"))
        elif item not in IMPLICIT_COMMANDS:
            state.commands.add(item)

    return state


DETECTORS: dict[str, object] = {
    "typescript": detect_typescript,
    "go": detect_go,
    "rust": detect_rust,
}


# ---------------------------------------------------------------------------
# Allowed divergence helpers
# ---------------------------------------------------------------------------

def build_divergence_index(allowed: list[dict]) -> set[tuple[str, str]]:
    """Build a set of (frontend, command_or_type) pairs that are excused."""
    excused: set[tuple[str, str]] = set()
    for div in allowed:
        if div.get("status") in ("accepted", "tracked"):
            for fe in div.get("affected_frontends", []):
                for cmd in div.get("affected_commands", []):
                    excused.add((fe, cmd))
    return excused


# ---------------------------------------------------------------------------
# Core validation
# ---------------------------------------------------------------------------

def validate(
    matrix: dict,
    detected_per_frontend: dict[str, DetectedState],
    registry_caps: dict[str, set[str]],
    result: Result,
    verbose: bool,
) -> None:
    allowed_divergences: list[dict] = matrix.get("allowed_divergences", []) or []
    excused = build_divergence_index(allowed_divergences)

    registry_commands = registry_caps.get("command", set())
    registry_types = registry_caps.get("complex_type", set())

    # -----------------------------------------------------------------------
    # Commands
    # -----------------------------------------------------------------------
    matrix_commands: dict = matrix.get("commands", {}) or {}

    for cmd_name, cmd_def in matrix_commands.items():
        if not isinstance(cmd_def, dict):
            result.error(f"commands.{cmd_name}: expected a mapping, got {type(cmd_def).__name__}")
            continue

        # Validate that command is known to the registry (or is implicit)
        if registry_commands and cmd_name not in registry_commands | IMPLICIT_COMMANDS:
            result.error(
                f"commands.{cmd_name}: not in protocol/registry.yaml capabilities.command "
                f"(and not an implicit command like handshake/shutdown)"
            )

        # handshake/shutdown are always present — skip capability validation
        if cmd_name in IMPLICIT_COMMANDS:
            if verbose:
                print(f"  [{cmd_name}] implicit command — skipping capability check")
            continue

        frontends_map: dict = cmd_def.get("frontends", {}) or {}

        for frontend_name, impl_status in frontends_map.items():
            detected = detected_per_frontend.get(frontend_name)
            if detected is None:
                result.error(
                    f"commands.{cmd_name}.frontends.{frontend_name}: "
                    f"no detector registered for frontend '{frontend_name}'"
                )
                continue

            in_code = cmd_name in detected.commands

            if impl_status == "implemented":
                if not in_code:
                    if (frontend_name, cmd_name) in excused:
                        result.warn(
                            f"[{frontend_name}] MISSING command '{cmd_name}' "
                            f"(excused by allowed_divergences)"
                        )
                    else:
                        result.error(
                            f"[{frontend_name}] MISSING_REQUIRED command '{cmd_name}': "
                            f"matrix says implemented but not detected in handler code"
                        )
            elif impl_status == "not_implemented":
                if in_code:
                    if (frontend_name, cmd_name) in excused:
                        result.warn(
                            f"[{frontend_name}] UNDOCUMENTED command '{cmd_name}' "
                            f"(excused by allowed_divergences)"
                        )
                    else:
                        result.error(
                            f"[{frontend_name}] UNDOCUMENTED command '{cmd_name}': "
                            f"handler advertises it but matrix says not_implemented"
                        )
            elif impl_status == "partial":
                if verbose:
                    status_str = "advertised" if in_code else "not advertised"
                    print(
                        f"  [{frontend_name}] command '{cmd_name}': partial "
                        f"({status_str} in handler)"
                    )
            else:
                result.error(
                    f"commands.{cmd_name}.frontends.{frontend_name}: "
                    f"unknown impl_status '{impl_status}' "
                    f"(expected: implemented | not_implemented | partial)"
                )

    # Detect commands in handler code that aren't in the matrix at all
    for frontend_name, detected in detected_per_frontend.items():
        for cmd in sorted(detected.commands):
            if cmd in IMPLICIT_COMMANDS:
                continue
            if cmd not in matrix_commands:
                if (frontend_name, cmd) in excused:
                    result.warn(
                        f"[{frontend_name}] UNDOCUMENTED command '{cmd}' not in matrix "
                        f"(excused by allowed_divergences)"
                    )
                else:
                    result.error(
                        f"[{frontend_name}] UNDOCUMENTED command '{cmd}': "
                        f"advertised by handler but absent from parity matrix"
                    )
            elif frontend_name not in (matrix_commands[cmd].get("frontends") or {}):
                result.error(
                    f"[{frontend_name}] command '{cmd}': frontend not listed in "
                    f"matrix commands.{cmd}.frontends"
                )

    # -----------------------------------------------------------------------
    # Complex type capabilities
    # -----------------------------------------------------------------------
    matrix_types: dict = matrix.get("complex_type_capabilities", {}) or {}

    for type_name, type_def in matrix_types.items():
        if not isinstance(type_def, dict):
            result.error(
                f"complex_type_capabilities.{type_name}: "
                f"expected a mapping, got {type(type_def).__name__}"
            )
            continue

        # Validate against registry
        if registry_types and type_name not in registry_types:
            result.error(
                f"complex_type_capabilities.{type_name}: "
                f"not in protocol/registry.yaml capabilities.complex_type"
            )

        frontends_map = type_def.get("frontends", {}) or {}

        for frontend_name, support_status in frontends_map.items():
            detected = detected_per_frontend.get(frontend_name)
            if detected is None:
                result.error(
                    f"complex_type_capabilities.{type_name}.frontends.{frontend_name}: "
                    f"no detector registered for frontend '{frontend_name}'"
                )
                continue

            in_code = type_name in detected.complex_types

            if support_status == "supported":
                if not in_code:
                    if (frontend_name, type_name) in excused:
                        result.warn(
                            f"[{frontend_name}] MISSING complex_type '{type_name}' "
                            f"(excused by allowed_divergences)"
                        )
                    else:
                        result.error(
                            f"[{frontend_name}] MISSING complex_type '{type_name}': "
                            f"matrix says supported but not detected in handler code"
                        )
            elif support_status == "not_supported":
                if in_code:
                    if (frontend_name, type_name) in excused:
                        result.warn(
                            f"[{frontend_name}] UNDOCUMENTED complex_type '{type_name}' "
                            f"(excused by allowed_divergences)"
                        )
                    else:
                        result.error(
                            f"[{frontend_name}] UNDOCUMENTED complex_type '{type_name}': "
                            f"handler advertises it but matrix says not_supported"
                        )
            else:
                result.error(
                    f"complex_type_capabilities.{type_name}.frontends.{frontend_name}: "
                    f"unknown support_status '{support_status}' "
                    f"(expected: supported | not_supported)"
                )

    # Detect complex types in handler code not in the matrix at all
    for frontend_name, detected in detected_per_frontend.items():
        for t in sorted(detected.complex_types):
            if t not in matrix_types:
                if (frontend_name, t) in excused:
                    result.warn(
                        f"[{frontend_name}] UNDOCUMENTED complex_type '{t}' not in matrix "
                        f"(excused by allowed_divergences)"
                    )
                else:
                    result.error(
                        f"[{frontend_name}] UNDOCUMENTED complex_type '{t}': "
                        f"advertised by handler but absent from parity matrix"
                    )
            elif frontend_name not in (matrix_types[t].get("frontends") or {}):
                result.error(
                    f"[{frontend_name}] complex_type '{t}': frontend not listed in "
                    f"matrix complex_type_capabilities.{t}.frontends"
                )


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--matrix",
        metavar="PATH",
        type=Path,
        default=DEFAULT_MATRIX_PATH,
        help=f"Path to parity matrix YAML (default: {DEFAULT_MATRIX_PATH})",
    )
    parser.add_argument(
        "--verbose", "-v", action="store_true",
        help="Show detected capabilities per frontend"
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    verbose: bool = args.verbose
    matrix_path: Path = args.matrix

    print(f"Loading parity matrix: {matrix_path}")
    matrix = load_matrix(matrix_path)

    n_commands = len(matrix.get("commands", {}) or {})
    n_types = len(matrix.get("complex_type_capabilities", {}) or {})
    n_divs = len(matrix.get("allowed_divergences", []) or [])
    print(
        f"Matrix: {n_commands} commands, {n_types} complex_types, "
        f"{n_divs} allowed_divergences"
    )

    print(f"\nLoading registry capabilities: {REGISTRY_PATH}")
    registry_caps = load_registry_capabilities(REGISTRY_PATH)
    if verbose:
        print(
            f"  Registry: {len(registry_caps['command'])} command caps, "
            f"{len(registry_caps['complex_type'])} complex_type caps"
        )

    # Detect capabilities from each frontend
    detected_per_frontend: dict[str, DetectedState] = {}
    all_frontends_in_matrix: set[str] = set()

    for cmd_def in (matrix.get("commands", {}) or {}).values():
        if isinstance(cmd_def, dict):
            all_frontends_in_matrix.update((cmd_def.get("frontends") or {}).keys())
    for type_def in (matrix.get("complex_type_capabilities", {}) or {}).values():
        if isinstance(type_def, dict):
            all_frontends_in_matrix.update((type_def.get("frontends") or {}).keys())

    print()
    for frontend_name in sorted(all_frontends_in_matrix):
        detector = DETECTORS.get(frontend_name)
        if detector is None:
            print(f"  WARNING: no detector for frontend '{frontend_name}' — skipping")
            continue
        print(f"Detecting capabilities: {frontend_name}")
        detected = detector(verbose)  # type: ignore[operator]
        detected_per_frontend[frontend_name] = detected
        if verbose:
            print(f"  commands: {sorted(detected.commands)}")
            print(f"  complex_types: {sorted(detected.complex_types)}")

    result = Result()
    print("\nValidating...")
    validate(matrix, detected_per_frontend, registry_caps, result, verbose)

    # Report
    print()
    if result.warnings:
        print("Warnings (informational, not failures):")
        for w in result.warnings:
            print(f"  warn: {w}")
        print()

    if result.errors:
        print(f"ERRORS ({len(result.errors)} failure(s)):")
        for e in result.errors:
            print(f"  FAIL: {e}")
        print()
        print("Parity check FAILED.")
        return 1

    print("Parity check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
