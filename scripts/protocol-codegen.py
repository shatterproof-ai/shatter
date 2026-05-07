#!/usr/bin/env python3
"""Protocol codegen driver.

Reads `protocol/registry.yaml` (the authoritative protocol contract) and
emits one or more deterministic artifacts via a registry of per-language
emitters. The skeleton in str-1hlk.6 emitted only the language-agnostic
manifest; str-1hlk.7 adds the TypeScript emitter; str-1hlk.8/.9 will plug
in Go and Rust the same way.

Modes:
  * default / --write : (re)write every emitter's artifact in place.
  * --check           : regenerate every artifact into a temp dir and diff
                        against the checked-in copy. Exit 1 with a unified
                        diff on stderr if any artifact has drifted.

Determinism: keys are sorted, indentation is fixed, line endings are LF,
files end with a single trailing newline, and no timestamps or
environment-derived values are emitted. Each emitter is responsible for
preserving these invariants in its own format.

Adding a new emitter
--------------------
Append a new `Emitter` to `EMITTERS`. Each emitter defines:
  * `name`     — short label used in CLI overrides and diagnostics.
  * `default_out` — repo-relative `Path` of the artifact this emitter owns.
  * `cli_flag` — argparse dest exposing a per-invocation override of
                 `default_out` (kept for back-compat with the str-1hlk.6
                 `--out` flag, which targets the manifest).
  * `render`   — pure function `(registry: dict) -> str` returning the
                 fully-rendered artifact text.
"""

from __future__ import annotations

import argparse
import difflib
import json
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

import yaml

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_REGISTRY = REPO_ROOT / "protocol" / "registry.yaml"
DEFAULT_MANIFEST = REPO_ROOT / "protocol" / "generated" / "manifest.json"
DEFAULT_TS_ENUMS = (
    REPO_ROOT / "shatter-ts" / "src" / "generated" / "protocol-enums.ts"
)
DEFAULT_GO_ENUMS = (
    REPO_ROOT / "shatter-go" / "protocol" / "protocol_enums_gen.go"
)

# Bumped when the manifest *shape* changes (not the registry contents).
MANIFEST_SCHEMA_VERSION = 1


# ---------------------------------------------------------------------------
# Manifest emitter (str-1hlk.6)
# ---------------------------------------------------------------------------


def _sorted_list(values: Any) -> list[str]:
    return sorted(str(v) for v in (values or []))


def build_manifest(registry: dict[str, Any]) -> dict[str, Any]:
    """Project the registry into a stable manifest shape.

    Only fields that are part of the public protocol surface are included.
    Anything that drifts (descriptions, free-form notes) is intentionally
    omitted so the manifest captures contract identity, not prose.
    """
    enums = {
        name: _sorted_list(spec.get("values", []))
        for name, spec in (registry.get("enums") or {}).items()
    }
    commands = {
        name: {
            "fields": _sorted_list(spec.get("fields", [])),
            "response_status": spec.get("response_status"),
        }
        for name, spec in (registry.get("commands") or {}).items()
    }
    statuses = sorted((registry.get("statuses") or {}).keys())
    error_codes = sorted((registry.get("error_codes") or {}).keys())
    return {
        "manifest_schema_version": MANIFEST_SCHEMA_VERSION,
        "protocol_version": registry.get("protocol_version"),
        "compatibility": registry.get("compatibility") or {},
        "enums": dict(sorted(enums.items())),
        "commands": dict(sorted(commands.items())),
        "statuses": statuses,
        "error_codes": error_codes,
    }


def render_manifest(registry: dict[str, Any]) -> str:
    # sort_keys + fixed indent + trailing newline = deterministic bytes.
    return (
        json.dumps(build_manifest(registry), indent=2, sort_keys=True) + "\n"
    )


# ---------------------------------------------------------------------------
# TypeScript enum emitter (str-1hlk.7)
# ---------------------------------------------------------------------------


def _ts_string_literal(value: str) -> str:
    # Registry values are plain ASCII identifiers; a JSON-style escape
    # is sufficient and keeps quoting style canonical.
    return json.dumps(value, ensure_ascii=True)


