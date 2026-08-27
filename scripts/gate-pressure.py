#!/usr/bin/env python3
"""Gate admission pressure signal provider (str-35vtk.17).

Reads local host resource pressure (memory, disk, and Linux PSI CPU/memory/IO
pressure) and reports a JSON v1 admission signal document to stdout. Read-only,
no network access, does not wait or retry — a single point-in-time snapshot.

Exit codes:
  0  - signals were computed successfully (overall status may be "ready" or
       "blocked" -- that distinction lives in the JSON, not the exit code)
  70 - a required signal could not be computed (missing/unreadable/malformed
       meminfo, a statvfs failure, a malformed-but-present PSI file, or an
       invalid override environment variable)
"""

from __future__ import annotations

import json
import math
import os
import sys

SCHEMA_VERSION = 1

DEFAULT_MIN_MEM_BYTES = 8589934592
DEFAULT_MIN_DISK_BYTES = 10737418240

PSI_SIGNALS = (
    {
        "name": "memory_full_avg10",
        "filename": "memory",
        "field": "full",
        "env": "SHATTER_GATE_MAX_MEMORY_FULL_AVG10",
        "default": 1.0,
    },
    {
        "name": "io_some_avg10",
        "filename": "io",
        "field": "some",
        "env": "SHATTER_GATE_MAX_IO_SOME_AVG10",
        "default": 20.0,
    },
    {
        "name": "cpu_some_avg10",
        "filename": "cpu",
        "field": "some",
        "env": "SHATTER_GATE_MAX_CPU_SOME_AVG10",
        "default": 20.0,
    },
)


class SignalError(Exception):
    """Base for errors that map to a hard exit-70 failure."""

    def __init__(self, reason: str, detail: str):
        super().__init__(detail)
        self.reason = reason
        self.detail = detail


class InvalidOverrideError(SignalError):
    def __init__(self, env_name: str, detail: str):
        super().__init__("invalid_override", detail)
        self.env_name = env_name


class MemInfoError(SignalError):
    pass


class PsiError(SignalError):
    pass


class DiskError(SignalError):
    pass


def detect_platform() -> str:
    return "linux" if sys.platform.startswith("linux") else "other"


def parse_bytes_override(env_name: str, default: int) -> int:
    raw = os.environ.get(env_name)
    if raw is None or raw.strip() == "":
        return default
    raw = raw.strip()
    try:
        value = int(raw)
    except ValueError as exc:
        raise InvalidOverrideError(env_name, f"{env_name}={raw!r} is not an integer") from exc
    if value < 0:
        raise InvalidOverrideError(env_name, f"{env_name}={raw!r} must be nonnegative")
    return value


def parse_percent_override(env_name: str, default: float) -> float:
    raw = os.environ.get(env_name)
    if raw is None or raw.strip() == "":
        return default
    raw = raw.strip()
    try:
        value = float(raw)
    except ValueError as exc:
        raise InvalidOverrideError(env_name, f"{env_name}={raw!r} is not a decimal number") from exc
    if not math.isfinite(value) or value < 0:
        raise InvalidOverrideError(env_name, f"{env_name}={raw!r} must be a finite nonnegative percentage")
    return value


def read_mem_available_bytes(proc_root: str) -> int:
    path = os.path.join(proc_root, "meminfo")
    if not os.path.exists(path):
        raise MemInfoError("missing", f"{path} not found")
    try:
        with open(path, "r", encoding="utf-8") as handle:
            content = handle.read()
    except OSError as exc:
        raise MemInfoError("unreadable", f"failed to read {path}: {exc}") from exc

    for line in content.splitlines():
        if not line.startswith("MemAvailable:"):
            continue
        parts = line.split()
        if len(parts) < 2:
            raise MemInfoError("malformed", f"malformed MemAvailable line in {path}: {line!r}")
        try:
            kb = int(parts[1])
        except ValueError as exc:
            raise MemInfoError("malformed", f"non-integer MemAvailable value in {path}: {line!r}") from exc
        return kb * 1024

    raise MemInfoError("malformed", f"MemAvailable field not found in {path}")


def read_psi_avg10(proc_root: str, filename: str, field: str):
    """Returns the avg10 float, or None if the PSI file is absent (unsupported)."""
    path = os.path.join(proc_root, "pressure", filename)
    if not os.path.exists(path):
        return None
    try:
        with open(path, "r", encoding="utf-8") as handle:
            content = handle.read()
    except OSError as exc:
        raise PsiError("unreadable", f"failed to read {path}: {exc}") from exc

    for line in content.splitlines():
        parts = line.split()
        if not parts or parts[0] != field:
            continue
        for token in parts[1:]:
            if token.startswith("avg10="):
                raw_value = token.split("=", 1)[1]
                try:
                    return float(raw_value)
                except ValueError as exc:
                    raise PsiError(
                        "malformed", f"non-numeric avg10 value in {path} for {field!r}: {raw_value!r}"
                    ) from exc
        raise PsiError("malformed", f"avg10 field missing on {field!r} line in {path}")

    raise PsiError("malformed", f"{field!r} line not found in {path}")


def resolve_fs_paths() -> list:
    raw = os.environ.get("SHATTER_PRESSURE_FS_PATHS")
    if raw is not None and raw.strip() != "":
        return [p for p in raw.split(os.pathsep) if p]

    # os.getcwd() must win over $PWD: PWD is shell-maintained and is not
    # refreshed by a parent process's os.chdir()/set_current_dir() before
    # spawning this script, so a stale inherited PWD would silently point
    # the disk check at the wrong directory.
    pwd = os.getcwd() or os.environ.get("PWD")
    cargo_target = os.environ.get("CARGO_TARGET_DIR") or os.path.join(pwd, "target")
    xdg_cache_home = os.environ.get("XDG_CACHE_HOME") or os.path.join(os.environ.get("HOME", ""), ".cache")
    sccache_dir = os.environ.get("SCCACHE_DIR") or os.path.join(xdg_cache_home, "sccache")
    tmpdir = os.environ.get("TMPDIR") or "/tmp"
    return [pwd, cargo_target, sccache_dir, tmpdir]


