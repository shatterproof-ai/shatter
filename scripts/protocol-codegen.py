#!/usr/bin/env python3
"""Skeleton protocol codegen for str-1hlk.6.

Reads `protocol/registry.yaml` (the authoritative protocol contract) and
emits a deterministic manifest summarising the public surface. This is the
framework that the per-language emitters (str-1hlk.7 TS, str-1hlk.8 Go,
str-1hlk.9 Rust) will plug into. For now only one artifact is emitted:
`protocol/generated/manifest.json`.

Modes:
  * default / --write : (re)write the manifest file in place.
  * --check           : regenerate into a temp dir and diff against the
                        checked-in manifest. Exit 1 with a unified diff on
                        stderr if drift is detected.

Determinism: keys are sorted, indentation is fixed, line endings are LF,
the file ends with a single trailing newline, and no timestamps or
environment-derived values are emitted.
"""

from __future__ import annotations

import argparse
import difflib
import json
import sys
import tempfile
from pathlib import Path
from typing import Any

import yaml

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_REGISTRY = REPO_ROOT / "protocol" / "registry.yaml"
DEFAULT_MANIFEST = REPO_ROOT / "protocol" / "generated" / "manifest.json"

# Bumped when the manifest *shape* changes (not the registry contents).
MANIFEST_SCHEMA_VERSION = 1


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


def render_manifest(manifest: dict[str, Any]) -> str:
    # sort_keys + fixed indent + trailing newline = deterministic bytes.
    return json.dumps(manifest, indent=2, sort_keys=True) + "\n"


def load_registry(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as fh:
        data = yaml.safe_load(fh)
    if not isinstance(data, dict):
        raise SystemExit(f"registry at {path} did not parse to a mapping")
    return data


def write_manifest(out: Path, content: str) -> None:
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(content, encoding="utf-8")


def check_manifest(out: Path, expected: str) -> int:
    """Compare expected content against `out`. Return 0 on match, 1 on drift."""
    if not out.exists():
        sys.stderr.write(
            f"protocol-codegen: missing generated artifact: {out}\n"
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
        "protocol-codegen: generated artifact is stale.\n"
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
    parser.add_argument(
        "--out",
        type=Path,
        default=DEFAULT_MANIFEST,
        help=f"path to generated manifest (default: {DEFAULT_MANIFEST})",
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--write",
        action="store_true",
        help="(re)write the manifest in place (default behaviour)",
    )
    mode.add_argument(
        "--check",
        action="store_true",
        help="regenerate to a temp dir and diff against --out; "
        "exit 1 on drift",
    )
    args = parser.parse_args(argv)

    registry = load_registry(args.registry)
    manifest = build_manifest(registry)
    rendered = render_manifest(manifest)

    if args.check:
        # Round-trip through a temp dir to prove the in-memory render and
        # an on-disk render agree, then compare against the checked-in file.
        with tempfile.TemporaryDirectory() as tmp:
            scratch = Path(tmp) / "manifest.json"
            write_manifest(scratch, rendered)
            scratch_bytes = scratch.read_bytes()
        if scratch_bytes.decode("utf-8") != rendered:
            sys.stderr.write(
                "protocol-codegen: internal error — temp-dir round-trip "
                "did not match in-memory render.\n"
            )
            return 2
        return check_manifest(args.out, rendered)

    write_manifest(args.out, rendered)
    return 0


if __name__ == "__main__":
    sys.exit(main())
