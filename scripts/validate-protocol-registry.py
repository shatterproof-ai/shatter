#!/usr/bin/env python3
"""Validate that protocol/registry.yaml matches the source-of-truth code.

Two layers of validation:

1. **IDL field-model layer** (str-1hlk.5). Verifies that every command in
   the registry carries a structured `field_model` block, that every
   `enum:<name>` reference resolves to a top-level `enums:` entry, and
   that legacy enum mirrors stay in sync with their authoritative
   counterparts under `enums:`. The legacy flat `fields:` list must equal
   the keys of `field_model.request_fields`.

2. **Source-name layer** (legacy). Two independent checks per frontend:
     - **Vocabulary**: the frontend's codegen-generated enum module (the
       only place command/status/error-code vocabulary is defined) must
       match the registry exactly. Any mismatch means codegen is stale —
       hard error.
     - **Implemented commands**: the commands each frontend's dispatch
       site (`match`/`switch` on the command string) actually handles.
       A dispatched command absent from the registry is a hard error
       (undeclared protocol surface); a registry command with no dispatch
       arm is only a warning (may be legitimately unimplemented by that
       frontend).
   Checked against:
     - shatter-core/src/protocol.rs                    (Rust core, authoritative enum)
     - shatter-ts/src/generated/protocol-enums.ts       (TS vocabulary)
     - shatter-ts/src/handlers.ts                       (TS implemented commands)
     - shatter-go/protocol/protocol_enums_gen.go        (Go vocabulary)
     - shatter-go/protocol/handler.go                   (Go implemented commands)
     - shatter-rust/src/generated/protocol_enums.rs     (Rust frontend vocabulary)
     - shatter-rust/src/handler.rs                      (Rust frontend implemented commands)

An extractor that finds nothing (a stale regex/pattern) fails loudly rather
than silently skipping validation.

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
TS_GENERATED_ENUMS = REPO_ROOT / "shatter-ts" / "src" / "generated" / "protocol-enums.ts"
TS_HANDLERS = REPO_ROOT / "shatter-ts" / "src" / "handlers.ts"
GO_GENERATED_ENUMS = REPO_ROOT / "shatter-go" / "protocol" / "protocol_enums_gen.go"
GO_HANDLER = REPO_ROOT / "shatter-go" / "protocol" / "handler.go"
RUST_FE_GENERATED_ENUMS = REPO_ROOT / "shatter-rust" / "src" / "generated" / "protocol_enums.rs"
RUST_FE_HANDLER = REPO_ROOT / "shatter-rust" / "src" / "handler.rs"


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


def _extract_braced_block(text: str, header_pattern: str) -> str:
    """Return the text of the brace-delimited block introduced by `header_pattern`.

    Finds the header, then balance-counts braces from the header's opening
    `{` to its matching `}`. Used to scope dispatch extraction to a single
    switch/match block so unrelated `case`/`=>` arms elsewhere in the file
    are not picked up.
    """
    m = re.search(header_pattern, text)
    if not m:
        return ""
    brace_start = text.index("{", m.end() - 1)
    depth = 0
    for i in range(brace_start, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[brace_start : i + 1]
    return text[brace_start:]


def extract_ts_vocab(path: Path) -> dict:
    """Extract commands, statuses, error codes from the TS generated enum module.

    This module is codegen output (str-1hlk.7) — the only place TS command/
    status/error-code vocabulary is defined. shatter-ts/src/protocol.ts just
    re-exports it, so reading it directly (rather than protocol.ts) is what
    keeps this check from silently validating nothing.
    """
    text = path.read_text()
    return {
        "commands": _extract_ts_const_list(text, "ALL_COMMANDS"),
        "statuses": _extract_ts_const_list(text, "ALL_RESPONSE_STATUSES"),
        "error_codes": _extract_ts_const_list(text, "ALL_ERROR_CODES"),
    }


def _extract_ts_const_list(text: str, const_name: str) -> set[str]:
    pattern = rf"{const_name}\s*=\s*\[(.*?)\]\s*as const;"
    m = re.search(pattern, text, re.DOTALL)
    if not m:
        return set()
    return set(re.findall(r'"([^"]+)"', m.group(1)))


def extract_ts_implemented_commands(path: Path) -> set[str]:
    """Extract commands actually dispatched by shatter-ts's top-level command switch."""
    text = path.read_text()
    block = _extract_braced_block(text, r"switch\s*\(request\.command\)\s*\{")
    return set(re.findall(r'^\s*case "([a-z_]+)":', block, re.MULTILINE))


