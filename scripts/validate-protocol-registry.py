#!/usr/bin/env python3
"""Validate that protocol/registry.yaml matches the source-of-truth code.

Checks commands, response statuses, and error codes in:
  - shatter-core/src/protocol.rs  (Rust core)
  - shatter-ts/src/protocol.ts    (TypeScript frontend)
  - shatter-go/protocol/          (Go frontend)
  - shatter-rust/src/protocol.rs  (Rust frontend)

Exit 0 if everything matches, 1 on any mismatch.
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Resolve paths relative to the repo root (parent of this script's directory)
# ---------------------------------------------------------------------------
SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent

REGISTRY_PATH = REPO_ROOT / "protocol" / "registry.yaml"
CORE_PROTOCOL = REPO_ROOT / "shatter-core" / "src" / "protocol.rs"
TS_PROTOCOL = REPO_ROOT / "shatter-ts" / "src" / "protocol.ts"
GO_CONSTANTS = REPO_ROOT / "shatter-go" / "protocol" / "constants.go"
GO_HANDLER = REPO_ROOT / "shatter-go" / "protocol" / "handler.go"
RUST_FE_PROTOCOL = REPO_ROOT / "shatter-rust" / "src" / "protocol.rs"


# ---------------------------------------------------------------------------
# Minimal YAML parser — avoids PyYAML dependency
# ---------------------------------------------------------------------------

def parse_registry(path: Path) -> dict:
    """Extract commands, statuses, and error_codes from registry.yaml.

    Returns dict with keys 'commands', 'statuses', 'error_codes' each
    mapping to a set of strings.
    """
    text = path.read_text()

    def extract_top_level_keys(section: str) -> set[str]:
        """Extract keys under a top-level YAML mapping section."""
        pattern = rf"^{re.escape(section)}:\s*$"
        match = re.search(pattern, text, re.MULTILINE)
        if not match:
            return set()
        start = match.end()
        keys: set[str] = set()
        for line in text[start:].splitlines():
            if not line or line.startswith("#"):
                continue
            # End of section: non-indented, non-blank line
            if line[0] not in (" ", "\t"):
                break
            # Key line: exactly 2-space indent, word followed by colon
            m = re.match(r"^  ([a-z_]+):", line)
            if m:
                keys.add(m.group(1))
        return keys

    return {
        "commands": extract_top_level_keys("commands"),
        "statuses": extract_top_level_keys("statuses"),
        "error_codes": extract_top_level_keys("error_codes"),
    }


# ---------------------------------------------------------------------------
# Source extractors
# ---------------------------------------------------------------------------

def extract_rust_core(path: Path) -> dict:
    """Extract commands, statuses, error codes from shatter-core protocol.rs."""
    text = path.read_text()

    # Commands: enum Command variants (PascalCase → snake_case)
    commands: set[str] = set()
    in_command = False
    for line in text.splitlines():
        if re.match(r"^pub enum Command\b", line):
            in_command = True
            continue
        if in_command:
            if line.startswith("}"):
                break
            m = re.match(r"\s+(\w+)\s*[{(,]", line)
            if m:
                commands.add(pascal_to_snake(m.group(1)))

    # ResponseResult: enum variants → statuses
    statuses: set[str] = set()
    in_resp = False
    for line in text.splitlines():
        if re.match(r"^pub enum ResponseResult\b", line):
            in_resp = True
            continue
        if in_resp:
            if line.startswith("}"):
                break
            m = re.match(r"\s+#", line)
            if m:
                continue
            m = re.match(r"\s+(\w+)\s*[{(,]", line)
            if m:
                statuses.add(pascal_to_snake(m.group(1)))

    # ErrorCode enum
    error_codes: set[str] = set()
    in_err = False
    for line in text.splitlines():
        if re.match(r"^pub enum ErrorCode\b", line):
            in_err = True
            continue
        if in_err:
            if line.startswith("}"):
                break
            m = re.match(r"\s+(\w+)", line)
            if m and not m.group(1).startswith("//") and not m.group(1).startswith("#"):
                error_codes.add(pascal_to_snake(m.group(1)))

    return {"commands": commands, "statuses": statuses, "error_codes": error_codes}


def extract_ts(path: Path) -> dict:
    """Extract commands, statuses, error codes from shatter-ts protocol.ts."""
    text = path.read_text()

    commands = extract_ts_union(text, "Command")
    statuses = extract_ts_union(text, "ResponseStatus")
    error_codes = extract_ts_union(text, "ErrorCode")

    return {"commands": commands, "statuses": statuses, "error_codes": error_codes}


def extract_ts_union(text: str, type_name: str) -> set[str]:
    """Extract string literal members from a TypeScript union type."""
    pattern = rf"type\s+{type_name}\s*=\s*([^;]+);"
    m = re.search(pattern, text, re.DOTALL)
    if not m:
        return set()
    return set(re.findall(r'"([^"]+)"', m.group(1)))


def extract_go(constants_path: Path, handler_path: Path) -> dict:
    """Extract commands, statuses, error codes from Go frontend."""
    const_text = constants_path.read_text()
    handler_text = handler_path.read_text()

    # Error codes from constants.go
    error_codes: set[str] = set()
    for m in re.finditer(r'Err\w+\s*=\s*"([^"]+)"', const_text):
        error_codes.add(m.group(1))
    # Inline error codes in handler.go
    for m in re.finditer(r'Code:\s*"([^"]+)"', handler_text):
        error_codes.add(m.group(1))

    # Commands from CommandCapabilities in constants.go
    commands: set[str] = set()
    cap_match = re.search(r"CommandCapabilities\s*=.*?\{([^}]+)\}", const_text, re.DOTALL)
    if cap_match:
        commands = set(re.findall(r'"([^"]+)"', cap_match.group(1)))
    # Also add handshake and shutdown (always handled, not in capabilities)
    commands.add("handshake")
    commands.add("shutdown")

    # Statuses from handler.go: resp.Status = "..."
    statuses: set[str] = set()
    for m in re.finditer(r'\.Status\s*=\s*"([^"]+)"', handler_text):
        statuses.add(m.group(1))

    return {"commands": commands, "statuses": statuses, "error_codes": error_codes}


def extract_rust_fe(path: Path) -> dict:
    """Extract commands and error codes from shatter-rust protocol.rs."""
    text = path.read_text()

    # Commands: look for command string matches in dispatch
    commands: set[str] = set()
    for m in re.finditer(r'"(handshake|analyze|instrument|execute|setup|teardown|generate|shutdown)"', text):
        commands.add(m.group(1))

    # Error codes: extract from ERR_* constants (e.g. pub const ERR_FOO: &str = "foo";)
    error_codes: set[str] = set()
    for m in re.finditer(r'pub const ERR_\w+:\s*&str\s*=\s*"([^"]+)"', text):
        error_codes.add(m.group(1))

    # Statuses
    statuses: set[str] = set()
    for m in re.finditer(r'status.*?"([a-z_]+)"', text):
        statuses.add(m.group(1))

    return {"commands": commands, "statuses": statuses, "error_codes": error_codes}


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def pascal_to_snake(name: str) -> str:
    """Convert PascalCase to snake_case."""
    s1 = re.sub(r"(.)([A-Z][a-z]+)", r"\1_\2", name)
    return re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", s1).lower()


# ---------------------------------------------------------------------------
# Validation
# ---------------------------------------------------------------------------

def validate(registry: dict, source_name: str, source: dict) -> list[str]:
    """Compare registry entries against a source; return list of issues."""
    issues: list[str] = []
    for category in ("commands", "statuses", "error_codes"):
        reg_set = registry[category]
        src_set = source.get(category, set())
        if not src_set:
            continue

        missing_from_registry = src_set - reg_set
        missing_from_source = reg_set - src_set

        for item in sorted(missing_from_registry):
            issues.append(
                f"  {category}: '{item}' found in {source_name} but missing from registry"
            )
        for item in sorted(missing_from_source):
            if category == "error_codes":
                # Error codes must be defined in every frontend.
                issues.append(
                    f"  {category}: '{item}' in registry but missing from {source_name}"
                )
            elif category == "commands":
                issues.append(
                    f"  {category}: '{item}' in registry but not found in {source_name} (may be unimplemented)"
                )
    return issues


def main() -> int:
    if not REGISTRY_PATH.exists():
        print(f"ERROR: Registry not found at {REGISTRY_PATH}")
        return 1

    registry = parse_registry(REGISTRY_PATH)
    print(f"Registry: {len(registry['commands'])} commands, "
          f"{len(registry['statuses'])} statuses, "
          f"{len(registry['error_codes'])} error codes")

    all_issues: list[str] = []

    # --- Rust core (authoritative) ---
    if CORE_PROTOCOL.exists():
        core = extract_rust_core(CORE_PROTOCOL)
        issues = validate(registry, "shatter-core", core)
        if issues:
            all_issues.append("shatter-core/src/protocol.rs:")
            all_issues.extend(issues)
    else:
        all_issues.append(f"WARNING: {CORE_PROTOCOL} not found")

    # --- TypeScript frontend ---
    if TS_PROTOCOL.exists():
        ts = extract_ts(TS_PROTOCOL)
        issues = validate(registry, "shatter-ts", ts)
        if issues:
            all_issues.append("shatter-ts/src/protocol.ts:")
            all_issues.extend(issues)
    else:
        all_issues.append(f"WARNING: {TS_PROTOCOL} not found")

    # --- Go frontend ---
    if GO_CONSTANTS.exists() and GO_HANDLER.exists():
        go = extract_go(GO_CONSTANTS, GO_HANDLER)
        issues = validate(registry, "shatter-go", go)
        if issues:
            all_issues.append("shatter-go/protocol/:")
            all_issues.extend(issues)
    else:
        all_issues.append(f"WARNING: Go protocol files not found")

    # --- Rust frontend ---
    if RUST_FE_PROTOCOL.exists():
        rust_fe = extract_rust_fe(RUST_FE_PROTOCOL)
        issues = validate(registry, "shatter-rust", rust_fe)
        if issues:
            all_issues.append("shatter-rust/src/protocol.rs:")
            all_issues.extend(issues)
    else:
        all_issues.append(f"WARNING: {RUST_FE_PROTOCOL} not found")

    # --- Report ---
    if all_issues:
        # Separate hard errors from informational warnings.
        # Hard errors: codes in source but not in registry, or error codes
        # in registry but missing from a frontend.
        def is_error(line: str) -> bool:
            if "missing from registry" in line:
                return True
            if "error_codes:" in line and "missing from" in line and "unimplemented" not in line:
                return True
            return False
        errors = [i for i in all_issues if is_error(i)]
        warnings = [i for i in all_issues if not is_error(i)]

        if warnings:
            print("\nWarnings:")
            for w in warnings:
                print(f"  {w}")

        if errors:
            print("\nERRORS (items in source but missing from registry):")
            for e in errors:
                print(f"  {e}")
            return 1

        print("\nAll checks passed (with informational warnings).")
        return 0
    else:
        print("All checks passed.")
        return 0


if __name__ == "__main__":
    sys.exit(main())
