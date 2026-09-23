#!/usr/bin/env python3
"""Observational pre-push receipt shadow check (str-35vtk.25).

Reads the git pre-push protocol lines ("<local ref> <local sha> <remote ref>
<remote sha>", one per line) from stdin and, for exactly one non-deletion ref
update, independently asks "did the real gate that is about to run (or just
ran) already have valid local-tier receipt evidence for this exact
candidate/base tree pair?" using the str-35vtk.23 validator
(scripts/gate-receipt.py) and the str-35vtk.18 event log store
(scripts/gate-event-log.py).

This is purely observational. It NEVER changes control flow: it does not
decide whether the real gate runs, and any failure here (including failure
to append the event) is reported on stderr only. The caller (the pre-push
hook body in scripts/setup-hooks.sh) is responsible for running the real
gate exactly as it always has, unaffected by anything in this module.

Decision values:
  reuse      -- a valid local-tier receipt already covers this exact
                candidate/base/requirements triple; the real gate could, in
                principle, have been skipped.
  invalid    -- no valid receipt covers it (missing, stale, or mismatched);
                the real gate needed to run for real, as it did.
  no_gate    -- this push required no product gate at all (PUSH_TASK empty).

reasons come straight from gate-receipt.py's own independently-recomputed
comparison (str-35vtk.23 already recomputes bindings/tools/tree equality
rather than trusting the stored receipt), plus a couple of shadow-specific
reasons for cases gate-receipt.py treats as a hard I/O error rather than a
soft "invalid" (there being no receipt file at all is the common case for a
brand-new candidate/base pair, not a bug).

classification grades the shadow check's own accuracy against what the real
gate actually did this push:
  false_accept    -- decision was "reuse" (would have skipped) but the real
                     gate that ran anyway actually failed. The dangerous case.
  expected_miss   -- decision was "invalid"/"no_gate" and reasons name a
                     concrete cause (including "no receipt at all").
  unexpected_miss -- decision was "invalid" but no concrete cause was found
                     (a bug signal in this shadow check itself).
  match           -- decision was "reuse" and the real gate passed, or there
                     was nothing to disagree about (no_gate).
"""

from __future__ import annotations

import argparse
import json
import os
import secrets
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
GATE_RECEIPT_PATH = SCRIPT_DIR / "gate-receipt.py"
GATE_EVENT_LOG_PATH = SCRIPT_DIR / "gate-event-log.py"

GATE_RECEIPT_EXIT_OK = 0
GATE_RECEIPT_EXIT_NOT_VALID = 65

ZERO_SHA = "0" * 40
EVENT_TYPE = "receipt_shadow"


class ShadowCheckError(Exception):
    """Raised for conditions the caller should report but never fail on."""


def parse_ref_lines(text: str) -> list[dict[str, str]]:
    """Parse pre-push protocol lines into dicts. Raises ValueError on a
    malformed line (the pre-push hook body already validates SHA shape and
    field count before this script ever sees the input, but this function is
    also exercised directly in tests, so it validates defensively too)."""
    updates = []
    for line in text.splitlines():
        if not line.strip():
            continue
        fields = line.split()
        if len(fields) != 4:
            raise ValueError(f"expected 4 fields, got {len(fields)}: {line!r}")
        local_ref, local_sha, remote_ref, remote_sha = fields
        updates.append(
            {
                "local_ref": local_ref,
                "local_sha": local_sha,
                "remote_ref": remote_ref,
                "remote_sha": remote_sha,
            }
        )
    return updates


def non_deletion_updates(updates: list[dict[str, str]]) -> list[dict[str, str]]:
    return [u for u in updates if u["local_sha"] != ZERO_SHA]


def _run_git(args: list[str], cwd: str) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise ShadowCheckError(
            f"git {' '.join(args)} failed: {result.stderr.strip() or result.returncode}"
        )
    return result.stdout.strip()


def compute_candidate_and_base(
    local_sha: str, remote_sha: str, cwd: str
) -> tuple[str, str]:
    candidate = _run_git(["rev-parse", f"{local_sha}^{{tree}}"], cwd)
    if remote_sha != ZERO_SHA:
        base = _run_git(["rev-parse", f"{remote_sha}^{{tree}}"], cwd)
    else:
        merge_base = _run_git(["merge-base", local_sha, "origin/main"], cwd)
        base = _run_git(["rev-parse", f"{merge_base}^{{tree}}"], cwd)
    return candidate, base


