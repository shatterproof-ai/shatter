#!/usr/bin/env python3
"""Inspect captured gauntlet step output for unexpected errors (str-jeen.59).

Replaces the gauntlet's inline grep with allowlist-aware checking:
  * Process-level error indicators ([error], panic, SIGSEGV, ...) are always
    reported.
  * `| FAIL |` scan-report rows are reported only when (basename(file),
    function) is not in the allowlist.
  * `Scan complete: ... N error(s)` summaries are reported only when N exceeds
    the allowlist's `expected_scan_errors.count`.

Exits 0 with empty stdout when nothing is flagged. Otherwise prints one
line per flagged item (already indented for ERROR_LOG appending) and exits 1.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from typing import Iterable

import yaml

PROCESS_ERROR_RE = re.compile(
    r"\[error\]|failed to deserialize|panic|SIGSEGV|error: exploration error",
    re.IGNORECASE,
)
SCAN_ERROR_SUMMARY_RE = re.compile(
    r"Scan complete:.*?\*?\*?(\d+)\*?\*? error\(s\)",
)
FAIL_ROW_RE = re.compile(
    r"^\|\s*FAIL\s*\|\s*[^|]*\|\s*`([^`]+)`\s*\|\s*([^|]+?)\s*\|"
)


def load_allowlist(path: str) -> tuple[set[tuple[str, str]], int]:
    with open(path, "r", encoding="utf-8") as fh:
        data = yaml.safe_load(fh) or {}
    failures = {
        (entry["file"], entry["function"])
        for entry in data.get("expected_failures", []) or []
    }
    expected_errors = int((data.get("expected_scan_errors") or {}).get("count", 0))
    return failures, expected_errors


def check(lines: Iterable[str], allowlist: set[tuple[str, str]], expected_errors: int) -> list[str]:
    flagged: list[str] = []
    for raw in lines:
        line = raw.rstrip("\n")
        if PROCESS_ERROR_RE.search(line):
            flagged.append(line)
            continue
        m = SCAN_ERROR_SUMMARY_RE.search(line)
        if m:
            count = int(m.group(1))
            if count > expected_errors:
                flagged.append(
                    f"{line}  [unexpected: {count} > allowlisted {expected_errors}]"
                )
            continue
        m = FAIL_ROW_RE.match(line)
        if m:
            function = m.group(1)
            file_path = m.group(2)
            basename = os.path.basename(file_path)
            if (basename, function) not in allowlist:
                flagged.append(line)
    return flagged


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allowlist", required=True)
    parser.add_argument("--output", required=True, help="Captured step output file")
    parser.add_argument("--step", default="", help="Step label included in error log")
    args = parser.parse_args()

    allowlist, expected_errors = load_allowlist(args.allowlist)
    with open(args.output, "r", encoding="utf-8", errors="replace") as fh:
        flagged = check(fh, allowlist, expected_errors)

    if not flagged:
        return 0

    label = f"Step {args.step}" if args.step else "Step"
    print(f"  {label}: errors detected:")
    for line in flagged:
        print(f"    {line}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