def extract_go_vocab(path: Path) -> dict:
    """Extract commands, statuses, error codes from the Go generated enum module."""
    text = path.read_text()
    return {
        "commands": _extract_go_const_list(text, "AllCommands"),
        "statuses": _extract_go_const_list(text, "AllResponseStatuses"),
        "error_codes": _extract_go_const_list(text, "AllErrorCodes"),
    }


def _extract_go_const_list(text: str, const_name: str) -> set[str]:
    pattern = rf"{const_name}\s*=\s*\[\]string\{{(.*?)\}}"
    m = re.search(pattern, text, re.DOTALL)
    if not m:
        return set()
    return set(re.findall(r'"([^"]+)"', m.group(1)))


def extract_go_implemented_commands(path: Path) -> set[str]:
    """Extract commands actually dispatched by shatter-go's top-level command switch."""
    text = path.read_text()
    block = _extract_braced_block(text, r"switch\s+req\.Command\s*\{")
    return set(re.findall(r'^\s*case "([a-z_]+)":', block, re.MULTILINE))


def extract_rust_fe_vocab(path: Path) -> dict:
    """Extract commands, statuses, error codes from the Rust frontend's generated enum module."""
    text = path.read_text()
    return {
        "commands": _extract_rust_const_list(text, "ALL_COMMANDS"),
        "statuses": _extract_rust_const_list(text, "ALL_RESPONSE_STATUSES"),
        "error_codes": _extract_rust_const_list(text, "ALL_ERROR_CODES"),
    }


def _extract_rust_const_list(text: str, const_name: str) -> set[str]:
    pattern = rf"{const_name}:\s*&\[&str\]\s*=\s*&\[(.*?)\];"
    m = re.search(pattern, text, re.DOTALL)
    if not m:
        return set()
    return set(re.findall(r'"([^"]+)"', m.group(1)))


def extract_rust_fe_implemented_commands(path: Path) -> set[str]:
    """Extract commands actually dispatched by shatter-rust's command match."""
    text = path.read_text()
    block = _extract_braced_block(text, r"match\s+req\.command\.as_str\(\)\s*\{")
    return set(re.findall(r'^\s*"([a-z_]+)" =>', block, re.MULTILINE))


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
    """Compare registry entries against a source; return list of issues.

    Used for the Rust core (authoritative) and for frontend vocabulary
    (codegen-generated, so it should exactly mirror the registry). Every
    mismatch here is a hard error — see `is_error` in `main`.
    """
    issues: list[str] = []
    for category in ("commands", "statuses", "error_codes"):
        reg_set = registry[category]
        src_set = source.get(category, set())

        missing_from_registry = src_set - reg_set
        missing_from_source = reg_set - src_set

        for item in sorted(missing_from_registry):
            issues.append(
                f"  {category}: '{item}' found in {source_name} but missing from registry"
            )
        for item in sorted(missing_from_source):
            issues.append(
                f"  {category}: '{item}' in registry but missing from {source_name}"
            )
    return issues