def _ts_singular_type_name(plural_const_name: str) -> str:
    # ALL_COMMANDS              -> Command
    # ALL_RESPONSE_STATUSES     -> ResponseStatus
    # ALL_ERROR_CODES           -> ErrorCode
    # ALL_SETUP_LEVELS          -> SetupLevel
    # ALL_GENERATOR_KINDS       -> GeneratorKind
    # ALL_BRANCH_TYPES          -> BranchType
    assert plural_const_name.startswith("ALL_"), plural_const_name
    parts = plural_const_name[len("ALL_") :].split("_")
    last = parts[-1]
    # Naive de-pluralisation: drop a trailing "ES" then "S" from the last
    # segment. The registry vocabulary doesn't include irregular plurals,
    # so a hand-written table would only add ceremony. STATUSES -> STATUS,
    # CODES -> CODE, COMMANDS -> COMMAND, LEVELS -> LEVEL, KINDS -> KIND,
    # TYPES -> TYPE.
    if last.endswith("ES") and not last.endswith("DES") and not last.endswith("PES"):
        last = last[:-2]
    elif last.endswith("S"):
        last = last[:-1]
    parts[-1] = last
    return "".join(p.capitalize() for p in parts)


def _ts_const_tuple(name: str, values: list[str], doc: str) -> list[str]:
    """Emit a sorted `as const` tuple plus a derived union type alias.

    Producing both shapes from one source of truth keeps `ALL_<NAME>` and
    the corresponding union type guaranteed-consistent in the generated
    file: there is no place for them to drift apart.
    """
    type_name = _ts_singular_type_name(name)
    out = [f"/** {doc} */", f"export const {name} = ["]
    for v in values:
        out.append(f"  {_ts_string_literal(v)},")
    out.append("] as const;")
    out.append(f"export type {type_name} = (typeof {name})[number];")
    out.append("")
    return out


def _ts_command_values(registry: dict[str, Any]) -> list[str]:
    return sorted((registry.get("commands") or {}).keys())


def _ts_response_status_values(registry: dict[str, Any]) -> list[str]:
    # Every command's response_status, plus the universal "error" status.
    statuses = {
        spec.get("response_status")
        for spec in (registry.get("commands") or {}).values()
        if spec.get("response_status")
    }
    statuses.add("error")
    return sorted(statuses)


def _ts_error_code_values(registry: dict[str, Any]) -> list[str]:
    return sorted((registry.get("error_codes") or {}).keys())


def _ts_enum_values(registry: dict[str, Any], enum_name: str) -> list[str]:
    enums = registry.get("enums") or {}
    spec = enums.get(enum_name)
    if not spec:
        raise SystemExit(
            f"protocol-codegen: registry missing required enum: {enum_name}"
        )
    return sorted(str(v) for v in (spec.get("values") or []))


def render_typescript(registry: dict[str, Any]) -> str:
    protocol_version = registry.get("protocol_version")
    if not isinstance(protocol_version, str):
        raise SystemExit(
            "protocol-codegen: registry missing string protocol_version"
        )

    lines: list[str] = [
        "// AUTO-GENERATED by scripts/protocol-codegen.py — do not edit.",
        "// Source of truth: protocol/registry.yaml",
        "// Regenerate with: python3 scripts/protocol-codegen.py --write",
        "//",
        "// This file is the only place command, response-status, error-code,",
        "// setup-level, generator-kind, and branch-type unions are defined.",
        "// shatter-ts/src/protocol.ts re-exports them so manual drift between",
        "// the registry and the TypeScript frontend is structurally impossible.",
        "",
        "/** Protocol version negotiated between core and the TypeScript frontend. */",
        f"export const PROTOCOL_VERSION = {_ts_string_literal(protocol_version)};",
        "",
    ]
    lines.extend(
        _ts_const_tuple(
            "ALL_COMMANDS",
            _ts_command_values(registry),
            "Every command core may send to a frontend (full registry surface; "
            "individual frontends may decline unsupported commands at runtime).",
        )
    )
    lines.extend(
        _ts_const_tuple(
            "ALL_RESPONSE_STATUSES",
            _ts_response_status_values(registry),
            'Every response status a frontend may emit, including the universal "error" status.',
        )
    )
    lines.extend(
        _ts_const_tuple(
            "ALL_ERROR_CODES",
            _ts_error_code_values(registry),
            "Canonical error codes. Frozen list mirrored from registry.yaml.",
        )
    )
    lines.extend(
        _ts_const_tuple(
            "ALL_SETUP_LEVELS",
            _ts_enum_values(registry, "setup_level"),
            "Setup/teardown lifecycle scope.",
        )
    )
    lines.extend(
        _ts_const_tuple(
            "ALL_GENERATOR_KINDS",
            _ts_enum_values(registry, "generator_kind"),
            "Whether a generate request targets a type name or a parameter name.",
        )
    )
    lines.extend(
        _ts_const_tuple(
            "ALL_BRANCH_TYPES",
            _ts_enum_values(registry, "branch_type"),
            "Source-level construct that produced a branch decision.",
        )
    )
    return "\n".join(lines).rstrip("\n") + "\n"


