#!/usr/bin/env python3
"""Validate frontend capabilities against the checked-in parity contract.

Fails (exit 1) if any of the following are detected:
  - The parity matrix YAML contains duplicate mapping keys
  - A command/type with status=implemented for a frontend is absent from
    that frontend's handler code (MISSING_REQUIRED)
  - A frontend's handler advertises a command/type that the matrix marks
    as not_implemented for that frontend (UNDOCUMENTED)
  - A command or complex_type detected in handler code is not present in
    the parity matrix at all (UNDOCUMENTED)
  - An allowed_divergences entry is missing required metadata
    (id/description/status/owner/tracking_issue/resolution_condition;
    resolved entries also require resolved_at) (str-1hlk.12)
  - A divergence id appears in protocol/PARITY.md but not in
    parity-matrix.yaml, or vice versa (silent drift)
  - A resolved divergence has a resolved_at date older than
    DRIFT_RESOLUTION_GRACE_DAYS (default 30)

Emits warnings (not failures) if:
  - A mismatch is covered by an entry in allowed_divergences with
    status: accepted or status: tracked
  - A resolved divergence is within the grace window but not yet removed

The --warn-as-error-within-days N flag escalates that last warning to a
failure (exit 1) once a resolved entry is within N days of its removal
deadline. The scheduled parity-expiry workflow (.github/workflows/parity-
expiry.yml) runs with this flag so an impending expiry surfaces as a red
scheduled run — an owner-actionable signal — before the hard deadline first
manifests as a red gate on someone's unrelated parity-touching branch
(str-5dx0).

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
        owner: '...'
        tracking_issue: '<bd-id>' | 'none'   # 'none' allowed only for accepted
        resolution_condition: '...'
        resolved_at: 'YYYY-MM-DD'            # required for resolved entries

Usage:
    python3 scripts/validate-parity.py [--matrix PATH] [--verbose] [--today YYYY-MM-DD]
                                       [--warn-as-error-within-days N]
"""

from __future__ import annotations

import argparse
import datetime as _dt
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent

DEFAULT_MATRIX_PATH = REPO_ROOT / "protocol" / "parity-matrix.yaml"
PARITY_MD_PATH = REPO_ROOT / "protocol" / "PARITY.md"
REGISTRY_PATH = REPO_ROOT / "protocol" / "registry.yaml"

# Frontend source files
TS_HANDLERS = REPO_ROOT / "shatter-ts" / "src" / "handlers.ts"
GO_CONSTANTS = REPO_ROOT / "shatter-go" / "protocol" / "constants.go"
GO_HANDLER = REPO_ROOT / "shatter-go" / "protocol" / "handler.go"
RUST_HANDLER = REPO_ROOT / "shatter-rust" / "src" / "handler.rs"

# str-1hlk.12: how long a `resolved` divergence may linger after its
# resolved_at date before the validator promotes the warning to a failure.
DRIFT_RESOLUTION_GRACE_DAYS = 30

# Required keys on every allowed_divergences entry.
REQUIRED_DIVERGENCE_FIELDS: tuple[str, ...] = (
    "id",
    "description",
    "affected_frontends",
    "affected_commands",
    "status",
    "resolution",
    "owner",
    "tracking_issue",
    "resolution_condition",
)

VALID_DIVERGENCE_STATUSES: frozenset[str] = frozenset(
    ("tracked", "accepted", "resolved")
)

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

    class UniqueKeyLoader(yaml.SafeLoader):
        pass

    def construct_mapping(loader: UniqueKeyLoader, node: object, deep: bool = False) -> dict:
        if not isinstance(node, yaml.MappingNode):
            raise yaml.constructor.ConstructorError(
                None, None, f"expected a mapping node, got {node.id}", node.start_mark
            )

        loader.flatten_mapping(node)
        seen: set[object] = set()
        for key_node, _ in node.value:
            key = loader.construct_object(key_node, deep=deep)
            if key in seen:
                raise yaml.constructor.ConstructorError(
                    "while constructing a mapping",
                    node.start_mark,
                    f"found duplicate key {key!r}",
                    key_node.start_mark,
                )
            seen.add(key)

        return yaml.SafeLoader.construct_mapping(loader, node, deep=deep)

    UniqueKeyLoader.add_constructor(
        yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG,
        construct_mapping,
    )

    try:
        with path.open() as f:
            data = yaml.load(f, Loader=UniqueKeyLoader)
    except yaml.constructor.ConstructorError as exc:
        print(f"ERROR: Invalid YAML in {path}: {exc}", file=sys.stderr)
        sys.exit(1)
    except yaml.YAMLError as exc:
        print(f"ERROR: Could not parse YAML in {path}: {exc}", file=sys.stderr)
        sys.exit(1)

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


