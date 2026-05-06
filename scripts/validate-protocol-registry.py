#!/usr/bin/env python3
"""Validate that protocol/registry.yaml matches the source-of-truth code.

Two layers of validation:

1. **IDL field-model layer** (str-1hlk.5). Verifies that every command in
   the registry carries a structured `field_model` block, that every
   `enum:<name>` reference resolves to a top-level `enums:` entry, and
   that legacy enum mirrors stay in sync with their authoritative
   counterparts under `enums:`. The legacy flat `fields:` list must equal
   the keys of `field_model.request_fields`.

2. **Source-name layer** (legacy). Cross-checks command, response-status,
   and error-code names against:
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
# IDL field-model layer (str-1hlk.5)
# ---------------------------------------------------------------------------
#
# We avoid taking on a PyYAML dependency, so this is a small structured-YAML
# loader scoped to the shapes the registry actually uses: top-level mappings,
# nested mappings under known sections, inline-flow `{key: value, ...}`
# entries, simple scalars, and bracketed flow lists. The loader is
# intentionally narrow; it is not a general YAML parser.

# Type-syntax recognized in field models. Anything else is treated as a
# semantic error.
_TYPE_PRIMITIVES = frozenset({
    "string", "integer", "number", "boolean", "any", "object",
})


def load_registry_structured(path: Path) -> dict:
    """Parse the registry into a nested dict.

    Supported syntax (sufficient for registry.yaml):
      - top-level mappings (`section:` followed by indented entries)
      - nested mappings, recursively
      - inline-flow mappings on a single line: `{ key: value, key: value }`
      - bracketed flow lists: `[a, b, c]`
      - scalar values (int / quoted-string / bareword)
      - YAML block list items (`- value`) under a key

    Comments (`#`) and blank lines are ignored.
    """
    text = path.read_text()
    lines = _strip_comments(text)
    cursor = _Cursor(lines)
    return _parse_block(cursor, indent=0)


class _Cursor:
    def __init__(self, lines: list[tuple[int, str]]) -> None:
        self.lines = lines
        self.pos = 0

    def peek(self) -> tuple[int, str] | None:
        if self.pos >= len(self.lines):
            return None
        return self.lines[self.pos]

    def advance(self) -> None:
        self.pos += 1


def _strip_comments(text: str) -> list[tuple[int, str]]:
    """Return [(indent, content)] for non-blank, non-comment lines."""
    out: list[tuple[int, str]] = []
    for raw in text.splitlines():
        # Strip trailing comments (a '#' not inside quotes).
        stripped = _strip_trailing_comment(raw).rstrip()
        if not stripped:
            continue
        if stripped.lstrip().startswith("#"):
            continue
        indent = len(stripped) - len(stripped.lstrip(" "))
        out.append((indent, stripped.lstrip(" ")))
    return out


def _strip_trailing_comment(line: str) -> str:
    in_single = False
    in_double = False
    for i, ch in enumerate(line):
        if ch == "'" and not in_double:
            in_single = not in_single
        elif ch == '"' and not in_single:
            in_double = not in_double
        elif ch == "#" and not in_single and not in_double:
            return line[:i]
    return line


def _parse_block(cursor: _Cursor, indent: int) -> dict:
    """Parse a mapping whose entries are indented at `indent` spaces."""
    result: dict = {}
    while True:
        nxt = cursor.peek()
        if nxt is None:
            break
        line_indent, content = nxt
        if line_indent < indent:
            break
        if line_indent > indent:
            # Stray deeper line — let outer caller handle, or skip.
            cursor.advance()
            continue
        cursor.advance()
        # Block list item under a key
        if content.startswith("- "):
            # Caller should have collected list via _parse_list; surface as scalar.
            result.setdefault("__items__", []).append(_parse_scalar(content[2:].strip()))
            continue
        key, _, rest = content.partition(":")
        key = key.strip()
        rest = rest.strip()
        if not rest:
            # Look ahead: nested mapping, block list, or empty value.
            child = cursor.peek()
            if child is None:
                result[key] = {}
                continue
            child_indent, child_content = child
            if child_indent <= indent:
                result[key] = {}
                continue
            if child_content.startswith("- "):
                result[key] = _parse_block_list(cursor, child_indent)
            else:
                result[key] = _parse_block(cursor, child_indent)
        else:
            result[key] = _parse_value(rest)
    return result


def _parse_block_list(cursor: _Cursor, indent: int) -> list:
    items: list = []
    while True:
        nxt = cursor.peek()
        if nxt is None:
            break
        line_indent, content = nxt
        if line_indent < indent or not content.startswith("- "):
            break
        cursor.advance()
        item_text = content[2:].strip()
        if ":" in item_text and not item_text.startswith(("{", "[")):
            # Inline mapping after `- `: collect into a single-entry dict
            # plus any following indented continuation.
            inline = _parse_inline_mapping_line(item_text)
            # Indented continuation lines belong to this item.
            child = cursor.peek()
            if child is not None and child[0] > indent:
                continuation = _parse_block(cursor, child[0])
                inline.update(continuation)
            items.append(inline)
        else:
            items.append(_parse_value(item_text))
    return items


def _parse_inline_mapping_line(text: str) -> dict:
    """Parse `key: value` (and possibly a trailing flow mapping)."""
    if text.startswith("{") and text.endswith("}"):
        return _parse_flow_mapping(text)
    key, _, rest = text.partition(":")
    return {key.strip(): _parse_value(rest.strip())}


def _parse_value(text: str):
    text = text.strip()
    if not text:
        return None
    if text.startswith("{") and text.endswith("}"):
        return _parse_flow_mapping(text)
    if text.startswith("[") and text.endswith("]"):
        return _parse_flow_list(text)
    return _parse_scalar(text)


def _parse_flow_mapping(text: str) -> dict:
    inner = text[1:-1].strip()
    if not inner:
        return {}
    out: dict = {}
    for entry in _split_flow(inner):
        key, _, val = entry.partition(":")
        out[key.strip()] = _parse_value(val.strip())
    return out


def _parse_flow_list(text: str) -> list:
    inner = text[1:-1].strip()
    if not inner:
        return []
    return [_parse_scalar(part.strip()) for part in _split_flow(inner)]


def _split_flow(text: str) -> list[str]:
    """Split on commas that are not inside nested {}/[] or quotes."""
    parts: list[str] = []
    depth = 0
    in_single = False
    in_double = False
    start = 0
    for i, ch in enumerate(text):
        if ch == "'" and not in_double:
            in_single = not in_single
        elif ch == '"' and not in_single:
            in_double = not in_double
        elif not in_single and not in_double:
            if ch in "{[":
                depth += 1
            elif ch in "}]":
                depth -= 1
            elif ch == "," and depth == 0:
                parts.append(text[start:i])
                start = i + 1
    parts.append(text[start:])
    return [p.strip() for p in parts if p.strip()]


def _parse_scalar(text: str):
    text = text.strip()
    if (text.startswith('"') and text.endswith('"')) or (
        text.startswith("'") and text.endswith("'")
    ):
        return text[1:-1]
    if text == "true":
        return True
    if text == "false":
        return False
    if text == "null" or text == "~":
        return None
    try:
        return int(text)
    except ValueError:
        pass
    try:
        return float(text)
    except ValueError:
        pass
    return text


def validate_field_model(registry_path: Path) -> list[str]:
    """Run the IDL field-model checks. Returns a list of error messages."""
    errors: list[str] = []
    doc = load_registry_structured(registry_path)

    enums = doc.get("enums", {}) or {}
    if not isinstance(enums, dict):
        errors.append("registry: top-level `enums:` is not a mapping")
        return errors

    # Validate each enum has a `values:` list.
    for enum_name, enum_def in enums.items():
        if not isinstance(enum_def, dict):
            errors.append(f"enums.{enum_name}: not a mapping")
            continue
        values = enum_def.get("values")
        if not isinstance(values, list) or not values:
            errors.append(
                f"enums.{enum_name}: missing or empty `values:` list"
            )

    # Mirror invariant: `mirror_of` legacy keys must match enum values.
    for enum_name, enum_def in enums.items():
        if not isinstance(enum_def, dict):
            continue
        mirror_target = enum_def.get("mirror_of")
        if not mirror_target:
            continue
        legacy = doc.get(mirror_target)
        if legacy is None:
            errors.append(
                f"enums.{enum_name}: mirror_of references missing legacy key "
                f"`{mirror_target}`"
            )
            continue
        legacy_values = _legacy_enum_values(legacy)
        enum_values = enum_def.get("values", [])
        if sorted(map(str, legacy_values)) != sorted(map(str, enum_values)):
            errors.append(
                f"enums.{enum_name}: drift from legacy `{mirror_target}`. "
                f"enum values={enum_values} legacy values={legacy_values}"
            )

    # Validate every command has a field_model.
    commands = doc.get("commands", {}) or {}
    if not isinstance(commands, dict):
        errors.append("registry: top-level `commands:` is not a mapping")
        return errors

    for cmd_name, cmd in commands.items():
        if not isinstance(cmd, dict):
            errors.append(f"commands.{cmd_name}: not a mapping")
            continue
        field_model = cmd.get("field_model")
        if field_model is None:
            errors.append(
                f"commands.{cmd_name}: missing required `field_model:` block"
            )
            continue
        if not isinstance(field_model, dict):
            errors.append(f"commands.{cmd_name}.field_model: not a mapping")
            continue
        request_fields = field_model.get("request_fields")
        response_fields = field_model.get("response_fields")
        if request_fields is None:
            errors.append(
                f"commands.{cmd_name}.field_model: missing `request_fields:`"
            )
            request_fields = {}
        if response_fields is None:
            errors.append(
                f"commands.{cmd_name}.field_model: missing `response_fields:`"
            )
            response_fields = {}

        # Per-field shape and enum-reference validation.
        for section_name, section in (
            ("request_fields", request_fields),
            ("response_fields", response_fields),
        ):
            if not isinstance(section, dict):
                errors.append(
                    f"commands.{cmd_name}.field_model.{section_name}: not a mapping"
                )
                continue
            for fname, fdef in section.items():
                errors.extend(
                    _validate_field_def(
                        f"commands.{cmd_name}.field_model.{section_name}.{fname}",
                        fdef,
                        enums,
                    )
                )

        # Superset rule: legacy flat `fields:` must equal request_fields.keys().
        flat_fields = cmd.get("fields")
        if isinstance(flat_fields, list) and isinstance(request_fields, dict):
            flat_set = {str(f) for f in flat_fields}
            model_set = set(request_fields.keys())
            extras_in_flat = flat_set - model_set
            extras_in_model = model_set - flat_set
            for extra in sorted(extras_in_flat):
                errors.append(
                    f"commands.{cmd_name}.fields: '{extra}' is not declared "
                    f"in field_model.request_fields"
                )
            for extra in sorted(extras_in_model):
                errors.append(
                    f"commands.{cmd_name}.fields: missing '{extra}' "
                    f"(declared in field_model.request_fields)"
                )

    return errors


def _legacy_enum_values(legacy) -> list[str]:
    """Extract string values from a legacy enum mirror.

    Handles three observed shapes in registry.yaml:
      - flat list of strings: `[a, b, c]`
      - block list of strings: `- a\\n- b`
      - block list of mappings each carrying an `id:` key (branch_types).
    """
    out: list[str] = []
    if isinstance(legacy, list):
        for item in legacy:
            if isinstance(item, dict) and "id" in item:
                out.append(str(item["id"]))
            else:
                out.append(str(item))
    return out


def _validate_field_def(path_label: str, fdef, enums: dict) -> list[str]:
    """Validate a single field model entry: must be `{type, optional, ...}`."""
    errors: list[str] = []
    if not isinstance(fdef, dict):
        errors.append(f"{path_label}: not a mapping")
        return errors
    if "type" not in fdef:
        errors.append(f"{path_label}: missing `type`")
        return errors
    if "optional" not in fdef:
        errors.append(f"{path_label}: missing `optional`")
    type_str = str(fdef["type"])
    errors.extend(_validate_type_expression(path_label, type_str, enums))
    return errors


def _validate_type_expression(path_label: str, type_str: str, enums: dict) -> list[str]:
    """Recursively validate a field type expression."""
    type_str = type_str.strip()
    if type_str in _TYPE_PRIMITIVES:
        return []
    if type_str.startswith("array<") and type_str.endswith(">"):
        inner = type_str[len("array<"):-1]
        return _validate_type_expression(path_label, inner, enums)
    if type_str.startswith("enum:"):
        enum_name = type_str[len("enum:"):]
        if enum_name not in enums:
            return [
                f"{path_label}: type `{type_str}` references undefined enum "
                f"(define under top-level `enums:`)"
            ]
        return []
    if type_str.startswith("ref:"):
        # `ref:` targets are validated structurally only; the schema files
        # themselves are validated by protocol/schemas/test_schema_validation.py.
        return []
    return [
        f"{path_label}: unrecognized type `{type_str}` "
        f"(expected primitive, array<...>, enum:<name>, or ref:<schema>)"
    ]


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

    # Layer 1: IDL field-model validation. Hard-fail on any issue here
    # before falling through to source-name parity, because a malformed
    # registry would produce confusing downstream messages.
    field_model_errors = validate_field_model(REGISTRY_PATH)
    if field_model_errors:
        print("ERRORS (registry IDL field-model):")
        for err in field_model_errors:
            print(f"  {err}")
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
