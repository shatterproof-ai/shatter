#!/usr/bin/env python3
"""Receipt shadow evidence threshold check (str-35vtk.26).

Queries the str-35vtk.18 gate event log (scripts/gate-event-log.py) for
`receipt_shadow` events written by scripts/receipt-shadow-check.py
(str-35vtk.25) and decides whether the project has accumulated enough clean
evidence to let str-35vtk.9 enable a receipt-based skip of Shatter's own
quality gates.

This script only QUERIES and SELECTS; it never re-derives decision,
classification, or diff_class -- those come straight from the event payload
that str-35vtk.25 already computed independently. See that module's
docstring for the decision/classification vocabulary this module consumes.

Eligibility (all must hold for an event to be considered):
  - event_type == "receipt_shadow"
  - exactly one non-deletion ref update in the push (single-ref, not
    multi-ref -- a push that also deleted an unrelated ref still counts as
    single-ref for this purpose, matching receipt-shadow-check.py's own
    non_deletion_updates() semantics)
  - that update's remote_ref is refs/heads/main or refs/heads/master
  - that update's local_sha is now an ancestor of origin/main (or
    origin/master, matched to the pushed ref) in the repo the check is
    run against -- this is what distinguishes a push that
    actually landed from one that was later abandoned, force-pushed over,
    or simply failed before completing. A push that never reached this state
    is not evidence either way and is excluded, not counted as a miss.
  - payload.diff_class == "other" (excludes beads_only and unknown)

Selection: eligible events are ordered by timestamp, deduplicated by
candidate tree (first timestamp wins), and the first N (default 10) distinct
entries are taken.

Threshold rule: zero false_accept and zero *unexplained* unexpected_miss
among the selected set, and the selected set must actually contain N
entries (fewer than N means "not enough evidence yet", which is reported
distinctly from an actual failure). Each expected_miss must carry concrete
reasons (already required by receipt-shadow-check.py's own classification --
this script verifies the field is populated, it does not invent reasons).
Each unexpected_miss needs an externally supplied linked-issue reference
(via --annotations) to count as "explained"; without one it fails the run.

Never touches the network unless explicitly asked to (git fetch is opt-in
via --fetch, off by default) and never mutates the tracker unless
--apply-note is passed.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from typing import Callable

ZERO_SHA = "0" * 40
MAIN_REFS = {"refs/heads/main", "refs/heads/master"}
DEFAULT_LIMIT = 10
DEFAULT_NOTE_ISSUE = "str-35vtk.9"

EXIT_PASS = 0
EXIT_FAIL = 1


def default_event_path() -> str:
    base = os.environ.get("XDG_CACHE_HOME") or os.path.join(
        os.path.expanduser("~"), ".cache"
    )
    return os.path.join(base, "shatter", "gate-events.jsonl")


# --------------------------------------------------------------------------
# Reading the event log
# --------------------------------------------------------------------------


def read_events(path: str) -> list[dict]:
    """Reads and parses the JSONL event log, returning only well-formed
    receipt_shadow envelopes. Malformed lines are skipped (warned on
    stderr) rather than raised -- this tool is read-only and must not choke
    on a line another writer is mid-appending or on unrelated event types."""
    events: list[dict] = []
    if not os.path.exists(path):
        return events
    with open(path, "r", encoding="utf-8") as f:
        for lineno, raw in enumerate(f, start=1):
            raw = raw.strip()
            if not raw:
                continue
            try:
                obj = json.loads(raw)
            except json.JSONDecodeError:
                print(f"[shadow-threshold] warning: skipping malformed line {lineno}", file=sys.stderr)
                continue
            if not isinstance(obj, dict) or obj.get("event_type") != "receipt_shadow":
                continue
            if "payload" not in obj or not isinstance(obj["payload"], dict):
                continue
            events.append(obj)
    return events


# --------------------------------------------------------------------------
# Eligibility filtering
# --------------------------------------------------------------------------


def single_main_ref_update(event: dict) -> dict | None:
    """Returns the one non-deletion update dict if this event is a
    single-ref push to refs/heads/main or refs/heads/master, else None."""
    updates = event.get("payload", {}).get("update") or []
    non_deletions = [u for u in updates if isinstance(u, dict) and u.get("local_sha") != ZERO_SHA]
    if len(non_deletions) != 1:
        return None
    only = non_deletions[0]
    if only.get("remote_ref") not in MAIN_REFS:
        return None
    return only


AncestryCheck = Callable[[str, str, str], bool]


def git_is_ancestor(sha: str, repo: str, ref: str) -> bool:
    result = subprocess.run(
        ["git", "merge-base", "--is-ancestor", sha, ref],
        cwd=repo,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return result.returncode == 0


def _origin_ref_for(remote_ref: str) -> str:
    # "refs/heads/main" -> "origin/main"; "refs/heads/master" -> "origin/master".
    return "origin/" + remote_ref.removeprefix("refs/heads/")


def is_eligible(event: dict, repo: str, ancestry_check: AncestryCheck) -> bool:
    payload = event.get("payload", {})
    if payload.get("diff_class") != "other":
        return False
    update = single_main_ref_update(event)
    if update is None:
        return False
    local_sha = update.get("local_sha")
    remote_ref = update.get("remote_ref")
    if not local_sha or not remote_ref:
        return False
    if not ancestry_check(local_sha, repo, _origin_ref_for(remote_ref)):
        return False
    return True


def eligible_events(events: list[dict], repo: str, ancestry_check: AncestryCheck) -> list[dict]:
    return [e for e in events if is_eligible(e, repo, ancestry_check)]


# --------------------------------------------------------------------------
# Ordering, dedup, selection
# --------------------------------------------------------------------------


def select_first_n_distinct(events: list[dict], limit: int) -> list[dict]:
    """Orders by timestamp ascending, deduplicates by candidate tree (first
    timestamp for a given tree wins), and returns up to `limit` distinct
    entries in that order."""
    ordered = sorted(events, key=lambda e: e.get("timestamp", ""))
    seen: set[str] = set()
    selected: list[dict] = []
    for event in ordered:
        candidate = event.get("payload", {}).get("candidate")
        if not candidate or candidate in seen:
            continue
        seen.add(candidate)
        selected.append(event)
        if len(selected) >= limit:
            break
    return selected


# --------------------------------------------------------------------------
# Record projection -- what we retain per selected entry
# --------------------------------------------------------------------------


def project_record(event: dict) -> dict:
    payload = event.get("payload", {})
    real_gate = payload.get("real_gate") or {}
    return {
        "push_id": payload.get("push_id"),
        "timestamp": event.get("timestamp"),
        "candidate": payload.get("candidate"),
        "decision": payload.get("decision"),
        "classification": payload.get("classification"),
        "reasons": list(payload.get("reasons") or []),
        "real_gate_exit_code": real_gate.get("exit_code"),
        "real_gate_ran": real_gate.get("ran"),
    }


# --------------------------------------------------------------------------
# Threshold evaluation
# --------------------------------------------------------------------------


def evaluate_threshold(records: list[dict], annotations: dict[str, str], limit: int) -> dict:
    false_accepts = [r for r in records if r["classification"] == "false_accept"]
    unexpected_misses = [r for r in records if r["classification"] == "unexpected_miss"]
    expected_misses = [r for r in records if r["classification"] == "expected_miss"]

    unexplained: list[dict] = []
    explained: list[dict] = []
    for r in unexpected_misses:
        key = r.get("candidate") or r.get("push_id")
        linked = annotations.get(key) if key else None
        if linked:
            explained.append({**r, "linked_issue": linked})
        else:
            unexplained.append(r)

    missing_reasons = [r for r in expected_misses if not r["reasons"]]

    insufficient = len(records) < limit

    passed = (
        not insufficient
        and not false_accepts
        and not unexplained
        and not missing_reasons
    )

    return {
        "passed": passed,
        "insufficient_evidence": insufficient,
        "selected_count": len(records),
        "limit": limit,
        "false_accepts": false_accepts,
        "unexpected_misses_unexplained": unexplained,
        "unexpected_misses_explained": explained,
        "expected_misses": expected_misses,
        "expected_misses_missing_reasons": missing_reasons,
        "records": records,
    }


# --------------------------------------------------------------------------
# Reporting
# --------------------------------------------------------------------------


def render_report(result: dict) -> str:
    lines = []
    if result["insufficient_evidence"]:
        lines.append(
            f"NOT MET: insufficient evidence — {result['selected_count']}/{result['limit']} "
            "distinct eligible main-push events found so far."
        )
    elif result["passed"]:
        lines.append(
            f"MET: {result['selected_count']} distinct eligible main-push events, "
            "zero false_accept, zero unexplained unexpected_miss."
        )
    else:
        lines.append("NOT MET:")
        if result["false_accepts"]:
            lines.append(f"  - {len(result['false_accepts'])} false_accept(s):")
            for r in result["false_accepts"]:
                lines.append(f"      push_id={r['push_id']} candidate={r['candidate']}")
        if result["unexpected_misses_unexplained"]:
            lines.append(
                f"  - {len(result['unexpected_misses_unexplained'])} unexplained "
                "unexpected_miss(es) (no linked tracker issue):"
            )
            for r in result["unexpected_misses_unexplained"]:
                lines.append(f"      push_id={r['push_id']} candidate={r['candidate']}")
        if result["expected_misses_missing_reasons"]:
            lines.append(
                f"  - {len(result['expected_misses_missing_reasons'])} expected_miss(es) "
                "missing concrete reasons (schema/bug signal):"
            )
            for r in result["expected_misses_missing_reasons"]:
                lines.append(f"      push_id={r['push_id']} candidate={r['candidate']}")

    lines.append("")
    lines.append(f"Selected {result['selected_count']}/{result['limit']}:")
    for r in result["records"]:
        explained_note = ""
        if r["classification"] == "unexpected_miss":
            explained_note = " [EXPLAINED]" if any(
                e["push_id"] == r["push_id"] for e in result["unexpected_misses_explained"]
            ) else " [UNEXPLAINED]"
        lines.append(
            f"  {r['timestamp']} candidate={r['candidate']} decision={r['decision']} "
            f"classification={r['classification']}{explained_note} "
            f"real_exit={r['real_gate_exit_code']} reasons={r['reasons']}"
        )
    return "\n".join(lines)


def build_note_text(result: dict) -> str:
    return (
        f"str-35vtk.26 milestone met: {result['selected_count']} distinct eligible "
        "single-ref main-push receipt_shadow events observed with zero false_accept "
        "and zero unexplained unexpected_miss (diff_class=other, local commit an "
        "ancestor of origin/main). Threshold satisfied for enabling reuse per "
        "str-35vtk.9."
    )


# --------------------------------------------------------------------------
# Fetch (injectable) and note-append (injectable)
# --------------------------------------------------------------------------


def fetch_origin(repo: str, run: Callable[..., subprocess.CompletedProcess] = subprocess.run) -> None:
    run(
        ["git", "fetch", "origin"],
        cwd=repo,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )


def append_note(
    issue: str,
    text: str,
    run: Callable[..., subprocess.CompletedProcess] = subprocess.run,
) -> tuple[bool, str]:
    result = run(["bd", "note", issue, text], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, check=False)
    if result.returncode != 0:
        return False, (result.stderr or "").strip()
    return True, ""


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------


def load_annotations(path: str | None) -> dict[str, str]:
    if not path:
        return {}
    if not os.path.exists(path):
        return {}
    with open(path, "r", encoding="utf-8") as f:
        data = json.load(f)
    if not isinstance(data, dict):
        raise ValueError("annotations file must contain a JSON object")
    return {str(k): str(v) for k, v in data.items() if v}


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="receipt-shadow-threshold-check.py", description=__doc__)
    p.add_argument("--event-log-path", default=None, help="override the event log file path")
    p.add_argument("--repo", default=".", help="git repo to run ancestry checks against (default: cwd)")
    p.add_argument("--fetch", action="store_true", help="run `git fetch origin` before querying (off by default; network access is opt-in)")
    p.add_argument("--limit", type=int, default=DEFAULT_LIMIT)
    p.add_argument("--annotations", default=None, help="JSON file mapping candidate tree (or push_id) -> linked tracker issue id, for explaining unexpected_miss entries")
    p.add_argument("--note-issue", default=DEFAULT_NOTE_ISSUE)
    p.add_argument("--apply-note", action="store_true", help="on a passing run, append a durable note to --note-issue via `bd note`")
    p.add_argument("--json", action="store_true", help="print the machine-readable report instead of the human report")
    return p


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    repo = os.path.abspath(args.repo)

    if args.fetch:
        fetch_origin(repo)

    event_path = args.event_log_path or default_event_path()
    events = read_events(event_path)
    eligible = eligible_events(events, repo, git_is_ancestor)
    selected = select_first_n_distinct(eligible, args.limit)
    records = [project_record(e) for e in selected]

    annotations = load_annotations(args.annotations)
    result = evaluate_threshold(records, annotations, args.limit)

    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
    else:
        print(render_report(result))

    if result["passed"] and args.apply_note:
        note_text = build_note_text(result)
        ok, err = append_note(args.note_issue, note_text)
        if ok:
            print(f"\nAppended note to {args.note_issue}.")
        else:
            print(f"\nWARNING: failed to append note to {args.note_issue}: {err}", file=sys.stderr)

    return EXIT_PASS if result["passed"] else EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