def ancestor_stat(path: str):
    """Walks up to the nearest existing ancestor and statvfs's it.

    Returns (resolved_path, st_dev, statvfs_result).
    """
    candidate = os.path.abspath(path)
    while not os.path.exists(candidate):
        parent = os.path.dirname(candidate)
        if parent == candidate:
            break
        candidate = parent
    try:
        st_dev = os.stat(candidate).st_dev
        vfs = os.statvfs(candidate)
    except OSError as exc:
        raise DiskError("unreadable", f"statvfs failed for {candidate}: {exc}") from exc
    return candidate, st_dev, vfs


def evaluate_disk(paths: list):
    """Returns (min_free_bytes, checked_paths, unique_device_count).

    Paths that resolve (via ancestor fallback) to the same device are
    deduplicated: only the first occurrence of a device contributes to the
    minimum free-bytes computation.
    """
    if not paths:
        raise DiskError("missing", "no filesystem paths configured")

    seen_devices = {}
    checked = []
    for raw_path in paths:
        resolved, st_dev, vfs = ancestor_stat(raw_path)
        free_bytes = vfs.f_bavail * vfs.f_frsize
        checked.append({"input": raw_path, "resolved": resolved, "device": st_dev})
        if st_dev not in seen_devices:
            seen_devices[st_dev] = free_bytes

    min_free = min(seen_devices.values())
    return min_free, checked, len(seen_devices)


def _blocking_signal(name, value, threshold, comparison, status, reason, extra=None):
    signal = {
        "name": name,
        "status": status,
        "value": value,
        "threshold": threshold,
        "comparison": comparison,
        "reason": reason,
    }
    if extra:
        signal["detail"] = extra
    return signal


def _unsupported_signal(name, threshold, comparison):
    return _blocking_signal(name, None, threshold, comparison, "unsupported", "unsupported")


def emit_error(reason: str, signal: str, detail: str) -> None:
    print(
        json.dumps(
            {
                "schema_version": SCHEMA_VERSION,
                "status": "error",
                "signal": signal,
                "reason": reason,
                "detail": detail,
            }
        )
    )


def run(argv=None) -> int:
    del argv  # no CLI flags; behavior is env-var driven per the issue spec

    try:
        min_mem_bytes = parse_bytes_override("SHATTER_GATE_MIN_MEM_BYTES", DEFAULT_MIN_MEM_BYTES)
        min_disk_bytes = parse_bytes_override("SHATTER_GATE_MIN_DISK_BYTES", DEFAULT_MIN_DISK_BYTES)
        psi_thresholds = {
            spec["name"]: parse_percent_override(spec["env"], spec["default"]) for spec in PSI_SIGNALS
        }
    except InvalidOverrideError as exc:
        emit_error(exc.reason, exc.env_name, exc.detail)
        return 70

    proc_root = os.environ.get("SHATTER_PROC_ROOT") or "/proc"
    platform = detect_platform()
    signals = []

    try:
        paths = resolve_fs_paths()
        min_free_bytes, checked_paths, unique_devices = evaluate_disk(paths)
    except DiskError as exc:
        emit_error(exc.reason, "disk_free_bytes", exc.detail)
        return 70

    disk_status = "ready" if min_free_bytes >= min_disk_bytes else "blocked"
    signals.append(
        _blocking_signal(
            "disk_free_bytes",
            min_free_bytes,
            min_disk_bytes,
            "min",
            disk_status,
            None if disk_status == "ready" else "below_min",
            extra={"paths_checked": checked_paths, "unique_devices": unique_devices},
        )
    )

    if platform == "linux":
        try:
            mem_available_bytes = read_mem_available_bytes(proc_root)
        except MemInfoError as exc:
            emit_error(exc.reason, "mem_available_bytes", exc.detail)
            return 70

        mem_status = "ready" if mem_available_bytes >= min_mem_bytes else "blocked"
        signals.append(
            _blocking_signal(
                "mem_available_bytes",
                mem_available_bytes,
                min_mem_bytes,
                "min",
                mem_status,
                None if mem_status == "ready" else "below_min",
            )
        )

        for spec in PSI_SIGNALS:
            threshold = psi_thresholds[spec["name"]]
            try:
                value = read_psi_avg10(proc_root, spec["filename"], spec["field"])
            except PsiError as exc:
                emit_error(exc.reason, spec["name"], exc.detail)
                return 70

            if value is None:
                signals.append(_unsupported_signal(spec["name"], threshold, "max"))
                continue

            status = "blocked" if value > threshold else "ready"
            signals.append(
                _blocking_signal(
                    spec["name"],
                    value,
                    threshold,
                    "max",
                    status,
                    None if status == "ready" else "above_max",
                )
            )
    else:
        signals.append(_unsupported_signal("mem_available_bytes", min_mem_bytes, "min"))
        for spec in PSI_SIGNALS:
            signals.append(_unsupported_signal(spec["name"], psi_thresholds[spec["name"]], "max"))

    overall_status = "blocked" if any(s["status"] == "blocked" for s in signals) else "ready"

    print(
        json.dumps(
            {
                "schema_version": SCHEMA_VERSION,
                "status": overall_status,
                "platform": platform,
                "signals": signals,
            }
        )
    )
    return 0


def main() -> int:
    return run(sys.argv[1:])


if __name__ == "__main__":
    sys.exit(main())