# ---------------------------------------------------------------------------
# Go enum emitter (str-1hlk.8)
# ---------------------------------------------------------------------------


def _go_const_name(plural_const_name: str) -> str:
    # ALL_COMMANDS -> AllCommands, ALL_RESPONSE_STATUSES -> AllResponseStatuses,
    # ALL_ERROR_CODES -> AllErrorCodes, etc.
    assert plural_const_name.startswith("ALL_"), plural_const_name
    parts = ["ALL", *plural_const_name[len("ALL_") :].split("_")]
    return "".join(p.capitalize() for p in parts)


def _go_string_literal(value: str) -> str:
    # Registry values are plain ASCII; JSON-style escaping yields a valid
    # Go interpreted string literal.
    return json.dumps(value, ensure_ascii=True)


def _go_var_block(name: str, values: list[str], doc: str) -> list[str]:
    """Emit a sorted `var <Name> = []string{...}` slice with a doc comment."""
    var_name = _go_const_name(name)
    out = [f"// {var_name} is {doc}", f"var {var_name} = []string{{"]
    for v in values:
        out.append(f"\t{_go_string_literal(v)},")
    out.append("}")
    out.append("")
    return out


def render_go(registry: dict[str, Any]) -> str:
    protocol_version = registry.get("protocol_version")
    if not isinstance(protocol_version, str):
        raise SystemExit(
            "protocol-codegen: registry missing string protocol_version"
        )

    lines: list[str] = [
        "// Code generated by scripts/protocol-codegen.py; DO NOT EDIT.",
        "// Source of truth: protocol/registry.yaml",
        "// Regenerate with: python3 scripts/protocol-codegen.py --write",
        "//",
        "// This file is the only place command, response-status, error-code,",
        "// setup-level, generator-kind, and branch-type slices are defined",
        "// in shatter-go. Hand-written typed constants in constants.go and",
        "// types.go are reconciled against these slices by",
        "// generated_enums_test.go so manual drift between the registry and",
        "// the Go frontend is caught at test time.",
        "",
        "package protocol",
        "",
        "// ProtocolVersion is the protocol version negotiated between core and",
        "// the Go frontend.",
        f"const ProtocolVersion = {_go_string_literal(protocol_version)}",
        "",
    ]
    lines.extend(
        _go_var_block(
            "ALL_COMMANDS",
            _ts_command_values(registry),
            "every command core may send to a frontend (full registry "
            "surface;\n// individual frontends may decline unsupported "
            "commands at runtime).",
        )
    )
    lines.extend(
        _go_var_block(
            "ALL_RESPONSE_STATUSES",
            _ts_response_status_values(registry),
            'every response status a frontend may emit, including the\n'
            '// universal "error" status.',
        )
    )
    lines.extend(
        _go_var_block(
            "ALL_ERROR_CODES",
            _ts_error_code_values(registry),
            "the canonical error codes. Frozen list mirrored from\n"
            "// registry.yaml.",
        )
    )
    lines.extend(
        _go_var_block(
            "ALL_SETUP_LEVELS",
            _ts_enum_values(registry, "setup_level"),
            "the setup/teardown lifecycle scopes.",
        )
    )
    lines.extend(
        _go_var_block(
            "ALL_GENERATOR_KINDS",
            _ts_enum_values(registry, "generator_kind"),
            "the generator-kind values (type vs parameter name).",
        )
    )
    lines.extend(
        _go_var_block(
            "ALL_BRANCH_TYPES",
            _ts_enum_values(registry, "branch_type"),
            "the source-level constructs that produce branch decisions.",
        )
    )
    return "\n".join(lines).rstrip("\n") + "\n"