def _is_nonempty_str(value: object) -> bool:
    return isinstance(value, str) and value.strip() != ""


def _parse_iso_date(value: object) -> _dt.date | None:
    """Parse a YYYY-MM-DD string. PyYAML may already return a date object."""
    if isinstance(value, _dt.date):
        return value
    if not isinstance(value, str):
        return None
    try:
        return _dt.date.fromisoformat(value.strip())
    except ValueError:
        return None


def parity_md_divergence_ids(parity_md_path: Path) -> set[str] | None:
    """Extract divergence ids from the `### `<id>`` headings in PARITY.md.

    Returns None if PARITY.md cannot be read (validator should still run; the
    sync check just gets skipped with an error).
    """
    if not parity_md_path.exists():
        return None
    text = parity_md_path.read_text()
    # Limit to the "Allowed Divergences" section so unrelated `### `...`` ids
    # elsewhere in the doc do not pollute the set.
    section_match = re.search(
        r"^##\s+Allowed Divergences\s*$", text, re.MULTILINE
    )
    if section_match is None:
        return set()
    section_start = section_match.end()
    next_h2 = re.search(r"^##\s+\S", text[section_start:], re.MULTILINE)
    section_text = (
        text[section_start : section_start + next_h2.start()]
        if next_h2
        else text[section_start:]
    )
    return set(re.findall(r"^###\s+`([^`]+)`\s*$", section_text, re.MULTILINE))


def validate_divergence_metadata(
    allowed: list[dict],
    parity_md_ids: set[str] | None,
    today: _dt.date,
    result: Result,
    grace_days: int = DRIFT_RESOLUTION_GRACE_DAYS,
    warn_as_error_within_days: int | None = None,
) -> None:
    """Enforce required metadata + grace-period rules on allowed_divergences.

    Implements str-1hlk.12 acceptance criteria:
      - Validator fails on divergence entries missing metadata.
      - Resolved drift entries fail after a grace period (DRIFT_RESOLUTION_GRACE_DAYS).
      - PARITY.md and parity-matrix.yaml no longer diverge silently.
    """
    yaml_ids: set[str] = set()
    seen_ids: set[str] = set()

    for idx, div in enumerate(allowed):
        if not isinstance(div, dict):
            result.error(
                f"allowed_divergences[{idx}]: expected a mapping, "
                f"got {type(div).__name__}"
            )
            continue

        div_id = div.get("id")
        id_label = div_id if _is_nonempty_str(div_id) else f"<index {idx}>"

        # Required scalar/list fields
        for required in REQUIRED_DIVERGENCE_FIELDS:
            if required not in div:
                result.error(
                    f"allowed_divergences[{id_label}]: missing required field "
                    f"'{required}'"
                )
                continue
            value = div[required]
            if required in ("affected_frontends", "affected_commands"):
                if not isinstance(value, list) or not value:
                    result.error(
                        f"allowed_divergences[{id_label}]: '{required}' must "
                        f"be a non-empty list"
                    )
            else:
                if not _is_nonempty_str(value):
                    result.error(
                        f"allowed_divergences[{id_label}]: '{required}' must "
                        f"be a non-empty string"
                    )

        # Duplicate id detection
        if _is_nonempty_str(div_id):
            assert isinstance(div_id, str)
            if div_id in seen_ids:
                result.error(
                    f"allowed_divergences[{div_id}]: duplicate id"
                )
            seen_ids.add(div_id)
            yaml_ids.add(div_id)

        status = div.get("status")
        if _is_nonempty_str(status) and status not in VALID_DIVERGENCE_STATUSES:
            assert isinstance(status, str)
            result.error(
                f"allowed_divergences[{id_label}]: unknown status "
                f"'{status}' (expected one of: "
                f"{sorted(VALID_DIVERGENCE_STATUSES)})"
            )

        tracking_issue = div.get("tracking_issue")
        if _is_nonempty_str(tracking_issue):
            assert isinstance(tracking_issue, str)
            normalized_ti = tracking_issue.strip().lower()
            if normalized_ti == "none" and status != "accepted":
                result.error(
                    f"allowed_divergences[{id_label}]: tracking_issue='none' "
                    f"is only allowed for status='accepted' (this entry is "
                    f"status='{status}')"
                )

        # Resolved-entry grace period
        if status == "resolved":
            resolved_at_raw = div.get("resolved_at")
            if resolved_at_raw is None or (
                isinstance(resolved_at_raw, str) and resolved_at_raw.strip() == ""
            ):
                result.error(
                    f"allowed_divergences[{id_label}]: status='resolved' "
                    f"requires 'resolved_at' (YYYY-MM-DD)"
                )
            else:
                resolved_at = _parse_iso_date(resolved_at_raw)
                if resolved_at is None:
                    result.error(
                        f"allowed_divergences[{id_label}]: resolved_at "
                        f"'{resolved_at_raw}' is not a valid YYYY-MM-DD date"
                    )
                else:
                    age_days = (today - resolved_at).days
                    if age_days > grace_days:
                        result.error(
                            f"allowed_divergences[{id_label}]: resolved on "
                            f"{resolved_at.isoformat()} ({age_days} days ago) "
                            f"— grace period of {grace_days} days has expired; "
                            f"remove this entry from parity-matrix.yaml and "
                            f"protocol/PARITY.md"
                        )
                    else:
                        remaining = grace_days - age_days
                        if (
                            warn_as_error_within_days is not None
                            and remaining <= warn_as_error_within_days
                        ):
                            result.error(
                                f"allowed_divergences[{id_label}]: resolved on "
                                f"{resolved_at.isoformat()} — grace period "
                                f"expires in {remaining} day(s); remove this "
                                f"entry from parity-matrix.yaml and "
                                f"protocol/PARITY.md now, before the hard "
                                f"deadline blocks an unrelated parity-touching "
                                f"branch (escalated by "
                                f"--warn-as-error-within-days="
                                f"{warn_as_error_within_days})"
                            )
                        else:
                            result.warn(
                                f"allowed_divergences[{id_label}]: resolved on "
                                f"{resolved_at.isoformat()} — schedule removal "
                                f"within {remaining} day(s)"
                            )

    # PARITY.md ↔ parity-matrix.yaml sync check.
    if parity_md_ids is None:
        result.error(
            f"protocol/PARITY.md not found at {PARITY_MD_PATH} — cannot "
            f"verify divergence id parity with parity-matrix.yaml"
        )
        return

    only_yaml = yaml_ids - parity_md_ids
    only_md = parity_md_ids - yaml_ids
    for div_id in sorted(only_yaml):
        result.error(
            f"divergence '{div_id}' present in parity-matrix.yaml but missing "
            f"from protocol/PARITY.md (silent drift)"
        )
    for div_id in sorted(only_md):
        result.error(
            f"divergence '{div_id}' present in protocol/PARITY.md but missing "
            f"from parity-matrix.yaml (silent drift)"
        )


