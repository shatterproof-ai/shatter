#!/usr/bin/env python3
"""Append gate events to a locked, size-bounded local JSONL event log."""

from __future__ import annotations

import argparse
import fcntl
import json
import os
import re
import sys
import tempfile
import time
from datetime import datetime

EXIT_OK = 0
EXIT_INVALID = 64
EXIT_IOERR = 74

MAX_LINE_BYTES = 1024 * 1024
RETENTION_TRIGGER_LINES = 10000
RETENTION_KEEP_LINES = 8000
DEFAULT_LOCK_TIMEOUT_S = 30.0
LOCK_POLL_INTERVAL_S = 0.05

EVENT_TYPE_RE = re.compile(r"^[a-z0-9_]+$")
RFC3339_UTC_RE = re.compile(
    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z$"
)


class EnvelopeError(Exception):
    """Raised when a JSONL line fails envelope validation."""


class LockError(Exception):
    """Raised when the append lock cannot be acquired in time."""


def default_event_path() -> str:
    base = os.environ.get("XDG_CACHE_HOME") or os.path.join(
        os.path.expanduser("~"), ".cache"
    )
    return os.path.join(base, "shatter", "gate-events.jsonl")


def is_normalized_abs_path(value: str) -> bool:
    if not value.startswith("/"):
        return False
    if value != "/" and value.endswith("/"):
        return False
    return os.path.normpath(value) == value


def validate_envelope_fields(obj: object) -> None:
    if not isinstance(obj, dict):
        raise EnvelopeError("envelope must be a JSON object")

    schema = obj.get("schema")
    if isinstance(schema, bool) or not isinstance(schema, int) or schema != 1:
        raise EnvelopeError("schema must be the integer 1")

    event_type = obj.get("event_type")
    if not isinstance(event_type, str) or not EVENT_TYPE_RE.fullmatch(event_type):
        raise EnvelopeError("event_type must match [a-z0-9_]+")

    timestamp = obj.get("timestamp")
    if not isinstance(timestamp, str) or not RFC3339_UTC_RE.fullmatch(timestamp):
        raise EnvelopeError("timestamp must be RFC3339 UTC ending in Z")
    try:
        datetime.fromisoformat(timestamp)
    except ValueError as exc:
        raise EnvelopeError("timestamp is not a valid date/time") from exc

    gate = obj.get("gate")
    if not isinstance(gate, str) or not gate or len(gate) > 128:
        raise EnvelopeError("gate must be a nonempty string of at most 128 chars")

    worktree = obj.get("worktree")
    if not isinstance(worktree, str) or not is_normalized_abs_path(worktree):
        raise EnvelopeError("worktree must be an absolute, normalized path")


def canonicalize(obj: dict) -> bytes:
    return json.dumps(obj, separators=(",", ":")).encode("utf-8")


def validate_line_bytes(raw: bytes) -> dict:
    if len(raw) > MAX_LINE_BYTES:
        raise EnvelopeError("serialized line exceeds 1MiB")
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise EnvelopeError("line is not valid UTF-8") from exc
    try:
        obj = json.loads(text)
    except json.JSONDecodeError as exc:
        raise EnvelopeError("line is not valid JSON") from exc
    validate_envelope_fields(obj)
    return obj


def split_lines(content: bytes) -> list[bytes]:
    if not content:
        return []
    if content.endswith(b"\n"):
        content = content[:-1]
    return content.split(b"\n")


def acquire_lock(lock_path: str, timeout: float) -> int:
    fd = os.open(lock_path, os.O_CREAT | os.O_RDWR, 0o600)
    try:
        os.fchmod(fd, 0o600)
    except OSError:
        pass
    deadline = time.monotonic() + timeout
    while True:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            return fd
        except BlockingIOError:
            if time.monotonic() >= deadline:
                os.close(fd)
                raise LockError(f"timed out waiting for lock: {lock_path}")
            time.sleep(LOCK_POLL_INTERVAL_S)
        except OSError:
            os.close(fd)
            raise