def validate_implemented_commands(
    registry: dict, source_name: str, implemented: set[str]
) -> list[str]:
    """Compare a frontend's dispatched commands against the registry.

    A command the frontend dispatches but the registry doesn't declare is a
    hard error (undeclared protocol surface). A registry command with no
    dispatch arm is only informational — the frontend may legitimately not
    implement every command (see `is_error` in `main`).
    """
    issues: list[str] = []
    reg_set = registry["commands"]

    for item in sorted(implemented - reg_set):
        issues.append(
            f"  commands: '{item}' found in {source_name} but missing from registry"
        )
    for item in sorted(reg_set - implemented):
        issues.append(
            f"  commands: '{item}' in registry but not found in {source_name} (may be unimplemented)"
        )
    return issues


def _require_nonempty(extracted: dict | set, file_path: Path, extractor_name: str) -> str | None:
    """Return an error message if extraction found nothing (a broken pattern), else None.

    A dict is treated as empty if any of its category sets is empty; a bare
    set is treated as empty if it has no members. Silently continuing past
    an empty extraction is exactly the TS-vocabulary bug this validator
    exists to catch (str-qwua7.7) — fail loud instead.
    """
    if isinstance(extracted, dict):
        empty_categories = [cat for cat, values in extracted.items() if not values]
        if empty_categories:
            return (
                f"ERROR: {extractor_name} found nothing for {', '.join(empty_categories)} "
                f"in {file_path} — the extraction pattern no longer matches this file's "
                f"structure and needs to be updated"
            )
        return None
    if not extracted:
        return (
            f"ERROR: {extractor_name} found no entries in {file_path} — the "
            f"extraction pattern no longer matches this file's structure and "
            f"needs to be updated"
        )
    return None


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
        empty_err = _require_nonempty(core, CORE_PROTOCOL, "extract_rust_core")
        if empty_err:
            print(empty_err)
            return 1
        issues = validate(registry, "shatter-core", core)
        if issues:
            all_issues.append("shatter-core/src/protocol.rs:")
            all_issues.extend(issues)
    else:
        all_issues.append(f"WARNING: {CORE_PROTOCOL} not found")

    # --- Frontend vocabulary + implemented-commands checks ---
    frontends = [
        (
            "shatter-ts",
            TS_GENERATED_ENUMS,
            extract_ts_vocab,
            TS_HANDLERS,
            extract_ts_implemented_commands,
        ),
        (
            "shatter-go",
            GO_GENERATED_ENUMS,
            extract_go_vocab,
            GO_HANDLER,
            extract_go_implemented_commands,
        ),
        (
            "shatter-rust",
            RUST_FE_GENERATED_ENUMS,
            extract_rust_fe_vocab,
            RUST_FE_HANDLER,
            extract_rust_fe_implemented_commands,
        ),
    ]

    for name, vocab_path, vocab_extractor, handler_path, impl_extractor in frontends:
        if not vocab_path.exists():
            all_issues.append(f"WARNING: {vocab_path} not found")
        else:
            vocab = vocab_extractor(vocab_path)
            empty_err = _require_nonempty(vocab, vocab_path, vocab_extractor.__name__)
            if empty_err:
                print(empty_err)
                return 1
            issues = validate(registry, f"{name} generated vocabulary", vocab)
            if issues:
                all_issues.append(f"{vocab_path.relative_to(REPO_ROOT)}:")
                all_issues.extend(issues)

        if not handler_path.exists():
            all_issues.append(f"WARNING: {handler_path} not found")
        else:
            implemented = impl_extractor(handler_path)
            empty_err = _require_nonempty(implemented, handler_path, impl_extractor.__name__)
            if empty_err:
                print(empty_err)
                return 1
            issues = validate_implemented_commands(registry, name, implemented)
            if issues:
                all_issues.append(f"{handler_path.relative_to(REPO_ROOT)} (implemented commands):")
                all_issues.extend(issues)

    # --- Report ---
    if all_issues:
        # Separate hard errors from informational warnings.
        def is_error(line: str) -> bool:
            if "missing from registry" in line:
                return True
            if "missing from" in line and "unimplemented" not in line:
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
