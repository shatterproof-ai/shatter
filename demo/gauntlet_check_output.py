#!/usr/bin/env python3
"""Inspect captured gauntlet/walkthrough step output for unexpected errors.

Originally str-jeen.59 (replaces an inline grep with allowlist-aware
checking); extended by str-qwua7.10 to catch two regression classes that
used to pass silently because they neither matched the process-error regex
nor tripped the coverage-threshold FAIL-row check:

  * A function completing exploration/scan with 0% coverage after at least
    one iteration/path (every call errored, most commonly on an input
    deserialization failure the old PROCESS_ERROR_RE didn't match).
  * A thrown-error cluster naming a lifecycle/scope class such as
    "Teardown scope mismatch" — produced when setup/teardown helpers get
    fuzzed as ordinary targets.

Checks performed, each already reported (unconditionally) or allowlist-aware:
  * Process-level error indicators ([error], panic, SIGSEGV, a deserialization
    failure, ...) are always reported.
  * `| FAIL |` scan-report rows are reported only when (basename(file),
    function) is not in the allowlist.
  * A function block (explore markdown, or a scan summary-table row) at 0%
    coverage with >=1 path/iteration is reported unless (basename(file),
    function) is allowlisted — the same `expected_failures` list used for
    `| FAIL |` rows, per str-qwua7.10's scope note that legitimately-0%
    fixtures use the ordinary allowlist mechanism.
  * A lifecycle/scope-mismatch thrown-error line (e.g. "Teardown scope
    mismatch") is reported unless its class is listed in
    `expected_lifecycle_clusters`.
  * `Scan complete: ... N error(s)` summaries are reported only when N exceeds
    the allowlist's `expected_scan_errors.count`.

Allowlist entries require an `expires: YYYY-MM-DD` date (schema is enforced
for `expected_failures`, `expected_lifecycle_clusters`, and
`expected_scan_errors`). An entry past its expiry stops suppressing and is
itself reported as a flagged line, so a stale allowlist entry fails the gate
rather than silently expiring into a blind spot.

Exits 0 with empty stdout when nothing is flagged. Otherwise prints one
line per flagged item (already indented for ERROR_LOG appending) and exits 1.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from datetime import date
from typing import Iterable

import yaml

PROCESS_ERROR_RE = re.compile(
    r"\[error\]|failed to deserialize|deserialization failed|panic|SIGSEGV"
    r"|error: exploration error",
    re.IGNORECASE,
)
SCAN_ERROR_SUMMARY_RE = re.compile(
    r"Scan complete:.*?\*?\*?(\d+)\*?\*? error\(s\)",
)
FAIL_ROW_RE = re.compile(
    r"^\|\s*FAIL\s*\|\s*[^|]*\|\s*`([^`]+)`\s*\|\s*([^|]+?)\s*\|"
)

# Matches a `## `function`` explore-markdown heading and, when present, its
# ` *(file:line-range)*` location suffix (shatter-cli/templates/explore_fn.md).
EXPLORE_HEADING_RE = re.compile(
    r"^##\s+`([^`]+)`(?:\s+\*\(([^:]+):[^)]*\)\*)?\s*$"
)
# Matches the summary line that follows a heading, e.g.
# "**4 path(s)** · **54%** coverage (7/13 lines)" or "**1 path(s)**" alone
# when the function has no line-coverage data (shatter-cli/templates/explore_fn.md).
EXPLORE_SUMMARY_RE = re.compile(
    r"\*\*(\d+)\s+path\(s\)\*\*(?:\s*·\s*\*\*(\d+(?:\.\d+)?)%\*\*\s*coverage)?"
)
# Matches a scan summary-table row, e.g.
# "| LOW | error_only | `classifyStatus` | 17-mock-branches.ts | 0.0% | 0/4 | 0/12 | 5 |"
# (shatter-core/src/report.rs write_md_summary_table: Status | Outcome |
# Function | File | Coverage | Branches | Lines | Iterations).
SCAN_ROW_RE = re.compile(
    r"^\|\s*\S+\s*\|\s*[^|]*\|\s*`([^`]+)`\s*\|\s*([^|]+?)\s*\|\s*"
    r"([\d.]+)%\s*\|\s*\d+/\d+\s*\|\s*\d+/\d+\s*\|\s*(\d+)\s*\|"
)
# Matches a thrown-error lifecycle/scope-mismatch cluster line, e.g.
# "throws Error: Teardown scope mismatch: expected a, got b" (behavior
# cluster rendering, shatter-core/src/report.rs) or the raw thrown message
# (examples/.../setup-file-level.ts's `throw new Error(...)`).
LIFECYCLE_CLUSTER_RE = re.compile(r"\b(Setup|Teardown)\s+scope\s+mismatch\b", re.IGNORECASE)


class AllowlistError(ValueError):
    """The allowlist file is malformed (e.g. missing a required `expires`)."""


def _parse_expiry(entry: dict, context: str, today: date) -> bool:
    """Return True if `entry` is expired as of `today`. Raises if `expires` is
    missing or unparseable — the field is required by schema."""
    expires_raw = entry.get("expires")
    if not expires_raw:
        raise AllowlistError(f"allowlist entry missing required 'expires': {context}")
    try:
        expires = date.fromisoformat(str(expires_raw))
    except ValueError as exc:
        raise AllowlistError(
            f"allowlist entry has invalid 'expires' ({expires_raw!r}): {context}"
        ) from exc
    return expires < today


def load_allowlist(
    path: str, today: date | None = None
) -> tuple[set[tuple[str, str]], int, set[str], list[str]]:
    """Load the allowlist. Returns (failures, expected_scan_errors_count,
    lifecycle_classes, expired_entry_descriptions)."""
    today = today or date.today()
    with open(path, "r", encoding="utf-8") as fh:
        data = yaml.safe_load(fh) or {}

    failures: set[tuple[str, str]] = set()
    expired: list[str] = []
    for entry in data.get("expected_failures", []) or []:
        context = f"expected_failures: {entry.get('file')}::{entry.get('function')}"
        if _parse_expiry(entry, context, today):
            expired.append(context)
            continue
        failures.add((entry["file"], entry["function"]))

    lifecycle: set[str] = set()
    for entry in data.get("expected_lifecycle_clusters", []) or []:
        context = f"expected_lifecycle_clusters: {entry.get('class')}"
        if _parse_expiry(entry, context, today):
            expired.append(context)
            continue
        lifecycle.add(str(entry["class"]).strip().lower())

    scan_errors_entry = data.get("expected_scan_errors") or {}
    expected_errors = int(scan_errors_entry.get("count", 0))
    if scan_errors_entry:
        context = "expected_scan_errors"
        if _parse_expiry(scan_errors_entry, context, today):
            expired.append(context)
            expected_errors = 0

    return failures, expected_errors, lifecycle, expired


def check(
    lines: Iterable[str],
    allowlist: set[tuple[str, str]],
    expected_errors: int,
    lifecycle_allowlist: frozenset[str] = frozenset(),
) -> list[str]:
    flagged: list[str] = []
    # Tracks the explore-markdown function block (## `name` ... through the
    # next heading or the report's closing "---") we're currently inside, so
    # a thrown-error table row belonging to an allowlisted function is
    # suppressed the same way its 0%-coverage summary line is — an
    # allowlisted flaky function's *own* errors are the point of the
    # allowlist entry, not just its headline coverage number.
    current_function: tuple[str, str, bool] | None = None  # (name, basename, allowed)
    awaiting_summary = False

    for raw in lines:
        line = raw.rstrip("\n")

        m = EXPLORE_HEADING_RE.match(line)
        if m:
            function, file_path = m.group(1), m.group(2)
            basename = os.path.basename(file_path) if file_path else ""
            current_function = (function, basename, (basename, function) in allowlist)
            awaiting_summary = True
            continue

        if line.strip() == "---":
            current_function = None
            awaiting_summary = False

        if PROCESS_ERROR_RE.search(line):
            if current_function is None or not current_function[2]:
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
            continue

        m = LIFECYCLE_CLUSTER_RE.search(line)
        if m:
            cluster_class = f"{m.group(1).capitalize()} scope mismatch"
            if cluster_class.lower() not in lifecycle_allowlist:
                flagged.append(line)
            continue

        if awaiting_summary and current_function is not None:
            if line.strip() == "":
                continue
            m = EXPLORE_SUMMARY_RE.search(line)
            if m:
                paths = int(m.group(1))
                pct = m.group(2)
                if pct is not None and float(pct) == 0.0 and paths >= 1 and not current_function[2]:
                    function, _basename, _allowed = current_function
                    flagged.append(
                        f"{function}: 0% coverage after {paths} path(s) "
                        f"explored ({line.strip()})"
                    )
            awaiting_summary = False
            # Fall through: this line may still be a scan row below (it
            # isn't, in practice — explore and scan output never interleave
            # a heading directly into a table row — but don't `continue`
            # so a genuinely unexpected shape isn't silently dropped).

        m = SCAN_ROW_RE.match(line)
        if m:
            function, file_path, pct, iters = m.groups()
            if float(pct) == 0.0 and int(iters) >= 1:
                basename = os.path.basename(file_path.strip())
                if (basename, function) not in allowlist:
                    flagged.append(line)

    return flagged


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allowlist", required=True)
    parser.add_argument("--output", required=True, help="Captured step output file")
    parser.add_argument("--step", default="", help="Step label included in error log")
    args = parser.parse_args()

    label = f"Step {args.step}" if args.step else "Step"

    try:
        allowlist, expected_errors, lifecycle_allowlist, expired = load_allowlist(
            args.allowlist
        )
    except AllowlistError as exc:
        print(f"  {label}: allowlist error: {exc}")
        return 1

    with open(args.output, "r", encoding="utf-8", errors="replace") as fh:
        flagged = check(fh, allowlist, expected_errors, frozenset(lifecycle_allowlist))

    flagged = [f"ALLOWLIST ENTRY EXPIRED: {e}" for e in expired] + flagged

    if not flagged:
        return 0

    print(f"  {label}: errors detected:")
    for line in flagged:
        print(f"    {line}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