def classify_diff(base_tree: str, candidate_tree: str, cwd: str) -> str:
    """beads_only | other | unknown.

    An empty diff satisfies "every changed path is under .beads/**"
    vacuously, so it classifies as beads_only.
    """
    try:
        result = subprocess.run(
            ["git", "diff", "--name-only", base_tree, candidate_tree],
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
    except OSError:
        return "unknown"
    if result.returncode != 0:
        return "unknown"
    paths = [p for p in result.stdout.splitlines() if p]
    if all(p.startswith(".beads/") for p in paths):
        return "beads_only"
    return "other"


def build_requirements(push_task: str) -> dict | None:
    if not push_task:
        return None
    gate_name = f"task {push_task}"
    return {"schema": 1, "requirements": [{"gate": gate_name, "argv": ["task", push_task]}]}


def validate_against_receipt(
    candidate: str, base: str, requirements: dict, cwd: str
) -> tuple[str, list[str]]:
    """Returns (decision, reasons) by shelling out to the str-35vtk.23
    validator (scripts/gate-receipt.py validate), which independently
    recomputes bindings/tools/tree equality rather than trusting the stored
    receipt's own claims -- this shadow check reuses that recomputation
    instead of re-implementing it."""
    requirements_file = tempfile.NamedTemporaryFile(
        mode="w", suffix=".json", delete=False
    )
    try:
        json.dump(requirements, requirements_file)
        requirements_file.close()
        result = subprocess.run(
            [
                sys.executable,
                str(GATE_RECEIPT_PATH),
                "validate",
                "--candidate",
                candidate,
                "--base",
                base,
                "--tier",
                "local",
                "--requirements",
                requirements_file.name,
            ],
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
    finally:
        try:
            os.unlink(requirements_file.name)
        except OSError:
            pass

    if result.returncode in (GATE_RECEIPT_EXIT_OK, GATE_RECEIPT_EXIT_NOT_VALID):
        try:
            parsed = json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            raise ShadowCheckError(
                f"gate-receipt.py validate produced non-JSON stdout: {exc}"
            ) from exc
        status = parsed.get("status")
        reasons = list(parsed.get("reasons", []))
        decision = "reuse" if status == "valid" else "invalid"
        return decision, reasons

    # Any other exit code is gate-receipt.py's own I/O or invalid-input
    # class (EXIT_INVALID=64, EXIT_IOERR=74). The overwhelmingly common
    # case here is "no receipt file exists at all for this candidate tree"
    # (a brand-new candidate/base pair that has never been through
    # land-work locally) -- gate-receipt.py treats that as a hard I/O
    # error rather than a soft "invalid", so this shadow check translates
    # it back into its own vocabulary instead of surfacing it as a crash.
    stderr = result.stderr.strip()
    if "cannot open receipt" in stderr and (
        "No such file or directory" in stderr or "not found" in stderr.lower()
    ):
        return "invalid", ["missing_gate"]
    return "invalid", [f"io_error:{stderr}" if stderr else "io_error"]


def compute_classification(
    decision: str, reasons: list[str], gate_ran: bool, gate_exit: int | None
) -> str:
    if decision == "reuse":
        if gate_ran and gate_exit is not None and gate_exit != 0:
            return "false_accept"
        return "match"
    if decision == "no_gate":
        return "match"
    # decision == "invalid" (including multi_ref / no_target_ref)
    if reasons:
        return "expected_miss"
    return "unexpected_miss"


def make_push_id() -> str:
    return f"{int(time.time() * 1000)}-{os.getpid()}-{secrets.token_hex(4)}"


def now_rfc3339() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def is_normalized_abs_path(value: str) -> bool:
    # Mirrors gate-event-log.py's identically-named helper. Not imported:
    # gate-event-log.py is invoked as a subprocess (hyphenated filename, not
    # a module) elsewhere in this file. Keep both copies in sync.
    if not value.startswith("/"):
        return False
    if value != "/" and value.endswith("/"):
        return False
    return os.path.normpath(value) == value


def build_event(
    *,
    worktree: str,
    push_task: str,
    gate_ran: bool,
    gate_exit: int | None,
    updates: list[dict[str, str]],
    timestamp: str | None = None,
    push_id: str | None = None,
) -> dict:
    """Builds the full receipt_shadow envelope (schema per
    scripts/gate-event-log.py) for the given stdin updates and real-gate
    outcome. Never raises for expected conditions (multi-ref, no receipt,
    git failures while computing candidate/base/diff) -- those are folded
    into decision/reasons/diff_class instead."""
    non_deletions = non_deletion_updates(updates)

    candidate: str | None = None
    base: str | None = None
    diff_class = "unknown"
    requirements: dict | None = None
    decision: str
    reasons: list[str]

    if len(non_deletions) != 1:
        decision = "invalid"
        reasons = ["multi_ref"] if len(non_deletions) > 1 else ["no_target_ref"]
    else:
        single = non_deletions[0]
        try:
            candidate, base = compute_candidate_and_base(
                single["local_sha"], single["remote_sha"], worktree
            )
            diff_class = classify_diff(base, candidate, worktree)
        except ShadowCheckError as exc:
            decision = "invalid"
            reasons = [f"git_error:{exc}"]
            candidate = None
            base = None
            diff_class = "unknown"
        else:
            requirements = build_requirements(push_task)
            if requirements is None:
                decision = "no_gate"
                reasons = []
            else:
                decision, reasons = validate_against_receipt(
                    candidate, base, requirements, worktree
                )

    classification = compute_classification(decision, reasons, gate_ran, gate_exit)

    payload = {
        "push_id": push_id or make_push_id(),
        "update": [dict(u) for u in updates],
        "candidate": candidate,
        "base": base,
        "diff_class": diff_class,
        "decision": decision,
        "reasons": reasons,
        "requirements": requirements,
        "real_gate": {
            "gate": f"task {push_task}" if push_task else None,
            "ran": gate_ran,
            "exit_code": gate_exit,
        },
        "classification": classification,
    }

    return {
        "schema": 1,
        "event_type": EVENT_TYPE,
        "timestamp": timestamp or now_rfc3339(),
        "gate": (f"task {push_task}" if push_task else "none"),
        "worktree": worktree,
        "payload": payload,
    }


def append_event(event: dict, event_log_path: str | None) -> tuple[bool, str]:
    args = [sys.executable, str(GATE_EVENT_LOG_PATH), "append"]
    if event_log_path:
        args.extend(["--path", event_log_path])
    try:
        result = subprocess.run(
            args,
            input=json.dumps(event),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
    except OSError as exc:
        return False, f"cannot run gate-event-log.py: {exc}"
    if result.returncode != 0:
        return False, (
            f"gate-event-log.py append exited {result.returncode}: "
            f"{result.stderr.strip()}"
        )
    return True, ""


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="receipt-shadow-check.py")
    p.add_argument("--worktree", required=True, help="absolute repo working-tree path")
    p.add_argument(
        "--push-task",
        default="",
        help="the real gate task name about to run/that ran (e.g. 'affected', "
        "'check'), or empty when no gate is required for this push",
    )
    p.add_argument("--gate-ran", choices=("0", "1"), default="0")
    p.add_argument("--gate-exit", type=int, default=None)
    p.add_argument("--event-log-path", default=None)
    return p


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)

    # This entire script is observational: it must never fail the caller.
    # Every expected/unexpected condition below is reported on stderr and
    # the script still exits 0.
    try:
        worktree = os.path.normpath(os.path.abspath(args.worktree))
        raw_stdin = sys.stdin.read()
        try:
            updates = parse_ref_lines(raw_stdin)
        except ValueError as exc:
            print(f"[shatter-shadow] warning: malformed stdin: {exc}", file=sys.stderr)
            return 0

        gate_ran = args.gate_ran == "1"
        event = build_event(
            worktree=worktree,
            push_task=args.push_task,
            gate_ran=gate_ran,
            gate_exit=args.gate_exit if gate_ran else None,
            updates=updates,
        )
        ok, message = append_event(event, args.event_log_path)
        if not ok:
            print(f"[shatter-shadow] warning: failed to append event: {message}", file=sys.stderr)
    except Exception as exc:  # noqa: BLE001 - never let the shadow check fail the hook
        print(f"[shatter-shadow] warning: shadow check errored: {exc}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
