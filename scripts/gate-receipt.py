#!/usr/bin/env python3
"""Write deterministic gate receipt v1 evidence for an exact Git tree."""

from __future__ import annotations

import argparse
from datetime import datetime
import fnmatch
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from typing import NoReturn


EXIT_OK = 0
EXIT_INVALID = 64
EXIT_IOERR = 74

TREE_RE = re.compile(r"^[0-9a-f]{40}$")
TIMESTAMP_RE = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
RESULT_KEYS = {"gate", "argv", "started_at", "ended_at", "exit_code"}
REQUIRED_CODE = ("scripts/gate-wrapper.sh", "scripts/gate-receipt.py")
LOCK_PATHS = (
    "Cargo.lock",
    "shatter-go/go.sum",
    "shatter-rust-runtime/Cargo.lock",
    "shatter-rust/Cargo.lock",
    "shatter-ts/package-lock.json",
)
TOOL_COMMANDS = {
    "cargo": ("cargo", "--version"),
    "go": ("go", "version"),
    "node": ("node", "--version"),
    "npm": ("npm", "--version"),
    "rustc": ("rustc", "--version"),
    "task": ("task", "--version"),
}
GIT_ENV_KEYS = (
    "GIT_DIR",
    "GIT_COMMON_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
)


class InvalidInput(Exception):
    """The caller supplied invalid receipt input."""


class DiscoveryIOError(Exception):
    """Repository/tool discovery or durable storage failed."""


class ReceiptArgumentParser(argparse.ArgumentParser):
    def error(self, message: str) -> NoReturn:
        raise InvalidInput(message)


def canonical_json(value: object) -> bytes:
    return json.dumps(
        value,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
        allow_nan=False,
    ).encode("utf-8")


def digest_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def git_environment() -> dict[str, str]:
    env = dict(os.environ)
    for key in GIT_ENV_KEYS:
        env.pop(key, None)
    return env


