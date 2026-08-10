#!/usr/bin/env python3
"""Bounded drift patrol — the recurring governance sweep (str-u394l.1).

Drift checks in this repo have historically depended on somebody remembering
to run a broad audit. This script is the single entry point for the bounded
subset of those checks that is cheap enough to run on a schedule, so drift
becomes visible before it blocks unrelated work.

Run it via `task drift-patrol` (which builds the frontends first, so the
conformance check is not degraded) or directly:

    python3 scripts/drift-patrol.py

Exit code 0 when every enabled check passes, 1 when any check fails. The
report printed to stdout is deliberately paste-ready: it carries enough
context — issue ids, remediation commands, and the offending items — to file
or update a Beads issue without re-running the audit.

See docs/DRIFT-PATROL.md for cadence, audience, and what to do with failures.

Check statuses
--------------
PASS     the check ran and found no drift
FAIL     the check ran and found drift (exit 1)
PENDING  the check is a placeholder for work tracked by another issue. It is
         reported loudly but does not fail the patrol by default, because a
         permanently red scheduled run trains everyone to ignore it — which is
         the exact failure mode this patrol exists to fix. Use
         --strict-pending for the literal "not implemented is a failure"
         behavior. A PENDING check whose tracking issue has been *closed* is
         always a FAIL: that means the feature landed without wiring the
         patrol, which is itself drift.
SKIP     the check could not run in this environment (missing build artifact,
         missing tracker data). --require-conformance turns the conformance
         skip into a failure; CI uses it after building the frontends.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent

PASS = "PASS"
FAIL = "FAIL"
PENDING = "PENDING"
SKIP = "SKIP"

# Frontend build artifacts the conformance harness needs. Mirrors the
# `build_check` entries in protocol/conformance/conformance_cases.yaml.
FRONTEND_BUILD_ARTIFACTS = {
    "typescript": "shatter-ts/dist/main.js",
    "go": "shatter-go/bin/shatter-go",
    "rust": "shatter-rust/target/debug/shatter-rust",
}

# Default staleness threshold for in_progress tracker issues, in days.
DEFAULT_STALE_DAYS = 14

# Default lead time for the parity divergence expiry escalation. Matches
# .github/workflows/parity-expiry.yml so the two triggers agree.
DEFAULT_WARN_WITHIN_DAYS = 14

# How many offending items to enumerate per check before truncating.
MAX_ITEMS_REPORTED = 50


@dataclass
class Result:
    """Outcome of one patrol check."""

    check_id: str
    title: str
    status: str
    summary: str
    details: list[str] = field(default_factory=list)
    tracking_issue: str | None = None
    remediation: str | None = None

    @property
    def failed(self) -> bool:
        return self.status == FAIL

    def to_dict(self) -> dict:
        return {
            "check": self.check_id,
            "title": self.title,
            "status": self.status,
            "summary": self.summary,
            "details": self.details,
            "tracking_issue": self.tracking_issue,
            "remediation": self.remediation,
        }


# ---------------------------------------------------------------------------
# Subprocess helper
# ---------------------------------------------------------------------------


def run_command(
    cmd: list[str], *, timeout: int = 900
) -> tuple[int, str]:
    """Run `cmd` from the repo root, returning (exit code, combined output)."""
    try:
        proc = subprocess.run(
            cmd,
            cwd=str(REPO_ROOT),
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=timeout,
        )
    except FileNotFoundError as exc:
        return 127, f"command not found: {exc}"
    except subprocess.TimeoutExpired:
        return 124, f"timed out after {timeout}s: {' '.join(cmd)}"
    return proc.returncode, proc.stdout.decode(errors="replace")


def tail(text: str, lines: int = 25) -> list[str]:
    """Last `lines` non-empty lines of `text`, for embedding in the report."""
    kept = [ln.rstrip() for ln in text.splitlines() if ln.strip()]
    return kept[-lines:]


def _subprocess_check(
    *,
    check_id: str,
    title: str,
    cmd: list[str],
    ok_summary: str,
    fail_summary: str,
    remediation: str,
    tracking_issue: str | None = None,
    timeout: int = 900,
) -> Result:
    """Shared shape for checks that delegate to an existing validator."""
    code, output = run_command(cmd, timeout=timeout)
    if code == 0:
        return Result(check_id, title, PASS, ok_summary, remediation=remediation)
    return Result(
        check_id,
        title,
        FAIL,
        f"{fail_summary} (exit {code})",
        details=[f"$ {' '.join(cmd)}", *tail(output)],
        tracking_issue=tracking_issue,
        remediation=remediation,
    )


# ---------------------------------------------------------------------------
# Tracker data
# ---------------------------------------------------------------------------


@dataclass
class Tracker:
    """Beads issues plus a note on where they came from."""

    issues: list[dict]
    source: str

    def by_id(self) -> dict[str, dict]:
        return {i["id"]: i for i in self.issues if i.get("id")}


def load_tracker() -> tuple[Tracker | None, str | None]:
    """Load all Beads issues.

    Prefers the `bd` CLI (authoritative, carries dependency edges); falls back
    to the checked-in `.beads/issues.jsonl` export so the patrol works in CI,
    where `bd` is not installed. Returns (tracker, None) or (None, reason).
    """
    if _which("bd"):
        code, output = run_command(
            ["bd", "list", "--all", "--json", "--limit", "0", "--no-pager"],
            timeout=180,
        )
        if code == 0:
            try:
                parsed = json.loads(output)
            except json.JSONDecodeError:
                parsed = None
            if isinstance(parsed, list) and parsed:
                return Tracker(parsed, "bd list --all"), None

    export = REPO_ROOT / ".beads" / "issues.jsonl"
    if not export.exists():
        return None, "no `bd` on PATH and .beads/issues.jsonl is absent"

    issues: list[dict] = []
    for line in export.read_text(errors="replace").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            continue
        if record.get("_type", "issue") == "issue":
            issues.append(record)
    if not issues:
        return None, ".beads/issues.jsonl contains no issue records"
    return Tracker(issues, ".beads/issues.jsonl (committed export)"), None


def _which(name: str) -> bool:
    for directory in os.environ.get("PATH", "").split(os.pathsep):
        if directory and (Path(directory) / name).exists():
            return True
    return False


def parse_ts(value: str | None) -> datetime | None:
    """Parse a Beads RFC3339 timestamp."""
    if not value:
        return None
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed


def parent_id(issue: dict) -> str | None:
    """Parent issue id, from the explicit field or the dotted-id convention.

    `bd list --json` carries an explicit `parent`. The committed JSONL export
    does not include dependency edges, so fall back to Beads' hierarchical id
    convention: `str-u394l.1`'s parent is `str-u394l`.
    """
    explicit = issue.get("parent")
    if explicit:
        return str(explicit)
    ident = issue.get("id") or ""
    if "." in ident:
        return ident.rsplit(".", 1)[0]
    return None


def issue_status(tracker: Tracker | None, issue_id: str) -> str | None:
    """Status of `issue_id`, or None when it is unknown."""
    if tracker is None:
        return None
    record = tracker.by_id().get(issue_id)
    return record.get("status") if record else None


# ---------------------------------------------------------------------------
# Placeholder checks for work tracked elsewhere
# ---------------------------------------------------------------------------


def pending_check(
    *,
    check_id: str,
    title: str,
    tracking_issue: str,
    what: str,
    remediation: str,
    tracker: Tracker | None,
    strict_pending: bool,
    landed: bool = False,
) -> Result:
    """A patrol slot whose implementation lives in another issue.

    `landed` lets a caller say "the dependency's artifact is present now" —
    used by the docs/stories check, whose real work can start the moment the
    directory exists.
    """
    status = issue_status(tracker, tracking_issue)
    if status == "closed" and not landed:
        return Result(
            check_id,
            title,
            FAIL,
            f"{tracking_issue} is closed but this patrol check is still a placeholder",
            details=[
                f"{what} was tracked by {tracking_issue}, which is now closed.",
                "Either the work landed without wiring it into the patrol, or the",
                "issue was closed as won't-do and this placeholder should be removed.",
            ],
            tracking_issue=tracking_issue,
            remediation=remediation,
        )

    detail_status = status or "unknown (tracker data unavailable)"
    result_status = FAIL if strict_pending else PENDING
    return Result(
        check_id,
        title,
        result_status,
        f"not implemented — tracked by {tracking_issue} (status: {detail_status})",
        details=[
            f"{what} is not implemented yet.",
            f"This patrol slot activates when {tracking_issue} lands; it is reported",
            "on every run so the gap stays visible instead of being forgotten.",
        ],
        tracking_issue=tracking_issue,
        remediation=remediation,
    )


# ---------------------------------------------------------------------------
# Checks
# ---------------------------------------------------------------------------


def check_protocol_registry(**_: object) -> Result:
    return _subprocess_check(
        check_id="protocol-registry",
        title="Protocol registry matches frontend/core source",
        cmd=[sys.executable, "scripts/validate-protocol-registry.py"],
        ok_summary="registry names and field models agree with the source of truth",
        fail_summary="protocol/registry.yaml has drifted from the implementations",
        remediation=(
            "Reconcile protocol/registry.yaml with the handler sources, then re-run "
            "`python3 scripts/validate-protocol-registry.py`. See protocol/GOVERNANCE.md."
        ),
    )


def check_protocol_codegen(**_: object) -> Result:
    return _subprocess_check(
        check_id="protocol-codegen",
        title="Generated protocol artifacts are in sync",
        cmd=[sys.executable, "scripts/protocol-codegen.py", "--check"],
        ok_summary="generated bindings match protocol/registry.yaml",
        fail_summary="generated protocol artifacts are stale",
        remediation=(
            "Regenerate with `python3 scripts/protocol-codegen.py` and commit the "
            "result. Never hand-edit generated bindings."
        ),
    )


def check_conformance(*, require_conformance: bool = False, **_: object) -> Result:
    """Protocol conformance across the built frontends.

    This is the patrol's documented fast subset of `task check`: it runs the
    conformance harness only, not the language test suites. The harness needs
    built frontends; without them it silently degrades to skipping most cases,
    so detect that here rather than reporting a hollow pass.
    """
    missing = [
        f"{name} ({path})"
        for name, path in FRONTEND_BUILD_ARTIFACTS.items()
        if not (REPO_ROOT / path).exists()
    ]
    if missing:
        remediation = (
            "Run `task drift-patrol`, which builds the frontends first, or build "
            "manually with `task ts:build go:build rust-fe:build`."
        )
        if require_conformance:
            return Result(
                "protocol-conformance",
                "Protocol conformance across frontends",
                FAIL,
                "frontend builds missing but --require-conformance was set",
                details=[f"missing build artifact: {m}" for m in missing],
                remediation=remediation,
            )
        return Result(
            "protocol-conformance",
            "Protocol conformance across frontends",
            SKIP,
            "frontend builds missing — conformance would be degraded, not run",
            details=[f"missing build artifact: {m}" for m in missing],
            remediation=remediation,
        )

    return _subprocess_check(
        check_id="protocol-conformance",
        title="Protocol conformance across frontends",
        cmd=[sys.executable, "protocol/conformance/conformance_harness.py"],
        ok_summary="all frontends conform to the protocol contract",
        fail_summary="a frontend diverges from the protocol contract",
        remediation=(
            "Fix the diverging frontend, then re-run `task conformance`. If the "
            "divergence is intentional, record it in protocol/parity-matrix.yaml "
            "and protocol/PARITY.md."
        ),
    )


def check_parity_expiry(*, warn_within_days: int = DEFAULT_WARN_WITHIN_DAYS, **_: object) -> Result:
    """Impending parity-divergence expiries (str-5dx0).

    str-5dx0 landed scripts/validate-parity.py's --warn-as-error-within-days
    escalation plus .github/workflows/parity-expiry.yml. The patrol runs the
    same escalation so a single red run names every governance gap, rather
    than making a reader correlate two workflows.
    """
    return _subprocess_check(
        check_id="parity-expiry",
        title="Parity divergence expiry warnings",
        cmd=[
            sys.executable,
            "scripts/validate-parity.py",
            "--warn-as-error-within-days",
            str(warn_within_days),
        ],
        ok_summary=(
            f"no resolved divergence is within {warn_within_days} day(s) of its "
            "removal deadline"
        ),
        fail_summary="a parity divergence is expiring or the parity contract has drifted",
        tracking_issue="str-5dx0",
        remediation=(
            "Remove the expiring entry from both protocol/parity-matrix.yaml and "
            "protocol/PARITY.md before the hard grace deadline turns `task parity` "
            "red on an unrelated branch."
        ),
    )


def check_cli_surface(*, tracker: Tracker | None = None, strict_pending: bool = False, **_: object) -> Result:
    """CLI-surface drift — placeholder until str-wurp lands."""
    return pending_check(
        check_id="cli-surface-drift",
        title="CLI surface vs SPEC.md and gauntlet coverage",
        tracking_issue="str-wurp",
        what="A mechanical CLI-surface drift gate (clap inventory vs SPEC.md §2 and gauntlet coverage)",
        remediation=(
            "Implement str-wurp's clap-inventory check, then replace this "
            "placeholder in scripts/drift-patrol.py with a call to it."
        ),
        tracker=tracker,
        strict_pending=strict_pending,
    )


def check_docs_stories(
    *, tracker: Tracker | None = None, strict_pending: bool = False, **_: object
) -> Result:
    """docs/stories existence and index freshness (str-u394l.3).

    Until the stories gate lands there is nothing to patrol, so this reports
    PENDING. Once docs/stories exists, the check becomes real: every story
    file must be listed in INDEX.md, and INDEX.md must not be older than the
    newest story it indexes.
    """
    stories_dir = REPO_ROOT / "docs" / "stories"
    if not stories_dir.is_dir():
        return pending_check(
            check_id="docs-stories",
            title="docs/stories exists and its index is fresh",
            tracking_issue="str-u394l.3",
            what="A stories coverage gate (docs/stories does not exist yet)",
            remediation=(
                "Land str-u394l.3 (stories coverage gate) — `storystore:stories-init` "
                "creates docs/stories and its INDEX.md."
            ),
            tracker=tracker,
            strict_pending=strict_pending,
        )

    index = stories_dir / "INDEX.md"
    if not index.is_file():
        return Result(
            "docs-stories",
            "docs/stories exists and its index is fresh",
            FAIL,
            "docs/stories exists but has no INDEX.md",
            tracking_issue="str-u394l.3",
            remediation="Regenerate the index with `storystore:stories-generate`.",
        )

    index_text = index.read_text(errors="replace")
    index_mtime = index.stat().st_mtime
    unindexed: list[str] = []
    newer: list[str] = []
    for story in sorted(stories_dir.rglob("*.md")):
        if story == index or story.name == "README.md":
            continue
        rel = story.relative_to(stories_dir).as_posix()
        if rel not in index_text and story.stem not in index_text:
            unindexed.append(rel)
        elif story.stat().st_mtime > index_mtime:
            newer.append(rel)

    if unindexed or newer:
        details = [f"not listed in INDEX.md: {p}" for p in unindexed[:MAX_ITEMS_REPORTED]]
        details += [f"modified after INDEX.md: {p}" for p in newer[:MAX_ITEMS_REPORTED]]
        return Result(
            "docs-stories",
            "docs/stories exists and its index is fresh",
            FAIL,
            f"{len(unindexed)} unindexed story file(s), {len(newer)} newer than INDEX.md",
            details=details,
            tracking_issue="str-u394l.3",
            remediation="Regenerate docs/stories/INDEX.md with `storystore:stories-generate`.",
        )

    return Result(
        "docs-stories",
        "docs/stories exists and its index is fresh",
        PASS,
        "every story is indexed and INDEX.md is no older than the stories it lists",
    )


def check_tracker_hygiene(
    *,
    tracker: Tracker | None = None,
    tracker_error: str | None = None,
    stale_days: int = DEFAULT_STALE_DAYS,
    now: datetime | None = None,
    **_: object,
) -> Result:
    """Two currently-observed tracker drift classes.

    1. `in_progress` issues untouched for more than `stale_days` — work that
       was claimed and then abandoned, which makes `bd ready` lie about what
       is actually free to pick up.
    2. Open or `in_progress` children whose parent is closed — the parent
       epic was closed over unfinished work, so the child has no owner.
    """
    title = "Tracker hygiene (stale in_progress, orphaned children)"
    if tracker is None:
        return Result(
            "tracker-hygiene",
            title,
            SKIP,
            f"tracker data unavailable: {tracker_error or 'unknown reason'}",
            remediation="Install `bd` or commit an up-to-date .beads/issues.jsonl export.",
        )

    now = now or datetime.now(timezone.utc)
    by_id = tracker.by_id()

    stale: list[tuple[int, str]] = []
    orphans: list[str] = []
    for issue in tracker.issues:
        status = issue.get("status")
        if status == "in_progress":
            updated = parse_ts(issue.get("updated_at"))
            if updated is not None:
                age = (now - updated).days
                if age > stale_days:
                    stale.append((age, f"{issue['id']} ({age}d) {issue.get('title', '')[:70]}"))

        if status in ("open", "in_progress"):
            parent = parent_id(issue)
            parent_record = by_id.get(parent) if parent else None
            if parent_record and parent_record.get("status") == "closed":
                orphans.append(
                    f"{issue['id']} [{status}] — parent {parent_record['id']} "
                    f"({parent_record.get('issue_type', 'issue')}) is closed: "
                    f"{issue.get('title', '')[:60]}"
                )

    stale.sort(reverse=True)
    remediation = (
        "For each stale issue: finish it, or `bd update <id> --status open` to "
        "release the claim. For each orphan: reopen the parent, re-parent the "
        "child, or close the child."
    )

    if not stale and not orphans:
        return Result(
            "tracker-hygiene",
            title,
            PASS,
            f"no in_progress issue older than {stale_days}d, no orphaned children",
            details=[f"source: {tracker.source}"],
            remediation=remediation,
        )

    details = [f"source: {tracker.source}"]
    if stale:
        details.append(f"-- in_progress with no update for >{stale_days}d ({len(stale)}) --")
        details += [line for _, line in stale[:MAX_ITEMS_REPORTED]]
        if len(stale) > MAX_ITEMS_REPORTED:
            details.append(f"... and {len(stale) - MAX_ITEMS_REPORTED} more")
    if orphans:
        details.append(f"-- open/in_progress children under a closed parent ({len(orphans)}) --")
        details += orphans[:MAX_ITEMS_REPORTED]
        if len(orphans) > MAX_ITEMS_REPORTED:
            details.append(f"... and {len(orphans) - MAX_ITEMS_REPORTED} more")

    return Result(
        "tracker-hygiene",
        title,
        FAIL,
        f"{len(stale)} stale in_progress issue(s), {len(orphans)} orphaned child issue(s)",
        details=details,
        tracking_issue="str-u394l.1",
        remediation=remediation,
    )


CHECKS = [
    ("protocol-registry", check_protocol_registry),
    ("protocol-codegen", check_protocol_codegen),
    ("protocol-conformance", check_conformance),
    ("parity-expiry", check_parity_expiry),
    ("cli-surface-drift", check_cli_surface),
    ("docs-stories", check_docs_stories),
    ("tracker-hygiene", check_tracker_hygiene),
]

CHECK_IDS = [check_id for check_id, _ in CHECKS]


# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------

STATUS_ORDER = {FAIL: 0, PENDING: 1, SKIP: 2, PASS: 3}


def render_report(results: list[Result], *, timestamp: str) -> str:
    """Markdown report, written to be pasted into a Beads issue verbatim."""
    counts = {status: 0 for status in (PASS, FAIL, PENDING, SKIP)}
    for result in results:
        counts[result.status] = counts.get(result.status, 0) + 1

    lines = [
        "# Shatter drift patrol",
        "",
        f"Run at {timestamp} — "
        f"{counts[FAIL]} failed, {counts[PENDING]} pending, "
        f"{counts[SKIP]} skipped, {counts[PASS]} passed.",
        "",
        "| Check | Status | Summary |",
        "|-------|--------|---------|",
    ]
    for result in results:
        summary = result.summary.replace("|", "\\|")
        lines.append(f"| `{result.check_id}` | {result.status} | {summary} |")

    actionable = sorted(
        (r for r in results if r.status in (FAIL, PENDING)),
        key=lambda r: STATUS_ORDER[r.status],
    )
    if actionable:
        lines += ["", "## Findings", ""]
        for result in actionable:
            lines.append(f"### {result.status}: {result.title} (`{result.check_id}`)")
            lines.append("")
            lines.append(result.summary)
            if result.tracking_issue:
                lines.append("")
                lines.append(f"Responsible issue: `{result.tracking_issue}`")
            if result.details:
                lines += ["", "```"]
                lines += result.details
                lines.append("```")
            if result.remediation:
                lines += ["", f"Remediation: {result.remediation}"]
            lines.append("")

    if counts[FAIL] == 0:
        lines += ["", "No failing checks. Nothing to file."]
    else:
        lines += [
            "",
            "File or update a Beads issue with this report under the "
            "`drift` label, parented to `str-u394l`.",
        ]
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--only",
        metavar="ID",
        action="append",
        choices=CHECK_IDS,
        help=f"run only the named check (repeatable). One of: {', '.join(CHECK_IDS)}",
    )
    parser.add_argument(
        "--skip",
        metavar="ID",
        action="append",
        choices=CHECK_IDS,
        help="skip the named check (repeatable)",
    )
    parser.add_argument(
        "--require-conformance",
        action="store_true",
        help="fail instead of skipping when the frontend builds are missing",
    )
    parser.add_argument(
        "--strict-pending",
        action="store_true",
        help="treat not-yet-implemented placeholder checks as failures",
    )
    parser.add_argument(
        "--stale-days",
        type=int,
        default=DEFAULT_STALE_DAYS,
        metavar="N",
        help=f"flag in_progress issues untouched for more than N days (default {DEFAULT_STALE_DAYS})",
    )
    parser.add_argument(
        "--warn-within-days",
        type=int,
        default=DEFAULT_WARN_WITHIN_DAYS,
        metavar="N",
        help=(
            "escalate a resolved parity divergence within N days of its removal "
            f"deadline (default {DEFAULT_WARN_WITHIN_DAYS})"
        ),
    )
    parser.add_argument(
        "--today",
        metavar="YYYY-MM-DD",
        help="override today's date (testing)",
    )
    parser.add_argument(
        "--report",
        metavar="PATH",
        help="also write the markdown report to PATH (e.g. $GITHUB_STEP_SUMMARY)",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="print machine-readable JSON instead of the markdown report",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)

    selected = set(args.only) if args.only else set(CHECK_IDS)
    if args.skip:
        selected -= set(args.skip)
    if not selected:
        print("no checks selected", file=sys.stderr)
        return 2

    if args.today:
        try:
            now = datetime.strptime(args.today, "%Y-%m-%d").replace(tzinfo=timezone.utc)
        except ValueError:
            print(f"invalid --today value: {args.today}", file=sys.stderr)
            return 2
    else:
        now = datetime.now(timezone.utc)

    tracker, tracker_error = (None, None)
    if selected & {"cli-surface-drift", "docs-stories", "tracker-hygiene"}:
        tracker, tracker_error = load_tracker()

    context = {
        "tracker": tracker,
        "tracker_error": tracker_error,
        "stale_days": args.stale_days,
        "warn_within_days": args.warn_within_days,
        "require_conformance": args.require_conformance,
        "strict_pending": args.strict_pending,
        "now": now,
    }

    results = [fn(**context) for check_id, fn in CHECKS if check_id in selected]
    report = render_report(results, timestamp=now.strftime("%Y-%m-%d %H:%M UTC"))

    if args.json:
        print(json.dumps({"results": [r.to_dict() for r in results]}, indent=2))
    else:
        print(report, end="")

    if args.report:
        path = Path(args.report)
        with path.open("a", encoding="utf-8") as handle:
            handle.write(report)

    return 1 if any(r.failed for r in results) else 0


if __name__ == "__main__":
    sys.exit(main())
