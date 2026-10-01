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
    failure, ...) are reported. Crash-class markers ([error], panic, SIGSEGV,
    "error: exploration error") are ALWAYS reported; a deserialization-
    failure row is excused only inside the explore block of an allowlisted
    function.
  * Known limitation: the markdown-shape checks (explore headings/summaries,
    scan-table rows, `| FAIL |` rows) match RAW markdown. When a demo is run
    from a TTY the scripts pass `--color always`, shatter renders markdown
    through termimad, and those checks cannot see the rendered output; only
    the plain-text process-error markers still apply. The authoritative
    non-TTY runs (git hooks, CI, agents) are unaffected.
  * Scan verdicts come from the scan's `--format json` report (`--scan-json`),
    never from markdown prose (str-49drv.149): every `codebase.failed[]` entry
    and every `codebase.skipped_functions[]` entry with `category ==
    "interrupted"` is reported unless (basename(file), function) is in the
    allowlist (a failure entry may also pin `reason_contains`). A step that
    legitimately bounds the run (`--timeout-total`) passes
    `--expect-interrupted REASON`; a step that executes nothing (`--dry-run`)
    passes `--no-scan-json REASON`, which is logged and skips the check. A
    missing/unparseable report when one was expected is itself an error.
  * A function block (explore markdown, or a scan summary-table row) at 0%
    coverage with >=1 path/iteration is reported unless (basename(file),
    function) is allowlisted — the same `expected_failures` list used for
    `| FAIL |` rows, per str-qwua7.10's scope note that legitimately-0%
    fixtures use the ordinary allowlist mechanism.
  * A lifecycle/scope-mismatch thrown-error line (e.g. "Teardown scope
    mismatch") is reported unless its class is listed in
    `expected_lifecycle_clusters`.

Allowlist entries require an `expires: YYYY-MM-DD` date (schema is enforced
for `expected_failures` and `expected_lifecycle_clusters`). An entry past its expiry stops suppressing and is
itself reported as a flagged line, so a stale allowlist entry fails the gate
rather than silently expiring into a blind spot.

Exits 0 with empty stdout when nothing is flagged. Otherwise prints one
line per flagged item (already indented for ERROR_LOG appending) and exits 1.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from datetime import date
from typing import Iterable, NamedTuple

import yaml

# Crash-class markers: always reported, even inside an allowlisted
# function's explore block (an allowlist entry excuses a function's known
# thrown-error *rows*, never a real panic/SIGSEGV/[error] that happens to
# follow it in the interleaved stdout+stderr capture).
HARD_ERROR_RE = re.compile(
    r"\[error\]|panic|SIGSEGV|error: exploration error",
    re.IGNORECASE,
)
# A function's own input-deserialization failure rows: the one process-level
# marker an allowlisted function's block may suppress.
DESERIALIZE_ERROR_RE = re.compile(
    r"failed to deserialize|deserialization failed",
    re.IGNORECASE,
)
PROCESS_ERROR_RE = re.compile(
    HARD_ERROR_RE.pattern + "|" + DESERIALIZE_ERROR_RE.pattern,
    re.IGNORECASE,
)
# Interleaved stderr log lines ("[info] ...") that can land between a
# function heading and its summary line in the combined capture.
LOG_LINE_RE = re.compile(r"^\s*\[(?:info|warn|warning|debug|trace)\]", re.IGNORECASE)

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


class Allowlist(NamedTuple):
    # (basename(file), function) -> reason_contains substrings pinned by its
    # entries ("" = any reason).
    failures: dict[tuple[str, str], list[str]]
    lifecycle: set[str]
    expired: list[str]

    def allows(self, basename: str, function: str, reason: str = "") -> bool:
        pins = self.failures.get((basename, function))
        return pins is not None and any(pin in reason for pin in pins)


def load_allowlist(path: str, today: date | None = None) -> Allowlist:
    """Load the allowlist (failures, lifecycle classes, expired-entry
    descriptions)."""
    today = today or date.today()
    with open(path, "r", encoding="utf-8") as fh:
        data = yaml.safe_load(fh) or {}

    failures: dict[tuple[str, str], list[str]] = {}
    expired: list[str] = []
    for entry in data.get("expected_failures", []) or []:
        context = f"expected_failures: {entry.get('file')}::{entry.get('function')}"
        if _parse_expiry(entry, context, today):
            expired.append(context)
            continue
        key = (entry["file"], entry["function"])
        failures.setdefault(key, []).append(str(entry.get("reason_contains", "")))

    lifecycle: set[str] = set()
    for entry in data.get("expected_lifecycle_clusters", []) or []:
        context = f"expected_lifecycle_clusters: {entry.get('class')}"
        if _parse_expiry(entry, context, today):
            expired.append(context)
            continue
        lifecycle.add(str(entry["class"]).strip().lower())

    return Allowlist(failures, lifecycle, expired)


def _basename_of_qualified_id(qualified_id: str) -> str:
    """`/abs/path/file.ts::fn` -> `file.ts` (skipped entries carry no file_path).

    The file is the text before the FIRST `::`: the name part can itself
    contain `::` (Rust `Type::method`), so splitting on the last one would
    yield `file.rs::Type` and never match a file-keyed allowlist entry.
    """
    return os.path.basename(qualified_id.split("::", 1)[0]) if "::" in qualified_id else ""


def check_scan_json(
    path: str, allowlist: Allowlist, expect_interrupted: str | None = None
) -> list[str]:
    """Flag failed and interrupted functions in a scan `--format json` report."""
    try:
        with open(path, "r", encoding="utf-8") as fh:
            codebase = json.load(fh)["codebase"]
    except (OSError, ValueError, KeyError, TypeError) as exc:
        return [f"scan JSON missing or unparseable ({path}): {exc!r}"]

    flagged: list[str] = []
    for f in codebase.get("failed") or []:
        name = f.get("function_name", "?")
        reason = f.get("reason", "")
        basename = os.path.basename(f.get("file_path", "")) or _basename_of_qualified_id(
            f.get("qualified_id", "")
        )
        if not allowlist.allows(basename, name, reason):
            flagged.append(f"scan failure: `{name}` ({basename or 'unknown file'}): {reason}")
    if expect_interrupted is None:
        for f in codebase.get("skipped_functions") or []:
            if f.get("category") != "interrupted":
                continue
            name = f.get("function_name", "?")
            basename = _basename_of_qualified_id(f.get("qualified_id", ""))
            if not allowlist.allows(basename, name):
                flagged.append(
                    f"scan interrupted: `{name}` ({basename or 'unknown file'}): {f.get('reason', '')}"
                )
    return flagged


def check(
    lines: Iterable[str],
    allowlist: Allowlist,
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
            current_function = (function, basename, allowlist.allows(basename, function))
            awaiting_summary = True
            continue

        if line.strip() == "---":
            current_function = None
            awaiting_summary = False

        if PROCESS_ERROR_RE.search(line):
            # Only an allowlisted function's own deserialization-failure
            # rows are excusable; crash-class markers (HARD_ERROR_RE) are
            # always reported. The block also never closes for single-
            # function output (no trailing "---"), so trailing stderr lines
            # must not inherit the suppression.
            excused = (
                current_function is not None
                and current_function[2]
                and not HARD_ERROR_RE.search(line)
            )
            if not excused:
                flagged.append(line)
            continue

        m = LIFECYCLE_CLUSTER_RE.search(line)
        if m:
            cluster_class = f"{m.group(1).capitalize()} scope mismatch"
            if cluster_class.lower() not in lifecycle_allowlist:
                flagged.append(line)
            continue

        if awaiting_summary and current_function is not None:
            if line.strip() == "" or LOG_LINE_RE.match(line):
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
                if not allowlist.allows(basename, function):
                    flagged.append(line)

    return flagged


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allowlist", required=True)
    parser.add_argument("--output", required=True, help="Captured step output file")
    parser.add_argument("--step", default="", help="Step label included in error log")
    parser.add_argument(
        "--scan-json",
        default=None,
        help="Scan `--format json` report for this step; failed and interrupted "
        "functions in it are flagged unless allowlisted",
    )
    parser.add_argument(
        "--expect-interrupted",
        default=None,
        metavar="REASON",
        help="Interrupted functions are expected on this step (e.g. it bounds "
        "the scan with --timeout-total); failures are still flagged",
    )
    parser.add_argument(
        "--no-scan-json",
        default=None,
        metavar="REASON",
        help="This scan step produces no report (e.g. --dry-run); the scan JSON "
        "check is skipped and REASON is logged to stderr",
    )
    parser.add_argument(
        "--today",
        default=None,
        help="YYYY-MM-DD override for the current date when evaluating "
        "allowlist expiry (test seam; unit tests pin it so they don't rot on "
        "date rollover -- the real gate always uses the real date)",
    )
    args = parser.parse_args()
    today = date.fromisoformat(args.today) if args.today else None

    label = f"Step {args.step}" if args.step else "Step"

    try:
        allowlist = load_allowlist(args.allowlist, today=today)
    except AllowlistError as exc:
        print(f"  {label}: allowlist error: {exc}")
        return 1

    with open(args.output, "r", encoding="utf-8", errors="replace") as fh:
        flagged = check(fh, allowlist, frozenset(allowlist.lifecycle))

    if args.scan_json is not None:
        flagged += check_scan_json(args.scan_json, allowlist, args.expect_interrupted)
    elif args.no_scan_json is not None:
        print(f"  {label}: scan JSON check skipped: {args.no_scan_json}", file=sys.stderr)

    flagged = [f"ALLOWLIST ENTRY EXPIRED: {e}" for e in allowlist.expired] + flagged

    if not flagged:
        return 0

    print(f"  {label}: errors detected:")
    for line in flagged:
        print(f"    {line}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