# ---------------------------------------------------------------------------
# Core validation
# ---------------------------------------------------------------------------

def validate(
    matrix: dict,
    detected_per_frontend: dict[str, DetectedState],
    registry_caps: dict[str, set[str]],
    result: Result,
    verbose: bool,
    today: _dt.date | None = None,
    parity_md_path: Path = PARITY_MD_PATH,
    warn_as_error_within_days: int | None = None,
) -> None:
    allowed_divergences: list[dict] = matrix.get("allowed_divergences", []) or []
    validate_divergence_metadata(
        allowed_divergences,
        parity_md_divergence_ids(parity_md_path),
        today or _dt.date.today(),
        result,
        warn_as_error_within_days=warn_as_error_within_days,
    )
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
    parser.add_argument(
        "--today",
        metavar="YYYY-MM-DD",
        default=None,
        help=(
            "Pin 'today' for the resolved-entry grace-period check "
            "(defaults to system date; tests use this to make assertions "
            "deterministic)"
        ),
    )
    parser.add_argument(
        "--warn-as-error-within-days",
        metavar="N",
        type=int,
        default=None,
        help=(
            "Escalate resolved-divergence grace-window warnings to errors "
            "(exit 1) when an entry is within N days of its removal deadline. "
            "Used by the scheduled parity-expiry workflow to surface an "
            "impending expiry to owners before it blocks an unrelated "
            "parity-touching branch (str-5dx0)."
        ),
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    verbose: bool = args.verbose
    matrix_path: Path = args.matrix

    if args.today:
        try:
            today = _dt.date.fromisoformat(args.today)
        except ValueError:
            print(
                f"ERROR: --today expects YYYY-MM-DD, got {args.today!r}",
                file=sys.stderr,
            )
            return 1
    else:
        today = _dt.date.today()

    warn_as_error_within_days: int | None = args.warn_as_error_within_days
    if warn_as_error_within_days is not None and warn_as_error_within_days < 0:
        print(
            "ERROR: --warn-as-error-within-days must be >= 0, got "
            f"{warn_as_error_within_days}",
            file=sys.stderr,
        )
        return 1

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
    validate(
        matrix, detected_per_frontend, registry_caps, result, verbose, today=today,
        warn_as_error_within_days=warn_as_error_within_days,
    )

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
