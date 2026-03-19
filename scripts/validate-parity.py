#!/usr/bin/env python3
"""Validate frontend capabilities against the checked-in parity contract.

Fails (exit 1) if any of the following are detected:
  - A required capability is absent from a frontend's handler code
  - A frontend's handler advertises a capability not documented in the parity matrix
  - A frontend's coded default timeout differs from the parity contract
  - The parity matrix references capabilities not defined in protocol/registry.yaml

Emits warnings (not failures) if:
  - An optional capability is absent from a frontend's handler code

Usage:
    python3 scripts/validate-parity.py [--matrix PATH] [--verbose]

Expected parity matrix schema: see protocol/parity-matrix.schema.yaml
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
TS_EXECUTOR = REPO_ROOT / "shatter-ts" / "src" / "executor.ts"
GO_CONSTANTS = REPO_ROOT / "shatter-go" / "protocol" / "constants.go"
GO_HANDLER = REPO_ROOT / "shatter-go" / "protocol" / "handler.go"
GO_EXECUTOR = REPO_ROOT / "shatter-go" / "instrument" / "executor.go"
RUST_HANDLER = REPO_ROOT / "shatter-rust" / "src" / "handler.rs"

SCHEMA_HELP = """
Expected parity matrix schema (protocol/parity-matrix.yaml):

    schema_version: "1"
    protocol_version: "0.1.0"
    frontends:
      <frontend-name>:           # e.g. typescript, go, rust
        required:
          commands: [...]        # must be present in handshake capabilities
          complex_types: [...]   # must be present in handshake capabilities
        optional:
          commands: [...]        # may be present; absence is a warning not a failure
          complex_types: [...]   # may be present; absence is a warning not a failure
        default_timeout_seconds: <int>   # frontend coded default; divergence is a failure

All capability names must appear in protocol/registry.yaml under
'capabilities.command' or 'capabilities.complex_type'.

See protocol/parity-matrix.schema.yaml for the full annotated schema.
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
# Parity matrix loader (requires pyyaml)
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
            "  Until that work lands, run the validator with --matrix pointing\n"
            "  at a manually created stub that matches the schema above.",
            file=sys.stderr,
        )
        sys.exit(1)

    with path.open() as f:
        data = yaml.safe_load(f)

    if not isinstance(data, dict):
        print(f"ERROR: Parity matrix at {path} must be a YAML mapping", file=sys.stderr)
        sys.exit(1)
    if "frontends" not in data:
        print(
            f"ERROR: Parity matrix at {path} is missing required 'frontends' key",
            file=sys.stderr,
        )
        sys.exit(1)

    return data


# ---------------------------------------------------------------------------
# Registry loader (no pyyaml — simple regex like validate-protocol-registry.py)
# ---------------------------------------------------------------------------

def load_registry_capabilities(path: Path) -> dict[str, set[str]]:
    """Extract valid command and complex_type capability names from registry.yaml."""
    if not path.exists():
        return {"command": set(), "complex_type": set()}

    text = path.read_text()

    def extract_list_under(section: str) -> set[str]:
        pattern = rf"^{re.escape(section)}:\s*$"
        m = re.search(pattern, text, re.MULTILINE)
        if not m:
            return set()
        items: set[str] = set()
        for line in text[m.end():].splitlines():
            if not line or line.startswith("#"):
                continue
            if line[0] not in (" ", "\t"):
                break
            item = re.match(r"^\s+-\s+(\S+)", line)
            if item:
                items.add(item.group(1))
        return items

    # capabilities.command and capabilities.complex_type are nested under
    # capabilities:, so locate the subsections by indentation.
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
            # Only collect list items (deeper indent than key)
            item = re.match(r"^\s{4,}-\s+(\S+)", line)
            if item:
                items.add(item.group(1))
        return items

    return {
        "command": extract_sublist(cap_block, "command"),
        "complex_type": extract_sublist(cap_block, "complex_type"),
    }


# ---------------------------------------------------------------------------
# Frontend capability detectors (static analysis of handler source)
# ---------------------------------------------------------------------------

@dataclass
class DetectedState:
    commands: set[str] = field(default_factory=set)
    complex_types: set[str] = field(default_factory=set)
    default_timeout_seconds: int | None = None