def release_lock(fd: int) -> None:
    try:
        fcntl.flock(fd, fcntl.LOCK_UN)
    finally:
        os.close(fd)


def ensure_parent_dir(parent: str) -> None:
    if os.path.isdir(parent):
        return
    os.makedirs(parent, mode=0o700, exist_ok=True)
    os.chmod(parent, 0o700)


def lock_timeout_seconds() -> float:
    raw = os.environ.get("GATE_EVENT_LOG_LOCK_TIMEOUT_S")
    if not raw:
        return DEFAULT_LOCK_TIMEOUT_S
    try:
        return float(raw)
    except ValueError:
        return DEFAULT_LOCK_TIMEOUT_S


def perform_retention(path: str, parent: str, lines: list[bytes]) -> None:
    for raw_line in lines:
        validate_line_bytes(raw_line)

    kept = lines[-RETENTION_KEEP_LINES:]
    tmp_fd, tmp_path = tempfile.mkstemp(
        prefix=".gate-events.", suffix=".tmp", dir=parent
    )
    try:
        with os.fdopen(tmp_fd, "wb") as tmp_f:
            for raw_line in kept:
                tmp_f.write(raw_line + b"\n")
            tmp_f.flush()
            os.fsync(tmp_f.fileno())
        os.replace(tmp_path, path)
    except BaseException:
        try:
            os.unlink(tmp_path)
        except OSError:
            pass
        raise


def cmd_append(path_arg: str | None) -> int:
    path = os.path.abspath(path_arg) if path_arg else default_event_path()
    parent = os.path.dirname(path)
    lock_path = path + ".lock"

    raw_stdin = sys.stdin.buffer.read()
    try:
        obj = json.loads(raw_stdin)
    except json.JSONDecodeError:
        return EXIT_INVALID
    try:
        validate_envelope_fields(obj)
    except EnvelopeError:
        return EXIT_INVALID
    line_bytes = canonicalize(obj)
    if len(line_bytes) > MAX_LINE_BYTES:
        return EXIT_INVALID

    try:
        ensure_parent_dir(parent)
    except OSError:
        return EXIT_IOERR

    try:
        lock_fd = acquire_lock(lock_path, lock_timeout_seconds())
    except (LockError, OSError):
        return EXIT_IOERR

    try:
        try:
            file_existed = os.path.exists(path)
            fd = os.open(path, os.O_CREAT | os.O_RDWR, 0o600)
            if not file_existed:
                os.fchmod(fd, 0o600)
        except OSError:
            return EXIT_IOERR

        with os.fdopen(fd, "r+b") as f:
            try:
                f.seek(0, os.SEEK_END)
                size = f.tell()
                if size > 0:
                    f.seek(size - 1)
                    if f.read(1) != b"\n":
                        f.write(b"\n")
                f.write(line_bytes + b"\n")
                f.flush()
                os.fsync(f.fileno())

                f.seek(0)
                content = f.read()
            except OSError:
                return EXIT_IOERR

            lines = split_lines(content)
            if len(lines) > RETENTION_TRIGGER_LINES:
                try:
                    perform_retention(path, parent, lines)
                except EnvelopeError:
                    return EXIT_IOERR
                except OSError:
                    return EXIT_IOERR

        return EXIT_OK
    finally:
        release_lock(lock_fd)


def main() -> int:
    parser = argparse.ArgumentParser(prog="gate-event-log.py", description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    append_parser = subparsers.add_parser(
        "append", help="validate and append one event envelope read from stdin"
    )
    append_parser.add_argument(
        "--path", default=None, help="override the event log file path"
    )

    args = parser.parse_args()
    if args.command == "append":
        return cmd_append(args.path)
    return EXIT_INVALID


if __name__ == "__main__":
    raise SystemExit(main())
