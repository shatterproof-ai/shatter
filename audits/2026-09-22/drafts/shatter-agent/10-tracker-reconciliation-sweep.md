# Tracker reconciliation: close resolved/obsolete/landed issues and add a landed-not-closed patrol check

- Priority: P2
- Type: chore
- Labels: agents,beads,drift,governance
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (overlaps str-qwua7.62, str-qwua7.17; note to append to str-qwua7.62)
- Source findings: agent-repo-05, sessions-09, prior-09, protocol-parity-16, frontend-rust-10 (tracker part)
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
The tracker lags reality: issues whose premise no longer holds stay open (and
block others in `bd ready`), landed work stays `in_progress`, duplicates stay
open, and closures carry bare "Closed" reasons. Work done under a different
issue ID never touches the umbrella issue.

## Current Code Facts (verify each before acting)
- str-qwua7.1 (repair bare primary): `git -C /home/ketan/project/shatter config core.bare` -> false. Open P1.
  Keep only its hygiene-check half if still wanted (covered by draft 04).
- str-qwua7.12 (exit codes): `explore nope.ts:foo`, unknown function, sandbox
  refusal and bad-file `spec-diff` all exit 2 at HEAD; `error_exit_code` dates
  from 464e3c9e (before the 09-04 audit base) — filed from a stale binary.
  Its SPEC `--failure-threshold` sub-item remains (SPEC.md:634).
- str-qwua7.18 (parked branches): 5 of 6 landed under their own IDs
  (str-rmcrl, str-na9db, str-0z1im, str-6vl7p, str-vr7vq); str-8q1b4 remains.
- str-qwua7.19: main == origin/main; one preview worktree
  `/tmp/land-work-preview-a5l9ycto` is still registered.
- str-qwua7.18 and .19 are `blocked_by` str-qwua7.1.
- str-mpgg1: `in_progress` since 2026-09-01; merged as 84941b37.
- str-8q1b4: `in_progress`, branch 105 behind / 4 ahead of origin/main, last
  commit 2026-08-31 — flagged by drift-patrol a third time.
- str-qe9pp: both symptoms fixed (golden updated 750b7ff8 on 2026-06-18;
  prepare timeout fixed by str-uoclg, closed 2026-08-13). Open, no notes.
- str-uj3y (open) duplicates str-da35 (closed, landed 4182e51e).
- str-1fik (P1, empty body) and str-wfd2 (P2) overlap; str-qwua7.62 decided to
  merge wfd2 into 1fik; audit reviewer suggests the reverse (wfd2 = cross-crate
  synthesis). Follow the recorded .62 decision unless the maintainer overrides.
- Orphans flagged by drift-patrol: str-qwua7.56.1, str-qwua7.9.1,
  str-hy9b.J3, str-hy9b.1.

## Acceptance Criteria
- Each item above is closed, re-scoped, or re-parented with a close reason that
  cites the evidence (command + output or SHA). Stale blocking edges from
  str-qwua7.1 removed.
- `python3 scripts/drift-patrol.py --only tracker-hygiene` passes (or lists only
  items with a documented reason).
- New drift-patrol check: FAIL for `in_progress` issues whose branch
  (`<id>-*` or `<id>`) is an ancestor of origin/main (landed-not-closed), and
  WARN for `in_progress` issues older than 14 days with no matching local/remote
  branch or worktree. Unit-tested with canned bd/git data.
- Note appended to str-qwua7.62 pointing here for the items above.

## Suggested Approach
Tracker-only work: no branch needed for the closes; the patrol check lands via
launch-work. Re-verify str-qwua7.14 separately (draft 12).

## Out of Scope
Priority policy (draft 11). Rust type-synthesis implementation.