def detect_typescript(verbose: bool) -> DetectedState:
    """Detect capabilities from shatter-ts/src/handlers.ts and executor.ts."""
    state = DetectedState()

    if not TS_HANDLERS.exists():
        if verbose:
            print(f"  [ts] handler file not found: {TS_HANDLERS}")
        return state

    text = TS_HANDLERS.read_text()

    # SUPPORTED_CAPABILITIES = ["analyze", "execute", ..., "complex_type:date", ...]
    # Find the array and collect all string literals from it.
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

    # DEFAULT_EXEC_TIMEOUT_MS from executor.ts
    if TS_EXECUTOR.exists():
        ex_text = TS_EXECUTOR.read_text()
        m = re.search(r"DEFAULT_EXEC_TIMEOUT_MS\s*=\s*([\d_]+)", ex_text)
        if m:
            ms = int(m.group(1).replace("_", ""))
            state.default_timeout_seconds = ms // 1000

    return state


def detect_go(verbose: bool) -> DetectedState:
    """Detect capabilities from shatter-go/protocol/constants.go and handler.go."""
    state = DetectedState()

    # CommandCapabilities from constants.go
    if GO_CONSTANTS.exists():
        const_text = GO_CONSTANTS.read_text()
        m = re.search(r"CommandCapabilities\s*=\s*\[\]string\{([^}]+)\}", const_text, re.DOTALL)
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
        # Find handleHandshake function and extract complex_type: items
        hh_match = re.search(
            r"func \(\w+ \*Handler\) handleHandshake\b.*?^}", handler_text,
            re.DOTALL | re.MULTILINE,
        )
        block = hh_match.group(0) if hh_match else handler_text
        for item in re.findall(r'"complex_type:([^"]+)"', block):
            state.complex_types.add(item)
    elif verbose:
        print(f"  [go] handler file not found: {GO_HANDLER}")

    # Default timeout from executor.go (defaultExecTimeout = N * time.Second)
    if GO_EXECUTOR.exists():
        ex_text = GO_EXECUTOR.read_text()
        m = re.search(r"defaultExecTimeout\s*=\s*(\d+)\s*\*\s*time\.Second", ex_text)
        if m:
            state.default_timeout_seconds = int(m.group(1))
    elif verbose:
        print(f"  [go] executor file not found: {GO_EXECUTOR}")

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
    # Find handle_handshake function and extract capability strings
    hh_match = re.search(
        r"fn handle_handshake\b.*?^\s*\}", text, re.DOTALL | re.MULTILINE
    )
    block = hh_match.group(0) if hh_match else text

    for item in re.findall(r'"([^"]+)"\.to_string\(\)', block):
        if item.startswith("complex_type:"):
            state.complex_types.add(item.removeprefix("complex_type:"))
        elif item not in ("handshake", "shutdown"):
            state.commands.add(item)

    # DEFAULT_EXEC_TIMEOUT_MS: u64 = NNNN;
    m = re.search(r"DEFAULT_EXEC_TIMEOUT_MS\s*:\s*u64\s*=\s*(\d+)", text)
    if m:
        state.default_timeout_seconds = int(m.group(1)) // 1000

    return state


DETECTORS = {
    "typescript": detect_typescript,
    "go": detect_go,
    "rust": detect_rust,
}


# ---------------------------------------------------------------------------
# Validation logic
# ---------------------------------------------------------------------------