def run_git(args: list[str]) -> bytes:
    try:
        result = subprocess.run(
            ["git", *args],
            env=git_environment(),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
    except OSError as exc:
        raise DiscoveryIOError(f"cannot run git: {exc}") from exc
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise DiscoveryIOError(f"git {' '.join(args)} failed: {detail or result.returncode}")
    return result.stdout


def discover_common_dir() -> Path:
    raw = run_git(["rev-parse", "--git-common-dir"])
    try:
        value = raw.decode("utf-8").strip()
    except UnicodeDecodeError as exc:
        raise DiscoveryIOError("Git common directory is not UTF-8") from exc
    if not value:
        raise DiscoveryIOError("Git returned an empty common directory")
    common_dir = Path(value)
    if not common_dir.is_absolute():
        common_dir = Path.cwd() / common_dir
    try:
        return common_dir.resolve(strict=True)
    except OSError as exc:
        raise DiscoveryIOError(f"cannot resolve Git common directory: {exc}") from exc


def validate_tree(value: str, label: str) -> None:
    if not TREE_RE.fullmatch(value):
        raise InvalidInput(f"{label} must be exactly 40 lowercase hexadecimal characters")
    try:
        resolved = run_git(["rev-parse", "--verify", f"{value}^{{tree}}"])
    except DiscoveryIOError as exc:
        # Repository discovery already succeeded, so an unresolvable supplied
        # object is invalid input rather than an environmental failure.
        raise InvalidInput(f"{label} is not an available tree object") from exc
    try:
        resolved_text = resolved.decode("ascii").strip()
    except UnicodeDecodeError as exc:
        raise DiscoveryIOError("Git returned a non-ASCII object ID") from exc
    if resolved_text != value:
        raise InvalidInput(f"{label} must identify a tree object directly")


def candidate_entries(candidate: str) -> dict[str, tuple[str, str]]:
    raw = run_git(["ls-tree", "-r", "-t", "-z", "--full-tree", candidate])
    entries: dict[str, tuple[str, str]] = {}
    for record in raw.split(b"\0"):
        if not record:
            continue
        try:
            metadata, raw_path = record.split(b"\t", 1)
            mode, object_type, object_id = metadata.decode("ascii").split(" ", 2)
            path = raw_path.decode("utf-8")
        except (ValueError, UnicodeDecodeError) as exc:
            raise DiscoveryIOError("Git returned an invalid tree record") from exc
        entries[path] = (object_type, object_id)
    return entries


def blob_bytes(entries: dict[str, tuple[str, str]], path: str, *, required: bool) -> bytes | None:
    entry = entries.get(path)
    if entry is None:
        if required:
            raise InvalidInput(f"candidate tree is missing required blob {path}")
        return None
    object_type, object_id = entry
    if object_type != "blob":
        raise InvalidInput(f"candidate path is not a blob: {path}")
    return run_git(["cat-file", "blob", object_id])


def collect_bindings(candidate: str) -> dict[str, list[dict[str, object]]]:
    entries = candidate_entries(candidate)
    code_paths = {
        path
        for path, (object_type, _) in entries.items()
        if object_type == "blob" and fnmatch.fnmatchcase(path, "*Taskfile*.yml")
    }
    code_paths.update(REQUIRED_CODE)
    code = []
    for path in sorted(code_paths):
        data = blob_bytes(entries, path, required=True)
        assert data is not None
        code.append({"path": path, "sha256": digest_bytes(data)})

    locks = []
    for path in LOCK_PATHS:
        data = blob_bytes(entries, path, required=False)
        locks.append(
            {
                "path": path,
                "present": data is not None,
                "sha256": digest_bytes(data) if data is not None else None,
            }
        )
    return {"code": code, "locks": locks}


def unique_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise InvalidInput(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def parse_timestamp(value: object, field: str) -> datetime:
    if not isinstance(value, str) or not TIMESTAMP_RE.fullmatch(value):
        raise InvalidInput(f"{field} must be an RFC3339 UTC whole-second timestamp")
    try:
        return datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ")
    except ValueError as exc:
        raise InvalidInput(f"{field} is not a valid timestamp") from exc


def load_gate_result(path: Path) -> dict[str, object]:
    try:
        raw = path.read_bytes()
    except OSError as exc:
        raise DiscoveryIOError(f"cannot read gate result {path}: {exc}") from exc
    try:
        value = json.loads(raw.decode("utf-8"), object_pairs_hook=unique_object)
    except InvalidInput:
        raise
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise InvalidInput(f"gate result is not valid UTF-8 JSON: {path}") from exc
    if not isinstance(value, dict) or set(value) != RESULT_KEYS:
        raise InvalidInput(f"gate result must contain exactly the v1 fields: {path}")

    gate = value["gate"]
    argv = value["argv"]
    exit_code = value["exit_code"]
    if not isinstance(gate, str) or not gate:
        raise InvalidInput("gate must be a nonempty string")
    if not isinstance(argv, list) or not argv or not all(isinstance(arg, str) for arg in argv):
        raise InvalidInput("argv must be a nonempty array of strings")
    if isinstance(exit_code, bool) or not isinstance(exit_code, int) or exit_code != 0:
        raise InvalidInput("gate result exit_code must be the integer 0")
    started = parse_timestamp(value["started_at"], "started_at")
    ended = parse_timestamp(value["ended_at"], "ended_at")
    if ended < started:
        raise InvalidInput("gate result ended_at precedes started_at")
    return value


def load_gate_results(paths: list[Path]) -> list[dict[str, object]]:
    results = sorted((load_gate_result(path) for path in paths), key=lambda item: str(item["gate"]))
    for previous, current in zip(results, results[1:]):
        if previous["gate"] == current["gate"]:
            raise InvalidInput(f"duplicate gate result: {current['gate']}")
    return results


def collect_tools() -> list[dict[str, str]]:
    tools = []
    for name, command in sorted(TOOL_COMMANDS.items()):
        try:
            result = subprocess.run(
                command,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
        except OSError as exc:
            raise DiscoveryIOError(f"cannot run {name} version command: {exc}") from exc
        if result.returncode != 0:
            raise DiscoveryIOError(f"{name} version command failed with exit {result.returncode}")
        try:
            version = result.stdout.decode("utf-8").strip()
        except UnicodeDecodeError as exc:
            raise DiscoveryIOError(f"{name} version output is not UTF-8") from exc
        if not version:
            raise DiscoveryIOError(f"{name} version output is empty")
        tools.append({"name": name, "version": version})
    return tools


def build_receipt(
    candidate: str,
    base: str,
    tier: str,
    gate_results: list[dict[str, object]],
) -> tuple[dict[str, object], str]:
    receipt: dict[str, object] = {
        "schema": 1,
        "candidate_tree": candidate,
        "base_tree": base,
        "tier": tier,
        "gate_results": gate_results,
        "bindings": collect_bindings(candidate),
        "tools": collect_tools(),
        "started_at": min(str(item["started_at"]) for item in gate_results),
        "ended_at": max(str(item["ended_at"]) for item in gate_results),
    }
    digest = f"sha256:{digest_bytes(canonical_json(receipt))}"
    receipt["digest"] = digest
    return receipt, digest


def ensure_private_directory(path: Path) -> None:
    try:
        path.mkdir(mode=0o700, exist_ok=True)
        if path.is_symlink() or not path.is_dir():
            raise DiscoveryIOError(f"receipt directory is not a directory: {path}")
        path.chmod(0o700)
    except DiscoveryIOError:
        raise
    except OSError as exc:
        raise DiscoveryIOError(f"cannot prepare receipt directory {path}: {exc}") from exc


def receipt_path(common_dir: Path, candidate: str) -> Path:
    runtime_value = os.environ.get("XDG_RUNTIME_DIR") or "/tmp"
    current = Path(runtime_value)
    for component in ("shatter-gate-receipts", "v1", digest_bytes(os.fsencode(common_dir))):
        current /= component
        ensure_private_directory(current)
    return current / f"{candidate}.json"


def atomic_write(path: Path, data: bytes) -> None:
    descriptor = -1
    temporary: str | None = None
    try:
        descriptor, temporary = tempfile.mkstemp(
            prefix=f".{path.stem}.", suffix=".tmp", dir=path.parent
        )
        os.fchmod(descriptor, 0o600)
        with os.fdopen(descriptor, "wb") as stream:
            descriptor = -1
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        temporary = None
    except OSError as exc:
        raise DiscoveryIOError(f"cannot write receipt {path}: {exc}") from exc
    finally:
        if descriptor >= 0:
            os.close(descriptor)
        if temporary is not None:
            try:
                os.unlink(temporary)
            except FileNotFoundError:
                pass


def parser() -> ReceiptArgumentParser:
    root = ReceiptArgumentParser(prog="gate-receipt.py")
    commands = root.add_subparsers(dest="command", required=True, parser_class=ReceiptArgumentParser)
    write = commands.add_parser("write")
    write.add_argument("--candidate", required=True)
    write.add_argument("--base", required=True)
    write.add_argument("--tier", choices=("local", "ci"), required=True)
    write.add_argument("--gate-result", action="append", required=True, type=Path)
    return root


def write_receipt(args: argparse.Namespace) -> dict[str, str]:
    common_dir = discover_common_dir()
    validate_tree(args.candidate, "candidate")
    validate_tree(args.base, "base")
    results = load_gate_results(args.gate_result)
    receipt, digest = build_receipt(args.candidate, args.base, args.tier, results)
    path = receipt_path(common_dir, args.candidate)
    atomic_write(path, canonical_json(receipt) + b"\n")
    return {"status": "written", "path": str(path), "digest": digest}


def main(argv: list[str] | None = None) -> int:
    try:
        args = parser().parse_args(argv)
        result = write_receipt(args)
    except InvalidInput as exc:
        print(f"gate-receipt: invalid input: {exc}", file=sys.stderr)
        return EXIT_INVALID
    except DiscoveryIOError as exc:
        print(f"gate-receipt: discovery/I/O error: {exc}", file=sys.stderr)
        return EXIT_IOERR
    print(canonical_json(result).decode("utf-8"))
    return EXIT_OK


if __name__ == "__main__":
    raise SystemExit(main())