# ---------------------------------------------------------------------------
# Emitter registry
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Emitter:
    name: str
    default_out: Path
    render: Callable[[dict[str, Any]], str]
    cli_flag: str  # argparse dest, also doubles as the CLI flag name


EMITTERS: tuple[Emitter, ...] = (
    Emitter(
        name="manifest",
        default_out=DEFAULT_MANIFEST,
        render=render_manifest,
        cli_flag="out",
    ),
    Emitter(
        name="typescript",
        default_out=DEFAULT_TS_ENUMS,
        render=render_typescript,
        cli_flag="ts_out",
    ),
    Emitter(
        name="go",
        default_out=DEFAULT_GO_ENUMS,
        render=render_go,
        cli_flag="go_out",
    ),
)


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------


def load_registry(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as fh:
        data = yaml.safe_load(fh)
    if not isinstance(data, dict):
        raise SystemExit(f"registry at {path} did not parse to a mapping")
    return data


def write_artifact(out: Path, content: str) -> None:
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(content, encoding="utf-8")


def check_artifact(name: str, out: Path, expected: str) -> int:
    """Compare expected content against `out`. Return 0 on match, 1 on drift."""
    if not out.exists():
        sys.stderr.write(
            f"protocol-codegen[{name}]: missing generated artifact: {out}\n"
            "Run `python3 scripts/protocol-codegen.py --write` and commit "
            "the result.\n"
        )
        return 1
    actual = out.read_text(encoding="utf-8")
    if actual == expected:
        return 0
    diff = difflib.unified_diff(
        actual.splitlines(keepends=True),
        expected.splitlines(keepends=True),
        fromfile=f"{out} (checked-in)",
        tofile=f"{out} (regenerated)",
    )
    sys.stderr.write(
        f"protocol-codegen[{name}]: generated artifact is stale.\n"
        "Run `python3 scripts/protocol-codegen.py --write` and commit "
        "the result.\n\n"
    )
    sys.stderr.writelines(diff)
    return 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--registry",
        type=Path,
        default=DEFAULT_REGISTRY,
        help=f"path to protocol registry YAML (default: {DEFAULT_REGISTRY})",
    )
    for emitter in EMITTERS:
        parser.add_argument(
            f"--{emitter.cli_flag.replace('_', '-')}",
            dest=emitter.cli_flag,
            type=Path,
            default=emitter.default_out,
            help=(
                f"path to generated {emitter.name} artifact "
                f"(default: {emitter.default_out})"
            ),
        )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--write",
        action="store_true",
        help="(re)write every emitter's artifact in place (default behaviour)",
    )
    mode.add_argument(
        "--check",
        action="store_true",
        help="regenerate every artifact to a temp dir and diff against the "
        "checked-in copy; exit 1 on drift",
    )
    args = parser.parse_args(argv)

    registry = load_registry(args.registry)

    plan: list[tuple[Emitter, Path, str]] = []
    for emitter in EMITTERS:
        out_path: Path = getattr(args, emitter.cli_flag)
        rendered = emitter.render(registry)
        plan.append((emitter, out_path, rendered))

    if args.check:
        # Round-trip every artifact through a temp dir to prove the in-memory
        # render and an on-disk render agree, then compare against the
        # checked-in file. Aggregate non-zero exits across all emitters so a
        # single CI run reports every drifted artifact at once.
        with tempfile.TemporaryDirectory() as tmp:
            for emitter, _out, rendered in plan:
                scratch = Path(tmp) / f"{emitter.name}.scratch"
                write_artifact(scratch, rendered)
                if scratch.read_bytes().decode("utf-8") != rendered:
                    sys.stderr.write(
                        f"protocol-codegen[{emitter.name}]: internal error — "
                        "temp-dir round-trip did not match in-memory render.\n"
                    )
                    return 2
        rc = 0
        for emitter, out_path, rendered in plan:
            rc |= check_artifact(emitter.name, out_path, rendered)
        return rc

    for _emitter, out_path, rendered in plan:
        write_artifact(out_path, rendered)
    return 0


if __name__ == "__main__":
    sys.exit(main())