def validate_frontend(
    name: str,
    matrix_def: dict,
    detected: DetectedState,
    registry_caps: dict[str, set[str]],
    result: Result,
    verbose: bool,
) -> None:
    required_cmds = set(matrix_def.get("required", {}).get("commands", []))
    required_types = set(matrix_def.get("required", {}).get("complex_types", []))
    optional_cmds = set(matrix_def.get("optional", {}).get("commands", []))
    optional_types = set(matrix_def.get("optional", {}).get("complex_types", []))
    documented_cmds = required_cmds | optional_cmds
    documented_types = required_types | optional_types
    contract_timeout = matrix_def.get("default_timeout_seconds")

    # --- Validate matrix entries are known to the registry ---
    valid_cmds = registry_caps.get("command", set())
    valid_types = registry_caps.get("complex_type", set())

    if valid_cmds:
        for cmd in sorted(documented_cmds):
            if cmd not in valid_cmds:
                result.error(
                    f"[{name}] Matrix references unknown command '{cmd}' "
                    f"(not in registry.yaml capabilities.command)"
                )
    if valid_types:
        for t in sorted(documented_types):
            if t not in valid_types:
                result.error(
                    f"[{name}] Matrix references unknown complex_type '{t}' "
                    f"(not in registry.yaml capabilities.complex_type)"
                )

    # --- Missing required capabilities ---
    for cmd in sorted(required_cmds - detected.commands):
        result.error(
            f"[{name}] MISSING REQUIRED command '{cmd}': "
            f"required by parity contract but not detected in handler code"
        )
    for t in sorted(required_types - detected.complex_types):
        result.error(
            f"[{name}] MISSING REQUIRED complex_type '{t}': "
            f"required by parity contract but not detected in handler code"
        )

    # --- Undocumented capabilities (in code but not in matrix) ---
    for cmd in sorted(detected.commands - documented_cmds):
        result.error(
            f"[{name}] UNDOCUMENTED command '{cmd}': "
            f"advertised by handler but not listed in parity matrix"
        )
    for t in sorted(detected.complex_types - documented_types):
        result.error(
            f"[{name}] UNDOCUMENTED complex_type '{t}': "
            f"advertised by handler but not listed in parity matrix"
        )

    # --- Optional capability gaps (warnings only) ---
    for cmd in sorted(optional_cmds - detected.commands):
        result.warn(
            f"[{name}] optional command '{cmd}' not implemented (expected, no action needed)"
        )
    for t in sorted(optional_types - detected.complex_types):
        result.warn(
            f"[{name}] optional complex_type '{t}' not implemented (expected, no action needed)"
        )

    # --- Default timeout contract ---
    if contract_timeout is not None and detected.default_timeout_seconds is not None:
        if detected.default_timeout_seconds != contract_timeout:
            result.error(
                f"[{name}] DEFAULT TIMEOUT MISMATCH: "
                f"parity contract says {contract_timeout}s but code defaults to "
                f"{detected.default_timeout_seconds}s — "
                f"update parity-matrix.yaml or the frontend source"
            )
    elif contract_timeout is None and verbose:
        print(f"  [{name}] no default_timeout_seconds in matrix — skipping timeout check")
    elif detected.default_timeout_seconds is None and verbose:
        print(f"  [{name}] could not detect default timeout from source — skipping timeout check")

    if verbose:
        print(f"  [{name}] detected commands: {sorted(detected.commands)}")
        print(f"  [{name}] detected complex_types: {sorted(detected.complex_types)}")
        if detected.default_timeout_seconds is not None:
            print(f"  [{name}] detected default_timeout_seconds: {detected.default_timeout_seconds}")


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
        "--verbose", "-v", action="store_true", help="Show detected capabilities per frontend"
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    verbose: bool = args.verbose
    matrix_path: Path = args.matrix

    print(f"Loading parity matrix: {matrix_path}")
    matrix = load_matrix(matrix_path)

    schema_ver = matrix.get("schema_version", "?")
    proto_ver = matrix.get("protocol_version", "?")
    frontends_def: dict = matrix.get("frontends", {})
    print(
        f"Matrix: schema_version={schema_ver}, protocol_version={proto_ver}, "
        f"frontends={list(frontends_def.keys())}"
    )

    print(f"\nLoading registry capabilities: {REGISTRY_PATH}")
    registry_caps = load_registry_capabilities(REGISTRY_PATH)
    if verbose:
        print(
            f"  Registry: {len(registry_caps['command'])} commands, "
            f"{len(registry_caps['complex_type'])} complex_types"
        )

    result = Result()

    for frontend_name, frontend_def in frontends_def.items():
        print(f"\nChecking frontend: {frontend_name}")

        detector = DETECTORS.get(frontend_name)
        if detector is None:
            result.error(
                f"[{frontend_name}] No detector implemented for this frontend name. "
                f"Known frontends: {list(DETECTORS)}"
            )
            continue

        detected = detector(verbose)
        validate_frontend(
            frontend_name, frontend_def, detected, registry_caps, result, verbose
        )

    # --- Report ---
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
